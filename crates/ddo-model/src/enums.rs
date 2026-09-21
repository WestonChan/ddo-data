//! Closed vocabularies. Every string the schema constrains with a `CHECK` or a seed table has an
//! enum here, so a value the game data uses that we do not know about is a compile-time or
//! parse-time error rather than a silently accepted row.
//!
//! Ids are stable and referenced by consumers; append, never renumber.

use serde::{Deserialize, Serialize};

/// Which broad kind of number a stat is. Drives grouping in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StatCategory {
    Ability,
    Defensive,
    Martial,
    Magical,
    Skill,
    Other,
}

impl StatCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ability => "ability",
            Self::Defensive => "defensive",
            Self::Martial => "martial",
            Self::Magical => "magical",
            Self::Skill => "skill",
            Self::Other => "other",
        }
    }
}

/// The stacking class of a bonus. DDO's rule is that bonuses of the same type to the same stat do
/// not stack unless the type is one of the few that do (see `stacks_with_self`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BonusType {
    Enhancement,
    Competence,
    Insight,
    Sacred,
    Profane,
    Luck,
    Morale,
    Alchemical,
    Dodge,
    Armor,
    NaturalArmor,
    Deflection,
    Shield,
    Size,
    Racial,
    Resistance,
    Festive,
    Exceptional,
    Quality,
    Artifact,
    Inherent,
    Stacking,
    Rage,
    Primal,
    Determination,
    Implement,
    Music,
    Equipment,
    Orb,
    Vitality,
    FalseLife,
    Legendary,
    Penalty,
}

impl BonusType {
    pub const ALL: &'static [BonusType] = &[
        Self::Enhancement,
        Self::Competence,
        Self::Insight,
        Self::Sacred,
        Self::Profane,
        Self::Luck,
        Self::Morale,
        Self::Alchemical,
        Self::Dodge,
        Self::Armor,
        Self::NaturalArmor,
        Self::Deflection,
        Self::Shield,
        Self::Size,
        Self::Racial,
        Self::Resistance,
        Self::Festive,
        Self::Exceptional,
        Self::Quality,
        Self::Artifact,
        Self::Inherent,
        Self::Stacking,
        Self::Rage,
        Self::Primal,
        Self::Determination,
        Self::Implement,
        Self::Music,
        Self::Equipment,
        Self::Orb,
        Self::Vitality,
        Self::FalseLife,
        Self::Legendary,
        Self::Penalty,
    ];

    pub const fn id(self) -> i64 {
        match self {
            Self::Enhancement => 1,
            Self::Competence => 2,
            Self::Insight => 3,
            Self::Sacred => 4,
            Self::Profane => 5,
            Self::Luck => 6,
            Self::Morale => 7,
            Self::Alchemical => 8,
            Self::Dodge => 9,
            Self::Armor => 10,
            Self::NaturalArmor => 11,
            Self::Deflection => 12,
            Self::Shield => 13,
            Self::Size => 14,
            Self::Racial => 15,
            Self::Resistance => 16,
            Self::Festive => 17,
            Self::Exceptional => 18,
            Self::Quality => 19,
            Self::Artifact => 20,
            Self::Inherent => 21,
            Self::Stacking => 22,
            Self::Rage => 23,
            Self::Primal => 24,
            Self::Determination => 25,
            Self::Implement => 26,
            Self::Music => 27,
            Self::Equipment => 28,
            Self::Orb => 29,
            Self::Vitality => 30,
            Self::FalseLife => 31,
            Self::Legendary => 32,
            Self::Penalty => 33,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Enhancement => "Enhancement",
            Self::Competence => "Competence",
            Self::Insight => "Insight",
            Self::Sacred => "Sacred",
            Self::Profane => "Profane",
            Self::Luck => "Luck",
            Self::Morale => "Morale",
            Self::Alchemical => "Alchemical",
            Self::Dodge => "Dodge",
            Self::Armor => "Armor",
            Self::NaturalArmor => "Natural Armor",
            Self::Deflection => "Deflection",
            Self::Shield => "Shield",
            Self::Size => "Size",
            Self::Racial => "Racial",
            Self::Resistance => "Resistance",
            Self::Festive => "Festive",
            Self::Exceptional => "Exceptional",
            Self::Quality => "Quality",
            Self::Artifact => "Artifact",
            Self::Inherent => "Inherent",
            Self::Stacking => "Stacking",
            Self::Rage => "Rage",
            Self::Primal => "Primal",
            Self::Determination => "Determination",
            Self::Implement => "Implement",
            Self::Music => "Music",
            Self::Equipment => "Equipment",
            Self::Orb => "Orb",
            Self::Vitality => "Vitality",
            Self::FalseLife => "False Life",
            Self::Legendary => "Legendary",
            Self::Penalty => "Penalty",
        }
    }

    /// Whether two bonuses of this type to the same stat add together. Dodge, Stacking and
    /// Penalty do; everything else takes the highest.
    pub const fn stacks_with_self(self) -> bool {
        matches!(self, Self::Dodge | Self::Stacking | Self::Penalty)
    }

    /// Look a type up by its canonical name.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|b| b.name() == name)
    }
}

/// Where on the body an item is worn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SlotCategory {
    Weapon,
    Armor,
    Accessory,
}

impl SlotCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Weapon => "weapon",
            Self::Armor => "armor",
            Self::Accessory => "accessory",
        }
    }
}

/// The sixteen equipment slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EquipmentSlot {
    MainHand,
    OffHand,
    Ranged,
    Quiver,
    Head,
    Neck,
    Trinket,
    Back,
    Wrists,
    Hands,
    Body,
    Waist,
    Feet,
    Goggles,
    Ring,
    Runearm,
}

impl EquipmentSlot {
    pub const ALL: &'static [EquipmentSlot] = &[
        Self::MainHand,
        Self::OffHand,
        Self::Ranged,
        Self::Quiver,
        Self::Head,
        Self::Neck,
        Self::Trinket,
        Self::Back,
        Self::Wrists,
        Self::Hands,
        Self::Body,
        Self::Waist,
        Self::Feet,
        Self::Goggles,
        Self::Ring,
        Self::Runearm,
    ];

    /// Id doubles as sort order.
    pub const fn id(self) -> i64 {
        match self {
            Self::MainHand => 1,
            Self::OffHand => 2,
            Self::Ranged => 3,
            Self::Quiver => 4,
            Self::Head => 5,
            Self::Neck => 6,
            Self::Trinket => 7,
            Self::Back => 8,
            Self::Wrists => 9,
            Self::Hands => 10,
            Self::Body => 11,
            Self::Waist => 12,
            Self::Feet => 13,
            Self::Goggles => 14,
            Self::Ring => 15,
            Self::Runearm => 16,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::MainHand => "Main Hand",
            Self::OffHand => "Off Hand",
            Self::Ranged => "Ranged",
            Self::Quiver => "Quiver",
            Self::Head => "Head",
            Self::Neck => "Neck",
            Self::Trinket => "Trinket",
            Self::Back => "Back",
            Self::Wrists => "Wrists",
            Self::Hands => "Hands",
            Self::Body => "Body",
            Self::Waist => "Waist",
            Self::Feet => "Feet",
            Self::Goggles => "Goggles",
            Self::Ring => "Ring",
            Self::Runearm => "Runearm",
        }
    }

    pub const fn category(self) -> SlotCategory {
        match self {
            Self::MainHand | Self::OffHand | Self::Ranged | Self::Quiver | Self::Runearm => SlotCategory::Weapon,
            Self::Head | Self::Back | Self::Wrists | Self::Hands | Self::Body | Self::Waist | Self::Feet => {
                SlotCategory::Armor
            }
            Self::Neck | Self::Trinket | Self::Goggles | Self::Ring => SlotCategory::Accessory,
        }
    }
}

/// The coarse item kind the picker filters on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemCategory {
    Armor,
    Shield,
    Weapon,
    Jewelry,
    Clothing,
}

impl ItemCategory {
    pub const ALL: &'static [ItemCategory] = &[Self::Armor, Self::Shield, Self::Weapon, Self::Jewelry, Self::Clothing];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Armor => "Armor",
            Self::Shield => "Shield",
            Self::Weapon => "Weapon",
            Self::Jewelry => "Jewelry",
            Self::Clothing => "Clothing",
        }
    }
}

/// How a weapon is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Handedness {
    OneHanded,
    TwoHanded,
    OffHand,
    Thrown,
}

impl Handedness {
    pub const ALL: &'static [Handedness] = &[Self::OneHanded, Self::TwoHanded, Self::OffHand, Self::Thrown];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OneHanded => "One-handed",
            Self::TwoHanded => "Two-handed",
            Self::OffHand => "Off-hand",
            Self::Thrown => "Thrown",
        }
    }
}

/// Which feat line lets a character use a weapon without penalty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WeaponProficiency {
    Simple,
    Martial,
    Exotic,
}

impl WeaponProficiency {
    pub const ALL: &'static [WeaponProficiency] = &[Self::Simple, Self::Martial, Self::Exotic];

    pub const fn id(self) -> i64 {
        match self {
            Self::Simple => 1,
            Self::Martial => 2,
            Self::Exotic => 3,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Simple => "Simple",
            Self::Martial => "Martial",
            Self::Exotic => "Exotic",
        }
    }
}

/// The armor weight class, which decides arcane spell failure and max dex bonus rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArmorType {
    Cloth,
    Light,
    Medium,
    Heavy,
    Docent,
    Shield,
}

impl ArmorType {
    pub const ALL: &'static [ArmorType] =
        &[Self::Cloth, Self::Light, Self::Medium, Self::Heavy, Self::Docent, Self::Shield];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cloth => "Cloth",
            Self::Light => "Light",
            Self::Medium => "Medium",
            Self::Heavy => "Heavy",
            Self::Docent => "Docent",
            Self::Shield => "Shield",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|a| a.as_str() == s)
    }
}

/// How an item drops from a quest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LootType {
    Chest,
    Reward,
    Raid,
}

impl LootType {
    pub const ALL: &'static [LootType] = &[Self::Chest, Self::Reward, Self::Raid];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chest => "chest",
            Self::Reward => "reward",
            Self::Raid => "raid",
        }
    }
}

/// The family a damage type belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DamageCategory {
    Physical,
    Elemental,
    Alignment,
    Energy,
    Untyped,
}

impl DamageCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Physical => "physical",
            Self::Elemental => "elemental",
            Self::Alignment => "alignment",
            Self::Energy => "energy",
            Self::Untyped => "untyped",
        }
    }
}

/// Render a list of variants as a SQL `IN (...)` clause for a `CHECK` constraint.
pub(crate) fn sql_in_list<'a>(names: impl Iterator<Item = &'a str>) -> String {
    let quoted: Vec<String> = names.map(|n| format!("'{n}'")).collect();
    format!("IN ({})", quoted.join(", "))
}
