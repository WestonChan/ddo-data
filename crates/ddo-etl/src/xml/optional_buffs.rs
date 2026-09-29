use super::effect::Effect;
use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

pub fn parse(path: &Path) -> Result<Vec<OptionalBuff>> {
    let file: OptionalBuffFile = super::parse_xml_file(path)?;
    Ok(file.buffs.into_iter().map(|b| OptionalBuff { name: b.name.trim().to_string(), ..b }).collect())
}

#[derive(Deserialize)]
struct OptionalBuffFile {
    #[serde(rename = "OptionalBuff", default)]
    buffs: Vec<OptionalBuff>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OptionalBuff {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Icon")]
    pub icon: Option<String>,
    #[serde(rename = "Description")]
    pub description: Option<String>,
    #[serde(rename = "Effect", default)]
    pub effects: Vec<Effect>,
}
