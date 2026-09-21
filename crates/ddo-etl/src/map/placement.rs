//! Where an item goes and what kind of thing it is, from its `<EquipmentSlot>` tags and its
//! `<Weapon>` or `<Armor>` subtype.

use super::MAPPING;
use crate::xml::items::{EquipmentSlots, SlotTag};
use anyhow::{bail, Result};
use ddo_model::enums::{EquipmentSlot, Handedness, ItemCategory};
use ddo_model::seeds::WeaponType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub slot: EquipmentSlot,
    pub category: ItemCategory,
    /// Set for anything held in a hand, including shields, orbs and rune arms.
    pub handedness: Option<Handedness>,
    /// The weapon type or armor weight class, as `items.item_type`.
    pub item_type: Option<String>,
}

impl Placement {
    /// The seed row for a weapon or shield, when the item is one.
    pub fn weapon_type(&self) -> Option<&'static WeaponType> {
        self.handedness.and(self.item_type.as_deref()).and_then(WeaponType::by_name)
    }
}

/// `Ok(None)` for items that only occupy cosmetic slots or none at all; those are not gear.
pub fn classify(slots: &EquipmentSlots, weapon: Option<&str>, armor: Option<&str>) -> Result<Option<Placement>> {
    let tags: Vec<SlotTag> = slots.tags.iter().copied().filter(|t| !t.is_cosmetic()).collect();
    let Some(&first) = tags.first() else {
        return Ok(None);
    };
    let has = |t: SlotTag| tags.contains(&t);

    if has(SlotTag::Weapon1) || has(SlotTag::Weapon2) {
        let Some(raw) = weapon else {
            bail!("item occupies a weapon slot but has no <Weapon>");
        };
        let name = MAPPING.weapon_aliases.get(raw).map(String::as_str).unwrap_or(raw);
        let Some(wt) = WeaponType::by_name(name) else {
            bail!("unknown weapon type {raw:?}; add it to ddo-model's WEAPON_TYPES or to [weapon_aliases]");
        };
        let (slot, category, handedness) = if wt.is_shield {
            (EquipmentSlot::OffHand, ItemCategory::Shield, Handedness::OffHand)
        } else if wt.name == "Rune Arm" {
            (EquipmentSlot::Runearm, ItemCategory::Weapon, Handedness::OffHand)
        } else if wt.name == "Orb" || !has(SlotTag::Weapon1) {
            (EquipmentSlot::OffHand, ItemCategory::Weapon, Handedness::OffHand)
        } else if wt.is_thrown() {
            (EquipmentSlot::MainHand, ItemCategory::Weapon, Handedness::Thrown)
        } else if has(SlotTag::Weapon2) {
            (EquipmentSlot::MainHand, ItemCategory::Weapon, Handedness::OneHanded)
        } else {
            (EquipmentSlot::MainHand, ItemCategory::Weapon, Handedness::TwoHanded)
        };
        return Ok(Some(Placement {
            slot,
            category,
            handedness: Some(handedness),
            item_type: Some(wt.name.to_string()),
        }));
    }

    let (slot, category) = match first {
        SlotTag::Armor => (EquipmentSlot::Body, ItemCategory::Armor),
        SlotTag::Ring => (EquipmentSlot::Ring, ItemCategory::Jewelry),
        SlotTag::Necklace => (EquipmentSlot::Neck, ItemCategory::Jewelry),
        SlotTag::Trinket => (EquipmentSlot::Trinket, ItemCategory::Jewelry),
        SlotTag::Goggles => (EquipmentSlot::Goggles, ItemCategory::Jewelry),
        SlotTag::Helmet => (EquipmentSlot::Head, ItemCategory::Clothing),
        SlotTag::Cloak => (EquipmentSlot::Back, ItemCategory::Clothing),
        SlotTag::Bracers => (EquipmentSlot::Wrists, ItemCategory::Clothing),
        SlotTag::Gloves => (EquipmentSlot::Hands, ItemCategory::Clothing),
        SlotTag::Belt => (EquipmentSlot::Waist, ItemCategory::Clothing),
        SlotTag::Boots => (EquipmentSlot::Feet, ItemCategory::Clothing),
        SlotTag::Quiver => (EquipmentSlot::Quiver, ItemCategory::Clothing),
        SlotTag::Weapon1 | SlotTag::Weapon2 => unreachable!("handled above"),
        cosmetic => unreachable!("cosmetic tags were filtered: {cosmetic:?}"),
    };
    let item_type = if first == SlotTag::Armor { armor.map(str::to_string) } else { None };
    Ok(Some(Placement { slot, category, handedness: None, item_type }))
}
