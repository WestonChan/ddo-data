use ddo_etl::build::build_database;
use ddo_etl::corrections::Corrections;
use ddo_etl::wiki::WikiOverrides;
use ddo_etl::xml::{classes, feats, races, stances};
use ddo_model::enums::RequirementGroupKind;
use ddo_model::DatasetVersion;
use rusqlite::{params, Connection};
use std::path::PathBuf;

fn data_files_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles")
}

fn built_fixture_db() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    let dataset_version = DatasetVersion { upstream_sha: "test".into(), built_at: "2026-09-21T00:00:00Z".into() };
    build_database(
        &data_files_fixture_dir(),
        &WikiOverrides::from_dir(&data_files_fixture_dir().parent().unwrap().join("wiki")).unwrap(),
        &Corrections::default(),
        &mut db,
        &dataset_version,
    )
    .expect("build succeeds on fixtures");
    db
}

fn count(db: &Connection, sql: &str) -> i64 {
    db.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn single_value<T: rusqlite::types::FromSql>(db: &Connection, sql: &str, sql_params: &[&dyn rusqlite::ToSql]) -> T {
    db.query_row(sql, sql_params, |r| r.get(0)).unwrap_or_else(|e| panic!("{sql}: {e}"))
}

#[test]
fn parses_feats_with_every_child_shape() {
    let parsed_feats = feats::parse(&data_files_fixture_dir().join("Feats.xml")).unwrap();
    assert_eq!(parsed_feats.len(), 13);
    let power = parsed_feats.iter().find(|f| f.name == "Power Attack").unwrap();
    assert_eq!(power.acquire.as_deref(), Some("Train"));
    assert_eq!(power.groups, vec!["Standard", "Epic Feat"]);
    assert_eq!(power.stances.len(), 1);
    assert_eq!(power.stances[0].incompatible_stances.len(), 4);
    assert_eq!(power.effects.len(), 3, "{:?}", power.effects.iter().map(|e| &e.types).collect::<Vec<_>>());
    let requirements = power.requirements.as_ref().unwrap();
    assert_eq!(
        (requirements.groups[0].requirements[0].kind.as_str(), requirements.groups[0].requirements[0].value.as_deref()),
        ("Ability", Some("13"))
    );

    let toughness = parsed_feats.iter().find(|f| f.name == "Toughness").unwrap();
    assert_eq!(toughness.maximum_times_acquired, Some(99));
    assert_eq!(toughness.effects[0].amount_type.as_deref(), Some("TotalLevel"));
    assert_eq!(toughness.effects[0].amounts.len(), 40);

    let attack = parsed_feats.iter().find(|f| f.name == "Attack").unwrap();
    assert!(attack.attack.is_some());
    assert!(attack.automatic_acquisition.is_some());

    let adept = parsed_feats.iter().find(|f| f.name == "Adept of Forms").unwrap();
    assert_eq!(adept.sub_items.len(), 4);
    assert!(
        parsed_feats.iter().any(|f| !f.conditional_groups.is_empty()),
        "the fixture keeps one ConditionalGroup feat"
    );
}

#[test]
fn parses_races_and_classes() {
    let dwarf = races::parse(&data_files_fixture_dir().join("Races/Dwarf.race.xml")).unwrap();
    assert_eq!(dwarf.name, "Dwarf");
    assert_eq!(dwarf.build_points, vec![28, 32, 34, 36]);
    assert_eq!(dwarf.ability_modifiers, vec![("Constitution".to_string(), 2), ("Charisma".to_string(), -2)]);
    assert_eq!(dwarf.granted_feat_names.len(), 5);
    assert_eq!(dwarf.feats.len(), 5);
    let bladeforged = races::parse(&data_files_fixture_dir().join("Races/Bladeforged.race.xml")).unwrap();
    assert_eq!(bladeforged.iconic_class.as_deref(), Some("Paladin"));
    assert!(bladeforged.is_construct);

    let paladin = classes::parse(&data_files_fixture_dir().join("Classes/Paladin.class.xml")).unwrap();
    assert_eq!(paladin.name, "Paladin");
    assert_eq!((paladin.hit_points, paladin.skill_points), (Some(10), Some(2)));
    assert_eq!(paladin.alignments, vec!["Lawful Good"]);
    assert_eq!(
        paladin.spell_slots_by_class_level.get(&5).map(|v| v[0]),
        Some(1),
        "one first-level slot at class level 5"
    );
    assert_eq!(paladin.spell_slots_by_class_level.get(&20).cloned(), Some(vec![4, 4, 4, 4]));
    assert_eq!(paladin.bab.len(), 21);
    assert!(paladin.class_spells.iter().any(|s| s.name == "Cure Light Wounds" && s.level == 1 && s.cost == Some(6)));
    assert!(paladin.automatic_feats.iter().any(|a| a.level == 2 && a.feat_names.contains(&"Lay on Hands".to_string())));
    let dark_apostate = classes::parse(&data_files_fixture_dir().join("Classes/DarkApostate.class.xml")).unwrap();
    assert_eq!(dark_apostate.base_class.as_deref(), Some("Cleric"));
}

#[test]
fn writes_feats_from_all_three_sources() {
    let db = built_fixture_db();
    assert_eq!(count(&db, "SELECT COUNT(*) FROM feats WHERE source_kind = 'standard'"), 13);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM feats WHERE source_kind = 'race'"), 7, "5 Dwarf + 2 Bladeforged");
    assert!(count(&db, "SELECT COUNT(*) FROM feats WHERE source_kind = 'class'") >= 13);

    let power: i64 =
        single_value(&db, "SELECT id FROM feats WHERE name = 'Power Attack' AND source_kind = 'standard'", &[]);
    let groups: i64 = single_value(&db, "SELECT COUNT(*) FROM feat_groups WHERE feat_id = ?1", &[&power]);
    assert_eq!(groups, 2);
    let (kind, items, value): (String, String, String) = db
        .query_row(
            "SELECT req_type, items, value FROM requirements WHERE owner_kind = 'feat' AND owner_id = ?1",
            params![power],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((kind.as_str(), items.as_str(), value.as_str()), ("Ability", r#"["Strength"]"#, "13"));
    let (stance_name, incompatible): (String, String) = db
        .query_row(
            "SELECT name, incompatible FROM stances WHERE owner_kind = 'feat' AND owner_id = ?1",
            params![power],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(stance_name, "Power Attack");
    assert!(incompatible.contains("Combat Expertise"));
    let modifier_count: i64 =
        single_value(&db, "SELECT COUNT(*) FROM modifiers WHERE source_kind = 'feat' AND source_id = ?1", &[&power]);
    assert_eq!(modifier_count, 3);
    let stance_reqs: i64 = single_value(
        &db,
        "SELECT COUNT(*) FROM requirements r JOIN modifiers m ON m.id = r.owner_id WHERE r.owner_kind = 'modifier' AND m.source_kind = 'feat' AND m.source_id = ?1 AND r.req_type = 'Stance'",
        &[&power],
    );
    assert_eq!(stance_reqs, 3);

    let toughness: i64 = single_value(&db, "SELECT id FROM feats WHERE name = 'Toughness'", &[]);
    let maximum_times_acquired: i64 =
        single_value(&db, "SELECT max_times_acquire FROM feats WHERE id = ?1", &[&toughness]);
    assert_eq!(maximum_times_acquired, 99);
    assert_eq!(
        count(&db, &format!("SELECT COUNT(*) FROM feat_effects WHERE feat_id = {toughness}")),
        0,
        "a TotalLevel vector is not a simple bonus"
    );

    let adept: i64 = single_value(&db, "SELECT id FROM feats WHERE name = 'Adept of Forms'", &[]);
    assert_eq!(count(&db, &format!("SELECT COUNT(*) FROM feat_sub_items WHERE feat_id = {adept}")), 4);
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM attacks WHERE owner_kind = 'feat'"),
        3,
        "Attack, Cleave and Improved Feint each carry an <Attack>"
    );
    assert!(count(&db, "SELECT COUNT(*) FROM feat_conditional_groups") >= 1);
    assert!(count(&db, "SELECT COUNT(*) FROM requirements WHERE owner_kind = 'feat_conditional_group'") >= 1);
    assert!(count(&db, "SELECT COUNT(*) FROM requirements WHERE owner_kind = 'feat_auto_acquire'") >= 1);

    let dwarven_stability: i64 =
        single_value(&db, "SELECT id FROM feats WHERE name = 'Dwarven Stability' AND source_kind = 'race'", &[]);
    let (bonus, bonus_type): (String, String) = db
        .query_row(
            "SELECT s.name || ' +' || ob.amount, bt.name FROM owner_bonuses ob
                    JOIN effects s ON s.id = ob.stat_id
                    JOIN bonus_types bt ON bt.id = ob.bonus_type_id
                    WHERE ob.owner_kind = 'feat' AND ob.owner_id = ?1",
            params![dwarven_stability],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((bonus.as_str(), bonus_type.as_str()), ("Balance +4", "Feat"));
}

#[test]
fn writes_races() {
    let db = built_fixture_db();
    assert_eq!(count(&db, "SELECT COUNT(*) FROM races"), 2);
    let dwarf: i64 = single_value(&db, "SELECT id FROM races WHERE name = 'Dwarf'", &[]);
    let (points, world): (String, String) = db
        .query_row("SELECT build_points, starting_world FROM races WHERE id = ?1", params![dwarf], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((points.as_str(), world.as_str()), ("[28,32,34,36]", "Eberron"));
    let modifiers: Vec<(String, i64)> = db
        .prepare("SELECT s.name, m.modifier FROM race_ability_modifiers m JOIN effects s ON s.id = m.stat_id WHERE m.race_id = ?1 ORDER BY s.id")
        .unwrap()
        .query_map(params![dwarf], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(modifiers, vec![("Constitution".to_string(), 2), ("Charisma".to_string(), -2)]);
    let granted: Vec<(String, Option<i64>)> = db
        .prepare("SELECT feat_name, feat_id FROM race_granted_feats WHERE race_id = ?1 ORDER BY sort_order")
        .unwrap()
        .query_map(params![dwarf], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(granted.len(), 5);
    assert!(
        granted.iter().filter(|(_, id)| id.is_some()).count() >= 4,
        "race-defined granted feats resolve: {granted:?}"
    );
    let resolved_source: String = single_value(
        &db,
        "SELECT f.source_kind FROM race_granted_feats g JOIN feats f ON f.id = g.feat_id WHERE g.race_id = ?1 AND g.feat_name = 'Dwarven Stability'",
        &[&dwarf],
    );
    assert_eq!(resolved_source, "race");

    let (iconic, construct): (String, bool) = db
        .query_row("SELECT iconic_class, is_construct FROM races WHERE name = 'Bladeforged'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(iconic, "Paladin");
    assert!(construct);
}

#[test]
fn writes_classes() {
    let db = built_fixture_db();
    assert_eq!(count(&db, "SELECT COUNT(*) FROM classes"), 2);
    let paladin: i64 = single_value(&db, "SELECT id FROM classes WHERE name = 'Paladin'", &[]);
    let (hit_points, skill_points, fortitude, reflex, will, alignments, casting_stats): (i64, i64, String, String, String, String, String) = db
        .query_row(
            "SELECT hit_points, skill_points, fortitude, reflex, will, alignments, casting_stats FROM classes WHERE id = ?1",
            params![paladin],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
        )
        .unwrap();
    assert_eq!((hit_points, skill_points), (10, 2));
    assert_eq!((fortitude.as_str(), reflex.as_str(), will.as_str()), ("good", "poor", "poor"));
    assert_eq!(alignments, r#"["Lawful Good"]"#);
    assert_eq!(casting_stats, r#"["Wisdom"]"#);
    assert_eq!(count(&db, &format!("SELECT COUNT(*) FROM class_skills WHERE class_id = {paladin}")), 4);
    let slots: i64 = single_value(
        &db,
        "SELECT slots FROM class_spell_slots WHERE class_id = ?1 AND class_level = 20 AND spell_level = 4",
        &[&paladin],
    );
    assert_eq!(slots, 4);
    assert_eq!(
        count(&db, &format!("SELECT COUNT(*) FROM class_spell_slots WHERE class_id = {paladin} AND slots > 0")),
        46
    );
    let (cost, maximum_caster_level): (i64, i64) = db
        .query_row(
            "SELECT cost, max_caster_level FROM class_spells WHERE class_id = ?1 AND spell_name = 'Cure Light Wounds'",
            params![paladin],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((cost, maximum_caster_level), (6, 5));
    let automatic_feats: Vec<(i64, String, Option<i64>)> = db
        .prepare("SELECT level, feat_name, feat_id FROM class_auto_feats WHERE class_id = ?1 AND level = 1 ORDER BY feat_name")
        .unwrap()
        .query_map(params![paladin], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(
        automatic_feats.iter().any(|(_, n, id)| n == "Heavy Armor Proficiency" && id.is_some()),
        "resolves to the standard feat: {automatic_feats:?}"
    );
    assert!(
        automatic_feats.iter().any(|(_, n, id)| n == "Aura of Good" && id.is_some()),
        "resolves to the class's own feat: {automatic_feats:?}"
    );
    let aura_source: String = single_value(
        &db,
        "SELECT f.source_kind FROM class_auto_feats a JOIN feats f ON f.id = a.feat_id WHERE a.class_id = ?1 AND a.feat_name = 'Aura of Good'",
        &[&paladin],
    );
    assert_eq!(aura_source, "class");
    assert!(count(&db, &format!("SELECT COUNT(*) FROM class_feat_slots WHERE class_id = {paladin}")) >= 1);

    let (base, base_id, not_heroic): (String, Option<i64>, bool) = db
        .query_row("SELECT base_class, base_class_id, not_heroic FROM classes WHERE name = 'Dark Apostate'", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert_eq!(base, "Cleric");
    assert!(base_id.is_none(), "Cleric is not in the fixture, so the name is kept without an id");
    assert!(!not_heroic);
}

#[test]
fn parses_standalone_stances() {
    let parsed_stances = stances::parse(&data_files_fixture_dir().join("Stances.xml")).unwrap();
    let names: Vec<&str> = parsed_stances.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Two Weapon Fighting", "Two Handed Fighting", "Aura of Good"]);
    let two_weapon_fighting = &parsed_stances[0];
    assert_eq!(two_weapon_fighting.group.as_deref(), Some("Auto"));
    assert!(two_weapon_fighting.auto_controlled.is_some());
    let groups: Vec<(RequirementGroupKind, usize)> = two_weapon_fighting
        .requirements
        .as_ref()
        .unwrap()
        .groups
        .iter()
        .map(|g| (g.kind, g.requirements.len()))
        .collect();
    assert_eq!(groups, [(RequirementGroupKind::All, 2), (RequirementGroupKind::NoneOf, 8)]);
    assert!(two_weapon_fighting.effects.is_empty());
}

#[test]
fn writes_standalone_stances_with_their_requirements() {
    let db = built_fixture_db();
    assert_eq!(count(&db, "SELECT COUNT(*) FROM stances WHERE owner_kind = 'standalone'"), 3);
    let (id, group, auto_controlled, sort_order): (i64, String, bool, i64) = db
        .query_row(
            "SELECT id, group_name, auto_controlled, sort_order FROM stances
              WHERE owner_kind = 'standalone' AND name = 'Aura of Good'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!((group.as_str(), auto_controlled, sort_order), ("Auto", true, 2));
    let (req_type, items, value): (String, String, String) = db
        .query_row(
            "SELECT req_type, items, value FROM requirements WHERE owner_kind = 'stance' AND owner_id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((req_type.as_str(), items.as_str(), value.as_str()), ("BaseClassMinLevel", r#"["Paladin"]"#, "1"));
    let two_weapon_fighting: i64 = single_value(
        &db,
        "SELECT id FROM stances WHERE owner_kind = 'standalone' AND name = 'Two Weapon Fighting'",
        &[],
    );
    let none_of: i64 = single_value(
        &db,
        "SELECT COUNT(*) FROM requirements WHERE owner_kind = 'stance' AND owner_id = ?1 AND group_kind = 'none_of'",
        &[&two_weapon_fighting],
    );
    assert_eq!(none_of, 8);
}

fn attack_bonuses(bonuses: Option<&feats::AttackBonuses>) -> Vec<(String, Vec<f64>)> {
    bonuses.map_or_else(Vec::new, |b| b.effects.iter().map(|e| (e.types.join(","), e.amounts.clone())).collect())
}

#[test]
fn parses_feat_attack_cooldown_this_attack_and_follow_on() {
    let parsed_feats = feats::parse(&data_files_fixture_dir().join("Feats.xml")).unwrap();
    let feat_attack = |name: &str| parsed_feats.iter().find(|f| f.name == name).and_then(|f| f.attack.clone()).unwrap();

    let cleave = feat_attack("Cleave");
    assert_eq!(cleave.cooldown_seconds, Some(5));
    assert_eq!(
        attack_bonuses(cleave.this_attack.as_ref()),
        [
            ("BonusDamagePercent".to_string(), vec![20.0]),
            ("BonusThreatRange".to_string(), vec![1.0]),
            ("BonusCriticalMultiplier".to_string(), vec![1.0]),
        ]
    );
    assert!(cleave.follow_on.is_none());

    let feint = feat_attack("Improved Feint");
    assert_eq!(feint.cooldown_seconds, Some(6));
    assert_eq!(attack_bonuses(feint.this_attack.as_ref()), [("BonusDamagePercent".to_string(), vec![20.0])]);
    assert_eq!(feint.follow_on.as_ref().unwrap().duration_seconds, Some(4));
    assert_eq!(
        attack_bonuses(feint.follow_on.as_ref()),
        [("AllowSneakAttack".to_string(), vec![])],
        "an empty flag element is a bonus without amounts"
    );

    let basic = feat_attack("Attack");
    assert!(attack_bonuses(basic.this_attack.as_ref()).is_empty(), "a <ThisAttack> holding only a comment");
}

#[test]
fn writes_feat_attack_cooldown_this_attack_and_follow_on() {
    let db = built_fixture_db();
    let standard_feat_id = |name: &str| -> i64 {
        single_value(&db, "SELECT id FROM feats WHERE name = ?1 AND source_kind = 'standard'", &[&name.to_string()])
    };
    let modifier_rows = |source_kind: &str, source_id: i64| -> Vec<(String, Option<String>)> {
        db.prepare(
            "SELECT effect_type, amounts FROM modifiers WHERE source_kind = ?1 AND source_id = ?2 ORDER BY sort_order",
        )
        .unwrap()
        .query_map(params![source_kind, source_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
    };
    let attack_timings = |feat_id: i64| -> (Option<i64>, Option<i64>) {
        db.query_row(
            "SELECT cooldown_seconds, duration_seconds FROM attacks WHERE owner_kind = 'feat' AND owner_id = ?1",
            params![feat_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    };

    let feint = standard_feat_id("Improved Feint");
    assert_eq!(attack_timings(feint), (Some(6), Some(4)));
    assert_eq!(modifier_rows("feat_follow_on", feint), [("AllowSneakAttack".to_string(), None)]);
    assert_eq!(
        modifier_rows("feat_this_attack", feint),
        [("BonusDamagePercent".to_string(), Some("[20]".to_string()))]
    );

    let cleave = standard_feat_id("Cleave");
    assert_eq!(attack_timings(cleave), (Some(5), None));
    assert_eq!(modifier_rows("feat_this_attack", cleave).len(), 3);
    assert_eq!(modifier_rows("feat_follow_on", cleave), []);
}
