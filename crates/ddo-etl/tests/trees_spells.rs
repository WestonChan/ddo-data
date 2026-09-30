use ddo_etl::build::build_database;
use ddo_etl::corrections::Corrections;
use ddo_etl::wiki::WikiOverrides;
use ddo_etl::xml::{spells, trees};
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

#[test]
fn parses_a_racial_tree() {
    let tree = trees::parse(&data_files_fixture_dir().join("EnhancementTrees/Aasimar.tree.xml")).unwrap();
    assert_eq!(tree.name, "Aasimar");
    assert!(tree.is_racial && !tree.is_destiny && !tree.is_legacy);
    assert_eq!(tree.enhancements.len(), 21);
    let core = &tree.enhancements[0];
    assert_eq!(
        (core.internal_name.as_str(), core.x, core.y, core.rank_count, core.minimum_points_spent),
        ("AasimarCore1", Some(0), Some(0), Some(1), Some(0))
    );
    assert_eq!(core.cost_per_rank, vec![1.0]);
    assert_eq!(core.arrows, vec!["ArrowRight"]);
    assert_eq!(core.effects.len(), 1);
    let chooser = &tree.enhancements[1];
    let selector = chooser.selector.as_ref().unwrap();
    assert_eq!(selector.selections.len(), 3);
    assert_eq!(selector.selections[0].name, "+1 Strength");
    let requirements = chooser.requirements.as_ref().unwrap();
    assert_eq!(
        requirements.groups[0].requirements.iter().map(|r| r.kind.as_str()).collect::<Vec<_>>(),
        vec!["Level", "Enhancement"]
    );
    assert_eq!(tree.requirements.as_ref().unwrap().groups[0].requirements[0].items, vec!["Aasimar"]);
}

#[test]
fn parses_a_class_tree_with_tier5_and_exclusions() {
    let tree = trees::parse(&data_files_fixture_dir().join("EnhancementTrees/Rogue_Assassin.tree.xml")).unwrap();
    assert!(!tree.is_racial);
    assert_eq!(tree.enhancements.iter().filter(|i| i.is_tier5).count(), 5);
    let with_exclusions = tree
        .enhancements
        .iter()
        .find(|i| i.selector.as_ref().is_some_and(|s| !s.excluded_internal_names.is_empty()))
        .unwrap();
    assert!(with_exclusions.selector.as_ref().unwrap().excluded_internal_names[0].starts_with("AssPoisonStrikes"));
    assert!(tree.enhancements.iter().any(|i| i.is_clickie));
}

#[test]
fn parses_attack_cooldown_duration_and_follow_on() {
    let tree = trees::parse(&data_files_fixture_dir().join("EnhancementTrees/Fighter_Kensei.tree.xml")).unwrap();
    let follow_on_effects = |attack: &ddo_etl::xml::feats::Attack| {
        attack
            .follow_on
            .as_ref()
            .unwrap()
            .effects
            .iter()
            .map(|e| (e.types.join(","), e.amount_type.clone().unwrap_or_default(), e.amounts.clone()))
            .collect::<Vec<_>>()
    };
    let surge = tree.enhancements.iter().find(|i| i.internal_name == "KenseiCore4").unwrap();
    let attack = surge.attack.as_ref().unwrap();
    assert_eq!(attack.cooldown_seconds, Some(60));
    assert_eq!(attack.follow_on.as_ref().unwrap().duration_seconds, Some(60));
    assert_eq!(
        follow_on_effects(attack),
        vec![
            ("BonusAttackBonus".to_string(), "Simple".to_string(), vec![4.0]),
            ("BonusDamage".to_string(), "Simple".to_string(), vec![4.0]),
        ],
        "every FollowOn child but Duration is an effect; the XML comment inside is skipped"
    );

    let boost = tree.enhancements.iter().find(|i| i.internal_name == "KenseiActionBoostI").unwrap();
    let haste = &boost.selector.as_ref().unwrap().selections[1];
    let attack = haste.attack.as_ref().unwrap();
    assert_eq!(attack.cooldown_seconds, Some(30), "a per-rank vector keeps its first value");
    assert_eq!(attack.follow_on.as_ref().unwrap().duration_seconds, Some(20));
    assert_eq!(
        follow_on_effects(attack),
        vec![("BonusAlacrity".to_string(), "Stacks".to_string(), vec![10.0, 20.0, 30.0])]
    );
}

#[test]
fn parses_spells() {
    let parsed_spells = spells::parse(&data_files_fixture_dir().join("Spells.xml")).unwrap();
    assert_eq!(parsed_spells.len(), 19, "the fixture keeps both Dominate Person definitions");
    let shock = parsed_spells.iter().find(|s| s.name == "Static Shock").unwrap();
    assert_eq!(shock.schools, vec!["Evocation"]);
    assert_eq!(shock.maximum_caster_level, Some(10));
    assert_eq!(shock.metamagics.len(), 7);
    assert!(shock.metamagics.contains(&"Quicken".to_string()));
    let damage = &shock.damage_components[0];
    assert_eq!(damage.base_dice().map(|d| (d.count, d.sides)), Some((Some(1), Some(6))));
    assert_eq!(damage.dice.per_caster_levels, Some(2));
    assert_eq!((damage.damage.as_deref(), damage.spell_power.as_deref()), (Some("Electric"), Some("Electric")));
    let dc = &shock.dcs[0];
    assert_eq!((dc.dc_type.as_deref(), dc.dc_versus.as_deref()), (Some("Half Damage"), Some("Reflex")));
    assert!(dc.adds_casting_stat_modifier());
    let ruin = parsed_spells.iter().find(|s| s.name == "Ruin").unwrap();
    assert!(ruin.cost.is_some());
    let evolution = parsed_spells.iter().find(|s| s.name == "Evolution").unwrap();
    assert_eq!(evolution.stances.len(), 6);
    assert_eq!(evolution.effects.len(), 6);
}

#[test]
fn writes_trees_enhancements_and_selections() {
    let db = built_fixture_db();
    let tree_kinds: Vec<(String, String)> = db
        .prepare("SELECT name, kind FROM enhancement_trees ORDER BY name")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        tree_kinds,
        vec![
            ("Aasimar".to_string(), "racial".to_string()),
            ("Assassin".to_string(), "class".to_string()),
            ("Kensei".to_string(), "class".to_string()),
            ("Legendary Dreadnought".to_string(), "destiny".to_string()),
        ]
    );
    assert_eq!(count(&db, "SELECT COUNT(*) FROM requirements WHERE owner_kind = 'enhancement_tree'"), 4);
    let aasimar: i64 =
        db.query_row("SELECT id FROM enhancement_trees WHERE name = 'Aasimar'", [], |r| r.get(0)).unwrap();
    assert_eq!(count(&db, &format!("SELECT COUNT(*) FROM enhancements WHERE tree_id = {aasimar}")), 21);
    let (x, y, cost_per_rank, arrows): (i64, i64, String, String) = db
        .query_row("SELECT x, y, cost_per_rank, arrows FROM enhancements WHERE tree_id = ?1 AND internal_name = 'AasimarCore1'", params![aasimar], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap();
    assert_eq!((x, y, cost_per_rank.as_str(), arrows.as_str()), (0, 0, "[1]", r#"["ArrowRight"]"#));
    let chooser: i64 = db
        .query_row(
            "SELECT id FROM enhancements WHERE tree_id = ?1 AND internal_name = 'AasimarCore2'",
            params![aasimar],
            |r| r.get(0),
        )
        .unwrap();
    let selections: Vec<String> = db
        .prepare("SELECT name FROM enhancement_selections WHERE enhancement_id = ?1 ORDER BY sort_order")
        .unwrap()
        .query_map(params![chooser], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(selections, vec!["+1 Strength", "+1 Wisdom", "+1 Charisma"]);
    assert_eq!(
        count(
            &db,
            &format!("SELECT COUNT(*) FROM requirements WHERE owner_kind = 'enhancement' AND owner_id = {chooser}")
        ),
        2
    );
    assert!(count(&db, "SELECT COUNT(*) FROM requirements WHERE owner_kind = 'enhancement_selection'") >= 3);
    assert!(count(&db, "SELECT COUNT(*) FROM modifiers WHERE source_kind = 'enhancement_selection'") >= 3);
    let strength_selection: i64 = db
        .query_row(
            "SELECT id FROM enhancement_selections WHERE name = '+1 Strength' AND enhancement_id = ?1",
            params![chooser],
            |r| r.get(0),
        )
        .unwrap();
    let (effect_type, bonus_type): (String, String) = db
        .query_row("SELECT m.effect_type, bt.name FROM modifiers m JOIN bonus_types bt ON bt.id = m.bonus_type_id WHERE m.source_kind = 'enhancement_selection' AND m.source_id = ?1", params![strength_selection], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert_eq!((effect_type.as_str(), bonus_type.as_str()), ("AbilityBonus", "Enhancement"));

    let assassin: i64 =
        db.query_row("SELECT id FROM enhancement_trees WHERE name = 'Assassin'", [], |r| r.get(0)).unwrap();
    assert_eq!(
        count(&db, &format!("SELECT COUNT(*) FROM enhancements WHERE tree_id = {assassin} AND is_tier5 = 1")),
        5
    );
    assert!(count(&db, &format!("SELECT COUNT(*) FROM enhancement_selector_exclusions x JOIN enhancements e ON e.id = x.enhancement_id WHERE e.tree_id = {assassin}")) >= 2);
    assert!(
        count(&db, &format!("SELECT COUNT(*) FROM enhancements WHERE tree_id = {assassin} AND is_clickie = 1")) >= 1
    );
    assert!(
        count(&db, "SELECT COUNT(*) FROM stances WHERE owner_kind IN ('enhancement', 'enhancement_selection')") >= 1
    );
}

#[test]
fn writes_enhancement_timings_and_follow_on_modifiers() {
    let db = built_fixture_db();
    let attack_timings = |table: &str, name: &str| -> (i64, Option<i64>, Option<i64>) {
        db.query_row(
            &format!("SELECT id, cooldown_seconds, duration_seconds FROM {table} WHERE name = ?1"),
            params![name],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"))
    };
    let modifier_rows = |source_kind: &str, source_id: i64| -> Vec<(String, String)> {
        db.prepare(
            "SELECT effect_type, amounts FROM modifiers WHERE source_kind = ?1 AND source_id = ?2 ORDER BY sort_order",
        )
        .unwrap()
        .query_map(params![source_kind, source_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
    };

    let (surge, cooldown, duration) = attack_timings("enhancements", "Kensei: Power Surge");
    assert_eq!((cooldown, duration), (Some(60), Some(60)));
    assert_eq!(
        modifier_rows("enhancement_follow_on", surge),
        vec![("BonusAttackBonus".to_string(), "[4]".to_string()), ("BonusDamage".to_string(), "[4]".to_string())]
    );
    assert_eq!(
        count(
            &db,
            &format!("SELECT COUNT(*) FROM modifiers WHERE source_kind = 'enhancement' AND source_id = {surge}")
        ),
        3
    );

    let (haste, cooldown, duration) = attack_timings("enhancement_selections", "Haste Boost");
    assert_eq!((cooldown, duration), (Some(30), Some(20)));
    assert_eq!(
        modifier_rows("enhancement_selection_follow_on", haste),
        vec![("BonusAlacrity".to_string(), "[10,20,30]".to_string())]
    );

    let (_, cooldown, duration) = attack_timings("enhancements", "Kensei: Action Boost");
    assert_eq!((cooldown, duration), (None, None), "the selector itself grants no attack");
}

#[test]
fn writes_spells_and_resolves_references() {
    let db = built_fixture_db();
    assert_eq!(count(&db, "SELECT COUNT(*) FROM spells"), 18, "duplicate Dominate Person collapses to one row");
    let shock: i64 = db.query_row("SELECT id FROM spells WHERE name = 'Static Shock'", [], |r| r.get(0)).unwrap();
    let (schools, maximum_caster_level, metamagics): (String, i64, String) = db
        .query_row("SELECT schools, max_caster_level, metamagics FROM spells WHERE id = ?1", params![shock], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert_eq!((schools.as_str(), maximum_caster_level), (r#"["Evocation"]"#, 10));
    assert!(metamagics.contains("Maximize") && metamagics.contains("Quicken"));
    let (base_dice_count, base_dice_sides, per_caster_levels, bonus_dice_count, damage, spell_power): (i64, i64, i64, i64, String, String) = db
        .query_row(
            "SELECT base_dice_number, base_dice_sides, per_caster_levels, bonus_dice_number, damage, spell_power FROM spell_damage WHERE spell_id = ?1",
            params![shock],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .unwrap();
    assert_eq!(
        (base_dice_count, base_dice_sides, per_caster_levels, bonus_dice_count, damage.as_str(), spell_power.as_str()),
        (1, 6, 2, 1, "Electric", "Electric")
    );
    let (dc_type, dc_versus, adds_casting_stat_modifier): (String, String, bool) = db
        .query_row(
            "SELECT dc_type, dc_versus, casting_stat_mod FROM spell_dcs WHERE spell_id = ?1",
            params![shock],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((dc_type.as_str(), dc_versus.as_str(), adds_casting_stat_modifier), ("Half Damage", "Reflex", true));
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM stances WHERE owner_kind = 'spell'"),
        18,
        "Evolution, Lesser and Greater carry six stances each"
    );
    assert!(count(&db, "SELECT COUNT(*) FROM modifiers WHERE source_kind = 'spell'") >= 18);

    let (resolved, total): (i64, i64) = db
        .query_row("SELECT SUM(spell_id IS NOT NULL), COUNT(*) FROM class_spells cs JOIN classes c ON c.id = cs.class_id WHERE c.name = 'Paladin'", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert!(resolved >= 12 && resolved < total, "only the fixture's spells resolve: {resolved}/{total}");
    let cure_light_wounds_id: i64 = db
        .query_row("SELECT s.id FROM class_spells cs JOIN spells s ON s.id = cs.spell_id WHERE cs.spell_name = 'Cure Light Wounds'", [], |r| r.get(0))
        .unwrap();
    assert!(cure_light_wounds_id > 0);
}

#[test]
fn writes_this_attack_modifiers_for_enhancements_and_selections() {
    let db = built_fixture_db();
    let tree = trees::parse(&data_files_fixture_dir().join("EnhancementTrees/Fighter_Kensei.tree.xml")).unwrap();
    let reed = tree.enhancements.iter().find(|i| i.internal_name == "KenseiReedInTheWind").unwrap();
    let this_attack = reed.attack.as_ref().unwrap().this_attack.as_ref().unwrap();
    assert_eq!(this_attack.effects[0].amounts, [20.0, 40.0, 60.0]);

    let this_attack_rows = |source_kind: &str, table: &str, name: &str| -> Vec<(String, String)> {
        db.prepare(&format!(
            "SELECT m.effect_type, m.amounts FROM {table} o JOIN modifiers m ON m.source_kind = ?1 AND m.source_id = o.id
              WHERE o.name = ?2 ORDER BY m.sort_order"
        ))
        .unwrap()
        .query_map(params![source_kind, name], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
    };
    assert_eq!(
        this_attack_rows("enhancement_this_attack", "enhancements", "Kensei: Reed In The Wind"),
        [("BonusDamagePercent".to_string(), "[20,40,60]".to_string())]
    );
    assert_eq!(
        this_attack_rows("enhancement_selection_this_attack", "enhancement_selections", "Shattering Strike"),
        [("BonusDamagePercent".to_string(), "[25,50,100]".to_string())]
    );
    assert_eq!(
        this_attack_rows("enhancement_selection_follow_on", "enhancement_selections", "Shattering Strike"),
        [("FortificationLoss".to_string(), "[15,15,15]".to_string())]
    );
}
