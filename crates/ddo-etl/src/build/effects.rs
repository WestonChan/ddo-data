use crate::map::buff::split_display_title;
use crate::map::effect_map::EFFECT_MAP;
use crate::xml::item_buffs::ItemBuffDefinition;
use anyhow::{ensure, Context, Result};
use ddo_model::enums::BonusType;
pub(super) use ddo_model::enums::EffectOwner;
use ddo_model::stats::{Stat, STATS};
use rusqlite::{params, Transaction};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Copy)]
pub(super) struct BonusRule {
    pub(super) stat: &'static Stat,
    pub(super) bonus_type: Option<BonusType>,
    pub(super) amount_from: i64,
    pub(super) constant: Option<i64>,
}

#[derive(Clone)]
pub(super) struct CachedEffect {
    pub(super) id: i64,
    pub(super) name: String,
    pub(super) verbose_name_template: String,
    pub(super) description_template: Option<String>,
    pub(super) amount_count: i64,
    pub(super) uses_link_type: bool,
    pub(super) is_stat: bool,
    pub(super) is_group: bool,
    default_value: Option<i64>,
    default_value2: Option<i64>,
    has_stats: bool,
}

type StatIdentity = (i64, i64, Option<i64>);
type StatAmount = (i64, Option<i64>);

pub(super) struct EffectCache<'a> {
    transaction: &'a Transaction<'a>,
    groupings: &'a crate::groupings::Groupings,
    families_by_name: HashMap<String, i64>,
    families_by_id: HashMap<i64, CachedEffect>,
    stats: HashMap<StatIdentity, StatAmount>,
    link_types_by_family: HashMap<i64, Option<BonusType>>,
}

fn stat_templates_share_fact(
    dynamic_text: &str,
    dynamic_description: Option<&str>,
    fixed_text: &str,
    fixed_description: Option<&str>,
) -> bool {
    let untyped_text = dynamic_text.replace("%b1", "").split_whitespace().collect::<Vec<_>>().join(" ");
    if untyped_text != fixed_text {
        return false;
    }
    BonusType::ALL.iter().any(|bonus_type| {
        dynamic_description.map(|description| description.replace("%b1", bonus_type.name())).as_deref()
            == fixed_description
    })
}

impl<'a> EffectCache<'a> {
    pub(super) fn refresh_family(&mut self, family_id: i64) -> Result<()> {
        let (name, verbose_name_template, description_template, default_value, default_value2): (
            String,
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<i64>,
        ) = self.transaction.query_row(
            "SELECT name, verbose_name_template, description_template, default_value, default_value2
               FROM effects WHERE id = ?1",
            [family_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )?;
        let family = self.families_by_id.get_mut(&family_id).expect("corrected effect is cached");
        let renamed = family.name != name;
        if renamed {
            self.families_by_name.remove(&family.name);
            self.families_by_name.insert(name.clone(), family_id);
        }
        family.name = name;
        family.verbose_name_template = verbose_name_template.unwrap_or_default();
        family.description_template = description_template;
        family.amount_count = amount_count(&family.verbose_name_template, family.description_template.as_deref());
        family.uses_link_type = family.verbose_name_template.contains("%b1")
            || family.description_template.as_deref().is_some_and(|template| template.contains("%b1"));
        family.default_value = default_value;
        family.default_value2 = default_value2;
        family.has_stats = false;
        self.stats.retain(|(effect_id, _, _), _| *effect_id != family_id);
        let mut statement = self.transaction.prepare(
            "SELECT target_effect_id, bonus_type_id, amount_from, constant
               FROM effect_bonuses WHERE effect_id = ?1",
        )?;
        for row in statement.query_map([family_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<i64>>(3)?,
            ))
        })? {
            let (target_id, bonus_type_id, amount_from, constant) = row?;
            self.stats.insert((family_id, target_id, bonus_type_id), (amount_from, constant));
            family.has_stats = true;
        }
        if renamed {
            self.write_family_aliases(family_id)?;
        }
        Ok(())
    }
    pub(super) fn new(transaction: &'a Transaction<'a>, groupings: &'a crate::groupings::Groupings) -> Self {
        let mut families_by_name = HashMap::new();
        let mut families_by_id = HashMap::new();
        for stat in STATS {
            families_by_name.insert(stat.name.to_string(), stat.id);
            families_by_id.insert(
                stat.id,
                CachedEffect {
                    id: stat.id,
                    name: stat.name.to_string(),
                    verbose_name_template: String::new(),
                    description_template: None,
                    amount_count: 1,
                    uses_link_type: false,
                    is_stat: true,
                    is_group: false,
                    default_value: None,
                    default_value2: None,
                    has_stats: true,
                },
            );
        }
        Self {
            transaction,
            groupings,
            families_by_name,
            families_by_id,
            stats: HashMap::new(),
            link_types_by_family: HashMap::new(),
        }
    }
    pub(super) fn family_named(&self, name: &str) -> Option<&CachedEffect> {
        self.families_by_name.get(name).and_then(|id| self.families_by_id.get(id))
    }

    pub(super) fn family(&self, id: i64) -> Option<&CachedEffect> {
        self.families_by_id.get(&id)
    }

    pub(super) fn stat_uses_link_type(&self, family_id: i64, stat: &Stat) -> bool {
        let family = self.family(family_id).expect("family cached before its rules");
        let stat_id = self.groupings.effect_named(stat.name).map(|effect| effect.id);
        family.uses_link_type
            && !self.groupings.effect_named(&family.name).is_some_and(|definition| {
                definition.bonuses.iter().any(|bonus| bonus.bonus_type.is_none())
                    && definition.bonuses.iter().any(|bonus| {
                        bonus.bonus_type.is_some()
                            && stat_id.is_some_and(|id| {
                                bonus.target_id == id || self.groupings.by_id[&bonus.target_id].stat_ids.contains(&id)
                            })
                    })
            })
    }

    pub(super) fn families(&self) -> impl Iterator<Item = &CachedEffect> {
        self.families_by_id.values()
    }

    pub(super) fn has_stats(&self, family_id: i64) -> bool {
        self.family(family_id).is_some_and(|family| family.has_stats)
    }

    pub(super) fn unique_link_bonus_type(&self, family_id: i64) -> Option<BonusType> {
        self.link_types_by_family.get(&family_id).copied().flatten()
    }

    pub(super) fn stat(&self, family_id: i64, stat_id: i64, bonus_type_id: Option<i64>) -> Option<(i64, Option<i64>)> {
        self.stats.get(&(family_id, stat_id, bonus_type_id)).copied()
    }

    pub(super) fn ensure_family(
        &mut self,
        name: &str,
        verbose_name_template: &str,
        description_template: Option<&str>,
        count: i64,
    ) -> Result<i64> {
        let family_id = self.ensure_family_templates(name, verbose_name_template, description_template, count)?;
        if let Some(default_value) = EFFECT_MAP.definition_defaults.get(name) {
            self.set_defaults(family_id, (Some(*default_value), None))?;
        }
        Ok(family_id)
    }

    fn ensure_family_templates(
        &mut self,
        name: &str,
        verbose_name_template: &str,
        description_template: Option<&str>,
        count: i64,
    ) -> Result<i64> {
        let paragraph = description_template.is_none()
            && verbose_name_template.split_whitespace().count() >= 7
            && (verbose_name_template
                .trim_end()
                .chars()
                .last()
                .is_some_and(|character| matches!(character, '.' | '!' | '?'))
                || (verbose_name_template.len() > 75 && verbose_name_template.contains(';')));
        let concise_template = if paragraph {
            Some(match count {
                0 => name.to_string(),
                _ => format!("{name} +{{1}}"),
            })
        } else {
            None
        };
        let description_template = if paragraph { Some(verbose_name_template) } else { description_template };
        let description_template =
            EFFECT_MAP.description_templates.get(name).map(String::as_str).or(description_template);
        let trimmed_description = description_template
            .filter(|_| EFFECT_MAP.description_trim_period.contains(name))
            .map(|template| template.trim_end_matches('.').to_string());
        let description_template = trimmed_description.as_deref().or(description_template);
        let verbose_name_template = concise_template.as_deref().unwrap_or(verbose_name_template);
        let signed_template =
            verbose_name_template.replace("{1}", "+{1}").replace("++{1}", "+{1}").replace("-+{1}", "-{1}");
        let verbose_name_template = EFFECT_MAP.line_templates.get(name).map(String::as_str).unwrap_or(&signed_template);
        let count =
            if EFFECT_MAP.line_templates.contains_key(name) || EFFECT_MAP.description_templates.contains_key(name) {
                amount_count(verbose_name_template, description_template)
            } else {
                count
            };
        if let Some(existing) = self.family_named(name) {
            if existing.is_stat && existing.verbose_name_template.is_empty() {
                let stat_id = existing.id;
                ensure!(count == 1, "stat effect {name:?} must read exactly one value");
                validate_templates(name, verbose_name_template, description_template, count)?;
                self.transaction.execute(
                    "UPDATE effects SET verbose_name_template = ?2, description_template = ?3,
                        home_bonus_type_id = COALESCE(home_bonus_type_id, ?4),
                        set_bonus_line_template = ?5 WHERE id = ?1",
                    params![
                        stat_id,
                        verbose_name_template,
                        description_template,
                        ddo_model::bonus_type_wording::stat_home_type(name).map(BonusType::id),
                        EFFECT_MAP.set_bonus_line_templates.get(name),
                    ],
                )?;
                let family = self.families_by_id.get_mut(&stat_id).expect("seed stat cached");
                family.verbose_name_template = verbose_name_template.to_string();
                family.description_template = description_template.map(str::to_string);
                family.amount_count = count;
                family.uses_link_type = verbose_name_template.contains("%b1")
                    || description_template.is_some_and(|template| template.contains("%b1"));
                return Ok(stat_id);
            }
            let incoming_uses_link_type = verbose_name_template.contains("%b1")
                || description_template.is_some_and(|template| template.contains("%b1"));
            if existing.is_stat && count == 1 && existing.uses_link_type != incoming_uses_link_type {
                let (dynamic_text, dynamic_description, fixed_text, fixed_description) = if existing.uses_link_type {
                    (
                        existing.verbose_name_template.as_str(),
                        existing.description_template.as_deref(),
                        verbose_name_template,
                        description_template,
                    )
                } else {
                    (
                        verbose_name_template,
                        description_template,
                        existing.verbose_name_template.as_str(),
                        existing.description_template.as_deref(),
                    )
                };
                ensure!(
                    stat_templates_share_fact(dynamic_text, dynamic_description, fixed_text, fixed_description),
                    "effect {name:?} has conflicting stat templates"
                );
                if existing.uses_link_type {
                    return Ok(existing.id);
                }
                let stat_id = existing.id;
                validate_templates(name, verbose_name_template, description_template, count)?;
                self.transaction.execute(
                    "UPDATE effects SET verbose_name_template = ?2, description_template = ?3,
                        home_bonus_type_id = COALESCE(home_bonus_type_id, ?4),
                        set_bonus_line_template = ?5 WHERE id = ?1",
                    params![
                        stat_id,
                        verbose_name_template,
                        description_template,
                        ddo_model::bonus_type_wording::stat_home_type(name).map(BonusType::id),
                        EFFECT_MAP.set_bonus_line_templates.get(name)
                    ],
                )?;
                let family = self.families_by_id.get_mut(&stat_id).expect("seed stat cached");
                family.verbose_name_template = verbose_name_template.to_string();
                family.description_template = description_template.map(str::to_string);
                family.uses_link_type = true;
                return Ok(stat_id);
            }
            ensure!(
                existing.verbose_name_template == verbose_name_template
                    && existing.description_template.as_deref() == description_template
                    && existing.amount_count == count,
                "effect {name:?} has conflicting templates or amount counts"
            );
            return Ok(existing.id);
        }
        ensure!(!name.contains(" — "), "effect family {name:?} contains an em dash; add a [names] entry");
        validate_templates(name, verbose_name_template, description_template, count)?;
        let identity =
            crate::tree::xml_entity_slug("effects", EFFECT_MAP.named_effect_ids.get(name).copied(), name, None);
        let indexed = self.groupings.effect_named(name);
        self.transaction
            .execute(
                "INSERT INTO effects (id, name, verbose_name_template, description_template, home_bonus_type_id,
                                  set_bonus_line_template, slug, stat_set_verdict, stat_set_reason)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    EFFECT_MAP.named_effect_ids.get(name),
                    name,
                    verbose_name_template,
                    description_template,
                    indexed
                        .and_then(|effect| effect.home_bonus_type.as_deref())
                        .and_then(BonusType::parse)
                        .map(BonusType::id),
                    EFFECT_MAP.set_bonus_line_templates.get(name),
                    identity,
                    indexed.and_then(|effect| effect.stat_set_verdict.as_deref()),
                    indexed.and_then(|effect| effect.stat_set_reason.as_deref())
                ],
            )
            .with_context(|| {
                match self
                    .transaction
                    .query_row("SELECT name FROM effects WHERE slug = ?1", [&identity], |row| row.get::<_, String>(0))
                {
                    Ok(existing_name) => format!("effects slug collision {identity:?}: {existing_name:?} and {name:?}"),
                    Err(_) => format!("inserting effect {name:?}"),
                }
            })?;
        let id = self.transaction.last_insert_rowid();
        self.families_by_name.insert(name.to_string(), id);
        self.families_by_id.insert(
            id,
            CachedEffect {
                id,
                name: name.to_string(),
                verbose_name_template: verbose_name_template.to_string(),
                description_template: description_template.map(str::to_string),
                amount_count: count,
                uses_link_type: verbose_name_template.contains("%b1")
                    || description_template.is_some_and(|template| template.contains("%b1")),
                is_stat: false,
                is_group: false,
                default_value: None,
                default_value2: None,
                has_stats: false,
            },
        );
        self.write_family_aliases(id)?;
        Ok(id)
    }

    pub(super) fn ensure_stat(
        &mut self,
        family_id: i64,
        stat: &'static Stat,
        bonus_type: Option<BonusType>,
        amount_from: i64,
        constant: Option<i64>,
        sort_order: usize,
    ) -> Result<()> {
        let key = (family_id, stat.id, bonus_type.map(BonusType::id));
        if let Some(existing) = self.stats.get(&key) {
            ensure!(
                *existing == (amount_from, constant),
                "effect {family_id} stat {:?} has conflicting amount sources",
                stat.name
            );
            return Ok(());
        }
        let family = self.family(family_id).expect("family is cached before its stat rows");
        ensure!(amount_from <= family.amount_count, "effect {family_id} stat {:?} reads beyond its slots", stat.name);
        if family.is_stat {
            ensure!(
                family_id == stat.id && amount_from == 1 && constant.is_none(),
                "direct stat effect {family_id} must read its own first value"
            );
            self.stats.insert(key, (amount_from, constant));
            return Ok(());
        }
        ensure!(
            !self.stat_uses_link_type(family_id, stat) || bonus_type.is_none(),
            "effect {family_id} stat {:?} has its bonus type at the wrong level",
            stat.name
        );
        self.transaction.execute(
            "INSERT INTO effect_bonuses (effect_id, target_effect_id, bonus_type_id, amount_from, constant, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![family_id, stat.id, bonus_type.map(BonusType::id), amount_from, constant, sort_order as i64],
        )?;
        self.stats.insert(key, (amount_from, constant));
        let family = self.families_by_id.get_mut(&family_id).expect("family is cached before its stat rows");
        family.has_stats = true;
        family.uses_link_type |= bonus_type.is_none();
        Ok(())
    }

    pub(super) fn ensure_group(&mut self, name: &str) -> Result<i64> {
        let members = EFFECT_MAP.group_members(name).expect("declared group has members");
        let description = name
            .strip_suffix(" Skills")
            .filter(|ability| EFFECT_MAP.skill_ability_groups.skills.contains_key(*ability))
            .map(|ability| format!("{{1}} %b1 bonus to all {ability} based skills."));
        let template = EFFECT_MAP.line_templates.get(name).cloned().unwrap_or_else(|| format!("%b1 {name} +{{1}}"));
        self.ensure_flat_group(name, members, &template, description.as_deref(), None)
    }

    pub(super) fn ensure_flat_group(
        &mut self,
        name: &str,
        members: &[String],
        verbose_name_template: &str,
        description: Option<&str>,
        set_bonus_line_template: Option<&str>,
    ) -> Result<i64> {
        let id = match self.family_named(name) {
            Some(family) => family.id,
            None => self.ensure_family(name, verbose_name_template, description, 1)?,
        };
        let family = self.family(id).expect("group family cached");
        ensure!(!family.is_stat && family.amount_count == 1, "group {name:?} must be a one-value named effect");
        let is_group = family.is_group;
        if let Some(description) = description {
            ensure!(
                family.description_template.as_deref().is_none_or(|existing| existing == description),
                "group {name:?} has a conflicting description"
            );
            if family.description_template.is_none() {
                self.transaction
                    .execute("UPDATE effects SET description_template = ?2 WHERE id = ?1", params![id, description])?;
                let family = self.families_by_id.get_mut(&id).expect("group family cached");
                family.description_template = Some(description.to_string());
                family.uses_link_type = true;
            }
        }
        if !is_group {
            self.transaction.execute("UPDATE effects SET is_group = 1 WHERE id = ?1", [id])?;
            self.families_by_id.get_mut(&id).expect("group family cached").is_group = true;
        }
        if let Some(template) = set_bonus_line_template {
            self.transaction
                .execute("UPDATE effects SET set_bonus_line_template = ?2 WHERE id = ?1", params![id, template])?;
        }
        for (sort_order, member) in members.iter().enumerate() {
            self.ensure_stat(id, Stat::by_name(member).expect("validated group member"), None, 1, None, sort_order)?;
        }
        Ok(id)
    }

    pub(super) fn ensure_indexed_effect(&mut self, definition: &crate::groupings::Grouping) -> Result<i64> {
        let index = self.groupings;
        if let Some(family) = self.family_named(&definition.name) {
            return Ok(family.id);
        }
        let id = self.ensure_family(
            &definition.name,
            &definition.verbose_name_template,
            definition.description_template.as_deref(),
            amount_count(&definition.verbose_name_template, definition.description_template.as_deref()),
        )?;
        self.transaction.execute(
            "UPDATE effects SET is_group = ?2, set_bonus_line_template = ?3, home_bonus_type_id = ?4, default_value = ?5, default_value2 = ?6 WHERE id = ?1",
            params![id, definition.is_group, definition.set_bonus_line_template,
                definition.home_bonus_type.as_deref().and_then(BonusType::parse).map(BonusType::id), definition.default_value, definition.default_value2],
        )?;
        self.families_by_id.get_mut(&id).unwrap().is_group = definition.is_group;
        for (sort_order, bonus) in definition.bonuses.iter().enumerate() {
            let target = &index.by_id[&bonus.target_id];
            let bonus_type = bonus
                .bonus_type
                .as_deref()
                .map(|name| BonusType::parse(name).context("indexed bonus type is known"))
                .transpose()?;
            if target.is_group {
                let target_id = self.ensure_indexed_effect(target)?;
                let stat = Stat::by_name(&index.by_id[&target.bonuses[0].target_id].name)
                    .context("group member is a known stat")?;
                self.ensure_group_target(
                    id,
                    target_id,
                    BonusRule { stat, bonus_type, amount_from: bonus.amount_from, constant: bonus.constant },
                    sort_order,
                )?;
            } else {
                self.ensure_stat(
                    id,
                    Stat::by_name(&target.name).context("definition target is a known stat")?,
                    bonus_type,
                    bonus.amount_from,
                    bonus.constant,
                    sort_order,
                )?;
            }
            if let Some(name) = &bonus.trigger_name {
                self.transaction.execute("INSERT OR IGNORE INTO triggers (name) VALUES (?1)", [name])?;
            }
            self.transaction.execute("UPDATE effect_bonuses SET scale=?3, rounding=?4, trigger_id=(SELECT id FROM triggers WHERE name=?5) WHERE effect_id=?1 AND sort_order=?2", params![id,sort_order as i64,bonus.scale,bonus.rounding,bonus.trigger_name])?;
        }
        self.refresh_family(id)?;
        Ok(id)
    }

    pub(super) fn fill_group_description(&mut self, group_id: i64, description: &str) -> Result<()> {
        let family = self.families_by_id.get_mut(&group_id).context("group is cached")?;
        if family.description_template.is_none() {
            self.transaction.execute(
                "UPDATE effects SET description_template = ?2 WHERE id = ?1",
                params![group_id, description],
            )?;
            family.description_template = Some(description.to_string());
        }
        Ok(())
    }

    fn ensure_group_target(&mut self, family_id: i64, group_id: i64, rule: BonusRule, sort_order: usize) -> Result<()> {
        let key = (family_id, group_id, rule.bonus_type.map(BonusType::id));
        if let Some(existing) = self.stats.get(&key) {
            ensure!(
                *existing == (rule.amount_from, rule.constant),
                "effect {family_id} group {group_id} has conflicting amounts"
            );
            return Ok(());
        }
        let family = self.family(family_id).expect("parent family cached");
        ensure!(!family.is_group && !family.is_stat, "group {family_id} cannot contain group {group_id}");
        ensure!(rule.amount_from <= family.amount_count, "effect {family_id} reads group {group_id} beyond its slots");
        self.transaction.execute(
            "INSERT INTO effect_bonuses (effect_id, target_effect_id, bonus_type_id, amount_from, constant, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                family_id,
                group_id,
                rule.bonus_type.map(BonusType::id),
                rule.amount_from,
                rule.constant,
                sort_order as i64
            ],
        )?;
        self.stats.insert(key, (rule.amount_from, rule.constant));
        let family = self.families_by_id.get_mut(&family_id).expect("parent family cached");
        family.has_stats = true;
        family.uses_link_type |= rule.bonus_type.is_none();
        Ok(())
    }

    pub(super) fn ensure_bonus_rules(&mut self, family_id: i64, rules: &[BonusRule]) -> Result<()> {
        let family = self.family(family_id).expect("family cached before its rules");
        if family.is_stat
            || self
                .groupings
                .effect_named(&family.name)
                .is_some_and(|effect| effect.stat_set_verdict.as_deref() == Some("distinct"))
        {
            for (sort_order, rule) in rules.iter().enumerate() {
                self.ensure_stat(family_id, rule.stat, rule.bonus_type, rule.amount_from, rule.constant, sort_order)?;
            }
            return Ok(());
        }
        let family_name = family.name.clone();
        let mut handled = HashSet::new();
        let mut group_names = EFFECT_MAP.group_names();
        group_names
            .sort_by_key(|name| std::cmp::Reverse(EFFECT_MAP.group_members(name).expect("declared group").len()));
        for group_name in group_names {
            let members = EFFECT_MAP.group_members(&group_name).expect("declared group");
            if members.len() == 1
                && !family_name.contains(&group_name)
                && !group_name.strip_suffix(" Skills").is_some_and(|ability| family_name == format!("Skills {ability}"))
            {
                continue;
            }
            let mut indices = Vec::new();
            for member in members {
                let Some(index) = rules
                    .iter()
                    .enumerate()
                    .find_map(|(index, rule)| (!handled.contains(&index) && rule.stat.name == member).then_some(index))
                else {
                    indices.clear();
                    break;
                };
                indices.push(index);
            }
            if indices.len() != members.len() {
                continue;
            }
            let first = rules[indices[0]];
            if !indices.iter().all(|index| {
                let rule = rules[*index];
                rule.bonus_type == first.bonus_type
                    && rule.amount_from == first.amount_from
                    && rule.constant == first.constant
            }) {
                continue;
            }
            let group_id = self.ensure_group(&group_name)?;
            if group_id != family_id {
                self.ensure_group_target(family_id, group_id, first, indices[0])?;
            }
            handled.extend(indices);
        }
        for (sort_order, rule) in rules.iter().enumerate() {
            if !handled.contains(&sort_order) {
                self.ensure_stat(family_id, rule.stat, rule.bonus_type, rule.amount_from, rule.constant, sort_order)?;
            }
        }
        Ok(())
    }

    pub(super) fn ensure_wiki_text(&mut self, source_name: &str, description: Option<&str>) -> Result<i64> {
        let family_name = source_name.split_once(" — ").map_or(source_name, |(title, _)| title);
        if let Some(existing) = self.family_named(family_name) {
            if self.has_stats(existing.id) {
                return Ok(existing.id);
            }
        }
        self.ensure_family(family_name, source_name, description, 0)
    }

    pub(super) fn ensure_text(&mut self, source_name: &str, rendered_text: &str) -> Result<i64> {
        let (verbose_name_template, description_template) = split_template(rendered_text);
        let family_name = source_name.split_once(" — ").map_or(source_name, |(title, _)| title);
        if let Some(existing) = self.family_named(family_name) {
            ensure!(
                !existing.has_stats
                    && existing.verbose_name_template == verbose_name_template
                    && existing.description_template == description_template,
                "effect family {family_name:?} has conflicting text {rendered_text:?}; add a [names] entry"
            );
        }
        let count = amount_count(&verbose_name_template, description_template.as_deref());
        self.ensure_family(family_name, &verbose_name_template, description_template.as_deref(), count)
    }

    pub(super) fn set_defaults(&mut self, family_id: i64, defaults: (Option<i64>, Option<i64>)) -> Result<()> {
        let family = self.families_by_id.get_mut(&family_id).expect("family is cached before its defaults");
        ensure!(!family.is_group, "group {} cannot carry definition defaults", family.name);
        let (first, second) = defaults;
        ensure!(
            first.is_none_or(|value| family.default_value.is_none_or(|current| current == value))
                && second.is_none_or(|value| family.default_value2.is_none_or(|current| current == value)),
            "effect {family_id} has conflicting definition defaults"
        );
        if (first.is_some() && family.default_value.is_none()) || (second.is_some() && family.default_value2.is_none())
        {
            self.transaction.execute(
                "UPDATE effects SET default_value = COALESCE(default_value, ?2),
                 default_value2 = COALESCE(default_value2, ?3) WHERE id = ?1",
                params![family_id, first, second],
            )?;
            family.default_value = family.default_value.or(first);
            family.default_value2 = family.default_value2.or(second);
        }
        Ok(())
    }

    pub(super) fn replace_family_text(&mut self, family_id: i64, verbose_name_template: &str) -> Result<()> {
        let family = self.families_by_id.get_mut(&family_id).expect("corrected family is cached");
        ensure!(!family.has_stats, "structured family text cannot be replaced");
        validate_templates(&family.name, verbose_name_template, None, 0)?;
        self.transaction.execute(
            "UPDATE effects SET verbose_name_template = ?2, description_template = NULL WHERE id = ?1",
            params![family_id, verbose_name_template],
        )?;
        family.verbose_name_template = verbose_name_template.to_string();
        family.description_template = None;
        Ok(())
    }

    pub(super) fn delete_unowned(&mut self, family_id: i64) -> Result<()> {
        if self.family(family_id).is_some_and(|family| family.is_stat) {
            return Ok(());
        }
        let deleted = self.transaction.execute(
            "DELETE FROM effects WHERE id = ?1
             AND NOT EXISTS (SELECT 1 FROM item_effects WHERE effect_id = ?1)
             AND NOT EXISTS (SELECT 1 FROM augment_effects WHERE effect_id = ?1)
             AND NOT EXISTS (SELECT 1 FROM set_bonus_tier_effects WHERE effect_id = ?1)
             AND NOT EXISTS (SELECT 1 FROM feat_effects WHERE effect_id = ?1)
             AND NOT EXISTS (SELECT 1 FROM item_augment_slot_option_effects WHERE effect_id = ?1)",
            [family_id],
        )?;
        if deleted > 0 {
            if let Some(family) = self.families_by_id.remove(&family_id) {
                self.families_by_name.remove(&family.name);
            }
            self.stats.retain(|(id, _, _), _| *id != family_id);
        }
        Ok(())
    }

    pub(super) fn merge_identical_text_family(&mut self, source_id: i64, destination_name: &str) -> Result<()> {
        let source = self.family(source_id).expect("source family is cached").clone();
        let destination = self.family_named(destination_name).expect("destination family is cached").clone();
        ensure!(
            !source.is_stat
                && !source.is_group
                && !source.has_stats
                && !destination.is_stat
                && !destination.is_group
                && !destination.has_stats
                && source.verbose_name_template == destination.verbose_name_template
                && source.description_template == destination.description_template
                && source.amount_count == destination.amount_count,
            "effect {:?} cannot merge into {destination_name:?}: their text or bonuses differ",
            source.name
        );
        for owner in [
            EffectOwner::Item,
            EffectOwner::Augment,
            EffectOwner::SetBonusTier,
            EffectOwner::Feat,
            EffectOwner::ItemAugmentSlotOption,
        ] {
            let (table, _) = owner.table();
            self.transaction.execute(
                &format!("UPDATE {table} SET effect_id = ?2 WHERE effect_id = ?1"),
                params![source_id, destination.id],
            )?;
        }
        self.transaction
            .execute("UPDATE modifiers SET effect_id = ?2 WHERE effect_id = ?1", params![source_id, destination.id])?;
        self.transaction.execute("DELETE FROM effects WHERE id = ?1", [source_id])?;
        self.families_by_name.remove(&source.name);
        self.families_by_id.remove(&source_id);
        self.link_types_by_family.remove(&source_id);
        Ok(())
    }

    pub(super) fn write_missing_linked_stat_templates(&mut self) -> Result<()> {
        let names = self
            .link_types_by_family
            .keys()
            .filter_map(|id| self.family(*id))
            .filter(|family| family.is_stat && family.verbose_name_template.is_empty())
            .map(|family| family.name.clone())
            .collect::<Vec<_>>();
        for name in names {
            self.ensure_family(&name, &format!("%b1 {name} +{{1}}"), None, 1)?;
        }
        Ok(())
    }

    pub(super) fn write_aliases(&self) -> Result<()> {
        for family in self.families_by_id.values() {
            self.write_family_aliases(family.id)?;
        }
        Ok(())
    }

    pub(super) fn write_family_aliases(&self, family_id: i64) -> Result<()> {
        if let Some(effect) = self.family(family_id).and_then(|family| self.groupings.effect_named(&family.name)) {
            for (position, alias) in effect.aliases.iter().enumerate() {
                self.transaction.execute(
                    "INSERT INTO effect_aliases (effect_id, name, slug, verbose_name_template, sort_order) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![family_id, alias.name, alias.slug, alias.verbose_name_template, position as i64],
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn alias_named(&self, family_id: i64, name: &str) -> Option<&str> {
        self.groupings
            .effect_named(&self.family(family_id)?.name)?
            .aliases
            .iter()
            .find(|alias| alias.name == name && alias.verbose_name_template.is_some())
            .map(|alias| alias.name.as_str())
    }

    pub(super) fn insert_link(
        &mut self,
        (owner, owner_id): (EffectOwner, i64),
        family_id: i64,
        alias: Option<&str>,
        bonus_type: Option<BonusType>,
        amounts: (Option<i64>, Option<i64>),
        sort_order: usize,
    ) -> Result<()> {
        let family = self.family(family_id).expect("every writer family is cached");
        if family.is_stat {
            ensure!(bonus_type.is_some(), "stat link {:?} needs a bonus type", family.name);
            ensure!(amounts.1.is_none(), "stat link {:?} cannot carry a second value", family.name);
        } else {
            validate_link(&family.name, family.amount_count, family.uses_link_type, bonus_type, amounts)?;
        }
        write_link(self.transaction, (owner, owner_id), family_id, alias, bonus_type, amounts, sort_order)?;
        if let Some(bonus_type) = bonus_type {
            self.link_types_by_family
                .entry(family_id)
                .and_modify(|existing| {
                    if *existing != Some(bonus_type) {
                        *existing = None;
                    }
                })
                .or_insert(Some(bonus_type));
        }
        Ok(())
    }
}

pub(crate) fn amount_count(verbose_name_template: &str, description_template: Option<&str>) -> i64 {
    let templates = [verbose_name_template, description_template.unwrap_or("")];
    if templates.iter().any(|template| template.contains("{2}")) {
        2
    } else if templates.iter().any(|template| template.contains("{1}")) {
        1
    } else {
        0
    }
}

pub(crate) fn validate_templates(
    name: &str,
    verbose_name_template: &str,
    description_template: Option<&str>,
    count: i64,
) -> Result<()> {
    ensure!((0..=2).contains(&count), "effect {name:?}: amount_count {count} is outside 0..=2");
    let template_text = format!("{verbose_name_template}{}", description_template.unwrap_or(""));
    for slot in 1..=2 {
        let has_slot = template_text.contains(&format!("{{{slot}}}"));
        ensure!(
            has_slot == (slot <= count),
            "effect {name:?}: template placeholders do not match amount_count {count}"
        );
    }
    ensure!(
        !template_text.contains("%v1") && !template_text.contains("%v2"),
        "effect {name:?}: unconverted amount placeholder"
    );
    Ok(())
}

fn validate_link(
    name: &str,
    count: i64,
    uses_link_type: bool,
    bonus_type: Option<BonusType>,
    amounts: (Option<i64>, Option<i64>),
) -> Result<()> {
    let (value, value2) = amounts;
    let present_count = i64::from(value.is_some()) + i64::from(value2.is_some());
    ensure!(present_count <= count, "effect {name:?} link has {present_count} values, at most {count} allowed");
    ensure!(uses_link_type == bonus_type.is_some(), "effect {name:?} link has its bonus type at the wrong level");
    Ok(())
}

fn write_link(
    transaction: &Transaction,
    (owner, owner_id): (EffectOwner, i64),
    effect_id: i64,
    alias: Option<&str>,
    bonus_type: Option<BonusType>,
    amounts: (Option<i64>, Option<i64>),
    sort_order: usize,
) -> Result<()> {
    let (value, value2) = amounts;
    let (table_name, owner_column) = owner.table();
    transaction.prepare_cached(
        &format!(
            "INSERT INTO {table_name} ({owner_column}, effect_id, alias, bonus_type_id, value, value2, sort_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
        ),
    )?.execute(params![owner_id, effect_id, alias, bonus_type.map(BonusType::id), value, value2, sort_order as i64])?;
    Ok(())
}

pub(super) fn split_template(display_text: &str) -> (String, Option<String>) {
    let converted = display_text.replace("%v1", "{1}").replace("%v2", "{2}");
    match split_display_title(&converted) {
        Some((text, description)) => (text.trim().to_string(), Some(description.trim().to_string())),
        None => (converted.trim().to_string(), None),
    }
}

pub(super) fn insert_tier_groups(effects: &EffectCache<'_>, tier_groups: &BTreeMap<String, Vec<String>>) -> Result<()> {
    let transaction = effects.transaction;
    for (group_name, steps) in tier_groups {
        ensure!(steps.len() >= 2, "tier group {group_name:?} needs at least two steps");
        let present_steps: Vec<_> = steps
            .iter()
            .filter_map(|step_name| effects.family_named(step_name).map(|family| (step_name, family.id)))
            .collect();
        if present_steps.len() < 2 {
            continue;
        }
        transaction.execute("INSERT INTO effect_tier_groups (name) VALUES (?1)", [group_name])?;
        let group_id = transaction.last_insert_rowid();
        for (index, (step_name, family_id)) in present_steps.iter().enumerate() {
            let changed_rows = transaction.execute(
                "UPDATE effects SET tier_group_id = ?1, tier = ?2 WHERE id = ?3 AND tier_group_id IS NULL",
                params![group_id, index as i64 + 1, family_id],
            )?;
            ensure!(
                changed_rows == 1,
                "tier group {group_name:?} step {step_name:?} has no unique owned effect family"
            );
        }
    }
    Ok(())
}

pub(super) fn insert_triggers(
    transaction: &Transaction,
    definitions: &HashMap<String, ItemBuffDefinition>,
) -> Result<()> {
    let mut names = std::collections::BTreeSet::from([
        "On Hit".to_string(),
        "On Critical Hit".to_string(),
        "On Vorpal".to_string(),
        "When Hit".to_string(),
        "When Missed in Melee".to_string(),
        "On Kill".to_string(),
    ]);
    for definition in definitions.values() {
        for line in definition.display_text.lines() {
            let candidate = line.split(':').next().unwrap_or("").trim();
            if (candidate.starts_with("On ") || candidate.starts_with("When "))
                && candidate.len() <= 50
                && !candidate.eq_ignore_ascii_case("On items")
                && !candidate.contains('.')
            {
                if candidate.to_ascii_lowercase().starts_with("on critical hits,") {
                    names.insert("On Critical Hit".to_string());
                } else if !candidate.contains(',') {
                    let canonical = names.iter().find(|name| name.eq_ignore_ascii_case(candidate)).cloned();
                    names.insert(canonical.unwrap_or_else(|| candidate.to_string()));
                }
            }
        }
    }
    for name in names {
        transaction.execute("INSERT INTO triggers (name) VALUES (?1)", [name])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{EffectCache, EffectOwner};
    use ddo_model::enums::BonusType;
    use ddo_model::stats::Stat;
    use rusqlite::Connection;

    #[test]
    fn refreshing_an_effect_after_its_last_bonus_is_removed_clears_has_stats() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        ddo_model::seeds::insert_all(&db).unwrap();
        let transaction = db.transaction().unwrap();
        let mut effects = EffectCache::new(&transaction, crate::groupings::fixture_groupings());
        let effect = effects.ensure_family("Fixed Strength", "Fixed Strength: Grants Strength", None, 0).unwrap();
        let strength = ddo_model::stats::STATS.iter().find(|stat| stat.name == "Strength").unwrap();
        effects.ensure_stat(effect, strength, Some(BonusType::Enhancement), 0, Some(2), 0).unwrap();
        assert!(effects.has_stats(effect));
        transaction.execute("DELETE FROM effect_bonuses WHERE effect_id = ?1", [effect]).unwrap();
        effects.refresh_family(effect).unwrap();
        assert!(!effects.has_stats(effect));
    }

    #[test]
    fn indexed_grouping_is_the_same_with_an_empty_or_populated_cache() {
        let index = crate::groupings::fixture_groupings();
        let stats =
            ["Cold Spell Power", "Electric Spell Power", "Sonic Spell Power"].map(str::to_string).into_iter().collect();
        let [grouping] = index.by_stat_set[&stats].as_slice() else {
            panic!("one Frozen Thunderstorm grouping");
        };
        assert_eq!(grouping.name, "Power of the Frozen Thunderstorm");
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        ddo_model::seeds::insert_all(&db).unwrap();
        let transaction = db.transaction().unwrap();
        let mut effects = EffectCache::new(&transaction, crate::groupings::fixture_groupings());
        let first = effects.ensure_indexed_effect(grouping).unwrap();
        let second = effects.ensure_indexed_effect(grouping).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            transaction
                .query_row("SELECT COUNT(*) FROM effect_bonuses WHERE effect_id = ?1", [first], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            3
        );
    }

    #[test]
    fn untitled_paragraph_becomes_description_with_a_concise_line_template() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        let transaction = db.transaction().unwrap();
        let mut effects = EffectCache::new(&transaction, crate::groupings::fixture_groupings());
        let paragraph = "Any creature struck by this weapon must succeed on a DC 17 Will save or be paralyzed. The target may try again to end the effect later.";
        let effect_id = effects.ensure_family("Paralyzing", paragraph, None, 0).unwrap();
        let templates: (String, Option<String>) = transaction
            .query_row(
                "SELECT verbose_name_template, description_template FROM effects WHERE id = ?1",
                [effect_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(templates, ("Paralyzing".to_string(), Some(paragraph.to_string())));
    }

    #[test]
    fn armor_bonus_definition_uses_its_amount_in_the_description() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        ddo_model::seeds::insert_all(&db).unwrap();
        let transaction = db.transaction().unwrap();
        let mut effects = EffectCache::new(&transaction, crate::groupings::fixture_groupings());
        let source = "This item surrounds the wearer with an invisible but tangible field of force, granting %b1 armor bonus to AC, just as though he were wearing armor.";
        let effect_id = effects.ensure_family("Armor Bonus", "Armor Bonus +{1}", Some(source), 1).unwrap();
        let description: String = transaction
            .query_row("SELECT description_template FROM effects WHERE id = ?1", [effect_id], |row| row.get(0))
            .unwrap();
        assert!(description.contains("granting {1} armor bonus"), "{description}");
    }

    #[test]
    fn repair_amplification_description_has_no_space_before_its_closing_parenthesis() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        ddo_model::seeds::insert_all(&db).unwrap();
        let transaction = db.transaction().unwrap();
        let mut effects = EffectCache::new(&transaction, crate::groupings::fixture_groupings());
        let source = "This effect amplifies all incoming repair energy healing by {1} (%b1 bonus ). Includes spells, potions, and other effects.";
        let effect_id =
            effects.ensure_family("Repair Amplification", "%b1 Repair Amplification {1}", Some(source), 1).unwrap();
        let description: String = transaction
            .query_row("SELECT description_template FROM effects WHERE id = ?1", [effect_id], |row| row.get(0))
            .unwrap();
        assert_eq!(description, source.replace("bonus )", "bonus)"));
    }

    #[test]
    fn fixed_dc_variants_share_the_wild_frenzy_family_and_keep_a_default() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        let transaction = db.transaction().unwrap();
        let mut effects = EffectCache::new(&transaction, crate::groupings::fixture_groupings());
        let base_description = "This weapon has a tendency to drive those it strikes insane. On an attack roll of 20 which is confirmed as a critical hit the target will go wild and attack its own allies for 15 seconds if it fails a DC 25 Will save. Enemies driven wild in this way, however, have a chance of coming to their senses if damaged.";
        let stronger_description = base_description.replace("DC 25", "DC 122");
        let base = effects.ensure_family("Wild Frenzy", "Wild Frenzy", Some(base_description), 0).unwrap();
        let stronger =
            effects.ensure_family("Wild Frenzy", "Wild Frenzy +122", Some(&stronger_description), 0).unwrap();
        assert_eq!(base, stronger);
        let templates: (String, String, i64) = transaction
            .query_row(
                "SELECT verbose_name_template, description_template, default_value FROM effects WHERE id = ?1",
                [base],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(templates.0, "Wild Frenzy +{1}");
        assert!(templates.1.contains("DC {1} Will save"));
        assert_eq!(templates.2, 25);
    }

    #[test]
    fn group_defaults_are_rejected() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        ddo_model::seeds::insert_all(&db).unwrap();
        let transaction = db.transaction().unwrap();
        let mut effects = EffectCache::new(&transaction, crate::groupings::fixture_groupings());
        let group_id = effects.ensure_group("Charisma Skills").unwrap();
        for defaults in [(Some(3), None), (None, Some(3)), (None, None)] {
            let error = effects.set_defaults(group_id, defaults).unwrap_err().to_string();
            assert!(error.contains("group Charisma Skills cannot carry definition defaults"), "{error}");
        }
        let defaults: (Option<i64>, Option<i64>) = transaction
            .query_row("SELECT default_value, default_value2 FROM effects WHERE id=?1", [group_id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(defaults, (None, None));
    }

    #[test]
    fn every_ability_skill_group_has_its_own_description() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        ddo_model::seeds::insert_all(&db).unwrap();
        let transaction = db.transaction().unwrap();
        let mut effects = EffectCache::new(&transaction, crate::groupings::fixture_groupings());
        effects.ensure_family("Dexterity Skills", "%b1 Dexterity Skills +{1}", None, 1).unwrap();
        for ability in ["Strength", "Dexterity", "Constitution", "Intelligence", "Wisdom", "Charisma"] {
            let group_name = format!("{ability} Skills");
            let group_id = effects.ensure_group(&group_name).unwrap();
            let description: String = transaction
                .query_row("SELECT description_template FROM effects WHERE id = ?1", [group_id], |row| row.get(0))
                .unwrap();
            assert_eq!(description, format!("{{1}} %b1 bonus to all {ability} based skills."));
        }
    }

    #[test]
    fn stat_templates_merge_only_when_type_is_the_difference() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        ddo_model::seeds::insert_all(&db).unwrap();
        let transaction = db.transaction().unwrap();
        let mut effects = EffectCache::new(&transaction, crate::groupings::fixture_groupings());
        let stat_id = effects
            .ensure_family(
                "Illusion Save",
                "Illusion Save {1}",
                Some("{1} Resistance bonus to your saves versus illusions"),
                1,
            )
            .unwrap();
        assert_eq!(
            effects
                .ensure_family(
                    "Illusion Save",
                    "%b1 Illusion Save {1}",
                    Some("{1} %b1 bonus to your saves versus illusions"),
                    1,
                )
                .unwrap(),
            stat_id
        );
        assert!(effects.ensure_family("Illusion Save", "Illusion Save {1}", Some("A different rule"), 1).is_err());
    }

    #[test]
    fn a_text_line_cannot_reuse_a_stat_family_name() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        ddo_model::seeds::insert_all(&db).unwrap();
        let transaction = db.transaction().unwrap();
        let mut effects = EffectCache::new(&transaction, crate::groupings::fixture_groupings());
        let stat_id = effects.ensure_family("Riposte", "Riposte {1}", None, 1).unwrap();
        let error = effects.ensure_text("Riposte", "Riposte").unwrap_err();
        assert!(error.to_string().contains("[names]"));
        assert!(stat_id > 0);
        assert!(effects.ensure_text("Linguistics", "Linguistics 10%").is_err());
    }

    #[test]
    fn a_link_may_omit_only_its_second_amount() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        ddo_model::seeds::insert_all(&db).unwrap();
        db.execute(
            "INSERT INTO items (name, slot_id, item_category, wiki_url, slug) VALUES ('First Amount Only',
             (SELECT id FROM equipment_slots WHERE name = 'Feet'), 'Clothing',
             'https://ddowiki.com/page/First_Amount_Only', 'first-amount-only')",
            [],
        )
        .unwrap();
        let item_id = db.last_insert_rowid();
        let transaction = db.transaction().unwrap();
        let mut effects = EffectCache::new(&transaction, crate::groupings::fixture_groupings());
        let effect_id = effects.ensure_family("Dual Speed", "Dual Speed {1}: {2}", None, 2).unwrap();
        for (sort_order, (stat_name, amount_from)) in
            [("Movement Speed", 1), ("Melee Attack Speed", 2)].into_iter().enumerate()
        {
            effects
                .ensure_stat(
                    effect_id,
                    Stat::by_name(stat_name).unwrap(),
                    Some(BonusType::Enhancement),
                    amount_from,
                    None,
                    sort_order,
                )
                .unwrap();
        }
        effects.insert_link((EffectOwner::Item, item_id), effect_id, None, None, (Some(19), None), 0).unwrap();
        let amounts: Vec<Option<i64>> = transaction
            .prepare(
                "SELECT CASE es.amount_from WHEN 1 THEN ie.value ELSE ie.value2 END
                 FROM item_effects ie JOIN effect_bonuses es ON es.effect_id = ie.effect_id
                 WHERE ie.item_id = ?1 ORDER BY es.sort_order",
            )
            .unwrap()
            .query_map([item_id], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(amounts, [Some(19), None]);
        effects.insert_link((EffectOwner::Item, item_id), effect_id, None, None, (None, None), 1).unwrap();
    }

    #[test]
    fn identical_text_families_merge_their_owner_links() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        ddo_model::seeds::insert_all(&db).unwrap();
        db.execute(
            "INSERT INTO items (name, slot_id, item_category, wiki_url, slug) VALUES ('Shared Text Probe',
             (SELECT id FROM equipment_slots WHERE name = 'Feet'), 'Clothing',
             'https://ddowiki.com/page/Shared_Text_Probe', 'shared-text-probe')",
            [],
        )
        .unwrap();
        let item_id = db.last_insert_rowid();
        let transaction = db.transaction().unwrap();
        let mut effects = EffectCache::new(&transaction, crate::groupings::fixture_groupings());
        let destination =
            effects.ensure_family("Earthen Guard", "Earthen Guard", Some("A Stone Skin may trigger."), 0).unwrap();
        let source =
            effects.ensure_family("Echoes of 2006", "Earthen Guard", Some("A Stone Skin may trigger."), 0).unwrap();
        effects.insert_link((EffectOwner::Item, item_id), source, None, None, (None, None), 0).unwrap();
        effects.merge_identical_text_family(source, "Earthen Guard").unwrap();
        let linked_effect: i64 = transaction
            .query_row("SELECT effect_id FROM item_effects WHERE item_id = ?1", [item_id], |row| row.get(0))
            .unwrap();
        assert_eq!(linked_effect, destination);
        assert!(effects.family(source).is_none());
    }

    #[test]
    fn direct_stat_links_have_exactly_one_value_slot() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        ddo_model::seeds::insert_all(&db).unwrap();
        db.execute(
            "INSERT INTO items (name, slot_id, item_category, wiki_url, slug) VALUES ('Stat Slot Check',
             (SELECT id FROM equipment_slots WHERE name = 'Feet'), 'Clothing',
             'https://ddowiki.com/page/Stat_Slot_Check', 'stat-slot-check')",
            [],
        )
        .unwrap();
        let item_id = db.last_insert_rowid();
        let transaction = db.transaction().unwrap();
        let mut effects = EffectCache::new(&transaction, crate::groupings::fixture_groupings());
        let stat_id = Stat::by_name("Movement Speed").unwrap().id;
        assert!(effects.ensure_family("Movement Speed", "Movement Speed {1} {2}", None, 2).is_err());
        effects.ensure_family("Movement Speed", "Movement Speed {1}", None, 1).unwrap();
        let error = effects
            .insert_link(
                (EffectOwner::Item, item_id),
                stat_id,
                None,
                Some(BonusType::Enhancement),
                (Some(10), Some(20)),
                0,
            )
            .unwrap_err();
        assert!(error.to_string().contains("cannot carry a second value"));
    }
}
