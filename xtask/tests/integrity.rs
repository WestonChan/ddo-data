use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use xtask::integrity::{integrity_report, CheckStatus, IntegrityOptions, Severity, INTEGRITY_CHECKS};

const TABLES_EMPTY_IN_FIXTURES: [&str; 3] = ["effect_damage", "race_feat_slots", "removed_entities"];
const PACK_QUEST_OVERLAP_SQL: &str =
    "INSERT INTO pack_drops (owner_kind, pack_id, owner_id, loot_type, chest, is_rare)
     SELECT CASE WHEN s.item_id IS NOT NULL THEN 'item' ELSE 'augment' END, q.pack_id, COALESCE(s.item_id, s.augment_id), s.loot_type, s.chest, s.is_rare
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
        let mut db = Connection::open(&db_path).unwrap();
        let tree = work_dir.path().join("data");
        copy_fixture_tree(&tree);
        let production = fixtures_dir().join("../../../../data/effects");
        for name in ["shadow-striker", "baphomets-tactics", "stat-groups/combat-mastery"] {
            let contents = std::fs::read_to_string(production.join(format!("{name}.toml"))).unwrap();
            let contents = if name == "shadow-striker" { contents.replace("+20%", "+15%") } else { contents };
            std::fs::write(tree.join(format!("effects/{name}.toml")), contents).unwrap();
        }
        let carrier = tree.join("packs/vault-of-night/quests/plane-of-night/gear/kundarak-delving-boots.toml");
        let mut contents = std::fs::read_to_string(&carrier).unwrap();
        contents.push_str("\n[[effect]]\nname = \"Shadow Striker\"\n\n[[effect]]\nname = \"Baphomet's Tactics\"\n");
        std::fs::write(carrier, contents).unwrap();
        ddo_etl::tree::build_from_data(&tree, &mut db).unwrap();
        (work_dir, db_path)
    });
    db_path
}

fn copy_fixture_tree(destination: &Path) {
    let source = fixtures_dir().join("data");
    for entry in walkdir::WalkDir::new(&source) {
        let entry = entry.unwrap();
        if entry.file_type().is_file() {
            let target = destination.join(entry.path().strip_prefix(&source).unwrap());
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
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
    IntegrityOptions { allowed_empty_tables: TABLES_EMPTY_IN_FIXTURES.iter().map(|table| table.to_string()).collect() }
}

#[test]
fn open_verification_requests_list_each_target_once_and_remain_warnings() {
    let scratch = tempfile::tempdir().unwrap();
    let tree = scratch.path().join("data");
    copy_fixture_tree(&tree);
    let path = tree.join("packs/shared/gear/alabaster-of-the-twelve.toml");
    let original = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        format!(
            r#"{original}
[[effect]]
name = "Charisma"
bonus_type = "Penalty"
value = -4
[[verification_request]]
kind = "item_bonus"
field = "value"
qualifier = "Charisma / Penalty"
question = "Does the recorded penalty apply?"
assumed = -4
raised = "2026-10-23"
[[verification_request.source]]
url = "https://ddowiki.com/page/Item:Alabaster_of_the_Twelve"
read = "2026-10-23"
says = "The typed list says minus four."
[[verification_request.source]]
in_game = true
read = "2026-10-23"
says = "The screenshot says minus four."
[[verification_request]]
question = "Is this the current item version?"
assumed = "As recorded"
raised = "2026-10-23"
[[verification_request.source]]
in_game = true
read = "2026-10-23"
says = "The item appears unchanged."
"#
        ),
    )
    .unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&tree, &mut db).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("open_verification_requests").expect("verification queue is registered");
    assert_eq!(outcome.status, CheckStatus::Warned);
    assert!(report.failed_hard_check_names().is_empty(), "{report}");
    assert_eq!(outcome.offenders.len(), 2);
    let owner_id: i64 =
        db.query_row("SELECT id FROM items WHERE name = 'Alabaster of the Twelve'", [], |row| row.get(0)).unwrap();
    for offender in &outcome.offenders {
        assert_eq!(offender.name, "Alabaster of the Twelve");
        assert_eq!(offender.id, Some(owner_id));
    }
    assert!(outcome.offenders.iter().any(|offender| offender.detail
        == "item_bonus value Charisma / Penalty: Does the recorded penalty apply?; assumed: -4"));
    assert!(
        outcome
            .offenders
            .iter()
            .any(|offender| offender.detail
                == "whole entity: Is this the current item version?; assumed: \"As recorded\"")
    );
    let check = INTEGRITY_CHECKS.iter().find(|check| check.name == "open_verification_requests").unwrap();
    assert_eq!(check.severity, Severity::Warn);
    assert!(check.description.contains("to-check queue"));
    std::fs::write(path, original).unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&tree, &mut db).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("open_verification_requests").unwrap();
    assert_eq!(outcome.status, CheckStatus::Passed);
    assert!(outcome.offenders.is_empty());
}

fn template_violations() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("T1", "Doublestrike and Doubleshot", "UPDATE effects SET description_template = REPLACE(description_template, '{1}', '5') WHERE name = 'Doublestrike and Doubleshot'"),
        ("T2", "Doublestrike and Doubleshot", "UPDATE effects SET verbose_name_template = REPLACE(verbose_name_template, '{1}', '5') WHERE name = 'Doublestrike and Doubleshot'"),
        ("T2", "Doublestrike and Doubleshot", "UPDATE effects SET verbose_name_template = REPLACE(verbose_name_template, '{1}', '{2}') WHERE name = 'Doublestrike and Doubleshot'"),
        ("T3", "Doublestrike and Doubleshot", "UPDATE effects SET description_template = REPLACE(description_template, '%b1', 'Enhancement') WHERE name = 'Doublestrike and Doubleshot'"),
        ("T3", "Doublestrike and Doubleshot", "UPDATE effects SET set_bonus_line_template = REPLACE(set_bonus_line_template, '%b1', 'Enhancement') WHERE name = 'Doublestrike and Doubleshot'"),
        ("T4", "Command", "UPDATE effects SET description_template = REPLACE(description_template, 'Insightful', '') WHERE name = 'Command'"),
        ("T5", "Doublestrike and Doubleshot", "UPDATE effects SET description_template = REPLACE(description_template, '{1}%', '{1}') WHERE name = 'Doublestrike and Doubleshot'"),
        ("T6", "Shadow Striker", "UPDATE effects SET description_template = REPLACE(description_template, '+15% Enhancement Bonus to Ranged', '+20% Enhancement Bonus to Ranged') WHERE name = 'Shadow Striker'"),
        ("T7", "Unmodelled Amount", "INSERT INTO effects (name, slug, verbose_name_template) VALUES ('Unmodelled Amount', 'unmodelled-amount', 'Unmodelled Amount +7%'); INSERT INTO item_effects (item_id, effect_id, sort_order) SELECT (SELECT MIN(id) FROM items), id, 9999 FROM effects WHERE name = 'Unmodelled Amount'"),
    ]
}

#[test]
fn alias_templates_share_line_amount_and_type_checks() {
    let scratch = tempfile::tempdir().unwrap();
    let db = Connection::open(fixture_db_copy_in(scratch.path())).unwrap();
    db.execute("INSERT INTO effect_aliases (effect_id, name, slug, sort_order) SELECT id, 'Search Only', 'search-only', 0 FROM effects WHERE name='Spot'", []).unwrap();
    for (owner, name, template, cause) in [
        ("Doublestrike and Doubleshot", "Alias Slot", "%b1 Alias Slot +8%", "T2"),
        ("Doublestrike and Doubleshot", "Alias Type", "Alias Type +{1}%", "T3"),
        ("Command", "Alias Fixed Type", "Alias Fixed Type", "T4"),
        ("Shadow Striker", "Alias Constant", "Alias Constant +99%", "T6"),
        ("True Seeing", "Alias Unmodelled", "Alias Unmodelled +5", "T7"),
        ("Spot", "Alias Stat", "Alias Stat +{1}", "T3"),
    ] {
        let changed = db.execute("INSERT INTO effect_aliases (effect_id, name, slug, verbose_name_template, sort_order) SELECT id, ?2, ?3, ?4, (SELECT COUNT(*) FROM effect_aliases WHERE effect_id=effects.id) FROM effects WHERE name=?1",
            rusqlite::params![owner, name, ddo_model::identity::slug(name), template]).unwrap();
        assert_eq!(changed, 1, "{owner}");
        let report = integrity_report(&db, &fixture_options()).unwrap();
        let outcome = report.outcome("effect_templates_without_their_placeholders").unwrap();
        assert!(
            outcome.offenders.iter().any(|finding| finding.name == name && finding.detail.contains(cause)),
            "{name}: {outcome}"
        );
    }
    db.execute_batch("UPDATE effect_aliases SET verbose_name_template = '%b1 Alias Type +{1}%' WHERE name = 'Alias Type'; UPDATE effect_aliases SET verbose_name_template = '%b1 Alias Stat +{1}' WHERE name = 'Alias Stat'").unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("effect_templates_without_their_placeholders").unwrap();
    assert!(
        !outcome.offenders.iter().any(|finding| matches!(finding.name.as_str(), "Alias Type" | "Alias Stat")),
        "{outcome}"
    );
}

#[test]
fn template_check_reports_each_injected_cause() {
    let mut missing = Vec::new();
    for (cause, effect_name, injected_sql) in template_violations() {
        let scratch = tempfile::tempdir().unwrap();
        let db = Connection::open(fixture_db_copy_with(scratch.path(), injected_sql)).unwrap();
        let report = integrity_report(&db, &fixture_options()).unwrap();
        let outcome = report.outcome("effect_templates_without_their_placeholders").unwrap();
        let findings = outcome.offenders.iter().filter(|offender| offender.name == effect_name).collect::<Vec<_>>();
        if findings.len() != 1 || !findings[0].detail.contains(cause) {
            missing.push(format!("{cause}: {effect_name}: {findings:?}"));
        }
        if cause == "T5" {
            let rendered = report.outcome("effect_descriptions_without_their_amounts").unwrap();
            assert!(!rendered.offenders.iter().any(|offender| offender.name == effect_name), "{rendered}");
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
fn constant_literals_use_declared_units_and_matching_values() {
    let scratch = tempfile::tempdir().unwrap();
    let db = Connection::open(fixture_db_copy_in(scratch.path())).unwrap();
    let has_constant_finding = |name: &str| {
        integrity_report(&db, &fixture_options())
            .unwrap()
            .outcome("effect_templates_without_their_placeholders")
            .unwrap()
            .offenders
            .iter()
            .any(|offender| offender.name == name && offender.detail.contains("T6"))
    };
    assert!(!has_constant_finding("Shadow Striker"));
    assert!(!has_constant_finding("Baphomet's Tactics"));
    db.execute("UPDATE effects SET description_template = REPLACE(description_template, '+3%', '+3') WHERE name = 'Shadow Striker'", []).unwrap();
    assert!(has_constant_finding("Shadow Striker"));
    db.execute("UPDATE effects SET description_template = REPLACE(description_template, '+3 ', '+3% ') WHERE name = 'Shadow Striker'", []).unwrap();
    assert!(!has_constant_finding("Shadow Striker"));
    db.execute("UPDATE effects SET description_template = REPLACE(description_template, '+15% Enhancement Bonus to Ranged', '+20% Enhancement Bonus to Ranged') WHERE name = 'Shadow Striker'", []).unwrap();
    assert!(has_constant_finding("Shadow Striker"));
    db.execute("UPDATE effects SET description_template = REPLACE(description_template, '+20% Enhancement Bonus to Ranged', '+15% Enhancement Bonus to Ranged') WHERE name = 'Shadow Striker'", []).unwrap();
    assert!(!has_constant_finding("Shadow Striker"));
}

#[test]
fn mixed_unit_templates_state_each_unit_without_renderer_restoration() {
    let scratch = tempfile::tempdir().unwrap();
    let tree = scratch.path().join("data");
    copy_fixture_tree(&tree);
    let production = fixtures_dir().join("../../../../data/effects/stat-groups");
    let names =
        ["dodge-and-dodge-cap", "melee-power-and-doublestrike", "doublestrike-doubleshot-and-offhand-strike-chance"];
    let mut group_names = Vec::new();
    for slug in names {
        let path = production.join(format!("{slug}.toml"));
        let contents = std::fs::read_to_string(&path).unwrap();
        let parsed: toml::Value = toml::from_str(&contents).unwrap();
        group_names.push(parsed["name"].as_str().unwrap().to_string());
        std::fs::copy(path, tree.join(format!("effects/stat-groups/{slug}.toml"))).unwrap();
    }
    let mut db = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&tree, &mut db).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("effect_templates_without_their_placeholders").unwrap();
    let incorrect =
        outcome.offenders.iter().filter(|offender| group_names.contains(&offender.name)).collect::<Vec<_>>();
    assert!(incorrect.is_empty(), "{incorrect:?}");
}

#[test]
fn set_lines_state_their_raw_bonus_type() {
    let scratch = tempfile::tempdir().unwrap();
    let tree = scratch.path().join("data");
    copy_fixture_tree(&tree);
    std::fs::write(
        tree.join("packs/shared/sets/insight-lines.toml"),
        r#"name = "Insight Lines"
[[tier]]
equipped_count = 2
[[tier.effect]]
effect = "Melee Threat Generation"
bonus_type = "Insight"
value = 20
[[tier.effect]]
effect = "Saving Throws"
bonus_type = "Insight"
value = 3
[[tier.effect]]
effect = "Sneak Attack Hit"
bonus_type = "Insight"
value = 4
[[tier.effect]]
effect = "Sneak Attack Damage"
bonus_type = "Insight"
value = 4
"#,
    )
    .unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&tree, &mut db).unwrap();
    let owner: i64 = db
        .query_row(
            "SELECT t.id FROM set_bonus_tiers t JOIN set_bonuses s ON s.id=t.set_id WHERE s.name='Insight Lines'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let lines = ddo_api::served_effect_lines(&db, "set_bonus_tier_effects", "tier_id").unwrap();
    assert_eq!(lines[&owner].len(), 4);
    for (_, line) in &lines[&owner] {
        assert!(line["description"].is_null());
        assert!(line["verbose_name"].as_str().unwrap().contains("Insight Bonus"));
    }
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("effect_descriptions_without_their_amounts").unwrap();
    let incorrect = outcome
        .offenders
        .iter()
        .filter(|finding| {
            finding.detail.contains(&format!("set_bonus_tier_effects owner {owner},"))
                && finding.detail.contains("type missing")
        })
        .collect::<Vec<_>>();
    assert!(incorrect.is_empty(), "{incorrect:?}");
}

#[test]
fn a_fixed_home_type_fills_the_rendering_token_without_a_link_type() {
    let scratch = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_in(scratch.path());
    let db = Connection::open(db_path).unwrap();
    db.execute("UPDATE effects SET description_template='{1}% %b1 bonus to Fire and Electric Spell Lore.' WHERE name='Firestorm Lore'", []).unwrap();
    db.execute(
        "INSERT INTO item_effects (item_id, effect_id, sort_order, value)
        SELECT (SELECT MIN(id) FROM items),id,995,4 FROM effects WHERE name='Firestorm Lore'",
        [],
    )
    .unwrap();
    let lines = ddo_api::served_effect_lines(&db, "item_effects", "item_id").unwrap();
    let line = lines.values().flatten().find(|(_, line)| line["name"] == "Firestorm Lore").unwrap();
    assert!(line.1["description"].as_str().unwrap().contains("Equipment"));
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("effect_line_rendering_tokens").unwrap();
    assert!(!outcome.offenders.iter().any(|finding| finding.name == "Firestorm Lore"), "{:?}", outcome.offenders);
}

#[test]
fn template_amount_literals_need_a_definition_but_ranks_and_context_numbers_do_not() {
    let scratch = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_in(scratch.path());
    let db = Connection::open(db_path).unwrap();
    for (name, template) in [
        ("Unmodelled Amount", "Unmodelled Amount +7%"),
        ("Rank Probe 7", "Rank Probe 7"),
        ("Roman Probe VI", "Roman Probe VI"),
        ("Dice Probe", "Dice Probe 15d6"),
        ("DC Probe", "DC Probe DC +25"),
        ("Level Probe", "Level Probe level +25"),
        ("Time Probe", "Time Probe for +10 seconds"),
    ] {
        db.execute(
            "INSERT INTO effects (name,slug,verbose_name_template) VALUES (?1,?2,?3)",
            [name, &name.to_ascii_lowercase().replace(" ", "-"), template],
        )
        .unwrap();
    }
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("effect_templates_without_their_placeholders").unwrap();
    let finding = outcome.offenders.iter().find(|finding| finding.name == "Unmodelled Amount");
    assert!(finding.is_some_and(|finding| finding.detail.contains("T7")), "{:?}", outcome.offenders);
    for name in ["Rank Probe 7", "Roman Probe VI", "Dice Probe", "DC Probe", "Level Probe", "Time Probe"] {
        assert!(!outcome.offenders.iter().any(|finding| finding.name == name), "{name}: {:?}", outcome.offenders);
    }
}

#[test]
fn digit_warnings_classify_unmodelled_numbers_without_duplicate_literal_findings() {
    let scratch = tempfile::tempdir().unwrap();
    let tree = scratch.path().join("data");
    copy_fixture_tree(&tree);
    let cases = [
        (
            "Acidic 3",
            Some("Acidic 3"),
            Some("Deals 3d6 Acid damage."),
            None,
            Some("rank-named step, nothing modelled"),
            None,
        ),
        ("Rank With Type 3", Some("%b1 Rank With Type 3"), None, None, Some("rank-named step, nothing modelled"), None),
        ("DR 5/-", Some("DR 5/-"), None, None, Some("unmodelled mechanic number"), None),
        ("On Hit Reduction", Some("Reduce resistance by 10"), None, None, Some("unmodelled mechanic number"), None),
        ("Charges", Some("Absorb Spells - 5 Charges"), None, None, Some("unmodelled mechanic number"), None),
        ("Dice Ammunition", Some("Deals 8d6 Fire damage"), None, None, Some("unmodelled mechanic number"), None),
        ("3rd Degree Burns", Some("3rd Degree Burns"), None, None, Some("unmodelled mechanic number"), None),
        ("Difficulty Class", Some("DC +25"), None, None, Some("unmodelled mechanic number"), None),
        ("Level Threshold", Some("level 25%"), None, None, Some("unmodelled mechanic number"), None),
        ("Challenge Rating", Some("CR 4"), None, None, Some("unmodelled mechanic number"), None),
        ("Rank Threshold", Some("rank +3"), None, None, Some("unmodelled mechanic number"), None),
        ("Tier Threshold", Some("tier +2"), None, None, Some("unmodelled mechanic number"), None),
        ("Duration", Some("Lasts +10 seconds"), None, None, Some("unmodelled mechanic number"), None),
        (
            "Dragon's Edge 7",
            Some("Dragon's Edge 7 +22"),
            Some("Grants +21 Damage."),
            Some(21),
            Some("rank-named step beside a modelled constant"),
            Some("T6"),
        ),
        (
            "Constant Charges",
            Some("Constant Charges 5"),
            Some("Grants +5 Damage."),
            Some(5),
            Some("number beside a modelled constant"),
            None,
        ),
        ("Constant Amount", Some("Constant Amount +5"), Some("Grants +5 Damage."), Some(5), None, None),
        ("+2 Charges of Something 3", Some("+2 Charges of Something 3"), None, None, None, Some("T7")),
        ("Negative Literal", Some("Rank 3 -2"), None, None, None, Some("T7")),
        ("Unsigned Percent", Some("Rank 3 2%"), None, None, None, Some("T7")),
        ("Description Dice", Some("Description Dice"), Some("Deals 3d6 Fire damage."), None, None, None),
        ("Line Slot", Some("Line Slot {1} 3"), None, None, None, None),
        ("Description Slot", Some("Description Slot 3"), Some("Grants {1}."), None, None, None),
        ("Empty Line", Some(""), None, None, None, None),
        ("Roman VI", Some("Roman VI"), None, None, None, None),
        ("Type Token", Some("%b1 Type Token"), None, None, None, None),
    ];
    for (name, line, description, constant, _, _) in &cases {
        let mut contents = format!("name = {name:?}\n");
        if let Some(line) = line {
            contents.push_str(&format!("verbose_name_template = {line:?}\n"));
        }
        if let Some(description) = description {
            contents.push_str(&format!("description_template = {description:?}\n"));
        }
        if let Some(constant) = constant {
            contents.push_str(&format!("[[bonus]]\ntarget = \"Damage Bonus\"\namount_from = 0\nconstant = {constant}\nbonus_type = \"Equipment\"\nscale = 1.0\nrounding = \"down\"\n"));
        }
        std::fs::write(tree.join(format!("effects/{}.toml", ddo_model::identity::slug(name))), contents).unwrap();
    }
    let mut db = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&tree, &mut db).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let digits = report.outcome("effect_templates_with_digits_without_amounts").expect("digits WARN is registered");
    assert_eq!(digits.severity, Severity::Warn);
    let templates = report.outcome("effect_templates_without_their_placeholders").unwrap();
    for (name, _, _, _, digit_cause, template_cause) in cases {
        let findings = digits.offenders.iter().filter(|finding| finding.name == name).collect::<Vec<_>>();
        assert_eq!(findings.len(), usize::from(digit_cause.is_some()), "{name}: {findings:?}");
        if let Some(cause) = digit_cause {
            assert_eq!(findings[0].detail, cause, "{name}");
        }
        let literal_causes = templates
            .offenders
            .iter()
            .filter(|finding| finding.name == name)
            .flat_map(|finding| finding.detail.split("; "))
            .filter(|cause| cause.contains("T6:") || cause.contains("T7:"))
            .collect::<Vec<_>>();
        assert_eq!(literal_causes.len(), usize::from(template_cause.is_some()), "{name}: {literal_causes:?}");
        if let Some(cause) = template_cause {
            assert!(literal_causes[0].contains(cause), "{name}: {literal_causes:?}");
        }
    }
}

#[test]
fn digit_warnings_exclude_either_amount_slot_in_either_template() {
    let scratch = tempfile::tempdir().unwrap();
    let tree = scratch.path().join("data");
    copy_fixture_tree(&tree);
    let mut names = Vec::new();
    for slot in [1, 2] {
        for slot_in_line in [false, true] {
            let name = format!("Slot {slot} in {}", if slot_in_line { "line" } else { "description" });
            let amount = format!("+{{{slot}}} %b1 Damage Bonus");
            let line = if slot_in_line { format!("Rank 3 {amount}") } else { "Rank 3".to_string() };
            let description = if slot_in_line { "Damage bonus." } else { &amount };
            std::fs::write(
                tree.join(format!("effects/{}.toml", ddo_model::identity::slug(&name))),
                format!(
                    "name = {name:?}\nverbose_name_template = {line:?}\ndescription_template = {description:?}\n\
                     [[bonus]]\ntarget = \"Damage Bonus\"\namount_from = {slot}\nscale = 1.0\nrounding = \"down\"\n"
                ),
            )
            .unwrap();
            names.push(name);
        }
    }
    let mut db = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&tree, &mut db).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let digits = report.outcome("effect_templates_with_digits_without_amounts").unwrap();
    let unexpected = digits.offenders.iter().filter(|finding| names.contains(&finding.name)).collect::<Vec<_>>();
    assert!(unexpected.is_empty(), "{unexpected:?}");
}

#[test]
fn fixed_template_types_share_the_served_hidden_type_rule() {
    let scratch = tempfile::tempdir().unwrap();
    let tree = scratch.path().join("data");
    copy_fixture_tree(&tree);
    let production = fixtures_dir().join("../../../../data/effects");
    for slug in [
        "rams-might",
        "ghostly",
        "enhanced-ghostly",
        "hidden-effect-all-threat",
        "stealth-strike",
        "eternal-faith",
        "blood",
    ] {
        std::fs::copy(production.join(format!("{slug}.toml")), tree.join(format!("effects/{slug}.toml"))).unwrap();
    }
    let mut db = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&tree, &mut db).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("effect_templates_without_their_placeholders").unwrap();
    let checked_names = [
        "Ram's Might",
        "Ghostly",
        "Enhanced Ghostly",
        "Hidden Effect All Threat",
        "Stealth Strike",
        "Eternal Faith",
        "Blood",
    ];
    let missing_types = outcome
        .offenders
        .iter()
        .filter(|finding| checked_names.contains(&finding.name.as_str()) && finding.detail.contains("T4:"))
        .map(|finding| finding.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(missing_types, ["Blood"]);
}

#[test]
fn mixed_constant_and_slot_templates_assign_each_defect_once() {
    let scratch = tempfile::tempdir().unwrap();
    let tree = scratch.path().join("data");
    copy_fixture_tree(&tree);
    std::fs::write(
        tree.join("effects/mixed-amounts.toml"),
        r#"name = "Mixed Amounts"
description_template = "+{1} %b1 bonus to Armor Class and +3% Artifact bonus to Doublestrike."
verbose_name_template = "Mixed Amounts +{1}"
set_bonus_line_template = "+{1} %b1 Bonus to Armor Class and +3% Artifact Bonus to Doublestrike"
[[bonus]]
target = "Armor Class"
amount_from = 1
scale = 1.0
rounding = "down"
[[bonus]]
target = "Doublestrike"
amount_from = 0
constant = 3
bonus_type = "Artifact"
scale = 1.0
rounding = "down"
"#,
    )
    .unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&tree, &mut db).unwrap();
    let mut failures = Vec::new();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("effect_templates_without_their_placeholders").unwrap();
    if outcome.offenders.iter().any(|finding| finding.name == "Mixed Amounts" && finding.detail.contains("T5")) {
        failures.push("flat slot 1 acquired the constant row's percent unit".to_string());
    }
    for column in ["verbose_name_template", "set_bonus_line_template"] {
        db.execute(
            &format!("UPDATE effects SET {column}='Mixed Amounts +{{1}} and +4%' WHERE name='Mixed Amounts'"),
            [],
        )
        .unwrap();
        let report = integrity_report(&db, &fixture_options()).unwrap();
        let outcome = report.outcome("effect_templates_without_their_placeholders").unwrap();
        let finding = outcome.offenders.iter().find(|finding| finding.name == "Mixed Amounts").unwrap();
        if !finding.detail.contains("T6") || finding.detail.contains("T2") {
            failures.push(format!("{column}: {}", finding.detail));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn served_type_check_requires_the_granted_type() {
    let scratch = tempfile::tempdir().unwrap();
    let db = Connection::open(fixture_db_copy_with(scratch.path(),
        "UPDATE effects SET description_template = REPLACE(description_template, 'Insightful', 'Profane') WHERE name = 'Command'"
    )).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("effect_descriptions_without_their_amounts").unwrap();
    assert!(
        outcome.offenders.iter().any(|offender| offender.name == "Command" && offender.detail.contains("type missing")),
        "{outcome}"
    );
}

#[test]
fn description_skip_flags_only_suppress_their_target_stat() {
    let mut failures = Vec::new();
    for (flagged, target, description, missing_amount, stale_flag) in [
        (true, "Strength", "+3 Strength and +4 Dexterity.", false, false),
        (false, "Strength", "+3 Strength and +4 Dexterity.", true, false),
        (true, "Strength", "+2 Strength and +4 Dexterity.", false, true),
        (true, "Dexterity", "+3 Strength and +4 Dexterity.", true, true),
    ] {
        let scratch = tempfile::tempdir().unwrap();
        let tree = scratch.path().join("data");
        copy_fixture_tree(&tree);
        std::fs::write(
            tree.join("effects/reviewed-bonuses.toml"),
            format!(
                r#"name = "Reviewed Bonuses"
verbose_name_template = "Reviewed Bonuses"
description_template = {description:?}
[[bonus]]
target = "Strength"
amount_from = 0
constant = 2
bonus_type = "Enhancement"
scale = 1.0
rounding = "down"
[[bonus]]
target = "Dexterity"
amount_from = 0
constant = 4
bonus_type = "Enhancement"
scale = 1.0
rounding = "down"
"#
            ),
        )
        .unwrap();
        let owner = tree.join("packs/shared/gear/alabaster-of-the-twelve.toml");
        let original = std::fs::read_to_string(&owner).unwrap();
        let flag = if flagged { "skip_description_check = true\n" } else { "" };
        std::fs::write(
            &owner,
            format!(
                r#"{original}
[[effect]]
name = "Reviewed Bonuses"
[[game_bug_correction]]
kind = "item_bonus"
field = "value"
qualifier = "{target} / Enhancement"
intended = 3
reason = "The tooltip states the intended bonus."
in_game = true
read = "2026-10-23"
{flag}"#
            ),
        )
        .unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        ddo_etl::tree::build_from_data(&tree, &mut db).unwrap();
        let report = integrity_report(&db, &fixture_options()).unwrap();
        let outcome = report.outcome("effect_descriptions_without_their_amounts").unwrap();
        let missing = outcome.offenders.iter().any(|finding| {
            finding.name == "Reviewed Bonuses"
                && finding.detail.contains("description omits an amount or states a different one")
        });
        let stale = outcome.offenders.iter().any(|finding| {
            finding.detail.contains("skip flag with nothing to skip") && finding.detail.contains(target)
        });
        if (missing, stale) != (missing_amount, stale_flag) {
            failures.push(format!("{flagged} {target} {description}: expected ({missing_amount}, {stale_flag}), got ({missing}, {stale}): {outcome}"));
        }
        let template_finding = report
            .outcome("effect_templates_without_their_placeholders")
            .unwrap()
            .offenders
            .iter()
            .any(|finding| finding.name == "Reviewed Bonuses" && finding.detail.contains("T6"));
        assert_eq!(template_finding, description.contains("+3"), "8c is unaffected");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_group_description_skip_covers_both_lines_and_becomes_stale_once() {
    let scratch = tempfile::tempdir().unwrap();
    let tree = scratch.path().join("data");
    copy_fixture_tree(&tree);
    let owner = tree.join("packs/shared/augments/named/storms-bulwark.toml");
    let original = std::fs::read_to_string(&owner).unwrap();
    std::fs::write(
        &owner,
        format!(
            r#"{original}
[[effect]]
name = "Doublestrike"
bonus_type = "Enhancement"
value = 3
[[effect]]
name = "Doubleshot"
bonus_type = "Enhancement"
value = 3
[[game_bug_correction]]
kind = "augment_bonus"
field = "lines"
qualifier = "Doublestrike and Doubleshot / Enhancement"
intended = "Doublestrike and Doubleshot"
reason = "The tooltip combines these bonuses."
in_game = true
read = "2026-10-23"
skip_description_check = true
"#
        ),
    )
    .unwrap();
    let stat = tree.join("effects/stats/doublestrike.toml");
    let original_stat = std::fs::read_to_string(&stat).unwrap();
    for (description, expected_stale) in [("Grants a combat bonus.", 0), ("Grants +{1}% Doublestrike.", 1)] {
        std::fs::write(&stat, format!("{original_stat}\ndescription_template = {description:?}\n")).unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        ddo_etl::tree::build_from_data(&tree, &mut db).unwrap();
        let line_count: i64 =
            db.query_row("SELECT COUNT(*) FROM note_lines WHERE note_kind = 'game_bug'", [], |row| row.get(0)).unwrap();
        assert_eq!(line_count, 2);
        let report = integrity_report(&db, &fixture_options()).unwrap();
        let outcome = report.outcome("effect_descriptions_without_their_amounts").unwrap();
        let owner_id: i64 =
            db.query_row("SELECT id FROM augments WHERE name = 'Storm''s Bulwark'", [], |row| row.get(0)).unwrap();
        assert!(
            !outcome
                .offenders
                .iter()
                .any(|finding| finding.detail.contains(&format!("augment_effects owner {owner_id},"))),
            "{outcome}"
        );
        let stale = outcome
            .offenders
            .iter()
            .filter(|finding| {
                finding.name == "Storm's Bulwark" && finding.detail.contains("skip flag with nothing to skip")
            })
            .count();
        assert_eq!(stale, expected_stale, "{outcome}");
    }
}

#[test]
fn confirmed_percent_stats_render_units_on_their_real_set_lines() {
    let scratch = tempfile::tempdir().unwrap();
    let tree = scratch.path().join("data");
    copy_fixture_tree(&tree);
    let production = fixtures_dir().join("../../../../data");
    for path in [
        "effects/stats/cold-spell-critical-damage.toml",
        "effects/stats/missile-deflection.toml",
        "effects/stat-groups/doublestrike-and-doubleshot.toml",
        "effects/stat-groups/melee-and-ranged-attack-speed.toml",
        "effects/movement-speed-on-tumble.toml",
        "packs/shared/sets/frozen-wanderer.toml",
        "packs/shared/sets/dance-of-the-wind.toml",
        "packs/shared/sets/celerity.toml",
        "icons/sets/FrozenWanderer.png",
        "icons/sets/DanceOfTheWind.png",
        "icons/sets/Celerity.png",
    ] {
        let destination = tree.join(path);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::copy(production.join(path), destination).unwrap();
    }
    let mut db = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&tree, &mut db).unwrap();
    let lines = ddo_api::served_effect_lines(&db, "set_bonus_tier_effects", "tier_id").unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("effect_descriptions_without_their_amounts").unwrap();
    for (owner, stat, amount) in [
        ("Frozen Wanderer", "Cold Spell Critical Damage", 5),
        ("Dance of the Wind", "Missile Deflection", 5),
        ("Celerity", "Doublestrike and Doubleshot", 2),
    ] {
        let tier: i64 = db.query_row(
            "SELECT t.id FROM set_bonus_tiers t JOIN set_bonuses s ON s.id=t.set_id WHERE s.name=?1 AND t.equipped_count=2",
            [owner], |row| row.get(0)
        ).unwrap();
        assert_eq!(lines[&tier].len(), 1, "{owner}");
        assert_eq!(lines[&tier][0].1["verbose_name"], format!("+{amount}% Stacking Bonus to {stat}"), "{owner}");
        assert!(
            !outcome
                .offenders
                .iter()
                .any(|finding| finding.detail.contains(&format!("set_bonus_tier_effects owner {tier},"))),
            "{owner}: {outcome}"
        );
    }
}

#[test]
fn served_amount_check_distinguishes_restored_units_from_missing_units() {
    for (injected_sql, expected_cause) in [
        (
            "UPDATE effects SET description_template = REPLACE(description_template, '{1}%', '{1}')
          WHERE name = 'Doublestrike and Doubleshot'",
            None,
        ),
        (
            "UPDATE effects SET description_template = REPLACE(description_template, '{1}%', '{1}'),
          verbose_name_template = REPLACE(verbose_name_template, '{1}%', '{1}')
          WHERE name = 'Doublestrike and Doubleshot'",
            Some("unit missing"),
        ),
        (
            "UPDATE effects SET set_bonus_line_template = REPLACE(set_bonus_line_template, '{1}%', '{1}')
          WHERE name = 'Doublestrike and Doubleshot'",
            Some("unit missing"),
        ),
        (
            "UPDATE effects SET description_template = 'Grants a bonus to Doublestrike and Doubleshot.'
          WHERE name = 'Doublestrike and Doubleshot'",
            Some("description omits an amount or states a different one"),
        ),
    ] {
        let scratch = tempfile::tempdir().unwrap();
        let db = Connection::open(fixture_db_copy_with(scratch.path(), injected_sql)).unwrap();
        let report = integrity_report(&db, &fixture_options()).unwrap();
        let outcome = report.outcome("effect_descriptions_without_their_amounts").unwrap();
        let findings = outcome
            .offenders
            .iter()
            .filter(|offender| offender.name == "Doublestrike and Doubleshot")
            .collect::<Vec<_>>();
        if let Some(cause) = expected_cause {
            assert!(findings.iter().any(|offender| offender.detail.contains(cause)), "{injected_sql}: {outcome}");
        } else {
            assert!(findings.is_empty(), "the renderer restores the unit: {findings:?}");
        }
    }
}

#[test]
fn shared_stat_set_report_lists_a_duplicate_but_not_a_constant_twin() {
    let scratch = tempfile::tempdir().unwrap();
    let exported = scratch.path().join("data");
    copy_fixture_tree(&exported);
    let frozen_thunderstorm =
        std::fs::read_to_string(fixtures_dir().join("../../../../data/effects/power-of-the-frozen-thunderstorm.toml"))
            .unwrap();
    std::fs::write(exported.join("effects/power-of-the-frozen-thunderstorm.toml"), &frozen_thunderstorm).unwrap();
    let duplicate = frozen_thunderstorm.replace("Power of the Frozen Thunderstorm", "Frozen Thunderstorm Duplicate");
    std::fs::write(exported.join("effects/stat-groups/frozen-thunderstorm-duplicate.toml"), duplicate).unwrap();
    let constant = frozen_thunderstorm
        .replace("Power of the Frozen Thunderstorm", "Frozen Thunderstorm Constant")
        .replace("{1}", "30")
        .replace("amount_from = 1", "amount_from = 0\nconstant = 30");
    std::fs::write(exported.join("effects/frozen-thunderstorm-constant.toml"), constant).unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&exported, &mut db).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("effects_sharing_a_stat_set").expect("shared stat set report");
    let printed = format!("{outcome}");
    assert!(printed.contains("Power of the Frozen Thunderstorm"), "{printed}");
    assert!(printed.contains("Frozen Thunderstorm Duplicate"), "{printed}");
    for stat in ["Cold Spell Power", "Electric Spell Power", "Sonic Spell Power"] {
        assert!(printed.contains(stat), "{printed}");
    }
    assert!(!printed.contains("Frozen Thunderstorm Constant"), "{printed}");
    assert_eq!(outcome.status, CheckStatus::Warned);
}

fn check_db_output(db_path: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
    command.arg("check-db").arg(db_path);
    for table in TABLES_EMPTY_IN_FIXTURES {
        command.args(["--allow-empty-table", table]);
    }
    command.output().unwrap()
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

#[test]
fn database_checks_need_no_xml_or_data_files() {
    let db = Connection::open(fixture_db_built_once()).unwrap();
    let options = fixture_options();
    let report = integrity_report(&db, &options).unwrap();
    assert!(report.failed_hard_check_names().is_empty(), "{report}");
    assert_ne!(report.outcome("effect_types_not_classified").unwrap().status, CheckStatus::Skipped);
    for check in [
        "buff_types_not_classified",
        "effect_definitions_with_partially_mapped_targets",
        "no_stale_corrections",
        "drop_text_disagrees_with_quest_loot",
    ] {
        assert!(report.outcome(check).is_none(), "retired input check {check} must not run against a database");
    }
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
fn duplicate_pack_names_after_normalization_fail() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(work_dir.path(),
        "INSERT INTO adventure_packs(name,slug) SELECT 'The ' || name, 'the-' || slug FROM adventure_packs WHERE name NOT LIKE 'The %' LIMIT 1;");
    let report = integrity_report(&Connection::open(db_path).unwrap(), &fixture_options()).unwrap();
    assert_eq!(report.outcome("adventure_pack_names_distinct").unwrap().status, CheckStatus::Failed);
}

#[test]
fn a_source_named_only_inside_a_prerequisite_clause_fails() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(work_dir.path(),
        "UPDATE quest_drops SET drop_text_segment = 'Stealing from Sorcere (requires completion of Sleeping with the Fishes)',
         source_match = 'Sleeping with the Fishes' WHERE id = (SELECT MIN(id) FROM quest_drops);");
    let report = integrity_report(&Connection::open(db_path).unwrap(), &fixture_options()).unwrap();
    assert_eq!(report.outcome("sources_not_from_prerequisite_clauses").unwrap().status, CheckStatus::Failed);
}

#[test]
fn a_crafting_station_in_quests_fails() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(work_dir.path(),
        "INSERT INTO crafting_systems(name,page,slug) SELECT name, 'https://ddowiki.com/page/Ritual_Table', slug FROM quests LIMIT 1;");
    let report = integrity_report(&Connection::open(db_path).unwrap(), &fixture_options()).unwrap();
    assert_eq!(report.outcome("quests_not_crafting_systems").unwrap().status, CheckStatus::Failed);
}

#[test]
fn pending_quest_version_difference_warns_without_failing_the_hard_rule() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(
        work_dir.path(),
        "INSERT OR IGNORE INTO adventure_packs (name, slug) VALUES ('Integrity Heroic Pack', 'integrity-heroic-pack'), ('Integrity Epic Pack', 'integrity-epic-pack');
         INSERT INTO quests (name, pack_id, level, slug) VALUES
           ('Dr. Rushmore''s Mansion - Picture Portals',
            (SELECT id FROM adventure_packs WHERE name = 'Integrity Heroic Pack'), 15, 'dr-rushmores-mansion-picture-portals'),
           ('Dr. Rushmore''s Mansion - Picture Portals - EPIC',
            (SELECT id FROM adventure_packs WHERE name = 'Integrity Epic Pack'), 25, 'dr-rushmores-mansion-picture-portals-epic');
         INSERT INTO pack_exceptions (pack_name, rule, row_index, entity_kind, entity_name, reason, url, read) VALUES
           ('Integrity Epic Pack', 'quest', 0, 'quest', 'Dr. Rushmore''s Mansion - Picture Portals - EPIC',
            'Heroic Integrity Heroic Pack and epic Integrity Epic Pack require a wiki pack comparison.',
            'https://ddowiki.com/page/Vaults_of_the_Artificers', '2026-10-07');",
    );
    let report = integrity_report(&Connection::open(db_path).unwrap(), &fixture_options()).unwrap();
    assert_eq!(report.outcome("quest_versions_match_heroic_pack_and_patron").unwrap().status, CheckStatus::Passed);
    assert_eq!(report.outcome("quest_versions_pending_wiki_read").unwrap().status, CheckStatus::Warned);
}

#[test]
fn legendary_quest_version_matches_a_heroic_name_with_an_article() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(
        work_dir.path(),
        "INSERT OR IGNORE INTO adventure_packs (name, slug) VALUES ('Integrity Heroic Pack', 'integrity-heroic-pack'), ('Integrity Legendary Pack', 'integrity-legendary-pack');
         INSERT INTO quests (name, pack_id, level, slug) VALUES
           ('The Integrity Article Quest', (SELECT id FROM adventure_packs WHERE name = 'Integrity Heroic Pack'), 10, 'the-integrity-article-quest'),
           ('Legendary Integrity Article Quest',
            (SELECT id FROM adventure_packs WHERE name = 'Integrity Legendary Pack'), 32, 'legendary-integrity-article-quest');",
    );
    let report = integrity_report(&Connection::open(db_path).unwrap(), &fixture_options()).unwrap();
    assert_eq!(report.outcome("quest_versions_match_heroic_pack_and_patron").unwrap().status, CheckStatus::Failed);
}

#[test]
fn cross_pack_drop_and_multi_patron_pack_are_warned() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(
        work_dir.path(),
        "INSERT INTO adventure_packs(name,slug) VALUES ('Unreviewed Second Pack','unreviewed-second-pack');
         INSERT INTO pack_drops(owner_kind,pack_id,owner_id,loot_type,is_rare)
           SELECT 'item', last_insert_rowid(), MIN(id), 'chest', 0 FROM items;
         INSERT INTO patrons(name,slug) VALUES ('Unreviewed Patron','unreviewed-patron');
         INSERT INTO quests(name,pack_id,patron_id,slug)
           SELECT 'Unreviewed Quest', pack_id, last_insert_rowid(), 'unreviewed-quest' FROM quests WHERE pack_id IS NOT NULL AND patron_id IS NOT NULL LIMIT 1;",
    );
    let report = integrity_report(&Connection::open(db_path).unwrap(), &fixture_options()).unwrap();
    assert_eq!(report.outcome("items_spanning_packs").unwrap().status, CheckStatus::Warned);
    assert_eq!(report.outcome("packs_with_several_patrons").unwrap().status, CheckStatus::Warned);
}

#[test]
fn effect_line_rendering_faults_fail_the_hard_check() {
    for (column, template, should_fail) in [
        ("description_template", "%b1 Enhancement bonus to Strength.", true),
        ("description_template", "Gain {3} bonus to Strength.", true),
        ("verbose_name_template", "%b1 Enhancement bonus to Strength.", false),
    ] {
        let work_dir = tempfile::tempdir().unwrap();
        let db_path = fixture_db_copy_in(work_dir.path());
        let db = Connection::open(&db_path).unwrap();
        db.execute("UPDATE effects SET home_bonus_type_id = NULL WHERE name = 'Strength'", []).unwrap();
        db.execute(&format!("UPDATE effects SET {column} = ?1 WHERE name = 'Strength'"), [template]).unwrap();
        let report = integrity_report(&db, &fixture_options()).unwrap();
        let outcome = report.outcome("effect_line_rendering_tokens").unwrap();
        assert_eq!(outcome.status == CheckStatus::Failed, should_fail, "{column}: {template}: {report}");
    }
}

#[test]
fn text_only_effect_named_after_a_stat_is_warned() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(
        work_dir.path(),
        "INSERT INTO effects (name, verbose_name_template, slug) VALUES ('hitpoints', 'hitpoints', 'hitpoints');
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
fn identifier_like_effect_names_are_warned() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(
        work_dir.path(),
        "INSERT INTO effects (name, verbose_name_template, slug) VALUES ('CamelCase', 'Camel Case', 'camelcase');
         INSERT INTO item_effects (item_id, effect_id, sort_order)
         VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 999);
         INSERT INTO effects (name, verbose_name_template, slug) VALUES ('Telekinetic117', 'Telekinetic 117', 'telekinetic117');
         INSERT INTO item_effects (item_id, effect_id, sort_order)
         VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 998);
         INSERT INTO effects (name, verbose_name_template, slug) VALUES ('Parrying Number', 'Parrying', 'parrying-number');
         INSERT INTO effects (name, verbose_name_template, slug) VALUES ('Riposte Riposte', 'Riposte', 'riposte-riposte');
         INSERT INTO effects (name, verbose_name_template, slug) VALUES ('Feat Elusive Target', 'Elusive Target', 'feat-elusive-target');
         INSERT INTO effects (name, verbose_name_template, slug) VALUES ('Penalty Good', 'Penalty', 'penalty-good');
         INSERT INTO effects (name, verbose_name_template, slug) VALUES ('Energy Absorption Negative', 'Absorption', 'energy-absorption-negative');",
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
        "INSERT INTO effects (name, verbose_name_template, slug) VALUES ('hitpoints', 'hitpoints', 'hitpoints');
         INSERT INTO items (name, slot_id, item_category, wiki_url, drop_location, minimum_level, slug)
         VALUES ('Integrity Probe Ring', (SELECT id FROM equipment_slots WHERE name = 'Ring'), 'Jewelry',
                 'https://ddowiki.com/page/Item:Integrity_Probe_Ring', 'Nowhere Keep, chest', 1, 'integrity-probe-ring');
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
         INSERT INTO vendor_offers (vendor_id, item_id)
         SELECT v.id, i.id FROM vendors v JOIN items i ON i.name = 'Thunder-Forged Orb';
         INSERT INTO vendor_offers (vendor_id, item_id)
         SELECT v.id, i.id FROM vendors v JOIN items i ON i.name = 'Ratkiller (legacy) (level 4)';",
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

fn integrity_pair_effects_sql() -> &'static str {
    "INSERT INTO effects (name, verbose_name_template, slug) VALUES ('Integrity Pair', '%b1 Integrity Pair +{1}', 'integrity-pair');
     INSERT INTO effect_bonuses (effect_id, target_effect_id, amount_from, sort_order)
     VALUES (last_insert_rowid(), (SELECT id FROM effects WHERE name = 'Strength'), 1, 0);
     INSERT INTO effect_bonuses (effect_id, target_effect_id, amount_from, sort_order)
     VALUES ((SELECT id FROM effects WHERE name = 'Integrity Pair'),
             (SELECT id FROM effects WHERE name = 'Dexterity'), 1, 1);
     INSERT INTO effects (name, verbose_name_template, slug) VALUES ('Integrity Overlap', '%b1 Integrity Overlap +{1}', 'integrity-overlap');
     INSERT INTO effect_bonuses (effect_id, target_effect_id, amount_from, sort_order)
     VALUES (last_insert_rowid(), (SELECT id FROM effects WHERE name = 'Strength'), 1, 0);
     INSERT INTO effect_bonuses (effect_id, target_effect_id, amount_from, sort_order)
     VALUES ((SELECT id FROM effects WHERE name = 'Integrity Overlap'),
             (SELECT id FROM effects WHERE name = 'Constitution'), 1, 1);"
}

fn link_item_sql(item_id_sql: &str, effect_name: &str, link_order: i64) -> String {
    format!(
        "INSERT INTO item_effects (item_id, effect_id, value, bonus_type_id, sort_order)
         VALUES (({item_id_sql}), (SELECT id FROM effects WHERE name = '{effect_name}'), 5,
                 (SELECT id FROM bonus_types WHERE name = 'Enhancement'), {link_order});"
    )
}

#[test]
fn a_link_whose_bonuses_another_link_already_grants_is_warned_but_a_partial_overlap_is_not() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(
        work_dir.path(),
        &format!(
            "{} {} {} {} {} {} {}",
            integrity_pair_effects_sql(),
            probe_ring_insert("Integrity Contained Ring"),
            probe_ring_insert("Integrity Overlapping Ring"),
            link_item_sql("SELECT id FROM items WHERE name = 'Integrity Contained Ring'", "Integrity Pair", 0),
            link_item_sql("SELECT id FROM items WHERE name = 'Integrity Contained Ring'", "Strength", 1),
            link_item_sql("SELECT id FROM items WHERE name = 'Integrity Overlapping Ring'", "Integrity Pair", 0),
            link_item_sql("SELECT id FROM items WHERE name = 'Integrity Overlapping Ring'", "Integrity Overlap", 1),
        ),
    );
    let db = Connection::open(&db_path).unwrap();
    let item_named =
        |name: &str| -> i64 { db.query_row("SELECT id FROM items WHERE name = ?1", [name], |row| row.get(0)).unwrap() };
    let first_item = item_named("Integrity Contained Ring");
    let last_item = item_named("Integrity Overlapping Ring");
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("owner_bonuses_repeated_across_links").unwrap();
    assert_eq!(outcome.status, CheckStatus::Warned, "{report}");
    assert!(
        outcome.offenders.iter().any(|offender| offender.id == Some(first_item)
            && offender.detail.contains("Strength")
            && offender.detail.contains("Integrity Pair")),
        "{report}"
    );
    assert!(!outcome.offenders.iter().any(|offender| offender.id == Some(last_item)), "{report}");
    assert!(!outcome.top_details.is_empty(), "{report}");
    let hard = report.outcome("owner_stat_links_repeat_effect_links").unwrap();
    assert_eq!(hard.status, CheckStatus::Failed, "{report}");
    assert!(hard.offenders.iter().any(|offender| offender.id == Some(first_item)), "{report}");
    assert!(!hard.offenders.iter().any(|offender| offender.id == Some(last_item)), "{report}");
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
        "INSERT INTO items (slug, name, slot_id, item_category, wiki_url, minimum_level, description)
         VALUES ('{identity}', '{name}', (SELECT id FROM equipment_slots WHERE name = 'Ring'), 'Jewelry',
                 'https://ddowiki.com/page/Item:Integrity_Probe', 1, 'A probe.');",
        identity = ddo_model::identity::slug(name)
    )
}

fn probe_quest_insert(is_raid: bool) -> String {
    format!(
        "INSERT INTO quests (name, pack_id, is_raid, slug)
     VALUES ('Integrity Probe Quest', (SELECT MIN(id) FROM adventure_packs), {}, 'integrity-probe-quest');",
        i64::from(is_raid)
    )
}

fn probe_quest_source_insert(loot_type: &str, chest: &str) -> String {
    format!(
        "{}
         INSERT INTO quest_drops (owner_kind, quest_id, owner_id, loot_type, chest)
         VALUES ('item', last_insert_rowid(), (SELECT MIN(id) FROM items), '{loot_type}', {chest});",
        probe_quest_insert(false)
    )
}

fn injected_violations() -> Vec<InjectedViolation> {
    let mut violations = vec![
        violation(
            "open_verification_requests",
            "INSERT INTO verification_requests (owner_kind,owner_id,question,assumed_value,raised)
             SELECT 'items',id,'Is this version current?','\"As recorded\"','2026-10-23'
             FROM items WHERE name='Alabaster of the Twelve';
             INSERT INTO verification_request_sources (request_id,sort_order,says,in_game,read)
             VALUES (last_insert_rowid(),0,'The item appears unchanged.',1,'2026-10-23');",
            "Alabaster of the Twelve",
        ),
        violation(
            "effect_descriptions_without_their_amounts",
            "UPDATE effects SET description_template = REPLACE(description_template, '{1}%', '{1}'),
             verbose_name_template = REPLACE(verbose_name_template, '{1}%', '{1}')
             WHERE name = 'Doublestrike and Doubleshot';",
            "Doublestrike and Doubleshot",
        ),
        violation(
            "effect_descriptions_without_their_amounts",
            "UPDATE effects SET set_bonus_line_template = REPLACE(set_bonus_line_template, '{1}%', '{1}')
             WHERE name = 'Doublestrike and Doubleshot';",
            "Doublestrike and Doubleshot",
        ),
        violation(
            "effect_descriptions_without_their_amounts",
            "UPDATE effects SET description_template = 'Grants a bonus to Doublestrike and Doubleshot.'
             WHERE name = 'Doublestrike and Doubleshot';",
            "Doublestrike and Doubleshot",
        ),
        violation(
            "redirects_agree_with_current_slugs",
            "INSERT INTO redirects (kind, from_slug, to_slug, since) VALUES ('items', 'old-item', 'missing-item', '2026-10-01');",
            "old-item",
        ),
        violation(
            "slugs_are_not_all_digits",
            "UPDATE entity_identity SET slug = '123456' WHERE kind = 'items' AND entity_id = (SELECT MIN(id) FROM items);",
            "123456",
        ),
        violation(
            "redirects_agree_with_current_slugs",
            "INSERT INTO redirects (kind, from_slug, to_slug, since) SELECT 'items', '123456', slug, '2026-10-01' FROM items ORDER BY id LIMIT 1;",
            "123456",
        ),
        violation(
            "redirects_agree_with_current_slugs",
            "INSERT INTO retired_slugs (kind, slug, since, reason) VALUES ('items', '123456', '2026-10-01', 'Retired numeric slug');",
            "123456",
        ),
        violation(
            "source_references_exist",
            "UPDATE quest_drops SET owner_id = -1 WHERE id = (SELECT MIN(id) FROM quest_drops);",
            "quest source",
        ),
        violation(
            "source_references_exist",
            "UPDATE vendor_offer_costs SET ingredient_id = -1 WHERE offer_id = (SELECT MIN(offer_id) FROM vendor_offer_costs);",
            "Morten Edgewright: Alabaster of the Twelve",
        ),
        violation(
            "augment_recipes_not_duplicated",
            "INSERT INTO augments (name, family, min_level, description, variant, slug) VALUES
               ('Integrity Duplicate Recipe', 'Integrity Family', 11, 'Earth:Opposition:Ethereal', 'first', 'integrity-duplicate-recipe-first'),
               ('Integrity Duplicate Recipe', 'Integrity Family', 11, 'Earth:Opposition:Ethereal', 'second', 'integrity-duplicate-recipe-second');",
            "Integrity Duplicate Recipe",
        ),
        violation(
            "quest_versions_match_heroic_pack_and_patron",
            "INSERT OR IGNORE INTO adventure_packs (name, slug) VALUES ('Integrity Heroic Pack', 'integrity-heroic-pack'), ('Integrity Epic Pack', 'integrity-epic-pack');
             INSERT INTO quests (name, pack_id, level, slug) VALUES
               ('Integrity Version Quest', (SELECT id FROM adventure_packs WHERE name = 'Integrity Heroic Pack'), 10, 'integrity-version-quest'),
               ('Epic Integrity Version Quest', (SELECT id FROM adventure_packs WHERE name = 'Integrity Epic Pack'), 20, 'epic-integrity-version-quest');",
            "Epic Integrity Version Quest",
        ),
        violation(
            "effect_templates_disagree_with_names",
            "INSERT INTO effects (name, verbose_name_template, slug) VALUES ('Integrity Title Probe', 'Wrong Title', 'integrity-title-probe');
             INSERT INTO item_effects (item_id, effect_id, sort_order)
             VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 996);",
            "Integrity Title Probe",
        ),
        violation(
            "effect_template_numbers_match_names",
            "INSERT INTO effects (name, verbose_name_template, slug) VALUES ('Integrity Solar IX', 'Integrity Solar VI', 'integrity-solar-ix');
             INSERT INTO item_effects (item_id, effect_id, sort_order)
             VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 997);",
            "Integrity Solar IX",
        ),
        violation(
            "effect_descriptions_with_fewer_critical_dice_than_hit_dice",
            "INSERT INTO effects (name, verbose_name_template, description_template, slug)
             VALUES ('Integrity Burst 7', 'Integrity Burst 7',
                     'This weapon deals 7d6 Good damage on a successful hit, and 1d6 Good damage on a critical hit.', 'integrity-burst-7');
             INSERT INTO item_effects (item_id, effect_id, sort_order)
             VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 995);",
            "Integrity Burst 7",
        ),
        violation(
            "owner_names_disagree_with_linked_effects",
            "INSERT INTO augments (name, family, slug) VALUES ('Integrity Combat Mastery', 'probe', 'integrity-combat-mastery');
             INSERT INTO augment_effects (augment_id, effect_id, bonus_type_id, value, sort_order)
             VALUES (last_insert_rowid(), (SELECT id FROM effects WHERE name = 'Strength'),
                     (SELECT id FROM bonus_types WHERE name = 'Insight'), 1, 0);",
            "Integrity Combat Mastery",
        ),
        violation(
            "universal_spell_power_has_no_elemental_types",
            "INSERT INTO item_effects (item_id, effect_id, bonus_type_id, value, sort_order)
             VALUES ((SELECT MIN(id) FROM items),
                     (SELECT id FROM effects WHERE name = 'Universal Spell Power'),
                     (SELECT id FROM bonus_types WHERE name = 'Equipment'), 10, 998);",
            "item:",
        ),
        violation(
            "skill_groups_share_home_bonus_type",
            "INSERT INTO effects (name, verbose_name_template, is_group, slug)
             VALUES ('Strength Skills', '%b1 Strength Skills +{1}', 1, 'strength-skills');
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
            "INSERT INTO effects (name, verbose_name_template, slug) VALUES ('Integrity Tied Type', '%b1 Integrity Tied Type +{1}', 'integrity-tied-type');
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
            "INSERT INTO effects (name, verbose_name_template, slug) VALUES ('Integrity Double Bonus', '%b1 Integrity Double Bonus +{1}', 'integrity-double-bonus');
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
            "owner_stat_links_repeat_effect_links",
            &format!(
                "{} {} {} {}",
                integrity_pair_effects_sql(),
                probe_ring_insert("Integrity Repeat Ring"),
                link_item_sql("SELECT id FROM items WHERE name = 'Integrity Repeat Ring'", "Integrity Pair", 0),
                link_item_sql("SELECT id FROM items WHERE name = 'Integrity Repeat Ring'", "Strength", 1),
            ),
            "item:",
        ),
        violation(
            "owner_bonuses_repeated_across_links",
            "INSERT INTO effects (name, verbose_name_template, slug) VALUES ('Integrity Link A', '%b1 Integrity Link A +{1}', 'integrity-link-a');
             INSERT INTO effect_bonuses (effect_id, target_effect_id, amount_from, sort_order)
             VALUES (last_insert_rowid(), (SELECT id FROM effects WHERE name = 'Strength'), 1, 0);
             INSERT INTO effects (name, verbose_name_template, slug) VALUES ('Integrity Link B', '%b1 Integrity Link B +{1}', 'integrity-link-b');
             INSERT INTO effect_bonuses (effect_id, target_effect_id, amount_from, sort_order)
             VALUES (last_insert_rowid(), (SELECT id FROM effects WHERE name = 'Strength'), 1, 0);
             INSERT INTO item_effects (item_id, effect_id, value, bonus_type_id, sort_order)
             SELECT MIN(id), (SELECT id FROM effects WHERE name = 'Integrity Link A'), 1,
                    (SELECT id FROM bonus_types WHERE name = 'Enhancement'), 995 FROM items;
             INSERT INTO item_effects (item_id, effect_id, value, bonus_type_id, sort_order)
             SELECT MIN(id), (SELECT id FROM effects WHERE name = 'Integrity Link B'), 1,
                    (SELECT id FROM bonus_types WHERE name = 'Enhancement'), 996 FROM items;",
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
            "INSERT INTO effects (name, verbose_name_template, slug) VALUES ('hitpoints', 'hitpoints', 'hitpoints'); \
             INSERT INTO item_effects (item_id, effect_id, sort_order) \
             VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 999);",
            "hitpoints",
        ),
        violation(
            "effect_templates_disagree_with_names",
            "INSERT INTO effects (name, verbose_name_template, slug) VALUES ('Integrity Probe Glow', 'Glow 7', 'integrity-probe-glow'); \
             INSERT INTO item_effects (item_id, effect_id, sort_order) \
             VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 998);",
            "Integrity Probe Glow",
        ),
        violation(
            "effects_with_amounts_named_with_digits",
            "INSERT INTO effects (name, verbose_name_template, slug)
             VALUES ('Integrity 12 Power', 'Power {1}', 'integrity-12-power');
             INSERT INTO item_effects (item_id, effect_id, value, sort_order)
             VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 12, 998);",
            "Integrity 12 Power",
        ),
        violation(
            "effects_with_values_in_names",
            "INSERT INTO effects (name, verbose_name_template, slug) VALUES ('+7 Integrity Probe', '+7 Integrity Probe', '7-integrity-probe'); \
             INSERT INTO item_effects (item_id, effect_id, sort_order) \
             VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 998);",
            "+7 Integrity Probe",
        ),
        violation(
            "effects_with_unused_default",
            "INSERT INTO effects (name, verbose_name_template, default_value, slug)
             VALUES ('Integrity Default', 'Default {1}', 7, 'integrity-default');
             INSERT INTO item_effects (item_id, effect_id, sort_order)
             VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 998);",
            "Integrity Default",
        ),
        violation(
            "effect_link_amount_counts",
            "INSERT INTO effects (name, verbose_name_template, slug)
             VALUES ('Integrity Excess Amount', 'No amount', 'integrity-excess-amount');
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
            "INSERT INTO effects (name, verbose_name_template, slug)
             VALUES ('Integrity Orphan Effect', 'Integrity Orphan Effect', 'integrity-orphan-effect');",
            "Integrity Orphan Effect",
        ),
        violation(
            "effect_groups_have_flat_members",
            "INSERT INTO effects (name, verbose_name_template, is_group, slug) VALUES ('Integrity Empty Group', 'Integrity Empty Group +{1}', 1, 'integrity-empty-group');",
            "Integrity Empty Group",
        ),
        violation(
            "effect_tier_groups_have_steps",
            "INSERT INTO effect_tier_groups (name) VALUES ('Integrity Tier Group');",
            "Integrity Tier Group",
        ),
        violation(
            "effect_names_have_no_em_dash",
            "INSERT INTO effects (name, verbose_name_template, slug)
             VALUES ('Integrity — Prose', 'Integrity prose', 'integrity-prose');
             INSERT INTO item_effects (item_id, effect_id, sort_order)
             VALUES ((SELECT MIN(id) FROM items), last_insert_rowid(), 997);",
            "Integrity — Prose",
        ),
        violation(
            "set_tier_lines_do_not_repeat_structured_facts",
            "INSERT INTO effects (slug, name, verbose_name_template)
             SELECT 'integrity-duplicate-tier-fact', 'Integrity Duplicate Tier Fact',
                    REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(e.verbose_name_template,
                    '+{1}', '{1}'), '+{2}', '{2}'),
                    '{1}', printf('%+d', COALESCE(te.value, e.default_value))),
                    '{2}', COALESCE(printf('%+d', COALESCE(te.value2, e.default_value2)), '')),
                    '%b1', COALESCE(bt.name, ''))
             FROM set_bonus_tier_effects te JOIN effects e ON e.id = te.effect_id
             LEFT JOIN bonus_types bt ON bt.id = te.bonus_type_id
             WHERE te.tier_id IN (SELECT t.id FROM set_bonus_tiers t JOIN set_bonuses s ON s.id = t.set_id WHERE s.name = 'Kundarak Delving Equipment')
               AND EXISTS (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)
             ORDER BY te.tier_id, te.sort_order LIMIT 1;
             INSERT INTO set_bonus_tier_effects (tier_id, effect_id, sort_order)
             SELECT te.tier_id, (SELECT id FROM effects WHERE name = 'Integrity Duplicate Tier Fact'), 999
             FROM set_bonus_tier_effects te JOIN effects e ON e.id = te.effect_id
             WHERE te.tier_id IN (SELECT t.id FROM set_bonus_tiers t JOIN set_bonuses s ON s.id = t.set_id WHERE s.name = 'Kundarak Delving Equipment')
               AND EXISTS (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)
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
            "INSERT INTO augment_slot_types (label, family, variant, slug) VALUES ('mystery: probe', 'mystery', 'probe', 'mystery-probe');
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
            "INSERT INTO augments (name, family, slug) VALUES ('Integrity Probe Augment', 'Ruby', 'integrity-probe-augment');",
            "Integrity Probe Augment",
        ),
        violation(
            "chains_and_sagas_have_quests",
            "INSERT INTO quest_chains (name, provenance, wiki_url, slug)
             VALUES ('Integrity Probe Chain', 'wiki', 'https://ddowiki.com/page/Integrity_Probe_Chain', 'integrity-probe-chain');",
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
            &format!("{} {}", probe_ring_insert("Integrity Probe Ring"), probe_ring_insert("IntegrityProbeRing")),
            "IntegrityProbeRing",
        ),
        violation(
            "raids_without_raid_loot",
            &probe_quest_insert(true),
            "Integrity Probe Quest",
        ),
        violation("quests_without_loot", &probe_quest_insert(false), "Integrity Probe Quest"),
        violation(
            "unreferenced_stats",
            "INSERT INTO effects (name, is_stat, category, slug) VALUES ('Integrity Probe Stat', 1, 'probe', 'integrity-probe-stat');",
            "Integrity Probe Stat",
        ),
        violation(
            "slot_types_no_augment_fits",
            "INSERT INTO augment_slot_types (label, family, variant, slug)
             VALUES ('crafting: integrity probe', 'crafting', 'integrity probe', 'crafting-integrity-probe');",
            "crafting: integrity probe",
        ),
        violation(
            "sets_without_members",
            "INSERT INTO set_bonuses (name, slug) VALUES ('Integrity Probe Set', 'integrity-probe-set');",
            "Integrity Probe Set",
        ),
    ];
    violations.extend(
        template_violations()
            .into_iter()
            .map(|(_, effect, sql)| violation("effect_templates_without_their_placeholders", sql, effect)),
    );
    violations.push(violation(
        "effect_templates_with_digits_without_amounts",
        "INSERT INTO effects (name, slug, verbose_name_template) VALUES ('Unmodelled Charges', 'unmodelled-charges', 'Absorb Spells - 5 Charges');
         INSERT INTO item_effects (item_id, effect_id, sort_order)
         SELECT (SELECT MIN(id) FROM items), id, 9999 FROM effects WHERE name = 'Unmodelled Charges'",
        "Unmodelled Charges",
    ));
    violations.push(violation("effect_descriptions_without_their_amounts",
        "UPDATE effects SET description_template = REPLACE(description_template, 'Insightful', 'Profane') WHERE name = 'Command'", "Command"));
    violations
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
        "INSERT INTO items (name, slot_id, item_category, wiki_url, drop_location, slug) VALUES
         ('Integrity Probe Ring', (SELECT id FROM equipment_slots WHERE name = 'Ring'), 'Jewelry',
          'https://ddowiki.com/page/Item:Integrity_Probe_Ring', 'Nowhere Keep, chest', 'integrity-probe-ring'),
         ('Integrity Probe Belt', (SELECT id FROM equipment_slots WHERE name = 'Waist'), 'Clothing',
          'https://ddowiki.com/page/Item:Integrity_Probe_Belt', 'Nowhere Keep', 'integrity-probe-belt');",
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
        outcome.notes.iter().any(|note| note == &format!("allowed empty: {}", TABLES_EMPTY_IN_FIXTURES.join(", "))),
        "{report}"
    );

    let options_allowing_nothing = IntegrityOptions { allowed_empty_tables: Vec::new() };
    let strict_report = integrity_report(&db, &options_allowing_nothing).unwrap();
    let strict_outcome = strict_report.outcome("tables_not_empty").unwrap();
    assert_eq!(strict_outcome.status, CheckStatus::Failed, "{strict_report}");
    assert!(strict_outcome.notes.iter().any(|note| note == "allowed empty: none"), "{strict_report}");
}

#[test]
fn optional_annotation_tables_may_be_empty_without_hiding_required_tables() {
    let scratch = tempfile::tempdir().unwrap();
    let db = Connection::open(fixture_db_copy_with(scratch.path(), "")).unwrap();
    for table in ["game_bug_corrections", "verification_requests", "verification_request_sources", "note_lines"] {
        db.execute_batch(&format!("CREATE TABLE IF NOT EXISTS {table} (id INTEGER PRIMARY KEY); DELETE FROM {table};"))
            .unwrap();
    }
    db.execute_batch("CREATE TABLE required_probe (id INTEGER PRIMARY KEY)").unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();
    let outcome = report.outcome("tables_not_empty").unwrap();
    assert_eq!(outcome.offenders.iter().map(|offender| offender.name.as_str()).collect::<Vec<_>>(), ["required_probe"]);
    for table in ["game_bug_corrections", "verification_requests", "verification_request_sources", "note_lines"] {
        assert!(
            outcome
                .notes
                .iter()
                .any(|note| note.contains(table) && note.contains("zero recorded bugs or open questions is valid")),
            "{outcome}"
        );
    }
}

#[test]
fn the_unknown_placeholder_class_needs_no_progression() {
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = fixture_db_copy_with(
        work_dir.path(),
        "INSERT INTO classes (name, skill_points, hit_points, fortitude, reflex, will, bab, spell_points_per_level, slug)
         VALUES ('Unknown', 2, 0, 'none', 'none', 'none',
                 '[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]', '[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]', 'unknown');",
    );
    let db = Connection::open(&db_path).unwrap();
    let report = integrity_report(&db, &fixture_options()).unwrap();

    assert_eq!(report.outcome("classes_have_full_progression").unwrap().status, CheckStatus::Passed, "{report}");
}

#[test]
fn a_set_only_an_augment_slot_option_joins_has_members() {
    let injected_sql =
        "INSERT INTO set_bonuses (name, slug) VALUES ('Integrity Probe Option Set', 'integrity-probe-option-set');
         INSERT INTO item_augment_slot_option_sets (option_id, set_id)
         VALUES ((SELECT MIN(id) FROM item_augment_slot_options), last_insert_rowid());
         INSERT INTO set_bonuses (name, slug) VALUES ('Integrity Probe Bare Set', 'integrity-probe-bare-set');";
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
    let injected_sql = "INSERT INTO augment_slot_types (label, family, variant, slug)
         VALUES ('crafting: integrity probe granted', 'crafting', 'integrity probe granted', 'crafting-integrity-probe-granted');
         INSERT INTO item_augment_slot_option_grants (option_id, sort_order, slot_id)
         VALUES ((SELECT MIN(id) FROM item_augment_slot_options), 99, last_insert_rowid());
         INSERT INTO augment_slot_types (label, family, variant, slug)
         VALUES ('crafting: integrity probe upgrade', 'crafting', 'integrity probe upgrade', 'crafting-integrity-probe-upgrade');
         INSERT INTO item_augment_slots (item_id, sort_order, slot_id)
         VALUES ((SELECT MIN(id) FROM items), 99, last_insert_rowid());
         INSERT INTO item_augment_slot_options (item_id, slot_order, option_order, name)
         VALUES ((SELECT MIN(id) FROM items), 99, 0, 'Integrity Probe Upgrade');
         INSERT INTO augment_slot_types (label, family, variant, slug)
         VALUES ('crafting: integrity probe bare', 'crafting', 'integrity probe bare', 'crafting-integrity-probe-bare');";
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
