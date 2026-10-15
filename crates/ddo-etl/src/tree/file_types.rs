use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(crate) fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum StatCategoryFile {
    Ability,
    Defensive,
    Magical,
    Martial,
    Other,
    Skill,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SaveProgressionFile {
    Good,
    Poor,
    None,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub(crate) enum ItemCategoryFile {
    Armor,
    Shield,
    Weapon,
    Jewelry,
    Clothing,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum FileNumber {
    Integer(i64),
    Real(f64),
}
