//! End to end: the fixture DataFiles directory becomes a database with the expected rows.

use ddo_etl::build::{build, BuildReport};
use ddo_etl::diff::compare;
use ddo_model::DatasetVersion;
use rusqlite::{params, Connection};
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles")
}

fn version() -> DatasetVersion {
    DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-09-20T00:00:00Z".into() }
}

fn built() -> (Connection, BuildReport) {
    let mut conn = Connection::open_in_memory().unwrap();
    let report = build(&fixtures(), &mut conn, &version()).expect("build succeeds on fixtures");
    (conn, report)
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn item_id(conn: &Connection, name: &str) -> i64 {
    conn.query_row("SELECT id FROM items WHERE name = ?1", params![name], |r| r.get(0))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn builds_items_and_skips_cosmetics() {
    let (conn, report) = built();
    assert_eq!(report.items_written, 12);
    assert_eq!(report.items_skipped_cosmetic, 1);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM items"), 12);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM items WHERE name = '17th Anniversary Dark Helm'"), 0);
    let reason: String = conn
        .query_row("SELECT reason FROM excluded_items WHERE name = '17th Anniversary Dark Helm'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(reason, "cosmetic-only slots");
    let (sha, ver): (String, i64) = conn
        .query_row("SELECT upstream_sha, (SELECT version FROM schema_version) FROM dataset_version", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(sha, "31ef0201");
    assert_eq!(ver, ddo_model::SCHEMA_VERSION);
}

#[test]
fn writes_item_core_columns() {
    let (conn, _) = built();
    let id = item_id(&conn, "Sireth, Spear of the Sky");
    let (slot, category, item_type, ml, enh, material, icon, sentient, drop): (String, String, String, i64, i64, String, String, bool, String) = conn
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
    assert_eq!(ml, 23);
    assert_eq!(enh, 7);
    assert_eq!(material, "Steel");
    assert_eq!(icon, "Quarterstaff_6a");
    assert!(sentient);
    assert_eq!(drop, "Caught in the Web, End Chest");

    let docent = item_id(&conn, "Docent of Defiance");
    let race: String =
        conn.query_row("SELECT race_required FROM items WHERE id = ?1", params![docent], |r| r.get(0)).unwrap();
    assert_eq!(race, "Construct");
    let gem: String = conn
        .query_row(
            "SELECT m.name FROM items i JOIN item_materials m ON m.id = i.material_id WHERE i.id = ?1",
            params![docent],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(gem, "Gem", "'(material)' suffix is stripped");
    let wiki: String = conn.query_row("SELECT wiki_url FROM items WHERE id = ?1", params![id], |r| r.get(0)).unwrap();
    assert_eq!(wiki, "https://ddowiki.com/page/Item:Sireth,_Spear_of_the_Sky");
}

#[test]
fn writes_weapon_stats_with_rendered_display_strings() {
    let (conn, _) = built();
    let id = item_id(&conn, "Sireth, Spear of the Sky");
    let (wt, dmg, crit, hand, mult, range): (String, String, String, String, f64, i64) = conn
        .query_row(
            "SELECT wt.name, s.damage, s.critical, s.handedness, s.damage_multiplier, s.critical_threat_range
               FROM item_weapon_stats s JOIN weapon_types wt ON wt.id = s.weapon_type_id WHERE s.item_id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .unwrap();
    assert_eq!(wt, "Quarterstaff");
    assert_eq!(dmg, "3.6[1d10] + 7 Good, Magic, Pierce, Slash");
    assert_eq!(crit, "16-20 / x2");
    assert_eq!(hand, "Two-handed");
    assert_eq!(mult, 3.6);
    assert_eq!(range, 5);
    assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM item_dr_bypass WHERE item_id = {id}")), 4);

    let axe = item_id(&conn, "+3 Combustion Scorched Battle Axe");
    let (dmg, crit, hand): (String, String, String) = conn
        .query_row("SELECT damage, critical, handedness FROM item_weapon_stats WHERE item_id = ?1", params![axe], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert_eq!(dmg, "1[1d8] + 3 Magic, Slash");
    assert_eq!(crit, "20 / x3");
    assert_eq!(hand, "One-handed");

    let shield = item_id(&conn, "+1 Starter Heavy Steel Shield");
    let (cat, armor_type, hand): (String, String, String) = conn
        .query_row(
            "SELECT i.item_category, a.armor_type, w.handedness FROM items i JOIN item_armor_stats a ON a.item_id = i.id JOIN item_weapon_stats w ON w.item_id = i.id WHERE i.id = ?1",
            params![shield],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((cat.as_str(), armor_type.as_str(), hand.as_str()), ("Shield", "Shield", "Off-hand"));
}

#[test]
fn writes_armor_stats() {
    let (conn, _) = built();
    let docent = item_id(&conn, "Docent of Defiance");
    let (armor_type, bonus, mithral, adamantine): (String, i64, i64, i64) = conn
        .query_row(
            "SELECT armor_type, armor_bonus, mithral_body, adamantine_body FROM item_armor_stats WHERE item_id = ?1",
            params![docent],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!((armor_type.as_str(), bonus, mithral, adamantine), ("Docent", -3, 5, 12));
    let heavy = item_id(&conn, "Argenti's Armor");
    let armor_type: String = conn
        .query_row("SELECT armor_type FROM item_armor_stats WHERE item_id = ?1", params![heavy], |r| r.get(0))
        .unwrap();
    assert_eq!(armor_type, "Heavy");
}

#[test]
fn splits_buffs_into_bonuses_and_effects() {
    let (conn, _) = built();
    let cloak = item_id(&conn, "Legendary Cloak of Winter");
    let bonuses: Vec<(String, String, Option<String>, i64)> = conn
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
    let effects: Vec<(String, Option<i64>, Option<String>)> = conn
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

    // The same "Fire Spell Power +54 (Enhancement)" bonus on two items is one bonuses row.
    let dup = count(&conn, "SELECT COUNT(*) FROM bonuses WHERE name = 'Fire Spell Power +54'");
    assert_eq!(dup, 1);
    let desc: String =
        conn.query_row("SELECT description FROM bonuses WHERE name = 'Hit Points +50'", [], |r| r.get(0)).unwrap();
    assert!(desc.contains("50"), "{desc}");
}

#[test]
fn writes_augment_slots_and_presets() {
    let (conn, _) = built();
    let cloak = item_id(&conn, "Legendary Cloak of Winter");
    let label: String = conn
        .query_row("SELECT t.label FROM item_augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.item_id = ?1", params![cloak], |r| r.get(0))
        .unwrap();
    assert_eq!(label, "green");
    let sireth = item_id(&conn, "Sireth, Spear of the Sky");
    let rows: Vec<(String, Option<String>)> = conn
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
    assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM item_augment_slot_options WHERE item_id = {cloak}")), 0);
}

#[test]
fn links_items_to_quests_from_drop_location() {
    let (conn, report) = built();
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM quests"), 7);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM patrons"), 22);
    assert!(count(&conn, "SELECT COUNT(*) FROM adventure_packs") >= 5);
    let (level, epic, raid, pack): (i64, Option<i64>, bool, String) = conn
        .query_row(
            "SELECT q.level, q.epic_level, q.is_raid, p.name FROM quests q JOIN adventure_packs p ON p.id = q.pack_id WHERE q.name = 'The Chronoscope'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!((level, epic, raid, pack.as_str()), (6, Some(21), true, "Devil Assault"));

    let sireth = item_id(&conn, "Sireth, Spear of the Sky");
    let (quest, loot): (String, String) = conn
        .query_row(
            "SELECT q.name, ql.loot_type FROM quest_loot ql JOIN quests q ON q.id = ql.quest_id WHERE ql.item_id = ?1",
            params![sireth],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((quest.as_str(), loot.as_str()), ("Caught in the Web", "raid"), "Caught in the Web is a raid");

    let docent = item_id(&conn, "Docent of Defiance");
    let loot: String =
        conn.query_row("SELECT loot_type FROM quest_loot WHERE item_id = ?1", params![docent], |r| r.get(0)).unwrap();
    assert_eq!(loot, "chest", "'The Cursed Crypt, End Chest' is a non-raid chest drop");

    // Upstream writes "Temple of Elemental Evil Part One" but Quests.xml names those quests
    // "Temple of Elemental Evil: Water Node" etc., so this drop text stays unlinked. The text is
    // still on items.drop_location.
    let axe = item_id(&conn, "+3 Combustion Scorched Battle Axe");
    assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM quest_loot WHERE item_id = {axe}")), 0);
    let drop: String =
        conn.query_row("SELECT drop_location FROM items WHERE id = ?1", params![axe], |r| r.get(0)).unwrap();
    assert!(drop.starts_with("Temple of Elemental Evil Part One"));
    assert!(report.quest_loot_links >= 4);
}

#[test]
fn diff_reports_coverage_against_a_legacy_database() {
    let (conn, _) = built();
    let legacy = Connection::open_in_memory().unwrap();
    legacy
        .execute_batch(
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
             INSERT INTO items (name) VALUES ('Sireth, Spear of the Sky'), ('Docent of Defiance'), ('Kundarak Delving Boots'),
                                             ('Something Only The Wiki Had'), ('17th Anniversary Dark Helm');",
        )
        .unwrap();
    let report = compare(&conn, &legacy).unwrap();
    assert_eq!(report.matched, 3);
    assert_eq!(report.only_legacy, vec!["Something Only The Wiki Had".to_string()]);
    assert_eq!(report.excluded_by_design, vec!["17th Anniversary Dark Helm".to_string()], "cosmetics are not gaps");
    assert_eq!(report.only_new.len(), 9);
    assert!((report.coverage() - 0.75).abs() < 1e-9);
}
