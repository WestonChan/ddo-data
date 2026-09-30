use anyhow::{bail, Context, Result};
use ddo_model::enums::{ArmorType, BonusType, EquipmentSlot, Handedness, ItemCategory, LootType};
use ddo_model::seeds::WeaponType;
use ddo_model::stats::Stat;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiItem {
    pub name: String,
    pub page: String,
    pub read: String,
    pub slot: String,
    pub category: String,
    pub item_type: Option<String>,
    pub minimum_level: i64,
    pub enhancement_bonus: Option<i64>,
    pub material: Option<String>,
    pub race_required: Option<String>,
    pub description: Option<String>,
    pub drop_location: String,
    #[serde(default)]
    pub quests: Vec<WikiItemQuest>,
    pub set: Option<String>,
    #[serde(default)]
    pub accepts_sentience: bool,
    #[serde(default)]
    pub is_minor_artifact: bool,
    #[serde(default)]
    pub augment_slots: Vec<String>,
    #[serde(default)]
    pub bonuses: Vec<WikiItemBonus>,
    #[serde(default)]
    pub effects: Vec<WikiItemEffect>,
    pub weapon: Option<WikiWeaponStats>,
    pub armor: Option<WikiArmorStats>,
    #[serde(skip)]
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiItemQuest {
    pub name: String,
    pub loot_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiItemBonus {
    pub stat: String,
    pub bonus_type: String,
    pub value: i64,
    pub value2: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiItemEffect {
    pub name: String,
    pub description: Option<String>,
    pub value: Option<i64>,
    pub target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiWeaponStats {
    pub damage_dice_count: Option<i64>,
    pub damage_dice_sides: Option<i64>,
    pub damage_dice_bonus: Option<i64>,
    pub damage_multiplier: Option<f64>,
    pub critical_threat_range: Option<i64>,
    pub critical_multiplier: Option<i64>,
    pub handedness: String,
    #[serde(default)]
    pub dr_bypass: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
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

impl WikiItemBonus {
    pub fn stat(&self) -> &'static Stat {
        Stat::by_name(&self.stat).expect("validated stat")
    }

    pub fn bonus_type(&self) -> BonusType {
        BonusType::parse(&self.bonus_type).expect("validated bonus_type")
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

    pub(super) fn validate(&self) -> Result<()> {
        if EquipmentSlot::ALL.iter().all(|s| s.name() != self.slot) {
            let slot_names: Vec<&str> = EquipmentSlot::ALL.iter().map(|s| s.name()).collect();
            bail!("slot {:?} must be one of {}", self.slot, slot_names.join(", "));
        }
        if ItemCategory::ALL.iter().all(|c| c.as_str() != self.category) {
            let category_names: Vec<&str> = ItemCategory::ALL.iter().map(|c| c.as_str()).collect();
            bail!("category {:?} must be one of {}", self.category, category_names.join(", "));
        }
        self.validate_weapon_and_armor()?;
        for quest in &self.quests {
            if LootType::ALL.iter().all(|l| l.as_str() != quest.loot_type) {
                let loot_type_names: Vec<&str> = LootType::ALL.iter().map(|l| l.as_str()).collect();
                bail!(
                    "quest {:?}: loot_type {:?} must be one of {}",
                    quest.name,
                    quest.loot_type,
                    loot_type_names.join(", ")
                );
            }
        }
        for bonus in &self.bonuses {
            if Stat::by_name(&bonus.stat).is_none() {
                bail!("bonus stat {:?} is not a stat /v1/stats lists; use its exact name", bonus.stat);
            }
            if BonusType::parse(&bonus.bonus_type).is_none() {
                bail!(
                    "bonus {:?}: bonus_type {:?} is not a bonus type /v1/bonus-types lists; use its exact name",
                    bonus.stat,
                    bonus.bonus_type
                );
            }
        }
        Ok(())
    }

    fn validate_weapon_and_armor(&self) -> Result<()> {
        let category = self.item_category();
        let needs_weapon = category == ItemCategory::Weapon;
        let needs_armor = matches!(category, ItemCategory::Armor | ItemCategory::Shield);
        if needs_weapon && self.weapon.is_none() {
            bail!("category \"Weapon\" needs an [item.weapon] table");
        }
        if self.weapon.is_some() && !matches!(category, ItemCategory::Weapon | ItemCategory::Shield) {
            bail!("[item.weapon] is only for Weapon and Shield items, not {:?}", self.category);
        }
        match (&self.armor, needs_armor) {
            (None, true) => bail!("category {:?} needs an [item.armor] table", self.category),
            (Some(_), false) => bail!("[item.armor] is only for Armor and Shield items, not {:?}", self.category),
            _ => {}
        }
        if matches!(category, ItemCategory::Weapon | ItemCategory::Shield) {
            let item_type = self.item_type.as_deref().with_context(|| {
                format!("category {:?} needs item_type, a weapon type /v1/weapon-types lists", self.category)
            })?;
            let weapon_type = WeaponType::by_name(item_type).with_context(|| {
                format!("item_type {item_type:?} is not a weapon type /v1/weapon-types lists; use its exact name")
            })?;
            if weapon_type.is_shield != (category == ItemCategory::Shield) {
                bail!("item_type {item_type:?} does not fit category {:?}", self.category);
            }
        }
        if let Some(weapon) = &self.weapon {
            if Handedness::ALL.iter().all(|h| h.as_str() != weapon.handedness) {
                let handedness_names: Vec<&str> = Handedness::ALL.iter().map(|h| h.as_str()).collect();
                bail!("weapon handedness {:?} must be one of {}", weapon.handedness, handedness_names.join(", "));
            }
        }
        if let Some(armor) = &self.armor {
            let Some(armor_type) = ArmorType::parse(&armor.armor_type) else {
                let armor_type_names: Vec<&str> = ArmorType::ALL.iter().map(|a| a.as_str()).collect();
                bail!("armor armor_type {:?} must be one of {}", armor.armor_type, armor_type_names.join(", "));
            };
            if (armor_type == ArmorType::Shield) != (category == ItemCategory::Shield) {
                bail!("armor armor_type {:?} does not fit category {:?}", armor.armor_type, self.category);
            }
            if category == ItemCategory::Armor && self.item_type.as_deref().is_some_and(|t| t != armor.armor_type) {
                bail!(
                    "item_type {:?} must be the armor type {:?} for Armor, as Maetrim's items carry it",
                    self.item_type.as_deref().unwrap_or_default(),
                    armor.armor_type
                );
            }
        }
        Ok(())
    }
}
