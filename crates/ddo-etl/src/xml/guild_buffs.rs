use super::effect::Effect;
use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

pub fn parse(path: &Path) -> Result<Vec<GuildBuff>> {
    let file: GuildBuffFile = super::parse_xml_file(path)?;
    Ok(file.buffs.into_iter().map(GuildBuff::from).collect())
}

#[derive(Deserialize)]
struct GuildBuffFile {
    #[serde(rename = "GuildBuff", default)]
    buffs: Vec<RawGuildBuff>,
}

#[derive(Deserialize)]
struct RawGuildBuff {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Description")]
    description: Option<String>,
    #[serde(rename = "Level")]
    guild_level: Option<i64>,
    #[serde(rename = "Effect", default)]
    effects: Vec<Effect>,
}

#[derive(Debug, Clone)]
pub struct GuildBuff {
    pub name: String,
    pub description: Option<String>,
    pub guild_level: Option<i64>,
    pub effects: Vec<Effect>,
}

impl From<RawGuildBuff> for GuildBuff {
    fn from(raw_buff: RawGuildBuff) -> Self {
        GuildBuff {
            name: raw_buff.name.trim().to_string(),
            description: raw_buff.description,
            guild_level: raw_buff.guild_level,
            effects: raw_buff.effects,
        }
    }
}
