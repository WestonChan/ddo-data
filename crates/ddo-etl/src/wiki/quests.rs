use crate::xml::quests::Difficulty;
use anyhow::{bail, Result};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiQuest {
    pub name: String,
    pub page: String,
    pub read: String,
    pub free_to_play: bool,
    pub legendary_level: Option<i64>,
    pub zone: Option<String>,
    pub bestowed_by: Option<String>,
    pub flagging: Option<String>,
    pub pack: Option<String>,
    pub patron: Option<String>,
    pub level: Option<i64>,
    pub epic_level: Option<i64>,
    pub favor: Option<i64>,
    pub is_raid: Option<bool>,
    pub difficulties: Option<Vec<String>>,
    #[serde(skip)]
    pub file_name: String,
}

impl WikiQuest {
    pub fn carries_quest_fields(&self) -> bool {
        self.pack.is_some()
            || self.patron.is_some()
            || self.level.is_some()
            || self.epic_level.is_some()
            || self.favor.is_some()
            || self.is_raid.is_some()
            || self.difficulties.is_some()
    }

    pub(super) fn validate(&self) -> Result<()> {
        if !self.carries_quest_fields() {
            return Ok(());
        }
        let required_quest_fields = [
            ("pack", self.pack.is_some()),
            ("level", self.level.is_some()),
            ("favor", self.favor.is_some()),
            ("is_raid", self.is_raid.is_some()),
            ("difficulties", self.difficulties.is_some()),
        ];
        let missing_fields: Vec<&str> =
            required_quest_fields.iter().filter(|(_, is_present)| !is_present).map(|(field, _)| *field).collect();
        if !missing_fields.is_empty() {
            bail!(
                "a quest created from the wiki needs pack, level, favor, is_raid and difficulties (patron and epic_level are optional); missing {}",
                missing_fields.join(", ")
            );
        }
        for difficulty in self.difficulties.iter().flatten() {
            if Difficulty::ALL.iter().all(|d| d.as_str() != difficulty) {
                let difficulty_names: Vec<&str> = Difficulty::ALL.iter().map(|d| d.as_str()).collect();
                bail!("difficulty {difficulty:?} must be one of {}", difficulty_names.join(", "));
            }
        }
        Ok(())
    }
}
