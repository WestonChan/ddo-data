//! `Items/*.item` → `items` and its satellite tables, plus item-level modifiers and clickies.

use super::{nonempty, BuildReport, Ctx};
use crate::map::buff::Resolved;
use crate::map::material;
use crate::map::placement::{classify, Placement};
use crate::xml::items::Item;
use anyhow::{bail, Result};
use ddo_model::enums::{ArmorType, LootType, ModifierSource};
use rusqlite::params;

impl Ctx<'_> {
    pub(super) fn write_item(&mut self, item: &Item, report: &mut BuildReport) -> Result<()> {
        let Some(placement) = classify(&item.equipment_slot, item.weapon.as_deref(), item.armor.as_deref())? else {
            self.tx.execute(
                "INSERT OR REPLACE INTO excluded_items (name, reason) VALUES (?1, ?2)",
                params![item.name.trim(), "cosmetic-only slots"],
            )?;
            report.items_skipped_cosmetic += 1;
            return Ok(());
        };
        let placement = &placement;
        let mut enhancement_bonus: Option<i64> = None;
        let mut resolved = Vec::with_capacity(item.buffs.len());
        for buff in &item.buffs {
            match self.buff_map.resolve(buff)? {
                Resolved::Enhancement(v) => enhancement_bonus = Some(v),
                other => resolved.push((buff, other)),
            }
        }

        let material_id = match material::normalize(item.material.as_deref()) {
            Some(name) => Some(self.material_id(&name)?),
            None => None,
        };
        let name = item.name.trim();
        self.tx.execute(
            "INSERT INTO items (name, slot_id, item_category, item_type, minimum_level, enhancement_bonus, material_id, race_required,
                                icon, description, drop_location, set_bonus, accepts_sentience, is_minor_artifact, wiki_url)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                name,
                placement.slot.id(),
                placement.category.as_str(),
                placement.item_type,
                item.min_level,
                enhancement_bonus,
                material_id,
                item.race_required(),
                item.icon.as_deref().map(str::trim),
                item.description.as_deref().map(str::trim).filter(|s| !s.is_empty()),
                item.drop_location.as_deref().map(str::trim).filter(|s| !s.is_empty()),
                item.set_bonus.first().map(|s| s.trim()),
                item.accepts_sentience,
                item.minor_artifact,
                wiki_url(name),
            ],
        )?;
        let item_id = self.tx.last_insert_rowid();

        if let Some(wt) = placement.weapon_type() {
            self.write_weapon_stats(item_id, item, placement, wt.id, enhancement_bonus)?;
            if wt.is_shield {
                self.write_armor_stats(item_id, item, ArmorType::Shield)?;
            }
        }
        if let Some(armor) = item.armor.as_deref() {
            let Some(armor_type) = ArmorType::parse(armor.trim()) else {
                bail!("unknown <Armor> type {armor:?}");
            };
            self.write_armor_stats(item_id, item, armor_type)?;
        }

        for (sort_order, (buff, res)) in resolved.into_iter().enumerate() {
            let template = self.templates.get(buff.kind.trim()).map(String::as_str).unwrap_or("");
            match res {
                Resolved::Bonus { stat, bonus_type, value, value2 } => {
                    let description =
                        if template.is_empty() { None } else { Some(self.buff_map.describe(template, buff)) };
                    let bonus_id = self.bonus_id(stat, bonus_type, value, value2, description.as_deref())?;
                    self.tx.execute(
                        "INSERT INTO item_bonuses (item_id, bonus_id, sort_order) VALUES (?1, ?2, ?3)",
                        params![item_id, bonus_id, sort_order as i64],
                    )?;
                }
                Resolved::Effect { name: effect_name, value, target } => {
                    let effect_id = match self.caches.effects.get(&effect_name) {
                        Some(id) => *id,
                        None => {
                            let description = if template.is_empty() { None } else { Some(template) };
                            self.tx.execute(
                                "INSERT INTO effects (name, description) VALUES (?1, ?2)",
                                params![effect_name, description],
                            )?;
                            let id = self.tx.last_insert_rowid();
                            self.caches.effects.insert(effect_name.clone(), id);
                            id
                        }
                    };
                    self.tx.execute(
                        "INSERT INTO item_effects (item_id, effect_id, sort_order, value, target) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![item_id, effect_id, sort_order as i64, value, target],
                    )?;
                }
                Resolved::Enhancement(_) => unreachable!("filtered above"),
            }
        }

        for (sort_order, aug) in item.augments.iter().enumerate() {
            let slot_id = self.slot_type_id(&aug.kind)?;
            self.tx.execute(
                "INSERT INTO item_augment_slots (item_id, sort_order, slot_id) VALUES (?1, ?2, ?3)",
                params![item_id, sort_order as i64, slot_id],
            )?;
            for (option_order, opt) in aug.options.iter().enumerate() {
                self.tx.execute(
                    "INSERT INTO item_augment_slot_options (item_id, slot_order, option_order, name, description, min_level)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        item_id,
                        sort_order as i64,
                        option_order as i64,
                        opt.name.trim(),
                        Some(opt.description.trim()).filter(|s| !s.is_empty()),
                        opt.min_level
                    ],
                )?;
            }
        }

        self.write_modifiers(ModifierSource::Item, item_id, &item.effects)?;
        for (clickie_order, e) in item.effects.iter().filter(|e| e.types[0] == "ItemClickie").enumerate() {
            let Some(clickie_name) = e.items.first().map(|s| s.trim()) else {
                bail!("{name}: ItemClickie effect without an <Item>");
            };
            // Names not in ItemClickies.xml are spells; the spells stage resolves those.
            let clickie_id = self.caches.clickies.get(clickie_name).copied();
            self.tx.execute(
                "INSERT INTO item_clickies (item_id, sort_order, name, clickie_id) VALUES (?1, ?2, ?3, ?4)",
                params![item_id, clickie_order as i64, clickie_name, clickie_id],
            )?;
        }
        if let Some(set) = nonempty(item.set_bonus.first().map(String::as_str)) {
            self.pending_set_items.push((item_id, set.to_string()));
        }

        if let Some(drop) = item.drop_location.as_deref() {
            report.quest_loot_links += self.link_quests(item_id, drop)?;
        }
        report.items_written += 1;
        Ok(())
    }

    fn write_weapon_stats(
        &self,
        item_id: i64,
        item: &Item,
        placement: &Placement,
        weapon_type_id: i64,
        enhancement: Option<i64>,
    ) -> Result<()> {
        let dice = item.base_dice.as_ref();
        let damage = render_damage(
            item.weapon_damage,
            dice.and_then(|d| Some((d.number?, d.sides?, d.bonus))),
            enhancement,
            &item.dr_bypass,
        );
        let critical = match (item.critical_threat_range, item.critical_multiplier) {
            (Some(range), Some(mult)) if range <= 1 => Some(format!("20 / x{mult}")),
            (Some(range), Some(mult)) => Some(format!("{}-20 / x{mult}", 21 - range)),
            _ => None,
        };
        self.tx.execute(
            "INSERT INTO item_weapon_stats (item_id, weapon_type_id, base_dice_count, base_dice_sides, base_dice_bonus, damage_multiplier,
                                            critical_threat_range, critical_multiplier, attack_modifier, damage_modifier, handedness, damage, critical)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                item_id,
                weapon_type_id,
                dice.and_then(|d| d.number),
                dice.and_then(|d| d.sides),
                dice.and_then(|d| d.bonus),
                item.weapon_damage,
                item.critical_threat_range,
                item.critical_multiplier,
                join_nonempty(&item.attack_modifier),
                join_nonempty(&item.damage_modifier),
                placement.handedness.map(|h| h.as_str()),
                damage,
                critical,
            ],
        )?;
        for bypass in &item.dr_bypass {
            let bypass = bypass.trim();
            if bypass.is_empty() || bypass == "-" {
                continue;
            }
            self.tx.execute(
                "INSERT OR IGNORE INTO item_dr_bypass (item_id, bypass) VALUES (?1, ?2)",
                params![item_id, bypass],
            )?;
        }
        Ok(())
    }

    fn write_armor_stats(&self, item_id: i64, item: &Item, armor_type: ArmorType) -> Result<()> {
        self.tx.execute(
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

    /// Link the item to every quest named in its drop text. Longer names are matched first and
    /// blanked out so a shorter quest name nested inside them does not also match.
    fn link_quests(&self, item_id: i64, drop: &str) -> Result<usize> {
        let mut text = drop.to_string();
        let lower = drop.to_lowercase();
        let mut links = 0;
        for (name, quest_id, is_raid) in &self.quests.by_length {
            if name.is_empty() || !text.contains(name.as_str()) {
                continue;
            }
            text = text.replace(name.as_str(), &" ".repeat(name.len()));
            let loot_type = if *is_raid {
                LootType::Raid
            } else if lower.contains("reward") {
                LootType::Reward
            } else {
                LootType::Chest
            };
            self.tx.execute(
                "INSERT OR IGNORE INTO quest_loot (quest_id, item_id, loot_type) VALUES (?1, ?2, ?3)",
                params![quest_id, item_id, loot_type.as_str()],
            )?;
            links += 1;
        }
        Ok(links)
    }
}

fn join_nonempty(parts: &[String]) -> Option<String> {
    let joined: Vec<&str> = parts.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if joined.is_empty() {
        None
    } else {
        Some(joined.join(", "))
    }
}

/// `3.6[1d10] + 7 Good, Magic, Pierce, Slash`, the legacy display form.
fn render_damage(
    multiplier: Option<f64>,
    dice: Option<(i64, i64, Option<i64>)>,
    enhancement: Option<i64>,
    bypass: &[String],
) -> Option<String> {
    let (n, s, b) = dice?;
    let mut out = String::new();
    if let Some(m) = multiplier {
        out.push_str(&m.to_string());
    }
    out.push_str(&format!("[{n}d{s}"));
    if let Some(b) = b.filter(|b| *b != 0) {
        out.push_str(&format!("{b:+}"));
    }
    out.push(']');
    if let Some(e) = enhancement {
        out.push_str(&format!(" + {e}"));
    }
    let bypass: Vec<&str> = bypass.iter().map(|s| s.trim()).filter(|s| !s.is_empty() && *s != "-").collect();
    if !bypass.is_empty() {
        out.push(' ');
        out.push_str(&bypass.join(", "));
    }
    Some(out)
}

fn wiki_url(name: &str) -> String {
    format!("https://ddowiki.com/page/Item:{}", name.replace(' ', "_").replace('+', "%2B"))
}
