use ddo_etl::corrections::Corrections;
use ddo_etl::wiki::WikiOverrides;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use xtask::dataset::build_database_file;
use xtask::integrity::{integrity_report, CheckStatus, IntegrityOptions, Severity, INTEGRITY_CHECKS};

const TABLES_EMPTY_IN_FIXTURES: [&str; 2] = ["corrections", "race_feat_slots"];

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/ddo-etl/tests/fixtures")
}

fn fixture_db_built_once() -> &'static Path {
    static FIXTURE_DB: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    let (_, db_path) = FIXTURE_DB.get_or_init(|| {
        let work_dir = tempfile::tempdir().unwrap();
        let db_path = work_dir.path().join("fixture.db");
        let wiki_overrides = WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap();
        build_database_file(&fixtures_dir().join("DataFiles"), &wiki_overrides, &Corrections::default(), &db_path)
            .unwrap();
        (work_dir, db_path)
    });
    db_path
}

fn fixture_db_copy_in(work_dir: &Path) -> PathBuf {
    let db_path = work_dir.join("fixture-copy.db");
    std::fs::copy(fixture_db_built_once(), &db_path).unwrap();
    db_path
}

fn fixture_db_copy_with(work_dir: &Path, injected_sql: &str) -> PathBuf {
    let db_path = fixture_db_copy_in(work_dir);
    let db = Connection::open(&db_path).unwrap();
    db.execute_batch("PRAGMA foreign_keys = OFF; PRAGMA ignore_check_constraints = ON;").unwrap();
    db.execute_batch(injected_sql).unwrap_or_else(|e| panic!("{injected_sql}: {e}"));
    db_path
}

fn fixture_options() -> IntegrityOptions {
    IntegrityOptions {
        corrections: Corrections::default(),
        allowed_empty_tables: TABLES_EMPTY_IN_FIXTURES.iter().map(|table| table.to_string()).collect(),
    }
}

fn check_db_output(db_path: &Path) -> Output {
    let no_corrections_dir = tempfile::tempdir().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
    command.arg("check-db").arg(db_path).arg("--corrections").arg(no_corrections_dir.path());
    for table in TABLES_EMPTY_IN_FIXTURES {
        command.args(["--allow-empty-table", table]);
    }
    command.output().unwrap()
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

#[test]
fn clean_fixture_database_passes_every_hard_check() {
    let db = Connection::open(fixture_db_built_once()).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();

    assert_eq!(report.failed_hard_check_names(), Vec::<&str>::new(), "{report}");
    for check in INTEGRITY_CHECKS.iter().filter(|check| check.severity == Severity::Hard) {
        let outcome = report.outcome(check.name).unwrap_or_else(|| panic!("no outcome for {}", check.name));
        assert!(matches!(outcome.status, CheckStatus::Passed | CheckStatus::Skipped), "{report}");
    }
}

#[test]
fn clean_fixture_database_exits_zero_and_prints_every_check() {
    let output = check_db_output(fixture_db_built_once());
    let stdout = stdout_of(&output);

    assert!(output.status.success(), "{stdout}{}", String::from_utf8_lossy(&output.stderr));
    for check in INTEGRITY_CHECKS {
        assert!(stdout.lines().any(|line| line.starts_with(&format!("check {}: ", check.name))), "{stdout}");
    }
    assert!(stdout.lines().any(|line| line == "check quests_have_packs: ok"), "{stdout}");
}

#[test]
fn quest_without_a_pack_fails_quests_have_packs_and_exits_non_zero() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(
        work_dir.path(),
        "INSERT INTO quests (name, is_challenge) VALUES ('Integrity Probe Quest', 0);",
    );

    let output = check_db_output(&db_path);
    let stdout = stdout_of(&output);

    assert!(!output.status.success(), "{stdout}");
    assert!(stdout.lines().any(|line| line == "check quests_have_packs: FAIL (1 offender)"), "{stdout}");
    assert!(stdout.contains("Integrity Probe Quest"), "{stdout}");
    assert!(stdout.lines().any(|line| line == "check items_have_names_and_slots: ok"), "{stdout}");
}

#[test]
fn a_warning_never_fails_the_command() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(
        work_dir.path(),
        "INSERT INTO effects (name) VALUES ('hitpoints');
         INSERT INTO items (name, slot_id, item_category, wiki_url, drop_location, minimum_level)
         VALUES ('Integrity Probe Ring', (SELECT id FROM equipment_slots WHERE name = 'Ring'), 'Jewelry',
                 'https://ddowiki.com/page/Item:Integrity_Probe_Ring', 'Nowhere Keep, chest', 1);",
    );

    let output = check_db_output(&db_path);
    let stdout = stdout_of(&output);

    assert!(output.status.success(), "{stdout}");
    assert!(stdout.lines().any(|line| line.starts_with("check items_without_a_source: WARN (")), "{stdout}");
    assert!(stdout.lines().any(|line| line.starts_with("check effects_named_after_stats: WARN (")), "{stdout}");
}

struct InjectedViolation {
    check_name: &'static str,
    injected_sql: &'static str,
    offender_name: &'static str,
}

const INJECTED_VIOLATIONS: &[InjectedViolation] = &[
    InjectedViolation {
        check_name: "drops_reference_existing_rows",
        injected_sql: "INSERT INTO drops (source_kind, quest_id, item_id, loot_type)
                       VALUES ('quest', (SELECT MIN(id) FROM quests), 999999, 'chest');",
        offender_name: "drops",
    },
    InjectedViolation {
        check_name: "items_have_names_and_slots",
        injected_sql: "UPDATE items SET name = '  ' WHERE id = (SELECT MIN(id) FROM items);",
        offender_name: "  ",
    },
    InjectedViolation {
        check_name: "items_have_names_and_slots",
        injected_sql: "UPDATE items SET slot_id = 999 WHERE name = (SELECT MAX(name) FROM items);",
        offender_name: "",
    },
    InjectedViolation {
        check_name: "quests_have_packs",
        injected_sql: "INSERT INTO quests (name, is_challenge) VALUES ('Integrity Probe Quest', 0);",
        offender_name: "Integrity Probe Quest",
    },
    InjectedViolation {
        check_name: "wiki_rows_have_pages",
        injected_sql: "INSERT INTO items (name, slot_id, item_category, source)
                       VALUES ('Integrity Probe Wiki Ring', (SELECT id FROM equipment_slots WHERE name = 'Ring'),
                               'Jewelry', 'wiki');",
        offender_name: "Integrity Probe Wiki Ring",
    },
    InjectedViolation {
        check_name: "items_without_a_source",
        injected_sql: "INSERT INTO items (name, slot_id, item_category, drop_location)
                       VALUES ('Integrity Probe Ring', (SELECT id FROM equipment_slots WHERE name = 'Ring'),
                               'Jewelry', 'Nowhere Keep, chest');",
        offender_name: "Integrity Probe Ring",
    },
    InjectedViolation {
        check_name: "effects_named_after_stats",
        injected_sql: "INSERT INTO effects (name) VALUES ('hitpoints');",
        offender_name: "hitpoints",
    },
    InjectedViolation {
        check_name: "untyped_item_bonuses",
        injected_sql: "INSERT INTO bonuses (name, stat_id, bonus_type_id, value)
                       VALUES ('Integrity Probe +77', (SELECT MIN(id) FROM stats), NULL, 77);
                       INSERT INTO item_bonuses (item_id, bonus_id, sort_order)
                       VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 900);",
        offender_name: "",
    },
];

#[test]
fn each_injected_violation_fails_or_warns_its_own_check() {
    for violation in INJECTED_VIOLATIONS {
        let work_dir = tempfile::tempdir().unwrap();
        let db_path = fixture_db_copy_with(work_dir.path(), violation.injected_sql);
        let db = Connection::open(&db_path).unwrap();
        let report = integrity_report(&db, &fixture_options()).unwrap();
        let check = INTEGRITY_CHECKS.iter().find(|check| check.name == violation.check_name).unwrap();
        let outcome = report.outcome(violation.check_name).unwrap();

        let expected_status = match check.severity {
            Severity::Hard => CheckStatus::Failed,
            Severity::Warn => CheckStatus::Warned,
        };
        assert_eq!(outcome.status, expected_status, "{}:\n{report}", violation.check_name);
        assert!(
            outcome.offenders.iter().any(|offender| offender.name.contains(violation.offender_name)),
            "{} lacks {:?}:\n{report}",
            violation.check_name,
            violation.offender_name
        );
        assert_eq!(report.failed_hard_check_names().is_empty(), check.severity == Severity::Warn, "{report}");
    }
}

#[test]
fn legacy_check_is_skipped_with_a_note_while_items_have_no_is_legacy_column() {
    let db = Connection::open(fixture_db_built_once()).unwrap();
    let has_is_legacy: bool = db
        .query_row("SELECT COUNT(*) > 0 FROM pragma_table_info('items') WHERE name = 'is_legacy'", [], |row| row.get(0))
        .unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("legacy_items_hidden").unwrap();

    if has_is_legacy {
        assert_eq!(outcome.status, CheckStatus::Passed, "{report}");
    } else {
        assert_eq!(outcome.status, CheckStatus::Skipped, "{report}");
        assert!(outcome.notes.iter().any(|note| note.contains("items.is_legacy")), "{report}");
    }
}

fn legacy_outcome_status(flagging_sql: &str) -> CheckStatus {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_in(work_dir.path());
    let db = Connection::open(&db_path).unwrap();
    let has_is_legacy: bool = db
        .query_row("SELECT COUNT(*) > 0 FROM pragma_table_info('items') WHERE name = 'is_legacy'", [], |row| row.get(0))
        .unwrap();
    if !has_is_legacy {
        db.execute_batch("ALTER TABLE items ADD COLUMN is_legacy INTEGER NOT NULL DEFAULT 0;").unwrap();
    }
    db.execute_batch(flagging_sql).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    report.outcome("legacy_items_hidden").unwrap().status
}

#[test]
fn legacy_item_needs_a_legacy_name_a_correction_or_no_drops() {
    let item_with_drops = "(SELECT MIN(item_id) FROM drops)";
    assert_eq!(
        legacy_outcome_status(&format!("UPDATE items SET is_legacy = 1 WHERE id = {item_with_drops};")),
        CheckStatus::Failed
    );
    assert_eq!(
        legacy_outcome_status(&format!(
            "UPDATE items SET is_legacy = 1, name = name || ' (legacy)' WHERE id = {item_with_drops};"
        )),
        CheckStatus::Passed
    );
    assert_eq!(
        legacy_outcome_status(&format!(
            "UPDATE items SET is_legacy = 1 WHERE id = {item_with_drops};
             INSERT INTO corrections (kind, name, field, from_value, to_value, reason, source, read)
             SELECT 'item', name, 'is_legacy', 'false', 'true', 'Probe.', 'https://ddowiki.com/page/Probe', '2026-10-02'
             FROM items WHERE id = {item_with_drops};"
        )),
        CheckStatus::Passed
    );
    assert_eq!(
        legacy_outcome_status(
            "UPDATE items SET is_legacy = 1 WHERE id = (SELECT MIN(id) FROM items i
             WHERE NOT EXISTS (SELECT 1 FROM drops d WHERE d.item_id = i.id));"
        ),
        CheckStatus::Passed
    );
}

#[test]
fn items_without_a_source_ranks_the_heads_of_their_drop_locations() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(
        work_dir.path(),
        "INSERT INTO items (name, slot_id, item_category, drop_location) VALUES
         ('Integrity Probe Ring', (SELECT id FROM equipment_slots WHERE name = 'Ring'), 'Jewelry', 'Nowhere Keep, chest'),
         ('Integrity Probe Belt', (SELECT id FROM equipment_slots WHERE name = 'Waist'), 'Clothing', 'Nowhere Keep');",
    );
    let db = Connection::open(&db_path).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("items_without_a_source").unwrap();

    assert!(outcome.top_details.contains(&("Nowhere Keep".to_string(), 2)), "{report}");
    assert!(outcome.top_details.len() <= 15, "{report}");
}

#[test]
fn wiki_rows_have_pages_notes_each_table_without_a_wiki_url_column() {
    let db = Connection::open(fixture_db_built_once()).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("wiki_rows_have_pages").unwrap();

    for table in ["quests", "augments"] {
        assert!(
            outcome.notes.iter().any(|note| note.starts_with(&format!("{table} has no wiki_url column"))),
            "{report}"
        );
    }
}
