use super::bonuses::WikiBonus;
use ddo_model::enums::{ArmorType, EquipmentSlot, Handedness, ItemCategory, LootType};
use ddo_model::seeds::WeaponType;

#[derive(Debug, Clone, PartialEq)]
pub struct WikiItem {
    pub name: String,
    pub wiki_url: String,
    pub slot: String,
    pub category: String,
    pub item_type: Option<String>,
    pub minimum_level: i64,
    pub enhancement_bonus: Option<i64>,
    pub material: Option<String>,
    pub race_required: Option<String>,
    pub description: Option<String>,
    pub drop_location: String,
    pub quests: Vec<WikiItemQuest>,
    pub set: Option<String>,
    pub accepts_sentience: bool,
    pub is_minor_artifact: bool,
    pub augment_slots: Vec<String>,
    pub bonuses: Vec<WikiBonus>,
    pub effects: Vec<WikiItemEffect>,
    pub weapon: Option<WikiWeaponStats>,
    pub armor: Option<WikiArmorStats>,
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiItemQuest {
    pub name: String,
    pub loot_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiItemEffect {
    pub name: String,
    pub description: Option<String>,
    pub value: Option<i64>,
    pub target: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WikiWeaponStats {
    pub damage_dice_count: Option<i64>,
    pub damage_dice_sides: Option<i64>,
    pub damage_dice_bonus: Option<i64>,
    pub damage_multiplier: Option<f64>,
    pub critical_threat_range: Option<i64>,
    pub critical_multiplier: Option<i64>,
    pub handedness: String,
    pub dr_bypass: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiArmorStats {
    pub armor_type: String,
    pub armor_bonus: Option<i64>,
    pub max_dex_bonus: Option<i64>,
    pub arcane_spell_failure: Option<i64>,
    pub armor_check_penalty: Option<i64>,
    pub shield_bonus: Option<i64>,
}

impl WikiItemQuest {
    pub fn loot_type(&self) -> LootType {
        LootType::ALL.iter().copied().find(|l| l.as_str() == self.loot_type).expect("validated loot_type")
    }
}

impl WikiWeaponStats {
    pub fn handedness(&self) -> Handedness {
        Handedness::ALL.iter().copied().find(|h| h.as_str() == self.handedness).expect("validated handedness")
    }
}

impl WikiItem {
    pub fn equipment_slot(&self) -> EquipmentSlot {
        EquipmentSlot::ALL.iter().copied().find(|s| s.name() == self.slot).expect("validated slot")
    }

    pub fn item_category(&self) -> ItemCategory {
        ItemCategory::ALL.iter().copied().find(|c| c.as_str() == self.category).expect("validated category")
    }

    pub fn weapon_type(&self) -> Option<&'static WeaponType> {
        self.weapon.as_ref().and(self.item_type.as_deref()).and_then(WeaponType::by_name)
    }

    pub fn armor_type(&self) -> Option<ArmorType> {
        self.armor.as_ref().and_then(|armor| ArmorType::parse(&armor.armor_type))
    }
}
