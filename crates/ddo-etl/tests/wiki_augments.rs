use ddo_etl::build::{build_database, BuildReport, ProbableDuplicateWikiEntry, SupersededWikiEntry};
use ddo_etl::corrections::Corrections;
use ddo_etl::wiki::WikiOverrides;
use ddo_model::DatasetVersion;
use rusqlite::Connection;
use std::path::PathBuf;

const WIKI_GEM: &str = "Test Gem of Oozing Resistance";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn augments_fixture_toml() -> String {
    std::fs::read_to_string(fixtures_dir().join("wiki/augments.toml")).unwrap()
}

fn parsed_wiki(files: &[(&str, &str)]) -> Result<WikiOverrides, String> {
    WikiOverrides::from_toml_files(files).map_err(|e| format!("{e:#}"))
}

fn parsed_edited_augments(edit: impl Fn(String) -> String) -> Result<WikiOverrides, String> {
    parsed_wiki(&[("augments.toml", &edit(augments_fixture_toml()))])
}

fn built_db_with(wiki: &WikiOverrides) -> Result<(Connection, BuildReport), String> {
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        &fixtures_dir().join("DataFiles"),
        wiki,
        &Corrections::default(),
        &mut db,
        &DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-09-20T00:00:00Z".into() },
    )
    .map_err(|e| format!("{e:#}"))?;
    Ok((db, report))
}

fn built_db_with_fixture_augments() -> (Connection, BuildReport) {
    built_db_with(&parsed_edited_augments(|s| s).unwrap()).unwrap()
}

fn string_column(db: &Connection, sql: &str) -> Vec<String> {
    let mut statement = db.prepare(sql).unwrap();
    statement.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

fn augment_count(db: &Connection, sql_condition: &str) -> i64 {
    db.query_row(&format!("SELECT COUNT(*) FROM augments WHERE {sql_condition}"), [], |r| r.get(0)).unwrap()
}

#[test]
fn reads_wiki_augments_from_augments_files() {
    let wiki = WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap();
    assert_eq!(wiki.augments.len(), 2);
    let gem = &wiki.augments[0];
    assert_eq!(
        (gem.name.as_str(), gem.family.as_str(), gem.min_level, gem.set.as_deref()),
        (WIKI_GEM, "Named", 29, Some("Eminence of Winter"))
    );
    assert_eq!(gem.slots, ["green", "colorless"]);
    assert_eq!(gem.bonuses.len(), 2);
    assert_eq!(gem.file_name, "augments.toml");
    assert_eq!(wiki.augments[1].effect_description, None);
}

#[test]
fn rejects_a_wiki_augment_value_outside_the_vocabularies_naming_the_augment_and_field() {
    for (field, good, bad) in [
        ("stat", "stat = \"Strength\"", "stat = \"Strenght\""),
        ("bonus_type", "bonus_type = \"Insight\"", "bonus_type = \"Insightful\""),
        ("slots", "slots = [\"green\", \"colorless\"]", "slots = []"),
    ] {
        let error = parsed_edited_augments(|s| s.replacen(good, bad, 1)).unwrap_err();
        assert!(error.contains(WIKI_GEM) && error.contains(field), "{field}: {error}");
    }
}

#[test]
fn rejects_unknown_and_missing_wiki_augment_fields() {
    let error =
        parsed_edited_augments(|s| s.replacen("min_level = 29", "min_level = 29\nrarity = \"Rare\"", 1)).unwrap_err();
    assert!(error.contains(WIKI_GEM) && error.contains("rarity"), "{error}");
    let error = parsed_edited_augments(|s| s.replacen("family = \"Named\"\n", "", 1)).unwrap_err();
    assert!(error.contains(WIKI_GEM) && error.contains("family"), "{error}");
}

#[test]
fn rejects_a_wiki_augment_listed_twice_in_one_family() {
    let gem_entry = augments_fixture_toml().split("\n\n").next().unwrap().to_string();
    let error =
        parsed_wiki(&[("augments.toml", &augments_fixture_toml()), ("augments_more.toml", &gem_entry)]).unwrap_err();
    assert!(error.contains(WIKI_GEM) && error.contains("augments.toml"), "{error}");
    let other_family_entry = gem_entry.replace("family = \"Named\"", "family = \"Ruby\"");
    assert!(parsed_wiki(&[("augments.toml", &augments_fixture_toml()), ("augments_more.toml", &other_family_entry)])
        .is_ok());
}

#[test]
fn writes_a_new_wiki_augment_with_its_slots_bonuses_and_set() {
    let (db, report) = built_db_with_fixture_augments();
    let augment_row: (String, Option<String>, Option<String>, i64, Option<String>, String) = db
        .query_row(
            "SELECT family, description, effect_description, min_level, set_bonus, provenance FROM augments WHERE name = ?1",
            [WIKI_GEM],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .unwrap();
    assert_eq!(
        augment_row,
        (
            "Named".into(),
            Some("Test description: a gem that drips.".into()),
            Some("Test effect description: the wearer resists ooze.".into()),
            29,
            Some("Eminence of Winter".into()),
            "wiki".into()
        )
    );
    let augment_id: i64 = db.query_row("SELECT id FROM augments WHERE name = ?1", [WIKI_GEM], |r| r.get(0)).unwrap();
    let augment_column = |sql: &str| string_column(&db, &sql.replace("?augment", &augment_id.to_string()));
    assert_eq!(
        augment_column(
            "SELECT t.label FROM augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id
              WHERE s.augment_id = ?augment ORDER BY t.label"
        ),
        ["colorless", "green"]
    );
    assert_eq!(
        augment_column(
            "SELECT s.name || '|' || bt.name || '|' || b.value FROM augment_bonuses ab JOIN bonuses b ON b.id = ab.bonus_id
               JOIN stats s ON s.id = b.stat_id JOIN bonus_types bt ON bt.id = b.bonus_type_id
              WHERE ab.augment_id = ?augment ORDER BY ab.sort_order"
        ),
        ["Strength|Insight|3", "Doublestrike|Enhancement|5"]
    );
    assert_eq!(
        augment_column(
            "SELECT s.name FROM set_bonus_augments sba JOIN set_bonuses s ON s.id = sba.set_id WHERE sba.augment_id = ?augment"
        ),
        ["Eminence of Winter"]
    );
    assert_eq!(report.wiki_augment_written_count, 1);
    assert_eq!(augment_count(&db, "provenance = 'wiki'"), 1);
    assert_eq!(report.wiki_augment_probable_duplicate_count, 0);
}

#[test]
fn drops_a_wiki_augment_maetrim_carries_in_that_family_and_reports_it() {
    let (without, _) = built_db_with(&WikiOverrides::default()).unwrap();
    let (with, report) = built_db_with_fixture_augments();
    assert_eq!(augment_count(&with, "name = 'Storm''s Bulwark'"), 1);
    assert_eq!(augment_count(&with, "name = 'Storm''s Bulwark' AND provenance = 'maetrim'"), 1);
    let description_sql = "SELECT description FROM augments WHERE name = 'Storm''s Bulwark'";
    assert_eq!(string_column(&with, description_sql), string_column(&without, description_sql));
    assert_eq!(report.wiki_augment_superseded_count, 1);
    assert_eq!(
        report.superseded_wiki_augments,
        [SupersededWikiEntry { name: "Storm's Bulwark".into(), file_name: "augments.toml".into() }]
    );
}

#[test]
fn writes_a_wiki_augment_whose_name_maetrim_uses_only_in_another_family() {
    let wiki = parsed_edited_augments(|s| {
        s.replace("family = \"Named\"\nmin_level = 1\n", "family = \"Ruby\"\nmin_level = 1\n")
    })
    .unwrap();
    let (db, report) = built_db_with(&wiki).unwrap();
    assert_eq!(augment_count(&db, "name = 'Storm''s Bulwark' AND family = 'Ruby' AND provenance = 'wiki'"), 1);
    assert_eq!((report.wiki_augment_written_count, report.wiki_augment_superseded_count), (2, 0));
}

#[test]
fn writes_a_probable_duplicate_of_a_maetrim_augment_and_reports_both_names() {
    let wiki =
        parsed_edited_augments(|s| s.replace("name = \"Storm's Bulwark\"", "name = \"Storms Bulwark\"")).unwrap();
    let (db, report) = built_db_with(&wiki).unwrap();
    assert_eq!(augment_count(&db, "name = 'Storms Bulwark' AND provenance = 'wiki'"), 1);
    assert_eq!((report.wiki_augment_written_count, report.wiki_augment_probable_duplicate_count), (2, 1));
    assert_eq!(
        report.probable_duplicate_wiki_augments,
        [ProbableDuplicateWikiEntry { name: "Storms Bulwark".into(), maetrim_name: "Storm's Bulwark".into() }]
    );
}

#[test]
fn build_fails_naming_a_wiki_augment_value_absent_from_maetrims_files() {
    for (field, good, bad) in [
        ("family", "family = \"Named\"", "family = \"Unnamed\""),
        ("slots", "slots = [\"green\", \"colorless\"]", "slots = [\"green\", \"colourless\"]"),
        ("set", "set = \"Eminence of Winter\"", "set = \"Eminence of Summer\""),
    ] {
        let wiki = parsed_edited_augments(|s| s.replacen(good, bad, 1)).unwrap();
        let error = built_db_with(&wiki).unwrap_err();
        let bad_value = if field == "slots" { "colourless" } else { bad.split('"').nth(1).unwrap() };
        assert!(
            error.contains(WIKI_GEM)
                && error.contains("augments.toml")
                && error.contains(field)
                && error.contains(bad_value),
            "{field}: {error}"
        );
    }
}

#[test]
fn wiki_augments_never_change_maetrims_augments() {
    let (without, _) = built_db_with(&WikiOverrides::default()).unwrap();
    let (with, _) = built_db_with_fixture_augments();
    assert_eq!(augment_count(&with, "provenance = 'maetrim'"), augment_count(&without, "1"));
    assert_eq!(augment_count(&without, "provenance = 'wiki'"), 0);
}

#[test]
fn links_a_wiki_augment_to_the_quests_its_drop_text_names() {
    let wiki = parsed_edited_augments(|s| {
        s.replace(
            "Test description: a gem that drips.",
            "Test description: a gem that drips.\\nDrops in: The Grotto, end chest",
        )
    })
    .unwrap();
    let (db, _) = built_db_with(&wiki).unwrap();
    assert_eq!(
        string_column(
            &db,
            "SELECT q.name || '|' || qal.loot_type || '|' || qal.chest FROM sources qal
               JOIN quests q ON q.id = qal.quest_id JOIN augments a ON a.id = qal.augment_id
              WHERE a.name = 'Test Gem of Oozing Resistance'"
        ),
        ["The Grotto|chest|end chest"]
    );
}

#[test]
fn a_crafting_recipe_may_yield_a_wiki_augment() {
    let crafting_toml = "[[system]]\nname = \"Test Named Forge\"\npage = \"https://ddowiki.com/page/Test_Named_Forge\"\n\
        read = \"2026-10-01\"\nfamilies = [\"Named\"]\n\n[[system.ingredient]]\nname = \"Test Token\"\ntier = \"any\"\n\n\
        [[system.recipe]]\ntier = \"legendary\"\noption = \"Test Gem\"\naugments = [\"Test Gem of Oozing Resistance\"]\n\
        cost = [{ ingredient = \"Test Token\", quantity = 1 }]\n";
    let wiki = parsed_wiki(&[("augments.toml", &augments_fixture_toml()), ("crafting.toml", crafting_toml)]).unwrap();
    let (db, _) = built_db_with(&wiki).unwrap();
    assert_eq!(
        string_column(
            &db,
            "SELECT r.option || '|' || a.provenance FROM crafting_recipe_augments cra
               JOIN crafting_recipes r ON r.id = cra.recipe_id JOIN augments a ON a.id = cra.augment_id
              WHERE a.name = 'Test Gem of Oozing Resistance'"
        ),
        ["Test Gem|wiki"]
    );
}

#[test]
fn a_rare_augment_drop_may_name_a_wiki_augment() {
    let quest_loot_toml = "[[quest]]\nname = \"The Grotto\"\npage = \"https://ddowiki.com/page/The_Grotto\"\n\
        read = \"2026-10-01\"\nrare_augments = [\"Test Gem of Oozing Resistance\"]\n";
    let wiki =
        parsed_wiki(&[("augments.toml", &augments_fixture_toml()), ("quest_loot.toml", quest_loot_toml)]).unwrap();
    let (db, _) = built_db_with(&wiki).unwrap();
    assert_eq!(
        string_column(
            &db,
            "SELECT q.name || '|' || qal.is_rare FROM sources qal
               JOIN quests q ON q.id = qal.quest_id JOIN augments a ON a.id = qal.augment_id
              WHERE a.name = 'Test Gem of Oozing Resistance'"
        ),
        ["The Grotto|1"]
    );
}
