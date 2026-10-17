use ddo_etl::database_tables::{table_columns, table_names, table_rows};
use ddo_etl::delta::IdentityIndex;
use ddo_etl::tree::data_file_bytes;
use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::Path;

#[test]
fn shared_identity_resolves_fixture_references() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data");
    let mut db = Connection::open_in_memory().unwrap();
    let fixtures = root.parent().unwrap();
    ddo_etl::build::build_database(
        &fixtures.join("DataFiles"),
        &ddo_etl::wiki::WikiOverrides::from_dir(&fixtures.join("wiki")).unwrap(),
        &ddo_etl::corrections::Corrections::default(),
        &mut db,
        &ddo_model::DatasetVersion {
            upstream_sha: "fixture".to_string(),
            built_at: "2026-10-08T00:00:00Z".to_string(),
        },
    )
    .unwrap();
    let mut identities = IdentityIndex::from_db(&db).unwrap();
    let mut references = 0;
    for table in table_names(&db).unwrap() {
        let columns = table_columns(&db, &table).unwrap();
        for row in table_rows(&db, &table).unwrap() {
            let fields: BTreeMap<_, _> = columns.iter().cloned().zip(row).collect();
            for (column, value) in &fields {
                if column == "id" {
                    continue;
                }
                if let SqlValue::Integer(id) = value {
                    if let Some(target) = identities.reference_target(&table, column, &fields) {
                        assert_eq!(
                            identities.normalized_cell(&table, column, value, &fields).unwrap(),
                            identities.identity(&target, *id).unwrap(),
                            "{table}.{column}"
                        );
                        references += 1;
                    }
                }
            }
        }
    }
    assert!(references > 100);
}

#[test]
fn data_file_bytes_contains_relative_toml_paths_and_exact_bytes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data");
    let files = data_file_bytes(&root).unwrap();
    assert!(files.contains_key(Path::new("upstream/commit.toml")));
    for (path, bytes) in files {
        assert!(path.is_relative());
        assert_eq!(path.extension().unwrap(), "toml");
        assert_eq!(bytes, std::fs::read(root.join(path)).unwrap());
    }
}
