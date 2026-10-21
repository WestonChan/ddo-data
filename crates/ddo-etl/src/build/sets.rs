use super::bonus_types::{BonusOwner, BonusOwnerKind};
use super::effects::EffectOwner;
use super::modifiers::DerivedEffectLink;
use super::{trimmed_non_empty, BuildReport, TableWriter};
use crate::map::effect_map::EFFECT_MAP;
use crate::xml::sentient_gems;
use crate::xml::set_bonuses::parse_set_bonus_file;
use crate::xml::set_bonuses::SetBonusTier;
use anyhow::Result;
use ddo_model::enums::ModifierSource;
use ddo_model::stats::Stat;
use rusqlite::params;
use std::collections::HashMap;
use std::path::Path;

impl TableWriter<'_> {
    pub(super) fn write_set_bonus_file(
        &mut self,
        path: &Path,
        is_filigree_set: bool,
        report: &mut BuildReport,
    ) -> Result<()> {
        if !path.is_file() {
            return Ok(());
        }
        let set_bonus_file = parse_set_bonus_file(path)?;
        for set_bonus in &set_bonus_file.set_bonuses {
            let set_name = set_bonus.name.trim();
            if self.written.set_bonus_ids_by_name.contains_key(set_name) {
                continue;
            }
            self.transaction.execute(
                "INSERT INTO set_bonuses (name, icon, is_filigree_set, slug) VALUES (?1, ?2, ?3, ?4)",
                params![
                    set_name,
                    trimmed_non_empty(set_bonus.icon.as_deref()),
                    is_filigree_set,
                    crate::tree::xml_entity_slug("set_bonuses", None, set_name, None)
                ],
            )?;
            let set_id = self.transaction.last_insert_rowid();
            self.written.set_bonus_ids_by_name.insert(set_name.to_string(), set_id);
            let mut tier_ids_and_orders: HashMap<i64, (i64, usize)> = HashMap::new();
            for tier in &set_bonus.tiers {
                let (tier_id, next_order) = match tier_ids_and_orders.entry(tier.equipped_count) {
                    std::collections::hash_map::Entry::Occupied(entry) => *entry.get(),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        self.transaction.execute(
                            "INSERT INTO set_bonus_tiers (set_id, equipped_count) VALUES (?1, ?2)",
                            params![set_id, tier.equipped_count],
                        )?;
                        *entry.insert((self.transaction.last_insert_rowid(), 0))
                    }
                };
                if let Some(description) = trimmed_non_empty(tier.description.as_deref()) {
                    self.written.set_tier_descriptions_by_id.entry(tier_id).or_insert_with(|| description.to_string());
                }
                self.write_modifiers(ModifierSource::SetBonusTier, tier_id, &tier.effects)?;
                let written_count = self.write_set_tier_effects(set_name, tier_id, tier, next_order)?;
                tier_ids_and_orders.insert(tier.equipped_count, (tier_id, next_order + written_count));
            }
        }
        for filigree in &set_bonus_file.filigrees {
            let set_id = match filigree.set_bonus_names.first().map(|s| s.trim()) {
                Some(set_name) => self.written.set_bonus_ids_by_name.get(set_name).copied(),
                None => None,
            };
            self.transaction.execute(
                "INSERT INTO filigrees (name, description, icon, menu, set_id, slug) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(name) DO NOTHING",
                params![filigree.name.trim(), trimmed_non_empty(filigree.description.as_deref()), trimmed_non_empty(filigree.icon.as_deref()), trimmed_non_empty(filigree.menu.as_deref()), set_id, crate::tree::xml_entity_slug("filigrees", None, filigree.name.trim(), None)],
            )?;
            let filigree_id: i64 = self.transaction.query_row(
                "SELECT id FROM filigrees WHERE name = ?1",
                params![filigree.name.trim()],
                |r| r.get(0),
            )?;
            self.write_modifiers(ModifierSource::Filigree, filigree_id, &filigree.effects)?;
            report.filigree_count += 1;
        }
        Ok(())
    }

    fn write_set_tier_effects(
        &mut self,
        set_name: &str,
        tier_id: i64,
        tier: &SetBonusTier,
        first_order: usize,
    ) -> Result<usize> {
        let owner = BonusOwner { kind: BonusOwnerKind::SetBonusTier, name: set_name, family: None };
        let removed_before = self.removed_derived_bonus_count;
        let derived_links =
            self.ensure_derived_effects(&owner, ModifierSource::SetBonusTier, tier_id, &tier.effects)?;
        let links = self.collapse_set_tier_stat_groups(derived_links)?;
        for (offset, link) in links.iter().enumerate() {
            self.effects.insert_link(
                EffectOwner::SetBonusTier,
                tier_id,
                link.effect_id,
                link.bonus_type,
                (link.value, link.value2),
                first_order + offset,
            )?;
        }
        if links.is_empty() && self.removed_derived_bonus_count == removed_before {
            if let Some(description) = trimmed_non_empty(tier.description.as_deref()) {
                if let Some(rule) = EFFECT_MAP.set_tier_prose_bonus(description, self.groupings)? {
                    let bonus_type =
                        ddo_model::enums::BonusType::parse(&rule.bonus_type).expect("validated prose type");
                    let effect_id = if let Some(stat) = Stat::by_name(&rule.effect) {
                        let existing = self
                            .effects
                            .family_named(&rule.effect)
                            .filter(|family| !family.verbose_name_template.is_empty())
                            .map(|family| family.id);
                        let effect_id = match existing {
                            Some(effect_id) => effect_id,
                            None => self.effects.ensure_family(
                                &rule.effect,
                                &format!("%b1 {} +{{1}}", rule.effect),
                                None,
                                1,
                            )?,
                        };
                        self.effects.ensure_stat(effect_id, stat, Some(bonus_type), 1, None, 0)?;
                        effect_id
                    } else if let Some(grouping) =
                        self.groupings.by_stat_set.values().flatten().find(|grouping| grouping.name == rule.effect)
                    {
                        self.effects.ensure_indexed_grouping(grouping, self.groupings)?
                    } else {
                        self.effects.ensure_group(&rule.effect)?
                    };
                    let family = self.effects.family(effect_id).expect("prose effect cached");
                    let linked_type = (family.is_stat || family.uses_link_type).then_some(bonus_type);
                    self.effects.insert_link(
                        EffectOwner::SetBonusTier,
                        tier_id,
                        effect_id,
                        linked_type,
                        (Some(rule.value), None),
                        first_order,
                    )?;
                    self.grouping_answers.insert(effect_id);
                    self.matched_set_tier_prose.insert(super::SetTierProseMatch {
                        set_name: set_name.to_string(),
                        equipped_count: tier.equipped_count,
                        description: description.to_string(),
                    });
                    return Ok(1);
                }
                let family_name = self.buff_resolver.set_tier_prose_name(set_name, tier.equipped_count, description)?;
                let existing = self
                    .effects
                    .family_named(&family_name)
                    .map(|family| (family.id, family.verbose_name_template.clone()));
                let effect_id = match existing {
                    Some((id, verbose_name_template)) if verbose_name_template == description => id,
                    Some(_) => anyhow::bail!(
                        "set {set_name:?} tier {} prose {description:?} collides with family {family_name:?}; add a [names] entry",
                        tier.equipped_count
                    ),
                    None => self.effects.ensure_family( &family_name, description, None, 0)?,
                };
                self.effects.insert_link(
                    EffectOwner::SetBonusTier,
                    tier_id,
                    effect_id,
                    None,
                    (None, None),
                    first_order,
                )?;
                return Ok(1);
            }
        }
        Ok(links.len())
    }

    fn collapse_set_tier_stat_groups(&mut self, links: Vec<DerivedEffectLink>) -> Result<Vec<DerivedEffectLink>> {
        let mut remaining: Vec<Option<DerivedEffectLink>> = links.into_iter().map(Some).collect();
        let mut group_names = EFFECT_MAP.group_names();
        group_names
            .sort_by_key(|name| std::cmp::Reverse(EFFECT_MAP.group_members(name).expect("declared group").len()));
        for group_name in group_names {
            let members = EFFECT_MAP.group_members(&group_name).expect("declared group");
            if members.len() < 2 {
                continue;
            }
            let member_ids: Vec<i64> =
                members.iter().map(|name| Stat::by_name(name).expect("validated group member").id).collect();
            for first_index in 0..remaining.len() {
                let Some(first) = remaining[first_index].as_ref() else { continue };
                if first.effect_id != member_ids[0] {
                    continue;
                }
                let indices: Vec<Vec<usize>> = member_ids
                    .iter()
                    .map(|member_id| {
                        remaining
                            .iter()
                            .enumerate()
                            .filter_map(|(index, candidate)| {
                                candidate
                                    .as_ref()
                                    .filter(|candidate| {
                                        candidate.effect_id == *member_id
                                            && candidate.bonus_type == first.bonus_type
                                            && (candidate.value, candidate.value2) == (first.value, first.value2)
                                    })
                                    .map(|_| index)
                            })
                            .collect()
                    })
                    .collect();
                if indices.iter().any(|matches| matches.len() != 1) {
                    continue;
                }
                let group_id = self.effects.ensure_group(&group_name)?;
                let first_link = remaining[first_index].as_mut().expect("first member remains");
                first_link.effect_id = group_id;
                for index in indices.into_iter().flatten().filter(|index| *index != first_index) {
                    remaining[index] = None;
                }
            }
        }
        Ok(remaining.into_iter().flatten().collect())
    }

    pub(super) fn write_sentient_gems(&mut self, path: &Path, report: &mut BuildReport) -> Result<()> {
        if !path.is_file() {
            return Ok(());
        }
        for gem in sentient_gems::parse(path)? {
            report.sentient_gem_count += self.transaction.execute(
                "INSERT INTO sentient_gems (name, icon, description, slug) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(name) DO NOTHING",
                params![
                    gem.name,
                    trimmed_non_empty(gem.icon.as_deref()),
                    trimmed_non_empty(gem.description.as_deref()),
                    crate::tree::xml_entity_slug("sentient_gems", None, &gem.name, None)
                ],
            )?;
        }
        Ok(())
    }

    pub(super) fn link_pending_set_members(&mut self) -> Result<()> {
        let pending_item_links = std::mem::take(&mut self.pending_set_item_links);
        self.insert_set_member_links(
            pending_item_links,
            "INSERT OR IGNORE INTO set_bonus_items (set_id, item_id) VALUES (?1, ?2)",
        )?;
        let pending_augment_links = std::mem::take(&mut self.pending_set_augment_links);
        self.insert_set_member_links(
            pending_augment_links,
            "INSERT OR IGNORE INTO set_bonus_augments (set_id, augment_id) VALUES (?1, ?2)",
        )?;
        let pending_option_links = std::mem::take(&mut self.pending_set_option_links);
        self.insert_set_member_links(
            pending_option_links,
            "INSERT OR IGNORE INTO item_augment_slot_option_sets (set_id, option_id) VALUES (?1, ?2)",
        )
    }

    fn insert_set_member_links(&self, pending_links: Vec<(i64, String)>, insert_sql: &str) -> Result<()> {
        for (member_id, set_name) in pending_links {
            if let Some(set_id) = self.written.set_bonus_ids_by_name.get(&set_name) {
                self.transaction.execute(insert_sql, params![set_id, member_id])?;
            }
        }
        Ok(())
    }
}
