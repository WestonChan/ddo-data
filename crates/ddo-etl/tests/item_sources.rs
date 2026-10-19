mod fixture;

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
    built_with(&WikiOverrides::from_database(&fixture::database(|_| {})).unwrap())
}

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
fn loot_adventure_packs_has_one_row_per_loot_pack_and_source() {
    let mut wiki = WikiOverrides::from_database(&fixture::database(|_| {})).unwrap();
    wiki.crafting_systems.iter_mut().find(|system| system.name == "Thunder-Forged").unwrap().pack =
        Some("Free to Play".into());
    wiki.quest_chains[0].pack = Some("Masterminds of Sharn".into());
    let (db, _) = built_with(&wiki);
    let mut statement = db
        .prepare(
            "SELECT p.name, packs.source_kind FROM loot_adventure_packs packs
         JOIN adventure_packs p ON p.id = packs.pack_id
         LEFT JOIN items i ON i.id = packs.item_id LEFT JOIN augments a ON a.id = packs.augment_id
         WHERE COALESCE(i.name, a.name) = ?1 ORDER BY p.name, packs.source_kind, packs.source_id",
        )
        .unwrap();
    for (loot_name, expected_packs) in [
        ("Acrobat's Ring", vec![("Free to Play", "quest_chain")]),
        ("Thunder-Forged Orb", vec![("Free to Play", "crafting_system")]),
        ("Ethereal Great Crossbow", vec![("Free to Play", "vendor")]),
        ("Epic Ring of the Stalker", vec![("Vaults of the Artificers", "challenge")]),
        ("Light Crossbow of the Golden Age", vec![("Magic of Myth Drannor", "adventure_pack")]),
        ("Lunar Gem of Magical Protection (Heroic)", vec![("Magic of Myth Drannor", "quest")]),
        ("Bold Trinket", vec![]),
        ("Blood-Red Lenses", vec![]),
        ("Visor of Fraz-Urb'luu", vec![]),
    ] {
        let packs = statement
            .query_map([loot_name], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let expected_packs: Vec<_> =
            expected_packs.into_iter().map(|(pack, kind)| (pack.to_string(), kind.to_string())).collect();
        assert_eq!(packs, expected_packs, "{loot_name}");
    }
    let duplicated_rows: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM (SELECT 1 FROM loot_adventure_packs
         GROUP BY item_id, augment_id, pack_id, source_kind, source_id HAVING COUNT(*) > 1)",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(duplicated_rows, 0);
    let mismatched_sources: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM loot_adventure_packs packs LEFT JOIN sources s ON s.id = packs.source_id
         WHERE s.id IS NULL OR s.kind <> packs.source_kind
         OR s.item_id IS NOT packs.item_id OR s.augment_id IS NOT packs.augment_id",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(mismatched_sources, 0);
    let saga_rows: Vec<(String, i64)> = db
        .prepare(
            "SELECT p.name, COUNT(*) FROM loot_adventure_packs packs JOIN adventure_packs p ON p.id = packs.pack_id
         JOIN items i ON i.id = packs.item_id WHERE i.name = 'Five Rings' AND packs.source_kind = 'saga'
         GROUP BY p.id ORDER BY p.name",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(saga_rows, [("Chill of Ravenloft".into(), 2), ("Masterminds of Sharn".into(), 2)]);
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
    let ours = fixture::database(|root| {
        std::fs::write(
            root.join("packs/unsourced/gear/test-catalysed-ring.toml"),
            r#"
name = "Test Catalysed Ring"
is_wiki_only = true
slot = "Ring"
item_category = "Jewelry"
minimum_level = 30
drop_location = "Catalyst Crafting, Turn in a test ring at the Strange Catalyst Forge"
"#,
        )
        .unwrap();
    });
    let (db, report) = built_with(&WikiOverrides::from_database(&ours).unwrap());
    assert_eq!(source_rows(&db, "Test Catalysed Ring"), ["crafting_system Catalyst Crafting 0"]);
    assert_eq!(report.drop_text_crafting_system_source_count, 3);
}

#[test]
fn links_no_crafting_system_the_data_files_lack() {
    let (db, report) = built_with(&WikiOverrides::default());
    assert_eq!(source_rows(&db, "Visor of Fraz-Urb'luu"), Vec::<String>::new());
    assert_eq!(report.drop_text_crafting_system_source_count, 0);
    assert!(unlinked_heads(&db).iter().any(|head| head == "Catalyst Crafting"));
}

#[test]
fn every_crafting_station_and_vendor_alias_names_a_wiki_crafting_system_or_vendor() {
    let mut ours = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data"), &mut ours).unwrap();
    let wiki = WikiOverrides::from_database(&ours).unwrap();
    let crafting_system_names: Vec<String> = wiki.crafting_systems.into_iter().map(|system| system.name).collect();
    let vendor_names: Vec<String> = wiki.vendors.into_iter().map(|vendor| vendor.name).collect();
    let aliases = SourceAliases::embedded().unwrap();
    assert!(!aliases.crafting_systems.is_empty() && !aliases.challenges.is_empty() && !aliases.vendors.is_empty());
    for alias in &aliases.crafting_systems {
        assert!(crafting_system_names.contains(&alias.system), "{alias:?} names no system in the canonical data tree");
    }
    for alias in &aliases.vendors {
        assert!(vendor_names.contains(&alias.vendor), "{alias:?} names no vendor in the canonical data tree");
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
        ["challenge Vaults of the Artificers 0"],
        "'Vaults of the Artificers, Turn in various ingredients' names the Cannith challenge pack"
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
        [("Alabaster of the Twelve".into(), Some("10 Planar Shard".into())), ("Ethereal Great Crossbow".into(), None)]
    );
    assert_eq!(source_rows(&db, "Bold Trinket"), ["event Treasure of Crystal Cove 0"]);
    assert!(source_rows(&db, "Acrobat's Ring").contains(&"event Treasure of Crystal Cove 0".to_string()));
    assert_eq!((report.wiki_vendor_count, report.vendor_item_count), (1, 2));
    assert_eq!((report.wiki_event_count, report.event_item_count), (1, 3));
    assert_eq!(
        (report.drop_text_vendor_source_count, report.drop_text_event_source_count),
        (1, 2),
        "the Crossbow; Bold Trinket and Ratkiller (legacy)"
    );
    let heads = unlinked_heads(&db);
    assert!(!heads.iter().any(|head| head == "Morten Edgewright" || head == "Treasure of Crystal Cove"), "{heads:?}");
}

#[test]
fn rejects_a_vendor_or_event_naming_an_item_or_pack_his_files_lack() {
    for (kind, missing_name) in [
        ("vendor", "No Such Item"),
        ("event", "No Such Item"),
        ("vendor", "No Such Pack"),
        ("event", "Cosmetic Jackorb"),
    ] {
        let ours = fixture::database(|root| {
            let owner = format!("Test {kind}");
            let path = root.join(format!("packs/shared/{kind}s/test-{kind}/{kind}.toml"));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let location = if kind == "vendor" { "location = \"Test location\"\n" } else { "" };
            let pack = if missing_name == "No Such Pack" {
                let pack_path = root.join("packs/no-such-pack/pack.toml");
                std::fs::create_dir_all(pack_path.parent().unwrap()).unwrap();
                std::fs::write(pack_path, "name = \"No Such Pack\"\n").unwrap();
                "pack = \"No Such Pack\"\n"
            } else {
                ""
            };
            std::fs::write(path, format!("name = {owner:?}\n{location}{pack}[[citation]]\nurl = \"https://ddowiki.com/page/Test\"\nread = \"2026-10-02\"\n")).unwrap();
            if missing_name != "No Such Pack" {
                if missing_name == "Cosmetic Jackorb" {
                    std::fs::write(root.join("upstream/excluded-items.toml"), "row = []\n").unwrap();
                }
                let path = root.join(format!("packs/unsourced/gear/{}.toml", ddo_etl::tree::slug(missing_name)));
                std::fs::write(path, format!("name = {missing_name:?}\nslot = \"Ring\"\nitem_category = \"Jewelry\"\nminimum_level = 1\n[[source]]\nkind = {kind:?}\nname = {owner:?}\n")).unwrap();
            }
        });
        let wiki = WikiOverrides::from_database(&ours).unwrap();
        let owner_file = if kind == "vendor" {
            &wiki.vendors.iter().find(|vendor| vendor.name == "Test vendor").unwrap().file_name
        } else {
            &wiki.events.iter().find(|event| event.name == "Test event").unwrap().file_name
        };
        let mut db = Connection::open_in_memory().unwrap();
        let dataset_version =
            DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-10-02T00:00:00Z".into() };
        let error = build_database(
            &fixtures_dir().join("DataFiles"),
            &wiki,
            &Corrections::default(),
            &mut db,
            &dataset_version,
        )
        .unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains(missing_name) && message.contains(owner_file), "{message}");
        if missing_name == "Cosmetic Jackorb" {
            assert!(message.contains("left out of the items on purpose (cosmetic shield)"), "{message}");
        }
    }
}

#[test]
fn links_an_item_turned_in_at_a_place_to_the_vendor_an_alias_names_for_its_head_and_turn_in() {
    let ours = fixture::database(|root| {
        for name in ["Raam Lukresh", "Osah Lukresh", "Squire Rale", "Balkar Aislin"] {
            let path = root.join(format!("packs/shared/vendors/{}/vendor.toml", ddo_etl::tree::slug(name)));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, format!("name = {name:?}\nlocation = \"Test location\"\n[[citation]]\nurl = \"https://ddowiki.com/page/Test_vendor\"\nread = \"2026-10-02\"\n")).unwrap();
        }
    });
    let (db, report) = built_with(&WikiOverrides::from_database(&ours).unwrap());
    assert_eq!(
        source_rows(&db, "Macabre Great Crossbow"),
        ["vendor Raam Lukresh 0"],
        "'Blue Water Inn, Turn in 40 Vistani Talismans'"
    );
    assert_eq!(
        source_rows(&db, "Nightmother's Great Crossbow"),
        ["vendor Osah Lukresh 0"],
        "'Blue Water Inn, Turn in 80 Vistani Totems', the same head as Raam Lukresh's"
    );
    assert_eq!(source_rows(&db, "Minos Legens"), ["vendor Balkar Aislin 0"]);
    assert_eq!(
        source_rows(&db, "Righteous Bracers"),
        ["vendor Squire Rale 0"],
        "'Necropolis, Turn in Dark Scarab Powder'"
    );
    assert_eq!(report.drop_text_vendor_source_count, 5, "the four above and the Ethereal Great Crossbow");
    let heads = unlinked_heads(&db);
    assert!(
        !heads.iter().any(|head| ["Blue Water Inn", "Tattered Tapestries", "Necropolis"].contains(&head.as_str())),
        "{heads:?}"
    );
}

#[test]
fn rejects_a_vendor_alias_repeating_a_head_and_turn_in_but_keeps_one_whose_turn_in_differs() {
    let alias = |contains: &str, vendor: &str| {
        format!(
            "[[vendor]]\ntext = \"Blue Water Inn\"\ncontains = {contains:?}\nvendor = {vendor:?}\nreason = \"Its turn-in.\"\n"
        )
    };
    let distinct_turn_ins =
        format!("{}{}", alias("Vistani Talismans", "Raam Lukresh"), alias("Vistani Totems", "Osah Lukresh"));
    let aliases = SourceAliases::from_toml_str(&distinct_turn_ins).unwrap();
    assert_eq!(aliases.vendors.len(), 2);
    let repeated = format!("{}{}", alias("Vistani Totems", "Raam Lukresh"), alias("vistani totems", "Osah Lukresh"));
    assert!(SourceAliases::from_toml_str(&repeated).is_err(), "{repeated}");
    assert!(SourceAliases::from_toml_str(&alias(" ", "Raam Lukresh")).is_err(), "a blank contains must fail");
}

#[test]
fn links_an_item_made_at_a_station_two_systems_share_to_the_one_its_minimum_level_falls_in() {
    let ours = fixture::database(|root| {
        for name in ["Green Steel items", "Legendary Green Steel items"] {
            let path = root.join(format!("packs/shared/crafting/{}/crafting.toml", ddo_etl::tree::slug(name)));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, format!("name = {name:?}\n[[citation]]\nurl = \"https://ddowiki.com/page/Test_crafting\"\nread = \"2026-10-02\"\n")).unwrap();
        }
    });
    let (db, report) = built_with(&WikiOverrides::from_database(&ours).unwrap());
    assert_eq!(
        source_rows(&db, "Green Steel Weave Boots"),
        ["crafting_system Green Steel items 0"],
        "'Altar of Fecundity, Manufactured Ingredient Recipes' at minimum level 11"
    );
    assert_eq!(
        source_rows(&db, "Legendary Green Steel Belt"),
        ["crafting_system Legendary Green Steel items 0"],
        "the same text at minimum level 26"
    );
    assert_eq!(report.drop_text_crafting_system_source_count, 4);
    assert!(!unlinked_heads(&db).iter().any(|head| head == "Altar of Fecundity"));
}

#[test]
fn rejects_crafting_system_aliases_whose_minimum_levels_overlap_but_keeps_ones_that_split_them() {
    let alias = |levels: &str, system: &str| {
        format!("[[crafting_system]]\ntext = \"Altar of Fecundity\"\n{levels}system = {system:?}\nreason = \"Its blanks.\"\n")
    };
    let split = format!(
        "{}{}",
        alias("max_minimum_level = 20\n", "Green Steel items"),
        alias("min_minimum_level = 21\n", "Legendary Green Steel items")
    );
    assert_eq!(SourceAliases::from_toml_str(&split).unwrap().crafting_systems.len(), 2);
    let overlapping = format!(
        "{}{}",
        alias("max_minimum_level = 21\n", "Green Steel items"),
        alias("min_minimum_level = 21\n", "Legendary Green Steel items")
    );
    assert!(SourceAliases::from_toml_str(&overlapping).is_err(), "{overlapping}");
    let empty_range = alias("min_minimum_level = 21\nmax_minimum_level = 20\n", "Green Steel items");
    assert!(SourceAliases::from_toml_str(&empty_range).is_err(), "{empty_range}");
}
