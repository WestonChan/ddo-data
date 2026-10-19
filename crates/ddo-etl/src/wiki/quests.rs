#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiQuest {
    pub name: String,
    pub pack: Option<String>,
    pub patron: Option<String>,
    pub level: Option<i64>,
    pub epic_level: Option<i64>,
    pub favor: Option<i64>,
    pub is_raid: Option<bool>,
    pub difficulties: Option<Vec<String>>,
    pub file_name: String,
}
