use super::bonus_types::BonusOwnerKind;
use super::bonus_types::{BonusOrigin, BonusOwner};
use super::effects::{BonusRule, EffectOwner};
use super::{json_number_array, json_string_array, trimmed_non_empty, TableWriter};
use crate::map::effect_map::EFFECT_MAP;
use crate::xml::effect::Effect;
use crate::xml::requirements::Requirements;
use anyhow::{Context, Result};
use ddo_model::enums::{ModifierSource, RequirementOwner};
use rusqlite::params;

const STAT_ROUTING_OWNERS: [EffectOwner; 3] =
    [EffectOwner::Item, EffectOwner::Augment, EffectOwner::ItemAugmentSlotOption];

pub struct StatLinkRoute {
    pub table: &'static str,
    pub owner_id: i64,
    pub sort_order: i64,
    pub stat: String,
    pub bonus_type: String,
    pub effect: String,
    owner_column: &'static str,
}

pub fn stat_links_at_routed_types(db: &rusqlite::Connection) -> Result<Vec<StatLinkRoute>> {
    let queries = STAT_ROUTING_OWNERS.iter().enumerate().map(|(position, owner)| {
        let (table, column) = owner.table();
        format!("SELECT {position}, l.{column}, l.sort_order, e.name, b.name FROM {table} l JOIN effects e ON e.id=l.effect_id JOIN bonus_types b ON b.id=l.bonus_type_id WHERE e.is_stat=1 AND l.alias IS NULL")
    }).collect::<Vec<_>>().join(" UNION ALL ");
    let mut statement = db.prepare(&queries)?;
    let mut rows = statement.query([])?;
    let mut routes = Vec::new();
    while let Some(row) = rows.next()? {
        let stat: String = row.get(3)?;
        let bonus_type: String = row.get(4)?;
        let Some(effect) = EFFECT_MAP.stat_type_effects.get(&stat).and_then(|types| types.get(&bonus_type)) else {
            continue;
        };
        let owner_index = usize::try_from(row.get::<_, i64>(0)?)?;
        let (table, owner_column) = STAT_ROUTING_OWNERS[owner_index].table();
        routes.push(StatLinkRoute {
            table,
            owner_column,
            owner_id: row.get(1)?,
            sort_order: row.get(2)?,
            stat,
            bonus_type,
            effect: effect.clone(),
        });
    }
    Ok(routes)
}

#[derive(Clone)]
pub(super) struct DerivedEffectLink {
    pub(super) alias: Option<String>,
    pub(super) effect_id: i64,
    pub(super) bonus_type: Option<ddo_model::enums::BonusType>,
    pub(super) value: Option<i64>,
    pub(super) value2: Option<i64>,
    pub(super) is_split_line: bool,
}

pub(super) struct PendingEffectLinks {
    kind: BonusOwnerKind,
    name: String,
    owner_id: i64,
    links: Vec<DerivedEffectLink>,
}

fn derived_bonus_link(bonus: &crate::map::effect::DerivedBonus) -> DerivedEffectLink {
    DerivedEffectLink {
        effect_id: bonus.stat.id,
        alias: None,
        bonus_type: bonus.bonus_type,
        value: Some(bonus.value),
        value2: None,
        is_split_line: false,
    }
}

struct DerivedLineCandidate {
    bonuses_by_type: Vec<Vec<crate::map::effect::DerivedBonus>>,
    original_bonuses: Vec<Vec<crate::map::effect::DerivedBonus>>,
    fixed_links_by_type: Vec<Vec<DerivedEffectLink>>,
    hold: Option<GroupingHold>,
}

struct PendingDerivedLines {
    source: super::PendingDerivedEffects,
    candidates: Vec<DerivedLineCandidate>,
    prose_links: Vec<DerivedEffectLink>,
    prose_members: Vec<LineSplitMember>,
}

impl super::PendingDerivedEffects {
    fn owner(&self) -> BonusOwner<'_> {
        let kind = match self.owner_kind {
            EffectOwner::Feat => BonusOwnerKind::Feat,
            EffectOwner::ItemAugmentSlotOption => BonusOwnerKind::ItemAugmentSlotOption,
            EffectOwner::Augment => BonusOwnerKind::Augment,
            EffectOwner::SetBonusTier => BonusOwnerKind::SetBonusTier,
            EffectOwner::Item => unreachable!("item buffs already have links"),
        };
        BonusOwner { kind, name: &self.owner_name, family: self.family.as_deref() }
    }

    fn effects(&self) -> &[Effect] {
        match &self.source {
            super::PendingEffectSource::Effects { effects, .. } => effects,
            super::PendingEffectSource::SetTier(tier) => &tier.effects,
        }
    }

    fn normalize_option_targets(&mut self) {
        let super::PendingEffectSource::Effects { effects, option_name: Some(name) } = &mut self.source else { return };
        let name = EFFECT_MAP
            .option_name_aliases
            .iter()
            .fold(name.clone(), |name, (source, replacement)| name.replace(source, replacement));
        for effect in effects.iter_mut().filter(|effect| effect.types.iter().any(|kind| kind == "TacticalDC")) {
            for (_, targets) in EFFECT_MAP.option_companion_targets.iter().filter(|(line, _)| name.contains(*line)) {
                effect.targets.retain(|target| !targets.contains(target));
            }
            for (_, aliases) in EFFECT_MAP.option_target_aliases.iter().filter(|(line, _)| name.contains(*line)) {
                for target in &mut effect.targets {
                    if let Some(alias) = aliases.get(target) {
                        *target = alias.clone();
                    }
                }
            }
        }
    }
}

pub(super) struct LineSplitMember {
    pub(super) bonus: crate::map::effect::DerivedBonus,
    pub(super) line_name: String,
}

pub(super) enum GroupingOutcome {
    Grouped(Vec<DerivedEffectLink>),
    Split { links: Vec<DerivedEffectLink>, correction: String },
    Held(GroupingHold),
    NotGrouped,
}

pub(super) enum GroupingHold {
    TypesWithoutStats(Vec<String>),
    DifferentValues,
    Amounts(anyhow::Error),
    DifferentTypes,
    Deferred(&'static crate::line_exceptions::DeferredAugmentSplit),
}

enum LineSplitMatch {
    Missing,
    Ready,
    Held(GroupingHold),
}

fn line_split_match(
    correction: &crate::corrections::Correction,
    stats: std::collections::BTreeSet<String>,
    links: &[DerivedEffectLink],
) -> Result<LineSplitMatch> {
    let Some(first) = links.first() else { return Ok(LineSplitMatch::Missing) };
    if correction.upstream_line_stats()? != [stats]
        || first.bonus_type.map(|kind| kind.name()) != correction.bonus_type.as_deref()
        || correction.bonus_value.is_some_and(|amount| first.value != Some(amount))
        || !links.iter().all(|link| link.bonus_type == first.bonus_type)
    {
        return Ok(LineSplitMatch::Missing);
    }
    Ok(if links.iter().all(|link| link.value == first.value) {
        LineSplitMatch::Ready
    } else {
        LineSplitMatch::Held(GroupingHold::DifferentValues)
    })
}

impl GroupingHold {
    fn reason(&self) -> String {
        match self {
            Self::TypesWithoutStats(types) => format!("effect types without derived stats: {}", types.join(", ")),
            Self::Amounts(error) => format!("{error:#}"),
            Self::DifferentValues => "different values within one effect".to_string(),
            Self::DifferentTypes => "different types within one effect".to_string(),
            Self::Deferred(exception) => exception.description(),
        }
    }
}

impl<'a> TableWriter<'a> {
    pub(super) fn queue_effect_links(&mut self, owner: &BonusOwner, owner_id: i64, links: Vec<DerivedEffectLink>) {
        if let Some(pending) = self
            .pending_effect_links
            .iter_mut()
            .find(|pending| pending.kind == owner.kind && pending.owner_id == owner_id)
        {
            pending.links.extend(links);
            return;
        }
        self.pending_effect_links.push(PendingEffectLinks {
            kind: owner.kind,
            name: owner.name.to_string(),
            owner_id,
            links,
        });
    }

    pub(super) fn write_pending_effect_links(&mut self) -> Result<()> {
        let mut owners = std::mem::take(&mut self.pending_effect_links);
        for correction in
            self.bonus_link_corrections.iter().chain(&self.bonus_removal_corrections).copied().collect::<Vec<_>>()
        {
            if correction.field == "bonus_type" && correction.from == crate::corrections::CorrectionValue::Null {
                continue;
            }
            let mut matching = Vec::new();
            let mut values = Vec::new();
            for (index, owner) in owners.iter().enumerate() {
                if correction.kind == owner.kind.bonus_link_correction_kind()
                    && correction.name == owner.name
                    && self.correction_owner_matches(correction, owner.owner_id)?
                {
                    let upstream = super::corrections::pending_bonus_correction_values(
                        self.transaction,
                        self.groupings,
                        correction,
                        owner.owner_id,
                        &owner.links,
                    )
                    .with_context(|| format!("correction file {}: {}", correction.file_name, correction.label()))?;
                    self.observed_bonus_corrections.extend(upstream.iter().cloned().map(|value| (correction, value)));
                    values.extend(upstream);
                    matching.push(index);
                }
            }
            if super::corrections::upstream_values_match(correction, &values)? {
                for index in matching {
                    let owner = &mut owners[index];
                    super::corrections::write_pending_bonus_correction(
                        self.transaction,
                        &mut self.effects,
                        self.groupings,
                        correction,
                        owner.owner_id,
                        &mut owner.links,
                    )
                    .with_context(|| format!("correction file {}: {}", correction.file_name, correction.label()))?;
                }
            }
        }
        self.write_grouped_line_corrections(&mut owners)?;
        for pending in owners {
            let effect_owner = match pending.kind {
                BonusOwnerKind::Item => EffectOwner::Item,
                BonusOwnerKind::Augment => EffectOwner::Augment,
                BonusOwnerKind::Feat => EffectOwner::Feat,
                BonusOwnerKind::SetBonusTier => EffectOwner::SetBonusTier,
                BonusOwnerKind::ItemAugmentSlotOption => EffectOwner::ItemAugmentSlotOption,
            };
            for (sort_order, link) in pending.links.into_iter().enumerate() {
                self.effects.insert_link(
                    (effect_owner, pending.owner_id),
                    link.effect_id,
                    link.alias.as_deref(),
                    link.bonus_type,
                    (link.value, link.value2),
                    sort_order,
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn write_stat_type_effects(&mut self, definitions: &[&crate::groupings::Grouping]) -> Result<()> {
        let routes = stat_links_at_routed_types(self.transaction)?;
        let mut targets = std::collections::BTreeMap::new();
        for definition in
            definitions.iter().filter(|definition| routes.iter().any(|route| route.effect == definition.name))
        {
            let id = self.effects.ensure_indexed_effect(definition)?;
            targets.insert(&definition.name, id);
        }
        for route in routes {
            let target = targets[&route.effect];
            self.transaction.execute(
                &format!("UPDATE {} SET effect_id=?1 WHERE {}=?2 AND sort_order=?3", route.table, route.owner_column),
                params![target, route.owner_id, route.sort_order],
            )?;
        }
        Ok(())
    }

    pub(super) fn link_pending_derived_effects(&mut self) -> Result<()> {
        let sources = std::mem::take(&mut self.pending_derived_effects);
        let mut pending = Vec::new();
        for mut source in sources {
            source.normalize_option_targets();
            pending.push(self.write_line_candidates(source)?);
        }
        self.apply_candidate_removals(&mut pending)?;
        for pending in &mut pending {
            let owner = pending.source.owner();
            for (effect, candidate) in pending.source.effects().iter().zip(&mut pending.candidates) {
                candidate.hold = self.candidate_grouping_hold(&owner, pending.source.owner_id, effect, candidate);
            }
        }
        self.select_candidate_splits(&mut pending)?;
        for mut pending in pending {
            let source = &pending.source;
            let owner = source.owner();
            let modifier_source = match source.owner_kind {
                EffectOwner::Feat => ModifierSource::Feat,
                EffectOwner::Augment => ModifierSource::Augment,
                EffectOwner::ItemAugmentSlotOption => ModifierSource::ItemAugmentSlotOption,
                EffectOwner::SetBonusTier => ModifierSource::SetBonusTier,
                EffectOwner::Item => unreachable!("item buffs already have links"),
            };
            if source.owner_kind == EffectOwner::SetBonusTier {
                self.write_modifiers(modifier_source, source.owner_id, source.effects())?;
            }
            let mut links = self.write_derived_candidates(
                &owner,
                modifier_source,
                source.owner_id,
                source.effects(),
                &mut pending.candidates,
            )?;
            if !pending.prose_links.is_empty() {
                if let Some(first) = pending.prose_members.first() {
                    if let Some(outcome) = self.write_split_lines(
                        &owner,
                        source.owner_id,
                        first.bonus.bonus_type.context("typed prose member")?,
                        &pending.prose_members,
                    )? {
                        let GroupingOutcome::Split { links: split_links, .. } = outcome else {
                            anyhow::bail!("set {:?} prose split cannot preserve its single value", owner.name)
                        };
                        pending.prose_links = split_links;
                    }
                }
                links.extend(pending.prose_links);
            }
            if source.owner_kind == EffectOwner::Augment {
                links = self.collapse_source_backed_augment_lines(&source.owner_name, links)?;
            }
            self.queue_effect_links(&owner, source.owner_id, links);
        }
        Ok(())
    }

    fn write_line_candidates(&mut self, source: super::PendingDerivedEffects) -> Result<PendingDerivedLines> {
        let owner = source.owner();
        let mut candidates = Vec::new();
        for effect in source.effects() {
            let mut bonuses_by_type = Vec::new();
            let mut fixed_links_by_type = Vec::new();
            for effect_type in &effect.types {
                let mut single_type = effect.clone();
                single_type.types = vec![effect_type.clone()];
                let mut bonuses = self.owner_derived_bonuses(&owner, &single_type)?;
                for bonus in &mut bonuses {
                    bonus.bonus_type = Some(self.bonus_type_of(
                        &BonusOrigin {
                            owner: &owner,
                            source_name: effect_type,
                            target_name: bonus.stat.name,
                            value: Some(bonus.value),
                        },
                        bonus.bonus_type,
                    )?);
                }
                let mut fixed_links = Vec::new();
                if bonuses.is_empty() {
                    self.ensure_targeted_fixed_effect(&single_type, &mut fixed_links)?;
                }
                bonuses_by_type.push(bonuses);
                fixed_links_by_type.push(fixed_links);
            }
            candidates.push(DerivedLineCandidate {
                original_bonuses: bonuses_by_type.clone(),
                bonuses_by_type,
                fixed_links_by_type,
                hold: None,
            });
        }
        let mut prose_links = Vec::new();
        let mut prose_members = Vec::new();
        if candidates.iter().all(|candidate| {
            candidate.bonuses_by_type.iter().all(Vec::is_empty)
                && candidate.fixed_links_by_type.iter().all(Vec::is_empty)
        }) {
            if let super::PendingEffectSource::SetTier(tier) = &source.source {
                prose_links = self.write_set_tier_prose(&source.owner_name, tier)?;
                prose_members = self.prose_split_members(&owner, source.owner_id, &prose_links)?;
            }
        }
        Ok(PendingDerivedLines { source, candidates, prose_links, prose_members })
    }

    fn prose_split_members(
        &self,
        owner: &BonusOwner,
        owner_id: i64,
        links: &[DerivedEffectLink],
    ) -> Result<Vec<LineSplitMember>> {
        let [link] = links else { return Ok(Vec::new()) };
        let Some(value) = link.value else { return Ok(Vec::new()) };
        let mut has_split = false;
        for correction in &self.line_corrections {
            if !correction.is_line_merge()
                && correction.name == owner.name
                && correction.kind == owner.kind.bonus_link_correction_kind()
                && correction.from.as_text() != Some(crate::corrections::NO_SUCH_BONUS)
                && self.correction_owner_matches(correction, owner_id)?
            {
                has_split = true;
                break;
            }
        }
        if !has_split {
            return Ok(Vec::new());
        }
        let target = &self.effects.family(link.effect_id).context("prose effect is cached")?.name;
        let mut rows = super::corrections::line_bonus_rows(
            self.transaction,
            owner.kind.bonus_link_correction_kind(),
            owner_id,
            Some(links),
        )
        .with_context(|| format!("set {:?} prose target {target:?} has invalid split members", owner.name))?;
        let Some(bonus_type) =
            rows.first().and_then(|row| row.bonus_type.as_deref()).and_then(ddo_model::enums::BonusType::parse)
        else {
            return Ok(Vec::new());
        };
        if self.split_corrections_for_owner(owner, owner_id, bonus_type, value)?.is_empty() {
            return Ok(Vec::new());
        }
        rows.sort_by(|left, right| left.stat_name.cmp(&right.stat_name));
        rows.into_iter()
            .map(|row| {
                let stat =
                    ddo_model::stats::Stat::by_name(&row.stat_name).with_context(|| {
                        format!(
                            "set {:?} prose target {target:?} has unknown split member {:?}",
                            owner.name, row.stat_name,
                        )
                    })?;
                Ok(LineSplitMember {
                    bonus: crate::map::effect::DerivedBonus { stat, bonus_type: Some(bonus_type), value },
                    line_name: row.stat_name,
                })
            })
            .collect()
    }

    fn matching_candidate_owners(
        &self,
        correction: &crate::corrections::Correction,
        pending: &[PendingDerivedLines],
    ) -> Result<std::collections::BTreeMap<i64, Vec<usize>>> {
        let mut owners = std::collections::BTreeMap::<i64, Vec<usize>>::new();
        for (index, pending) in pending.iter().enumerate() {
            let source = &pending.source;
            let owner = source.owner();
            if correction.kind == owner.kind.bonus_link_correction_kind()
                && correction.name == owner.name
                && self.correction_owner_matches(correction, source.owner_id)?
            {
                owners.entry(source.owner_id).or_default().push(index);
            }
        }
        Ok(owners)
    }

    fn candidate_grouping_hold(
        &self,
        owner: &BonusOwner,
        owner_id: i64,
        effect: &Effect,
        candidate: &DerivedLineCandidate,
    ) -> Option<GroupingHold> {
        let stats = candidate
            .bonuses_by_type
            .iter()
            .flatten()
            .map(|bonus| bonus.stat.id)
            .collect::<std::collections::BTreeSet<_>>();
        if stats.len() < 2 {
            return None;
        }
        if owner.kind == BonusOwnerKind::Augment {
            if let Some(exception) = self.deferred_augment_splits.get(&owner_id) {
                return Some(GroupingHold::Deferred(exception));
            }
        }
        let without_stats = effect
            .types
            .iter()
            .zip(&candidate.original_bonuses)
            .filter(|(_, bonuses)| bonuses.is_empty())
            .map(|(kind, _)| kind.clone())
            .collect::<Vec<_>>();
        if !without_stats.is_empty() {
            return Some(GroupingHold::TypesWithoutStats(without_stats));
        }
        let types = candidate
            .bonuses_by_type
            .iter()
            .flatten()
            .map(|bonus| bonus.bonus_type.map(|kind| kind.id()))
            .collect::<std::collections::BTreeSet<_>>();
        (types.len() > 1).then_some(GroupingHold::DifferentTypes)
    }

    fn apply_candidate_removals(&mut self, pending: &mut [PendingDerivedLines]) -> Result<()> {
        let removals = self
            .bonus_removal_corrections
            .iter()
            .copied()
            .filter(|correction| correction.stat.as_deref().and_then(ddo_model::stats::Stat::by_name).is_some())
            .collect::<Vec<_>>();
        let mut decided = Vec::new();
        for correction in removals {
            let owners = self.matching_candidate_owners(correction, pending)?;
            if owners.is_empty() {
                continue;
            }
            let mut observations = Vec::new();
            for (owner_id, indices) in &owners {
                let links = indices
                    .iter()
                    .flat_map(|index| {
                        let source = &pending[*index];
                        source
                            .candidates
                            .iter()
                            .flat_map(|candidate| candidate.bonuses_by_type.iter().flatten())
                            .map(derived_bonus_link)
                            .chain(source.prose_links.iter().cloned())
                    })
                    .collect::<Vec<_>>();
                observations.extend(
                    super::corrections::pending_bonus_correction_values(
                        self.transaction,
                        self.groupings,
                        correction,
                        *owner_id,
                        &links,
                    )
                    .with_context(|| format!("correction file {}: {}", correction.file_name, correction.label()))?,
                );
            }
            let applies = super::corrections::upstream_values_match(correction, &observations)?;
            self.observed_bonus_corrections.extend(observations.into_iter().map(|upstream| (correction, upstream)));
            decided.push(correction);
            if !applies {
                continue;
            }
            for (owner_id, indices) in owners {
                let bonuses = indices
                    .iter()
                    .flat_map(|index| pending[*index].candidates.iter())
                    .flat_map(|candidate| candidate.bonuses_by_type.iter().flatten())
                    .map(|bonus| {
                        (bonus.stat.id, bonus.bonus_type.expect("candidate has resolved type").id(), bonus.value)
                    })
                    .collect::<Vec<_>>();
                let removed = super::corrections::removed_derived_bonus_indices(
                    self.transaction,
                    correction,
                    owner_id,
                    &bonuses,
                )?;
                let mut position = 0;
                for index in indices {
                    if !pending[index].prose_links.is_empty() {
                        super::corrections::write_pending_bonus_correction(
                            self.transaction,
                            &mut self.effects,
                            self.groupings,
                            correction,
                            owner_id,
                            &mut pending[index].prose_links,
                        )?;
                    }
                    for candidate in &mut pending[index].candidates {
                        for bonuses in &mut candidate.bonuses_by_type {
                            bonuses.retain(|_| {
                                let keep = !removed.contains(&position);
                                position += 1;
                                keep
                            });
                        }
                    }
                }
            }
        }
        self.bonus_removal_corrections
            .retain(|correction| !decided.iter().any(|known| std::ptr::eq(*known, *correction)));
        Ok(())
    }

    fn select_candidate_splits(&mut self, pending: &mut [PendingDerivedLines]) -> Result<()> {
        use crate::corrections::{CorrectionValue, NO_SUCH_BONUS};
        let mut blocked = Vec::new();
        for correction in self
            .line_corrections
            .iter()
            .copied()
            .filter(|correction| !correction.is_line_merge() && correction.from.as_text() != Some(NO_SUCH_BONUS))
        {
            let owners = self.matching_candidate_owners(correction, pending)?;
            let mut observations = Vec::new();
            for indices in owners.values() {
                let mut matches = false;
                for index in indices {
                    let pending = &mut pending[*index];
                    for candidate in &mut pending.candidates {
                        if candidate.hold.is_none() {
                            let links =
                                candidate.bonuses_by_type.iter().flatten().map(derived_bonus_link).collect::<Vec<_>>();
                            match self.split_source_match(correction, &links)? {
                                LineSplitMatch::Ready => matches = true,
                                LineSplitMatch::Held(hold) => candidate.hold = Some(hold),
                                LineSplitMatch::Missing => {}
                            }
                        }
                    }
                    let links = pending
                        .prose_members
                        .iter()
                        .map(|member| derived_bonus_link(&member.bonus))
                        .collect::<Vec<_>>();
                    matches |= matches!(self.split_source_match(correction, &links)?, LineSplitMatch::Ready);
                }
                observations.push(if matches {
                    correction.from.clone()
                } else {
                    CorrectionValue::Text(NO_SUCH_BONUS.into())
                });
            }
            if !owners.is_empty() && !super::corrections::upstream_values_match(correction, &observations)? {
                self.observed_bonus_corrections.extend(observations.into_iter().map(|upstream| (correction, upstream)));
                blocked.push(correction);
            }
        }
        self.line_corrections.retain(|correction| !blocked.iter().any(|known| std::ptr::eq(*known, *correction)));
        Ok(())
    }

    fn split_source_match(
        &self,
        correction: &crate::corrections::Correction,
        line: &[DerivedEffectLink],
    ) -> Result<LineSplitMatch> {
        let stats = line
            .iter()
            .map(|link| {
                self.effects.family(link.effect_id).map(|family| family.name.clone()).context("derived stat is cached")
            })
            .collect::<Result<std::collections::BTreeSet<_>>>()?;
        line_split_match(correction, stats, line)
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

    fn write_derived_candidates(
        &mut self,
        owner: &BonusOwner,
        modifier_source: ModifierSource,
        owner_id: i64,
        effects: &[Effect],
        candidates: &mut [DerivedLineCandidate],
    ) -> Result<Vec<DerivedEffectLink>> {
        let mut links = Vec::new();
        for (source_order, (effect, candidate)) in effects.iter().zip(candidates).enumerate() {
            let mut derived_by_type = std::mem::take(&mut candidate.bonuses_by_type);
            let first_link = links.len();
            let types_had_stats =
                candidate.original_bonuses.iter().map(|bonuses| !bonuses.is_empty()).collect::<Vec<_>>();
            let all_bonuses_removed =
                types_had_stats.iter().any(|had_stats| *had_stats) && derived_by_type.iter().all(Vec::is_empty);
            let source_index = self.derived_effect_lines.len();
            let stat_names = derived_by_type
                .iter()
                .flatten()
                .map(|bonus| bonus.stat.name.to_string())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            if all_bonuses_removed {
                candidate.bonuses_by_type = candidate.original_bonuses.clone();
                candidate.hold = self.candidate_grouping_hold(owner, owner_id, effect, candidate);
                derived_by_type = std::mem::take(&mut candidate.bonuses_by_type);
            }
            let outcome =
                self.write_grouped_effect_links(owner, owner_id, effect, &derived_by_type, candidate.hold.take())?;
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
            let preserve_member_lines = matches!(
                outcome,
                GroupingOutcome::Held(
                    GroupingHold::DifferentTypes
                        | GroupingHold::DifferentValues
                        | GroupingHold::Amounts(_)
                        | GroupingHold::Deferred(_)
                )
            );
            let grouped_links = match outcome {
                GroupingOutcome::Grouped(links) => Some(links),
                GroupingOutcome::Split { links, correction } => {
                    self.line_splits.push(split(correction));
                    Some(links)
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
                if !all_bonuses_removed {
                    self.derived_effect_lines[source_index].link_count = grouped_links.len();
                    links.extend(grouped_links);
                }
                continue;
            }
            for (type_index, effect_type) in effect.types.iter().enumerate() {
                if types_had_stats[type_index] && derived_by_type[type_index].is_empty() {
                    continue;
                }
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
                            alias: None,
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
                    links.append(&mut candidate.fixed_links_by_type[type_index]);
                    continue;
                }
                let all_abilities =
                    effect_type == "AbilityBonus" && effect.targets.iter().any(|target| target == "All");
                let groups: Vec<Vec<_>> =
                    if !preserve_member_lines && (all_abilities || effect_type == "SkillBonusAbility") {
                        vec![derived_bonuses]
                    } else {
                        derived_bonuses.into_iter().map(|bonus| vec![bonus]).collect()
                    };
                for (group_order, bonuses) in groups.into_iter().enumerate() {
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
                    } else if effect.targets.len() > group_order {
                        Some(effect.targets[group_order].as_str())
                    } else {
                        effect.targets.first().map(String::as_str)
                    };
                    let line_name = (preserve_member_lines && target.is_none()).then_some(first_bonus.stat.name);
                    links.push(self.write_derived_line(owner, effect_type, target, &bonuses, line_name)?);
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
            if all_bonuses_removed {
                let retained = links
                    .drain(first_link..)
                    .filter(|link| !self.effects.has_stats(link.effect_id))
                    .collect::<Vec<_>>();
                links.extend(retained);
            }
            self.derived_effect_lines[source_index].link_count = links.len() - first_link;
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
        let bonus_type = first_bonus.bonus_type.context("derived candidate has a resolved type")?;
        anyhow::ensure!(
            bonuses.iter().all(|bonus| bonus.bonus_type == Some(bonus_type)),
            "{:?} {:?} has different types within one {effect_type} line",
            owner.kind,
            owner.name
        );
        if let ("SkillBonusAbility", Some(ability)) = (effect_type, target) {
            let complete_skill_group = EFFECT_MAP.skill_ability_groups.skills.get(ability).is_some_and(|members| {
                members.iter().all(|member| bonuses.iter().any(|bonus| bonus.stat.name == member))
                    && bonuses.iter().all(|bonus| members.iter().any(|member| member == bonus.stat.name))
            });
            if complete_skill_group {
                let effect_id = self.effects.ensure_group(&format!("{ability} Skills"))?;
                return Ok(DerivedEffectLink {
                    alias: None,
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
        self.write_bonus_line(owner, &source_name, bonuses, bonus_type, all_abilities)
    }

    fn write_bonus_line(
        &mut self,
        owner: &BonusOwner,
        source_name: &str,
        bonuses: &[crate::map::effect::DerivedBonus],
        bonus_type: ddo_model::enums::BonusType,
        all_abilities: bool,
    ) -> Result<DerivedEffectLink> {
        let first_bonus = bonuses[0];
        let family_name = if owner.kind == BonusOwnerKind::Augment {
            EFFECT_MAP.augment_line_name(owner.name, source_name).unwrap_or(source_name)
        } else {
            source_name
        }
        .to_string();
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
            alias: None,
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
        derived_by_type: &[Vec<crate::map::effect::DerivedBonus>],
        hold: Option<GroupingHold>,
    ) -> Result<GroupingOutcome> {
        let mut bonuses = Vec::new();
        for (effect_type, derived) in effect.types.iter().zip(derived_by_type.iter()) {
            bonuses.extend(derived.iter().copied().map(|bonus| (effect_type.as_str(), bonus)));
        }
        let stats =
            bonuses.iter().map(|(_, bonus)| bonus.stat.name.to_string()).collect::<std::collections::BTreeSet<_>>();
        if stats.len() < 2 {
            return Ok(GroupingOutcome::NotGrouped);
        }
        if let Some(hold) = hold {
            return Ok(GroupingOutcome::Held(hold));
        }
        let value = bonuses[0].1.value;
        let same_value = bonuses.iter().all(|(_, bonus)| bonus.value == value);
        let bonus_type = bonuses[0].1.bonus_type.context("derived candidate has a resolved type")?;
        let split_members = bonuses
            .iter()
            .map(|(effect_type, bonus)| {
                let target = effect
                    .targets
                    .iter()
                    .find(|target| {
                        EFFECT_MAP.effect_stat(effect_type, target).is_some_and(|stat| stat.id == bonus.stat.id)
                    })
                    .map(String::as_str);
                LineSplitMember {
                    bonus: *bonus,
                    line_name: if *effect_type == "SkillBonusAbility" {
                        bonus.stat.name.to_string()
                    } else {
                        self.buff_resolver.effect_tier_name(effect_type, target, bonus.stat.name)
                    },
                }
            })
            .collect::<Vec<_>>();
        if let Some(split) = self.write_split_lines(owner, owner_id, bonus_type, &split_members)? {
            return Ok(split);
        }
        let matching = self.groupings.by_stat_set.get(&stats).map(Vec::as_slice).unwrap_or_default();
        let mut amounts = (Some(value), None);
        let effect_id = match matching {
            [grouping] if grouping.accepts_link_values_at_type(bonus_type.name()) => {
                let member_amounts =
                    bonuses.iter().map(|(_, bonus)| (bonus.stat.name, Some(bonus.value))).collect::<Vec<_>>();
                let resolved_amounts = match self.groupings.link_amounts(grouping, &member_amounts) {
                    Ok(amounts) => amounts,
                    Err(error) => return Ok(GroupingOutcome::Held(GroupingHold::Amounts(error))),
                };
                amounts = resolved_amounts;
                self.effects.ensure_indexed_effect(grouping)?
            }
            [] => {
                if !same_value {
                    return Ok(GroupingOutcome::Held(GroupingHold::DifferentValues));
                }
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
                anyhow::bail!(
                    "{:?} {:?}: no unique usable grouping for {:?} at {}: {}",
                    owner.kind,
                    owner.name,
                    stats,
                    bonus_type.name(),
                    matching.iter().map(|grouping| grouping.name.as_str()).collect::<Vec<_>>().join(", ")
                );
            }
        };
        let mut link =
            self.write_upstream_grouping_line(owner, effect_id, bonus_type, amounts.0.unwrap_or_default())?;
        link.value = amounts.0;
        link.value2 = amounts.1;
        Ok(GroupingOutcome::Grouped(vec![link]))
    }

    pub(super) fn write_split_lines(
        &mut self,
        owner: &BonusOwner,
        owner_id: i64,
        bonus_type: ddo_model::enums::BonusType,
        members: &[LineSplitMember],
    ) -> Result<Option<GroupingOutcome>> {
        let Some(first) = members.first() else { return Ok(None) };
        let value = first.bonus.value;
        let source_links = members
            .iter()
            .map(|member| DerivedEffectLink {
                effect_id: member.bonus.stat.id,
                bonus_type: Some(bonus_type),
                value: Some(member.bonus.value),
                value2: None,
                alias: None,
                is_split_line: false,
            })
            .collect::<Vec<_>>();
        let split_corrections = self.split_corrections_for_owner(owner, owner_id, bonus_type, value)?;
        let mut split_correction = None;
        for correction in split_corrections {
            match self.split_source_match(correction, &source_links)? {
                LineSplitMatch::Missing => continue,
                LineSplitMatch::Held(hold) => return Ok(Some(GroupingOutcome::Held(hold))),
                LineSplitMatch::Ready => {}
            }
            anyhow::ensure!(split_correction.is_none(), "{}: overlapping split corrections", correction.label());
            split_correction = Some(correction);
        }
        if let Some(correction) = split_correction {
            let mut partitions = correction.line_partition_stats(self.groupings)?;
            partitions.sort_by_key(|(_, partition)| {
                members.iter().position(|member| partition.contains(member.bonus.stat.name))
            });
            let mut split_links = Vec::new();
            for (line_name, partition) in partitions {
                let mut link = if ddo_model::stats::Stat::by_name(&line_name).is_some() {
                    let member = members
                        .iter()
                        .find(|member| partition.contains(member.bonus.stat.name))
                        .with_context(|| {
                            format!(
                                "{}: split target {line_name:?} has no member on {}",
                                correction.label(),
                                owner.name
                            )
                        })?;
                    self.write_bonus_line(owner, &member.line_name, &[member.bonus], bonus_type, false)?
                } else {
                    let line_effect = self.groupings.effect_named(&line_name).ok_or_else(|| {
                        anyhow::anyhow!(
                            "{}: split line {line_name:?} has no indexed effect definition",
                            correction.label()
                        )
                    })?;
                    let effect_id = self.effects.ensure_indexed_effect(line_effect)?;
                    self.write_upstream_grouping_line(owner, effect_id, bonus_type, value)?
                };
                link.is_split_line = true;
                split_links.push(link);
            }
            if !self.applied_link_corrections.iter().any(|applied| std::ptr::eq(*applied, correction)) {
                self.applied_link_corrections.push(correction);
            }
            return Ok(Some(GroupingOutcome::Split { links: split_links, correction: correction.label() }));
        }
        Ok(None)
    }

    pub(super) fn split_corrections_for_owner(
        &self,
        owner: &BonusOwner,
        owner_id: i64,
        bonus_type: ddo_model::enums::BonusType,
        value: i64,
    ) -> Result<Vec<&'a crate::corrections::Correction>> {
        let candidates = self
            .line_corrections
            .iter()
            .copied()
            .filter(|correction| {
                correction.name == owner.name
                    && !correction.is_line_merge()
                    && correction.from.as_text() != Some(crate::corrections::NO_SUCH_BONUS)
                    && correction.bonus_type.as_deref() == Some(bonus_type.name())
                    && correction.bonus_value.is_none_or(|amount| amount == value)
                    && matches!(
                        (owner.kind, correction.kind),
                        (BonusOwnerKind::Augment, ddo_model::enums::CorrectionKind::AugmentBonus)
                            | (BonusOwnerKind::Feat, ddo_model::enums::CorrectionKind::FeatBonus)
                            | (
                                BonusOwnerKind::ItemAugmentSlotOption,
                                ddo_model::enums::CorrectionKind::SocketOptionBonus
                            )
                            | (BonusOwnerKind::SetBonusTier, ddo_model::enums::CorrectionKind::SetTierBonus)
                    )
            })
            .collect::<Vec<_>>();
        candidates
            .into_iter()
            .filter_map(|correction| match self.correction_owner_matches(correction, owner_id) {
                Ok(true) => Some(Ok(correction)),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            })
            .collect()
    }

    fn correction_owner_matches(&self, correction: &crate::corrections::Correction, owner_id: i64) -> Result<bool> {
        if correction.kind == ddo_model::enums::CorrectionKind::SocketOptionBonus {
            let targets =
                self.socket_option_targets.as_ref().expect("socket-option targets loaded before writing links");
            return Ok(targets.option_id(correction, &correction.name)? == Some(owner_id));
        }
        super::corrections::correction_owner_matches(self.transaction, correction, owner_id)
    }

    fn matching_pending_owners(
        &self,
        correction: &crate::corrections::Correction,
        owners: &[PendingEffectLinks],
    ) -> Result<Vec<usize>> {
        let mut matching = Vec::new();
        for (index, owner) in owners.iter().enumerate() {
            if correction.kind == owner.kind.bonus_link_correction_kind()
                && correction.name == owner.name
                && self.correction_owner_matches(correction, owner.owner_id)?
            {
                matching.push(index);
            }
        }
        Ok(matching)
    }

    fn write_grouped_line_corrections(&mut self, owners: &mut [PendingEffectLinks]) -> Result<()> {
        use crate::corrections::{CorrectionValue, NO_SUCH_BONUS};
        let mut conflicting_additions = vec![std::collections::BTreeSet::new(); owners.len()];
        for correction in self.bonus_additions.clone() {
            let mut matching = Vec::new();
            for index in self.matching_pending_owners(correction, owners)? {
                let owner = &owners[index];
                let rows = super::corrections::added_bonus_rows(
                    self.transaction,
                    correction,
                    owner.owner_id,
                    Some(&owner.links),
                )?;
                let upstream = super::corrections::added_bonus_upstream(correction, &rows)?;
                self.observed_bonus_corrections.push((correction, upstream.clone()));
                matching.push((index, upstream, rows));
            }
            let values = matching.iter().map(|(_, value, _)| value.clone()).collect::<Vec<_>>();
            let applies = super::corrections::upstream_values_match(correction, &values)?
                && correction.from == CorrectionValue::Null;
            let CorrectionValue::Bonus(bonus) = &correction.to else { unreachable!("validated bonus addition") };
            for (index, upstream, rows) in matching {
                if applies {
                    owners[index].links.push(
                        super::corrections::write_added_bonus_link(
                            self.transaction,
                            &mut self.effects,
                            self.groupings,
                            bonus,
                        )
                        .with_context(|| {
                            format!(
                                "writing effect lines for {:?}: correction file {}: {}",
                                owners[index].name,
                                correction.file_name,
                                correction.label()
                            )
                        })?,
                    );
                } else if upstream != correction.to {
                    conflicting_additions[index].extend(rows.iter().map(|row| row.order));
                }
            }
        }
        let mut rows_by_owner = std::collections::BTreeMap::new();
        let mut consumed = vec![std::collections::BTreeSet::new(); owners.len()];
        let mut removed = vec![std::collections::BTreeSet::new(); owners.len()];
        for correction in self.line_corrections.clone().into_iter().filter(|correction| correction.is_line_merge()) {
            correction.line_partition_stats(self.groupings)?;
            let mut matching = Vec::new();
            let mut observations = Vec::new();
            for index in self.matching_pending_owners(correction, owners)? {
                let owner = &owners[index];
                let rows = match rows_by_owner.entry(index) {
                    std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(super::corrections::line_bonus_rows(
                            self.transaction,
                            correction.kind,
                            owner.owner_id,
                            Some(&owner.links),
                        )?)
                    }
                };
                let selection = super::corrections::line_merge_selection(correction, rows)?
                    .filter(|selection| selection.orders.is_disjoint(&conflicting_additions[index]));
                let upstream = if selection.is_some() {
                    correction.from.clone()
                } else {
                    CorrectionValue::Text(NO_SUCH_BONUS.into())
                };
                self.observed_bonus_corrections.push((correction, upstream.clone()));
                observations.push(upstream);
                if let Some(selection) = selection {
                    matching.push((index, selection));
                }
            }
            if !super::corrections::upstream_values_match(correction, &observations)? {
                continue;
            }
            for (index, selection) in matching {
                let links = &mut owners[index].links;
                anyhow::ensure!(
                    selection.orders.is_disjoint(&consumed[index]),
                    "{}: overlapping merge corrections",
                    correction.label()
                );
                anyhow::ensure!(
                    selection.orders.iter().all(|order| !links[*order].is_split_line),
                    "{}: a merge and a split cannot apply to one line",
                    correction.label()
                );
                let grouping_name = correction.to.as_text().context("validated merge target")?;
                let grouping = self
                    .groupings
                    .effect_named(grouping_name)
                    .with_context(|| format!("{}: merge target is not indexed", correction.label()))?;
                let bonus_type = correction
                    .bonus_type
                    .as_deref()
                    .and_then(ddo_model::enums::BonusType::parse)
                    .with_context(|| format!("{}: merged line needs a named type", correction.label()))?;
                anyhow::ensure!(
                    selection.stats == self.groupings.expanded_stat_names(grouping_name)?,
                    "{}: merged line has different stats",
                    correction.label()
                );
                let selected = rows_by_owner[&index]
                    .iter()
                    .filter(|row| selection.orders.contains(&row.order))
                    .collect::<Vec<_>>();
                let (value, value2) = super::corrections::merged_line_amounts(self.groupings, correction, &selected)?;
                let effect_id = self.effects.ensure_indexed_effect(grouping)?;
                let first = *selection.orders.first().context("merge has source lines")?;
                links[first] = self.write_grouping_line(effect_id, bonus_type, value.unwrap_or_default())?;
                links[first].value = value;
                links[first].value2 = value2;
                removed[index].extend(selection.orders.iter().copied().filter(|order| *order != first));
                consumed[index].extend(selection.orders);
            }
        }
        for (owner, removed) in owners.iter_mut().zip(removed) {
            let mut order = 0;
            owner.links.retain(|_| {
                let keep = !removed.contains(&order);
                order += 1;
                keep
            });
        }
        Ok(())
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
            alias: None,
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
            links.push(DerivedEffectLink {
                alias: None,
                is_split_line: false,
                effect_id,
                bonus_type: None,
                value,
                value2: None,
            });
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
                alias: None,
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
        links.push(DerivedEffectLink {
            alias: None,
            is_split_line: false,
            effect_id,
            bonus_type: None,
            value,
            value2: None,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_with_unequal_member_values_is_held() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let corrections = crate::corrections::Corrections::from_data(&root).unwrap();
        let correction = corrections
            .entries
            .iter()
            .find(|correction| correction.field == "lines" && !correction.is_line_merge())
            .unwrap();
        let stats = correction.upstream_line_stats().unwrap().remove(0);
        let bonus_type = ddo_model::enums::BonusType::parse(correction.bonus_type.as_deref().unwrap()).unwrap();
        let value = correction.bonus_value.unwrap_or(3);
        let mut links = stats
            .iter()
            .map(|name| {
                derived_bonus_link(&crate::map::effect::DerivedBonus {
                    stat: ddo_model::stats::Stat::by_name(name).unwrap(),
                    bonus_type: Some(bonus_type),
                    value,
                })
            })
            .collect::<Vec<_>>();
        assert!(links.len() > 1);
        assert!(matches!(line_split_match(correction, stats.clone(), &links).unwrap(), LineSplitMatch::Ready));
        links.last_mut().unwrap().value = Some(value + 1);
        assert!(matches!(
            line_split_match(correction, stats, &links).unwrap(),
            LineSplitMatch::Held(GroupingHold::DifferentValues)
        ));
    }
}
