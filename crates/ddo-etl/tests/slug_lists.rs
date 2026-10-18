use ddo_model::enums::EntityKind;
use rusqlite::Connection;
use std::collections::BTreeSet;
use std::path::Path;

#[test]
fn fixture_identity_kinds_match_the_entity_kind_declaration() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut db = Connection::open_in_memory().unwrap();
    ddo_etl::build::build_database(
        &fixtures.join("DataFiles"),
        &ddo_etl::wiki::WikiOverrides::from_dir(&fixtures.join("wiki")).unwrap(),
        &ddo_etl::corrections::Corrections::default(),
        &mut db,
        &ddo_model::DatasetVersion {
            upstream_sha: "fixture".to_string(),
            built_at: "2026-10-17T00:00:00Z".to_string(),
        },
    )
    .unwrap();
    let actual: BTreeSet<String> = db
        .prepare("SELECT DISTINCT kind FROM entity_identity")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let expected = EntityKind::ALL.iter().map(|kind| kind.table_name().to_string()).collect();
    assert_eq!(actual, expected);
}
