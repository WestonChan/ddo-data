use anyhow::{bail, Result};
use ddo_model::enums::LootType;
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
    #[serde(default)]
    pub items: Vec<ListedDrop>,
    #[serde(default)]
    pub augments: Vec<ListedDrop>,
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum ListedDrop {
    Named(String),
    Described(DescribedListedDrop),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DescribedListedDrop {
    pub name: String,
    pub loot_type: Option<String>,
    pub chest: Option<String>,
}

impl ListedDrop {
    pub fn name(&self) -> &str {
        match self {
            Self::Named(name) => name,
            Self::Described(drop) => &drop.name,
        }
    }

    pub fn loot_type(&self) -> LootType {
        self.loot_type_name().map_or(LootType::Chest, |name| {
            LootType::ALL.iter().copied().find(|l| l.as_str() == name).expect("validated loot_type")
        })
    }

    pub fn chest(&self) -> Option<&str> {
        match self {
            Self::Named(_) => None,
            Self::Described(drop) => drop.chest.as_deref(),
        }
    }

    pub fn names_loot_type(&self) -> bool {
        self.loot_type_name().is_some()
    }

    fn loot_type_name(&self) -> Option<&str> {
        match self {
            Self::Named(_) => None,
            Self::Described(drop) => drop.loot_type.as_deref(),
        }
    }
}

impl QuestLoot {
    pub(super) fn validate(&self) -> Result<()> {
        for listed_drop in self.items.iter().chain(&self.augments) {
            if let Some(loot_type_name) = listed_drop.loot_type_name() {
                if LootType::ALL.iter().all(|l| l.as_str() != loot_type_name) {
                    let loot_type_names: Vec<&str> = LootType::ALL.iter().map(|l| l.as_str()).collect();
                    bail!(
                        "listed drop {:?}: loot_type {loot_type_name:?} must be one of {}",
                        listed_drop.name(),
                        loot_type_names.join(", ")
                    );
                }
            }
            if listed_drop.loot_type() == LootType::Reward && listed_drop.chest().is_some() {
                bail!(
                    "listed drop {:?}: a reward comes from the quest's end, not from a chest; remove its chest",
                    listed_drop.name()
                );
            }
        }
        Ok(())
    }
}
