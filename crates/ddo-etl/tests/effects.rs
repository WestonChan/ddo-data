//! The two grammars every upstream family shares: `<Effect>` and `<Requirements>`, plus the map
//! from effect vocabulary onto our stats.

use ddo_etl::map::effect::{Derived, EffectMap};
use ddo_etl::xml::effect::{parse_effect, Effect};
use ddo_etl::xml::requirements::{parse_requirements, RequirementGroupKind};
use ddo_model::enums::BonusType;

fn effect(xml: &str) -> Effect {
    parse_effect(xml).expect("effect parses")
}

#[test]
fn parses_a_simple_effect() {
    let e = effect(
        r#"<Effect>
            <Type>SpellPower</Type>
            <Bonus>Equipment</Bonus>
            <Item>Fire</Item>
            <AType>Simple</AType>
            <Amount size="1">152</Amount>
        </Effect>"#,
    );
    assert_eq!(e.types, vec!["SpellPower"]);
    assert_eq!(e.bonus.as_deref(), Some("Equipment"));
    assert_eq!(e.amount_type.as_deref(), Some("Simple"));
    assert_eq!(e.amounts, vec![152.0]);
    assert_eq!(e.items, vec!["Fire"]);
    assert!(e.dice.is_none());
    assert!(!e.apply_as_item_effect);
}

#[test]
fn parses_vectors_multiple_types_dice_and_flags() {
    let e = effect(
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
    assert_eq!(e.types, vec!["MeleePower", "Doublestrike"]);
    assert_eq!(e.amounts, vec![3.5, 4.0, 4.5, 4.5]);
    assert_eq!(e.display_name.as_deref(), Some("Thunderforged Base Damage"));
    let dice = e.dice.as_ref().unwrap();
    assert_eq!(
        (dice.number.clone(), dice.sides.clone(), dice.damage.as_deref()),
        (vec![6.0], vec![10.0], Some("Bane"))
    );
    assert!(e.percent && e.rare && e.apply_as_item_effect && e.is_item_specific);
    assert_eq!(e.rank, Some(4));
    assert_eq!(e.cap.as_deref(), Some("100"));
    assert_eq!(e.stack_source.as_deref(), Some("Dexterity"));
    assert_eq!(e.value.as_deref(), Some("Byeshk"));
    let reqs = e.requirements.as_ref().unwrap();
    assert_eq!(reqs.groups.len(), 2);
    assert_eq!(reqs.groups[0].kind, RequirementGroupKind::All);
    assert_eq!(reqs.groups[0].requirements[0].kind, "EnemyType");
    assert_eq!(reqs.groups[0].requirements[0].items, vec!["Goblinoid", "Orc"]);
    assert_eq!(reqs.groups[1].kind, RequirementGroupKind::OneOf);
    assert_eq!(reqs.groups[1].requirements[1].value.as_deref(), Some("2"));
}

#[test]
fn parses_requirement_blocks_in_order() {
    let reqs = parse_requirements(
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
    let kinds: Vec<_> = reqs.groups.iter().map(|g| g.kind).collect();
    assert_eq!(
        kinds,
        vec![
            RequirementGroupKind::OneOf,
            RequirementGroupKind::NoneOf,
            RequirementGroupKind::NoneOf,
            RequirementGroupKind::All
        ]
    );
    assert_eq!(reqs.groups[0].requirements.len(), 2);
    assert_eq!(reqs.groups[2].display_description.as_deref(), Some("Not with Mithral Body"));
    assert_eq!(reqs.groups[3].requirements[0].value.as_deref(), Some("4"));
}

fn simple(kind: &str, bonus: &str, amount: f64, items: &[&str]) -> Effect {
    let items = items.iter().map(|i| format!("<Item>{i}</Item>")).collect::<String>();
    effect(&format!(
        r#"<Effect><Type>{kind}</Type><Bonus>{bonus}</Bonus><AType>Simple</AType><Amount size="1">{amount}</Amount>{items}</Effect>"#
    ))
}

#[test]
fn effect_map_derives_bonuses_from_simple_effects() {
    let map = EffectMap::load().unwrap();
    let derived = |e: &Effect| map.derive(e).unwrap();

    let fire = derived(&simple("SpellPower", "Equipment", 152.0, &["Fire"]));
    assert_eq!(fire.len(), 1);
    assert_eq!(
        fire[0],
        Derived {
            stat: ddo_model::stat_by_name("Fire Spell Power").unwrap(),
            bonus_type: Some(BonusType::Equipment),
            value: 152
        }
    );

    let cases: &[(Effect, &str)] = &[
        (simple("SpellPower", "Equipment", 24.0, &["All"]), "Universal Spell Power"),
        (simple("SpellPower", "Equipment", 24.0, &[]), "Universal Spell Power"),
        (simple("SpellLore", "Equipment", 21.0, &["Fire"]), "Fire Spell Lore"),
        (simple("SaveBonus", "Resistance", 4.0, &["All"]), "Saving Throws"),
        (simple("SaveBonus", "Stacking", 2.0, &["Will"]), "Will Save"),
        (simple("TacticalDC", "Enhancement", 3.0, &["Trip"]), "Trip DC"),
        (simple("SpellDC", "Equipment", 2.0, &["Evocation"]), "Evocation Spell Focus"),
        (simple("SpellDC", "Equipment", 2.0, &["All"]), "Spell DCs"),
        (simple("PRR", "Stacking", 10.0, &[]), "Physical Resistance Rating"),
        (simple("ACBonus", "Insightful", 1.0, &[]), "Armor Class"),
        (simple("Hitpoints", "Vitality", 20.0, &[]), "Hit Points"),
        (simple("SpellCriticalDamage", "Stacking", 5.0, &["Light/Alignment"]), "Light Spell Critical Damage"),
        (simple("EnergyAbsorbance", "Enhancement", 20.0, &["Lawful"]), "Law Absorption"),
        (simple("HealingAmplification", "Competence", 56.0, &[]), "Healing Amplification"),
        (simple("Weapon_Attack", "Stacking", 2.0, &["All"]), "Attack Bonus"),
    ];
    for (e, expected) in cases {
        let d = derived(e);
        assert_eq!(d.len(), 1, "{}: {:?}", e.types[0], d);
        assert_eq!(d[0].stat.name, *expected, "{}", e.types[0]);
    }

    // One bonus per target.
    let two = derived(&simple("SkillBonus", "Enhancement", 5.0, &["Hide", "Move Silently"]));
    assert_eq!(two.iter().map(|d| d.stat.name).collect::<Vec<_>>(), vec!["Hide", "Move Silently"]);

    // Not a plain number on a stat: no bonus rows, and no error.
    assert!(derived(&effect(r#"<Effect><Type>Hitpoints</Type><Bonus>Feat</Bonus><AType>TotalLevel</AType><Amount size="3">3 4 5</Amount></Effect>"#)).is_empty());
    assert!(derived(&simple("SpellLikeAbility", "Enhancement", 1.0, &["Force Shot"])).is_empty());
    assert!(
        derived(&simple("Weapon_Attack", "Enhancement", 2.0, &["Maul"])).is_empty(),
        "per-weapon bonuses are not a stat"
    );
    assert!(derived(&effect(r#"<Effect><Type>MeleePower</Type><Type>Doublestrike</Type><Bonus>Stacking</Bonus><AType>Simple</AType><Amount size="1">5</Amount></Effect>"#)).is_empty(), "multi-type effects stay modifiers only");
    let unmapped = map.unmapped_types();
    assert_eq!(unmapped.get("SpellLikeAbility"), Some(&1), "unmapped types are counted, not errors: {unmapped:?}");
    assert!(!unmapped.contains_key("Weapon_Attack"), "a mapped type with an unmappable target is not 'unmapped'");
}

#[test]
fn effect_bonus_types_normalise_including_his_vocabulary() {
    let map = EffectMap::load().unwrap();
    assert_eq!(map.bonus_type("Insightful").unwrap(), Some(BonusType::Insight));
    assert_eq!(map.bonus_type("Feat").unwrap(), Some(BonusType::Feat));
    assert_eq!(map.bonus_type("Not Set").unwrap(), None);
    assert_eq!(
        map.bonus_type("Weapon Enchantment").unwrap(),
        Some(BonusType::WeaponEnchantment),
        "his own type, not folded into Enhancement here"
    );
    assert!(map.bonus_type("Bogus").is_err());
}
