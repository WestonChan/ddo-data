use ddo_etl::build::{build_database, BuildReport};
use ddo_etl::corrections::Corrections;
use ddo_etl::diff::item_coverage;
use ddo_etl::wiki::WikiOverrides;
use ddo_etl::xml::{challenges, sentient_gems};
use ddo_model::DatasetVersion;
use rusqlite::{params, Connection};
use std::path::PathBuf;

fn data_files_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles")
}

fn fixture_dataset_version() -> DatasetVersion {
    DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-09-20T00:00:00Z".into() }
}

fn built_fixture_db() -> (Connection, BuildReport) {
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        &data_files_fixture_dir(),
        &WikiOverrides::from_dir(&data_files_fixture_dir().parent().unwrap().join("wiki")).unwrap(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .expect("build succeeds on fixtures");
    (db, report)
}

fn count(db: &Connection, sql: &str) -> i64 {
    db.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn item_id(db: &Connection, name: &str) -> i64 {
    db.query_row("SELECT id FROM items WHERE name = ?1", params![name], |r| r.get(0))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn builds_items_and_skips_cosmetics() {
    let (db, report) = built_fixture_db();
    assert_eq!(report.written_item_count, 21);
    assert_eq!(report.skipped_cosmetic_item_count, 1);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM items WHERE source = 'maetrim'"), 21);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM items WHERE name = '17th Anniversary Dark Helm'"), 0);
    let reason: String = db
        .query_row("SELECT reason FROM excluded_items WHERE name = '17th Anniversary Dark Helm'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(reason, "cosmetic-only slots");
    let (sha, schema_version): (String, i64) = db
        .query_row("SELECT upstream_sha, (SELECT version FROM schema_version) FROM dataset_version", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(sha, "31ef0201");
    assert_eq!(schema_version, ddo_model::SCHEMA_VERSION);
}

#[test]
fn writes_item_core_columns() {
    let (db, _) = built_fixture_db();
    let id = item_id(&db, "Sireth, Spear of the Sky");
    let (slot, category, item_type, minimum_level, enhancement_bonus, material, icon, accepts_sentience, drop): (String, String, String, i64, i64, String, String, bool, String) = db
        .query_row(
            "SELECT es.name, i.item_category, i.item_type, i.minimum_level, i.enhancement_bonus, m.name, i.icon, i.accepts_sentience, i.drop_location
               FROM items i JOIN equipment_slots es ON es.id = i.slot_id LEFT JOIN item_materials m ON m.id = i.material_id
              WHERE i.id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?)),
        )
        .unwrap();
    assert_eq!(slot, "Main Hand");
    assert_eq!(category, "Weapon");
    assert_eq!(item_type, "Quarterstaff");
    assert_eq!(minimum_level, 23);
    assert_eq!(enhancement_bonus, 7);
    assert_eq!(material, "Steel");
    assert_eq!(icon, "Quarterstaff_6a");
    assert!(accepts_sentience);
    assert_eq!(drop, "Caught in the Web, End Chest");

    let docent = item_id(&db, "Docent of Defiance");
    let race: String =
        db.query_row("SELECT race_required FROM items WHERE id = ?1", params![docent], |r| r.get(0)).unwrap();
    assert_eq!(race, "Construct");
    let material_name: String = db
        .query_row(
            "SELECT m.name FROM items i JOIN item_materials m ON m.id = i.material_id WHERE i.id = ?1",
            params![docent],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(material_name, "Gem", "'(material)' suffix is stripped");
    let wiki: String = db.query_row("SELECT wiki_url FROM items WHERE id = ?1", params![id], |r| r.get(0)).unwrap();
    assert_eq!(wiki, "https://ddowiki.com/page/Item:Sireth,_Spear_of_the_Sky");
}

#[test]
fn writes_weapon_stats_with_rendered_display_strings() {
    let (db, _) = built_fixture_db();
    let id = item_id(&db, "Sireth, Spear of the Sky");
    let (weapon_type, damage, critical, handedness, damage_multiplier, critical_threat_range): (
        String,
        String,
        String,
        String,
        f64,
        i64,
    ) = db
        .query_row(
            "SELECT wt.name, s.damage, s.critical, s.handedness, s.damage_multiplier, s.critical_threat_range
               FROM item_weapon_stats s JOIN weapon_types wt ON wt.id = s.weapon_type_id WHERE s.item_id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .unwrap();
    assert_eq!(weapon_type, "Quarterstaff");
    assert_eq!(damage, "3.6[1d10] + 7 Good, Magic, Pierce, Slash");
    assert_eq!(critical, "16-20 / x2");
    assert_eq!(handedness, "Two-handed");
    assert_eq!(damage_multiplier, 3.6);
    assert_eq!(critical_threat_range, 5);
    assert_eq!(count(&db, &format!("SELECT COUNT(*) FROM item_dr_bypass WHERE item_id = {id}")), 4);

    let axe = item_id(&db, "+3 Combustion Scorched Battle Axe");
    let (damage, critical, handedness): (String, String, String) = db
        .query_row("SELECT damage, critical, handedness FROM item_weapon_stats WHERE item_id = ?1", params![axe], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert_eq!(damage, "1[1d8] + 3 Magic, Slash");
    assert_eq!(critical, "20 / x3");
    assert_eq!(handedness, "One-handed");

    let shield = item_id(&db, "+1 Starter Heavy Steel Shield");
    let (category, armor_type, handedness): (String, String, String) = db
        .query_row(
            "SELECT i.item_category, a.armor_type, w.handedness FROM items i JOIN item_armor_stats a ON a.item_id = i.id JOIN item_weapon_stats w ON w.item_id = i.id WHERE i.id = ?1",
            params![shield],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((category.as_str(), armor_type.as_str(), handedness.as_str()), ("Shield", "Shield", "Off-hand"));
}

#[test]
fn writes_armor_stats() {
    let (db, _) = built_fixture_db();
    let docent = item_id(&db, "Docent of Defiance");
    let (armor_type, bonus, mithral, adamantine): (String, i64, i64, i64) = db
        .query_row(
            "SELECT armor_type, armor_bonus, mithral_body, adamantine_body FROM item_armor_stats WHERE item_id = ?1",
            params![docent],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!((armor_type.as_str(), bonus, mithral, adamantine), ("Docent", -3, 5, 12));
    let heavy = item_id(&db, "Argenti's Armor");
    let armor_type: String = db
        .query_row("SELECT armor_type FROM item_armor_stats WHERE item_id = ?1", params![heavy], |r| r.get(0))
        .unwrap();
    assert_eq!(armor_type, "Heavy");
}

#[test]
fn splits_buffs_into_bonuses_and_effects() {
    let (db, _) = built_fixture_db();
    let cloak = item_id(&db, "Legendary Cloak of Winter");
    let bonuses: Vec<(String, String, Option<String>, i64)> = db
        .prepare(
            "SELECT b.name, s.name, bt.name, ib.sort_order FROM item_bonuses ib JOIN bonuses b ON b.id = ib.bonus_id
               JOIN stats s ON s.id = b.stat_id LEFT JOIN bonus_types bt ON bt.id = b.bonus_type_id
              WHERE ib.item_id = ?1 ORDER BY ib.sort_order",
        )
        .unwrap()
        .query_map(params![cloak], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        bonuses,
        vec![
            ("Cold Absorption +34".to_string(), "Cold Absorption".to_string(), Some("Enhancement".to_string()), 0),
            ("Hit Points +50".to_string(), "Hit Points".to_string(), Some("Enhancement".to_string()), 1),
        ]
    );
    let effects: Vec<(String, Option<i64>, Option<String>)> = db
        .prepare("SELECT e.name, ie.value, e.description FROM item_effects ie JOIN effects e ON e.id = ie.effect_id WHERE ie.item_id = ?1 ORDER BY ie.sort_order")
        .unwrap()
        .query_map(params![cloak], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(effects.len(), 2);
    assert_eq!(effects[0].0, "Legendary Ice Barrier");
    assert_eq!((effects[1].0.as_str(), effects[1].1), ("Lifesealed", Some(34)));
    assert!(effects[0].2.as_deref().unwrap_or("").len() > 10, "effects carry ItemBuffs.xml text");

    let duplicate_bonus_count = count(&db, "SELECT COUNT(*) FROM bonuses WHERE name = 'Fire Spell Power +54'");
    assert_eq!(duplicate_bonus_count, 1);
    let description: String =
        db.query_row("SELECT description FROM bonuses WHERE name = 'Hit Points +50'", [], |r| r.get(0)).unwrap();
    assert!(description.contains("50"), "{description}");
}

#[test]
fn writes_tactical_dc_buffs_as_bonuses() {
    let (db, _) = built_fixture_db();
    let ring = item_id(&db, "Legendary Ring of Unbridled Might");
    let tactical_dc_bonuses: Vec<(String, Option<String>, i64)> = db
        .prepare(
            "SELECT s.name, bt.name, b.value FROM item_bonuses ib JOIN bonuses b ON b.id = ib.bonus_id
               JOIN stats s ON s.id = b.stat_id LEFT JOIN bonus_types bt ON bt.id = b.bonus_type_id
              WHERE ib.item_id = ?1 AND s.name LIKE '% DC' ORDER BY s.name",
        )
        .unwrap()
        .query_map(params![ring], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        tactical_dc_bonuses,
        vec![
            ("Stun DC".to_string(), Some("Insight".to_string()), 7),
            ("Sunder DC".to_string(), Some("Insight".to_string()), 7),
        ]
    );
    let shatter_effect_count = count(
        &db,
        &format!(
            "SELECT COUNT(*) FROM item_effects ie JOIN effects e ON e.id = ie.effect_id WHERE ie.item_id = {ring} AND e.name = 'Shatter'"
        ),
    );
    assert_eq!(shatter_effect_count, 0);
}

#[test]
fn writes_augment_slots_and_presets() {
    let (db, _) = built_fixture_db();
    let cloak = item_id(&db, "Legendary Cloak of Winter");
    let label: String = db
        .query_row("SELECT t.label FROM item_augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.item_id = ?1", params![cloak], |r| r.get(0))
        .unwrap();
    assert_eq!(label, "green");
    let sireth = item_id(&db, "Sireth, Spear of the Sky");
    let rows: Vec<(String, Option<String>)> = db
        .prepare(
            "SELECT t.label, o.name FROM item_augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id
               LEFT JOIN item_augment_slot_options o ON o.item_id = s.item_id AND o.slot_order = s.sort_order
              WHERE s.item_id = ?1 ORDER BY s.sort_order, o.option_order",
        )
        .unwrap()
        .query_map(params![sireth], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(rows.len(), 4, "four slots, each with exactly one fixed option");
    assert_eq!(rows[0], ("crafting: attuned to heroism 1".to_string(), Some("Planar Conflux".to_string())));
    assert_eq!(rows[1].1.as_deref(), Some("+8 Enhancement Bonus"));
    assert_eq!(count(&db, &format!("SELECT COUNT(*) FROM item_augment_slot_options WHERE item_id = {cloak}")), 0);
}

#[test]
fn links_items_to_quests_from_drop_location() {
    let (db, report) = built_fixture_db();
    assert_eq!(count(&db, "SELECT COUNT(*) FROM quests WHERE NOT is_challenge AND source = 'maetrim'"), 22);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM patrons"), 22);
    assert!(count(&db, "SELECT COUNT(*) FROM adventure_packs") >= 5);
    let (level, epic_level, is_raid, pack): (i64, Option<i64>, bool, String) = db
        .query_row(
            "SELECT q.level, q.epic_level, q.is_raid, p.name FROM quests q JOIN adventure_packs p ON p.id = q.pack_id WHERE q.name = 'The Chronoscope'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!((level, epic_level, is_raid, pack.as_str()), (6, Some(21), true, "Devil Assault"));

    let sireth = item_id(&db, "Sireth, Spear of the Sky");
    let (quest, loot): (String, String) = db
        .query_row(
            "SELECT q.name, ql.loot_type FROM quest_loot ql JOIN quests q ON q.id = ql.quest_id WHERE ql.item_id = ?1",
            params![sireth],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((quest.as_str(), loot.as_str()), ("Caught in the Web", "raid"), "Caught in the Web is a raid");

    let docent = item_id(&db, "Docent of Defiance");
    let loot: String =
        db.query_row("SELECT loot_type FROM quest_loot WHERE item_id = ?1", params![docent], |r| r.get(0)).unwrap();
    assert_eq!(loot, "chest", "'The Cursed Crypt, End Chest' is a non-raid chest drop");

    let axe = item_id(&db, "+3 Combustion Scorched Battle Axe");
    assert_eq!(count(&db, &format!("SELECT COUNT(*) FROM quest_loot WHERE item_id = {axe}")), 0);
    let drop: String =
        db.query_row("SELECT drop_location FROM items WHERE id = ?1", params![axe], |r| r.get(0)).unwrap();
    assert!(drop.starts_with("Temple of Elemental Evil Part One"));
    assert!(report.quest_loot_link_count >= 4);
}

fn quest_loot_types(db: &Connection, quest: &str, item: &str) -> Vec<String> {
    quest_loot_chests(db, quest, item).into_iter().map(|(loot_type, _)| loot_type).collect()
}

fn quest_loot_chests(db: &Connection, quest: &str, item: &str) -> Vec<(String, Option<String>)> {
    let mut statement = db
        .prepare(
            "SELECT ql.loot_type, ql.chest FROM quest_loot ql JOIN quests q ON q.id = ql.quest_id JOIN items i ON i.id = ql.item_id
              WHERE q.name = ?1 AND i.name = ?2 ORDER BY ql.loot_type",
        )
        .unwrap();
    statement.query_map(params![quest, item], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
}

#[test]
fn links_an_item_once_per_loot_type_the_quests_own_segment_names() {
    let (db, _) = built_fixture_db();
    assert_eq!(
        quest_loot_types(&db, "The Tide Turns", "Rusted Crown"),
        ["chest", "reward"],
        "'The Tide Turns, End Chest, End Reward' is both"
    );
    assert_eq!(
        quest_loot_chests(&db, "The Tide Turns", "Rusted Crown"),
        [("chest".to_string(), Some("end chest".to_string())), ("reward".to_string(), None)],
        "the chest row names only the chest, and a reward is not a chest"
    );
    assert_eq!(
        quest_loot_types(&db, "Project Nemesis", "Band of Diani ir'Wynarn"),
        ["raid"],
        "a saga's end reward in another segment is not the raid's own reward"
    );
}

#[test]
fn links_augments_to_the_quests_their_descriptions_name() {
    let (db, report) = built_fixture_db();
    let augment_links = |augment: &str| -> Vec<(String, String, bool, Option<String>)> {
        let mut statement = db
            .prepare(
                "SELECT q.name, qal.loot_type, qal.is_rare, qal.chest FROM quest_augment_loot qal
                   JOIN quests q ON q.id = qal.quest_id JOIN augments a ON a.id = qal.augment_id
                  WHERE a.name = ?1 ORDER BY q.name",
            )
            .unwrap();
        statement
            .query_map([augment], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert_eq!(
        augment_links("Solar Gem of Elemental Absorption (Heroic)"),
        [("Land of Lamordia".into(), "chest".into(), true, Some("vornir frosthelm's chest".into()))]
    );
    assert_eq!(
        augment_links("Lunar Gem of Magical Protection (Heroic)"),
        [("Book Burning".into(), "chest".into(), true, Some("end chest".into()))],
        "the quests his text names that the fixture lacks link nothing; the wiki fixture marks this one rare"
    );
    assert_eq!(
        augment_links("Lunar Gem of Evocation (Heroic)"),
        [("Book Burning".into(), "chest".into(), false, Some("end chest".into()))],
        "his 'Drops in: ?' names no quest; the wiki fixture's description names Book Burning"
    );
    assert_eq!(report.wiki_description_augment_link_count, 1);
    assert_eq!((report.quest_augment_loot_link_count, report.drop_text_rare_augment_link_count), (6, 1));
    let description: String = db
        .query_row(
            "SELECT description FROM augments WHERE name = 'Lunar Gem of Magical Protection (Heroic)'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(description.ends_with("A Light in the Attic, secret area chest"), "the description keeps its drop text");
}

#[test]
fn records_the_chest_each_item_drops_from() {
    let (db, _) = built_fixture_db();
    let chest_of = |quest: &str, item: &str| -> Option<String> {
        db.query_row(
            "SELECT ql.chest FROM quest_loot ql JOIN quests q ON q.id = ql.quest_id JOIN items i ON i.id = ql.item_id
              WHERE q.name = ?1 AND i.name = ?2",
            [quest, item],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(chest_of("The Cursed Crypt", "Docent of Defiance").as_deref(), Some("end chest"));
    assert_eq!(
        chest_of("Book Burning", "Buckler of the Golden Age").as_deref(),
        Some("end chest"),
        "rare marker trimmed"
    );
    assert_eq!(
        chest_of("Land of Lamordia", "Gravekeeper's Docent").as_deref(),
        Some("red-named rare encounter chests")
    );
}

#[test]
fn matches_quest_names_his_drop_text_capitalises_differently() {
    let (db, _) = built_fixture_db();
    let item_chests = |item: &str| -> Vec<(String, Option<String>)> {
        let mut statement = db
            .prepare(
                "SELECT q.name, ql.chest FROM quest_loot ql JOIN quests q ON q.id = ql.quest_id
                   JOIN items i ON i.id = ql.item_id WHERE i.name = ?1 ORDER BY q.name",
            )
            .unwrap();
        statement.query_map([item], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
    };
    let end_chest = Some("end chest".to_string());
    assert_eq!(
        item_chests("Prismatic Cloak, Grey (Level 27)"),
        [
            ("A Break in the Ice".to_string(), end_chest.clone()),
            ("Breaking the Ranks".to_string(), end_chest.clone()),
            ("Lines of Supply".to_string(), end_chest.clone()),
            ("The Tracker's Trap".to_string(), end_chest.clone()),
            ("What Goes Up".to_string(), end_chest.clone()),
        ],
        "'A Break In the Ice' is the quest 'A Break in the Ice', not the chest of the quests before it"
    );
    let augment_quests = |augment: &str| -> Vec<String> {
        let mut statement = db
            .prepare(
                "SELECT q.name FROM quest_augment_loot qal JOIN quests q ON q.id = qal.quest_id
                   JOIN augments a ON a.id = qal.augment_id WHERE a.name = ?1",
            )
            .unwrap();
        statement.query_map([augment], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    };
    assert_eq!(augment_quests("Ruby of Fey Bane"), ["Wake Me Up Inside"]);
    assert_eq!(augment_quests("Storm's Bulwark"), ["The Knight Who Cried Windmill"]);
}

#[test]
fn matches_a_quest_name_his_drop_text_wraps_onto_the_next_line() {
    let (db, _) = built_fixture_db();
    let mut statement = db
        .prepare(
            "SELECT q.name, qal.is_rare, qal.chest FROM quest_augment_loot qal JOIN quests q ON q.id = qal.quest_id
               JOIN augments a ON a.id = qal.augment_id WHERE a.name = 'Ruby of Acid Blast' ORDER BY q.name",
        )
        .unwrap();
    let links: Vec<(String, bool, Option<String>)> =
        statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect();
    let rare_encounter_chests = Some("rare encounter chests".to_string());
    assert_eq!(
        links,
        [
            ("ToEE: First Level and Earth Temple".to_string(), false, rare_encounter_chests.clone()),
            ("ToEE: Lower Temple Complex".to_string(), false, rare_encounter_chests),
        ],
        "'ToEE: Lower\\nTemple Complex' is one quest name, not the chest 'and toee: lower'"
    );
}

#[test]
fn writes_quest_difficulties_and_epic_name() {
    let (db, _) = built_fixture_db();
    let quest = |name: &str| -> (String, Option<String>) {
        db.query_row("SELECT difficulties, epic_name FROM quests WHERE name = ?1", params![name], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap_or_else(|e| panic!("{name}: {e}"))
    };
    assert_eq!(quest("The Chronoscope"), (r#"["casual","normal","hard","elite","reaper"]"#.into(), None));
    assert_eq!(quest("The Grotto"), (r#"["solo"]"#.into(), None));
    assert_eq!(quest("Land of Lamordia"), ("[]".into(), None));
    assert_eq!(
        quest("Madstone Crater"),
        (r#"["normal","hard","elite","reaper"]"#.into(), Some("Return to Madstone Crater".into()))
    );
}

#[test]
fn diff_reports_coverage_against_a_legacy_database() {
    let (db, _) = built_fixture_db();
    let legacy_db = Connection::open_in_memory().unwrap();
    legacy_db
        .execute_batch(
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
             INSERT INTO items (name) VALUES ('Sireth, Spear of the Sky'), ('Docent of Defiance'), ('Kundarak Delving Boots'),
                                             ('Something Only The Wiki Had'), ('17th Anniversary Dark Helm');",
        )
        .unwrap();
    let coverage = item_coverage(&db, &legacy_db).unwrap();
    assert_eq!(coverage.matched_count, 3);
    assert_eq!(coverage.names_only_in_legacy, vec!["Something Only The Wiki Had".to_string()]);
    assert_eq!(
        coverage.names_excluded_by_design,
        vec!["17th Anniversary Dark Helm".to_string()],
        "cosmetics are not gaps"
    );
    assert_eq!(coverage.names_only_in_built.len(), 19, "the wiki fixture item is only in the build");
    assert!((coverage.coverage_ratio() - 0.75).abs() < 1e-9);
}

#[test]
fn writes_augments_with_slots_bonuses_and_modifiers() {
    let (db, report) = built_fixture_db();
    assert_eq!(report.augment_count, 16);
    let ruby: i64 =
        db.query_row("SELECT id FROM augments WHERE name = 'Ruby of Acid Damage'", [], |r| r.get(0)).unwrap();
    let (family, has_selectable_level, levels, values): (String, bool, String, String) = db
        .query_row(
            "SELECT family, choose_level, levels, level_values FROM augments WHERE id = ?1",
            params![ruby],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(family, "Ruby");
    assert!(has_selectable_level);
    assert_eq!(levels, "[1,4,8,12,16,20,24,28,32,36]");
    assert_eq!(values, "[1,2,3,4,5,6,7,8,9,10]");
    let slots: Vec<String> = db
        .prepare("SELECT t.label FROM augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.augment_id = ?1 ORDER BY t.label")
        .unwrap()
        .query_map(params![ruby], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(slots, vec!["orange", "purple", "red"]);
    assert_eq!(count(&db, &format!("SELECT COUNT(*) FROM augment_bonuses WHERE augment_id = {ruby}")), 0);
    let (modifier_count, dice_damage, dice_counts): (i64, String, String) = db
        .query_row(
            "SELECT COUNT(*), MIN(dice_damage), MIN(dice_number) FROM modifiers WHERE source_kind = 'augment' AND source_id = ?1",
            params![ruby],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((modifier_count, dice_damage.as_str(), dice_counts.as_str()), (2, "Acid", "[2]"));

    let silver: i64 = db.query_row("SELECT id FROM augments WHERE name = 'Silverscale'", [], |r| r.get(0)).unwrap();
    let bonuses: Vec<(String, String)> = db
        .prepare(
            "SELECT b.name, bt.name FROM augment_bonuses ab JOIN bonuses b ON b.id = ab.bonus_id JOIN bonus_types bt ON bt.id = b.bonus_type_id
              WHERE ab.augment_id = ?1 ORDER BY ab.sort_order",
        )
        .unwrap()
        .query_map(params![silver], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        bonuses,
        vec![
            ("Healing Amplification +56".to_string(), "Competence".to_string()),
            ("Negative Healing Amplification +56".to_string(), "Profane".to_string()),
            ("Repair Amplification +56".to_string(), "Enhancement".to_string()),
        ]
    );
    let label: String = db
        .query_row(
            "SELECT t.label FROM augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.augment_id = ?1",
            params![silver],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(label, "isle of dread: scale (armor)");

    let (added_augment, slot, description): (String, String, String) = db
        .query_row(
            "SELECT a.adds_augment, t.label, a.effect_description FROM augments a JOIN augment_slots s ON s.augment_id = a.id JOIN augment_slot_types t ON t.id = s.slot_id WHERE a.name = 'Fire I: Combustion'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(added_augment, "Legendary Alchemical Tier 2");
    assert_eq!(slot, "crafting: legendary alchemical tier 1");
    assert!(description.starts_with("Combustion 152") && description.contains("Fire Lore +21%"), "{description}");
    let fire: Vec<String> = db
        .prepare("SELECT b.name FROM augment_bonuses ab JOIN bonuses b ON b.id = ab.bonus_id JOIN augments a ON a.id = ab.augment_id WHERE a.name = 'Fire I: Combustion' ORDER BY ab.sort_order")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(fire, vec!["Fire Spell Power +152", "Fire Spell Lore +21"]);

    let (suppresses_set_bonus, set): (bool, String) = db
        .query_row("SELECT suppress_set_bonus, set_bonus FROM augments WHERE name = 'Perfect Silence'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert!(suppresses_set_bonus);
    assert_eq!(set, "Perfect Silence");
}

#[test]
fn writes_sets_filigrees_and_their_items() {
    let (db, report) = built_fixture_db();
    assert_eq!(report.set_bonus_count, 5, "four gear sets and one filigree set");
    let (icon, filigree): (String, bool) = db
        .query_row("SELECT icon, is_filigree_set FROM set_bonuses WHERE name = 'The Inevitable Grave'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert!(!icon.is_empty());
    assert!(filigree);
    let winter: i64 =
        db.query_row("SELECT id FROM set_bonuses WHERE name = 'Eminence of Winter'", [], |r| r.get(0)).unwrap();
    let tiers: Vec<(i64, String)> = db
        .prepare("SELECT equipped_count, description FROM set_bonus_tiers WHERE set_id = ?1 ORDER BY equipped_count")
        .unwrap()
        .query_map(params![winter], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(!tiers.is_empty());
    assert!(tiers.iter().all(|(n, _)| *n >= 2));
    let tier_modifier_count = count(&db, &format!("SELECT COUNT(*) FROM modifiers m JOIN set_bonus_tiers t ON t.id = m.source_id WHERE m.source_kind = 'set_bonus_tier' AND t.set_id = {winter}"));
    assert!(tier_modifier_count >= 1);
    let members: Vec<String> = db
        .prepare("SELECT i.name FROM set_bonus_items sbi JOIN items i ON i.id = sbi.item_id WHERE sbi.set_id = ?1")
        .unwrap()
        .query_map(params![winter], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(members, vec!["Legendary Cloak of Winter"]);

    assert_eq!(count(&db, "SELECT COUNT(*) FROM filigrees"), 4);
    let (set_name, menu): (String, String) = db
        .query_row("SELECT s.name, f.menu FROM filigrees f JOIN set_bonuses s ON s.id = f.set_id LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(set_name, "The Inevitable Grave");
    assert!(!menu.is_empty());
    assert!(
        count(&db, "SELECT COUNT(*) FROM modifiers WHERE source_kind = 'filigree' AND is_rare = 1") >= 1,
        "rare filigree bonuses are flagged"
    );
}

#[test]
fn links_sets_to_the_augments_that_grant_them() {
    let (db, _) = built_fixture_db();
    let rows: Vec<(String, String)> = db
        .prepare(
            "SELECT s.name, a.name FROM set_bonus_augments sba JOIN set_bonuses s ON s.id = sba.set_id
               JOIN augments a ON a.id = sba.augment_id WHERE a.source = 'maetrim' ORDER BY s.name, a.name",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(rows, vec![("Perfect Silence".to_string(), "Perfect Silence".to_string())]);
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM set_bonus_items sbi JOIN set_bonuses s ON s.id = sbi.set_id WHERE s.name = 'Perfect Silence'"),
        0,
        "no item names the set; only the augment grants it"
    );
}

#[test]
fn writes_clickies_and_item_level_effects() {
    let (db, report) = built_fixture_db();
    assert_eq!(report.clickie_count, 2);
    let alabaster = item_id(&db, "Alabaster of the Twelve");
    let name: String = db
        .query_row(
            "SELECT c.name FROM item_clickies ic JOIN clickies c ON c.id = ic.clickie_id WHERE ic.item_id = ?1",
            params![alabaster],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(name, "Cure Serious Wounds, Mass");
    let effect_types: Vec<String> = db
        .prepare("SELECT effect_type FROM modifiers WHERE source_kind = 'item' AND source_id = ?1 ORDER BY sort_order")
        .unwrap()
        .query_map(params![alabaster], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(effect_types.contains(&"ItemClickie".to_string()), "{effect_types:?}");
    let rune_arm = item_id(&db, "Acid Rune Arm");
    let (clickie_name, clickie_id): (String, Option<i64>) = db
        .query_row("SELECT name, clickie_id FROM item_clickies WHERE item_id = ?1", params![rune_arm], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(clickie_name, "Acid Shot");
    assert!(clickie_id.is_some(), "a name ItemClickies.xml defines resolves immediately");
    assert!(report.modifier_count > 20);
}

#[test]
fn parses_and_writes_sentient_gems() {
    let parsed_gems = sentient_gems::parse(&data_files_fixture_dir().join("Sentient.gems.xml")).unwrap();
    let names: Vec<&str> = parsed_gems.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(
        names,
        ["Sentient Jewel of the Hopeful", "Sentient Jewel of the Inquisitive", "Sentient Jewel of the Resolute"]
    );
    let (db, report) = built_fixture_db();
    assert_eq!(report.sentient_gem_count, 3);
    let (icon, description): (String, String) = db
        .query_row(
            "SELECT icon, description FROM sentient_gems WHERE name = 'Sentient Jewel of the Resolute'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((icon.as_str(), description.as_str()), ("SentientJewel_Blue", "Voiced by: Ally Murphy"));
}

#[test]
fn parses_challenges_with_their_level_range() {
    let parsed_challenges = challenges::parse(&data_files_fixture_dir().join("Challenges.xml")).unwrap();
    assert_eq!(parsed_challenges.len(), 3, "the grouping comments are skipped");
    let door = &parsed_challenges[0];
    assert_eq!(door.name, "Dr. Rushmore's Mansion - Behind the Door");
    assert_eq!(door.patron.as_deref(), Some("House Cannith"));
    assert_eq!(door.adventure_pack.as_deref(), Some("Free to Play"));
    assert_eq!(door.level_range, [4, 15]);
    assert_eq!(parsed_challenges[2].patron, None, "<Patron>None</Patron> is no patron");
}

#[test]
fn writes_challenges_into_quests() {
    let (db, report) = built_fixture_db();
    assert_eq!(report.challenge_count, 3);
    let challenge = |name: &str| -> (Option<String>, String, i64, i64, bool, Option<i64>) {
        db.query_row(
            "SELECT pt.name, p.name, q.level, q.max_level, q.is_challenge, q.epic_level
               FROM quests q JOIN adventure_packs p ON p.id = q.pack_id LEFT JOIN patrons pt ON pt.id = q.patron_id
              WHERE q.name = ?1",
            params![name],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"))
    };
    assert_eq!(
        challenge("Dr. Rushmore's Mansion - Moving Targets - EPIC"),
        (Some("House Cannith".into()), "Secrets of the Artificers".into(), 21, 25, true, None)
    );
    assert_eq!(
        challenge("Extraplanar Mining - Epic The Dragon's Horde"),
        (None, "Free to Play".into(), 15, 20, true, None)
    );
    let (is_challenge, max_level): (bool, Option<i64>) = db
        .query_row("SELECT is_challenge, max_level FROM quests WHERE name = 'The Grotto'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((is_challenge, max_level), (false, None), "regular quests are not challenges");
}
