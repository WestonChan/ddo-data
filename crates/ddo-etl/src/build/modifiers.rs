use super::bonus_types::BonusOwnerKind;
use super::bonus_types::{BonusOrigin, BonusOwner};
use super::effects::{BonusRule, EffectOwner};
use super::{json_number_array, json_string_array, trimmed_non_empty, TableWriter};
use crate::map::effect_map::EFFECT_MAP;
use crate::xml::effect::Effect;
use crate::xml::requirements::Requirements;
use anyhow::Result;
use ddo_model::enums::{ModifierSource, RequirementOwner};
use rusqlite::params;

pub(super) struct DerivedEffectLink {
    pub(super) effect_id: i64,
    pub(super) bonus_type: Option<ddo_model::enums::BonusType>,
    pub(super) value: Option<i64>,
    pub(super) value2: Option<i64>,
}

impl TableWriter<'_> {
    pub(super) fn link_pending_derived_effects(&mut self) -> Result<()> {
        let pending = std::mem::take(&mut self.pending_derived_effects);
        for (owner_kind, owner_id, owner_name, effects) in pending {
            let bonus_owner_kind = match owner_kind {
                EffectOwner::Feat => BonusOwnerKind::Feat,
                EffectOwner::ItemAugmentSlotOption => BonusOwnerKind::ItemAugmentSlotOption,
                _ => unreachable!(),
            };
            let owner = BonusOwner { kind: bonus_owner_kind, name: &owner_name, family: None };
            let modifier_source = match owner_kind {
                EffectOwner::Feat => ModifierSource::Feat,
                EffectOwner::ItemAugmentSlotOption => ModifierSource::ItemAugmentSlotOption,
                _ => unreachable!(),
            };
            for (sort_order, link) in
                self.ensure_derived_effects(&owner, modifier_source, owner_id, &effects)?.into_iter().enumerate()
            {
                self.effects.insert_link(
                    owner_kind,
                    owner_id,
                    link.effect_id,
                    link.bonus_type,
                    (link.value, link.value2),
                    sort_order,
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn write_modifiers(&mut self, source: ModifierSource, source_id: i64, effects: &[Effect]) -> Result<()> {
        for (sort_order, effect) in effects.iter().enumerate() {
            let bonus_type_id =
                self.effect_resolver.parse_bonus_type(effect.bonus.as_deref().unwrap_or(""))?.map(|b| b.id());
            let dice = effect.dice.as_ref();
            self.transaction.execute(
                "INSERT INTO modifiers (source_kind, source_id, sort_order, effect_type, extra_types, bonus, bonus_type_id, amount_type,
                                        amounts, targets, value, dice_number, dice_sides, dice_bonus, dice_damage, damage, percent, rank, cap,
                                        stack_source, display_name, apply_as_item_effect, is_item_specific, is_rare)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
                params![
                    source.as_str(),
                    source_id,
                    sort_order as i64,
                    effect.types[0],
                    json_string_array(&effect.types[1..]),
                    trimmed_non_empty(effect.bonus.as_deref()),
                    bonus_type_id,
                    trimmed_non_empty(effect.amount_type.as_deref()),
                    json_number_array(&effect.amounts),
                    json_string_array(&effect.targets),
                    trimmed_non_empty(effect.value.as_deref()),
                    dice.and_then(|d| json_number_array(&d.counts)),
                    dice.and_then(|d| json_number_array(&d.sides)),
                    dice.and_then(|d| json_number_array(&d.bonuses)),
                    dice.and_then(|d| trimmed_non_empty(d.damage.as_deref())),
                    trimmed_non_empty(effect.damage.as_deref()),
                    effect.is_percent,
                    effect.rank,
                    trimmed_non_empty(effect.cap.as_deref()),
                    trimmed_non_empty(effect.stack_source.as_deref()),
                    trimmed_non_empty(effect.display_name.as_deref()),
                    effect.applies_as_item_effect,
                    effect.is_item_specific,
                    effect.is_rare,
                ],
            )?;
            let modifier_id = self.transaction.last_insert_rowid();
            if let Some(requirements) = &effect.requirements {
                self.write_requirements(RequirementOwner::Modifier, modifier_id, requirements)?;
            }
            self.written.modifier_count += 1;
        }
        Ok(())
    }

    pub(super) fn write_requirements(
        &mut self,
        owner: RequirementOwner,
        owner_id: i64,
        requirements: &Requirements,
    ) -> Result<()> {
        for (group_index, group) in requirements.groups.iter().enumerate() {
            for (sort_order, requirement) in group.requirements.iter().enumerate() {
                self.transaction.execute(
                    "INSERT INTO requirements (owner_kind, owner_id, group_kind, group_index, sort_order, req_type, items, value)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        owner.as_str(),
                        owner_id,
                        group.kind.as_str(),
                        group_index as i64,
                        sort_order as i64,
                        requirement.kind,
                        json_string_array(&requirement.items),
                        trimmed_non_empty(requirement.value.as_deref()),
                    ],
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn ensure_derived_effects(
        &mut self,
        owner: &BonusOwner,
        modifier_source: ModifierSource,
        owner_id: i64,
        effects: &[Effect],
    ) -> Result<Vec<DerivedEffectLink>> {
        let mut links = Vec::new();
        for (source_order, effect) in effects.iter().enumerate() {
            let first_link = links.len();
            for effect_type in &effect.types {
                let mut single_type_effect = effect.clone();
                single_type_effect.types = vec![effect_type.clone()];
                let derived_bonuses = self.effect_resolver.derive_bonuses(&single_type_effect)?;
                if derived_bonuses.is_empty() {
                    self.ensure_targeted_fixed_effect(&single_type_effect, &mut links)?;
                    continue;
                }
                let all_abilities =
                    effect_type == "AbilityBonus" && effect.targets.iter().any(|target| target == "All");
                let shared_target = all_abilities || effect_type == "SkillBonusAbility";
                let groups: Vec<Vec<_>> = if shared_target {
                    vec![derived_bonuses]
                } else {
                    derived_bonuses.into_iter().map(|bonus| vec![bonus]).collect()
                };
                for (group_order, bonuses) in groups.into_iter().enumerate() {
                    let first_bonus = bonuses[0];
                    let target = if all_abilities {
                        Some("All")
                    } else if effect.targets.len() > group_order {
                        Some(effect.targets[group_order].as_str())
                    } else {
                        effect.targets.first().map(String::as_str)
                    };
                    let family_name = self.buff_resolver.effect_family_name(effect_type, target, first_bonus.stat.name);
                    let bonus_origin = BonusOrigin {
                        owner,
                        source_name: effect_type,
                        stat_name: first_bonus.stat.name,
                        value: Some(first_bonus.value),
                    };
                    let bonus_type = self.bonus_type_of(&bonus_origin, first_bonus.bonus_type)?;
                    let existing = self
                        .effects
                        .family_named(&family_name)
                        .filter(|family| !family.text_template.is_empty())
                        .map(|family| (family.id, family.amount_count, family.uses_link_type));
                    let (effect_id, count, uses_link_type) = match existing {
                        Some(family) => family,
                        None => {
                            let (text_template, description_template) = if all_abilities {
                                ("%b1 All Ability Scores +{1}".to_string(), Some("{1} %b1 bonus to all Ability Scores"))
                            } else {
                                (format!("%b1 {family_name} +{{1}}"), None)
                            };
                            let effect_id =
                                self.effects.ensure_family(&family_name, &text_template, description_template, 1)?;
                            (effect_id, 1, true)
                        }
                    };
                    let stat_bonus_type = (!uses_link_type).then_some(bonus_type);
                    let existing_stat = self.effects.stat(
                        effect_id,
                        first_bonus.stat.id,
                        stat_bonus_type.map(ddo_model::enums::BonusType::id),
                    );
                    let (amount_from, constant) =
                        existing_stat.unwrap_or(if count == 0 { (0, Some(first_bonus.value)) } else { (1, None) });
                    let rules: Vec<BonusRule> = bonuses
                        .iter()
                        .map(|bonus| BonusRule { stat: bonus.stat, bonus_type: stat_bonus_type, amount_from, constant })
                        .collect();
                    self.effects.ensure_bonus_rules(effect_id, &rules)?;
                    let (value, value2) = match amount_from {
                        0 => (None, None),
                        1 => (Some(first_bonus.value), None),
                        2 => (Some(first_bonus.value), Some(first_bonus.value)),
                        _ => unreachable!(),
                    };
                    links.push(DerivedEffectLink {
                        effect_id,
                        bonus_type: (uses_link_type
                            || self.effects.family(effect_id).is_some_and(|family| family.is_stat))
                        .then_some(bonus_type),
                        value,
                        value2,
                    });
                }
            }
            if let Some(first) = links.get(first_link) {
                if links[first_link..].iter().all(|link| link.effect_id == first.effect_id) {
                    self.transaction.execute(
                        "UPDATE modifiers SET effect_id = ?4 WHERE source_kind = ?1 AND source_id = ?2 AND sort_order = ?3",
                        params![modifier_source.as_str(), owner_id, source_order as i64, first.effect_id],
                    )?;
                }
            }
        }
        Ok(links)
    }

    fn ensure_targeted_fixed_effect(&mut self, effect: &Effect, links: &mut Vec<DerivedEffectLink>) -> Result<()> {
        if let Some(family_name) = self.effect_resolver.qualified_targeted_name(effect) {
            let value = effect.simple_integer_amount();
            let text_template = if value.is_some() {
                let (stat_name, targets) = family_name.split_once(" (").expect("qualified name has targets");
                format!("{stat_name} +{{1}}% ({targets}")
            } else {
                family_name.clone()
            };
            let effect_id =
                self.effects.ensure_family(&family_name, &text_template, None, i64::from(value.is_some()))?;
            links.push(DerivedEffectLink { effect_id, bonus_type: None, value, value2: None });
            return Ok(());
        }
        let Some(stat_name) = EFFECT_MAP.effect.fixed.get(&effect.types[0]) else {
            return Ok(());
        };
        let Some(target) = self.effect_resolver.qualified_targets(effect) else {
            return Ok(());
        };
        let family_name = format!("{stat_name} ({target})");
        if effect.types[0] == "Weapon_CriticalRange" && effect.amount_type.as_deref() == Some("Stacks") {
            let effect_id = self.effects.ensure_family(&family_name, &family_name, None, 0)?;
            links.push(DerivedEffectLink { effect_id, bonus_type: None, value: None, value2: None });
            return Ok(());
        }
        let value = effect.simple_integer_amount();
        let text_template = format!("{stat_name} +{{1}} ({target})");
        let effect_id = self.effects.ensure_family(&family_name, &text_template, None, 1)?;
        links.push(DerivedEffectLink { effect_id, bonus_type: None, value, value2: None });
        Ok(())
    }
}
