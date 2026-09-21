//! The adapter from DDOBuilderV2's vocabulary onto ours. Every mapping is data in `data/*.toml`;
//! these tests pin the semantics the roadmap's V2 entry records.

use ddo_etl::map::augment_slot::{decode, SlotSpec};
use ddo_etl::map::bonus_type::normalize;
use ddo_etl::map::buff::{BuffMap, Resolved};
use ddo_etl::map::placement::{classify, Placement};
use ddo_etl::xml::items::{Buff, EquipmentSlots, SlotTag};
use ddo_model::enums::{BonusType, EquipmentSlot, Handedness, ItemCategory};

fn slots(tags: &[SlotTag]) -> EquipmentSlots {
    EquipmentSlots { tags: tags.to_vec() }
}

fn buff(kind: &str, item: Option<&str>, value1: Option<i64>, bonus_type: Option<&str>) -> Buff {
    Buff {
        kind: kind.to_string(),
        item: item.map(str::to_string),
        item2: None,
        value1,
        value2: None,
        bonus_type: bonus_type.map(str::to_string),
        description1: None,
    }
}

#[test]
fn bonus_types_normalise_onto_ours() {
    assert_eq!(normalize("Enhancement").unwrap(), Some(BonusType::Enhancement));
    assert_eq!(normalize("Weapon Enchantment").unwrap(), Some(BonusType::Enhancement));
    assert_eq!(normalize("Armor Enhancement").unwrap(), Some(BonusType::Enhancement));
    assert_eq!(normalize("Insightful").unwrap(), Some(BonusType::Insight));
    assert_eq!(normalize("resistance").unwrap(), Some(BonusType::Resistance));
    assert_eq!(normalize("Vitality").unwrap(), Some(BonusType::Vitality));
    assert_eq!(normalize("Not Set").unwrap(), None);
    assert_eq!(normalize("").unwrap(), None);
    assert!(normalize("Bogus").is_err(), "an unknown bonus type is a parse error");
}

#[test]
fn placement_from_slot_tags_and_weapon_type() {
    let one_handed = classify(&slots(&[SlotTag::Weapon1, SlotTag::Weapon2]), Some("Longsword"), None).unwrap();
    assert_eq!(
        one_handed,
        Some(Placement {
            slot: EquipmentSlot::MainHand,
            category: ItemCategory::Weapon,
            handedness: Some(Handedness::OneHanded),
            item_type: Some("Longsword".into()),
        })
    );
    let bow = classify(&slots(&[SlotTag::Weapon1]), Some("Longbow"), None).unwrap().unwrap();
    assert_eq!((bow.slot, bow.handedness), (EquipmentSlot::MainHand, Some(Handedness::TwoHanded)));
    let dart = classify(&slots(&[SlotTag::Weapon1, SlotTag::Weapon2]), Some("Dart"), None).unwrap().unwrap();
    assert_eq!(dart.handedness, Some(Handedness::Thrown));
    let shield = classify(&slots(&[SlotTag::Weapon2]), Some("Large Shield"), None).unwrap().unwrap();
    assert_eq!(
        (shield.slot, shield.category, shield.handedness),
        (EquipmentSlot::OffHand, ItemCategory::Shield, Some(Handedness::OffHand))
    );
    let rune_arm = classify(&slots(&[SlotTag::Weapon2]), Some("RuneArm"), None).unwrap().unwrap();
    assert_eq!((rune_arm.slot, rune_arm.item_type.as_deref()), (EquipmentSlot::Runearm, Some("Rune Arm")));

    let docent = classify(&slots(&[SlotTag::Armor]), None, Some("Docent")).unwrap().unwrap();
    assert_eq!(
        (docent.slot, docent.category, docent.item_type.as_deref()),
        (EquipmentSlot::Body, ItemCategory::Armor, Some("Docent"))
    );
    let gloves = classify(&slots(&[SlotTag::Gloves]), None, None).unwrap().unwrap();
    assert_eq!((gloves.slot, gloves.category), (EquipmentSlot::Hands, ItemCategory::Clothing));
    let ring = classify(&slots(&[SlotTag::Ring, SlotTag::Trinket]), None, None).unwrap().unwrap();
    assert_eq!((ring.slot, ring.category), (EquipmentSlot::Ring, ItemCategory::Jewelry));

    assert_eq!(
        classify(&slots(&[SlotTag::CosmeticHelm]), None, None).unwrap(),
        None,
        "cosmetic-only items are excluded"
    );
    assert_eq!(classify(&slots(&[]), None, None).unwrap(), None);
    assert!(
        classify(&slots(&[SlotTag::Weapon1]), Some("Lightsaber"), None).is_err(),
        "unknown weapon type is an error"
    );
}

#[test]
fn buffs_resolve_to_enhancement_bonus_stat_or_effect() {
    let map = BuffMap::load().unwrap();

    assert_eq!(
        map.resolve(&buff("WeaponEnchantment", None, Some(7), Some("Weapon Enchantment"))).unwrap(),
        Resolved::Enhancement(7)
    );
    assert_eq!(
        map.resolve(&buff("ArmorEnchantment", None, Some(4), Some("Armor Enhancement"))).unwrap(),
        Resolved::Enhancement(4)
    );

    match map.resolve(&buff("AbilityBonus", Some("Strength"), Some(8), Some("Enhancement"))).unwrap() {
        Resolved::Bonus { stat, bonus_type, value, value2 } => {
            assert_eq!(stat.name, "Strength");
            assert_eq!(bonus_type, Some(BonusType::Enhancement));
            assert_eq!((value, value2), (Some(8), None));
        }
        other => panic!("{other:?}"),
    }
    let cases = [
        (buff("SkillBonus", Some("Move Silently"), Some(11), Some("Competence")), "Move Silently"),
        (buff("SchoolFocusNumber", Some("Rune Arm"), Some(2), Some("Equipment")), "Rune Arm Spell Focus"),
        (buff("Absorption", Some("Cold"), Some(34), Some("Enhancement")), "Cold Absorption"),
        (buff("Absorption", Some("Lawful"), Some(20), Some("Enhancement")), "Law Absorption"),
        (buff("EnergyResistance", Some("Fire"), Some(20), Some("Enhancement")), "Fire Resistance"),
        (buff("SpellcastingImplement", None, Some(9), Some("Implement")), "Universal Spell Power"),
        (buff("Combustion", Some("Fire"), Some(54), Some("Enhancement")), "Fire Spell Power"),
        (buff("Combustion", None, Some(54), Some("Enhancement")), "Fire Spell Power"),
        (buff("PhysicalSheltering", None, Some(30), Some("Enhancement")), "Physical Resistance Rating"),
        (buff("Seeker", None, Some(9), Some("Enhancement")), "Seeker"),
        (buff("FalseLife", None, Some(50), Some("Enhancement")), "Hit Points"),
        (buff("Protection", None, Some(5), Some("Deflection")), "Armor Class"),
        (buff("Resistance", None, Some(4), Some("Resistance")), "Saving Throws"),
        (buff("WizardryNumber", None, Some(200), Some("Enhancement")), "Spell Points"),
    ];
    for (b, expected) in cases {
        match map.resolve(&b).unwrap() {
            Resolved::Bonus { stat, .. } => assert_eq!(stat.name, expected, "{}", b.kind),
            other => panic!("{}: {other:?}", b.kind),
        }
    }

    match map.resolve(&buff("Sovereign Vorpal", Some("All"), None, None)).unwrap() {
        Resolved::Effect { name, value, target } => {
            assert_eq!(name, "Sovereign Vorpal");
            assert_eq!(value, None);
            assert_eq!(target.as_deref(), Some("All"));
        }
        other => panic!("{other:?}"),
    }
    match map.resolve(&buff("Lifesealed", None, Some(34), Some("Enhancement"))).unwrap() {
        Resolved::Effect { value, .. } => assert_eq!(value, Some(34)),
        other => panic!("{other:?}"),
    }
    assert!(
        map.resolve(&buff("AbilityBonus", Some("Luck"), Some(1), None)).is_err(),
        "a by-item template that resolves to no stat is an error, not a silent effect"
    );
}

#[test]
fn bonus_descriptions_fill_the_display_template() {
    let map = BuffMap::load().unwrap();
    let text = map.describe(
        "%b1 %i1 %v1: Passive: %v1 %b1 bonus to %i1.",
        &buff("AbilityBonus", Some("Strength"), Some(8), Some("Enhancement")),
    );
    assert_eq!(text, "Enhancement Strength 8: Passive: 8 Enhancement bonus to Strength.");
}

#[test]
fn augment_slot_types_decode_into_family_variant_qualifier() {
    assert_eq!(
        decode("Red"),
        SlotSpec { label: "red".into(), family: "standard".into(), variant: "red".into(), qualifier: None }
    );
    assert_eq!(decode("Colorless").label, "colorless");
    assert_eq!(decode("Sun").family, "standard");
    assert_eq!(
        decode("Melancholic Slot (Accessory)"),
        SlotSpec {
            label: "lamordia: melancholic (accessory)".into(),
            family: "lamordia".into(),
            variant: "melancholic".into(),
            qualifier: Some("accessory".into())
        }
    );
    assert_eq!(
        decode("IoD: Weapon: Scale Slot"),
        SlotSpec {
            label: "isle of dread: scale (weapon)".into(),
            family: "dino".into(),
            variant: "scale".into(),
            qualifier: Some("weapon".into())
        }
    );
    assert_eq!(decode("IoD: Set Bonus Slot").label, "isle of dread: set bonus");
    assert_eq!(
        decode("Tier 2"),
        SlotSpec {
            label: "upgrade: tier 2".into(),
            family: "upgrade".into(),
            variant: "tier 2".into(),
            qualifier: None
        }
    );
    assert_eq!(decode("Greensteel Weapon Tier 1").family, "crafting");
    assert_eq!(decode("Greensteel Weapon Tier 1").label, "crafting: greensteel weapon tier 1");
}
