use serde::{Deserialize, Serialize};

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
    WeaponDr,
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
        Self::WeaponDr,
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
            Self::WeaponDr => 72,
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
            Self::WeaponDr => "Weapon DR",
            Self::WeaponEnchantment => "Weapon Enchantment",
        }
    }

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
                | Self::WeaponDr
        )
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|b| b.name() == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EquipmentSlotCategory {
    Weapon,
    Armor,
    Accessory,
}

impl EquipmentSlotCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Weapon => "weapon",
            Self::Armor => "armor",
            Self::Accessory => "accessory",
        }
    }
}

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

    pub const fn category(self) -> EquipmentSlotCategory {
        match self {
            Self::MainHand | Self::OffHand | Self::Ranged | Self::Quiver | Self::Runearm => {
                EquipmentSlotCategory::Weapon
            }
            Self::Head | Self::Back | Self::Wrists | Self::Hands | Self::Body | Self::Waist | Self::Feet => {
                EquipmentSlotCategory::Armor
            }
            Self::Neck | Self::Trinket | Self::Goggles | Self::Ring => EquipmentSlotCategory::Accessory,
        }
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RowSource {
    Maetrim,
    Wiki,
}

impl RowSource {
    pub const ALL: &'static [RowSource] = &[Self::Maetrim, Self::Wiki];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Maetrim => "maetrim",
            Self::Wiki => "wiki",
        }
    }
}

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

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|a| a.as_str() == text)
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CraftingTier {
    Heroic,
    Epic,
    Legendary,
    Any,
}

impl CraftingTier {
    pub const ALL: &'static [CraftingTier] = &[Self::Heroic, Self::Epic, Self::Legendary, Self::Any];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Heroic => "heroic",
            Self::Epic => "epic",
            Self::Legendary => "legendary",
            Self::Any => "any",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|t| t.as_str() == text)
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModifierSource {
    Item,
    Augment,
    SetBonusTier,
    Filigree,
    Clickie,
    Feat,
    FeatFollowOn,
    FeatThisAttack,
    Enhancement,
    EnhancementFollowOn,
    EnhancementThisAttack,
    EnhancementSelection,
    EnhancementSelectionFollowOn,
    EnhancementSelectionThisAttack,
    Spell,
    Stance,
    GuildBuff,
    OptionalBuff,
}

impl ModifierSource {
    pub const ALL: &'static [ModifierSource] = &[
        Self::Item,
        Self::Augment,
        Self::SetBonusTier,
        Self::Filigree,
        Self::Clickie,
        Self::Feat,
        Self::FeatFollowOn,
        Self::FeatThisAttack,
        Self::Enhancement,
        Self::EnhancementFollowOn,
        Self::EnhancementThisAttack,
        Self::EnhancementSelection,
        Self::EnhancementSelectionFollowOn,
        Self::EnhancementSelectionThisAttack,
        Self::Spell,
        Self::Stance,
        Self::GuildBuff,
        Self::OptionalBuff,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Item => "item",
            Self::Augment => "augment",
            Self::SetBonusTier => "set_bonus_tier",
            Self::Filigree => "filigree",
            Self::Clickie => "clickie",
            Self::Feat => "feat",
            Self::FeatFollowOn => "feat_follow_on",
            Self::FeatThisAttack => "feat_this_attack",
            Self::Enhancement => "enhancement",
            Self::EnhancementFollowOn => "enhancement_follow_on",
            Self::EnhancementThisAttack => "enhancement_this_attack",
            Self::EnhancementSelection => "enhancement_selection",
            Self::EnhancementSelectionFollowOn => "enhancement_selection_follow_on",
            Self::EnhancementSelectionThisAttack => "enhancement_selection_this_attack",
            Self::Spell => "spell",
            Self::Stance => "stance",
            Self::GuildBuff => "guild_buff",
            Self::OptionalBuff => "optional_buff",
        }
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeatSource {
    Standard,
    Class,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AbilityOwner {
    Feat,
    Enhancement,
    EnhancementSelection,
    Spell,
    Standalone,
}

impl AbilityOwner {
    pub const ALL: &'static [AbilityOwner] =
        &[Self::Feat, Self::Enhancement, Self::EnhancementSelection, Self::Spell, Self::Standalone];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Feat => "feat",
            Self::Enhancement => "enhancement",
            Self::EnhancementSelection => "enhancement_selection",
            Self::Spell => "spell",
            Self::Standalone => "standalone",
        }
    }
}

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

    pub fn parse(code: &str) -> Option<Self> {
        match code.trim() {
            "Type2" => Some(Self::Good),
            "Type1" => Some(Self::Poor),
            "None" => Some(Self::None),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RequirementGroupKind {
    All,
    OneOf,
    NoneOf,
}

impl RequirementGroupKind {
    pub const ALL: &'static [RequirementGroupKind] = &[Self::All, Self::OneOf, Self::NoneOf];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::OneOf => "one_of",
            Self::NoneOf => "none_of",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EnhancementTreeKind {
    Class,
    Racial,
    Universal,
    Reaper,
    Destiny,
}

impl EnhancementTreeKind {
    pub const ALL: &'static [EnhancementTreeKind] =
        &[Self::Class, Self::Racial, Self::Universal, Self::Reaper, Self::Destiny];

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CorrectionKind {
    Item,
    Augment,
    Quest,
    Feat,
    Enhancement,
    Race,
    Class,
    AdventurePack,
    Patron,
    SetBonus,
    Spell,
}

impl CorrectionKind {
    pub const ALL: &'static [CorrectionKind] = &[
        Self::Item,
        Self::Augment,
        Self::Quest,
        Self::Feat,
        Self::Enhancement,
        Self::Race,
        Self::Class,
        Self::AdventurePack,
        Self::Patron,
        Self::SetBonus,
        Self::Spell,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Item => "item",
            Self::Augment => "augment",
            Self::Quest => "quest",
            Self::Feat => "feat",
            Self::Enhancement => "enhancement",
            Self::Race => "race",
            Self::Class => "class",
            Self::AdventurePack => "adventure_pack",
            Self::Patron => "patron",
            Self::SetBonus => "set_bonus",
            Self::Spell => "spell",
        }
    }

    pub const fn table_name(self) -> &'static str {
        match self {
            Self::Item => "items",
            Self::Augment => "augments",
            Self::Quest => "quests",
            Self::Feat => "feats",
            Self::Enhancement => "enhancements",
            Self::Race => "races",
            Self::Class => "classes",
            Self::AdventurePack => "adventure_packs",
            Self::Patron => "patrons",
            Self::SetBonus => "set_bonuses",
            Self::Spell => "spells",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.as_str() == text)
    }
}
