use super::bonus_type::parse_buff_bonus_type;
use super::{BuffVocabulary, BUFF_VOCABULARY};
use crate::xml::items::Buff;
use anyhow::{bail, Result};
use ddo_model::enums::BonusType;
use ddo_model::stats::Stat;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedBuff {
    EnhancementBonus(i64),
    Bonus { stat: &'static Stat, bonus_type: Option<BonusType>, value: Option<i64>, second_value: Option<i64> },
    Effect { name: String, value: Option<i64>, target: Option<String> },
}

pub struct BuffMap {
    vocabulary: &'static BuffVocabulary,
}

impl BuffMap {
    pub fn load() -> Result<Self> {
        let vocabulary: &'static BuffVocabulary = &BUFF_VOCABULARY;
        for (buff_kind, stat_name) in &vocabulary.fixed {
            if Stat::by_name(stat_name).is_none() {
                bail!("[fixed] {buff_kind} names unknown stat {stat_name:?}");
            }
        }
        Ok(Self { vocabulary })
    }

    pub fn resolved(&self, buff: &Buff) -> Result<ResolvedBuff> {
        let buff_kind = buff.kind.trim();
        if self.vocabulary.enhancement.iter().any(|k| k == buff_kind) {
            return match buff.value {
                Some(value) => Ok(ResolvedBuff::EnhancementBonus(value)),
                None => bail!("{buff_kind} without Value1"),
            };
        }
        let stat_name = if let Some(fixed_stat_name) = self.vocabulary.fixed.get(buff_kind) {
            Some(fixed_stat_name.clone())
        } else if let Some(stat_template) = self.vocabulary.by_item.get(buff_kind) {
            let Some(target) = buff.target.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
                bail!("{buff_kind} needs an <Item> sub-target to name its stat");
            };
            let target = self.vocabulary.item_aliases.get(target).map(String::as_str).unwrap_or(target);
            Some(stat_template.replace("{item}", target))
        } else {
            None
        };
        let Some(stat_name) = stat_name else {
            return Ok(ResolvedBuff::Effect {
                name: buff_kind.to_string(),
                value: buff.value,
                target: buff.target.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
            });
        };
        let Some(stat) = Stat::by_name(&stat_name) else {
            bail!("{buff_kind} with Item {:?} resolves to {stat_name:?}, which is not a stat; extend [by_item], [fixed] or the stats seed", buff.target);
        };
        let bonus_type = parse_buff_bonus_type(buff.bonus_type.as_deref().unwrap_or(""))?;
        Ok(ResolvedBuff::Bonus { stat, bonus_type, value: buff.value, second_value: buff.second_value })
    }

    pub fn description(&self, template: &str, buff: &Buff) -> String {
        let bonus_type_name = buff
            .bonus_type
            .as_deref()
            .map(|upstream_name| match parse_buff_bonus_type(upstream_name) {
                Ok(Some(bonus_type)) => bonus_type.name().to_string(),
                _ => upstream_name.trim().to_string(),
            })
            .unwrap_or_default();
        template
            .replace("%v1", &buff.value.map(|v| v.to_string()).unwrap_or_default())
            .replace("%v2", &buff.second_value.map(|v| v.to_string()).unwrap_or_default())
            .replace("%i1", buff.target.as_deref().unwrap_or(""))
            .replace("%i2", buff.second_target.as_deref().unwrap_or(""))
            .replace("%b1", &bonus_type_name)
            .trim()
            .to_string()
    }
}
