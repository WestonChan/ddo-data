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
///
/// Ids 1–33 are the DDO Tools originals; 34 onward were appended from DDOBuilderV2's
/// `BonusTypes.xml` (2026-09-20), which also supplies the stacking rule. Append, never renumber.
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
    ActionBoost,
    ArmorEnhancement,
    Base,
    Centered,
    Circumstance,
    Class,
    CombatStyle,
    Destiny,
    Divine,
    Enchantment,
    ElementalEnergy,
    ElementalSpellPower,
    EternalFaith,
    Epic,
    Feat,
    Fortune,
    GreaterElementalEnergy,
    GreaterElementalSpellPower,
    Guild,
    ImprovedElementalEnergy,
    ImprovedElementalSpellPower,
    Inspiration,
    Keen,
    LegendaryElementalEnergy,
    LegendaryElementalSpellPower,
    LevelUp,
    Mythic,
    Pirate,
    Psionic,
    Reaper,
    ShieldEnhancement,
    SilverFlame,
    Special,
    Spooky,
    Temporary,
    Unique,
    Universal,
    Untyped,
    WeaponDR,
    WeaponEnchantment,
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
        Self::ActionBoost,
        Self::ArmorEnhancement,
        Self::Base,
        Self::Centered,
        Self::Circumstance,
        Self::Class,
        Self::CombatStyle,
        Self::Destiny,
        Self::Divine,
        Self::Enchantment,
        Self::ElementalEnergy,
        Self::ElementalSpellPower,
        Self::EternalFaith,
        Self::Epic,
        Self::Feat,
        Self::Fortune,
        Self::GreaterElementalEnergy,
        Self::GreaterElementalSpellPower,
        Self::Guild,
        Self::ImprovedElementalEnergy,
        Self::ImprovedElementalSpellPower,
        Self::Inspiration,
        Self::Keen,
        Self::LegendaryElementalEnergy,
        Self::LegendaryElementalSpellPower,
        Self::LevelUp,
        Self::Mythic,
        Self::Pirate,
        Self::Psionic,
        Self::Reaper,
        Self::ShieldEnhancement,
        Self::SilverFlame,
        Self::Special,
        Self::Spooky,
        Self::Temporary,
        Self::Unique,
        Self::Universal,
        Self::Untyped,
        Self::WeaponDR,
        Self::WeaponEnchantment,
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
            Self::ActionBoost => 34,
            Self::ArmorEnhancement => 35,
            Self::Base => 36,
            Self::Centered => 37,
            Self::Circumstance => 38,
            Self::Class => 39,
            Self::CombatStyle => 40,
            Self::Destiny => 41,
            Self::Divine => 42,
            Self::Enchantment => 43,
            Self::ElementalEnergy => 44,
            Self::ElementalSpellPower => 45,
            Self::EternalFaith => 46,
            Self::Epic => 47,
            Self::Feat => 48,
            Self::Fortune => 49,
            Self::GreaterElementalEnergy => 50,
            Self::GreaterElementalSpellPower => 51,
            Self::Guild => 52,
            Self::ImprovedElementalEnergy => 53,
            Self::ImprovedElementalSpellPower => 54,
            Self::Inspiration => 55,
            Self::Keen => 56,
            Self::LegendaryElementalEnergy => 57,
            Self::LegendaryElementalSpellPower => 58,
            Self::LevelUp => 59,
            Self::Mythic => 60,
            Self::Pirate => 61,
            Self::Psionic => 62,
            Self::Reaper => 63,
            Self::ShieldEnhancement => 64,
            Self::SilverFlame => 65,
            Self::Special => 66,
            Self::Spooky => 67,
            Self::Temporary => 68,
            Self::Unique => 69,
            Self::Universal => 70,
            Self::Untyped => 71,
            Self::WeaponDR => 72,
            Self::WeaponEnchantment => 73,
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
            Self::ActionBoost => "Action Boost",
            Self::ArmorEnhancement => "Armor Enhancement",
            Self::Base => "Base",
            Self::Centered => "Centered",
            Self::Circumstance => "Circumstance",
            Self::Class => "Class",
            Self::CombatStyle => "Combat Style",
            Self::Destiny => "Destiny",
            Self::Divine => "Divine",
            Self::Enchantment => "Enchantment",
            Self::ElementalEnergy => "Elemental Energy",
            Self::ElementalSpellPower => "Elemental Spell Power",
            Self::EternalFaith => "Eternal Faith",
            Self::Epic => "Epic",
            Self::Feat => "Feat",
            Self::Fortune => "Fortune",
            Self::GreaterElementalEnergy => "Greater Elemental Energy",
            Self::GreaterElementalSpellPower => "Greater Elemental Spell Power",
            Self::Guild => "Guild",
            Self::ImprovedElementalEnergy => "Improved Elemental Energy",
            Self::ImprovedElementalSpellPower => "Improved Elemental Spell Power",
            Self::Inspiration => "Inspiration",
            Self::Keen => "Keen",
            Self::LegendaryElementalEnergy => "Legendary Elemental Energy",
            Self::LegendaryElementalSpellPower => "Legendary Elemental Spell Power",
            Self::LevelUp => "Level Up",
            Self::Mythic => "Mythic",
            Self::Pirate => "Pirate",
            Self::Psionic => "Psionic",
            Self::Reaper => "Reaper",
            Self::ShieldEnhancement => "Shield Enhancement",
            Self::SilverFlame => "Silver Flame",
            Self::Special => "Special",
            Self::Spooky => "Spooky",
            Self::Temporary => "Temporary",
            Self::Unique => "Unique",
            Self::Universal => "Universal",
            Self::Untyped => "Untyped",
            Self::WeaponDR => "Weapon DR",
            Self::WeaponEnchantment => "Weapon Enchantment",
        }
    }

    /// Whether two bonuses of this type to the same stat add together (his `Always`) rather than
    /// the highest applying (`Highest Only`).
    pub const fn stacks_with_self(self) -> bool {
        matches!(
            self,
            Self::Dodge
                | Self::Stacking
                | Self::Penalty
                | Self::ArmorEnhancement
                | Self::Destiny
                | Self::Mythic
                | Self::Reaper
                | Self::ShieldEnhancement
                | Self::Temporary
                | Self::Unique
                | Self::Untyped
                | Self::WeaponDR
        )
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

/// What a `modifiers` row belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModifierSource {
    Item,
    Augment,
    SetBonusTier,
    Filigree,
    Clickie,
    Feat,
    Enhancement,
    EnhancementSelection,
    Spell,
}

impl ModifierSource {
    pub const ALL: &'static [ModifierSource] = &[
        Self::Item,
        Self::Augment,
        Self::SetBonusTier,
        Self::Filigree,
        Self::Clickie,
        Self::Feat,
        Self::Enhancement,
        Self::EnhancementSelection,
        Self::Spell,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Item => "item",
            Self::Augment => "augment",
            Self::SetBonusTier => "set_bonus_tier",
            Self::Filigree => "filigree",
            Self::Clickie => "clickie",
            Self::Feat => "feat",
            Self::Enhancement => "enhancement",
            Self::EnhancementSelection => "enhancement_selection",
            Self::Spell => "spell",
        }
    }
}

/// What a `requirements` row belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RequirementOwner {
    Modifier,
    Feat,
    FeatConditionalGroup,
    FeatAutoAcquire,
    Stance,
    EnhancementTree,
    Enhancement,
    EnhancementSelection,
}

impl RequirementOwner {
    pub const ALL: &'static [RequirementOwner] = &[
        Self::Modifier,
        Self::Feat,
        Self::FeatConditionalGroup,
        Self::FeatAutoAcquire,
        Self::Stance,
        Self::EnhancementTree,
        Self::Enhancement,
        Self::EnhancementSelection,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Modifier => "modifier",
            Self::Feat => "feat",
            Self::FeatConditionalGroup => "feat_conditional_group",
            Self::FeatAutoAcquire => "feat_auto_acquire",
            Self::Stance => "stance",
            Self::EnhancementTree => "enhancement_tree",
            Self::Enhancement => "enhancement",
            Self::EnhancementSelection => "enhancement_selection",
        }
    }
}

/// Where a feat definition came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeatSource {
    /// `Feats.xml`
    Standard,
    /// A `<Feat>` inside a class file; `source_id` is the class.
    Class,
    /// A `<Feat>` inside a race file; `source_id` is the race.
    Race,
}

impl FeatSource {
    pub const ALL: &'static [FeatSource] = &[Self::Standard, Self::Class, Self::Race];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Class => "class",
            Self::Race => "race",
        }
    }
}

/// What a `stances`, `dcs` or `attacks` row belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AbilityOwner {
    Feat,
    Enhancement,
    EnhancementSelection,
    Spell,
}

impl AbilityOwner {
    pub const ALL: &'static [AbilityOwner] = &[Self::Feat, Self::Enhancement, Self::EnhancementSelection, Self::Spell];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Feat => "feat",
            Self::Enhancement => "enhancement",
            Self::EnhancementSelection => "enhancement_selection",
            Self::Spell => "spell",
        }
    }
}

/// A class's saving-throw progression. Upstream's `Type2` is the good progression (Paladin
/// Fortitude), `Type1` the poor one, `None` no save bonus at all (Epic, Legendary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SaveProgression {
    Good,
    Poor,
    None,
}

impl SaveProgression {
    pub const ALL: &'static [SaveProgression] = &[Self::Good, Self::Poor, Self::None];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Poor => "poor",
            Self::None => "none",
        }
    }

    pub fn from_upstream(s: &str) -> Option<Self> {
        match s.trim() {
            "Type2" => Some(Self::Good),
            "Type1" => Some(Self::Poor),
            "None" => Some(Self::None),
            _ => None,
        }
    }
}

/// How the requirements in one `<Requirements>` child group combine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RequirementGroup {
    All,
    OneOf,
    NoneOf,
}

impl RequirementGroup {
    pub const ALL: &'static [RequirementGroup] = &[Self::All, Self::OneOf, Self::NoneOf];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::OneOf => "one_of",
            Self::NoneOf => "none_of",
        }
    }
}

/// Which action-point pool an enhancement tree draws from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TreeKind {
    Class,
    Racial,
    Universal,
    Reaper,
    Destiny,
}

impl TreeKind {
    pub const ALL: &'static [TreeKind] = &[Self::Class, Self::Racial, Self::Universal, Self::Reaper, Self::Destiny];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::Racial => "racial",
            Self::Universal => "universal",
            Self::Reaper => "reaper",
            Self::Destiny => "destiny",
        }
    }
}
