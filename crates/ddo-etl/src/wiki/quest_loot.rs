use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestLoot {
    pub name: String,
    pub page: String,
    pub read: String,
    #[serde(default)]
    pub rare: Vec<String>,
}
