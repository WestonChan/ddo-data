use rusqlite::Connection;
use std::path::Path;
use xtask::diff_db::{compare_tables, diff_databases, diff_databases_by_slug, DiffReport};

fn fixture_database(path: &Path) {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/ddo-etl/tests/fixtures");
    let mut db = rusqlite::Connection::open(path).unwrap();
    ddo_etl::tree::build_from_data(&fixtures.join("data"), &mut db).unwrap();
}

#[test]
fn database_comparison_exposes_no_difference_allowance() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_xtask")).args(["diff-db", "--help"]).output().unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    for flag in ["--allow-list", "--write-allow-list"] {
        assert!(!help.contains(flag), "retired allowance {flag}: {help}");
    }
    for flag in ["--by-slug", "--against-api"] {
        assert!(help.contains(flag), "missing comparison mode {flag}: {help}");
    }
}

#[test]
fn equal_databases_have_no_table_or_served_response_difference() {
    let work_dir = tempfile::tempdir().unwrap();
    let first = work_dir.path().join("first.db");
    let second = work_dir.path().join("second.db");
    fixture_database(&first);
    std::fs::copy(&first, &second).unwrap();

    let report = diff_databases(&first, &second).unwrap();
    assert!(report.is_equal(), "{report}");
    assert!(report.compared_tables > 80);
    assert!(report.compared_responses > 30);
}

#[test]
fn a_changed_cell_names_its_table() {
    let work_dir = tempfile::tempdir().unwrap();
    let first = work_dir.path().join("first.db");
    let second = work_dir.path().join("second.db");
    fixture_database(&first);
    std::fs::copy(&first, &second).unwrap();
    Connection::open(&second)
        .unwrap()
        .execute(
            "UPDATE items SET description = 'Edited' WHERE id = (SELECT MIN(id) FROM items WHERE is_legacy = 0)",
            [],
        )
        .unwrap();

    let mut report = DiffReport::default();
    compare_tables(&Connection::open(&first).unwrap(), &Connection::open(&second).unwrap(), &mut report).unwrap();
    assert!(
        report
            .table_differences
            .iter()
            .any(|difference| difference.contains("table items:") && difference.contains("column description:")),
        "{report}"
    );
}

#[test]
fn a_missing_row_names_its_table() {
    let work_dir = tempfile::tempdir().unwrap();
    let first = work_dir.path().join("first.db");
    let second = work_dir.path().join("second.db");
    fixture_database(&first);
    std::fs::copy(&first, &second).unwrap();
    Connection::open(&second)
        .unwrap()
        .execute("DELETE FROM bonus_type_aliases WHERE rowid = (SELECT MIN(rowid) FROM bonus_type_aliases)", [])
        .unwrap();

    let mut report = DiffReport::default();
    compare_tables(&Connection::open(&first).unwrap(), &Connection::open(&second).unwrap(), &mut report).unwrap();
    assert!(
        report.table_differences.iter().any(|difference| difference.contains("table bonus_type_aliases:")),
        "{report}"
    );
}

#[test]
fn a_changed_served_field_names_its_route() {
    let work_dir = tempfile::tempdir().unwrap();
    let first = work_dir.path().join("first.db");
    let second = work_dir.path().join("second.db");
    fixture_database(&first);
    std::fs::copy(&first, &second).unwrap();
    Connection::open(&second)
        .unwrap()
        .execute(
            "UPDATE items SET description = 'Edited' WHERE id = (SELECT MIN(id) FROM items WHERE is_legacy = 0)",
            [],
        )
        .unwrap();

    let report = diff_databases(&first, &second).unwrap();
    assert!(report.response_differences.iter().any(|difference| difference.contains("/v1/items")), "{report}");
}

#[test]
fn by_slug_compares_a_renumbered_patron_and_its_quest_references() {
    let work_dir = tempfile::tempdir().unwrap();
    let first = work_dir.path().join("first.db");
    let second = work_dir.path().join("second.db");
    fixture_database(&first);
    std::fs::copy(&first, &second).unwrap();
    let db = Connection::open(&second).unwrap();
    db.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    let old_id: i64 = db.query_row("SELECT MIN(id) FROM patrons", [], |row| row.get(0)).unwrap();
    let new_id = old_id + 1_000_000;
    db.execute("UPDATE quests SET patron_id = ?1 WHERE patron_id = ?2", [new_id, old_id]).unwrap();
    db.execute("UPDATE patrons SET id = ?1 WHERE id = ?2", [new_id, old_id]).unwrap();
    db.execute("UPDATE entity_identity SET entity_id = ?1 WHERE kind = 'patrons' AND entity_id = ?2", [new_id, old_id])
        .unwrap();
    drop(db);

    let report = diff_databases_by_slug(&first, &second).unwrap();
    assert!(report.is_equal(), "{report}");
}

#[test]
fn by_slug_still_reports_a_changed_served_field() {
    let work_dir = tempfile::tempdir().unwrap();
    let first = work_dir.path().join("first.db");
    let second = work_dir.path().join("second.db");
    fixture_database(&first);
    std::fs::copy(&first, &second).unwrap();
    Connection::open(&second)
        .unwrap()
        .execute(
            "UPDATE items SET description = 'Edited' WHERE id = (SELECT MIN(id) FROM items WHERE is_legacy = 0)",
            [],
        )
        .unwrap();

    let report = diff_databases_by_slug(&first, &second).unwrap();
    assert!(!report.is_equal());
    assert!(report.table_differences.iter().any(|difference| difference.contains("table items:")), "{report}");
    assert!(report.response_differences.iter().any(|difference| difference.contains("/v1/items")), "{report}");
}

#[test]
fn by_slug_reports_reordered_nested_rows() {
    let work_dir = tempfile::tempdir().unwrap();
    let first = work_dir.path().join("first.db");
    let second = work_dir.path().join("second.db");
    fixture_database(&first);
    std::fs::copy(&first, &second).unwrap();
    Connection::open(&second)
        .unwrap()
        .execute(
            "UPDATE race_ability_modifiers SET sort_order = 2 - sort_order
             WHERE race_id = (SELECT id FROM races WHERE name = 'Bladeforged')",
            [],
        )
        .unwrap();

    let report = diff_databases_by_slug(&first, &second).unwrap();
    assert!(
        report.table_differences.iter().any(|difference| difference.contains("table race_ability_modifiers:")),
        "{report}"
    );
    assert!(report.response_differences.iter().any(|difference| difference.contains("/v1/races/")), "{report}");
}

#[test]
fn by_slug_compares_a_renumbered_modifier_and_its_requirements() {
    let work_dir = tempfile::tempdir().unwrap();
    let first = work_dir.path().join("first.db");
    let second = work_dir.path().join("second.db");
    fixture_database(&first);
    std::fs::copy(&first, &second).unwrap();
    let db = Connection::open(&second).unwrap();
    let old_id: i64 = db.query_row("SELECT MIN(id) FROM modifiers", [], |row| row.get(0)).unwrap();
    let new_id = old_id + 1_000_000;
    db.execute(
        "UPDATE requirements SET owner_id = ?1 WHERE owner_kind = 'modifier' AND owner_id = ?2",
        [new_id, old_id],
    )
    .unwrap();
    db.execute("UPDATE modifiers SET id = ?1 WHERE id = ?2", [new_id, old_id]).unwrap();
    drop(db);

    let report = diff_databases_by_slug(&first, &second).unwrap();
    assert!(report.is_equal(), "{report}");
}
