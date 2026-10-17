use super::{database_delta, table_merge, IdentityIndex, TableMerge, REBUILT_COLUMNS, TABLE_MERGES};
use crate::database_tables::{table_columns, table_names, table_rows_by_rowid};
use rusqlite::Connection;
use std::collections::BTreeSet;
use std::path::Path;

pub(super) fn fixture_database() -> Connection {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import_ddobuilder/ba7e18e/DataFiles");
    let mut original = Connection::open_in_memory().unwrap();
    crate::build::build_database(
        &source,
        &crate::wiki::WikiOverrides::default(),
        &crate::corrections::Corrections::default(),
        &mut original,
        &ddo_model::DatasetVersion { upstream_sha: "fixture".into(), built_at: "2026-10-17T00:00:00Z".into() },
    )
    .unwrap();
    rebuilt_database(&original)
}

pub(super) fn rebuilt_database(original: &Connection) -> Connection {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import_ddobuilder/ba7e18e/DataFiles");
    let root = std::env::temp_dir().join(format!("ddo-delta-{}-{:?}", std::process::id(), std::thread::current().id()));
    crate::tree::export_ddobuilder_tree(original, &source, &root).unwrap();
    let mut rebuilt = Connection::open_in_memory().unwrap();
    crate::tree::build_from_data(&root, &mut rebuilt).unwrap();
    std::fs::remove_dir_all(root).unwrap();
    rebuilt
}

#[test]
fn every_table_has_one_merge_rule() {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(ddo_model::ddl()).unwrap();
    let declared: BTreeSet<_> = TABLE_MERGES.iter().map(|(table, _)| table.to_string()).collect();
    assert_eq!(declared.len(), TABLE_MERGES.len());
    assert_eq!(declared, table_names(&db).unwrap());
    for &(table, column) in REBUILT_COLUMNS {
        assert!(table_columns(&db, table).unwrap().contains(&column.to_string()), "{table}.{column}");
    }
}

#[test]
fn delta_of_two_builds_of_the_same_files_is_empty() {
    let previous = fixture_database();
    let new = fixture_database();
    let delta = database_delta(&previous, &new).unwrap();
    assert!(delta.is_empty(), "{delta}");
    assert_eq!(delta.previous_upstream_sha, "fixture");
    assert_eq!(delta.new_upstream_sha, "fixture");
}

#[test]
fn entity_identities_are_unique() {
    let db = fixture_database();
    let mut index = IdentityIndex::from_db(&db).unwrap();
    for &(table, ref rule) in TABLE_MERGES {
        if !matches!(rule, TableMerge::Entity { .. }) {
            continue;
        }
        let columns = table_columns(&db, table).unwrap();
        let id_column = columns.iter().position(|column| column == "id").unwrap();
        let mut identities = BTreeSet::new();
        for (_, row) in table_rows_by_rowid(&db, table).unwrap() {
            let rusqlite::types::Value::Integer(id) = row[id_column] else { panic!("{table} id") };
            assert!(identities.insert(index.identity(table, id).unwrap()), "{table} {id}");
        }
    }
}

#[test]
fn every_array_row_resolves_its_owner() {
    let db = fixture_database();
    let index = IdentityIndex::from_db(&db).unwrap();
    for &(table, ref rule) in TABLE_MERGES {
        let TableMerge::Array { owner_column, .. } = rule else { continue };
        let columns = table_columns(&db, table).unwrap();
        for (_, row) in table_rows_by_rowid(&db, table).unwrap() {
            let fields = columns.iter().cloned().zip(row).collect();
            let target = index.reference_target(table, owner_column, &fields).unwrap();
            let rusqlite::types::Value::Integer(id) = fields[*owner_column] else { panic!("{table} owner") };
            assert!(index.row(&target, id).is_some(), "{table} -> {target}/{id}");
            assert!(matches!(table_merge(&target), Some(TableMerge::Array { .. } | TableMerge::Entity { .. })));
        }
    }
}

#[test]
fn delta_rejects_different_schema_versions() {
    let previous = fixture_database();
    let new = fixture_database();
    new.execute("UPDATE schema_version SET version = version + 1", []).unwrap();
    assert!(database_delta(&previous, &new).unwrap_err().to_string().contains("schema version"));
}

fn assert_array_reference_contract(db: &Connection) {
    let snapshot = super::compute::DatabaseSnapshot::from_database(db).unwrap();
    for (table, contents) in &snapshot.tables {
        if !matches!(table_merge(table), Some(TableMerge::Array { .. } | TableMerge::Entity { .. })) {
            continue;
        }
        for row_index in 0..contents.rows.len() {
            let address = super::compute::RowAddress { table: table.clone(), row_index };
            for column in contents.columns.iter().filter(|column| column.as_str() != "id") {
                let Some(target) = snapshot.reference(&address, column).unwrap() else { continue };
                match table_merge(&target.table).unwrap() {
                    TableMerge::Entity { .. } => {}
                    TableMerge::Array { .. } => {
                        assert_eq!(
                            snapshot.array_keys.get(&address),
                            snapshot.array_keys.get(&target),
                            "{table}.{column}"
                        );
                        assert!(
                            snapshot.array_keys.contains_key(&address),
                            "entity {table} references array {}",
                            target.table
                        );
                    }
                    rule => panic!("{table}.{column} references {rule:?}"),
                }
            }
        }
    }
}

#[test]
fn array_references_point_to_entities_or_their_own_array() {
    assert_array_reference_contract(&fixture_database());
}

#[test]
fn delta_reports_columns_and_ordered_arrays_separately() {
    let previous = fixture_database();
    let new = fixture_database();
    new.execute("UPDATE enhancements SET cost_per_rank = '[3]' WHERE internal_name = 'SDDefenseBoost'", []).unwrap();
    new.execute("UPDATE dcs SET description = 'Updated check' WHERE icon = 'Knock'", []).unwrap();
    let new = rebuilt_database(&new);
    let delta = database_delta(&previous, &new).unwrap();
    let changes = &delta.entities["enhancements"].changed;
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].column, "cost_per_rank");
    assert_eq!(changes[0].previous, "text:\"[2]\"");
    assert_eq!(changes[0].new, "text:\"[3]\"");
    assert_eq!(delta.arrays.len(), 1);
    let (key, change) = delta.arrays.first_key_value().unwrap();
    assert_eq!(key.array, "dc");
    assert!(key.owner.starts_with("enhancements/"));
    assert_eq!(change.previous.len(), change.new.len());
    assert!(change.new.iter().any(|row| row.contains("Updated check")));
}

#[test]
fn delta_ignores_rebuilt_columns_and_build_times() {
    let previous = fixture_database();
    let new = fixture_database();
    new.execute("UPDATE quests SET provenance = 'wiki'", []).unwrap();
    new.execute("UPDATE dataset_version SET built_at = 'later'", []).unwrap();
    new.execute("UPDATE schema_version SET applied_at = 'later'", []).unwrap();
    assert!(database_delta(&previous, &new).unwrap().is_empty());
}

#[test]
fn delta_query_count_does_not_grow_with_rows() {
    use rusqlite::trace::{TraceEvent, TraceEventCodes};
    use std::sync::atomic::{AtomicUsize, Ordering};
    static QUERY_COUNT: AtomicUsize = AtomicUsize::new(0);
    let mut fixture = fixture_database();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let mut committed = Connection::open_in_memory().unwrap();
    crate::tree::build_from_data(&root, &mut committed).unwrap();
    let mut counts = Vec::new();
    for db in [&mut fixture, &mut committed] {
        db.trace_v2(
            TraceEventCodes::SQLITE_TRACE_STMT,
            Some(|event| {
                if let TraceEvent::Stmt(_, _) = event {
                    QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
                }
            }),
        );
        QUERY_COUNT.store(0, Ordering::SeqCst);
        assert_array_reference_contract(db);
        counts.push(QUERY_COUNT.load(Ordering::SeqCst));
        db.trace_v2(TraceEventCodes::empty(), None);
    }
    assert_eq!(counts[0], counts[1]);
    assert_eq!(counts[0], 1 + 3 * TABLE_MERGES.len());
}
