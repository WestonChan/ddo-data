use ddo_etl::corrections::Corrections;
use ddo_etl::wiki::WikiOverrides;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use xtask::dataset::build_database_file;
use xtask::integrity::{integrity_report, CheckStatus, IntegrityOptions, Severity, INTEGRITY_CHECKS};

const TABLES_EMPTY_IN_FIXTURES: [&str; 3] = ["corrections", "effect_damage", "race_feat_slots"];
const PACK_QUEST_OVERLAP_SQL: &str =
    "INSERT INTO sources (kind, pack_id, item_id, augment_id, loot_type, chest, is_rare)
     SELECT 'adventure_pack', q.pack_id, s.item_id, s.augment_id, s.loot_type, s.chest, s.is_rare
     FROM sources s JOIN quests q ON q.id = s.quest_id
     WHERE s.kind = 'quest' AND s.loot_type <> 'reward' AND q.pack_id IS NOT NULL
       AND NOT EXISTS (SELECT 1 FROM sources p WHERE p.kind = 'adventure_pack'
                       AND p.pack_id = q.pack_id AND p.item_id IS s.item_id AND p.augment_id IS s.augment_id)
     LIMIT 1";

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
        data_files_dir: fixtures_dir().join("DataFiles"),
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
fn effect_line_rendering_faults_fail_the_hard_check() {
    for bad_template in ["%b1 Enhancement bonus to Strength.", "Gain {3} bonus to Strength."] {
        let work_dir = tempfile::tempdir().unwrap();
        let db_path = fixture_db_copy_in(work_dir.path());
        let db = Connection::open(&db_path).unwrap();
        db.execute("UPDATE effects SET description_template = ?1 WHERE name = 'Strength'", [bad_template]).unwrap();
        let report = integrity_report(&db, &fixture_options()).unwrap();
        let outcome = report.outcome("effect_line_rendering_tokens").unwrap();
        assert_eq!(outcome.status, CheckStatus::Failed, "{bad_template}: {report}");
    }
}

#[test]
fn text_only_effect_named_after_a_stat_is_warned() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(
        work_dir.path(),
        "INSERT INTO effects (name, verbose_name_template) VALUES ('hitpoints', 'hitpoints');
         INSERT INTO item_effects (item_id, effect_id, sort_order)
         VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 999);",
    );
    let db = Connection::open(&db_path).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("effects_named_after_stats").unwrap();
    assert!(outcome.offenders.iter().any(|offender| offender.name == "hitpoints"));
}

#[test]
fn a_link_granting_only_zero_bonuses_is_warned() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(
        work_dir.path(),
        "UPDATE item_effects SET value = 0 WHERE item_id =
           (SELECT id FROM items WHERE name = 'Epic Ethereal Bracers')
           AND effect_id = (SELECT id FROM effects WHERE name = 'Riposte');",
    );
    let db = Connection::open(&db_path).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("effect_links_with_only_zero_bonuses").unwrap();
    assert!(outcome.offenders.iter().any(|offender| offender.name == "Riposte"));
    let bonus_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM owner_bonuses WHERE owner_kind = 'item'
             AND owner_id = (SELECT id FROM items WHERE name = 'Epic Ethereal Bracers')
             AND via_effect_id = (SELECT id FROM effects WHERE name = 'Riposte')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(bonus_count, 0);
}

#[test]
fn an_unclassified_effect_type_reports_its_family_and_item_count() {
    let source_dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(source_dir.path().join("Items")).unwrap();
    std::fs::write(
        source_dir.path().join("ItemBuffs.xml"),
        "<Buffs><Buff><Type>ProbeFamily</Type><DisplayText>Probe Family: test</DisplayText><Effect><Type>UnclassifiedProbe</Type><AType>NotNeeded</AType></Effect></Buff></Buffs>",
    )
    .unwrap();
    let item_text =
        std::fs::read_to_string(fixtures_dir().join("DataFiles/Items/Celestial Emerald Ring.item")).unwrap();
    std::fs::write(
        source_dir.path().join("Items/Probe.item"),
        item_text.replacen("<Type>Linguistics</Type>", "<Type>ProbeFamily</Type>", 1),
    )
    .unwrap();
    let db = Connection::open(fixture_db_built_once()).unwrap();
    let options = IntegrityOptions { data_files_dir: source_dir.path().to_path_buf(), ..fixture_options() };
    let report = integrity_report(&db, &options).unwrap();
    let outcome = report.outcome("effect_types_not_classified").unwrap();
    assert!(outcome
        .offenders
        .iter()
        .any(|offender| offender.name == "UnclassifiedProbe" && offender.detail == "1 family; 1 item"));
}

#[test]
fn identifier_like_effect_names_are_warned() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(
        work_dir.path(),
        "INSERT INTO effects (name, verbose_name_template) VALUES ('CamelCase', 'Camel Case');
         INSERT INTO item_effects (item_id, effect_id, sort_order)
         VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 999);
         INSERT INTO effects (name, verbose_name_template) VALUES ('Telekinetic117', 'Telekinetic 117');
         INSERT INTO item_effects (item_id, effect_id, sort_order)
         VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 998);
         INSERT INTO effects (name, verbose_name_template) VALUES ('Parrying Number', 'Parrying');
         INSERT INTO effects (name, verbose_name_template) VALUES ('Riposte Riposte', 'Riposte');
         INSERT INTO effects (name, verbose_name_template) VALUES ('Feat Elusive Target', 'Elusive Target');
         INSERT INTO effects (name, verbose_name_template) VALUES ('Penalty Good', 'Penalty');
         INSERT INTO effects (name, verbose_name_template) VALUES ('Energy Absorption Negative', 'Absorption');",
    );
    let db = Connection::open(&db_path).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("effects_named_like_identifiers").unwrap();
    assert!(outcome.offenders.iter().any(|offender| offender.name == "CamelCase"));
    assert!(outcome.offenders.iter().any(|offender| offender.name == "Telekinetic117"));
    for name in
        ["Parrying Number", "Riposte Riposte", "Feat Elusive Target", "Penalty Good", "Energy Absorption Negative"]
    {
        assert!(outcome.offenders.iter().any(|offender| offender.name == name), "{name}");
    }
}

#[test]
fn a_pack_wide_drop_with_the_same_or_unknown_quest_chest_is_hard() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(work_dir.path(), PACK_QUEST_OVERLAP_SQL);
    let db = Connection::open(&db_path).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("pack_wide_drops_repeat_quest_drops").unwrap();
    assert_eq!(outcome.status, CheckStatus::Failed);
    assert_eq!(outcome.offenders.len(), 1);
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
        "INSERT INTO effects (name, verbose_name_template) VALUES ('hitpoints', 'hitpoints');
         INSERT INTO items (name, slot_id, item_category, wiki_url, drop_location, minimum_level)
         VALUES ('Integrity Probe Ring', (SELECT id FROM equipment_slots WHERE name = 'Ring'), 'Jewelry',
                 'https://ddowiki.com/page/Item:Integrity_Probe_Ring', 'Nowhere Keep, chest', 1);
         INSERT INTO item_effects (item_id, effect_id, sort_order)
         VALUES (last_insert_rowid(), (SELECT id FROM effects WHERE name = 'hitpoints'), 999);",
    );

    let output = check_db_output(&db_path);
    let stdout = stdout_of(&output);

    assert!(output.status.success(), "{stdout}");
    assert!(stdout.lines().any(|line| line.starts_with("check items_without_a_source: WARN (")), "{stdout}");
    assert!(stdout.lines().any(|line| line.starts_with("check effects_named_after_stats: WARN (")), "{stdout}");
}

#[test]
fn items_with_a_source_but_no_pack_counts_items_by_kind_and_excludes_unrelated_items() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(
        work_dir.path(),
        "UPDATE vendors SET pack_id = NULL;
         UPDATE crafting_systems SET pack_id = NULL;
         INSERT INTO sources (kind, vendor_id, item_id)
         SELECT 'vendor', v.id, i.id FROM vendors v JOIN items i ON i.name = 'Thunder-Forged Orb';
         INSERT INTO sources (kind, vendor_id, item_id)
         SELECT 'vendor', v.id, i.id FROM vendors v JOIN items i ON i.name = 'Ratkiller (legacy) (level 4)';",
    );
    let db = Connection::open(&db_path).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("items_with_a_source_but_no_pack").expect("the missing-pack warning is registered");
    assert_eq!(outcome.severity, Severity::Warn);
    assert_eq!(outcome.status, CheckStatus::Warned);
    assert_eq!(
        outcome.offenders.iter().map(|offender| offender.name.as_str()).collect::<Vec<_>>(),
        ["Ethereal Great Crossbow", "Thunder-Forged Orb", "Visor of Fraz-Urb'luu"]
    );
    assert!(
        outcome.notes.iter().any(|note| note.contains("crafting_system: 2") && note.contains("vendor: 2")),
        "{report}"
    );
    let output = check_db_output(&db_path);
    assert!(output.status.success(), "{}", stdout_of(&output));
    assert!(stdout_of(&output).contains("check items_with_a_source_but_no_pack: WARN (3 offenders)"));
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
            "effect_templates_disagree_with_names",
            "INSERT INTO effects (name, verbose_name_template) VALUES ('Integrity Title Probe', 'Wrong Title');
             INSERT INTO item_effects (item_id, effect_id, sort_order)
             VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 996);",
            "Integrity Title Probe",
        ),
        violation(
            "skill_groups_share_home_bonus_type",
            "INSERT INTO effects (name, verbose_name_template, is_group)
             VALUES ('Strength Skills', '%b1 Strength Skills +{1}', 1);
             INSERT INTO effect_bonuses (effect_id, target_effect_id, amount_from, sort_order)
             VALUES ((SELECT id FROM effects WHERE name = 'Strength Skills'),
                     (SELECT id FROM effects WHERE name = 'Jump'), 1, 0);
             INSERT INTO item_effects (item_id, effect_id, bonus_type_id, value, sort_order)
             VALUES ((SELECT MIN(id) FROM items), (SELECT id FROM effects WHERE name = 'Strength Skills'),
                     (SELECT id FROM bonus_types WHERE name = 'Insight'), 1, 995);
             UPDATE effects SET home_bonus_type_id = (SELECT id FROM bonus_types WHERE name = 'Insight')
             WHERE name = 'Strength Skills';
             UPDATE effects SET home_bonus_type_id = NULL WHERE name = 'Charisma Skills';",
            "Strength Skills",
        ),
        violation(
            "effects_with_tied_home_bonus_types",
            "INSERT INTO effects (name, verbose_name_template) VALUES ('Integrity Tied Type', '%b1 Integrity Tied Type +{1}');
             INSERT INTO item_effects (item_id, effect_id, bonus_type_id, value, sort_order)
             VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(),
                     (SELECT id FROM bonus_types WHERE name = 'Insight'), 1, 994);
             INSERT INTO effect_vocabulary_counts (kind, id, item_count, augment_count, set_count)
             VALUES ('effect', (SELECT id FROM effects WHERE name = 'Integrity Tied Type'), 2, 0, 0);
             INSERT INTO effect_vocabulary_bonus_types (kind, id, bonus_type_id, item_count)
             SELECT 'effect', (SELECT id FROM effects WHERE name = 'Integrity Tied Type'), id, 1
             FROM bonus_types WHERE name IN ('Insight', 'Equipment');",
            "Integrity Tied Type",
        ),
        violation(
            "owner_bonuses_have_one_stat_type_per_source_line",
            "INSERT INTO effects (name, verbose_name_template) VALUES ('Integrity Double Bonus', '%b1 Integrity Double Bonus +{1}');
             INSERT INTO effect_bonuses (effect_id, target_effect_id, bonus_type_id, amount_from, sort_order)
             VALUES ((SELECT id FROM effects WHERE name = 'Integrity Double Bonus'),
                     (SELECT id FROM effects WHERE name = 'Strength'), NULL, 1, 0);
             INSERT INTO effect_bonuses (effect_id, target_effect_id, bonus_type_id, amount_from, sort_order)
             VALUES ((SELECT id FROM effects WHERE name = 'Integrity Double Bonus'),
                     (SELECT id FROM effects WHERE name = 'Strength'),
                     (SELECT id FROM bonus_types WHERE name = 'Enhancement'), 1, 1);
             INSERT INTO item_effects (item_id, effect_id, value, bonus_type_id, sort_order)
             VALUES ((SELECT MIN(id) FROM items), (SELECT id FROM effects WHERE name = 'Integrity Double Bonus'),
                     1, (SELECT id FROM bonus_types WHERE name = 'Enhancement'), 997);",
            "item:",
        ),
        violation(
            "items_with_a_source_but_no_pack",
            "UPDATE vendors SET pack_id = NULL WHERE name = 'Morten Edgewright';",
            "Ethereal Great Crossbow",
        ),
        violation(
            "items_without_a_source",
            &format!(
                "{} UPDATE items SET drop_location = 'Nowhere Keep, chest' WHERE name = 'Integrity Probe Ring';",
                probe_ring_insert("Integrity Probe Ring")
            ),
            "Integrity Probe Ring",
        ),
        violation(
            "effects_named_after_stats",
            "INSERT INTO effects (name, verbose_name_template) VALUES ('hitpoints', 'hitpoints'); \
             INSERT INTO item_effects (item_id, effect_id, sort_order) \
             VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 999);",
            "hitpoints",
        ),
        violation(
            "effect_templates_with_digits_without_amounts",
            "INSERT INTO effects (name, verbose_name_template) VALUES ('Integrity Probe Glow', 'Glow 7'); \
             INSERT INTO item_effects (item_id, effect_id, sort_order) \
             VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 998);",
            "Integrity Probe Glow",
        ),
        violation(
            "effects_with_amounts_named_with_digits",
            "INSERT INTO effects (name, verbose_name_template)
             VALUES ('Integrity 12 Power', 'Power {1}');
             INSERT INTO item_effects (item_id, effect_id, value, sort_order)
             VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 12, 998);",
            "Integrity 12 Power",
        ),
        violation(
            "effects_with_values_in_names",
            "INSERT INTO effects (name, verbose_name_template) VALUES ('+7 Integrity Probe', '+7 Integrity Probe'); \
             INSERT INTO item_effects (item_id, effect_id, sort_order) \
             VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 998);",
            "+7 Integrity Probe",
        ),
        violation(
            "effects_with_unused_default",
            "INSERT INTO effects (name, verbose_name_template, default_value)
             VALUES ('Integrity Default', 'Default {1}', 7);
             INSERT INTO item_effects (item_id, effect_id, sort_order)
             VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 998);",
            "Integrity Default",
        ),
        violation(
            "effect_link_amount_counts",
            "INSERT INTO effects (name, verbose_name_template)
             VALUES ('Integrity Excess Amount', 'No amount');
             INSERT INTO item_effects (item_id, effect_id, value, sort_order)
             VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 1, 998);",
            "Integrity Excess Amount",
        ),
        violation(
            "effect_stat_amount_sources",
            "UPDATE effect_bonuses SET amount_from = 2 WHERE effect_id =
             (SELECT id FROM effects WHERE name = 'Improved Deception');",
            "Improved Deception",
        ),
        violation(
            "effect_template_placeholders",
            "UPDATE effects SET verbose_name_template = 'Only {2}', description_template = NULL
             WHERE name = 'Improved Deception';",
            "Improved Deception",
        ),
        violation(
            "effect_bonus_type_sources",
            "UPDATE item_effects SET bonus_type_id = NULL WHERE effect_id =
             (SELECT id FROM effects WHERE name = 'Improved Deception');",
            "Improved Deception",
        ),
        violation(
            "effect_bonuses_have_one_rule_per_stat",
            "INSERT INTO effect_bonuses (effect_id, target_effect_id, bonus_type_id, amount_from, sort_order)
             SELECT eb.effect_id, eb.target_effect_id, (SELECT id FROM bonus_types WHERE name = 'Feat'),
                    eb.amount_from, 998 FROM effect_bonuses eb JOIN effects e ON e.id = eb.effect_id
              JOIN effects s ON s.id = eb.target_effect_id WHERE e.name = 'Improved Deception' AND s.name = 'Bluff' LIMIT 1;",
            "Improved Deception",
        ),
        violation(
            "stat_links_have_bonus_types",
            "UPDATE item_effects SET bonus_type_id = NULL WHERE effect_id =
             (SELECT id FROM effects WHERE name = 'Strength');",
            "Strength",
        ),
        violation(
            "stat_links_missing_value",
            "UPDATE item_effects SET value = NULL WHERE effect_id =
             (SELECT id FROM effects WHERE name = 'Strength');",
            "Strength",
        ),
        violation(
            "effect_link_amount_counts",
            "UPDATE item_effects SET value2 = 42 WHERE effect_id =
             (SELECT id FROM effects WHERE name = 'Strength') AND value IS NOT NULL;",
            "Strength",
        ),
        violation(
            "effects_with_dice_but_no_damage_rows",
            "INSERT INTO modifiers (source_kind, source_id, sort_order, effect_type, effect_id, dice_number, dice_sides)
             VALUES ('item', (SELECT MIN(id) FROM items), 998, 'IntegrityDice',
             (SELECT id FROM effects WHERE name = 'Improved Deception'), '[1]', '[6]');",
            "item / IntegrityDice",
        ),
        violation(
            "effects_with_dice_but_no_damage_rows",
            "INSERT INTO modifiers (source_kind, source_id, sort_order, effect_type, dice_number, dice_sides)
             VALUES ('item', (SELECT MIN(id) FROM items), 998, 'UnresolvedDice', '[1]', '[6]');",
            "item / UnresolvedDice",
        ),
        violation(
            "effect_families_have_owners",
            "INSERT INTO effects (name, verbose_name_template)
             VALUES ('Integrity Orphan Effect', 'Integrity Orphan Effect');",
            "Integrity Orphan Effect",
        ),
        violation(
            "effect_groups_have_flat_members",
            "INSERT INTO effects (name, verbose_name_template, is_group) VALUES ('Integrity Empty Group', 'Integrity Empty Group +{1}', 1);",
            "Integrity Empty Group",
        ),
        violation(
            "effect_tier_groups_have_steps",
            "INSERT INTO effect_tier_groups (name) VALUES ('Integrity Tier Group');",
            "Integrity Tier Group",
        ),
        violation(
            "effect_names_have_no_em_dash",
            "INSERT INTO effects (name, verbose_name_template)
             VALUES ('Integrity — Prose', 'Integrity prose');
             INSERT INTO item_effects (item_id, effect_id, sort_order)
             VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 997);",
            "Integrity — Prose",
        ),
        violation(
            "set_tier_lines_do_not_repeat_structured_facts",
            "INSERT INTO effects (name, verbose_name_template)
             SELECT 'Integrity Duplicate Tier Fact',
                    REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(e.verbose_name_template,
                    '+{1}', '{1}'), '+{2}', '{2}'),
                    '{1}', printf('%+d', COALESCE(te.value, e.default_value))),
                    '{2}', COALESCE(printf('%+d', COALESCE(te.value2, e.default_value2)), '')),
                    '%b1', COALESCE(bt.name, ''))
             FROM set_bonus_tier_effects te JOIN effects e ON e.id = te.effect_id
             LEFT JOIN bonus_types bt ON bt.id = te.bonus_type_id
             WHERE EXISTS (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)
             ORDER BY te.tier_id, te.sort_order LIMIT 1;
             INSERT INTO set_bonus_tier_effects (tier_id, effect_id, sort_order)
             SELECT te.tier_id, (SELECT id FROM effects WHERE name = 'Integrity Duplicate Tier Fact'), 999
             FROM set_bonus_tier_effects te JOIN effects e ON e.id = te.effect_id
             WHERE EXISTS (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)
             ORDER BY te.tier_id, te.sort_order LIMIT 1;",
            "Kundarak Delving Equipment",
        ),
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
        violation("pack_wide_drops_repeat_quest_drops", PACK_QUEST_OVERLAP_SQL, ""),
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
        violation("items_without_effects", &probe_ring_insert("Integrity Probe Ring"), "Integrity Probe Ring"),
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
            "unreferenced_stats",
            "INSERT INTO effects (name, is_stat, category) VALUES ('Integrity Probe Stat', 1, 'probe');",
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
fn items_without_effects_counts_every_property_an_item_can_carry() {
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
        .outcome("items_without_effects")
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
    assert!(
        outcome.notes.iter().any(|note| note == "allowed empty: corrections, effect_damage, race_feat_slots"),
        "{report}"
    );

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

#[test]
fn a_socket_type_an_option_grants_or_holds_options_needs_no_augment() {
    let injected_sql = "INSERT INTO augment_slot_types (label, family, variant)
         VALUES ('crafting: integrity probe granted', 'crafting', 'integrity probe granted');
         INSERT INTO item_augment_slot_option_grants (option_id, sort_order, slot_id)
         VALUES ((SELECT MIN(id) FROM item_augment_slot_options), 99, last_insert_rowid());
         INSERT INTO augment_slot_types (label, family, variant)
         VALUES ('crafting: integrity probe upgrade', 'crafting', 'integrity probe upgrade');
         INSERT INTO item_augment_slots (item_id, sort_order, slot_id)
         VALUES ((SELECT MIN(id) FROM items), 99, last_insert_rowid());
         INSERT INTO item_augment_slot_options (item_id, slot_order, option_order, name)
         VALUES ((SELECT MIN(id) FROM items), 99, 0, 'Integrity Probe Upgrade');
         INSERT INTO augment_slot_types (label, family, variant)
         VALUES ('crafting: integrity probe bare', 'crafting', 'integrity probe bare');";
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(work_dir.path(), injected_sql);
    let db = Connection::open(&db_path).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let offender_names: Vec<&str> = report
        .outcome("slot_types_no_augment_fits")
        .unwrap()
        .offenders
        .iter()
        .map(|offender| offender.name.as_str())
        .collect();

    assert!(offender_names.contains(&"crafting: integrity probe bare"), "{report}");
    assert!(
        !offender_names.contains(&"crafting: integrity probe granted"),
        "an option's granted socket is filled by the option:\n{report}"
    );
    assert!(
        !offender_names.contains(&"crafting: integrity probe upgrade"),
        "a socket holding upgrade options is filled by them:\n{report}"
    );
}

#[test]
fn a_quest_chain_or_saga_without_quests_fails_the_deploy() {
    let check = INTEGRITY_CHECKS.iter().find(|check| check.name == "chains_and_sagas_have_quests").unwrap();
    assert_eq!(check.severity, Severity::Hard, "every chain and saga in the wiki files lists its quests");
}
