mod fixture;

use ddo_etl::build::{build_database, ProbableDuplicateWikiEntry, SupersededWikiEntry};
use ddo_etl::corrections::Corrections;
use ddo_etl::wiki::{
    CraftingSystem, DescribedListedDrop, ListedDrop, QuestLoot, RareDrop, RareDropInChest, WikiItemEffect,
    WikiOverrides,
};
use ddo_model::DatasetVersion;
use rusqlite::Connection;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture_dataset_version() -> DatasetVersion {
    DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-09-20T00:00:00Z".into() }
}

fn authored_wiki(sql: &str) -> WikiOverrides {
    let ours = fixture::database(|_| {});
    ours.execute_batch(sql).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    ddo_etl::tree::export_database(&ours, scratch.path()).unwrap();
    let source = fixtures_dir().join("DataFiles");
    ddo_etl::icons::export_icons(&source, &scratch.path().join("icons")).unwrap();
    for missing in ddo_etl::icons::complete_referenced_icons(scratch.path()).unwrap().missing_icons {
        let path = scratch.path().join("icons").join(missing);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::copy(source.join("FeatImages/PowerAttack.png"), path).unwrap();
    }
    ddo_etl::icons::place_enhancement_icons_beside_trees(scratch.path()).unwrap();
    let mut rebuilt = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(scratch.path(), &mut rebuilt).unwrap();
    WikiOverrides::from_database(&rebuilt).unwrap()
}

fn fixture_wiki() -> WikiOverrides {
    WikiOverrides::from_database(&fixture::database(|_| {})).unwrap()
}

fn crafting_wiki(edit: impl FnOnce(&mut Vec<CraftingSystem>)) -> WikiOverrides {
    let mut crafting_systems = fixture_wiki().crafting_systems;
    edit(&mut crafting_systems);
    WikiOverrides { crafting_systems, ..WikiOverrides::default() }
}

fn crafting_system<'a>(systems: &'a mut [CraftingSystem], name: &str) -> &'a mut CraftingSystem {
    systems.iter_mut().find(|system| system.name == name).unwrap()
}

type LootRow<'a> = (&'a str, &'a str, &'a str, &'a str, Option<&'a str>, bool);

fn loot_wiki(drops: &[LootRow]) -> WikiOverrides {
    let mut quest_loot: Vec<QuestLoot> = Vec::new();
    for (quest, kind, name, loot_type, chest, rare) in drops {
        let position = quest_loot.iter().position(|entry| entry.name == *quest).unwrap_or_else(|| {
            quest_loot.push(QuestLoot {
                name: (*quest).into(),
                rare: Vec::new(),
                rare_augments: Vec::new(),
                items: Vec::new(),
                augments: Vec::new(),
                file_name: format!("packs/test/quests/{}/quest.toml", ddo_etl::tree::slug(quest)),
            });
            quest_loot.len() - 1
        });
        let entry = &mut quest_loot[position];
        let listed = ListedDrop::Described(DescribedListedDrop {
            name: (*name).into(),
            loot_type: Some((*loot_type).into()),
            chest: chest.map(String::from),
        });
        let rare_drop = chest.map_or_else(
            || RareDrop::Named((*name).into()),
            |chest| RareDrop::InChest(RareDropInChest { name: (*name).into(), chest: chest.into() }),
        );
        let (listed_drops, rare_drops) = if *kind == "item" {
            (&mut entry.items, &mut entry.rare)
        } else {
            (&mut entry.augments, &mut entry.rare_augments)
        };
        listed_drops.push(listed);
        if *rare {
            rare_drops.push(rare_drop);
        }
    }
    WikiOverrides { quest_loot, ..WikiOverrides::default() }
}

fn build_report_with(wiki: &WikiOverrides) -> Result<ddo_etl::build::BuildReport, String> {
    let mut db = Connection::open_in_memory().unwrap();
    build_database(
        &fixtures_dir().join("DataFiles"),
        wiki,
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .map_err(|e| format!("{e:#}"))
}

#[test]
fn reader_emits_a_quests_rare_drops_by_name_and_by_chest() {
    let wiki = WikiOverrides::from_database(&fixture::database(|_| {})).unwrap();
    let entry = wiki.quest_loot.iter().find(|quest| quest.name == "Book Burning").unwrap();
    assert_eq!(entry.name, "Book Burning");
    assert!(entry.file_name.ends_with("/book-burning/quest.toml"));
    assert_eq!(entry.rare.iter().map(RareDrop::name).collect::<Vec<_>>(), ["Buckler of the Golden Age"]);
    assert_eq!(
        entry.rare_augments.iter().map(RareDrop::name).collect::<Vec<_>>(),
        ["Lunar Gem of Magical Protection (Heroic)"]
    );
}

fn quest_augment_loot_row(db: &Connection, quest: &str, augment: &str) -> Option<(String, bool, Option<String>)> {
    db.query_row(
        "SELECT qal.loot_type, qal.is_rare, qal.chest FROM sources qal
           JOIN quests q ON q.id = qal.quest_id JOIN augments a ON a.id = qal.augment_id
          WHERE q.name = ?1 AND a.name = ?2",
        [quest, augment],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .ok()
}

#[test]
fn marks_rare_augments_on_the_links_maetrims_description_made() {
    let (db, report) = built_db_with(&WikiOverrides::from_database(&fixture::database(|_| {})).unwrap());
    assert_eq!(
        quest_augment_loot_row(&db, "Book Burning", "Lunar Gem of Magical Protection (Heroic)"),
        Some(("chest".into(), true, Some("end chest".into())))
    );
    assert_eq!((report.wiki_rare_augment_drop_count, report.wiki_added_quest_augment_loot_link_count), (2, 0));
}

#[test]
fn adds_a_chest_link_for_a_rare_augment_and_fills_only_a_chest_his_text_left_blank() {
    let wiki = loot_wiki(&[
        ("Book Burning", "item", "Buckler of the Golden Age", "chest", Some("optional chest"), true),
        ("Book Burning", "augment", "Lunar Gem of Evocation (Heroic)", "chest", Some("end chest"), true),
    ]);
    let (db, report) = built_db_with(&wiki);
    assert_eq!(
        quest_augment_loot_row(&db, "Book Burning", "Lunar Gem of Evocation (Heroic)"),
        Some(("chest".into(), true, Some("end chest".into())))
    );
    let buckler_chest: Option<String> = db
        .query_row(
            "SELECT ql.chest FROM sources ql JOIN quests q ON q.id = ql.quest_id JOIN items i ON i.id = ql.item_id
              WHERE q.name = 'Book Burning' AND i.name = 'Buckler of the Golden Age'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(buckler_chest.as_deref(), Some("end chest"), "his text's chest wins");
    assert_eq!((report.wiki_rare_augment_drop_count, report.wiki_added_quest_augment_loot_link_count), (1, 1));
}

#[test]
fn build_fails_naming_a_rare_augment_absent_from_the_augments_table() {
    let wiki = loot_wiki(&[("Book Burning", "augment", "Lunar Gem of Missing Things", "chest", None, true)]);
    let error = build_report_with(&wiki).unwrap_err();
    assert!(error.contains("Lunar Gem of Missing Things") && error.contains("Book Burning"), "{error}");
}

#[test]
fn build_fails_naming_a_quest_absent_from_the_quests_table() {
    let wiki = loot_wiki(&[("The Missing Quest", "item", "Docent of Defiance", "chest", None, true)]);
    let error = build_report_with(&wiki).unwrap_err();
    assert!(error.contains("The Missing Quest"), "{error}");
}

#[test]
fn build_fails_naming_an_item_absent_from_the_items_table() {
    let wiki = loot_wiki(&[("Book Burning", "item", "Buckler of the Missing Age", "chest", None, true)]);
    let error = build_report_with(&wiki).unwrap_err();
    assert!(error.contains("Buckler of the Missing Age") && error.contains("Book Burning"), "{error}");
}

#[test]
fn build_reports_the_wiki_entries_it_applied() {
    let wiki = WikiOverrides::from_database(&fixture::database(|_| {})).unwrap();
    let report = build_report_with(&wiki).unwrap();
    assert_eq!(report.wiki_quest_loot_entry_count, 4);
    assert_eq!(report.wiki_rare_drop_count, 1);
}

fn built_db_with(wiki: &WikiOverrides) -> (Connection, ddo_etl::build::BuildReport) {
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        &fixtures_dir().join("DataFiles"),
        wiki,
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    (db, report)
}

fn quest_loot_row(db: &Connection, quest: &str, item: &str) -> Option<(String, bool)> {
    db.query_row(
        "SELECT ql.loot_type, ql.is_rare FROM sources ql JOIN quests q ON q.id = ql.quest_id JOIN items i ON i.id = ql.item_id
          WHERE q.name = ?1 AND i.name = ?2",
        [quest, item],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .ok()
}

#[test]
fn marks_rare_drops_on_the_links_maetrims_drop_text_made() {
    let (db, report) = built_db_with(&WikiOverrides::from_database(&fixture::database(|_| {})).unwrap());
    assert_eq!(quest_loot_row(&db, "Book Burning", "Buckler of the Golden Age"), Some(("chest".into(), true)));
    assert_eq!(quest_loot_row(&db, "The Cursed Crypt", "Docent of Defiance"), Some(("chest".into(), false)));
    assert_eq!(
        report.wiki_added_quest_loot_link_count, 2,
        "Maetrim's drop text already links the buckler; the two added links are the wiki axe's quests"
    );
}

#[test]
fn adds_a_chest_link_for_a_rare_drop_and_never_changes_maetrims_loot_type() {
    let wiki = loot_wiki(&[
        ("The Grotto", "item", "Docent of Defiance", "chest", None, true),
        ("Caught in the Web", "item", "Sireth, Spear of the Sky", "raid", None, true),
    ]);
    let (db, report) = built_db_with(&wiki);
    assert_eq!(quest_loot_row(&db, "The Grotto", "Docent of Defiance"), Some(("chest".into(), true)));
    assert_eq!(quest_loot_row(&db, "The Cursed Crypt", "Docent of Defiance"), Some(("chest".into(), false)));
    assert_eq!(quest_loot_row(&db, "Caught in the Web", "Sireth, Spear of the Sky"), Some(("raid".into(), true)));
    assert_eq!(report.wiki_added_quest_loot_link_count, 1);
    assert_eq!(report.wiki_rare_drop_count, 2);
}

const WIKI_QUEST: &str = "Ghosts of Perdition";

fn new_quest_wiki(name: &str) -> WikiOverrides {
    let quoted_name = name.replace('\'', "''");
    let wiki = authored_wiki(&format!(
        "UPDATE quests SET name='{quoted_name}',slug='{}' WHERE name='Ghosts of Perdition';",
        ddo_etl::tree::slug(name)
    ));
    WikiOverrides {
        quests: wiki.quests.into_iter().filter(|quest| quest.name == name).collect(),
        ..WikiOverrides::default()
    }
}

#[test]
fn links_maetrims_items_to_a_wiki_quest_their_drop_text_names() {
    let (db, report) = built_db_with_fixture_wiki();
    let drop_text_links: Vec<(String, String, bool)> = db
        .prepare(
            "SELECT i.name, ql.loot_type, ql.is_rare FROM sources ql
               JOIN quests q ON q.id = ql.quest_id JOIN items i ON i.id = ql.item_id
              WHERE q.name = ?1 AND i.provenance = 'maetrim' ORDER BY i.name",
        )
        .unwrap()
        .query_map([WIKI_QUEST], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        drop_text_links,
        [
            ("Argenti's Armor".to_string(), "chest".to_string(), false),
            ("Epic Ethereal Bracers".to_string(), "chest".to_string(), false),
        ]
    );
    assert_eq!(report.drop_text_wiki_quest_link_count, 2);
    let (_, without_report) = built_db_with(&WikiOverrides::default());
    assert_eq!(without_report.drop_text_wiki_quest_link_count, 0);
    assert_eq!(report.quest_loot_link_count, without_report.quest_loot_link_count + 2);
}

type CreatedQuestRow = (String, Option<String>, Option<i64>, Option<i64>, Option<i64>, bool, String, String);

#[test]
fn creates_a_wiki_quest_maetrims_files_lack() {
    let (db, report) = built_db_with_fixture_wiki();
    let quest_row: CreatedQuestRow = db
        .query_row(
            "SELECT ap.name, p.name, q.level, q.epic_level, q.favor, q.is_raid, q.difficulties, q.provenance
               FROM quests q JOIN adventure_packs ap ON ap.id = q.pack_id LEFT JOIN patrons p ON p.id = q.patron_id
              WHERE q.name = ?1",
            [WIKI_QUEST],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
        )
        .unwrap();
    assert_eq!(
        quest_row,
        (
            "Chill of Ravenloft".into(),
            Some("The Coin Lords".into()),
            Some(32),
            Some(33),
            Some(150),
            false,
            "[\"normal\",\"hard\",\"elite\",\"reaper\"]".into(),
            "wiki".into()
        )
    );
    assert_eq!(report.wiki_quest_created_count, 1);
    let wiki_quest_count: i64 =
        db.query_row("SELECT COUNT(*) FROM quests WHERE provenance = 'wiki'", [], |r| r.get(0)).unwrap();
    assert_eq!(wiki_quest_count, 1);
    let maetrim_quest_count: i64 =
        db.query_row("SELECT COUNT(*) FROM quests WHERE provenance = 'maetrim'", [], |r| r.get(0)).unwrap();
    assert_eq!(maetrim_quest_count as usize, report.quest_count + report.challenge_count);
}

#[test]
fn skips_creating_a_quest_maetrim_already_carries() {
    let wiki = authored_wiki("UPDATE quests SET provenance='wiki',level=99,favor=999 WHERE name='The Grotto';");
    let (db, report) = built_db_with(&wiki);
    assert_eq!(report.wiki_quest_superseded_count, 1);
    assert_eq!(
        report.superseded_wiki_quests,
        [SupersededWikiEntry {
            name: "The Grotto".into(),
            file_name: "packs/free-to-play/quests/the-grotto/quest.toml".into()
        }]
    );
    let (level, favor, provenance): (Option<i64>, Option<i64>, String) = db
        .query_row("SELECT level, favor, provenance FROM quests WHERE name = 'The Grotto'", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert_ne!((level, favor), (Some(99), Some(999)), "his level and favor stand");
    assert_eq!(provenance, "maetrim");
}

#[test]
fn creates_a_probable_duplicate_wiki_quest_and_reports_both_names() {
    let wiki = new_quest_wiki("The Chrono-scope");
    let (db, report) = built_db_with(&wiki);
    let created_count: i64 = db
        .query_row("SELECT COUNT(*) FROM quests WHERE name = 'The Chrono-scope' AND provenance = 'wiki'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(created_count, 1);
    assert_eq!((report.wiki_quest_created_count, report.wiki_quest_probable_duplicate_count), (1, 1));
    assert_eq!(
        report.probable_duplicate_wiki_quests,
        [ProbableDuplicateWikiEntry { name: "The Chrono-scope".into(), maetrim_name: "The Chronoscope".into() }]
    );
}

#[test]
fn a_wiki_quest_with_a_new_name_is_no_probable_duplicate() {
    let (_, report) = built_db_with_fixture_wiki();
    assert_eq!(report.wiki_quest_probable_duplicate_count, 0);
    assert!(report.probable_duplicate_wiki_quests.is_empty());
}

#[test]
fn build_fails_naming_a_wiki_quest_pack_or_patron_absent_from_maetrims_files() {
    let owner = fixture_wiki().quests.into_iter().find(|quest| quest.name == WIKI_QUEST).unwrap();
    for (field, missing) in [("pack", "No Such Pack"), ("patron", "No Such Patron")] {
        let mut quest = owner.clone();
        if field == "pack" {
            quest.pack = Some(missing.into());
        } else {
            quest.patron = Some(missing.into());
        }
        let error = build_report_with(&WikiOverrides { quests: vec![quest], ..WikiOverrides::default() }).unwrap_err();
        assert!(
            error.contains(&owner.file_name)
                && error.contains(WIKI_QUEST)
                && error.contains(field)
                && error.contains(missing),
            "{error}"
        );
    }
}

#[test]
fn wiki_quests_never_change_maetrims_quests() {
    let (without, _) = built_db_with(&WikiOverrides::default());
    let (with, _) = built_db_with_fixture_wiki();
    let maetrim_quests = |db: &Connection| {
        string_column(
            db,
            "SELECT name || '|' || COALESCE(level, '') || '|' || COALESCE(favor, '') || '|' || difficulties
               FROM quests WHERE provenance = 'maetrim' ORDER BY name",
        )
    };
    assert_eq!(maetrim_quests(&with), maetrim_quests(&without));
}

#[test]
fn reader_emits_crafting_systems_with_their_ingredients_and_recipes() {
    let wiki = WikiOverrides::from_database(&fixture::database(|_| {})).unwrap();
    assert_eq!(wiki.crafting_systems.len(), 4);
    let system = wiki.crafting_systems.iter().find(|system| system.name == "Heroic Green Steel").unwrap();
    assert_eq!(system.name, "Heroic Green Steel");
    assert_eq!(system.wiki_url, "https://ddowiki.com/page/Green_Steel_items");
    assert_eq!(system.families, vec!["Greensteel_Heroic".to_string()]);
    assert_eq!(system.npc.as_deref(), Some("Altar of Invasion"));
    assert!(system.pack.is_none());
    assert_eq!(system.ingredients.len(), 4);
    assert_eq!(system.ingredients[0].bind.as_deref(), Some("Bound to Account"));
    assert_eq!(system.recipes.len(), 3);
    assert_eq!(system.recipes[0].cost[1].ingredient, "Small Focus of Earth");
    assert_eq!(system.recipes[2].augments, Vec::<String>::new());
    let upgrade_system = wiki.crafting_systems.iter().find(|system| system.name == "Test Upgrade Altar").unwrap();
    assert_eq!(upgrade_system.name, "Test Upgrade Altar");
    assert!(upgrade_system.families.is_empty());
    assert_eq!(upgrade_system.recipes[0].grants_slot.as_deref(), Some("upgrade: tier 2"));
}

#[test]
fn build_fails_naming_an_unknown_crafting_family() {
    let wiki = crafting_wiki(|systems| {
        crafting_system(systems, "Heroic Green Steel").families = vec!["Greensteel_Mythic".into()];
    });
    let error = build_report_with(&wiki).unwrap_err();
    assert!(error.contains("Heroic Green Steel") && error.contains("Greensteel_Mythic"), "{error}");
}

#[test]
fn build_fails_naming_an_augment_absent_from_the_systems_families() {
    let wiki = crafting_wiki(|systems| {
        let recipes = &mut crafting_system(systems, "Heroic Green Steel").recipes;
        recipes.iter_mut().find(|recipe| recipe.option == "Minor Fire Guard").unwrap().augments =
            vec!["Silverscale".into()];
    });
    let error = build_report_with(&wiki).unwrap_err();
    assert!(
        error.contains("Heroic Green Steel") && error.contains("Silverscale") && error.contains("Greensteel_Heroic"),
        "{error}"
    );
}

#[test]
fn build_fails_naming_an_unknown_slot_label() {
    let wiki = crafting_wiki(|systems| {
        let recipes = &mut crafting_system(systems, "Heroic Green Steel").recipes;
        recipes[0].slot = Some("crafting: accessory invasions".into());
    });
    let error = build_report_with(&wiki).unwrap_err();
    assert!(error.contains("Heroic Green Steel") && error.contains("crafting: accessory invasions"), "{error}");
}

#[test]
fn build_fails_naming_an_unknown_grants_slot_label() {
    let wiki = crafting_wiki(|systems| {
        let recipes = &mut crafting_system(systems, "Test Upgrade Altar").recipes;
        recipes[0].grants_slot = Some("upgrade: tier 9".into());
    });
    let error = build_report_with(&wiki).unwrap_err();
    assert!(
        error.contains("Test Upgrade Altar")
            && error.contains("Test tier 2 upgrade")
            && error.contains("upgrade: tier 9"),
        "{error}"
    );
}

#[test]
fn build_fails_naming_an_unknown_pack() {
    let wiki = crafting_wiki(|systems| {
        crafting_system(systems, "Heroic Green Steel").pack = Some("The Shroud Pack".into());
    });
    let error = build_report_with(&wiki).unwrap_err();
    assert!(error.contains("Heroic Green Steel") && error.contains("The Shroud Pack"), "{error}");
}

type RecipeRow = (String, Option<String>, String, Option<String>, i64);

fn recipe_rows(db: &Connection) -> Vec<RecipeRow> {
    let mut statement = db
        .prepare(
            "SELECT r.tier, t.label, r.option, r.note, r.sort_order FROM crafting_recipes r
               JOIN crafting_systems s ON s.id = r.system_id LEFT JOIN augment_slot_types t ON t.id = r.slot_id ORDER BY s.name, r.sort_order",
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
    let (db, report) = built_db_with(&WikiOverrides::from_database(&fixture::database(|_| {})).unwrap());
    assert_eq!(
        (report.wiki_crafting_system_count, report.wiki_crafting_recipe_count, report.wiki_crafting_ingredient_count),
        (4, 5, 5)
    );
    let (name, page, pack, npc): (String, String, Option<i64>, Option<String>) = db
        .query_row("SELECT name, page, pack_id, npc FROM crafting_systems WHERE name = 'Heroic Green Steel'", [], |r| {
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
            "SELECT name || '|' || tier || '|' || COALESCE(bind, '') FROM crafting_ingredients WHERE system_id = (SELECT id FROM crafting_systems WHERE name = 'Heroic Green Steel')
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
              WHERE r.system_id = (SELECT id FROM crafting_systems WHERE name = 'Heroic Green Steel') ORDER BY r.sort_order, i.id"
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
    let wiki = crafting_wiki(|systems| {
        crafting_system(systems, "Heroic Green Steel").pack = Some("Free to Play".into());
    });
    let report = build_report_with(&wiki).unwrap();
    assert_eq!(report.wiki_crafting_system_count, 4);
}

#[test]
fn merges_a_family_less_system_whose_recipes_grant_sockets() {
    let (db, _) = built_db_with(&WikiOverrides::from_database(&fixture::database(|_| {})).unwrap());
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
    let (with, _) = built_db_with(&WikiOverrides::from_database(&fixture::database(|_| {})).unwrap());
    for table in [
        "augments WHERE provenance = 'maetrim'",
        "item_effects JOIN items ON items.id = item_effects.item_id WHERE items.provenance = 'maetrim'",
        "items WHERE provenance = 'maetrim'",
        "adventure_packs",
        "augment_slot_types",
    ] {
        let count =
            |c: &Connection| c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(count(&with), count(&without), "{table}");
    }
}

fn edited_axe_wiki(sql: &str) -> WikiOverrides {
    authored_wiki(sql)
}

fn axe_effects_edited(edit: impl FnOnce(&mut Vec<WikiItemEffect>)) -> WikiOverrides {
    let mut wiki = WikiOverrides::from_database(&fixture::database(|_| {})).unwrap();
    edit(&mut wiki.items.iter_mut().find(|item| item.name == WIKI_AXE).unwrap().effects);
    wiki
}

const WIKI_AXE: &str = "Battle Axe of the Oozing Hunger";

#[test]
fn reads_wiki_items_from_items_files() {
    let wiki = WikiOverrides::from_database(&fixture::database(|_| {})).unwrap();
    assert_eq!(wiki.items.len(), 1);
    let axe = &wiki.items[0];
    assert_eq!(axe.name, WIKI_AXE);
    assert!(axe.file_name.ends_with("/battle-axe-of-the-oozing-hunger.toml"));
    assert_eq!(
        (axe.slot.as_str(), axe.category.as_str(), axe.item_type.as_deref()),
        ("Main Hand", "Weapon", Some("Battle Axe"))
    );
    assert_eq!((axe.minimum_level, axe.enhancement_bonus), (29, Some(15)));
    let listing_quests: Vec<(&str, Option<&str>)> = wiki
        .quest_loot
        .iter()
        .flat_map(|quest| {
            quest.items.iter().filter(|drop| drop.name() == WIKI_AXE).map(|drop| {
                (
                    quest.name.as_str(),
                    match drop {
                        ListedDrop::Described(drop) => drop.loot_type.as_deref(),
                        ListedDrop::Named(_) => None,
                    },
                )
            })
        })
        .collect();
    assert_eq!(listing_quests, [("The Grotto", Some("chest")), ("Ghosts of Perdition", Some("reward"))]);
    assert_eq!(axe.augment_slots, ["red", "colorless"]);
    assert_eq!(axe.bonuses[1].stat.as_deref(), Some("Doublestrike"));
    assert_eq!(axe.effects[1].name, "Ethereal");
    let weapon = axe.weapon.as_ref().unwrap();
    assert_eq!(
        (weapon.damage_dice_count, weapon.damage_dice_sides, weapon.damage_multiplier),
        (Some(1), Some(8), Some(3.0))
    );
    assert_eq!(weapon.dr_bypass, ["Magic", "Slash"]);
    assert!(axe.armor.is_none());
}

#[test]
fn flags_a_wiki_item_with_a_legacy_name_as_legacy() {
    let legacy_name = format!("{WIKI_AXE} (legacy)");
    let wiki = edited_axe_wiki(&rename_item_sql(WIKI_AXE, &legacy_name));
    let (db, report) = built_db_with(&wiki);
    let is_legacy: bool =
        db.query_row("SELECT is_legacy FROM items WHERE name = ?1", [&legacy_name], |r| r.get(0)).unwrap();
    assert!(is_legacy);
    assert_eq!(report.legacy_item_count, 4, "the wiki axe and the three legacy fixture items");
}

fn rename_item_sql(from: &str, to: &str) -> String {
    format!(
        "UPDATE items SET name='{}',slug='{}' WHERE name='{}';",
        to.replace('\'', "''"),
        ddo_etl::tree::slug(to),
        from.replace('\'', "''")
    )
}

fn built_db_with_fixture_wiki() -> (Connection, ddo_etl::build::BuildReport) {
    built_db_with(&WikiOverrides::from_database(&fixture::database(|_| {})).unwrap())
}

fn item_count(db: &Connection, sql_condition: &str) -> i64 {
    db.query_row(&format!("SELECT COUNT(*) FROM items WHERE {sql_condition}"), [], |r| r.get(0)).unwrap()
}

#[test]
fn drops_a_wiki_item_maetrim_already_carries_and_reports_it() {
    let (without, _) = built_db_with(&WikiOverrides::default());
    let wiki = authored_wiki("UPDATE items SET provenance='wiki',minimum_level=99 WHERE name='Five Rings';");
    let (with, report) = built_db_with(&wiki);
    assert_eq!(item_count(&with, "name = 'Five Rings'"), 1);
    assert_eq!(item_count(&with, "name = 'Five Rings' AND provenance = 'maetrim'"), 1);
    let bonus_names = |db: &Connection| {
        string_column(
            db,
            "SELECT s.name || ' +' || ob.amount
             FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id JOIN items i ON i.id = ob.owner_id
             WHERE ob.owner_kind = 'item' AND i.name = 'Five Rings' ORDER BY ob.effect_link_order, ob.stat_id",
        )
    };
    assert_eq!(bonus_names(&with), bonus_names(&without));
    assert_eq!(report.wiki_item_superseded_count, 1);
    assert_eq!(
        report.superseded_wiki_items,
        [SupersededWikiEntry {
            name: "Five Rings".into(),
            file_name: "packs/masterminds-of-sharn/gear/five-rings.toml".into()
        }]
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
                    i.wiki_url, i.provenance
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
            "SELECT s.name || '|' || bt.name || '|' || ob.amount
               FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
               JOIN bonus_types bt ON bt.id = ob.bonus_type_id
              WHERE ob.owner_kind = 'item' AND ob.owner_id = ?item ORDER BY ob.effect_link_order, ob.stat_id"
        ),
        [
            "Strength|Enhancement|15",
            "Doublestrike|Insight|5",
            "Fire Spell Power|Equipment|49",
            "Cold Spell Power|Equipment|49",
            "Electric Spell Power|Equipment|49",
            "Acid Spell Power|Equipment|49",
            "Sonic Spell Power|Equipment|49",
            "Light Spell Power|Equipment|49",
            "Force Spell Power|Equipment|49",
            "Negative Spell Power|Equipment|49",
            "Positive Spell Power|Equipment|49",
            "Repair Spell Power|Equipment|49",
            "Poison Spell Power|Equipment|49",
            "Chaos Spell Power|Equipment|49",
            "Good Spell Power|Equipment|49",
            "Evil Spell Power|Equipment|49",
            "Law Spell Power|Equipment|49",
        ]
    );
    let effect_lines = item_column(
        "SELECT e.name || '|' || COALESCE(e.verbose_name_template || ': ' || e.description_template, '')
           FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
           WHERE ie.item_id = ?item AND e.is_stat = 0 AND NOT EXISTS
             (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id) ORDER BY ie.sort_order",
    );
    assert_eq!(effect_lines[0], "Test Oozing Hunger|Test Oozing Hunger: Test description: on hit, the target oozes.");
    assert!(
        effect_lines[1].starts_with("Ethereal|Ethereal: Equipping this item"),
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
    assert_eq!(quest_loot_row(&db, WIKI_QUEST, WIKI_AXE), Some(("reward".into(), false)), "links the wiki quest");
    assert_eq!(report.wiki_item_written_count, 1);
    assert_eq!(item_count(&db, "provenance = 'wiki'"), report.wiki_item_written_count as i64);
}

#[test]
fn a_wiki_item_effect_with_a_value_reuses_the_written_buff_family() {
    let wiki = axe_effects_edited(|effects| {
        let oozing = effects.iter_mut().find(|effect| effect.name == "Test Oozing Hunger").unwrap();
        *oozing = WikiItemEffect {
            name: "Command".into(),
            description: Some("The wiki describes this command bonus.".into()),
            value: Some(3),
            target: None,
        };
    });
    let (db, _) = built_db_with(&wiki);
    let identity_for = |item_name: &str| -> (i64, Option<i64>, Option<i64>) {
        db.query_row(
            "SELECT e.id, link.value, link.value2 FROM items i
             JOIN item_effects link ON link.item_id = i.id
             JOIN effects e ON e.id = link.effect_id
             WHERE i.name = ?1 AND e.name = 'Command'",
            [item_name],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap()
    };
    let maetrim_family = identity_for("Grudgebearer's Plate").0;
    assert_eq!(identity_for(WIKI_AXE), (maetrim_family, Some(3), None));
    let command_is_stat: i64 =
        db.query_row("SELECT is_stat FROM effects WHERE id = ?1", [maetrim_family], |row| row.get(0)).unwrap();
    assert_eq!(command_is_stat, 0);
}

#[test]
fn writes_a_probable_duplicate_of_a_maetrim_item_and_reports_both_names() {
    let wiki = edited_axe_wiki(&rename_item_sql(WIKI_AXE, "Argentis Armor (Level 12)"));
    let (db, report) = built_db_with(&wiki);
    assert_eq!(item_count(&db, "name = 'Argentis Armor (Level 12)' AND provenance = 'wiki'"), 1);
    assert_eq!((report.wiki_item_written_count, report.wiki_item_probable_duplicate_count), (1, 1));
    assert_eq!(
        report.probable_duplicate_wiki_items,
        [ProbableDuplicateWikiEntry {
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

type ItemValueEdit = (&'static str, &'static str, fn(&mut ddo_etl::wiki::WikiItem));

#[test]
fn build_fails_naming_a_wiki_item_value_absent_from_maetrims_files() {
    let edits: [ItemValueEdit; 3] = [
        ("material", "Byeshk", |item| item.material = Some("Byeshk".into())),
        ("set", "Inevitable Imbalance", |item| item.set = Some("Inevitable Imbalance".into())),
        ("augment_slots", "colourless", |item| item.augment_slots[1] = "colourless".into()),
    ];
    for (field, bad_value, edit) in edits {
        let mut wiki = fixture_wiki();
        edit(wiki.items.iter_mut().find(|item| item.name == WIKI_AXE).unwrap());
        let error = build_report_with(&wiki).unwrap_err();
        assert!(
            error.contains(WIKI_AXE)
                && error.contains("battle-axe-of-the-oozing-hunger.toml")
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
    assert_eq!(item_count(&with, "provenance = 'maetrim'"), item_count(&without, "1"));
    assert_eq!(item_count(&without, "provenance = 'wiki'"), 0);
}

const RUNE_ARM_FILE: &str = "packs/shared/gear/test-rune-arm-of-the-oozing-hunger.toml";

const WIKI_RUNE_ARM: &str = r#"name = "Test Rune Arm of the Oozing Hunger"
is_wiki_only = true
drop_location = "Test source"
item_category = "Weapon"
item_type = "Rune Arm"
minimum_level = 29
slot = "Off Hand"

[[citation]]
url = "https://ddowiki.com/page/Item:Test"
read = "2026-09-29"

[[weapon]]
handedness = "Off-hand"
weapon_type = "Rune Arm"
"#;

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
    let wiki = WikiOverrides::from_database(&fixture::database(|root| {
        std::fs::write(root.join(RUNE_ARM_FILE), WIKI_RUNE_ARM).unwrap();
    }))
    .unwrap();
    let (db, report) = built_db_with(&wiki);
    assert_eq!(report.wiki_item_written_count, 2);
    let wiki_rune_arm = weapon_stats_columns(&db, "Test Rune Arm of the Oozing Hunger");
    assert_eq!(wiki_rune_arm, weapon_stats_columns(&db, "Acid Rune Arm"));
    assert_eq!(
        wiki_rune_arm,
        ("Rune Arm".into(), None, None, None, None, None, None, Some("Off-hand".into()), None, None)
    );
}

#[test]
fn reuses_maetrims_effect_whose_name_differs_only_by_case_spaces_and_hyphens() {
    let wiki = axe_effects_edited(|effects| {
        effects.push(WikiItemEffect {
            name: "Feather Falling".into(),
            description: Some("Test description: not his.".into()),
            value: None,
            target: None,
        });
        effects.push(WikiItemEffect {
            name: "rune arm imbue acid ii".into(),
            description: None,
            value: None,
            target: None,
        });
    });
    let (without, _) = built_db_with(&WikiOverrides::default());
    let (db, _) = built_db_with(&wiki);
    let effect_lines = string_column(
        &db,
        &format!(
            "SELECT e.name || '|' || COALESCE(e.verbose_name_template || ': ' || e.description_template, '')
             FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
             JOIN items i ON i.id = ie.item_id WHERE i.name = '{WIKI_AXE}'
             AND e.is_stat = 0 AND NOT EXISTS (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)
             ORDER BY ie.sort_order"
        ),
    );
    assert!(
        effect_lines[1].starts_with("Ethereal|Ethereal: Equipping"),
        "an exact match still reuses his: {effect_lines:?}"
    );
    assert!(effect_lines[2].starts_with("Feather Falling|Feather Falling: This item"), "{effect_lines:?}");
    assert_eq!(effect_lines[3], "Rune Arm Imbue - Acid II|");
    assert_eq!(non_bonus_effect_count(&db), non_bonus_effect_count(&without) + 1, "only Test Oozing Hunger is new");
}

#[test]
fn reuses_maetrims_effect_whose_name_differs_only_by_colons_commas_periods_and_apostrophes() {
    let wiki = axe_effects_edited(|effects| {
        for name in ["Maximum Charge Tier: III", "Rune Arm Imbue: Acid II", "Rune Arm Imbue - 'Acid', II."] {
            effects.push(WikiItemEffect { name: name.into(), description: None, value: None, target: None });
        }
    });
    let (without, _) = built_db_with(&WikiOverrides::default());
    let (db, _) = built_db_with(&wiki);
    let effect_names = string_column(
        &db,
        &format!(
            "SELECT e.name FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
               JOIN items i ON i.id = ie.item_id WHERE i.name = '{WIKI_AXE}'
               AND e.is_stat = 0 AND NOT EXISTS (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)
               ORDER BY ie.sort_order"
        ),
    );
    assert_eq!(effect_names[2..], ["Maximum Charge Tier III", "Rune Arm Imbue - Acid II"], "{effect_names:?}");
    assert_eq!(non_bonus_effect_count(&db), non_bonus_effect_count(&without) + 1, "only Test Oozing Hunger is new");
}

fn non_bonus_effect_count(db: &Connection) -> i64 {
    db.query_row(
        "SELECT COUNT(*) FROM effects e WHERE e.is_stat = 0 AND NOT EXISTS (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)",
        [],
        |r| r.get(0),
    )
    .unwrap()
}

fn quest_loot_link_rows(db: &Connection, quest: &str, item: &str) -> Vec<(String, bool, Option<String>)> {
    let mut statement = db
        .prepare(
            "SELECT ql.loot_type, ql.is_rare, ql.chest FROM sources ql JOIN quests q ON q.id = ql.quest_id
               JOIN items i ON i.id = ql.item_id WHERE q.name = ?1 AND i.name = ?2 ORDER BY ql.loot_type",
        )
        .unwrap();
    statement.query_map([quest, item], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
}

#[test]
fn adds_a_listed_item_link_of_its_loot_type_beside_the_one_his_text_made() {
    let wiki = loot_wiki(&[
        ("Book Burning", "item", "Docent of Defiance", "reward", None, false),
        ("Book Burning", "item", "Buckler of the Golden Age", "reward", None, false),
        ("Book Burning", "item", "Sireth, Spear of the Sky", "reward", None, true),
    ]);
    let (db, report) = built_db_with(&wiki);
    assert_eq!(quest_loot_link_rows(&db, "Book Burning", "Docent of Defiance"), [("reward".into(), false, None)]);
    assert_eq!(
        quest_loot_link_rows(&db, "Book Burning", "Buckler of the Golden Age"),
        [("chest".into(), true, Some("end chest".into())), ("reward".into(), false, None)],
        "his chest link keeps its chest and rarity, and the listed reward is a second link"
    );
    assert_eq!(
        quest_loot_link_rows(&db, "Book Burning", "Sireth, Spear of the Sky"),
        [("reward".into(), true, None)],
        "an item also listed in rare is rare, and rare adds no chest link beside a listed reward"
    );
    assert_eq!((report.wiki_loot_drop_count, report.wiki_added_quest_loot_link_count), (3, 3));
}

#[test]
fn adds_no_link_for_a_listed_drop_when_his_text_links_it_already() {
    let wiki = loot_wiki(&[("Book Burning", "item", "Buckler of the Golden Age", "chest", Some("end chest"), false)]);
    let (db, report) = built_db_with(&wiki);
    assert_eq!(
        quest_loot_link_rows(&db, "Book Burning", "Buckler of the Golden Age"),
        [("chest".into(), true, Some("end chest".into()))]
    );
    assert_eq!(report.wiki_added_quest_loot_link_count, 0);
}

#[test]
fn build_fails_naming_a_listed_item_absent_from_the_items_table() {
    let wiki = loot_wiki(&[("Book Burning", "item", "Docent of Missing Things", "chest", None, false)]);
    let error = build_report_with(&wiki).unwrap_err();
    assert!(error.contains("Docent of Missing Things") && error.contains("Book Burning"), "{error}");
}

#[test]
fn adds_a_listed_augment_link_that_is_not_rare() {
    let wiki = loot_wiki(&[
        ("Book Burning", "augment", "Lunar Gem of Evocation (Heroic)", "chest", Some("end chest"), false),
        ("Book Burning", "augment", "Lunar Gem of Magical Protection (Heroic)", "reward", None, false),
    ]);
    let (db, report) = built_db_with(&wiki);
    assert_eq!(
        quest_augment_loot_row(&db, "Book Burning", "Lunar Gem of Evocation (Heroic)"),
        Some(("chest".into(), false, Some("end chest".into())))
    );
    let magical_protection_links: Vec<(String, Option<String>)> = {
        let mut statement = db
            .prepare(
                "SELECT qal.loot_type, qal.chest FROM sources qal JOIN quests q ON q.id = qal.quest_id
                   JOIN augments a ON a.id = qal.augment_id
                  WHERE q.name = 'Book Burning' AND a.name = 'Lunar Gem of Magical Protection (Heroic)'
                  ORDER BY qal.loot_type",
            )
            .unwrap();
        statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
    };
    assert_eq!(
        magical_protection_links,
        [("chest".into(), Some("end chest".into())), ("reward".into(), None)],
        "his description's chest link stands beside the listed reward"
    );
    assert_eq!((report.wiki_loot_augment_drop_count, report.wiki_added_quest_augment_loot_link_count), (2, 2));
}

#[test]
fn build_fails_naming_a_listed_augment_absent_from_the_augments_table() {
    let wiki = loot_wiki(&[("Book Burning", "augment", "Lunar Gem of Missing Things", "chest", None, false)]);
    let error = build_report_with(&wiki).unwrap_err();
    assert!(error.contains("Lunar Gem of Missing Things") && error.contains("Book Burning"), "{error}");
}

#[test]
fn links_drop_text_to_his_quest_whose_epic_name_a_wiki_quest_also_has() {
    let wiki = new_quest_wiki("Return to Prison of the Planes");
    let (db, _) = built_db_with(&wiki);
    let linked_quests: Vec<String> = db
        .prepare(
            "SELECT q.name FROM sources s JOIN quests q ON q.id = s.quest_id JOIN items i ON i.id = s.item_id
              WHERE i.name = 'Kardin''s Eye (Level 24)' ORDER BY q.name",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(linked_quests, ["The Prison of the Planes"], "his quest's epic name wins over the wiki quest's name");
}

#[test]
fn reports_a_wiki_quest_named_as_his_quest_s_epic_version_as_a_probable_duplicate() {
    let wiki = new_quest_wiki("Return to Prison of the Planes");
    let (_, report) = built_db_with(&wiki);
    assert_eq!(
        report.probable_duplicate_wiki_quests,
        [ProbableDuplicateWikiEntry {
            name: "Return to Prison of the Planes".into(),
            maetrim_name: "The Prison of the Planes".into()
        }],
        "his The Prison of the Planes goes by that name in epic"
    );
}
