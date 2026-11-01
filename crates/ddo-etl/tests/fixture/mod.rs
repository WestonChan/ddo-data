#![allow(dead_code)]

use ddo_etl::corrections::{Correction, CorrectionValue, Corrections};
use ddo_model::enums::CorrectionKind;
use std::ops::Add;

pub(crate) fn groupings() -> &'static ddo_etl::groupings::Groupings {
    static GROUPINGS: std::sync::OnceLock<ddo_etl::groupings::Groupings> = std::sync::OnceLock::new();
    GROUPINGS.get_or_init(|| ddo_etl::groupings::Groupings::from_database(&database(|_| {})).unwrap())
}

pub(crate) fn grouping_cases() -> &'static ddo_etl::groupings::Groupings {
    static GROUPINGS: std::sync::OnceLock<ddo_etl::groupings::Groupings> = std::sync::OnceLock::new();
    GROUPINGS.get_or_init(|| {
        let db = grouping_case_database(|_| {});
        ddo_etl::groupings::Groupings::from_database(&db).unwrap()
    })
}

pub(crate) fn grouping_case_database(edit: impl Fn(&std::path::Path)) -> rusqlite::Connection {
    database(|root| {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/effect-groups/data");
        for entry in
            walkdir::WalkDir::new(&source).into_iter().map(Result::unwrap).filter(|entry| entry.file_type().is_file())
        {
            let destination = root.join(entry.path().strip_prefix(&source).unwrap());
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), destination).unwrap();
        }
        edit(root);
    })
}

pub(crate) fn xml_source() -> tempfile::TempDir {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles");
    let scratch = tempfile::tempdir().unwrap();
    for entry in
        walkdir::WalkDir::new(&source).into_iter().map(Result::unwrap).filter(|entry| entry.file_type().is_file())
    {
        let destination = scratch.path().join(entry.path().strip_prefix(&source).unwrap());
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::copy(entry.path(), destination).unwrap();
    }
    scratch
}

pub(crate) fn bonus_correction_twins(case: &str, diverges: bool) -> (String, CorrectionSet) {
    let mut first =
        include_str!("../fixtures/effect-groups/WoefulShadow.Augments.xml").replace("    <Icon>Lamordia</Icon>\n", "");
    let (field, qualifier, upstream, corrected) = match case {
        "add" => ("add", "", "'null'", "{ stat = 'Fire Spell Power', bonus_type = 'Equipment', value = 7 }"),
        "remove" => ("remove", "stat = 'Doubleshot'\nbonus_type = 'Profane'", "3", "'null'"),
        "split" => (
            "lines",
            "stat = 'Doublestrike, Doubleshot'\nbonus_type = 'Profane'",
            "[['Doublestrike', 'Doubleshot']]",
            "['Doublestrike', 'Doubleshot']",
        ),
        "merge" => {
            first = first.replace("      <Type>Doubleshot</Type>\n", "");
            let effect = first.split_once("    <Effect>").unwrap().1.split_once("    </Effect>").unwrap().0;
            first = first.replacen(
                "    </Effect>",
                &format!("    </Effect>\n    <Effect>{}    </Effect>", effect.replace("Doublestrike", "Doubleshot")),
                1,
            );
            (
                "lines",
                "stat = 'Doublestrike, Doubleshot'\nbonus_type = 'Profane'",
                "[['Doublestrike'], ['Doubleshot']]",
                "'Doublestrike and Doubleshot'",
            )
        }
        _ => panic!("unknown twin correction {case}"),
    };
    let mut second = first.clone();
    if diverges {
        if case == "add" {
            second = second.replace("  </Augment>", "    <Effect><Type>SpellPower</Type><Bonus>Equipment</Bonus><AType>Simple</AType><Amount size=\"1\">8</Amount><Item>Fire</Item></Effect>\n  </Augment>");
        } else {
            second = second.replace("<Type>Doubleshot</Type>", "<Type>MeleePower</Type>");
        }
    }
    second = second
        .replace("<Augments>", "")
        .replace("</Augments>", "")
        .replace("(Legendary)</Name>", "(Legendary) </Name>")
        .replace("<Description>", "<Description>Second source. ");
    (
        first.replace("</Augments>", &format!("{second}</Augments>")),
        new_qualified_correction("augment_bonus", "Woeful Shadow (Legendary)", qualifier, field, upstream, corrected),
    )
}

pub(crate) fn write_scion_feat(source: &std::path::Path) {
    let path = source.join("Feats.xml");
    let feats = std::fs::read_to_string(&path).unwrap();
    let scion = include_str!("../fixtures/effect-groups/ScionOfTheAstralPlane.Feat.xml");
    std::fs::write(path, feats.replace("</Feats>", &format!("{scion}</Feats>"))).unwrap();
    std::fs::create_dir_all(source.join("FeatImages")).unwrap();
    std::fs::write(
        source.join("FeatImages/ScionOfTheAstralPlane.png"),
        include_bytes!("../fixtures/effect-groups/icons/ScionOfTheAstralPlane.png"),
    )
    .unwrap();
}

pub(crate) fn write_prose_sets(source: &std::path::Path) {
    std::fs::create_dir_all(source.join("FiligreeSets")).unwrap();
    for (name, closing_tag, additions) in [
        ("SetBonuses.xml", "</SetBonuses>", include_str!("../fixtures/effect-groups/TextOnlySets.xml")),
        ("ItemBuffs.xml", "</Buffs>", include_str!("../fixtures/effect-groups/TextOnlyEffects.Buffs.xml")),
    ] {
        let path = source.join(name);
        let original = std::fs::read_to_string(&path).unwrap();
        std::fs::write(path, original.replace(closing_tag, &format!("{additions}{closing_tag}"))).unwrap();
    }
    for (name, contents) in [
        ("ThroughTheMists.Filigree.xml", include_str!("../fixtures/effect-groups/ThroughTheMists.Filigree.xml")),
        ("RadiantShield.Filigree.xml", include_str!("../fixtures/effect-groups/RadiantShield.Filigree.xml")),
    ] {
        std::fs::write(source.join("FiligreeSets").join(name), contents).unwrap();
    }
    for (path, bytes) in [
        (
            "SetBonusImages/CorsairsCunning.png",
            include_bytes!("../fixtures/effect-groups/icons/CorsairsCunning.png").as_slice(),
        ),
        (
            "SetBonusImages/SubterfugePlanarConfluxSetBonus.png",
            include_bytes!("../fixtures/effect-groups/icons/SubterfugePlanarConfluxSetBonus.png").as_slice(),
        ),
        (
            "FiligreeImages/ThroughTheMists.png",
            include_bytes!("../fixtures/effect-groups/icons/ThroughTheMists.png").as_slice(),
        ),
        (
            "FiligreeImages/RadiantShield.png",
            include_bytes!("../fixtures/effect-groups/icons/RadiantShield.png").as_slice(),
        ),
    ] {
        let destination = source.join(path);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::write(destination, bytes).unwrap();
    }
}

pub(crate) fn write_plague_wolf_feat(source: &std::path::Path) {
    let path = source.join("Feats.xml");
    let original = std::fs::read_to_string(&path).unwrap();
    let feat = include_str!("../fixtures/effect-groups/PlagueWolf.Feat.xml");
    std::fs::write(path, original.replace("</Feats>", &format!("{feat}</Feats>"))).unwrap();
    std::fs::create_dir_all(source.join("FeatImages")).unwrap();
    std::fs::write(
        source.join("FeatImages/WildShapePlagueWolf.png"),
        include_bytes!("../fixtures/effect-groups/icons/WildShapePlagueWolf.png"),
    )
    .unwrap();
}

pub(crate) fn write_diadem_item(source: &std::path::Path) {
    let path = source.join("ItemBuffs.xml");
    let buffs = std::fs::read_to_string(&path).unwrap();
    let definitions = include_str!("../fixtures/effect-groups/TheDiademOfRot.Buffs.xml");
    std::fs::write(path, buffs.replace("</Buffs>", &format!("{definitions}</Buffs>"))).unwrap();
    std::fs::write(
        source.join("Items/TheDiademOfRot.item"),
        include_str!("../fixtures/effect-groups/TheDiademOfRot.item"),
    )
    .unwrap();
    std::fs::create_dir_all(source.join("ItemImages/Helmets")).unwrap();
    std::fs::write(
        source.join("ItemImages/Helmets/Helmet_71a.png"),
        include_bytes!("../fixtures/effect-groups/icons/Helmet_71a.png"),
    )
    .unwrap();
}

pub(crate) fn write_ship_kickers(source: &std::path::Path) {
    std::fs::create_dir_all(source.join("Items")).unwrap();
    std::fs::write(source.join("Items/ShipKickers.item"), include_str!("../fixtures/DataFiles/Items/ShipKickers.item"))
        .unwrap();
    std::fs::create_dir_all(source.join("ItemImages/Boots")).unwrap();
    std::fs::write(
        source.join("ItemImages/Boots/Boots_1.png"),
        include_bytes!("../fixtures/DataFiles/ItemImages/Boots/Boots_1.png"),
    )
    .unwrap();
}

pub(crate) fn riposte_rank_database() -> rusqlite::Connection {
    grouping_case_database(|_| {})
}

pub(crate) fn write_lore_items(source: &std::path::Path) {
    let path = source.join("ItemBuffs.xml");
    let buffs = std::fs::read_to_string(&path).unwrap();
    let definitions = include_str!("../fixtures/effect-groups/Lore.Buffs.xml");
    std::fs::write(path, buffs.replace("</Buffs>", &format!("{definitions}</Buffs>"))).unwrap();
    for (name, item) in [
        ("LuminousTruth.item", include_str!("../fixtures/effect-groups/LuminousTruth.item")),
        ("GleamingPlate.item", include_str!("../fixtures/effect-groups/GleamingPlate.item")),
    ] {
        std::fs::write(source.join("Items").join(name), item).unwrap();
    }
    for (path, bytes) in [
        (
            "ItemImages/Weapon_Quarterstaff/Quarterstaff_5g.png",
            include_bytes!("../fixtures/effect-groups/icons/Quarterstaff_5g.png").as_slice(),
        ),
        (
            "ItemImages/Armor_Heavy/HeavyArmor_18.png",
            include_bytes!("../fixtures/effect-groups/icons/HeavyArmor_18.png").as_slice(),
        ),
    ] {
        let destination = source.join(path);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::write(destination, bytes).unwrap();
    }
}

pub(crate) fn plague_wolf_corrections() -> CorrectionSet {
    let removal = new_qualified_correction(
        "feat_bonus",
        "Wild Shape: Plague Wolf",
        "stat = 'Intelligence'\nbonus_type = 'Penalty'",
        "remove",
        "-2",
        "\"null\"",
    );
    let value = new_qualified_correction(
        "feat_bonus",
        "Wild Shape: Plague Wolf",
        "stat = 'Charisma'\nbonus_type = 'Penalty'",
        "value",
        "-2",
        "-4",
    );
    removal + &value
}

pub(crate) fn plague_wolf_penalties(db: &rusqlite::Connection) -> Vec<(String, i64, String, bool)> {
    db.prepare("SELECT e.name, l.value, bt.name, e.is_stat FROM feat_effects l JOIN feats f ON f.id=l.feat_id JOIN effects e ON e.id=l.effect_id JOIN bonus_types bt ON bt.id=l.bonus_type_id WHERE f.name='Wild Shape: Plague Wolf' AND bt.name='Penalty' ORDER BY e.name").unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap()
}

pub(crate) fn scion_dodge_split() -> CorrectionSet {
    new_qualified_correction(
        "feat_bonus",
        "Scion of the Astral Plane",
        "stat = 'Dodge, Dodge Cap'\nbonus_type = 'Feat'",
        "lines",
        "[['Dodge', 'Dodge Cap']]",
        "[\"Dodge\", \"Dodge Cap\"]",
    )
}

pub(crate) fn write_eye_of_lamannia(source: &std::path::Path) {
    std::fs::write(
        source.join("Augments/EyeOfLamannia.Augments.xml"),
        include_str!("../fixtures/effect-groups/EyeOfLamannia.Augments.xml"),
    )
    .unwrap();
    std::fs::write(
        source.join("AugmentImages/RubyEye.png"),
        include_bytes!("../fixtures/effect-groups/icons/RubyEye.png"),
    )
    .unwrap();
}

pub(crate) fn write_stat_set_split_augments(source: &std::path::Path) {
    for (name, text) in [
        (
            "CurseOfIntangibility.Augments.xml",
            include_str!("../fixtures/effect-groups/CurseOfIntangibility.Augments.xml"),
        ),
        (
            "DolorousInvigorator.Augments.xml",
            include_str!("../fixtures/effect-groups/DolorousInvigorator.Augments.xml"),
        ),
    ] {
        std::fs::create_dir_all(source.join("Augments")).unwrap();
        std::fs::write(source.join("Augments").join(name), text).unwrap();
    }
    for (name, bytes) in [
        ("DeckOfCurses.png", include_bytes!("../fixtures/effect-groups/icons/DeckOfCurses.png").as_slice()),
        ("Lamordia.png", include_bytes!("../fixtures/effect-groups/icons/Lamordia.png").as_slice()),
    ] {
        std::fs::create_dir_all(source.join("AugmentImages")).unwrap();
        std::fs::write(source.join("AugmentImages").join(name), bytes).unwrap();
    }
}

pub(crate) fn dolorous_stat_set_corrections() -> CorrectionSet {
    let mut corrections = new_qualified_correction(
        "augment_bonus",
        "Dolorous Invigorator (Legendary)",
        "stat = 'Trip DC, Sunder DC, Stun DC, Assassinate DC'\nbonus_type = 'Profane'",
        "lines",
        "[['Trip DC', 'Sunder DC', 'Stun DC', 'Assassinate DC']]",
        "['Combat Mastery', 'Assassinate']",
    );
    for tier in ["Heroic", "Legendary"] {
        for stat in ["Trip DC", "Sunder DC", "Stun DC", "Assassinate DC"] {
            corrections = corrections
                + &new_qualified_correction(
                    "augment_bonus",
                    &format!("Dolorous Invigorator ({tier})"),
                    &format!("stat = {stat:?}\nbonus_type = 'null'"),
                    "bonus_type",
                    "'null'",
                    "'Profane'",
                );
        }
    }
    corrections
}

pub(crate) fn scion_dodge_lines(db: &rusqlite::Connection) -> Vec<(String, i64)> {
    db.prepare(
        "SELECT e.name,l.value FROM feat_effects l JOIN effects e ON e.id=l.effect_id
         JOIN feats f ON f.id=l.feat_id WHERE f.name='Scion of the Astral Plane'
         AND e.name LIKE '%Dodge%' ORDER BY e.name",
    )
    .unwrap()
    .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
    .unwrap()
    .collect::<rusqlite::Result<_>>()
    .unwrap()
}

pub(crate) fn write_reaper_augment(source: &std::path::Path, resistances: Option<(i64, i64)>) {
    std::fs::create_dir_all(source.join("Augments")).unwrap();
    let mut xml = include_str!("../fixtures/effect-groups/Reaper.Augments.xml").to_string();
    if let Some((physical, magical)) = resistances {
        for (kind, amount) in [("PRR", physical), ("MRR", magical)] {
            let effect = format!("<Effect><Type>{kind}</Type><Bonus>Reaper</Bonus><AType>Simple</AType><Amount size=\"1\">{amount}</Amount></Effect>");
            xml = xml.replace("</Augment>", &format!("{effect}</Augment>"));
        }
    }
    std::fs::write(source.join("Augments/Reaper.Augments.xml"), xml).unwrap();
}

pub(crate) fn reaper_additions() -> CorrectionSet {
    let mut notes = CorrectionSet::default();
    for stat in ["Physical Resistance Rating", "Magical Resistance Rating"] {
        notes = notes
            + &new_qualified_correction(
                "augment_bonus",
                "+2 Melee/Ranged and Spell Power",
                "",
                "add",
                "'null'",
                &format!("{{stat = {stat:?}, bonus_type = 'Reaper', value = 2}}"),
            );
    }
    notes
}

pub(crate) fn reaper_merge() -> CorrectionSet {
    new_qualified_correction(
        "augment_bonus",
        "+2 Melee/Ranged and Spell Power",
        "stat = 'Melee Power, Ranged Power, Physical Resistance Rating, Magical Resistance Rating, Universal Spell Power'\nbonus_type = 'Reaper'",
        "lines",
        "[['Melee Power', 'Ranged Power'], ['Universal Spell Power'], ['Physical Resistance Rating'], ['Magical Resistance Rating']]",
        "'Reaper Necklace Enchantment'",
    )
}

pub(crate) fn reaper_lines(db: &rusqlite::Connection) -> Vec<(String, Option<i64>)> {
    db.prepare("SELECT e.name,l.value FROM augment_effects l JOIN effects e ON e.id=l.effect_id JOIN augments a ON a.id=l.augment_id WHERE a.name='+2 Melee/Ranged and Spell Power' ORDER BY l.sort_order")
        .unwrap().query_map([], |row| Ok((row.get(0)?,row.get(1)?)))
        .unwrap().collect::<rusqlite::Result<_>>().unwrap()
}

pub(crate) fn reaper_bonuses(db: &rusqlite::Connection) -> Vec<(String, String, i64)> {
    db.prepare("SELECT s.name,t.name,b.amount FROM owner_bonuses b JOIN effects s ON s.id=b.stat_id JOIN bonus_types t ON t.id=b.bonus_type_id JOIN augments a ON a.id=b.owner_id WHERE b.owner_kind='augment' AND a.name='+2 Melee/Ranged and Spell Power' ORDER BY s.name")
        .unwrap().query_map([], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)))
        .unwrap().collect::<rusqlite::Result<_>>().unwrap()
}

pub(crate) fn write_search_and_spot_item(source: &std::path::Path, amount: i64, include_spot: bool) {
    std::fs::create_dir_all(source.join("Items")).unwrap();
    let mut item = include_str!("../fixtures/effect-groups/SearchAndSpot.item")
        .replace("<Value1>20</Value1>", &format!("<Value1>{amount}</Value1>"));
    if !include_spot {
        let target = item.find("<Item>Spot</Item>").unwrap();
        let start = item[..target].rfind("<Buff>").unwrap();
        let end = target + item[target..].find("</Buff>").unwrap() + "</Buff>".len();
        item.replace_range(start..end, "");
    }
    std::fs::write(source.join("Items/SearchAndSpot.item"), item).unwrap();
    let path = source.join("ItemBuffs.xml");
    if !ddo_etl::xml::item_buffs::parse(&path).unwrap().contains_key("SkillBonus") {
        let buffs = std::fs::read_to_string(&path).unwrap();
        let definition = include_str!("../fixtures/effect-groups/SkillBonus.Buff.xml");
        std::fs::write(path, buffs.replace("</Buffs>", &format!("{definition}</Buffs>"))).unwrap();
    }
}

pub(crate) fn write_spyglass_options(source: &std::path::Path) {
    std::fs::create_dir_all(source.join("Items")).unwrap();
    std::fs::create_dir_all(source.join("ItemImages/Trinkets")).unwrap();
    std::fs::write(
        source.join("ItemImages/Trinkets/Trinket_17.png"),
        include_bytes!("../fixtures/effect-groups/icons/Trinket_17.png"),
    )
    .unwrap();
    std::fs::create_dir_all(source.join("AugmentImages")).unwrap();
    std::fs::write(
        source.join("AugmentImages/Upgrade.png"),
        include_bytes!("../fixtures/data/icons/augments/Upgrade.png"),
    )
    .unwrap();
    std::fs::write(
        source.join("Items/SpyglassOptions.item"),
        include_str!("../fixtures/effect-groups/SpyglassOptions.item"),
    )
    .unwrap();
    let path = source.join("ItemBuffs.xml");
    let mut buffs = std::fs::read_to_string(&path).unwrap();
    let existing = ddo_etl::xml::item_buffs::parse(&path).unwrap();
    for definition in include_str!("../fixtures/effect-groups/SpyglassOptions.Buffs.xml").split("  <Buff>").skip(1) {
        let kind = definition.split_once("<Type>").unwrap().1.split_once("</Type>").unwrap().0;
        if !existing.contains_key(kind) {
            buffs = buffs.replace("</Buffs>", &format!("  <Buff>{definition}</Buffs>"));
        }
    }
    std::fs::write(path, buffs).unwrap();
}

pub(crate) fn spyglass_option_correction(
    field: &str,
    upstream: serde_json::Value,
    applied: CorrectionValue,
) -> CorrectionSet {
    let note = serde_json::from_value::<ddo_etl::tree::DdobuilderCorrection>(serde_json::json!({
        "kind": "socket_option_bonus",
        "field": field,
        "qualifier": "socket \"upgrade: tier 2\", option \"Spot +20, Search +20, Insightful Intelligence +2\", Search and Spot / Competence",
        "upstream": upstream,
        "reason": "Test correction on a captured socket option.",
        "url": "https://ddowiki.com/page/Item:Epic_Treasure_Hunter%27s_Spyglass_(Level_20)",
        "read": "2026-10-23"
    })).unwrap();
    let mut correction =
        ddo_etl::tree::correction_for_note(&note, "Epic Treasure Hunter's Spyglass (legacy) (level 20)").unwrap();
    correction.from =
        CorrectionValue::from_json(&note.upstream.unwrap_or(serde_json::Value::Null).to_string()).unwrap();
    if field == "lines" {
        correction.stat = Some("Search, Spot".into());
    }
    correction.to = applied;
    CorrectionSet(vec![correction])
}

pub(crate) fn write_rank_options(source: &std::path::Path) {
    write_spyglass_options(source);
    std::fs::write(
        source.join("AugmentImages/Variant.png"),
        include_bytes!("../fixtures/effect-groups/icons/Variant.png"),
    )
    .unwrap();
    let path = source.join("Items/SpyglassOptions.item");
    let item = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        path,
        item.replace("  </Item>", &format!("{}  </Item>", include_str!("../fixtures/effect-groups/RankOptions.xml"))),
    )
    .unwrap();
}

pub(crate) fn write_command_owners(source: &std::path::Path) {
    for (name, contents) in [
        ("Items/RingOfPower.item", include_str!("../fixtures/effect-groups/RingOfPower.item")),
        ("Items/GrandRingmastersPlate4.item", include_str!("../fixtures/effect-groups/GrandRingmastersPlate4.item")),
        (
            "Augments/BrightbaneEmerald.Augments.xml",
            include_str!("../fixtures/effect-groups/BrightbaneEmerald.Augments.xml"),
        ),
    ] {
        std::fs::write(source.join(name), contents).unwrap();
    }
    let sets = source.join("SetBonuses.xml");
    let original = std::fs::read_to_string(&sets).unwrap();
    std::fs::write(
        sets,
        original.replace(
            "</SetBonuses>",
            &format!("{}</SetBonuses>", include_str!("../fixtures/effect-groups/CommandOwners.SetBonuses.xml")),
        ),
    )
    .unwrap();
    for (name, bytes) in [
        ("ItemImages/Rings/Ring_31.png", include_bytes!("../fixtures/effect-groups/icons/Ring_31.png").as_slice()),
        (
            "ItemImages/Armor_Heavy/HeavyArmor_1a.png",
            include_bytes!("../fixtures/effect-groups/icons/HeavyArmor_1a.png").as_slice(),
        ),
        (
            "SetBonusImages/SlavelordsMight.png",
            include_bytes!("../fixtures/effect-groups/icons/SlavelordsMight.png").as_slice(),
        ),
        (
            "SetBonusImages/SlavelordsSorcery.png",
            include_bytes!("../fixtures/effect-groups/icons/SlavelordsSorcery.png").as_slice(),
        ),
        (
            "SetBonusImages/SlavelordsEndurance.png",
            include_bytes!("../fixtures/effect-groups/icons/SlavelordsEndurance.png").as_slice(),
        ),
        ("AugmentImages/Emerald.png", include_bytes!("../fixtures/effect-groups/icons/Emerald.png").as_slice()),
    ] {
        let path = source.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}

pub(crate) fn write_elemental_resistance_owners(source: &std::path::Path) {
    let captured =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/effect-groups/ElementalResistance");
    for entry in
        walkdir::WalkDir::new(&captured).into_iter().map(Result::unwrap).filter(|entry| entry.file_type().is_file())
    {
        let relative = entry.path().strip_prefix(&captured).unwrap();
        let destination = source.join(relative);
        let closing_tag = match relative.to_str().unwrap() {
            "ItemBuffs.xml" => Some("</Buffs>"),
            "Feats.xml" => Some("</Feats>"),
            "SetBonuses.xml" => Some("</SetBonuses>"),
            _ => None,
        };
        if let Some(closing_tag) = closing_tag {
            let original = std::fs::read_to_string(&destination).unwrap();
            let additions = std::fs::read_to_string(entry.path()).unwrap();
            std::fs::write(destination, original.replace(closing_tag, &format!("{additions}{closing_tag}"))).unwrap();
        } else {
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), destination).unwrap();
        }
    }
}

pub(crate) fn rank_option_merge(rank: &str) -> CorrectionSet {
    let mut correction = spyglass_option_correction(
        "lines",
        serde_json::json!([["Armor Class"], ["Saving Throws"]]),
        CorrectionValue::Text(rank.into()),
    );
    correction.0[0].socket = Some("crafting: variant".into());
    correction.0[0].option = Some(rank.into());
    correction.0[0].stat = Some("Armor Class, Saving Throws".into());
    correction.0[0].bonus_type = Some("Insight".into());
    correction
}

pub(crate) fn rank_option_description_correction(source: &std::path::Path) -> CorrectionSet {
    let item = ddo_etl::xml::items::parse_item_file(&source.join("Items/SpyglassOptions.item")).unwrap();
    let description = &item.items[0]
        .augment_slots
        .iter()
        .flat_map(|slot| &slot.options)
        .find(|option| option.name == "Riposte VI")
        .unwrap()
        .description;
    let note = serde_json::from_value::<ddo_etl::tree::DdobuilderCorrection>(serde_json::json!({
        "kind": "item", "field": "description",
        "qualifier": "socket \"crafting: variant\", option \"Riposte VI\"",
        "upstream": description, "reason": "Correct the captured option's saves text.",
        "url": "https://ddowiki.com/page/Item:Fell_Shiv_(Level_25)", "read": "2026-10-26"
    }))
    .unwrap();
    let mut correction = ddo_etl::tree::correction_for_note(&note, &item.items[0].name).unwrap();
    correction.from = CorrectionValue::Text(description.clone());
    correction.to = CorrectionValue::Text(description.replace("+2 Insight", "+3 Insight"));
    CorrectionSet(vec![correction])
}

pub(crate) fn rank_option_value_correction() -> CorrectionSet {
    let mut correction = rank_option_merge("Riposte VI");
    correction.0[0].stat = Some("Saving Throws".into());
    correction.0[0].field = "value".into();
    correction.0[0].from = CorrectionValue::Integer(2);
    correction.0[0].to = CorrectionValue::Integer(3);
    correction
}

pub(crate) fn spyglass_option_lines(db: &rusqlite::Connection) -> Vec<(String, i64)> {
    db.prepare("SELECT e.name,l.value FROM item_augment_slot_option_effects l JOIN effects e ON e.id=l.effect_id
        JOIN item_augment_slot_options o ON o.id=l.option_id JOIN items i ON i.id=o.item_id
        WHERE i.name='Epic Treasure Hunter''s Spyglass (legacy) (level 20)' AND o.slot_order=0 AND e.name != 'Intelligence'
        ORDER BY e.name").unwrap().query_map([], |row| Ok((row.get(0)?,row.get(1)?))).unwrap()
        .collect::<rusqlite::Result<_>>().unwrap()
}

pub(crate) fn write_swiftness_augment(source: &std::path::Path, movement: i64) {
    let original = include_str!("../fixtures/effect-groups/TopazOfSwiftness.Augments.xml");
    std::fs::create_dir_all(source.join("Augments")).unwrap();
    std::fs::write(
        source.join("Augments/Topaz.Augments.xml"),
        original.replace("<Amount size=\"1\">30</Amount>", &format!("<Amount size=\"1\">{movement}</Amount>")),
    )
    .unwrap();
    std::fs::create_dir_all(source.join("AugmentImages")).unwrap();
    std::fs::write(source.join("AugmentImages/Topaz.png"), include_bytes!("../fixtures/data/icons/augments/Topaz.png"))
        .unwrap();
}

pub(crate) fn swiftness_merge() -> CorrectionSet {
    new_qualified_correction(
        "augment_bonus",
        "Topaz of Swiftness",
        "stat = \"Movement Speed, Melee Attack Speed, Ranged Attack Speed\"\nbonus_type = 'Enhancement'",
        "lines",
        "[[\"Movement Speed\"], [\"Melee Attack Speed\", \"Ranged Attack Speed\"]]",
        "'Speed'",
    )
}

pub(crate) fn swiftness_links(db: &rusqlite::Connection) -> Vec<(String, Option<i64>, Option<i64>)> {
    db.prepare("SELECT e.name,l.value,l.value2 FROM augment_effects l JOIN effects e ON e.id=l.effect_id JOIN augments a ON a.id=l.augment_id WHERE a.name='Topaz of Swiftness' ORDER BY l.sort_order")
        .unwrap().query_map([], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)))
        .unwrap().collect::<rusqlite::Result<_>>().unwrap()
}

pub(crate) fn search_and_spot_merge() -> CorrectionSet {
    new_qualified_correction(
        "item_bonus",
        "Legendary Keylock Ring",
        "stat = \"Search, Spot\"\nbonus_type = 'Competence'",
        "lines",
        "[[\"Search\"], [\"Spot\"]]",
        "\"Search and Spot\"",
    )
}

pub(crate) fn search_and_spot_lines(db: &rusqlite::Connection) -> Vec<(String, Option<i64>)> {
    db.prepare(
        "SELECT e.name,l.value FROM item_effects l JOIN effects e ON e.id=l.effect_id
         JOIN items i ON i.id=l.item_id WHERE i.name='Legendary Keylock Ring' ORDER BY l.sort_order",
    )
    .unwrap()
    .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
    .unwrap()
    .collect::<rusqlite::Result<_>>()
    .unwrap()
}

pub(crate) fn database(edit: impl FnOnce(&std::path::Path)) -> rusqlite::Connection {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data");
    let scratch = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(&source).into_iter().map(Result::unwrap) {
        if entry.file_type().is_file() {
            let destination = scratch.path().join(entry.path().strip_prefix(&source).unwrap());
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), destination).unwrap();
        }
    }
    edit(scratch.path());
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(scratch.path(), &mut db).unwrap();
    db
}

pub(crate) fn verification_note_database(edit: impl FnOnce(&std::path::Path)) -> rusqlite::Connection {
    database(|root| {
        let captured = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/verification-notes/data");
        for entry in
            walkdir::WalkDir::new(&captured).into_iter().map(Result::unwrap).filter(|entry| entry.file_type().is_file())
        {
            let destination = root.join(entry.path().strip_prefix(&captured).unwrap());
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), destination).unwrap();
        }
        edit(root);
    })
}

pub(crate) fn fixture_icon_corrections() -> Corrections {
    let mut corrections = Corrections::from_database(&database(|_| {})).unwrap();
    corrections.entries.retain(|entry| entry.field == "icon");
    corrections
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CorrectionSet(pub(crate) Vec<Correction>);

impl CorrectionSet {
    pub(crate) fn sourced(mut self, source: &str) -> Self {
        for correction in &mut self.0 {
            correction.source = source.to_string().into();
        }
        self
    }

    pub(crate) fn cited(mut self, reason: &str, source: &str, read: &str) -> Self {
        for correction in &mut self.0 {
            correction.reason = reason.into();
            correction.source = source.to_string().into();
            correction.read = read.into();
        }
        self
    }
}

impl FromIterator<CorrectionSet> for CorrectionSet {
    fn from_iter<Sets: IntoIterator<Item = CorrectionSet>>(sets: Sets) -> Self {
        CorrectionSet(sets.into_iter().flat_map(|set| set.0).collect())
    }
}

impl Add<&CorrectionSet> for CorrectionSet {
    type Output = CorrectionSet;

    fn add(mut self, other: &CorrectionSet) -> CorrectionSet {
        self.0.extend(other.0.iter().cloned());
        self
    }
}

impl Add<&str> for CorrectionSet {
    type Output = CorrectionSet;

    fn add(mut self, qualifier_keys: &str) -> CorrectionSet {
        let correction = self.0.last_mut().expect("qualifiers belong to a correction");
        set_qualifiers(correction, qualifier_keys);
        self
    }
}

pub(crate) fn correction_value(literal: &str) -> CorrectionValue {
    let table: toml::Table = toml::from_str(&format!("value = {literal}")).unwrap();
    let mut json = serde_json::to_value(&table["value"]).unwrap();
    if json == serde_json::json!("null") {
        json = serde_json::Value::Null;
    }
    CorrectionValue::from_json(&json.to_string()).unwrap()
}

pub(crate) fn new_correction(kind: &str, name: &str, field: &str, from: &str, to: &str) -> CorrectionSet {
    correction_between(kind, name, field, correction_value(from), correction_value(to))
}

pub(crate) fn correction_between(
    kind: &str,
    name: &str,
    field: &str,
    from: CorrectionValue,
    to: CorrectionValue,
) -> CorrectionSet {
    CorrectionSet(vec![Correction {
        kind: CorrectionKind::parse(kind).unwrap_or_else(|| panic!("unknown correction kind {kind:?}")),
        name: name.into(),
        field: field.into(),
        from,
        to,
        family: None,
        socket: None,
        option: None,
        selection: None,
        stance: None,
        dc: None,
        effect: None,
        stat: None,
        bonus_type: None,
        bonus_value: None,
        equipped_count: None,
        reason: "Test reason.".into(),
        source: "https://ddowiki.com/page/Test".to_string().into(),
        read: "2026-09-29".into(),
        file_name: "corrections.toml".into(),
    }])
}

pub(crate) fn new_qualified_correction(
    kind: &str,
    name: &str,
    qualifier_keys: &str,
    field: &str,
    from: &str,
    to: &str,
) -> CorrectionSet {
    new_correction(kind, name, field, from, to) + qualifier_keys
}

fn set_qualifiers(correction: &mut Correction, qualifier_keys: &str) {
    let keys: toml::Table = toml::from_str(qualifier_keys).unwrap();
    for (key, value) in keys {
        let text = || value.as_str().unwrap().to_string();
        match key.as_str() {
            "socket" => correction.socket = Some(text()),
            "option" => correction.option = Some(text()),
            "family" => correction.family = Some(text()),
            "selection" => correction.selection = Some(text()),
            "stance" => correction.stance = Some(text()),
            "dc" => correction.dc = Some(text()),
            "effect" => correction.effect = Some(text()),
            "stat" => correction.stat = Some(text()),
            "bonus_type" => correction.bonus_type = Some(text()),
            "bonus_value" => correction.bonus_value = value.as_integer(),
            "equipped_count" => correction.equipped_count = value.as_integer(),
            unknown => panic!("unknown correction qualifier {unknown:?}"),
        }
    }
}

pub(crate) fn corrections_from(files: &[(&str, &CorrectionSet)]) -> Result<Corrections, String> {
    let mut entries = Vec::new();
    for (file_name, set) in files {
        for correction in &set.0 {
            let mut correction = correction.clone();
            correction.file_name = (*file_name).into();
            correction
                .validate()
                .map_err(|error| format!("correction file {file_name}: {}: {error:#}", correction.label()))?;
            entries.push(correction);
        }
    }
    Ok(Corrections { entries })
}

pub(crate) fn data_build_error(edit: impl FnOnce(&std::path::Path)) -> String {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data");
    let scratch = tempfile::tempdir().unwrap();
    for entry in walkdir::WalkDir::new(&source).into_iter().map(Result::unwrap) {
        if entry.file_type().is_file() {
            let destination = scratch.path().join(entry.path().strip_prefix(&source).unwrap());
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), destination).unwrap();
        }
    }
    edit(scratch.path());
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    match ddo_etl::tree::build_from_data(scratch.path(), &mut db) {
        Ok(_) => String::new(),
        Err(error) => format!("{error:#}").replace(&scratch.path().display().to_string(), "<data>"),
    }
}
