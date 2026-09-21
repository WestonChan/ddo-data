//! `<Buff>` → the item's enhancement bonus, a typed stat bonus, or a named effect.

use super::bonus_type::normalize;
use super::{MappingData, MAPPING};
use crate::xml::items::Buff;
use anyhow::{bail, Result};
use ddo_model::enums::BonusType;
use ddo_model::stats::Stat;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// `items.enhancement_bonus`.
    Enhancement(i64),
    /// A `bonuses` row.
    Bonus { stat: &'static Stat, bonus_type: Option<BonusType>, value: Option<i64>, value2: Option<i64> },
    /// An `effects` row.
    Effect { name: String, value: Option<i64>, target: Option<String> },
}

pub struct BuffMap {
    data: &'static MappingData,
}

impl BuffMap {
    pub fn load() -> Result<Self> {
        let data: &'static MappingData = &MAPPING;
        for (kind, stat) in &data.fixed {
            if ddo_model::stat_by_name(stat).is_none() {
                bail!("[fixed] {kind} names unknown stat {stat:?}");
            }
        }
        Ok(Self { data })
    }

    pub fn resolve(&self, buff: &Buff) -> Result<Resolved> {
        let kind = buff.kind.trim();
        if self.data.enhancement.iter().any(|k| k == kind) {
            return match buff.value1 {
                Some(v) => Ok(Resolved::Enhancement(v)),
                None => bail!("{kind} without Value1"),
            };
        }
        let stat_name = if let Some(fixed) = self.data.fixed.get(kind) {
            Some(fixed.clone())
        } else if let Some(template) = self.data.by_item.get(kind) {
            let Some(item) = buff.item.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
                bail!("{kind} needs an <Item> sub-target to name its stat");
            };
            let item = self.data.item_aliases.get(item).map(String::as_str).unwrap_or(item);
            Some(template.replace("{item}", item))
        } else {
            None
        };
        let Some(stat_name) = stat_name else {
            return Ok(Resolved::Effect {
                name: kind.to_string(),
                value: buff.value1,
                target: buff.item.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
            });
        };
        let Some(stat) = ddo_model::stat_by_name(&stat_name) else {
            bail!("{kind} with Item {:?} resolves to {stat_name:?}, which is not a stat; extend [by_item], [fixed] or the stats seed", buff.item);
        };
        let bonus_type = normalize(buff.bonus_type.as_deref().unwrap_or(""))?;
        Ok(Resolved::Bonus { stat, bonus_type, value: buff.value1, value2: buff.value2 })
    }

    /// Fill an `ItemBuffs.xml` template with this buff's values. Unknown bonus types render as
    /// their raw text rather than failing, since this is display only.
    pub fn describe(&self, template: &str, buff: &Buff) -> String {
        let bonus_type = buff
            .bonus_type
            .as_deref()
            .map(|raw| match normalize(raw) {
                Ok(Some(b)) => b.name().to_string(),
                _ => raw.trim().to_string(),
            })
            .unwrap_or_default();
        template
            .replace("%v1", &buff.value1.map(|v| v.to_string()).unwrap_or_default())
            .replace("%v2", &buff.value2.map(|v| v.to_string()).unwrap_or_default())
            .replace("%i1", buff.item.as_deref().unwrap_or(""))
            .replace("%i2", buff.item2.as_deref().unwrap_or(""))
            .replace("%b1", &bonus_type)
            .trim()
            .to_string()
    }
}
