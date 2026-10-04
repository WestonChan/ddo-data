use super::effect_map::EFFECT_MAP;
use crate::xml::items::{EquipmentSlotTag, EquipmentSlots};
use anyhow::{bail, Result};
use ddo_model::enums::{EquipmentSlot, Handedness, ItemCategory};
use ddo_model::seeds::WeaponType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub equipment_slot: EquipmentSlot,
    pub category: ItemCategory,
    pub handedness: Option<Handedness>,
    pub item_type: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CosmeticExclusion {
    CosmeticOnlySlots,
    CosmeticShield,
}

impl CosmeticExclusion {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::CosmeticOnlySlots => "cosmetic-only slots",
            Self::CosmeticShield => "cosmetic shield",
        }
    }
}

impl Placement {
    pub fn weapon_type(&self) -> Option<&'static WeaponType> {
        self.handedness.and(self.item_type.as_deref()).and_then(WeaponType::by_name)
    }
}

pub fn placement_of(
    equipment_slots: &EquipmentSlots,
    weapon_name: Option<&str>,
    armor_name: Option<&str>,
) -> Result<Result<Placement, CosmeticExclusion>> {
    let wearable_tags: Vec<EquipmentSlotTag> =
        equipment_slots.tags.iter().copied().filter(|t| !t.is_cosmetic()).collect();
    let Some(&first_tag) = wearable_tags.first() else {
        return Ok(Err(CosmeticExclusion::CosmeticOnlySlots));
    };
    let has_tag = |tag: EquipmentSlotTag| wearable_tags.contains(&tag);

    if has_tag(EquipmentSlotTag::Weapon1) || has_tag(EquipmentSlotTag::Weapon2) {
        let Some(upstream_weapon_name) = weapon_name else {
            bail!("item occupies a weapon slot but has no <Weapon>");
        };
        let canonical_weapon_name =
            EFFECT_MAP.weapon_aliases.get(upstream_weapon_name).map(String::as_str).unwrap_or(upstream_weapon_name);
        let Some(weapon_type) = WeaponType::by_name(canonical_weapon_name) else {
            bail!(
                "unknown weapon type {upstream_weapon_name:?}; add it to ddo-model's WEAPON_TYPES or to [weapon_aliases]"
            );
        };
        if weapon_type.is_cosmetic() {
            return Ok(Err(CosmeticExclusion::CosmeticShield));
        }
        let (equipment_slot, category, handedness) = if weapon_type.is_shield {
            (EquipmentSlot::OffHand, ItemCategory::Shield, Handedness::OffHand)
        } else if weapon_type.name == "Rune Arm" {
            (EquipmentSlot::Runearm, ItemCategory::Weapon, Handedness::OffHand)
        } else if weapon_type.name == "Orb" || !has_tag(EquipmentSlotTag::Weapon1) {
            (EquipmentSlot::OffHand, ItemCategory::Weapon, Handedness::OffHand)
        } else if weapon_type.is_thrown() {
            (EquipmentSlot::MainHand, ItemCategory::Weapon, Handedness::Thrown)
        } else if has_tag(EquipmentSlotTag::Weapon2) {
            (EquipmentSlot::MainHand, ItemCategory::Weapon, Handedness::OneHanded)
        } else {
            (EquipmentSlot::MainHand, ItemCategory::Weapon, Handedness::TwoHanded)
        };
        return Ok(Ok(Placement {
            equipment_slot,
            category,
            handedness: Some(handedness),
            item_type: Some(weapon_type.name.to_string()),
        }));
    }

    let (equipment_slot, category) = match first_tag {
        EquipmentSlotTag::Armor => (EquipmentSlot::Body, ItemCategory::Armor),
        EquipmentSlotTag::Ring => (EquipmentSlot::Ring, ItemCategory::Jewelry),
        EquipmentSlotTag::Necklace => (EquipmentSlot::Neck, ItemCategory::Jewelry),
        EquipmentSlotTag::Trinket => (EquipmentSlot::Trinket, ItemCategory::Jewelry),
        EquipmentSlotTag::Goggles => (EquipmentSlot::Goggles, ItemCategory::Jewelry),
        EquipmentSlotTag::Helmet => (EquipmentSlot::Head, ItemCategory::Clothing),
        EquipmentSlotTag::Cloak => (EquipmentSlot::Back, ItemCategory::Clothing),
        EquipmentSlotTag::Bracers => (EquipmentSlot::Wrists, ItemCategory::Clothing),
        EquipmentSlotTag::Gloves => (EquipmentSlot::Hands, ItemCategory::Clothing),
        EquipmentSlotTag::Belt => (EquipmentSlot::Waist, ItemCategory::Clothing),
        EquipmentSlotTag::Boots => (EquipmentSlot::Feet, ItemCategory::Clothing),
        EquipmentSlotTag::Quiver => (EquipmentSlot::Quiver, ItemCategory::Clothing),
        EquipmentSlotTag::Weapon1 | EquipmentSlotTag::Weapon2 => unreachable!("handled above"),
        cosmetic_tag => unreachable!("cosmetic tags were filtered: {cosmetic_tag:?}"),
    };
    let item_type = if first_tag == EquipmentSlotTag::Armor { armor_name.map(str::to_string) } else { None };
    Ok(Ok(Placement { equipment_slot, category, handedness: None, item_type }))
}
