use ddo_etl::build::{build_database, unlinked_drop_segment_heads, unlinked_reward_givers, BuildReport};
use ddo_etl::corrections::Corrections;
use ddo_etl::map::source_alias::SourceAliases;
use ddo_etl::wiki::WikiOverrides;
use ddo_model::DatasetVersion;
use rusqlite::Connection;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn built_with(wiki: &WikiOverrides) -> (Connection, BuildReport) {
    let mut db = Connection::open_in_memory().unwrap();
    let dataset_version = DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-10-02T00:00:00Z".into() };
    let report =
        build_database(&fixtures_dir().join("DataFiles"), wiki, &Corrections::default(), &mut db, &dataset_version)
            .unwrap();
    (db, report)
}

fn built_with_fixture_wiki() -> (Connection, BuildReport) {
    built_with(&WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap())
}

fn fixture_wiki_with(extra_file_name: &str, extra_toml: &str) -> WikiOverrides {
    let mut toml_files: Vec<(String, String)> = std::fs::read_dir(fixtures_dir().join("wiki"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .map(|path| (path.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read_to_string(&path).unwrap()))
        .collect();
    toml_files.sort();
    toml_files.push((extra_file_name.to_string(), extra_toml.to_string()));
    let borrowed_files: Vec<(&str, &str)> =
        toml_files.iter().map(|(file_name, toml_text)| (file_name.as_str(), toml_text.as_str())).collect();
    WikiOverrides::from_toml_files(&borrowed_files).unwrap()
}

const WIKI_ITEM_FROM_A_CRAFTING_STATION: &str =
    "# A test item, not read from ddowiki, whose drop text names a crafting system.\n\
     [[item]]\nname = \"Test Catalysed Ring\"\npage = \"https://ddowiki.com/page/Item:Test_Catalysed_Ring\"\n\
     read = \"2026-10-02\"\nslot = \"Ring\"\ncategory = \"Jewelry\"\nminimum_level = 30\n\
     drop_location = \"Catalyst Crafting, Turn in a test ring at the Strange Catalyst Forge\"\n";

fn source_rows(db: &Connection, loot_name: &str) -> Vec<String> {
    let mut statement = db
        .prepare(
            "SELECT s.kind || ' ' || COALESCE(q.name, c.name, sg.name, p.name, cs.name, v.name, e.name, s.character_level)
                    || ' ' || s.is_rare
               FROM sources s LEFT JOIN quests q ON q.id = s.quest_id LEFT JOIN quest_chains c ON c.id = s.chain_id
               LEFT JOIN sagas sg ON sg.id = s.saga_id LEFT JOIN adventure_packs p ON p.id = s.pack_id
               LEFT JOIN crafting_systems cs ON cs.id = s.crafting_system_id
               LEFT JOIN vendors v ON v.id = s.vendor_id LEFT JOIN events e ON e.id = s.event_id
               LEFT JOIN items i ON i.id = s.item_id LEFT JOIN augments a ON a.id = s.augment_id
              WHERE COALESCE(i.name, a.name) = ?1 ORDER BY 1",
        )
        .unwrap();
    statement.query_map([loot_name], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

fn unlinked_heads(db: &Connection) -> Vec<String> {
    unlinked_drop_segment_heads(db).unwrap().into_iter().map(|head| head.head).collect()
}

#[test]
fn links_an_item_to_the_crafting_system_its_drop_text_or_station_names() {
    let (db, report) = built_with_fixture_wiki();
    assert_eq!(
        source_rows(&db, "Visor of Fraz-Urb'luu"),
        ["crafting_system Catalyst Crafting 0"],
        "'Catalyst Crafting, Turn in Visor of the Flesh Render Guards, ...'"
    );
    assert_eq!(
        source_rows(&db, "Thunder-Forged Orb"),
        ["crafting_system Thunder-Forged 0"],
        "'Magma Forge, Crafted from various ingredients', Magma Forge being Thunder-Forged's station"
    );
    assert_eq!(report.drop_text_crafting_system_source_count, 2);
    let heads = unlinked_heads(&db);
    assert!(!heads.iter().any(|head| head == "Catalyst Crafting" || head == "Magma Forge"), "{heads:?}");
}

#[test]
fn links_a_wiki_item_to_the_crafting_system_its_drop_text_names() {
    let (db, report) = built_with(&fixture_wiki_with("items_test.toml", WIKI_ITEM_FROM_A_CRAFTING_STATION));
    assert_eq!(source_rows(&db, "Test Catalysed Ring"), ["crafting_system Catalyst Crafting 0"]);
    assert_eq!(report.drop_text_crafting_system_source_count, 3);
}

#[test]
fn links_no_crafting_system_the_wiki_files_lack() {
    let (db, report) = built_with(&WikiOverrides::default());
    assert_eq!(source_rows(&db, "Visor of Fraz-Urb'luu"), Vec::<String>::new());
    assert_eq!(report.drop_text_crafting_system_source_count, 0);
    assert!(unlinked_heads(&db).iter().any(|head| head == "Catalyst Crafting"));
}

#[test]
fn every_crafting_station_alias_names_a_wiki_crafting_system() {
    let crafting_system_names: Vec<String> =
        WikiOverrides::embedded().unwrap().crafting_systems.into_iter().map(|system| system.name).collect();
    let aliases = SourceAliases::embedded().unwrap();
    assert!(!aliases.crafting_systems.is_empty() && !aliases.challenges.is_empty());
    for alias in &aliases.crafting_systems {
        assert!(crafting_system_names.contains(&alias.system), "{alias:?} names no system in data/wiki");
    }
}

#[test]
fn rejects_an_alias_file_with_an_unknown_field_a_blank_text_or_a_repeated_text() {
    let alias = |text: &str| {
        format!("[[crafting_system]]\ntext = {text:?}\nsystem = \"Thunder-Forged\"\nreason = \"Its station.\"\n")
    };
    for (alias_file, broken_rule) in [
        (format!("{}colour = \"red\"\n", alias("Magma Forge")), "an unknown field"),
        (alias(" "), "a blank text"),
        (format!("{}{}", alias("Magma Forge"), alias("magma forge")), "a text listed twice"),
    ] {
        assert!(SourceAliases::from_toml_str(&alias_file).is_err(), "{broken_rule} must fail: {alias_file}");
    }
}

#[test]
fn links_an_item_his_text_gives_for_challenge_ingredients_to_the_challenge_pack() {
    let (db, report) = built_with(&WikiOverrides::default());
    assert_eq!(
        source_rows(&db, "Epic Ring of the Stalker"),
        ["challenge Secrets of the Artificers 0"],
        "'Vaults of the Artificers, Turn in various ingredients', the Vaults being where the Cannith challenges of \
         Secrets of the Artificers are turned in"
    );
    assert_eq!(report.drop_text_challenge_source_count, 1);
    assert!(!unlinked_heads(&db).iter().any(|head| head == "Vaults of the Artificers"));
    assert!(
        report.unresolved_source_aliases.iter().any(|alias| alias == "Eveningstar"),
        "the fixtures carry no Eveningstar Challenge Pack: {:?}",
        report.unresolved_source_aliases
    );
}

#[test]
fn links_an_iconic_starter_item_to_the_level_that_earns_it() {
    let (db, report) = built_with(&WikiOverrides::default());
    assert_eq!(source_rows(&db, "Blood-Red Lenses"), ["starter 15 0"], "'Advance to level 15, End reward'");
    assert_eq!(report.drop_text_starter_source_count, 1);
    assert!(!unlinked_heads(&db).iter().any(|head| head.starts_with("Advance to level")));
    let reward_giver_names: Vec<String> =
        unlinked_reward_givers(&db).unwrap().into_iter().map(|reward_giver| reward_giver.name).collect();
    assert!(!reward_giver_names.iter().any(|name| name.starts_with("Advance to level")), "{reward_giver_names:?}");
}

fn vendor_costs(db: &Connection) -> Vec<(String, Option<String>)> {
    let mut statement = db
        .prepare(
            "SELECT i.name, s.cost FROM sources s JOIN items i ON i.id = s.item_id WHERE s.kind = 'vendor' ORDER BY i.name",
        )
        .unwrap();
    statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
}

#[test]
fn writes_the_wiki_vendors_and_events_with_the_items_they_list_and_his_text_names() {
    let (db, report) = built_with_fixture_wiki();
    let vendor: (String, String, String, String, String) = db
        .query_row(
            "SELECT v.name, v.location, p.name, v.provenance, v.wiki_url FROM vendors v JOIN adventure_packs p ON p.id = v.pack_id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(
        vendor,
        (
            "Morten Edgewright".into(),
            "House Jorasco".into(),
            "Free to Play".into(),
            "wiki".into(),
            "https://ddowiki.com/page/Morten_Edgewright".into()
        )
    );
    assert_eq!(
        source_rows(&db, "Ethereal Great Crossbow"),
        ["vendor Morten Edgewright 0"],
        "'Morten Edgewright, Turn in 1 Ethereal Ingot', which the wiki file also lists"
    );
    assert!(source_rows(&db, "Alabaster of the Twelve").contains(&"vendor Morten Edgewright 0".to_string()));
    assert_eq!(
        vendor_costs(&db),
        [("Alabaster of the Twelve".into(), Some("10 Planar Shards".into())), ("Ethereal Great Crossbow".into(), None)]
    );
    assert_eq!(source_rows(&db, "Bold Trinket"), ["event Treasure of Crystal Cove 0"]);
    assert!(source_rows(&db, "Acrobat's Ring").contains(&"event Treasure of Crystal Cove 0".to_string()));
    assert_eq!((report.wiki_vendor_count, report.vendor_item_count), (1, 2));
    assert_eq!((report.wiki_event_count, report.event_item_count), (1, 1));
    assert_eq!(
        (report.drop_text_vendor_source_count, report.drop_text_event_source_count),
        (1, 2),
        "the Crossbow; Bold Trinket and Ratkiller (legacy)"
    );
    let heads = unlinked_heads(&db);
    assert!(!heads.iter().any(|head| head == "Morten Edgewright" || head == "Treasure of Crystal Cove"), "{heads:?}");
}

fn built_with_fixture_wiki_and(extra_file_name: &str, extra_toml: &str) -> Result<(), String> {
    let mut db = Connection::open_in_memory().unwrap();
    let dataset_version = DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-10-02T00:00:00Z".into() };
    let wiki = WikiOverrides::from_toml_files(&[(extra_file_name, extra_toml)]).map_err(|e| format!("{e:#}"))?;
    build_database(&fixtures_dir().join("DataFiles"), &wiki, &Corrections::default(), &mut db, &dataset_version)
        .map(|_| ())
        .map_err(|e| format!("{e:#}"))
}

#[test]
fn rejects_a_vendor_or_event_naming_an_item_or_pack_his_files_lack_or_listing_an_item_twice() {
    let vendor = |extra: &str| {
        format!(
            "# Test values, not read from ddowiki.\n[[vendor]]\nname = \"Test Vendor\"\npage = \"https://ddowiki.com/page/Test_Vendor\"\n\
             read = \"2026-10-02\"\n{extra}"
        )
    };
    let unknown_item =
        built_with_fixture_wiki_and("vendors.toml", &vendor("items = [\"No Such Item\"]\n")).unwrap_err();
    assert!(unknown_item.contains("vendors.toml") && unknown_item.contains("No Such Item"), "{unknown_item}");
    let unknown_pack = built_with_fixture_wiki_and("vendors.toml", &vendor("pack = \"No Such Pack\"\n")).unwrap_err();
    assert!(unknown_pack.contains("No Such Pack"), "{unknown_pack}");
    let repeated_item = built_with_fixture_wiki_and(
        "vendors.toml",
        &vendor("items = [\"Bold Trinket\", { name = \"Bold Trinket\", cost = \"5 tokens\" }]\n"),
    )
    .unwrap_err();
    assert!(repeated_item.contains("listed twice"), "{repeated_item}");
    let event = "# Test values, not read from ddowiki.\n[[event]]\nname = \"Test Event\"\npage = \"https://ddowiki.com/page/Test_Event\"\n\
                 read = \"2026-10-02\"\nitems = [\"No Such Item\"]\n";
    let unknown_event_item = built_with_fixture_wiki_and("events.toml", event).unwrap_err();
    assert!(
        unknown_event_item.contains("events.toml") && unknown_event_item.contains("No Such Item"),
        "{unknown_event_item}"
    );
    let unknown_field = built_with_fixture_wiki_and("events.toml", &event.replace("items", "rewards")).unwrap_err();
    assert!(unknown_field.contains("rewards"), "{unknown_field}");
}
