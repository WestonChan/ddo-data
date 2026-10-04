use super::{json_number_array, json_string_array, trimmed_non_empty, BuildReport, TableWriter};
use crate::xml::feats::Attack;
use crate::xml::spells;
use crate::xml::trees::{self, Enhancement, EnhancementSelection};
use anyhow::Result;
use ddo_model::enums::{AbilityOwner, EnhancementTreeKind, ModifierSource, RequirementOwner};
use rusqlite::{params, OptionalExtension};
use std::path::Path;

impl TableWriter<'_> {
    pub(super) fn write_tree_file(&mut self, path: &Path, report: &mut BuildReport) -> Result<()> {
        let tree = trees::parse(path)?;
        let tree_kind = if tree.is_racial {
            EnhancementTreeKind::Racial
        } else if tree.is_destiny {
            EnhancementTreeKind::Destiny
        } else if tree.is_reaper {
            EnhancementTreeKind::Reaper
        } else if tree.is_universal {
            EnhancementTreeKind::Universal
        } else {
            EnhancementTreeKind::Class
        };
        let existing_tree_id: Option<i64> = self
            .transaction
            .query_row("SELECT id FROM enhancement_trees WHERE name = ?1", params![tree.name], |r| r.get(0))
            .optional()?;
        if existing_tree_id.is_some() {
            report.skipped_duplicate_tree_count += 1;
            return Ok(());
        }
        self.transaction.execute(
            "INSERT INTO enhancement_trees (name, version, kind, is_legacy, icon, background) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                tree.name,
                tree.version,
                tree_kind.as_str(),
                tree.is_legacy,
                trimmed_non_empty(tree.icon.as_deref()),
                trimmed_non_empty(tree.background.as_deref())
            ],
        )?;
        let tree_id = self.transaction.last_insert_rowid();
        if let Some(requirements) = &tree.requirements {
            self.write_requirements(RequirementOwner::EnhancementTree, tree_id, requirements)?;
        }
        for enhancement in &tree.enhancements {
            self.write_enhancement(tree_id, enhancement)?;
            report.enhancement_count += 1;
        }
        report.enhancement_tree_count += 1;
        Ok(())
    }

    fn write_enhancement(&mut self, tree_id: i64, enhancement: &Enhancement) -> Result<()> {
        self.transaction.execute(
            "INSERT INTO enhancements (tree_id, internal_name, name, description, icon, x, y, cost_per_rank, ranks, min_spent, is_tier5, is_clickie, arrows,
                                       cooldown_seconds, duration_seconds)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                tree_id,
                enhancement.internal_name,
                enhancement.name,
                trimmed_non_empty(enhancement.description.as_deref()),
                trimmed_non_empty(enhancement.icon.as_deref()),
                enhancement.x,
                enhancement.y,
                json_number_array(&enhancement.cost_per_rank),
                enhancement.rank_count,
                enhancement.minimum_points_spent,
                enhancement.is_tier5,
                enhancement.is_clickie,
                json_string_array(&enhancement.arrows),
                enhancement.attack.as_ref().and_then(|a| a.cooldown_seconds),
                enhancement.attack.as_ref().and_then(Attack::duration_seconds),
            ],
        )?;
        let enhancement_id = self.transaction.last_insert_rowid();
        if let Some(requirements) = &enhancement.requirements {
            self.write_requirements(RequirementOwner::Enhancement, enhancement_id, requirements)?;
        }
        self.write_abilities(
            AbilityOwner::Enhancement,
            enhancement_id,
            &enhancement.stances,
            &enhancement.dcs,
            enhancement.attack.as_ref(),
        )?;
        self.write_modifiers(ModifierSource::Enhancement, enhancement_id, &enhancement.effects)?;
        self.write_attack_bonuses(AbilityOwner::Enhancement, enhancement_id, enhancement.attack.as_ref())?;
        if let Some(selector) = &enhancement.selector {
            for excluded_internal_name in &selector.excluded_internal_names {
                self.transaction.execute(
                    "INSERT OR IGNORE INTO enhancement_selector_exclusions (enhancement_id, internal_name) VALUES (?1, ?2)",
                    params![enhancement_id, excluded_internal_name],
                )?;
            }
            for (sort_order, selection) in selector.selections.iter().enumerate() {
                self.write_enhancement_selection(enhancement_id, sort_order as i64, selection)?;
            }
        }
        Ok(())
    }

    fn write_enhancement_selection(
        &mut self,
        enhancement_id: i64,
        sort_order: i64,
        selection: &EnhancementSelection,
    ) -> Result<()> {
        self.transaction.execute(
            "INSERT INTO enhancement_selections (enhancement_id, sort_order, name, description, icon, cost_per_rank, ranks, min_spent, is_clickie,
                                                 cooldown_seconds, duration_seconds)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                enhancement_id,
                sort_order,
                selection.name,
                trimmed_non_empty(selection.description.as_deref()),
                trimmed_non_empty(selection.icon.as_deref()),
                json_number_array(&selection.cost_per_rank),
                selection.rank_count,
                selection.minimum_points_spent,
                selection.is_clickie,
                selection.attack.as_ref().and_then(|a| a.cooldown_seconds),
                selection.attack.as_ref().and_then(Attack::duration_seconds),
            ],
        )?;
        let selection_id = self.transaction.last_insert_rowid();
        if let Some(requirements) = &selection.requirements {
            self.write_requirements(RequirementOwner::EnhancementSelection, selection_id, requirements)?;
        }
        self.write_abilities(
            AbilityOwner::EnhancementSelection,
            selection_id,
            &selection.stances,
            &selection.dcs,
            selection.attack.as_ref(),
        )?;
        self.write_modifiers(ModifierSource::EnhancementSelection, selection_id, &selection.effects)?;
        self.write_attack_bonuses(AbilityOwner::EnhancementSelection, selection_id, selection.attack.as_ref())?;
        Ok(())
    }

    pub(super) fn write_spells(&mut self, spells: &[spells::Spell], report: &mut BuildReport) -> Result<()> {
        for spell in spells {
            let inserted_row_count = self.transaction.execute(
                "INSERT OR IGNORE INTO spells (name, description, icon, schools, max_caster_level, cost, metamagics) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    spell.name,
                    trimmed_non_empty(spell.description.as_deref()),
                    trimmed_non_empty(spell.icon.as_deref()),
                    json_string_array(&spell.schools),
                    spell.maximum_caster_level,
                    spell.cost,
                    json_string_array(&spell.metamagics),
                ],
            )?;
            if inserted_row_count == 0 {
                report.skipped_duplicate_spell_count += 1;
                continue;
            }
            let spell_id = self.transaction.last_insert_rowid();
            for (sort_order, damage) in spell.damage_components.iter().enumerate() {
                let base_dice = damage.base_dice();
                let bonus_dice = damage.dice.bonus_dice.as_ref();
                self.transaction.execute(
                    "INSERT INTO spell_damage (spell_id, sort_order, base_dice_number, base_dice_sides, base_dice_bonus, per_caster_levels,
                                               bonus_dice_number, bonus_dice_sides, bonus_dice_bonus, damage, spell_power)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        spell_id,
                        sort_order as i64,
                        base_dice.and_then(|d| d.count),
                        base_dice.and_then(|d| d.sides),
                        base_dice.and_then(|d| d.bonus),
                        damage.dice.per_caster_levels,
                        bonus_dice.and_then(|d| d.count),
                        bonus_dice.and_then(|d| d.sides),
                        bonus_dice.and_then(|d| d.bonus),
                        trimmed_non_empty(damage.damage.as_deref()),
                        trimmed_non_empty(damage.spell_power.as_deref()),
                    ],
                )?;
            }
            for (sort_order, dc) in spell.dcs.iter().enumerate() {
                let amounts = dc.amount.as_ref().map(|v| v.numbers()).transpose().map_err(anyhow::Error::msg)?;
                self.transaction.execute(
                    "INSERT INTO spell_dcs (spell_id, sort_order, dc_type, dc_versus, schools, casting_stat_mod, amount, mod_abilities)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        spell_id,
                        sort_order as i64,
                        trimmed_non_empty(dc.dc_type.as_deref()),
                        trimmed_non_empty(dc.dc_versus.as_deref()),
                        json_string_array(&dc.schools),
                        dc.adds_casting_stat_modifier(),
                        amounts.as_deref().and_then(json_number_array),
                        json_string_array(&dc.modifier_abilities),
                    ],
                )?;
            }
            self.write_abilities(AbilityOwner::Spell, spell_id, &spell.stances, &[], None)?;
            self.write_modifiers(ModifierSource::Spell, spell_id, &spell.effects)?;
            report.spell_count += 1;
        }
        Ok(())
    }

    pub(super) fn link_spell_references(&mut self) -> Result<()> {
        self.transaction.execute(
            "UPDATE class_spells SET spell_id = (SELECT s.id FROM spells s WHERE s.name = class_spells.spell_name) WHERE spell_id IS NULL",
            [],
        )?;
        self.transaction.execute(
            "UPDATE item_clickies SET spell_id = (SELECT s.id FROM spells s WHERE s.name = item_clickies.name)
              WHERE clickie_id IS NULL AND spell_id IS NULL",
            [],
        )?;
        Ok(())
    }
}
