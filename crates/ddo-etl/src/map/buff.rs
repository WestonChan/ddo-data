use super::enchantment::{EnchantmentMap, ENCHANTMENT_MAP};
use crate::xml::item_buffs::ItemBuffDefinition;
use crate::xml::items::Buff;
use anyhow::{bail, Result};
use ddo_model::enums::BonusType;
use ddo_model::stats::Stat;
use std::collections::{BTreeMap, BTreeSet, HashMap};

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
    vocabulary: &'static EnchantmentMap,
    definitions_by_buff_kind: HashMap<String, ItemBuffDefinition>,
}

impl BuffResolver {
    pub fn from_definitions(item_buff_definitions: &HashMap<String, ItemBuffDefinition>) -> Self {
        let vocabulary: &'static EnchantmentMap = &ENCHANTMENT_MAP;
        Self { vocabulary, definitions_by_buff_kind: item_buff_definitions.clone() }
    }

    pub fn family_resolutions(&self) -> Result<BTreeMap<String, FamilyResolution>> {
        let mut family_names: BTreeSet<&str> = self.definitions_by_buff_kind.keys().map(String::as_str).collect();
        family_names.extend(self.vocabulary.family.enhancement.iter().map(String::as_str));
        family_names.extend(self.vocabulary.family.fixed.keys().map(String::as_str));
        family_names.extend(self.vocabulary.family.by_item.keys().map(String::as_str));
        family_names
            .into_iter()
            .map(|family_name| Ok((family_name.to_string(), self.family_resolution(family_name)?)))
            .collect()
    }

    pub fn family_resolution(&self, buff_kind: &str) -> Result<FamilyResolution> {
        if self.vocabulary.family.enhancement.iter().any(|kind| kind == buff_kind) {
            return Ok(FamilyResolution::Enhancement);
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
        let mapped_stats: Option<Vec<_>> =
            definition.effects.iter().map(|effect| self.vocabulary.stats_for_plain_effect(effect)).collect();
        let Some(mapped_stats) = mapped_stats else {
            return Ok(FamilyResolution::Effect);
        };
        let has_first_value = definition.display_text.contains("%v1");
        let has_second_value = definition.display_text.contains("%v2");
        let mut stats = Vec::new();
        for (effect_index, (effect, effect_stats)) in definition.effects.iter().zip(mapped_stats).enumerate() {
            let definition_amount = effect.plain_integer_amount().expect("checked by stats_for_plain_effect");
            let amount_from = if has_second_value && (effect_index > 0 || !has_first_value) {
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
        Ok(FamilyResolution::EffectFallback(stats))
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
                if resolved_stat.amount(buff).is_none() {
                    return Ok(ResolvedBuff::Effect {
                        name: buff_kind.to_string(),
                        value: buff.value,
                        target: target.map(str::to_string),
                    });
                }
                Ok(ResolvedBuff::Bonuses { source: BuffResolutionSource::Family, stats: vec![resolved_stat] })
            }
            FamilyResolution::EffectFallback(mut stats) => {
                if stats.iter().any(|stat| stat.amount(buff).is_none()) {
                    return Ok(ResolvedBuff::Effect {
                        name: buff_kind.to_string(),
                        value: buff.value,
                        target: target.map(str::to_string),
                    });
                }
                let item_bonus_type = self.vocabulary.family_bonus_type(buff.bonus_type.as_deref().unwrap_or(""))?;
                for stat in &mut stats {
                    stat.bonus_type = self.resolved_bonus_type(buff_kind, item_bonus_type, stat.bonus_type)?;
                }
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
        if self.definition_uses_item_bonus_type(buff_kind) {
            Ok(item_bonus_type.or(effect_bonus_type).or(definition_bonus_type))
        } else {
            Ok(effect_bonus_type.or(definition_bonus_type).or(item_bonus_type))
        }
    }

    pub fn description_template(&self, buff_kind: &str) -> &str {
        self.definitions_by_buff_kind.get(buff_kind).map(|definition| definition.display_text.as_str()).unwrap_or("")
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
            .replace("%i1", buff.target.as_deref().unwrap_or(""))
            .replace("%i2", buff.second_target.as_deref().unwrap_or(""))
            .replace("%b1", &bonus_type_name)
            .trim()
            .to_string()
    }
}
