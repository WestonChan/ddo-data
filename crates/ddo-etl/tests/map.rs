use ddo_etl::map::augment_slot::{decode, SlotSpec};
use ddo_etl::map::bonus_type::normalize;
use ddo_etl::map::buff::{BuffMap, Resolved};
use ddo_etl::map::effect::EffectMap;
use ddo_etl::map::placement::{classify, Placement};
use ddo_etl::xml::effect::Effect;
use ddo_etl::xml::items::{Buff, EquipmentSlots, SlotTag};
use ddo_model::enums::{BonusType, EquipmentSlot, Handedness, ItemCategory, StatCategory};
use std::collections::BTreeMap;

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

fn simple_effect(kind: &str) -> Effect {
    Effect {
        types: vec![kind.to_string()],
        bonus: Some("Enhancement".to_string()),
        amount_type: Some("Simple".to_string()),
        amounts: vec![3.0],
        ..Effect::default()
    }
}

#[test]
fn character_wide_effect_types_resolve_to_their_stats() {
    use StatCategory::*;
    let cases: &[(&str, &str, StatCategory)] = &[
        ("ArcaneSpellFailure", "Arcane Spell Failure", Magical),
        ("ArcaneSpellFailureShields", "Arcane Spell Failure (Shield)", Magical),
        ("ArmorCheckPenalty", "Armor Check Penalty", Defensive),
        ("ACBonusShield", "Shield Armor Class", Defensive),
        ("MaxDexBonusTowerShield", "Max Dex Bonus (Tower Shield)", Defensive),
        ("Displacement", "Displacement", Defensive),
        ("Incorporeality", "Incorporeality", Defensive),
        ("DodgeBypass", "Dodge Bypass", Martial),
        ("MissileDeflection", "Missile Deflection", Defensive),
        ("ThreatBonusSpell", "Spell Threat Generation", Other),
        ("ThreatBonusRanged", "Ranged Threat Generation", Other),
        ("RuneArmChargeRate", "Rune Arm Charge Rate", Other),
        ("PointBlankShotRange", "Point Blank Shot Range", Martial),
        ("SpellPointCostPercent", "Spell Point Cost Reduction", Magical),
        ("FatePoint", "Fate Points", Other),
        ("ExtraTurns", "Turn Undead Uses", Other),
        ("TurnLevelBonus", "Turn Undead Level", Other),
        ("TurnDiceBonus", "Turn Undead Dice", Other),
        ("TurnMaxDice", "Turn Undead Max Dice", Other),
        ("TurnBonus", "Turn Undead Bonus", Other),
        ("ExtraLayOnHands", "Lay on Hands Uses", Other),
        ("KiHit", "Ki on Hit", Other),
        ("KiCritical", "Ki on Critical", Other),
        ("EldritchBlastD8", "Eldritch Blast d8 Dice", Magical),
        ("MetamagicCostEmpower", "Empower Cost Reduction", Magical),
        ("MetamagicCostMaximize", "Maximize Cost Reduction", Magical),
        ("MetamagicCostQuicken", "Quicken Cost Reduction", Magical),
        ("MetamagicCostEnlarge", "Enlarge Cost Reduction", Magical),
        ("MetamagicCostHeighten", "Heighten Cost Reduction", Magical),
        ("MetamagicCostExtend", "Extend Cost Reduction", Magical),
        ("MetamagicCostEmpowerHealing", "Empower Healing Cost Reduction", Magical),
        ("MetamagicCostIntensify", "Intensify Cost Reduction", Magical),
        ("MetamagicCostAccelerate", "Accelerate Cost Reduction", Magical),
        ("MetamagicCostEschewMaterials", "Eschew Materials Cost Reduction", Magical),
        ("MetamagicCostEmbolden", "Embolden Cost Reduction", Magical),
        ("SongDuration", "Song Duration", Other),
        ("SongACBonus", "Song Armor Class", Other),
        ("SongDodgeBonus", "Song Dodge", Other),
        ("SongSaveBonus", "Song Saving Throws", Other),
        ("SongSkillBonus", "Song Skills", Other),
        ("SongPRR", "Song Physical Resistance Rating", Other),
        ("SongUniversalSpellPower", "Song Universal Spell Power", Other),
        ("SongHealingAmp", "Song Healing Amplification", Other),
        ("HirelingAbilityBonus", "Hireling Abilities", Other),
        ("HirelingDodge", "Hireling Dodge", Other),
        ("HirelingFortification", "Hireling Fortification", Other),
        ("HirelingHitpoints", "Hireling Hit Points", Other),
        ("HirelingMRR", "Hireling Magical Resistance Rating", Other),
        ("HirelingMeleePower", "Hireling Melee Power", Other),
        ("HirelingPRR", "Hireling Physical Resistance Rating", Other),
        ("HirelingRangedPower", "Hireling Ranged Power", Other),
        ("HirelingSpellPower", "Hireling Spell Power", Other),
    ];
    let map = EffectMap::load().unwrap();
    let failures: Vec<String> = cases
        .iter()
        .filter_map(|&(kind, name, category)| {
            let derived = map.derive(&simple_effect(kind)).unwrap();
            match derived.as_slice() {
                [d] if d.stat.name == name && d.stat.category == category && d.value == 3 => None,
                other => Some(format!("{kind}: expected {name} ({category:?}), got {other:?}")),
            }
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} effect types unresolved:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
    assert!(map.unmapped_types().is_empty(), "{:?}", map.unmapped_types());

    let vector =
        Effect { amount_type: Some("TotalLevel".to_string()), amounts: vec![1.0, 2.0], ..simple_effect("FatePoint") };
    assert!(
        map.derive(&vector).unwrap().is_empty(),
        "a vector amount stays modifier-only even when the type is mapped"
    );
}

#[test]
fn every_stat_named_in_the_effect_map_exists() {
    #[derive(serde::Deserialize)]
    struct Sections {
        fixed: BTreeMap<String, String>,
        by_item_default: BTreeMap<String, String>,
    }
    let sections: Sections = toml::from_str(include_str!("../data/effect_map.toml")).unwrap();
    let missing: Vec<_> = sections
        .fixed
        .iter()
        .chain(&sections.by_item_default)
        .filter(|(_, stat)| ddo_model::stat_by_name(stat).is_none())
        .collect();
    assert!(missing.is_empty(), "stats missing from STATS: {missing:?}");
    assert!(EffectMap::load().is_ok(), "the load-time validation agrees");
}
