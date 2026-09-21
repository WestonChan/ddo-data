//! `Quests.xml`: every quest and adventure zone with its patron, pack, favor and levels.

use super::Empty;
use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

pub fn parse(path: &Path) -> Result<Vec<Quest>> {
    let file: QuestFile = super::read_xml(path)?;
    file.quests.into_iter().map(Quest::try_from).collect()
}

#[derive(Debug, Deserialize)]
#[serde(rename = "Quests")]
struct QuestFile {
    #[serde(rename = "Quest", default)]
    quests: Vec<RawQuest>,
}

#[derive(Debug, Deserialize)]
struct RawQuest {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Patron")]
    patron: Option<String>,
    #[serde(rename = "AdventurePack")]
    adventure_pack: Option<String>,
    #[serde(rename = "Favor")]
    favor: Option<i64>,
    #[serde(rename = "Levels")]
    levels: Option<Levels>,
    #[serde(rename = "IsRaid")]
    is_raid: Option<Empty>,
    #[serde(rename = "DoNotShow")]
    do_not_show: Option<Empty>,
}

/// `<Levels size="2">6 21</Levels>`: heroic level, then epic level if the quest has one.
#[derive(Debug, Deserialize)]
struct Levels {
    #[serde(rename = "$text", default)]
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quest {
    pub name: String,
    pub patron: Option<String>,
    pub adventure_pack: Option<String>,
    pub favor: Option<i64>,
    pub levels: Vec<i64>,
    pub is_raid: bool,
    /// Adventure zones and other entries upstream hides from its own quest list.
    pub do_not_show: bool,
}

impl TryFrom<RawQuest> for Quest {
    type Error = anyhow::Error;

    fn try_from(raw: RawQuest) -> Result<Self> {
        let levels = raw
            .levels
            .map(|l| {
                l.text
                    .split_whitespace()
                    .map(|n| n.parse::<i64>().map_err(|e| anyhow::anyhow!("quest {}: level {n:?}: {e}", raw.name)))
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();
        Ok(Quest {
            name: raw.name.trim().to_string(),
            patron: raw.patron.filter(|p| p != "None"),
            adventure_pack: raw.adventure_pack,
            favor: raw.favor,
            levels,
            is_raid: raw.is_raid.is_some(),
            do_not_show: raw.do_not_show.is_some(),
        })
    }
}
