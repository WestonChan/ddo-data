use super::EmptyElement;
use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

pub fn parse(path: &Path) -> Result<Vec<Quest>> {
    let file: QuestFile = super::parse_xml_file(path)?;
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
    levels: Option<RawQuestLevels>,
    #[serde(rename = "IsRaid")]
    is_raid: Option<EmptyElement>,
    #[serde(rename = "DoNotShow")]
    do_not_show: Option<EmptyElement>,
    #[serde(rename = "EpicName")]
    epic_name: Option<String>,
    #[serde(rename = "Casual")]
    casual: Option<EmptyElement>,
    #[serde(rename = "Normal")]
    normal: Option<EmptyElement>,
    #[serde(rename = "Hard")]
    hard: Option<EmptyElement>,
    #[serde(rename = "Elite")]
    elite: Option<EmptyElement>,
    #[serde(rename = "Reaper")]
    reaper: Option<EmptyElement>,
    #[serde(rename = "Solo")]
    solo: Option<EmptyElement>,
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
struct RawQuestLevels {
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
    pub is_hidden: bool,
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
    pub const ALL: &'static [Difficulty] =
        &[Self::Casual, Self::Normal, Self::Hard, Self::Elite, Self::Reaper, Self::Solo];

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

    fn try_from(raw_quest: RawQuest) -> Result<Self> {
        let difficulties = raw_quest.difficulties();
        let levels = raw_quest
            .levels
            .map(|raw_levels| {
                raw_levels
                    .text
                    .split_whitespace()
                    .map(|n| {
                        n.parse::<i64>().map_err(|e| anyhow::anyhow!("quest {}: level {n:?}: {e}", raw_quest.name))
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();
        Ok(Quest {
            name: raw_quest.name.trim().to_string(),
            patron: raw_quest.patron.filter(|p| p != "None"),
            adventure_pack: raw_quest.adventure_pack,
            favor: raw_quest.favor,
            levels,
            is_raid: raw_quest.is_raid.is_some(),
            is_hidden: raw_quest.do_not_show.is_some(),
            epic_name: raw_quest.epic_name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()),
            difficulties,
        })
    }
}
