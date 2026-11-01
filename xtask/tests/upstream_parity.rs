use ddo_etl::build::build_database;
use ddo_etl::corrections::Corrections;
use ddo_etl::import_ddobuilder::normalized_upstream_database;
use ddo_etl::wiki::WikiOverrides;
use rusqlite::Connection;
use std::path::Path;
use xtask::upstream_parity::upstream_differences;

#[test]
fn normalized_fixture_matches_and_reports_an_edited_link() {
    let scratch = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/ddo-etl/tests/fixtures/import_ddobuilder/ba7e18e/DataFiles");
    let version = ddo_model::DatasetVersion { upstream_sha: "fixture".into(), built_at: "2026-10-20T00:00:00Z".into() };
    let mut original = Connection::open_in_memory().unwrap();
    let mut grouping_db = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/ddo-etl/tests/fixtures/data"),
        &mut grouping_db,
    )
    .unwrap();
    let groupings = ddo_etl::groupings::Groupings::from_database(&grouping_db).unwrap();
    build_database(&groupings, &source, &WikiOverrides::default(), &Corrections::default(), &mut original, &version)
        .unwrap();
    let exported = scratch.path().join("ours");
    ddo_etl::tree::export_ddobuilder_tree(&original, &source, &exported).unwrap();
    let mut ours = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&exported, &mut ours).unwrap();
    let upstream =
        normalized_upstream_database(&source, &ours, &version, false, &scratch.path().join("upstream")).unwrap();
    assert!(upstream_differences(&upstream.db, &ours).unwrap().is_empty());
    let effect_id: i64 = ours.query_row("SELECT MIN(id) FROM effects WHERE is_stat", [], |row| row.get(0)).unwrap();
    ours.execute("INSERT INTO effect_aliases(effect_id,name,slug,sort_order) VALUES (?1,'Parity Search Alias','parity-search-alias',0)", [effect_id]).unwrap();
    let aliases = upstream_differences(&upstream.db, &ours).unwrap();
    assert_eq!(aliases.len(), 1, "{aliases:?}");
    assert!(aliases[0].starts_with("effect_aliases:"), "{aliases:?}");
    ours.execute("DELETE FROM effect_aliases WHERE name='Parity Search Alias'", []).unwrap();
    let changed = ours.execute(
        "UPDATE feat_effects SET value = value + 1 WHERE rowid = (SELECT MIN(rowid) FROM feat_effects WHERE value IS NOT NULL)",
        [],
    ).unwrap();
    assert_eq!(changed, 1);
    let differences = upstream_differences(&upstream.db, &ours).unwrap();
    assert!(!differences.is_empty(), "an edited link must be reported");
    assert!(differences.iter().all(|difference| difference.contains("feat_effects")), "{differences:?}");
}
