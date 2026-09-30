use ddo_etl::build::{build_database, BuildReport};
use ddo_etl::corrections::Corrections;
use ddo_etl::map::drop_location::marks_rare_loot;
use ddo_etl::wiki::WikiOverrides;
use ddo_model::DatasetVersion;
use rusqlite::Connection;
use std::path::PathBuf;

fn built_without_wiki() -> (Connection, BuildReport) {
    let mut db = Connection::open_in_memory().unwrap();
    let data_files_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles");
    let dataset_version = DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-09-20T00:00:00Z".into() };
    let report =
        build_database(&data_files_dir, &WikiOverrides::default(), &Corrections::default(), &mut db, &dataset_version)
            .unwrap();
    (db, report)
}

fn is_rare(db: &Connection, quest: &str, item: &str) -> Option<bool> {
    db.query_row(
        "SELECT ql.is_rare FROM quest_loot ql JOIN quests q ON q.id = ql.quest_id JOIN items i ON i.id = ql.item_id
          WHERE q.name = ?1 AND i.name = ?2",
        [quest, item],
        |r| r.get(0),
    )
    .ok()
}

#[test]
fn marks_a_rare_chest_drop_from_maetrims_text_alone() {
    let (db, report) = built_without_wiki();
    assert_eq!(is_rare(&db, "Book Burning", "Buckler of the Golden Age"), Some(true), "'end chest (rare)'");
    assert_eq!(report.drop_text_rare_link_count, 1);
}

#[test]
fn leaves_unmarked_and_rare_encounter_drops_common() {
    let (db, _) = built_without_wiki();
    assert_eq!(is_rare(&db, "The Cursed Crypt", "Docent of Defiance"), Some(false));
    assert_eq!(
        is_rare(&db, "Land of Lamordia", "Gravekeeper's Docent"),
        Some(false),
        "'red-named rare encounter chests' names a rare monster, not rare loot"
    );
}

#[test]
fn classifies_drop_text_segments_by_their_rarity_marker() {
    for rare in [
        "Book Burning, end chest (rare)",
        "The Final Draw, end chest (rare )",
        "Ruins of Myth Drannor, Yegora 's chest (RARE)",
        "Magic of Myth Drannor, rare drop in any end chest",
        "also Rare Drop in any Sharn quest",
    ] {
        assert!(marks_rare_loot(rare), "{rare}");
    }
    for common in [
        "The Cursed Crypt, End Chest",
        "Land of Lamordia, red-named rare encounter chests",
        "The Borderlands, chest (Rare Encounters)",
        "Three-Barrel Cove (heroic), Brine 's Chest (Rare Encounter)",
        "Korthos Island, Any of the Rares",
    ] {
        assert!(!marks_rare_loot(common), "{common}");
    }
}
