//! The two generic writers: `modifiers` (one row per `<Effect>`) and `requirements` (one row per
//! `<Requirement>`), plus the `bonuses` rows derived from simple effects.

use super::{json_numbers, json_strings, nonempty, Ctx};
use crate::xml::effect::Effect;
use crate::xml::requirements::Requirements;
use anyhow::Result;
use ddo_model::enums::{ModifierSource, RequirementOwner};
use rusqlite::params;

impl Ctx<'_> {
    /// Store every effect faithfully. Returns the new modifier ids in order.
    pub(super) fn write_modifiers(
        &mut self,
        source: ModifierSource,
        source_id: i64,
        effects: &[Effect],
    ) -> Result<Vec<i64>> {
        let mut ids = Vec::with_capacity(effects.len());
        for (sort_order, e) in effects.iter().enumerate() {
            let bonus_type_id = self.effect_map.bonus_type(e.bonus.as_deref().unwrap_or(""))?.map(|b| b.id());
            let dice = e.dice.as_ref();
            self.tx.execute(
                "INSERT INTO modifiers (source_kind, source_id, sort_order, effect_type, extra_types, bonus, bonus_type_id, amount_type,
                                        amounts, targets, value, dice_number, dice_sides, dice_bonus, dice_damage, damage, percent, rank, cap,
                                        stack_source, display_name, apply_as_item_effect, is_item_specific, is_rare)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
                params![
                    source.as_str(),
                    source_id,
                    sort_order as i64,
                    e.types[0],
                    json_strings(&e.types[1..]),
                    nonempty(e.bonus.as_deref()),
                    bonus_type_id,
                    nonempty(e.amount_type.as_deref()),
                    json_numbers(&e.amounts),
                    json_strings(&e.items),
                    nonempty(e.value.as_deref()),
                    dice.and_then(|d| json_numbers(&d.number)),
                    dice.and_then(|d| json_numbers(&d.sides)),
                    dice.and_then(|d| json_numbers(&d.bonus)),
                    dice.and_then(|d| nonempty(d.damage.as_deref())),
                    nonempty(e.damage.as_deref()),
                    e.percent,
                    e.rank,
                    nonempty(e.cap.as_deref()),
                    nonempty(e.stack_source.as_deref()),
                    nonempty(e.display_name.as_deref()),
                    e.apply_as_item_effect,
                    e.is_item_specific,
                    e.rare,
                ],
            )?;
            let id = self.tx.last_insert_rowid();
            if let Some(reqs) = &e.requirements {
                self.write_requirements(RequirementOwner::Modifier, id, reqs)?;
            }
            ids.push(id);
            self.caches.modifiers_written += 1;
        }
        Ok(ids)
    }

    pub(super) fn write_requirements(
        &mut self,
        owner: RequirementOwner,
        owner_id: i64,
        reqs: &Requirements,
    ) -> Result<()> {
        for (group_index, group) in reqs.groups.iter().enumerate() {
            for (sort_order, r) in group.requirements.iter().enumerate() {
                self.tx.execute(
                    "INSERT INTO requirements (owner_kind, owner_id, group_kind, group_index, sort_order, req_type, items, value)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        owner.as_str(),
                        owner_id,
                        group.kind.to_model().as_str(),
                        group_index as i64,
                        sort_order as i64,
                        r.kind,
                        json_strings(&r.items),
                        nonempty(r.value.as_deref()),
                    ],
                )?;
            }
        }
        Ok(())
    }

    /// `bonuses` ids implied by these effects, in effect order, for a junction table.
    pub(super) fn derived_bonus_ids(&mut self, effects: &[Effect]) -> Result<Vec<i64>> {
        let mut ids = Vec::new();
        for e in effects {
            for d in self.effect_map.derive(e)? {
                ids.push(self.bonus_id(d.stat, d.bonus_type, Some(d.value), None, None)?);
            }
        }
        Ok(ids)
    }
}
