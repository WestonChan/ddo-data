use super::drop_text::{
    insert_source_link, insert_source_link_unless_linked_as, insert_source_link_unless_loot_linked_there,
    mark_quest_source_link_rare, DropTextLinker, DroppedLoot, SourceLink,
};
use super::effects::EffectOwner;
use super::items::{ArmorStatsRow, ItemRow, WeaponStatsRow};
use super::{BuildReport, ProbableDuplicateWikiEntry, SupersededWikiEntry, TableWriter};
use crate::map::drop_location::drop_text_in_description;
use crate::wiki::{
    CraftingRecipe, CraftingSystem, DescriptionKind, ListedDrop, WikiAugment, WikiBonus, WikiDescription, WikiItem,
    WikiItemEffect, WikiOverrides, WikiQuest,
};
use anyhow::{bail, Context, Result};
use ddo_model::enums::{LootType, Provenance};
use rusqlite::{params, OptionalExtension, Transaction};
use std::collections::{HashMap, HashSet};

pub(super) fn apply_wiki_overrides(
    transaction: &Transaction,
    wiki_overrides: &WikiOverrides,
    drop_text_linker: &DropTextLinker,
    report: &mut BuildReport,
) -> Result<()> {
    for wiki_description in &wiki_overrides.descriptions {
        fill_blank_descriptions(transaction, wiki_description, drop_text_linker, report).with_context(|| {
            format!(
                "wiki descriptions {} {:?} ({})",
                wiki_description.kind.as_str(),
                wiki_description.name,
                wiki_description.page
            )
        })?;
    }
    for quest_loot in &wiki_overrides.quest_loot {
        let citation = format!("wiki quest_loot {:?} ({})", quest_loot.name, quest_loot.page);
        let quest_id = id_by_name(transaction, "quests", &quest_loot.name)?.with_context(|| {
            format!(
                "{citation}: no quest has this name in Quests.xml or Challenges.xml; fix the name to match Maetrim's"
            )
        })?;
        for listed_item in &quest_loot.items {
            let item_name = listed_item.name();
            let item_id = id_by_name(transaction, "items", item_name)?.with_context(|| {
                format!("{citation}: listed item {item_name:?} is not in Maetrim's items or a wiki item; names must match exactly")
            })?;
            report.wiki_added_quest_loot_link_count +=
                insert_listed_loot_link(transaction, quest_id, DroppedLoot::Item(item_id), listed_item)?;
            report.wiki_loot_drop_count += 1;
        }
        for listed_augment in &quest_loot.augments {
            let augment_name = listed_augment.name();
            let augment_ids = ids_by_name(transaction, "augments", augment_name)?;
            if augment_ids.is_empty() {
                bail!("{citation}: listed augment {augment_name:?} is not in Maetrim's augments or a wiki augment; names must match exactly");
            }
            for augment_id in augment_ids {
                report.wiki_added_quest_augment_loot_link_count +=
                    insert_listed_loot_link(transaction, quest_id, DroppedLoot::Augment(augment_id), listed_augment)?;
            }
            report.wiki_loot_augment_drop_count += 1;
        }
        for rare_item in &quest_loot.rare {
            let item_name = rare_item.name();
            let item_id = id_by_name(transaction, "items", item_name)?.with_context(|| {
                format!("{citation}: rare item {item_name:?} is not in Maetrim's items; report it upstream rather than adding it here")
            })?;
            report.wiki_added_quest_loot_link_count +=
                mark_rare_loot(transaction, quest_id, DroppedLoot::Item(item_id), rare_item.chest())?;
            report.wiki_rare_drop_count += 1;
        }
        for rare_augment in &quest_loot.rare_augments {
            let augment_name = rare_augment.name();
            let augment_ids = ids_by_name(transaction, "augments", augment_name)?;
            if augment_ids.is_empty() {
                bail!("{citation}: rare augment {augment_name:?} is not in Maetrim's augments; names must match his exactly");
            }
            for augment_id in augment_ids {
                report.wiki_added_quest_augment_loot_link_count +=
                    mark_rare_loot(transaction, quest_id, DroppedLoot::Augment(augment_id), rare_augment.chest())?;
            }
            report.wiki_rare_augment_drop_count += 1;
        }
        report.wiki_quest_loot_entry_count += 1;
    }
    for wiki_quest in &wiki_overrides.quests {
        let citation = format!("wiki quests {:?} ({})", wiki_quest.name, wiki_quest.page);
        let quest_id = id_by_name(transaction, "quests", &wiki_quest.name)?.with_context(|| {
            format!(
                "{citation}: no quest has this name in Quests.xml or Challenges.xml; fix the name to match Maetrim's, or give the quest fields that create it"
            )
        })?;
        transaction.execute(
            "UPDATE quests SET is_free_to_play = ?2, legendary_level = ?3, zone = ?4, bestowed_by = ?5, flagging = ?6
              WHERE id = ?1",
            params![
                quest_id,
                wiki_quest.free_to_play,
                wiki_quest.legendary_level,
                wiki_quest.zone,
                wiki_quest.bestowed_by,
                wiki_quest.flagging
            ],
        )?;
        report.wiki_quest_entry_count += 1;
    }
    for crafting_system in &wiki_overrides.crafting_systems {
        insert_crafting_system_contents(transaction, crafting_system, report)
            .with_context(|| format!("wiki crafting {:?} ({})", crafting_system.name, crafting_system.page))?;
    }
    Ok(())
}

pub(super) fn write_wiki_quests(
    transaction: &Transaction,
    wiki_quests: &[WikiQuest],
    report: &mut BuildReport,
) -> Result<()> {
    let maetrim_quest_names_and_epic_names: Vec<(String, Option<String>)> = {
        let mut statement = transaction.prepare("SELECT name, epic_name FROM quests ORDER BY name")?;
        let names = statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        names
    };
    let maetrim_quest_names: Vec<String> =
        maetrim_quest_names_and_epic_names.iter().map(|(name, _)| name.clone()).collect();
    let maetrim_quest_name_set: HashSet<&str> = maetrim_quest_names.iter().map(String::as_str).collect();
    let mut maetrim_quest_names_by_normalised_name = names_by_normalised_name(&maetrim_quest_names);
    for (name, epic_name) in &maetrim_quest_names_and_epic_names {
        if let Some(epic_name) = epic_name {
            maetrim_quest_names_by_normalised_name.entry(normalised_name(epic_name)).or_insert(name.as_str());
        }
    }
    for wiki_quest in wiki_quests.iter().filter(|wiki_quest| wiki_quest.carries_quest_fields()) {
        if maetrim_quest_name_set.contains(wiki_quest.name.as_str()) {
            report
                .superseded_wiki_quests
                .push(SupersededWikiEntry { name: wiki_quest.name.clone(), file_name: wiki_quest.file_name.clone() });
            report.wiki_quest_superseded_count += 1;
            continue;
        }
        insert_wiki_quest(transaction, wiki_quest).with_context(|| {
            format!("wiki {} quest {:?} ({})", wiki_quest.file_name, wiki_quest.name, wiki_quest.page)
        })?;
        report.wiki_quest_created_count += 1;
        if let Some(maetrim_name) = maetrim_quest_names_by_normalised_name.get(&normalised_name(&wiki_quest.name)) {
            report.probable_duplicate_wiki_quests.push(ProbableDuplicateWikiEntry {
                name: wiki_quest.name.clone(),
                maetrim_name: (*maetrim_name).to_string(),
            });
            report.wiki_quest_probable_duplicate_count += 1;
        }
    }
    Ok(())
}

fn insert_wiki_quest(transaction: &Transaction, wiki_quest: &WikiQuest) -> Result<()> {
    let pack_name = wiki_quest.pack.as_deref().expect("validated pack");
    let pack_id = id_by_name(transaction, "adventure_packs", pack_name)?.with_context(|| {
        format!(
            "pack {pack_name:?} is not an adventure pack in Maetrim's Quests.xml or Challenges.xml; use his spelling"
        )
    })?;
    let patron_id = match &wiki_quest.patron {
        Some(patron_name) => Some(id_by_name(transaction, "patrons", patron_name)?.with_context(|| {
            format!("patron {patron_name:?} is not a patron in Maetrim's Patrons.xml; use his spelling")
        })?),
        None => None,
    };
    transaction.execute(
        "INSERT INTO quests (name, pack_id, patron_id, level, epic_level, favor, is_raid, difficulties, provenance)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            wiki_quest.name,
            pack_id,
            patron_id,
            wiki_quest.level,
            wiki_quest.epic_level,
            wiki_quest.favor,
            wiki_quest.is_raid.unwrap_or_default(),
            serde_json::to_string(wiki_quest.difficulties.as_deref().unwrap_or_default())?,
            Provenance::Wiki.as_str(),
        ],
    )?;
    Ok(())
}

fn fill_blank_descriptions(
    transaction: &Transaction,
    wiki_description: &WikiDescription,
    drop_text_linker: &DropTextLinker,
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
                if wiki_description.kind == DescriptionKind::Augment {
                    report.wiki_description_augment_link_count +=
                        link_augment_to_quests_named_in(transaction, drop_text_linker, row_id, &filled_description)?;
                }
            }
            None => report.wiki_description_skipped_count += 1,
        }
    }
    report.wiki_description_entry_count += 1;
    Ok(())
}

fn link_augment_to_quests_named_in(
    transaction: &Transaction,
    drop_text_linker: &DropTextLinker,
    augment_id: i64,
    description: &str,
) -> Result<usize> {
    let Some(drop_text) = drop_text_in_description(description) else {
        return Ok(0);
    };
    Ok(drop_text_linker.link_loot_to_quests_named_in(transaction, DroppedLoot::Augment(augment_id), drop_text)?.len())
}

pub(super) fn write_wiki_crafting_systems(transaction: &Transaction, wiki_overrides: &WikiOverrides) -> Result<()> {
    for crafting_system in &wiki_overrides.crafting_systems {
        transaction.execute(
            "INSERT INTO crafting_systems (name, page, npc) VALUES (?1, ?2, ?3)",
            params![crafting_system.name, crafting_system.page, crafting_system.npc],
        )?;
    }
    Ok(())
}

fn insert_crafting_system_contents(
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
    let system_id = id_by_name(transaction, "crafting_systems", &crafting_system.name)?
        .context("the crafting system row is written before his items")?;
    transaction.execute("UPDATE crafting_systems SET pack_id = ?2 WHERE id = ?1", params![system_id, pack_id])?;
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

fn mark_rare_loot(transaction: &Transaction, quest_id: i64, loot: DroppedLoot, chest: Option<&str>) -> Result<usize> {
    let added_link_count = insert_source_link_unless_loot_linked_there(
        transaction,
        &SourceLink::from_quest(quest_id, loot, LootType::Chest),
    )?;
    mark_quest_source_link_rare(transaction, quest_id, loot, chest)?;
    Ok(added_link_count)
}

fn insert_listed_loot_link(
    transaction: &Transaction,
    quest_id: i64,
    loot: DroppedLoot,
    listed_drop: &ListedDrop,
) -> Result<usize> {
    let listed_source_link =
        SourceLink { chest: listed_drop.chest(), ..SourceLink::from_quest(quest_id, loot, listed_drop.loot_type()) };
    if listed_drop.names_loot_type() {
        insert_source_link_unless_linked_as(transaction, &listed_source_link)
    } else {
        insert_source_link_unless_loot_linked_there(transaction, &listed_source_link)
    }
}

fn ids_by_name(transaction: &Transaction, table: &str, name: &str) -> Result<Vec<i64>> {
    let mut statement = transaction.prepare(&format!("SELECT id FROM {table} WHERE name = ?1 ORDER BY id"))?;
    let ids = statement.query_map(params![name], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    Ok(ids)
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
        let maetrim_item_names_by_normalised_name = names_by_normalised_name(&maetrim_item_names);
        let maetrim_item_names_by_unprefixed_name: HashMap<String, &str> =
            maetrim_item_names.iter().map(|name| (unprefixed_item_name(name), name.as_str())).collect();
        let mut effect_ids_by_folded_name: HashMap<String, (i64, i64)> = HashMap::new();
        let mut text_families: Vec<_> =
            self.effects.families().filter(|family| !self.effects.has_stats(family.id)).collect();
        text_families.sort_by_key(|family| family.id);
        for family in text_families {
            effect_ids_by_folded_name
                .entry(folded_effect_name(&family.name))
                .or_insert((family.id, family.amount_count));
        }
        for wiki_item in wiki_items {
            if maetrim_item_name_set.contains(wiki_item.name.as_str()) {
                report
                    .superseded_wiki_items
                    .push(SupersededWikiEntry { name: wiki_item.name.clone(), file_name: wiki_item.file_name.clone() });
                report.wiki_item_superseded_count += 1;
                continue;
            }
            let item_id = self.write_wiki_item(wiki_item, &mut effect_ids_by_folded_name).with_context(|| {
                format!("wiki {} item {:?} ({})", wiki_item.file_name, wiki_item.name, wiki_item.page)
            })?;
            report.pack_loot_link_count +=
                self.link_to_drop_text_packs(DroppedLoot::Item(item_id), &wiki_item.drop_location)?;
            self.link_to_sources_named_in_drop_text(DroppedLoot::Item(item_id), &wiki_item.drop_location, report)?;
            report.wiki_item_written_count += 1;
            if let Some(maetrim_name) = maetrim_item_names_by_normalised_name
                .get(&normalised_name(&wiki_item.name))
                .or_else(|| maetrim_item_names_by_unprefixed_name.get(&unprefixed_item_name(&wiki_item.name)))
            {
                report.probable_duplicate_wiki_items.push(ProbableDuplicateWikiEntry {
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
        effect_ids_by_folded_name: &mut HashMap<String, (i64, i64)>,
    ) -> Result<i64> {
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
                let quest_id = id_by_name(self.transaction, "quests", &quest.name)?.with_context(|| {
                    format!(
                        "quest {:?} is not a quest in Maetrim's Quests.xml or Challenges.xml or the wiki quests; use his spelling",
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
            provenance: Provenance::Wiki,
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
            let (effect_id, link_bonus_type) = self.ensure_wiki_effect(bonus)?;
            self.effects.insert_link(
                EffectOwner::Item,
                item_id,
                effect_id,
                link_bonus_type,
                (Some(bonus.value), bonus.value2),
                sort_order,
            )?;
        }
        for (sort_order, effect) in wiki_item.effects.iter().enumerate() {
            let (effect_id, count) = self.wiki_effect_id(effect, effect_ids_by_folded_name)?;
            let bonus_type = self.effects.family(effect_id).and_then(|family| {
                if family.is_stat {
                    Some(ddo_model::enums::BonusType::Equipment)
                } else if family.uses_link_type {
                    self.effects.unique_link_bonus_type(effect_id)
                } else {
                    None
                }
            });
            self.effects.insert_link(
                EffectOwner::Item,
                item_id,
                effect_id,
                bonus_type,
                (if count > 0 { effect.value } else { None }, None),
                wiki_item.bonuses.len() + sort_order,
            )?;
        }
        for (sort_order, slot_type_id) in slot_type_ids.into_iter().enumerate() {
            self.insert_item_augment_slot(item_id, sort_order, slot_type_id)?;
        }
        for (quest_id, loot_type) in quest_links {
            insert_source_link(
                self.transaction,
                &SourceLink::from_quest(quest_id, DroppedLoot::Item(item_id), loot_type),
            )?;
        }
        if let Some(set_name) = &wiki_item.set {
            self.pending_set_item_links.push((item_id, set_name.clone()));
        }
        Ok(item_id)
    }

    fn ensure_wiki_effect(&mut self, bonus: &WikiBonus) -> Result<(i64, Option<ddo_model::enums::BonusType>)> {
        let stat = bonus.stat();
        let bonus_type = bonus.bonus_type();
        let count = if bonus.value2.is_some() { 2 } else { 1 };
        let family_name = stat.name.to_string();
        if self.effects.family_named(&family_name).is_some_and(|family| family.text_template.is_empty()) {
            let text_template = if count == 2 {
                format!("%b1 {family_name} +{{1}} {{2}}")
            } else {
                format!("%b1 {family_name} +{{1}}")
            };
            self.effects.ensure_family(&family_name, &text_template, None, count)?;
        }
        let (effect_id, uses_link_type) = match self.effects.family_named(&family_name) {
            Some(family) if family.amount_count >= count => (family.id, family.uses_link_type),
            Some(family) => anyhow::bail!(
                "wiki bonus {family_name:?} carries {count} amounts but its shared family allows {}",
                family.amount_count
            ),
            None => {
                let text_template = if count == 2 {
                    format!("%b1 {family_name} +{{1}} {{2}}")
                } else {
                    format!("%b1 {family_name} +{{1}}")
                };
                (self.effects.ensure_family(&family_name, &text_template, None, count)?, true)
            }
        };
        self.effects.ensure_stat(effect_id, stat, (!uses_link_type).then_some(bonus_type), 1, None, 0)?;
        Ok((
            effect_id,
            (uses_link_type || self.effects.family(effect_id).is_some_and(|family| family.is_stat))
                .then_some(bonus_type),
        ))
    }

    pub(super) fn write_wiki_augments(
        &mut self,
        wiki_augments: &[WikiAugment],
        report: &mut BuildReport,
    ) -> Result<()> {
        let maetrim_augment_names_by_family: HashMap<String, Vec<String>> = {
            let mut statement = self
                .transaction
                .prepare("SELECT family, name FROM augments WHERE provenance = ?1 ORDER BY family, name")?;
            let mut names_by_family: HashMap<String, Vec<String>> = HashMap::new();
            for family_and_name in statement
                .query_map([Provenance::Maetrim.as_str()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            {
                let (family, name) = family_and_name?;
                names_by_family.entry(family).or_default().push(name);
            }
            names_by_family
        };
        for wiki_augment in wiki_augments {
            let citation =
                format!("wiki {} augment {:?} ({})", wiki_augment.file_name, wiki_augment.name, wiki_augment.page);
            let maetrim_family_names = maetrim_augment_names_by_family.get(&wiki_augment.family).with_context(|| {
                format!(
                    "{citation}: family {:?} has no augments in Maetrim's Augments/; a family is an augment file's name before .Augments.xml",
                    wiki_augment.family
                )
            })?;
            if maetrim_family_names.contains(&wiki_augment.name) {
                report.superseded_wiki_augments.push(SupersededWikiEntry {
                    name: wiki_augment.name.clone(),
                    file_name: wiki_augment.file_name.clone(),
                });
                report.wiki_augment_superseded_count += 1;
                continue;
            }
            self.write_wiki_augment(wiki_augment, report).with_context(|| citation.clone())?;
            report.wiki_augment_written_count += 1;
            if let Some(maetrim_name) =
                names_by_normalised_name(maetrim_family_names).get(&normalised_name(&wiki_augment.name))
            {
                report.probable_duplicate_wiki_augments.push(ProbableDuplicateWikiEntry {
                    name: wiki_augment.name.clone(),
                    maetrim_name: (*maetrim_name).to_string(),
                });
                report.wiki_augment_probable_duplicate_count += 1;
            }
        }
        Ok(())
    }

    fn write_wiki_augment(&mut self, wiki_augment: &WikiAugment, report: &mut BuildReport) -> Result<()> {
        if let Some(set_name) = &wiki_augment.set {
            if !self.written.set_bonus_ids_by_name.contains_key(set_name) {
                bail!("set {set_name:?} is not a set in Maetrim's SetBonuses.xml or FiligreeSets/; use his spelling");
            }
        }
        let slot_type_ids = wiki_augment
            .slots
            .iter()
            .map(|label| {
                self.written.augment_slot_type_ids_by_label.get(label).copied().with_context(|| {
                    format!("slots label {label:?} is not a socket label in Maetrim's files; use one /v1/augment-slot-types lists")
                })
            })
            .collect::<Result<Vec<i64>>>()?;
        self.transaction.execute(
            "INSERT INTO augments (name, family, description, effect_description, min_level, set_bonus, provenance)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                wiki_augment.name,
                wiki_augment.family,
                wiki_augment.description,
                wiki_augment.effect_description,
                wiki_augment.min_level,
                wiki_augment.set,
                Provenance::Wiki.as_str(),
            ],
        )?;
        let augment_id = self.transaction.last_insert_rowid();
        for slot_type_id in slot_type_ids {
            self.transaction.execute(
                "INSERT OR IGNORE INTO augment_slots (augment_id, slot_id) VALUES (?1, ?2)",
                params![augment_id, slot_type_id],
            )?;
        }
        for (sort_order, bonus) in wiki_augment.bonuses.iter().enumerate() {
            let (effect_id, link_bonus_type) = self.ensure_wiki_effect(bonus)?;
            self.effects.insert_link(
                EffectOwner::Augment,
                augment_id,
                effect_id,
                link_bonus_type,
                (Some(bonus.value), bonus.value2),
                sort_order,
            )?;
        }
        if let Some(set_name) = &wiki_augment.set {
            self.pending_set_augment_links.push((augment_id, set_name.clone()));
        }
        self.link_augment_to_quests(augment_id, &wiki_augment.description, report)
    }

    fn wiki_effect_id(
        &mut self,
        effect: &WikiItemEffect,
        effect_ids_by_folded_name: &mut HashMap<String, (i64, i64)>,
    ) -> Result<(i64, i64)> {
        let folded_name = folded_effect_name(&effect.name);
        if let Some(effect_id) = effect_ids_by_folded_name.get(&folded_name) {
            return Ok(*effect_id);
        }
        let matching_stat_family = self.effects.family_named(&effect.name).and_then(|family| {
            (!family.uses_link_type
                && self.effects.has_stats(family.id)
                && ((family.amount_count == 0
                    && family.description_template.as_deref() == effect.description.as_deref())
                    || (family.amount_count > 0 && effect.value.is_some())))
            .then_some((family.id, family.amount_count))
        });
        let effect_id = match matching_stat_family {
            Some(identity) => identity,
            None => (self.effects.ensure_wiki_text(&effect.name, effect.description.as_deref())?, 0),
        };
        effect_ids_by_folded_name.insert(folded_name, effect_id);
        Ok(effect_id)
    }
}

pub(super) fn folded_effect_name(effect_name: &str) -> String {
    effect_name
        .to_lowercase()
        .chars()
        .filter(|character| !matches!(character, ' ' | '-' | ':' | ',' | '.' | '\''))
        .collect()
}

fn names_by_normalised_name(names: &[String]) -> HashMap<String, &str> {
    let mut names_by_normalised_name = HashMap::new();
    for name in names {
        names_by_normalised_name.entry(normalised_name(name)).or_insert(name.as_str());
    }
    names_by_normalised_name
}

fn normalised_name(name: &str) -> String {
    let lowercase_name = name.trim().to_lowercase();
    let name_without_level = lowercase_name
        .strip_suffix(')')
        .and_then(|before_paren| before_paren.rsplit_once("(level "))
        .filter(|(_, level)| !level.is_empty() && level.bytes().all(|b| b.is_ascii_digit()))
        .map_or(lowercase_name.as_str(), |(before_level, _)| before_level);
    name_without_level.chars().filter(|character| character.is_alphanumeric()).collect()
}

fn unprefixed_item_name(name: &str) -> String {
    let lowercase = name.trim().to_lowercase();
    let unprefixed = ["epic ", "legendary ", "heroic "]
        .into_iter()
        .find_map(|prefix| lowercase.strip_prefix(prefix))
        .unwrap_or(&lowercase);
    unprefixed.chars().filter(|character| character.is_alphanumeric()).collect()
}

#[cfg(test)]
mod tests {
    use super::unprefixed_item_name;

    #[test]
    fn rarity_prefix_and_level_case_do_not_hide_a_duplicate() {
        assert_eq!(
            unprefixed_item_name("Epic Docent of Shadow (Level 28)"),
            unprefixed_item_name("Docent of Shadow (level 28)")
        );
        assert_ne!(
            unprefixed_item_name("Legendary Docent of Shadow (Level 32)"),
            unprefixed_item_name("Docent of Shadow (level 28)")
        );
    }
}
