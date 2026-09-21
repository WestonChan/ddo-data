//! Enhancement trees and spells, and the by-name spell references they resolve.

use ddo_etl::build::build;
use ddo_etl::xml::{spells, trees};
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

#[test]
fn parses_a_racial_tree() {
    let tree = trees::parse(&fixtures().join("EnhancementTrees/Aasimar.tree.xml")).unwrap();
    assert_eq!(tree.name, "Aasimar");
    assert!(tree.is_racial && !tree.is_destiny && !tree.is_legacy);
    assert_eq!(tree.items.len(), 21);
    let core = &tree.items[0];
    assert_eq!(
        (core.internal_name.as_str(), core.x, core.y, core.ranks, core.min_spent),
        ("AasimarCore1", Some(0), Some(0), Some(1), Some(0))
    );
    assert_eq!(core.cost_per_rank, vec![1.0]);
    assert_eq!(core.arrows, vec!["ArrowRight"]);
    assert_eq!(core.effects.len(), 1);
    let chooser = &tree.items[1];
    let sel = chooser.selector.as_ref().unwrap();
    assert_eq!(sel.selections.len(), 3);
    assert_eq!(sel.selections[0].name, "+1 Strength");
    let reqs = chooser.requirements.as_ref().unwrap();
    assert_eq!(
        reqs.groups[0].requirements.iter().map(|r| r.kind.as_str()).collect::<Vec<_>>(),
        vec!["Level", "Enhancement"]
    );
    assert_eq!(tree.requirements.as_ref().unwrap().groups[0].requirements[0].items, vec!["Aasimar"]);
}

#[test]
fn parses_a_class_tree_with_tier5_and_exclusions() {
    let tree = trees::parse(&fixtures().join("EnhancementTrees/Rogue_Assassin.tree.xml")).unwrap();
    assert!(!tree.is_racial);
    assert_eq!(tree.items.iter().filter(|i| i.is_tier5).count(), 5);
    let with_exclusions =
        tree.items.iter().find(|i| i.selector.as_ref().is_some_and(|s| !s.exclusions.is_empty())).unwrap();
    assert!(with_exclusions.selector.as_ref().unwrap().exclusions[0].starts_with("AssPoisonStrikes"));
    assert!(tree.items.iter().any(|i| i.is_clickie));
}

#[test]
fn parses_spells() {
    let list = spells::parse(&fixtures().join("Spells.xml")).unwrap();
    assert_eq!(list.len(), 19, "the fixture keeps both Dominate Person definitions");
    let shock = list.iter().find(|s| s.name == "Static Shock").unwrap();
    assert_eq!(shock.schools, vec!["Evocation"]);
    assert_eq!(shock.max_caster_level, Some(10));
    assert_eq!(shock.metamagics.len(), 7);
    assert!(shock.metamagics.contains(&"Quicken".to_string()));
    let dmg = &shock.damage[0];
    assert_eq!(dmg.base_dice().map(|d| (d.number, d.sides)), Some((Some(1), Some(6))));
    assert_eq!(dmg.dice.per_caster_levels, Some(2));
    assert_eq!((dmg.damage.as_deref(), dmg.spell_power.as_deref()), (Some("Electric"), Some("Electric")));
    let dc = &shock.dcs[0];
    assert_eq!((dc.dc_type.as_deref(), dc.dc_versus.as_deref()), (Some("Half Damage"), Some("Reflex")));
    assert!(dc.casting_stat_mod());
    let ruin = list.iter().find(|s| s.name == "Ruin").unwrap();
    assert!(ruin.cost.is_some());
    let evo = list.iter().find(|s| s.name == "Evolution").unwrap();
    assert_eq!(evo.stances.len(), 6);
    assert_eq!(evo.effects.len(), 6);
}

#[test]
fn writes_trees_enhancements_and_selections() {
    let conn = built();
    let kinds: Vec<(String, String)> = conn
        .prepare("SELECT name, kind FROM enhancement_trees ORDER BY name")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("Aasimar".to_string(), "racial".to_string()),
            ("Assassin".to_string(), "class".to_string()),
            ("Legendary Dreadnought".to_string(), "destiny".to_string()),
        ]
    );
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM requirements WHERE owner_kind = 'enhancement_tree'"), 3);
    let aasimar: i64 =
        conn.query_row("SELECT id FROM enhancement_trees WHERE name = 'Aasimar'", [], |r| r.get(0)).unwrap();
    assert_eq!(count(&conn, &format!("SELECT COUNT(*) FROM enhancements WHERE tree_id = {aasimar}")), 21);
    let (x, y, cost, arrows): (i64, i64, String, String) = conn
        .query_row("SELECT x, y, cost_per_rank, arrows FROM enhancements WHERE tree_id = ?1 AND internal_name = 'AasimarCore1'", params![aasimar], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap();
    assert_eq!((x, y, cost.as_str(), arrows.as_str()), (0, 0, "[1]", r#"["ArrowRight"]"#));
    let chooser: i64 = conn
        .query_row(
            "SELECT id FROM enhancements WHERE tree_id = ?1 AND internal_name = 'AasimarCore2'",
            params![aasimar],
            |r| r.get(0),
        )
        .unwrap();
    let selections: Vec<String> = conn
        .prepare("SELECT name FROM enhancement_selections WHERE enhancement_id = ?1 ORDER BY sort_order")
        .unwrap()
        .query_map(params![chooser], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(selections, vec!["+1 Strength", "+1 Wisdom", "+1 Charisma"]);
    assert_eq!(
        count(
            &conn,
            &format!("SELECT COUNT(*) FROM requirements WHERE owner_kind = 'enhancement' AND owner_id = {chooser}")
        ),
        2
    );
    assert!(count(&conn, "SELECT COUNT(*) FROM requirements WHERE owner_kind = 'enhancement_selection'") >= 3);
    assert!(count(&conn, "SELECT COUNT(*) FROM modifiers WHERE source_kind = 'enhancement_selection'") >= 3);
    // The +1 Strength selection derives a plain bonus, stored as a modifier with a mapped bonus type.
    let str_sel: i64 = conn
        .query_row(
            "SELECT id FROM enhancement_selections WHERE name = '+1 Strength' AND enhancement_id = ?1",
            params![chooser],
            |r| r.get(0),
        )
        .unwrap();
    let (etype, bt): (String, String) = conn
        .query_row("SELECT m.effect_type, bt.name FROM modifiers m JOIN bonus_types bt ON bt.id = m.bonus_type_id WHERE m.source_kind = 'enhancement_selection' AND m.source_id = ?1", params![str_sel], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert_eq!((etype.as_str(), bt.as_str()), ("AbilityBonus", "Enhancement"));

    let assassin: i64 =
        conn.query_row("SELECT id FROM enhancement_trees WHERE name = 'Assassin'", [], |r| r.get(0)).unwrap();
    assert_eq!(
        count(&conn, &format!("SELECT COUNT(*) FROM enhancements WHERE tree_id = {assassin} AND is_tier5 = 1")),
        5
    );
    assert!(count(&conn, &format!("SELECT COUNT(*) FROM enhancement_selector_exclusions x JOIN enhancements e ON e.id = x.enhancement_id WHERE e.tree_id = {assassin}")) >= 2);
    assert!(
        count(&conn, &format!("SELECT COUNT(*) FROM enhancements WHERE tree_id = {assassin} AND is_clickie = 1")) >= 1
    );
    assert!(
        count(&conn, "SELECT COUNT(*) FROM stances WHERE owner_kind IN ('enhancement', 'enhancement_selection')") >= 1
    );
}

#[test]
fn writes_spells_and_resolves_references() {
    let conn = built();
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM spells"), 18, "duplicate Dominate Person collapses to one row");
    let shock: i64 = conn.query_row("SELECT id FROM spells WHERE name = 'Static Shock'", [], |r| r.get(0)).unwrap();
    let (schools, mcl, meta): (String, i64, String) = conn
        .query_row("SELECT schools, max_caster_level, metamagics FROM spells WHERE id = ?1", params![shock], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert_eq!((schools.as_str(), mcl), (r#"["Evocation"]"#, 10));
    assert!(meta.contains("Maximize") && meta.contains("Quicken"));
    let (n, s, per, bn, dmg, power): (i64, i64, i64, i64, String, String) = conn
        .query_row(
            "SELECT base_dice_number, base_dice_sides, per_caster_levels, bonus_dice_number, damage, spell_power FROM spell_damage WHERE spell_id = ?1",
            params![shock],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .unwrap();
    assert_eq!((n, s, per, bn, dmg.as_str(), power.as_str()), (1, 6, 2, 1, "Electric", "Electric"));
    let (dc_type, versus, csm): (String, String, bool) = conn
        .query_row(
            "SELECT dc_type, dc_versus, casting_stat_mod FROM spell_dcs WHERE spell_id = ?1",
            params![shock],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((dc_type.as_str(), versus.as_str(), csm), ("Half Damage", "Reflex", true));
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM stances WHERE owner_kind = 'spell'"),
        18,
        "Evolution, Lesser and Greater carry six stances each"
    );
    assert!(count(&conn, "SELECT COUNT(*) FROM modifiers WHERE source_kind = 'spell'") >= 18);

    // Class spell lists resolve to spell rows once spells exist.
    let (resolved, total): (i64, i64) = conn
        .query_row("SELECT SUM(spell_id IS NOT NULL), COUNT(*) FROM class_spells cs JOIN classes c ON c.id = cs.class_id WHERE c.name = 'Paladin'", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert!(resolved >= 12 && resolved < total, "only the fixture's spells resolve: {resolved}/{total}");
    let clw: i64 = conn
        .query_row("SELECT s.id FROM class_spells cs JOIN spells s ON s.id = cs.spell_id WHERE cs.spell_name = 'Cure Light Wounds'", [], |r| r.get(0))
        .unwrap();
    assert!(clw > 0);
}
