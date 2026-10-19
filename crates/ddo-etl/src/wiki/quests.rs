#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiQuest {
    pub name: String,
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
}
