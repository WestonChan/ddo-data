use crate::xml::effect::Effect;
use anyhow::{bail, Result};
use ddo_model::enums::BonusType;
use ddo_model::stats::Stat;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Deserialize)]
struct EffectVocabulary {
    fixed: BTreeMap<String, String>,
    by_item: BTreeMap<String, String>,
    by_item_default: BTreeMap<String, String>,
    item_aliases: BTreeMap<String, String>,
    bonus_type_aliases: BTreeMap<String, String>,
    engine_only: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerivedBonus {
    pub stat: &'static Stat,
    pub bonus_type: Option<BonusType>,
    pub value: i64,
}

pub struct EffectMap {
    vocabulary: EffectVocabulary,
    unmapped_type_counts: RefCell<BTreeMap<String, usize>>,
}

impl EffectMap {
    pub fn load() -> Result<Self> {
        Self::from_toml(include_str!("../../data/effect_map.toml"))
    }

    pub fn from_toml(toml_text: &str) -> Result<Self> {
        let vocabulary: EffectVocabulary = toml::from_str(toml_text)?;
        for (effect_type, stat_name) in vocabulary.fixed.iter().chain(vocabulary.by_item_default.iter()) {
            if Stat::by_name(stat_name).is_none() {
                bail!("effect_map.toml: {effect_type} names unknown stat {stat_name:?}");
            }
        }
        let mapped_types =
            vocabulary.fixed.keys().chain(vocabulary.by_item.keys()).chain(vocabulary.by_item_default.keys());
        let types_both_mapped_and_engine_only: BTreeSet<&String> =
            mapped_types.filter(|effect_type| vocabulary.engine_only.contains_key(*effect_type)).collect();
        if !types_both_mapped_and_engine_only.is_empty() {
            bail!(
                "effect_map.toml: {types_both_mapped_and_engine_only:?} are both mapped to a stat and listed in [engine_only]"
            );
        }
        Ok(Self { vocabulary, unmapped_type_counts: RefCell::new(BTreeMap::new()) })
    }

    pub fn parse_bonus_type(&self, upstream_name: &str) -> Result<Option<BonusType>> {
        let upstream_name = upstream_name.trim();
        let canonical_name =
            self.vocabulary.bonus_type_aliases.get(upstream_name).map(String::as_str).unwrap_or(upstream_name);
        if canonical_name.is_empty() {
            return Ok(None);
        }
        match BonusType::parse(canonical_name) {
            Some(bonus_type) => Ok(Some(bonus_type)),
            None => bail!(
                "unknown <Bonus> {upstream_name:?}; add it to ddo-model's BonusType or effect_map.toml [bonus_type_aliases]"
            ),
        }
    }

    pub fn derive_bonuses(&self, effect: &Effect) -> Result<Vec<DerivedBonus>> {
        let Some(value) = effect.simple_integer_amount() else {
            return Ok(Vec::new());
        };
        let effect_type = effect.types[0].as_str();
        let bonus_type = self.parse_bonus_type(effect.bonus.as_deref().unwrap_or(""))?;

        if let Some(stat_name) = self.vocabulary.fixed.get(effect_type) {
            let stat = Stat::by_name(stat_name).expect("validated at load");
            return Ok(vec![DerivedBonus { stat, bonus_type, value }]);
        }

        let stat_template = self.vocabulary.by_item.get(effect_type);
        let default_stat_name = self.vocabulary.by_item_default.get(effect_type);
        if stat_template.is_none() && default_stat_name.is_none() {
            if self.vocabulary.engine_only.contains_key(effect_type) {
                return Ok(Vec::new());
            }
            *self.unmapped_type_counts.borrow_mut().entry(effect_type.to_string()).or_default() += 1;
            return Ok(Vec::new());
        }

        let targets: Vec<&str> =
            if effect.targets.is_empty() { vec![""] } else { effect.targets.iter().map(String::as_str).collect() };
        let mut bonuses = Vec::new();
        for target in targets {
            let stat = if target.is_empty() || target == "All" {
                default_stat_name.and_then(|name| Stat::by_name(name))
            } else {
                stat_template.and_then(|template| {
                    let target_word = self.vocabulary.item_aliases.get(target).map(String::as_str).unwrap_or(target);
                    Stat::by_name(&template.replace("{item}", target_word))
                })
            };
            if let Some(stat) = stat {
                bonuses.push(DerivedBonus { stat, bonus_type, value });
            }
        }
        Ok(bonuses)
    }

    pub fn unmapped_type_counts(&self) -> BTreeMap<String, usize> {
        self.unmapped_type_counts.borrow().clone()
    }
}
