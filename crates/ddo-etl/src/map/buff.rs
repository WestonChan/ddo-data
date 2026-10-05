use super::effect_map::{EffectMap, EffectTargetQualifiers, EFFECT_MAP};
use crate::xml::item_buffs::ItemBuffDefinition;
use crate::xml::items::Buff;
use anyhow::{bail, Result};
use ddo_model::enums::BonusType;
use ddo_model::stats::Stat;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmountFrom {
    ItemValue1,
    ItemValue2,
    Constant(i64),
}

impl AmountFrom {
    pub fn amount(self, buff: &Buff) -> Option<i64> {
        match self {
            Self::ItemValue1 => buff.value,
            Self::ItemValue2 => buff.second_value,
            Self::Constant(amount) => Some(amount),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedStat {
    pub stat: &'static Stat,
    pub bonus_type: Option<BonusType>,
    pub amount_from: AmountFrom,
    pub definition_amount: Option<i64>,
}

impl ResolvedStat {
    pub fn amount(&self, buff: &Buff) -> Option<i64> {
        self.amount_from.amount(buff).or(self.definition_amount)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FamilyResolution {
    Enhancement,
    Mapped {
        fixed_stat_name: Option<String>,
        stat_template: Option<String>,
        amount_from: AmountFrom,
        definition_amount: Option<i64>,
    },
    EffectFallback(Vec<ResolvedStat>),
    SkillGroup,
    Effect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuffResolutionSource {
    Family,
    EffectFallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedBuff {
    EnhancementBonus(i64),
    Bonuses { source: BuffResolutionSource, stats: Vec<ResolvedStat> },
    Effect { name: String, value: Option<i64>, target: Option<String> },
}

pub struct BuffResolver {
    vocabulary: &'static EffectMap,
    definitions_by_buff_kind: HashMap<String, ItemBuffDefinition>,
    colliding_titles: HashSet<String>,
    qualifiers: EffectTargetQualifiers,
}

impl BuffResolver {
    pub fn from_definitions(item_buff_definitions: &HashMap<String, ItemBuffDefinition>) -> Self {
        let vocabulary: &'static EffectMap = &EFFECT_MAP;
        let mut kinds_by_title: HashMap<String, Vec<&str>> = HashMap::new();
        for (buff_kind, definition) in item_buff_definitions {
            let title = split_display_title(&definition.display_text)
                .map(|(title, _)| title_without_display_tokens(title))
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| words_from_buff_kind(buff_kind));
            kinds_by_title.entry(title).or_default().push(buff_kind);
        }
        let colliding_titles =
            kinds_by_title.into_iter().filter_map(|(title, kinds)| (kinds.len() > 1).then_some(title)).collect();
        Self {
            vocabulary,
            definitions_by_buff_kind: item_buff_definitions.clone(),
            colliding_titles,
            qualifiers: EffectTargetQualifiers::default(),
        }
    }

    pub fn with_qualifiers(mut self, qualifiers: EffectTargetQualifiers) -> Self {
        self.qualifiers = qualifiers;
        self
    }

    pub fn family_resolutions(&self) -> Result<BTreeMap<String, FamilyResolution>> {
        let mut family_names: BTreeSet<&str> = self.definitions_by_buff_kind.keys().map(String::as_str).collect();
        family_names.extend(self.vocabulary.family.enhancement.iter().map(String::as_str));
        family_names.extend(self.vocabulary.family.fixed.keys().map(String::as_str));
        family_names.extend(self.vocabulary.family.by_item.keys().map(String::as_str));
        family_names.extend(self.vocabulary.family.text_only.keys().map(String::as_str));
        family_names
            .into_iter()
            .map(|family_name| Ok((family_name.to_string(), self.family_resolution(family_name)?)))
            .collect()
    }

    pub fn family_resolution(&self, buff_kind: &str) -> Result<FamilyResolution> {
        if buff_kind == "SkillBonusAbility" {
            return Ok(FamilyResolution::SkillGroup);
        }
        if self.vocabulary.family.enhancement.iter().any(|kind| kind == buff_kind) {
            return Ok(FamilyResolution::Enhancement);
        }
        if self.vocabulary.family.text_only.contains_key(buff_kind) {
            return Ok(FamilyResolution::Effect);
        }
        let fixed_stat_name = self.vocabulary.family.fixed.get(buff_kind).cloned();
        let stat_template = self.vocabulary.family.by_item.get(buff_kind).cloned();
        if fixed_stat_name.is_some() || stat_template.is_some() {
            let definition_amount =
                self.definitions_by_buff_kind.get(buff_kind).and_then(|definition| definition.fixed_amount);
            let amount_from = self
                .definitions_by_buff_kind
                .get(buff_kind)
                .and_then(|definition| definition.fixed_amount.filter(|_| !definition.display_text.contains("%v1")))
                .map(AmountFrom::Constant)
                .unwrap_or(AmountFrom::ItemValue1);
            return Ok(FamilyResolution::Mapped { fixed_stat_name, stat_template, amount_from, definition_amount });
        }
        let Some(definition) = self.definitions_by_buff_kind.get(buff_kind) else {
            return Ok(FamilyResolution::Effect);
        };
        if definition.has_activation_condition || definition.effects.is_empty() {
            return Ok(FamilyResolution::Effect);
        }
        let has_first_value = definition.display_text.contains("%v1");
        let has_second_value = definition.display_text.contains("%v2");
        let mut stats = Vec::new();
        for effect in &definition.effects {
            let mut effect_stats = Vec::new();
            let mut definition_amount = None;
            for effect_type in &effect.types {
                if self.vocabulary.effect.engine_only.contains_key(effect_type) {
                    continue;
                }
                let mut single_type_effect = effect.clone();
                single_type_effect.types = vec![effect_type.clone()];
                if single_type_effect.plain_integer_amount().is_none() {
                    return Ok(FamilyResolution::Effect);
                }
                let Some(type_stats) = self.vocabulary.stats_for_effect(&single_type_effect, &self.qualifiers) else {
                    return Ok(FamilyResolution::Effect);
                };
                definition_amount = single_type_effect.plain_integer_amount();
                effect_stats.extend(type_stats);
            }
            if effect_stats.is_empty() {
                continue;
            }
            let definition_amount = definition_amount.expect("checked by stats_for_effect");
            let amount_from = if has_second_value && (!stats.is_empty() || !has_first_value) {
                AmountFrom::ItemValue2
            } else if has_first_value || definition_amount == 0 {
                AmountFrom::ItemValue1
            } else {
                AmountFrom::Constant(definition_amount)
            };
            let bonus_type = self.vocabulary.effect_bonus_type(effect.bonus.as_deref().unwrap_or(""))?;
            let fallback_amount = (definition_amount != 0).then_some(definition_amount);
            stats.extend(effect_stats.into_iter().map(|stat| ResolvedStat {
                stat,
                bonus_type,
                amount_from,
                definition_amount: fallback_amount,
            }));
        }
        if stats.is_empty() {
            Ok(FamilyResolution::Effect)
        } else {
            Ok(FamilyResolution::EffectFallback(stats))
        }
    }

    pub fn resolved(&self, buff: &Buff) -> Result<ResolvedBuff> {
        let buff_kind = buff.kind.trim();
        let target = buff.target.as_deref().map(str::trim).filter(|target| !target.is_empty());
        match self.family_resolution(buff_kind)? {
            FamilyResolution::Enhancement => match buff.value {
                Some(value) => Ok(ResolvedBuff::EnhancementBonus(value)),
                None => bail!("{buff_kind} without Value1"),
            },
            FamilyResolution::Mapped { fixed_stat_name, stat_template, amount_from, definition_amount } => {
                let stat_name = if let Some(fixed_stat_name) = fixed_stat_name {
                    stat_template
                        .as_deref()
                        .zip(target)
                        .map(|(stat_template, target)| self.stat_name_from_template(stat_template, target))
                        .filter(|stat_name| Stat::by_name(stat_name).is_some())
                        .unwrap_or(fixed_stat_name)
                } else {
                    let Some((stat_template, target)) = stat_template.as_deref().zip(target) else {
                        bail!("{buff_kind} needs an <Item> sub-target to name its stat");
                    };
                    self.stat_name_from_template(stat_template, target)
                };
                let Some(stat) = Stat::by_name(&stat_name) else {
                    bail!("{buff_kind} with Item {:?} resolves to {stat_name:?}, which is not a stat; extend [family.by_item], [family.fixed] or the stats seed", buff.target);
                };
                let item_bonus_type = self.vocabulary.family_bonus_type(buff.bonus_type.as_deref().unwrap_or(""))?;
                let bonus_type = self.resolved_bonus_type(buff_kind, item_bonus_type, None)?;
                let resolved_stat = ResolvedStat { stat, bonus_type, amount_from, definition_amount };
                Ok(ResolvedBuff::Bonuses { source: BuffResolutionSource::Family, stats: vec![resolved_stat] })
            }
            FamilyResolution::EffectFallback(mut stats) => {
                let item_bonus_type = self.vocabulary.family_bonus_type(buff.bonus_type.as_deref().unwrap_or(""))?;
                for stat in &mut stats {
                    stat.bonus_type = self.resolved_bonus_type(buff_kind, item_bonus_type, stat.bonus_type)?;
                }
                Ok(ResolvedBuff::Bonuses { source: BuffResolutionSource::EffectFallback, stats })
            }
            FamilyResolution::SkillGroup => {
                let ability = target.ok_or_else(|| anyhow::anyhow!("SkillBonusAbility needs an ability target"))?;
                let group_name = format!("{ability} Skills");
                let members = self
                    .vocabulary
                    .group_members(&group_name)
                    .ok_or_else(|| anyhow::anyhow!("SkillBonusAbility has unknown ability {ability:?}"))?;
                let bonus_type = self.vocabulary.family_bonus_type(buff.bonus_type.as_deref().unwrap_or(""))?;
                let stats = members
                    .iter()
                    .map(|name| ResolvedStat {
                        stat: Stat::by_name(name).expect("validated skill stat"),
                        bonus_type,
                        amount_from: AmountFrom::ItemValue1,
                        definition_amount: None,
                    })
                    .collect();
                Ok(ResolvedBuff::Bonuses { source: BuffResolutionSource::EffectFallback, stats })
            }
            FamilyResolution::Effect => Ok(ResolvedBuff::Effect {
                name: buff_kind.to_string(),
                value: buff.value,
                target: target.map(str::to_string),
            }),
        }
    }

    fn stat_name_from_template(&self, stat_template: &str, target: &str) -> String {
        let target = self.vocabulary.item_aliases.get(target).map(String::as_str).unwrap_or(target);
        stat_template.replace("{item}", target)
    }

    fn definition_bonus_type(&self, buff_kind: &str) -> Result<Option<BonusType>> {
        match self.definitions_by_buff_kind.get(buff_kind).and_then(|definition| definition.bonus_type_name.as_deref())
        {
            Some(bonus_type_name) => self.vocabulary.family_bonus_type(bonus_type_name),
            None => Ok(None),
        }
    }

    fn definition_uses_item_bonus_type(&self, buff_kind: &str) -> bool {
        self.definitions_by_buff_kind.get(buff_kind).is_some_and(|definition| definition.display_text.contains("%b1"))
    }

    fn resolved_bonus_type(
        &self,
        buff_kind: &str,
        item_bonus_type: Option<BonusType>,
        effect_bonus_type: Option<BonusType>,
    ) -> Result<Option<BonusType>> {
        let definition_bonus_type = self.definition_bonus_type(buff_kind)?;
        if self.definition_uses_item_bonus_type(buff_kind)
            || self.vocabulary.family.owner_type_precedence.contains(buff_kind)
        {
            Ok(item_bonus_type.or(effect_bonus_type).or(definition_bonus_type))
        } else {
            let fixed_default = (self.vocabulary.family.fixed.contains_key(buff_kind)
                && self
                    .definitions_by_buff_kind
                    .get(buff_kind)
                    .is_some_and(|definition| definition.effects.is_empty()))
            .then_some(BonusType::Equipment);
            Ok(effect_bonus_type.or(definition_bonus_type).or(fixed_default).or(item_bonus_type))
        }
    }

    pub fn description_template(&self, buff_kind: &str) -> &str {
        self.definitions_by_buff_kind.get(buff_kind).map(|definition| definition.display_text.as_str()).unwrap_or("")
    }

    pub fn definition_defaults(&self, buff_kind: &str, amount_count: i64) -> (Option<i64>, Option<i64>) {
        let Some(definition) = self.definitions_by_buff_kind.get(buff_kind) else {
            return (None, None);
        };
        let amounts: Vec<i64> = definition
            .effects
            .iter()
            .filter_map(|effect| {
                let mut single_type = effect.clone();
                single_type.types.truncate(1);
                single_type.simple_integer_amount()
            })
            .filter(|amount| *amount != 0)
            .collect();
        let first = (amount_count >= 1).then(|| definition.fixed_amount.or_else(|| amounts.first().copied())).flatten();
        let second = (amount_count >= 2).then(|| amounts.get(1).copied()).flatten();
        (first, second)
    }

    pub fn link_bonus_type(&self, buff: &Buff) -> Result<Option<BonusType>> {
        self.resolved_bonus_type(
            buff.kind.trim(),
            self.vocabulary.family_bonus_type(buff.bonus_type.as_deref().unwrap_or(""))?,
            None,
        )
    }

    pub fn family_name(&self, buff: &Buff, stat_name: Option<&str>) -> String {
        let buff_kind = buff.kind.trim();
        if self.vocabulary.family.by_item.contains_key(buff_kind) {
            return stat_name
                .map(str::to_string)
                .or_else(|| {
                    self.vocabulary.family.by_item.get(buff_kind).and_then(|template| {
                        buff.target.as_deref().map(|target| self.stat_name_from_template(template, target))
                    })
                })
                .unwrap_or_else(|| words_from_buff_kind(buff_kind));
        }
        let title = self.vocabulary.names.get(buff_kind).cloned().unwrap_or_else(|| {
            self.definitions_by_buff_kind
                .get(buff_kind)
                .and_then(|definition| split_display_title(&definition.display_text).map(|(title, _)| title))
                .map(title_without_display_tokens)
                .filter(|title| !title.is_empty())
                .filter(|title| !self.colliding_titles.contains(title))
                .unwrap_or_else(|| words_from_buff_kind(buff_kind))
        });
        let target_name = self
            .definitions_by_buff_kind
            .get(buff_kind)
            .filter(|definition| definition.display_text.contains("%i1"))
            .and_then(|_| buff.target.as_deref().or(buff.description.as_deref()))
            .map(str::trim)
            .filter(|target| !target.is_empty());
        if let Some(target_name) = target_name {
            format!("{title} {target_name}")
        } else {
            title
        }
    }

    pub fn effect_family_name(&self, effect_type: &str, target: Option<&str>, stat_name: &str) -> String {
        if effect_type == "AbilityBonus" && target == Some("All") {
            return "All Ability Scores".to_string();
        }
        if !self.definitions_by_buff_kind.contains_key(effect_type) {
            return stat_name.to_string();
        }
        self.family_name(
            &Buff {
                kind: effect_type.to_string(),
                target: target.map(str::to_string),
                second_target: None,
                value: None,
                second_value: None,
                bonus_type: None,
                description: None,
            },
            Some(stat_name),
        )
    }

    pub fn set_tier_prose_name(&self, set_name: &str, equipped_count: i64, description: &str) -> Result<String> {
        let normalized_description = description.split_whitespace().collect::<Vec<_>>().join(" ");
        if let Some(name) = self.vocabulary.names.get(&normalized_description) {
            return Ok(name.clone());
        }
        let first_sentence =
            normalized_description.split_once('.').map_or(normalized_description.as_str(), |(sentence, _)| sentence);
        let name = first_sentence.split_whitespace().collect::<Vec<_>>().join(" ");
        if name.chars().count() > 60 {
            bail!("set {set_name:?} tier {equipped_count} prose needs a short [names] entry for {description:?}");
        }
        if name.is_empty() || name.contains(" — ") {
            bail!("set {set_name:?} tier {equipped_count} prose needs a valid [names] entry for {description:?}");
        }
        Ok(name)
    }

    pub fn family_template(&self, buff: &Buff) -> String {
        if let Some(definition) = self.definitions_by_buff_kind.get(buff.kind.trim()) {
            if let Some((effect_index, name)) =
                definition.effects.iter().enumerate().find_map(|(index, effect)| {
                    self.vocabulary.qualified_targeted_name(effect).map(|name| (index, name))
                })
            {
                let (stat_name, qualifier) = name.split_once(" (").expect("qualified name has targets");
                let slot = if effect_index > 0 && definition.display_text.contains("%v2") { "%v2" } else { "%v1" };
                let text = format!("{stat_name} +{slot}% ({qualifier}");
                return split_display_title(&definition.display_text)
                    .map_or(text.clone(), |(_, description)| format!("{text}: {description}"));
            }
        }
        self.description_template(buff.kind.trim())
            .replace("%i1", buff.target.as_deref().or(buff.description.as_deref()).unwrap_or(""))
            .replace("%i2", buff.second_target.as_deref().unwrap_or(""))
    }

    pub fn description(&self, template: &str, buff: &Buff) -> String {
        let bonus_type_name = buff
            .bonus_type
            .as_deref()
            .map(|upstream_name| match self.vocabulary.family_bonus_type(upstream_name) {
                Ok(Some(bonus_type)) => bonus_type.name().to_string(),
                _ => upstream_name.trim().to_string(),
            })
            .unwrap_or_default();
        template
            .replace("%v1", &buff.value.map(|value| value.to_string()).unwrap_or_default())
            .replace("%v2", &buff.second_value.map(|value| value.to_string()).unwrap_or_default())
            .replace("%i1", buff.target.as_deref().or(buff.description.as_deref()).unwrap_or(""))
            .replace("%i2", buff.second_target.as_deref().unwrap_or(""))
            .replace("%b1", &bonus_type_name)
            .trim()
            .to_string()
    }
}

pub(crate) fn split_display_title(display_text: &str) -> Option<(&str, &str)> {
    let mut parentheses = 0;
    for (index, character) in display_text.char_indices() {
        match character {
            '(' => parentheses += 1,
            ')' => parentheses = (parentheses - 1).max(0),
            ':' if parentheses == 0 => return Some((&display_text[..index], &display_text[index + 1..])),
            _ => {}
        }
    }
    None
}

fn title_without_display_tokens(title: &str) -> String {
    let mut characters = title.chars().peekable();
    let mut visible = String::new();
    while let Some(character) = characters.next() {
        if character == '%' && matches!(characters.peek(), Some('v' | 'b' | 'i')) {
            characters.next();
            while characters.peek().is_some_and(char::is_ascii_digit) {
                characters.next();
            }
            if characters.peek() == Some(&'%') {
                characters.next();
            }
        } else {
            visible.push(character);
        }
    }
    let normalized = visible.split_whitespace().collect::<Vec<_>>().join(" ");
    let title = normalized.trim().trim_end_matches(':').trim();
    let leading_number = title.split_whitespace().next().is_some_and(|word| {
        let unsigned = word.trim_start_matches(['+', '-']);
        !unsigned.is_empty() && unsigned.chars().all(|character| character.is_ascii_digit())
    });
    if title.starts_with("On ") || title.starts_with("Set Bonus") || leading_number {
        String::new()
    } else {
        title.to_string()
    }
}

fn words_from_buff_kind(buff_kind: &str) -> String {
    let mut words = String::new();
    let mut previous: Option<char> = None;
    for character in buff_kind.chars() {
        if let Some(previous_character) = previous {
            if (previous_character.is_lowercase() && character.is_uppercase())
                || (previous_character.is_alphabetic() && character.is_ascii_digit())
                || (previous_character.is_ascii_digit() && character.is_alphabetic())
            {
                words.push(' ');
            }
        }
        words.push(character);
        previous = Some(character);
    }
    let split_words = words.split_whitespace().collect::<Vec<_>>();
    split_words
        .iter()
        .enumerate()
        .filter_map(|(index, word)| {
            if matches!(*word, "Number" | "Numeral")
                || (*word == "Roman" && split_words.get(index + 1) == Some(&"Numeral"))
            {
                None
            } else {
                Some(*word)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
