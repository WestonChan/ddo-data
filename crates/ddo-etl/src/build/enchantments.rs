use crate::map::buff::split_display_title;
use anyhow::{ensure, Result};
use ddo_model::enums::BonusType;
use ddo_model::stats::Stat;
use rusqlite::{params, Transaction};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone)]
pub(super) struct CachedEnchantment {
    pub(super) id: i64,
    pub(super) name: String,
    pub(super) text_template: String,
    pub(super) description_template: Option<String>,
    pub(super) amount_count: i64,
    pub(super) uses_link_type: bool,
    default_value: Option<i64>,
    default_value2: Option<i64>,
    has_stats: bool,
}

type StatIdentity = (i64, i64, Option<i64>);
type StatAmount = (i64, Option<i64>);

pub(super) struct EnchantmentCache<'a> {
    transaction: &'a Transaction<'a>,
    families_by_name: HashMap<String, i64>,
    families_by_id: HashMap<i64, CachedEnchantment>,
    stats: HashMap<StatIdentity, StatAmount>,
}

impl<'a> EnchantmentCache<'a> {
    pub(super) fn new(transaction: &'a Transaction<'a>) -> Self {
        Self { transaction, families_by_name: HashMap::new(), families_by_id: HashMap::new(), stats: HashMap::new() }
    }
    pub(super) fn family_named(&self, name: &str) -> Option<&CachedEnchantment> {
        self.families_by_name.get(name).and_then(|id| self.families_by_id.get(id))
    }

    pub(super) fn family(&self, id: i64) -> Option<&CachedEnchantment> {
        self.families_by_id.get(&id)
    }

    pub(super) fn families(&self) -> impl Iterator<Item = &CachedEnchantment> {
        self.families_by_id.values()
    }

    pub(super) fn has_stats(&self, family_id: i64) -> bool {
        self.family(family_id).is_some_and(|family| family.has_stats)
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
            ensure!(
                existing.text_template == text_template
                    && existing.description_template.as_deref() == description_template
                    && existing.amount_count == count,
                "enchantment {name:?} has conflicting templates or amount counts"
            );
            return Ok(existing.id);
        }
        ensure!(!name.contains(" — "), "enchantment family {name:?} contains an em dash; add a [names] entry");
        validate_templates(name, text_template, description_template, count)?;
        self.transaction.execute(
            "INSERT INTO enchantments (name, text_template, description_template, amount_count) VALUES (?1, ?2, ?3, ?4)",
            params![name, text_template, description_template, count],
        )?;
        let id = self.transaction.last_insert_rowid();
        self.families_by_name.insert(name.to_string(), id);
        self.families_by_id.insert(
            id,
            CachedEnchantment {
                id,
                name: name.to_string(),
                text_template: text_template.to_string(),
                description_template: description_template.map(str::to_string),
                amount_count: count,
                uses_link_type: text_template.contains("%b1")
                    || description_template.is_some_and(|template| template.contains("%b1")),
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
                "enchantment {family_id} stat {:?} has conflicting amount sources",
                stat.name
            );
            return Ok(());
        }
        let family = self.family(family_id).expect("family is cached before its stat rows");
        ensure!(
            amount_from <= family.amount_count,
            "enchantment {family_id} stat {:?} reads beyond its slots",
            stat.name
        );
        ensure!(
            family.uses_link_type == bonus_type.is_none(),
            "enchantment {family_id} stat {:?} has its bonus type at the wrong level",
            stat.name
        );
        self.transaction.execute(
            "INSERT INTO enchantment_stats (enchantment_id, stat_id, bonus_type_id, amount_from, constant, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![family_id, stat.id, bonus_type.map(BonusType::id), amount_from, constant, sort_order as i64],
        )?;
        self.stats.insert(key, (amount_from, constant));
        self.families_by_id.get_mut(&family_id).expect("family is cached before its stat rows").has_stats = true;
        Ok(())
    }

    pub(super) fn ensure_wiki_text(&mut self, source_name: &str, description: Option<&str>) -> Result<i64> {
        let family_name = source_name.split_once(" — ").map_or(source_name, |(title, _)| title);
        if let Some(existing) = self.family_named(family_name) {
            ensure!(
                !self.has_stats(existing.id),
                "enchantment family {family_name:?} has both text-only and structured uses"
            );
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
                "enchantment family {family_name:?} has conflicting text {rendered_text:?}; add a [names] entry"
            );
        }
        self.ensure_family(family_name, &text_template, description_template.as_deref(), 0)
    }

    pub(super) fn set_defaults(&mut self, family_id: i64, defaults: (Option<i64>, Option<i64>)) -> Result<()> {
        let family = self.families_by_id.get_mut(&family_id).expect("family is cached before its defaults");
        let (first, second) = defaults;
        ensure!(
            first.is_none_or(|value| family.default_value.is_none_or(|current| current == value))
                && second.is_none_or(|value| family.default_value2.is_none_or(|current| current == value)),
            "enchantment {family_id} has conflicting definition defaults"
        );
        if (first.is_some() && family.default_value.is_none()) || (second.is_some() && family.default_value2.is_none())
        {
            self.transaction.execute(
                "UPDATE enchantments SET default_value = COALESCE(default_value, ?2),
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
            "UPDATE enchantments SET text_template = ?2, description_template = NULL, amount_count = 0 WHERE id = ?1",
            params![family_id, text_template],
        )?;
        family.text_template = text_template.to_string();
        family.description_template = None;
        Ok(())
    }

    pub(super) fn delete_unowned(&mut self, family_id: i64) -> Result<()> {
        let deleted = self.transaction.execute(
            "DELETE FROM enchantments WHERE id = ?1
             AND NOT EXISTS (SELECT 1 FROM item_enchantments WHERE enchantment_id = ?1)
             AND NOT EXISTS (SELECT 1 FROM augment_enchantments WHERE enchantment_id = ?1)
             AND NOT EXISTS (SELECT 1 FROM set_bonus_tier_enchantments WHERE enchantment_id = ?1)
             AND NOT EXISTS (SELECT 1 FROM feat_enchantments WHERE enchantment_id = ?1)
             AND NOT EXISTS (SELECT 1 FROM item_augment_slot_option_enchantments WHERE enchantment_id = ?1)",
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
        &self,
        owner: EnchantmentOwner,
        owner_id: i64,
        family_id: i64,
        bonus_type: Option<BonusType>,
        amounts: (Option<i64>, Option<i64>),
        sort_order: usize,
    ) -> Result<()> {
        let family = self.family(family_id).expect("every writer family is cached");
        validate_link(&family.name, family.amount_count, family.uses_link_type, bonus_type, amounts)?;
        write_link(self.transaction, owner, owner_id, family_id, bonus_type, amounts, sort_order)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EnchantmentOwner {
    Item,
    Augment,
    SetBonusTier,
    Feat,
    ItemAugmentSlotOption,
}

impl EnchantmentOwner {
    pub(super) const fn table(self) -> (&'static str, &'static str) {
        match self {
            Self::Item => ("item_enchantments", "item_id"),
            Self::Augment => ("augment_enchantments", "augment_id"),
            Self::SetBonusTier => ("set_bonus_tier_enchantments", "tier_id"),
            Self::Feat => ("feat_enchantments", "feat_id"),
            Self::ItemAugmentSlotOption => ("item_augment_slot_option_enchantments", "option_id"),
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
    ensure!((0..=2).contains(&count), "enchantment {name:?}: amount_count {count} is outside 0..=2");
    let template_text = format!("{text_template}{}", description_template.unwrap_or(""));
    for slot in 1..=2 {
        let has_slot = template_text.contains(&format!("{{{slot}}}"));
        ensure!(
            has_slot == (slot <= count),
            "enchantment {name:?}: template placeholders do not match amount_count {count}"
        );
    }
    ensure!(
        !template_text.contains("%v1") && !template_text.contains("%v2"),
        "enchantment {name:?}: unconverted amount placeholder"
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
    ensure!(present_count <= count, "enchantment {name:?} link has {present_count} values, at most {count} allowed");
    ensure!(uses_link_type == bonus_type.is_some(), "enchantment {name:?} link has its bonus type at the wrong level");
    Ok(())
}

fn write_link(
    transaction: &Transaction,
    owner: EnchantmentOwner,
    owner_id: i64,
    enchantment_id: i64,
    bonus_type: Option<BonusType>,
    amounts: (Option<i64>, Option<i64>),
    sort_order: usize,
) -> Result<()> {
    let (value, value2) = amounts;
    let (table_name, owner_column) = owner.table();
    transaction.prepare_cached(
        &format!(
            "INSERT INTO {table_name} ({owner_column}, enchantment_id, bonus_type_id, value, value2, sort_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
        ),
    )?.execute(params![owner_id, enchantment_id, bonus_type.map(BonusType::id), value, value2, sort_order as i64])?;
    Ok(())
}

pub(super) fn split_template(display_text: &str) -> (String, Option<String>) {
    let converted = display_text.replace("%v1", "{1}").replace("%v2", "{2}");
    match split_display_title(&converted) {
        Some((text, description)) => (text.trim().to_string(), Some(description.trim().to_string())),
        None => (converted.trim().to_string(), None),
    }
}

pub(super) fn insert_ladders(
    enchantments: &EnchantmentCache<'_>,
    ladders: &BTreeMap<String, Vec<String>>,
) -> Result<()> {
    let transaction = enchantments.transaction;
    for (ladder_name, steps) in ladders {
        ensure!(steps.len() >= 2, "ladder {ladder_name:?} needs at least two steps");
        let present_steps: Vec<_> = steps
            .iter()
            .filter_map(|step_name| enchantments.family_named(step_name).map(|family| (step_name, family.id)))
            .collect();
        if present_steps.len() < 2 {
            continue;
        }
        transaction.execute("INSERT INTO enchantment_ladders (name) VALUES (?1)", [ladder_name])?;
        let ladder_id = transaction.last_insert_rowid();
        for (index, (step_name, family_id)) in present_steps.iter().enumerate() {
            let changed_rows = transaction.execute(
                "UPDATE enchantments SET ladder_id = ?1, ladder_rank = ?2 WHERE id = ?3 AND ladder_id IS NULL",
                params![ladder_id, index as i64 + 1, family_id],
            )?;
            ensure!(
                changed_rows == 1,
                "ladder {ladder_name:?} step {step_name:?} has no unique owned enchantment family"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{EnchantmentCache, EnchantmentOwner};
    use ddo_model::enums::BonusType;
    use ddo_model::stats::Stat;
    use rusqlite::Connection;

    #[test]
    fn a_text_line_cannot_reuse_a_stat_family_name() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        let transaction = db.transaction().unwrap();
        let mut enchantments = EnchantmentCache::new(&transaction);
        let stat_id = enchantments.ensure_family("Riposte", "Riposte {1}", None, 1).unwrap();
        let error = enchantments.ensure_text("Riposte", "Riposte").unwrap_err();
        assert!(error.to_string().contains("[names]"));
        assert!(stat_id > 0);
        let linguistics_id = enchantments.ensure_text("Linguistics", "Linguistics 10%").unwrap();
        let linguistics_name: String = transaction
            .query_row("SELECT name FROM enchantments WHERE id = ?1", [linguistics_id], |row| row.get(0))
            .unwrap();
        assert_eq!(linguistics_name, "Linguistics");
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
        let mut enchantments = EnchantmentCache::new(&transaction);
        let enchantment_id = enchantments.ensure_family("Dual Speed", "Dual Speed {1}: {2}", None, 2).unwrap();
        for (sort_order, (stat_name, amount_from)) in
            [("Movement Speed", 1), ("Attack Speed", 2)].into_iter().enumerate()
        {
            enchantments
                .ensure_stat(
                    enchantment_id,
                    Stat::by_name(stat_name).unwrap(),
                    Some(BonusType::Enhancement),
                    amount_from,
                    None,
                    sort_order,
                )
                .unwrap();
        }
        enchantments.insert_link(EnchantmentOwner::Item, item_id, enchantment_id, None, (Some(19), None), 0).unwrap();
        let amounts: Vec<Option<i64>> = transaction
            .prepare(
                "SELECT CASE es.amount_from WHEN 1 THEN ie.value ELSE ie.value2 END
                 FROM item_enchantments ie JOIN enchantment_stats es ON es.enchantment_id = ie.enchantment_id
                 WHERE ie.item_id = ?1 ORDER BY es.sort_order",
            )
            .unwrap()
            .query_map([item_id], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(amounts, [Some(19), None]);
        enchantments.insert_link(EnchantmentOwner::Item, item_id, enchantment_id, None, (None, None), 1).unwrap();
    }
}
