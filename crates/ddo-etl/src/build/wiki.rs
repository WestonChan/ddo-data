use super::items::{ArmorStatsRow, ItemRow, WeaponStatsRow};
use super::{BuildReport, ProbableDuplicateWikiItem, SupersededWikiItem, TableWriter};
use crate::wiki::{CraftingRecipe, CraftingSystem, WikiDescription, WikiItem, WikiItemEffect, WikiOverrides};
use anyhow::{bail, Context, Result};
use ddo_model::enums::{ItemSource, LootType};
use rusqlite::{params, OptionalExtension, Transaction};
use std::collections::{HashMap, HashSet};

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

impl TableWriter<'_> {
    pub(super) fn write_wiki_items(&mut self, wiki_items: &[WikiItem], report: &mut BuildReport) -> Result<()> {
        let maetrim_item_names: Vec<String> = {
            let mut statement = self.transaction.prepare("SELECT name FROM items ORDER BY name")?;
            let names = statement.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            names
        };
        let maetrim_item_name_set: HashSet<&str> = maetrim_item_names.iter().map(String::as_str).collect();
        let mut maetrim_item_names_by_normalised_name: HashMap<String, &str> = HashMap::new();
        for maetrim_item_name in &maetrim_item_names {
            maetrim_item_names_by_normalised_name
                .entry(normalised_item_name(maetrim_item_name))
                .or_insert(maetrim_item_name);
        }
        let mut effect_ids_by_folded_name: HashMap<String, i64> = HashMap::new();
        let mut effect_names_by_id: Vec<(&String, &i64)> = self.written.effect_ids_by_name.iter().collect();
        effect_names_by_id.sort_by_key(|(_, id)| **id);
        for (effect_name, effect_id) in effect_names_by_id {
            effect_ids_by_folded_name.entry(folded_effect_name(effect_name)).or_insert(*effect_id);
        }
        for wiki_item in wiki_items {
            if maetrim_item_name_set.contains(wiki_item.name.as_str()) {
                report
                    .superseded_wiki_items
                    .push(SupersededWikiItem { name: wiki_item.name.clone(), file_name: wiki_item.file_name.clone() });
                report.wiki_item_superseded_count += 1;
                continue;
            }
            self.write_wiki_item(wiki_item, &mut effect_ids_by_folded_name).with_context(|| {
                format!("wiki {} item {:?} ({})", wiki_item.file_name, wiki_item.name, wiki_item.page)
            })?;
            report.wiki_item_written_count += 1;
            if let Some(maetrim_name) =
                maetrim_item_names_by_normalised_name.get(&normalised_item_name(&wiki_item.name))
            {
                report.probable_duplicate_wiki_items.push(ProbableDuplicateWikiItem {
                    name: wiki_item.name.clone(),
                    maetrim_name: (*maetrim_name).to_string(),
                });
                report.wiki_item_probable_duplicate_count += 1;
            }
        }
        Ok(())
    }

    fn write_wiki_item(
        &mut self,
        wiki_item: &WikiItem,
        effect_ids_by_folded_name: &mut HashMap<String, i64>,
    ) -> Result<()> {
        let material_id = match &wiki_item.material {
            Some(material_name) => Some(*self.written.material_ids_by_name.get(material_name).with_context(|| {
                format!(
                    "material {material_name:?} is not a material of Maetrim's items; use one /v1/items/{{id}} shows"
                )
            })?),
            None => None,
        };
        if let Some(set_name) = &wiki_item.set {
            if !self.written.set_bonus_ids_by_name.contains_key(set_name) {
                bail!("set {set_name:?} is not a set in Maetrim's SetBonuses.xml or FiligreeSets/; use his spelling");
            }
        }
        let slot_type_ids = wiki_item
            .augment_slots
            .iter()
            .map(|label| {
                self.written.augment_slot_type_ids_by_label.get(label).copied().with_context(|| {
                    format!("augment_slots label {label:?} is not a socket label in Maetrim's files; use one /v1/augment-slot-types lists")
                })
            })
            .collect::<Result<Vec<i64>>>()?;
        let quest_links = wiki_item
            .quests
            .iter()
            .map(|quest| {
                let quest_id = self.written_quests.id_named(&quest.name).with_context(|| {
                    format!(
                        "quest {:?} is not a quest in Maetrim's Quests.xml or Challenges.xml; use his spelling",
                        quest.name
                    )
                })?;
                Ok((quest_id, quest.loot_type()))
            })
            .collect::<Result<Vec<(i64, LootType)>>>()?;

        let item_id = self.insert_item_row(&ItemRow {
            name: &wiki_item.name,
            equipment_slot: wiki_item.equipment_slot(),
            category: wiki_item.item_category(),
            item_type: wiki_item.item_type.as_deref(),
            minimum_level: Some(wiki_item.minimum_level),
            enhancement_bonus: wiki_item.enhancement_bonus,
            material_id,
            race_required: wiki_item.race_required.clone(),
            icon: None,
            description: wiki_item.description.as_deref(),
            drop_location: Some(&wiki_item.drop_location),
            set_bonus: wiki_item.set.as_deref(),
            accepts_sentience: wiki_item.accepts_sentience,
            is_minor_artifact: wiki_item.is_minor_artifact,
            wiki_url: wiki_item.page.clone(),
            source: ItemSource::Wiki,
        })?;
        if let (Some(weapon), Some(weapon_type)) = (&wiki_item.weapon, wiki_item.weapon_type()) {
            self.insert_weapon_stats(
                item_id,
                &WeaponStatsRow {
                    weapon_type_id: weapon_type.id,
                    base_dice_count: weapon.damage_dice_count,
                    base_dice_sides: weapon.damage_dice_sides,
                    base_dice_bonus: weapon.damage_dice_bonus,
                    damage_multiplier: weapon.damage_multiplier,
                    critical_threat_range: weapon.critical_threat_range,
                    critical_multiplier: weapon.critical_multiplier,
                    attack_modifier: None,
                    damage_modifier: None,
                    handedness: Some(weapon.handedness()),
                    enhancement_bonus: wiki_item.enhancement_bonus,
                    dr_bypasses: &weapon.dr_bypass,
                },
            )?;
        }
        if let (Some(armor), Some(armor_type)) = (&wiki_item.armor, wiki_item.armor_type()) {
            self.insert_armor_stats(
                item_id,
                &ArmorStatsRow {
                    armor_type,
                    armor_bonus: armor.armor_bonus,
                    max_dex_bonus: armor.max_dex_bonus,
                    arcane_spell_failure: armor.arcane_spell_failure,
                    armor_check_penalty: armor.armor_check_penalty,
                    shield_bonus: armor.shield_bonus,
                    damage_reduction: None,
                    mithral_body: None,
                    adamantine_body: None,
                },
            )?;
        }
        for (sort_order, bonus) in wiki_item.bonuses.iter().enumerate() {
            let bonus_id =
                self.ensure_bonus(bonus.stat(), Some(bonus.bonus_type()), Some(bonus.value), bonus.value2, None)?;
            self.insert_item_bonus(item_id, bonus_id, sort_order)?;
        }
        for (sort_order, effect) in wiki_item.effects.iter().enumerate() {
            let effect_id = self.wiki_effect_id(effect, effect_ids_by_folded_name)?;
            self.insert_item_effect(item_id, effect_id, sort_order, effect.value, effect.target.as_deref())?;
        }
        for (sort_order, slot_type_id) in slot_type_ids.into_iter().enumerate() {
            self.insert_item_augment_slot(item_id, sort_order, slot_type_id)?;
        }
        for (quest_id, loot_type) in quest_links {
            self.insert_quest_loot_link(quest_id, item_id, loot_type, false)?;
        }
        if let Some(set_name) = &wiki_item.set {
            self.pending_set_item_links.push((item_id, set_name.clone()));
        }
        Ok(())
    }

    fn wiki_effect_id(
        &mut self,
        effect: &WikiItemEffect,
        effect_ids_by_folded_name: &mut HashMap<String, i64>,
    ) -> Result<i64> {
        if let Some(effect_id) = self.written.effect_ids_by_name.get(&effect.name) {
            return Ok(*effect_id);
        }
        let folded_name = folded_effect_name(&effect.name);
        if let Some(effect_id) = effect_ids_by_folded_name.get(&folded_name) {
            return Ok(*effect_id);
        }
        let effect_id = self.ensure_effect(&effect.name, effect.description.as_deref())?;
        effect_ids_by_folded_name.insert(folded_name, effect_id);
        Ok(effect_id)
    }
}

fn folded_effect_name(effect_name: &str) -> String {
    effect_name.to_lowercase().chars().filter(|character| !matches!(character, ' ' | '-')).collect()
}

fn normalised_item_name(item_name: &str) -> String {
    let lowercase_name = item_name.trim().to_lowercase();
    let name_without_level = lowercase_name
        .strip_suffix(')')
        .and_then(|before_paren| before_paren.rsplit_once("(level "))
        .filter(|(_, level)| !level.is_empty() && level.bytes().all(|b| b.is_ascii_digit()))
        .map_or(lowercase_name.as_str(), |(before_level, _)| before_level);
    name_without_level.chars().filter(|character| character.is_alphanumeric()).collect()
}
