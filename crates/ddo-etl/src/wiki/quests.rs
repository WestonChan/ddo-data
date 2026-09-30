use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestFacts {
    pub name: String,
    pub page: String,
    pub read: String,
    pub free_to_play: bool,
    pub legendary_level: Option<i64>,
    pub zone: Option<String>,
    pub bestowed_by: Option<String>,
    pub flagging: Option<String>,
}
