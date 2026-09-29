use ddo_etl::map::augment_slot::AugmentSlotType;
use ddo_etl::map::bonus_type::parse_buff_bonus_type;
use ddo_etl::map::buff::{BuffMap, ResolvedBuff};
use ddo_etl::map::effect::EffectMap;
use ddo_etl::map::placement::{placement_of, Placement};
use ddo_etl::xml::effect::Effect;
use ddo_etl::xml::items::{Buff, EquipmentSlotTag, EquipmentSlots};
use ddo_model::enums::{BonusType, EquipmentSlot, Handedness, ItemCategory, StatCategory};
use std::collections::BTreeMap;

fn equipment_slots(tags: &[EquipmentSlotTag]) -> EquipmentSlots {
    EquipmentSlots { tags: tags.to_vec() }
}

fn buff(kind: &str, target: Option<&str>, value: Option<i64>, bonus_type: Option<&str>) -> Buff {
    Buff {
        kind: kind.to_string(),
        target: target.map(str::to_string),
        second_target: None,
        value,
        second_value: None,
        bonus_type: bonus_type.map(str::to_string),
        description: None,
    }
}

#[test]
fn bonus_types_normalise_onto_ours() {
    assert_eq!(parse_buff_bonus_type("Enhancement").unwrap(), Some(BonusType::Enhancement));
    assert_eq!(parse_buff_bonus_type("Weapon Enchantment").unwrap(), Some(BonusType::Enhancement));
    assert_eq!(parse_buff_bonus_type("Armor Enhancement").unwrap(), Some(BonusType::Enhancement));
    assert_eq!(parse_buff_bonus_type("Insightful").unwrap(), Some(BonusType::Insight));
    assert_eq!(parse_buff_bonus_type("resistance").unwrap(), Some(BonusType::Resistance));
    assert_eq!(parse_buff_bonus_type("Vitality").unwrap(), Some(BonusType::Vitality));
    assert_eq!(parse_buff_bonus_type("Not Set").unwrap(), None);
    assert_eq!(parse_buff_bonus_type("").unwrap(), None);
    assert!(parse_buff_bonus_type("Bogus").is_err(), "an unknown bonus type is a parse error");
}

#[test]
fn placement_from_slot_tags_and_weapon_type() {
    let one_handed = placement_of(
        &equipment_slots(&[EquipmentSlotTag::Weapon1, EquipmentSlotTag::Weapon2]),
        Some("Longsword"),
        None,
    )
    .unwrap();
    assert_eq!(
        one_handed,
        Some(Placement {
            equipment_slot: EquipmentSlot::MainHand,
            category: ItemCategory::Weapon,
            handedness: Some(Handedness::OneHanded),
            item_type: Some("Longsword".into()),
        })
    );
    let bow = placement_of(&equipment_slots(&[EquipmentSlotTag::Weapon1]), Some("Longbow"), None).unwrap().unwrap();
    assert_eq!((bow.equipment_slot, bow.handedness), (EquipmentSlot::MainHand, Some(Handedness::TwoHanded)));
    let dart =
        placement_of(&equipment_slots(&[EquipmentSlotTag::Weapon1, EquipmentSlotTag::Weapon2]), Some("Dart"), None)
            .unwrap()
            .unwrap();
    assert_eq!(dart.handedness, Some(Handedness::Thrown));
    let shield =
        placement_of(&equipment_slots(&[EquipmentSlotTag::Weapon2]), Some("Large Shield"), None).unwrap().unwrap();
    assert_eq!(
        (shield.equipment_slot, shield.category, shield.handedness),
        (EquipmentSlot::OffHand, ItemCategory::Shield, Some(Handedness::OffHand))
    );
    let rune_arm =
        placement_of(&equipment_slots(&[EquipmentSlotTag::Weapon2]), Some("RuneArm"), None).unwrap().unwrap();
    assert_eq!((rune_arm.equipment_slot, rune_arm.item_type.as_deref()), (EquipmentSlot::Runearm, Some("Rune Arm")));

    let docent = placement_of(&equipment_slots(&[EquipmentSlotTag::Armor]), None, Some("Docent")).unwrap().unwrap();
    assert_eq!(
        (docent.equipment_slot, docent.category, docent.item_type.as_deref()),
        (EquipmentSlot::Body, ItemCategory::Armor, Some("Docent"))
    );
    let gloves = placement_of(&equipment_slots(&[EquipmentSlotTag::Gloves]), None, None).unwrap().unwrap();
    assert_eq!((gloves.equipment_slot, gloves.category), (EquipmentSlot::Hands, ItemCategory::Clothing));
    let ring = placement_of(&equipment_slots(&[EquipmentSlotTag::Ring, EquipmentSlotTag::Trinket]), None, None)
        .unwrap()
        .unwrap();
    assert_eq!((ring.equipment_slot, ring.category), (EquipmentSlot::Ring, ItemCategory::Jewelry));

    assert_eq!(
        placement_of(&equipment_slots(&[EquipmentSlotTag::CosmeticHelm]), None, None).unwrap(),
        None,
        "cosmetic-only items are excluded"
    );
    assert_eq!(placement_of(&equipment_slots(&[]), None, None).unwrap(), None);
    assert!(
        placement_of(&equipment_slots(&[EquipmentSlotTag::Weapon1]), Some("Lightsaber"), None).is_err(),
        "unknown weapon type is an error"
    );
}

#[test]
fn buffs_resolve_to_enhancement_bonus_stat_or_effect() {
    let map = BuffMap::load().unwrap();

    assert_eq!(
        map.resolved(&buff("WeaponEnchantment", None, Some(7), Some("Weapon Enchantment"))).unwrap(),
        ResolvedBuff::EnhancementBonus(7)
    );
    assert_eq!(
        map.resolved(&buff("ArmorEnchantment", None, Some(4), Some("Armor Enhancement"))).unwrap(),
        ResolvedBuff::EnhancementBonus(4)
    );

    match map.resolved(&buff("AbilityBonus", Some("Strength"), Some(8), Some("Enhancement"))).unwrap() {
        ResolvedBuff::Bonus { stat, bonus_type, value, second_value } => {
            assert_eq!(stat.name, "Strength");
            assert_eq!(bonus_type, Some(BonusType::Enhancement));
            assert_eq!((value, second_value), (Some(8), None));
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
        match map.resolved(&b).unwrap() {
            ResolvedBuff::Bonus { stat, .. } => assert_eq!(stat.name, expected, "{}", b.kind),
            other => panic!("{}: {other:?}", b.kind),
        }
    }

    match map.resolved(&buff("Sovereign Vorpal", Some("All"), None, None)).unwrap() {
        ResolvedBuff::Effect { name, value, target } => {
            assert_eq!(name, "Sovereign Vorpal");
            assert_eq!(value, None);
            assert_eq!(target.as_deref(), Some("All"));
        }
        other => panic!("{other:?}"),
    }
    match map.resolved(&buff("Lifesealed", None, Some(34), Some("Enhancement"))).unwrap() {
        ResolvedBuff::Effect { value, .. } => assert_eq!(value, Some(34)),
        other => panic!("{other:?}"),
    }
    assert!(
        map.resolved(&buff("AbilityBonus", Some("Luck"), Some(1), None)).is_err(),
        "a by-item template that resolves to no stat is an error, not a silent effect"
    );
}

#[test]
fn bonus_descriptions_fill_the_display_template() {
    let map = BuffMap::load().unwrap();
    let text = map.description(
        "%b1 %i1 %v1: Passive: %v1 %b1 bonus to %i1.",
        &buff("AbilityBonus", Some("Strength"), Some(8), Some("Enhancement")),
    );
    assert_eq!(text, "Enhancement Strength 8: Passive: 8 Enhancement bonus to Strength.");
}

#[test]
fn augment_slot_types_decode_into_family_variant_qualifier() {
    assert_eq!(
        AugmentSlotType::parse("Red"),
        AugmentSlotType { label: "red".into(), family: "standard".into(), variant: "red".into(), qualifier: None }
    );
    assert_eq!(AugmentSlotType::parse("Colorless").label, "colorless");
    assert_eq!(AugmentSlotType::parse("Sun").family, "standard");
    assert_eq!(
        AugmentSlotType::parse("Melancholic Slot (Accessory)"),
        AugmentSlotType {
            label: "lamordia: melancholic (accessory)".into(),
            family: "lamordia".into(),
            variant: "melancholic".into(),
            qualifier: Some("accessory".into())
        }
    );
    assert_eq!(
        AugmentSlotType::parse("IoD: Weapon: Scale Slot"),
        AugmentSlotType {
            label: "isle of dread: scale (weapon)".into(),
            family: "dino".into(),
            variant: "scale".into(),
            qualifier: Some("weapon".into())
        }
    );
    assert_eq!(AugmentSlotType::parse("IoD: Set Bonus Slot").label, "isle of dread: set bonus");
    assert_eq!(
        AugmentSlotType::parse("Tier 2"),
        AugmentSlotType {
            label: "upgrade: tier 2".into(),
            family: "upgrade".into(),
            variant: "tier 2".into(),
            qualifier: None
        }
    );
    assert_eq!(AugmentSlotType::parse("Greensteel Weapon Tier 1").family, "crafting");
    assert_eq!(AugmentSlotType::parse("Greensteel Weapon Tier 1").label, "crafting: greensteel weapon tier 1");
}

fn simple_effect(effect_type: &str) -> Effect {
    Effect {
        types: vec![effect_type.to_string()],
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
            let derived_bonuses = map.derive_bonuses(&simple_effect(kind)).unwrap();
            match derived_bonuses.as_slice() {
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
    assert!(map.unmapped_type_counts().is_empty(), "{:?}", map.unmapped_type_counts());

    let vector_amount_effect =
        Effect { amount_type: Some("TotalLevel".to_string()), amounts: vec![1.0, 2.0], ..simple_effect("FatePoint") };
    assert!(
        map.derive_bonuses(&vector_amount_effect).unwrap().is_empty(),
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
        .filter(|(_, stat)| ddo_model::stats::Stat::by_name(stat).is_none())
        .collect();
    assert!(missing.is_empty(), "stats missing from STATS: {missing:?}");
    assert!(EffectMap::load().is_ok(), "the load-time validation agrees");
}

#[test]
fn engine_only_effect_types_are_not_reported_as_unmapped() {
    let map = EffectMap::load().unwrap();
    for effect_type in ["SkillBonusAbility", "DR", "Weapon_BaseDamage", "SpellCostReduction"] {
        assert!(map.derive_bonuses(&simple_effect(effect_type)).unwrap().is_empty(), "{effect_type} derives no bonus");
    }
    assert!(map.derive_bonuses(&simple_effect("NotAnEffectType")).unwrap().is_empty());
    let unmapped = map.unmapped_type_counts();
    assert_eq!(unmapped.keys().collect::<Vec<_>>(), vec!["NotAnEffectType"], "{unmapped:?}");
}

#[test]
fn an_effect_type_cannot_be_both_mapped_and_engine_only() {
    let effect_map_toml = |extra_fixed: &str| {
        format!(
            "[fixed]\n{extra_fixed}\n[by_item]\n[by_item_default]\n[item_aliases]\n[bonus_type_aliases]\n[engine_only]\nDR = \"typed by bypass material\"\n"
        )
    };
    assert!(EffectMap::from_toml(&effect_map_toml("PRR = \"Physical Resistance Rating\"")).is_ok());
    let error =
        EffectMap::from_toml(&effect_map_toml("DR = \"Physical Resistance Rating\"")).err().expect("overlap rejected");
    assert!(error.to_string().contains("DR"), "{error}");
}
