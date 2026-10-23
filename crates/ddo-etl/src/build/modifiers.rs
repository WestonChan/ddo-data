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

#[derive(Clone)]
pub(super) struct DerivedEffectLink {
    pub(super) effect_id: i64,
    pub(super) bonus_type: Option<ddo_model::enums::BonusType>,
    pub(super) value: Option<i64>,
    pub(super) value2: Option<i64>,
    pub(super) is_split_line: bool,
}

enum GroupingOutcome {
    Grouped(Vec<DerivedEffectLink>),
    Split { links: Vec<DerivedEffectLink>, correction: String },
    SharedTargets(Vec<String>),
    Held(GroupingHold),
    NotGrouped,
}

enum GroupingHold {
    TypesWithoutStats(Vec<String>),
    DifferentValues,
    DifferentTypes,
    NoUsableGrouping(Vec<String>),
}

impl GroupingHold {
    fn reason(&self) -> String {
        match self {
            Self::TypesWithoutStats(types) => format!("effect types without derived stats: {}", types.join(", ")),
            Self::DifferentValues => "different values within one effect".to_string(),
            Self::DifferentTypes => "different types within one effect".to_string(),
            Self::NoUsableGrouping(names) => format!("no unique usable grouping: {}", names.join(", ")),
        }
    }
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
            let mut derived_by_type = effect
                .types
                .iter()
                .map(|effect_type| {
                    let mut single_type = effect.clone();
                    single_type.types = vec![effect_type.clone()];
                    self.owner_derived_bonuses(owner, &single_type)
                })
                .collect::<Result<Vec<_>>>()?;
            let first_link = links.len();
            let split_count = self.shared_target_splits.len();
            let types_had_stats = derived_by_type.iter().map(|bonuses| !bonuses.is_empty()).collect::<Vec<_>>();
            let source_index = self.derived_effect_lines.len();
            let outcome = self.write_grouped_effect_links(owner, owner_id, effect, &mut derived_by_type)?;
            let stat_names = derived_by_type
                .iter()
                .flatten()
                .map(|bonus| bonus.stat.name.to_string())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            self.derived_effect_lines.push(super::DerivedEffectLines {
                owner_kind: modifier_source.as_str().to_string(),
                owner_name: owner.name.to_string(),
                stat_names: stat_names.clone(),
                link_count: 0,
            });
            let split = |reason| super::DerivedEffectSplit {
                source_index,
                owner_kind: modifier_source.as_str().to_string(),
                owner_name: owner.name.to_string(),
                owner_id,
                effect_types: effect.types.clone(),
                targets: effect.targets.clone(),
                stats: stat_names.clone(),
                reason,
            };
            let preserve_member_lines =
                matches!(outcome, GroupingOutcome::Held(GroupingHold::DifferentTypes | GroupingHold::DifferentValues));
            let grouped_links = match outcome {
                GroupingOutcome::Grouped(links) => Some(links),
                GroupingOutcome::Split { links, correction } => {
                    self.line_splits.push(split(correction));
                    Some(links)
                }
                GroupingOutcome::SharedTargets(entries) => {
                    self.shared_target_splits.push(split(entries.join(", ")));
                    None
                }
                GroupingOutcome::Held(hold) => {
                    self.held_back_effects.push(split(hold.reason()));
                    None
                }
                GroupingOutcome::NotGrouped => None,
            };
            if let Some(grouped_links) = grouped_links {
                if grouped_links.len() == 1 {
                    self.transaction.execute(
                        "UPDATE modifiers SET effect_id = ?4 WHERE source_kind = ?1 AND source_id = ?2 AND sort_order = ?3",
                        params![modifier_source.as_str(), owner_id, source_order as i64, grouped_links[0].effect_id],
                    )?;
                }
                self.derived_effect_lines[source_index].link_count = grouped_links.len();
                links.extend(grouped_links);
                continue;
            }
            for (type_index, effect_type) in effect.types.iter().enumerate() {
                if types_had_stats[type_index] && derived_by_type[type_index].is_empty() {
                    continue;
                }
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
                            is_split_line: false,
                            effect_id,
                            bonus_type: Some(bonus_type),
                            value: Some(value),
                            value2: None,
                        });
                        continue;
                    }
                }
                let derived_bonuses = derived_by_type[type_index].clone();
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
                let groups: Vec<(Vec<_>, Option<&str>)> = if preserve_member_lines {
                    derived_bonuses.into_iter().map(|bonus| (vec![bonus], None)).collect()
                } else if all_abilities || effect_type == "SkillBonusAbility" {
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
                    let target = if preserve_member_lines {
                        if all_abilities || effect_type == "SkillBonusAbility" {
                            None
                        } else {
                            effect.targets.iter().find_map(|target| {
                                EFFECT_MAP
                                    .effect_stat(effect_type, target)
                                    .filter(|stat| stat.id == first_bonus.stat.id)
                                    .map(|_| target.as_str())
                            })
                        }
                    } else if all_abilities {
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
                    let line_name = shared_name
                        .or_else(|| (preserve_member_lines && target.is_none()).then_some(first_bonus.stat.name));
                    links.push(self.write_derived_line(owner, effect_type, target, &bonuses, line_name)?);
                }
            }
            self.derived_effect_lines[source_index].link_count = links.len() - first_link;
            if links.len() - first_link <= 1 {
                self.shared_target_splits.truncate(split_count);
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

    fn owner_derived_bonuses(
        &self,
        owner: &BonusOwner,
        effect: &Effect,
    ) -> Result<Vec<crate::map::effect::DerivedBonus>> {
        let mut derived_bonuses = self.effect_resolver.derive_bonuses(effect)?;
        for rule in EFFECT_MAP.effect.owner_extra_stats.iter().filter(|rule| {
            rule.owner_kind == "augment"
                && owner.kind == BonusOwnerKind::Augment
                && rule.owner_name == owner.name
                && rule.effect_type == effect.types[0]
        }) {
            if let Some(first) = derived_bonuses.first().copied() {
                derived_bonuses.push(crate::map::effect::DerivedBonus {
                    stat: ddo_model::stats::Stat::by_name(&rule.stat).expect("validated extra stat"),
                    bonus_type: first.bonus_type,
                    value: first.value,
                });
            }
        }
        Ok(derived_bonuses)
    }

    fn write_derived_line(
        &mut self,
        owner: &BonusOwner,
        effect_type: &str,
        target: Option<&str>,
        bonuses: &[crate::map::effect::DerivedBonus],
        line_name: Option<&str>,
    ) -> Result<DerivedEffectLink> {
        let first_bonus = bonuses[0];
        let all_abilities = bonuses.len() > 1 && effect_type == "AbilityBonus" && target == Some("All");
        let mut resolved_types = bonuses.iter().map(|bonus| {
            self.bonus_type_of(
                &BonusOrigin { owner, source_name: effect_type, stat_name: bonus.stat.name, value: Some(bonus.value) },
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
        if let ("SkillBonusAbility", Some(ability)) = (effect_type, target) {
            let complete_skill_group = EFFECT_MAP.skill_ability_groups.skills.get(ability).is_some_and(|members| {
                members.iter().all(|member| bonuses.iter().any(|bonus| bonus.stat.name == member))
                    && bonuses.iter().all(|bonus| members.iter().any(|member| member == bonus.stat.name))
            });
            if complete_skill_group {
                let effect_id = self.effects.ensure_group(&format!("{ability} Skills"))?;
                return Ok(DerivedEffectLink {
                    is_split_line: false,
                    effect_id,
                    bonus_type: Some(bonus_type),
                    value: Some(first_bonus.value),
                    value2: None,
                });
            }
        }
        let source_name = line_name.map(str::to_string).unwrap_or_else(|| {
            if effect_type == "SkillBonusAbility" && bonuses.len() == 1 {
                first_bonus.stat.name.to_string()
            } else {
                self.buff_resolver.effect_tier_name(effect_type, target, first_bonus.stat.name)
            }
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
                let effect_id =
                    self.effects.ensure_family(&family_name, &verbose_name_template, description_template, 1)?;
                (effect_id, 1, true)
            }
        };
        let stat_bonus_type = (!uses_link_type).then_some(bonus_type);
        let existing_stat =
            self.effects.stat(effect_id, first_bonus.stat.id, stat_bonus_type.map(ddo_model::enums::BonusType::id));
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
        Ok(DerivedEffectLink {
            is_split_line: false,
            effect_id,
            bonus_type: (uses_link_type || self.effects.family(effect_id).is_some_and(|family| family.is_stat))
                .then_some(bonus_type),
            value,
            value2,
        })
    }

    fn write_grouped_effect_links(
        &mut self,
        owner: &BonusOwner,
        owner_id: i64,
        effect: &Effect,
        derived_by_type: &mut [Vec<crate::map::effect::DerivedBonus>],
    ) -> Result<GroupingOutcome> {
        let types_without_stats = effect
            .types
            .iter()
            .zip(derived_by_type.iter())
            .filter(|(_, bonuses)| bonuses.is_empty())
            .map(|(effect_type, _)| effect_type.clone())
            .collect::<Vec<_>>();
        let original_stats =
            derived_by_type.iter().flatten().map(|bonus| bonus.stat.id).collect::<std::collections::BTreeSet<_>>();
        if original_stats.len() > 1 {
            let removals = self
                .bonus_removal_corrections
                .iter()
                .copied()
                .filter(|correction| {
                    correction.name == owner.name
                        && match owner.kind {
                            BonusOwnerKind::SetBonusTier => {
                                correction.kind == ddo_model::enums::CorrectionKind::SetTierBonus
                            }
                            BonusOwnerKind::Augment => {
                                correction.kind == ddo_model::enums::CorrectionKind::AugmentBonus
                            }
                            _ => false,
                        }
                })
                .collect::<Vec<_>>();
            for correction in removals {
                if !super::corrections::removal_owner_matches(self.transaction, correction, owner_id)? {
                    continue;
                }
                let mut resolved_bonuses = Vec::new();
                for (effect_type, derived) in effect.types.iter().zip(derived_by_type.iter()) {
                    for bonus in derived {
                        let bonus_type = self.bonus_type_of(
                            &BonusOrigin {
                                owner,
                                source_name: effect_type,
                                stat_name: bonus.stat.name,
                                value: Some(bonus.value),
                            },
                            bonus.bonus_type,
                        )?;
                        resolved_bonuses.push((bonus.stat.id, bonus_type.id(), bonus.value));
                    }
                }
                let removed = super::corrections::removed_derived_bonus_indices(
                    self.transaction,
                    correction,
                    owner_id,
                    &resolved_bonuses,
                )?;
                if !removed.is_empty() {
                    self.removed_derived_bonus_count += removed.len();
                    if !self.applied_bonus_removal_corrections.iter().any(|applied| std::ptr::eq(*applied, correction))
                    {
                        self.applied_bonus_removal_corrections.push(correction);
                    }
                    let mut index = 0;
                    for derived in derived_by_type.iter_mut() {
                        derived.retain(|_| {
                            let survives = !removed.contains(&index);
                            index += 1;
                            survives
                        });
                    }
                }
            }
        }
        let mut bonuses = Vec::new();
        let mut shared_targets = Vec::new();
        for (effect_type, derived) in effect.types.iter().zip(derived_by_type.iter()) {
            if let Some(targeted) = EFFECT_MAP.effect.targeted.get(effect_type) {
                let targets = if effect.targets.is_empty() {
                    vec!["All"]
                } else {
                    effect.targets.iter().map(String::as_str).collect()
                };
                for target in targets {
                    if targeted.shared_targets.contains_key(target) {
                        shared_targets.push(format!("effect.targeted.{effect_type}.shared_targets.{target}"));
                    }
                }
            }
            bonuses.extend(derived.iter().copied().map(|bonus| (effect_type.as_str(), bonus)));
        }
        let stats =
            bonuses.iter().map(|(_, bonus)| bonus.stat.name.to_string()).collect::<std::collections::BTreeSet<_>>();
        if stats.len() < 2 {
            return Ok(GroupingOutcome::NotGrouped);
        }
        if !types_without_stats.is_empty() {
            return Ok(GroupingOutcome::Held(GroupingHold::TypesWithoutStats(types_without_stats)));
        }
        let value = bonuses[0].1.value;
        if bonuses.iter().any(|(_, bonus)| bonus.value != value) {
            return Ok(GroupingOutcome::Held(GroupingHold::DifferentValues));
        }
        let mut bonus_type = None;
        for (effect_type, bonus) in &bonuses {
            let resolved = self.bonus_type_of(
                &BonusOrigin { owner, source_name: effect_type, stat_name: bonus.stat.name, value: Some(bonus.value) },
                bonus.bonus_type,
            )?;
            if bonus_type.is_some_and(|previous| previous != resolved) {
                return Ok(GroupingOutcome::Held(GroupingHold::DifferentTypes));
            }
            bonus_type = Some(resolved);
        }
        if !shared_targets.is_empty() {
            return Ok(GroupingOutcome::SharedTargets(shared_targets));
        }
        let bonus_type = bonus_type.expect("multi-stat effect has a bonus type");
        let matching = self.groupings.by_stat_set.get(&stats).map(Vec::as_slice).unwrap_or_default();
        if let [grouping] = matching {
            if grouping.accepts_value_at_type(bonus_type.name()) {
                let split_corrections = self
                    .line_corrections
                    .iter()
                    .copied()
                    .filter(|correction| {
                        correction.name == owner.name
                            && correction.from.as_text() == Some(grouping.name.as_str())
                            && correction.bonus_type.as_deref() == Some(bonus_type.name())
                            && matches!(
                                (owner.kind, correction.kind),
                                (BonusOwnerKind::Augment, ddo_model::enums::CorrectionKind::AugmentBonus)
                                    | (BonusOwnerKind::SetBonusTier, ddo_model::enums::CorrectionKind::SetTierBonus)
                            )
                    })
                    .collect::<Vec<_>>();
                for correction in split_corrections {
                    if !super::corrections::removal_owner_matches(self.transaction, correction, owner_id)? {
                        continue;
                    }
                    let mut partitions = correction.line_partition_stats(self.groupings)?;
                    self.effects.ensure_indexed_grouping(grouping, self.groupings)?;
                    partitions.sort_by_key(|(_, partition)| {
                        bonuses.iter().position(|(_, bonus)| partition.contains(bonus.stat.name))
                    });
                    let mut split_links = Vec::new();
                    for (line_name, partition) in partitions {
                        let mut link = if partition.len() == 1 {
                            let (effect_type, bonus) = bonuses
                                .iter()
                                .find(|(_, bonus)| partition.contains(bonus.stat.name))
                                .expect("validated split member");
                            let target = effect
                                .targets
                                .iter()
                                .find(|target| {
                                    EFFECT_MAP
                                        .effect_stat(effect_type, target)
                                        .is_some_and(|stat| stat.id == bonus.stat.id)
                                })
                                .map(String::as_str);
                            self.write_derived_line(owner, effect_type, target, &[*bonus], None)?
                        } else {
                            let line_grouping = self.groupings.grouping_named(&line_name).ok_or_else(|| {
                                anyhow::anyhow!(
                                    "{}: split line {line_name:?} is not a link-valued grouping",
                                    correction.label()
                                )
                            })?;
                            let effect_id = self.effects.ensure_indexed_grouping(line_grouping, self.groupings)?;
                            self.write_upstream_grouping_line(owner, effect_id, bonus_type, value)?
                        };
                        link.is_split_line = true;
                        split_links.push(link);
                    }
                    if !self.applied_line_corrections.iter().any(|applied| std::ptr::eq(*applied, correction)) {
                        self.applied_line_corrections.push(correction);
                    }
                    return Ok(GroupingOutcome::Split { links: split_links, correction: correction.label() });
                }
            }
        }
        let effect_id = match matching {
            [grouping] if grouping.accepts_value_at_type(bonus_type.name()) => {
                self.effects.ensure_indexed_grouping(grouping, self.groupings)?
            }
            [] => {
                let declared = EFFECT_MAP
                    .group_names()
                    .into_iter()
                    .filter(|name| self.groupings.effect_named(name).is_none_or(|effect| effect.is_group))
                    .find(|name| {
                        EFFECT_MAP
                            .group_members(name)
                            .unwrap()
                            .iter()
                            .cloned()
                            .collect::<std::collections::BTreeSet<_>>()
                            == stats
                    });
                if let Some(name) = declared {
                    let id = self.effects.ensure_group(&name)?;
                    self.effects.fill_group_description(id, &format!("+{{1}} %b1 bonus to {name}."))?;
                    id
                } else {
                    let name = crate::groupings::name_from_stats(&stats)?;
                    anyhow::ensure!(
                        self.effects.family_named(&name).is_none_or(|family| family.is_group),
                        "new stat group {name:?} collides with an existing named effect"
                    );
                    let percent_stats = stats
                        .iter()
                        .filter(|name| {
                            self.groupings
                                .effect_named(name)
                                .map(|effect| effect.verbose_name_template.as_str())
                                .filter(|template| !template.is_empty())
                                .or_else(|| EFFECT_MAP.line_templates.get(name.as_str()).map(String::as_str))
                                .is_some_and(|template| {
                                    ddo_model::effect_amount::template_slot_states_percent(template, 1)
                                })
                        })
                        .cloned()
                        .collect::<std::collections::BTreeSet<_>>();
                    let members = ddo_model::stats::STATS
                        .iter()
                        .filter(|stat| stats.contains(stat.name))
                        .map(|stat| stat.name.to_string())
                        .collect::<Vec<_>>();
                    let suffix = if percent_stats.len() == stats.len() { "%" } else { "" };
                    let mixed_units = !percent_stats.is_empty() && percent_stats.len() != stats.len();
                    let template = if mixed_units {
                        members
                            .iter()
                            .map(|member| {
                                let unit = if percent_stats.contains(member) { "%" } else { "" };
                                format!("+{{1}}{unit} %b1 Bonus to {member}")
                            })
                            .collect::<Vec<_>>()
                            .join(" and ")
                    } else {
                        format!("%b1 {name} +{{1}}{suffix}")
                    };
                    let description = if mixed_units {
                        format!("{}.", template.replace("Bonus to", "bonus to"))
                    } else {
                        format!("+{{1}}{suffix} %b1 bonus to {name}.")
                    };
                    self.effects.ensure_flat_group(
                        &name,
                        &members,
                        &template,
                        Some(&description),
                        mixed_units.then_some(template.as_str()),
                    )?
                }
            }
            _ => {
                return Ok(GroupingOutcome::Held(GroupingHold::NoUsableGrouping(
                    matching.iter().map(|grouping| grouping.name.clone()).collect(),
                )));
            }
        };
        Ok(GroupingOutcome::Grouped(vec![self.write_upstream_grouping_line(owner, effect_id, bonus_type, value)?]))
    }

    pub(super) fn write_merged_lines(
        &mut self,
        owner: &BonusOwner,
        owner_id: i64,
        mut links: Vec<DerivedEffectLink>,
    ) -> Result<Vec<DerivedEffectLink>> {
        use ddo_model::enums::CorrectionKind;
        let kind = match owner.kind {
            BonusOwnerKind::Item => CorrectionKind::ItemBonus,
            BonusOwnerKind::Augment => CorrectionKind::AugmentBonus,
            BonusOwnerKind::SetBonusTier => CorrectionKind::SetTierBonus,
            _ => return Ok(links),
        };
        let corrections = self
            .line_corrections
            .iter()
            .copied()
            .filter(|correction| {
                correction.kind == kind
                    && correction.name == owner.name
                    && matches!(correction.from, crate::corrections::CorrectionValue::Texts(_))
            })
            .collect::<Vec<_>>();
        if corrections.is_empty() {
            return Ok(links);
        }
        let rows = super::corrections::line_bonus_rows(self.transaction, kind, owner_id, Some(&links))?;
        let mut consumed = std::collections::BTreeSet::new();
        let mut removed = std::collections::BTreeSet::new();
        for correction in corrections {
            if !super::corrections::removal_owner_matches(self.transaction, correction, owner_id)? {
                continue;
            }
            correction.line_partition_stats(self.groupings)?;
            let Some(selection) = super::corrections::line_merge_selection(correction, &rows)? else { continue };
            anyhow::ensure!(
                selection.orders.is_disjoint(&consumed),
                "{}: overlapping merge corrections",
                correction.label()
            );
            anyhow::ensure!(
                selection.orders.iter().all(|order| !links[*order].is_split_line),
                "{}: a merge and a split cannot apply to one line",
                correction.label()
            );
            let grouping_name = correction.stat.as_deref().expect("validated grouping qualifier");
            let grouping = self
                .groupings
                .grouping_named(grouping_name)
                .ok_or_else(|| anyhow::anyhow!("{}: merge target is not a link-valued grouping", correction.label()))?;
            let bonus_type = correction
                .bonus_type
                .as_deref()
                .and_then(ddo_model::enums::BonusType::parse)
                .ok_or_else(|| anyhow::anyhow!("{}: merged line needs a named type", correction.label()))?;
            anyhow::ensure!(
                selection.stats == self.groupings.expanded_stat_names(grouping_name)?,
                "{}: merged line has different stats",
                correction.label()
            );
            let effect_id = self.effects.ensure_indexed_grouping(grouping, self.groupings)?;
            let first = *selection.orders.first().expect("merge has at least two lines");
            links[first] = self.write_grouping_line(effect_id, bonus_type, selection.amount)?;
            removed.extend(selection.orders.iter().copied().filter(|order| *order != first));
            consumed.extend(selection.orders);
            self.applied_line_corrections.push(correction);
        }
        Ok(links
            .into_iter()
            .enumerate()
            .filter_map(|(order, link)| (!removed.contains(&order)).then_some(link))
            .collect())
    }

    fn write_upstream_grouping_line(
        &mut self,
        owner: &BonusOwner,
        effect_id: i64,
        bonus_type: ddo_model::enums::BonusType,
        value: i64,
    ) -> Result<DerivedEffectLink> {
        let name = self.effects.family(effect_id).expect("grouping cached").name.clone();
        let effect_id = if owner.kind == BonusOwnerKind::Augment {
            if let Some(line_name) = EFFECT_MAP.augment_line_name(owner.name, &name) {
                self.effects
                    .family_named(line_name)
                    .ok_or_else(|| anyhow::anyhow!("augment line {line_name:?} is not cached"))?
                    .id
            } else {
                effect_id
            }
        } else {
            effect_id
        };
        self.write_grouping_line(effect_id, bonus_type, value)
    }

    fn write_grouping_line(
        &mut self,
        effect_id: i64,
        bonus_type: ddo_model::enums::BonusType,
        value: i64,
    ) -> Result<DerivedEffectLink> {
        self.grouping_answers.insert(effect_id);
        let family = self.effects.family(effect_id).expect("grouping cached");
        Ok(DerivedEffectLink {
            is_split_line: false,
            effect_id,
            bonus_type: family.uses_link_type.then_some(bonus_type),
            value: Some(value),
            value2: None,
        })
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
            links.push(DerivedEffectLink { is_split_line: false, effect_id, bonus_type: None, value, value2: None });
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
            links.push(DerivedEffectLink {
                is_split_line: false,
                effect_id,
                bonus_type: None,
                value: None,
                value2: None,
            });
            return Ok(());
        }
        let value = effect.simple_integer_amount();
        let verbose_name_template = format!("{stat_name} +{{1}} ({target})");
        let effect_id = self.effects.ensure_family(&family_name, &verbose_name_template, None, 1)?;
        links.push(DerivedEffectLink { is_split_line: false, effect_id, bonus_type: None, value, value2: None });
        Ok(())
    }
}
