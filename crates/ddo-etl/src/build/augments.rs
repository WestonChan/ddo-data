use super::bonus_types::{BonusOwner, BonusOwnerKind};
use super::drop_text::DroppedLoot;
use super::enchantments::EnchantmentOwner;
use super::{joined_non_empty, json_number_array, trimmed_non_empty, BuildReport, TableWriter};
use crate::map::drop_location::drop_text_in_description;
use crate::xml::augments::parse_augments_file;
use anyhow::Result;
use ddo_model::enums::ModifierSource;
use rusqlite::params;
use std::path::Path;

impl TableWriter<'_> {
    pub(super) fn write_augments_file(&mut self, path: &Path, report: &mut BuildReport) -> Result<()> {
        let (family, augments) = parse_augments_file(path)?;
        for augment in &augments {
            self.transaction.execute(
                "INSERT INTO augments (name, family, description, effect_description, min_level, icon, choose_level, levels,
                                       level_values, level_values2, dual_values, enter_value, suppress_set_bonus, set_bonus,
                                       adds_augment, grants_augment, weapon_class)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                params![
                    augment.name,
                    family,
                    trimmed_non_empty(augment.description.as_deref()),
                    joined_non_empty(&augment.effect_descriptions, "\n"),
                    augment.minimum_level,
                    trimmed_non_empty(augment.icon.as_deref()),
                    augment.has_selectable_level,
                    json_number_array(&augment.levels),
                    json_number_array(&augment.level_values),
                    json_number_array(&augment.second_level_values),
                    augment.has_dual_values,
                    augment.has_enterable_value,
                    augment.suppresses_set_bonus,
                    joined_non_empty(&augment.set_bonus_names, "\n"),
                    joined_non_empty(&augment.added_augments, "\n"),
                    trimmed_non_empty(augment.granted_augment.as_deref()),
                    trimmed_non_empty(augment.weapon_class.as_deref()),
                ],
            )?;
            let augment_id = self.transaction.last_insert_rowid();
            for set_name in augment.set_bonus_names.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                self.pending_set_augment_links.push((augment_id, set_name.to_string()));
            }
            for slot_type_name in &augment.slot_types {
                let slot_type_id = self.ensure_augment_slot_type(slot_type_name)?;
                self.transaction.execute(
                    "INSERT OR IGNORE INTO augment_slots (augment_id, slot_id) VALUES (?1, ?2)",
                    params![augment_id, slot_type_id],
                )?;
            }
            if let Some(description) = augment.description.as_deref() {
                self.link_augment_to_quests(augment_id, description, report)?;
            }
            self.write_modifiers(ModifierSource::Augment, augment_id, &augment.effects)?;
            let augment_owner =
                BonusOwner { kind: BonusOwnerKind::Augment, name: &augment.name, family: Some(&family) };
            for (sort_order, link) in
                self.ensure_derived_enchantments(&augment_owner, &augment.effects)?.into_iter().enumerate()
            {
                self.enchantments.insert_link(
                    EnchantmentOwner::Augment,
                    augment_id,
                    link.enchantment_id,
                    link.bonus_type,
                    (link.value, link.value2),
                    sort_order,
                )?;
            }
        }
        report.augment_count += augments.len();
        Ok(())
    }

    pub(super) fn link_augment_to_quests(
        &self,
        augment_id: i64,
        description: &str,
        report: &mut BuildReport,
    ) -> Result<()> {
        let Some(drop_text) = drop_text_in_description(description) else {
            return Ok(());
        };
        report.pack_augment_loot_link_count +=
            self.link_to_drop_text_packs(DroppedLoot::Augment(augment_id), drop_text)?;
        self.link_to_sources_named_in_drop_text(DroppedLoot::Augment(augment_id), drop_text, report)?;
        for linked_quest in self.link_to_drop_text_quests(DroppedLoot::Augment(augment_id), drop_text)? {
            report.quest_augment_loot_link_count += 1;
            if linked_quest.is_newly_rare {
                report.drop_text_rare_augment_link_count += 1;
            }
        }
        Ok(())
    }
}
