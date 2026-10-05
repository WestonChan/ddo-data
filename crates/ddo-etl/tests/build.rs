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
fn potency_grants_each_named_spell_power_without_granting_universal_spell_power() {
    let (db, _) = built_fixture_db();
    let item = item_id(&db, "Band of Diani ir'Wynarn");
    let spell_powers: Vec<String> = db
        .prepare(
            "SELECT s.name FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
             WHERE ob.owner_kind = 'item' AND ob.owner_id = ?1
               AND ob.via_effect_id = (SELECT id FROM effects WHERE name = 'Potency') ORDER BY s.name",
        )
        .unwrap()
        .query_map([item], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(spell_powers.len(), 15);
    assert!(spell_powers.contains(&"Fire Spell Power".to_string()));
    assert!(!spell_powers.contains(&"Universal Spell Power".to_string()));
}

#[test]
fn writes_improved_deception_from_its_definition_as_a_typed_bluff_bonus() {
    let (db, report) = built_fixture_db();
    assert_eq!(report.effect_fallback_buff_count, 23);
    assert!(report.family_buff_count > 0);
    assert!(report.effect_buff_count > 0);
    let gloves = item_id(&db, "Backstabber's Gloves (Level 25)");
    let written_bonuses: Vec<(String, i64, String)> = db
        .prepare(
            "SELECT s.name, CASE es.amount_from WHEN 0 THEN es.constant WHEN 1 THEN ie.value ELSE ie.value2 END, bt.name
             FROM item_effects ie JOIN effect_bonuses es ON es.effect_id = ie.effect_id
             JOIN effects s ON s.id = es.target_effect_id JOIN bonus_types bt ON bt.id = COALESCE(es.bonus_type_id, ie.bonus_type_id)
             WHERE ie.item_id = ?1 AND ie.value = 5",
        )
        .unwrap()
        .query_map(params![gloves], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(written_bonuses.contains(&("Bluff".into(), 5, "Enhancement".into())));
}

#[test]
fn written_deception_steps_share_the_named_ladder_in_order() {
    let (db, _) = built_fixture_db();
    let steps: Vec<(String, i64)> = db
        .prepare(
            "SELECT e.name, e.tier FROM effects e
                  JOIN effect_tier_groups l ON l.id = e.tier_group_id
                  WHERE l.name = 'Deception' ORDER BY e.tier",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(steps, [("Deception".into(), 1), ("Improved Deception".into(), 2)]);
}

#[test]
fn effect_fallback_rows_stay_adjacent_and_distinct_descriptions_survive() {
    let (db, _) = built_fixture_db();
    let gauntlets = item_id(&db, "Alaric's Grim Gauntlets");
    let ordered_rows: Vec<(String, i64, i64)> = db
        .prepare(
            "SELECT s.name, ob.amount, ob.effect_link_order
             FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
             WHERE ob.owner_kind = 'item' AND ob.owner_id = ?1 ORDER BY ob.effect_link_order, ob.stat_id",
        )
        .unwrap()
        .query_map(params![gauntlets], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        &ordered_rows[4..],
        &[("Armor Class".into(), 5, 5), ("Saving Throws".into(), 5, 5), ("Bluff".into(), 3, 6)]
    );

    let bluff_rows: Vec<(String, String, String)> = db
        .prepare(
            "SELECT i.name, e.verbose_name_template || ': ' || e.description_template, bt.name FROM item_effects ie
             JOIN items i ON i.id = ie.item_id JOIN effects e ON e.id = ie.effect_id
             JOIN effect_bonuses es ON es.effect_id = e.id JOIN effects s ON s.id = es.target_effect_id
             JOIN bonus_types bt ON bt.id = COALESCE(es.bonus_type_id, ie.bonus_type_id)
             WHERE i.name IN ('Alaric''s Grim Gauntlets', 'Acrobat''s Ring') \
             AND s.name = 'Bluff' AND CASE es.amount_from WHEN 0 THEN es.constant WHEN 1 THEN ie.value ELSE ie.value2 END = 3
             ORDER BY i.name",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(bluff_rows.len(), 2);
    assert_eq!(bluff_rows[0].2, "Enhancement");
    assert_eq!(bluff_rows[1].2, "Enhancement");
    assert!(bluff_rows
        .iter()
        .any(|(item, description, _)| item == "Acrobat's Ring" && description.contains("Deception")));
    assert!(bluff_rows
        .iter()
        .any(|(item, description, _)| item == "Alaric's Grim Gauntlets" && description.contains("Improved Deception")));
}

#[test]
fn excludes_cosmetic_shields_from_items() {
    let (db, _) = built_fixture_db();
    assert_eq!(count(&db, "SELECT COUNT(*) FROM items WHERE name = 'Cosmetic Jackorb'"), 0);
    let reason: String = db
        .query_row("SELECT reason FROM excluded_items WHERE name = 'Cosmetic Jackorb'", [], |r| r.get(0))
        .expect("Cosmetic Jackorb is recorded in excluded_items");
    assert_eq!(reason, "cosmetic shield");
}

#[test]
fn builds_items_and_skips_cosmetics() {
    let (db, report) = built_fixture_db();
    assert_eq!(report.written_item_count, 58);
    assert_eq!(report.effect_count as i64, count(&db, "SELECT COUNT(*) FROM effects"));
    assert_eq!(report.bonus_count as i64, count(&db, "SELECT COUNT(*) FROM effect_bonuses"));
    assert_eq!(
        report.text_only_effect_count as i64,
        count(
            &db,
            "SELECT COUNT(*) FROM effects e WHERE e.is_stat = 0 AND NOT EXISTS (SELECT 1 FROM effect_bonuses b WHERE b.effect_id = e.id)"
        )
    );
    assert_eq!(report.skipped_cosmetic_item_count, 2, "the cosmetic helm and the cosmetic shield");
    assert_eq!(count(&db, "SELECT COUNT(*) FROM items WHERE provenance = 'maetrim'"), 58);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM items WHERE name = '17th Anniversary Dark Helm'"), 0);
    let reason: String = db
        .query_row("SELECT reason FROM excluded_items WHERE name = '17th Anniversary Dark Helm'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(reason, "cosmetic-only slots");
    assert_eq!(
        report.legacy_item_count, 3,
        "the two legacy-named fixture items and the axe that drops only in a retired Temple of Elemental Evil part"
    );
    for old_version_name in ["Ratkiller (legacy) (level 4)", "Allegiance (historic)"] {
        let is_legacy: bool = db
            .query_row("SELECT is_legacy FROM items WHERE name = ?1", params![old_version_name], |r| r.get(0))
            .unwrap_or_else(|e| panic!("{old_version_name} is written: {e}"));
        assert!(is_legacy, "{old_version_name}");
        assert_eq!(count(&db, &format!("SELECT COUNT(*) FROM excluded_items WHERE name = \"{old_version_name}\"")), 0);
    }
    assert_eq!(count(&db, "SELECT COUNT(*) FROM items WHERE is_legacy"), 3);
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
            "SELECT s.name || ' +' || ob.amount,
                    s.name, bt.name, ob.effect_link_order FROM owner_bonuses ob
               JOIN effects s ON s.id = ob.stat_id LEFT JOIN bonus_types bt ON bt.id = ob.bonus_type_id
              WHERE ob.owner_kind = 'item' AND ob.owner_id = ?1 ORDER BY ob.effect_link_order, ob.stat_id",
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
            (
                "Negative Absorption +34".to_string(),
                "Negative Absorption".to_string(),
                Some("Enhancement".to_string()),
                3
            ),
        ]
    );
    let effects: Vec<(String, Option<i64>, Option<String>)> = db
        .prepare(
            "SELECT e.name, ie.value, e.verbose_name_template || ': ' || COALESCE(e.description_template, '')
                  FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
                  WHERE ie.item_id = ?1 AND e.is_stat = 0 AND NOT EXISTS (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)
                  ORDER BY ie.sort_order",
        )
        .unwrap()
        .query_map(params![cloak], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].0, "Legendary Ice Barrier");
    assert!(effects[0].2.as_deref().unwrap_or("").len() > 10, "effects carry ItemBuffs.xml text");

    let fire_family_count = count(
        &db,
        "SELECT COUNT(*) FROM effects e JOIN effect_bonuses es ON es.effect_id = e.id
         JOIN effects s ON s.id = es.target_effect_id WHERE s.name = 'Fire Spell Power' AND e.name = 'Combustion'",
    );
    assert_eq!(fire_family_count, 1);
    let description_template: String = db
        .query_row(
            "SELECT e.description_template FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
         JOIN effect_bonuses es ON es.effect_id = e.id JOIN effects s ON s.id = es.target_effect_id
         WHERE ie.item_id = ?1 AND s.name = 'Hit Points'",
            [cloak],
            |r| r.get(0),
        )
        .unwrap();
    assert!(description_template.contains("{1}"), "{description_template}");
}

#[test]
fn writes_tactical_dc_buffs_as_bonuses() {
    let (db, _) = built_fixture_db();
    let ring = item_id(&db, "Legendary Ring of Unbridled Might");
    let tactical_dc_bonuses: Vec<(String, Option<String>, i64)> = db
        .prepare(
            "SELECT s.name, bt.name, ob.amount
               FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
               LEFT JOIN bonus_types bt ON bt.id = ob.bonus_type_id
              WHERE ob.owner_kind = 'item' AND ob.owner_id = ?1 AND s.name LIKE '% DC' ORDER BY s.name",
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
            "SELECT COUNT(*) FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
             WHERE ie.item_id = {ring} AND e.name = 'Shatter' AND NOT EXISTS
               (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)"
        ),
    );
    assert_eq!(shatter_effect_count, 0);
}

#[test]
fn writes_a_buff_without_a_value_at_the_amount_its_definition_fixes() {
    let (db, _) = built_fixture_db();
    let reign = item_id(&db, "Yeenoghu's Reign");
    let rage_bonuses: Vec<(Option<String>, Option<i64>)> = db
        .prepare(
            "SELECT bt.name, ob.amount
               FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
               LEFT JOIN bonus_types bt ON bt.id = ob.bonus_type_id
              WHERE ob.owner_kind = 'item' AND ob.owner_id = ?1 AND s.name = 'Rage Uses'",
        )
        .unwrap()
        .query_map(params![reign], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        rage_bonuses,
        vec![(Some("Equipment".to_string()), Some(5))],
        "Major Anger's definition carries one Simple ExtraRage effect of 5 Equipment"
    );
    let major_anger_effect_count = count(
        &db,
        &format!(
            "SELECT COUNT(*) FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
             WHERE ie.item_id = {reign} AND e.name = 'Major Anger' AND NOT EXISTS
               (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)"
        ),
    );
    assert_eq!(major_anger_effect_count, 0);
}

#[test]
fn writes_buffs_named_after_a_stat_as_bonuses() {
    let (db, _) = built_fixture_db();
    let stat_bonuses_by_item = |item_name: &str, stat_name: &str| -> Vec<(Option<String>, Option<i64>, Option<i64>)> {
        db.prepare(
            "SELECT bt.name, ob.amount, ie.value2
               FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
               JOIN item_effects ie ON ie.item_id = ob.owner_id AND ie.sort_order = ob.effect_link_order
               LEFT JOIN bonus_types bt ON bt.id = ob.bonus_type_id
              WHERE ob.owner_kind = 'item' AND ob.owner_id = ?1 AND s.name = ?2 ORDER BY ob.effect_link_order, ob.stat_id",
        )
        .unwrap()
        .query_map(params![item_id(&db, item_name), stat_name], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
    };
    let insight = Some("Insight".to_string());
    let enhancement = Some("Enhancement".to_string());
    assert_eq!(
        stat_bonuses_by_item("Backstabber's Gloves (Level 25)", "Sneak Attack"),
        vec![(enhancement.clone(), Some(5), Some(8)), (insight.clone(), Some(3), Some(5))]
    );
    assert_eq!(
        stat_bonuses_by_item("Grudgebearer's Plate", "Bluff"),
        vec![(insight.clone(), Some(2), Some(-6))],
        "Command grants an Insight bonus to Charisma skill checks"
    );
    assert_eq!(
        stat_bonuses_by_item("Grudgebearer's Plate", "Hide"),
        vec![(Some("Penalty".to_string()), Some(-6), Some(-6))]
    );
    assert_eq!(
        stat_bonuses_by_item("Bold Trinket", "Damage Bonus"),
        vec![(Some("Competence".to_string()), Some(1), None)]
    );
    assert_eq!(
        stat_bonuses_by_item("The Stablestone", "Alignment Absorption"),
        vec![(enhancement.clone(), Some(22), None)]
    );
    assert_eq!(
        stat_bonuses_by_item("Cyran Guard (Level 27)", "Elemental Absorption"),
        vec![(enhancement.clone(), Some(19), None)]
    );
    assert_eq!(
        stat_bonuses_by_item("Visor of Fraz-Urb'luu", "Illusion Save"),
        vec![(Some("Resistance".to_string()), Some(5), None)]
    );
    assert_eq!(
        stat_bonuses_by_item("Celestial Emerald Ring", "Linguistics"),
        vec![(Some("Equipment".to_string()), Some(10), None)]
    );
    let linguistic_line: String = db
        .query_row(
            "SELECT e.verbose_name_template FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
         JOIN items i ON i.id = ie.item_id WHERE i.name = 'Celestial Emerald Ring' AND e.name = 'Linguistics'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(linguistic_line, "Linguistics +{1}%");
    assert_eq!(
        stat_bonuses_by_item("Echoes of Night", "Rune Arm Charge Rate"),
        vec![(enhancement.clone(), Some(5), None)]
    );
    let equipment = Some("Equipment".to_string());
    let resistance = Some("Resistance".to_string());
    assert_eq!(
        stat_bonuses_by_item("Alaric's Grim Gauntlets", "Dark Restoration Lore"),
        vec![(equipment.clone(), Some(23), None)]
    );
    assert_eq!(
        stat_bonuses_by_item("Epic Glimpse of the Soul", "Enchantment Save"),
        vec![(resistance.clone(), Some(6), None)]
    );
    assert_eq!(stat_bonuses_by_item("Epic Glimpse of the Soul", "Illusion Save"), vec![(resistance, Some(6), None)]);
    assert_eq!(stat_bonuses_by_item("Sightless", "Melee Power"), vec![(enhancement.clone(), Some(6), None)]);
    assert_eq!(stat_bonuses_by_item("Sightless", "Ranged Power"), vec![(enhancement, Some(6), None)]);
    assert_eq!(
        stat_bonuses_by_item("A Memento of Mori", "Sacred Ground Lore"),
        vec![(equipment.clone(), Some(22), None)]
    );
    assert_eq!(stat_bonuses_by_item("Darstil's Gloves", "Spell Lore"), vec![(equipment.clone(), Some(3), None)]);
    assert_eq!(
        stat_bonuses_by_item("Alarphon's Staff", "Spell Lore"),
        vec![(equipment.clone(), Some(6), None)],
        "a numeral <Item> names no school, so the lore is to every spell"
    );
    assert_eq!(
        stat_bonuses_by_item("Epic Ring of Master Artifice", "Repair Spell Lore"),
        vec![(equipment, Some(16), None)],
        "an <Item> naming a school narrows Spell Lore to that school's lore"
    );
    assert_eq!(stat_bonuses_by_item("Epic Ring of Master Artifice", "Spell Lore"), vec![]);
    let effects_named_after_a_stat: Vec<String> = db
        .prepare(
            "SELECT DISTINCT e.name FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
              WHERE e.is_stat = 0 AND lower(replace(e.name, ' ', '')) IN (SELECT lower(replace(name, ' ', '')) FROM effects WHERE is_stat = 1)
              AND e.is_stat = 0 AND NOT EXISTS (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)
              ORDER BY e.name",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(effects_named_after_a_stat.is_empty(), "{effects_named_after_a_stat:?}");
}

#[test]
fn one_stat_effect_uses_each_owners_bonus_type() {
    let (db, _) = built_fixture_db();
    let separate_effect_count: i64 =
        db.query_row("SELECT COUNT(*) FROM effects WHERE name = 'Extra Lay on Hands'", [], |row| row.get(0)).unwrap();
    assert_eq!(separate_effect_count, 0);
    let item: (i64, String) = db
        .query_row(
            "SELECT ob.amount, bt.name FROM owner_bonuses ob JOIN items i ON i.id = ob.owner_id
             JOIN effects s ON s.id = ob.stat_id JOIN bonus_types bt ON bt.id = ob.bonus_type_id
             WHERE ob.owner_kind = 'item' AND i.name = 'Lindal''s Mighty Belt' AND s.name = 'Lay on Hands Uses'
               AND ob.via_effect_id IS NULL",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(item, (3, "Enhancement".into()));
    let feat_types: Vec<String> = db
        .prepare(
            "SELECT bt.name FROM feat_effects fe JOIN effects e ON e.id = fe.effect_id
             JOIN bonus_types bt ON bt.id = fe.bonus_type_id WHERE e.name = 'Lay on Hands Uses'",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(!feat_types.is_empty());
    assert!(feat_types.iter().all(|bonus_type| bonus_type == "Feat"));
    let illusion_stat: (i64, String) = db
        .query_row("SELECT is_stat, verbose_name_template FROM effects WHERE name = 'Illusion Save'", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(illusion_stat, (1, "%b1 Illusion Save +{1}".into()));
    let illusion_named_effect_count: i64 = db
        .query_row("SELECT COUNT(*) FROM effects WHERE name = 'Illusion Save (variable type)'", [], |row| row.get(0))
        .unwrap();
    assert_eq!(illusion_named_effect_count, 0);

    let fixed_item_types: Vec<(String, String)> = db
        .prepare(
            "SELECT e.name, bt.name FROM owner_bonuses ob
             JOIN items i ON i.id = ob.owner_id JOIN effects e ON e.id = ob.via_effect_id
             JOIN bonus_types bt ON bt.id = ob.bonus_type_id
             WHERE ob.owner_kind = 'item' AND i.name = 'Lindal''s Mighty Belt'
               AND e.name IN ('Invisibility', 'Unwieldy') ORDER BY e.name",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(fixed_item_types, [("Invisibility".into(), "Deflection".into()), ("Unwieldy".into(), "Penalty".into())]);

    for (owner_kind, owner_table, owner_name, expected_type) in [
        ("item", "items", "Visor of Fraz-Urb'luu", "Resistance"),
        ("augment", "augments", "Storm's Bulwark", "Insight"),
        ("feat", "feats", "Adamantine Body", "Feat"),
    ] {
        let sql = format!(
            "SELECT bt.name FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
             JOIN bonus_types bt ON bt.id = ob.bonus_type_id
             WHERE ob.owner_kind = ?1 AND ob.owner_id = (SELECT id FROM {owner_table} WHERE name = ?2)
               AND s.name = 'Illusion Save'"
        );
        let served_type: String = db.query_row(&sql, params![owner_kind, owner_name], |row| row.get(0)).unwrap();
        assert_eq!(served_type, expected_type, "{owner_kind} {owner_name}");
    }
}

#[test]
fn skill_ability_item_links_the_group_without_a_wrapper() {
    let (db, _) = built_fixture_db();
    let linked: (i64, String) = db
        .query_row(
            "SELECT ie.value, bt.name FROM item_effects ie
         JOIN items i ON i.id = ie.item_id JOIN effects e ON e.id = ie.effect_id
         JOIN bonus_types bt ON bt.id = ie.bonus_type_id
         WHERE i.name = 'Lindal''s Mighty Belt' AND e.name = 'Charisma Skills' AND e.is_group = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(linked, (4, "Insight".into()));
    assert_eq!(count(&db, "SELECT COUNT(*) FROM effects WHERE name = 'Skills Charisma'"), 0);
    let augment_link: (i64, String) = db
        .query_row(
            "SELECT ae.value, bt.name FROM augment_effects ae
         JOIN augments a ON a.id = ae.augment_id JOIN effects e ON e.id = ae.effect_id
         JOIN bonus_types bt ON bt.id = ae.bonus_type_id
         WHERE a.name = 'Storm''s Bulwark' AND e.name = 'Charisma Skills' AND e.is_group = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(augment_link, (5, "Insight".into()));
    let description: String = db
        .query_row("SELECT description_template FROM effects WHERE name = 'Charisma Skills'", [], |row| row.get(0))
        .unwrap();
    assert_eq!(description, "{1} %b1 bonus to all Charisma based skills.");
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

fn option_id(db: &Connection, item_name: &str, slot_order: i64) -> i64 {
    db.query_row(
        "SELECT o.id FROM item_augment_slot_options o JOIN items i ON i.id = o.item_id
          WHERE i.name = ?1 AND o.slot_order = ?2 AND o.option_order = 0",
        params![item_name, slot_order],
        |r| r.get(0),
    )
    .unwrap_or_else(|e| panic!("{item_name} slot {slot_order}: {e}"))
}

fn option_strings(db: &Connection, sql: &str, option_id: i64) -> Vec<String> {
    db.prepare(sql).unwrap().query_map(params![option_id], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

const OPTION_GRANT_LABELS_SQL: &str = "SELECT t.label FROM item_augment_slot_option_grants g
    JOIN augment_slot_types t ON t.id = g.slot_id WHERE g.option_id = ?1 ORDER BY g.sort_order";

const OPTION_BONUSES_SQL: &str = "SELECT s.name || ' ' || bt.name || ' ' || ob.amount
     FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
     JOIN bonus_types bt ON bt.id = ob.bonus_type_id
     WHERE ob.owner_kind = 'item_augment_slot_option' AND ob.owner_id = ?1
     ORDER BY ob.effect_link_order, ob.stat_id";

#[test]
fn keeps_what_an_augment_slot_option_gives_on_the_option() {
    let (db, _) = built_fixture_db();
    let first_tier = option_id(&db, "+3 Combustion Scorched Battle Axe", 0);
    assert_eq!(
        option_strings(&db, OPTION_BONUSES_SQL, first_tier),
        ["Spell Penetration Equipment 1", "Armor Class Insight 1"],
        "the option's simple effects become typed bonuses on the option"
    );
    assert_eq!(
        option_strings(
            &db,
            "SELECT effect_type FROM modifiers WHERE source_kind = 'item_augment_slot_option' AND source_id = ?1 ORDER BY sort_order",
            first_tier
        ),
        ["SpellPenetrationBonus", "ACBonus"]
    );
    let second_tier = option_id(&db, "+3 Combustion Scorched Battle Axe", 1);
    assert_eq!(option_strings(&db, OPTION_GRANT_LABELS_SQL, second_tier), ["purple"]);
    assert_eq!(option_strings(&db, OPTION_BONUSES_SQL, second_tier), ["Fire Spell Lore Equipment 13"]);
    let axe = item_id(&db, "+3 Combustion Scorched Battle Axe");
    assert_eq!(count(&db, &format!("SELECT COUNT(*) FROM item_augment_slots WHERE item_id = {axe}")), 2);
    assert_eq!(
        count(
            &db,
            &format!(
                "SELECT COUNT(*) FROM item_effects ie JOIN effect_bonuses es ON es.effect_id = ie.effect_id
                 JOIN effects s ON s.id = es.target_effect_id WHERE ie.item_id = {axe}
                 AND s.name IN ('Spell Penetration', 'Fire Spell Lore')"
            )
        ),
        0,
        "a tier the player still unlocks gives the item none of its bonuses"
    );

    assert_eq!(option_strings(&db, OPTION_GRANT_LABELS_SQL, option_id(&db, "Sireth, Spear of the Sky", 3)), ["red"]);
    let planar_conflux = option_id(&db, "Sireth, Spear of the Sky", 0);
    let icon: Option<String> = db
        .query_row("SELECT icon FROM item_augment_slot_options WHERE id = ?1", params![planar_conflux], |r| r.get(0))
        .unwrap();
    assert_eq!(icon.as_deref(), Some("Heroism"));
    assert_eq!(
        option_strings(&db, OPTION_GRANT_LABELS_SQL, option_id(&db, "Baz'Morath, the Curator of Decay", 0)),
        ["purple"],
        "<AddAugment> adds a socket as <GrantAugment> does"
    );

    let fabricators_ingenuity = option_id(&db, "Fabricator's Gauntlets", 0);
    assert_eq!(
        option_strings(
            &db,
            "SELECT s.name FROM item_augment_slot_option_sets os JOIN set_bonuses s ON s.id = os.set_id WHERE os.option_id = ?1",
            fabricators_ingenuity
        ),
        ["Fabricator's Ingenuity"]
    );
    let gauntlets = item_id(&db, "Fabricator's Gauntlets");
    assert_eq!(count(&db, &format!("SELECT COUNT(*) FROM set_bonus_items WHERE item_id = {gauntlets}")), 0);
}

#[test]
fn links_items_to_quests_from_drop_location() {
    let (db, report) = built_fixture_db();
    assert_eq!(count(&db, "SELECT COUNT(*) FROM quests WHERE NOT is_challenge AND provenance = 'maetrim'"), 23);
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
            "SELECT q.name, ql.loot_type FROM sources ql JOIN quests q ON q.id = ql.quest_id WHERE ql.item_id = ?1",
            params![sireth],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((quest.as_str(), loot.as_str()), ("Caught in the Web", "raid"), "Caught in the Web is a raid");

    let docent = item_id(&db, "Docent of Defiance");
    let loot: String = db
        .query_row("SELECT loot_type FROM sources WHERE kind = 'quest' AND item_id = ?1", params![docent], |r| r.get(0))
        .unwrap();
    assert_eq!(loot, "chest", "'The Cursed Crypt, End Chest' is a non-raid chest drop");

    let axe = item_id(&db, "+3 Combustion Scorched Battle Axe");
    assert_eq!(count(&db, &format!("SELECT COUNT(*) FROM sources WHERE kind = 'quest' AND item_id = {axe}")), 0);
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
            "SELECT ql.loot_type, ql.chest FROM sources ql JOIN quests q ON q.id = ql.quest_id JOIN items i ON i.id = ql.item_id
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
                "SELECT q.name, qal.loot_type, qal.is_rare, qal.chest FROM sources qal
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
            "SELECT ql.chest FROM sources ql JOIN quests q ON q.id = ql.quest_id JOIN items i ON i.id = ql.item_id
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
                "SELECT q.name, ql.chest FROM sources ql JOIN quests q ON q.id = ql.quest_id
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
                "SELECT q.name FROM sources qal JOIN quests q ON q.id = qal.quest_id
                   JOIN augments a ON a.id = qal.augment_id WHERE a.name = ?1",
            )
            .unwrap();
        statement.query_map([augment], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    };
    assert_eq!(augment_quests("Ruby of Fey Bane"), ["Wake Me Up Inside"]);
    assert_eq!(augment_quests("Storm's Bulwark"), ["The Knight Who Cried Windmill"]);
}

#[test]
fn matches_the_epic_name_a_quest_goes_by() {
    let (db, _) = built_fixture_db();
    assert_eq!(
        quest_loot_chests(&db, "The Prison of the Planes", "Kardin's Eye (Level 24)"),
        [("chest".to_string(), Some("xoriat room's chest".to_string()))],
        "'Return to Prison of the Planes' is the epic name of The Prison of the Planes"
    );
}
#[test]
fn matches_a_quest_name_his_drop_text_wraps_onto_the_next_line() {
    let (db, _) = built_fixture_db();
    let mut statement = db
        .prepare(
            "SELECT q.name, qal.is_rare, qal.chest FROM sources qal JOIN quests q ON q.id = qal.quest_id
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
    assert_eq!(
        coverage.names_only_in_built.len(),
        56,
        "the wiki fixture item, Lindal's Mighty Belt and the legacy fixture items are only in the build"
    );
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
    assert_eq!(count(&db, &format!("SELECT COUNT(*) FROM augment_effects WHERE augment_id = {ruby}")), 0);
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
            "SELECT s.name || ' +' || ob.amount, bt.name FROM owner_bonuses ob
              JOIN effects s ON s.id = ob.stat_id JOIN bonus_types bt ON bt.id = ob.bonus_type_id
              WHERE ob.owner_kind = 'augment' AND ob.owner_id = ?1 ORDER BY ob.effect_link_order, ob.stat_id",
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
        .prepare(
            "SELECT s.name || ' +' || ob.amount
                  FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
                  JOIN augments a ON a.id = ob.owner_id
                  WHERE ob.owner_kind = 'augment' AND a.name = 'Fire I: Combustion'
                  ORDER BY ob.effect_link_order, ob.stat_id",
        )
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
    assert_eq!(report.set_bonus_count, 8, "seven gear sets and one filigree set");
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
        .prepare(
            "SELECT t.equipped_count, e.name FROM set_bonus_tiers t
                  JOIN set_bonus_tier_effects te ON te.tier_id = t.id AND te.sort_order = 0
                  JOIN effects e ON e.id = te.effect_id
                  WHERE t.set_id = ?1 ORDER BY t.equipped_count",
        )
        .unwrap()
        .query_map(params![winter], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(!tiers.is_empty());
    assert!(tiers.iter().all(|(n, _)| *n >= 2));
    let tiers_with_prose_and_structured_facts = count(
        &db,
        "SELECT COUNT(*) FROM set_bonus_tier_effects te JOIN effects prose ON prose.id = te.effect_id
         WHERE prose.is_stat = 0 AND NOT EXISTS (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = te.effect_id)
           AND EXISTS (SELECT 1 FROM set_bonus_tier_effects structured JOIN effects e ON e.id = structured.effect_id
                       WHERE structured.tier_id = te.tier_id AND (e.is_stat = 1 OR EXISTS
                       (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)))",
    );
    assert_eq!(tiers_with_prose_and_structured_facts, 1);
    let tier_modifier_count = count(&db, &format!("SELECT COUNT(*) FROM modifiers m JOIN set_bonus_tiers t ON t.id = m.source_id WHERE m.source_kind = 'set_bonus_tier' AND t.set_id = {winter}"));
    assert!(tier_modifier_count >= 1);
    let winter_tier_bonuses: Vec<(i64, String, i64)> = db
        .prepare(
            "SELECT t.equipped_count, s.name, ob.amount
               FROM set_bonus_tiers t JOIN owner_bonuses ob ON ob.owner_kind = 'set_bonus_tier' AND ob.owner_id = t.id
               JOIN effects s ON s.id = ob.stat_id
              WHERE t.set_id = ?1 ORDER BY t.equipped_count, ob.effect_link_order, ob.stat_id",
        )
        .unwrap()
        .query_map(params![winter], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(
        winter_tier_bonuses.contains(&(2, "Physical Resistance Rating".into(), 30))
            && winter_tier_bonuses.contains(&(6, "Constitution".into(), 3)),
        "a tier's plain stat effects derive bonuses as a feat's do: {winter_tier_bonuses:?}"
    );
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
fn all_ability_set_tier_uses_one_family_with_six_stats() {
    let (db, _) = built_fixture_db();
    let families: Vec<(String, i64)> = db
        .prepare(
            "SELECT e.name, COUNT(es.target_effect_id) FROM set_bonus_tier_effects l
             JOIN set_bonus_tiers t ON t.id = l.tier_id
             JOIN set_bonuses sb ON sb.id = t.set_id
             JOIN effects e ON e.id = l.effect_id
             JOIN effect_bonuses es ON es.effect_id = e.id
             WHERE sb.name = 'Fried & Frozen Frenzy' AND t.equipped_count = 2
             GROUP BY e.id",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(families, [("All Ability Scores".to_string(), 6)]);
    let templates: (String, Option<String>) = db
        .query_row(
            "SELECT e.verbose_name_template, e.description_template FROM effects e
         WHERE e.name = 'All Ability Scores'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        templates,
        ("%b1 All Ability Scores +{1}".to_string(), Some("{1} %b1 bonus to all Ability Scores".to_string()))
    );
}

#[test]
fn set_tier_links_one_group_when_both_members_share_type_and_value() {
    let (db, _) = built_fixture_db();
    let tier_id: i64 = db
        .query_row(
            "SELECT t.id FROM set_bonus_tiers t JOIN set_bonuses s ON s.id = t.set_id
             WHERE s.name = 'Kundarak Delving Equipment' AND t.equipped_count = 3",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let links: Vec<(String, i64, String)> = db
        .prepare(
            "SELECT e.name, l.value, bt.name FROM set_bonus_tier_effects l
             JOIN effects e ON e.id = l.effect_id JOIN bonus_types bt ON bt.id = l.bonus_type_id
             WHERE l.tier_id = ?1 AND e.name IN ('Doublestrike', 'Doubleshot', 'Doublestrike and Doubleshot')",
        )
        .unwrap()
        .query_map([tier_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(links, [("Doublestrike and Doubleshot".to_string(), 5, "Artifact".to_string())]);
    let bonuses: Vec<(String, i64)> = db
        .prepare(
            "SELECT e.name, ob.amount FROM owner_bonuses ob JOIN effects e ON e.id = ob.stat_id
             WHERE ob.owner_kind = 'set_bonus_tier' AND ob.owner_id = ?1
               AND e.name IN ('Doublestrike', 'Doubleshot') ORDER BY e.name",
        )
        .unwrap()
        .query_map([tier_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(bonuses, [("Doubleshot".to_string(), 5), ("Doublestrike".to_string(), 5)]);
}

#[test]
fn links_an_item_to_every_set_it_names() {
    let (db, _) = built_fixture_db();
    let fried_sword_fish = item_id(&db, "Fried Sword Fish");
    let set_names: Vec<String> = db
        .prepare(
            "SELECT s.name FROM set_bonus_items sbi JOIN set_bonuses s ON s.id = sbi.set_id
               WHERE sbi.item_id = ?1 ORDER BY s.name",
        )
        .unwrap()
        .query_map([fried_sword_fish], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(set_names, ["Cooking By the Book", "Fried & Frozen Frenzy"]);
}

#[test]
fn links_sets_to_the_augments_that_grant_them() {
    let (db, _) = built_fixture_db();
    let rows: Vec<(String, String)> = db
        .prepare(
            "SELECT s.name, a.name FROM set_bonus_augments sba JOIN set_bonuses s ON s.id = sba.set_id
               JOIN augments a ON a.id = sba.augment_id WHERE a.provenance = 'maetrim' ORDER BY s.name, a.name",
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

#[test]
fn builds_with_foreign_keys_enforced_on_every_insert() {
    let (db, _) = built_fixture_db();
    let is_enforced: bool = db.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
    assert!(is_enforced, "the build connection enforces foreign keys, so a row naming a missing one fails the build");
    assert_eq!(count(&db, "SELECT COUNT(*) FROM pragma_foreign_key_check()"), 0);
}
