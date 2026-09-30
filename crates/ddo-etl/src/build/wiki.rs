use super::BuildReport;
use crate::wiki::{CraftingRecipe, CraftingSystem, WikiDescription, WikiOverrides};
use anyhow::{bail, Context, Result};
use ddo_model::enums::LootType;
use rusqlite::{params, OptionalExtension, Transaction};
use std::collections::HashMap;

pub(super) fn apply_wiki_overrides(
    transaction: &Transaction,
    wiki_overrides: &WikiOverrides,
    report: &mut BuildReport,
) -> Result<()> {
    for quest_loot in &wiki_overrides.quest_loot {
        let citation = format!("wiki quest_loot {:?} ({})", quest_loot.name, quest_loot.page);
        let quest_id = id_by_name(transaction, "quests", &quest_loot.name)?.with_context(|| {
            format!(
                "{citation}: no quest has this name in Quests.xml or Challenges.xml; fix the name to match Maetrim's"
            )
        })?;
        for item_name in &quest_loot.rare {
            let item_id = id_by_name(transaction, "items", item_name)?.with_context(|| {
                format!("{citation}: rare item {item_name:?} is not in Maetrim's items; report it upstream rather than adding it here")
            })?;
            report.wiki_added_quest_loot_link_count += transaction.execute(
                "INSERT OR IGNORE INTO quest_loot (quest_id, item_id, loot_type) VALUES (?1, ?2, ?3)",
                params![quest_id, item_id, LootType::Chest.as_str()],
            )?;
            transaction.execute(
                "UPDATE quest_loot SET is_rare = 1 WHERE quest_id = ?1 AND item_id = ?2",
                params![quest_id, item_id],
            )?;
            report.wiki_rare_drop_count += 1;
        }
        report.wiki_quest_loot_entry_count += 1;
    }
    for quest_facts in &wiki_overrides.quest_facts {
        let citation = format!("wiki quests {:?} ({})", quest_facts.name, quest_facts.page);
        let quest_id = id_by_name(transaction, "quests", &quest_facts.name)?.with_context(|| {
            format!(
                "{citation}: no quest has this name in Quests.xml or Challenges.xml; fix the name to match Maetrim's"
            )
        })?;
        transaction.execute(
            "UPDATE quests SET is_free_to_play = ?2, legendary_level = ?3, zone = ?4, bestowed_by = ?5, flagging = ?6
              WHERE id = ?1",
            params![
                quest_id,
                quest_facts.free_to_play,
                quest_facts.legendary_level,
                quest_facts.zone,
                quest_facts.bestowed_by,
                quest_facts.flagging
            ],
        )?;
        report.wiki_quest_entry_count += 1;
    }
    for crafting_system in &wiki_overrides.crafting_systems {
        insert_crafting_system(transaction, crafting_system, report)
            .with_context(|| format!("wiki crafting {:?} ({})", crafting_system.name, crafting_system.page))?;
    }
    for wiki_description in &wiki_overrides.descriptions {
        fill_blank_descriptions(transaction, wiki_description, report).with_context(|| {
            format!(
                "wiki descriptions {} {:?} ({})",
                wiki_description.kind.as_str(),
                wiki_description.name,
                wiki_description.page
            )
        })?;
    }
    Ok(())
}

fn fill_blank_descriptions(
    transaction: &Transaction,
    wiki_description: &WikiDescription,
    report: &mut BuildReport,
) -> Result<()> {
    let table_name = wiki_description.kind.table_name();
    let mut statement = transaction.prepare(&format!("SELECT id, description FROM {table_name} WHERE name = ?1"))?;
    let maetrim_descriptions_by_id = statement
        .query_map(params![wiki_description.name], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if maetrim_descriptions_by_id.is_empty() {
        bail!(
            "no {} in Maetrim's files has this name (matched against {table_name}.name); fix the name to match his",
            wiki_description.kind.as_str()
        );
    }
    for (row_id, maetrim_description) in maetrim_descriptions_by_id {
        match wiki_description.kind.filled_description(maetrim_description.as_deref(), &wiki_description.description) {
            Some(filled_description) => {
                transaction.execute(
                    &format!("UPDATE {table_name} SET description = ?2 WHERE id = ?1"),
                    params![row_id, filled_description],
                )?;
                report.wiki_description_filled_count += 1;
            }
            None => report.wiki_description_skipped_count += 1,
        }
    }
    report.wiki_description_entry_count += 1;
    Ok(())
}

fn insert_crafting_system(
    transaction: &Transaction,
    crafting_system: &CraftingSystem,
    report: &mut BuildReport,
) -> Result<()> {
    let pack_id = match &crafting_system.pack {
        Some(pack_name) => Some(id_by_name(transaction, "adventure_packs", pack_name)?.with_context(|| {
            format!(
                "pack {pack_name:?} is not an adventure pack in Maetrim's Quests.xml or Challenges.xml; use his spelling"
            )
        })?),
        None => None,
    };
    for family in &crafting_system.families {
        let family_has_augments: bool = transaction.query_row(
            "SELECT EXISTS (SELECT 1 FROM augments WHERE family = ?1)",
            params![family],
            |r| r.get(0),
        )?;
        if !family_has_augments {
            bail!("family {family:?} has no augments in Maetrim's Augments/; a family is an augment file's name before .Augments.xml");
        }
    }
    transaction.execute(
        "INSERT INTO crafting_systems (name, page, pack_id, npc) VALUES (?1, ?2, ?3, ?4)",
        params![crafting_system.name, crafting_system.page, pack_id, crafting_system.npc],
    )?;
    let system_id = transaction.last_insert_rowid();
    for family in &crafting_system.families {
        transaction.execute(
            "INSERT OR IGNORE INTO crafting_system_families (system_id, family) VALUES (?1, ?2)",
            params![system_id, family],
        )?;
    }
    let mut ingredient_ids_by_name_and_tier = HashMap::new();
    for ingredient in &crafting_system.ingredients {
        transaction.execute(
            "INSERT INTO crafting_ingredients (system_id, name, tier, bind, source) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![system_id, ingredient.name, ingredient.tier, ingredient.bind, ingredient.source],
        )?;
        ingredient_ids_by_name_and_tier
            .insert((ingredient.name.as_str(), ingredient.tier.as_str()), transaction.last_insert_rowid());
        report.wiki_crafting_ingredient_count += 1;
    }
    for (sort_order, recipe) in crafting_system.recipes.iter().enumerate() {
        insert_crafting_recipe(
            transaction,
            crafting_system,
            system_id,
            sort_order,
            recipe,
            &ingredient_ids_by_name_and_tier,
        )
        .with_context(|| format!("recipe {:?}", recipe.option))?;
        report.wiki_crafting_recipe_count += 1;
    }
    report.wiki_crafting_system_count += 1;
    Ok(())
}

fn insert_crafting_recipe(
    transaction: &Transaction,
    crafting_system: &CraftingSystem,
    system_id: i64,
    sort_order: usize,
    recipe: &CraftingRecipe,
    ingredient_ids_by_name_and_tier: &HashMap<(&str, &str), i64>,
) -> Result<()> {
    let slot_type_id =
        recipe.slot.as_deref().map(|label| slot_type_id_labelled(transaction, "slot", label)).transpose()?;
    let granted_slot_type_id = recipe
        .grants_slot
        .as_deref()
        .map(|label| slot_type_id_labelled(transaction, "grants_slot", label))
        .transpose()?;
    transaction.execute(
        "INSERT INTO crafting_recipes (system_id, tier, slot_id, grants_slot_id, option, note, sort_order)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            system_id,
            recipe.tier,
            slot_type_id,
            granted_slot_type_id,
            recipe.option,
            recipe.note,
            sort_order as i64
        ],
    )?;
    let recipe_id = transaction.last_insert_rowid();
    for augment_name in &recipe.augments {
        let augment_ids = augment_ids_named_in(transaction, augment_name, &crafting_system.families)?;
        if augment_ids.is_empty() {
            bail!(
                "augment {augment_name:?} is in none of the families {}; names must match Maetrim's exactly",
                crafting_system.families.join(", ")
            );
        }
        for augment_id in augment_ids {
            transaction.execute(
                "INSERT OR IGNORE INTO crafting_recipe_augments (recipe_id, augment_id) VALUES (?1, ?2)",
                params![recipe_id, augment_id],
            )?;
        }
    }
    for cost in &recipe.cost {
        let ingredient = crafting_system.ingredient_for(recipe, &cost.ingredient)?;
        transaction.execute(
            "INSERT INTO crafting_recipe_ingredients (recipe_id, ingredient_id, quantity) VALUES (?1, ?2, ?3)",
            params![
                recipe_id,
                ingredient_ids_by_name_and_tier[&(ingredient.name.as_str(), ingredient.tier.as_str())],
                cost.quantity
            ],
        )?;
    }
    Ok(())
}

fn slot_type_id_labelled(transaction: &Transaction, field: &str, label: &str) -> Result<i64> {
    transaction
        .query_row("SELECT id FROM augment_slot_types WHERE label = ?1", params![label], |r| r.get(0))
        .optional()?
        .with_context(|| {
            format!("{field} {label:?} is not a socket label in Maetrim's files; use one /v1/augment-slot-types lists")
        })
}

fn augment_ids_named_in(transaction: &Transaction, augment_name: &str, families: &[String]) -> Result<Vec<i64>> {
    let mut statement =
        transaction.prepare_cached("SELECT id FROM augments WHERE name = ?1 AND family = ?2 ORDER BY id")?;
    let mut augment_ids = Vec::new();
    for family in families {
        for id in statement.query_map(params![augment_name, family], |r| r.get(0))? {
            augment_ids.push(id?);
        }
    }
    Ok(augment_ids)
}

fn id_by_name(transaction: &Transaction, table: &str, name: &str) -> Result<Option<i64>> {
    Ok(transaction
        .query_row(&format!("SELECT id FROM {table} WHERE name = ?1"), params![name], |r| r.get(0))
        .optional()?)
}
