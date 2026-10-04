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
fn ddl_creates_effect_tables_without_legacy_bonus_tables() {
    let db = fresh_db();
    let tables = table_names(&db);
    for expected_table in [
        "schema_version",
        "stats",
        "bonus_types",
        "bonus_type_aliases",
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
        "effects",
        "effect_ladders",
        "effect_bonuses",
        "effect_vocabulary_counts",
        "effect_vocabulary_bonus_types",
        "item_effects",
        "item_augment_slots",
        "item_augment_slot_options",
        "sources",
        "vendors",
        "events",
        "quest_chains",
        "quest_chain_quests",
        "sagas",
        "saga_quests",
        "modifiers",
        "requirements",
        "augments",
        "augment_slots",
        "augment_effects",
        "set_bonuses",
        "set_bonus_tiers",
        "set_bonus_tier_effects",
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
        "feat_effects",
        "item_augment_slot_option_effects",
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
    for removed_table in [
        "bonuses",
        "effect_tiers",
        "item_bonuses",
        "augment_bonuses",
        "set_bonus_tier_bonuses",
        "feat_bonuses",
        "item_augment_slot_option_bonuses",
    ] {
        assert!(!tables.contains(removed_table), "legacy table {removed_table} remains");
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
    db.execute("INSERT INTO items (name, slot_id, item_category, wiki_url) VALUES ('His Ring', 15, 'Jewelry', 'https://ddowiki.com/page/Item:His_Ring')", []).unwrap();
    db.execute(
        "INSERT INTO items (name, slot_id, item_category, provenance, wiki_url) VALUES ('Wiki Ring', 15, 'Jewelry', 'wiki', 'https://ddowiki.com/page/Item:Wiki_Ring')",
        [],
    )
    .unwrap();
    let provenances: Vec<String> = db
        .prepare("SELECT provenance FROM items ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(provenances, ["maetrim", "wiki"]);
    assert!(db
        .execute(
            "INSERT INTO items (name, slot_id, item_category, provenance, wiki_url) VALUES ('Odd Ring', 15, 'Jewelry', 'ddowiki', 'https://ddowiki.com/page/Item:Odd_Ring')",
            []
        )
        .is_err());
}

#[test]
fn quests_come_from_maetrim_unless_the_wiki_supplied_them() {
    let db = fresh_db();
    db.execute("INSERT INTO adventure_packs (id, name) VALUES (1, 'Free to Play')", []).unwrap();
    db.execute("INSERT INTO quests (name, pack_id) VALUES ('His Quest', 1)", []).unwrap();
    db.execute("INSERT INTO quests (name, pack_id, provenance) VALUES ('Wiki Quest', 1, 'wiki')", []).unwrap();
    let provenances: Vec<String> = db
        .prepare("SELECT provenance FROM quests ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(provenances, ["maetrim", "wiki"]);
    assert!(db
        .execute("INSERT INTO quests (name, pack_id, provenance) VALUES ('Odd Quest', 1, 'ddowiki')", [])
        .is_err());
}

#[test]
fn augments_come_from_maetrim_unless_the_wiki_supplied_them() {
    let db = fresh_db();
    db.execute("INSERT INTO augments (name, family) VALUES ('His Gem', 'Named')", []).unwrap();
    db.execute("INSERT INTO augments (name, family, provenance) VALUES ('Wiki Gem', 'Named', 'wiki')", []).unwrap();
    let provenances: Vec<String> = db
        .prepare("SELECT provenance FROM augments ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(provenances, ["maetrim", "wiki"]);
    assert!(db
        .execute("INSERT INTO augments (name, family, provenance) VALUES ('Odd Gem', 'Named', 'ddowiki')", [])
        .is_err());
}

#[test]
fn items_carry_a_legacy_flag_that_defaults_to_current() {
    let db = fresh_db();
    let (is_not_null, default_value): (bool, String) = db
        .query_row("SELECT \"notnull\", dflt_value FROM pragma_table_info('items') WHERE name = 'is_legacy'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("items has an is_legacy column");
    assert_eq!((is_not_null, default_value.as_str()), (true, "0"));
    let items_sql: String =
        db.query_row("SELECT sql FROM sqlite_master WHERE name = 'items'", [], |r| r.get(0)).unwrap();
    assert!(items_sql.contains("CHECK (is_legacy IN (0, 1))"), "{items_sql}");
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

fn insert_source(db: &Connection, columns_and_values: &str) -> rusqlite::Result<usize> {
    let (columns, values) = columns_and_values.split_once(" = ").unwrap();
    db.execute(&format!("INSERT INTO sources ({columns}) VALUES ({values})"), [])
}

fn db_with_one_source_of_each_kind() -> Connection {
    let db = fresh_db();
    db.execute_batch(
        "PRAGMA foreign_keys = OFF;
         INSERT INTO items (id, name, slot_id, item_category, wiki_url)
              VALUES (1, 'Rusted Crown', 1, 'Jewelry', 'https://ddowiki.com/page/Item:Rusted_Crown');
         INSERT INTO adventure_packs (id, name) VALUES (1, 'Magic of Myth Drannor');
         INSERT INTO quests (id, name, pack_id) VALUES (1, 'Book Burning', 1);
         INSERT INTO quest_chains (id, name, provenance, wiki_url) VALUES (1, 'The Necropolis', 'wiki', 'https://ddowiki.com/page/Necropolis');
         INSERT INTO sagas (id, name, provenance, wiki_url) VALUES (1, 'Dread', 'wiki', 'https://ddowiki.com/page/Dread');
         INSERT INTO augments (id, name, family) VALUES (1, 'Lunar Gem of Magical Protection (Heroic)', 'SunAndMoon');
         INSERT INTO crafting_systems (id, name, page) VALUES (1, 'Catalyst Crafting', 'https://ddowiki.com/page/Catalyst_Crafting');
         INSERT INTO vendors (id, name, location, pack_id, provenance, wiki_url)
              VALUES (1, 'Morten Edgewright', 'House Jorasco', NULL, 'wiki', 'https://ddowiki.com/page/Morten_Edgewright');
         INSERT INTO events (id, name, provenance, wiki_url)
              VALUES (1, 'Treasure of Crystal Cove', 'wiki', 'https://ddowiki.com/page/Crystal_Cove');",
    )
    .unwrap();
    db
}

#[test]
fn sources_link_each_loot_to_exactly_one_source_of_its_kind() {
    let db = db_with_one_source_of_each_kind();
    for valid_source in [
        "kind, quest_id, item_id, loot_type, chest = 'quest', 1, 1, 'chest', 'end chest'",
        "kind, quest_id, item_id, loot_type = 'quest', 1, 1, 'reward'",
        "kind, quest_id, augment_id, loot_type = 'quest', 1, 1, 'raid'",
        "kind, chain_id, item_id = 'quest_chain', 1, 1",
        "kind, saga_id, item_id, tier = 'saga', 1, 1, 'legendary'",
        "kind, saga_id, item_id = 'saga', 1, 1",
        "kind, pack_id, item_id, loot_type, is_rare = 'adventure_pack', 1, 1, 'chest', 1",
        "kind, pack_id, augment_id, loot_type = 'adventure_pack', 1, 1, 'chest'",
        "kind, pack_id, item_id = 'challenge', 1, 1",
        "kind, crafting_system_id, item_id, is_rare = 'crafting_system', 1, 1, 1",
        "kind, vendor_id, item_id, cost = 'vendor', 1, 1, '1 Ethereal Ingot'",
        "kind, event_id, augment_id = 'event', 1, 1",
        "kind, character_level, item_id = 'starter', 15, 1",
    ] {
        insert_source(&db, valid_source).unwrap_or_else(|error| panic!("{valid_source}: {error}"));
    }
    for (invalid_source, broken_rule) in [
        ("kind, quest_id, pack_id, item_id, loot_type = 'quest', 1, 1, 1, 'chest'", "two sources"),
        ("kind, item_id, loot_type = 'quest', 1, 'chest'", "no source"),
        ("kind, pack_id, item_id, loot_type = 'quest', 1, 1, 'chest'", "a kind its source does not match"),
        ("kind, quest_id, item_id, loot_type = 'bag', 1, 1, 'chest'", "an unknown kind"),
        ("kind, quest_id, item_id, augment_id, loot_type = 'quest', 1, 1, 1, 'chest'", "two loots"),
        ("kind, quest_id, loot_type = 'quest', 1, 'chest'", "no loot"),
        ("kind, quest_id, item_id, loot_type, tier = 'quest', 1, 1, 'chest', 'epic'", "a tier on a quest drop"),
        ("kind, quest_id, item_id, loot_type, chest = 'quest', 1, 1, 'reward', 'end chest'", "a chest on a reward"),
        ("kind, chain_id, item_id, chest = 'quest_chain', 1, 1, 'end chest'", "a chest on a chain reward"),
        ("kind, quest_id, item_id = 'quest', 1, 1", "a quest drop without a loot type"),
        ("kind, saga_id, item_id, loot_type = 'saga', 1, 1, 'reward'", "a loot type on a saga reward"),
        ("kind, quest_id, item_id, loot_type = 'quest', 1, 1, 'bag'", "an unknown loot type"),
        ("kind, saga_id, item_id, tier = 'saga', 1, 1, 'mythic'", "an unknown tier"),
        ("kind, quest_id, item_id, loot_type = 'quest', 1, 1, 'chest'", "a second identical quest drop"),
        ("kind, chain_id, item_id = 'quest_chain', 1, 1", "a second identical chain reward"),
        ("kind, saga_id, item_id = 'saga', 1, 1", "a second untiered saga reward"),
        ("kind, pack_id, augment_id, loot_type = 'adventure_pack', 1, 1, 'chest'", "a second identical pack drop"),
        ("kind, pack_id, item_id = 'challenge', 1, 1", "a second identical challenge reward"),
        ("kind, pack_id, item_id, loot_type = 'challenge', 1, 1, 'chest'", "a loot type on a challenge reward"),
        ("kind, quest_id, item_id = 'challenge', 1, 1", "a challenge reward naming a quest, not its pack"),
        ("kind, crafting_system_id, item_id, chest = 'crafting_system', 1, 1, 'end chest'", "a chest on a crafting"),
        ("kind, vendor_id, pack_id, item_id = 'vendor', 1, 1, 1", "two sources on a vendor"),
        ("kind, event_id, item_id, cost = 'event', 1, 1, '5 tokens'", "a cost on an event reward"),
        ("kind, item_id = 'starter', 1", "a starter item with no character level"),
        ("kind, character_level, item_id, tier = 'starter', 15, 1, 'epic'", "a tier on a starter item"),
    ] {
        assert!(insert_source(&db, invalid_source).is_err(), "{broken_rule} must fail: {invalid_source}");
    }
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM sources", [], |r| r.get::<_, i64>(0)).unwrap(),
        13,
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
        ["id", "name", "pack_id", "provenance", "wiki_url"]
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
        insert_source(&db, &format!("kind, saga_id, item_id, tier = 'saga', 1, 1, {tier}")).unwrap();
        assert!(
            insert_source(&db, &format!("kind, saga_id, item_id, tier = 'saga', 1, 1, {tier}")).is_err(),
            "one row per item and tier {tier}"
        );
    }
    assert!(
        db.execute(
            "INSERT INTO sagas (name, provenance, wiki_url) VALUES ('Odd Saga', 'ddowiki', 'https://ddowiki.com/page/Odd')",
            []
        )
        .is_err(),
        "provenance is maetrim or wiki"
    );
}

#[test]
fn sources_record_the_chest_as_free_text() {
    let db = fresh_db();
    let mut statement = db.prepare("SELECT name, type FROM pragma_table_info('sources') ORDER BY cid").unwrap();
    let columns: Vec<(String, String)> =
        statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
    let column_names: Vec<&str> = columns.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        column_names,
        [
            "id",
            "kind",
            "quest_id",
            "chain_id",
            "saga_id",
            "pack_id",
            "crafting_system_id",
            "vendor_id",
            "event_id",
            "character_level",
            "item_id",
            "augment_id",
            "loot_type",
            "chest",
            "is_rare",
            "tier",
            "cost"
        ]
    );
    assert!(columns.contains(&("chest".to_string(), "TEXT".to_string())));
}

#[test]
fn vendors_and_events_carry_their_provenance_and_page() {
    let db = fresh_db();
    assert_eq!(
        column_shapes(&db, "vendors", &[]).iter().map(|(name, ..)| name.as_str()).collect::<Vec<_>>(),
        ["id", "name", "location", "pack_id", "provenance", "wiki_url"]
    );
    assert_eq!(
        column_shapes(&db, "events", &[]).iter().map(|(name, ..)| name.as_str()).collect::<Vec<_>>(),
        ["id", "name", "provenance", "wiki_url"]
    );
    assert!(db
        .execute(
            "INSERT INTO events (name, provenance, wiki_url) VALUES ('Odd', 'ddowiki', 'https://ddowiki.com/page/Odd')",
            []
        )
        .is_err());
    assert!(db.execute("INSERT INTO vendors (name, provenance) VALUES ('No Page', 'wiki')", []).is_err());
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
fn stats_carry_no_second_name_for_a_stat_the_maps_already_target() {
    let canonical_stat_by_alias = [
        ("Physical Sheltering", "Physical Resistance Rating"),
        ("Magical Sheltering", "Magical Resistance Rating"),
        ("Sheltering", "Physical and Magical Resistance Rating"),
        ("Potency", "Universal Spell Power"),
        ("Alignment Spell Power", "Light Spell Power"),
        ("Protection", "Armor Class"),
        ("Natural Armor", "Armor Class"),
        ("Resistance", "Saving Throws"),
        ("Shatter", "Sunder DC"),
        ("Wizardry", "Spell Points"),
        ("Maximum Spell Points", "Spell Points"),
        ("Universal Spell Focus", "Spell DCs"),
        ("Spell Critical Damage", "Universal Spell Critical Damage"),
        ("Negative Energy Absorption", "Negative Absorption"),
        ("Negative Energy Resistance", "Negative Resistance"),
        ("Saving Throws vs Traps", "Trap Save"),
        ("Sneak Attack Bonus", "Sneak Attack"),
        ("Critical Damage Multiplier", "Critical Multiplier"),
        ("Critical Confirmation", "Seeker"),
        ("Critical Damage", "Seeker"),
        ("Max Dex Bonus (Shield)", "Max Dex Bonus (Tower Shield)"),
        ("Threat Generation", "Melee Threat Generation"),
        ("Melee and Ranged Power", "Melee Power"),
        ("Doublestrike and Doubleshot", "Doublestrike"),
        ("Melee and Ranged Threat Reduction", "Threat Reduction"),
        ("Positive and Negative Spell Power", "Positive Spell Power"),
        ("Positive and Negative Healing Amplification", "Healing Amplification"),
    ];
    for (alias, canonical_stat) in canonical_stat_by_alias {
        assert!(Stat::by_name(alias).is_none(), "{alias} duplicates {canonical_stat}");
        assert!(Stat::by_name(canonical_stat).is_some(), "{canonical_stat} is missing");
    }
    let names_that_are_effects_or_bonus_types = [
        "Persuasion",
        "Tendon Slice",
        "Spell Absorption",
        "Quality",
        "Attack and Damage vs Evil",
        "Damage vs Evil",
        "Saves vs Evil",
        "Curse Absorption",
        "Positive Absorption",
        "Repair Absorption",
    ];
    for name in names_that_are_effects_or_bonus_types {
        assert!(Stat::by_name(name).is_none(), "{name} is no stat");
    }
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

#[test]
fn effect_stat_amount_sources_and_constants_are_checked() {
    let db = fresh_db();
    ddo_model::seeds::insert_all(&db).unwrap();
    db.execute("INSERT INTO effects (id, name, text_template, amount_count) VALUES (1, 'Probe', 'Probe {1}', 1)", [])
        .unwrap();
    for (amount_from, constant) in [(0, "NULL"), (1, "1"), (3, "NULL")] {
        assert!(db
            .execute(
                &format!(
                    "INSERT INTO effect_bonuses (effect_id, stat_id, bonus_type_id, amount_from, constant, sort_order)
             VALUES (1, 1, 1, {amount_from}, {constant}, 0)"
                ),
                []
            )
            .is_err());
    }
    db.execute(
        "INSERT INTO effect_bonuses (effect_id, stat_id, bonus_type_id, amount_from, sort_order) VALUES (1, 1, 1, 1, 0)",
        [],
    ).unwrap();
}

#[test]
fn effect_defaults_and_scaled_stat_constraints_are_checked() {
    let db = fresh_db();
    ddo_model::seeds::insert_all(&db).unwrap();
    db.execute(
        "INSERT INTO effects (id, name, text_template, amount_count, default_value, default_value2)
         VALUES (1, 'Scaled Probe', 'Scaled Probe {1} {2}', 2, 3, 4)",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO effect_bonuses
         (effect_id, stat_id, bonus_type_id, amount_from, scale, rounding, sort_order)
         VALUES (1, 1, 1, 1, 0.5, 'up', 0)",
        [],
    )
    .unwrap();
    for (amount_from, constant, scale, rounding) in [
        (0, "2", "0.5", "'up'"),
        (0, "2", "1", "'nearest'"),
        (1, "NULL", "1", "'away'"),
        (1, "NULL", "0", "'down'"),
        (1, "NULL", "-0.5", "'down'"),
    ] {
        assert!(db
            .execute(
                &format!(
                    "INSERT INTO effect_bonuses
                     (effect_id, stat_id, bonus_type_id, amount_from, constant, scale, rounding, sort_order)
                     VALUES (1, 2, 1, {amount_from}, {constant}, {scale}, {rounding}, 1)"
                ),
                [],
            )
            .is_err());
    }
}

#[test]
fn rounded_amount_sql_handles_positive_and_negative_halves() {
    let db = fresh_db();
    let expression = ddo_model::effect_amount::rounded_amount_sql("?1", "0.5", "?2");
    for (amount, rounding, expected) in
        [(3, "down", 1), (3, "up", 2), (3, "nearest", 2), (-3, "down", -2), (-3, "up", -1), (-3, "nearest", -2)]
    {
        let actual: i64 = db.query_row(&format!("SELECT {expression}"), (amount, rounding), |row| row.get(0)).unwrap();
        assert_eq!(actual, expected, "{amount} with {rounding}");
    }
}

#[test]
fn effect_family_amount_urls_and_ladder_positions_are_checked() {
    let db = fresh_db();
    for (name, amount_count, wiki_url) in [
        ("Negative", -1, "NULL"),
        ("Third", 3, "NULL"),
        ("Wrong Host", 1, "'https://example.com/page/Wrong_Host'"),
        ("Wrong Path", 1, "'https://ddowiki.com/Wrong_Path'"),
    ] {
        assert!(
            db.execute(
                &format!(
                    "INSERT INTO effects (name, text_template, amount_count, wiki_url)
             VALUES (?1, 'A {{1}}', {amount_count}, {wiki_url})"
                ),
                [name]
            )
            .is_err(),
            "{name}"
        );
    }
    db.execute("INSERT INTO effect_ladders (id, name) VALUES (1, 'Deception')", []).unwrap();
    db.execute(
        "INSERT INTO effects (id, name, text_template, amount_count, ladder_id, ladder_rank, wiki_url)
                VALUES (1, 'Deception', 'Deception {1}', 1, 1, 1, 'https://ddowiki.com/page/Deception')",
        [],
    )
    .unwrap();
    assert!(db
        .execute(
            "INSERT INTO effects (name, text_template, amount_count, ladder_id)
                        VALUES ('Improved Deception', 'Improved Deception', 0, 1)",
            []
        )
        .is_err());
    assert!(db
        .execute(
            "INSERT INTO effects (name, text_template, amount_count, ladder_rank)
                        VALUES ('Greater Deception', 'Greater Deception', 0, 2)",
            []
        )
        .is_err());
    assert!(db
        .execute(
            "INSERT INTO effects (name, text_template, amount_count, ladder_id, ladder_rank)
                        VALUES ('Duplicate Rank', 'Duplicate Rank', 0, 1, 1)",
            []
        )
        .is_err());
}

#[test]
fn effect_owner_links_require_the_first_amount_before_the_second() {
    let db = fresh_db();
    db.execute(
        "INSERT INTO effects (id, name, text_template, amount_count)
                VALUES (1, 'Probe', 'Probe {1} {2}', 2)",
        [],
    )
    .unwrap();
    for (table, owner_column) in [
        ("item_effects", "item_id"),
        ("augment_effects", "augment_id"),
        ("set_bonus_tier_effects", "tier_id"),
        ("feat_effects", "feat_id"),
        ("item_augment_slot_option_effects", "option_id"),
    ] {
        assert!(
            db.execute(
                &format!("INSERT INTO {table} ({owner_column}, effect_id, value2, sort_order) VALUES (1, 1, 2, 0)"),
                []
            )
            .is_err(),
            "{table}"
        );
    }
}

#[test]
fn single_table_rules_are_schema_constraints() {
    let db = fresh_db();
    ddo_model::seeds::insert_all(&db).unwrap();
    db.execute("INSERT INTO adventure_packs (id, name) VALUES (1, 'Magic of Myth Drannor')", []).unwrap();
    let item_insert = |name: &str, wiki_url_sql: &str| {
        db.execute(
            &format!(
                "INSERT INTO items (name, slot_id, item_category, wiki_url) VALUES ('{name}', (SELECT id FROM equipment_slots WHERE name = 'Ring'), 'Jewelry', {wiki_url_sql})"
            ),
            [],
        )
    };
    assert!(item_insert("  ", "'https://ddowiki.com/page/Item:Blank'").is_err(), "an item has a non-blank name");
    assert!(item_insert("No Page", "NULL").is_err(), "an item links to a ddowiki page");
    assert!(item_insert("Elsewhere", "'https://example.com/page/Item:Elsewhere'").is_err(), "the page is on ddowiki");
    assert!(item_insert("Bare", "'https://ddowiki.com/page/'").is_err(), "the page has a title");
    item_insert("Rusted Crown", "'https://ddowiki.com/page/Item:Rusted_Crown'").unwrap();

    assert!(
        db.execute("INSERT INTO quests (name, is_challenge) VALUES ('Packless Quest', 0)", []).is_err(),
        "a quest that is no challenge belongs to an adventure pack"
    );
    db.execute("INSERT INTO quests (name, is_challenge) VALUES ('Packless Challenge', 1)", []).unwrap();
    db.execute("INSERT INTO quests (id, name, pack_id) VALUES (10, 'Book Burning', 1)", []).unwrap();

    let source_insert = |chest: &str| {
        db.execute(
            &format!(
                "INSERT INTO sources (kind, quest_id, item_id, loot_type, chest) VALUES ('quest', 10, (SELECT MIN(id) FROM items), 'chest', '{chest}')"
            ),
            [],
        )
    };
    assert!(source_insert("end Reward chest").is_err(), "a chest never says reward");
    source_insert("end chest").unwrap();

    for (table, columns_and_values) in [
        ("items", "name, slot_id, item_category, provenance) VALUES ('Wiki Ring', 1, 'Jewelry', 'wiki'"),
        ("quest_chains", "name, provenance) VALUES ('Wiki Chain', 'wiki'"),
        ("sagas", "name, provenance) VALUES ('Wiki Saga', 'wiki'"),
        ("vendors", "name, provenance) VALUES ('Wiki Vendor', 'wiki'"),
        ("events", "name, provenance) VALUES ('Wiki Event', 'wiki'"),
    ] {
        assert!(
            db.execute(&format!("INSERT INTO {table} ({columns_and_values})"), []).is_err(),
            "a {table} row from the wiki carries its page"
        );
    }
}

#[test]
fn an_item_augment_slot_option_keeps_its_granted_socket_sets_and_bonuses_on_the_option() {
    let db = fresh_db();
    ddo_model::seeds::insert_all(&db).unwrap();
    db.execute_batch(
        "PRAGMA foreign_keys = ON;
         INSERT INTO items (id, name, slot_id, item_category, wiki_url) VALUES (1, 'Sireth', 1, 'Weapon', 'https://ddowiki.com/page/Item:Sireth');
         INSERT INTO augment_slot_types (id, label, family, variant) VALUES (1, 'crafting: attuned to heroism 4', 'crafting', 'attuned to heroism 4'), (2, 'red', 'standard', 'red');
         INSERT INTO item_augment_slots (item_id, sort_order, slot_id) VALUES (1, 0, 1);
         INSERT INTO set_bonuses (id, name) VALUES (1, 'Prowess / Planar Conflux Set Bonus');
         INSERT INTO effects (id, name, text_template, amount_count) VALUES (1, 'Strength', 'Strength {1}', 1);
         INSERT INTO effect_bonuses (effect_id, stat_id, bonus_type_id, amount_from, sort_order) VALUES (1, 1, 1, 1, 0);",
    )
    .unwrap();
    db.execute(
        "INSERT INTO item_augment_slot_options (item_id, slot_order, option_order, name, icon) VALUES (1, 0, 0, 'Red Augment Slot', 'Heroism')",
        [],
    )
    .unwrap();
    let option_id = db.last_insert_rowid();
    db.execute(
        "INSERT INTO item_augment_slot_option_grants (option_id, sort_order, slot_id) VALUES (?1, 0, 2)",
        [option_id],
    )
    .unwrap();
    db.execute("INSERT INTO item_augment_slot_option_sets (option_id, set_id) VALUES (?1, 1)", [option_id]).unwrap();
    db.execute(
        "INSERT INTO item_augment_slot_option_effects (option_id, effect_id, value, sort_order) VALUES (?1, 1, 8, 0)",
        [option_id],
    )
    .unwrap();
    db.execute(
        "INSERT INTO modifiers (source_kind, source_id, sort_order, effect_type) VALUES ('item_augment_slot_option', ?1, 0, 'AbilityBonus')",
        [option_id],
    )
    .unwrap();
    assert!(db
        .execute(
            "INSERT INTO item_augment_slot_option_grants (option_id, sort_order, slot_id) VALUES (?1, 1, 99)",
            [option_id]
        )
        .is_err());
    assert!(db
        .execute(
            "INSERT INTO item_augment_slot_options (item_id, slot_order, option_order, name) VALUES (1, 0, 0, 'Again')",
            []
        )
        .is_err());
    let item_socket_count: i64 =
        db.query_row("SELECT COUNT(*) FROM item_augment_slots WHERE item_id = 1", [], |r| r.get(0)).unwrap();
    assert_eq!(item_socket_count, 1, "a granted socket stays on the option, never an item socket");
    db.execute("DELETE FROM items WHERE id = 1", []).unwrap();
    for option_table in
        ["item_augment_slot_option_grants", "item_augment_slot_option_sets", "item_augment_slot_option_effects"]
    {
        let row_count: i64 = db.query_row(&format!("SELECT COUNT(*) FROM {option_table}"), [], |r| r.get(0)).unwrap();
        assert_eq!(row_count, 0, "{option_table} rows go with their item");
    }
    let options_sql: String = db
        .query_row("SELECT sql FROM sqlite_master WHERE name = 'item_augment_slot_options'", [], |r| r.get(0))
        .unwrap();
    assert!(!options_sql.contains("already applied"), "{options_sql}");
    const { assert!(SCHEMA_VERSION >= 19, "item augment slot options require schema 19 or newer") };
}
