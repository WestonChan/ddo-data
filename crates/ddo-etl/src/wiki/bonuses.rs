use anyhow::{bail, Result};
use ddo_model::enums::BonusType;
use ddo_model::stats::Stat;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiBonus {
    pub stat: Option<String>,
    pub group: Option<String>,
    pub effect: Option<String>,
    pub bonus_type: String,
    pub value: i64,
    pub value2: Option<i64>,
}

impl WikiBonus {
    pub fn name(&self) -> &str {
        self.stat.as_deref().or(self.group.as_deref()).or(self.effect.as_deref()).expect("validated bonus target")
    }

    pub fn bonus_type(&self) -> BonusType {
        BonusType::parse(&self.bonus_type).expect("validated bonus_type")
    }
}

pub(super) fn validate_bonuses(bonuses: &[WikiBonus]) -> Result<()> {
    for bonus in bonuses {
        if [bonus.stat.is_some(), bonus.group.is_some(), bonus.effect.is_some()]
            .into_iter()
            .filter(|present| *present)
            .count()
            != 1
        {
            bail!("wiki bonus needs exactly one stat, group or effect");
        }
        if bonus.stat.as_deref().is_some_and(|name| Stat::by_name(name).is_none()) {
            bail!("bonus stat {:?} is not a stat /v1/effects lists; use its exact name", bonus.stat);
        }
        if bonus.group.as_deref().is_some_and(|name| crate::map::effect_map::EFFECT_MAP.group_members(name).is_none()) {
            bail!("bonus group {:?} is not declared in effect_map.toml", bonus.group);
        }
        if bonus
            .effect
            .as_deref()
            .is_some_and(|name| !crate::map::effect_map::EFFECT_MAP.is_linkable_named_effect(name))
        {
            bail!("bonus effect {:?} is not a declared named effect", bonus.effect);
        }
        if BonusType::parse(&bonus.bonus_type).is_none() {
            bail!(
                "bonus {:?}: bonus_type {:?} is not a bonus type /v1/bonus-types lists; use its exact name",
                bonus.name(),
                bonus.bonus_type
            );
        }
    }
    Ok(())
}
