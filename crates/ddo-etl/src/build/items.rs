use super::{joined_non_empty, trimmed_non_empty, BuildReport, TableWriter};
use crate::map::buff::ResolvedBuff;
use crate::map::drop_location::{marks_rare_loot, segment_containing};
use crate::map::material;
use crate::map::placement::placement_of;
use crate::xml::items::Item;
use anyhow::{bail, Result};
use ddo_model::enums::{ArmorType, EquipmentSlot, Handedness, ItemCategory, LootType, ModifierSource, RowSource};
use rusqlite::params;

pub(super) struct ItemRow<'a> {
    pub(super) name: &'a str,
    pub(super) equipment_slot: EquipmentSlot,
    pub(super) category: ItemCategory,
    pub(super) item_type: Option<&'a str>,
    pub(super) minimum_level: Option<i64>,
    pub(super) enhancement_bonus: Option<i64>,
    pub(super) material_id: Option<i64>,
    pub(super) race_required: Option<String>,
    pub(super) icon: Option<&'a str>,
    pub(super) description: Option<&'a str>,
    pub(super) drop_location: Option<&'a str>,
    pub(super) set_bonus: Option<&'a str>,
    pub(super) accepts_sentience: bool,
    pub(super) is_minor_artifact: bool,
    pub(super) wiki_url: String,
    pub(super) source: RowSource,
}

pub(super) struct WeaponStatsRow<'a> {
    pub(super) weapon_type_id: i64,
    pub(super) base_dice_count: Option<i64>,
    pub(super) base_dice_sides: Option<i64>,
    pub(super) base_dice_bonus: Option<i64>,
    pub(super) damage_multiplier: Option<f64>,
    pub(super) critical_threat_range: Option<i64>,
    pub(super) critical_multiplier: Option<i64>,
    pub(super) attack_modifier: Option<String>,
    pub(super) damage_modifier: Option<String>,
    pub(super) handedness: Option<Handedness>,
    pub(super) enhancement_bonus: Option<i64>,
    pub(super) dr_bypasses: &'a [String],
}

pub(super) struct ArmorStatsRow {
    pub(super) armor_type: ArmorType,
    pub(super) armor_bonus: Option<i64>,
    pub(super) max_dex_bonus: Option<i64>,
    pub(super) arcane_spell_failure: Option<i64>,
    pub(super) armor_check_penalty: Option<i64>,
    pub(super) shield_bonus: Option<i64>,
    pub(super) damage_reduction: Option<i64>,
    pub(super) mithral_body: Option<i64>,
    pub(super) adamantine_body: Option<i64>,
}

impl ArmorStatsRow {
    fn from_item(item: &Item, armor_type: ArmorType) -> Self {
        Self {
            armor_type,
            armor_bonus: item.armor_bonus,
            max_dex_bonus: item.maximum_dexterity_bonus,
            arcane_spell_failure: item.arcane_spell_failure,
            armor_check_penalty: item.armor_check_penalty,
            shield_bonus: item.shield_bonus,
            damage_reduction: item.damage_reduction,
            mithral_body: item.mithral_body,
            adamantine_body: item.adamantine_body,
        }
    }
}

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
        let item_id = self.insert_item_row(&ItemRow {
            name: item_name,
            equipment_slot: placement.equipment_slot,
            category: placement.category,
            item_type: placement.item_type.as_deref(),
            minimum_level: item.minimum_level,
            enhancement_bonus,
            material_id,
            race_required: item.race_required(),
            icon: item.icon.as_deref().map(str::trim),
            description: trimmed_non_empty(item.description.as_deref()),
            drop_location: trimmed_non_empty(item.drop_location.as_deref()),
            set_bonus: item.set_bonus_names.first().map(|s| s.trim()),
            accepts_sentience: item.accepts_sentience,
            is_minor_artifact: item.is_minor_artifact,
            wiki_url: item_wiki_url(item_name),
            source: RowSource::Maetrim,
        })?;

        if let Some(weapon_type) = placement.weapon_type() {
            let base_dice = item.base_dice.as_ref();
            self.insert_weapon_stats(
                item_id,
                &WeaponStatsRow {
                    weapon_type_id: weapon_type.id,
                    base_dice_count: base_dice.and_then(|d| d.count),
                    base_dice_sides: base_dice.and_then(|d| d.sides),
                    base_dice_bonus: base_dice.and_then(|d| d.bonus),
                    damage_multiplier: item.damage_multiplier,
                    critical_threat_range: item.critical_threat_range,
                    critical_multiplier: item.critical_multiplier,
                    attack_modifier: joined_non_empty(&item.attack_modifiers, ", "),
                    damage_modifier: joined_non_empty(&item.damage_modifiers, ", "),
                    handedness: placement.handedness,
                    enhancement_bonus,
                    dr_bypasses: &item.dr_bypasses,
                },
            )?;
            if weapon_type.is_shield {
                self.insert_armor_stats(item_id, &ArmorStatsRow::from_item(item, ArmorType::Shield))?;
            }
        }
        if let Some(armor_name) = item.armor.as_deref() {
            let Some(armor_type) = ArmorType::parse(armor_name.trim()) else {
                bail!("unknown <Armor> type {armor_name:?}");
            };
            self.insert_armor_stats(item_id, &ArmorStatsRow::from_item(item, armor_type))?;
        }

        for (sort_order, (buff, resolved_buff)) in resolved_buffs.into_iter().enumerate() {
            let template = self.buff_description_templates.get(buff.kind.trim()).map(String::as_str).unwrap_or("");
            match resolved_buff {
                ResolvedBuff::Bonus { stat, bonus_type, value, second_value } => {
                    let description =
                        if template.is_empty() { None } else { Some(self.buff_map.description(template, buff)) };
                    let bonus_id = self.ensure_bonus(stat, bonus_type, value, second_value, description.as_deref())?;
                    self.insert_item_bonus(item_id, bonus_id, sort_order)?;
                }
                ResolvedBuff::Effect { name: effect_name, value, target } => {
                    let description = if template.is_empty() { None } else { Some(template) };
                    let effect_id = self.ensure_effect(&effect_name, description)?;
                    self.insert_item_effect(item_id, effect_id, sort_order, value, target.as_deref())?;
                }
                ResolvedBuff::EnhancementBonus(_) => unreachable!("filtered above"),
            }
        }

        for (slot_order, augment_slot) in item.augment_slots.iter().enumerate() {
            let slot_type_id = self.ensure_augment_slot_type(&augment_slot.kind)?;
            self.insert_item_augment_slot(item_id, slot_order, slot_type_id)?;
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

    pub(super) fn insert_item_row(&self, row: &ItemRow) -> Result<i64> {
        self.transaction.execute(
            "INSERT INTO items (name, slot_id, item_category, item_type, minimum_level, enhancement_bonus, material_id, race_required,
                                icon, description, drop_location, set_bonus, accepts_sentience, is_minor_artifact, wiki_url, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                row.name,
                row.equipment_slot.id(),
                row.category.as_str(),
                row.item_type,
                row.minimum_level,
                row.enhancement_bonus,
                row.material_id,
                row.race_required,
                row.icon,
                row.description,
                row.drop_location,
                row.set_bonus,
                row.accepts_sentience,
                row.is_minor_artifact,
                row.wiki_url,
                row.source.as_str(),
            ],
        )?;
        Ok(self.transaction.last_insert_rowid())
    }

    pub(super) fn insert_weapon_stats(&self, item_id: i64, row: &WeaponStatsRow) -> Result<()> {
        let damage_text = weapon_damage_text(
            row.damage_multiplier,
            row.base_dice_count.zip(row.base_dice_sides).map(|(count, sides)| (count, sides, row.base_dice_bonus)),
            row.enhancement_bonus,
            row.dr_bypasses,
        );
        let critical_text = match (row.critical_threat_range, row.critical_multiplier) {
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
                row.weapon_type_id,
                row.base_dice_count,
                row.base_dice_sides,
                row.base_dice_bonus,
                row.damage_multiplier,
                row.critical_threat_range,
                row.critical_multiplier,
                row.attack_modifier,
                row.damage_modifier,
                row.handedness.map(|h| h.as_str()),
                damage_text,
                critical_text,
            ],
        )?;
        for bypass in row.dr_bypasses {
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

    pub(super) fn insert_armor_stats(&self, item_id: i64, row: &ArmorStatsRow) -> Result<()> {
        self.transaction.execute(
            "INSERT OR REPLACE INTO item_armor_stats (item_id, armor_type, armor_bonus, max_dex_bonus, arcane_spell_failure, armor_check_penalty,
                                                      shield_bonus, damage_reduction, mithral_body, adamantine_body)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                item_id,
                row.armor_type.as_str(),
                row.armor_bonus,
                row.max_dex_bonus,
                row.arcane_spell_failure,
                row.armor_check_penalty,
                row.shield_bonus,
                row.damage_reduction,
                row.mithral_body,
                row.adamantine_body,
            ],
        )?;
        Ok(())
    }

    pub(super) fn insert_item_bonus(&self, item_id: i64, bonus_id: i64, sort_order: usize) -> Result<()> {
        self.transaction.execute(
            "INSERT INTO item_bonuses (item_id, bonus_id, sort_order) VALUES (?1, ?2, ?3)",
            params![item_id, bonus_id, sort_order as i64],
        )?;
        Ok(())
    }

    pub(super) fn ensure_effect(&mut self, effect_name: &str, description: Option<&str>) -> Result<i64> {
        if let Some(id) = self.written.effect_ids_by_name.get(effect_name) {
            return Ok(*id);
        }
        self.transaction
            .execute("INSERT INTO effects (name, description) VALUES (?1, ?2)", params![effect_name, description])?;
        let id = self.transaction.last_insert_rowid();
        self.written.effect_ids_by_name.insert(effect_name.to_string(), id);
        Ok(id)
    }

    pub(super) fn insert_item_effect(
        &self,
        item_id: i64,
        effect_id: i64,
        sort_order: usize,
        value: Option<i64>,
        target: Option<&str>,
    ) -> Result<()> {
        self.transaction.execute(
            "INSERT INTO item_effects (item_id, effect_id, sort_order, value, target) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![item_id, effect_id, sort_order as i64, value, target],
        )?;
        Ok(())
    }

    pub(super) fn insert_item_augment_slot(&self, item_id: i64, sort_order: usize, slot_type_id: i64) -> Result<()> {
        self.transaction.execute(
            "INSERT INTO item_augment_slots (item_id, sort_order, slot_id) VALUES (?1, ?2, ?3)",
            params![item_id, sort_order as i64, slot_type_id],
        )?;
        Ok(())
    }

    pub(super) fn insert_quest_loot_link(
        &self,
        quest_id: i64,
        item_id: i64,
        loot_type: LootType,
        is_rare: bool,
    ) -> Result<usize> {
        Ok(self.transaction.execute(
            "INSERT INTO quest_loot (quest_id, item_id, loot_type, is_rare) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (quest_id, item_id) DO UPDATE SET is_rare = 1 WHERE excluded.is_rare > quest_loot.is_rare",
            params![quest_id, item_id, loot_type.as_str(), is_rare],
        )?)
    }

    fn link_item_to_quests(&self, item_id: i64, drop_location: &str, report: &mut BuildReport) -> Result<()> {
        let mut unmatched_text = drop_location.to_string();
        let lowercase_drop_location = drop_location.to_lowercase();
        for quest in &self.drop_text_quests.longest_name_first {
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
            let changed_row_count = self.insert_quest_loot_link(quest.id, item_id, loot_type, is_rare)?;
            report.quest_loot_link_count += 1;
            if quest.is_wiki {
                report.drop_text_wiki_quest_link_count += 1;
            }
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

pub(super) fn item_wiki_url(item_name: &str) -> String {
    format!("https://ddowiki.com/page/Item:{}", item_name.replace(' ', "_").replace('+', "%2B"))
}
