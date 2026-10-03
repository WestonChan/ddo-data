use super::enchantment::{EnchantmentMap, ENCHANTMENT_MAP};
use crate::xml::effect::Effect;
use anyhow::Result;
use ddo_model::enums::BonusType;
use ddo_model::stats::Stat;
use std::cell::RefCell;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerivedBonus {
    pub stat: &'static Stat,
    pub bonus_type: Option<BonusType>,
    pub value: i64,
}

pub struct EffectResolver {
    vocabulary: &'static EnchantmentMap,
    unmapped_type_counts: RefCell<BTreeMap<String, usize>>,
}

impl Default for EffectResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl EffectResolver {
    pub fn new() -> Self {
        Self { vocabulary: &ENCHANTMENT_MAP, unmapped_type_counts: RefCell::new(BTreeMap::new()) }
    }

    pub fn parse_bonus_type(&self, upstream_name: &str) -> Result<Option<BonusType>> {
        self.vocabulary.effect_bonus_type(upstream_name)
    }

    pub fn derive_bonuses(&self, effect: &Effect) -> Result<Vec<DerivedBonus>> {
        let Some(value) = effect.simple_integer_amount() else {
            return Ok(Vec::new());
        };
        let effect_type = effect.types[0].as_str();
        let bonus_type = self.parse_bonus_type(effect.bonus.as_deref().unwrap_or(""))?;

        if let Some(stat_name) = self.vocabulary.effect.fixed.get(effect_type) {
            let stat = Stat::by_name(stat_name).expect("validated at load");
            return Ok(vec![DerivedBonus { stat, bonus_type, value }]);
        }

        let stat_template = self.vocabulary.effect.by_item.get(effect_type);
        let default_stat_name = self.vocabulary.effect.by_item_default.get(effect_type);
        if stat_template.is_none() && default_stat_name.is_none() {
            if self.vocabulary.effect.engine_only.contains_key(effect_type) {
                return Ok(Vec::new());
            }
            *self.unmapped_type_counts.borrow_mut().entry(effect_type.to_string()).or_default() += 1;
            return Ok(Vec::new());
        }

        let targets: Vec<&str> =
            if effect.targets.is_empty() { vec![""] } else { effect.targets.iter().map(String::as_str).collect() };
        let mut bonuses = Vec::new();
        for target in targets {
            let stat = self.vocabulary.effect_stat(effect_type, target);
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
