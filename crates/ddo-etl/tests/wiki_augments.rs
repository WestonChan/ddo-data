mod fixture;

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

const GEM_FILE: &str = "packs/shared/augments/named/test-gem-of-oozing-resistance.toml";
const STORM_FILE: &str = "packs/shared/augments/named/storms-bulwark.toml";

fn edited_augments(edit: impl FnOnce(&std::path::Path)) -> WikiOverrides {
    let ours = fixture::database(|root| {
        std::fs::write(
            root.join(STORM_FILE),
            r#"
name = "Storm's Bulwark"
is_wiki_only = true
family = "Named"
min_level = 1
description = "Test description: a wiki copy of an augment Maetrim already carries."
[[slot]]
slot = "green"
"#,
        )
        .unwrap();
        edit(root);
    });
    let wiki = WikiOverrides::from_database(&ours).unwrap();
    WikiOverrides { augments: wiki.augments, ..WikiOverrides::default() }
}

fn replace_file_text(root: &std::path::Path, path: &str, before: &str, after: &str) {
    let path = root.join(path);
    let contents = std::fs::read_to_string(&path).unwrap();
    assert!(contents.contains(before), "{path:?}: missing {before:?}");
    std::fs::write(path, contents.replacen(before, after, 1)).unwrap();
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
    built_db_with(&edited_augments(|_| {})).unwrap()
}

fn string_column(db: &Connection, sql: &str) -> Vec<String> {
    let mut statement = db.prepare(sql).unwrap();
    statement.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

fn augment_count(db: &Connection, sql_condition: &str) -> i64 {
    db.query_row(&format!("SELECT COUNT(*) FROM augments WHERE {sql_condition}"), [], |r| r.get(0)).unwrap()
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
            "SELECT s.name || '|' || bt.name || '|' || ob.amount
               FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
               JOIN bonus_types bt ON bt.id = ob.bonus_type_id
              WHERE ob.owner_kind = 'augment' AND ob.owner_id = ?augment ORDER BY ob.effect_link_order, ob.stat_id"
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
        [SupersededWikiEntry { name: "Storm's Bulwark".into(), file_name: STORM_FILE.into() }]
    );
}

fn assert_colliding_augment_variants_export(db: &Connection, scenario: &str) {
    let output = std::env::temp_dir().join(format!("ddo-wiki-augment-variant-{}-{scenario}", std::process::id()));
    ddo_etl::tree::export_database(db, &output).unwrap();
    let mut select = db
        .prepare(
            "SELECT name, slug, variant FROM augments WHERE name IN ('Storm''s Bulwark', 'Storms Bulwark') ORDER BY id",
        )
        .unwrap();
    let rows = select
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(rows.len(), 2);
    for (name, slug, variant) in rows {
        assert!(!variant.is_empty());
        assert_eq!(slug, format!("{}-{}", ddo_etl::tree::slug(&name), ddo_etl::tree::slug(&variant)));
        let path = walkdir::WalkDir::new(&output)
            .into_iter()
            .map(Result::unwrap)
            .find(|entry| entry.path().file_stem().is_some_and(|stem| stem == slug.as_str()))
            .unwrap()
            .into_path();
        let file: toml::Value = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(file["variant"].as_str(), Some(variant.as_str()));
    }
    std::fs::remove_dir_all(output).unwrap();
}

#[test]
fn writes_a_wiki_augment_whose_name_maetrim_uses_only_in_another_family() {
    let wiki = edited_augments(|root| {
        replace_file_text(root, STORM_FILE, "family = \"Named\"", "family = \"Ruby\"");
    });
    let (db, report) = built_db_with(&wiki).unwrap();
    assert_eq!(augment_count(&db, "name = 'Storm''s Bulwark' AND family = 'Ruby' AND provenance = 'wiki'"), 1);
    assert_eq!((report.wiki_augment_written_count, report.wiki_augment_superseded_count), (2, 0));
    assert_colliding_augment_variants_export(&db, "family");
}

#[test]
fn writes_a_probable_duplicate_of_a_maetrim_augment_and_reports_both_names() {
    let wiki = edited_augments(|root| {
        replace_file_text(root, STORM_FILE, "Storm's Bulwark", "Storms Bulwark");
    });
    let (db, report) = built_db_with(&wiki).unwrap();
    assert_eq!(augment_count(&db, "name = 'Storms Bulwark' AND provenance = 'wiki'"), 1);
    assert_eq!((report.wiki_augment_written_count, report.wiki_augment_probable_duplicate_count), (2, 1));
    assert_colliding_augment_variants_export(&db, "name");
    assert_eq!(
        report.probable_duplicate_wiki_augments,
        [ProbableDuplicateWikiEntry { name: "Storms Bulwark".into(), maetrim_name: "Storm's Bulwark".into() }]
    );
}

#[test]
fn build_fails_naming_a_wiki_augment_value_absent_from_maetrims_files() {
    for (field, before, after, missing) in [
        ("family", "family = \"Named\"", "family = \"Unnamed\"", "Unnamed"),
        ("slots", "slot = \"colorless\"", "slot = \"colourless\"", "colourless"),
        ("set", "set_bonus = \"Eminence of Winter\"", "set_bonus = \"Eminence of Summer\"", "Eminence of Summer"),
    ] {
        let wiki = edited_augments(|root| {
            if field == "family" {
                replace_file_text(
                    root,
                    "packs/shared/sets/eminence-of-winter.toml",
                    "family = \"Named\"",
                    "family = \"Unnamed\"",
                );
            }
            if field == "slots" {
                let path = root.join("effects/augment-slot-types.toml");
                let mut contents = std::fs::read_to_string(&path).unwrap();
                contents
                    .push_str("\n[[row]]\nfamily = \"standard\"\nlabel = \"colourless\"\nvariant = \"colorless\"\n");
                std::fs::write(path, contents).unwrap();
            }
            if field == "set" {
                let path = root.join("packs/shared/sets/eminence-of-summer.toml");
                std::fs::write(path, "name = \"Eminence of Summer\"\n[[augment]]\nname = \"Test Gem of Oozing Resistance\"\nfamily = \"Named\"\n").unwrap();
                replace_file_text(
                    root,
                    "packs/shared/sets/eminence-of-winter.toml",
                    "[[augment]]\nname = \"Test Gem of Oozing Resistance\"\nfamily = \"Named\"\n",
                    "",
                );
            }
            replace_file_text(root, GEM_FILE, before, after);
            if field == "family" {
                let destination = root.join("packs/shared/augments/unnamed/test-gem-of-oozing-resistance.toml");
                std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
                std::fs::rename(root.join(GEM_FILE), destination).unwrap();
            }
        });
        let owner_file = &wiki.augments.iter().find(|augment| augment.name == WIKI_GEM).unwrap().file_name;
        let error = built_db_with(&wiki).unwrap_err();
        assert!(
            error.contains(WIKI_GEM) && error.contains(owner_file) && error.contains(field) && error.contains(missing),
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
    let wiki = edited_augments(|root| {
        replace_file_text(
            root,
            GEM_FILE,
            "Test description: a gem that drips.",
            "Test description: a gem that drips.\\nDrops in: The Grotto, end chest",
        );
    });
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
    let ours = fixture::database(|root| {
        let path = root.join("packs/shared/crafting/test-named-forge/crafting.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            r#"
name = "Test Named Forge"
[[citation]]
url = "https://ddowiki.com/page/Test_Named_Forge"
read = "2026-10-01"
[[family]]
family = "Named"
[[ingredient]]
name = "Test Token"
tier = "any"
[[recipe]]
tier = "legendary"
option = "Test Gem"
[[recipe.augment]]
name = "Test Gem of Oozing Resistance"
level = 29
[[recipe.ingredient]]
ingredient = "Test Token"
quantity = 1
"#,
        )
        .unwrap();
    });
    let wiki = WikiOverrides::from_database(&ours).unwrap();
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
    let ours = fixture::database(|root| {
        let path = root.join(GEM_FILE);
        let mut contents = std::fs::read_to_string(&path).unwrap();
        contents
            .push_str("\n[[source]]\nkind = \"quest\"\nname = \"The Grotto\"\nloot_type = \"chest\"\nis_rare = true\n");
        std::fs::write(path, contents).unwrap();
    });
    let wiki = WikiOverrides::from_database(&ours).unwrap();
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
