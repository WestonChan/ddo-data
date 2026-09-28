use ddo_etl::build::build;
use ddo_etl::xml::requirements::RequirementGroupKind;
use ddo_etl::xml::{classes, feats, races, stances};
use ddo_model::DatasetVersion;
use rusqlite::{params, Connection};
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles")
}

fn built() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    let version = DatasetVersion { upstream_sha: "test".into(), built_at: "2026-09-21T00:00:00Z".into() };
    build(&fixtures(), &mut conn, &version).expect("build succeeds on fixtures");
    conn
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn one<T: rusqlite::types::FromSql>(conn: &Connection, sql: &str, p: &[&dyn rusqlite::ToSql]) -> T {
    conn.query_row(sql, p, |r| r.get(0)).unwrap_or_else(|e| panic!("{sql}: {e}"))
}

#[test]
fn parses_feats_with_every_child_shape() {
    let list = feats::parse(&fixtures().join("Feats.xml")).unwrap();
    assert_eq!(list.len(), 13);
    let power = list.iter().find(|f| f.name == "Power Attack").unwrap();
    assert_eq!(power.acquire.as_deref(), Some("Train"));
    assert_eq!(power.groups, vec!["Standard", "Epic Feat"]);
    assert_eq!(power.stances.len(), 1);
    assert_eq!(power.stances[0].incompatible.len(), 4);
    assert_eq!(power.effects.len(), 3, "{:?}", power.effects.iter().map(|e| &e.types).collect::<Vec<_>>());
    let reqs = power.requirements.as_ref().unwrap();
    assert_eq!(
        (reqs.groups[0].requirements[0].kind.as_str(), reqs.groups[0].requirements[0].value.as_deref()),
        ("Ability", Some("13"))
    );

    let tough = list.iter().find(|f| f.name == "Toughness").unwrap();
    assert_eq!(tough.max_times_acquire, Some(99));
    assert_eq!(tough.effects[0].amount_type.as_deref(), Some("TotalLevel"));
    assert_eq!(tough.effects[0].amounts.len(), 40);

    let attack = list.iter().find(|f| f.name == "Attack").unwrap();
    assert!(attack.attack.is_some());
    assert!(attack.automatic_acquisition.is_some());

    let adept = list.iter().find(|f| f.name == "Adept of Forms").unwrap();
    assert_eq!(adept.sub_items.len(), 4);
    assert!(list.iter().any(|f| !f.conditional_groups.is_empty()), "the fixture keeps one ConditionalGroup feat");
}

#[test]
fn parses_races_and_classes() {
    let dwarf = races::parse(&fixtures().join("Races/Dwarf.race.xml")).unwrap();
    assert_eq!(dwarf.name, "Dwarf");
    assert_eq!(dwarf.build_points, vec![28, 32, 34, 36]);
    assert_eq!(dwarf.ability_modifiers, vec![("Constitution".to_string(), 2), ("Charisma".to_string(), -2)]);
    assert_eq!(dwarf.granted_feats.len(), 5);
    assert_eq!(dwarf.feats.len(), 5);
    let blade = races::parse(&fixtures().join("Races/Bladeforged.race.xml")).unwrap();
    assert_eq!(blade.iconic_class.as_deref(), Some("Paladin"));
    assert!(blade.is_construct);

    let pal = classes::parse(&fixtures().join("Classes/Paladin.class.xml")).unwrap();
    assert_eq!(pal.name, "Paladin");
    assert_eq!((pal.hit_points, pal.skill_points), (Some(10), Some(2)));
    assert_eq!(pal.alignments, vec!["Lawful Good"]);
    assert_eq!(pal.spell_slots.get(&5).map(|v| v[0]), Some(1), "one first-level slot at class level 5");
    assert_eq!(pal.spell_slots.get(&20).cloned(), Some(vec![4, 4, 4, 4]));
    assert_eq!(pal.bab.len(), 21);
    assert!(pal.class_spells.iter().any(|s| s.name == "Cure Light Wounds" && s.level == 1 && s.cost == Some(6)));
    assert!(pal.automatic_feats.iter().any(|a| a.level == 2 && a.feats.contains(&"Lay on Hands".to_string())));
    let dark = classes::parse(&fixtures().join("Classes/DarkApostate.class.xml")).unwrap();
    assert_eq!(dark.base_class.as_deref(), Some("Cleric"));
}

#[test]
fn writes_feats_from_all_three_sources() {
    let conn = built();
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM feats WHERE source_kind = 'standard'"), 13);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM feats WHERE source_kind = 'race'"), 7, "5 Dwarf + 2 Bladeforged");
    assert!(count(&conn, "SELECT COUNT(*) FROM feats WHERE source_kind = 'class'") >= 13);

    let power: i64 = one(&conn, "SELECT id FROM feats WHERE name = 'Power Attack' AND source_kind = 'standard'", &[]);
    let groups: i64 = one(&conn, "SELECT COUNT(*) FROM feat_groups WHERE feat_id = ?1", &[&power]);
    assert_eq!(groups, 2);
    let (kind, items, value): (String, String, String) = conn
        .query_row(
            "SELECT req_type, items, value FROM requirements WHERE owner_kind = 'feat' AND owner_id = ?1",
            params![power],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((kind.as_str(), items.as_str(), value.as_str()), ("Ability", r#"["Strength"]"#, "13"));
    let (stance_name, incompatible): (String, String) = conn
        .query_row(
            "SELECT name, incompatible FROM stances WHERE owner_kind = 'feat' AND owner_id = ?1",
            params![power],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(stance_name, "Power Attack");
    assert!(incompatible.contains("Combat Expertise"));
    let mods: i64 =
        one(&conn, "SELECT COUNT(*) FROM modifiers WHERE source_kind = 'feat' AND source_id = ?1", &[&power]);
    assert_eq!(mods, 3);
    let stance_reqs: i64 = one(
        &conn,
        "SELECT COUNT(*) FROM requirements r JOIN modifiers m ON m.id = r.owner_id WHERE r.owner_kind = 'modifier' AND m.source_kind = 'feat' AND m.source_id = ?1 AND r.req_type = 'Stance'",
        &[&power],
    );
    assert_eq!(stance_reqs, 3);

    let tough: i64 = one(&conn, "SELECT id FROM feats WHERE name = 'Toughness'", &[]);
    let max: i64 = one(&conn, "SELECT max_times_acquire FROM feats WHERE id = ?1", &[&tough]);
    assert_eq!(max, 99);
    assert_eq!(
        count(&conn, &format!("SELECT COUNT(*) FROM feat_bonuses WHERE feat_id = {tough}")),
        0,
        "a TotalLevel vector is not a simple bonus"
    );

    let adept: i64 = one(&conn, "SELECT id FROM feats WHERE name = 'Adept of Forms'", &[]);
    assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM feat_sub_items WHERE feat_id = {adept}")), 4);
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM attacks WHERE owner_kind = 'feat'"),
        3,
        "Attack, Cleave and Improved Feint each carry an <Attack>"
    );
    assert!(count(&conn, "SELECT COUNT(*) FROM feat_conditional_groups") >= 1);
    assert!(count(&conn, "SELECT COUNT(*) FROM requirements WHERE owner_kind = 'feat_conditional_group'") >= 1);
    assert!(count(&conn, "SELECT COUNT(*) FROM requirements WHERE owner_kind = 'feat_auto_acquire'") >= 1);

    let stab: i64 = one(&conn, "SELECT id FROM feats WHERE name = 'Dwarven Stability' AND source_kind = 'race'", &[]);
    let (bonus, bt): (String, String) = conn
        .query_row("SELECT b.name, bt.name FROM feat_bonuses fb JOIN bonuses b ON b.id = fb.bonus_id JOIN bonus_types bt ON bt.id = b.bonus_type_id WHERE fb.feat_id = ?1", params![stab], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert_eq!((bonus.as_str(), bt.as_str()), ("Balance +4", "Feat"));
}

#[test]
fn writes_races() {
    let conn = built();
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM races"), 2);
    let dwarf: i64 = one(&conn, "SELECT id FROM races WHERE name = 'Dwarf'", &[]);
    let (points, world): (String, String) = conn
        .query_row("SELECT build_points, starting_world FROM races WHERE id = ?1", params![dwarf], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((points.as_str(), world.as_str()), ("[28,32,34,36]", "Eberron"));
    let mods: Vec<(String, i64)> = conn
        .prepare("SELECT s.name, m.modifier FROM race_ability_modifiers m JOIN stats s ON s.id = m.stat_id WHERE m.race_id = ?1 ORDER BY s.id")
        .unwrap()
        .query_map(params![dwarf], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(mods, vec![("Constitution".to_string(), 2), ("Charisma".to_string(), -2)]);
    let granted: Vec<(String, Option<i64>)> = conn
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
    let resolved_source: String = one(
        &conn,
        "SELECT f.source_kind FROM race_granted_feats g JOIN feats f ON f.id = g.feat_id WHERE g.race_id = ?1 AND g.feat_name = 'Dwarven Stability'",
        &[&dwarf],
    );
    assert_eq!(resolved_source, "race");

    let (iconic, construct): (String, bool) = conn
        .query_row("SELECT iconic_class, is_construct FROM races WHERE name = 'Bladeforged'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(iconic, "Paladin");
    assert!(construct);
}

#[test]
fn writes_classes() {
    let conn = built();
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM classes"), 2);
    let pal: i64 = one(&conn, "SELECT id FROM classes WHERE name = 'Paladin'", &[]);
    let (hp, sp, fort, reflex, will, align, casting): (i64, i64, String, String, String, String, String) = conn
        .query_row(
            "SELECT hit_points, skill_points, fortitude, reflex, will, alignments, casting_stats FROM classes WHERE id = ?1",
            params![pal],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
        )
        .unwrap();
    assert_eq!((hp, sp), (10, 2));
    assert_eq!((fort.as_str(), reflex.as_str(), will.as_str()), ("good", "poor", "poor"));
    assert_eq!(align, r#"["Lawful Good"]"#);
    assert_eq!(casting, r#"["Wisdom"]"#);
    assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM class_skills WHERE class_id = {pal}")), 4);
    let slots: i64 = one(
        &conn,
        "SELECT slots FROM class_spell_slots WHERE class_id = ?1 AND class_level = 20 AND spell_level = 4",
        &[&pal],
    );
    assert_eq!(slots, 4);
    assert_eq!(
        count(&conn, &format!("SELECT COUNT(*) FROM class_spell_slots WHERE class_id = {pal} AND slots > 0")),
        46
    );
    let (cost, mcl): (i64, i64) = conn
        .query_row(
            "SELECT cost, max_caster_level FROM class_spells WHERE class_id = ?1 AND spell_name = 'Cure Light Wounds'",
            params![pal],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((cost, mcl), (6, 5));
    let auto: Vec<(i64, String, Option<i64>)> = conn
        .prepare("SELECT level, feat_name, feat_id FROM class_auto_feats WHERE class_id = ?1 AND level = 1 ORDER BY feat_name")
        .unwrap()
        .query_map(params![pal], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(
        auto.iter().any(|(_, n, id)| n == "Heavy Armor Proficiency" && id.is_some()),
        "resolves to the standard feat: {auto:?}"
    );
    assert!(
        auto.iter().any(|(_, n, id)| n == "Aura of Good" && id.is_some()),
        "resolves to the class's own feat: {auto:?}"
    );
    let aura_source: String = one(
        &conn,
        "SELECT f.source_kind FROM class_auto_feats a JOIN feats f ON f.id = a.feat_id WHERE a.class_id = ?1 AND a.feat_name = 'Aura of Good'",
        &[&pal],
    );
    assert_eq!(aura_source, "class");
    assert!(count(&conn, &format!("SELECT COUNT(*) FROM class_feat_slots WHERE class_id = {pal}")) >= 1);

    let (base, base_id, not_heroic): (String, Option<i64>, bool) = conn
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
    let list = stances::parse(&fixtures().join("Stances.xml")).unwrap();
    let names: Vec<&str> = list.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Two Weapon Fighting", "Two Handed Fighting", "Aura of Good"]);
    let twf = &list[0];
    assert_eq!(twf.group.as_deref(), Some("Auto"));
    assert!(twf.auto_controlled.is_some());
    let groups: Vec<(RequirementGroupKind, usize)> =
        twf.requirements.as_ref().unwrap().groups.iter().map(|g| (g.kind, g.requirements.len())).collect();
    assert_eq!(groups, [(RequirementGroupKind::All, 2), (RequirementGroupKind::NoneOf, 8)]);
    assert!(twf.effects.is_empty());
}

#[test]
fn writes_standalone_stances_with_their_requirements() {
    let conn = built();
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM stances WHERE owner_kind = 'standalone'"), 3);
    let (id, group, auto_controlled, sort_order): (i64, String, bool, i64) = conn
        .query_row(
            "SELECT id, group_name, auto_controlled, sort_order FROM stances
              WHERE owner_kind = 'standalone' AND name = 'Aura of Good'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!((group.as_str(), auto_controlled, sort_order), ("Auto", true, 2));
    let (req_type, items, value): (String, String, String) = conn
        .query_row(
            "SELECT req_type, items, value FROM requirements WHERE owner_kind = 'stance' AND owner_id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((req_type.as_str(), items.as_str(), value.as_str()), ("BaseClassMinLevel", r#"["Paladin"]"#, "1"));
    let twf: i64 =
        one(&conn, "SELECT id FROM stances WHERE owner_kind = 'standalone' AND name = 'Two Weapon Fighting'", &[]);
    let none_of: i64 = one(
        &conn,
        "SELECT COUNT(*) FROM requirements WHERE owner_kind = 'stance' AND owner_id = ?1 AND group_kind = 'none_of'",
        &[&twf],
    );
    assert_eq!(none_of, 8);
}

fn attack_bonuses(bonuses: Option<&feats::AttackBonuses>) -> Vec<(String, Vec<f64>)> {
    bonuses.map_or_else(Vec::new, |b| b.effects.iter().map(|e| (e.types.join(","), e.amounts.clone())).collect())
}

#[test]
fn parses_feat_attack_cooldown_this_attack_and_follow_on() {
    let list = feats::parse(&fixtures().join("Feats.xml")).unwrap();
    let attack = |name: &str| list.iter().find(|f| f.name == name).and_then(|f| f.attack.clone()).unwrap();

    let cleave = attack("Cleave");
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

    let feint = attack("Improved Feint");
    assert_eq!(feint.cooldown_seconds, Some(6));
    assert_eq!(attack_bonuses(feint.this_attack.as_ref()), [("BonusDamagePercent".to_string(), vec![20.0])]);
    assert_eq!(feint.follow_on.as_ref().unwrap().duration_seconds, Some(4));
    assert_eq!(
        attack_bonuses(feint.follow_on.as_ref()),
        [("AllowSneakAttack".to_string(), vec![])],
        "an empty flag element is a bonus without amounts"
    );

    let basic = attack("Attack");
    assert!(attack_bonuses(basic.this_attack.as_ref()).is_empty(), "a <ThisAttack> holding only a comment");
}

#[test]
fn writes_feat_attack_cooldown_this_attack_and_follow_on() {
    let conn = built();
    let feat = |name: &str| -> i64 {
        one(&conn, "SELECT id FROM feats WHERE name = ?1 AND source_kind = 'standard'", &[&name.to_string()])
    };
    let modifiers = |kind: &str, id: i64| -> Vec<(String, Option<String>)> {
        conn.prepare(
            "SELECT effect_type, amounts FROM modifiers WHERE source_kind = ?1 AND source_id = ?2 ORDER BY sort_order",
        )
        .unwrap()
        .query_map(params![kind, id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
    };
    let timings = |id: i64| -> (Option<i64>, Option<i64>) {
        conn.query_row(
            "SELECT cooldown_seconds, duration_seconds FROM attacks WHERE owner_kind = 'feat' AND owner_id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    };

    let feint = feat("Improved Feint");
    assert_eq!(timings(feint), (Some(6), Some(4)));
    assert_eq!(modifiers("feat_follow_on", feint), [("AllowSneakAttack".to_string(), None)]);
    assert_eq!(modifiers("feat_this_attack", feint), [("BonusDamagePercent".to_string(), Some("[20]".to_string()))]);

    let cleave = feat("Cleave");
    assert_eq!(timings(cleave), (Some(5), None));
    assert_eq!(modifiers("feat_this_attack", cleave).len(), 3);
    assert_eq!(modifiers("feat_follow_on", cleave), []);
}
