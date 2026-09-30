use ddo_etl::build::{build_database, ProbableDuplicateWikiItem, SupersededWikiItem};
use ddo_etl::wiki::{DescriptionKind, WikiOverrides};
use ddo_model::DatasetVersion;
use rusqlite::Connection;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture_dataset_version() -> DatasetVersion {
    DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-09-20T00:00:00Z".into() }
}

fn quest_loot_toml(name: &str, rare_items: &[&str]) -> String {
    let quoted_rare_items: Vec<String> = rare_items.iter().map(|r| format!("{r:?}")).collect();
    format!(
        "[[quest]]\nname = {name:?}\npage = \"https://ddowiki.com/page/{}\"\nread = \"2026-09-27\"\nrare = [{}]\n",
        name.replace(' ', "_"),
        quoted_rare_items.join(", ")
    )
}

fn parsed_wiki(files: &[(&str, &str)]) -> Result<WikiOverrides, String> {
    WikiOverrides::from_toml_files(files).map_err(|e| format!("{e:#}"))
}

fn build_report_with(wiki: &WikiOverrides) -> Result<ddo_etl::build::BuildReport, String> {
    let mut db = Connection::open_in_memory().unwrap();
    build_database(&fixtures_dir().join("DataFiles"), wiki, &mut db, &fixture_dataset_version())
        .map_err(|e| format!("{e:#}"))
}

#[test]
fn reads_quest_loot_from_every_toml_file_in_the_directory() {
    let wiki = WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap();
    assert_eq!(wiki.quest_loot.len(), 1);
    let entry = &wiki.quest_loot[0];
    assert_eq!(entry.name, "Book Burning");
    assert_eq!(entry.page, "https://ddowiki.com/page/Book_Burning");
    assert_eq!(entry.read, "2026-09-27");
    assert_eq!(entry.rare, vec!["Buckler of the Golden Age".to_string()]);
}

#[test]
fn embedded_wiki_files_load() {
    let wiki = WikiOverrides::embedded().unwrap();
    assert!(wiki.quest_loot.iter().any(|q| q.name == "Book Burning"));
}

#[test]
fn rejects_a_page_outside_ddowiki() {
    let toml_text = quest_loot_toml("The Grotto", &[]).replace("https://ddowiki.com/page/", "https://example.com/");
    let error = parsed_wiki(&[("quest_loot_a.toml", &toml_text)]).unwrap_err();
    assert!(error.contains("The Grotto") && error.contains("https://ddowiki.com/page/"), "{error}");
}

#[test]
fn rejects_a_read_date_that_is_not_iso() {
    for bad in ["27/09/2026", "2026-9-27", "2026-13-01", "2026-02-30", "yesterday"] {
        let toml_text = quest_loot_toml("The Grotto", &[]).replace("2026-09-27", bad);
        let error = parsed_wiki(&[("quest_loot_a.toml", &toml_text)]).unwrap_err();
        assert!(error.contains("The Grotto") && error.contains(bad), "{bad}: {error}");
    }
}

#[test]
fn rejects_a_quest_listed_twice_across_files() {
    let error = parsed_wiki(&[
        ("quest_loot_a.toml", &quest_loot_toml("The Grotto", &[])),
        ("quest_loot_b.toml", &quest_loot_toml("The Grotto", &[])),
    ])
    .unwrap_err();
    assert!(
        error.contains("The Grotto") && error.contains("quest_loot_a.toml") && error.contains("quest_loot_b.toml"),
        "{error}"
    );
}

#[test]
fn rejects_unknown_fields() {
    let toml_text = quest_loot_toml("The Grotto", &[]) + "common = [\"Docent of Defiance\"]\n";
    let error = parsed_wiki(&[("quest_loot_a.toml", &toml_text)]).unwrap_err();
    assert!(error.contains("quest_loot_a.toml") && error.contains("common"), "{error}");
}

#[test]
fn build_fails_naming_a_quest_absent_from_the_quests_table() {
    let wiki = parsed_wiki(&[("quest_loot_a.toml", &quest_loot_toml("The Missing Quest", &[]))]).unwrap();
    let error = build_report_with(&wiki).unwrap_err();
    assert!(error.contains("The Missing Quest"), "{error}");
}

#[test]
fn build_fails_naming_an_item_absent_from_the_items_table() {
    let wiki = parsed_wiki(&[("quest_loot_a.toml", &quest_loot_toml("Book Burning", &["Buckler of the Missing Age"]))])
        .unwrap();
    let error = build_report_with(&wiki).unwrap_err();
    assert!(error.contains("Buckler of the Missing Age") && error.contains("Book Burning"), "{error}");
}

#[test]
fn build_reports_the_wiki_entries_it_applied() {
    let wiki = WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap();
    let report = build_report_with(&wiki).unwrap();
    assert_eq!(report.wiki_quest_loot_entry_count, 1);
    assert_eq!(report.wiki_rare_drop_count, 1);
}

fn built_db_with(wiki: &WikiOverrides) -> (Connection, ddo_etl::build::BuildReport) {
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(&fixtures_dir().join("DataFiles"), wiki, &mut db, &fixture_dataset_version()).unwrap();
    (db, report)
}

fn quest_loot_row(db: &Connection, quest: &str, item: &str) -> Option<(String, bool)> {
    db.query_row(
        "SELECT ql.loot_type, ql.is_rare FROM quest_loot ql JOIN quests q ON q.id = ql.quest_id JOIN items i ON i.id = ql.item_id
          WHERE q.name = ?1 AND i.name = ?2",
        [quest, item],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .ok()
}

#[test]
fn marks_rare_drops_on_the_links_maetrims_drop_text_made() {
    let (db, report) = built_db_with(&WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap());
    assert_eq!(quest_loot_row(&db, "Book Burning", "Buckler of the Golden Age"), Some(("chest".into(), true)));
    assert_eq!(quest_loot_row(&db, "The Cursed Crypt", "Docent of Defiance"), Some(("chest".into(), false)));
    assert_eq!(report.wiki_added_quest_loot_link_count, 0, "Maetrim's drop text already links the buckler");
}

#[test]
fn adds_a_chest_link_for_a_rare_drop_and_never_changes_maetrims_loot_type() {
    let wiki = parsed_wiki(&[
        ("quest_loot_a.toml", &quest_loot_toml("The Grotto", &["Docent of Defiance"])),
        ("quest_loot_b.toml", &quest_loot_toml("Caught in the Web", &["Sireth, Spear of the Sky"])),
    ])
    .unwrap();
    let (db, report) = built_db_with(&wiki);
    assert_eq!(quest_loot_row(&db, "The Grotto", "Docent of Defiance"), Some(("chest".into(), true)));
    assert_eq!(quest_loot_row(&db, "The Cursed Crypt", "Docent of Defiance"), Some(("chest".into(), false)));
    assert_eq!(quest_loot_row(&db, "Caught in the Web", "Sireth, Spear of the Sky"), Some(("raid".into(), true)));
    assert_eq!(report.wiki_added_quest_loot_link_count, 1);
    assert_eq!(report.wiki_rare_drop_count, 2);
}

fn quest_facts_toml(name: &str, extra_lines: &str) -> String {
    format!(
        "[[quest]]\nname = {name:?}\npage = \"https://ddowiki.com/page/Quests_by_level_and_XP\"\nread = \"2026-09-28\"\nfree_to_play = false\n{extra_lines}"
    )
}

#[test]
fn reads_quest_facts_from_quests_files() {
    let wiki = WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap();
    assert_eq!(wiki.quest_facts.len(), 3);
    let chronoscope = &wiki.quest_facts[0];
    assert_eq!(chronoscope.name, "The Chronoscope");
    assert!(!chronoscope.free_to_play);
    assert_eq!(chronoscope.legendary_level, Some(34));
    assert_eq!(chronoscope.zone.as_deref(), Some("The Harbor"));
    assert_eq!(chronoscope.bestowed_by.as_deref(), Some("A harbor quest giver"));
    assert_eq!(chronoscope.flagging.as_deref(), Some("None; open to all."));
    assert_eq!(wiki.quest_loot.len(), 1, "quests.toml tables are not read as quest loot");
}

#[test]
fn embedded_quests_file_loads() {
    let wiki = WikiOverrides::embedded().unwrap();
    assert!(wiki.quest_facts.iter().any(|q| q.name == "A Blood Pact" && q.legendary_level == Some(37)));
}

#[test]
fn rejects_a_file_name_that_names_no_wiki_file_type() {
    let error = parsed_wiki(&[("loot.toml", &quest_loot_toml("The Grotto", &[]))]).unwrap_err();
    assert!(
        error.contains("loot.toml")
            && error.contains("quest_loot")
            && error.contains("quests")
            && error.contains("crafting")
            && error.contains("items"),
        "{error}"
    );
}

#[test]
fn rejects_quest_duration_and_xp_as_unknown_fields() {
    for removed_field_line in ["duration = \"Long\"\n", "xp.epic = { normal = 100 }\n"] {
        let error = parsed_wiki(&[("quests.toml", &quest_facts_toml("The Grotto", removed_field_line))]).unwrap_err();
        assert!(error.contains("quests.toml") && error.contains("unknown field"), "{error}");
    }
}

#[test]
fn rejects_quest_facts_without_free_to_play() {
    let toml_text = quest_facts_toml("The Grotto", "").replace("free_to_play = false\n", "");
    let error = parsed_wiki(&[("quests.toml", &toml_text)]).unwrap_err();
    assert!(error.contains("quests.toml") && error.contains("free_to_play"), "{error}");
}

#[test]
fn rejects_quest_facts_citing_a_page_outside_ddowiki() {
    let toml_text = quest_facts_toml("The Grotto", "").replace("https://ddowiki.com/page/", "https://example.com/");
    let error = parsed_wiki(&[("quests.toml", &toml_text)]).unwrap_err();
    assert!(error.contains("The Grotto") && error.contains("https://ddowiki.com/page/"), "{error}");
}

#[test]
fn rejects_quest_facts_listed_twice_across_quests_files() {
    let error = parsed_wiki(&[
        ("quests.toml", &quest_facts_toml("The Grotto", "")),
        ("quests_epic.toml", &quest_facts_toml("The Grotto", "")),
    ])
    .unwrap_err();
    assert!(
        error.contains("The Grotto") && error.contains("quests.toml") && error.contains("quests_epic.toml"),
        "{error}"
    );
}

#[test]
fn a_quest_may_carry_both_loot_and_facts() {
    let wiki = parsed_wiki(&[
        ("quest_loot.toml", &quest_loot_toml("The Grotto", &[])),
        ("quests.toml", &quest_facts_toml("The Grotto", "")),
    ]);
    assert!(wiki.is_ok(), "{wiki:?}");
}

#[test]
fn build_fails_naming_quest_facts_for_a_quest_absent_from_the_quests_table() {
    let wiki = parsed_wiki(&[("quests.toml", &quest_facts_toml("The Missing Quest", ""))]).unwrap();
    let error = build_report_with(&wiki).unwrap_err();
    assert!(error.contains("The Missing Quest") && error.contains("quests"), "{error}");
}

type WikiQuestColumns = (bool, Option<i64>, Option<String>, Option<String>, Option<String>);

fn wiki_columns(db: &Connection, quest: &str) -> WikiQuestColumns {
    db.query_row(
        "SELECT is_free_to_play, legendary_level, zone, bestowed_by, flagging FROM quests WHERE name = ?1",
        [quest],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )
    .unwrap()
}

type MaetrimQuestColumns = (Option<i64>, Option<i64>, Option<i64>, Option<i64>, Option<i64>, bool, String);

fn maetrim_columns(db: &Connection, quest: &str) -> MaetrimQuestColumns {
    db.query_row(
        "SELECT level, epic_level, pack_id, patron_id, favor, is_raid, difficulties FROM quests WHERE name = ?1",
        [quest],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
    )
    .unwrap()
}

#[test]
fn fills_the_wiki_only_quest_columns() {
    let (db, report) = built_db_with(&WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap());
    assert_eq!(
        wiki_columns(&db, "The Chronoscope"),
        (
            false,
            Some(34),
            Some("The Harbor".into()),
            Some("A harbor quest giver".into()),
            Some("None; open to all.".into())
        )
    );
    assert_eq!(wiki_columns(&db, "The Grotto"), (true, None, None, None, None));
    assert_eq!(wiki_columns(&db, "Caught in the Web"), (false, None, None, None, None));
    assert_eq!(report.wiki_quest_entry_count, 3);
}

#[test]
fn quest_facts_never_change_maetrims_quest_columns() {
    let (without, _) = built_db_with(&WikiOverrides::default());
    let (with, _) = built_db_with(&WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap());
    for quest in ["The Chronoscope", "The Grotto", "Book Burning"] {
        assert_eq!(maetrim_columns(&with, quest), maetrim_columns(&without, quest), "{quest}");
    }
}

fn crafting_fixture_toml() -> String {
    std::fs::read_to_string(fixtures_dir().join("wiki/crafting.toml")).unwrap()
}

fn parsed_edited_crafting(edit: impl Fn(String) -> String) -> Result<WikiOverrides, String> {
    parsed_wiki(&[("crafting.toml", &edit(crafting_fixture_toml()))])
}

#[test]
fn reads_crafting_systems_from_crafting_files() {
    let wiki = WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap();
    assert_eq!(wiki.crafting_systems.len(), 2);
    let system = &wiki.crafting_systems[0];
    assert_eq!(system.name, "Heroic Green Steel");
    assert_eq!(system.page, "https://ddowiki.com/page/Green_Steel_items");
    assert_eq!(system.families, vec!["Greensteel_Heroic".to_string()]);
    assert_eq!(system.npc.as_deref(), Some("Altar of Invasion"));
    assert!(system.pack.is_none());
    assert_eq!(system.ingredients.len(), 4);
    assert_eq!(system.ingredients[0].bind.as_deref(), Some("Bound to Account"));
    assert_eq!(system.recipes.len(), 3);
    assert_eq!(system.recipes[0].cost[1].ingredient, "Small Focus of Earth");
    assert_eq!(system.recipes[2].augments, Vec::<String>::new());
    let upgrade_system = &wiki.crafting_systems[1];
    assert_eq!(upgrade_system.name, "Test Upgrade Altar");
    assert!(upgrade_system.families.is_empty());
    assert_eq!(upgrade_system.recipes[0].grants_slot.as_deref(), Some("upgrade: tier 2"));
    assert_eq!(wiki.quest_facts.len(), 3, "crafting.toml tables are not read as quests");
}

#[test]
fn rejects_a_crafting_cost_naming_an_undeclared_ingredient() {
    let error = parsed_edited_crafting(|s| {
        s.replacen("ingredient = \"Small Focus of Fire\"", "ingredient = \"Large Focus of Fire\"", 1)
    })
    .unwrap_err();
    assert!(
        error.contains("Heroic Green Steel")
            && error.contains("Minor Fire Guard")
            && error.contains("Large Focus of Fire"),
        "{error}"
    );
}

#[test]
fn rejects_a_crafting_recipe_with_no_augments_and_no_note() {
    let error = parsed_edited_crafting(|s| {
        s.replace("note = \"Returns a crafted item to its blank state; no augment counterpart.\"\n", "")
    })
    .unwrap_err();
    assert!(
        error.contains("Heroic Green Steel") && error.contains("Cleanse an item") && error.contains("note"),
        "{error}"
    );
}

#[test]
fn rejects_a_crafting_tier_outside_the_four() {
    let error = parsed_edited_crafting(|s| s.replacen("tier = \"heroic\"", "tier = \"mythic\"", 1)).unwrap_err();
    assert!(error.contains("Heroic Green Steel") && error.contains("mythic") && error.contains("legendary"), "{error}");
}

#[test]
fn rejects_a_crafting_cost_quantity_that_is_not_positive() {
    let error = parsed_edited_crafting(|s| s.replace("quantity = 5", "quantity = 0")).unwrap_err();
    assert!(
        error.contains("Heroic Green Steel") && error.contains("Shroud Commendation") && error.contains('0'),
        "{error}"
    );
}

#[test]
fn rejects_a_family_less_system_whose_recipe_lists_augments_naming_the_system_and_recipe() {
    let error = parsed_edited_crafting(|s| s.replace("families = [\"Greensteel_Heroic\"]\n", "")).unwrap_err();
    assert!(
        error.contains("Heroic Green Steel") && error.contains("+5 Fortitude Save") && error.contains("families"),
        "{error}"
    );
}

#[test]
fn rejects_an_empty_families_list_when_a_recipe_lists_augments() {
    let error =
        parsed_edited_crafting(|s| s.replace("families = [\"Greensteel_Heroic\"]", "families = []")).unwrap_err();
    assert!(error.contains("Heroic Green Steel") && error.contains("+5 Fortitude Save"), "{error}");
}

#[test]
fn rejects_a_crafting_recipe_with_no_augments_note_or_grants_slot() {
    let error = parsed_edited_crafting(|s| s.replace("grants_slot = \"upgrade: tier 2\"\n", "")).unwrap_err();
    assert!(
        error.contains("Test Upgrade Altar")
            && error.contains("Test tier 2 upgrade")
            && error.contains("note")
            && error.contains("grants_slot"),
        "{error}"
    );
}

#[test]
fn rejects_an_ingredient_declared_twice_in_a_system() {
    let error =
        parsed_edited_crafting(|s| s.replace("name = \"Small Focus of Fire\"", "name = \"Small Focus of Earth\""))
            .unwrap_err();
    assert!(error.contains("Heroic Green Steel") && error.contains("Small Focus of Earth"), "{error}");
}

#[test]
fn rejects_a_crafting_system_listed_twice_across_files() {
    let error =
        parsed_wiki(&[("crafting.toml", &crafting_fixture_toml()), ("crafting_more.toml", &crafting_fixture_toml())])
            .unwrap_err();
    assert!(error.contains("Heroic Green Steel") && error.contains("crafting_more.toml"), "{error}");
}

fn build_report_with_edited_crafting(edit: impl Fn(String) -> String) -> Result<ddo_etl::build::BuildReport, String> {
    build_report_with(&parsed_edited_crafting(edit).unwrap())
}

#[test]
fn build_fails_naming_an_unknown_crafting_family() {
    let error = build_report_with_edited_crafting(|s| {
        s.replace("[\"Greensteel_Heroic\"]", "[\"Greensteel_Heroic\", \"Greensteel_Mythic\"]")
    })
    .unwrap_err();
    assert!(error.contains("Heroic Green Steel") && error.contains("Greensteel_Mythic"), "{error}");
}

#[test]
fn build_fails_naming_an_augment_absent_from_the_systems_families() {
    let error = build_report_with_edited_crafting(|s| {
        s.replace("augments = [\"Minor Fire Guard\"]", "augments = [\"Silverscale\"]")
    })
    .unwrap_err();
    assert!(
        error.contains("Heroic Green Steel") && error.contains("Silverscale") && error.contains("Greensteel_Heroic"),
        "{error}"
    );
}

#[test]
fn build_fails_naming_an_unknown_slot_label() {
    let error = build_report_with_edited_crafting(|s| {
        s.replacen("crafting: accessory invasion", "crafting: accessory invasions", 1)
    })
    .unwrap_err();
    assert!(error.contains("Heroic Green Steel") && error.contains("crafting: accessory invasions"), "{error}");
}

#[test]
fn build_fails_naming_an_unknown_grants_slot_label() {
    let error =
        build_report_with_edited_crafting(|s| s.replace("\"upgrade: tier 2\"", "\"upgrade: tier 9\"")).unwrap_err();
    assert!(
        error.contains("Test Upgrade Altar")
            && error.contains("Test tier 2 upgrade")
            && error.contains("upgrade: tier 9"),
        "{error}"
    );
}

#[test]
fn build_fails_naming_an_unknown_pack() {
    let error = build_report_with_edited_crafting(|s| s.replacen("npc = ", "pack = \"The Shroud Pack\"\nnpc = ", 1))
        .unwrap_err();
    assert!(error.contains("Heroic Green Steel") && error.contains("The Shroud Pack"), "{error}");
}

type RecipeRow = (String, Option<String>, String, Option<String>, i64);

fn recipe_rows(db: &Connection) -> Vec<RecipeRow> {
    let mut statement = db
        .prepare(
            "SELECT r.tier, t.label, r.option, r.note, r.sort_order FROM crafting_recipes r
               LEFT JOIN augment_slot_types t ON t.id = r.slot_id ORDER BY r.system_id, r.sort_order",
        )
        .unwrap();
    statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn string_column(db: &Connection, sql: &str) -> Vec<String> {
    let mut statement = db.prepare(sql).unwrap();
    statement.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

#[test]
fn merges_crafting_systems_ingredients_and_recipes() {
    let (db, report) = built_db_with(&WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap());
    assert_eq!(
        (report.wiki_crafting_system_count, report.wiki_crafting_recipe_count, report.wiki_crafting_ingredient_count),
        (2, 5, 5)
    );
    let (name, page, pack, npc): (String, String, Option<i64>, Option<String>) = db
        .query_row("SELECT name, page, pack_id, npc FROM crafting_systems ORDER BY id LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .unwrap();
    assert_eq!(
        (name.as_str(), page.as_str(), pack, npc.as_deref()),
        ("Heroic Green Steel", "https://ddowiki.com/page/Green_Steel_items", None, Some("Altar of Invasion"))
    );
    assert_eq!(string_column(&db, "SELECT family FROM crafting_system_families"), ["Greensteel_Heroic"]);
    assert_eq!(
        string_column(
            &db,
            "SELECT name || '|' || tier || '|' || COALESCE(bind, '') FROM crafting_ingredients WHERE system_id = 1
              ORDER BY id"
        ),
        [
            "Small Shard of Power|heroic|Bound to Account",
            "Small Focus of Earth|heroic|",
            "Small Focus of Fire|heroic|",
            "Shroud Commendation|any|Unbound"
        ]
    );
    assert_eq!(
        recipe_rows(&db)[..3],
        [
            ("heroic".into(), Some("crafting: accessory invasion".into()), "+5 Fortitude Save".into(), None, 0),
            ("heroic".into(), Some("crafting: accessory invasion".into()), "Minor Fire Guard".into(), None, 1),
            (
                "any".into(),
                None,
                "Cleanse an item".into(),
                Some("Returns a crafted item to its blank state; no augment counterpart.".into()),
                2
            ),
        ]
    );
    let fortitude_augments = string_column(
        &db,
        "SELECT a.name || '|' || a.description FROM crafting_recipe_augments ra JOIN augments a ON a.id = ra.augment_id
           JOIN crafting_recipes r ON r.id = ra.recipe_id WHERE r.option = '+5 Fortitude Save' ORDER BY a.id",
    );
    assert_eq!(fortitude_augments.len(), 2, "every augment of that name in the family: {fortitude_augments:?}");
    assert!(
        fortitude_augments[0].contains("Earth:Opposition") && fortitude_augments[1].contains("Negative:Opposition")
    );
    assert_eq!(
        string_column(
            &db,
            "SELECT r.option || '|' || i.name || '|' || ri.quantity FROM crafting_recipe_ingredients ri
               JOIN crafting_recipes r ON r.id = ri.recipe_id JOIN crafting_ingredients i ON i.id = ri.ingredient_id
              WHERE r.system_id = 1 ORDER BY r.sort_order, i.id"
        ),
        [
            "+5 Fortitude Save|Small Shard of Power|1",
            "+5 Fortitude Save|Small Focus of Earth|1",
            "Minor Fire Guard|Small Shard of Power|1",
            "Minor Fire Guard|Small Focus of Fire|1",
            "Cleanse an item|Shroud Commendation|5"
        ]
    );
}

#[test]
fn crafting_pack_resolves_to_maetrims_adventure_pack() {
    let report =
        build_report_with_edited_crafting(|s| s.replacen("npc = ", "pack = \"Free to Play\"\nnpc = ", 1)).unwrap();
    assert_eq!(report.wiki_crafting_system_count, 2);
}

#[test]
fn merges_a_family_less_system_whose_recipes_grant_sockets() {
    let (db, _) = built_db_with(&WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap());
    assert_eq!(
        string_column(
            &db,
            "SELECT r.option || '|' || t.label || '|' || COALESCE(r.note, '') || '|' || r.sort_order
               FROM crafting_recipes r JOIN crafting_systems s ON s.id = r.system_id
               JOIN augment_slot_types t ON t.id = r.grants_slot_id
              WHERE s.name = 'Test Upgrade Altar' ORDER BY r.sort_order"
        ),
        [
            "Test tier 2 upgrade|upgrade: tier 2||0",
            "Test Colorless Augment Slot|colorless|Test note: adds a socket, never an augment.|1"
        ]
    );
    assert_eq!(
        string_column(
            &db,
            "SELECT f.family FROM crafting_system_families f JOIN crafting_systems s ON s.id = f.system_id
              WHERE s.name = 'Test Upgrade Altar'"
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        string_column(
            &db,
            "SELECT r.option FROM crafting_recipes r JOIN crafting_systems s ON s.id = r.system_id
              WHERE s.name = 'Heroic Green Steel' AND r.grants_slot_id IS NOT NULL"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn crafting_never_adds_innate_item_bonuses_or_augments() {
    let (without, _) = built_db_with(&WikiOverrides::default());
    let (with, _) = built_db_with(&WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap());
    for table in [
        "augments",
        "item_bonuses JOIN items ON items.id = item_bonuses.item_id WHERE items.source = 'maetrim'",
        "items WHERE source = 'maetrim'",
        "adventure_packs",
        "augment_slot_types",
    ] {
        let count =
            |c: &Connection| c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(count(&with), count(&without), "{table}");
    }
}

fn description_toml(kind: &str, name: &str, description: &str) -> String {
    format!(
        "[[entry]]\nkind = {kind:?}\nname = {name:?}\npage = \"https://ddowiki.com/page/{}\"\nread = \"2026-09-29\"\ndescription = {description:?}\n",
        name.replace(' ', "_")
    )
}

fn description_of(db: &Connection, table: &str, name: &str) -> Vec<Option<String>> {
    let mut statement = db.prepare(&format!("SELECT description FROM {table} WHERE name = ?1 ORDER BY id")).unwrap();
    statement.query_map([name], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

#[test]
fn reads_descriptions_from_descriptions_files() {
    let wiki = WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap();
    assert_eq!(wiki.descriptions.len(), 4);
    let crossbow = &wiki.descriptions[0];
    assert_eq!(crossbow.kind, DescriptionKind::Item);
    assert_eq!(crossbow.name, "+1 Ember Repeating Light Crossbow");
    assert_eq!(crossbow.read, "2026-09-29");
    assert_eq!(crossbow.description, "Test description: a repeating crossbow that burns.");
    assert_eq!(wiki.descriptions[2].kind, DescriptionKind::Augment);
    assert_eq!(wiki.quest_facts.len(), 3, "descriptions.toml tables are not read as quests");
}

#[test]
fn fills_a_blank_item_description() {
    let (db, _) = built_db_with(&WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap());
    assert_eq!(
        description_of(&db, "items", "+1 Ember Repeating Light Crossbow"),
        [Some("Test description: a repeating crossbow that burns.".to_string())]
    );
}

#[test]
fn never_replaces_an_item_description_maetrim_wrote() {
    let (without, _) = built_db_with(&WikiOverrides::default());
    let (with, _) = built_db_with(&WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap());
    let maetrims_description = description_of(&without, "items", "Buckler of the Golden Age");
    assert!(maetrims_description[0].as_deref().is_some_and(|text| text.starts_with("Forged in the Glory")));
    assert_eq!(description_of(&with, "items", "Buckler of the Golden Age"), maetrims_description);
}

#[test]
fn fills_the_drops_in_placeholder_line_of_an_augment_and_keeps_his_lines_before_it() {
    let (db, _) = built_db_with(&WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap());
    assert_eq!(
        description_of(&db, "augments", "Lunar Gem of Evocation (Heroic)"),
        [Some("+2 Profane Bonus to Evocation DCs\nDrops in: Test Quest, end chest".to_string())]
    );
}

#[test]
fn never_replaces_an_augment_description_without_the_placeholder() {
    let wiki = parsed_wiki(&[(
        "descriptions.toml",
        &description_toml("augment", "Lunar Gem of Strength (Heroic)", "Drops in: Test Quest"),
    )])
    .unwrap();
    let (db, report) = built_db_with(&wiki);
    let description = description_of(&db, "augments", "Lunar Gem of Strength (Heroic)");
    assert!(description[0].as_deref().is_some_and(|text| text.contains("Seeds of Decay")), "{description:?}");
    assert_eq!((report.wiki_description_filled_count, report.wiki_description_skipped_count), (0, 1));
}

#[test]
fn weighs_a_feat_description_against_every_feat_with_that_name() {
    let (without, _) = built_db_with(&WikiOverrides::default());
    let (with, _) = built_db_with(&WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap());
    let maetrims_descriptions = description_of(&without, "feats", "Lay on Hands");
    assert_eq!(maetrims_descriptions.len(), 2);
    assert_eq!(description_of(&with, "feats", "Lay on Hands"), maetrims_descriptions);
}

#[test]
fn build_reports_description_entries_filled_and_skipped() {
    let report = build_report_with(&WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap()).unwrap();
    assert_eq!(
        (
            report.wiki_description_entry_count,
            report.wiki_description_filled_count,
            report.wiki_description_skipped_count
        ),
        (4, 2, 3)
    );
}

#[test]
fn rejects_a_description_kind_outside_the_five() {
    let error = parsed_wiki(&[("descriptions.toml", &description_toml("spell", "Fireball", "Burns."))]).unwrap_err();
    assert!(
        error.contains("descriptions.toml")
            && error.contains("Fireball")
            && error.contains("spell")
            && error.contains("enhancement"),
        "{error}"
    );
}

#[test]
fn rejects_an_empty_or_untrimmed_description() {
    for bad in ["", "   ", " Leading space.", "Trailing newline.\n"] {
        let error = parsed_wiki(&[("descriptions.toml", &description_toml("item", "Five Rings", bad))]).unwrap_err();
        assert!(error.contains("Five Rings") && error.contains("description"), "{bad:?}: {error}");
    }
}

#[test]
fn rejects_a_kind_and_name_listed_twice_across_descriptions_files() {
    let error = parsed_wiki(&[
        ("descriptions.toml", &description_toml("item", "Five Rings", "One.")),
        ("descriptions_more.toml", &description_toml("item", "Five Rings", "Two.")),
    ])
    .unwrap_err();
    assert!(
        error.contains("Five Rings") && error.contains("descriptions.toml") && error.contains("descriptions_more.toml"),
        "{error}"
    );
}

#[test]
fn a_name_may_carry_a_description_for_each_kind() {
    let wiki = parsed_wiki(&[(
        "descriptions.toml",
        &(description_toml("feat", "Lay on Hands", "One.") + &description_toml("enhancement", "Lay on Hands", "Two.")),
    )]);
    assert!(wiki.is_ok(), "{wiki:?}");
}

#[test]
fn build_fails_naming_a_description_for_a_name_absent_from_its_kind() {
    for kind in ["item", "augment", "race", "feat", "enhancement"] {
        let wiki =
            parsed_wiki(&[("descriptions.toml", &description_toml(kind, "The Missing Thing", "Missing."))]).unwrap();
        let error = build_report_with(&wiki).unwrap_err();
        assert!(error.contains("The Missing Thing") && error.contains(kind), "{kind}: {error}");
    }
}

#[test]
fn build_fails_naming_an_item_description_for_a_race_name() {
    let wiki = parsed_wiki(&[("descriptions.toml", &description_toml("item", "Dwarf", "Short."))]).unwrap();
    let error = build_report_with(&wiki).unwrap_err();
    assert!(error.contains("Dwarf") && error.contains("item"), "{error}");
}

fn items_fixture_toml() -> String {
    std::fs::read_to_string(fixtures_dir().join("wiki/items.toml")).unwrap()
}

fn parsed_edited_items(edit: impl Fn(String) -> String) -> Result<WikiOverrides, String> {
    parsed_wiki(&[("items.toml", &edit(items_fixture_toml()))])
}

const WIKI_AXE: &str = "Battle Axe of the Oozing Hunger";

#[test]
fn reads_wiki_items_from_items_files() {
    let wiki = WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap();
    assert_eq!(wiki.items.len(), 2);
    let axe = &wiki.items[0];
    assert_eq!(axe.name, WIKI_AXE);
    assert_eq!(axe.file_name, "items.toml");
    assert_eq!(
        (axe.slot.as_str(), axe.category.as_str(), axe.item_type.as_deref()),
        ("Main Hand", "Weapon", Some("Battle Axe"))
    );
    assert_eq!((axe.minimum_level, axe.enhancement_bonus), (29, Some(15)));
    assert_eq!(axe.quests[0].name, "The Grotto");
    assert_eq!(axe.quests[0].loot_type, "chest");
    assert_eq!(axe.augment_slots, ["red", "colorless"]);
    assert_eq!(axe.bonuses[1].stat, "Doublestrike");
    assert_eq!(axe.effects[1].name, "Ethereal");
    let weapon = axe.weapon.as_ref().unwrap();
    assert_eq!(
        (weapon.damage_dice_count, weapon.damage_dice_sides, weapon.damage_multiplier),
        (Some(1), Some(8), Some(3.0))
    );
    assert_eq!(weapon.dr_bypass, ["Magic", "Slash"]);
    assert!(axe.armor.is_none());
    assert!(wiki.items[1].weapon.is_none() && wiki.items[1].quests.is_empty());
    assert_eq!(wiki.quest_facts.len(), 3, "items.toml tables are not read as quests");
}

#[test]
fn rejects_a_wiki_item_listed_twice_across_items_files() {
    let error =
        parsed_wiki(&[("items.toml", &items_fixture_toml()), ("items_more.toml", &items_fixture_toml())]).unwrap_err();
    assert!(error.contains(WIKI_AXE) && error.contains("items.toml") && error.contains("items_more.toml"), "{error}");
}

#[test]
fn rejects_a_wiki_item_value_outside_maetrims_vocabularies_naming_the_item_and_field() {
    for (field, good, bad) in [
        ("slot", "slot = \"Main Hand\"", "slot = \"Main hand\""),
        ("category", "category = \"Weapon\"", "category = \"Weapons\""),
        ("item_type", "item_type = \"Battle Axe\"", "item_type = \"Battleaxe\""),
        ("stat", "stat = \"Strength\"", "stat = \"Strenght\""),
        ("bonus_type", "bonus_type = \"Insight\"", "bonus_type = \"Insightful\""),
        ("loot_type", "loot_type = \"chest\"", "loot_type = \"end chest\""),
        ("handedness", "handedness = \"One-handed\"", "handedness = \"One handed\""),
    ] {
        let error = parsed_edited_items(|s| s.replacen(good, bad, 1)).unwrap_err();
        let bad_value = bad.split('"').nth(1).unwrap();
        assert!(
            error.contains("items.toml")
                && error.contains(WIKI_AXE)
                && error.contains(field)
                && error.contains(bad_value),
            "{field}: {error}"
        );
    }
}

#[test]
fn rejects_a_weapon_without_weapon_stats_and_weapon_stats_on_jewelry() {
    let error = parsed_edited_items(|s| {
        let weapon_start = s.find("[item.weapon]").unwrap();
        let weapon_end = weapon_start + s[weapon_start..].find("[[item]]").unwrap();
        format!("{}{}", &s[..weapon_start], &s[weapon_end..])
    })
    .unwrap_err();
    assert!(error.contains(WIKI_AXE) && error.contains("weapon"), "{error}");
    let error = parsed_edited_items(|s| s.replace("category = \"Weapon\"", "category = \"Jewelry\"")).unwrap_err();
    assert!(error.contains(WIKI_AXE) && error.contains("weapon"), "{error}");
}

#[test]
fn rejects_armor_without_a_known_armor_type() {
    let armor_item = "[[item]]\nname = \"Test Plate of the Oozing Hunger\"\npage = \"https://ddowiki.com/page/Item:Test\"\nread = \"2026-09-29\"\nslot = \"Body\"\ncategory = \"Armor\"\nitem_type = \"Heavy\"\nminimum_level = 29\ndrop_location = \"Test source\"\n\n[item.armor]\narmor_type = \"Heavy\"\narmor_bonus = 30\n";
    assert!(parsed_wiki(&[("items.toml", armor_item)]).is_ok());
    let error = parsed_wiki(&[("items.toml", &armor_item.replace("armor_type = \"Heavy\"", "armor_type = \"Plate\""))])
        .unwrap_err();
    assert!(
        error.contains("Test Plate of the Oozing Hunger") && error.contains("armor_type") && error.contains("Plate"),
        "{error}"
    );
    let error = parsed_wiki(&[(
        "items.toml",
        &armor_item.replace("\n[item.armor]\narmor_type = \"Heavy\"\narmor_bonus = 30\n", ""),
    )])
    .unwrap_err();
    assert!(error.contains("Test Plate of the Oozing Hunger") && error.contains("armor"), "{error}");
}

#[test]
fn rejects_unknown_wiki_item_fields() {
    let error = parsed_edited_items(|s| s.replacen("minimum_level = 29", "minimum_level = 29\nrarity = \"Rare\"", 1))
        .unwrap_err();
    assert!(error.contains(WIKI_AXE) && error.contains("rarity"), "{error}");
}

fn built_db_with_fixture_wiki() -> (Connection, ddo_etl::build::BuildReport) {
    built_db_with(&WikiOverrides::from_dir(&fixtures_dir().join("wiki")).unwrap())
}

fn item_count(db: &Connection, sql_condition: &str) -> i64 {
    db.query_row(&format!("SELECT COUNT(*) FROM items WHERE {sql_condition}"), [], |r| r.get(0)).unwrap()
}

#[test]
fn drops_a_wiki_item_maetrim_already_carries_and_reports_it() {
    let (without, _) = built_db_with(&WikiOverrides::default());
    let (with, report) = built_db_with_fixture_wiki();
    assert_eq!(item_count(&with, "name = 'Five Rings'"), 1);
    assert_eq!(item_count(&with, "name = 'Five Rings' AND source = 'maetrim'"), 1);
    let bonus_names = |db: &Connection| {
        string_column(
            db,
            "SELECT b.name FROM item_bonuses ib JOIN bonuses b ON b.id = ib.bonus_id JOIN items i ON i.id = ib.item_id
              WHERE i.name = 'Five Rings' ORDER BY ib.sort_order",
        )
    };
    assert_eq!(bonus_names(&with), bonus_names(&without));
    assert_eq!(report.wiki_item_superseded_count, 1);
    assert_eq!(
        report.superseded_wiki_items,
        [SupersededWikiItem { name: "Five Rings".into(), file_name: "items.toml".into() }]
    );
}

type WikiItemRow =
    (String, String, Option<String>, i64, Option<i64>, Option<String>, Option<String>, Option<String>, String);

#[test]
fn writes_a_new_wiki_item_with_its_stats_bonuses_effects_sockets_set_and_quests() {
    let (db, report) = built_db_with_fixture_wiki();
    let item_row: WikiItemRow = db
        .query_row(
            "SELECT es.name, i.item_category, i.item_type, i.minimum_level, i.enhancement_bonus, m.name, i.set_bonus,
                    i.wiki_url, i.source
               FROM items i JOIN equipment_slots es ON es.id = i.slot_id LEFT JOIN item_materials m ON m.id = i.material_id
              WHERE i.name = ?1",
            [WIKI_AXE],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?)),
        )
        .unwrap();
    assert_eq!(
        item_row,
        (
            "Main Hand".into(),
            "Weapon".into(),
            Some("Battle Axe".into()),
            29,
            Some(15),
            Some("Steel".into()),
            Some("Inevitable Balance".into()),
            Some("https://ddowiki.com/page/Item:Battle_Axe_of_the_Oozing_Hunger".into()),
            "wiki".into()
        )
    );
    let item_id: i64 = db.query_row("SELECT id FROM items WHERE name = ?1", [WIKI_AXE], |r| r.get(0)).unwrap();
    let item_column = |sql: &str| string_column(&db, &sql.replace("?item", &item_id.to_string()));
    assert_eq!(
        item_column(
            "SELECT wt.name || '|' || w.base_dice_count || 'd' || w.base_dice_sides || '+' || w.base_dice_bonus || '|' ||
                    w.damage_multiplier || '|' || w.critical_threat_range || '|' || w.critical_multiplier || '|' ||
                    w.handedness || '|' || w.damage || '|' || w.critical
               FROM item_weapon_stats w JOIN weapon_types wt ON wt.id = w.weapon_type_id WHERE w.item_id = ?item"
        ),
        ["Battle Axe|1d8+0|3.0|2|3|One-handed|3[1d8] + 15 Magic, Slash|19-20 / x3"]
    );
    assert_eq!(
        item_column("SELECT bypass FROM item_dr_bypass WHERE item_id = ?item ORDER BY bypass"),
        ["Magic", "Slash"]
    );
    assert_eq!(
        item_column(
            "SELECT s.name || '|' || bt.name || '|' || b.value FROM item_bonuses ib JOIN bonuses b ON b.id = ib.bonus_id
               JOIN stats s ON s.id = b.stat_id JOIN bonus_types bt ON bt.id = b.bonus_type_id
              WHERE ib.item_id = ?item ORDER BY ib.sort_order"
        ),
        ["Strength|Enhancement|15", "Doublestrike|Insight|5"]
    );
    let effect_lines = item_column(
        "SELECT e.name || '|' || COALESCE(ie.value, '') || '|' || COALESCE(ie.target, '') || '|' ||
                COALESCE(e.description, '')
           FROM item_effects ie JOIN effects e ON e.id = ie.effect_id WHERE ie.item_id = ?item ORDER BY ie.sort_order",
    );
    assert_eq!(effect_lines[0], "Test Oozing Hunger|3|All|Test description: on hit, the target oozes.");
    assert!(
        effect_lines[1].starts_with("Ethereal|||Ethereal: Equipping this item"),
        "his description stands: {effect_lines:?}"
    );
    assert_eq!(effect_lines.len(), 2);
    assert_eq!(
        string_column(&db, "SELECT COUNT(*) || '' FROM effects WHERE name = 'Ethereal'"),
        ["1"],
        "his Ethereal is reused"
    );
    assert_eq!(
        item_column(
            "SELECT t.label FROM item_augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id
              WHERE s.item_id = ?item ORDER BY s.sort_order"
        ),
        ["red", "colorless"]
    );
    assert_eq!(
        item_column(
            "SELECT s.name FROM set_bonus_items sbi JOIN set_bonuses s ON s.id = sbi.set_id WHERE sbi.item_id = ?item"
        ),
        ["Inevitable Balance"]
    );
    assert_eq!(quest_loot_row(&db, "The Grotto", WIKI_AXE), Some(("chest".into(), false)));
    assert_eq!(report.wiki_item_written_count, 1);
    assert_eq!(item_count(&db, "source = 'wiki'"), report.wiki_item_written_count as i64);
}

#[test]
fn writes_a_probable_duplicate_of_a_maetrim_item_and_reports_both_names() {
    let wiki = parsed_edited_items(|s| s.replace(WIKI_AXE, "Argentis Armor (Level 12)")).unwrap();
    let (db, report) = built_db_with(&wiki);
    assert_eq!(item_count(&db, "name = 'Argentis Armor (Level 12)' AND source = 'wiki'"), 1);
    assert_eq!((report.wiki_item_written_count, report.wiki_item_probable_duplicate_count), (1, 1));
    assert_eq!(
        report.probable_duplicate_wiki_items,
        [ProbableDuplicateWikiItem {
            name: "Argentis Armor (Level 12)".into(),
            maetrim_name: "Argenti's Armor".into()
        }]
    );
}

#[test]
fn a_wiki_item_with_a_new_name_is_no_probable_duplicate() {
    let (_, report) = built_db_with_fixture_wiki();
    assert_eq!(report.wiki_item_probable_duplicate_count, 0);
    assert!(report.probable_duplicate_wiki_items.is_empty());
}

#[test]
fn build_fails_naming_a_wiki_item_value_absent_from_maetrims_files() {
    for (field, good, bad) in [
        ("material", "material = \"Steel\"", "material = \"Byeshk\""),
        ("set", "set = \"Inevitable Balance\"", "set = \"Inevitable Imbalance\""),
        ("augment_slots", "augment_slots = [\"red\", \"colorless\"]", "augment_slots = [\"red\", \"colourless\"]"),
        ("quest", "{ name = \"The Grotto\", loot_type", "{ name = \"The Grotto Revisited\", loot_type"),
    ] {
        let wiki = parsed_edited_items(|s| s.replacen(good, bad, 1)).unwrap();
        let error = build_report_with(&wiki).unwrap_err();
        let bad_value = if field == "augment_slots" { "colourless" } else { bad.split('"').nth(1).unwrap() };
        assert!(
            error.contains(WIKI_AXE)
                && error.contains("items.toml")
                && error.contains(field)
                && error.contains(bad_value),
            "{field}: {error}"
        );
    }
}

#[test]
fn wiki_items_never_change_maetrims_items() {
    let (without, _) = built_db_with(&WikiOverrides::default());
    let (with, _) = built_db_with_fixture_wiki();
    assert_eq!(item_count(&with, "source = 'maetrim'"), item_count(&without, "1"));
    assert_eq!(item_count(&without, "source = 'wiki'"), 0);
}

const WIKI_RUNE_ARM: &str = "[[item]]\nname = \"Test Rune Arm of the Oozing Hunger\"\npage = \"https://ddowiki.com/page/Item:Test\"\nread = \"2026-09-29\"\nslot = \"Runearm\"\ncategory = \"Weapon\"\nitem_type = \"Rune Arm\"\nminimum_level = 29\ndrop_location = \"Test source\"\n\n[item.weapon]\nhandedness = \"Off-hand\"\n";

type WeaponStatsColumns = (
    String,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<f64>,
    Option<i64>,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<String>,
);

fn weapon_stats_columns(db: &Connection, item_name: &str) -> WeaponStatsColumns {
    db.query_row(
        "SELECT wt.name, w.base_dice_count, w.base_dice_sides, w.base_dice_bonus, w.damage_multiplier,
                w.critical_threat_range, w.critical_multiplier, w.handedness, w.damage, w.critical
           FROM item_weapon_stats w JOIN weapon_types wt ON wt.id = w.weapon_type_id JOIN items i ON i.id = w.item_id
          WHERE i.name = ?1",
        [item_name],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
            ))
        },
    )
    .unwrap()
}

#[test]
fn writes_a_rune_arm_without_dice_as_maetrims_rune_arms_are_written() {
    let wiki = parsed_wiki(&[("items.toml", WIKI_RUNE_ARM)]).unwrap();
    let (db, report) = built_db_with(&wiki);
    assert_eq!(report.wiki_item_written_count, 1);
    let wiki_rune_arm = weapon_stats_columns(&db, "Test Rune Arm of the Oozing Hunger");
    assert_eq!(wiki_rune_arm, weapon_stats_columns(&db, "Acid Rune Arm"));
    assert_eq!(
        wiki_rune_arm,
        ("Rune Arm".into(), None, None, None, None, None, None, Some("Off-hand".into()), None, None)
    );
}

#[test]
fn rejects_weapon_stats_without_handedness() {
    let error = parsed_wiki(&[("items.toml", &WIKI_RUNE_ARM.replace("handedness = \"Off-hand\"\n", ""))]).unwrap_err();
    assert!(error.contains("Test Rune Arm of the Oozing Hunger") && error.contains("handedness"), "{error}");
}
