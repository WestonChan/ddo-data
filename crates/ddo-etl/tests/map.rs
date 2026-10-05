use ddo_etl::map::augment_slot::AugmentSlotType;
use ddo_etl::map::bonus_type::parse_buff_bonus_type;
use ddo_etl::map::buff::{AmountFrom, BuffResolutionSource, BuffResolver, FamilyResolution, ResolvedBuff};
use ddo_etl::map::effect::EffectResolver;
use ddo_etl::map::effect_map::{EffectMap, EffectTargetQualifiers, EFFECT_MAP};
use ddo_etl::map::item_version::names_legacy_version;
use ddo_etl::map::placement::{placement_of, CosmeticExclusion, Placement};
use ddo_etl::xml::effect::Effect;
use ddo_etl::xml::item_buffs::{self, ItemBuffDefinition};
use ddo_etl::xml::items::{parse_item_file, Buff, EquipmentSlotTag, EquipmentSlots};
use ddo_model::enums::{BonusType, EquipmentSlot, Handedness, ItemCategory, StatCategory};
use ddo_model::seeds::WEAPON_TYPES;
use std::collections::HashMap;
use std::path::Path;

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

fn fixture_item_buff_definitions() -> HashMap<String, ItemBuffDefinition> {
    item_buffs::parse(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles/ItemBuffs.xml")).unwrap()
}

fn effect_item_buff_definitions() -> HashMap<String, ItemBuffDefinition> {
    item_buffs::parse(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/effect_item_buffs.xml")).unwrap()
}

fn resolved_bonus_type(map: &BuffResolver, buff: &Buff) -> Option<BonusType> {
    match map.resolved(buff).unwrap() {
        ResolvedBuff::Bonuses { stats, .. } if stats.len() == 1 => stats[0].bonus_type,
        other => panic!("{}: {other:?}", buff.kind),
    }
}

#[test]
fn family_titles_keep_meaningful_punctuation_and_reject_generic_leads() {
    let cases = [
        ("RevelInBlood", "Revel in Blood (Slashing): %v1", "Revel in Blood (Slashing)"),
        ("ConstructFortification", "Construct Fortification (10%): %v1", "Construct Fortification (10%)"),
        ("DamageReduction", "DR 5/-: %v1", "DR 5/-"),
        ("VorpalLethargy", "On Strike: %v1", "Vorpal Lethargy"),
        ("WeaponEnchantmentBad", "-1 Enhancement Bonus: -%v1", "Weapon Enchantment Bad"),
        (
            "+2 vs Evil",
            "+2 vs Evil: Grants an additional +2 enhancement bonus to attack chance against evil creatures.",
            "Attack Bonus vs Evil",
        ),
        ("SetBonusExample", "Set Bonus: %v1", "Set Bonus Example"),
    ];
    for (kind, display_text, expected_name) in cases {
        let definitions = HashMap::from([(
            kind.to_string(),
            ItemBuffDefinition {
                display_text: display_text.to_string(),
                bonus_type_name: None,
                fixed_amount: None,
                effects: Vec::new(),
                has_activation_condition: false,
            },
        )]);
        let resolver = BuffResolver::from_definitions(&definitions);
        assert_eq!(resolver.family_name(&buff(kind, None, None, None), None), expected_name, "{kind}");
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
        Ok(Placement {
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
        Err(CosmeticExclusion::CosmeticOnlySlots),
        "cosmetic-only items are excluded"
    );
    assert_eq!(placement_of(&equipment_slots(&[]), None, None).unwrap(), Err(CosmeticExclusion::CosmeticOnlySlots));
    assert_eq!(
        placement_of(&equipment_slots(&[EquipmentSlotTag::Weapon2]), Some("Cosmetic Shield"), None).unwrap(),
        Err(CosmeticExclusion::CosmeticShield),
        "a cosmetic shield in the off-hand slot is excluded"
    );
    assert!(
        placement_of(&equipment_slots(&[EquipmentSlotTag::Weapon1]), Some("Lightsaber"), None).is_err(),
        "unknown weapon type is an error"
    );
}

#[test]
fn buffs_resolve_to_enhancement_bonus_stat_or_effect() {
    let map = BuffResolver::from_definitions(&fixture_item_buff_definitions());

    assert_eq!(
        map.resolved(&buff("WeaponEnchantment", None, Some(7), Some("Weapon Enchantment"))).unwrap(),
        ResolvedBuff::EnhancementBonus(7)
    );
    assert_eq!(
        map.resolved(&buff("ArmorEnchantment", None, Some(4), Some("Armor Enhancement"))).unwrap(),
        ResolvedBuff::EnhancementBonus(4)
    );

    match map.resolved(&buff("AbilityBonus", Some("Strength"), Some(8), Some("Enhancement"))).unwrap() {
        ResolvedBuff::Bonuses { source, stats } => {
            assert_eq!(source, BuffResolutionSource::Family);
            assert_eq!(stats.len(), 1);
            assert_eq!(stats[0].stat.name, "Strength");
            assert_eq!(stats[0].bonus_type, Some(BonusType::Enhancement));
            assert_eq!(stats[0].amount_from, AmountFrom::ItemValue1);
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
        (buff("Shatter", None, Some(7), Some("Insightful")), "Sunder DC"),
        (buff("Vertigo", None, Some(15), Some("Enhancement")), "Trip DC"),
        (buff("Sneak Attack", None, Some(5), Some("Enhancement")), "Sneak Attack"),
        (buff("Damage Bonus", None, Some(1), Some("Competence")), "Damage Bonus"),
        (buff("Alignment Absorption", None, Some(22), Some("Enhancement")), "Alignment Absorption"),
        (buff("Elemental Absorption", None, Some(19), None), "Elemental Absorption"),
        (buff("Illusion Save", None, Some(5), None), "Illusion Save"),
        (buff("Rune Arm Charge Rate", None, Some(5), Some("Enhancement")), "Rune Arm Charge Rate"),
        (buff("DarkRestorationLore", None, Some(23), Some("Equipment")), "Dark Restoration Lore"),
        (buff("EnchantmentSave", None, Some(6), Some("Resistance")), "Enchantment Save"),
        (buff("IllusionSave", None, Some(6), Some("resistance")), "Illusion Save"),
        (buff("MeleePower", None, Some(6), Some("Enhancement")), "Melee Power"),
        (buff("RangedPower", None, Some(6), Some("Enhancement")), "Ranged Power"),
        (buff("SacredGroundLore", None, Some(22), Some("Equipment")), "Sacred Ground Lore"),
        (buff("SpellLore", None, Some(3), Some("Equipment")), "Spell Lore"),
        (buff("SpellLore", Some("VI"), Some(6), Some("Equipment")), "Spell Lore"),
        (buff("SpellLore", Some("Repair"), Some(16), Some("Equipment")), "Repair Spell Lore"),
        (buff("Astute Skills Bonus", None, Some(9), Some("Exceptional")), "Astute"),
        (buff("Exceptional Nimble Skills", None, Some(10), Some("Exceptional")), "Nimble"),
        (buff("Insightful Nimble Skills", None, Some(3), Some("Insightful")), "Nimble"),
        (buff("Quality Nimble Skills", None, Some(2), Some("Quality")), "Nimble"),
        (buff("Exceptional Prudent Skills", None, Some(9), Some("Exceptional")), "Prudent"),
        (buff("CurseResistance", None, Some(5), Some("Resistance")), "Curse Save"),
        (buff("Breath Weapon Focus", None, Some(3), Some("Equipment")), "Breath Weapon Spell Focus"),
    ];
    for (b, expected) in cases {
        match map.resolved(&b).unwrap() {
            ResolvedBuff::Bonuses { stats, .. } => assert_eq!(stats[0].stat.name, expected, "{}", b.kind),
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
        ResolvedBuff::Bonuses { stats, .. } => {
            assert_eq!(stats.len(), 1);
            assert_eq!((stats[0].stat.name, stats[0].amount_from), ("Negative Absorption", AmountFrom::ItemValue1));
        }
        other => panic!("{other:?}"),
    }
    for item_type in ["Equipment", "Enhancement"] {
        assert_eq!(
            resolved_bonus_type(&map, &buff("Linguistics", None, Some(10), Some(item_type))),
            Some(BonusType::Equipment)
        );
    }
    for prose_only_buff in [
        buff("Tendon Slice", None, Some(6), None),
        buff("Persuasion", None, None, Some("")),
        buff("NegativeEnergyAbsorption", None, None, None),
    ] {
        match map.resolved(&prose_only_buff).unwrap() {
            ResolvedBuff::Effect { name, .. } => assert_eq!(name, prose_only_buff.kind),
            other => panic!("{}: {other:?}", prose_only_buff.kind),
        }
    }
    assert!(
        map.resolved(&buff("AbilityBonus", Some("Luck"), Some(1), None)).is_err(),
        "a by-item template that resolves to no stat is an error, not a silent effect"
    );
}

#[test]
fn an_untyped_buff_takes_the_bonus_type_its_definition_fixes() {
    let map = BuffResolver::from_definitions(&fixture_item_buff_definitions());
    let cases = [
        ("Elemental Absorption", Some(BonusType::Enhancement)),
        ("Illusion Save", Some(BonusType::Resistance)),
        ("Shield", Some(BonusType::Shield)),
        ("SpellcastingImplement", Some(BonusType::Implement)),
        ("Damage Bonus", None),
    ];
    for (buff_kind, expected_bonus_type) in cases {
        assert_eq!(
            resolved_bonus_type(&map, &buff(buff_kind, None, Some(2), None)),
            expected_bonus_type,
            "{buff_kind}"
        );
        assert_eq!(
            resolved_bonus_type(&map, &buff(buff_kind, None, Some(2), Some("Not Set"))),
            expected_bonus_type,
            "{buff_kind} with an explicit Not Set"
        );
    }
    let ResolvedBuff::Bonuses { stats, .. } =
        map.resolved(&buff("Command", None, Some(2), Some("Competence"))).unwrap()
    else {
        panic!("Command grants Charisma skills and a Hide penalty");
    };
    assert!(stats
        .iter()
        .filter(|stat| stat.stat.name != "Hide")
        .all(|stat| stat.bonus_type == Some(BonusType::Insight)));
}

#[test]
fn bonus_descriptions_fill_the_display_template() {
    let map = BuffResolver::from_definitions(&HashMap::new());
    let text = map.description(
        "%b1 %i1 %v1: Passive: %v1 %b1 bonus to %i1.",
        &buff("AbilityBonus", Some("Strength"), Some(8), Some("Enhancement")),
    );
    assert_eq!(text, "Enhancement Strength 8: Passive: 8 Enhancement bonus to Strength.");
}

#[test]
fn effect_fallback_uses_definition_amounts_and_item_bonus_types() {
    let definitions = effect_item_buff_definitions();
    let map = BuffResolver::from_definitions(&definitions);
    let cases = [
        ("Improved Deception", 13, "Insight", vec![("Bluff", 13, BonusType::Insight)]),
        ("ProofAgainstPoison", 7, "Enhancement", vec![("Poison Save", 7, BonusType::Enhancement)]),
        ("ProofAgainstDisease", 8, "Enhancement", vec![("Disease Save", 8, BonusType::Enhancement)]),
        ("NegativeAmplification", 17, "Profane", vec![("Negative Healing Amplification", 17, BonusType::Profane)]),
        (
            "Parrying",
            4,
            "Insightful",
            vec![("Armor Class", 4, BonusType::Insight), ("Saving Throws", 4, BonusType::Insight)],
        ),
    ];
    for (kind, amount, item_bonus_type, expected) in cases {
        let item_buff = buff(kind, None, Some(amount), Some(item_bonus_type));
        let resolved = map.resolved(&item_buff).unwrap();
        let ResolvedBuff::Bonuses { source: BuffResolutionSource::EffectFallback, stats } = resolved else {
            panic!("{kind}: {resolved:?}");
        };
        let actual: Vec<_> =
            stats.iter().map(|row| (row.stat.name, row.amount(&item_buff).unwrap(), row.bonus_type.unwrap())).collect();
        assert_eq!(actual, expected, "{kind}");
    }
    let riposte_without_item_value = buff("Riposte", None, None, Some("Insightful"));
    let ResolvedBuff::Bonuses { stats: default_stats, .. } = map.resolved(&riposte_without_item_value).unwrap() else {
        panic!("Riposte uses its definition amount when an item carries none");
    };
    assert!(default_stats.iter().all(|stat| stat.amount(&riposte_without_item_value) == Some(2)));
    let riposte_with_item_value = buff("Riposte", None, Some(5), Some("Insightful"));
    let ResolvedBuff::Bonuses { source: BuffResolutionSource::EffectFallback, stats } =
        map.resolved(&riposte_with_item_value).unwrap()
    else {
        panic!("Riposte with an item value must resolve through its effects");
    };
    assert_eq!(
        stats.iter().map(|row| (row.stat.name, row.amount_from)).collect::<Vec<_>>(),
        [("Armor Class", AmountFrom::ItemValue1), ("Saving Throws", AmountFrom::ItemValue1)]
    );
    assert!(stats.iter().all(|stat| stat.amount(&riposte_with_item_value) == Some(5)));
}

#[test]
fn speed_deception_and_command_grant_the_stats_their_definitions_describe() {
    let resolver = BuffResolver::from_definitions(&fixture_item_buff_definitions());
    let cases = [
        (
            "Speed",
            "Enhancement",
            vec![
                ("Movement Speed", AmountFrom::ItemValue1, BonusType::Enhancement),
                ("Melee Attack Speed", AmountFrom::ItemValue2, BonusType::Enhancement),
                ("Ranged Attack Speed", AmountFrom::ItemValue2, BonusType::Enhancement),
            ],
        ),
        (
            "Deception",
            "Insightful",
            vec![
                ("Sneak Attack Hit", AmountFrom::ItemValue1, BonusType::Insight),
                ("Sneak Attack Damage", AmountFrom::ItemValue2, BonusType::Insight),
            ],
        ),
        (
            "Command",
            "Insightful",
            vec![
                ("Bluff", AmountFrom::ItemValue1, BonusType::Insight),
                ("Diplomacy", AmountFrom::ItemValue1, BonusType::Insight),
                ("Haggle", AmountFrom::ItemValue1, BonusType::Insight),
                ("Intimidate", AmountFrom::ItemValue1, BonusType::Insight),
                ("Perform", AmountFrom::ItemValue1, BonusType::Insight),
                ("Use Magic Device", AmountFrom::ItemValue1, BonusType::Insight),
                ("Hide", AmountFrom::ItemValue2, BonusType::Penalty),
            ],
        ),
    ];
    for (kind, item_type, expected) in cases {
        let mut item_buff = buff(kind, None, Some(10), Some(item_type));
        item_buff.second_value = Some(4);
        let ResolvedBuff::Bonuses { source: BuffResolutionSource::EffectFallback, stats } =
            resolver.resolved(&item_buff).unwrap()
        else {
            panic!("{kind} should resolve through definition effects");
        };
        let actual: Vec<_> =
            stats.iter().map(|row| (row.stat.name, row.amount_from, row.bonus_type.unwrap())).collect();
        assert_eq!(actual, expected, "{kind}");
    }
}

#[test]
fn attack_speed_effect_types_resolve_all_and_class_targets() {
    let resolver = EffectResolver::new();
    for (effect_type, target, expected) in [
        ("Weapon_Alacrity", "All", vec!["Melee Attack Speed", "Ranged Attack Speed"]),
        ("WeaponAlacrityClass", "Melee", vec!["Melee Attack Speed"]),
        ("WeaponAlacrityClass", "Ranged", vec!["Ranged Attack Speed"]),
    ] {
        let effect = Effect { targets: vec![target.into()], ..simple_effect(effect_type) };
        let actual: Vec<_> = resolver.derive_bonuses(&effect).unwrap().iter().map(|bonus| bonus.stat.name).collect();
        assert_eq!(actual, expected, "{effect_type} {target}");
    }
    for (weapon, expected) in [("Longbow", "Ranged Attack Speed"), ("Kama", "Melee Attack Speed")] {
        let effect = Effect { targets: vec![weapon.into()], ..simple_effect("Weapon_Alacrity") };
        assert!(resolver.derive_bonuses(&effect).unwrap().is_empty(), "{weapon} stays qualified text");
        assert_eq!(resolver.qualified_targeted_name(&effect), Some(format!("{expected} ({weapon})")));
    }
    let definitions = effect_item_buff_definitions();
    let buff_resolver = BuffResolver::from_definitions(&definitions);
    assert!(buff_resolver
        .family_template(&buff("RangedAlacrity", None, Some(15), Some("Enhancement")))
        .starts_with("Ranged Alacrity %v1%"));
    for (kind, expected) in [
        ("RangedAlacrity", vec!["Ranged Attack Speed"]),
        ("RangedSpeedRomanNumeralXV", vec!["Movement Speed", "Ranged Attack Speed"]),
    ] {
        let FamilyResolution::EffectFallback(stats) = buff_resolver.family_resolution(kind).unwrap() else {
            panic!("{kind} should map its complete ranged weapon list");
        };
        assert_eq!(stats.iter().map(|row| row.stat.name).collect::<Vec<_>>(), expected);
    }
    let mut complete_with_extra_thrown = definitions["RangedAlacrity"].effects[0].clone();
    complete_with_extra_thrown.targets.push("Shuriken".into());
    assert_eq!(
        resolver
            .derive_bonuses(&complete_with_extra_thrown)
            .unwrap()
            .iter()
            .map(|bonus| bonus.stat.name)
            .collect::<Vec<_>>(),
        ["Ranged Attack Speed"]
    );
    let all_melee: Vec<String> = WEAPON_TYPES
        .iter()
        .filter(|weapon| weapon.proficiency.is_some() && !weapon.is_ranged() && !weapon.is_thrown())
        .map(|weapon| weapon.name.to_string())
        .collect();
    let melee_class = Effect { targets: all_melee.clone(), ..simple_effect("Weapon_Alacrity") };
    assert_eq!(
        resolver.derive_bonuses(&melee_class).unwrap().iter().map(|bonus| bonus.stat.name).collect::<Vec<_>>(),
        ["Melee Attack Speed"]
    );
    let partial_melee = Effect { targets: all_melee.into_iter().skip(1).collect(), ..melee_class };
    assert!(resolver.derive_bonuses(&partial_melee).unwrap().is_empty());
    let short_list = Effect { targets: vec!["Dart".into(), "Longbow".into()], ..simple_effect("Weapon_Alacrity") };
    assert!(resolver.derive_bonuses(&short_list).unwrap().is_empty());
    assert_eq!(resolver.qualified_targeted_name(&short_list), Some("Ranged Attack Speed (Dart, Longbow)".into()));
    let stacks = Effect {
        amount_type: Some("Stacks".into()),
        amounts: vec![7.5, 10.0, 12.5, 15.0],
        targets: vec!["Melee".into(), "Thrown".into()],
        ..simple_effect("WeaponAlacrityClass")
    };
    assert!(resolver.derive_bonuses(&stacks).unwrap().is_empty());
    assert_eq!(resolver.qualified_targeted_name(&stacks), Some("Melee and Ranged Attack Speed (Melee, Thrown)".into()));
}

#[test]
fn mapped_effects_survive_engine_only_siblings_and_keep_definition_defaults() {
    let map = BuffResolver::from_definitions(&effect_item_buff_definitions());
    let ghostly = buff("Ghostly", None, None, None);
    match map.resolved(&ghostly).unwrap() {
        ResolvedBuff::Bonuses { stats, .. } => {
            let names_and_amounts: Vec<_> = stats.iter().map(|stat| (stat.stat.name, stat.amount(&ghostly))).collect();
            assert_eq!(
                names_and_amounts,
                [("Incorporeality", Some(10)), ("Hide", Some(5)), ("Move Silently", Some(5))]
            );
        }
        other => panic!("{other:?}"),
    }
    for (item_value, expected) in [(None, 2), (Some(9), 9), (Some(13), 13)] {
        let finesse = buff("Finesse", None, item_value, None);
        match map.resolved(&finesse).unwrap() {
            ResolvedBuff::Bonuses { stats, .. } => {
                assert_eq!(stats.len(), 1);
                assert_eq!((stats[0].stat.name, stats[0].amount(&finesse)), ("Dexterity", Some(expected)));
                assert_eq!(stats[0].amount_from, AmountFrom::ItemValue1);
            }
            other => panic!("{other:?}"),
        }
    }
    let boost = buff("ActionBoostEnhancement", None, None, None);
    match map.resolved(&boost).unwrap() {
        ResolvedBuff::Bonuses { stats, .. } => {
            assert_eq!(stats.len(), 1);
            assert_eq!((stats[0].stat.name, stats[0].amount_from), ("Action Boosts", AmountFrom::Constant(3)));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(map.family_name(&boost, Some("Action Boosts")), "Action Boost Enhancement");
    assert_eq!(map.family_name(&buff("Finesse", None, Some(9), None), Some("Dexterity")), "Finesse");
    assert_eq!(map.family_name(&ghostly, Some("Incorporeality")), "Ghostly");
}

#[test]
fn effect_amount_sources_follow_definition_text_and_item_values() {
    let definitions = effect_item_buff_definitions();
    let resolver = BuffResolver::from_definitions(&definitions);
    let items = parse_item_file(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/effect_real_items.item"))
        .unwrap()
        .items;
    let cases = [
        ("Legendary Amethyst Loupe", "Improved Deception", vec![("Bluff", AmountFrom::ItemValue1, Some(20))]),
        (
            "Epic Ethereal Bracers",
            "Riposte",
            vec![("Armor Class", AmountFrom::ItemValue1, Some(5)), ("Saving Throws", AmountFrom::ItemValue1, Some(5))],
        ),
        ("Blooming Gauntlets", "Undying", vec![("Unconsciousness Range", AmountFrom::ItemValue1, Some(50))]),
        (
            "Epic Ethereal Bracers",
            "SpeedRomanNumeral",
            vec![
                ("Movement Speed", AmountFrom::ItemValue1, Some(30)),
                ("Melee Attack Speed", AmountFrom::ItemValue2, Some(14)),
                ("Ranged Attack Speed", AmountFrom::ItemValue2, Some(14)),
            ],
        ),
        (
            "Legendary Storm",
            "Thunderstorm Lore",
            vec![
                ("Electric Spell Lore", AmountFrom::ItemValue1, Some(22)),
                ("Sonic Spell Lore", AmountFrom::ItemValue1, Some(22)),
            ],
        ),
        (
            "Legendary Crown of Ioun",
            "FireAndColdAbsorption",
            vec![
                ("Fire Absorption", AmountFrom::ItemValue1, Some(40)),
                ("Cold Absorption", AmountFrom::ItemValue1, Some(40)),
            ],
        ),
        (
            "Legendary Crabshell Buckler",
            "Chitinous Covering",
            vec![("Fire Absorption", AmountFrom::ItemValue1, Some(34))],
        ),
        (
            "Legendary Fancy Flayer's Cape",
            "Charisma Skills - Exceptional",
            vec![
                ("Bluff", AmountFrom::ItemValue1, Some(6)),
                ("Diplomacy", AmountFrom::ItemValue1, Some(6)),
                ("Haggle", AmountFrom::ItemValue1, Some(6)),
                ("Intimidate", AmountFrom::ItemValue1, Some(6)),
                ("Perform", AmountFrom::ItemValue1, Some(6)),
                ("Use Magic Device", AmountFrom::ItemValue1, Some(6)),
            ],
        ),
    ];
    for (item_name, kind, expected) in cases {
        assert!(definitions[kind].display_text.contains("%v"), "{kind}");
        let item = items.iter().find(|item| item.name == item_name).unwrap();
        let item_buff = item.buffs.iter().find(|item_buff| item_buff.kind == kind).unwrap();
        let ResolvedBuff::Bonuses { stats, .. } = resolver.resolved(item_buff).unwrap() else {
            panic!("{kind} did not resolve to bonuses");
        };
        let actual: Vec<_> =
            stats.iter().map(|stat| (stat.stat.name, stat.amount_from, stat.amount(item_buff))).collect();
        assert_eq!(actual, expected, "{kind}");
    }
}

#[test]
fn definition_bonus_type_is_fixed_unless_display_text_has_an_item_type_slot() {
    let definitions = effect_item_buff_definitions();
    let resolver = BuffResolver::from_definitions(&definitions);
    let items = parse_item_file(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/effect_real_items.item"))
        .unwrap()
        .items;
    let alaric = items.iter().find(|item| item.name == "Alaric's Grim Gauntlets").unwrap();
    for (kind, stat_name, expected_type, has_item_type_slot) in [
        ("AbilityBonus", "Wisdom", BonusType::Enhancement, true),
        ("DarkRestorationLore", "Dark Restoration Lore", BonusType::Equipment, true),
        ("Hallowed", "Turn Undead Max Dice", BonusType::Enhancement, false),
        ("Sacred", "Turn Undead Level", BonusType::Enhancement, false),
    ] {
        assert_eq!(definitions[kind].display_text.contains("%b1"), has_item_type_slot, "{kind}");
        let item_buff = alaric.buffs.iter().find(|buff| buff.kind == kind).unwrap();
        let ResolvedBuff::Bonuses { stats, .. } = resolver.resolved(item_buff).unwrap() else {
            panic!("{kind} did not resolve to bonuses");
        };
        assert_eq!((stats[0].stat.name, stats[0].bonus_type), (stat_name, Some(expected_type)), "{kind}");
    }
    let bracers = items.iter().find(|item| item.name == "Epic Ethereal Bracers").unwrap();
    let riposte = bracers.buffs.iter().find(|buff| buff.kind == "Riposte").unwrap();
    assert!(definitions["Riposte"].display_text.contains("%b1"));
    let ResolvedBuff::Bonuses { stats, .. } = resolver.resolved(riposte).unwrap() else {
        panic!("Riposte did not resolve to bonuses");
    };
    assert!(stats.iter().all(|stat| stat.bonus_type == Some(BonusType::Insight)));
    let crest = items.iter().find(|item| item.name == "Shining Crest of St. Markus").unwrap();
    let silver_flame = crest.buffs.iter().find(|buff| buff.kind == "Power of the Silver Flame").unwrap();
    assert!(definitions["Power of the Silver Flame"].display_text.contains("%b1"));
    let ResolvedBuff::Bonuses { stats, .. } = resolver.resolved(silver_flame).unwrap() else {
        panic!("Power of the Silver Flame did not resolve to bonuses");
    };
    assert!(stats.iter().all(|stat| stat.bonus_type == Some(BonusType::Enhancement)));
    let cape = items.iter().find(|item| item.name == "Legendary Fancy Flayer's Cape").unwrap();
    let charisma_skills = cape.buffs.iter().find(|buff| buff.kind == "Charisma Skills - Exceptional").unwrap();
    assert!(!definitions["Charisma Skills - Exceptional"].display_text.contains("%b1"));
    let ResolvedBuff::Bonuses { stats, .. } = resolver.resolved(charisma_skills).unwrap() else {
        panic!("Charisma Skills - Exceptional did not resolve to bonuses");
    };
    assert!(stats.iter().all(|stat| stat.bonus_type == Some(BonusType::Competence)));
}

#[test]
fn fixed_definition_type_beats_an_items_default_type() {
    let resolver = BuffResolver::from_definitions(&effect_item_buff_definitions());
    for (kind, stat_name, bonus_type) in
        [("Invisibility", "Armor Class", BonusType::Deflection), ("Unwieldy", "Dexterity", BonusType::Penalty)]
    {
        let buff_resolution = resolver.resolved(&buff(kind, None, Some(2), Some("Enhancement"))).unwrap();
        let ResolvedBuff::Bonuses { stats, .. } = buff_resolution else { panic!("{kind} did not grant a bonus") };
        assert_eq!(stats.len(), 1, "{kind}");
        assert_eq!((stats[0].stat.name, stats[0].bonus_type), (stat_name, Some(bonus_type)), "{kind}");
    }
    let resolver = BuffResolver::from_definitions(&fixture_item_buff_definitions());
    assert_eq!(
        resolved_bonus_type(&resolver, &buff("IllusionSave", None, Some(3), Some("Insight"))),
        Some(BonusType::Insight)
    );
    assert_eq!(
        resolved_bonus_type(&resolver, &buff("Illusion Save", None, Some(3), Some("Enhancement"))),
        Some(BonusType::Resistance)
    );
}

#[test]
fn family_mapping_precedes_effect_fallback_and_unmappable_definitions_stay_effects() {
    let map = BuffResolver::from_definitions(&effect_item_buff_definitions());
    for kind in ["Hallowed", "Sacred"] {
        assert!(
            matches!(
                map.resolved(&buff(kind, None, Some(4), Some("Enhancement"))).unwrap(),
                ResolvedBuff::Bonuses { source: BuffResolutionSource::Family, .. }
            ),
            "{kind}"
        );
    }
    for kind in [
        "Shield Bashing",
        "Vorpal",
        "Constitution Poison, Lesser",
        "Mind Drain",
        "Fixture Variable Armor Class",
        "Mark of Healing",
    ] {
        assert!(
            matches!(
                map.resolved(&buff(kind, None, Some(4), Some("Enhancement"))).unwrap(),
                ResolvedBuff::Effect { .. }
            ),
            "{kind}"
        );
    }
    assert!(matches!(
        map.resolved(&buff("MeleeAlacrity", None, Some(4), Some("Enhancement"))).unwrap(),
        ResolvedBuff::Bonuses { source: BuffResolutionSource::EffectFallback, .. }
    ));
    assert!(matches!(
        map.resolved(&buff("RepairLore", None, Some(4), Some("Equipment"))).unwrap(),
        ResolvedBuff::Bonuses { source: BuffResolutionSource::EffectFallback, .. }
    ));
    assert!(matches!(
        map.resolved(&buff("RepairLore", None, Some(2122), Some("Equipment"))).unwrap(),
        ResolvedBuff::Bonuses { source: BuffResolutionSource::EffectFallback, .. }
    ));
    let silent_moves = buff("Silent Moves", None, None, None);
    let FamilyResolution::EffectFallback(stats) = map.family_resolution("Silent Moves").unwrap() else {
        panic!("Silent Moves has an effect fallback plan");
    };
    assert_eq!(stats[0].amount_from, AmountFrom::ItemValue1);
    assert_eq!(stats[0].amount(&silent_moves), None);
    assert!(matches!(map.resolved(&silent_moves).unwrap(), ResolvedBuff::Bonuses { .. }));
}

#[test]
fn fixed_effects_with_specific_targets_stay_family_effects() {
    let definitions = effect_item_buff_definitions();
    let qualifiers = EffectTargetQualifiers::from_vocabularies(["Sorcerer".to_string()], &[]);
    let resolver = BuffResolver::from_definitions(&definitions).with_qualifiers(qualifiers);
    let items = parse_item_file(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/effect_real_items.item"))
        .unwrap()
        .items;
    let kind = "Arcane Augmentation IX";
    let definition = &definitions[kind];
    assert!(definition
        .effects
        .iter()
        .any(|effect| { effect.types == ["CasterLevel"] && effect.targets.iter().any(|target| target == "Sorcerer") }));
    let item = items.iter().find(|item| item.name == "Creeping Dust Conduit").unwrap();
    let item_buff = item.buffs.iter().find(|buff| buff.kind == kind).unwrap();
    assert!(matches!(resolver.family_resolution(kind).unwrap(), FamilyResolution::Effect), "{kind}");
    assert!(matches!(resolver.resolved(item_buff).unwrap(), ResolvedBuff::Effect { .. }), "{kind}");
    for kind in ["Arcane Augmentation I", "Memory of Shattered Life"] {
        assert!(matches!(resolver.family_resolution(kind).unwrap(), FamilyResolution::Effect), "{kind}");
    }
    assert!(matches!(resolver.family_resolution("SpeedRomanNumeral").unwrap(), FamilyResolution::EffectFallback(_)));
}

#[test]
fn an_untyped_item_keeps_the_family_plan_and_needs_a_per_item_correction() {
    let definitions = effect_item_buff_definitions();
    let typed_poison = buff("ProofAgainstPoison", None, Some(7), Some("Enhancement"));
    let untyped_poison = buff("ProofAgainstPoison", None, Some(6), None);
    let resolver = BuffResolver::from_definitions(&definitions);
    assert!(matches!(resolver.family_resolution("ProofAgainstPoison").unwrap(), FamilyResolution::EffectFallback(_)));
    assert!(matches!(
        resolver.family_resolutions().unwrap()["ProofAgainstPoison"],
        FamilyResolution::EffectFallback(_)
    ));
    assert_eq!(resolved_bonus_type(&resolver, &typed_poison), Some(BonusType::Enhancement));
    assert_eq!(resolved_bonus_type(&resolver, &untyped_poison), None);
}

#[test]
fn a_missing_placeholder_value_affects_only_its_item() {
    let definitions = effect_item_buff_definitions();
    let valued = buff("Silent Moves", None, Some(5), Some("Competence"));
    let missing = buff("Silent Moves", None, None, Some("Competence"));
    let resolver = BuffResolver::from_definitions(&definitions);
    assert!(matches!(resolver.family_resolution("Silent Moves").unwrap(), FamilyResolution::EffectFallback(_)));
    assert!(matches!(resolver.resolved(&valued).unwrap(), ResolvedBuff::Bonuses { .. }));
    assert!(matches!(resolver.resolved(&missing).unwrap(), ResolvedBuff::Bonuses { .. }));
}

#[test]
fn merged_effect_map_rejects_unknown_stat_template_alias_and_conflicting_type() {
    let source = include_str!("../data/effect_map.toml");
    let invalid_cases = [
        (source.replace("Hallowed = \"Turn Undead Max Dice\"", "Hallowed = \"Unknown Stat\""), "Hallowed"),
        (source.replace("AbilityBonus = \"{item}\"", "AbilityBonus = \"{item} Unknown Stat\""), "AbilityBonus"),
        (source.replace("Lawful = \"Law\"", "Lawful = \"Unknown Word\""), "Lawful"),
        (source.replace("Insightful = \"Insight\"", "Insightful = \"Unknown Bonus\""), "Insightful"),
        (source.replace("RuneArm = \"Rune Arm\"", "RuneArm = \"Unknown Weapon\""), "RuneArm"),
        (source.replace("GhostTouch = \"mechanic flag\"", "PRR = \"mechanic flag\""), "PRR"),
        (
            source.replace("PRR = \"Physical Resistance Rating\"", "SpellPower = \"Physical Resistance Rating\""),
            "SpellPower",
        ),
        (
            source.replace("Hallowed = \"Turn Undead Max Dice\"", "WeaponEnchantment = \"Turn Undead Max Dice\""),
            "WeaponEnchantment",
        ),
    ];
    for (invalid_source, entry) in invalid_cases {
        let error = EffectMap::from_toml(&invalid_source).expect_err("invalid map rejected");
        assert!(error.to_string().contains(entry), "{entry}: {error}");
    }
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
fn skill_ability_effects_and_item_buffs_grant_the_same_curated_groups() {
    let effect_resolver = EffectResolver::new();
    let buff_resolver = BuffResolver::from_definitions(&HashMap::new());
    for (ability, expected) in [
        ("Strength", &["Jump", "Swim"][..]),
        ("Dexterity", &["Balance", "Hide", "Move Silently", "Open Lock", "Tumble"]),
        ("Constitution", &["Concentration"]),
        ("Intelligence", &["Disable Device", "Repair", "Search", "Spellcraft"]),
        ("Wisdom", &["Heal", "Listen", "Spot"]),
        ("Charisma", &["Bluff", "Diplomacy", "Haggle", "Intimidate", "Perform", "Use Magic Device"]),
    ] {
        let mut effect = simple_effect("SkillBonusAbility");
        effect.targets = vec![ability.to_string()];
        let derived: Vec<_> =
            effect_resolver.derive_bonuses(&effect).unwrap().iter().map(|bonus| bonus.stat.name).collect();
        assert_eq!(derived, expected, "{ability} effect");
        let ResolvedBuff::Bonuses { stats, .. } =
            buff_resolver.resolved(&buff("SkillBonusAbility", Some(ability), Some(3), Some("Enhancement"))).unwrap()
        else {
            panic!("{ability} item buff did not resolve");
        };
        let resolved: Vec<_> = stats.iter().map(|bonus| bonus.stat.name).collect();
        assert_eq!(resolved, expected, "{ability} item buff");
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
        ("ExtraRage", "Rage Uses", Other),
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
    let map = EffectResolver::new();
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
    let missing: Vec<_> = EFFECT_MAP
        .effect
        .fixed
        .iter()
        .chain(&EFFECT_MAP.effect.by_item_default)
        .filter(|(_, stat)| ddo_model::stats::Stat::by_name(stat).is_none())
        .collect();
    assert!(missing.is_empty(), "stats missing from STATS: {missing:?}");
    assert!(EffectMap::load().is_ok(), "the load-time validation agrees");
}

#[test]
fn engine_only_effect_types_are_not_reported_as_unmapped() {
    let map = EffectResolver::new();
    for effect_type in
        ["SkillBonusAbility", "DR", "Weapon_BaseDamage", "SpellCostReduction", "GrantFeat", "Immunity", "ItemClickie"]
    {
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
            "[family]\nenhancement = []\n[family.fixed]\n[family.by_item]\n[effect.fixed]\n{extra_fixed}\n[effect.targeted]\n[effect.by_item]\n[effect.by_item_default]\n[effect.companion_targets]\nwords = []\n[effect.energy_target_artifacts]\nstats = []\n[effect.engine_only]\nDR = \"typed by bypass material\"\n[item_aliases]\n[bonus_type_aliases]\n[weapon_aliases]\n"
        )
    };
    assert!(EffectMap::from_toml(&effect_map_toml("PRR = \"Physical Resistance Rating\"")).is_ok());
    let error =
        EffectMap::from_toml(&effect_map_toml("DR = \"Physical Resistance Rating\"")).expect_err("overlap rejected");
    assert!(error.to_string().contains("DR"), "{error}");
}

#[test]
fn legacy_and_historic_versions_are_named_by_a_parenthesised_word() {
    for legacy_name in [
        "Brawling Gloves (legacy) (level 12)",
        "Allegiance (historic)",
        "Allegiance (Historic)",
        "Glass Cannon (LEGACY) (tier 1)",
    ] {
        assert!(names_legacy_version(legacy_name), "{legacy_name}");
    }
    for current_name in ["Allegiance", "Legacy of Lorikk", "Historic Ring", "Brawling Gloves (level 12)"] {
        assert!(!names_legacy_version(current_name), "{current_name}");
    }
}
