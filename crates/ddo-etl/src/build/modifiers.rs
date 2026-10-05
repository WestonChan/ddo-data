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
        for super::PendingDerivedEffects { owner_kind, owner_id, owner_name, option_name, effects } in pending {
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
            let mut mapped_effects = effects;
            let normalized_option_name = option_name.as_deref().map(|name| {
                EFFECT_MAP
                    .option_name_aliases
                    .iter()
                    .fold(name.to_string(), |name, (source, replacement)| name.replace(source, replacement))
            });
            if let Some(option_name) = normalized_option_name.as_deref() {
                for (line_name, companion_targets) in &EFFECT_MAP.option_companion_targets {
                    if option_name.contains(line_name) {
                        for effect in &mut mapped_effects {
                            if effect.types.iter().any(|effect_type| effect_type == "TacticalDC") {
                                effect.targets.retain(|target| !companion_targets.contains(target));
                            }
                        }
                    }
                }
                for (line_name, aliases) in &EFFECT_MAP.option_target_aliases {
                    if option_name.contains(line_name) {
                        for effect in &mut mapped_effects {
                            if effect.types.iter().any(|effect_type| effect_type == "TacticalDC") {
                                for target in &mut effect.targets {
                                    if let Some(alias) = aliases.get(target) {
                                        *target = alias.clone();
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let mut links = self.ensure_derived_effects(&owner, modifier_source, owner_id, &mapped_effects)?;
            if let Some(option_name) = normalized_option_name.as_deref() {
                self.collapse_combat_mastery_option(option_name, &mut links)?;
            }
            for (sort_order, link) in links.into_iter().enumerate() {
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

    fn collapse_combat_mastery_option(&self, option_name: &str, links: &mut Vec<DerivedEffectLink>) -> Result<()> {
        if !option_name.contains("Combat Mastery") {
            return Ok(());
        }
        let source_names = ["Tactics", "Trip DC", "Sunder DC", "Stun DC"];
        let matching_indices: Vec<usize> = links
            .iter()
            .enumerate()
            .filter_map(|(index, link)| {
                self.effects
                    .family(link.effect_id)
                    .filter(|family| source_names.contains(&family.name.as_str()))
                    .map(|_| index)
            })
            .collect();
        let source_set: std::collections::BTreeSet<&str> = matching_indices
            .iter()
            .filter_map(|index| self.effects.family(links[*index].effect_id).map(|family| family.name.as_str()))
            .collect();
        anyhow::ensure!(
            source_set == std::collections::BTreeSet::from(["Tactics"])
                || source_set == std::collections::BTreeSet::from(["Trip DC", "Sunder DC", "Stun DC"]),
            "option {option_name:?} names Combat Mastery but grants {source_set:?}"
        );
        let first_index = matching_indices[0];
        let first_type = links[first_index].bonus_type;
        let first_value = links[first_index].value;
        anyhow::ensure!(
            matching_indices.iter().all(|index| {
                links[*index].bonus_type == first_type
                    && links[*index].value == first_value
                    && links[*index].value2.is_none()
            }),
            "option {option_name:?} names one Combat Mastery line with different amounts or types"
        );
        links[first_index].effect_id =
            self.effects.family_named("Combat Mastery").expect("declared Combat Mastery effect").id;
        for index in matching_indices.into_iter().skip(1).rev() {
            links.remove(index);
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
                if (effect_type == "SpellPower"
                    && (effect.targets.is_empty() || effect.targets.iter().all(|target| target == "All"))
                    && matches!(effect.bonus.as_deref(), Some("Equipment" | "Insight" | "Insightful" | "Quality")))
                    || (effect_type == "UniversalSpellPower"
                        && matches!(effect.bonus.as_deref(), Some("Equipment" | "Insight" | "Insightful" | "Quality")))
                {
                    if let Some(value) = effect.simple_integer_amount() {
                        let bonus_type =
                            self.effect_resolver.parse_bonus_type(effect.bonus.as_deref().unwrap_or(""))?.ok_or_else(
                                || anyhow::anyhow!("{:?} {:?} Potency has no bonus type", owner.kind, owner.name),
                            )?;
                        let effect_id = self
                            .effects
                            .family_named("Potency")
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "Potency family was not written before {:?} {:?}",
                                    owner.kind,
                                    owner.name
                                )
                            })?
                            .id;
                        links.push(DerivedEffectLink {
                            effect_id,
                            bonus_type: Some(bonus_type),
                            value: Some(value),
                            value2: None,
                        });
                        continue;
                    }
                }
                let mut derived_bonuses = self.effect_resolver.derive_bonuses(&single_type_effect)?;
                for rule in EFFECT_MAP.effect.owner_extra_stats.iter().filter(|rule| {
                    rule.owner_kind == "augment"
                        && owner.kind == BonusOwnerKind::Augment
                        && rule.owner_name == owner.name
                        && rule.effect_type == *effect_type
                }) {
                    if let Some(first) = derived_bonuses.first().copied() {
                        derived_bonuses.push(crate::map::effect::DerivedBonus {
                            stat: ddo_model::stats::Stat::by_name(&rule.stat).expect("validated extra stat"),
                            bonus_type: first.bonus_type,
                            value: first.value,
                        });
                    }
                }
                if derived_bonuses.is_empty() {
                    self.ensure_targeted_fixed_effect(&single_type_effect, &mut links)?;
                    continue;
                }
                let all_abilities =
                    effect_type == "AbilityBonus" && effect.targets.iter().any(|target| target == "All");
                let named_shared_target = EFFECT_MAP.effect.targeted.get(effect_type).and_then(|targeted| {
                    effect
                        .targets
                        .iter()
                        .find_map(|target| {
                            targeted.shared_targets.get(target).map(|name| (target.as_str(), name.as_str()))
                        })
                        .or_else(|| {
                            effect
                                .targets
                                .is_empty()
                                .then(|| targeted.shared_targets.get("All").map(|name| ("All", name.as_str())))
                                .flatten()
                        })
                });
                let groups: Vec<(Vec<_>, Option<&str>)> = if all_abilities || effect_type == "SkillBonusAbility" {
                    vec![(derived_bonuses, None)]
                } else if let Some((shared_target, shared_name)) = named_shared_target {
                    let members = &EFFECT_MAP.effect.targeted[effect_type].targets[shared_target];
                    let mut remaining = derived_bonuses;
                    let mut grouped = Vec::new();
                    while let Some(first) = remaining.first().copied() {
                        if members.iter().any(|member| member == first.stat.name) {
                            let shared: Vec<_> = remaining
                                .iter()
                                .copied()
                                .filter(|bonus| members.iter().any(|member| member == bonus.stat.name))
                                .collect();
                            remaining.retain(|bonus| !members.iter().any(|member| member == bonus.stat.name));
                            grouped.push((shared, Some(shared_name)));
                        } else {
                            grouped.push((vec![remaining.remove(0)], None));
                        }
                    }
                    grouped
                } else {
                    derived_bonuses.into_iter().map(|bonus| (vec![bonus], None)).collect()
                };
                for (group_order, (bonuses, shared_name)) in groups.into_iter().enumerate() {
                    let first_bonus = bonuses[0];
                    let target = if all_abilities {
                        Some("All")
                    } else if let Some((shared_target, _)) = named_shared_target {
                        shared_name.map(|_| shared_target).or_else(|| {
                            effect.targets.iter().find_map(|target| {
                                EFFECT_MAP
                                    .effect_stat(effect_type, target)
                                    .filter(|stat| stat.id == first_bonus.stat.id)
                                    .map(|_| target.as_str())
                            })
                        })
                    } else if effect.targets.len() > group_order {
                        Some(effect.targets[group_order].as_str())
                    } else {
                        effect.targets.first().map(String::as_str)
                    };
                    let mut resolved_types = bonuses.iter().map(|bonus| {
                        self.bonus_type_of(
                            &BonusOrigin {
                                owner,
                                source_name: effect_type,
                                stat_name: bonus.stat.name,
                                value: Some(bonus.value),
                            },
                            bonus.bonus_type,
                        )
                    });
                    let bonus_type = resolved_types.next().expect("derived group has a first bonus")?;
                    for resolved_type in resolved_types {
                        anyhow::ensure!(
                            resolved_type? == bonus_type,
                            "{:?} {:?} has different types within one {effect_type} line",
                            owner.kind,
                            owner.name
                        );
                    }
                    if effect_type == "SkillBonusAbility" {
                        let ability = target.expect("resolved skill group has an ability target");
                        let effect_id = self.effects.ensure_group(&format!("{ability} Skills"))?;
                        links.push(DerivedEffectLink {
                            effect_id,
                            bonus_type: Some(bonus_type),
                            value: Some(first_bonus.value),
                            value2: None,
                        });
                        continue;
                    }
                    let source_name = shared_name.map(str::to_string).unwrap_or_else(|| {
                        self.buff_resolver.effect_family_name(effect_type, target, first_bonus.stat.name)
                    });
                    let family_name = if owner.kind == BonusOwnerKind::Augment {
                        EFFECT_MAP.augment_line_name(owner.name, &source_name).unwrap_or(&source_name).to_string()
                    } else {
                        source_name
                    };
                    let existing = self
                        .effects
                        .family_named(&family_name)
                        .filter(|family| !family.verbose_name_template.is_empty())
                        .map(|family| (family.id, family.amount_count, family.uses_link_type));
                    let (effect_id, count, uses_link_type) = match existing {
                        Some(family) => family,
                        None => {
                            let (verbose_name_template, description_template) = if all_abilities {
                                ("%b1 All Ability Scores +{1}".to_string(), Some("{1} %b1 bonus to all Ability Scores"))
                            } else {
                                (format!("%b1 {family_name} +{{1}}"), None)
                            };
                            let effect_id = self.effects.ensure_family(
                                &family_name,
                                &verbose_name_template,
                                description_template,
                                1,
                            )?;
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
            let verbose_name_template = if value.is_some() {
                let (stat_name, targets) = family_name.split_once(" (").expect("qualified name has targets");
                format!("{stat_name} +{{1}}% ({targets}")
            } else {
                family_name.clone()
            };
            let effect_id =
                self.effects.ensure_family(&family_name, &verbose_name_template, None, i64::from(value.is_some()))?;
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
        let verbose_name_template = format!("{stat_name} +{{1}} ({target})");
        let effect_id = self.effects.ensure_family(&family_name, &verbose_name_template, None, 1)?;
        links.push(DerivedEffectLink { effect_id, bonus_type: None, value, value2: None });
        Ok(())
    }
}
