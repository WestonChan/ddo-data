use ddo_model::enums::{BonusType, EquipmentSlot, EquipmentSlotCategory, SaveProgression, WeaponProficiency};
use ddo_model::seeds::{WeaponType, DAMAGE_TYPES, WEAPON_TYPES};
use ddo_model::stats::{Stat, STATS};
use ddo_model::{ddl, SCHEMA_VERSION};
use rusqlite::Connection;
use std::collections::HashSet;

fn fresh_db() -> Connection {
    let db = Connection::open_in_memory().expect("in-memory sqlite");
    db.execute_batch(ddl()).expect("DDL applies to a fresh database");
    db
}

fn table_names(db: &Connection) -> HashSet<String> {
    let mut statement =
        db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'").unwrap();
    statement.query_map([], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap).collect()
}

#[test]
fn ddl_creates_every_v2_table() {
    let db = fresh_db();
    let tables = table_names(&db);
    for expected_table in [
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
        "drops",
        "quest_chains",
        "quest_chain_quests",
        "sagas",
        "saga_quests",
        "modifiers",
        "requirements",
        "augments",
        "augment_slots",
        "augment_bonuses",
        "set_bonuses",
        "set_bonus_tiers",
        "set_bonus_tier_bonuses",
        "set_bonus_items",
        "set_bonus_augments",
        "filigrees",
        "sentient_gems",
        "guild_buffs",
        "optional_buffs",
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
        "crafting_systems",
        "crafting_system_families",
        "crafting_ingredients",
        "crafting_recipes",
        "crafting_recipe_augments",
        "crafting_recipe_ingredients",
        "corrections",
    ] {
        assert!(tables.contains(expected_table), "missing table {expected_table}");
    }
}

#[test]
fn ddl_drops_the_columns_with_no_source() {
    let db = fresh_db();
    let mut statement = db.prepare("SELECT name FROM pragma_table_info('items')").unwrap();
    let columns: HashSet<String> =
        statement.query_map([], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap).collect();
    for dropped_column in ["dat_id", "rarity", "tooltip", "binding", "base_value", "equipment_slot", "material"] {
        assert!(!columns.contains(dropped_column), "items.{dropped_column} should have been dropped");
    }
    for kept_column in
        ["slot_id", "material_id", "drop_location", "set_bonus", "accepts_sentience", "enhancement_bonus"]
    {
        assert!(columns.contains(kept_column), "items.{kept_column} missing");
    }
}

#[test]
fn items_come_from_maetrim_unless_the_wiki_supplied_them() {
    let db = fresh_db();
    ddo_model::seeds::insert_all(&db).unwrap();
    db.execute("INSERT INTO items (name, slot_id, item_category) VALUES ('His Ring', 15, 'Jewelry')", []).unwrap();
    db.execute(
        "INSERT INTO items (name, slot_id, item_category, source) VALUES ('Wiki Ring', 15, 'Jewelry', 'wiki')",
        [],
    )
    .unwrap();
    let sources: Vec<String> = db
        .prepare("SELECT source FROM items ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(sources, ["maetrim", "wiki"]);
    assert!(db
        .execute(
            "INSERT INTO items (name, slot_id, item_category, source) VALUES ('Odd Ring', 15, 'Jewelry', 'ddowiki')",
            []
        )
        .is_err());
}

#[test]
fn quests_come_from_maetrim_unless_the_wiki_supplied_them() {
    let db = fresh_db();
    db.execute("INSERT INTO quests (name) VALUES ('His Quest')", []).unwrap();
    db.execute("INSERT INTO quests (name, source) VALUES ('Wiki Quest', 'wiki')", []).unwrap();
    let sources: Vec<String> = db
        .prepare("SELECT source FROM quests ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(sources, ["maetrim", "wiki"]);
    assert!(db.execute("INSERT INTO quests (name, source) VALUES ('Odd Quest', 'ddowiki')", []).is_err());
}

#[test]
fn augments_come_from_maetrim_unless_the_wiki_supplied_them() {
    let db = fresh_db();
    db.execute("INSERT INTO augments (name, family) VALUES ('His Gem', 'Named')", []).unwrap();
    db.execute("INSERT INTO augments (name, family, source) VALUES ('Wiki Gem', 'Named', 'wiki')", []).unwrap();
    let sources: Vec<String> = db
        .prepare("SELECT source FROM augments ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(sources, ["maetrim", "wiki"]);
    assert!(db
        .execute("INSERT INTO augments (name, family, source) VALUES ('Odd Gem', 'Named', 'ddowiki')", [])
        .is_err());
    assert_eq!(SCHEMA_VERSION, 13);
}

#[test]
fn quests_carry_no_duration_and_no_xp_table_exists() {
    let db = fresh_db();
    assert!(!table_names(&db).contains("quest_xp"));
    let error = db.execute("INSERT INTO quests (id, name, duration) VALUES (1, 'The Grotto', 'Long')", []).unwrap_err();
    assert!(error.to_string().contains("duration"), "{error}");
}

#[test]
fn crafting_tables_accept_only_the_four_crafting_tiers_and_one_ingredient_per_name_and_tier() {
    let db = fresh_db();
    db.execute("INSERT INTO crafting_systems (id, name, page) VALUES (1, 'Green Steel', 'p')", []).unwrap();
    for tier in ["heroic", "epic", "legendary", "any"] {
        db.execute("INSERT INTO crafting_ingredients (system_id, name, tier) VALUES (1, 'Shard', ?1)", [tier]).unwrap();
        db.execute("INSERT INTO crafting_recipes (system_id, tier, option, sort_order) VALUES (1, ?1, 'x', 0)", [tier])
            .unwrap();
    }
    assert!(db
        .execute("INSERT INTO crafting_ingredients (system_id, name, tier) VALUES (1, 'Shard', 'heroic')", [])
        .is_err());
    assert!(db
        .execute("INSERT INTO crafting_ingredients (system_id, name, tier) VALUES (1, 'Gem', 'mythic')", [])
        .is_err());
    assert!(db
        .execute("INSERT INTO crafting_recipes (system_id, tier, option, sort_order) VALUES (1, 'mythic', 'x', 0)", [])
        .is_err());
    assert!(db.execute("INSERT INTO crafting_systems (name, page) VALUES ('Green Steel', 'q')", []).is_err());
}

#[test]
fn corrections_record_each_kind_field_and_value_change_once() {
    let db = fresh_db();
    let mut statement = db.prepare("SELECT name FROM pragma_table_info('corrections') ORDER BY cid").unwrap();
    let columns: Vec<String> = statement.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    assert_eq!(
        columns,
        ["id", "kind", "name", "qualifier", "field", "from_value", "to_value", "reason", "source", "read"]
    );
    let insert_correction = |kind: &str, qualifier: &str| {
        db.execute(
            "INSERT INTO corrections (kind, name, qualifier, field, from_value, to_value, reason, source, read)
             VALUES (?1, 'The Fury''s Rage', ?2, 'min_level', '318', '18', 'typo', 'https://ddowiki.com/page/Lost_Purpose', '2026-09-29')",
            [kind, qualifier],
        )
    };
    insert_correction("augment", "").unwrap();
    assert!(insert_correction("augment", "").is_err(), "a (kind, name, qualifier, field) is corrected once");
    insert_correction("augment", "family \"LostPurpose\"").unwrap();
    for new_kind in ["augment_bonus", "item_bonus", "item_effect", "item_socket", "socket_label"] {
        insert_correction(new_kind, "").unwrap();
    }
    assert!(insert_correction("gem", "").is_err(), "kind is one of the correctable tables");
}

fn insert_drop(db: &Connection, columns_and_values: &str) -> rusqlite::Result<usize> {
    let (columns, values) = columns_and_values.split_once(" = ").unwrap();
    db.execute(&format!("INSERT INTO drops ({columns}) VALUES ({values})"), [])
}

fn db_with_one_source_of_each_kind() -> Connection {
    let db = fresh_db();
    db.execute_batch(
        "PRAGMA foreign_keys = OFF;
         INSERT INTO items (id, name, slot_id, item_category) VALUES (1, 'Rusted Crown', 1, 'Jewelry');
         INSERT INTO adventure_packs (id, name) VALUES (1, 'Magic of Myth Drannor');
         INSERT INTO quests (id, name) VALUES (1, 'Book Burning');
         INSERT INTO quest_chains (id, name, source, wiki_url) VALUES (1, 'The Necropolis', 'wiki', 'https://ddowiki.com/page/Necropolis');
         INSERT INTO sagas (id, name, source, wiki_url) VALUES (1, 'Dread', 'wiki', 'https://ddowiki.com/page/Dread');
         INSERT INTO augments (id, name, family) VALUES (1, 'Lunar Gem of Magical Protection (Heroic)', 'SunAndMoon');",
    )
    .unwrap();
    db
}

#[test]
fn drops_links_each_loot_to_exactly_one_source_of_its_kind() {
    let db = db_with_one_source_of_each_kind();
    for valid_drop in [
        "source_kind, quest_id, item_id, loot_type, chest = 'quest', 1, 1, 'chest', 'end chest'",
        "source_kind, quest_id, item_id, loot_type = 'quest', 1, 1, 'reward'",
        "source_kind, quest_id, augment_id, loot_type = 'quest', 1, 1, 'raid'",
        "source_kind, chain_id, item_id = 'quest_chain', 1, 1",
        "source_kind, saga_id, item_id, tier = 'saga', 1, 1, 'legendary'",
        "source_kind, saga_id, item_id = 'saga', 1, 1",
        "source_kind, pack_id, item_id, loot_type, is_rare = 'adventure_pack', 1, 1, 'chest', 1",
        "source_kind, pack_id, augment_id, loot_type = 'adventure_pack', 1, 1, 'chest'",
    ] {
        insert_drop(&db, valid_drop).unwrap_or_else(|error| panic!("{valid_drop}: {error}"));
    }
    for (invalid_drop, broken_rule) in [
        ("source_kind, quest_id, pack_id, item_id, loot_type = 'quest', 1, 1, 1, 'chest'", "two sources"),
        ("source_kind, item_id, loot_type = 'quest', 1, 'chest'", "no source"),
        ("source_kind, pack_id, item_id, loot_type = 'quest', 1, 1, 'chest'", "a kind its source does not match"),
        ("source_kind, quest_id, item_id, loot_type = 'bag', 1, 1, 'chest'", "an unknown kind"),
        ("source_kind, quest_id, item_id, augment_id, loot_type = 'quest', 1, 1, 1, 'chest'", "two loots"),
        ("source_kind, quest_id, loot_type = 'quest', 1, 'chest'", "no loot"),
        ("source_kind, quest_id, item_id, loot_type, tier = 'quest', 1, 1, 'chest', 'epic'", "a tier on a quest drop"),
        (
            "source_kind, quest_id, item_id, loot_type, chest = 'quest', 1, 1, 'reward', 'end chest'",
            "a chest on a reward",
        ),
        ("source_kind, chain_id, item_id, chest = 'quest_chain', 1, 1, 'end chest'", "a chest on a chain reward"),
        ("source_kind, quest_id, item_id = 'quest', 1, 1", "a quest drop without a loot type"),
        ("source_kind, saga_id, item_id, loot_type = 'saga', 1, 1, 'reward'", "a loot type on a saga reward"),
        ("source_kind, quest_id, item_id, loot_type = 'quest', 1, 1, 'bag'", "an unknown loot type"),
        ("source_kind, saga_id, item_id, tier = 'saga', 1, 1, 'mythic'", "an unknown tier"),
        ("source_kind, quest_id, item_id, loot_type = 'quest', 1, 1, 'chest'", "a second identical quest drop"),
        ("source_kind, chain_id, item_id = 'quest_chain', 1, 1", "a second identical chain reward"),
        ("source_kind, saga_id, item_id = 'saga', 1, 1", "a second untiered saga reward"),
        (
            "source_kind, pack_id, augment_id, loot_type = 'adventure_pack', 1, 1, 'chest'",
            "a second identical pack drop",
        ),
    ] {
        assert!(insert_drop(&db, invalid_drop).is_err(), "{broken_rule} must fail: {invalid_drop}");
    }
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM drops", [], |r| r.get::<_, i64>(0)).unwrap(),
        8,
        "an item and an augment with one id are different loot"
    );
}

fn column_shapes(
    db: &Connection,
    table: &str,
    columns_to_skip: &[&str],
) -> Vec<(String, String, bool, Option<String>)> {
    let mut statement = db
        .prepare(&format!("SELECT name, type, \"notnull\", dflt_value FROM pragma_table_info('{table}') ORDER BY cid"))
        .unwrap();
    statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .filter(|(name, ..): &(String, String, bool, Option<String>)| !columns_to_skip.contains(&name.as_str()))
        .collect()
}

#[test]
fn quest_chains_and_sagas_share_their_columns_quest_links_and_reward_shape() {
    let db = fresh_db();
    assert_eq!(column_shapes(&db, "quest_chains", &[]), column_shapes(&db, "sagas", &[]));
    assert_eq!(
        column_shapes(&db, "quest_chains", &[]).iter().map(|(name, ..)| name.as_str()).collect::<Vec<_>>(),
        ["id", "name", "pack_id", "source", "wiki_url"]
    );
    assert_eq!(
        column_shapes(&db, "quest_chain_quests", &["chain_id"]),
        column_shapes(&db, "saga_quests", &["saga_id"])
    );
}

#[test]
fn a_saga_reward_has_one_row_per_item_and_tier() {
    let db = db_with_one_source_of_each_kind();
    for tier in ["'epic'", "'legendary'", "NULL"] {
        insert_drop(&db, &format!("source_kind, saga_id, item_id, tier = 'saga', 1, 1, {tier}")).unwrap();
        assert!(
            insert_drop(&db, &format!("source_kind, saga_id, item_id, tier = 'saga', 1, 1, {tier}")).is_err(),
            "one row per item and tier {tier}"
        );
    }
    assert!(
        db.execute(
            "INSERT INTO sagas (name, source, wiki_url) VALUES ('Odd Saga', 'ddowiki', 'https://ddowiki.com/page/Odd')",
            []
        )
        .is_err(),
        "source is maetrim or wiki"
    );
}

#[test]
fn drops_records_the_chest_as_free_text() {
    let db = fresh_db();
    let mut statement = db.prepare("SELECT name, type FROM pragma_table_info('drops') ORDER BY cid").unwrap();
    let columns: Vec<(String, String)> =
        statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
    let column_names: Vec<&str> = columns.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        column_names,
        [
            "id",
            "source_kind",
            "quest_id",
            "chain_id",
            "saga_id",
            "pack_id",
            "item_id",
            "augment_id",
            "loot_type",
            "chest",
            "is_rare",
            "tier"
        ]
    );
    assert!(columns.contains(&("chest".to_string(), "TEXT".to_string())));
}

#[test]
fn ddl_is_idempotent() {
    let db = fresh_db();
    db.execute_batch(ddl()).expect("re-applying the DDL is a no-op");
    let recorded_version: i64 =
        db.query_row("SELECT COALESCE(MAX(version), 0) FROM schema_version", [], |r| r.get(0)).unwrap();
    assert!(recorded_version <= SCHEMA_VERSION, "a fresh database records no newer version than the code");
}

#[test]
fn stats_have_unique_ids_and_names() {
    let ids: HashSet<i64> = STATS.iter().map(|s| s.id).collect();
    let names: HashSet<&str> = STATS.iter().map(|s| s.name).collect();
    assert_eq!(ids.len(), STATS.len(), "duplicate stat id");
    assert_eq!(names.len(), STATS.len(), "duplicate stat name");
    assert_eq!(Stat::by_name("Fire Spell Power").map(|s| s.id), Some(25));
    assert_eq!(Stat::by_name("Nope"), None);
}

#[test]
fn equipment_slots_say_hands_not_arms() {
    let names: Vec<&str> = EquipmentSlot::ALL.iter().map(|s| s.name()).collect();
    assert!(names.contains(&"Hands"));
    assert!(!names.contains(&"Arms"));
    assert_eq!(EquipmentSlot::Hands.category(), EquipmentSlotCategory::Armor);
    assert_eq!(EquipmentSlot::MainHand.id(), 1);
    assert_eq!(EquipmentSlot::ALL.len(), 16);
}

#[test]
fn bonus_types_include_the_v2_additions() {
    let names: Vec<&str> = BonusType::ALL.iter().map(|b| b.name()).collect();
    for added_bonus_type in ["Vitality", "False Life", "Legendary", "Penalty"] {
        assert!(names.contains(&added_bonus_type), "{added_bonus_type} missing from bonus_types");
    }
    assert_eq!(BonusType::Enhancement.id(), 1);
    assert_eq!(BonusType::Penalty.id(), 33, "the original 33 ids are stable");
    assert!(BonusType::ALL.len() >= 73, "his BonusTypes.xml vocabulary is appended");
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
    let weapon_type_named = |name: &str| WeaponType::by_name(name).unwrap_or_else(|| panic!("{name}"));
    assert_eq!(weapon_type_named("Great Axe").proficiency, Some(WeaponProficiency::Martial));
    assert_eq!(weapon_type_named("Khopesh").proficiency, Some(WeaponProficiency::Exotic));
    assert_eq!(weapon_type_named("Dagger").proficiency, Some(WeaponProficiency::Simple));
    assert_eq!(weapon_type_named("Large Shield").proficiency, None);
    assert!(weapon_type_named("Large Shield").is_shield);
    assert!(!weapon_type_named("Handwraps").is_shield);
    assert!(WEAPON_TYPES.iter().any(|w| w.name == "Rune Arm"));
    assert!(!WEAPON_TYPES.iter().any(|w| w.name == "Greataxe"));
    assert_eq!(DAMAGE_TYPES.len(), 18);
}

#[test]
fn seed_tables_load_into_the_schema() {
    let db = fresh_db();
    ddo_model::seeds::insert_all(&db).expect("seeds insert");
    let stat_count: i64 = db.query_row("SELECT COUNT(*) FROM stats", [], |r| r.get(0)).unwrap();
    assert_eq!(stat_count as usize, STATS.len());
    let hands: String = db.query_row("SELECT name FROM equipment_slots WHERE id = 10", [], |r| r.get(0)).unwrap();
    assert_eq!(hands, "Hands");
    let martial_weapon_type_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM weapon_types wt JOIN weapon_proficiencies p ON p.id = wt.proficiency_id WHERE p.name = 'Martial'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(martial_weapon_type_count > 10);
}

#[test]
fn save_progressions_follow_upstream_type_codes() {
    assert_eq!(SaveProgression::parse("Type2"), Some(SaveProgression::Good), "Paladin Fortitude is Type2");
    assert_eq!(SaveProgression::parse("Type1"), Some(SaveProgression::Poor));
    assert_eq!(SaveProgression::parse("None"), Some(SaveProgression::None));
    assert_eq!(SaveProgression::parse("Type3"), None);
}
