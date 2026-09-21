//! The schema contract: the DDL applies cleanly, the seed tables agree with the enums that
//! describe them, and the corrections recorded in the roadmap's V2 entry are in place.

use ddo_model::enums::{BonusType, EquipmentSlot, SaveProgression, SlotCategory, WeaponProficiency};
use ddo_model::seeds::{BONUS_TYPES, DAMAGE_TYPES, EQUIPMENT_SLOTS, WEAPON_TYPES};
use ddo_model::stats::STATS;
use ddo_model::{ddl, stat_by_name, SCHEMA_VERSION};
use rusqlite::Connection;
use std::collections::HashSet;

fn fresh_db() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory sqlite");
    conn.execute_batch(ddl()).expect("DDL applies to a fresh database");
    conn
}

fn table_names(conn: &Connection) -> HashSet<String> {
    let mut stmt =
        conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'").unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap).collect()
}

#[test]
fn ddl_creates_every_v2_table() {
    let conn = fresh_db();
    let tables = table_names(&conn);
    for expected in [
        "schema_version",
        "stats",
        "bonus_types",
        "equipment_slots",
        "weapon_proficiencies",
        "weapon_types",
        "damage_types",
        "item_materials",
        "augment_slot_types",
        "adventure_packs",
        "patrons",
        "quests",
        "items",
        "excluded_items",
        "item_weapon_stats",
        "item_dr_bypass",
        "item_armor_stats",
        "bonuses",
        "item_bonuses",
        "effects",
        "item_effects",
        "item_augment_slots",
        "item_augment_slot_options",
        "quest_loot",
        "modifiers",
        "requirements",
        "augments",
        "augment_slots",
        "augment_bonuses",
        "set_bonuses",
        "set_bonus_tiers",
        "set_bonus_items",
        "filigrees",
        "clickies",
        "item_clickies",
        "feats",
        "feat_groups",
        "feat_conditional_groups",
        "feat_sub_items",
        "feat_bonuses",
        "stances",
        "dcs",
        "attacks",
        "races",
        "race_ability_modifiers",
        "race_granted_feats",
        "race_feat_slots",
        "classes",
        "class_skills",
        "class_spell_slots",
        "class_spells",
        "class_feat_slots",
        "class_auto_feats",
        "enhancement_trees",
        "enhancements",
        "enhancement_selections",
        "enhancement_selector_exclusions",
        "spells",
        "spell_damage",
        "spell_dcs",
    ] {
        assert!(tables.contains(expected), "missing table {expected}");
    }
}

#[test]
fn ddl_drops_the_columns_with_no_source() {
    let conn = fresh_db();
    let mut stmt = conn.prepare("SELECT name FROM pragma_table_info('items')").unwrap();
    let columns: HashSet<String> = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap).collect();
    for gone in ["dat_id", "rarity", "tooltip", "binding", "base_value", "equipment_slot", "material"] {
        assert!(!columns.contains(gone), "items.{gone} should have been dropped");
    }
    for kept in ["slot_id", "material_id", "drop_location", "set_bonus", "accepts_sentience", "enhancement_bonus"] {
        assert!(columns.contains(kept), "items.{kept} missing");
    }
}

#[test]
fn ddl_is_idempotent() {
    let conn = fresh_db();
    conn.execute_batch(ddl()).expect("re-applying the DDL is a no-op");
    let recorded: i64 =
        conn.query_row("SELECT COALESCE(MAX(version), 0) FROM schema_version", [], |r| r.get(0)).unwrap();
    assert!(recorded <= SCHEMA_VERSION, "a fresh database records no newer version than the code");
}

#[test]
fn stats_have_unique_ids_and_names() {
    let ids: HashSet<i64> = STATS.iter().map(|s| s.id).collect();
    let names: HashSet<&str> = STATS.iter().map(|s| s.name).collect();
    assert_eq!(ids.len(), STATS.len(), "duplicate stat id");
    assert_eq!(names.len(), STATS.len(), "duplicate stat name");
    assert_eq!(stat_by_name("Fire Spell Power").map(|s| s.id), Some(25));
    assert_eq!(stat_by_name("Nope"), None);
}

#[test]
fn equipment_slots_say_hands_not_arms() {
    let names: Vec<&str> = EQUIPMENT_SLOTS.iter().map(|s| s.slot.name()).collect();
    assert!(names.contains(&"Hands"));
    assert!(!names.contains(&"Arms"));
    assert_eq!(EquipmentSlot::Hands.category(), SlotCategory::Armor);
    assert_eq!(EquipmentSlot::MainHand.id(), 1);
    assert_eq!(EQUIPMENT_SLOTS.len(), 16);
}

#[test]
fn bonus_types_include_the_v2_additions() {
    let names: Vec<&str> = BONUS_TYPES.iter().map(|b| b.bonus_type.name()).collect();
    for added in ["Vitality", "False Life", "Legendary", "Penalty"] {
        assert!(names.contains(&added), "{added} missing from bonus_types");
    }
    assert_eq!(BonusType::Enhancement.id(), 1);
    assert_eq!(BonusType::Penalty.id(), 33, "the original 33 ids are stable");
    assert!(BONUS_TYPES.len() >= 73, "his BonusTypes.xml vocabulary is appended");
    assert_eq!(BonusType::parse("Feat").map(BonusType::id), Some(48));
    assert!(BonusType::Destiny.stacks_with_self(), "his 'Always' rule");
    assert!(!BonusType::Feat.stacks_with_self(), "his 'Highest Only' rule");
    assert!(BonusType::parse("Not Set").is_none());
    assert!(BonusType::Dodge.stacks_with_self());
    assert!(!BonusType::Enhancement.stacks_with_self());
    assert_eq!(BonusType::parse("Insight"), Some(BonusType::Insight));
}

#[test]
fn weapon_types_carry_proficiency_and_ddo_spellings() {
    let by_name = |n: &str| WEAPON_TYPES.iter().find(|w| w.name == n).unwrap_or_else(|| panic!("{n}"));
    assert_eq!(by_name("Great Axe").proficiency, Some(WeaponProficiency::Martial));
    assert_eq!(by_name("Khopesh").proficiency, Some(WeaponProficiency::Exotic));
    assert_eq!(by_name("Dagger").proficiency, Some(WeaponProficiency::Simple));
    assert_eq!(by_name("Large Shield").proficiency, None);
    assert!(by_name("Large Shield").is_shield);
    assert!(!by_name("Handwraps").is_shield);
    assert!(WEAPON_TYPES.iter().any(|w| w.name == "Rune Arm"));
    assert!(!WEAPON_TYPES.iter().any(|w| w.name == "Greataxe"));
    assert_eq!(DAMAGE_TYPES.len(), 18);
}

#[test]
fn seed_tables_load_into_the_schema() {
    let conn = fresh_db();
    ddo_model::seeds::insert_all(&conn).expect("seeds insert");
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM stats", [], |r| r.get(0)).unwrap();
    assert_eq!(n as usize, STATS.len());
    let hands: String = conn.query_row("SELECT name FROM equipment_slots WHERE id = 10", [], |r| r.get(0)).unwrap();
    assert_eq!(hands, "Hands");
    let martial: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM weapon_types wt JOIN weapon_proficiencies p ON p.id = wt.proficiency_id WHERE p.name = 'Martial'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(martial > 10);
}

#[test]
fn save_progressions_follow_upstream_type_codes() {
    assert_eq!(SaveProgression::from_upstream("Type2"), Some(SaveProgression::Good), "Paladin Fortitude is Type2");
    assert_eq!(SaveProgression::from_upstream("Type1"), Some(SaveProgression::Poor));
    assert_eq!(SaveProgression::from_upstream("None"), Some(SaveProgression::None));
    assert_eq!(SaveProgression::from_upstream("Type3"), None);
}
