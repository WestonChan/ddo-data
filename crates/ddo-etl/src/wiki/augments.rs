use super::bonuses::WikiBonus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiAugment {
    pub name: String,
    pub family: String,
    pub min_level: i64,
    pub slots: Vec<String>,
    pub description: String,
    pub effect_description: Option<String>,
    pub set: Option<String>,
    pub bonuses: Vec<WikiBonus>,
    pub file_name: String,
}
