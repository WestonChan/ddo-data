use crate::map::buff::split_display_title;
use crate::xml::item_buffs::ItemBuffDefinition;
use anyhow::{ensure, Result};
use ddo_model::enums::BonusType;
use ddo_model::stats::{Stat, STATS};
use rusqlite::{params, Transaction};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone)]
pub(super) struct CachedEffect {
    pub(super) id: i64,
    pub(super) name: String,
    pub(super) text_template: String,
    pub(super) description_template: Option<String>,
    pub(super) amount_count: i64,
    pub(super) uses_link_type: bool,
    pub(super) is_stat: bool,
    default_value: Option<i64>,
    default_value2: Option<i64>,
    has_stats: bool,
}

type StatIdentity = (i64, i64, Option<i64>);
type StatAmount = (i64, Option<i64>);

pub(super) struct EffectCache<'a> {
    transaction: &'a Transaction<'a>,
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
    pub(super) fn new(transaction: &'a Transaction<'a>) -> Self {
        let mut families_by_name = HashMap::new();
        let mut families_by_id = HashMap::new();
        for stat in STATS {
            families_by_name.insert(stat.name.to_string(), stat.id);
            families_by_id.insert(
                stat.id,
                CachedEffect {
                    id: stat.id,
                    name: stat.name.to_string(),
                    text_template: String::new(),
                    description_template: None,
                    amount_count: 1,
                    uses_link_type: false,
                    is_stat: true,
                    default_value: None,
                    default_value2: None,
                    has_stats: true,
                },
            );
        }
        Self {
            transaction,
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
        text_template: &str,
        description_template: Option<&str>,
        count: i64,
    ) -> Result<i64> {
        if let Some(existing) = self.family_named(name) {
            if existing.is_stat && existing.text_template.is_empty() {
                let stat_id = existing.id;
                ensure!(count == 1, "stat effect {name:?} must read exactly one value");
                validate_templates(name, text_template, description_template, count)?;
                self.transaction.execute(
                    "UPDATE effects SET text_template = ?2, description_template = ?3 WHERE id = ?1",
                    params![stat_id, text_template, description_template],
                )?;
                let family = self.families_by_id.get_mut(&stat_id).expect("seed stat cached");
                family.text_template = text_template.to_string();
                family.description_template = description_template.map(str::to_string);
                family.amount_count = count;
                family.uses_link_type = text_template.contains("%b1")
                    || description_template.is_some_and(|template| template.contains("%b1"));
                return Ok(stat_id);
            }
            let incoming_uses_link_type =
                text_template.contains("%b1") || description_template.is_some_and(|template| template.contains("%b1"));
            if existing.is_stat && count == 1 && existing.uses_link_type != incoming_uses_link_type {
                let (dynamic_text, dynamic_description, fixed_text, fixed_description) = if existing.uses_link_type {
                    (
                        existing.text_template.as_str(),
                        existing.description_template.as_deref(),
                        text_template,
                        description_template,
                    )
                } else {
                    (
                        text_template,
                        description_template,
                        existing.text_template.as_str(),
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
                validate_templates(name, text_template, description_template, count)?;
                self.transaction.execute(
                    "UPDATE effects SET text_template = ?2, description_template = ?3 WHERE id = ?1",
                    params![stat_id, text_template, description_template],
                )?;
                let family = self.families_by_id.get_mut(&stat_id).expect("seed stat cached");
                family.text_template = text_template.to_string();
                family.description_template = description_template.map(str::to_string);
                family.uses_link_type = true;
                return Ok(stat_id);
            }
            ensure!(
                existing.text_template == text_template
                    && existing.description_template.as_deref() == description_template
                    && existing.amount_count == count,
                "effect {name:?} has conflicting templates or amount counts"
            );
            return Ok(existing.id);
        }
        ensure!(!name.contains(" — "), "effect family {name:?} contains an em dash; add a [names] entry");
        validate_templates(name, text_template, description_template, count)?;
        self.transaction.execute(
            "INSERT INTO effects (name, text_template, description_template) VALUES (?1, ?2, ?3)",
            params![name, text_template, description_template],
        )?;
        let id = self.transaction.last_insert_rowid();
        self.families_by_name.insert(name.to_string(), id);
        self.families_by_id.insert(
            id,
            CachedEffect {
                id,
                name: name.to_string(),
                text_template: text_template.to_string(),
                description_template: description_template.map(str::to_string),
                amount_count: count,
                uses_link_type: text_template.contains("%b1")
                    || description_template.is_some_and(|template| template.contains("%b1")),
                is_stat: false,
                default_value: None,
                default_value2: None,
                has_stats: false,
            },
        );
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
            !family.uses_link_type || bonus_type.is_none(),
            "effect {family_id} stat {:?} has its bonus type at the wrong level",
            stat.name
        );
        self.transaction.execute(
            "INSERT INTO effect_bonuses (effect_id, stat_id, bonus_type_id, amount_from, constant, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![family_id, stat.id, bonus_type.map(BonusType::id), amount_from, constant, sort_order as i64],
        )?;
        self.stats.insert(key, (amount_from, constant));
        let family = self.families_by_id.get_mut(&family_id).expect("family is cached before its stat rows");
        family.has_stats = true;
        family.uses_link_type |= bonus_type.is_none();
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
        let (text_template, description_template) = split_template(rendered_text);
        let family_name = source_name.split_once(" — ").map_or(source_name, |(title, _)| title);
        if let Some(existing) = self.family_named(family_name) {
            ensure!(
                !existing.has_stats
                    && existing.text_template == text_template
                    && existing.description_template == description_template,
                "effect family {family_name:?} has conflicting text {rendered_text:?}; add a [names] entry"
            );
        }
        let count = amount_count(&text_template, description_template.as_deref());
        self.ensure_family(family_name, &text_template, description_template.as_deref(), count)
    }

    pub(super) fn set_defaults(&mut self, family_id: i64, defaults: (Option<i64>, Option<i64>)) -> Result<()> {
        let family = self.families_by_id.get_mut(&family_id).expect("family is cached before its defaults");
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

    pub(super) fn replace_family_text(&mut self, family_id: i64, text_template: &str) -> Result<()> {
        let family = self.families_by_id.get_mut(&family_id).expect("corrected family is cached");
        ensure!(!family.has_stats, "structured family text cannot be replaced");
        validate_templates(&family.name, text_template, None, 0)?;
        self.transaction.execute(
            "UPDATE effects SET text_template = ?2, description_template = NULL WHERE id = ?1",
            params![family_id, text_template],
        )?;
        family.text_template = text_template.to_string();
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

    pub(super) fn insert_link(
        &mut self,
        owner: EffectOwner,
        owner_id: i64,
        family_id: i64,
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
        write_link(self.transaction, owner, owner_id, family_id, bonus_type, amounts, sort_order)?;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EffectOwner {
    Item,
    Augment,
    SetBonusTier,
    Feat,
    ItemAugmentSlotOption,
}

impl EffectOwner {
    pub(super) const fn table(self) -> (&'static str, &'static str) {
        match self {
            Self::Item => ("item_effects", "item_id"),
            Self::Augment => ("augment_effects", "augment_id"),
            Self::SetBonusTier => ("set_bonus_tier_effects", "tier_id"),
            Self::Feat => ("feat_effects", "feat_id"),
            Self::ItemAugmentSlotOption => ("item_augment_slot_option_effects", "option_id"),
        }
    }
}

pub(super) fn amount_count(text_template: &str, description_template: Option<&str>) -> i64 {
    let templates = [text_template, description_template.unwrap_or("")];
    if templates.iter().any(|template| template.contains("{2}")) {
        2
    } else if templates.iter().any(|template| template.contains("{1}")) {
        1
    } else {
        0
    }
}

pub(super) fn validate_templates(
    name: &str,
    text_template: &str,
    description_template: Option<&str>,
    count: i64,
) -> Result<()> {
    ensure!((0..=2).contains(&count), "effect {name:?}: amount_count {count} is outside 0..=2");
    let template_text = format!("{text_template}{}", description_template.unwrap_or(""));
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
    owner: EffectOwner,
    owner_id: i64,
    effect_id: i64,
    bonus_type: Option<BonusType>,
    amounts: (Option<i64>, Option<i64>),
    sort_order: usize,
) -> Result<()> {
    let (value, value2) = amounts;
    let (table_name, owner_column) = owner.table();
    transaction.prepare_cached(
        &format!(
            "INSERT INTO {table_name} ({owner_column}, effect_id, bonus_type_id, value, value2, sort_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
        ),
    )?.execute(params![owner_id, effect_id, bonus_type.map(BonusType::id), value, value2, sort_order as i64])?;
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
    fn stat_templates_merge_only_when_type_is_the_difference() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        ddo_model::seeds::insert_all(&db).unwrap();
        let transaction = db.transaction().unwrap();
        let mut effects = EffectCache::new(&transaction);
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
        let transaction = db.transaction().unwrap();
        let mut effects = EffectCache::new(&transaction);
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
            "INSERT INTO items (name, slot_id, item_category, wiki_url) VALUES ('First Amount Only',
             (SELECT id FROM equipment_slots WHERE name = 'Feet'), 'Clothing',
             'https://ddowiki.com/page/First_Amount_Only')",
            [],
        )
        .unwrap();
        let item_id = db.last_insert_rowid();
        let transaction = db.transaction().unwrap();
        let mut effects = EffectCache::new(&transaction);
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
        effects.insert_link(EffectOwner::Item, item_id, effect_id, None, (Some(19), None), 0).unwrap();
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
        effects.insert_link(EffectOwner::Item, item_id, effect_id, None, (None, None), 1).unwrap();
    }

    #[test]
    fn direct_stat_links_have_exactly_one_value_slot() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        ddo_model::seeds::insert_all(&db).unwrap();
        db.execute(
            "INSERT INTO items (name, slot_id, item_category, wiki_url) VALUES ('Stat Slot Check',
             (SELECT id FROM equipment_slots WHERE name = 'Feet'), 'Clothing',
             'https://ddowiki.com/page/Stat_Slot_Check')",
            [],
        )
        .unwrap();
        let item_id = db.last_insert_rowid();
        let transaction = db.transaction().unwrap();
        let mut effects = EffectCache::new(&transaction);
        let stat_id = Stat::by_name("Movement Speed").unwrap().id;
        assert!(effects.ensure_family("Movement Speed", "Movement Speed {1} {2}", None, 2).is_err());
        effects.ensure_family("Movement Speed", "Movement Speed {1}", None, 1).unwrap();
        let error = effects
            .insert_link(EffectOwner::Item, item_id, stat_id, Some(BonusType::Enhancement), (Some(10), Some(20)), 0)
            .unwrap_err();
        assert!(error.to_string().contains("cannot carry a second value"));
    }
}
