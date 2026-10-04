use anyhow::{bail, Result};
use ddo_model::enums::BonusType;
use ddo_model::stats::Stat;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiBonus {
    pub stat: String,
    pub bonus_type: String,
    pub value: i64,
    pub value2: Option<i64>,
}

impl WikiBonus {
    pub fn stat(&self) -> &'static Stat {
        Stat::by_name(&self.stat).expect("validated stat")
    }

    pub fn bonus_type(&self) -> BonusType {
        BonusType::parse(&self.bonus_type).expect("validated bonus_type")
    }
}

pub(super) fn validate_bonuses(bonuses: &[WikiBonus]) -> Result<()> {
    for bonus in bonuses {
        if Stat::by_name(&bonus.stat).is_none() {
            bail!("bonus stat {:?} is not a stat /v1/effects lists; use its exact name", bonus.stat);
        }
        if BonusType::parse(&bonus.bonus_type).is_none() {
            bail!(
                "bonus {:?}: bonus_type {:?} is not a bonus type /v1/bonus-types lists; use its exact name",
                bonus.stat,
                bonus.bonus_type
            );
        }
    }
    Ok(())
}
