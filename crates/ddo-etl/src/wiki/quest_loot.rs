use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestLoot {
    pub name: String,
    pub page: String,
    pub read: String,
    #[serde(default)]
    pub rare: Vec<RareDrop>,
    #[serde(default)]
    pub rare_augments: Vec<RareDrop>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum RareDrop {
    Named(String),
    InChest(RareDropInChest),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RareDropInChest {
    pub name: String,
    pub chest: String,
}

impl RareDrop {
    pub fn name(&self) -> &str {
        match self {
            Self::Named(name) => name,
            Self::InChest(drop) => &drop.name,
        }
    }

    pub fn chest(&self) -> Option<&str> {
        match self {
            Self::Named(_) => None,
            Self::InChest(drop) => Some(&drop.chest),
        }
    }
}
