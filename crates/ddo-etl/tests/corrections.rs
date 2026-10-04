use ddo_etl::build::{build_database, BuildReport, StaleCorrection, StaleCorrectionCause};
use ddo_etl::corrections::{CorrectionValue, Corrections};
use ddo_etl::wiki::WikiOverrides;
use ddo_model::DatasetVersion;
use rusqlite::{Connection, OptionalExtension};
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture_dataset_version() -> DatasetVersion {
    DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-09-20T00:00:00Z".into() }
}

fn correction_toml(kind: &str, name: &str, field: &str, from: &str, to: &str) -> String {
    format!(
        "[[correction]]\nkind = {kind:?}\nname = {name:?}\nfield = {field:?}\nfrom = {from}\nto = {to}\n\
         reason = \"Test reason.\"\nsource = \"https://ddowiki.com/page/Test\"\nread = \"2026-09-29\"\n"
    )
}

fn parsed_corrections(files: &[(&str, &str)]) -> Result<Corrections, String> {
    Corrections::from_toml_files(files).map_err(|e| format!("{e:#}"))
}

fn built_db_with_wiki(wiki: &WikiOverrides, corrections: &Corrections) -> Result<(Connection, BuildReport), String> {
    let mut db = Connection::open_in_memory().unwrap();
    let report =
        build_database(&fixtures_dir().join("DataFiles"), wiki, corrections, &mut db, &fixture_dataset_version())
            .map_err(|e| format!("{e:#}"))?;
    Ok((db, report))
}

fn built_db_with(correction_files: &[(&str, &str)]) -> Result<(Connection, BuildReport), String> {
    built_db_with_wiki(&WikiOverrides::default(), &parsed_corrections(correction_files)?)
}

fn augment_min_levels(db: &Connection, name: &str) -> Vec<Option<i64>> {
    let mut statement = db.prepare("SELECT min_level FROM augments WHERE name = ?1 ORDER BY id").unwrap();
    statement.query_map([name], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

fn recorded_corrections(db: &Connection) -> Vec<(String, String, String, String, String)> {
    let mut statement = db
        .prepare("SELECT kind, name, field, from_value, to_value FROM corrections ORDER BY kind, name, field")
        .unwrap();
    statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn reads_a_correction_with_typed_values_and_its_citation() {
    let corrections = parsed_corrections(&[(
        "corrections.toml",
        &correction_toml("augment", "Perfect Silence", "min_level", "30", "29"),
    )])
    .unwrap();
    let [correction] = corrections.entries.as_slice() else { panic!("{corrections:?}") };
    assert_eq!(correction.kind.as_str(), "augment");
    assert_eq!(correction.name, "Perfect Silence");
    assert_eq!(correction.field, "min_level");
    assert_eq!(correction.from, CorrectionValue::Integer(30));
    assert_eq!(correction.to, CorrectionValue::Integer(29));
    assert_eq!(correction.reason, "Test reason.");
    assert_eq!(correction.source, "https://ddowiki.com/page/Test");
    assert_eq!(correction.read, "2026-09-29");
    assert_eq!(correction.file_name, "corrections.toml");
}

#[test]
fn reads_null_as_the_literal_string_and_other_values_by_type() {
    let corrections = parsed_corrections(&[(
        "corrections.toml",
        &(correction_toml("augment", "Ruby of Acid Damage", "min_level", "\"null\"", "5")
            + &correction_toml("item", "Docent of Defiance", "description", "\"null\"", "\"A docent.\"")),
    )])
    .unwrap();
    assert_eq!(corrections.entries[0].from, CorrectionValue::Null);
    assert_eq!(corrections.entries[1].to, CorrectionValue::Text("A docent.".into()));
}

#[test]
fn embedded_corrections_load_and_fix_the_furys_rage_level() {
    let corrections = Corrections::embedded().unwrap();
    let furys_rage = corrections.entries.iter().find(|c| c.name == "The Fury's Rage").unwrap();
    assert_eq!(
        (furys_rage.from.clone(), furys_rage.to.clone()),
        (CorrectionValue::Integer(318), CorrectionValue::Integer(18))
    );
}

#[test]
fn rejects_a_kind_outside_the_correctable_tables() {
    let error = parsed_corrections(&[(
        "corrections.toml",
        &correction_toml("gem", "Perfect Silence", "min_level", "30", "29"),
    )])
    .unwrap_err();
    assert!(error.contains("corrections.toml") && error.contains("gem") && error.contains("augment"), "{error}");
}

#[test]
fn rejects_a_field_outside_the_kinds_allow_list() {
    for (kind, field) in [("augment", "icon"), ("augment", "id"), ("item", "icon"), ("feat", "icon"), ("quest", "icon")]
    {
        let error =
            parsed_corrections(&[("corrections.toml", &correction_toml(kind, "Perfect Silence", field, "1", "2"))])
                .unwrap_err();
        assert!(error.contains("Perfect Silence") && error.contains(field), "{kind}.{field}: {error}");
    }
}

#[test]
fn allows_the_quest_facts_his_files_carry_and_not_the_wikis() {
    for (field, from, to) in [
        ("level", "1", "2"),
        ("epic_level", "\"null\"", "30"),
        ("favor", "1", "2"),
        ("is_raid", "0", "1"),
        ("pack", "\"Vault of Night\"", "\"Devil Assault\""),
        ("patron", "\"null\"", "\"The Twelve\""),
    ] {
        parsed_corrections(&[("corrections.toml", &correction_toml("quest", "Plane of Night", field, from, to))])
            .unwrap_or_else(|error| panic!("{field}: {error}"));
    }
    for wiki_field in ["is_free_to_play", "legendary_level"] {
        let error = parsed_corrections(&[(
            "corrections.toml",
            &correction_toml("quest", "Plane of Night", wiki_field, "0", "1"),
        )])
        .unwrap_err();
        assert!(error.contains(wiki_field) && error.contains("wiki"), "{error}");
    }
}

#[test]
fn rejects_the_same_kind_name_and_field_twice_across_files() {
    let correction = correction_toml("augment", "Perfect Silence", "min_level", "30", "29");
    let error = parsed_corrections(&[("a.toml", &correction), ("b.toml", &correction)]).unwrap_err();
    assert!(error.contains("Perfect Silence") && error.contains("a.toml") && error.contains("b.toml"), "{error}");
}

#[test]
fn rejects_a_correction_without_a_source_url_reason_or_real_read_date() {
    let correction = correction_toml("augment", "Perfect Silence", "min_level", "30", "29");
    for (broken_correction, expected_text) in [
        (correction.replace("https://ddowiki.com/page/Test", "the wiki"), "source"),
        (correction.replace("Test reason.", ""), "reason"),
        (correction.replace("2026-09-29", "2026-02-30"), "2026-02-30"),
        (correction.replace("to = 29", "to = 30"), "to"),
        (correction.clone() + "note = \"extra\"\n", "note"),
    ] {
        let error = parsed_corrections(&[("corrections.toml", &broken_correction)]).unwrap_err();
        assert!(error.contains("Perfect Silence") && error.contains(expected_text), "{expected_text}: {error}");
    }
}

#[test]
fn build_fails_naming_a_row_his_files_lack() {
    let error =
        built_db_with(&[("corrections.toml", &correction_toml("augment", "Missing Gem", "min_level", "30", "29"))])
            .unwrap_err();
    assert!(error.contains("Missing Gem") && error.contains("corrections.toml"), "{error}");
}

#[test]
fn applies_a_correction_whose_from_matches_and_records_it() {
    let (db, report) =
        built_db_with(&[("corrections.toml", &correction_toml("augment", "Perfect Silence", "min_level", "30", "29"))])
            .unwrap();
    assert_eq!(augment_min_levels(&db, "Perfect Silence"), [Some(29)]);
    assert_eq!(
        recorded_corrections(&db),
        [("augment".into(), "Perfect Silence".into(), "min_level".into(), "30".into(), "29".into())]
    );
    let (reason, source, read): (String, String, String) = db
        .query_row("SELECT reason, source, read FROM corrections", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    assert_eq!(
        (reason.as_str(), source.as_str(), read.as_str()),
        ("Test reason.", "https://ddowiki.com/page/Test", "2026-09-29")
    );
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
    assert!(report.stale_corrections.is_empty());
}

#[test]
fn leaves_his_value_and_reports_a_correction_whose_from_is_stale() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &correction_toml("augment", "Perfect Silence", "min_level", "318", "18"),
    )])
    .unwrap();
    assert_eq!(augment_min_levels(&db, "Perfect Silence"), [Some(30)]);
    assert!(recorded_corrections(&db).is_empty());
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 1));
    assert_eq!(
        report.stale_corrections,
        [StaleCorrection {
            kind: "augment".into(),
            name: "Perfect Silence".into(),
            field: "min_level".into(),
            cause: StaleCorrectionCause::ValueChanged { expected_value: "318".into(), maetrim_value: "30".into() },
            file_name: "corrections.toml".into(),
        }]
    );
}

#[test]
fn matches_and_writes_null_through_the_literal_string() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(correction_toml("augment", "Ruby of Acid Damage", "min_level", "\"null\"", "5")
            + &correction_toml("augment", "Perfect Silence", "min_level", "30", "\"null\"")),
    )])
    .unwrap();
    assert_eq!(augment_min_levels(&db, "Ruby of Acid Damage"), [Some(5)]);
    assert_eq!(augment_min_levels(&db, "Perfect Silence"), [None]);
    assert_eq!(report.correction_applied_count, 2);
    assert!(recorded_corrections(&db)
        .iter()
        .any(|(_, name, _, from, _)| name == "Ruby of Acid Damage" && from == "null"));
}

#[test]
fn applies_a_repeated_name_to_every_row_with_that_name() {
    assert_eq!(augment_min_levels(&built_db_with(&[]).unwrap().0, "+5 Fortitude Save"), [Some(11), Some(11)]);
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &correction_toml("augment", "+5 Fortitude Save", "min_level", "11", "12"),
    )])
    .unwrap();
    assert_eq!(augment_min_levels(&db, "+5 Fortitude Save"), [Some(12), Some(12)]);
    assert_eq!(report.correction_applied_count, 1);
    assert_eq!(recorded_corrections(&db).len(), 1);
}

#[test]
fn resolves_reference_fields_by_name() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(correction_toml("item", "Docent of Defiance", "slot", "\"Body\"", "\"Waist\"")
            + &correction_toml("item", "Docent of Defiance", "material", "\"Gem\"", "\"Steel\"")
            + &correction_toml("quest", "Plane of Night", "pack", "\"Vault of Night\"", "\"Devil Assault\"")
            + &correction_toml("quest", "Plane of Night", "patron", "\"House Kundarak\"", "\"The Twelve\"")),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 4, "{:?}", report.stale_corrections);
    let (slot, material): (String, String) = db
        .query_row(
            "SELECT s.name, m.name FROM items i JOIN equipment_slots s ON s.id = i.slot_id JOIN item_materials m ON m.id = i.material_id
              WHERE i.name = 'Docent of Defiance'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((slot.as_str(), material.as_str()), ("Waist", "Steel"));
    let (pack, patron): (String, String) = db
        .query_row(
            "SELECT p.name, pa.name FROM quests q JOIN adventure_packs p ON p.id = q.pack_id JOIN patrons pa ON pa.id = q.patron_id
              WHERE q.name = 'Plane of Night'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((pack.as_str(), patron.as_str()), ("Devil Assault", "The Twelve"));
}

#[test]
fn build_fails_naming_a_reference_his_files_lack() {
    let error = built_db_with(&[(
        "corrections.toml",
        &correction_toml("item", "Docent of Defiance", "material", "\"Gem\"", "\"Unobtainium\""),
    )])
    .unwrap_err();
    assert!(error.contains("Unobtainium") && error.contains("Docent of Defiance"), "{error}");
}

#[test]
fn renames_a_pack_and_refuses_a_name_already_taken() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &correction_toml("adventure_pack", "Vault of Night", "name", "\"Vault of Night\"", "\"The Vault of Night\""),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 1);
    let pack: String = db
        .query_row(
            "SELECT p.name FROM quests q JOIN adventure_packs p ON p.id = q.pack_id WHERE q.name = 'Plane of Night'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(pack, "The Vault of Night");
    let error = built_db_with(&[(
        "corrections.toml",
        &correction_toml("patron", "The Twelve", "name", "\"The Twelve\"", "\"House Cannith\""),
    )])
    .unwrap_err();
    assert!(error.contains("House Cannith") && error.contains("The Twelve"), "{error}");
}

#[test]
fn renaming_a_set_renames_what_his_items_and_augments_call_it() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(correction_toml(
            "set_bonus",
            "Kundarak Delving Equipment",
            "name",
            "\"Kundarak Delving Equipment\"",
            "\"Kundarak Delving Gear\"",
        ) + &correction_toml("set_bonus", "Perfect Silence", "name", "\"Perfect Silence\"", "\"Perfect Quiet\"")),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 2);
    let (item_set_name, linked_set_name): (String, String) = db
        .query_row(
            "SELECT i.set_bonus, s.name FROM items i JOIN set_bonus_items si ON si.item_id = i.id JOIN set_bonuses s ON s.id = si.set_id
              WHERE i.name = 'Kundarak Delving Boots'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((item_set_name.as_str(), linked_set_name.as_str()), ("Kundarak Delving Gear", "Kundarak Delving Gear"));
    let augment_set_name: String =
        db.query_row("SELECT set_bonus FROM augments WHERE name = 'Perfect Silence'", [], |r| r.get(0)).unwrap();
    assert_eq!(augment_set_name, "Perfect Quiet");
}

#[test]
fn correcting_an_items_set_moves_its_set_link() {
    let (db, _) = built_db_with(&[(
        "corrections.toml",
        &correction_toml(
            "item",
            "Kundarak Delving Boots",
            "set_bonus",
            "\"Kundarak Delving Equipment\"",
            "\"Eminence of Winter\"",
        ),
    )])
    .unwrap();
    let linked_set_names: Vec<String> = db
        .prepare(
            "SELECT s.name FROM set_bonus_items si JOIN set_bonuses s ON s.id = si.set_id JOIN items i ON i.id = si.item_id
              WHERE i.name = 'Kundarak Delving Boots'",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(linked_set_names, ["Eminence of Winter"]);
    let error = built_db_with(&[(
        "corrections.toml",
        &correction_toml(
            "item",
            "Kundarak Delving Boots",
            "set_bonus",
            "\"Kundarak Delving Equipment\"",
            "\"No Such Set\"",
        ),
    )])
    .unwrap_err();
    assert!(error.contains("No Such Set"), "{error}");
}

#[test]
fn corrects_only_his_items_not_the_wikis() {
    let wiki = WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap();
    let corrections = parsed_corrections(&[(
        "corrections.toml",
        &correction_toml("item", "Battle Axe of the Oozing Hunger", "minimum_level", "29", "30"),
    )])
    .unwrap();
    let error = built_db_with_wiki(&wiki, &corrections).unwrap_err();
    assert!(error.contains("Battle Axe of the Oozing Hunger"), "{error}");
}

#[test]
fn corrects_only_his_quests_not_the_wikis() {
    let wiki = WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap();
    let corrections = parsed_corrections(&[(
        "corrections.toml",
        &correction_toml("quest", "Ghosts of Perdition", "level", "32", "31"),
    )])
    .unwrap();
    let error = built_db_with_wiki(&wiki, &corrections).unwrap_err();
    assert!(error.contains("Ghosts of Perdition"), "{error}");
}

#[test]
fn corrects_only_his_augments_not_the_wikis() {
    let wiki = WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap();
    for correction_text in [
        correction_toml("augment", "Test Gem of Oozing Resistance", "min_level", "29", "30"),
        qualified_correction_toml(
            "augment_bonus",
            "Test Gem of Oozing Resistance",
            "stat = \"Strength\"\nbonus_type = \"Insight\"",
            "value",
            "3",
            "4",
        ),
    ] {
        let corrections = parsed_corrections(&[("corrections.toml", &correction_text)]).unwrap();
        let error = built_db_with_wiki(&wiki, &corrections).unwrap_err();
        assert!(error.contains("Test Gem of Oozing Resistance"), "{correction_text}: {error}");
    }
}

#[test]
fn runs_before_the_wiki_merge_so_a_corrected_description_is_his() {
    let wiki = WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap();
    let corrections = parsed_corrections(&[(
        "corrections.toml",
        &correction_toml("item", "+1 Ember Repeating Light Crossbow", "description", "\"null\"", "\"Corrected text.\""),
    )])
    .unwrap();
    let (db, report) = built_db_with_wiki(&wiki, &corrections).unwrap();
    assert_eq!(report.correction_applied_count, 1);
    let description: Option<String> = db
        .query_row("SELECT description FROM items WHERE name = '+1 Ember Repeating Light Crossbow'", [], |r| r.get(0))
        .optional()
        .unwrap()
        .flatten();
    assert_eq!(description.as_deref(), Some("Corrected text."));
}

#[test]
fn a_boolean_field_takes_zero_or_one() {
    let (db, _) =
        built_db_with(&[("corrections.toml", &correction_toml("quest", "Plane of Night", "is_raid", "1", "0"))])
            .unwrap();
    let is_raid: bool =
        db.query_row("SELECT is_raid FROM quests WHERE name = 'Plane of Night'", [], |r| r.get(0)).unwrap();
    assert!(!is_raid);
    let error =
        parsed_corrections(&[("corrections.toml", &correction_toml("quest", "Plane of Night", "is_raid", "1", "2"))])
            .unwrap_err();
    assert!(error.contains("is_raid") && error.contains('2'), "{error}");
}

#[test]
fn flags_an_item_legacy_and_counts_it() {
    let (db, report) =
        built_db_with(&[("corrections.toml", &correction_toml("item", "Docent of Defiance", "is_legacy", "0", "1"))])
            .unwrap();
    let is_legacy: bool =
        db.query_row("SELECT is_legacy FROM items WHERE name = 'Docent of Defiance'", [], |r| r.get(0)).unwrap();
    assert!(is_legacy);
    assert_eq!(report.legacy_item_count, 4, "the corrected item and the three legacy fixture items");
}

#[test]
fn embedded_corrections_flag_the_tempests_spine_armours_the_wiki_says_no_longer_drop() {
    let corrections = Corrections::embedded().unwrap();
    for (old_name, replacement_name) in [
        ("Mithral Breastplate of the Elements", "Elemental Mithral Breastplate"),
        ("Platemail of Giants", "Full Plate of Giants"),
        ("Robe of Arcane Power", "Robe of Arcane Puissance"),
    ] {
        let legacy_flag = corrections
            .entries
            .iter()
            .find(|c| c.name == old_name && c.field == "is_legacy")
            .unwrap_or_else(|| panic!("{old_name} has no is_legacy correction"));
        assert_eq!(
            (legacy_flag.from.clone(), legacy_flag.to.clone()),
            (CorrectionValue::Integer(0), CorrectionValue::Integer(1))
        );
        assert!(legacy_flag.reason.contains(replacement_name), "{}", legacy_flag.reason);
    }
}

fn qualified_correction_toml(
    kind: &str,
    name: &str,
    qualifier_keys: &str,
    field: &str,
    from: &str,
    to: &str,
) -> String {
    correction_toml(kind, name, field, from, to).replacen("\nfield = ", &format!("\n{qualifier_keys}\nfield = "), 1)
}

#[test]
fn item_and_set_tier_stat_transform_corrections_follow_written_links() {
    let (source_db, _) = built_db_with(&[]).unwrap();
    for (kind, owner_sql) in [
        (
            "item_bonus",
            "SELECT i.name, NULL, s.name, bt.name FROM items i
             JOIN item_enchantments l ON l.item_id = i.id",
        ),
        (
            "set_tier_bonus",
            "SELECT sb.name, t.equipped_count, s.name, bt.name FROM set_bonuses sb
             JOIN set_bonus_tiers t ON t.set_id = sb.id
             JOIN set_bonus_tier_enchantments l ON l.tier_id = t.id",
        ),
    ] {
        let source_sql = format!(
            "{owner_sql} JOIN enchantments e ON e.id = l.enchantment_id
             JOIN enchantment_stats es ON es.enchantment_id = e.id
             JOIN stats s ON s.id = es.stat_id
             JOIN bonus_types bt ON bt.id = COALESCE(es.bonus_type_id, l.bonus_type_id)
             WHERE es.amount_from = 1 AND l.value IS NOT NULL AND es.scale = 1
             ORDER BY l.value % 2 DESC, l.value DESC LIMIT 1"
        );
        let (owner_name, tier_count, stat_name, bonus_type): (String, Option<i64>, String, String) = source_db
            .query_row(&source_sql, [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
            .unwrap();
        let qualifier = format!(
            "{}stat = {stat_name:?}\nbonus_type = {bonus_type:?}",
            tier_count.map_or(String::new(), |count| format!("equipped_count = {count}\n"))
        );
        let corrections = format!(
            "{}{}",
            qualified_correction_toml(kind, &owner_name, &qualifier, "scale", "1.0", "0.5"),
            qualified_correction_toml(kind, &owner_name, &qualifier, "rounding", "\"down\"", "\"up\""),
        );
        let (corrected_db, report) = built_db_with(&[("transforms.toml", &corrections)]).unwrap();
        assert_eq!(
            (report.correction_applied_count, report.correction_stale_count),
            (2, 0),
            "{kind}: {:?}",
            report.stale_corrections
        );
        let corrected_sql = source_sql
            .replace("SELECT i.name, NULL, s.name, bt.name", "SELECT es.scale, es.rounding, l.value, NULL")
            .replace("SELECT sb.name, t.equipped_count, s.name, bt.name", "SELECT es.scale, es.rounding, l.value, NULL")
            .replace("AND es.scale = 1", "AND es.scale = 0.5");
        let (scale, rounding, _, _): (f64, String, i64, Option<i64>) = corrected_db
            .query_row(&corrected_sql, [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
            .unwrap();
        assert_eq!((scale, rounding.as_str()), (0.5, "up"));
    }
}

fn item_names(db: &Connection) -> Vec<String> {
    db.prepare("SELECT name FROM items ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn augment_bonus_rows(db: &Connection, augment_name: &str) -> Vec<(String, Option<String>, Option<i64>)> {
    db.prepare(
        "SELECT s.name, bt.name, CASE es.amount_from WHEN 0 THEN es.constant WHEN 1 THEN ae.value ELSE ae.value2 END
           FROM augments a JOIN augment_enchantments ae ON ae.augment_id = a.id
           JOIN enchantment_stats es ON es.enchantment_id = ae.enchantment_id JOIN stats s ON s.id = es.stat_id
           LEFT JOIN bonus_types bt ON bt.id = COALESCE(es.bonus_type_id, ae.bonus_type_id)
          WHERE a.name = ?1 ORDER BY a.id, ae.sort_order, es.sort_order",
    )
    .unwrap()
    .query_map([augment_name], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

fn item_socket_labels(db: &Connection, item_name: &str) -> Vec<String> {
    db.prepare(
        "SELECT t.label FROM items i JOIN item_augment_slots s ON s.item_id = i.id JOIN augment_slot_types t ON t.id = s.slot_id
          WHERE i.name = ?1 ORDER BY s.sort_order",
    )
    .unwrap()
    .query_map([item_name], |r| r.get(0))
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

fn row_count(db: &Connection, sql: &str) -> i64 {
    db.query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn renames_an_item_augment_and_quest_after_their_other_corrections() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(correction_toml("item", "Docent of Defiance", "name", "\"Docent of Defiance\"", "\"Docent of the Defiant\"")
            + &correction_toml("item", "Docent of Defiance", "minimum_level", "10", "11")
            + &correction_toml("augment", "Voidscale", "name", "\"Voidscale\"", "\"Void Scale\"")
            + &correction_toml("augment", "Voidscale", "min_level", "31", "30")
            + &correction_toml("quest", "Plane of Night", "name", "\"Plane of Night\"", "\"The Plane of Night\"")),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 5, "{:?}", report.stale_corrections);
    let (minimum_level, wiki_url): (i64, String) = db
        .query_row("SELECT minimum_level, wiki_url FROM items WHERE name = 'Docent of the Defiant'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((minimum_level, wiki_url.as_str()), (11, "https://ddowiki.com/page/Item:Docent_of_the_Defiant"));
    assert!(!item_names(&db).contains(&"Docent of Defiance".to_string()));
    assert_eq!(augment_min_levels(&db, "Void Scale"), [Some(30)]);
    assert_eq!(row_count(&db, "SELECT COUNT(*) FROM quests WHERE name = 'The Plane of Night'"), 1);
}

#[test]
fn a_renamed_quest_links_the_loot_his_drop_text_names_by_the_new_name() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &correction_toml(
            "quest",
            "ToEE: First Level and Earth Temple",
            "name",
            "\"ToEE: First Level and Earth Temple\"",
            "\"Temple of Elemental Evil Part One\"",
        ),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 1, "{:?}", report.stale_corrections);
    let linked_items: Vec<String> = db
        .prepare(
            "SELECT items.name FROM sources JOIN quests ON quests.id = sources.quest_id
               JOIN items ON items.id = sources.item_id
              WHERE quests.name = 'Temple of Elemental Evil Part One'",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(linked_items, ["+3 Combustion Scorched Battle Axe"]);
    let is_legacy: bool = db
        .query_row("SELECT is_legacy FROM items WHERE name = '+3 Combustion Scorched Battle Axe'", [], |r| r.get(0))
        .unwrap();
    assert!(!is_legacy, "a segment naming a quest of his is current, even when it also names a legacy source");
}

#[test]
fn a_rename_his_files_already_carry_goes_stale_for_every_renamable_kind() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(correction_toml("item", "Docent of Defiant", "name", "\"Docent of Defiant\"", "\"Docent of Defiance\"")
            + &correction_toml("augment", "Void Scale", "name", "\"Void Scale\"", "\"Voidscale\"")
            + &correction_toml("quest", "Plane of Nite", "name", "\"Plane of Nite\"", "\"Plane of Night\"")
            + &correction_toml("adventure_pack", "Vault of Nite", "name", "\"Vault of Nite\"", "\"Vault of Night\"")
            + &correction_toml("patron", "The Twelfe", "name", "\"The Twelfe\"", "\"The Twelve\"")
            + &correction_toml("set_bonus", "Perfect Silense", "name", "\"Perfect Silense\"", "\"Perfect Silence\"")
            + &correction_toml("socket_label", "redd", "name", "\"redd\"", "\"red\"")),
    )])
    .unwrap();
    assert_eq!(
        (report.correction_applied_count, report.correction_stale_count),
        (0, 7),
        "{:?}",
        report.stale_corrections
    );
    assert!(recorded_corrections(&db).is_empty());
    assert_eq!(
        report.stale_corrections.iter().find(|stale_correction| stale_correction.kind == "item").unwrap(),
        &StaleCorrection {
            kind: "item".into(),
            name: "Docent of Defiant".into(),
            field: "name".into(),
            cause: StaleCorrectionCause::RenameDoneUpstream { new_name: "Docent of Defiance".into() },
            file_name: "corrections.toml".into(),
        }
    );
    assert!(item_names(&db).contains(&"Docent of Defiance".to_string()));
}

#[test]
fn a_rename_whose_name_and_to_both_match_nothing_fails() {
    let error = built_db_with(&[(
        "corrections.toml",
        &correction_toml("quest", "Plane of Nite", "name", "\"Plane of Nite\"", "\"Plain of Night\""),
    )])
    .unwrap_err();
    assert!(
        error.contains("Plane of Nite") && error.contains("Plain of Night") && error.contains("corrections.toml"),
        "{error}"
    );
}

#[test]
fn renaming_an_item_or_quest_to_a_name_already_taken_fails() {
    let error = built_db_with(&[(
        "corrections.toml",
        &correction_toml("item", "Docent of Defiance", "name", "\"Docent of Defiance\"", "\"Five Rings\""),
    )])
    .unwrap_err();
    assert!(error.contains("Five Rings") && error.contains("Docent of Defiance"), "{error}");
}

#[test]
fn a_wiki_file_naming_the_old_spelling_fails_the_build() {
    let wiki = WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap();
    let corrections = parsed_corrections(&[(
        "corrections.toml",
        &correction_toml("augment", "Minor Fire Guard", "name", "\"Minor Fire Guard\"", "\"Lesser Fire Guard\""),
    )])
    .unwrap();
    let error = built_db_with_wiki(&wiki, &corrections).unwrap_err();
    assert!(error.contains("Minor Fire Guard"), "{error}");
}

#[test]
fn a_family_narrows_an_augment_correction() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(qualified_correction_toml(
            "augment",
            "+5 Fortitude Save",
            "family = \"Greensteel_Heroic\"",
            "min_level",
            "11",
            "12",
        ) + &qualified_correction_toml(
            "augment",
            "+5 Fortitude Save",
            "family = \"Greensteel_Heroic\"",
            "name",
            "\"+5 Fortitude Save\"",
            "\"+5 Fortitude\"",
        )),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 2, "{:?}", report.stale_corrections);
    assert_eq!(augment_min_levels(&db, "+5 Fortitude"), [Some(12), Some(12)]);
    let qualifiers: Vec<String> = db
        .prepare("SELECT qualifier FROM corrections ORDER BY field")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(qualifiers, ["family \"Greensteel_Heroic\"", "family \"Greensteel_Heroic\""]);
    let error = built_db_with(&[(
        "corrections.toml",
        &qualified_correction_toml("augment", "+5 Fortitude Save", "family = \"Ruby\"", "min_level", "11", "12"),
    )])
    .unwrap_err();
    assert!(error.contains("+5 Fortitude Save") && error.contains("Ruby"), "{error}");
    let error = parsed_corrections(&[(
        "corrections.toml",
        &qualified_correction_toml("item", "Five Rings", "family = \"Ruby\"", "minimum_level", "1", "2"),
    )])
    .unwrap_err();
    assert!(error.contains("family") && error.contains("Five Rings"), "{error}");
}

#[test]
fn removes_an_item_with_its_child_rows() {
    let item_ids: Vec<i64> = {
        let db = built_db_with(&[]).unwrap().0;
        ["Legendary Cloak of Winter", "Acid Rune Arm"]
            .iter()
            .map(|name| db.query_row("SELECT id FROM items WHERE name = ?1", [name], |r| r.get(0)).unwrap())
            .collect()
    };
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(correction_toml("item", "Legendary Cloak of Winter", "remove", "0", "1")
            + &correction_toml("item", "Acid Rune Arm", "remove", "0", "1")),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 2, "{:?}", report.stale_corrections);
    assert!(!item_names(&db).iter().any(|name| name == "Legendary Cloak of Winter" || name == "Acid Rune Arm"));
    let id_list = format!("({}, {})", item_ids[0], item_ids[1]);
    for child_table in ["item_enchantments", "item_augment_slots", "sources", "set_bonus_items", "item_clickies"] {
        assert_eq!(
            row_count(&db, &format!("SELECT COUNT(*) FROM {child_table} WHERE item_id IN {id_list}")),
            0,
            "{child_table}"
        );
    }
    assert_eq!(
        row_count(
            &db,
            &format!("SELECT COUNT(*) FROM modifiers WHERE source_kind = 'item' AND source_id IN {id_list}")
        ),
        0
    );
    assert_eq!(
        recorded_corrections(&db)[0],
        ("item".into(), "Acid Rune Arm".into(), "remove".into(), "0".into(), "1".into())
    );
    let error =
        parsed_corrections(&[("corrections.toml", &correction_toml("item", "Acid Rune Arm", "remove", "1", "0"))])
            .unwrap_err();
    assert!(error.contains("remove") && error.contains("Acid Rune Arm"), "{error}");
}

#[test]
fn removes_an_augment_with_its_child_rows() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(correction_toml("augment", "Perfect Silence", "remove", "0", "1")
            + &correction_toml("augment", "Ruby of Acid Damage", "remove", "0", "1")),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 2, "{:?}", report.stale_corrections);
    assert_eq!(
        row_count(&db, "SELECT COUNT(*) FROM augments WHERE name IN ('Perfect Silence', 'Ruby of Acid Damage')"),
        0
    );
    for orphan_sql in [
        "SELECT COUNT(*) FROM augment_slots WHERE augment_id NOT IN (SELECT id FROM augments)",
        "SELECT COUNT(*) FROM augment_enchantments WHERE augment_id NOT IN (SELECT id FROM augments)",
        "SELECT COUNT(*) FROM set_bonus_augments WHERE augment_id NOT IN (SELECT id FROM augments)",
        "SELECT COUNT(*) FROM modifiers WHERE source_kind = 'augment' AND source_id NOT IN (SELECT id FROM augments)",
    ] {
        assert_eq!(row_count(&db, orphan_sql), 0, "{orphan_sql}");
    }
}

#[test]
fn corrects_an_augment_bonus_value_and_type_without_touching_the_shared_bonus() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(qualified_correction_toml(
            "augment_bonus",
            "Silverscale",
            "stat = \"Healing Amplification\"\nbonus_type = \"Competence\"",
            "value",
            "56",
            "60",
        ) + &qualified_correction_toml(
            "augment_bonus",
            "Silverscale",
            "stat = \"Repair Amplification\"\nbonus_type = \"Enhancement\"",
            "bonus_type",
            "\"Enhancement\"",
            "\"Competence\"",
        ) + &qualified_correction_toml(
            "augment_bonus",
            "Silverscale",
            "stat = \"Negative Healing Amplification\"\nbonus_type = \"Profane\"",
            "value",
            "55",
            "57",
        )),
    )])
    .unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (2, 1));
    assert_eq!(
        augment_bonus_rows(&db, "Silverscale"),
        [
            ("Healing Amplification".into(), Some("Competence".into()), Some(60)),
            ("Negative Healing Amplification".into(), Some("Profane".into()), Some(56)),
            ("Repair Amplification".into(), Some("Competence".into()), Some(56)),
        ]
    );
    assert!(
        matches!(&report.stale_corrections[0].cause, StaleCorrectionCause::ValueChanged { maetrim_value, .. } if maetrim_value == "56"),
        "{:?}",
        report.stale_corrections
    );
    assert_eq!(
        row_count(
            &db,
            "SELECT COUNT(*) FROM augment_enchantments ae JOIN augments a ON a.id = ae.augment_id
            JOIN enchantment_stats es ON es.enchantment_id = ae.enchantment_id JOIN stats s ON s.id = es.stat_id
            WHERE a.name = 'Silverscale' AND s.name = 'Healing Amplification' AND ae.value = 56"
        ),
        0,
        "the old bonus row is left unchanged for anything else that carries it, then deleted as nothing does"
    );
    assert_eq!(
        row_count(
            &db,
            "SELECT COUNT(*) FROM enchantments e WHERE NOT EXISTS (SELECT 1 FROM item_enchantments r WHERE r.enchantment_id = e.id)
               AND NOT EXISTS (SELECT 1 FROM item_augment_slot_option_enchantments r WHERE r.enchantment_id = e.id)
               AND NOT EXISTS (SELECT 1 FROM augment_enchantments r WHERE r.enchantment_id = e.id)
               AND NOT EXISTS (SELECT 1 FROM feat_enchantments r WHERE r.enchantment_id = e.id)
               AND NOT EXISTS (SELECT 1 FROM set_bonus_tier_enchantments r WHERE r.enchantment_id = e.id)"
        ),
        0,
        "a bonus the corrections leave nothing carrying is deleted"
    );
    assert_eq!(report.bonus_count as i64, row_count(&db, "SELECT COUNT(*) FROM enchantment_stats"));
    let qualifiers: Vec<String> = db
        .prepare("SELECT qualifier FROM corrections ORDER BY qualifier")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(qualifiers, ["Healing Amplification / Competence", "Repair Amplification / Enhancement"]);
}

#[test]
fn adds_a_bonus_an_augment_lacks_and_goes_stale_once_he_carries_it() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(correction_toml(
            "augment_bonus",
            "Voidscale",
            "add",
            "\"null\"",
            "{ stat = \"Physical Resistance Rating\", bonus_type = \"Exceptional\", value = 2 }",
        ) + &correction_toml(
            "augment_bonus",
            "Voidscale",
            "add",
            "\"null\"",
            "{ stat = \"Universal Spell Lore\", bonus_type = \"Exceptional\", value = 5 }",
        )),
    )])
    .unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 1));
    assert_eq!(
        augment_bonus_rows(&db, "Voidscale"),
        [
            ("Universal Spell Lore".into(), Some("Exceptional".into()), Some(5)),
            ("Physical Resistance Rating".into(), Some("Exceptional".into()), Some(2)),
        ]
    );
    let to_value: String = db.query_row("SELECT to_value FROM corrections", [], |r| r.get(0)).unwrap();
    assert_eq!(to_value, r#"{"bonus_type":"Exceptional","stat":"Physical Resistance Rating","value":2}"#);
    let error = built_db_with(&[(
        "corrections.toml",
        &correction_toml(
            "augment_bonus",
            "Voidscale",
            "add",
            "\"null\"",
            "{ stat = \"Physical Resistence\", bonus_type = \"Exceptional\", value = 2 }",
        ),
    )])
    .unwrap_err();
    assert!(error.contains("Physical Resistence"), "{error}");
    let error =
        parsed_corrections(&[("corrections.toml", &correction_toml("augment_bonus", "Voidscale", "value", "5", "6"))])
            .unwrap_err();
    assert!(error.contains("stat") && error.contains("Voidscale"), "{error}");
}

fn item_bonus_rows(db: &Connection, item_name: &str) -> Vec<(String, Option<String>, Option<i64>)> {
    db.prepare(
        "SELECT s.name, bt.name, CASE es.amount_from WHEN 0 THEN es.constant WHEN 1 THEN ie.value ELSE ie.value2 END
           FROM items i JOIN item_enchantments ie ON ie.item_id = i.id
           JOIN enchantment_stats es ON es.enchantment_id = ie.enchantment_id JOIN stats s ON s.id = es.stat_id
           LEFT JOIN bonus_types bt ON bt.id = COALESCE(es.bonus_type_id, ie.bonus_type_id)
          WHERE i.name = ?1 ORDER BY ie.sort_order, es.sort_order",
    )
    .unwrap()
    .query_map([item_name], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

fn set_tier_bonus_rows(db: &Connection, set_name: &str, equipped_count: i64) -> Vec<(String, String, i64)> {
    db.prepare(
        "SELECT stats.name, bonus_types.name,
                CASE enchantment_stats.amount_from WHEN 0 THEN enchantment_stats.constant
                  WHEN 1 THEN set_bonus_tier_enchantments.value ELSE set_bonus_tier_enchantments.value2 END
         FROM set_bonuses
         JOIN set_bonus_tiers ON set_bonus_tiers.set_id = set_bonuses.id
         JOIN set_bonus_tier_enchantments ON set_bonus_tier_enchantments.tier_id = set_bonus_tiers.id
         JOIN enchantment_stats ON enchantment_stats.enchantment_id = set_bonus_tier_enchantments.enchantment_id
         JOIN stats ON stats.id = enchantment_stats.stat_id
         JOIN bonus_types ON bonus_types.id = COALESCE(enchantment_stats.bonus_type_id, set_bonus_tier_enchantments.bonus_type_id)
         WHERE set_bonuses.name = ?1 AND set_bonus_tiers.equipped_count = ?2
         ORDER BY set_bonus_tier_enchantments.sort_order, enchantment_stats.sort_order",
    )
    .unwrap()
    .query_map(rusqlite::params![set_name, equipped_count], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

#[test]
fn corrects_set_tier_bonuses_and_creates_a_missing_tier() {
    let corrections = qualified_correction_toml(
        "set_tier_bonus",
        "Eminence of Winter",
        "equipped_count = 2\nstat = \"Physical Resistance Rating\"\nbonus_type = \"Artifact\"",
        "value",
        "30",
        "31",
    ) + &qualified_correction_toml(
        "set_tier_bonus",
        "Eminence of Winter",
        "equipped_count = 2",
        "add",
        "\"null\"",
        "{ stat = \"Magical Resistance Rating\", bonus_type = \"Artifact\", value = 31 }",
    ) + &qualified_correction_toml(
        "set_tier",
        "Eminence of Winter",
        "equipped_count = 8",
        "add",
        "\"null\"",
        "{ equipped_count = 8, description = \"A new tier.\" }",
    ) + &qualified_correction_toml(
        "set_tier_bonus",
        "Eminence of Winter",
        "equipped_count = 8",
        "add",
        "\"null\"",
        "{ stat = \"Constitution\", bonus_type = \"Artifact\", value = 4 }",
    );
    let (db, report) = built_db_with(&[("corrections.toml", &corrections)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (4, 0));
    assert_eq!(
        set_tier_bonus_rows(&db, "Eminence of Winter", 2),
        [
            ("Physical Resistance Rating".into(), "Artifact".into(), 31),
            ("Magical Resistance Rating".into(), "Artifact".into(), 31),
        ]
    );
    assert_eq!(set_tier_bonus_rows(&db, "Eminence of Winter", 8), [("Constitution".into(), "Artifact".into(), 4)]);
}

#[test]
fn set_tier_add_goes_stale_when_the_tier_exists() {
    let correction = qualified_correction_toml(
        "set_tier",
        "Eminence of Winter",
        "equipped_count = 2",
        "add",
        "\"null\"",
        "{ equipped_count = 2, description = \"Already here.\" }",
    );
    let (_, report) = built_db_with(&[("corrections.toml", &correction)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 1));
}

#[test]
fn refuses_a_set_tier_bonus_without_a_type() {
    let correction = qualified_correction_toml(
        "set_tier_bonus",
        "Eminence of Winter",
        "equipped_count = 2",
        "add",
        "\"null\"",
        "{ stat = \"Constitution\", bonus_type = \"null\", value = 1 }",
    );
    assert!(parsed_corrections(&[("corrections.toml", &correction)]).is_err());
    for kind in ["item_bonus", "augment_bonus"] {
        let correction = correction_toml(
            kind,
            "Owner",
            "add",
            "\"null\"",
            "{ stat = \"Constitution\", bonus_type = \"null\", value = 1 }",
        );
        assert!(parsed_corrections(&[("corrections.toml", &correction)]).is_err(), "{kind}");
    }
}

#[test]
fn missing_set_tiers_name_the_set_and_equipped_count() {
    for kind in ["set_tier", "set_tier_bonus"] {
        let (field, from, to) = if kind == "set_tier" {
            ("description", "\"null\"", "\"Missing tier\"")
        } else {
            ("add", "\"null\"", "{ stat = \"Constitution\", bonus_type = \"Artifact\", value = 1 }")
        };
        let correction = qualified_correction_toml(kind, "Eminence of Winter", "equipped_count = 99", field, from, to);
        let error = built_db_with(&[("corrections.toml", &correction)]).unwrap_err();
        assert!(error.contains("set \"Eminence of Winter\" has no tier with equipped_count 99"), "{error}");
        assert!(!error.contains("set_bonus_tiers.name"), "{error}");
    }
    let correction = qualified_correction_toml(
        "set_tier",
        "Unknown set",
        "equipped_count = 2",
        "add",
        "\"null\"",
        "{ equipped_count = 2, description = \"A new tier\" }",
    );
    let error = built_db_with(&[("corrections.toml", &correction)]).unwrap_err();
    assert!(error.contains("set_bonuses.name") && !error.contains("set_bonus_tiers.name"), "{error}");
}

#[test]
fn removes_only_the_qualified_bonus_link_for_each_owner_kind() {
    for (kind, name, owner_qualifier) in [
        ("item_bonus", "Docent of Defiance", ""),
        ("augment_bonus", "Silverscale", "family = \"DinosaurBone\""),
        ("set_tier_bonus", "Eminence of Winter", "equipped_count = 2"),
    ] {
        let corrections = qualified_correction_toml(
            kind,
            name,
            owner_qualifier,
            "add",
            "\"null\"",
            "{ stat = \"Strength\", bonus_type = \"Quality\", value = 2 }",
        ) + &qualified_correction_toml(
            kind,
            name,
            &format!("{owner_qualifier}\nstat = \"Strength\"\nbonus_type = \"Quality\""),
            "value",
            "2",
            "3",
        ) + &qualified_correction_toml(
            kind,
            name,
            owner_qualifier,
            "add",
            "\"null\"",
            "{ stat = \"Strength\", bonus_type = \"Artifact\", value = 2 }",
        ) + &qualified_correction_toml(
            "item_bonus",
            "Acid Rune Arm",
            "",
            "add",
            "\"null\"",
            "{ stat = \"Strength\", bonus_type = \"Quality\", value = 3 }",
        );
        let removal_qualifier = format!("{owner_qualifier}\nstat = \"Strength\"\nbonus_type = \"Quality\"");
        for (bonus_qualifier, from, applied, stale) in
            [("", "3", 5, 0), ("\nbonus_value = 3", "3", 5, 0), ("", "2", 4, 1), ("\nbonus_value = 2", "2", 4, 1)]
        {
            let removal = qualified_correction_toml(
                kind,
                name,
                &format!("{removal_qualifier}{bonus_qualifier}"),
                "remove",
                from,
                "\"null\"",
            );
            let (db, report) = built_db_with(&[("corrections.toml", &(corrections.clone() + &removal))]).unwrap();
            assert_eq!((report.correction_applied_count, report.correction_stale_count), (applied, stale), "{kind}");
            let shared_bonuses = item_bonus_rows(&db, "Acid Rune Arm");
            assert!(shared_bonuses.contains(&("Strength".into(), Some("Quality".into()), Some(3))));
            let owner_bonuses = match kind {
                "item_bonus" => item_bonus_rows(&db, name),
                "augment_bonus" => augment_bonus_rows(&db, name),
                _ => set_tier_bonus_rows(&db, name, 2)
                    .into_iter()
                    .map(|(stat, bonus_type, amount)| (stat, Some(bonus_type), Some(amount)))
                    .collect(),
            };
            assert_eq!(
                owner_bonuses.contains(&("Strength".into(), Some("Quality".into()), Some(3))),
                stale == 1,
                "{kind} {bonus_qualifier} {from}"
            );
            assert!(owner_bonuses.contains(&("Strength".into(), Some("Artifact".into()), Some(2))));
        }
        let absent_removal = qualified_correction_toml(kind, name, &removal_qualifier, "remove", "3", "\"null\"");
        let (_, report) = built_db_with(&[("corrections.toml", &absent_removal)]).unwrap();
        assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 1), "{kind}");
        let same_type_corrections = corrections
            + &qualified_correction_toml(
                kind,
                name,
                &format!("{owner_qualifier}\nstat = \"Strength\"\nbonus_type = \"Artifact\""),
                "bonus_type",
                "\"Artifact\"",
                "\"Quality\"",
            );
        for from in ["3", "2", "99"] {
            let ambiguous_removal =
                qualified_correction_toml(kind, name, &removal_qualifier, "remove", from, "\"null\"");
            let error = built_db_with(&[("corrections.toml", &(same_type_corrections.clone() + &ambiguous_removal))])
                .unwrap_err();
            for expected_text in [name, "Strength", "Quality", "values [3, 2]", "specify bonus_value"] {
                assert!(error.contains(expected_text), "{expected_text} missing from {error}");
            }
        }
        let equal_value_corrections = same_type_corrections.clone()
            + &qualified_correction_toml(
                kind,
                name,
                &format!("{removal_qualifier}\nbonus_value = 2"),
                "value",
                "2",
                "3",
            )
            + &absent_removal;
        let error = built_db_with(&[("corrections.toml", &equal_value_corrections)]).unwrap_err();
        assert!(error.contains("values [3, 3]") && error.contains("specify bonus_value"), "{error}");
        let narrowed_removal = qualified_correction_toml(
            kind,
            name,
            &format!("{removal_qualifier}\nbonus_value = 3"),
            "remove",
            "3",
            "\"null\"",
        );
        let (db, report) =
            built_db_with(&[("corrections.toml", &(same_type_corrections + &narrowed_removal))]).unwrap();
        assert_eq!((report.correction_applied_count, report.correction_stale_count), (6, 0), "{kind}");
        let remaining_bonuses = match kind {
            "item_bonus" => item_bonus_rows(&db, name),
            "augment_bonus" => augment_bonus_rows(&db, name),
            _ => set_tier_bonus_rows(&db, name, 2)
                .into_iter()
                .map(|(stat, bonus_type, amount)| (stat, Some(bonus_type), Some(amount)))
                .collect(),
        };
        assert!(remaining_bonuses.contains(&("Strength".into(), Some("Quality".into()), Some(2))), "{kind}");
        assert!(!remaining_bonuses.contains(&("Strength".into(), Some("Quality".into()), Some(3))), "{kind}");
    }
}

#[test]
fn bonus_removal_requires_qualifiers_a_current_integer_and_a_null_destination() {
    for (kind, owner_qualifier) in [("item_bonus", ""), ("augment_bonus", ""), ("set_tier_bonus", "equipped_count = 2")]
    {
        for (bonus_qualifier, from, to) in [
            ("", "3", "\"null\""),
            ("stat = \"Strength\"", "3", "\"null\""),
            ("stat = \"Strength\"\nbonus_type = \"Quality\"", "\"null\"", "1"),
            ("stat = \"Strength\"\nbonus_type = \"Quality\"", "3", "1"),
            ("stat = \"Strength\"\nbonus_type = \"Quality\"", "3.5", "\"null\""),
            ("stat = \"Strength\"\nbonus_type = \"Quality\"", "\"3\"", "\"null\""),
        ] {
            let correction = qualified_correction_toml(
                kind,
                "Owner",
                &format!("{owner_qualifier}\n{bonus_qualifier}"),
                "remove",
                from,
                to,
            );
            assert!(parsed_corrections(&[("corrections.toml", &correction)]).is_err(), "{correction}");
        }
    }
}

#[test]
fn corrects_a_set_tier_bonus_type_and_value() {
    let corrections = qualified_correction_toml(
        "set_tier_bonus",
        "Eminence of Winter",
        "equipped_count = 2\nstat = \"Physical Resistance Rating\"\nbonus_type = \"Artifact\"",
        "bonus_type",
        "\"Artifact\"",
        "\"Profane\"",
    ) + &qualified_correction_toml(
        "set_tier_bonus",
        "Eminence of Winter",
        "equipped_count = 2\nstat = \"Physical Resistance Rating\"\nbonus_type = \"Profane\"",
        "value",
        "30",
        "31",
    ) + &qualified_correction_toml(
        "set_tier_bonus",
        "Eminence of Winter",
        "equipped_count = 2",
        "add",
        "\"null\"",
        "{ stat = \"Physical Resistance Rating\", bonus_type = \"Profane\", value = 99 }",
    );
    let (db, report) = built_db_with(&[("corrections.toml", &corrections)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (2, 1));
    assert_eq!(
        set_tier_bonus_rows(&db, "Eminence of Winter", 2),
        [("Physical Resistance Rating".into(), "Profane".into(), 31),]
    );
}

#[test]
fn corrects_a_set_tier_description_and_removes_an_unlisted_tier() {
    let corrections =
        qualified_correction_toml(
            "set_tier",
            "Eminence of Winter",
            "equipped_count = 2",
            "description",
            "\"+30 Artifact bonus to Physical Resistance Rating\"",
            "\"+31 Artifact PRR\"",
        ) + &qualified_correction_toml("set_tier", "Eminence of Winter", "equipped_count = 3", "remove", "0", "1");
    let (db, report) = built_db_with(&[("corrections.toml", &corrections)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (2, 0));
    assert_eq!(row_count(&db, "SELECT COUNT(*) FROM set_bonus_tiers t JOIN set_bonuses s ON s.id = t.set_id WHERE s.name = 'Eminence of Winter' AND t.equipped_count = 3"), 0);
    assert_eq!(
        row_count(
            &db,
            "SELECT COUNT(*) FROM set_bonus_tier_enchantments WHERE tier_id NOT IN (SELECT id FROM set_bonus_tiers)"
        ),
        0
    );
    let (family_template, recorded_description): (String, String) = db
        .query_row(
            "SELECT e.text_template, c.to_value FROM set_bonus_tiers t JOIN set_bonuses s ON s.id = t.set_id
         JOIN set_bonus_tier_enchantments te ON te.tier_id = t.id JOIN enchantments e ON e.id = te.enchantment_id
         JOIN corrections c ON c.name = s.name AND c.kind = 'set_tier' AND c.field = 'description'
         WHERE s.name = 'Eminence of Winter' AND t.equipped_count = 2
         ORDER BY te.sort_order LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(family_template, "%b1 Physical Resistance Rating +{1}");
    assert_eq!(recorded_description, "\"+31 Artifact PRR\"");
}

#[test]
fn removing_a_set_tier_deletes_its_unowned_prose_family() {
    let data_files_dir = data_files_with_untyped("remove-prose-family", &[]);
    let set_file = data_files_dir.join("SetBonuses.xml");
    let set_xml = std::fs::read_to_string(&set_file).unwrap();
    let extra_set = "<SetBonus><Type>Removable Family Set</Type><Buff><EquippedCount>2</EquippedCount><Description>Unshared removable prose</Description></Buff></SetBonus></SetBonuses>";
    std::fs::write(&set_file, set_xml.replace("</SetBonuses>", extra_set)).unwrap();
    let correction =
        qualified_correction_toml("set_tier", "Removable Family Set", "equipped_count = 2", "remove", "0", "1");
    let (db, report) = built_db_from(&data_files_dir, &[("corrections.toml", &correction)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
    assert_eq!(row_count(&db, "SELECT COUNT(*) FROM enchantments WHERE name = 'Unshared removable prose'"), 0);
}

#[test]
fn set_tier_qualifiers_must_be_complete_and_match_the_added_tier() {
    for correction in [
        correction_toml("set_tier", "Eminence of Winter", "remove", "0", "1"),
        qualified_correction_toml("set_tier", "Eminence of Winter", "equipped_count = 0", "remove", "0", "1"),
        qualified_correction_toml(
            "set_tier",
            "Eminence of Winter",
            "equipped_count = 3",
            "add",
            "\"null\"",
            "{ equipped_count = 4, description = \"Wrong tier\" }",
        ),
    ] {
        assert!(parsed_corrections(&[("corrections.toml", &correction)]).is_err(), "{correction}");
    }
}

#[test]
fn embedded_set_corrections_keep_only_fact_changes() {
    let corrections = Corrections::embedded().unwrap();
    let set_bonus_additions = corrections
        .entries
        .iter()
        .filter(|correction| correction.kind.as_str() == "set_tier_bonus" && correction.field == "add");
    assert_eq!(set_bonus_additions.count(), 50);
    for stat in ["Melee Power", "Ranged Power"] {
        assert!(
            corrections.entries.iter().any(|correction| {
                correction.kind.as_str() == "set_tier_bonus"
                    && correction.name == "Heart of Blades"
                    && correction.equipped_count == Some(3)
                    && correction.stat.as_deref() == Some(stat)
                    && correction.bonus_type.as_deref() == Some("Artifact")
                    && correction.field == "value"
                    && correction.from == CorrectionValue::Integer(5)
                    && correction.to == CorrectionValue::Integer(10)
            }),
            "{stat}"
        );
    }
}

#[test]
fn embedded_set_corrections_fix_feather_falling_and_remove_only_celeritys_movement_bonus() {
    let corrections = Corrections::embedded().unwrap();
    assert!(corrections.entries.iter().any(|correction| {
        correction.kind.as_str() == "set_tier"
            && correction.name == "Tharne's Wrath"
            && correction.equipped_count == Some(3)
            && correction.field == "description"
            && correction.from == CorrectionValue::Text("Father Falling".into())
            && correction.to == CorrectionValue::Text("Feather Falling".into())
    }));
    assert!(corrections.entries.iter().any(|correction| {
        correction.kind.as_str() == "set_tier_bonus"
            && correction.name == "Celerity"
            && correction.equipped_count == Some(3)
            && correction.field == "remove"
            && correction.stat.as_deref() == Some("Movement Speed")
            && correction.bonus_type.as_deref() == Some("Enhancement")
            && correction.from == CorrectionValue::Integer(32)
            && correction.to == CorrectionValue::Null
    }));
    assert!(!corrections.entries.iter().any(|correction| {
        correction.kind.as_str() == "set_tier"
            && correction.name == "The Wreath of Flame"
            && correction.equipped_count == Some(3)
            && correction.field == "description"
    }));
}

fn item_effect_names(db: &Connection, item_name: &str) -> Vec<String> {
    db.prepare(
        "SELECT CASE WHEN INSTR(e.name, ' — ') > 0 THEN SUBSTR(e.name, 1, INSTR(e.name, ' — ') - 1) ELSE e.name END
           FROM items i JOIN item_enchantments ie ON ie.item_id = i.id JOIN enchantments e ON e.id = ie.enchantment_id
          WHERE i.name = ?1 AND NOT EXISTS (SELECT 1 FROM enchantment_stats es WHERE es.enchantment_id = e.id)
          ORDER BY ie.sort_order",
    )
    .unwrap()
    .query_map([item_name], |r| r.get(0))
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

#[test]
fn adds_a_bonus_an_item_lacks_and_goes_stale_once_he_carries_it() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(correction_toml(
            "item_bonus",
            "Docent of Defiance",
            "add",
            "\"null\"",
            "{ stat = \"Acid Resistance\", bonus_type = \"Enhancement\", value = 20 }",
        ) + &correction_toml(
            "item_bonus",
            "Docent of Defiance",
            "add",
            "\"null\"",
            "{ stat = \"Fire Resistance\", bonus_type = \"Enhancement\", value = 30 }",
        )),
    )])
    .unwrap();
    assert_eq!(
        (report.correction_applied_count, report.correction_stale_count),
        (1, 1),
        "{:?}",
        report.stale_corrections
    );
    assert_eq!(
        item_bonus_rows(&db, "Docent of Defiance"),
        [
            ("Fire Resistance".into(), Some("Enhancement".into()), Some(20)),
            ("Electric Resistance".into(), Some("Enhancement".into()), Some(20)),
            ("Cold Resistance".into(), Some("Enhancement".into()), Some(20)),
            ("Acid Resistance".into(), Some("Enhancement".into()), Some(20)),
        ]
    );
    let (qualifier, to_value): (String, String) =
        db.query_row("SELECT qualifier, to_value FROM corrections", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(qualifier, "Acid Resistance / Enhancement");
    assert_eq!(to_value, r#"{"bonus_type":"Enhancement","stat":"Acid Resistance","value":20}"#);
    let error = built_db_with(&[(
        "corrections.toml",
        &correction_toml(
            "item_bonus",
            "Docent of Defiance",
            "add",
            "\"null\"",
            "{ stat = \"Acid Resistence\", bonus_type = \"Enhancement\", value = 20 }",
        ),
    )])
    .unwrap_err();
    assert!(error.contains("Acid Resistence") && error.contains("Docent of Defiance"), "{error}");
}

#[test]
fn adds_an_effect_an_item_lacks_reusing_his_effect_row_and_goes_stale_once_he_carries_it() {
    let effect_count_without_corrections =
        row_count(&built_db_with(&[]).unwrap().0, "SELECT COUNT(*) FROM enchantments");
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(correction_toml("item_effect", "Docent of Defiance", "add", "\"null\"", "\"Feather Falling\"")
            + &correction_toml("item_effect", "Docent of Defiance", "add", "\"null\"", "\"Book Shot\"")
            + &correction_toml("item_effect", "Kundarak Delving Boots", "add", "\"null\"", "\"freedom of movement\"")),
    )])
    .unwrap();
    assert_eq!(
        (report.correction_applied_count, report.correction_stale_count),
        (2, 1),
        "{:?}",
        report.stale_corrections
    );
    assert_eq!(
        item_effect_names(&db, "Docent of Defiance"),
        ["Hidden Effect - Cursed Defiance", "Feather Falling", "Book Shot"]
    );
    assert_eq!(item_effect_names(&db, "Kundarak Delving Boots"), ["Freedom of Movement"]);
    assert_eq!(row_count(&db, "SELECT COUNT(*) FROM enchantments"), effect_count_without_corrections + 1);
    let qualifiers: Vec<String> = db
        .prepare("SELECT qualifier FROM corrections ORDER BY qualifier")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(qualifiers, ["Book Shot", "Feather Falling"]);
    let error = parsed_corrections(&[(
        "corrections.toml",
        &correction_toml("item_effect", "Docent of Defiance", "add", "\"null\"", "3"),
    )])
    .unwrap_err();
    assert!(error.contains("Docent of Defiance") && error.contains("effect"), "{error}");
}

fn effect_description(db: &Connection, effect_name: &str) -> Option<String> {
    db.query_row("SELECT description_template FROM enchantments WHERE name = ?1", [effect_name], |r| r.get(0)).unwrap()
}

#[test]
fn an_added_effect_writes_its_description_only_on_the_effect_it_creates() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(correction_toml(
            "item_effect",
            "Docent of Defiance",
            "add",
            "\"null\"",
            "{ name = \"Book Shot\", description = \"Hurls books at enemies.\" }",
        ) + &correction_toml(
            "item_effect",
            "Docent of Defiance",
            "add",
            "\"null\"",
            "{ name = \"Feather Falling\", description = \"Not his text.\" }",
        ) + &correction_toml(
            "item_effect",
            "Kundarak Delving Boots",
            "add",
            "\"null\"",
            "{ name = \"freedom of movement\", description = \"Not his text either.\" }",
        )),
    )])
    .unwrap();
    assert_eq!(
        (report.correction_applied_count, report.correction_stale_count),
        (2, 1),
        "{:?}",
        report.stale_corrections
    );
    assert_eq!(
        item_effect_names(&db, "Docent of Defiance"),
        ["Hidden Effect - Cursed Defiance", "Book Shot", "Feather Falling"]
    );
    assert_eq!(effect_description(&db, "Book Shot").as_deref(), Some("Hurls books at enemies."));
    assert!(
        effect_description(&db, "Feather Falling").unwrap().starts_with("This item"),
        "his description wins on the effect row the correction reuses"
    );
    let recorded: Vec<(String, String)> = db
        .prepare("SELECT qualifier, to_value FROM corrections WHERE qualifier = 'Book Shot'")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        recorded,
        [("Book Shot".to_string(), r#"{"description":"Hurls books at enemies.","name":"Book Shot"}"#.to_string())]
    );
    let error = parsed_corrections(&[(
        "corrections.toml",
        &correction_toml("item_effect", "Docent of Defiance", "add", "\"null\"", "{ name = \"Book Shot\" }"),
    )])
    .unwrap_err();
    assert!(error.contains("Docent of Defiance"), "{error}");
}

#[test]
fn adds_a_socket_an_item_lacks_and_goes_stale_once_he_carries_it() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(correction_toml("item_socket", "Docent of Defiance", "add", "\"null\"", "\"red\"")
            + &correction_toml(
                "item_socket",
                "Docent of Defiance",
                "add",
                "\"null\"",
                "\"crafting: slavelords extra\"",
            )
            + &correction_toml("item_socket", "Buckler of the Golden Age", "add", "\"null\"", "\"red\"")),
    )])
    .unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (2, 1));
    assert_eq!(item_socket_labels(&db, "Docent of Defiance"), ["red", "crafting: slavelords extra"]);
    assert_eq!(item_socket_labels(&db, "Buckler of the Golden Age"), ["red"]);
    let error = built_db_with(&[(
        "corrections.toml",
        &correction_toml("item_socket", "Docent of Defiance", "add", "\"null\"", "\"crafting: no such socket\""),
    )])
    .unwrap_err();
    assert!(error.contains("crafting: no such socket"), "{error}");
}

#[test]
fn renames_a_socket_label_everywhere_and_merges_into_an_existing_one() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(correction_toml(
            "socket_label",
            "crafting: attuned to heroism 1",
            "name",
            "\"crafting: attuned to heroism 1\"",
            "\"crafting: attuned by heroism: tier 1\"",
        ) + &correction_toml(
            "socket_label",
            "crafting: attuned to heroism 3",
            "name",
            "\"crafting: attuned to heroism 3\"",
            "\"crafting: attuned to heroism 2\"",
        )),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 2, "{:?}", report.stale_corrections);
    assert_eq!(
        item_socket_labels(&db, "Sireth, Spear of the Sky"),
        [
            "crafting: attuned by heroism: tier 1",
            "crafting: attuned to heroism 2",
            "crafting: attuned to heroism 2",
            "crafting: attuned to heroism 4"
        ]
    );
    assert_eq!(
        row_count(&db, "SELECT COUNT(*) FROM augment_slot_types WHERE label LIKE 'crafting: attuned to heroism 3'"),
        0
    );
    let (family, variant): (String, String) = db
        .query_row(
            "SELECT family, variant FROM augment_slot_types WHERE label = 'crafting: attuned by heroism: tier 1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((family.as_str(), variant.as_str()), ("crafting", "attuned by heroism: tier 1"));
    assert_eq!(report.augment_slot_type_count, row_count(&db, "SELECT COUNT(*) FROM augment_slot_types") as usize);
}

fn copied_directory(source_dir: &std::path::Path, copy_dir: &std::path::Path) {
    std::fs::create_dir_all(copy_dir).unwrap();
    for entry in std::fs::read_dir(source_dir).unwrap() {
        let entry = entry.unwrap();
        let copy_path = copy_dir.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copied_directory(&entry.path(), &copy_path);
        } else {
            std::fs::copy(entry.path(), copy_path).unwrap();
        }
    }
}

const UNTYPED_ITEMS: (&str, &str) = ("untyped_items", "Items");
const UNTYPED_AUGMENTS: (&str, &str) = ("untyped_augments", "Augments");
const UNTYPED_ITEM_AUGMENT_SLOT_OPTIONS: (&str, &str) = ("untyped_item_augment_slot_options", "Items");

fn data_files_with_untyped(test_name: &str, untyped_fixture_dirs: &[(&str, &str)]) -> PathBuf {
    let data_files_dir =
        std::env::temp_dir().join(format!("ddo-etl-untyped-{test_name}-{}", std::process::id())).join("DataFiles");
    let _ = std::fs::remove_dir_all(&data_files_dir);
    copied_directory(&fixtures_dir().join("DataFiles"), &data_files_dir);
    for (fixture_dir_name, data_files_subdir) in untyped_fixture_dirs {
        copied_directory(&fixtures_dir().join(fixture_dir_name), &data_files_dir.join(data_files_subdir));
    }
    data_files_dir
}

fn built_db_from(
    data_files_dir: &std::path::Path,
    correction_files: &[(&str, &str)],
) -> Result<(Connection, BuildReport), String> {
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        data_files_dir,
        &WikiOverrides::default(),
        &parsed_corrections(correction_files)?,
        &mut db,
        &fixture_dataset_version(),
    )
    .map_err(|e| format!("{e:#}"))?;
    Ok((db, report))
}

#[test]
fn a_set_tier_correction_shares_a_family_with_the_same_effect_on_another_set() {
    let data_files_dir = data_files_with_untyped("shared-set-family", &[]);
    let set_file = data_files_dir.join("SetBonuses.xml");
    let source = std::fs::read_to_string(&set_file).unwrap();
    let added_sets = r#"
  <SetBonus>
    <Type>Shaman's Fury</Type>
    <Buff><EquippedCount>2</EquippedCount><Description>+55 Equipment bonus to your Acid, Cold, Electric, and Fire Spell Power.</Description>
      <Effect><Type>SpellPower</Type><Bonus>Equipment</Bonus><AType>Simple</AType><Amount size="1">55</Amount>
        <Item>Acid</Item><Item>Cold</Item><Item>Electric</Item><Item>Fire</Item></Effect>
    </Buff>
  </SetBonus>
  <SetBonus>
    <Type>Epic Shaman's Fury</Type>
    <Buff><EquippedCount>2</EquippedCount><Description>+20 Artifact bonus Fire, Cold, Electric, and Acid Spell Power</Description>
      <Effect><Type>SpellPower</Type><Bonus>Artifact</Bonus><AType>Simple</AType><Amount size="1">20</Amount>
        <Item>Acid</Item><Item>Cold</Item><Item>Electric</Item><Item>Fire</Item></Effect>
    </Buff>
  </SetBonus>
"#;
    std::fs::write(&set_file, source.replace("</SetBonuses>", &format!("{added_sets}</SetBonuses>"))).unwrap();
    let mut corrections = String::new();
    for stat in ["Acid", "Cold", "Electric", "Fire"] {
        let qualifier = format!("equipped_count = 2\nstat = \"{stat} Spell Power\"\nbonus_type = \"Equipment\"");
        corrections.push_str(&qualified_correction_toml(
            "set_tier_bonus",
            "Shaman's Fury",
            &qualifier,
            "bonus_type",
            "\"Equipment\"",
            "\"Artifact\"",
        ));
        let qualifier = format!("equipped_count = 2\nstat = \"{stat} Spell Power\"\nbonus_type = \"Artifact\"");
        corrections.push_str(&qualified_correction_toml(
            "set_tier_bonus",
            "Shaman's Fury",
            &qualifier,
            "value",
            "55",
            "10",
        ));
    }
    let (db, report) = built_db_from(&data_files_dir, &[("corrections.toml", &corrections)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (8, 0));
    let mut first_family_id = None;
    for (set_name, expected_value) in [("Shaman's Fury", 10), ("Epic Shaman's Fury", 20)] {
        let rows = set_tier_bonus_rows(&db, set_name, 2);
        assert_eq!(rows.len(), 4, "{set_name}: {rows:?}");
        assert!(
            rows.iter().all(|(_, bonus_type, value)| bonus_type == "Artifact" && *value == expected_value),
            "{set_name}: {rows:?}"
        );
        let family: (i64, String, String, Option<i64>) = db
            .query_row(
                "SELECT e.id, e.name, e.text_template, te.value FROM set_bonus_tiers t JOIN set_bonuses s ON s.id = t.set_id
             JOIN set_bonus_tier_enchantments te ON te.tier_id = t.id JOIN enchantments e ON e.id = te.enchantment_id
             WHERE s.name = ?1 AND t.equipped_count = 2 AND EXISTS
               (SELECT 1 FROM enchantment_stats es JOIN stats stat ON stat.id = es.stat_id
                WHERE es.enchantment_id = e.id AND stat.name = 'Acid Spell Power')",
                [set_name],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(family.1, "Acid Spell Power", "{family:?}");
        assert_eq!(family.2, "%b1 Acid Spell Power +{1}", "{family:?}");
        assert_eq!(family.3, Some(expected_value));
        if let Some(first_id) = first_family_id {
            assert_eq!(family.0, first_id);
        } else {
            first_family_id = Some(family.0);
        }
    }
}

#[test]
fn types_an_item_bonus_his_files_leave_untyped_and_retypes_a_typed_one() {
    let data_files_dir = data_files_with_untyped("item-bonus-type", &[UNTYPED_ITEMS]);
    let (db, report) = built_db_from(
        &data_files_dir,
        &[(
            "corrections.toml",
            &(qualified_correction_toml(
                "item_bonus",
                "Embrace of the Spider Queen",
                "stat = \"Fortification\"\nbonus_type = \"null\"\nbonus_value = 100",
                "bonus_type",
                "\"null\"",
                "\"Enhancement\"",
            ) + &qualified_correction_toml(
                "item_bonus",
                "Embrace of the Spider Queen",
                "stat = \"Fortification\"\nbonus_type = \"null\"\nbonus_value = 10",
                "bonus_type",
                "\"null\"",
                "\"Insight\"",
            ) + &qualified_correction_toml(
                "item_bonus",
                "Docent of Defiance",
                "stat = \"Fire Resistance\"\nbonus_type = \"Enhancement\"",
                "bonus_type",
                "\"Enhancement\"",
                "\"Competence\"",
            ) + &qualified_correction_toml(
                "item_bonus",
                "Docent of Defiance",
                "stat = \"Acid Resistance\"\nbonus_type = \"null\"",
                "bonus_type",
                "\"null\"",
                "\"Enhancement\"",
            )),
        )],
    )
    .unwrap();
    assert_eq!(
        (report.correction_applied_count, report.correction_stale_count),
        (3, 1),
        "{:?}",
        report.stale_corrections
    );
    assert_eq!(
        item_bonus_rows(&db, "Embrace of the Spider Queen"),
        [
            ("Fortification".into(), Some("Enhancement".into()), Some(100)),
            ("Fortification".into(), Some("Insight".into()), Some(10)),
        ]
    );
    assert_eq!(
        item_bonus_rows(&db, "Docent of Defiance")[0],
        ("Fire Resistance".into(), Some("Competence".into()), Some(20))
    );
    let qualifiers: Vec<String> = db
        .prepare("SELECT qualifier FROM corrections ORDER BY qualifier")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        qualifiers,
        ["Fire Resistance / Enhancement", "Fortification / null / 10", "Fortification / null / 100"]
    );
    let error = parsed_corrections(&[(
        "corrections.toml",
        &qualified_correction_toml(
            "item_bonus",
            "Embrace of the Spider Queen",
            "stat = \"Fortification\"\nbonus_type = \"null\"",
            "bonus_type",
            "\"Enhancement\"",
            "\"Insight\"",
        ),
    )])
    .unwrap_err();
    assert!(error.contains("from must be the bonus_type"), "{error}");
}

#[test]
fn corrects_an_item_bonus_value_without_touching_the_shared_bonus() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &qualified_correction_toml(
            "item_bonus",
            "Docent of Defiance",
            "stat = \"Cold Resistance\"\nbonus_type = \"Enhancement\"",
            "value",
            "20",
            "25",
        ),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 1, "{:?}", report.stale_corrections);
    let docent_bonuses = item_bonus_rows(&db, "Docent of Defiance");
    assert!(
        docent_bonuses.contains(&("Cold Resistance".into(), Some("Enhancement".into()), Some(25))),
        "{docent_bonuses:?}"
    );
    assert!(
        docent_bonuses.contains(&("Fire Resistance".into(), Some("Enhancement".into()), Some(20))),
        "{docent_bonuses:?}"
    );
}

#[test]
fn refuses_to_write_an_item_bonus_without_a_type() {
    let data_files_dir = data_files_with_untyped("untyped-item", &[UNTYPED_ITEMS]);
    let error = built_db_from(&data_files_dir, &[]).unwrap_err();
    for expected_text in ["item \"Embrace of the Spider Queen\"", "buff \"Fortification\"", "stat \"Fortification\""] {
        assert!(error.contains(expected_text), "{expected_text} missing from {error}");
    }
}

#[test]
fn refuses_to_write_an_augment_bonus_without_a_type_unless_a_correction_types_it() {
    let data_files_dir = data_files_with_untyped("untyped-augment", &[UNTYPED_AUGMENTS]);
    let error = built_db_from(&data_files_dir, &[]).unwrap_err();
    for expected_text in ["augment \"Dolorous Invigorator (Heroic)\"", "effect \"TacticalDC\"", "stat \"Trip DC\""] {
        assert!(error.contains(expected_text), "{expected_text} missing from {error}");
    }
    let profane_corrections: String = ["Trip DC", "Sunder DC", "Stun DC", "Tactics", "Assassinate DC"]
        .iter()
        .map(|stat_name| {
            qualified_correction_toml(
                "augment_bonus",
                "Dolorous Invigorator (Heroic)",
                &format!("stat = {stat_name:?}\nbonus_type = \"null\""),
                "bonus_type",
                "\"null\"",
                "\"Profane\"",
            )
        })
        .collect();
    let (db, report) = built_db_from(&data_files_dir, &[("corrections.toml", &profane_corrections)]).unwrap();
    assert_eq!(
        (report.correction_applied_count, report.correction_stale_count),
        (5, 0),
        "{:?}",
        report.stale_corrections
    );
    let dolorous_bonuses = augment_bonus_rows(&db, "Dolorous Invigorator (Heroic)");
    for stat_name in ["Trip DC", "Sunder DC", "Stun DC", "Tactics", "Assassinate DC"] {
        assert!(
            dolorous_bonuses.contains(&(stat_name.into(), Some("Profane".into()), Some(1))),
            "{dolorous_bonuses:?}"
        );
    }
    assert!(dolorous_bonuses.contains(&("Spell DCs".into(), Some("Profane".into()), Some(1))), "{dolorous_bonuses:?}");
    assert_eq!(dolorous_bonuses.len(), 6);
    assert_eq!(row_count(&db, "SELECT COUNT(*) FROM corrections WHERE kind = 'augment_bonus'"), 5);
}

#[test]
fn removing_an_item_removes_its_augment_slot_options_and_their_modifiers() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &correction_toml("item", "+3 Combustion Scorched Battle Axe", "remove", "0", "1"),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 1, "{:?}", report.stale_corrections);
    assert_eq!(
        row_count(
            &db,
            "SELECT COUNT(*) FROM modifiers m WHERE m.source_kind = 'item_augment_slot_option'
              AND NOT EXISTS (SELECT 1 FROM item_augment_slot_options o WHERE o.id = m.source_id)"
        ),
        0
    );
}

#[test]
fn merging_a_socket_label_moves_the_sockets_options_grant() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &correction_toml("socket_label", "red", "name", "\"red\"", "\"purple\""),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 1, "{:?}", report.stale_corrections);
    let granted_labels: Vec<String> = db
        .prepare(
            "SELECT t.label FROM item_augment_slot_option_grants g JOIN augment_slot_types t ON t.id = g.slot_id
               JOIN item_augment_slot_options o ON o.id = g.option_id JOIN items i ON i.id = o.item_id
              WHERE i.name = 'Sireth, Spear of the Sky'",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(granted_labels, ["purple"]);
}

#[test]
fn refuses_an_untyped_augment_slot_option_bonus_unless_an_item_bonus_correction_types_it() {
    let data_files_dir = data_files_with_untyped("untyped-slot-option", &[UNTYPED_ITEM_AUGMENT_SLOT_OPTIONS]);
    let error = built_db_from(&data_files_dir, &[]).unwrap_err();
    for expected_text in [
        "augment slot option of item \"Epic Bracers of Wind\"",
        "effect \"SpellLore\"",
        "stat \"Electric Spell Lore\"",
        "item_bonus bonus_type correction",
    ] {
        assert!(error.contains(expected_text), "{expected_text} missing from {error}");
    }
    let equipment_corrections: String = [17, 18]
        .iter()
        .map(|bonus_value| {
            qualified_correction_toml(
                "item_bonus",
                "Epic Bracers of Wind",
                &format!("stat = \"Electric Spell Lore\"\nbonus_type = \"null\"\nbonus_value = {bonus_value}"),
                "bonus_type",
                "\"null\"",
                "\"Equipment\"",
            )
        })
        .collect();
    let (db, report) = built_db_from(&data_files_dir, &[("corrections.toml", &equipment_corrections)]).unwrap();
    assert_eq!(
        (report.correction_applied_count, report.correction_stale_count),
        (2, 0),
        "{:?}",
        report.stale_corrections
    );
    let lore_bonuses: Vec<(String, i64)> = db
        .prepare(
            "SELECT bt.name, CASE es.amount_from WHEN 0 THEN es.constant WHEN 1 THEN oe.value ELSE oe.value2 END
               FROM item_augment_slot_option_enchantments oe JOIN enchantment_stats es ON es.enchantment_id = oe.enchantment_id
               JOIN stats s ON s.id = es.stat_id JOIN bonus_types bt ON bt.id = COALESCE(es.bonus_type_id, oe.bonus_type_id)
               JOIN item_augment_slot_options o ON o.id = oe.option_id JOIN items i ON i.id = o.item_id
              WHERE i.name = 'Epic Bracers of Wind' AND s.name = 'Electric Spell Lore' ORDER BY 2",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(lore_bonuses, [("Equipment".to_string(), 17), ("Equipment".to_string(), 18)]);
}
