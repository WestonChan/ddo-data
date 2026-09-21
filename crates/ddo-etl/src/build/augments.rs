//! `Augments/*.xml` → `augments`, `augment_slots`, `augment_bonuses`, modifiers.

use super::{json_numbers, nonempty, Ctx};
use crate::xml::augments::parse_augments_file;
use anyhow::Result;
use ddo_model::enums::ModifierSource;
use rusqlite::params;
use std::path::Path;

impl Ctx<'_> {
    /// Returns how many augments the file contributed.
    pub(super) fn write_augments_file(&mut self, path: &Path) -> Result<usize> {
        let (family, augments) = parse_augments_file(path)?;
        for a in &augments {
            self.tx.execute(
                "INSERT INTO augments (name, family, description, effect_description, min_level, icon, choose_level, levels,
                                       level_values, level_values2, dual_values, enter_value, suppress_set_bonus, set_bonus,
                                       adds_augment, grants_augment, weapon_class)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                params![
                    a.name,
                    family,
                    nonempty(a.description.as_deref()),
                    join_lines(&a.effect_descriptions),
                    a.min_level,
                    nonempty(a.icon.as_deref()),
                    a.choose_level,
                    json_numbers(&a.levels),
                    json_numbers(&a.level_values),
                    json_numbers(&a.level_values2),
                    a.dual_values,
                    a.enter_value,
                    a.suppress_set_bonus,
                    join_lines(&a.set_bonus),
                    join_lines(&a.add_augment),
                    nonempty(a.grant_augment.as_deref()),
                    nonempty(a.weapon_class.as_deref()),
                ],
            )?;
            let id = self.tx.last_insert_rowid();
            for slot in &a.slot_types {
                let slot_id = self.slot_type_id(slot)?;
                self.tx.execute(
                    "INSERT OR IGNORE INTO augment_slots (augment_id, slot_id) VALUES (?1, ?2)",
                    params![id, slot_id],
                )?;
            }
            self.write_modifiers(ModifierSource::Augment, id, &a.effects)?;
            for (sort_order, bonus_id) in self.derived_bonus_ids(&a.effects)?.into_iter().enumerate() {
                self.tx.execute(
                    "INSERT INTO augment_bonuses (augment_id, bonus_id, sort_order) VALUES (?1, ?2, ?3)",
                    params![id, bonus_id, sort_order as i64],
                )?;
            }
        }
        Ok(augments.len())
    }
}

fn join_lines(parts: &[String]) -> Option<String> {
    let kept: Vec<&str> = parts.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if kept.is_empty() {
        None
    } else {
        Some(kept.join("\n"))
    }
}
