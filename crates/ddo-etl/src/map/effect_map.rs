use crate::xml::effect::Effect;
use crate::xml::spells::Spell;
use anyhow::{bail, ensure, Context, Result};
use ddo_model::enums::{BonusType, DamageCategory};
use ddo_model::seeds::{WeaponType, DAMAGE_TYPES, WEAPON_TYPES};
use ddo_model::stats::{Stat, STATS};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::LazyLock;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectMap {
    #[serde(default)]
    pub description_trim_period: BTreeSet<String>,
    #[serde(default)]
    pub damage_as_target: BTreeSet<String>,
    #[serde(default)]
    pub stat_type_effects: BTreeMap<String, BTreeMap<String, String>>,
    pub family: FamilyVocabulary,
    pub effect: EffectVocabulary,
    #[serde(default)]
    pub named_effect_ids: BTreeMap<String, i64>,
    #[serde(default)]
    pub augment_line_names: Vec<AugmentLineName>,
    #[serde(default)]
    pub set_tier_prose_bonuses: Vec<SetTierProseLink>,
    #[serde(default)]
    pub augment_combined_lines: Vec<AugmentCombinedLine>,
    #[serde(default)]
    pub option_companion_targets: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub option_target_aliases: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default)]
    pub option_name_aliases: BTreeMap<String, String>,
    pub item_aliases: BTreeMap<String, String>,
    #[serde(default)]
    pub stat_name_aliases: BTreeMap<String, String>,
    pub bonus_type_aliases: BTreeMap<String, String>,
    pub weapon_aliases: BTreeMap<String, String>,
    #[serde(default)]
    pub names: BTreeMap<String, String>,
    #[serde(default)]
    pub line_templates: BTreeMap<String, String>,
    #[serde(default)]
    pub template_name_aliases: BTreeMap<String, String>,
    #[serde(default)]
    pub set_bonus_line_templates: BTreeMap<String, String>,
    #[serde(default)]
    pub description_templates: BTreeMap<String, String>,
    #[serde(default)]
    pub definition_defaults: BTreeMap<String, i64>,
    #[serde(default)]
    pub fixed_buff_values: BTreeMap<String, i64>,
    #[serde(default)]
    pub tier_groups: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub groups: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub skill_ability_groups: SkillAbilityGroups,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillAbilityGroups {
    pub source: String,
    pub read: String,
    pub skills: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetTierProseLink {
    pub description: String,
    pub effect: String,
    pub bonus_type: Option<String>,
    pub value: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilyVocabulary {
    pub enhancement: Vec<String>,
    #[serde(default)]
    pub owner_type_precedence: BTreeSet<String>,
    #[serde(default)]
    pub groups: BTreeMap<String, String>,
    pub fixed: BTreeMap<String, String>,
    pub by_item: BTreeMap<String, String>,
    #[serde(default)]
    pub item_named_effects: BTreeMap<String, String>,
    #[serde(default)]
    pub text_only: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectVocabulary {
    pub fixed: BTreeMap<String, String>,
    pub targeted: BTreeMap<String, TargetedEffect>,
    pub by_item: BTreeMap<String, String>,
    pub by_item_default: BTreeMap<String, String>,
    pub companion_targets: CompanionTargets,
    pub energy_target_artifacts: EnergyTargetArtifacts,
    pub engine_only: BTreeMap<String, String>,
    #[serde(default)]
    pub owner_extra_stats: Vec<OwnerExtraStat>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerExtraStat {
    pub owner_kind: String,
    pub owner_name: String,
    pub effect_type: String,
    pub stat: String,
    pub source: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetedEffect {
    pub targets: BTreeMap<String, Vec<String>>,
    pub qualified_name: Option<String>,
    #[serde(default)]
    pub family_names: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AugmentLineName {
    pub augment: String,
    pub source_name: String,
    pub effect_name: String,
    pub source: String,
    pub read: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AugmentCombinedLine {
    pub augment: String,
    pub first_effect: String,
    pub first_bonus_type: String,
    pub first_value_from: i64,
    pub second_effect: String,
    pub second_bonus_type: String,
    pub second_value: i64,
    pub effect_name: String,
    pub source: String,
    pub read: String,
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

pub static EFFECT_MAP: LazyLock<EffectMap> =
    LazyLock::new(|| EffectMap::load().expect("data/effect_map.toml is valid"));

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

    pub fn contains(&self, target: &str, vocabulary: &EffectMap) -> bool {
        self.names.contains(target)
            || WeaponType::by_name(target).is_some()
            || vocabulary.weapon_aliases.get(target).is_some_and(|name| WeaponType::by_name(name).is_some())
            || DAMAGE_TYPES.iter().any(|damage_type| damage_type.name == target)
    }
}

impl EffectMap {
    pub fn load() -> Result<Self> {
        Self::from_toml(include_str!("../../data/effect_map.toml"))
    }

    pub fn from_toml(toml_text: &str) -> Result<Self> {
        let vocabulary: Self = toml::from_str(toml_text)?;
        for (stat, types) in &vocabulary.stat_type_effects {
            ensure!(
                Stat::by_name(stat).is_some() && !types.is_empty(),
                "effect_map.toml [stat_type_effects] {stat:?} needs a known stat and at least one type"
            );
            for (bonus_type, effect) in types {
                ensure!(BonusType::parse(bonus_type).is_some() && !effect.trim().is_empty(), "effect_map.toml [stat_type_effects] {stat:?} / {bonus_type:?} needs a known type and a named target");
            }
        }
        let mut prose_descriptions = BTreeSet::new();
        for rule in &vocabulary.set_tier_prose_bonuses {
            if rule.description.trim().is_empty()
                || rule.effect.trim().is_empty()
                || match (&rule.bonus_type, rule.value) {
                    (Some(bonus_type), Some(_)) => BonusType::parse(bonus_type).is_none(),
                    (None, None) => false,
                    _ => true,
                }
                || !prose_descriptions.insert(&rule.description)
            {
                bail!(
                    "effect_map.toml [[set_tier_prose_bonuses]] has an invalid or repeated rule for {:?}",
                    rule.description
                );
            }
        }
        let mut reserved_ids = BTreeSet::new();
        for (name, effect_id) in &vocabulary.named_effect_ids {
            if name.trim().is_empty()
                || *effect_id <= 0
                || !reserved_ids.insert(effect_id)
                || STATS.iter().any(|stat| stat.id == *effect_id || stat.name == name)
            {
                bail!("effect_map.toml [named_effect_ids] {name:?} has a blank name, duplicate or seeded stat id");
            }
        }
        let mut augmented_names = BTreeSet::new();
        for rule in &vocabulary.augment_line_names {
            if rule.augment.trim().is_empty()
                || rule.source_name.trim().is_empty()
                || !vocabulary.named_effect_ids.contains_key(&rule.effect_name)
                || !(rule.source.starts_with("https://ddowiki.com/page/")
                    || rule.source == "maetrim:Augments/CannithAndRandomItem.Augments.xml")
                || rule.read.len() != 10
                || !augmented_names.insert((&rule.augment, &rule.source_name))
            {
                bail!("effect_map.toml [[augment_line_names]] has an invalid or repeated rule for {:?}", rule.augment);
            }
        }
        let mut combined_augments = BTreeSet::new();
        for rule in &vocabulary.augment_combined_lines {
            if !combined_augments.insert(&rule.augment)
                || rule.augment.trim().is_empty()
                || rule.first_effect == rule.second_effect
                || !rule.source.starts_with("https://ddowiki.com/page/")
                || rule.read.len() != 10
                || BonusType::parse(&rule.first_bonus_type).is_none()
                || BonusType::parse(&rule.second_bonus_type).is_none()
            {
                bail!("effect_map.toml [[augment_combined_lines]] has invalid rule for {:?}", rule.augment);
            }
        }
        for (line_name, targets) in &vocabulary.option_companion_targets {
            if line_name.trim().is_empty()
                || targets.is_empty()
                || targets.iter().any(|target| target.trim().is_empty())
            {
                bail!("effect_map.toml [option_companion_targets] {line_name:?} needs nonblank targets");
            }
        }
        for (line_name, aliases) in &vocabulary.option_target_aliases {
            if line_name.trim().is_empty()
                || aliases.is_empty()
                || aliases.iter().any(|(source, target)| source.trim().is_empty() || target.trim().is_empty())
            {
                bail!("effect_map.toml [option_target_aliases] {line_name:?} needs nonblank aliases");
            }
        }
        for (source, replacement) in &vocabulary.option_name_aliases {
            if source.trim().is_empty() || replacement.trim().is_empty() {
                bail!("effect_map.toml [option_name_aliases] needs nonblank words");
            }
        }
        for (effect_name, template) in &vocabulary.line_templates {
            if effect_name.trim().is_empty() || template.trim().is_empty() {
                bail!("effect_map.toml [line_templates] {effect_name:?} has an empty template");
            }
        }
        for (effect_name, alias) in &vocabulary.template_name_aliases {
            if effect_name.trim().is_empty() || alias.trim().is_empty() {
                bail!("effect_map.toml [template_name_aliases] {effect_name:?} needs an effect name and alias");
            }
        }
        for (effect_name, template) in &vocabulary.set_bonus_line_templates {
            if effect_name.trim().is_empty() || template.trim().is_empty() || !template.contains("{1}") {
                bail!("effect_map.toml [set_bonus_line_templates] {effect_name:?} needs a value template");
            }
        }
        for (effect_name, template) in &vocabulary.description_templates {
            if effect_name.trim().is_empty() || template.trim().is_empty() {
                bail!("effect_map.toml [description_templates] {effect_name:?} has an empty template");
            }
        }
        for effect_name in vocabulary.definition_defaults.keys() {
            if effect_name.trim().is_empty() {
                bail!("effect_map.toml [definition_defaults] has an empty effect name");
            }
        }
        for buff_kind in vocabulary.fixed_buff_values.keys() {
            if buff_kind.trim().is_empty() {
                bail!("effect_map.toml [fixed_buff_values] has an empty buff type");
            }
        }
        for (section, stat_names) in [
            ("family.fixed", &vocabulary.family.fixed),
            ("effect.fixed", &vocabulary.effect.fixed),
            ("effect.by_item_default", &vocabulary.effect.by_item_default),
        ] {
            for (kind, stat_name) in stat_names {
                if Stat::by_name(stat_name).is_none() {
                    bail!("effect_map.toml [{section}] {kind} names unknown stat {stat_name:?}");
                }
            }
        }
        for (kind, group_name) in &vocabulary.family.groups {
            if kind.trim().is_empty() || vocabulary.group_members(group_name).is_none() {
                bail!("effect_map.toml [family.groups] {kind:?} names unknown group {group_name:?}");
            }
        }
        for (kind, template) in &vocabulary.family.item_named_effects {
            if kind.trim().is_empty() || template.matches("{item}").count() != 1 {
                bail!("effect_map.toml [family.item_named_effects] {kind:?} needs one {{item}} in {template:?}");
            }
        }
        for (section, templates) in
            [("family.by_item", &vocabulary.family.by_item), ("effect.by_item", &vocabulary.effect.by_item)]
        {
            for (kind, template) in templates {
                let Some((prefix, suffix)) = template.split_once("{item}") else {
                    bail!("effect_map.toml [{section}] {kind} template {template:?} names no stat");
                };
                if suffix.contains("{item}")
                    || !STATS.iter().any(|stat| stat.name.starts_with(prefix) && stat.name.ends_with(suffix))
                {
                    bail!("effect_map.toml [{section}] {kind} template {template:?} names no stat");
                }
            }
        }
        for (effect_type, targeted) in &vocabulary.effect.targeted {
            if targeted.targets.is_empty()
                || targeted.qualified_name.as_deref().is_some_and(|name| name.trim().is_empty())
            {
                bail!("effect_map.toml [effect.targeted] {effect_type} needs targets and a nonblank qualified name");
            }
            for (target, stat_names) in &targeted.targets {
                if target.trim().is_empty()
                    || stat_names.is_empty()
                    || stat_names.iter().collect::<BTreeSet<_>>().len() != stat_names.len()
                {
                    bail!("effect_map.toml [effect.targeted] {effect_type} has invalid target {target:?} or duplicate stats");
                }
                for stat_name in stat_names {
                    if Stat::by_name(stat_name).is_none() {
                        bail!(
                            "effect_map.toml [effect.targeted] {effect_type}.{target} names unknown stat {stat_name:?}"
                        );
                    }
                }
            }
            for (stat_name, family_name) in &targeted.family_names {
                if family_name.trim().is_empty()
                    || !targeted.targets.values().any(|stat_names| stat_names.contains(stat_name))
                {
                    bail!("effect_map.toml [effect.targeted] {effect_type} has a family name for unmapped stat {stat_name:?}");
                }
            }
        }
        if !vocabulary.skill_ability_groups.skills.is_empty()
            && (vocabulary.skill_ability_groups.source != "https://ddowiki.com/page/Skills"
                || vocabulary.skill_ability_groups.read.trim().is_empty())
        {
            bail!("effect_map.toml [skill_ability_groups] needs its Skills page and read date");
        }
        for (group_name, stat_names) in &vocabulary.groups {
            if group_name.trim().is_empty() || stat_names.len() < 2 {
                bail!("effect_map.toml [groups] {group_name:?} needs at least two stats");
            }
            for stat_name in stat_names {
                if Stat::by_name(stat_name).is_none() {
                    bail!("effect_map.toml [groups] {group_name:?} names unknown stat {stat_name:?}");
                }
            }
        }
        for (ability, stat_names) in &vocabulary.skill_ability_groups.skills {
            if Stat::by_name(ability).is_none() || stat_names.is_empty() {
                bail!("effect_map.toml [skill_ability_groups.skills] invalid ability {ability:?}");
            }
            for stat_name in stat_names {
                if Stat::by_name(stat_name).is_none() {
                    bail!("effect_map.toml [skill_ability_groups.skills] {ability:?} names unknown stat {stat_name:?}");
                }
            }
        }
        for stat_name in &vocabulary.effect.energy_target_artifacts.stats {
            if !vocabulary.effect.fixed.values().any(|mapped_stat| mapped_stat == stat_name) {
                bail!("effect_map.toml [effect.energy_target_artifacts] names unmapped fixed stat {stat_name:?}");
            }
        }
        for word in &vocabulary.effect.companion_targets.words {
            if word.trim().is_empty() || word == "All" {
                bail!("effect_map.toml [effect.companion_targets] has invalid word {word:?}");
            }
        }
        let mapped_effect_types = vocabulary
            .effect
            .fixed
            .keys()
            .chain(vocabulary.effect.targeted.keys())
            .chain(vocabulary.effect.by_item.keys())
            .chain(vocabulary.effect.by_item_default.keys());
        let mapped_effect_types: BTreeSet<_> = mapped_effect_types.collect();
        for effect_type in &vocabulary.damage_as_target {
            if !mapped_effect_types.contains(effect_type) {
                bail!("effect_map.toml damage_as_target names {effect_type:?}, which is not mapped to a stat bonus");
            }
        }
        let overlapping_types: BTreeSet<_> =
            mapped_effect_types.into_iter().filter(|kind| vocabulary.effect.engine_only.contains_key(*kind)).collect();
        if !overlapping_types.is_empty() {
            bail!(
                "effect_map.toml: {overlapping_types:?} are both mapped to a stat and listed in [effect.engine_only]"
            );
        }
        for kind in vocabulary.effect.fixed.keys() {
            if vocabulary.effect.by_item.contains_key(kind) || vocabulary.effect.targeted.contains_key(kind) {
                bail!("effect_map.toml [effect.fixed] {kind} is also mapped in [effect.by_item]");
            }
        }
        for kind in vocabulary.effect.targeted.keys() {
            if vocabulary.effect.by_item.contains_key(kind) || vocabulary.effect.by_item_default.contains_key(kind) {
                bail!("effect_map.toml [effect.targeted] {kind} is also mapped in another effect section");
            }
        }
        for kind in &vocabulary.family.enhancement {
            if vocabulary.family.fixed.contains_key(kind)
                || vocabulary.family.by_item.contains_key(kind)
                || vocabulary.family.groups.contains_key(kind)
                || vocabulary.family.text_only.contains_key(kind)
            {
                bail!("effect_map.toml [family.enhancement] {kind} is also mapped in another family section");
            }
        }
        if vocabulary.family.owner_type_precedence.iter().any(|kind| kind.trim().is_empty()) {
            bail!("effect_map.toml [family] owner_type_precedence contains a blank buff type");
        }
        for (kind, reason) in &vocabulary.family.text_only {
            if kind.trim().is_empty() || reason.trim().is_empty() {
                bail!("effect_map.toml [family.text_only] {kind:?} needs a buff type and reason");
            }
            if vocabulary.family.fixed.contains_key(kind)
                || vocabulary.family.by_item.contains_key(kind)
                || vocabulary.family.groups.contains_key(kind)
            {
                bail!("effect_map.toml [family.text_only] {kind} is also mapped to a stat");
            }
        }
        for kind in vocabulary.family.groups.keys() {
            if vocabulary.family.fixed.contains_key(kind) || vocabulary.family.by_item.contains_key(kind) {
                bail!("effect_map.toml [family.groups] {kind} is also mapped to a stat");
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
                bail!("effect_map.toml [item_aliases] {alias} names no stat through {word:?}");
            }
        }
        for (source_name, stat_name) in &vocabulary.stat_name_aliases {
            if source_name.trim().is_empty()
                || Stat::by_name(source_name).is_some()
                || Stat::by_name(stat_name).is_none()
            {
                bail!("effect_map.toml [stat_name_aliases] {source_name:?} must name a retired source stat and a seeded stat");
            }
        }
        for (alias, bonus_type_name) in &vocabulary.bonus_type_aliases {
            if !bonus_type_name.is_empty() && BonusType::parse(bonus_type_name).is_none() {
                bail!("effect_map.toml [bonus_type_aliases] {alias} names unknown bonus type {bonus_type_name:?}");
            }
        }
        for (alias, weapon_name) in &vocabulary.weapon_aliases {
            if WeaponType::by_name(weapon_name).is_none() {
                bail!("effect_map.toml [weapon_aliases] {alias} names unknown weapon type {weapon_name:?}");
            }
        }
        for rule in &vocabulary.effect.owner_extra_stats {
            if rule.owner_kind != "augment"
                || rule.owner_name.trim().is_empty()
                || rule.effect_type.trim().is_empty()
                || Stat::by_name(&rule.stat).is_none()
                || rule.source != "maetrim:Augments/Reaper.Augments.xml"
            {
                bail!("effect_map.toml [[effect.owner_extra_stats]] has invalid rule for {:?}", rule.owner_name);
            }
        }
        for (buff_type, family_name) in &vocabulary.names {
            if buff_type.trim().is_empty() || family_name.trim().is_empty() {
                bail!("effect_map.toml [names] {buff_type:?} needs a buff type and a family name");
            }
        }
        let mut assigned_steps = BTreeSet::new();
        for (group_name, steps) in &vocabulary.tier_groups {
            if group_name.trim().is_empty() || steps.len() < 2 {
                bail!("effect_map.toml [tier_groups] {group_name:?} needs at least two named steps");
            }
            for step in steps {
                if step.trim().is_empty() || !assigned_steps.insert(step) {
                    bail!("effect_map.toml [tier_groups] {group_name:?} repeats or leaves blank step {step:?}");
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
        self.stat_from_source_name(&template.replace("{item}", target_word)).or_else(|| {
            self.effect
                .by_item_default
                .get(effect_type)
                .filter(|default_stat| default_stat.as_str() == target_word)
                .and_then(|default_stat| Stat::by_name(default_stat))
        })
    }

    pub fn stat_name_from_source(&self, source_name: &str) -> String {
        self.stat_name_aliases.get(source_name).cloned().unwrap_or_else(|| source_name.to_string())
    }

    fn stat_from_source_name(&self, source_name: &str) -> Option<&'static Stat> {
        Stat::by_name(&self.stat_name_from_source(source_name))
    }

    pub fn is_classified_effect_type(&self, effect_type: &str) -> bool {
        let vocabulary = &self.effect;

        vocabulary.fixed.contains_key(effect_type)
            || vocabulary.targeted.contains_key(effect_type)
            || vocabulary.by_item.contains_key(effect_type)
            || vocabulary.by_item_default.contains_key(effect_type)
            || vocabulary.engine_only.contains_key(effect_type)
            || effect_type == "SkillBonusAbility"
    }

    pub fn stats_for_effect(&self, effect: &Effect, qualifiers: &EffectTargetQualifiers) -> Option<Vec<&'static Stat>> {
        if let Some(damage) = effect.damage.as_deref().filter(|damage| {
            effect.types.first().is_some_and(|kind| {
                self.damage_as_target.contains(kind) && self.damage_target_stats(kind, damage, qualifiers).is_some()
            })
        }) {
            let mut targeted = effect.clone();
            if !targeted.targets.iter().any(|target| target == damage) {
                targeted.targets.push(damage.to_string());
            }
            return self.stats_for_effect_targets(&targeted, qualifiers);
        }
        self.stats_for_effect_targets(effect, qualifiers)
    }

    pub(crate) fn damage_target_stats(
        &self,
        effect_type: &str,
        damage: &str,
        qualifiers: &EffectTargetQualifiers,
    ) -> Option<Vec<&'static Stat>> {
        if damage.trim().is_empty() {
            return None;
        }
        let target = Effect {
            types: vec![effect_type.to_string()],
            amount_type: Some("Simple".to_string()),
            amounts: vec![1.0],
            targets: vec![damage.to_string()],
            ..Effect::default()
        };
        self.stats_for_effect_targets(&target, qualifiers)
    }

    fn stats_for_effect_targets(
        &self,
        effect: &Effect,
        qualifiers: &EffectTargetQualifiers,
    ) -> Option<Vec<&'static Stat>> {
        effect.simple_integer_amount()?;
        let effect_type = effect.types.first()?;
        if self.effect.engine_only.contains_key(effect_type) {
            return None;
        }
        if effect_type == "SkillBonusAbility" {
            let [ability] = effect.targets.as_slice() else { return None };
            return self
                .skill_ability_groups
                .skills
                .get(ability)
                .map(|names| names.iter().map(|name| Stat::by_name(name).expect("validated skill stat")).collect());
        }
        if let Some(targeted) = self.effect.targeted.get(effect_type) {
            let targets: Vec<&str> = if let Some(weapon_class) = self.complete_weapon_class(effect, targeted) {
                vec![weapon_class]
            } else if effect.targets.is_empty() {
                vec!["All"]
            } else {
                effect.targets.iter().map(String::as_str).collect()
            };
            let mut stats = Vec::new();
            for target in targets {
                for stat_name in targeted.targets.get(target)? {
                    let stat = Stat::by_name(stat_name).expect("validated at load");
                    if !stats.iter().any(|existing: &&Stat| existing.id == stat.id) {
                        stats.push(stat);
                    }
                }
            }
            return (!stats.is_empty()).then_some(stats);
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

    pub fn is_linkable_named_effect(&self, effect_name: &str) -> bool {
        self.named_effect_ids.contains_key(effect_name) || self.family.groups.contains_key(effect_name)
    }

    pub fn set_tier_prose_link(
        &self,
        description: &str,
        groupings: &crate::groupings::Groupings,
    ) -> Result<Option<&SetTierProseLink>> {
        let Some(rule) = self.set_tier_prose_bonuses.iter().find(|rule| rule.description == description) else {
            return Ok(None);
        };
        let Some(bonus_type) = rule.bonus_type.as_deref() else {
            let target = groupings.effect_named(&rule.effect).filter(|effect| {
                Stat::by_name(&rule.effect).is_none()
                    && !effect.is_group
                    && effect.bonuses.is_empty()
                    && effect.default_value.is_none()
                    && crate::build::amount_count(&effect.verbose_name_template, effect.description_template.as_deref())
                        == 0
            });
            anyhow::ensure!(
                target.is_some(),
                "prose line {description:?} names {:?}, which is not a text-only effect without amounts in C",
                rule.effect
            );
            return Ok(Some(rule));
        };
        if Stat::by_name(&rule.effect).is_none() {
            if let Some(grouping) = groupings.grouping_named(&rule.effect) {
                anyhow::ensure!(
                    grouping.accepts_value_at_type(bonus_type),
                    "prose bonus {:?} cannot take one value at {}",
                    rule.effect,
                    bonus_type
                );
            } else if self.group_members(&rule.effect).is_none() {
                bail!("prose bonus {description:?} names {:?}, which is not a stat or grouping in C", rule.effect);
            }
        }
        Ok(Some(rule))
    }

    pub fn group_members(&self, group_name: &str) -> Option<&[String]> {
        self.groups
            .get(group_name)
            .or_else(|| {
                group_name.strip_suffix(" Skills").and_then(|ability| self.skill_ability_groups.skills.get(ability))
            })
            .map(Vec::as_slice)
    }

    pub fn stat_type_effect_definitions<'a>(
        &self,
        index: &'a crate::groupings::Groupings,
    ) -> Result<Vec<&'a crate::groupings::Grouping>> {
        let mut definitions = BTreeMap::new();
        for (stat, types) in &self.stat_type_effects {
            for (bonus_type, name) in types {
                let effect = index.effect_named(name).with_context(|| {
                    format!(
                        "effect_map.toml [stat_type_effects] {stat:?} / {bonus_type:?} names missing effect {name:?}"
                    )
                })?;
                let stat_id = index.effect_named(stat).map(|stat| stat.id);
                let matches_home = effect.home_bonus_type.as_deref() == Some(bonus_type)
                    || (effect.home_bonus_type.is_none()
                        && ddo_model::bonus_type_wording::is_always_unnamed(
                            BonusType::parse(bonus_type).expect("validated type"),
                        ));
                ensure!(
                    !effect.is_group
                        && effect.bonuses.len() == 1
                        && crate::build::amount_count(&effect.verbose_name_template, effect.description_template.as_deref()) == 1
                        && matches_home
                        && effect.bonuses.iter().all(|bonus| {
                            Some(bonus.target_id) == stat_id
                                && bonus.amount_from == 1
                                && bonus.constant.is_none()
                                && bonus.scale == 1.0
                                && bonus.trigger_id.is_none()
                                && bonus.bonus_type.as_deref().is_none_or(|fixed| fixed == bonus_type)
                        }),
                    "effect_map.toml [stat_type_effects] {stat:?} / {bonus_type:?} -> {name:?} needs a single unscaled slot-one bonus on that stat at that type, with a matching home"
                );
                definitions.insert(name, effect);
            }
        }
        Ok(definitions.into_values().collect())
    }

    pub(crate) fn item_named_definitions<'a>(
        &self,
        index: &'a crate::groupings::Groupings,
    ) -> Result<Vec<&'a crate::groupings::Grouping>> {
        let mut definitions = BTreeMap::new();
        for (buff, pattern) in &self.family.item_named_effects {
            let (prefix, suffix) = pattern.split_once("{item}").expect("route pattern is validated");
            let matches = index
                .by_id
                .values()
                .filter(|effect| {
                    effect
                        .name
                        .strip_prefix(prefix)
                        .and_then(|tail| tail.strip_suffix(suffix))
                        .is_some_and(|item| !item.is_empty() && !item.chars().any(char::is_whitespace))
                })
                .collect::<Vec<_>>();
            if matches.is_empty() {
                bail!("effect_map.toml [family.item_named_effects] {buff:?} = {pattern:?} matches no canonical effect");
            }
            for effect in matches {
                definitions.insert(&effect.name, effect);
            }
        }
        Ok(definitions.into_values().collect())
    }

    pub fn group_names(&self) -> Vec<String> {
        self.groups
            .keys()
            .cloned()
            .chain(self.skill_ability_groups.skills.keys().map(|ability| format!("{ability} Skills")))
            .collect()
    }

    pub fn augment_line_name<'a>(&'a self, augment_name: &str, source_name: &str) -> Option<&'a str> {
        self.augment_line_names
            .iter()
            .find(|rule| rule.augment == augment_name && rule.source_name == source_name)
            .map(|rule| rule.effect_name.as_str())
    }

    pub fn qualified_targeted_name(&self, effect: &Effect) -> Option<String> {
        let targeted = self.effect.targeted.get(effect.types.first()?)?;
        let base_name = targeted.qualified_name.as_deref()?;
        if self.complete_weapon_class(effect, targeted).is_some()
            || effect.targets.is_empty()
            || effect.targets.iter().all(|target| targeted.targets.contains_key(target))
        {
            return None;
        }
        let weapon_kinds: Option<Vec<&str>> = effect
            .targets
            .iter()
            .map(|target| {
                let canonical = self.weapon_aliases.get(target).map(String::as_str).unwrap_or(target);
                if matches!(canonical, "One Handed" | "Melee") {
                    return Some("Melee");
                }
                if canonical == "Thrown" {
                    return Some("Ranged");
                }
                let weapon = WeaponType::by_name(canonical)?;
                Some(if weapon.is_ranged() || weapon.is_thrown() { "Ranged" } else { "Melee" })
            })
            .collect();
        let weapon_stat_name = weapon_kinds
            .as_ref()
            .and_then(|kinds| {
                kinds
                    .iter()
                    .all(|kind| *kind == kinds[0])
                    .then(|| targeted.targets.get(kinds[0]).and_then(|stats| stats.first()).map(String::as_str))
            })
            .flatten();
        let name = weapon_stat_name.unwrap_or(base_name);
        Some(format!("{name} ({})", effect.targets.join(", ")))
    }

    fn complete_weapon_class(&self, effect: &Effect, targeted: &TargetedEffect) -> Option<&'static str> {
        let actual: BTreeSet<&str> = effect
            .targets
            .iter()
            .map(|target| self.weapon_aliases.get(target).map(String::as_str).unwrap_or(target))
            .collect();
        if actual.is_empty() || actual.len() != effect.targets.len() {
            return None;
        }
        let ranged: BTreeSet<&str> = WEAPON_TYPES
            .iter()
            .filter(|weapon| weapon.is_ranged() || weapon.name == "Dart")
            .map(|weapon| weapon.name)
            .collect();
        if actual.is_superset(&ranged)
            && actual
                .iter()
                .all(|name| WeaponType::by_name(name).is_some_and(|weapon| weapon.is_ranged() || weapon.is_thrown()))
            && targeted.targets.contains_key("Ranged")
        {
            return Some("Ranged");
        }
        let melee: BTreeSet<&str> = WEAPON_TYPES
            .iter()
            .filter(|weapon| weapon.proficiency.is_some() && !weapon.is_ranged() && !weapon.is_thrown())
            .map(|weapon| weapon.name)
            .collect();
        (actual == melee && targeted.targets.contains_key("Melee")).then_some("Melee")
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
            .ok_or_else(|| anyhow::anyhow!("unknown bonus type {upstream_name:?}; add it to data/effect_map.toml [bonus_type_aliases] or to ddo-model's BonusType"))
    }
}
