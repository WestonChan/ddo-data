use ddo_etl::build::{build_database, BuildReport, StaleCorrection};
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
    for (kind, field) in [("augment", "name"), ("augment", "id"), ("item", "icon"), ("feat", "icon"), ("quest", "name")]
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
            expected_value: "318".into(),
            maetrim_value: "30".into(),
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
