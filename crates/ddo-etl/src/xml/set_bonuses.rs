//! `SetBonuses.xml` (gear sets) and `FiligreeSets/*.xml` (a `<SetBonus>` plus its `<Filigree>`
//! children). Both roots hold the same `<SetBonus>` shape, so one parser reads either.

use super::effect::Effect;
use super::Empty;
use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Default)]
pub struct SetFile {
    pub sets: Vec<SetBonus>,
    pub filigrees: Vec<Filigree>,
}

pub fn parse_set_file(path: &Path) -> Result<SetFile> {
    let raw: RawSetFile = super::read_xml(path)?;
    let mut out = SetFile::default();
    for entry in raw.entries {
        match entry {
            SetEntry::SetBonus(s) => out.sets.push(s),
            SetEntry::Filigree(f) => out.filigrees.push(f),
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
struct RawSetFile {
    #[serde(rename = "$value", default)]
    entries: Vec<SetEntry>,
}

#[derive(Deserialize)]
enum SetEntry {
    SetBonus(SetBonus),
    Filigree(Filigree),
}

#[derive(Debug, Deserialize)]
pub struct SetBonus {
    /// Upstream calls the set's name `Type`.
    #[serde(rename = "Type")]
    pub name: String,
    #[serde(rename = "Icon")]
    pub icon: Option<String>,
    #[serde(rename = "IgnoreForParse")]
    pub ignore_for_parse: Option<Empty>,
    #[serde(rename = "AdditionalDescription")]
    pub additional_description: Option<String>,
    #[serde(rename = "Buff", default)]
    pub tiers: Vec<SetTier>,
}

#[derive(Debug, Deserialize)]
pub struct SetTier {
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
    pub set_bonus: Vec<String>,
    #[serde(rename = "Effect", default)]
    pub effects: Vec<Effect>,
}
