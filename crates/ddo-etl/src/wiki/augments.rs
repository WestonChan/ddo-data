use super::bonuses::{validate_bonuses, WikiBonus};
use anyhow::{bail, Result};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiAugment {
    pub name: String,
    pub page: String,
    pub read: String,
    pub family: String,
    pub min_level: i64,
    pub slots: Vec<String>,
    pub description: String,
    pub effect_description: Option<String>,
    pub set: Option<String>,
    #[serde(default)]
    pub bonuses: Vec<WikiBonus>,
    #[serde(skip)]
    pub file_name: String,
}

impl WikiAugment {
    pub(super) fn validate(&self) -> Result<()> {
        if self.slots.is_empty() {
            bail!("slots is empty; list the socket labels the augment fits, as /v1/augment-slot-types lists them");
        }
        validate_bonuses(&self.bonuses)
    }
}
