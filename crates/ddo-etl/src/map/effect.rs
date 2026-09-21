//! `<Effect>` → derived `bonuses` rows, and his `<Bonus>` → our [`BonusType`].

use crate::xml::effect::Effect;
use anyhow::{bail, Result};
use ddo_model::enums::BonusType;
use ddo_model::stats::Stat;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
struct EffectMapData {
    fixed: BTreeMap<String, String>,
    by_item: BTreeMap<String, String>,
    by_item_default: BTreeMap<String, String>,
    item_aliases: BTreeMap<String, String>,
    bonus_type_aliases: BTreeMap<String, String>,
}

/// A bonus row an effect implies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Derived {
    pub stat: &'static Stat,
    pub bonus_type: Option<BonusType>,
    pub value: i64,
}

pub struct EffectMap {
    data: EffectMapData,
    unmapped: RefCell<BTreeMap<String, usize>>,
}

impl EffectMap {
    pub fn load() -> Result<Self> {
        let data: EffectMapData = toml::from_str(include_str!("../../data/effect_map.toml"))?;
        for (kind, stat) in data.fixed.iter().chain(data.by_item_default.iter()) {
            if ddo_model::stat_by_name(stat).is_none() {
                bail!("effect_map.toml: {kind} names unknown stat {stat:?}");
            }
        }
        Ok(Self { data, unmapped: RefCell::new(BTreeMap::new()) })
    }

    /// His `<Bonus>` text as one of our bonus types. Unknown spellings are an error so they get
    /// added to the enum or the alias table; empty and "Not Set" are `None`.
    pub fn bonus_type(&self, raw: &str) -> Result<Option<BonusType>> {
        let raw = raw.trim();
        let canonical = self.data.bonus_type_aliases.get(raw).map(String::as_str).unwrap_or(raw);
        if canonical.is_empty() {
            return Ok(None);
        }
        match BonusType::parse(canonical) {
            Some(b) => Ok(Some(b)),
            None => bail!(
                "unknown <Bonus> {raw:?}; add it to ddo-model's BonusType or effect_map.toml [bonus_type_aliases]"
            ),
        }
    }

    /// The bonus rows this effect implies: none unless it is a plain integer on a stat we know.
    pub fn derive(&self, effect: &Effect) -> Result<Vec<Derived>> {
        let Some(value) = effect.simple_integer() else {
            return Ok(Vec::new());
        };
        let kind = effect.types[0].as_str();
        let bonus_type = self.bonus_type(effect.bonus.as_deref().unwrap_or(""))?;

        if let Some(stat_name) = self.data.fixed.get(kind) {
            let stat = ddo_model::stat_by_name(stat_name).expect("validated at load");
            return Ok(vec![Derived { stat, bonus_type, value }]);
        }

        let template = self.data.by_item.get(kind);
        let default = self.data.by_item_default.get(kind);
        if template.is_none() && default.is_none() {
            *self.unmapped.borrow_mut().entry(kind.to_string()).or_default() += 1;
            return Ok(Vec::new());
        }

        let targets: Vec<&str> =
            if effect.items.is_empty() { vec![""] } else { effect.items.iter().map(String::as_str).collect() };
        let mut out = Vec::new();
        for target in targets {
            let stat = if target.is_empty() || target == "All" {
                default.and_then(|d| ddo_model::stat_by_name(d))
            } else {
                template.and_then(|t| {
                    let word = self.data.item_aliases.get(target).map(String::as_str).unwrap_or(target);
                    ddo_model::stat_by_name(&t.replace("{item}", word))
                })
            };
            if let Some(stat) = stat {
                out.push(Derived { stat, bonus_type, value });
            }
        }
        Ok(out)
    }

    /// Effect types seen by `derive` that have no mapping, with counts.
    pub fn unmapped_types(&self) -> BTreeMap<String, usize> {
        self.unmapped.borrow().clone()
    }
}
