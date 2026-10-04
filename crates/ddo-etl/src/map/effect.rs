use super::effect_map::{EffectMap, EffectTargetQualifiers, EFFECT_MAP};
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
    vocabulary: &'static EffectMap,
    unmapped_type_counts: RefCell<BTreeMap<String, usize>>,
    qualifiers: EffectTargetQualifiers,
}

impl Default for EffectResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl EffectResolver {
    pub fn new() -> Self {
        Self {
            vocabulary: &EFFECT_MAP,
            unmapped_type_counts: RefCell::new(BTreeMap::new()),
            qualifiers: EffectTargetQualifiers::default(),
        }
    }

    pub fn with_qualifiers(mut self, qualifiers: EffectTargetQualifiers) -> Self {
        self.qualifiers = qualifiers;
        self
    }

    pub fn qualified_targets(&self, effect: &Effect) -> Option<String> {
        self.vocabulary.qualified_targets(effect, &self.qualifiers)
    }

    pub fn qualified_targeted_name(&self, effect: &Effect) -> Option<String> {
        self.vocabulary.qualified_targeted_name(effect)
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

        if effect_type == "AbilityBonus" && effect.targets.iter().any(|target| target == "All") {
            return Ok(["Strength", "Dexterity", "Constitution", "Intelligence", "Wisdom", "Charisma"]
                .into_iter()
                .map(|name| DerivedBonus { stat: Stat::by_name(name).expect("seeded ability stat"), bonus_type, value })
                .collect());
        }

        if !self.vocabulary.effect.fixed.contains_key(effect_type)
            && !self.vocabulary.effect.targeted.contains_key(effect_type)
            && !self.vocabulary.effect.by_item.contains_key(effect_type)
            && !self.vocabulary.effect.by_item_default.contains_key(effect_type)
        {
            if self.vocabulary.effect.engine_only.contains_key(effect_type) {
                return Ok(Vec::new());
            }
            *self.unmapped_type_counts.borrow_mut().entry(effect_type.to_string()).or_default() += 1;
            return Ok(Vec::new());
        }

        Ok(self
            .vocabulary
            .stats_for_effect(effect, &self.qualifiers)
            .unwrap_or_default()
            .into_iter()
            .map(|stat| DerivedBonus { stat, bonus_type, value })
            .collect())
    }

    pub fn unmapped_type_counts(&self) -> BTreeMap<String, usize> {
        self.unmapped_type_counts.borrow().clone()
    }
}
