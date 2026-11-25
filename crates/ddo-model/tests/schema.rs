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
fn only_effect_aliases_allow_a_missing_slug() {
    let db = fresh_db();
    for table in table_names(&db) {
        let mut columns = db.prepare(&format!("PRAGMA table_info({table})")).unwrap();
        for row in columns.query_map([], |row| Ok((row.get::<_, String>(1)?, row.get::<_, bool>(3)?))).unwrap() {
            let (column, required) = row.unwrap();
            if column == "slug" {
                assert_eq!(required, table != "effect_aliases", "{table}.slug nullability");
            }
        }
    }
}

#[test]
fn ddl_creates_effect_tables_without_legacy_bonus_tables() {
    let db = fresh_db();
    let tables = table_names(&db);
    for expected_table in [
        "schema_version",
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
        "effect_tier_groups",
        "effect_bonuses",
        "effect_damage",
        "triggers",
        "effect_vocabulary_counts",
        "effect_vocabulary_bonus_types",
        "item_effects",
        "item_augment_slots",
        "socket_options",
        "quest_drops",
        "pack_drops",
        "quest_chain_rewards",
        "saga_rewards",
        "vendor_offers",
        "vendor_offer_costs",
        "vendor_currencies",
        "crafting_outputs",
        "event_rewards",
        "challenge_rewards",
        "starter_gear",
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
        "crafting_step_effects",
        "socket_option_effects",
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
        "stats",
        "effect_ladders",
        "bonuses",
        "effect_tiers",
        "item_bonuses",
        "augment_bonuses",
        "set_bonus_tier_bonuses",
        "feat_bonuses",
        "socket_option_bonuses",
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
    db.execute("INSERT INTO items (name, slot_id, item_category, wiki_url, slug) VALUES ('His Ring', 15, 'Jewelry', 'https://ddowiki.com/page/Item:His_Ring', 'his-ring')", []).unwrap();
    db.execute(
        "INSERT INTO items (name, slot_id, item_category, provenance, wiki_url, slug) VALUES ('Wiki Ring', 15, 'Jewelry', 'wiki', 'https://ddowiki.com/page/Item:Wiki_Ring', 'wiki-ring')",
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
            "INSERT INTO items (name, slot_id, item_category, provenance, wiki_url, slug) VALUES ('Odd Ring', 15, 'Jewelry', 'ddowiki', 'https://ddowiki.com/page/Item:Odd_Ring', 'odd-ring')",
            []
        )
        .is_err());
}

#[test]
fn quests_come_from_maetrim_unless_the_wiki_supplied_them() {
    let db = fresh_db();
    db.execute("INSERT INTO adventure_packs (id, name, slug) VALUES (1, 'Free to Play', 'free-to-play')", []).unwrap();
    db.execute("INSERT INTO quests (name, pack_id, slug) VALUES ('His Quest', 1, 'his-quest')", []).unwrap();
    db.execute(
        "INSERT INTO quests (name, pack_id, provenance, slug) VALUES ('Wiki Quest', 1, 'wiki', 'wiki-quest')",
        [],
    )
    .unwrap();
    let provenances: Vec<String> = db
        .prepare("SELECT provenance FROM quests ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(provenances, ["maetrim", "wiki"]);
    assert!(db
        .execute(
            "INSERT INTO quests (name, pack_id, provenance, slug) VALUES ('Odd Quest', 1, 'ddowiki', 'odd-quest')",
            []
        )
        .is_err());
}

#[test]
fn augments_come_from_maetrim_unless_the_wiki_supplied_them() {
    let db = fresh_db();
    db.execute("INSERT INTO augments (name, family, slug) VALUES ('His Gem', 'Named', 'his-gem')", []).unwrap();
    db.execute(
        "INSERT INTO augments (name, family, provenance, slug) VALUES ('Wiki Gem', 'Named', 'wiki', 'wiki-gem')",
        [],
    )
    .unwrap();
    let provenances: Vec<String> = db
        .prepare("SELECT provenance FROM augments ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(provenances, ["maetrim", "wiki"]);
    assert!(db
        .execute(
            "INSERT INTO augments (name, family, provenance, slug) VALUES ('Odd Gem', 'Named', 'ddowiki', 'odd-gem')",
            []
        )
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
    let error = db
        .execute("INSERT INTO quests (id, name, duration, slug) VALUES (1, 'The Grotto', 'Long', 'the-grotto')", [])
        .unwrap_err();
    assert!(error.to_string().contains("duration"), "{error}");
}

#[test]
fn ingredient_names_and_variants_are_unique_without_an_owner() {
    for variant in [None, Some("Heroic")] {
        let db = fresh_db();
        db.execute(
            "INSERT INTO crafting_ingredients (name,variant,tier,slug) VALUES ('Crystal',?1,'any','first-crystal')",
            [variant],
        )
        .unwrap();
        let duplicate = db.execute(
            "INSERT INTO crafting_ingredients (name,variant,tier,slug) VALUES ('Crystal',?1,'heroic','second-crystal')",
            [variant],
        );
        assert!(duplicate.is_err(), "duplicate ingredient name and variant accepted: {variant:?}");
        assert!(duplicate.unwrap_err().to_string().contains("UNIQUE"));
        db.execute("INSERT INTO crafting_ingredients (name,variant,tier,slug) VALUES ('Crystal','Legendary','legendary','legendary-crystal')", []).unwrap();
    }
}

#[test]
fn crafting_item_statements_are_nullable_booleans() {
    let db = fresh_db();
    db.execute("INSERT INTO crafting_systems (id,slug,name,page) VALUES (1,'test','Test','https://example.com')", [])
        .unwrap();
    for field in ["has_item_input", "has_item_output", "can_apply_to_any_item"] {
        for value in ["NULL", "0", "1"] {
            db.execute(&format!("UPDATE crafting_systems SET {field}={value}"), []).unwrap();
        }
        for value in ["-1", "2", "'true'"] {
            assert!(
                db.execute(&format!("UPDATE crafting_systems SET {field}={value}"), []).is_err(),
                "{field} accepted {value}"
            );
        }
    }
}

#[test]
fn crafting_tables_accept_only_the_four_crafting_tiers() {
    let db = fresh_db();
    db.execute("INSERT INTO crafting_systems (id, name, page, slug) VALUES (1, 'Green Steel', 'p', 'green-steel')", [])
        .unwrap();
    for tier in ["heroic", "epic", "legendary", "any"] {
        db.execute(
            "INSERT INTO crafting_ingredients (name, tier, slug) VALUES ('Shard ' || ?1, ?1, 'shard-' || ?1)",
            [tier],
        )
        .unwrap();
        db.execute("INSERT INTO crafting_recipes (system_id, tier, option, sort_order) VALUES (1, ?1, 'x', 0)", [tier])
            .unwrap();
    }
    assert!(db
        .execute(
            "INSERT INTO crafting_ingredients (name, tier, slug) VALUES ('Shard heroic', 'heroic', 'shard-heroic')",
            []
        )
        .is_err());
    assert!(db
        .execute("INSERT INTO crafting_ingredients (name, tier, slug) VALUES ('Gem', 'mythic', 'gem')", [])
        .is_err());
    assert!(db
        .execute("INSERT INTO crafting_recipes (system_id, tier, option, sort_order) VALUES (1, 'mythic', 'x', 0)", [])
        .is_err());
    assert!(db
        .execute("INSERT INTO crafting_systems (name, page, slug) VALUES ('Green Steel', 'q', 'green-steel')", [])
        .is_err());
}

fn crafting_schema_db() -> Connection {
    let db = fresh_db();
    db.execute_batch(
        "INSERT INTO crafting_systems (id, name, page, slug) VALUES (1, 'Forge', 'p', 'forge'), (2, 'Altar', 'p', 'altar');
        INSERT INTO augment_slot_types (id, slug, label, family, variant) VALUES (1, 'tier-1', 'Tier 1', 'crafting', 'tier 1'), (2, 'tier-2', 'Tier 2', 'crafting', 'tier 2');
        INSERT INTO equipment_slots (id, name, slug, sort_order, category) VALUES (1, 'Off Hand', 'off-hand', 0, 'weapon');
        INSERT INTO items (id, name, slug, slot_id, item_category, wiki_url) VALUES (1, 'Orb', 'orb', 1, 'Weapon', 'https://ddowiki.com/page/Item:Orb');
        INSERT INTO crafting_ingredients (id, name, slug, tier) VALUES (1, 'Ingot', 'ingot', 'epic');
        INSERT INTO crafting_recipes (id, system_id, tier, option, sort_order) VALUES (1, 1, 'epic', 'Upgrade', 0);
        INSERT INTO effects (id, name, slug, verbose_name_template) VALUES (1, 'Orb Bonus', 'orb-bonus', 'Orb Bonus');
        INSERT INTO augments (id, name, slug, family) VALUES (1, 'Choice', 'choice', 'Thunderforged');
        INSERT INTO crafting_steps (id, system_id, name, sort_order, slot_id) VALUES (1, 1, 'Tier 1', 0, 1);
        INSERT INTO crafting_stations (id, name, slug, kind, sort_order) VALUES (1, 'Forge', 'forge', 'interface', 0);
        INSERT INTO socket_option_sets (id, name, slug, slot_id) VALUES (1, 'Choices', 'choices', 1);
        INSERT INTO socket_options (id, set_id, option_order, name) VALUES (1, 1, 0, 'Choice');"
    ).unwrap();
    db
}

#[test]
fn crafting_steps_consumed_outputs_is_a_positive_count() {
    let db = crafting_schema_db();
    for quantity in [None, Some(1), Some(2), Some(i64::MAX)] {
        db.execute("UPDATE crafting_steps SET consumed_outputs=?1 WHERE id=1", [quantity]).unwrap();
        assert_eq!(
            db.query_row("SELECT consumed_outputs FROM crafting_steps WHERE id=1", [], |row| row
                .get::<_, Option<i64>>(0))
                .unwrap(),
            quantity
        );
    }
    for quantity in [0, -1, i64::MIN] {
        assert!(db.execute("UPDATE crafting_steps SET consumed_outputs=?1 WHERE id=1", [quantity]).is_err());
    }
}

#[test]
fn crafting_steps_use_distinct_slots_and_grant_scoped_lines() {
    let db = crafting_schema_db();
    db.execute_batch("INSERT INTO crafting_steps (system_id, name, sort_order, slot_id) VALUES (2, 'Tier 2', 0, 2);
        INSERT INTO crafting_steps (system_id, name, sort_order) VALUES (1, 'Blank', 1), (1, 'Finish', 2);
        INSERT INTO crafting_step_effects (step_id, effect_id, value, sort_order, equipment_slot_id) VALUES (1, 1, 10, 0, 1);
        INSERT INTO crafting_step_effects (step_id, effect_id, sort_order) VALUES (1, 1, 1);").unwrap();
    assert!(db
        .execute("INSERT INTO crafting_steps (system_id, name, sort_order, slot_id) VALUES (1, 'Duplicate', 3, 1)", [])
        .is_err());
    assert!(db.execute("INSERT INTO crafting_steps (system_id, name, sort_order) VALUES (1, 'Blank', 3)", []).is_err());
    assert!(db
        .execute("INSERT INTO crafting_steps (system_id, name, sort_order, is_random) VALUES (1, 'Random', 3, 2)", [])
        .is_err());
    assert!(db
        .execute("INSERT INTO crafting_step_effects (step_id, effect_id, value2, sort_order) VALUES (1, 1, 11, 2)", [])
        .is_err());
    db.execute("DELETE FROM crafting_systems WHERE id = 1", []).unwrap();
    assert_eq!(db.query_row("SELECT count(*) FROM crafting_step_effects", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
}

#[test]
fn a_slot_type_is_filled_by_one_systems_step() {
    let db = crafting_schema_db();
    let duplicate =
        db.execute("INSERT INTO crafting_steps (system_id,name,sort_order,slot_id) VALUES (2,'Shared slot',0,1)", []);
    assert!(duplicate.is_err(), "a second system filled the same slot type");
    assert!(duplicate.unwrap_err().to_string().contains("UNIQUE"));
    db.execute("INSERT INTO crafting_steps (system_id,name,sort_order,slot_id) VALUES (2,'Own slot',0,2)", []).unwrap();
    db.execute("INSERT INTO crafting_steps (system_id,name,sort_order) VALUES (1,'Unslotted',1),(2,'Unslotted',1)", [])
        .unwrap();
}

#[test]
fn crafting_step_costs_are_one_set_per_grade() {
    let db = crafting_schema_db();
    for tier in ["heroic", "epic", "legendary", "any"] {
        db.execute("INSERT INTO crafting_step_costs (step_id, tier, ingredient_id, quantity, sort_order) VALUES (1, ?1, 1, 60, 0)", [tier]).unwrap();
        assert!(db.execute("INSERT INTO crafting_step_costs (step_id, tier, ingredient_id, quantity, sort_order) VALUES (1, ?1, 1, 61, 0)", [tier]).is_err());
    }
    for (tier, quantity) in [("mythic", 1), ("epic", 0), ("epic", -1)] {
        assert!(db.execute("INSERT INTO crafting_step_costs (step_id, tier, ingredient_id, quantity, sort_order) VALUES (1, ?1, 1, ?2, 1)", rusqlite::params![tier, quantity]).is_err());
    }
}

#[test]
fn crafting_cost_views_add_own_lines_to_exact_tiers_and_any_fallbacks() {
    let db = crafting_schema_db();
    db.execute_batch(
        "UPDATE crafting_recipes SET step_id=1 WHERE id=1;
        INSERT INTO crafting_steps (id,system_id,name,sort_order) VALUES (2,1,'Exact only',1);
        INSERT INTO crafting_recipes (id,system_id,step_id,tier,option,sort_order) VALUES
            (2,1,2,'heroic','Exact',1), (3,1,1,'legendary','Fallback',2),
            (4,1,1,'epic','Shadow',3), (5,1,NULL,'any','No step',4),
            (6,1,2,'legendary','No grade cost',5);
        INSERT INTO crafting_step_costs (step_id,tier,sort_order,ingredient_id,item_id,quantity) VALUES
            (1,'any',0,1,NULL,3), (1,'any',1,NULL,1,2),
            (1,'epic',0,1,NULL,60), (2,'heroic',0,1,NULL,11);
        INSERT INTO crafting_recipe_ingredients (recipe_id,sort_order,item_id,quantity) VALUES (1,0,1,7);",
    )
    .unwrap();
    let step_rows = db.prepare("SELECT recipe_id,sort_order,ingredient_id,item_id,tier,quantity FROM crafting_recipe_step_costs ORDER BY recipe_id,sort_order").unwrap()
        .query_map([], |row| Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,Option<i64>>(2)?,row.get::<_,Option<i64>>(3)?,row.get::<_,String>(4)?,row.get::<_,i64>(5)?))).unwrap()
        .collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(
        step_rows,
        [
            (1, 0, Some(1), None, "epic".into(), 60),
            (2, 0, Some(1), None, "heroic".into(), 11),
            (3, 0, Some(1), None, "legendary".into(), 3),
            (3, 1, None, Some(1), "legendary".into(), 2),
            (4, 0, Some(1), None, "epic".into(), 60),
        ]
    );
    let effective = db.prepare("SELECT recipe_id,sort_order,ingredient_id,item_id,tier,quantity,from_step FROM crafting_recipe_effective_costs ORDER BY recipe_id,from_step,sort_order").unwrap()
        .query_map([], |row| Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,Option<i64>>(2)?,row.get::<_,Option<i64>>(3)?,row.get::<_,String>(4)?,row.get::<_,i64>(5)?,row.get::<_,i64>(6)?))).unwrap()
        .collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(
        effective,
        [
            (1, 0, None, Some(1), "epic".into(), 7, 0),
            (1, 0, Some(1), None, "epic".into(), 60, 1),
            (2, 0, Some(1), None, "heroic".into(), 11, 1),
            (3, 0, Some(1), None, "legendary".into(), 3, 1),
            (3, 1, None, Some(1), "legendary".into(), 2, 1),
            (4, 0, Some(1), None, "epic".into(), 60, 1),
        ]
    );
}

#[test]
fn cost_rows_point_at_exactly_one_ingredient_or_item() {
    let db = crafting_schema_db();
    for (table, owner_columns, owner_values) in
        [("crafting_recipe_ingredients", "recipe_id", "1"), ("crafting_step_costs", "step_id, tier", "1, 'epic'")]
    {
        for (position, (ingredient, item, valid)) in
            [(None, None, false), (Some(1), Some(1), false), (Some(1), None, true), (None, Some(1), true)]
                .into_iter()
                .enumerate()
        {
            assert_eq!(db.execute(&format!("INSERT INTO {table} ({owner_columns}, ingredient_id, item_id, quantity, sort_order) VALUES ({owner_values}, ?1, ?2, 1, ?3)"), rusqlite::params![ingredient, item, i64::try_from(position).unwrap()]).is_ok(), valid, "{table}: {ingredient:?}/{item:?}");
        }
    }
    db.execute("DELETE FROM items WHERE id = 1", []).unwrap();
    for table in ["crafting_recipe_ingredients", "crafting_step_costs"] {
        assert_eq!(db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    }
}

#[test]
fn crafting_unlocks_belong_to_exactly_one_parent_and_follow_it() {
    for parent in ["system_id", "station_id", "recipe_id"] {
        let db = crafting_schema_db();
        db.execute(
            &format!("INSERT INTO crafting_unlocks ({parent}, sort_order, kind, text) VALUES (1, 0, 'other', 'Gate')"),
            [],
        )
        .unwrap();
        let parent_table = match parent {
            "system_id" => "crafting_systems",
            "station_id" => "crafting_stations",
            _ => "crafting_recipes",
        };
        db.execute(&format!("DELETE FROM {parent_table} WHERE id = 1"), []).unwrap();
        assert_eq!(db.query_row("SELECT count(*) FROM crafting_unlocks", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
    }
    let db = crafting_schema_db();
    for parents in ["NULL, NULL, NULL", "1, 1, NULL", "1, NULL, 1", "NULL, 1, 1", "1, 1, 1"] {
        assert!(db.execute(&format!("INSERT INTO crafting_unlocks (system_id, station_id, recipe_id, sort_order, kind, text) VALUES ({parents}, 0, 'other', 'Gate')"), []).is_err());
    }
}

#[test]
fn crafting_unlocks_tie_each_kind_to_its_reference() {
    let db = crafting_schema_db();
    db.execute_batch(
        "INSERT INTO adventure_packs (id, name, slug) VALUES (1, 'Pack', 'pack');
        INSERT INTO quests (id, name, slug, pack_id) VALUES (1, 'Quest', 'quest', 1);
        INSERT INTO patrons (id, name, slug) VALUES (1, 'Patron', 'patron');
        INSERT INTO feats (id, name, slug, source_kind) VALUES (1, 'Feat', 'feat', 'standard');",
    )
    .unwrap();
    let reference_columns =
        ["quest_id", "patron_id", "pack_id", "item_id", "crafting_ingredient_id", "feat_id", "value"];
    for (kind, required) in [
        ("quest", vec!["quest_id"]),
        ("raid_flagging", vec!["quest_id"]),
        ("favor", vec!["patron_id", "value"]),
        ("level", vec!["value"]),
        ("feat", vec!["feat_id"]),
        ("pack", vec!["pack_id"]),
        ("kept_item", vec!["item_id"]),
        ("kept_item", vec!["crafting_ingredient_id"]),
        ("guild_amenity", vec!["value"]),
        ("crafting_level", vec!["value"]),
        ("reaper_experience", vec!["value"]),
        ("other", vec![]),
    ] {
        for mask in 0..128 {
            let present: Vec<_> = reference_columns
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1 << index) != 0)
                .map(|(_, column)| *column)
                .collect();
            let valid = present == required
                || (kind == "kept_item" && (present == ["item_id"] || present == ["crafting_ingredient_id"]));
            let values = reference_columns
                .iter()
                .map(|column| if present.contains(column) { "1" } else { "NULL" })
                .collect::<Vec<_>>()
                .join(", ");
            let inserted = db.execute(&format!("INSERT INTO crafting_unlocks (system_id, sort_order, kind, text, {}) VALUES (1, 0, ?1, 'Gate', {values})", reference_columns.join(", ")), [kind]);
            assert_eq!(inserted.is_ok(), valid, "{kind}: {present:?}: {inserted:?}");
        }
    }
}

#[test]
fn item_sockets_can_only_point_at_a_set_of_their_own_label() {
    let db = crafting_schema_db();
    db.execute("INSERT INTO item_augment_slots (item_id, sort_order, slot_id, option_set_id) VALUES (1, 0, 1, 1)", [])
        .unwrap();
    db.execute("INSERT INTO item_augment_slots (item_id, sort_order, slot_id) VALUES (1, 1, 2)", []).unwrap();
    assert!(db
        .execute("INSERT INTO item_augment_slots (item_id, sort_order, slot_id, option_set_id) VALUES (1, 2, 2, 1)", [])
        .is_err());
}

#[test]
fn socket_option_sets_cascade_their_options_and_lines() {
    let db = crafting_schema_db();
    db.execute_batch(
        "INSERT INTO socket_option_effects (option_id, effect_id, sort_order) VALUES (1, 1, 0);
        INSERT INTO socket_option_grants (option_id, sort_order, slot_id) VALUES (1, 0, 2);
        INSERT INTO set_bonuses (id, name, slug) VALUES (1, 'Set', 'set');
        INSERT INTO socket_option_set_bonuses (option_id, set_bonus_id) VALUES (1, 1);
        DELETE FROM socket_option_sets WHERE id = 1;",
    )
    .unwrap();
    for table in ["socket_options", "socket_option_effects", "socket_option_grants", "socket_option_set_bonuses"] {
        assert_eq!(
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get::<_, i64>(0)).unwrap(),
            0,
            "{table}"
        );
    }
}

#[test]
fn augment_socket_grants_cascade_with_their_augment() {
    let db = crafting_schema_db();
    db.execute_batch(
        "INSERT INTO augment_socket_grants (augment_id, sort_order, slot_id) VALUES (1, 0, 1), (1, 1, 2);
        DELETE FROM augments WHERE id = 1;",
    )
    .unwrap();
    assert_eq!(db.query_row("SELECT count(*) FROM augment_socket_grants", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
}

#[test]
fn socket_labels_carry_an_optional_not_crafting_reason() {
    let db = crafting_schema_db();
    assert_eq!(
        db.query_row("SELECT not_crafting_reason FROM augment_slot_types WHERE id = 1", [], |r| r
            .get::<_, Option<String>>(0))
            .unwrap(),
        None
    );
    db.execute("UPDATE augment_slot_types SET not_crafting_reason = 'Random loot generator' WHERE id = 1", []).unwrap();
    assert_eq!(
        db.query_row("SELECT not_crafting_reason FROM augment_slot_types WHERE id = 1", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "Random loot generator"
    );
}

#[test]
fn crafting_station_and_option_set_variants_distinguish_equal_names() {
    let db = crafting_schema_db();
    for (table, name, extra_columns, extra_values) in [
        ("crafting_stations", "Forge", "kind, sort_order", "'interface', 1"),
        ("socket_option_sets", "Choices", "slot_id", "1"),
    ] {
        for (variant, slug, valid) in [
            (None, "duplicate", false),
            (Some(""), "empty", false),
            (Some("second"), "second", true),
            (Some("second"), "duplicate-second", false),
        ] {
            assert_eq!(db.execute(&format!("INSERT INTO {table} (name, slug, variant, {extra_columns}) VALUES (?1, ?2, ?3, {extra_values})"), rusqlite::params![name, slug, variant]).is_ok(), valid, "{table}/{variant:?}");
        }
    }
}

#[test]
fn crafting_stations_require_a_place_unless_they_are_an_interface() {
    let db = fresh_db();
    for (kind, place, valid) in [
        ("device", None, false),
        ("npc", None, false),
        ("interface", None, true),
        ("device", Some("Meridia"), true),
        ("npc", Some("Meridia"), true),
        ("interface", Some("Inventory"), true),
        ("unknown", Some("Meridia"), false),
    ] {
        db.execute("DELETE FROM crafting_stations", []).unwrap();
        assert_eq!(db.execute("INSERT INTO crafting_stations (name, slug, kind, place, sort_order) VALUES ('Station', 'station', ?1, ?2, 0)", rusqlite::params![kind, place]).is_ok(), valid, "{kind}/{place:?}");
    }
}

#[test]
fn crafting_shared_ingredients_do_not_require_a_system_during_transition() {
    let db = fresh_db();
    db.execute("INSERT INTO crafting_ingredients (name, slug, tier) VALUES ('Thread', 'thread', 'any')", []).unwrap();
}

#[test]
fn annotations_require_exactly_one_page_or_in_game_source() {
    for (url, in_game, valid) in [
        (Some("https://ddowiki.com/page/Test"), 0, true),
        (None, 1, true),
        (Some("https://ddowiki.com/page/Test"), 1, false),
        (None, 0, false),
        (None, 2, false),
    ] {
        let db = fresh_db();
        let citation = db.execute(
            "INSERT INTO citations (owner_kind, owner_id, field, url, in_game, read) VALUES ('enhancements', 1, 'cost_per_rank', ?1, ?2, '2026-10-18')",
            rusqlite::params![url, in_game],
        );
        assert_eq!(citation.is_ok(), valid, "citation: {url:?}/{in_game}");
        let correction = db.execute(
            "INSERT INTO corrections (kind, name, field, from_value, to_value, reason, source, in_game, read) VALUES ('enhancement', 'Test', 'cost_per_rank', '[2]', '[1]', 'Observed cost.', ?1, ?2, '2026-10-18')",
            rusqlite::params![url, in_game],
        );
        assert_eq!(correction.is_ok(), valid, "correction: {url:?}/{in_game}");
        if valid {
            assert!(db.execute(
                "INSERT INTO citations (owner_kind, owner_id, field, url, in_game, read) VALUES ('enhancements', 1, 'cost_per_rank', ?1, ?2, '2026-10-19')",
                rusqlite::params![url, in_game],
            ).is_err());
        }
    }
}

#[test]
fn corrections_record_each_kind_field_and_value_change_once() {
    let db = fresh_db();
    let mut statement = db.prepare("SELECT name FROM pragma_table_info('corrections') ORDER BY cid").unwrap();
    let columns: Vec<String> = statement.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    assert_eq!(
        columns,
        ["id", "kind", "name", "qualifier", "field", "from_value", "to_value", "reason", "source", "in_game", "read"]
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

fn insert_source(db: &Connection, table: &str, columns_and_values: &str) -> rusqlite::Result<usize> {
    let (columns, values) = columns_and_values.split_once(" = ").unwrap();
    db.execute(&format!("INSERT INTO {table} ({columns}) VALUES ({values})"), [])
}

fn db_with_one_source_of_each_kind() -> Connection {
    let db = fresh_db();
    db.execute_batch(
        "PRAGMA foreign_keys = OFF;
         INSERT INTO items (id, name, slot_id, item_category, wiki_url, slug)
              VALUES (1, 'Rusted Crown', 1, 'Jewelry', 'https://ddowiki.com/page/Item:Rusted_Crown', 'rusted-crown');
         INSERT INTO adventure_packs (id, name, slug) VALUES (1, 'Magic of Myth Drannor', 'magic-of-myth-drannor');
         INSERT INTO quests (id, name, pack_id, slug) VALUES (1, 'Book Burning', 1, 'book-burning');
         INSERT INTO quest_chains (id, name, provenance, wiki_url, slug) VALUES (1, 'The Necropolis', 'wiki', 'https://ddowiki.com/page/Necropolis', 'the-necropolis');
         INSERT INTO sagas (id, name, provenance, wiki_url, slug) VALUES (1, 'Dread', 'wiki', 'https://ddowiki.com/page/Dread', 'dread');
         INSERT INTO augments (id, name, family, slug) VALUES (1, 'Lunar Gem of Magical Protection (Heroic)', 'SunAndMoon', 'lunar-gem-of-magical-protection-heroic');
         INSERT INTO crafting_systems (id, name, page, slug) VALUES (1, 'Catalyst Crafting', 'https://ddowiki.com/page/Catalyst_Crafting', 'catalyst-crafting');
         INSERT INTO vendors (id, name, location, pack_id, provenance, wiki_url, slug)
              VALUES (1, 'Morten Edgewright', 'House Jorasco', NULL, 'wiki', 'https://ddowiki.com/page/Morten_Edgewright', 'morten-edgewright');
         INSERT INTO events (id, name, provenance, wiki_url, slug)
              VALUES (1, 'Treasure of Crystal Cove', 'wiki', 'https://ddowiki.com/page/Crystal_Cove', 'treasure-of-crystal-cove');",
    )
    .unwrap();
    db
}

#[test]
fn source_kinds_have_strict_tables_and_a_read_only_union() {
    let db = fresh_db();
    for table in [
        "quest_drops",
        "pack_drops",
        "vendor_offers",
        "vendor_offer_costs",
        "event_rewards",
        "quest_chain_rewards",
        "saga_rewards",
        "starter_gear",
        "crafting_outputs",
        "challenge_rewards",
        "vendor_currencies",
    ] {
        assert_eq!(
            db.query_row("SELECT type FROM sqlite_master WHERE name = ?1", [table], |row| row.get::<_, String>(0))
                .unwrap(),
            "table"
        );
        let mut columns =
            db.prepare(&format!("SELECT name, \"notnull\", pk FROM pragma_table_info('{table}')")).unwrap();
        for column in columns
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?, row.get::<_, bool>(2)?)))
            .unwrap()
        {
            let (name, required, primary) = column.unwrap();
            assert!(
                required || primary || ["chest", "drop_text_segment", "source_match"].contains(&name.as_str()),
                "{table}.{name} must be required"
            );
        }
    }
    assert_eq!(
        db.query_row("SELECT type FROM sqlite_master WHERE name = 'sources'", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "view"
    );
    assert!(db.execute("INSERT INTO sources (kind) VALUES ('quest')", []).is_err());
}

#[test]
fn source_owner_reads_use_indexes_in_every_union_branch() {
    let db = fresh_db();
    let mut query = db.prepare("EXPLAIN QUERY PLAN SELECT * FROM sources WHERE item_id = 1").unwrap();
    let steps =
        query.query_map([], |row| row.get::<_, String>(3)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert!(!steps.iter().any(|step| step.starts_with("SCAN owned")), "{steps:?}");
}

#[test]
fn deleting_loot_removes_only_its_own_source_rows() {
    use ddo_model::enums::SourceKind;
    let db = db_with_one_source_of_each_kind();
    for kind in SourceKind::ALL {
        let table = kind.table_name();
        let target = kind.identifying_column();
        if *kind == SourceKind::Vendor {
            db.execute("INSERT INTO vendor_offers (vendor_id, item_id) VALUES (1, 1)", []).unwrap();
            continue;
        }
        let (extra_columns, extra_values) = if kind.has_loot_type() { (", loot_type", ", 'chest'") } else { ("", "") };
        for owner in ["item", "augment"] {
            db.execute(&format!("INSERT INTO {table} ({target}, owner_kind, owner_id{extra_columns}) VALUES (1, ?1, 1{extra_values})"), [owner]).unwrap();
        }
    }
    db.execute("PRAGMA foreign_keys = ON", []).unwrap();
    db.execute("DELETE FROM items WHERE id = 1", []).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM sources WHERE item_id = 1", [], |row| row.get::<_, i64>(0)).unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM sources WHERE augment_id = 1", [], |row| row.get::<_, i64>(0)).unwrap(),
        8
    );
    db.execute("DELETE FROM augments WHERE id = 1", []).unwrap();
    assert_eq!(db.query_row("SELECT COUNT(*) FROM sources", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
}

#[test]
fn vendor_offer_cost_constraints_reject_invalid_prices() {
    let db = db_with_one_source_of_each_kind();
    db.execute("INSERT INTO vendor_offers (id, vendor_id, item_id) VALUES (1, 1, 1)", []).unwrap();
    for (kind, quantity, position) in
        [("vendor_currency", 0, 0), ("vendor_currency", -1, 0), ("gold", 1, 0), ("vendor_currency", 1, -1)]
    {
        let error = db.execute(
            "INSERT INTO vendor_offer_costs (offer_id, ingredient_kind, ingredient_id, quantity, sort_order) VALUES (1, ?1, 1, ?2, ?3)",
            rusqlite::params![kind, quantity, position],
        ).unwrap_err();
        assert!(error.to_string().contains("CHECK constraint failed"), "{error}");
    }
    for (position, kind) in ["vendor_currency", "crafting_ingredient"].into_iter().enumerate() {
        db.execute(
            "INSERT INTO vendor_offer_costs (offer_id, ingredient_kind, ingredient_id, quantity, sort_order) VALUES (1, ?1, 1, ?2, ?3)",
            rusqlite::params![kind, i64::MAX, position as i64],
        ).unwrap();
    }
}

#[test]
fn sources_link_each_loot_to_exactly_one_source_of_its_kind() {
    let db = db_with_one_source_of_each_kind();
    for (table, valid_source) in [
        ("quest_drops", "quest_id, owner_kind, owner_id, loot_type, chest = 1, 'item', 1, 'chest', 'end chest'"),
        ("quest_drops", "quest_id, owner_kind, owner_id, loot_type = 1, 'item', 1, 'reward'"),
        ("quest_drops", "quest_id, owner_kind, owner_id, loot_type = 1, 'augment', 1, 'raid'"),
        ("quest_chain_rewards", "chain_id, owner_kind, owner_id = 1, 'item', 1"),
        ("saga_rewards", "saga_id, owner_kind, owner_id, tier = 1, 'item', 1, 'legendary'"),
        ("saga_rewards", "saga_id, owner_kind, owner_id = 1, 'item', 1"),
        ("pack_drops", "pack_id, owner_kind, owner_id, loot_type, is_rare = 1, 'item', 1, 'chest', 1"),
        ("pack_drops", "pack_id, owner_kind, owner_id, loot_type = 1, 'augment', 1, 'chest'"),
        ("challenge_rewards", "pack_id, owner_kind, owner_id = 1, 'item', 1"),
        ("crafting_outputs", "crafting_system_id, owner_kind, owner_id, is_rare = 1, 'item', 1, 1"),
        ("vendor_offers", "vendor_id, item_id = 1, 1"),
        ("event_rewards", "event_id, owner_kind, owner_id = 1, 'augment', 1"),
        ("starter_gear", "character_level, owner_kind, owner_id = 15, 'item', 1"),
    ] {
        insert_source(&db, table, valid_source).unwrap_or_else(|error| panic!("{table}: {valid_source}: {error}"));
        assert!(insert_source(&db, table, valid_source).is_err(), "duplicate {table} source must fail");
        for invalid_rarity in ["NULL", "2", "-1"] {
            assert!(db.execute(&format!("UPDATE {table} SET is_rare = {invalid_rarity}"), []).is_err());
        }
        for rarity in [0, 1] {
            db.execute(&format!("UPDATE {table} SET is_rare = {rarity}"), []).unwrap();
        }
        for column in
            if table == "vendor_offers" { vec!["vendor_id", "item_id"] } else { vec!["owner_kind", "owner_id"] }
        {
            assert!(
                db.execute(&format!("UPDATE {table} SET {column} = NULL"), []).is_err(),
                "{table}.{column} is required"
            );
        }
        if table != "vendor_offers" {
            assert!(db.execute(&format!("UPDATE {table} SET owner_kind = 'unknown'"), []).is_err());
        }
    }
    for (table, invalid_source) in [
        ("quest_drops", "owner_kind, owner_id, loot_type = 'item', 1, 'chest'"),
        ("quest_drops", "quest_id, owner_kind, owner_id, loot_type, chest = 1, 'item', 1, 'reward', 'end chest'"),
        ("quest_drops", "quest_id, owner_kind, owner_id = 1, 'item', 1"),
        ("quest_drops", "quest_id, owner_kind, owner_id, loot_type = 1, 'item', 1, 'bag'"),
        ("pack_drops", "pack_id, owner_kind, owner_id, loot_type, chest = 1, 'item', 1, 'reward', 'end chest'"),
        ("saga_rewards", "saga_id, owner_kind, owner_id, tier = 1, 'item', 1, 'mythic'"),
        ("saga_rewards", "saga_id, owner_kind, owner_id, tier = 1, 'item', 1, NULL"),
        ("starter_gear", "character_level, owner_kind, owner_id = 0, 'item', 1"),
    ] {
        assert!(insert_source(&db, table, invalid_source).is_err(), "{table}: {invalid_source} must fail");
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
        ["id", "slug", "variant", "name", "pack_id", "provenance", "wiki_url"]
    );
    assert_eq!(
        column_shapes(&db, "quest_chain_quests", &["chain_id"]),
        column_shapes(&db, "saga_quests", &["saga_id"])
    );
}

#[test]
fn a_saga_reward_has_one_row_per_item_and_tier() {
    let db = db_with_one_source_of_each_kind();
    for tier in ["'epic'", "'legendary'", "'any'"] {
        insert_source(&db, "saga_rewards", &format!("saga_id, owner_kind, owner_id, tier = 1, 'item', 1, {tier}"))
            .unwrap();
        assert!(
            insert_source(&db, "saga_rewards", &format!("saga_id, owner_kind, owner_id, tier = 1, 'item', 1, {tier}"))
                .is_err(),
            "one row per item and tier {tier}"
        );
    }
    assert!(
        db.execute(
            "INSERT INTO sagas (name, provenance, wiki_url, slug) VALUES ('Odd Saga', 'ddowiki', 'https://ddowiki.com/page/Odd', 'odd-saga')",
            []
        )
        .is_err(),
        "provenance is maetrim or wiki"
    );
}

#[test]
fn sources_record_the_chest_and_drop_text_provenance() {
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
            "cost",
            "drop_text_segment",
            "source_match"
        ]
    );
    assert!(columns.contains(&("chest".to_string(), "TEXT".to_string())));
    assert!(columns.contains(&("drop_text_segment".to_string(), "TEXT".to_string())));
    assert!(columns.contains(&("source_match".to_string(), "TEXT".to_string())));
    let source_db = db_with_one_source_of_each_kind();
    insert_source(&source_db, "quest_drops", "quest_id, owner_kind, owner_id, loot_type = 1, 'item', 1, 'chest'")
        .unwrap();
    assert!(source_db
        .execute(
            "UPDATE quest_drops SET drop_text_segment = 'Quest chest' WHERE id = (SELECT MIN(id) FROM quest_drops)",
            []
        )
        .is_err());
}

#[test]
fn sources_distinguish_chests_but_reject_an_exact_duplicate_even_with_nulls() {
    let db = fresh_db();
    ddo_model::seeds::insert_all(&db).unwrap();
    db.execute("INSERT INTO adventure_packs (name, slug) VALUES ('Source Identity Pack', 'source-identity-pack')", [])
        .unwrap();
    db.execute(
        "INSERT INTO quests (name, pack_id, slug) VALUES ('Source Identity Quest', (SELECT id FROM adventure_packs WHERE name = 'Source Identity Pack'), 'source-identity-quest')",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO items (name, slot_id, item_category, wiki_url, slug) VALUES
         ('Source Identity Item', (SELECT id FROM equipment_slots WHERE name = 'Ring'), 'Jewelry',
          'https://ddowiki.com/page/Item:Source_Identity_Item', 'source-identity-item')",
        [],
    )
    .unwrap();
    let source = "INSERT INTO quest_drops (owner_kind, quest_id, owner_id, loot_type, chest) VALUES
        ('item', (SELECT id FROM quests WHERE name = 'Source Identity Quest'),
         (SELECT id FROM items WHERE name = 'Source Identity Item'), 'chest', ?1)";
    db.execute(source, ["end chest"]).unwrap();
    assert!(db.execute(source, ["end chest"]).is_err());
    db.execute(source, ["optional chest"]).unwrap();
    db.execute(source, ["null chest probe"]).unwrap();
    let without_chest = "INSERT INTO quest_drops (owner_kind, quest_id, owner_id, loot_type) VALUES
        ('item', (SELECT id FROM quests WHERE name = 'Source Identity Quest'),
         (SELECT id FROM items WHERE name = 'Source Identity Item'), 'chest')";
    db.execute(without_chest, []).unwrap();
    assert!(db.execute(without_chest, []).is_err());
}

#[test]
fn vendors_and_events_carry_their_provenance_and_page() {
    let db = fresh_db();
    assert_eq!(
        column_shapes(&db, "vendors", &[]).iter().map(|(name, ..)| name.as_str()).collect::<Vec<_>>(),
        ["id", "slug", "variant", "name", "location", "pack_id", "provenance", "wiki_url"]
    );
    assert_eq!(
        column_shapes(&db, "events", &[]).iter().map(|(name, ..)| name.as_str()).collect::<Vec<_>>(),
        ["id", "slug", "variant", "name", "provenance", "wiki_url"]
    );
    assert!(db
        .execute(
            "INSERT INTO events (name, provenance, wiki_url, slug) VALUES ('Odd', 'ddowiki', 'https://ddowiki.com/page/Odd', 'odd')",
            []
        )
        .is_err());
    assert!(db
        .execute("INSERT INTO vendors (name, provenance, slug) VALUES ('No Page', 'wiki', 'no-page')", [])
        .is_err());
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
    for retired in ["Command", "Speed", "Deception", "Attack Speed"] {
        assert!(Stat::by_name(retired).is_none(), "{retired} is an effect or retired stat name");
    }
    assert!(Stat::by_name("Melee Attack Speed").is_some());
    assert!(Stat::by_name("Ranged Attack Speed").is_some());
    for (school, id) in [
        ("Abjuration", 92),
        ("Conjuration", 93),
        ("Enchantment", 94),
        ("Evocation", 95),
        ("Illusion", 96),
        ("Necromancy", 97),
        ("Transmutation", 98),
        ("Divination", 233),
    ] {
        assert_eq!(Stat::by_name(&format!("{school} Focus")).map(|stat| stat.id), Some(id));
        assert!(Stat::by_name(&format!("{school} Spell Focus")).is_none());
    }
}

#[test]
fn stats_carry_no_second_name_for_a_stat_the_maps_already_target() {
    let canonical_stat_by_alias = [
        ("Physical Sheltering", "Physical Resistance Rating"),
        ("Magical Sheltering", "Magical Resistance Rating"),
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
    let stat_count: i64 = db.query_row("SELECT COUNT(*) FROM effects WHERE is_stat = 1", [], |r| r.get(0)).unwrap();
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
fn seeded_stats_share_effect_identity_and_direct_links_yield_owner_bonuses() {
    let db = fresh_db();
    ddo_model::seeds::insert_all(&db).unwrap();
    let strength_id: i64 = db
        .query_row(
            "SELECT id FROM effects WHERE name = 'Strength' AND is_stat = 1 AND category = 'ability'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(strength_id, Stat::by_name("Strength").unwrap().id);
    db.execute("INSERT INTO items (id, name, slot_id, item_category, provenance, wiki_url, slug) VALUES (9001, 'Probe', 1, 'Jewelry', 'maetrim', 'https://ddowiki.com/page/Item:Probe', 'probe')", []).unwrap();
    db.execute(
        "INSERT INTO item_effects (item_id, effect_id, bonus_type_id, value, sort_order) VALUES (9001, ?1, 1, 7, 0)",
        [strength_id],
    )
    .unwrap();
    let bonus: (i64, i64, String, Option<i64>, String, f64) = db.query_row(
        "SELECT stat_id, amount, amount_source, via_effect_id, owner_kind, scale FROM owner_bonuses WHERE owner_id = 9001",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
    ).unwrap();
    assert_eq!(bonus, (strength_id, 7, "owner".to_string(), None, "item".to_string(), 1.0));
}

#[test]
fn omitted_bonus_type_is_a_boolean_restricted_to_named_effects() {
    let db = fresh_db();
    ddo_model::seeds::insert_all(&db).unwrap();
    db.execute("INSERT INTO effects (name,slug,verbose_name_template,omits_bonus_type) VALUES ('Command','command','Command +{1}',1)", []).unwrap();
    for sql in [
        "UPDATE effects SET omits_bonus_type=1 WHERE name='Strength'",
        "UPDATE effects SET is_group=1 WHERE name='Command'",
        "UPDATE effects SET omits_bonus_type=2 WHERE name='Command'",
        "UPDATE effects SET omits_bonus_type=-1 WHERE name='Command'",
        "UPDATE effects SET omits_bonus_type=NULL WHERE name='Command'",
    ] {
        assert!(db.execute(sql, []).is_err(), "{sql}");
    }
    db.execute("INSERT INTO effect_aliases (effect_id,name,verbose_name_template,omits_bonus_type,sort_order) SELECT id,'Quiet Command','Quiet Command +{1}',1,0 FROM effects WHERE name='Command'", []).unwrap();
    for literal in ["2", "-1", "NULL"] {
        assert!(db.execute(&format!("UPDATE effect_aliases SET omits_bonus_type={literal}"), []).is_err());
    }
}

#[test]
fn group_bonus_targets_expand_once_and_reject_nested_groups() {
    let db = fresh_db();
    ddo_model::seeds::insert_all(&db).unwrap();
    db.execute("INSERT INTO effects (id, name, is_group, verbose_name_template, slug) VALUES (1000, 'Charisma Skills', 1, '%b1 Charisma Skills {1}', 'charisma-skills')", []).unwrap();
    db.execute("INSERT INTO effects (id, name, verbose_name_template, slug) VALUES (1001, 'Command', 'Command {1} {2}', 'command')", [])
        .unwrap();
    db.execute(
        "INSERT INTO effects (id, name, is_group, verbose_name_template, slug) VALUES (1002, 'Other Group', 1, 'Other Group {1}', 'other-group')",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO effect_bonuses (effect_id, target_effect_id, amount_from, sort_order) VALUES (1000, 42, 1, 0)",
        [],
    )
    .unwrap();
    db.execute("INSERT INTO effect_bonuses (effect_id, target_effect_id, bonus_type_id, amount_from, sort_order) VALUES (1001, 1000, 1, 1, 0)", []).unwrap();
    assert!(db.execute("UPDATE effects SET is_group = 0 WHERE id = 1000", []).is_err());
    assert!(db.execute("UPDATE effects SET is_group = 1 WHERE id = 1001", []).is_err());
    assert!(db.execute("INSERT INTO effect_bonuses (effect_id, target_effect_id, amount_from, sort_order) VALUES (1002, 1000, 1, 0)", []).is_err());
    assert!(db.execute("INSERT INTO effect_bonuses (effect_id, target_effect_id, amount_from, sort_order) VALUES (1000, 1001, 1, 1)", []).is_err());
    assert!(db
        .execute(
            "INSERT INTO effect_bonuses (effect_id, target_effect_id, amount_from, sort_order) VALUES (1000, 44, 2, 1)",
            []
        )
        .is_err());
    db.execute("INSERT INTO items (id, name, slot_id, item_category, wiki_url, slug) VALUES (9001, 'Probe', 1, 'Jewelry', 'https://ddowiki.com/page/Item:Probe', 'probe')", []).unwrap();
    db.execute("INSERT INTO item_effects (item_id, effect_id, value, sort_order) VALUES (9001, 1001, 7, 0)", [])
        .unwrap();
    let bonus: (i64, i64, i64, i64) = db
        .query_row(
            "SELECT stat_id, amount, via_effect_id, group_effect_id FROM owner_bonuses WHERE owner_id = 9001",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(bonus, (42, 7, 1001, 1000));
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
    db.execute(
        "INSERT INTO effects (id, name, verbose_name_template, slug) VALUES (1000, 'Probe', 'Probe {1}', 'probe')",
        [],
    )
    .unwrap();
    for (amount_from, constant) in [(0, "NULL"), (1, "1"), (3, "NULL")] {
        assert!(db
            .execute(
                &format!(
                    "INSERT INTO effect_bonuses (effect_id, target_effect_id, bonus_type_id, amount_from, constant, sort_order)
             VALUES (1000, 1, 1, {amount_from}, {constant}, 0)"
                ),
                []
            )
            .is_err());
    }
    db.execute(
        "INSERT INTO effect_bonuses (effect_id, target_effect_id, bonus_type_id, amount_from, sort_order) VALUES (1000, 1, 1, 1, 0)",
        [],
    ).unwrap();
}

#[test]
fn effect_defaults_and_scaled_stat_constraints_are_checked() {
    let db = fresh_db();
    ddo_model::seeds::insert_all(&db).unwrap();
    db.execute(
        "INSERT INTO effects (id, name, verbose_name_template, default_value, default_value2, slug)
         VALUES (1000, 'Scaled Probe', 'Scaled Probe {1} {2}', 3, 4, 'scaled-probe')",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO effect_bonuses
         (effect_id, target_effect_id, bonus_type_id, amount_from, scale, rounding, sort_order)
         VALUES (1000, 1, 1, 1, 0.5, 'up', 0)",
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
                     (effect_id, target_effect_id, bonus_type_id, amount_from, constant, scale, rounding, sort_order)
                     VALUES (1000, 2, 1, {amount_from}, {constant}, {scale}, {rounding}, 1)"
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
fn effect_urls_and_tier_positions_are_checked() {
    let db = fresh_db();
    for (name, wiki_url) in
        [("Wrong Host", "'https://example.com/page/Wrong_Host'"), ("Wrong Path", "'https://ddowiki.com/Wrong_Path'")]
    {
        assert!(
            db.execute(
                &format!(
                    "INSERT INTO effects (name, verbose_name_template, wiki_url, slug)
             VALUES (?1, 'A {{1}}', {wiki_url}, ?2)"
                ),
                rusqlite::params![name, ddo_model::identity::slug(name)]
            )
            .is_err(),
            "{name}"
        );
    }
    db.execute("INSERT INTO effect_tier_groups (id, name) VALUES (1, 'Deception')", []).unwrap();
    db.execute(
        "INSERT INTO effects (id, name, verbose_name_template, tier_group_id, tier, wiki_url, slug)
                VALUES (1000, 'Deception', 'Deception {1}', 1, 1, 'https://ddowiki.com/page/Deception', 'deception')",
        [],
    )
    .unwrap();
    assert!(db
        .execute(
            "INSERT INTO effects (name, verbose_name_template, tier_group_id, slug)
                        VALUES ('Improved Deception', 'Improved Deception', 1, 'improved-deception')",
            []
        )
        .is_err());
    assert!(db
        .execute(
            "INSERT INTO effects (name, verbose_name_template, tier, slug)
                        VALUES ('Greater Deception', 'Greater Deception', 2, 'greater-deception')",
            []
        )
        .is_err());
    assert!(db
        .execute(
            "INSERT INTO effects (name, verbose_name_template, tier_group_id, tier, slug)
                        VALUES ('Duplicate Rank', 'Duplicate Rank', 1, 1, 'duplicate-rank')",
            []
        )
        .is_err());
}

#[test]
fn stat_set_declarations_require_a_known_verdict_and_a_nonblank_reason() {
    let db = fresh_db();
    db.execute(
        "INSERT INTO effects (name,slug,verbose_name_template,stat_set_verdict,stat_set_reason)
         VALUES ('Declared Effect','declared-effect','Declared Effect','distinct','Different rank')",
        [],
    )
    .unwrap();
    for (verdict, reason) in
        [(None, None), (Some("distinct"), Some("Different rank")), (Some("composes"), Some("Additional mechanic"))]
    {
        db.execute(
            "UPDATE effects SET stat_set_verdict=?1,stat_set_reason=?2 WHERE slug='declared-effect'",
            rusqlite::params![verdict, reason],
        )
        .unwrap();
    }
    for (verdict, reason) in [
        (Some("distinct"), None),
        (None, Some("Different rank")),
        (Some("distinct"), Some("")),
        (Some("composes"), Some("   ")),
        (Some("unknown"), Some("Different rank")),
    ] {
        assert!(
            db.execute(
                "UPDATE effects SET stat_set_verdict=?1,stat_set_reason=?2 WHERE slug='declared-effect'",
                rusqlite::params![verdict, reason],
            )
            .is_err(),
            "{verdict:?}: {reason:?}"
        );
    }
}

#[test]
fn effect_owner_links_require_the_first_amount_before_the_second() {
    let db = fresh_db();
    db.execute(
        "INSERT INTO effects (id, name, verbose_name_template, slug)
                VALUES (1000, 'Probe', 'Probe {1} {2}', 'probe')",
        [],
    )
    .unwrap();
    for (table, owner_column) in [
        ("item_effects", "item_id"),
        ("augment_effects", "augment_id"),
        ("set_bonus_tier_effects", "tier_id"),
        ("feat_effects", "feat_id"),
        ("socket_option_effects", "option_id"),
    ] {
        assert!(
            db.execute(
                &format!("INSERT INTO {table} ({owner_column}, effect_id, value2, sort_order) VALUES (1, 1000, 2, 0)"),
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
    db.execute(
        "INSERT INTO adventure_packs (id, name, slug) VALUES (1, 'Magic of Myth Drannor', 'magic-of-myth-drannor')",
        [],
    )
    .unwrap();
    let item_insert = |name: &str, wiki_url_sql: &str| {
        db.execute(
            &format!(
                "INSERT INTO items (slug, name, slot_id, item_category, wiki_url) VALUES ('{identity}', '{name}', (SELECT id FROM equipment_slots WHERE name = 'Ring'), 'Jewelry', {wiki_url_sql})", identity = ddo_model::identity::slug(name)
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
        db.execute("INSERT INTO quests (name, is_challenge, slug) VALUES ('Packless Quest', 0, 'packless-quest')", [])
            .is_err(),
        "a quest that is no challenge belongs to an adventure pack"
    );
    db.execute(
        "INSERT INTO quests (name, is_challenge, slug) VALUES ('Packless Challenge', 1, 'packless-challenge')",
        [],
    )
    .unwrap();
    db.execute("INSERT INTO quests (id, name, pack_id, slug) VALUES (10, 'Book Burning', 1, 'book-burning')", [])
        .unwrap();

    let source_insert = |chest: &str| {
        db.execute(
            &format!(
                "INSERT INTO quest_drops (owner_kind, quest_id, owner_id, loot_type, chest) VALUES ('item', 10, (SELECT MIN(id) FROM items), 'chest', '{chest}')"
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
fn a_socket_option_keeps_its_granted_socket_sets_and_bonuses_on_the_option() {
    let db = fresh_db();
    ddo_model::seeds::insert_all(&db).unwrap();
    db.execute_batch(
        "PRAGMA foreign_keys = ON;
         INSERT INTO items (id, name, slot_id, item_category, wiki_url, slug) VALUES (1, 'Sireth', 1, 'Weapon', 'https://ddowiki.com/page/Item:Sireth', 'sireth');
         INSERT INTO augment_slot_types (id, label, family, variant, slug) VALUES (1, 'crafting: attuned to heroism 4', 'crafting', 'attuned to heroism 4', 'crafting-attuned-to-heroism-4'), (2, 'red', 'standard', 'red', 'red');
         INSERT INTO socket_option_sets (id,slot_id,name,slug) VALUES (1,1,'Sireth','crafting-attuned-to-heroism-4-sireth');
         INSERT INTO item_augment_slots (item_id, sort_order, slot_id, option_set_id) VALUES (1, 0, 1, 1);
         INSERT INTO set_bonuses (id, name, slug) VALUES (1, 'Prowess / Planar Conflux Set Bonus', 'prowess-planar-conflux-set-bonus');",
    )
    .unwrap();
    db.execute(
        "INSERT INTO socket_options (set_id, option_order, name, icon) VALUES (1, 0, 'Red Augment Slot', 'Heroism')",
        [],
    )
    .unwrap();
    let option_id = db.last_insert_rowid();
    db.execute("INSERT INTO socket_option_grants (option_id, sort_order, slot_id) VALUES (?1, 0, 2)", [option_id])
        .unwrap();
    db.execute("INSERT INTO socket_option_set_bonuses (option_id, set_bonus_id) VALUES (?1, 1)", [option_id]).unwrap();
    db.execute(
        "INSERT INTO socket_option_effects (option_id, effect_id, bonus_type_id, value, sort_order) VALUES (?1, 1, 1, 8, 0)",
        [option_id],
    )
    .unwrap();
    db.execute(
        "INSERT INTO modifiers (source_kind, source_id, sort_order, effect_type) VALUES ('socket_option', ?1, 0, 'AbilityBonus')",
        [option_id],
    )
    .unwrap();
    assert!(db
        .execute("INSERT INTO socket_option_grants (option_id, sort_order, slot_id) VALUES (?1, 1, 99)", [option_id])
        .is_err());
    assert!(db.execute("INSERT INTO socket_options (set_id, option_order, name) VALUES (1, 0, 'Again')", []).is_err());
    let item_socket_count: i64 =
        db.query_row("SELECT COUNT(*) FROM item_augment_slots WHERE item_id = 1", [], |r| r.get(0)).unwrap();
    assert_eq!(item_socket_count, 1, "a granted socket stays on the option, never an item socket");
    db.execute("DELETE FROM items WHERE id = 1", []).unwrap();
    assert_eq!(db.query_row("SELECT COUNT(*) FROM socket_options", [], |row| row.get::<_, i64>(0)).unwrap(), 1);
    db.execute("DELETE FROM socket_option_sets WHERE id = 1", []).unwrap();
    for option_table in ["socket_option_grants", "socket_option_set_bonuses", "socket_option_effects"] {
        let row_count: i64 = db.query_row(&format!("SELECT COUNT(*) FROM {option_table}"), [], |r| r.get(0)).unwrap();
        assert_eq!(row_count, 0, "{option_table} rows go with their set");
    }
    let options_sql: String =
        db.query_row("SELECT sql FROM sqlite_master WHERE name = 'socket_options'", [], |r| r.get(0)).unwrap();
    assert!(!options_sql.contains("already applied"), "{options_sql}");
    const { assert!(SCHEMA_VERSION >= 49, "socket option sets require schema 49 or newer") };
}
