//! Enhancement trees and spells, then the by-name spell references other stages left behind.

use super::{json_numbers, json_strings, nonempty, BuildReport, Ctx};
use crate::xml::spells;
use crate::xml::trees::{self, Selection, TreeItem};
use anyhow::Result;
use ddo_model::enums::{AbilityOwner, ModifierSource, RequirementOwner, TreeKind};
use rusqlite::{params, OptionalExtension};
use std::path::Path;

impl Ctx<'_> {
    pub(super) fn write_tree_file(&mut self, path: &Path, report: &mut BuildReport) -> Result<()> {
        let t = trees::parse(path)?;
        let kind = if t.is_racial {
            TreeKind::Racial
        } else if t.is_destiny {
            TreeKind::Destiny
        } else if t.is_reaper {
            TreeKind::Reaper
        } else if t.is_universal {
            TreeKind::Universal
        } else {
            TreeKind::Class
        };
        let existing: Option<i64> = self
            .tx
            .query_row("SELECT id FROM enhancement_trees WHERE name = ?1", params![t.name], |r| r.get(0))
            .optional()?;
        if existing.is_some() {
            // Upstream keeps a stray duplicate file for one tree; the first definition wins.
            report.duplicate_trees_skipped += 1;
            return Ok(());
        }
        self.tx.execute(
            "INSERT INTO enhancement_trees (name, version, kind, is_legacy, icon, background) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![t.name, t.version, kind.as_str(), t.is_legacy, nonempty(t.icon.as_deref()), nonempty(t.background.as_deref())],
        )?;
        let tree_id = self.tx.last_insert_rowid();
        if let Some(reqs) = &t.requirements {
            self.write_requirements(RequirementOwner::EnhancementTree, tree_id, reqs)?;
        }
        for item in &t.items {
            self.write_tree_item(tree_id, item)?;
            report.enhancements += 1;
        }
        report.enhancement_trees += 1;
        Ok(())
    }

    fn write_tree_item(&mut self, tree_id: i64, i: &TreeItem) -> Result<()> {
        self.tx.execute(
            "INSERT INTO enhancements (tree_id, internal_name, name, description, icon, x, y, cost_per_rank, ranks, min_spent, is_tier5, is_clickie, arrows)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                tree_id,
                i.internal_name,
                i.name,
                nonempty(i.description.as_deref()),
                nonempty(i.icon.as_deref()),
                i.x,
                i.y,
                json_numbers(&i.cost_per_rank),
                i.ranks,
                i.min_spent,
                i.is_tier5,
                i.is_clickie,
                json_strings(&i.arrows),
            ],
        )?;
        let id = self.tx.last_insert_rowid();
        if let Some(reqs) = &i.requirements {
            self.write_requirements(RequirementOwner::Enhancement, id, reqs)?;
        }
        self.write_ability_children(AbilityOwner::Enhancement, id, &i.stances, &i.dcs, i.attack.as_ref())?;
        self.write_modifiers(ModifierSource::Enhancement, id, &i.effects)?;
        if let Some(sel) = &i.selector {
            for ex in &sel.exclusions {
                self.tx.execute(
                    "INSERT OR IGNORE INTO enhancement_selector_exclusions (enhancement_id, internal_name) VALUES (?1, ?2)",
                    params![id, ex],
                )?;
            }
            for (order, s) in sel.selections.iter().enumerate() {
                self.write_selection(id, order as i64, s)?;
            }
        }
        Ok(())
    }

    fn write_selection(&mut self, enhancement_id: i64, order: i64, s: &Selection) -> Result<()> {
        self.tx.execute(
            "INSERT INTO enhancement_selections (enhancement_id, sort_order, name, description, icon, cost_per_rank, ranks, min_spent, is_clickie)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                enhancement_id,
                order,
                s.name,
                nonempty(s.description.as_deref()),
                nonempty(s.icon.as_deref()),
                json_numbers(&s.cost_per_rank),
                s.ranks,
                s.min_spent,
                s.is_clickie,
            ],
        )?;
        let id = self.tx.last_insert_rowid();
        if let Some(reqs) = &s.requirements {
            self.write_requirements(RequirementOwner::EnhancementSelection, id, reqs)?;
        }
        self.write_ability_children(AbilityOwner::EnhancementSelection, id, &s.stances, &s.dcs, s.attack.as_ref())?;
        self.write_modifiers(ModifierSource::EnhancementSelection, id, &s.effects)?;
        Ok(())
    }

    pub(super) fn write_spells(&mut self, path: &Path, report: &mut BuildReport) -> Result<()> {
        if !path.is_file() {
            return Ok(());
        }
        for s in spells::parse(path)? {
            let inserted = self.tx.execute(
                "INSERT OR IGNORE INTO spells (name, description, icon, schools, max_caster_level, cost, metamagics) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    s.name,
                    nonempty(s.description.as_deref()),
                    nonempty(s.icon.as_deref()),
                    json_strings(&s.schools),
                    s.max_caster_level,
                    s.cost,
                    json_strings(&s.metamagics),
                ],
            )?;
            if inserted == 0 {
                report.duplicate_spells_skipped += 1;
                continue;
            }
            let id = self.tx.last_insert_rowid();
            for (order, d) in s.damage.iter().enumerate() {
                let base = d.base_dice();
                let bonus = d.dice.bonus_dice.as_ref();
                self.tx.execute(
                    "INSERT INTO spell_damage (spell_id, sort_order, base_dice_number, base_dice_sides, base_dice_bonus, per_caster_levels,
                                               bonus_dice_number, bonus_dice_sides, bonus_dice_bonus, damage, spell_power)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        id,
                        order as i64,
                        base.and_then(|b| b.number),
                        base.and_then(|b| b.sides),
                        base.and_then(|b| b.bonus),
                        d.dice.per_caster_levels,
                        bonus.and_then(|b| b.number),
                        bonus.and_then(|b| b.sides),
                        bonus.and_then(|b| b.bonus),
                        nonempty(d.damage.as_deref()),
                        nonempty(d.spell_power.as_deref()),
                    ],
                )?;
            }
            for (order, dc) in s.dcs.iter().enumerate() {
                let amount = dc.amount.as_ref().map(|v| v.numbers()).transpose().map_err(anyhow::Error::msg)?;
                self.tx.execute(
                    "INSERT INTO spell_dcs (spell_id, sort_order, dc_type, dc_versus, schools, casting_stat_mod, amount, mod_abilities)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        id,
                        order as i64,
                        nonempty(dc.dc_type.as_deref()),
                        nonempty(dc.dc_versus.as_deref()),
                        json_strings(&dc.schools),
                        dc.casting_stat_mod(),
                        amount.as_deref().and_then(json_numbers),
                        json_strings(&dc.mod_abilities),
                    ],
                )?;
            }
            self.write_ability_children(AbilityOwner::Spell, id, &s.stances, &[], None)?;
            self.write_modifiers(ModifierSource::Spell, id, &s.effects)?;
            report.spells += 1;
        }
        Ok(())
    }

    /// Fill `class_spells.spell_id` and `item_clickies.spell_id` from names, now that spells exist.
    pub(super) fn resolve_spell_references(&mut self) -> Result<()> {
        self.tx.execute(
            "UPDATE class_spells SET spell_id = (SELECT s.id FROM spells s WHERE s.name = class_spells.spell_name) WHERE spell_id IS NULL",
            [],
        )?;
        self.tx.execute(
            "UPDATE item_clickies SET spell_id = (SELECT s.id FROM spells s WHERE s.name = item_clickies.name)
              WHERE clickie_id IS NULL AND spell_id IS NULL",
            [],
        )?;
        Ok(())
    }
}
