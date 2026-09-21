//! Parsing DDOBuilderV2 XML into Rust structs. Fixtures under `tests/fixtures/DataFiles` are
//! verbatim copies of upstream files (Items) or trimmed copies keeping the file shape (Quests,
//! ItemBuffs, Patrons).

use ddo_etl::xml::items::{parse_item_file, SlotTag};
use ddo_etl::xml::{item_buffs, patrons, quests};
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles")
}

fn item(name: &str) -> ddo_etl::xml::items::Item {
    let file = parse_item_file(&fixtures().join("Items").join(format!("{name}.item"))).expect(name);
    assert_eq!(file.items.len(), 1, "one <Item> per file");
    file.items.into_iter().next().unwrap()
}

#[test]
fn parses_a_named_weapon() {
    let it = item("Sireth, Spear of the Sky");
    assert_eq!(it.name, "Sireth, Spear of the Sky");
    assert_eq!(it.icon.as_deref(), Some("Quarterstaff_6a"));
    assert_eq!(it.min_level, Some(23));
    assert_eq!(it.equipment_slot.tags, vec![SlotTag::Weapon1]);
    assert_eq!(it.weapon.as_deref(), Some("Quarterstaff"));
    assert_eq!(it.attack_modifier, vec!["Strength"]);
    assert_eq!(it.dr_bypass, vec!["Good", "Magic", "Pierce", "Slash"]);
    assert_eq!(it.weapon_damage, Some(3.6));
    let dice = it.base_dice.as_ref().unwrap();
    assert_eq!((dice.number, dice.sides, dice.bonus), (Some(1), Some(10), None));
    assert_eq!(it.critical_multiplier, Some(2));
    assert_eq!(it.critical_threat_range, Some(5));
    assert_eq!(it.material.as_deref(), Some("Steel"));
    assert_eq!(it.drop_location.as_deref(), Some("Caught in the Web, End Chest"));
    assert!(it.accepts_sentience);
    assert!(!it.minor_artifact);

    assert_eq!(it.buffs.len(), 6);
    let first = &it.buffs[0];
    assert_eq!(first.kind, "WeaponEnchantment");
    assert_eq!(first.value1, Some(7));
    assert_eq!(first.bonus_type.as_deref(), Some("Weapon Enchantment"));
    let good = &it.buffs[1];
    assert_eq!(good.kind, "Supreme Good");
    assert_eq!(good.item.as_deref(), Some("All"));
    assert_eq!(good.value1, None);

    assert_eq!(it.augments.len(), 4);
    assert_eq!(it.augments[0].kind, "Attuned to Heroism 1");
    let preset = &it.augments[1].options[0];
    assert_eq!(preset.name, "+8 Enhancement Bonus");
    assert!(preset.description.starts_with("+8 Enhancement Bonus"));
    assert_eq!(it.augments[3].options[0].grant_augment, vec!["Red"]);
    let cloak = item("Legendary Cloak of Winter");
    assert!(cloak.augments[0].options.is_empty(), "an open Green socket has no fixed content");
}

#[test]
fn parses_armor_with_requirements_and_docent_fields() {
    let it = item("Docent of Defiance");
    assert_eq!(it.equipment_slot.tags, vec![SlotTag::Armor]);
    assert_eq!(it.armor.as_deref(), Some("Docent"));
    assert_eq!(it.armor_bonus, Some(-3));
    assert_eq!(it.mithral_body, Some(5));
    assert_eq!(it.adamantine_body, Some(12));
    let reqs = it.requirements.as_ref().unwrap();
    assert_eq!(reqs.groups.len(), 1);
    assert_eq!(reqs.groups[0].requirements[0].kind, "RaceConstruct");
    assert!(reqs.groups[0].requirements[0].items.is_empty());
    // Three EnergyResistance buffs carry their element in <Item>.
    let elements: Vec<_> =
        it.buffs.iter().filter(|b| b.kind == "EnergyResistance").map(|b| b.item.clone().unwrap()).collect();
    assert_eq!(elements, vec!["Fire", "Electric", "Cold"]);
}

#[test]
fn parses_the_two_slot_ring_and_value2() {
    let it = item("Five Rings");
    assert_eq!(it.equipment_slot.tags.len(), 2);
    let stalker = item("Epic Ring of the Stalker");
    let deception = stalker.buffs.iter().find(|b| b.kind == "Deception").unwrap();
    assert_eq!((deception.value1, deception.value2), (Some(3), Some(5)));
    assert_eq!(deception.bonus_type.as_deref(), Some("Insightful"));
    assert_eq!(stalker.set_bonus, Vec::<String>::new());
    let boots = item("Kundarak Delving Boots");
    assert_eq!(boots.set_bonus, vec!["Kundarak Delving Equipment"]);
}

#[test]
fn parses_every_fixture_item() {
    let dir = fixtures().join("Items");
    let mut n = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        parse_item_file(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        n += 1;
    }
    assert_eq!(n, 14);
}

#[test]
fn parses_interleaved_repeated_elements() {
    // Upstream's Acid Rune Arm lists <Buff>, <Effect>, <Buff>; element order must not matter.
    let it = item("Acid Rune Arm");
    assert_eq!(it.buffs.len(), 2, "{:?}", it.buffs.iter().map(|b| &b.kind).collect::<Vec<_>>());
    assert_eq!(it.weapon.as_deref(), Some("Rune Arm"));
    assert_eq!(it.augments.len(), 2);
}

#[test]
fn parses_quests_patrons_and_item_buffs() {
    let qs = quests::parse(&fixtures().join("Quests.xml")).unwrap();
    assert_eq!(qs.len(), 7);
    let chrono = qs.iter().find(|q| q.name == "The Chronoscope").unwrap();
    assert_eq!(chrono.levels, vec![6, 21]);
    assert!(chrono.is_raid);
    assert_eq!(chrono.patron.as_deref(), Some("The Coin Lords"));
    assert_eq!(chrono.adventure_pack.as_deref(), Some("Devil Assault"));
    assert_eq!(chrono.favor, Some(5));
    let lamordia = qs.iter().find(|q| q.name == "Land of Lamordia").unwrap();
    assert!(lamordia.do_not_show);
    assert!(!lamordia.is_raid);

    let ps = patrons::parse(&fixtures().join("Patrons.xml")).unwrap();
    assert!(ps.iter().any(|p| p.name == "House Cannith"));

    let buffs = item_buffs::parse(&fixtures().join("ItemBuffs.xml")).unwrap();
    assert_eq!(
        buffs.get("WeaponEnchantment").map(String::as_str),
        Some("%v1 Enhancement Bonus: %v1 Enhancement bonus to attack and damage rolls.")
    );
    assert!(buffs.contains_key("Supreme Good"));
    assert_eq!(
        buffs.get("Fixture Multi Paragraph").map(String::as_str),
        Some("First paragraph.\nSecond paragraph."),
        "repeated <DisplayText> elements are paragraphs of one description"
    );
}
