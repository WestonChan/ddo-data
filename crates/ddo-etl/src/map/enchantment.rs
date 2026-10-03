use crate::xml::effect::Effect;
use anyhow::{bail, Result};
use ddo_model::enums::BonusType;
use ddo_model::seeds::WeaponType;
use ddo_model::stats::{Stat, STATS};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnchantmentMap {
    pub family: FamilyVocabulary,
    pub effect: EffectVocabulary,
    pub item_aliases: BTreeMap<String, String>,
    pub bonus_type_aliases: BTreeMap<String, String>,
    pub weapon_aliases: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilyVocabulary {
    pub enhancement: Vec<String>,
    pub fixed: BTreeMap<String, String>,
    pub by_item: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectVocabulary {
    pub fixed: BTreeMap<String, String>,
    pub by_item: BTreeMap<String, String>,
    pub by_item_default: BTreeMap<String, String>,
    pub engine_only: BTreeMap<String, String>,
}

pub static ENCHANTMENT_MAP: LazyLock<EnchantmentMap> =
    LazyLock::new(|| EnchantmentMap::load().expect("data/enchantment_map.toml is valid"));

impl EnchantmentMap {
    pub fn load() -> Result<Self> {
        Self::from_toml(include_str!("../../data/enchantment_map.toml"))
    }

    pub fn from_toml(toml_text: &str) -> Result<Self> {
        let vocabulary: Self = toml::from_str(toml_text)?;
        for (section, stat_names) in [
            ("family.fixed", &vocabulary.family.fixed),
            ("effect.fixed", &vocabulary.effect.fixed),
            ("effect.by_item_default", &vocabulary.effect.by_item_default),
        ] {
            for (kind, stat_name) in stat_names {
                if Stat::by_name(stat_name).is_none() {
                    bail!("enchantment_map.toml [{section}] {kind} names unknown stat {stat_name:?}");
                }
            }
        }
        for (section, templates) in
            [("family.by_item", &vocabulary.family.by_item), ("effect.by_item", &vocabulary.effect.by_item)]
        {
            for (kind, template) in templates {
                let Some((prefix, suffix)) = template.split_once("{item}") else {
                    bail!("enchantment_map.toml [{section}] {kind} template {template:?} names no stat");
                };
                if suffix.contains("{item}")
                    || !STATS.iter().any(|stat| stat.name.starts_with(prefix) && stat.name.ends_with(suffix))
                {
                    bail!("enchantment_map.toml [{section}] {kind} template {template:?} names no stat");
                }
            }
        }
        let mapped_effect_types = vocabulary
            .effect
            .fixed
            .keys()
            .chain(vocabulary.effect.by_item.keys())
            .chain(vocabulary.effect.by_item_default.keys());
        let overlapping_types: BTreeSet<_> =
            mapped_effect_types.filter(|kind| vocabulary.effect.engine_only.contains_key(*kind)).collect();
        if !overlapping_types.is_empty() {
            bail!("enchantment_map.toml: {overlapping_types:?} are both mapped to a stat and listed in [effect.engine_only]");
        }
        for kind in vocabulary.effect.fixed.keys() {
            if vocabulary.effect.by_item.contains_key(kind) {
                bail!("enchantment_map.toml [effect.fixed] {kind} is also mapped in [effect.by_item]");
            }
        }
        for kind in &vocabulary.family.enhancement {
            if vocabulary.family.fixed.contains_key(kind) || vocabulary.family.by_item.contains_key(kind) {
                bail!("enchantment_map.toml [family.enhancement] {kind} is also mapped in another family section");
            }
        }
        for (alias, word) in &vocabulary.item_aliases {
            let names_a_stat = Stat::by_name(word).is_some()
                || vocabulary
                    .family
                    .by_item
                    .values()
                    .chain(vocabulary.effect.by_item.values())
                    .any(|template| Stat::by_name(&template.replace("{item}", word)).is_some());
            if !names_a_stat {
                bail!("enchantment_map.toml [item_aliases] {alias} names no stat through {word:?}");
            }
        }
        for (alias, bonus_type_name) in &vocabulary.bonus_type_aliases {
            if !bonus_type_name.is_empty() && BonusType::parse(bonus_type_name).is_none() {
                bail!("enchantment_map.toml [bonus_type_aliases] {alias} names unknown bonus type {bonus_type_name:?}");
            }
        }
        for (alias, weapon_name) in &vocabulary.weapon_aliases {
            if WeaponType::by_name(weapon_name).is_none() {
                bail!("enchantment_map.toml [weapon_aliases] {alias} names unknown weapon type {weapon_name:?}");
            }
        }
        Ok(vocabulary)
    }

    pub fn effect_stat(&self, effect_type: &str, target: &str) -> Option<&'static Stat> {
        if let Some(stat_name) = self.effect.fixed.get(effect_type) {
            return Stat::by_name(stat_name);
        }
        if target.is_empty() || target == "All" {
            return self.effect.by_item_default.get(effect_type).and_then(|name| Stat::by_name(name));
        }
        let template = self.effect.by_item.get(effect_type)?;
        let target_word = self.item_aliases.get(target).map(String::as_str).unwrap_or(target);
        Stat::by_name(&template.replace("{item}", target_word))
    }

    pub fn stats_for_plain_effect(&self, effect: &Effect) -> Option<Vec<&'static Stat>> {
        effect.plain_integer_amount()?;
        let effect_type = effect.types.first()?;
        if self.effect.engine_only.contains_key(effect_type) {
            return None;
        }
        if let Some(stat_name) = self.effect.fixed.get(effect_type) {
            if effect.targets.iter().any(|target| !target.is_empty() && target != "All") {
                return None;
            }
            return Some(vec![Stat::by_name(stat_name).expect("validated at load")]);
        }
        let targets: Vec<&str> =
            if effect.targets.is_empty() { vec![""] } else { effect.targets.iter().map(String::as_str).collect() };
        targets.into_iter().map(|target| self.effect_stat(effect_type, target)).collect()
    }

    pub fn family_bonus_type(&self, upstream_name: &str) -> Result<Option<BonusType>> {
        let upstream_name = upstream_name.trim();
        let canonical_name = self.bonus_type_aliases.get(upstream_name).map(String::as_str).unwrap_or(upstream_name);
        self.parsed_bonus_type(upstream_name, canonical_name)
    }

    pub fn effect_bonus_type(&self, upstream_name: &str) -> Result<Option<BonusType>> {
        let upstream_name = upstream_name.trim();
        if let Some(bonus_type) = BonusType::parse(upstream_name) {
            return Ok(Some(bonus_type));
        }
        let canonical_name = self.bonus_type_aliases.get(upstream_name).map(String::as_str).unwrap_or(upstream_name);
        self.parsed_bonus_type(upstream_name, canonical_name)
    }

    fn parsed_bonus_type(&self, upstream_name: &str, canonical_name: &str) -> Result<Option<BonusType>> {
        if canonical_name.is_empty() {
            return Ok(None);
        }
        BonusType::parse(canonical_name)
            .map(Some)
            .ok_or_else(|| anyhow::anyhow!("unknown bonus type {upstream_name:?}; add it to data/enchantment_map.toml [bonus_type_aliases] or to ddo-model's BonusType"))
    }
}
