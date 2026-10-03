use ddo_etl::map::effect::{DerivedBonus, EffectResolver};
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
    assert!(
        derived(&simple_effect("Weapon_Attack", "Enhancement", 2.0, &["Maul"])).is_empty(),
        "per-weapon bonuses are not a stat"
    );
    assert!(derived(&parsed_effect(r#"<Effect><Type>MeleePower</Type><Type>Doublestrike</Type><Bonus>Stacking</Bonus><AType>Simple</AType><Amount size="1">5</Amount></Effect>"#)).is_empty(), "multi-type effects stay modifiers only");
    let unmapped = map.unmapped_type_counts();
    assert_eq!(unmapped.get("SpellLikeAbility"), Some(&1), "unmapped types are counted, not errors: {unmapped:?}");
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
