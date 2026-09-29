use super::{joined_non_empty, trimmed_non_empty, BuildReport, TableWriter};
use crate::map::buff::ResolvedBuff;
use crate::map::drop_location::{marks_rare_loot, segment_containing};
use crate::map::material;
use crate::map::placement::{placement_of, Placement};
use crate::xml::items::Item;
use anyhow::{bail, Result};
use ddo_model::enums::{ArmorType, LootType, ModifierSource};
use rusqlite::params;

impl TableWriter<'_> {
    pub(super) fn write_item(&mut self, item: &Item, report: &mut BuildReport) -> Result<()> {
        let Some(placement) = placement_of(&item.equipment_slots, item.weapon.as_deref(), item.armor.as_deref())?
        else {
            self.transaction.execute(
                "INSERT OR REPLACE INTO excluded_items (name, reason) VALUES (?1, ?2)",
                params![item.name.trim(), "cosmetic-only slots"],
            )?;
            report.skipped_cosmetic_item_count += 1;
            return Ok(());
        };
        let placement = &placement;
        let mut enhancement_bonus: Option<i64> = None;
        let mut resolved_buffs = Vec::with_capacity(item.buffs.len());
        for buff in &item.buffs {
            match self.buff_map.resolved(buff)? {
                ResolvedBuff::EnhancementBonus(value) => enhancement_bonus = Some(value),
                resolved_buff => resolved_buffs.push((buff, resolved_buff)),
            }
        }

        let material_id = match material::normalized_name(item.material.as_deref()) {
            Some(material_name) => Some(self.ensure_material(&material_name)?),
            None => None,
        };
        let item_name = item.name.trim();
        self.transaction.execute(
            "INSERT INTO items (name, slot_id, item_category, item_type, minimum_level, enhancement_bonus, material_id, race_required,
                                icon, description, drop_location, set_bonus, accepts_sentience, is_minor_artifact, wiki_url)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                item_name,
                placement.equipment_slot.id(),
                placement.category.as_str(),
                placement.item_type,
                item.minimum_level,
                enhancement_bonus,
                material_id,
                item.race_required(),
                item.icon.as_deref().map(str::trim),
                item.description.as_deref().map(str::trim).filter(|s| !s.is_empty()),
                item.drop_location.as_deref().map(str::trim).filter(|s| !s.is_empty()),
                item.set_bonus_names.first().map(|s| s.trim()),
                item.accepts_sentience,
                item.is_minor_artifact,
                item_wiki_url(item_name),
            ],
        )?;
        let item_id = self.transaction.last_insert_rowid();

        if let Some(weapon_type) = placement.weapon_type() {
            self.write_weapon_stats(item_id, item, placement, weapon_type.id, enhancement_bonus)?;
            if weapon_type.is_shield {
                self.write_armor_stats(item_id, item, ArmorType::Shield)?;
            }
        }
        if let Some(armor_name) = item.armor.as_deref() {
            let Some(armor_type) = ArmorType::parse(armor_name.trim()) else {
                bail!("unknown <Armor> type {armor_name:?}");
            };
            self.write_armor_stats(item_id, item, armor_type)?;
        }

        for (sort_order, (buff, resolved_buff)) in resolved_buffs.into_iter().enumerate() {
            let template = self.buff_description_templates.get(buff.kind.trim()).map(String::as_str).unwrap_or("");
            match resolved_buff {
                ResolvedBuff::Bonus { stat, bonus_type, value, second_value } => {
                    let description =
                        if template.is_empty() { None } else { Some(self.buff_map.description(template, buff)) };
                    let bonus_id = self.ensure_bonus(stat, bonus_type, value, second_value, description.as_deref())?;
                    self.transaction.execute(
                        "INSERT INTO item_bonuses (item_id, bonus_id, sort_order) VALUES (?1, ?2, ?3)",
                        params![item_id, bonus_id, sort_order as i64],
                    )?;
                }
                ResolvedBuff::Effect { name: effect_name, value, target } => {
                    let effect_id = match self.written.effect_ids_by_name.get(&effect_name) {
                        Some(id) => *id,
                        None => {
                            let description = if template.is_empty() { None } else { Some(template) };
                            self.transaction.execute(
                                "INSERT INTO effects (name, description) VALUES (?1, ?2)",
                                params![effect_name, description],
                            )?;
                            let id = self.transaction.last_insert_rowid();
                            self.written.effect_ids_by_name.insert(effect_name.clone(), id);
                            id
                        }
                    };
                    self.transaction.execute(
                        "INSERT INTO item_effects (item_id, effect_id, sort_order, value, target) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![item_id, effect_id, sort_order as i64, value, target],
                    )?;
                }
                ResolvedBuff::EnhancementBonus(_) => unreachable!("filtered above"),
            }
        }

        for (slot_order, augment_slot) in item.augment_slots.iter().enumerate() {
            let slot_type_id = self.ensure_augment_slot_type(&augment_slot.kind)?;
            self.transaction.execute(
                "INSERT INTO item_augment_slots (item_id, sort_order, slot_id) VALUES (?1, ?2, ?3)",
                params![item_id, slot_order as i64, slot_type_id],
            )?;
            for (option_order, slot_option) in augment_slot.options.iter().enumerate() {
                self.transaction.execute(
                    "INSERT INTO item_augment_slot_options (item_id, slot_order, option_order, name, description, min_level)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        item_id,
                        slot_order as i64,
                        option_order as i64,
                        slot_option.name.trim(),
                        Some(slot_option.description.trim()).filter(|s| !s.is_empty()),
                        slot_option.minimum_level
                    ],
                )?;
            }
        }

        self.write_modifiers(ModifierSource::Item, item_id, &item.effects)?;
        for (clickie_order, effect) in item.effects.iter().filter(|e| e.types[0] == "ItemClickie").enumerate() {
            let Some(clickie_name) = effect.targets.first().map(|s| s.trim()) else {
                bail!("{item_name}: ItemClickie effect without an <Item>");
            };
            let clickie_id = self.written.clickie_ids_by_name.get(clickie_name).copied();
            self.transaction.execute(
                "INSERT INTO item_clickies (item_id, sort_order, name, clickie_id) VALUES (?1, ?2, ?3, ?4)",
                params![item_id, clickie_order as i64, clickie_name, clickie_id],
            )?;
        }
        if let Some(set_name) = trimmed_non_empty(item.set_bonus_names.first().map(String::as_str)) {
            self.pending_set_item_links.push((item_id, set_name.to_string()));
        }

        if let Some(drop_location) = item.drop_location.as_deref() {
            self.link_item_to_quests(item_id, drop_location, report)?;
        }
        report.written_item_count += 1;
        Ok(())
    }

    fn write_weapon_stats(
        &self,
        item_id: i64,
        item: &Item,
        placement: &Placement,
        weapon_type_id: i64,
        enhancement_bonus: Option<i64>,
    ) -> Result<()> {
        let base_dice = item.base_dice.as_ref();
        let damage_text = weapon_damage_text(
            item.damage_multiplier,
            base_dice.and_then(|d| Some((d.count?, d.sides?, d.bonus))),
            enhancement_bonus,
            &item.dr_bypasses,
        );
        let critical_text = match (item.critical_threat_range, item.critical_multiplier) {
            (Some(threat_range), Some(multiplier)) if threat_range <= 1 => Some(format!("20 / x{multiplier}")),
            (Some(threat_range), Some(multiplier)) => Some(format!("{}-20 / x{multiplier}", 21 - threat_range)),
            _ => None,
        };
        self.transaction.execute(
            "INSERT INTO item_weapon_stats (item_id, weapon_type_id, base_dice_count, base_dice_sides, base_dice_bonus, damage_multiplier,
                                            critical_threat_range, critical_multiplier, attack_modifier, damage_modifier, handedness, damage, critical)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                item_id,
                weapon_type_id,
                base_dice.and_then(|d| d.count),
                base_dice.and_then(|d| d.sides),
                base_dice.and_then(|d| d.bonus),
                item.damage_multiplier,
                item.critical_threat_range,
                item.critical_multiplier,
                joined_non_empty(&item.attack_modifiers, ", "),
                joined_non_empty(&item.damage_modifiers, ", "),
                placement.handedness.map(|h| h.as_str()),
                damage_text,
                critical_text,
            ],
        )?;
        for bypass in &item.dr_bypasses {
            let bypass = bypass.trim();
            if bypass.is_empty() || bypass == "-" {
                continue;
            }
            self.transaction.execute(
                "INSERT OR IGNORE INTO item_dr_bypass (item_id, bypass) VALUES (?1, ?2)",
                params![item_id, bypass],
            )?;
        }
        Ok(())
    }

    fn write_armor_stats(&self, item_id: i64, item: &Item, armor_type: ArmorType) -> Result<()> {
        self.transaction.execute(
            "INSERT OR REPLACE INTO item_armor_stats (item_id, armor_type, armor_bonus, max_dex_bonus, arcane_spell_failure, armor_check_penalty,
                                                      shield_bonus, damage_reduction, mithral_body, adamantine_body)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                item_id,
                armor_type.as_str(),
                item.armor_bonus,
                item.maximum_dexterity_bonus,
                item.arcane_spell_failure,
                item.armor_check_penalty,
                item.shield_bonus,
                item.damage_reduction,
                item.mithral_body,
                item.adamantine_body,
            ],
        )?;
        Ok(())
    }

    fn link_item_to_quests(&self, item_id: i64, drop_location: &str, report: &mut BuildReport) -> Result<()> {
        let mut unmatched_text = drop_location.to_string();
        let lowercase_drop_location = drop_location.to_lowercase();
        for quest in &self.written_quests.longest_name_first {
            let quest_name = quest.name.as_str();
            if quest_name.is_empty() || !unmatched_text.contains(quest_name) {
                continue;
            }
            let is_rare = unmatched_text
                .match_indices(quest_name)
                .any(|(byte_offset, _)| marks_rare_loot(segment_containing(drop_location, byte_offset)));
            unmatched_text = unmatched_text.replace(quest_name, &" ".repeat(quest_name.len()));
            let loot_type = if quest.is_raid {
                LootType::Raid
            } else if lowercase_drop_location.contains("reward") {
                LootType::Reward
            } else {
                LootType::Chest
            };
            let changed_row_count = self.transaction.execute(
                "INSERT INTO quest_loot (quest_id, item_id, loot_type, is_rare) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (quest_id, item_id) DO UPDATE SET is_rare = 1 WHERE excluded.is_rare > quest_loot.is_rare",
                params![quest.id, item_id, loot_type.as_str(), is_rare],
            )?;
            report.quest_loot_link_count += 1;
            if is_rare && changed_row_count > 0 {
                report.drop_text_rare_link_count += 1;
            }
        }
        Ok(())
    }
}

fn weapon_damage_text(
    damage_multiplier: Option<f64>,
    base_dice: Option<(i64, i64, Option<i64>)>,
    enhancement_bonus: Option<i64>,
    dr_bypasses: &[String],
) -> Option<String> {
    let (dice_count, dice_sides, dice_bonus) = base_dice?;
    let mut text = String::new();
    if let Some(multiplier) = damage_multiplier {
        text.push_str(&multiplier.to_string());
    }
    text.push_str(&format!("[{dice_count}d{dice_sides}"));
    if let Some(bonus) = dice_bonus.filter(|b| *b != 0) {
        text.push_str(&format!("{bonus:+}"));
    }
    text.push(']');
    if let Some(enhancement) = enhancement_bonus {
        text.push_str(&format!(" + {enhancement}"));
    }
    let bypasses: Vec<&str> = dr_bypasses.iter().map(|s| s.trim()).filter(|s| !s.is_empty() && *s != "-").collect();
    if !bypasses.is_empty() {
        text.push(' ');
        text.push_str(&bypasses.join(", "));
    }
    Some(text)
}

fn item_wiki_url(item_name: &str) -> String {
    format!("https://ddowiki.com/page/Item:{}", item_name.replace(' ', "_").replace('+', "%2B"))
}
