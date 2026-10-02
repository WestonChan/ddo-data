use ddo_etl::build::{build_database, unlinked_drop_segment_heads, BuildReport};
use ddo_etl::corrections::Corrections;
use ddo_etl::map::legacy_drop_source::LegacyDropSources;
use ddo_etl::wiki::WikiOverrides;
use ddo_model::DatasetVersion;
use rusqlite::{params, Connection};
use std::path::PathBuf;

const LEGACY_TOEE_AXE: &str = "+3 Combustion Scorched Battle Axe";
const PART_ONE: &str = "Temple of Elemental Evil Part One";
const PART_TWO: &str = "Temple of Elemental Evil Part Two";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn built_fixture_db() -> (Connection, BuildReport) {
    let mut db = Connection::open_in_memory().unwrap();
    let dataset_version = DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-10-02T00:00:00Z".into() };
    let report = build_database(
        &fixtures_dir().join("DataFiles"),
        &WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap(),
        &Corrections::default(),
        &mut db,
        &dataset_version,
    )
    .unwrap();
    (db, report)
}

fn is_legacy(db: &Connection, item_name: &str) -> bool {
    db.query_row("SELECT is_legacy FROM items WHERE name = ?1", params![item_name], |r| r.get(0))
        .unwrap_or_else(|error| panic!("{item_name}: {error}"))
}

fn legacy_sources_file(text: &str) -> String {
    format!(
        "[[legacy]]\ntext = \"{text}\"\nreason = \"Test entry.\"\nsource = \"https://ddowiki.com/page/Test\"\n\
         read = \"2026-10-02\"\n"
    )
}

#[test]
fn flags_an_item_whose_every_drop_segment_names_a_legacy_source() {
    let (db, report) = built_fixture_db();
    assert!(is_legacy(&db, LEGACY_TOEE_AXE), "'{PART_ONE}, and {PART_TWO} any chest' names only legacy sources");
    assert!(!is_legacy(&db, "Sireth, Spear of the Sky"));
    assert_eq!(report.legacy_source_flagged_count, 1);
    let axe_drop_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM drops JOIN items ON items.id = drops.item_id WHERE items.name = ?1",
            params![LEGACY_TOEE_AXE],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(axe_drop_count, 0);
}

#[test]
fn leaves_no_unlinked_head_for_a_legacy_segment() {
    let (db, _) = built_fixture_db();
    let heads: Vec<String> = unlinked_drop_segment_heads(&db).unwrap().into_iter().map(|head| head.head).collect();
    assert!(!heads.iter().any(|head| head.starts_with("Temple of Elemental Evil Part")), "{heads:?}");
}

#[test]
fn names_the_legacy_texts_only_when_every_segment_carries_one() {
    let legacy_sources = LegacyDropSources::embedded().unwrap();
    assert_eq!(
        legacy_sources
            .texts_naming_every_segment(&[&format!("{PART_ONE}, any chest"), &format!("{PART_TWO}, any chest")]),
        Some(vec![PART_ONE, PART_TWO])
    );
    assert_eq!(
        legacy_sources.texts_naming_every_segment(&[&format!("{PART_ONE}, and {PART_TWO} any chest")]),
        Some(vec![PART_ONE, PART_TWO])
    );
    assert_eq!(
        legacy_sources.texts_naming_every_segment(&["Temple of elemental evil part two, Elemental nodes chests"]),
        Some(vec![PART_TWO]),
        "matched ignoring the case of every letter after the first, as quest names are"
    );
    assert_eq!(
        legacy_sources
            .texts_naming_every_segment(&[&format!("{PART_ONE}, any chest"), "Ruins of Gianthold, any chest"]),
        None,
        "a segment naming a current source keeps the item current"
    );
    assert_eq!(legacy_sources.texts_naming_every_segment(&["Temple of Elemental Evil Part Three, any chest"]), None);
    assert_eq!(legacy_sources.texts_naming_every_segment(&[]), None);
}

#[test]
fn rejects_a_legacy_sources_file_with_a_malformed_entry() {
    let rejected_files = [
        legacy_sources_file(""),
        legacy_sources_file("Old Quest") + &legacy_sources_file("old quest"),
        legacy_sources_file("Old Quest").replace("https://", "http://"),
        legacy_sources_file("Old Quest").replace("2026-10-02", "2026-02-30"),
        legacy_sources_file("Old Quest").replace("Test entry.", " "),
        legacy_sources_file("Old Quest") + "kind = \"quest\"\n",
    ];
    for legacy_file in rejected_files {
        assert!(LegacyDropSources::from_toml_str(&legacy_file).is_err(), "{legacy_file}");
    }
    assert!(LegacyDropSources::from_toml_str(&legacy_sources_file("Old Quest")).is_ok());
}
