use ddo_etl::build::{build_database, unlinked_drop_segment_heads, BuildReport};
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
            "SELECT s.kind || ' ' || COALESCE(q.name, c.name, sg.name, p.name, cs.name, s.character_level)
                    || ' ' || s.is_rare
               FROM sources s LEFT JOIN quests q ON q.id = s.quest_id LEFT JOIN quest_chains c ON c.id = s.chain_id
               LEFT JOIN sagas sg ON sg.id = s.saga_id LEFT JOIN adventure_packs p ON p.id = s.pack_id
               LEFT JOIN crafting_systems cs ON cs.id = s.crafting_system_id
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
