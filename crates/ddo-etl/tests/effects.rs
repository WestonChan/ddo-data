use ddo_etl::map::effect::{DerivedBonus, EffectResolver};
use ddo_etl::map::effect_map::{EffectTargetQualifiers, EFFECT_MAP};
use ddo_etl::xml::effect::{parse_effect, Effect};
use ddo_etl::xml::requirements::parse_requirements;
use ddo_model::enums::{BonusType, RequirementGroupKind};

fn parsed_effect(xml: &str) -> Effect {
    parse_effect(xml).expect("effect parses")
}

#[test]
fn parses_a_simple_effect() {
    let effect = parsed_effect(
        r#"<Effect>
            <Type>SpellPower</Type>
            <Bonus>Equipment</Bonus>
            <Item>Fire</Item>
            <AType>Simple</AType>
            <Amount size="1">152</Amount>
        </Effect>"#,
    );
    assert_eq!(effect.types, vec!["SpellPower"]);
    assert_eq!(effect.bonus.as_deref(), Some("Equipment"));
    assert_eq!(effect.amount_type.as_deref(), Some("Simple"));
    assert_eq!(effect.amounts, vec![152.0]);
    assert_eq!(effect.targets, vec!["Fire"]);
    assert!(effect.dice.is_none());
    assert!(!effect.applies_as_item_effect);
}

#[test]
fn fixed_stat_effects_with_class_targets_do_not_become_generic_bonuses() {
    let qualifiers = EffectTargetQualifiers::from_vocabularies(["Sorcerer".to_string()], &[]);
    let resolver = EffectResolver::new().with_qualifiers(qualifiers);
    let class_effect = parsed_effect(
        "<Effect><Type>CasterLevel</Type><Bonus>Equipment</Bonus><AType>Simple</AType><Amount size=\"1\">2</Amount><Item>Sorcerer</Item></Effect>",
    );
    assert!(resolver.derive_bonuses(&class_effect).unwrap().is_empty());
    let all_effect = parsed_effect(
        "<Effect><Type>CasterLevel</Type><Bonus>Equipment</Bonus><AType>Simple</AType><Amount size=\"1\">2</Amount><Item>All</Item></Effect>",
    );
    assert_eq!(resolver.derive_bonuses(&all_effect).unwrap().len(), 1);
}

#[test]
fn fixed_effect_target_vocabulary_is_shared_by_both_mapping_paths() {
    let spell = ddo_etl::xml::spells::Spell {
        schools: vec!["Evocation".to_string()],
        primers: vec!["Crimsonite".to_string()],
        ..Default::default()
    };
    let qualifiers = EffectTargetQualifiers::from_vocabularies(["Sorcerer".to_string()], &[spell]);
    let resolver = EffectResolver::new().with_qualifiers(qualifiers.clone());
    for (effect_type, target, maps) in [
        ("DodgeBonus", "Strength", true),
        ("DodgeBonus", "Wind", true),
        ("DodgeBonus", "Electric", true),
        ("UniversalSpellPower", "Elemental Evil", true),
        ("UniversalSpellPower", "Acid", true),
        ("UniversalSpellPower", "Fire", true),
        ("UniversalSpellPower", "Arcane", false),
        ("KiPassive", "Strength", true),
        ("KiPassive", "Fire", true),
        ("CasterLevel", "Sorcerer", false),
        ("CasterLevel", "Arcane", false),
        ("CasterLevel", "Evocation", false),
        ("CasterLevel", "Crimsonite", false),
        ("CasterLevel", "Fire", false),
        ("CasterLevel", "Positive", false),
        ("MaxCasterLevel", "Positive", false),
        ("SpellPenetrationBonus", "Negative", false),
        ("SpellPower", "Fire", true),
        ("SpellLore", "Fire", true),
        ("EnergyResistance", "Fire", true),
        ("EnergyAbsorbance", "Fire", true),
        ("Weapon_Alacrity", "Kama", false),
        ("Weapon_Alacrity", "Handwraps", false),
        ("Weapon_Alacrity", "Sickle", false),
    ] {
        let effect = parsed_effect(&format!(
            "<Effect><Type>{effect_type}</Type><Bonus>Equipment</Bonus><AType>Simple</AType><Amount size=\"1\">4</Amount><Item>{target}</Item></Effect>"
        ));
        assert_eq!(resolver.derive_bonuses(&effect).unwrap().is_empty(), !maps, "{effect_type} {target}");
        assert_eq!(EFFECT_MAP.stats_for_effect(&effect, &qualifiers).is_some(), maps, "{effect_type} {target}");
    }
    let paired_weapons = parsed_effect(
        "<Effect><Type>Weapon_CriticalRange</Type><Bonus>Equipment</Bonus><AType>Simple</AType><Amount size=\"1\">1</Amount><Item>Shortbow</Item><Item>Longbow</Item></Effect>",
    );
    assert_eq!(resolver.qualified_targets(&paired_weapons).as_deref(), Some("Shortbow, Longbow"));
}

#[test]
fn by_item_effect_maps_only_when_every_target_names_a_stat() {
    let resolver = EffectResolver::new();
    let qualifiers = EffectTargetQualifiers::default();
    for (targets, expected_stats) in [
        ("<Item>Repair</Item>", vec!["Repair Spell Lore"]),
        ("<Item>Repair</Item><Item>Rust</Item>", vec!["Repair Spell Lore"]),
        ("<Item>Rust</Item>", vec![]),
        ("<Item>Repair</Item><Item>Uncatalogued</Item>", vec![]),
    ] {
        let effect = parsed_effect(&format!(
            "<Effect><Type>SpellLore</Type><Bonus>Equipment</Bonus><AType>Simple</AType><Amount size=\"1\">2122</Amount>{targets}</Effect>"
        ));
        let mapped_stats = EFFECT_MAP
            .stats_for_effect(&effect, &qualifiers)
            .map(|stats| stats.into_iter().map(|stat| stat.name).collect::<Vec<_>>())
            .unwrap_or_default();
        assert_eq!(mapped_stats, expected_stats);
        let derived_stats =
            resolver.derive_bonuses(&effect).unwrap().into_iter().map(|bonus| bonus.stat.name).collect::<Vec<_>>();
        assert_eq!(derived_stats, expected_stats);
    }
}

#[test]
fn real_companion_targets_preserve_their_mapped_siblings() {
    let resolver = EffectResolver::new();
    for (xml, expected_stats, expected_value) in [
        (
            "<Effect><Type>SpellPower</Type><Bonus>Equipment</Bonus><Item>Force</Item><Item>Physical</Item><Item>Untyped</Item><AType>Simple</AType><Amount size=\"1\">152</Amount></Effect>",
            vec!["Force Spell Power"],
            152,
        ),
        (
            "<Effect><Type>TacticalDC</Type><Bonus>Not Set</Bonus><AType>Simple</AType><Amount size=\"1\">1</Amount><Item>Trip</Item><Item>Sunder</Item><Item>Stun</Item><Item>General</Item><Item>Assassinate</Item></Effect>",
            vec!["Trip DC", "Sunder DC", "Stun DC", "Tactics", "Assassinate DC"],
            1,
        ),
        (
            "<Effect><Type>TacticalDC</Type><Bonus>Equipment</Bonus><AType>Simple</AType><Amount size=\"1\">4</Amount><Item>Vertigo</Item></Effect>",
            vec!["Trip DC"],
            4,
        ),
    ] {
        let effect = parsed_effect(xml);
        let stat_names: Vec<_> = resolver
            .derive_bonuses(&effect)
            .unwrap()
            .into_iter()
            .map(|bonus| {
                assert_eq!(bonus.value, expected_value);
                bonus.stat.name
            })
            .collect();
        assert_eq!(stat_names, expected_stats);
        let mapped_stat_names: Vec<_> = EFFECT_MAP
            .stats_for_effect(&effect, &EffectTargetQualifiers::default())
            .unwrap()
            .into_iter()
            .map(|stat| stat.name)
            .collect();
        assert_eq!(mapped_stat_names, expected_stats);
    }
}

#[test]
fn positive_caster_level_targets_remain_qualified_text() {
    let resolver = EffectResolver::new();
    for (effect_type, amount) in [("CasterLevel", 2), ("MaxCasterLevel", 1)] {
        let effect = parsed_effect(&format!(
            "<Effect><Type>{effect_type}</Type><Bonus>Equipment</Bonus><AType>Simple</AType><Amount size=\"1\">{amount}</Amount><Item>Positive</Item></Effect>"
        ));
        assert_eq!(resolver.qualified_targets(&effect).as_deref(), Some("Positive"));
        assert!(resolver.derive_bonuses(&effect).unwrap().is_empty());
    }
}

#[test]
fn epic_elemental_evil_and_bracers_of_wind_effects_keep_their_stats() {
    let resolver = EffectResolver::new();
    let epic_elemental_evil = parsed_effect(
        "<Effect><Type>UniversalSpellPower</Type><Bonus>Quality</Bonus><AType>Simple</AType><Amount size=\"1\">20</Amount><Item>Acid</Item></Effect>",
    );
    let universal = resolver.derive_bonuses(&epic_elemental_evil).unwrap();
    assert_eq!(universal.len(), 1);
    assert_eq!(
        (universal[0].stat.name, universal[0].bonus_type, universal[0].value),
        ("Universal Spell Power", Some(BonusType::Quality), 20)
    );

    for amount in [7, 8, 9] {
        let bracers_of_wind_option = parsed_effect(&format!(
            "<Effect><Type>DodgeBonus</Type><Bonus>Enhancement</Bonus><AType>Simple</AType><Amount size=\"1\">{amount}</Amount><Item>Electric</Item></Effect>"
        ));
        let dodge = resolver.derive_bonuses(&bracers_of_wind_option).unwrap();
        assert_eq!(dodge.len(), 1);
        assert_eq!(
            (dodge[0].stat.name, dodge[0].bonus_type, dodge[0].value),
            ("Dodge", Some(BonusType::Enhancement), amount)
        );
    }
}

#[test]
fn parses_vectors_multiple_types_dice_and_flags() {
    let effect = parsed_effect(
        r#"<Effect>
            <DisplayName>Thunderforged Base Damage</DisplayName>
            <Type>MeleePower</Type>
            <Type>Doublestrike</Type>
            <Bonus>Base</Bonus>
            <Amount size="4">3.5 4.0 4.5 4.5</Amount>
            <Item>All</Item>
            <IsItemSpecific />
            <AType>Stacks</AType>
            <Dice>
                <Number size="1">6</Number>
                <Sides size="1">10</Sides>
                <Damage>Bane</Damage>
            </Dice>
            <Percent />
            <Rare />
            <ApplyAsItemEffect />
            <Rank>4</Rank>
            <Cap>100</Cap>
            <StackSource>Dexterity</StackSource>
            <Value>Byeshk</Value>
            <Requirements>
                <Requirement>
                    <Type>EnemyType</Type>
                    <Item>Goblinoid</Item>
                    <Item>Orc</Item>
                </Requirement>
                <RequiresOneOf>
                    <Requirement><Type>Stance</Type><Item>Power Attack</Item></Requirement>
                    <Requirement><Type>Feat</Type><Item>Cleave</Item><Value>2</Value></Requirement>
                </RequiresOneOf>
            </Requirements>
        </Effect>"#,
    );
    assert_eq!(effect.types, vec!["MeleePower", "Doublestrike"]);
    assert_eq!(effect.amounts, vec![3.5, 4.0, 4.5, 4.5]);
    assert_eq!(effect.display_name.as_deref(), Some("Thunderforged Base Damage"));
    let dice = effect.dice.as_ref().unwrap();
    assert_eq!(
        (dice.counts.clone(), dice.sides.clone(), dice.damage.as_deref()),
        (vec![6.0], vec![10.0], Some("Bane"))
    );
    assert!(effect.is_percent && effect.is_rare && effect.applies_as_item_effect && effect.is_item_specific);
    assert_eq!(effect.rank, Some(4));
    assert_eq!(effect.cap.as_deref(), Some("100"));
    assert_eq!(effect.stack_source.as_deref(), Some("Dexterity"));
    assert_eq!(effect.value.as_deref(), Some("Byeshk"));
    let requirements = effect.requirements.as_ref().unwrap();
    assert_eq!(requirements.groups.len(), 2);
    assert_eq!(requirements.groups[0].kind, RequirementGroupKind::All);
    assert_eq!(requirements.groups[0].requirements[0].kind, "EnemyType");
    assert_eq!(requirements.groups[0].requirements[0].items, vec!["Goblinoid", "Orc"]);
    assert_eq!(requirements.groups[1].kind, RequirementGroupKind::OneOf);
    assert_eq!(requirements.groups[1].requirements[1].value.as_deref(), Some("2"));
}

#[test]
fn parses_requirement_blocks_in_order() {
    let requirements = parse_requirements(
        r#"<Requirements>
            <RequiresOneOf>
                <Requirement><Type>Race</Type><Item>Warforged</Item></Requirement>
                <Requirement><Type>Race</Type><Item>Bladeforged</Item></Requirement>
            </RequiresOneOf>
            <RequiresNoneOf>
                <Requirement><Type>Feat</Type><Item>Druidic Oath</Item></Requirement>
            </RequiresNoneOf>
            <RequiresNoneOf>
                <DisplayDescription>Not with Mithral Body</DisplayDescription>
                <Requirement><Type>Feat</Type><Item>Mithral Body</Item></Requirement>
            </RequiresNoneOf>
            <Requirement><Type>Level</Type><Value>4</Value></Requirement>
        </Requirements>"#,
    )
    .unwrap();
    let kinds: Vec<_> = requirements.groups.iter().map(|g| g.kind).collect();
    assert_eq!(
        kinds,
        vec![
            RequirementGroupKind::OneOf,
            RequirementGroupKind::NoneOf,
            RequirementGroupKind::NoneOf,
            RequirementGroupKind::All
        ]
    );
    assert_eq!(requirements.groups[0].requirements.len(), 2);
    assert_eq!(requirements.groups[2].display_description.as_deref(), Some("Not with Mithral Body"));
    assert_eq!(requirements.groups[3].requirements[0].value.as_deref(), Some("4"));
}

fn simple_effect(effect_type: &str, bonus: &str, amount: f64, targets: &[&str]) -> Effect {
    let target_elements = targets.iter().map(|t| format!("<Item>{t}</Item>")).collect::<String>();
    parsed_effect(&format!(
        r#"<Effect><Type>{effect_type}</Type><Bonus>{bonus}</Bonus><AType>Simple</AType><Amount size="1">{amount}</Amount>{target_elements}</Effect>"#
    ))
}

#[test]
fn effect_resolver_derives_bonuses_from_simple_effects() {
    let map = EffectResolver::new();
    let derived = |effect: &Effect| map.derive_bonuses(effect).unwrap();

    let fire = derived(&simple_effect("SpellPower", "Equipment", 152.0, &["Fire"]));
    assert_eq!(fire.len(), 1);
    assert_eq!(
        fire[0],
        DerivedBonus {
            stat: ddo_model::stats::Stat::by_name("Fire Spell Power").unwrap(),
            bonus_type: Some(BonusType::Equipment),
            value: 152
        }
    );

    let cases: &[(Effect, &str)] = &[
        (simple_effect("SpellPower", "Equipment", 24.0, &["All"]), "Universal Spell Power"),
        (simple_effect("SpellPower", "Equipment", 24.0, &[]), "Universal Spell Power"),
        (simple_effect("SpellLore", "Equipment", 21.0, &["Fire"]), "Fire Spell Lore"),
        (simple_effect("SaveBonus", "Resistance", 4.0, &["All"]), "Saving Throws"),
        (simple_effect("SaveBonus", "Stacking", 2.0, &["Will"]), "Will Save"),
        (simple_effect("TacticalDC", "Enhancement", 3.0, &["Trip"]), "Trip DC"),
        (simple_effect("SpellDC", "Equipment", 2.0, &["Evocation"]), "Evocation Spell Focus"),
        (simple_effect("SpellDC", "Equipment", 2.0, &["All"]), "Spell DCs"),
        (simple_effect("PRR", "Stacking", 10.0, &[]), "Physical Resistance Rating"),
        (simple_effect("ACBonus", "Insightful", 1.0, &[]), "Armor Class"),
        (simple_effect("Hitpoints", "Vitality", 20.0, &[]), "Hit Points"),
        (simple_effect("SpellCriticalDamage", "Stacking", 5.0, &["Light/Alignment"]), "Light Spell Critical Damage"),
        (simple_effect("EnergyAbsorbance", "Enhancement", 20.0, &["Lawful"]), "Law Absorption"),
        (simple_effect("HealingAmplification", "Competence", 56.0, &[]), "Healing Amplification"),
        (simple_effect("Weapon_Attack", "Stacking", 2.0, &["All"]), "Attack Bonus"),
    ];
    for (effect, expected) in cases {
        let bonuses = derived(effect);
        assert_eq!(bonuses.len(), 1, "{}: {:?}", effect.types[0], bonuses);
        assert_eq!(bonuses[0].stat.name, *expected, "{}", effect.types[0]);
    }

    let two = derived(&simple_effect("SkillBonus", "Enhancement", 5.0, &["Hide", "Move Silently"]));
    assert_eq!(two.iter().map(|bonuses| bonuses.stat.name).collect::<Vec<_>>(), vec!["Hide", "Move Silently"]);

    assert!(derived(&parsed_effect(r#"<Effect><Type>Hitpoints</Type><Bonus>Feat</Bonus><AType>TotalLevel</AType><Amount size="3">3 4 5</Amount></Effect>"#)).is_empty());
    assert!(derived(&simple_effect("SpellLikeAbility", "Enhancement", 1.0, &["Force Shot"])).is_empty());
    assert!(derived(&simple_effect("UnclassifiedProbe", "Enhancement", 1.0, &[])).is_empty());
    assert!(
        derived(&simple_effect("Weapon_Attack", "Enhancement", 2.0, &["Maul"])).is_empty(),
        "per-weapon bonuses are not a stat"
    );
    assert!(derived(&parsed_effect(r#"<Effect><Type>MeleePower</Type><Type>Doublestrike</Type><Bonus>Stacking</Bonus><AType>Simple</AType><Amount size="1">5</Amount></Effect>"#)).is_empty(), "multi-type effects stay modifiers only");
    let unmapped = map.unmapped_type_counts();
    assert_eq!(unmapped.get("UnclassifiedProbe"), Some(&1), "unmapped types are counted, not errors: {unmapped:?}");
    assert!(!unmapped.contains_key("SpellLikeAbility"), "classified mechanics do not enter the unmapped list");
    assert!(!unmapped.contains_key("Weapon_Attack"), "a mapped type with an unmappable target is not 'unmapped'");
}

#[test]
fn effect_bonus_types_normalise_including_his_vocabulary() {
    let map = EffectResolver::new();
    assert_eq!(map.parse_bonus_type("Insightful").unwrap(), Some(BonusType::Insight));
    assert_eq!(map.parse_bonus_type("Feat").unwrap(), Some(BonusType::Feat));
    assert_eq!(map.parse_bonus_type("Not Set").unwrap(), None);
    assert_eq!(
        map.parse_bonus_type("Weapon Enchantment").unwrap(),
        Some(BonusType::WeaponEnchantment),
        "his own type, not folded into Enhancement here"
    );
    assert_eq!(map.parse_bonus_type("Armor Enhancement").unwrap(), Some(BonusType::ArmorEnhancement));
    assert_eq!(map.parse_bonus_type("Shield Enhancement").unwrap(), Some(BonusType::ShieldEnhancement));
    assert!(map.parse_bonus_type("Bogus").is_err());
}
