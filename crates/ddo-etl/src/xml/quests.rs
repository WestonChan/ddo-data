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
    #[serde(rename = "EpicName")]
    epic_name: Option<String>,
    #[serde(rename = "Casual")]
    casual: Option<Empty>,
    #[serde(rename = "Normal")]
    normal: Option<Empty>,
    #[serde(rename = "Hard")]
    hard: Option<Empty>,
    #[serde(rename = "Elite")]
    elite: Option<Empty>,
    #[serde(rename = "Reaper")]
    reaper: Option<Empty>,
    #[serde(rename = "Solo")]
    solo: Option<Empty>,
}

impl RawQuest {
    fn difficulties(&self) -> Vec<Difficulty> {
        [
            (Difficulty::Casual, self.casual),
            (Difficulty::Normal, self.normal),
            (Difficulty::Hard, self.hard),
            (Difficulty::Elite, self.elite),
            (Difficulty::Reaper, self.reaper),
            (Difficulty::Solo, self.solo),
        ]
        .into_iter()
        .filter_map(|(difficulty, flag)| flag.map(|_| difficulty))
        .collect()
    }
}

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
    pub do_not_show: bool,
    pub epic_name: Option<String>,
    pub difficulties: Vec<Difficulty>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Difficulty {
    Casual,
    Normal,
    Hard,
    Elite,
    Reaper,
    Solo,
}

impl Difficulty {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Casual => "casual",
            Self::Normal => "normal",
            Self::Hard => "hard",
            Self::Elite => "elite",
            Self::Reaper => "reaper",
            Self::Solo => "solo",
        }
    }
}

impl TryFrom<RawQuest> for Quest {
    type Error = anyhow::Error;

    fn try_from(raw: RawQuest) -> Result<Self> {
        let difficulties = raw.difficulties();
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
            epic_name: raw.epic_name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()),
            difficulties,
        })
    }
}
