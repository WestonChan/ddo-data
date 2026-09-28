use ddo_etl::build::build;
use ddo_etl::wiki::WikiOverrides;
use ddo_model::DatasetVersion;
use rusqlite::Connection;
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn version() -> DatasetVersion {
    DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-09-20T00:00:00Z".into() }
}

fn quest(name: &str, rare: &[&str]) -> String {
    let rare: Vec<String> = rare.iter().map(|r| format!("{r:?}")).collect();
    format!(
        "[[quest]]\nname = {name:?}\npage = \"https://ddowiki.com/page/{}\"\nread = \"2026-09-27\"\nrare = [{}]\n",
        name.replace(' ', "_"),
        rare.join(", ")
    )
}

fn load(files: &[(&str, &str)]) -> Result<WikiOverrides, String> {
    WikiOverrides::from_sources(files).map_err(|e| format!("{e:#}"))
}

fn build_with(wiki: &WikiOverrides) -> Result<ddo_etl::build::BuildReport, String> {
    let mut conn = Connection::open_in_memory().unwrap();
    build(&fixtures().join("DataFiles"), wiki, &mut conn, &version()).map_err(|e| format!("{e:#}"))
}

#[test]
fn reads_quest_loot_from_every_toml_file_in_the_directory() {
    let wiki = WikiOverrides::from_dir(&fixtures().join("wiki")).unwrap();
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
    let src = quest("The Grotto", &[]).replace("https://ddowiki.com/page/", "https://example.com/");
    let err = load(&[("quest_loot_a.toml", &src)]).unwrap_err();
    assert!(err.contains("The Grotto") && err.contains("https://ddowiki.com/page/"), "{err}");
}

#[test]
fn rejects_a_read_date_that_is_not_iso() {
    for bad in ["27/09/2026", "2026-9-27", "2026-13-01", "2026-02-30", "yesterday"] {
        let src = quest("The Grotto", &[]).replace("2026-09-27", bad);
        let err = load(&[("quest_loot_a.toml", &src)]).unwrap_err();
        assert!(err.contains("The Grotto") && err.contains(bad), "{bad}: {err}");
    }
}

#[test]
fn rejects_a_quest_listed_twice_across_files() {
    let err =
        load(&[("quest_loot_a.toml", &quest("The Grotto", &[])), ("quest_loot_b.toml", &quest("The Grotto", &[]))])
            .unwrap_err();
    assert!(
        err.contains("The Grotto") && err.contains("quest_loot_a.toml") && err.contains("quest_loot_b.toml"),
        "{err}"
    );
}

#[test]
fn rejects_unknown_fields() {
    let src = quest("The Grotto", &[]) + "common = [\"Docent of Defiance\"]\n";
    let err = load(&[("quest_loot_a.toml", &src)]).unwrap_err();
    assert!(err.contains("quest_loot_a.toml") && err.contains("common"), "{err}");
}

#[test]
fn build_fails_naming_a_quest_absent_from_the_quests_table() {
    let wiki = load(&[("quest_loot_a.toml", &quest("The Missing Quest", &[]))]).unwrap();
    let err = build_with(&wiki).unwrap_err();
    assert!(err.contains("The Missing Quest"), "{err}");
}

#[test]
fn build_fails_naming_an_item_absent_from_the_items_table() {
    let wiki = load(&[("quest_loot_a.toml", &quest("Book Burning", &["Buckler of the Missing Age"]))]).unwrap();
    let err = build_with(&wiki).unwrap_err();
    assert!(err.contains("Buckler of the Missing Age") && err.contains("Book Burning"), "{err}");
}

#[test]
fn build_reports_the_wiki_entries_it_applied() {
    let wiki = WikiOverrides::from_dir(&fixtures().join("wiki")).unwrap();
    let report = build_with(&wiki).unwrap();
    assert_eq!(report.wiki_quest_loot_entries, 1);
    assert_eq!(report.wiki_rare_drops, 1);
}

fn built_with(wiki: &WikiOverrides) -> (Connection, ddo_etl::build::BuildReport) {
    let mut conn = Connection::open_in_memory().unwrap();
    let report = build(&fixtures().join("DataFiles"), wiki, &mut conn, &version()).unwrap();
    (conn, report)
}

fn loot(conn: &Connection, quest: &str, item: &str) -> Option<(String, bool)> {
    conn.query_row(
        "SELECT ql.loot_type, ql.is_rare FROM quest_loot ql JOIN quests q ON q.id = ql.quest_id JOIN items i ON i.id = ql.item_id
          WHERE q.name = ?1 AND i.name = ?2",
        [quest, item],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .ok()
}

#[test]
fn marks_rare_drops_on_the_links_maetrims_drop_text_made() {
    let (conn, report) = built_with(&WikiOverrides::from_dir(&fixtures().join("wiki")).unwrap());
    assert_eq!(loot(&conn, "Book Burning", "Buckler of the Golden Age"), Some(("chest".into(), true)));
    assert_eq!(loot(&conn, "The Cursed Crypt", "Docent of Defiance"), Some(("chest".into(), false)));
    assert_eq!(report.wiki_quest_loot_links_added, 0, "Maetrim's drop text already links the buckler");
}

#[test]
fn adds_a_chest_link_for_a_rare_drop_and_never_changes_maetrims_loot_type() {
    let wiki = load(&[
        ("quest_loot_a.toml", &quest("The Grotto", &["Docent of Defiance"])),
        ("quest_loot_b.toml", &quest("Caught in the Web", &["Sireth, Spear of the Sky"])),
    ])
    .unwrap();
    let (conn, report) = built_with(&wiki);
    assert_eq!(loot(&conn, "The Grotto", "Docent of Defiance"), Some(("chest".into(), true)));
    assert_eq!(loot(&conn, "The Cursed Crypt", "Docent of Defiance"), Some(("chest".into(), false)));
    assert_eq!(loot(&conn, "Caught in the Web", "Sireth, Spear of the Sky"), Some(("raid".into(), true)));
    assert_eq!(report.wiki_quest_loot_links_added, 1);
    assert_eq!(report.wiki_rare_drops, 2);
}

fn quest_facts(name: &str, extra: &str) -> String {
    format!(
        "[[quest]]\nname = {name:?}\npage = \"https://ddowiki.com/page/Quests_by_level_and_XP\"\nread = \"2026-09-28\"\nfree_to_play = false\n{extra}"
    )
}

#[test]
fn reads_quest_facts_from_quests_files() {
    let wiki = WikiOverrides::from_dir(&fixtures().join("wiki")).unwrap();
    assert_eq!(wiki.quests.len(), 3);
    let chronoscope = &wiki.quests[0];
    assert_eq!(chronoscope.name, "The Chronoscope");
    assert_eq!(chronoscope.duration.as_deref(), Some("Long"));
    assert!(!chronoscope.free_to_play);
    assert_eq!(chronoscope.legendary_level, Some(34));
    assert_eq!(chronoscope.zone.as_deref(), Some("The Harbor"));
    assert_eq!(chronoscope.bestowed_by.as_deref(), Some("A harbor quest giver"));
    assert_eq!(chronoscope.flagging.as_deref(), Some("None; open to all."));
    let epic = chronoscope.xp.epic.as_ref().unwrap();
    assert_eq!((epic.casual, epic.normal, epic.hard, epic.elite), (None, Some(23883), Some(24669), Some(25456)));
    assert!(chronoscope.xp.legendary.is_none());
    assert_eq!(wiki.quest_loot.len(), 1, "quests.toml tables are not read as quest loot");
}

#[test]
fn embedded_quests_file_loads() {
    let wiki = WikiOverrides::embedded().unwrap();
    assert!(wiki.quests.iter().any(|q| q.name == "A Blood Pact" && q.legendary_level == Some(37)));
}

#[test]
fn rejects_a_file_name_that_names_no_wiki_file_type() {
    let err = load(&[("loot.toml", &quest("The Grotto", &[]))]).unwrap_err();
    assert!(
        err.contains("loot.toml") && err.contains("quest_loot") && err.contains("quests") && err.contains("crafting"),
        "{err}"
    );
}

#[test]
fn rejects_a_duration_outside_the_four_the_wiki_uses() {
    let err = load(&[("quests.toml", &quest_facts("The Grotto", "duration = \"Epic\"\n"))]).unwrap_err();
    assert!(err.contains("The Grotto") && err.contains("Epic") && err.contains("Very long"), "{err}");
}

#[test]
fn rejects_negative_xp() {
    let src = quest_facts("The Grotto", "xp.epic = { normal = -1 }\n");
    let err = load(&[("quests.toml", &src)]).unwrap_err();
    assert!(err.contains("The Grotto") && err.contains("epic") && err.contains("-1"), "{err}");
}

#[test]
fn rejects_quest_facts_without_free_to_play() {
    let src = quest_facts("The Grotto", "").replace("free_to_play = false\n", "");
    let err = load(&[("quests.toml", &src)]).unwrap_err();
    assert!(err.contains("quests.toml") && err.contains("free_to_play"), "{err}");
}

#[test]
fn rejects_quest_facts_citing_a_page_outside_ddowiki() {
    let src = quest_facts("The Grotto", "").replace("https://ddowiki.com/page/", "https://example.com/");
    let err = load(&[("quests.toml", &src)]).unwrap_err();
    assert!(err.contains("The Grotto") && err.contains("https://ddowiki.com/page/"), "{err}");
}

#[test]
fn rejects_quest_facts_listed_twice_across_quests_files() {
    let err =
        load(&[("quests.toml", &quest_facts("The Grotto", "")), ("quests_epic.toml", &quest_facts("The Grotto", ""))])
            .unwrap_err();
    assert!(err.contains("The Grotto") && err.contains("quests.toml") && err.contains("quests_epic.toml"), "{err}");
}

#[test]
fn a_quest_may_carry_both_loot_and_facts() {
    let wiki = load(&[("quest_loot.toml", &quest("The Grotto", &[])), ("quests.toml", &quest_facts("The Grotto", ""))]);
    assert!(wiki.is_ok(), "{wiki:?}");
}

#[test]
fn build_fails_naming_quest_facts_for_a_quest_absent_from_the_quests_table() {
    let wiki = load(&[("quests.toml", &quest_facts("The Missing Quest", ""))]).unwrap();
    let err = build_with(&wiki).unwrap_err();
    assert!(err.contains("The Missing Quest") && err.contains("quests"), "{err}");
}

type WikiQuestColumns = (Option<String>, bool, Option<i64>, Option<String>, Option<String>, Option<String>);

fn wiki_columns(conn: &Connection, quest: &str) -> WikiQuestColumns {
    conn.query_row(
        "SELECT duration, is_free_to_play, legendary_level, zone, bestowed_by, flagging FROM quests WHERE name = ?1",
        [quest],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
    )
    .unwrap()
}

type XpRow = (String, Option<i64>, Option<i64>, Option<i64>, Option<i64>);

fn xp_rows(conn: &Connection, quest: &str) -> Vec<XpRow> {
    let mut stmt = conn
        .prepare(
            "SELECT x.tier, x.casual, x.normal, x.hard, x.elite FROM quest_xp x JOIN quests q ON q.id = x.quest_id
              WHERE q.name = ?1 ORDER BY x.tier",
        )
        .unwrap();
    stmt.query_map([quest], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

type MaetrimQuestColumns = (Option<i64>, Option<i64>, Option<i64>, Option<i64>, Option<i64>, bool, String);

fn maetrim_columns(conn: &Connection, quest: &str) -> MaetrimQuestColumns {
    conn.query_row(
        "SELECT level, epic_level, pack_id, patron_id, favor, is_raid, difficulties FROM quests WHERE name = ?1",
        [quest],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
    )
    .unwrap()
}

#[test]
fn fills_the_wiki_only_quest_columns_and_xp_rows() {
    let (conn, report) = built_with(&WikiOverrides::from_dir(&fixtures().join("wiki")).unwrap());
    assert_eq!(
        wiki_columns(&conn, "The Chronoscope"),
        (
            Some("Long".into()),
            false,
            Some(34),
            Some("The Harbor".into()),
            Some("A harbor quest giver".into()),
            Some("None; open to all.".into())
        )
    );
    assert_eq!(wiki_columns(&conn, "The Grotto"), (None, true, None, None, None, None));
    assert_eq!(
        xp_rows(&conn, "The Chronoscope"),
        vec![
            ("epic".into(), None, Some(23883), Some(24669), Some(25456)),
            ("heroic".into(), None, Some(4240), Some(4516), Some(4792)),
        ]
    );
    assert_eq!(xp_rows(&conn, "The Grotto"), vec![("heroic".into(), Some(304), None, None, None)]);
    assert_eq!(wiki_columns(&conn, "Caught in the Web"), (None, false, None, None, None, None));
    assert_eq!(report.wiki_quest_entries, 3);
    assert_eq!(report.wiki_quest_xp_rows, 5);
}

#[test]
fn quest_facts_never_change_maetrims_quest_columns() {
    let (without, _) = built_with(&WikiOverrides::default());
    let (with, _) = built_with(&WikiOverrides::from_dir(&fixtures().join("wiki")).unwrap());
    for quest in ["The Chronoscope", "The Grotto", "Book Burning"] {
        assert_eq!(maetrim_columns(&with, quest), maetrim_columns(&without, quest), "{quest}");
    }
}

fn crafting_fixture() -> String {
    std::fs::read_to_string(fixtures().join("wiki/crafting.toml")).unwrap()
}

fn crafting_with(edit: impl Fn(String) -> String) -> Result<WikiOverrides, String> {
    load(&[("crafting.toml", &edit(crafting_fixture()))])
}

#[test]
fn reads_crafting_systems_from_crafting_files() {
    let wiki = WikiOverrides::from_dir(&fixtures().join("wiki")).unwrap();
    assert_eq!(wiki.crafting.len(), 1);
    let system = &wiki.crafting[0];
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
    assert_eq!(wiki.quests.len(), 3, "crafting.toml tables are not read as quests");
}

#[test]
fn rejects_a_crafting_cost_naming_an_undeclared_ingredient() {
    let err = crafting_with(|s| {
        s.replacen("ingredient = \"Small Focus of Fire\"", "ingredient = \"Large Focus of Fire\"", 1)
    })
    .unwrap_err();
    assert!(
        err.contains("Heroic Green Steel") && err.contains("Minor Fire Guard") && err.contains("Large Focus of Fire"),
        "{err}"
    );
}

#[test]
fn rejects_a_crafting_recipe_with_no_augments_and_no_note() {
    let err = crafting_with(|s| {
        s.replace("note = \"Returns a crafted item to its blank state; no augment counterpart.\"\n", "")
    })
    .unwrap_err();
    assert!(err.contains("Heroic Green Steel") && err.contains("Cleanse an item") && err.contains("note"), "{err}");
}

#[test]
fn rejects_a_crafting_tier_outside_the_four() {
    let err = crafting_with(|s| s.replacen("tier = \"heroic\"", "tier = \"mythic\"", 1)).unwrap_err();
    assert!(err.contains("Heroic Green Steel") && err.contains("mythic") && err.contains("legendary"), "{err}");
}

#[test]
fn rejects_a_crafting_cost_quantity_that_is_not_positive() {
    let err = crafting_with(|s| s.replace("quantity = 5", "quantity = 0")).unwrap_err();
    assert!(err.contains("Heroic Green Steel") && err.contains("Shroud Commendation") && err.contains('0'), "{err}");
}

#[test]
fn rejects_a_crafting_system_missing_its_families_naming_the_system() {
    let err = crafting_with(|s| s.replace("families = [\"Greensteel_Heroic\"]\n", "")).unwrap_err();
    assert!(err.contains("Heroic Green Steel") && err.contains("families"), "{err}");
}

#[test]
fn rejects_an_ingredient_declared_twice_in_a_system() {
    let err =
        crafting_with(|s| s.replace("name = \"Small Focus of Fire\"", "name = \"Small Focus of Earth\"")).unwrap_err();
    assert!(err.contains("Heroic Green Steel") && err.contains("Small Focus of Earth"), "{err}");
}

#[test]
fn rejects_a_crafting_system_listed_twice_across_files() {
    let err = load(&[("crafting.toml", &crafting_fixture()), ("crafting_more.toml", &crafting_fixture())]).unwrap_err();
    assert!(err.contains("Heroic Green Steel") && err.contains("crafting_more.toml"), "{err}");
}

fn build_crafting_with(edit: impl Fn(String) -> String) -> Result<ddo_etl::build::BuildReport, String> {
    build_with(&crafting_with(edit).unwrap())
}

#[test]
fn build_fails_naming_an_unknown_crafting_family() {
    let err =
        build_crafting_with(|s| s.replace("[\"Greensteel_Heroic\"]", "[\"Greensteel_Heroic\", \"Greensteel_Mythic\"]"))
            .unwrap_err();
    assert!(err.contains("Heroic Green Steel") && err.contains("Greensteel_Mythic"), "{err}");
}

#[test]
fn build_fails_naming_an_augment_absent_from_the_systems_families() {
    let err = build_crafting_with(|s| s.replace("augments = [\"Minor Fire Guard\"]", "augments = [\"Silverscale\"]"))
        .unwrap_err();
    assert!(
        err.contains("Heroic Green Steel") && err.contains("Silverscale") && err.contains("Greensteel_Heroic"),
        "{err}"
    );
}

#[test]
fn build_fails_naming_an_unknown_slot_label() {
    let err = build_crafting_with(|s| s.replacen("crafting: accessory invasion", "crafting: accessory invasions", 1))
        .unwrap_err();
    assert!(err.contains("Heroic Green Steel") && err.contains("crafting: accessory invasions"), "{err}");
}

#[test]
fn build_fails_naming_an_unknown_pack() {
    let err = build_crafting_with(|s| s.replace("npc = ", "pack = \"The Shroud Pack\"\nnpc = ")).unwrap_err();
    assert!(err.contains("Heroic Green Steel") && err.contains("The Shroud Pack"), "{err}");
}

type RecipeRow = (String, Option<String>, String, Option<String>, i64);

fn recipe_rows(conn: &Connection) -> Vec<RecipeRow> {
    let mut stmt = conn
        .prepare(
            "SELECT r.tier, t.label, r.option, r.note, r.sort_order FROM crafting_recipes r
               LEFT JOIN augment_slot_types t ON t.id = r.slot_id ORDER BY r.sort_order",
        )
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn strings(conn: &Connection, sql: &str) -> Vec<String> {
    let mut stmt = conn.prepare(sql).unwrap();
    stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

#[test]
fn merges_crafting_systems_ingredients_and_recipes() {
    let (conn, report) = built_with(&WikiOverrides::from_dir(&fixtures().join("wiki")).unwrap());
    assert_eq!(
        (report.wiki_crafting_systems, report.wiki_crafting_recipes, report.wiki_crafting_ingredients),
        (1, 3, 4)
    );
    let (name, page, pack, npc): (String, String, Option<i64>, Option<String>) = conn
        .query_row("SELECT name, page, pack_id, npc FROM crafting_systems", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .unwrap();
    assert_eq!(
        (name.as_str(), page.as_str(), pack, npc.as_deref()),
        ("Heroic Green Steel", "https://ddowiki.com/page/Green_Steel_items", None, Some("Altar of Invasion"))
    );
    assert_eq!(strings(&conn, "SELECT family FROM crafting_system_families"), ["Greensteel_Heroic"]);
    assert_eq!(
        strings(&conn, "SELECT name || '|' || tier || '|' || COALESCE(bind, '') FROM crafting_ingredients ORDER BY id"),
        [
            "Small Shard of Power|heroic|Bound to Account",
            "Small Focus of Earth|heroic|",
            "Small Focus of Fire|heroic|",
            "Shroud Commendation|any|Unbound"
        ]
    );
    assert_eq!(
        recipe_rows(&conn),
        vec![
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
    let fortitude_augments = strings(
        &conn,
        "SELECT a.name || '|' || a.description FROM crafting_recipe_augments ra JOIN augments a ON a.id = ra.augment_id
           JOIN crafting_recipes r ON r.id = ra.recipe_id WHERE r.option = '+5 Fortitude Save' ORDER BY a.id",
    );
    assert_eq!(fortitude_augments.len(), 2, "every augment of that name in the family: {fortitude_augments:?}");
    assert!(
        fortitude_augments[0].contains("Earth:Opposition") && fortitude_augments[1].contains("Negative:Opposition")
    );
    assert_eq!(
        strings(
            &conn,
            "SELECT r.option || '|' || i.name || '|' || ri.quantity FROM crafting_recipe_ingredients ri
               JOIN crafting_recipes r ON r.id = ri.recipe_id JOIN crafting_ingredients i ON i.id = ri.ingredient_id
              ORDER BY r.sort_order, i.id"
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
    let report = build_crafting_with(|s| s.replace("npc = ", "pack = \"Free to Play\"\nnpc = ")).unwrap();
    assert_eq!(report.wiki_crafting_systems, 1);
}

#[test]
fn crafting_never_adds_innate_item_bonuses_or_augments() {
    let (without, _) = built_with(&WikiOverrides::default());
    let (with, _) = built_with(&WikiOverrides::from_dir(&fixtures().join("wiki")).unwrap());
    for table in ["augments", "item_bonuses", "items", "adventure_packs", "augment_slot_types"] {
        let count =
            |c: &Connection| c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(count(&with), count(&without), "{table}");
    }
}
