use ddo_model::enums::LootType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestLoot {
    pub name: String,
    pub rare: Vec<RareDrop>,
    pub rare_augments: Vec<RareDrop>,
    pub items: Vec<ListedDrop>,
    pub augments: Vec<ListedDrop>,
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RareDrop {
    Named(String),
    InChest(RareDropInChest),
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListedDrop {
    Named(String),
    Described(DescribedListedDrop),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescribedListedDrop {
    pub name: String,
    pub loot_type: Option<String>,
    pub chest: Option<String>,
    pub replaces_pack_drop: bool,
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

    pub fn replaces_pack_drop(&self) -> bool {
        matches!(self, Self::Described(drop) if drop.replaces_pack_drop)
    }

    fn loot_type_name(&self) -> Option<&str> {
        match self {
            Self::Named(_) => None,
            Self::Described(drop) => drop.loot_type.as_deref(),
        }
    }
}
