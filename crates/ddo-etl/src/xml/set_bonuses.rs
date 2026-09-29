use super::effect::Effect;
use super::EmptyElement;
use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Default)]
pub struct SetBonusFile {
    pub set_bonuses: Vec<SetBonus>,
    pub filigrees: Vec<Filigree>,
}

pub fn parse_set_bonus_file(path: &Path) -> Result<SetBonusFile> {
    let raw_file: RawSetBonusFile = super::parse_xml_file(path)?;
    let mut set_bonus_file = SetBonusFile::default();
    for child in raw_file.children {
        match child {
            SetBonusFileChild::SetBonus(set_bonus) => set_bonus_file.set_bonuses.push(set_bonus),
            SetBonusFileChild::Filigree(filigree) => set_bonus_file.filigrees.push(filigree),
        }
    }
    Ok(set_bonus_file)
}

#[derive(Deserialize)]
struct RawSetBonusFile {
    #[serde(rename = "$value", default)]
    children: Vec<SetBonusFileChild>,
}

#[derive(Deserialize)]
enum SetBonusFileChild {
    SetBonus(SetBonus),
    Filigree(Filigree),
}

#[derive(Debug, Deserialize)]
pub struct SetBonus {
    #[serde(rename = "Type")]
    pub name: String,
    #[serde(rename = "Icon")]
    pub icon: Option<String>,
    #[serde(rename = "IgnoreForParse")]
    pub ignore_for_parse: Option<EmptyElement>,
    #[serde(rename = "AdditionalDescription")]
    pub additional_description: Option<String>,
    #[serde(rename = "Buff", default)]
    pub tiers: Vec<SetBonusTier>,
}

#[derive(Debug, Deserialize)]
pub struct SetBonusTier {
    #[serde(rename = "EquippedCount")]
    pub equipped_count: i64,
    #[serde(rename = "Description")]
    pub description: Option<String>,
    #[serde(rename = "Effect", default)]
    pub effects: Vec<Effect>,
}

#[derive(Debug, Deserialize)]
pub struct Filigree {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Description")]
    pub description: Option<String>,
    #[serde(rename = "Icon")]
    pub icon: Option<String>,
    #[serde(rename = "Menu")]
    pub menu: Option<String>,
    #[serde(rename = "SetBonus", default)]
    pub set_bonus_names: Vec<String>,
    #[serde(rename = "Effect", default)]
    pub effects: Vec<Effect>,
}
