use crate::xml::effect::Effect;
use crate::xml::spells::Spell;
use anyhow::{bail, Result};
use ddo_model::enums::{BonusType, DamageCategory};
use ddo_model::seeds::{WeaponType, DAMAGE_TYPES};
use ddo_model::stats::{Stat, STATS};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::LazyLock;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnchantmentMap {
    pub family: FamilyVocabulary,
    pub effect: EffectVocabulary,
    pub item_aliases: BTreeMap<String, String>,
    pub bonus_type_aliases: BTreeMap<String, String>,
    pub weapon_aliases: BTreeMap<String, String>,
    #[serde(default)]
    pub names: BTreeMap<String, String>,
    #[serde(default)]
    pub ladders: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilyVocabulary {
    pub enhancement: Vec<String>,
    pub fixed: BTreeMap<String, String>,
    pub by_item: BTreeMap<String, String>,
    #[serde(default)]
    pub text_only: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectVocabulary {
    pub fixed: BTreeMap<String, String>,
    pub by_item: BTreeMap<String, String>,
    pub by_item_default: BTreeMap<String, String>,
    pub companion_targets: CompanionTargets,
    pub energy_target_artifacts: EnergyTargetArtifacts,
    pub engine_only: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanionTargets {
    pub words: BTreeSet<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnergyTargetArtifacts {
    pub stats: BTreeSet<String>,
}

pub static ENCHANTMENT_MAP: LazyLock<EnchantmentMap> =
    LazyLock::new(|| EnchantmentMap::load().expect("data/enchantment_map.toml is valid"));

#[derive(Clone, Default)]
pub struct EffectTargetQualifiers {
    names: HashSet<String>,
}

impl EffectTargetQualifiers {
    pub fn from_vocabularies(class_names: impl IntoIterator<Item = String>, spells: &[Spell]) -> Self {
        let mut names: HashSet<String> = class_names.into_iter().collect();
        names.extend(["Arcane", "Divine"].into_iter().map(str::to_string));
        names.extend(spells.iter().flat_map(|spell| spell.schools.iter().chain(&spell.primers)).cloned());
        Self { names }
    }

    pub fn contains(&self, target: &str, vocabulary: &EnchantmentMap) -> bool {
        self.names.contains(target)
            || WeaponType::by_name(target).is_some()
            || vocabulary.weapon_aliases.get(target).is_some_and(|name| WeaponType::by_name(name).is_some())
            || DAMAGE_TYPES.iter().any(|damage_type| damage_type.name == target)
    }
}

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
        for stat_name in &vocabulary.effect.energy_target_artifacts.stats {
            if !vocabulary.effect.fixed.values().any(|mapped_stat| mapped_stat == stat_name) {
                bail!("enchantment_map.toml [effect.energy_target_artifacts] names unmapped fixed stat {stat_name:?}");
            }
        }
        for word in &vocabulary.effect.companion_targets.words {
            if word.trim().is_empty() || word == "All" {
                bail!("enchantment_map.toml [effect.companion_targets] has invalid word {word:?}");
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
            if vocabulary.family.fixed.contains_key(kind)
                || vocabulary.family.by_item.contains_key(kind)
                || vocabulary.family.text_only.contains_key(kind)
            {
                bail!("enchantment_map.toml [family.enhancement] {kind} is also mapped in another family section");
            }
        }
        for (kind, reason) in &vocabulary.family.text_only {
            if kind.trim().is_empty() || reason.trim().is_empty() {
                bail!("enchantment_map.toml [family.text_only] {kind:?} needs a buff type and reason");
            }
            if vocabulary.family.fixed.contains_key(kind) || vocabulary.family.by_item.contains_key(kind) {
                bail!("enchantment_map.toml [family.text_only] {kind} is also mapped to a stat");
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
        for (buff_type, family_name) in &vocabulary.names {
            if buff_type.trim().is_empty() || family_name.trim().is_empty() {
                bail!("enchantment_map.toml [names] {buff_type:?} needs a buff type and a family name");
            }
        }
        let mut assigned_steps = BTreeSet::new();
        for (ladder_name, steps) in &vocabulary.ladders {
            if ladder_name.trim().is_empty() || steps.len() < 2 {
                bail!("enchantment_map.toml [ladders] {ladder_name:?} needs at least two named steps");
            }
            for step in steps {
                if step.trim().is_empty() || !assigned_steps.insert(step) {
                    bail!("enchantment_map.toml [ladders] {ladder_name:?} repeats or leaves blank step {step:?}");
                }
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
        Stat::by_name(&template.replace("{item}", target_word)).or_else(|| {
            self.effect
                .by_item_default
                .get(effect_type)
                .filter(|default_stat| default_stat.as_str() == target_word)
                .and_then(|default_stat| Stat::by_name(default_stat))
        })
    }

    pub fn stats_for_effect(&self, effect: &Effect, qualifiers: &EffectTargetQualifiers) -> Option<Vec<&'static Stat>> {
        effect.simple_integer_amount()?;
        let effect_type = effect.types.first()?;
        if self.effect.engine_only.contains_key(effect_type) {
            return None;
        }
        if let Some(stat_name) = self.effect.fixed.get(effect_type) {
            if self.qualified_targets(effect, qualifiers).is_some() {
                return None;
            }
            return Some(vec![Stat::by_name(stat_name).expect("validated at load")]);
        }
        let targets: Vec<&str> =
            if effect.targets.is_empty() { vec![""] } else { effect.targets.iter().map(String::as_str).collect() };
        let mut stats = Vec::new();
        for target in targets {
            if let Some(stat) = self.effect_stat(effect_type, target) {
                stats.push(stat);
            } else if !self.effect.companion_targets.words.contains(target) {
                return None;
            }
        }
        (!stats.is_empty()).then_some(stats)
    }

    pub fn qualified_targets(&self, effect: &Effect, qualifiers: &EffectTargetQualifiers) -> Option<String> {
        let effect_type = effect.types.first()?.as_str();
        let stat_name = self.effect.fixed.get(effect_type).map(String::as_str)?;
        let targets: Vec<&str> = effect
            .targets
            .iter()
            .map(String::as_str)
            .filter(|target| !target.is_empty() && *target != "All")
            .filter(|target| !self.target_contradicts_stat(stat_name, target))
            .collect();
        targets.iter().any(|target| qualifiers.contains(target, self)).then(|| targets.join(", "))
    }

    fn target_contradicts_stat(&self, stat_name: &str, target: &str) -> bool {
        let is_energy = DAMAGE_TYPES.iter().any(|damage_type| {
            damage_type.name == target
                && matches!(damage_type.category, DamageCategory::Elemental | DamageCategory::Energy)
        });
        is_energy && self.effect.energy_target_artifacts.stats.contains(stat_name)
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
