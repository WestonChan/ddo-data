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
        for set_bonus in set_bonus_file.set_bonuses {
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
            let mut tier_ids = HashMap::new();
            for tier in set_bonus.tiers {
                let tier_id = match tier_ids.entry(tier.equipped_count) {
                    std::collections::hash_map::Entry::Occupied(entry) => *entry.get(),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        self.transaction.execute(
                            "INSERT INTO set_bonus_tiers (set_id, equipped_count) VALUES (?1, ?2)",
                            params![set_id, tier.equipped_count],
                        )?;
                        *entry.insert(self.transaction.last_insert_rowid())
                    }
                };
                if let Some(description) = trimmed_non_empty(tier.description.as_deref()) {
                    self.written.set_tier_descriptions_by_id.entry(tier_id).or_insert_with(|| description.to_string());
                }
                self.pending_derived_effects.push(super::PendingDerivedEffects {
                    owner_kind: super::effects::EffectOwner::SetBonusTier,
                    owner_id: tier_id,
                    owner_name: set_name.to_string(),
                    family: None,
                    source: super::PendingEffectSource::SetTier(tier),
                });
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

    pub(super) fn write_set_tier_prose(
        &mut self,
        set_name: &str,
        tier: &SetBonusTier,
    ) -> Result<Vec<DerivedEffectLink>> {
        if let Some(description) = trimmed_non_empty(tier.description.as_deref()) {
            if let Some(rule) = EFFECT_MAP.set_tier_prose_link(description, self.groupings)? {
                let bonus_type = rule
                    .bonus_type
                    .as_deref()
                    .map(|name| ddo_model::enums::BonusType::parse(name).expect("validated prose type"));
                let effect_id = if bonus_type.is_none() {
                    if let Some(family) = self.effects.family_named(&rule.effect) {
                        anyhow::ensure!(
                            family.amount_count == 0 && !family.is_stat && !self.effects.has_stats(family.id),
                            "prose line {description:?} names {:?}, which is no longer text-only",
                            rule.effect
                        );
                        family.id
                    } else {
                        let effect = self.groupings.effect_named(&rule.effect).expect("validated prose target");
                        self.effects.ensure_family(
                            &effect.name,
                            &effect.verbose_name_template,
                            effect.description_template.as_deref(),
                            0,
                        )?
                    }
                } else if let Some(stat) = Stat::by_name(&rule.effect) {
                    let existing = self
                        .effects
                        .family_named(&rule.effect)
                        .filter(|family| !family.verbose_name_template.is_empty())
                        .map(|family| family.id);
                    let effect_id = match existing {
                        Some(effect_id) => effect_id,
                        None => {
                            self.effects.ensure_family(&rule.effect, &format!("%b1 {} +{{1}}", rule.effect), None, 1)?
                        }
                    };
                    self.effects.ensure_stat(effect_id, stat, bonus_type, 1, None, 0)?;
                    effect_id
                } else if let Some(grouping) = self.groupings.grouping_named(&rule.effect) {
                    self.effects.ensure_indexed_effect(grouping)?
                } else {
                    self.effects.ensure_group(&rule.effect)?
                };
                let family = self.effects.family(effect_id).expect("prose effect cached");
                let linked_type = bonus_type.filter(|_| family.is_stat || family.uses_link_type);
                if bonus_type.is_some() {
                    self.grouping_answers.insert(effect_id);
                }
                self.matched_set_tier_prose.insert(super::SetTierProseMatch {
                    set_name: set_name.to_string(),
                    equipped_count: tier.equipped_count,
                    description: description.to_string(),
                });
                let link = DerivedEffectLink {
                    alias: None,
                    effect_id,
                    bonus_type: linked_type,
                    value: rule.value,
                    value2: None,
                    is_split_line: false,
                };
                return Ok(vec![link]);
            }
            let family_name = self.buff_resolver.set_tier_prose_name(set_name, tier.equipped_count, description)?;
            let existing =
                self.effects.family_named(&family_name).map(|family| (family.id, family.verbose_name_template.clone()));
            let effect_id = match existing {
                    Some((id, verbose_name_template)) if verbose_name_template == description => id,
                    Some(_) => anyhow::bail!(
                        "set {set_name:?} tier {} prose {description:?} collides with family {family_name:?}; add a [names] entry",
                        tier.equipped_count
                    ),
                    None => self.effects.ensure_family( &family_name, description, None, 0)?,
                };
            return Ok(vec![DerivedEffectLink {
                alias: None,
                effect_id,
                bonus_type: None,
                value: None,
                value2: None,
                is_split_line: false,
            }]);
        }
        Ok(Vec::new())
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
