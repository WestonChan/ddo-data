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
        "SELECT s.name, bt.name, b.value FROM augments a JOIN augment_bonuses ab ON ab.augment_id = a.id
           JOIN bonuses b ON b.id = ab.bonus_id JOIN stats s ON s.id = b.stat_id LEFT JOIN bonus_types bt ON bt.id = b.bonus_type_id
          WHERE a.name = ?1 ORDER BY a.id, ab.sort_order",
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
            "SELECT items.name FROM quest_loot JOIN quests ON quests.id = quest_loot.quest_id
               JOIN items ON items.id = quest_loot.item_id
              WHERE quests.name = 'Temple of Elemental Evil Part One'",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(linked_items, ["+3 Combustion Scorched Battle Axe"]);
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
    for child_table in
        ["item_bonuses", "item_effects", "item_augment_slots", "quest_loot", "set_bonus_items", "item_clickies"]
    {
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
        "SELECT COUNT(*) FROM augment_bonuses WHERE augment_id NOT IN (SELECT id FROM augments)",
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
    assert_eq!(report.stale_corrections[0].maetrim_value, "56");
    assert_eq!(
        row_count(&db, "SELECT COUNT(*) FROM bonuses WHERE name = 'Healing Amplification +56'"),
        1,
        "the old bonus row stays for anything else that carries it"
    );
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
