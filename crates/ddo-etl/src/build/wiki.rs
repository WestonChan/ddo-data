use super::BuildReport;
use crate::wiki::{CraftingSystem, Recipe, WikiOverrides};
use anyhow::{bail, Context, Result};
use ddo_model::enums::LootType;
use rusqlite::{params, OptionalExtension, Transaction};
use std::collections::HashMap;

pub(super) fn apply_wiki(tx: &Transaction, wiki: &WikiOverrides, report: &mut BuildReport) -> Result<()> {
    for entry in &wiki.quest_loot {
        let cited = format!("wiki quest_loot {:?} ({})", entry.name, entry.page);
        let quest_id = id_by_name(tx, "quests", &entry.name)?.with_context(|| {
            format!("{cited}: no quest has this name in Quests.xml or Challenges.xml; fix the name to match Maetrim's")
        })?;
        for item in &entry.rare {
            let item_id = id_by_name(tx, "items", item)?.with_context(|| {
                format!("{cited}: rare item {item:?} is not in Maetrim's items; report it upstream rather than adding it here")
            })?;
            report.wiki_quest_loot_links_added += tx.execute(
                "INSERT OR IGNORE INTO quest_loot (quest_id, item_id, loot_type) VALUES (?1, ?2, ?3)",
                params![quest_id, item_id, LootType::Chest.as_str()],
            )?;
            tx.execute(
                "UPDATE quest_loot SET is_rare = 1 WHERE quest_id = ?1 AND item_id = ?2",
                params![quest_id, item_id],
            )?;
            report.wiki_rare_drops += 1;
        }
        report.wiki_quest_loot_entries += 1;
    }
    for entry in &wiki.quests {
        let cited = format!("wiki quests {:?} ({})", entry.name, entry.page);
        let quest_id = id_by_name(tx, "quests", &entry.name)?.with_context(|| {
            format!("{cited}: no quest has this name in Quests.xml or Challenges.xml; fix the name to match Maetrim's")
        })?;
        tx.execute(
            "UPDATE quests SET duration = ?2, is_free_to_play = ?3, legendary_level = ?4, zone = ?5, bestowed_by = ?6,
                    flagging = ?7
              WHERE id = ?1",
            params![
                quest_id,
                entry.duration().map(|d| d.as_str()),
                entry.free_to_play,
                entry.legendary_level,
                entry.zone,
                entry.bestowed_by,
                entry.flagging
            ],
        )?;
        for (tier, xp) in entry.xp.tiers() {
            report.wiki_quest_xp_rows += tx.execute(
                "INSERT INTO quest_xp (quest_id, tier, casual, normal, hard, elite) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![quest_id, tier.as_str(), xp.casual, xp.normal, xp.hard, xp.elite],
            )?;
        }
        report.wiki_quest_entries += 1;
    }
    for system in &wiki.crafting {
        apply_crafting_system(tx, system, report)
            .with_context(|| format!("wiki crafting {:?} ({})", system.name, system.page))?;
    }
    Ok(())
}

fn apply_crafting_system(tx: &Transaction, system: &CraftingSystem, report: &mut BuildReport) -> Result<()> {
    let pack_id = match &system.pack {
        Some(pack) => Some(id_by_name(tx, "adventure_packs", pack)?.with_context(|| {
            format!(
                "pack {pack:?} is not an adventure pack in Maetrim's Quests.xml or Challenges.xml; use his spelling"
            )
        })?),
        None => None,
    };
    for family in &system.families {
        let has_augments: bool =
            tx.query_row("SELECT EXISTS (SELECT 1 FROM augments WHERE family = ?1)", params![family], |r| r.get(0))?;
        if !has_augments {
            bail!("family {family:?} has no augments in Maetrim's Augments/; a family is an augment file's name before .Augments.xml");
        }
    }
    tx.execute(
        "INSERT INTO crafting_systems (name, page, pack_id, npc) VALUES (?1, ?2, ?3, ?4)",
        params![system.name, system.page, pack_id, system.npc],
    )?;
    let system_id = tx.last_insert_rowid();
    for family in &system.families {
        tx.execute(
            "INSERT OR IGNORE INTO crafting_system_families (system_id, family) VALUES (?1, ?2)",
            params![system_id, family],
        )?;
    }
    let mut ingredient_ids = HashMap::new();
    for ingredient in &system.ingredients {
        tx.execute(
            "INSERT INTO crafting_ingredients (system_id, name, tier, bind, source) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![system_id, ingredient.name, ingredient.tier, ingredient.bind, ingredient.source],
        )?;
        ingredient_ids.insert((ingredient.name.as_str(), ingredient.tier.as_str()), tx.last_insert_rowid());
        report.wiki_crafting_ingredients += 1;
    }
    for (sort_order, recipe) in system.recipes.iter().enumerate() {
        apply_crafting_recipe(tx, system, system_id, sort_order, recipe, &ingredient_ids)
            .with_context(|| format!("recipe {:?}", recipe.option))?;
        report.wiki_crafting_recipes += 1;
    }
    report.wiki_crafting_systems += 1;
    Ok(())
}

fn apply_crafting_recipe(
    tx: &Transaction,
    system: &CraftingSystem,
    system_id: i64,
    sort_order: usize,
    recipe: &Recipe,
    ingredient_ids: &HashMap<(&str, &str), i64>,
) -> Result<()> {
    let slot_id = match &recipe.slot {
        Some(label) => Some(
            tx.query_row("SELECT id FROM augment_slot_types WHERE label = ?1", params![label], |r| r.get::<_, i64>(0))
                .optional()?
                .with_context(|| {
                    format!(
                        "slot {label:?} is not a socket label in Maetrim's files; use one /v1/augment-slot-types lists"
                    )
                })?,
        ),
        None => None,
    };
    tx.execute(
        "INSERT INTO crafting_recipes (system_id, tier, slot_id, option, note, sort_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![system_id, recipe.tier, slot_id, recipe.option, recipe.note, sort_order as i64],
    )?;
    let recipe_id = tx.last_insert_rowid();
    for name in &recipe.augments {
        let augment_ids = augments_named_in(tx, name, &system.families)?;
        if augment_ids.is_empty() {
            bail!(
                "augment {name:?} is in none of the families {}; names must match Maetrim's exactly",
                system.families.join(", ")
            );
        }
        for augment_id in augment_ids {
            tx.execute(
                "INSERT OR IGNORE INTO crafting_recipe_augments (recipe_id, augment_id) VALUES (?1, ?2)",
                params![recipe_id, augment_id],
            )?;
        }
    }
    for cost in &recipe.cost {
        let ingredient = system.ingredient_for(recipe, &cost.ingredient)?;
        tx.execute(
            "INSERT INTO crafting_recipe_ingredients (recipe_id, ingredient_id, quantity) VALUES (?1, ?2, ?3)",
            params![recipe_id, ingredient_ids[&(ingredient.name.as_str(), ingredient.tier.as_str())], cost.quantity],
        )?;
    }
    Ok(())
}

fn augments_named_in(tx: &Transaction, name: &str, families: &[String]) -> Result<Vec<i64>> {
    let mut stmt = tx.prepare_cached("SELECT id FROM augments WHERE name = ?1 AND family = ?2 ORDER BY id")?;
    let mut ids = Vec::new();
    for family in families {
        for id in stmt.query_map(params![name, family], |r| r.get(0))? {
            ids.push(id?);
        }
    }
    Ok(ids)
}

fn id_by_name(tx: &Transaction, table: &str, name: &str) -> Result<Option<i64>> {
    Ok(tx.query_row(&format!("SELECT id FROM {table} WHERE name = ?1"), params![name], |r| r.get(0)).optional()?)
}
