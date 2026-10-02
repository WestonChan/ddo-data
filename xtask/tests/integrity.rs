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
    assert!(stdout.lines().any(|line| line == "check raid_loot_only_on_raids: ok"), "{stdout}");
}

#[test]
fn raid_loot_from_a_quest_that_is_no_raid_fails_and_exits_non_zero() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(work_dir.path(), &probe_quest_source_insert("raid", "NULL"));

    let output = check_db_output(&db_path);
    let stdout = stdout_of(&output);

    assert!(!output.status.success(), "{stdout}");
    assert!(stdout.lines().any(|line| line == "check raid_loot_only_on_raids: FAIL (1 offender)"), "{stdout}");
    assert!(stdout.contains("Integrity Probe Quest"), "{stdout}");
    assert!(stdout.lines().any(|line| line == "check trees_have_enhancements: ok"), "{stdout}");
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
    injected_sql: String,
    offender_name: &'static str,
}

fn violation(check_name: &'static str, injected_sql: &str, offender_name: &'static str) -> InjectedViolation {
    InjectedViolation { check_name, injected_sql: injected_sql.to_string(), offender_name }
}

fn probe_ring_insert(name: &str) -> String {
    format!(
        "INSERT INTO items (name, slot_id, item_category, wiki_url, minimum_level, description)
         VALUES ('{name}', (SELECT id FROM equipment_slots WHERE name = 'Ring'), 'Jewelry',
                 'https://ddowiki.com/page/Item:Integrity_Probe', 1, 'A probe.');"
    )
}

const PROBE_QUEST_INSERT: &str = "INSERT INTO quests (name, pack_id, is_raid)
     VALUES ('Integrity Probe Quest', (SELECT MIN(id) FROM adventure_packs), 0);";

fn probe_quest_source_insert(loot_type: &str, chest: &str) -> String {
    format!(
        "{PROBE_QUEST_INSERT}
         INSERT INTO sources (kind, quest_id, item_id, loot_type, chest)
         VALUES ('quest', last_insert_rowid(), (SELECT MIN(id) FROM items), '{loot_type}', {chest});"
    )
}

fn injected_violations() -> Vec<InjectedViolation> {
    vec![
        violation(
            "items_without_a_source",
            &format!(
                "{} UPDATE items SET drop_location = 'Nowhere Keep, chest' WHERE name = 'Integrity Probe Ring';",
                probe_ring_insert("Integrity Probe Ring")
            ),
            "Integrity Probe Ring",
        ),
        violation("effects_named_after_stats", "INSERT INTO effects (name) VALUES ('hitpoints');", "hitpoints"),
        violation("tables_not_empty", "DELETE FROM guild_buffs;", "guild_buffs"),
        violation(
            "weapon_and_armor_stats_match_category",
            "DELETE FROM item_weapon_stats WHERE item_id = (SELECT MIN(id) FROM items WHERE item_category = 'Weapon');",
            "",
        ),
        violation(
            "weapon_and_armor_stats_match_category",
            "INSERT INTO item_armor_stats (item_id, armor_type)
             VALUES ((SELECT MIN(id) FROM items WHERE item_category = 'Jewelry'), 'Light');",
            "",
        ),
        violation("raid_loot_only_on_raids", &probe_quest_source_insert("raid", "NULL"), "Integrity Probe Quest"),
        violation(
            "item_sockets_use_known_labels",
            "INSERT INTO augment_slot_types (label, family, variant) VALUES ('mystery: probe', 'mystery', 'probe');
             INSERT INTO item_augment_slots (item_id, sort_order, slot_id)
             VALUES ((SELECT MIN(id) FROM items), 900, last_insert_rowid());",
            "",
        ),
        violation(
            "trees_have_enhancements",
            "DELETE FROM enhancements WHERE tree_id = (SELECT MIN(id) FROM enhancement_trees);",
            "",
        ),
        violation(
            "classes_have_full_progression",
            "UPDATE classes SET hit_points = 0 WHERE name = 'Paladin';",
            "Paladin",
        ),
        violation(
            "classes_have_full_progression",
            "UPDATE classes SET bab = '[0, 1, 2]' WHERE name = 'Dark Apostate';",
            "Dark Apostate",
        ),
        violation(
            "items_have_minimum_level",
            &probe_ring_insert("Integrity Probe Ring").replace(", 1, 'A probe.'", ", 0, 'A probe.'"),
            "Integrity Probe Ring",
        ),
        violation(
            "augments_have_slot_and_family",
            "INSERT INTO augments (name, family) VALUES ('Integrity Probe Augment', 'Ruby');",
            "Integrity Probe Augment",
        ),
        violation(
            "chains_and_sagas_have_quests",
            "INSERT INTO quest_chains (name, provenance, wiki_url)
             VALUES ('Integrity Probe Chain', 'wiki', 'https://ddowiki.com/page/Integrity_Probe_Chain');",
            "Integrity Probe Chain",
        ),
        violation("items_without_enchantments", &probe_ring_insert("Integrity Probe Ring"), "Integrity Probe Ring"),
        violation(
            "items_without_description",
            &probe_ring_insert("Integrity Probe Ring").replace("'A probe.'", "NULL"),
            "Integrity Probe Ring",
        ),
        violation(
            "near_duplicate_item_names",
            &format!("{} {}", probe_ring_insert("Integrity Probe Ring"), probe_ring_insert("integrity-probe ring")),
            "integrity-probe ring",
        ),
        violation(
            "raids_without_raid_loot",
            &PROBE_QUEST_INSERT.replace("adventure_packs), 0)", "adventure_packs), 1)"),
            "Integrity Probe Quest",
        ),
        violation("quests_without_loot", PROBE_QUEST_INSERT, "Integrity Probe Quest"),
        violation(
            "unreferenced_effects",
            "INSERT INTO effects (name) VALUES ('Integrity Probe Effect');",
            "Integrity Probe Effect",
        ),
        violation(
            "unreferenced_bonuses",
            "INSERT INTO bonuses (name, stat_id, bonus_type_id, value)
             VALUES ('Integrity Probe +77', (SELECT MIN(id) FROM stats), (SELECT MIN(id) FROM bonus_types), 77);",
            "Integrity Probe +77",
        ),
        violation(
            "unreferenced_stats",
            "INSERT INTO stats (name, category) VALUES ('Integrity Probe Stat', 'probe');",
            "Integrity Probe Stat",
        ),
        violation(
            "slot_types_no_augment_fits",
            "INSERT INTO augment_slot_types (label, family, variant)
             VALUES ('crafting: integrity probe', 'crafting', 'integrity probe');",
            "crafting: integrity probe",
        ),
        violation(
            "sets_without_members",
            "INSERT INTO set_bonuses (name) VALUES ('Integrity Probe Set');",
            "Integrity Probe Set",
        ),
    ]
}

#[test]
fn each_injected_violation_fails_or_warns_its_own_check() {
    for violation in injected_violations() {
        let work_dir = tempfile::tempdir().unwrap();
        let db_path = fixture_db_copy_with(work_dir.path(), &violation.injected_sql);
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
fn legacy_item_needs_a_legacy_name_a_correction_or_no_sources() {
    let item_with_sources = "(SELECT MIN(item_id) FROM sources)";
    assert_eq!(
        legacy_outcome_status(&format!("UPDATE items SET is_legacy = 1 WHERE id = {item_with_sources};")),
        CheckStatus::Failed
    );
    assert_eq!(
        legacy_outcome_status(&format!(
            "UPDATE items SET is_legacy = 1, name = name || ' (legacy)' WHERE id = {item_with_sources};"
        )),
        CheckStatus::Passed
    );
    assert_eq!(
        legacy_outcome_status(&format!(
            "UPDATE items SET is_legacy = 1 WHERE id = {item_with_sources};
             INSERT INTO corrections (kind, name, field, from_value, to_value, reason, source, read)
             SELECT 'item', name, 'is_legacy', 'false', 'true', 'Probe.', 'https://ddowiki.com/page/Probe', '2026-10-02'
             FROM items WHERE id = {item_with_sources};"
        )),
        CheckStatus::Passed
    );
    assert_eq!(
        legacy_outcome_status(
            "UPDATE items SET is_legacy = 1 WHERE id = (SELECT MIN(id) FROM items i
             WHERE NOT EXISTS (SELECT 1 FROM sources d WHERE d.item_id = i.id));"
        ),
        CheckStatus::Passed
    );
}

#[test]
fn items_without_a_source_ranks_the_heads_of_their_drop_locations() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(
        work_dir.path(),
        "INSERT INTO items (name, slot_id, item_category, wiki_url, drop_location) VALUES
         ('Integrity Probe Ring', (SELECT id FROM equipment_slots WHERE name = 'Ring'), 'Jewelry',
          'https://ddowiki.com/page/Item:Integrity_Probe_Ring', 'Nowhere Keep, chest'),
         ('Integrity Probe Belt', (SELECT id FROM equipment_slots WHERE name = 'Waist'), 'Clothing',
          'https://ddowiki.com/page/Item:Integrity_Probe_Belt', 'Nowhere Keep');",
    );
    let db = Connection::open(&db_path).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("items_without_a_source").unwrap();

    assert!(outcome.top_details.contains(&("Nowhere Keep".to_string(), 2)), "{report}");
    assert!(outcome.top_details.len() <= 15, "{report}");
}

#[test]
fn items_without_enchantments_counts_every_property_an_item_can_carry() {
    let property_inserts = [
        ("Integrity Probe Enhanced Ring", "UPDATE items SET enhancement_bonus = 3 WHERE id = last_insert_rowid();"),
        (
            "Integrity Probe Socketed Ring",
            "INSERT INTO item_augment_slots (item_id, sort_order, slot_id)
             VALUES (last_insert_rowid(), 0, (SELECT MIN(id) FROM augment_slot_types));",
        ),
        (
            "Integrity Probe Clicky Ring",
            "INSERT INTO item_clickies (item_id, sort_order, name) VALUES (last_insert_rowid(), 0, 'Probe Clicky');",
        ),
        (
            "Integrity Probe Modified Ring",
            "INSERT INTO modifiers (source_kind, source_id, sort_order, effect_type)
             VALUES ('item', last_insert_rowid(), 0, 'MaxDexBonus');",
        ),
        (
            "Integrity Probe Set Ring",
            "INSERT INTO set_bonus_items (set_id, item_id) VALUES ((SELECT MIN(id) FROM set_bonuses), last_insert_rowid());",
        ),
    ];
    let mut injected_sql = probe_ring_insert("Integrity Probe Bare Ring");
    for (probe_name, property_insert) in property_inserts {
        injected_sql.push_str(&probe_ring_insert(probe_name));
        injected_sql.push_str(property_insert);
    }
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(work_dir.path(), &injected_sql);
    let db = Connection::open(&db_path).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let offender_names: Vec<&str> = report
        .outcome("items_without_enchantments")
        .unwrap()
        .offenders
        .iter()
        .map(|offender| offender.name.as_str())
        .collect();

    assert!(offender_names.contains(&"Integrity Probe Bare Ring"), "{report}");
    for (probe_name, _) in property_inserts {
        assert!(!offender_names.contains(&probe_name), "{probe_name} carries a property:\n{report}");
    }
}

#[test]
fn tables_not_empty_prints_the_tables_allowed_to_be_empty() {
    let db = Connection::open(fixture_db_built_once()).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("tables_not_empty").unwrap();

    assert_eq!(outcome.status, CheckStatus::Passed, "{report}");
    assert!(outcome.notes.iter().any(|note| note == "allowed empty: corrections, race_feat_slots"), "{report}");

    let options_allowing_nothing = IntegrityOptions { allowed_empty_tables: Vec::new(), ..fixture_options() };
    let strict_report = integrity_report(&db, &options_allowing_nothing).unwrap();
    let strict_outcome = strict_report.outcome("tables_not_empty").unwrap();
    assert_eq!(strict_outcome.status, CheckStatus::Failed, "{strict_report}");
    assert!(strict_outcome.notes.iter().any(|note| note == "allowed empty: none"), "{strict_report}");
}

#[test]
fn a_correction_the_build_did_not_apply_fails_no_stale_corrections() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = work_dir.path().join("fixture-with-corrections.db");
    let corrections = Corrections::from_dir(&fixtures_dir().join("corrections")).unwrap();
    let wiki_overrides = WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap();
    build_database_file(&fixtures_dir().join("DataFiles"), &wiki_overrides, &corrections, &db_path).unwrap();
    let db = Connection::open(&db_path).unwrap();
    let options = IntegrityOptions { corrections, ..fixture_options() };

    let report = integrity_report(&db, &options).unwrap();
    let outcome = report.outcome("no_stale_corrections").unwrap();

    assert_eq!(outcome.status, CheckStatus::Failed, "{report}");
    assert_eq!(outcome.offenders.len(), 1, "{report}");
    assert!(outcome.offenders[0].name.contains("Ruby of Acid Damage"), "{report}");
}

#[test]
fn the_unknown_placeholder_class_needs_no_progression() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(
        work_dir.path(),
        "INSERT INTO classes (name, skill_points, hit_points, fortitude, reflex, will, bab, spell_points_per_level)
         VALUES ('Unknown', 2, 0, 'none', 'none', 'none',
                 '[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]', '[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]');",
    );
    let db = Connection::open(&db_path).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();

    assert_eq!(report.outcome("classes_have_full_progression").unwrap().status, CheckStatus::Passed, "{report}");
}

#[test]
fn a_set_only_an_augment_slot_option_joins_has_members() {
    let injected_sql = "INSERT INTO set_bonuses (name) VALUES ('Integrity Probe Option Set');
         INSERT INTO item_augment_slot_option_sets (option_id, set_id)
         VALUES ((SELECT MIN(id) FROM item_augment_slot_options), last_insert_rowid());
         INSERT INTO set_bonuses (name) VALUES ('Integrity Probe Bare Set');";
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(work_dir.path(), injected_sql);
    let db = Connection::open(&db_path).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let offender_names: Vec<&str> = report
        .outcome("sets_without_members")
        .unwrap()
        .offenders
        .iter()
        .map(|offender| offender.name.as_str())
        .collect();

    assert!(offender_names.contains(&"Integrity Probe Bare Set"), "{report}");
    assert!(
        !offender_names.contains(&"Integrity Probe Option Set"),
        "an item counts toward its option's set once the player unlocks it:\n{report}"
    );
}
