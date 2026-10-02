use ddo_etl::xml::items::{parse_item_file, EquipmentSlotTag};
use ddo_etl::xml::{item_buffs, patrons, quests};
use std::path::PathBuf;

fn data_files_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles")
}

fn parsed_item(name: &str) -> ddo_etl::xml::items::Item {
    let file = parse_item_file(&data_files_fixture_dir().join("Items").join(format!("{name}.item"))).expect(name);
    assert_eq!(file.items.len(), 1, "one <Item> per file");
    file.items.into_iter().next().unwrap()
}

#[test]
fn parses_a_named_weapon() {
    let item = parsed_item("Sireth, Spear of the Sky");
    assert_eq!(item.name, "Sireth, Spear of the Sky");
    assert_eq!(item.icon.as_deref(), Some("Quarterstaff_6a"));
    assert_eq!(item.minimum_level, Some(23));
    assert_eq!(item.equipment_slots.tags, vec![EquipmentSlotTag::Weapon1]);
    assert_eq!(item.weapon.as_deref(), Some("Quarterstaff"));
    assert_eq!(item.attack_modifiers, vec!["Strength"]);
    assert_eq!(item.dr_bypasses, vec!["Good", "Magic", "Pierce", "Slash"]);
    assert_eq!(item.damage_multiplier, Some(3.6));
    let dice = item.base_dice.as_ref().unwrap();
    assert_eq!((dice.count, dice.sides, dice.bonus), (Some(1), Some(10), None));
    assert_eq!(item.critical_multiplier, Some(2));
    assert_eq!(item.critical_threat_range, Some(5));
    assert_eq!(item.material.as_deref(), Some("Steel"));
    assert_eq!(item.drop_location.as_deref(), Some("Caught in the Web, End Chest"));
    assert!(item.accepts_sentience);
    assert!(!item.is_minor_artifact);

    assert_eq!(item.buffs.len(), 6);
    let first = &item.buffs[0];
    assert_eq!(first.kind, "WeaponEnchantment");
    assert_eq!(first.value, Some(7));
    assert_eq!(first.bonus_type.as_deref(), Some("Weapon Enchantment"));
    let good = &item.buffs[1];
    assert_eq!(good.kind, "Supreme Good");
    assert_eq!(good.target.as_deref(), Some("All"));
    assert_eq!(good.value, None);

    assert_eq!(item.augment_slots.len(), 4);
    assert_eq!(item.augment_slots[0].kind, "Attuned to Heroism 1");
    let preset = &item.augment_slots[1].options[0];
    assert_eq!(preset.name, "+8 Enhancement Bonus");
    assert!(preset.description.starts_with("+8 Enhancement Bonus"));
    assert_eq!(item.augment_slots[3].options[0].granted_augments, vec!["Red"]);
    let cloak = parsed_item("Legendary Cloak of Winter");
    assert!(cloak.augment_slots[0].options.is_empty(), "an open Green socket has no fixed content");
}

#[test]
fn parses_armor_with_requirements_and_docent_fields() {
    let item = parsed_item("Docent of Defiance");
    assert_eq!(item.equipment_slots.tags, vec![EquipmentSlotTag::Armor]);
    assert_eq!(item.armor.as_deref(), Some("Docent"));
    assert_eq!(item.armor_bonus, Some(-3));
    assert_eq!(item.mithral_body, Some(5));
    assert_eq!(item.adamantine_body, Some(12));
    let requirements = item.requirements.as_ref().unwrap();
    assert_eq!(requirements.groups.len(), 1);
    assert_eq!(requirements.groups[0].requirements[0].kind, "RaceConstruct");
    assert!(requirements.groups[0].requirements[0].items.is_empty());
    let elements: Vec<_> =
        item.buffs.iter().filter(|b| b.kind == "EnergyResistance").map(|b| b.target.clone().unwrap()).collect();
    assert_eq!(elements, vec!["Fire", "Electric", "Cold"]);
}

#[test]
fn parses_the_two_slot_ring_and_value2() {
    let item = parsed_item("Five Rings");
    assert_eq!(item.equipment_slots.tags.len(), 2);
    let stalker = parsed_item("Epic Ring of the Stalker");
    let deception = stalker.buffs.iter().find(|b| b.kind == "Deception").unwrap();
    assert_eq!((deception.value, deception.second_value), (Some(3), Some(5)));
    assert_eq!(deception.bonus_type.as_deref(), Some("Insightful"));
    assert_eq!(stalker.set_bonus_names, Vec::<String>::new());
    let boots = parsed_item("Kundarak Delving Boots");
    assert_eq!(boots.set_bonus_names, vec!["Kundarak Delving Equipment"]);
}

#[test]
fn parses_every_fixture_item() {
    let items_dir = data_files_fixture_dir().join("Items");
    let mut item_file_count = 0;
    for entry in std::fs::read_dir(&items_dir).unwrap() {
        let path = entry.unwrap().path();
        parse_item_file(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        item_file_count += 1;
    }
    assert_eq!(item_file_count, 41);
}

#[test]
fn parses_interleaved_repeated_elements() {
    let item = parsed_item("Acid Rune Arm");
    assert_eq!(item.buffs.len(), 2, "{:?}", item.buffs.iter().map(|b| &b.kind).collect::<Vec<_>>());
    assert_eq!(item.weapon.as_deref(), Some("Rune Arm"));
    assert_eq!(item.augment_slots.len(), 2);
}

#[test]
fn parses_quests_patrons_and_item_buffs() {
    let parsed_quests = quests::parse(&data_files_fixture_dir().join("Quests.xml")).unwrap();
    assert_eq!(parsed_quests.len(), 22);
    let chronoscope = parsed_quests.iter().find(|q| q.name == "The Chronoscope").unwrap();
    assert_eq!(chronoscope.levels, vec![6, 21]);
    assert!(chronoscope.is_raid);
    assert_eq!(chronoscope.patron.as_deref(), Some("The Coin Lords"));
    assert_eq!(chronoscope.adventure_pack.as_deref(), Some("Devil Assault"));
    assert_eq!(chronoscope.favor, Some(5));
    let lamordia = parsed_quests.iter().find(|q| q.name == "Land of Lamordia").unwrap();
    assert!(lamordia.is_hidden);
    assert!(!lamordia.is_raid);

    use quests::Difficulty::*;
    assert_eq!(chronoscope.difficulties, vec![Casual, Normal, Hard, Elite, Reaper]);
    assert_eq!(chronoscope.epic_name, None);
    assert!(lamordia.difficulties.is_empty(), "no flag elements, no difficulties");
    let grotto = parsed_quests.iter().find(|q| q.name == "The Grotto").unwrap();
    assert_eq!(grotto.difficulties, vec![Solo]);
    let madstone = parsed_quests.iter().find(|q| q.name == "Madstone Crater").unwrap();
    assert_eq!(madstone.epic_name.as_deref(), Some("Return to Madstone Crater"));
    assert_eq!(madstone.difficulties, vec![Normal, Hard, Elite, Reaper]);

    let parsed_patrons = patrons::parse(&data_files_fixture_dir().join("Patrons.xml")).unwrap();
    assert!(parsed_patrons.iter().any(|p| p.name == "House Cannith"));

    let definitions_by_buff_kind = item_buffs::parse(&data_files_fixture_dir().join("ItemBuffs.xml")).unwrap();
    let display_text = |buff_kind: &str| definitions_by_buff_kind.get(buff_kind).map(|d| d.display_text.as_str());
    assert_eq!(
        display_text("WeaponEnchantment"),
        Some("%v1 Enhancement Bonus: %v1 Enhancement bonus to attack and damage rolls.")
    );
    assert!(definitions_by_buff_kind.contains_key("Supreme Good"));
    assert_eq!(
        display_text("Fixture Multi Paragraph"),
        Some("First paragraph.\nSecond paragraph."),
        "repeated <DisplayText> elements are paragraphs of one description"
    );
    let bonus_type_name = |buff_kind: &str| definitions_by_buff_kind[buff_kind].bonus_type_name.as_deref();
    assert_eq!(bonus_type_name("Command"), Some("Insightful"), "the first <Effect> carries Value1, not the Penalty");
    assert_eq!(bonus_type_name("Illusion Save"), Some("Resistance"));
    assert_eq!(bonus_type_name("Damage Bonus"), Some("Not Set"));
    assert_eq!(bonus_type_name("Linguistics"), None, "no <Effect>");
}
