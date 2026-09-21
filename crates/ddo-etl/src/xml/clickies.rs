//! `ItemClickies.xml`: the spell-like effects items can carry, keyed by name from the item-level
//! `<Effect><Type>ItemClickie</Type><Item>name</Item></Effect>`.

use super::effect::Effect;
use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

pub fn parse(path: &Path) -> Result<Vec<Clickie>> {
    let file: ClickieFile = super::read_xml(path)?;
    Ok(file.clickies)
}

#[derive(Deserialize)]
struct ClickieFile {
    #[serde(rename = "Spell", default)]
    clickies: Vec<Clickie>,
}

#[derive(Debug, Deserialize)]
pub struct Clickie {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Description")]
    pub description: Option<String>,
    #[serde(rename = "Icon")]
    pub icon: Option<String>,
    #[serde(rename = "School")]
    pub school: Option<String>,
    #[serde(rename = "Effect", default)]
    pub effects: Vec<Effect>,
}
