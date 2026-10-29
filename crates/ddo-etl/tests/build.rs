mod fixture;

use ddo_etl::build::{build_database, BuildReport};
use ddo_etl::corrections::Corrections;
use ddo_etl::diff::item_coverage;
use ddo_etl::wiki::WikiOverrides;
use ddo_etl::xml::{challenges, sentient_gems};
use ddo_model::DatasetVersion;
use rusqlite::{params, Connection};
use std::path::PathBuf;

fn data_files_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles")
}

fn fixture_dataset_version() -> DatasetVersion {
    DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-09-20T00:00:00Z".into() }
}

fn built_fixture_db() -> (Connection, BuildReport) {
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        fixture::groupings(),
        &data_files_fixture_dir(),
        &WikiOverrides::from_database(&fixture::database(|_| {})).unwrap(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .expect("build succeeds on fixtures");
    (db, report)
}

fn built_ambiguous_fixture_db() -> (Connection, BuildReport) {
    let source = fixture::xml_source();
    let cases = data_files_fixture_dir().parent().unwrap().join("effect-groups");
    for (file, closing_tag) in [
        ("Augments/Ruby.Augments.xml", "</Augments>"),
        ("Augments/SunAndMoon.Augments.xml", "</Augments>"),
        ("Feats.xml", "</Feats>"),
    ] {
        let path = source.path().join(file);
        let original = std::fs::read_to_string(&path).unwrap();
        let addition = std::fs::read_to_string(cases.join(file)).unwrap();
        std::fs::write(path, original.replace(closing_tag, &format!("{addition}{closing_tag}"))).unwrap();
    }
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        fixture::grouping_cases(),
        source.path(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    (db, report)
}

fn count(db: &Connection, sql: &str) -> i64 {
    db.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn item_id(db: &Connection, name: &str) -> i64 {
    db.query_row("SELECT id FROM items WHERE name = ?1", params![name], |r| r.get(0))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn note_lines_resolve_split_links_and_only_the_qualified_set_tier() {
    let db = fixture::verification_note_database(|root| {
        let path = root.join("packs/free-to-play/the-harpers/quests/search-and-rescue/gear/heartwood.toml");
        let contents = std::fs::read_to_string(&path).unwrap();
        std::fs::write(path, format!("{contents}\n[[verification_request]]\nkind = 'item_bonus'\nfield = 'lines'\nqualifier = 'Sheltering / Insight'\nquestion = 'Are these two separate Insight lines?'\nassumed = 'Separate Physical and Magical Sheltering'\nraised = '2026-10-23'\n[[verification_request.source]]\nurl = 'https://ddowiki.com/page/Item:Heartwood'\nread = '2026-10-23'\nsays = 'Two Insight Sheltering lines'\n")).unwrap();
        let path = root.join("packs/shared/sets/legendary-hruits-influence.toml");
        let contents = std::fs::read_to_string(&path).unwrap();
        std::fs::write(path, format!("{contents}\n[[tier]]\nequipped_count = 4\n[[tier.effect]]\neffect = 'Wisdom'\nbonus_type = 'Artifact'\nvalue = 3\n[[verification_request]]\nkind = 'set_tier_bonus'\nfield = 'value'\nqualifier = 'equipped_count 3, Wisdom / Artifact'\nquestion = 'Is the recorded Wisdom value current?'\nassumed = 3\nraised = '2026-10-23'\n[[verification_request.source]]\nin_game = true\nread = '2026-10-23'\nsays = 'The displayed value is three.'\n")).unwrap();
        let path = root.join("packs/shared/gear/alabaster-of-the-twelve.toml");
        let contents = std::fs::read_to_string(&path).unwrap();
        std::fs::write(path, format!("{contents}\n[[game_bug_correction]]\nkind = 'item_bonus'\nfield = 'remove'\nqualifier = 'Intelligence / Penalty'\nintended = -2\nreason = 'The intended penalty is absent in game.'\nin_game = true\nread = '2026-10-23'\n")).unwrap();
    });
    let heartwood: Vec<i64> = db.prepare("SELECT line.link_sort_order FROM note_lines line JOIN items item ON item.id=line.owner_id WHERE line.owner_kind='item' AND item.name='Heartwood' ORDER BY line.link_sort_order").unwrap().query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert_eq!(heartwood, [0, 1]);
    let tiers: Vec<(i64, String)> = db.prepare("SELECT tier.equipped_count,effect.name FROM note_lines note JOIN set_bonus_tiers tier ON tier.id=note.owner_id JOIN set_bonus_tier_effects line ON line.tier_id=tier.id AND line.sort_order=note.link_sort_order JOIN effects effect ON effect.id=line.effect_id WHERE note.owner_kind='set_bonus_tier'").unwrap().query_map([], |row| Ok((row.get(0)?,row.get(1)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert_eq!(tiers, [(3, "Wisdom".into())]);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM note_lines WHERE note_kind='game_bug'"), 0);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM game_bug_corrections"), 1);
    ddo_etl::tree::rederive_data_build_tables(&db).unwrap();
    assert_eq!(count(&db, "SELECT COUNT(*) FROM note_lines"), 3);
}

#[test]
fn note_lines_include_zero_amounts_and_named_effect_targets() {
    let db = fixture::verification_note_database(|root| {
        let path = root.join("packs/free-to-play/the-harpers/quests/search-and-rescue/gear/heartwood.toml");
        let contents = std::fs::read_to_string(&path).unwrap().replacen("value = 10", "value = 0", 1);
        std::fs::write(path, format!("{contents}\n[[verification_request]]\nkind = 'item_bonus'\nfield = 'value'\nqualifier = 'Physical Sheltering / Insight'\nquestion = 'Does the zero bonus apply?'\nassumed = 0\nraised = '2026-10-23'\n[[verification_request.source]]\nin_game = true\nread = '2026-10-23'\nsays = 'Zero on the line.'\n[[verification_request]]\nkind = 'item_effect'\nfield = 'value'\nqualifier = 'effect \"Repair Amplification\"'\nquestion = 'Is this named line current?'\nassumed = 33\nraised = '2026-10-23'\n[[verification_request.source]]\nin_game = true\nread = '2026-10-23'\nsays = 'Thirty-three on the line.'\n")).unwrap();
    });
    let orders: Vec<i64> = db
        .prepare("SELECT link_sort_order FROM note_lines ORDER BY link_sort_order")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(orders, [0, 2]);
}

#[test]
fn committed_verification_requests_cover_the_initial_review_cases() {
    let mut db = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"), &mut db)
        .unwrap();
    for (owner, field, qualifier, expected_assumed, line_count) in [
        (
            "Epic Death's Rampart",
            "bonus_type",
            "Elemental Resistance / Competence",
            Some(serde_json::json!("Competence")),
            1,
        ),
        (
            "Epic Chain of Conviction",
            "bonus_type",
            "Elemental Resistance / Competence",
            Some(serde_json::json!("Competence")),
            1,
        ),
        ("Dark Bargainer's Armor", "lines", "Physical Sheltering / Enhancement", None, 1),
        ("Hammer of Life", "value", "Natural Armor / Natural Armor", Some(serde_json::json!(2)), 1),
        (
            "Wild Shape: Blighted Wolf",
            "value",
            "Melee and Ranged Attack Speed / Combat Style",
            Some(serde_json::json!(10)),
            1,
        ),
        ("Heartwood", "lines", "Sheltering / Insight", None, 2),
        ("Legendary Heartwood", "lines", "Sheltering / Insight", None, 2),
        ("Legendary Raven's Talons", "value", "Sneak Attack Damage / Enhancement", Some(serde_json::json!(17)), 1),
        ("Cutthroat's Smallblade (level 8)", "", "", None, 0),
        ("Cutthroat's Smallblade (level 12)", "", "", None, 0),
        ("Cutthroat's Smallblade (level 16)", "", "", None, 0),
        ("Epic Cutthroat's Smallblade (level 24)", "", "", None, 0),
    ] {
        let requests = db
            .prepare(
                "SELECT request.assumed_value,
                    (SELECT COUNT(*) FROM note_lines note WHERE note.note_kind = 'unverified' AND note.note_id = request.id)
                 FROM verification_requests request
                 JOIN entity_identity identity ON identity.kind = request.owner_kind AND identity.entity_id = request.owner_id
                 WHERE identity.name = ?1 AND request.field = ?2 AND request.qualifier = ?3",
            )
            .unwrap()
            .query_map(rusqlite::params![owner, field, qualifier], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(requests.len(), 1, "{owner}: the requested review target must be recorded once");
        let assumed: serde_json::Value = serde_json::from_str(&requests[0].0).unwrap();
        if let Some(expected) = expected_assumed {
            assert_eq!(assumed, expected, "{owner}");
        } else {
            assert!(assumed.as_str().is_some_and(|text| !text.is_empty()), "{owner}");
        }
        assert_eq!(requests[0].1, line_count, "{owner}");
    }
}

#[test]
fn plague_wolf_member_removal_precedes_grouping_and_value_correction() {
    let source = fixture::xml_source();
    fixture::write_plague_wolf_feat(source.path());
    let corrections = fixture::corrections_from(&[("plague-wolf.toml", &fixture::plague_wolf_corrections())]).unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        fixture::groupings(),
        source.path(),
        &WikiOverrides::default(),
        &corrections,
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    assert_eq!(fixture::plague_wolf_penalties(&db), [("Charisma".into(), -4, "Penalty".into(), true)]);
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (2, 0));
    assert_eq!(count(&db, "SELECT COUNT(*) FROM effects WHERE name='Intelligence and Charisma'"), 0);
}

#[test]
fn a_structured_set_effect_takes_precedence_over_its_prose_vocabulary() {
    let source = fixture::xml_source();
    let path = source.path().join("SetBonuses.xml");
    let original = std::fs::read_to_string(&path).unwrap();
    assert_eq!(original.matches("<Type>Weapon_AttackAndDamage</Type>").count(), 1);
    std::fs::write(path, original.replace("<Type>Weapon_AttackAndDamage</Type>", "<Type>Weapon_Attack</Type>"))
        .unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        fixture::groupings(),
        source.path(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    assert!(report.matched_set_tier_prose.is_empty());
    let bonuses = db
        .prepare(
            "SELECT stat.name, ob.amount FROM owner_bonuses ob JOIN effects stat ON stat.id = ob.stat_id
         JOIN set_bonus_tiers tier ON tier.id = ob.owner_id JOIN set_bonuses owner ON owner.id = tier.set_id
         WHERE ob.owner_kind = 'set_bonus_tier' AND owner.name = 'Fabricator''s Ingenuity' ORDER BY stat.name",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(bonuses, vec![("Attack Bonus".into(), 2), ("Fortification".into(), 25)]);
}

#[test]
fn valued_prose_reuses_a_stats_existing_description() {
    let source = fixture::xml_source();
    let path = source.path().join("SetBonuses.xml");
    let original = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        path,
        original.replace(
            "</SetBonuses>",
            &format!("{}</SetBonuses>", include_str!("fixtures/effect-groups/ValuedProseSets.xml")),
        ),
    )
    .unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    build_database(
        fixture::groupings(),
        source.path(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    let amounts = db
        .prepare("SELECT amount FROM owner_bonuses WHERE owner_kind = 'set_bonus_tier' AND stat_id = (SELECT id FROM effects WHERE name = 'Seeker') AND bonus_type_id = (SELECT id FROM bonus_types WHERE name = 'Quality') ORDER BY amount")
        .unwrap()
        .query_map([], |row| row.get::<_, i64>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(amounts, [1, 2]);
    let description: String =
        db.query_row("SELECT description_template FROM effects WHERE name = 'Seeker'", [], |row| row.get(0)).unwrap();
    assert!(description.contains("confirm critical hits"), "{description}");
}

#[test]
fn fabricators_valued_prose_links_attack_and_damage_once() {
    let (db, _) = built_fixture_db();
    let links = db
        .prepare(
            "SELECT e.name, bt.name, link.value FROM set_bonus_tier_effects link
         JOIN set_bonus_tiers tier ON tier.id = link.tier_id JOIN set_bonuses owner ON owner.id = tier.set_id
         JOIN effects e ON e.id = link.effect_id LEFT JOIN bonus_types bt ON bt.id = link.bonus_type_id
         WHERE owner.name = 'Fabricator''s Ingenuity' AND tier.equipped_count = 2 ORDER BY link.sort_order",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, Option<i64>>(2)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert!(links.contains(&("Attack and Damage".into(), Some("Artifact".into()), Some(2))), "{links:?}");
    assert_eq!(count(&db, "SELECT COUNT(*) FROM effects WHERE name = '+2 Artifact bonus to attack and damage'"), 0);
}

#[test]
fn unbreakable_adamancy_prose_links_sheltering_once() {
    let canonical = fixture::database(|root| {
        std::fs::remove_file(root.join("effects/sheltering.toml")).unwrap();
        std::fs::write(
            root.join("effects/stat-groups/sheltering.toml"),
            include_str!("../../../data/effects/stat-groups/sheltering.toml"),
        )
        .unwrap();
    });
    let groupings = ddo_etl::groupings::Groupings::from_database(&canonical).unwrap();
    let source = fixture::xml_source();
    let definitions = source.path().join("ItemBuffs.xml");
    let original = std::fs::read_to_string(&definitions).unwrap();
    std::fs::write(
        definitions,
        original
            .replace("</Buffs>", &format!("{}</Buffs>", include_str!("fixtures/effect-groups/Sheltering.Buff.xml"))),
    )
    .unwrap();
    let path = source.path().join("SetBonuses.xml");
    let original = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        path,
        original.replace(
            "</SetBonuses>",
            &format!("{}</SetBonuses>", include_str!("fixtures/effect-groups/UnbreakableAdamancy.SetBonus.xml")),
        ),
    )
    .unwrap();
    std::fs::create_dir_all(source.path().join("SetBonusImages")).unwrap();
    std::fs::write(
        source.path().join("SetBonusImages/UnbreakableAdamancy.png"),
        include_bytes!("fixtures/effect-groups/icons/UnbreakableAdamancy.png"),
    )
    .unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    build_database(
        &groupings,
        source.path(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    let links = db.prepare("SELECT e.name, bt.name, l.value FROM set_bonus_tier_effects l JOIN set_bonus_tiers t ON t.id=l.tier_id JOIN set_bonuses s ON s.id=t.set_id JOIN effects e ON e.id=l.effect_id LEFT JOIN bonus_types bt ON bt.id=l.bonus_type_id WHERE s.name='Unbreakable Adamancy' AND t.equipped_count=2 ORDER BY l.sort_order").unwrap()
        .query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,Option<String>>(1)?,row.get::<_,Option<i64>>(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(links, [("Sheltering".into(), Some("Luck".into()), Some(5))]);
    let bonuses = db.prepare("SELECT stat.name, bt.name, b.amount FROM owner_bonuses b JOIN set_bonus_tiers t ON b.owner_kind='set_bonus_tier' AND t.id=b.owner_id JOIN set_bonuses s ON s.id=t.set_id JOIN effects stat ON stat.id=b.stat_id JOIN bonus_types bt ON bt.id=b.bonus_type_id WHERE s.name='Unbreakable Adamancy' AND t.equipped_count=2 ORDER BY stat.name").unwrap()
        .query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(
        bonuses,
        [
            ("Magical Resistance Rating".into(), "Luck".into(), 5),
            ("Physical Resistance Rating".into(), "Luck".into(), 5)
        ]
    );
    assert_eq!(count(&db, "SELECT COUNT(*) FROM effects WHERE name='Gain a +5 Luck bonus to PRR and MRR'"), 0);
}

#[test]
fn text_only_set_prose_links_the_named_effect_without_an_amount() {
    for has_item_carriers in [true, false] {
        let source = fixture::xml_source();
        fixture::write_prose_sets(source.path());
        if !has_item_carriers {
            for name in ["Sightless.item", "Epic Glimpse of the Soul.item", "The Stablestone.item"] {
                std::fs::remove_file(source.path().join("Items").join(name)).unwrap();
            }
        }
        let mut db = Connection::open_in_memory().unwrap();
        let report = build_database(
            fixture::grouping_cases(),
            source.path(),
            &WikiOverrides::default(),
            &Corrections::default(),
            &mut db,
            &fixture_dataset_version(),
        )
        .unwrap();
        for (owner, pieces, expected) in [
            ("Corsair's Cunning", 2, vec![("Underwater Action", None, None)]),
            ("Through the Mists", 2, vec![("True Seeing", None, None)]),
            (
                "Planar Focus: Subterfuge",
                2,
                vec![
                    ("Dodge", Some("Exceptional"), Some(3)),
                    ("Sneak Attack Damage", Some("Exceptional"), Some(5)),
                    ("True Seeing", None, None),
                ],
            ),
            ("Radiant Shield", 4, vec![("Intercession Ward", None, None)]),
        ] {
            let links = db.prepare("SELECT e.name, bt.name, l.value FROM set_bonus_tier_effects l JOIN set_bonus_tiers t ON t.id=l.tier_id JOIN set_bonuses s ON s.id=t.set_id JOIN effects e ON e.id=l.effect_id LEFT JOIN bonus_types bt ON bt.id=l.bonus_type_id WHERE s.name=?1 AND t.equipped_count=?2 ORDER BY e.name").unwrap()
            .query_map(params![owner,pieces], |row| Ok((row.get::<_,String>(0)?,row.get::<_,Option<String>>(1)?,row.get::<_,Option<i64>>(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
            assert_eq!(
                links,
                expected
                    .into_iter()
                    .map(|(effect, bonus_type, value)| (effect.to_string(), bonus_type.map(str::to_string), value))
                    .collect::<Vec<_>>(),
                "{owner}"
            );
            assert!(
                report
                    .matched_set_tier_prose
                    .iter()
                    .any(|matched| matched.set_name == owner && matched.equipped_count == pieces),
                "{owner}"
            );
        }
        for name in [
            "Underwater action",
            "Gain the effects of True Seeing",
            "You gain True seeing",
            "You have Intercession Ward, and are immune to being Quelled",
        ] {
            assert_eq!(
                db.query_row("SELECT COUNT(*) FROM effects WHERE name=?1", [name], |row| row.get::<_, i64>(0)).unwrap(),
                0,
                "{name}"
            );
        }
        for (name, description) in [
            ("Underwater Action", "This item grants the wearer the ability to breathe water as if it were air."),
            ("True Seeing", "Passive: You gain the benefit of the spell True Seeing."),
            ("Intercession Ward", concat!(
                "Protects the user's connection to a source of Divine power and wards that connection from such things as a ",
                "Quell's intercession ability. This effect only protects from incoming attacks and will not restore a lost connection."
            )),
        ] {
            let actual: String = db.query_row(
                "SELECT description_template FROM effects WHERE name=?1", [name], |row| row.get(0)
            ).unwrap();
            assert_eq!(actual, description, "{name}");
        }
    }
}

#[test]
fn flaming_blast_roman_buff_types_use_the_numeric_enchantment_names() {
    let db = build_item_buff_fixture(
        include_str!("fixtures/effect-groups/FlamingBlast.Buffs.xml"),
        &[
            ("HellfireCrossbow.item", include_str!("fixtures/effect-groups/HellfireCrossbow.item")),
            ("Tinder.item", include_str!("fixtures/effect-groups/Tinder.item")),
            ("EpicHellfireCrossbow.item", include_str!("fixtures/effect-groups/EpicHellfireCrossbow.item")),
        ],
        &[
            (
                "ItemImages/Weapon_RepeatingLightCrossbow/RepeatingLightCrossbow_7.png",
                include_bytes!("fixtures/effect-groups/icons/RepeatingLightCrossbow_7.png"),
            ),
            (
                "ItemImages/Weapon_Quarterstaff/Quarterstaff_5g.png",
                include_bytes!("fixtures/effect-groups/icons/Quarterstaff_5g.png"),
            ),
            (
                "ItemImages/Weapon_RepeatingHeavyCrossbow/RepeatingHeavyCrossbow_2.png",
                include_bytes!("fixtures/effect-groups/icons/RepeatingHeavyCrossbow_2.png"),
            ),
        ],
    );
    for (owner, rank) in [("Hellfire Crossbow", 2), ("Tinder", 3), ("Epic Hellfire Crossbow", 4)] {
        let lines = db.prepare("SELECT e.name,e.verbose_name_template,e.description_template,l.value,l.value2 FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effects e ON e.id=l.effect_id WHERE i.name=?1 AND e.name LIKE 'Flaming Blast %' ORDER BY l.sort_order").unwrap()
            .query_map([owner], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,Option<i64>>(3)?,row.get::<_,Option<i64>>(4)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        let name = format!("Flaming Blast {rank}");
        let description = format!("This weapon burns with deadly magical fire, dealing {rank}d6 Fire damage on a successful hit, and an additional {rank}d6 Fire damage on a critical hit.");
        assert_eq!(lines, [(name.clone(), name, description, None, None)], "{owner}");
    }
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM effects WHERE name IN ('Flaming Blast II','Flaming Blast III','Flaming Blast IV')"
        ),
        0
    );
}

#[test]
fn elusive_target_buff_spellings_share_one_effect() {
    let definitions = format!(
        "{}{}",
        include_str!("fixtures/effect-groups/GorthsMageHand.Buffs.xml"),
        include_str!("fixtures/effect-groups/AzureRotella.Buffs.xml")
    );
    let db = build_item_buff_fixture(
        &definitions,
        &[
            ("AzureRotella.item", include_str!("fixtures/effect-groups/AzureRotella.item")),
            ("GorthsMageHand.item", include_str!("fixtures/effect-groups/GorthsMageHand.item")),
        ],
        &[
            ("ItemImages/Shields/Shield_4l.png", include_bytes!("fixtures/effect-groups/icons/Shield_4l.png")),
            ("ItemImages/Gloves/Gloves_7e.png", include_bytes!("fixtures/effect-groups/icons/Gloves_7e.png")),
        ],
    );
    for owner in ["Azure Rotella", "Gorth's Mage Hand"] {
        let lines = db.prepare("SELECT e.name,e.description_template,l.value,l.value2,l.bonus_type_id FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effects e ON e.id=l.effect_id WHERE i.name=?1 AND e.name LIKE '%Elusive Target%' ORDER BY l.sort_order").unwrap()
            .query_map([owner], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,Option<i64>>(2)?,row.get::<_,Option<i64>>(3)?,row.get::<_,Option<i64>>(4)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        assert_eq!(
            lines,
            [(
                "Feat: Elusive Target".into(),
                "While equipped, this item grants the feat Elusive Target.".into(),
                None,
                None,
                None
            )],
            "{owner}"
        );
    }
    assert_eq!(count(&db, "SELECT COUNT(*) FROM effects WHERE name='Feat Elusive Target'"), 0);
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM effects WHERE name='Feat: Elusive Target' AND slug='feat-elusive-target-2'"),
        1
    );
}

#[test]
fn shifted_effect_ids_keep_catalog_variants_and_citations() {
    let db = build_item_buff_fixture(
        include_str!("fixtures/effect-groups/GorthsMageHand.Buffs.xml"),
        &[("GorthsMageHand.item", include_str!("fixtures/effect-groups/GorthsMageHand.item"))],
        &[("ItemImages/Gloves/Gloves_7e.png", include_bytes!("fixtures/effect-groups/icons/Gloves_7e.png"))],
    );
    let (id, slug, variant): (i64, String, Option<String>) = db
        .query_row("SELECT id, slug, variant FROM effects WHERE name = 'Feat: Elusive Target'", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap();
    assert_ne!(id, 1380);
    assert_eq!(slug, "feat-elusive-target-2");
    assert_eq!(variant.as_deref(), Some("2"));
    assert_eq!(count(&db, "SELECT COUNT(*) FROM citations c JOIN effects e ON e.id = c.owner_id WHERE c.owner_kind = 'effects' AND e.name = 'Feat: Elusive Target'"), 1);
}

#[test]
fn fixed_equipment_groupings_take_their_bonus_type_from_item_links() {
    let source = fixture::xml_source();
    let buffs = source.path().join("ItemBuffs.xml");
    let original = std::fs::read_to_string(&buffs).unwrap();
    std::fs::write(
        buffs,
        original.replace("</Buffs>", &format!("{}</Buffs>", include_str!("fixtures/effect-groups/ItemBuffs.xml"))),
    )
    .unwrap();
    let item = source.path().join("Items/Legendary Cloak of Winter.item");
    let original = std::fs::read_to_string(&item).unwrap();
    std::fs::write(
        item,
        original.replace(
            "  </Item>",
            "<Buff><Type>Silver Flame Lore</Type><Value1>6</Value1></Buff><Buff><Type>PowerOfTheDarkRestoration</Type><Value1>80</Value1></Buff></Item>",
        ),
    )
    .unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    build_database(
        fixture::groupings(),
        source.path(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    for (name, value, stats) in [
        (
            "Silver Flame Lore",
            6,
            vec![
                "Chaos Spell Lore",
                "Evil Spell Lore",
                "Good Spell Lore",
                "Law Spell Lore",
                "Light Spell Lore",
                "Positive Spell Lore",
            ],
        ),
        (
            "Power of the Dark Restoration",
            80,
            vec!["Negative Spell Power", "Poison Spell Power", "Positive Spell Power"],
        ),
    ] {
        let (effect_id, linked_type, home_type): (i64, Option<String>, Option<String>) = db
            .query_row(
                "SELECT e.id, bt.name, home.name FROM item_effects link JOIN effects e ON e.id = link.effect_id
             JOIN items i ON i.id = link.item_id LEFT JOIN bonus_types bt ON bt.id = link.bonus_type_id
             LEFT JOIN bonus_types home ON home.id = e.home_bonus_type_id
             WHERE i.name = 'Legendary Cloak of Winter' AND e.name = ?1",
                [name],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let description: String = db
            .query_row("SELECT description_template FROM effects WHERE id = ?1", [effect_id], |row| row.get(0))
            .unwrap();
        assert!(description.contains("%b1"), "{name}: {description}");
        let slug = name.to_lowercase().replace(' ', "-");
        let canonical: toml::Value = toml::from_str(
            &std::fs::read_to_string(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../data/effects/{slug}.toml")),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(canonical["description_template"].as_str(), Some(description.as_str()), "{name}");
        assert_eq!(linked_type.as_deref(), Some("Equipment"), "{name}");
        assert_eq!(home_type.as_deref(), Some("Equipment"), "{name}");
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM effect_bonuses WHERE effect_id = ?1 AND bonus_type_id IS NOT NULL",
                [effect_id],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0,
            "{name}"
        );
        let bonuses = db
            .prepare(
                "SELECT s.name, bt.name, ob.amount FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
             JOIN bonus_types bt ON bt.id = ob.bonus_type_id WHERE ob.owner_kind = 'item' AND ob.via_effect_id = ?1
             AND ob.owner_id = (SELECT id FROM items WHERE name = 'Legendary Cloak of Winter')
             ORDER BY s.name",
            )
            .unwrap()
            .query_map([effect_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            bonuses,
            stats.into_iter().map(|stat| (stat.to_string(), "Equipment".to_string(), value)).collect::<Vec<_>>()
        );
    }
}

#[test]
fn generated_mixed_unit_groups_keep_each_members_unit() {
    let canonical = fixture::database(|root| {
        std::fs::remove_file(root.join("effects/stat-groups/melee-power-and-doublestrike.toml")).unwrap();
        let set = root.join("packs/shared/sets/inevitable-balance.toml");
        let contents = std::fs::read_to_string(&set).unwrap();
        std::fs::write(
            set,
            format!(
                "{}\n[[tier.effect]]\nbonus_type = 'Stacking'\neffect = 'Doublestrike'\nvalue = 5\n",
                contents.replace("Melee Power and Doublestrike", "Melee Power")
            ),
        )
        .unwrap();
    });
    let groupings = ddo_etl::groupings::Groupings::from_database(&canonical).unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    build_database(
        &groupings,
        &data_files_fixture_dir(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    let templates: (String, String, String) = db.query_row(
        "SELECT verbose_name_template, COALESCE(set_bonus_line_template, ''), description_template FROM effects WHERE name = 'Melee Power and Doublestrike'",
        [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).unwrap();
    assert_eq!(templates.0, "+{1} %b1 Bonus to Melee Power and +{1}% %b1 Bonus to Doublestrike");
    assert_eq!(templates.1, templates.0);
    assert_eq!(templates.2, "+{1} %b1 bonus to Melee Power and +{1}% %b1 bonus to Doublestrike.");
}

#[test]
fn a_new_lore_group_keeps_percent_when_c_has_no_stat_template() {
    let canonical = fixture::database(|root| {
        std::fs::remove_file(root.join("packs/shared/sets/legendary-hruits-influence.toml")).unwrap();
        std::fs::remove_file(root.join("effects/stat-groups/fire-cold-electric-and-positive-spell-lore.toml")).unwrap();
    });
    let groupings = ddo_etl::groupings::Groupings::from_database(&canonical).unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    build_database(
        &groupings,
        &data_files_fixture_dir(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    let template: String = db
        .query_row(
            "SELECT verbose_name_template FROM effects WHERE name = 'Fire, Cold, Electric and Positive Spell Lore'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(template, "%b1 Fire, Cold, Electric and Positive Spell Lore +{1}%");
}

#[test]
fn hruits_single_effect_grants_four_spell_powers_on_one_line() {
    let (db, _) = built_fixture_db();
    let tier: i64 = db
        .query_row(
            "SELECT t.id FROM set_bonus_tiers t JOIN set_bonuses s ON s.id = t.set_id
         WHERE s.name = 'Legendary Hruit''s Influence' AND t.equipped_count = 3",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM set_bonus_tier_effects WHERE tier_id = ?1", [tier], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        4
    );
    let bonuses = db
        .prepare(
            "SELECT s.name, b.name, ob.amount, ob.via_effect_id FROM owner_bonuses ob
         JOIN effects s ON s.id = ob.stat_id JOIN bonus_types b ON b.id = ob.bonus_type_id
         WHERE ob.owner_kind = 'set_bonus_tier' AND ob.owner_id = ?1 AND s.name LIKE '%Spell Power' ORDER BY s.name",
        )
        .unwrap()
        .query_map([tier], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(
        bonuses
            .iter()
            .map(|(name, bonus_type, amount, _)| (name.as_str(), bonus_type.as_str(), *amount))
            .collect::<Vec<_>>(),
        vec![
            ("Cold Spell Power", "Artifact", 30),
            ("Electric Spell Power", "Artifact", 30),
            ("Fire Spell Power", "Artifact", 30),
            ("Positive Spell Power", "Artifact", 30),
        ]
    );
    assert!(bonuses.iter().all(|bonus| bonus.3 == bonuses[0].3));
    let nested_groups = db
        .prepare(
            "SELECT DISTINCT target.name FROM effect_bonuses b JOIN effects target ON target.id = b.target_effect_id
         JOIN effects owner ON owner.id = b.effect_id WHERE target.is_group = 1 AND owner.is_group = 0",
        )
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let declared_groups = ddo_etl::map::effect_map::EFFECT_MAP.group_names();
    assert!(nested_groups.iter().all(|name| declared_groups.contains(name)), "{nested_groups:?}");
}

#[test]
fn avalanche_uses_the_existing_frozen_thunderstorm_effect() {
    let (db, _) = built_fixture_db();
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM set_bonus_tier_effects link JOIN effects e ON e.id = link.effect_id
         JOIN set_bonus_tiers t ON t.id = link.tier_id JOIN set_bonuses s ON s.id = t.set_id
         WHERE s.name = 'Anger of the Avalanche' AND e.name = 'Power of the Frozen Thunderstorm'"
        ),
        1
    );
}

#[test]
fn scion_keeps_tactical_and_assassinate_dcs_on_one_line() {
    let source = fixture::xml_source();
    fixture::write_scion_feat(source.path());
    let mut db = Connection::open_in_memory().unwrap();
    build_database(
        fixture::grouping_cases(),
        source.path(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    let links = db.prepare(
        "SELECT DISTINCT e.name,l.value FROM feat_effects l JOIN effects e ON e.id=l.effect_id
         JOIN feats f ON f.id=l.feat_id JOIN owner_bonuses b ON b.owner_kind='feat' AND b.owner_id=f.id AND b.via_effect_id=e.id
         JOIN effects stat ON stat.id=b.stat_id WHERE f.name='Scion of the Astral Plane' AND stat.name IN ('Trip DC','Sunder DC','Stun DC','Assassinate DC')",
    ).unwrap().query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?)))
        .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(links, [("Trip, Sunder, Stun and Assassinate DC".into(), 4)]);
}

#[test]
fn combat_mastery_composition_and_cited_dolorous_splits_follow_the_xml_writer() {
    let source = fixture::xml_source();
    let buffs = source.path().join("ItemBuffs.xml");
    let original = std::fs::read_to_string(&buffs).unwrap();
    std::fs::write(
        buffs,
        original.replace(
            "</Buffs>",
            &format!("{}</Buffs>", include_str!("fixtures/effect-groups/BaphometsTactics.Buff.xml")),
        ),
    )
    .unwrap();
    std::fs::write(
        source.path().join("Augments/DolorousInvigorator.Augments.xml"),
        include_str!("fixtures/effect-groups/DolorousInvigorator.Augments.xml"),
    )
    .unwrap();
    std::fs::write(
        source.path().join("Items/EchoOfHeartcleaver.item"),
        include_str!("fixtures/effect-groups/EchoOfHeartcleaver.item"),
    )
    .unwrap();
    let mut corrections =
        Corrections::from_data(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap();
    corrections.entries.retain(|entry| {
        matches!(entry.name.as_str(), "Dolorous Invigorator (Heroic)" | "Dolorous Invigorator (Legendary)")
    });
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        fixture::grouping_cases(),
        source.path(),
        &WikiOverrides::default(),
        &corrections,
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    assert_eq!(report.correction_stale_count, 0, "{:?}", report.stale_corrections);
    assert_eq!(report.line_splits.len(), 2);
    for tier in ["Heroic", "Legendary"] {
        let owner = format!("Dolorous Invigorator ({tier})");
        let lines = db.prepare("SELECT e.name FROM augment_effects l JOIN effects e ON e.id=l.effect_id JOIN augments a ON a.id=l.augment_id WHERE a.name=? ORDER BY l.sort_order")
            .unwrap().query_map([owner], |row| row.get::<_,String>(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        assert_eq!(lines, ["Spell Focus Mastery", "Combat Mastery", "Assassinate"]);
    }
    let bonuses = db.prepare("SELECT target.name,bt.name,b.constant FROM effect_bonuses b JOIN effects e ON e.id=b.effect_id JOIN effects target ON target.id=b.target_effect_id JOIN bonus_types bt ON bt.id=b.bonus_type_id WHERE e.name=\"Baphomet's Tactics\"")
        .unwrap().query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?)))
        .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(bonuses, [("Combat Mastery".into(), "Enhancement".into(), 13)]);
}

#[test]
fn declared_damage_targets_preserve_thunderforged_stats_and_requirements() {
    let source = fixture::xml_source();
    std::fs::write(
        source.path().join("Augments/ThunderforgedDamageTargets.Augments.xml"),
        include_str!("fixtures/effect-groups/ThunderforgedDamageTargets.Augments.xml"),
    )
    .unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    build_database(
        fixture::grouping_cases(),
        source.path(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    let bonuses: Vec<(String, String, String, i64)> = db
        .prepare(
            "SELECT a.name,s.name,bt.name,b.amount FROM owner_bonuses b
         JOIN augments a ON a.id=b.owner_id JOIN effects s ON s.id=b.stat_id
         JOIN bonus_types bt ON bt.id=b.bonus_type_id WHERE b.owner_kind='augment'
         AND a.name IN ('Blinding Fear','Dwarvencraft Acid Lore') ORDER BY a.name,s.name",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let mut expected: Vec<_> = [
        "Acid Resistance",
        "Cold Resistance",
        "Electric Resistance",
        "Fire Resistance",
        "Saving Throws",
        "Sonic Resistance",
    ]
    .map(|stat| ("Blinding Fear".to_string(), stat.to_string(), "Orb".to_string(), 10))
    .into();
    expected.push(("Dwarvencraft Acid Lore".into(), "Acid Spell Lore".into(), "Equipment".into(), 22));
    assert_eq!(bonuses, expected);
    assert_eq!(count(&db, "SELECT COUNT(DISTINCT b.via_effect_id) FROM owner_bonuses b JOIN augments a ON a.id=b.owner_id JOIN effects s ON s.id=b.stat_id WHERE b.owner_kind='augment' AND a.name='Blinding Fear' AND s.name LIKE '% Resistance'"), 1);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM requirements r JOIN modifiers m ON r.owner_kind='modifier' AND r.owner_id=m.id JOIN augments a ON m.source_kind='augment' AND m.source_id=a.id WHERE a.name='Blinding Fear' AND m.effect_type IN ('SaveBonus','EnergyResistance') AND r.req_type='Stance' AND r.items='[\"Blocking\"]'"), 2);
}

#[test]
fn weapon_damage_is_preserved_outside_declared_damage_targets() {
    let source = fixture::xml_source();
    std::fs::write(
        source.path().join("Augments/WeaponFireDamage.Augments.xml"),
        include_str!("fixtures/effect-groups/ThunderforgedDamageTargets.Augments.xml")
            .replace("<Type>EnergyResistance</Type>", "<Type>WeaponOtherDamageBonus</Type>")
            .replace("<Damage>Acid</Damage>", "<Damage>Fire</Damage>"),
    )
    .unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    build_database(
        fixture::grouping_cases(),
        source.path(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    let stats: Vec<String> = db.prepare("SELECT s.name FROM owner_bonuses b JOIN augments a ON a.id=b.owner_id JOIN effects s ON s.id=b.stat_id WHERE b.owner_kind='augment' AND a.name='Blinding Fear'").unwrap()
        .query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert_eq!(stats, ["Saving Throws"]);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM modifiers m JOIN augments a ON a.id=m.source_id WHERE m.source_kind='augment' AND a.name='Blinding Fear' AND m.effect_type='WeaponOtherDamageBonus' AND m.damage='Fire'"), 1);
}

#[test]
fn orb_bonus_grants_saves_and_five_resistances_from_its_definition() {
    let db = build_item_buff_fixture(
        include_str!("fixtures/effect-groups/OrbOfAcid.Buffs.xml"),
        &[("OrbOfAcid.item", include_str!("fixtures/effect-groups/OrbOfAcid.item"))],
        &[("ItemImages/Orbs/Orb_1b.png", include_bytes!("fixtures/effect-groups/icons/Orb_1b.png"))],
    );
    let bonuses = db.prepare("SELECT s.name,bt.name,b.amount FROM owner_bonuses b JOIN effects s ON s.id=b.stat_id JOIN effects e ON e.id=b.via_effect_id JOIN bonus_types bt ON bt.id=b.bonus_type_id JOIN items i ON i.id=b.owner_id WHERE b.owner_kind='item' AND i.name='Orb of Acid' AND e.name='Orb Bonus' ORDER BY s.name").unwrap()
        .query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?)))
        .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    let expected = [
        "Acid Resistance",
        "Cold Resistance",
        "Electric Resistance",
        "Fire Resistance",
        "Saving Throws",
        "Sonic Resistance",
    ]
    .map(|stat| (stat.to_string(), "Orb".to_string(), 6));
    assert_eq!(bonuses, expected);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effects e ON e.id=l.effect_id WHERE i.name='Orb of Acid' AND e.name='Orb Bonus' AND l.value=6"), 1);
    let home_types = db.prepare("SELECT e.name,bt.name FROM effects e LEFT JOIN bonus_types bt ON bt.id=e.home_bonus_type_id WHERE e.name IN ('Acid Resistance','Cold Resistance','Electric Resistance','Fire Resistance','Sonic Resistance','Spell DCs') ORDER BY e.name").unwrap()
        .query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,Option<String>>(1)?)))
        .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(
        home_types,
        [
            ("Acid Resistance".into(), Some("Enhancement".into())),
            ("Cold Resistance".into(), Some("Enhancement".into())),
            ("Electric Resistance".into(), Some("Enhancement".into())),
            ("Fire Resistance".into(), Some("Enhancement".into())),
            ("Sonic Resistance".into(), Some("Enhancement".into())),
            ("Spell DCs".into(), None),
        ]
    );
}

fn build_item_buff_fixture(definitions: &str, items: &[(&str, &str)], icons: &[(&str, &[u8])]) -> Connection {
    let source = fixture::xml_source();
    let buffs = source.path().join("ItemBuffs.xml");
    let original = std::fs::read_to_string(&buffs).unwrap();
    std::fs::write(buffs, original.replace("</Buffs>", &format!("{definitions}</Buffs>"))).unwrap();
    for (name, contents) in items {
        std::fs::write(source.path().join("Items").join(name), contents).unwrap();
    }
    for (path, icon) in icons {
        let destination = source.path().join(path);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::write(destination, icon).unwrap();
    }
    let mut db = Connection::open_in_memory().unwrap();
    build_database(
        fixture::grouping_cases(),
        source.path(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    db
}

#[test]
fn touch_of_immortality_composes_heal_and_repair_without_losing_regeneration() {
    let db = build_item_buff_fixture(
        include_str!("fixtures/effect-groups/GauntletsOfImmortality.Buffs.xml"),
        &[("GauntletsOfImmortality.item", include_str!("fixtures/effect-groups/GauntletsOfImmortality.item"))],
        &[
            ("ItemImages/Gloves/Gloves_13.png", include_bytes!("fixtures/effect-groups/icons/Gloves_13.png")),
            ("AugmentImages/Heroism.png", include_bytes!("fixtures/effect-groups/icons/Heroism.png")),
        ],
    );
    let rows = db
        .prepare(
            "SELECT target.name,bt.name,b.amount_from,b.constant FROM effect_bonuses b
             JOIN effects e ON e.id=b.effect_id JOIN effects target ON target.id=b.target_effect_id
             JOIN bonus_types bt ON bt.id=b.bonus_type_id WHERE e.name='Touch of Immortality'",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(rows, [("Heal and Repair".into(), "Competence".into(), 0, 20)]);
    let granted = db
        .prepare(
            "SELECT stat.name,bt.name,b.amount FROM owner_bonuses b
             JOIN items i ON i.id=b.owner_id JOIN effects stat ON stat.id=b.stat_id
             JOIN effects e ON e.id=b.via_effect_id JOIN effects g ON g.id=b.group_effect_id
             JOIN bonus_types bt ON bt.id=b.bonus_type_id
             WHERE b.owner_kind='item' AND i.name='Gauntlets of Immortality'
             AND e.name='Touch of Immortality' AND g.name='Heal and Repair' ORDER BY stat.name",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(granted, ["Heal", "Repair"].map(|name| (name.into(), "Competence".into(), 20)));
    let description: String = db
        .query_row("SELECT description_template FROM effects WHERE name='Touch of Immortality'", [], |row| row.get(0))
        .unwrap();
    assert!(description.contains("regenerate 2-4 Hit Points every 15 seconds"));
    let amounts: (Option<i64>, Option<i64>) = db
        .query_row(
            "SELECT l.value,l.value2 FROM item_effects l JOIN items i ON i.id=l.item_id
             JOIN effects e ON e.id=l.effect_id WHERE i.name='Gauntlets of Immortality' AND e.name='Touch of Immortality'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(amounts, (None, None));
    let home_type: Option<i64> = db
        .query_row("SELECT home_bonus_type_id FROM effects WHERE name='Heal and Repair'", [], |row| row.get(0))
        .unwrap();
    assert_eq!(home_type, None);
}

#[test]
fn threat_composes_preserve_their_opposite_constants_and_fixed_types() {
    let db = build_item_buff_fixture(
        include_str!("fixtures/effect-groups/ThreatComposes.Buffs.xml"),
        &[
            ("DarkDiversion.item", include_str!("fixtures/effect-groups/DarkDiversion.item")),
            ("RingOfLies.item", include_str!("fixtures/effect-groups/RingOfLies.item")),
        ],
        &[
            ("ItemImages/Necklace/Necklace_17.png", include_bytes!("fixtures/effect-groups/icons/Necklace_17.png")),
            ("ItemImages/Rings/Ring_11b.png", include_bytes!("fixtures/effect-groups/icons/Ring_11b.png")),
            ("AugmentImages/Stormreaver.png", include_bytes!("fixtures/effect-groups/icons/Stormreaver.png")),
        ],
    );
    for (effect, owner, bonus_type, amount, prose) in [
        (
            "Hidden Effect All Threat",
            "Ring of Lies",
            "Penalty",
            15,
            "Increases spellcasting, melee, and ranged threat by 15%",
        ),
        (
            "Occultation",
            "Dark Diversion",
            "Enhancement",
            -20,
            "20% Enhancement bonus to Decreased threat generated from all damage.",
        ),
    ] {
        let rows = db
            .prepare(
                "SELECT target.name,bt.name,b.amount_from,b.constant FROM effect_bonuses b
                 JOIN effects e ON e.id=b.effect_id JOIN effects target ON target.id=b.target_effect_id
                 JOIN bonus_types bt ON bt.id=b.bonus_type_id WHERE e.name=?1",
            )
            .unwrap()
            .query_map([effect], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            rows,
            [("Melee, Spell and Ranged Threat Generation".into(), bonus_type.into(), 0, amount)],
            "{effect}"
        );
        let granted = db
            .prepare(
                "SELECT stat.name,bt.name,b.amount FROM owner_bonuses b
                 JOIN items i ON i.id=b.owner_id JOIN effects stat ON stat.id=b.stat_id
                 JOIN effects e ON e.id=b.via_effect_id JOIN effects g ON g.id=b.group_effect_id
                 JOIN bonus_types bt ON bt.id=b.bonus_type_id
                 WHERE b.owner_kind='item' AND i.name=?1 AND e.name=?2
                 AND g.name='Melee, Spell and Ranged Threat Generation' ORDER BY stat.name",
            )
            .unwrap()
            .query_map([owner, effect], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            granted,
            ["Melee Threat Generation", "Ranged Threat Generation", "Spell Threat Generation"].map(|stat| (
                stat.to_string(),
                bonus_type.to_string(),
                amount
            )),
            "{effect}"
        );
        let description: String =
            db.query_row("SELECT description_template FROM effects WHERE name=?1", [effect], |row| row.get(0)).unwrap();
        assert_eq!(description, prose);
        let amounts: (Option<i64>, Option<i64>) = db
            .query_row(
                "SELECT l.value,l.value2 FROM item_effects l JOIN items i ON i.id=l.item_id
                 JOIN effects e ON e.id=l.effect_id WHERE i.name=?1 AND e.name=?2",
                [owner, effect],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(amounts, (None, None));
    }
    let home_type: Option<i64> = db
        .query_row(
            "SELECT home_bonus_type_id FROM effects WHERE name='Melee, Spell and Ranged Threat Generation'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(home_type, None);
}

#[test]
fn overfocus_composes_search_and_spot_with_its_constant_penalty() {
    let db = build_item_buff_fixture(
        include_str!("fixtures/effect-groups/Overfocus.Buffs.xml"),
        &[("LegendaryDemonsArc.item", include_str!("fixtures/effect-groups/LegendaryDemonsArc.item"))],
        &[("ItemImages/Weapon_Longbow/Longbow_9.png", include_bytes!("fixtures/effect-groups/icons/Longbow_9.png"))],
    );
    let definitions = db
        .prepare(
            "SELECT target.name,bt.name,b.amount_from,b.constant FROM effect_bonuses b
             JOIN effects e ON e.id=b.effect_id JOIN effects target ON target.id=b.target_effect_id
             JOIN bonus_types bt ON bt.id=b.bonus_type_id WHERE e.name='Overfocus'",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(definitions, [("Search and Spot".into(), "Penalty".into(), 0, -10)]);
    let bonuses = db
        .prepare(
            "SELECT stat.name,bt.name,b.amount FROM owner_bonuses b
             JOIN items i ON i.id=b.owner_id JOIN effects stat ON stat.id=b.stat_id
             JOIN effects e ON e.id=b.via_effect_id JOIN effects g ON g.id=b.group_effect_id
             JOIN bonus_types bt ON bt.id=b.bonus_type_id
             WHERE b.owner_kind='item' AND i.name='Legendary Demon''s Arc' AND e.name='Overfocus'
             AND g.name='Search and Spot' ORDER BY stat.name",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(bonuses, [("Search".into(), "Penalty".into(), -10), ("Spot".into(), "Penalty".into(), -10)]);
    let amounts: (Option<i64>, Option<i64>) = db
        .query_row(
            "SELECT l.value,l.value2 FROM item_effects l JOIN items i ON i.id=l.item_id
             JOIN effects e ON e.id=l.effect_id WHERE i.name='Legendary Demon''s Arc' AND e.name='Overfocus'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(amounts, (None, None));
    let description: String =
        db.query_row("SELECT description_template FROM effects WHERE name='Overfocus'", [], |row| row.get(0)).unwrap();
    assert_eq!(description, "This item helps you focus your attention, but it makes you somewhat oblivious to your surroundings, causing a -10 penalty to Search and Spot.");
    let home_type: Option<i64> = db
        .query_row("SELECT home_bonus_type_id FROM effects WHERE name='Search and Spot'", [], |row| row.get(0))
        .unwrap();
    assert_eq!(home_type, None);
}

#[test]
fn protection_from_evil_keeps_the_spell_name_without_a_rank() {
    let db = build_item_buff_fixture(
        include_str!("fixtures/effect-groups/ProtectionFromEvil.Buffs.xml"),
        &[("FragmentOfTheSilverFlame.item", include_str!("fixtures/effect-groups/FragmentOfTheSilverFlame.item"))],
        &[("ItemImages/Trinkets/Trinket_2.png", include_bytes!("fixtures/effect-groups/icons/Trinket_2.png"))],
    );
    let (title, description): (String, String) = db
        .query_row(
            "SELECT verbose_name_template,description_template FROM effects WHERE name='Protection from Evil'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(title, "Protection from Evil");
    assert_eq!(description, "Passive: You gain the benefit of the spell Protection from Evil.");
    let bonuses = db
        .prepare(
            "SELECT stat.name,bt.name,b.amount FROM owner_bonuses b
             JOIN items i ON i.id=b.owner_id JOIN effects stat ON stat.id=b.stat_id
             JOIN effects e ON e.id=b.via_effect_id JOIN bonus_types bt ON bt.id=b.bonus_type_id
             WHERE b.owner_kind='item' AND i.name='Fragment of the Silver Flame'
             AND e.name='Protection from Evil' ORDER BY stat.name",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(bonuses, [("Armor Class".into(), "Deflection".into(), 2)]);
}

#[test]
fn item_named_routes_load_unowned_canonical_definitions_before_writing_owners() {
    let canonical = fixture::riposte_rank_database();
    let index = ddo_etl::groupings::Groupings::from_database(&canonical).unwrap();
    let source = fixture::xml_source();
    let mut db = Connection::open_in_memory().unwrap();
    build_database(
        &index,
        source.path(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    let definitions = |db: &Connection| {
        db.prepare("SELECT e.name,e.description_template,e.stat_set_verdict,e.stat_set_reason,s.name,b.constant,b.amount_from,bt.name FROM effects e JOIN effect_bonuses b ON b.effect_id=e.id JOIN effects s ON s.id=b.target_effect_id JOIN bonus_types bt ON bt.id=b.bonus_type_id WHERE e.name='Riposte IV' ORDER BY b.sort_order").unwrap().query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,i64>(5)?,row.get::<_,i64>(6)?,row.get::<_,String>(7)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
    };
    assert_eq!(definitions(&canonical).len(), 2);
    assert_eq!(definitions(&db), definitions(&canonical));
    let links: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM item_effects WHERE effect_id=(SELECT id FROM effects WHERE name='Riposte IV')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(links, 0);
}

#[test]
fn an_item_named_route_without_canonical_definitions_fails_before_owners() {
    let canonical = fixture::riposte_rank_database();
    canonical
        .execute("DELETE FROM item_effects WHERE effect_id IN (SELECT id FROM effects WHERE name LIKE 'Riposte %')", [])
        .unwrap();
    canonical
        .execute(
            "DELETE FROM effect_bonuses WHERE effect_id IN (SELECT id FROM effects WHERE name LIKE 'Riposte %')",
            [],
        )
        .unwrap();
    canonical.execute("DELETE FROM effects WHERE name LIKE 'Riposte %'", []).unwrap();
    let index = ddo_etl::groupings::Groupings::from_database(&canonical).unwrap();
    let source = fixture::xml_source();
    let mut db = Connection::open_in_memory().unwrap();
    let error = build_database(
        &index,
        source.path(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("family.item_named_effects") && message.contains("Riposte {item}"), "{message}");
    let owners: i64 = db.query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0)).unwrap();
    assert_eq!(owners, 0);
}

#[test]
fn riposte_item_rank_routes_to_the_canonical_constant_effect() {
    let index = ddo_etl::groupings::Groupings::from_database(&fixture::riposte_rank_database()).unwrap();
    let source = fixture::xml_source();
    fixture::write_ship_kickers(source.path());
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        &index,
        source.path(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    assert!(report.stale_corrections.is_empty());
    let link: (String,Option<i64>,Option<i64>) = db.query_row("SELECT e.name,l.value,l.value2 FROM item_effects l JOIN effects e ON e.id=l.effect_id JOIN items i ON i.id=l.item_id WHERE i.name='Epic Ethereal Bracers' AND e.name LIKE 'Riposte%'", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    assert_eq!(link, ("Riposte IX".into(), None, None));
    let bonuses = db.prepare("SELECT stat.name,b.amount FROM owner_bonuses b JOIN effects stat ON stat.id=b.stat_id JOIN effects e ON e.id=b.via_effect_id JOIN items i ON i.id=b.owner_id WHERE b.owner_kind='item' AND i.name='Epic Ethereal Bracers' AND e.name='Riposte IX' ORDER BY stat.name").unwrap().query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(bonuses, [("Armor Class".into(), 5), ("Saving Throws".into(), 4)]);
    let generic: (String,Option<i64>) = db.query_row("SELECT e.name,l.value FROM item_effects l JOIN effects e ON e.id=l.effect_id JOIN items i ON i.id=l.item_id WHERE i.name='Ship Kickers' AND e.name LIKE 'Riposte%'", [], |row| Ok((row.get(0)?,row.get(1)?))).unwrap();
    assert_eq!(generic, ("Riposte".into(), Some(3)));
}

#[test]
fn numeric_riposte_keeps_full_owner_and_default_amounts() {
    let canonical = fixture::riposte_rank_database();
    let bonuses = |db: &Connection| {
        db.prepare(
            "SELECT stat.name,b.amount,b.amount_source,b.scale FROM owner_bonuses b
             JOIN items item ON item.id=b.owner_id JOIN effects stat ON stat.id=b.stat_id
             JOIN effects effect ON effect.id=b.via_effect_id
             WHERE b.owner_kind='item' AND item.name='Ship Kickers' AND effect.name='Riposte'
             ORDER BY stat.name",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?, row.get::<_, f64>(3)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
    };
    assert_eq!(
        bonuses(&canonical),
        [("Armor Class".into(), 3, "owner".into(), 1.0), ("Saving Throws".into(), 3, "owner".into(), 1.0)]
    );
    let verdict: String =
        canonical.query_row("SELECT stat_set_verdict FROM effects WHERE name='Riposte'", [], |row| row.get(0)).unwrap();
    assert_eq!(verdict, "distinct");
    let index = ddo_etl::groupings::Groupings::from_database(&canonical).unwrap();
    let corrections = Corrections::from_database(&canonical).unwrap();
    for (amount, expected, amount_source) in
        [(Some(3), 3, "owner"), (Some(1), 1, "owner"), (None, 2, "default"), (Some(0), 0, "owner")]
    {
        let source = fixture::xml_source();
        let item = source.path().join("Items/ShipKickers.item");
        let original = std::fs::read_to_string(&item).unwrap();
        let replacement = amount.map_or_else(String::new, |amount| format!("<Value1>{amount}</Value1>"));
        assert_eq!(original.matches("<Value1>3</Value1>").count(), 1);
        std::fs::write(item, original.replace("<Value1>3</Value1>", &replacement)).unwrap();
        let mut upstream = Connection::open_in_memory().unwrap();
        build_database(
            &index,
            source.path(),
            &WikiOverrides::default(),
            &corrections,
            &mut upstream,
            &fixture_dataset_version(),
        )
        .unwrap();
        let expected_bonuses = if expected == 0 {
            Vec::new()
        } else {
            ["Armor Class", "Saving Throws"]
                .map(|stat| (stat.to_string(), expected, amount_source.to_string(), 1.0))
                .to_vec()
        };
        assert_eq!(bonuses(&upstream), expected_bonuses, "amount {amount:?}");
    }
}

#[test]
fn riposte_item_rank_missing_from_the_index_names_the_buff_and_owner() {
    let source = fixture::xml_source();
    let canonical = fixture::riposte_rank_database();
    canonical
        .execute("DELETE FROM item_effects WHERE effect_id=(SELECT id FROM effects WHERE name='Riposte IX')", [])
        .unwrap();
    canonical
        .execute("DELETE FROM effect_bonuses WHERE effect_id=(SELECT id FROM effects WHERE name='Riposte IX')", [])
        .unwrap();
    canonical.execute("DELETE FROM effects WHERE name='Riposte IX'", []).unwrap();
    let index = ddo_etl::groupings::Groupings::from_database(&canonical).unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    let error = build_database(
        &index,
        source.path(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap_err();
    let message = format!("{error:#}");
    for expected in ["Riposte IX", "Riposte", "Epic Ethereal Bracers"] {
        assert!(message.contains(expected), "{message}");
    }
}

#[test]
fn roman_riposte_ranks_keep_direct_constants_and_cited_composition_declarations() {
    let mut db = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"), &mut db)
        .unwrap();
    for (rank, armor_class, saves) in [
        ("IV", 2, 2),
        ("V", 3, 2),
        ("VI", 3, 3),
        ("VII", 4, 3),
        ("VIII", 4, 4),
        ("IX", 5, 4),
        ("X", 5, 5),
        ("XI", 6, 5),
        ("XII", 6, 6),
    ] {
        let name = format!("Riposte {rank}");
        let rows = db.prepare("SELECT target.name,b.constant,b.amount_from,target.is_stat,bt.name FROM effect_bonuses b JOIN effects effect ON effect.id=b.effect_id JOIN effects target ON target.id=b.target_effect_id JOIN bonus_types bt ON bt.id=b.bonus_type_id WHERE effect.name=?1 ORDER BY target.name")
            .unwrap().query_map([&name], |row| Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?,row.get::<_,bool>(3)?,row.get::<_,String>(4)?)))
            .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        assert_eq!(
            rows,
            [
                ("Armor Class".into(), armor_class, 0, true, "Insight".into()),
                ("Saving Throws".into(), saves, 0, true, "Insight".into())
            ],
            "{name}"
        );
        let (verdict, reason, citations, description): (Option<String>, Option<String>, i64, String) = db.query_row(
            "SELECT stat_set_verdict,stat_set_reason,(SELECT COUNT(*) FROM citations WHERE owner_kind='effects' AND owner_id=effects.id AND url='https://ddowiki.com/page/Riposte'),description_template FROM effects WHERE name=?1",
            [&name], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
        ).unwrap();
        assert_eq!(verdict.as_deref(), Some("composes"), "{name}");
        assert_eq!(reason.as_deref(), Some(format!("Riposte {rank} is Parrying {rank} plus a when-missed proc; factored in #57 because odd ranks grant unequal AC and saves").as_str()));
        assert_eq!(citations, 1, "{name}");
        assert!(description.contains("When Missed by an attack:"), "{name}: {description}");
    }
}

#[test]
fn legacy_riposte_has_a_distinct_line_without_passive_bonuses() {
    let db = build_item_buff_fixture(
        include_str!("fixtures/effect-groups/LegacyRiposte.Buffs.xml"),
        &[("EpicSwashbucklerLegacy24.item", include_str!("fixtures/effect-groups/EpicSwashbucklerLegacy24.item"))],
        &[("ItemImages/Shields/Shield_18d.png", include_bytes!("fixtures/effect-groups/icons/Shield_18d.png"))],
    );
    let (title, description, value, bonus_count): (String, String, Option<i64>, i64) = db
        .query_row(
            "SELECT e.verbose_name_template,e.description_template,l.value,
                    (SELECT COUNT(*) FROM effect_bonuses WHERE effect_id=e.id)
             FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effects e ON e.id=l.effect_id
             WHERE i.name='Epic Swashbuckler (legacy) (level 24)' AND e.name='Riposte Riposte'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(title, "Riposte (legacy)");
    assert_eq!(description, "When its wearer is successfully attacked in melee, this shield occasionally allows him or her to make an immediate counterattack, dealing bludgeoning damage to the offending enemy.");
    assert_eq!((value, bonus_count), (None, 0));
    let passive_link_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effects e ON e.id=l.effect_id
             WHERE i.name='Epic Swashbuckler (legacy) (level 24)' AND e.name='Riposte'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(passive_link_count, 1);
}

fn assert_every_multi_stat_effect_has_one_link_or_an_exception(report: &BuildReport) {
    let exceptions = report
        .line_splits
        .iter()
        .chain(&report.held_back_effects)
        .map(|split| split.source_index)
        .collect::<std::collections::BTreeSet<_>>();
    let mut checked = 0;
    for (index, output) in report.derived_effect_lines.iter().enumerate() {
        if output.stat_names.len() > 1 {
            checked += 1;
            assert!(output.link_count == 1 || exceptions.contains(&index), "{index}: {output:?}");
        }
    }
    assert!(checked > 0);
}

#[test]
fn every_fixture_multi_stat_effect_has_one_link_or_a_recorded_exception() {
    let mut owner_kinds = std::collections::BTreeSet::new();
    for (_, report) in [built_fixture_db(), built_ambiguous_fixture_db()] {
        assert_every_multi_stat_effect_has_one_link_or_an_exception(&report);
        owner_kinds.extend(
            report
                .derived_effect_lines
                .into_iter()
                .filter(|output| output.stat_names.len() > 1)
                .map(|output| output.owner_kind),
        );
    }
    for kind in ["augment", "feat", "item_augment_slot_option", "set_bonus_tier"] {
        assert!(owner_kinds.contains(kind), "{owner_kinds:?}");
    }
}

#[test]
fn partially_mapped_multi_stat_effects_keep_their_lines_and_are_held() {
    let source = fixture::xml_source();
    let path = source.path().join("SetBonuses.xml");
    let contents = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        path,
        contents.replace("<Type>Doubleshot</Type>", "<Type>Doubleshot</Type><Type>Weapon_AttackAndDamage</Type>"),
    )
    .unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        fixture::groupings(),
        source.path(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    assert_every_multi_stat_effect_has_one_link_or_an_exception(&report);
    assert!(report.held_back_effects.iter().any(|effect| effect.reason.contains("without derived stats")
        && effect.effect_types.contains(&"Weapon_AttackAndDamage".to_string())));
}

fn create_untyped_shadow_source() -> tempfile::TempDir {
    let source = fixture::xml_source();
    std::fs::write(
        source.path().join("Augments/WoefulShadow.Augments.xml"),
        include_str!("fixtures/effect-groups/WoefulShadow.Augments.xml").replace("<Bonus>Profane</Bonus>", ""),
    )
    .unwrap();
    std::fs::copy(
        data_files_fixture_dir().parent().unwrap().join("effect-groups/icons/Lamordia.png"),
        source.path().join("AugmentImages/Lamordia.png"),
    )
    .unwrap();
    source
}

#[test]
fn member_removal_keeps_a_sibling_type_without_derived_stats() {
    let source = create_untyped_shadow_source();
    let path = source.path().join("Augments/WoefulShadow.Augments.xml");
    let contents = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        path,
        contents.replace(
            "<Type>Doublestrike</Type>\n      <Type>Doubleshot</Type>",
            "<Type>SpellPower</Type><Type>Weapon_CriticalRange</Type><Bonus>Profane</Bonus><Item>Fire</Item><Item>Cold</Item>",
        ),
    ).unwrap();
    let corrections = fixture::new_qualified_correction(
        "augment_bonus",
        "Woeful Shadow (Legendary)",
        "stat = 'Fire Spell Power'\nbonus_type = 'Profane'\nbonus_value = 3",
        "remove",
        "3",
        "\"null\"",
    );
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        fixture::groupings(),
        source.path(),
        &WikiOverrides::default(),
        &fixture::corrections_from(&[("removal.toml", &corrections)]).unwrap(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
    let names = db
        .prepare(
            "SELECT e.name FROM augment_effects link JOIN effects e ON e.id=link.effect_id
         JOIN augments a ON a.id=link.augment_id WHERE a.name='Woeful Shadow (Legendary)'",
        )
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert!(names.contains(&"Critical Threat Range (Fire, Cold)".to_string()), "{names:?}");
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM owner_bonuses b JOIN augments a ON a.id=b.owner_id
         JOIN effects s ON s.id=b.stat_id JOIN bonus_types bt ON bt.id=b.bonus_type_id
         WHERE b.owner_kind='augment' AND a.name='Woeful Shadow (Legendary)'
         AND s.name='Fire Spell Power' AND bt.name='Profane' AND b.amount=3"
        ),
        0
    );
}

#[test]
fn a_skill_group_with_one_member_keeps_its_declared_group_link() {
    let source = fixture::xml_source();
    let path = source.path().join("Augments/Named.Augments.xml");
    let contents = std::fs::read_to_string(&path).unwrap();
    std::fs::write(path, contents.replace("<Item>Charisma</Item>", "<Item>Constitution</Item>")).unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    build_database(
        fixture::groupings(),
        source.path(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    let (effect, stat, amount, bonus_type): (String, String, i64, String) = db
        .query_row(
            "SELECT e.name,s.name,b.amount,bt.name FROM owner_bonuses b
             JOIN augments a ON a.id=b.owner_id JOIN effects e ON e.id=COALESCE(b.via_effect_id,b.stat_id)
             JOIN effects s ON s.id=b.stat_id JOIN bonus_types bt ON bt.id=b.bonus_type_id
             WHERE b.owner_kind='augment' AND a.name='Storm''s Bulwark' AND s.name='Concentration'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        (effect.as_str(), stat.as_str(), amount, bonus_type.as_str()),
        ("Constitution Skills", "Concentration", 5, "Insight")
    );
}

#[test]
fn a_skill_group_reduced_to_one_member_does_not_restore_removed_stats() {
    let corrections = ["Diplomacy", "Haggle", "Intimidate", "Perform", "Use Magic Device"]
        .into_iter()
        .map(|stat| {
            fixture::new_qualified_correction(
                "augment_bonus",
                "Storm's Bulwark",
                &format!("stat = {stat:?}\nbonus_type = 'Insight'\nbonus_value = 5"),
                "remove",
                "5",
                "\"null\"",
            )
        })
        .collect::<fixture::CorrectionSet>();
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        fixture::groupings(),
        &data_files_fixture_dir(),
        &WikiOverrides::default(),
        &fixture::corrections_from(&[("skill-removals.toml", &corrections)]).unwrap(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (5, 0));
    let skills = db
        .prepare(
            "SELECT s.name FROM owner_bonuses b JOIN augments a ON a.id=b.owner_id
             JOIN effects s ON s.id=b.stat_id WHERE b.owner_kind='augment'
             AND a.name='Storm''s Bulwark' AND s.category='skill' ORDER BY s.name",
        )
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(skills, ["Bluff"]);
}

#[test]
fn held_member_lines_keep_their_targeted_family_names() {
    for (effect_type, targets, title, stats, families) in [
        (
            "SpellPower",
            "<Item>Fire</Item><Item>Cold</Item>",
            "Spell Power",
            ["Fire Spell Power", "Cold Spell Power"],
            ["Spell Power Fire", "Spell Power Cold"],
        ),
        (
            "Weapon_Alacrity",
            "<Item>All</Item>",
            "Attack Speed",
            ["Melee Attack Speed", "Ranged Attack Speed"],
            ["Melee Attack Speed", "Ranged Attack Speed"],
        ),
    ] {
        let source = create_untyped_shadow_source();
        let path = source.path().join("Augments/WoefulShadow.Augments.xml");
        let contents = std::fs::read_to_string(&path).unwrap();
        std::fs::write(
            path,
            contents.replace(
                "<Type>Doublestrike</Type>\n      <Type>Doubleshot</Type>",
                &format!("<Type>{effect_type}</Type>{targets}"),
            ),
        )
        .unwrap();
        let path = source.path().join("ItemBuffs.xml");
        let contents = std::fs::read_to_string(&path).unwrap();
        std::fs::write(
            path,
            contents.replace(
                "</Buffs>",
                &format!(
                    r#"<Buff>
                <Type>{effect_type}</Type><DisplayText>{title} %i1 %v1: %v1 %b1 bonus to %i1.</DisplayText>
                <Effect><Type>{effect_type}</Type><Bonus>Not Set</Bonus><AType>Simple</AType>
                <Amount size="1">0</Amount>{targets}</Effect>
                </Buff></Buffs>"#
                ),
            ),
        )
        .unwrap();
        let corrections = stats
            .iter()
            .zip(["Artifact", "Profane"])
            .map(|(stat, bonus_type)| {
                fixture::new_qualified_correction(
                    "augment_bonus",
                    "Woeful Shadow (Legendary)",
                    &format!("stat = '{stat}'\nbonus_type = 'null'"),
                    "bonus_type",
                    "\"null\"",
                    &format!("{bonus_type:?}"),
                )
            })
            .reduce(|combined, correction| combined + &correction)
            .unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        let report = build_database(
            fixture::groupings(),
            source.path(),
            &WikiOverrides::default(),
            &fixture::corrections_from(&[("targeted-types.toml", &corrections)]).unwrap(),
            &mut db,
            &fixture_dataset_version(),
        )
        .unwrap();
        assert_every_multi_stat_effect_has_one_link_or_an_exception(&report);
        let lines = db
            .prepare(
                "SELECT e.name, s.name, bt.name, b.amount FROM owner_bonuses b JOIN effects e ON e.id=COALESCE(b.via_effect_id,b.stat_id)
                 JOIN effects s ON s.id=b.stat_id JOIN bonus_types bt ON bt.id=b.bonus_type_id
                 JOIN augments a ON a.id=b.owner_id WHERE b.owner_kind='augment'
                 AND a.name='Woeful Shadow (Legendary)'
                 ORDER BY e.name",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let mut expected = families
            .into_iter()
            .zip(stats)
            .zip(["Artifact", "Profane"])
            .map(|((family, stat), bonus_type)| (family.to_string(), stat.to_string(), bonus_type.to_string(), 3))
            .collect::<Vec<_>>();
        expected.extend([
            ("Melee Alacrity".to_string(), "Melee Attack Speed".to_string(), "Enhancement".to_string(), 15),
            ("Ranged Alacrity".to_string(), "Ranged Attack Speed".to_string(), "Enhancement".to_string(), 20),
        ]);
        expected.sort();
        assert_eq!(lines, expected, "{effect_type}: {targets}");
    }
}

#[test]
fn mixed_types_from_owner_corrections_keep_stat_lines_and_are_held() {
    let source = create_untyped_shadow_source();
    let corrections = fixture::new_qualified_correction(
        "augment_bonus",
        "Woeful Shadow (Legendary)",
        "stat = 'Doublestrike'\nbonus_type = 'null'",
        "bonus_type",
        "\"null\"",
        "\"Profane\"",
    ) + &fixture::new_qualified_correction(
        "augment_bonus",
        "Woeful Shadow (Legendary)",
        "stat = 'Doubleshot'\nbonus_type = 'null'",
        "bonus_type",
        "\"null\"",
        "\"Artifact\"",
    );
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        fixture::groupings(),
        source.path(),
        &WikiOverrides::default(),
        &fixture::corrections_from(&[("mixed-types.toml", &corrections)]).unwrap(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    assert_every_multi_stat_effect_has_one_link_or_an_exception(&report);
    assert!(report
        .held_back_effects
        .iter()
        .any(|effect| effect.owner_name == "Woeful Shadow (Legendary)" && effect.reason.contains("different types")));
    let bonuses = db.prepare(
        "SELECT e.name,bt.name,b.amount FROM owner_bonuses b JOIN augments a ON a.id=b.owner_id
         JOIN effects e ON e.id=b.stat_id JOIN bonus_types bt ON bt.id=b.bonus_type_id
         WHERE b.owner_kind='augment' AND a.name='Woeful Shadow (Legendary)' AND e.name IN ('Doublestrike','Doubleshot') ORDER BY e.name"
    ).unwrap().query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?)))
        .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(bonuses, [("Doubleshot".into(), "Artifact".into(), 3), ("Doublestrike".into(), "Profane".into(), 3)]);
}

#[test]
fn mixed_types_inside_an_ability_group_keep_every_members_type() {
    let source = create_untyped_shadow_source();
    let path = source.path().join("Augments/WoefulShadow.Augments.xml");
    let contents = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        path,
        contents.replace(
            "<Type>Doublestrike</Type>\n      <Type>Doubleshot</Type>",
            "<Type>AbilityBonus</Type><Item>All</Item>",
        ),
    )
    .unwrap();
    let stats = ["Strength", "Dexterity", "Constitution", "Intelligence", "Wisdom", "Charisma"];
    let corrections = stats
        .iter()
        .enumerate()
        .map(|(index, stat)| {
            fixture::new_qualified_correction(
                "augment_bonus",
                "Woeful Shadow (Legendary)",
                &format!("stat = {stat:?}\nbonus_type = 'null'"),
                "bonus_type",
                "\"null\"",
                if index == 0 { "\"Artifact\"" } else { "\"Profane\"" },
            )
        })
        .collect::<fixture::CorrectionSet>();
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        fixture::groupings(),
        source.path(),
        &WikiOverrides::default(),
        &fixture::corrections_from(&[("mixed-abilities.toml", &corrections)]).unwrap(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    assert_every_multi_stat_effect_has_one_link_or_an_exception(&report);
    for (index, stat) in stats.iter().enumerate() {
        let bonus: (String, i64) = db
            .query_row(
                "SELECT bt.name,b.amount FROM owner_bonuses b JOIN augments a ON a.id=b.owner_id
             JOIN effects e ON e.id=b.stat_id JOIN bonus_types bt ON bt.id=b.bonus_type_id
             WHERE b.owner_kind='augment' AND a.name='Woeful Shadow (Legendary)' AND e.name=?1",
                [stat],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(bonus, (if index == 0 { "Artifact".into() } else { "Profane".into() }, 3));
    }
}

#[test]
fn ambiguous_upstream_stat_sets_keep_their_stat_links_and_are_reported() {
    let (db, report) = built_ambiguous_fixture_db();
    for (owner, expected_stats) in [
        ("Ruby of Negative Spell Power", vec!["Negative Spell Power", "Poison Spell Power"]),
        ("Solar Gem of Tactical DCs (Heroic)", vec!["Combat Mastery"]),
    ] {
        let mut statement = db
            .prepare(
                "SELECT DISTINCT e.name FROM augment_effects link JOIN augments a ON a.id = link.augment_id
             JOIN effects e ON e.id = link.effect_id WHERE a.name = ?1 ORDER BY e.name",
            )
            .unwrap();
        let linked = statement
            .query_map([owner], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(linked, expected_stats, "{owner}");
        assert_eq!(
            report.held_back_effects.iter().any(|effect| effect.owner_name == owner),
            owner == "Ruby of Negative Spell Power",
            "{owner}"
        );
    }
}

#[test]
fn declared_ladder_steps_stay_addressable_without_blocking_their_group() {
    for verdict in [None, Some("distinct"), Some("composes")] {
        let db = fixture::grouping_case_database(|root| {
            let path = root.join("effects/void-lore-vi.toml");
            let original = std::fs::read_to_string(&path).unwrap();
            let contents = match verdict {
                Some(verdict) => {
                    original.replace("stat_set_verdict = \"distinct\"", &format!("stat_set_verdict = \"{verdict}\""))
                }
                None => original.lines().filter(|line| !line.starts_with("stat_set_")).collect::<Vec<_>>().join("\n"),
            };
            std::fs::write(path, contents).unwrap();
        });
        let groupings = ddo_etl::groupings::Groupings::from_database(&db).unwrap();
        let stats = ["Negative Spell Lore".to_string(), "Poison Spell Lore".to_string()].into_iter().collect();
        let candidates = groupings.by_stat_set[&stats].iter().map(|effect| effect.name.as_str()).collect::<Vec<_>>();
        let expected = if verdict.is_some() { vec!["Void Lore"] } else { vec!["Void Lore", "Void Lore VI"] };
        assert_eq!(candidates, expected, "{verdict:?}");
        assert!(groupings.by_id.values().any(|effect| effect.name == "Void Lore VI"));
        assert_eq!(
            groupings
                .pairs(ddo_etl::groupings::GroupingPairDomain::LinkValued)
                .iter()
                .any(|pair| pair.first.name == "Void Lore VI" || pair.second.name == "Void Lore VI"),
            verdict.is_none(),
            "{verdict:?}"
        );
        assert!(groupings
            .pairs(ddo_etl::groupings::GroupingPairDomain::AllBonuses)
            .iter()
            .any(|pair| pair.first.name == "Void Lore" && pair.second.name == "Void Lore VI"));
    }
}

#[test]
fn declared_speed_rank_preserves_its_label_and_two_amounts() {
    let (db, _) = built_fixture_db();
    let line: (String, i64, i64, Option<String>) = db
        .query_row(
            "SELECT e.name,l.value,l.value2,e.stat_set_verdict FROM item_effects l
             JOIN effects e ON e.id=l.effect_id JOIN items i ON i.id=l.item_id
             WHERE i.name='Epic Ethereal Bracers' AND e.name='Speed XIV'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(line, ("Speed XIV".into(), 30, 14, Some("distinct".into())));
    let bonuses: Vec<(String, String, i64)> = db
        .prepare(
            "SELECT s.name,t.name,b.amount FROM owner_bonuses b
             JOIN effects s ON s.id=b.stat_id JOIN effects e ON e.id=b.via_effect_id
             JOIN bonus_types t ON t.id=b.bonus_type_id JOIN items i ON i.id=b.owner_id
             WHERE b.owner_kind='item' AND i.name='Epic Ethereal Bracers' AND e.name='Speed XIV'
             ORDER BY s.name",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        bonuses,
        [
            ("Melee Attack Speed".into(), "Enhancement".into(), 14),
            ("Movement Speed".into(), "Enhancement".into(), 30),
            ("Ranged Attack Speed".into(), "Enhancement".into(), 14),
        ]
    );
    let groupings = ddo_etl::groupings::Groupings::from_database(&db).unwrap();
    assert!(groupings.by_id.values().any(|effect| effect.name == "Speed XIV"));
    assert!(!groupings
        .pairs(ddo_etl::groupings::GroupingPairDomain::LinkValued)
        .iter()
        .any(|pair| pair.first.name == "Speed XIV" || pair.second.name == "Speed XIV"));
}

#[test]
fn declared_parrying_ranks_release_aura_of_good_as_one_line() {
    let mut canonical = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data"), &mut canonical)
        .unwrap();
    let mut upstream = Connection::open_in_memory().unwrap();
    let report = build_database(
        fixture::grouping_cases(),
        &data_files_fixture_dir(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut upstream,
        &fixture_dataset_version(),
    )
    .unwrap();
    for db in [&canonical, &upstream] {
        let links = db
            .prepare(
                "SELECT e.name,bt.name,l.value FROM feat_effects l JOIN feats f ON f.id=l.feat_id
             JOIN effects e ON e.id=l.effect_id JOIN bonus_types bt ON bt.id=l.bonus_type_id
             WHERE f.name='Aura of Good' ORDER BY l.sort_order",
            )
            .unwrap()
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(links, [("Parrying".into(), "Feat".into(), 1)]);
        let bonuses = db
            .prepare(
                "SELECT s.name,bt.name,b.amount FROM owner_bonuses b JOIN feats f ON f.id=b.owner_id
             JOIN effects s ON s.id=b.stat_id JOIN bonus_types bt ON bt.id=b.bonus_type_id
             WHERE b.owner_kind='feat' AND f.name='Aura of Good' ORDER BY s.name",
            )
            .unwrap()
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(bonuses, [("Armor Class".into(), "Feat".into(), 1), ("Saving Throws".into(), "Feat".into(), 1)]);
    }
    assert!(!report.held_back_effects.iter().any(|effect| effect.owner_name == "Aura of Good"));
}

#[test]
fn two_slot_deception_cannot_replace_a_single_value_sneak_attack_line() {
    let (db, report) = built_ambiguous_fixture_db();
    let names = db.prepare(
        "SELECT e.name FROM feat_effects link JOIN feats f ON f.id = link.feat_id JOIN effects e ON e.id = link.effect_id
         WHERE f.name = 'Past Life: Ranger - Dark Hunter' ORDER BY e.name",
    ).unwrap().query_map([], |row| row.get::<_, String>(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(names, ["Sneak Attack Damage", "Sneak Attack Hit"]);
    assert!(report
        .held_back_effects
        .iter()
        .any(|effect| effect.owner_name == "Past Life: Ranger - Dark Hunter" && effect.reason.contains("Deception")));
}

#[test]
fn mixed_fixed_types_keep_the_amplification_stats_separate() {
    let (db, report) = built_ambiguous_fixture_db();
    let names = db
        .prepare(
            "SELECT e.name FROM set_bonus_tier_effects link JOIN effects e ON e.id = link.effect_id
         JOIN set_bonus_tiers t ON t.id = link.tier_id JOIN set_bonuses s ON s.id = t.set_id
         WHERE s.name = 'Eminence of Winter' AND t.equipped_count = 3 ORDER BY e.name",
        )
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(names, ["Healing Amplification", "Negative Healing Amplification", "Repair Amplification"]);
    assert!(report
        .held_back_effects
        .iter()
        .any(|effect| effect.owner_name == "Eminence of Winter"
            && effect.reason.contains("Lifeblood of the Undead Prince")));
}

#[test]
fn kundaraks_multi_type_effect_keeps_one_doublestrike_and_doubleshot_link() {
    let (db, _) = built_fixture_db();
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM set_bonus_tier_effects link JOIN effects e ON e.id = link.effect_id
         JOIN set_bonus_tiers t ON t.id = link.tier_id JOIN set_bonuses s ON s.id = t.set_id
         WHERE s.name = 'Kundarak Delving Equipment' AND e.name = 'Doublestrike and Doubleshot' AND link.value = 5"
        ),
        1
    );
}

#[test]
fn ambiguous_set_lines_keep_the_declared_group_and_cited_tactics_removals_apply() {
    let source = data_files_fixture_dir();
    let scratch = fixture::xml_source();
    std::fs::copy(source.parent().unwrap().join("effect-groups/SetBonuses.xml"), scratch.path().join("SetBonuses.xml"))
        .unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    let mut corrections =
        Corrections::from_data(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap();
    corrections.entries.retain(|entry| entry.name == "Eminence of Summer");
    let report = build_database(
        fixture::grouping_cases(),
        scratch.path(),
        &WikiOverrides::default(),
        &corrections,
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM set_bonus_tier_effects link JOIN effects e ON e.id = link.effect_id
         JOIN set_bonus_tiers t ON t.id = link.tier_id JOIN set_bonuses s ON s.id = t.set_id
         WHERE s.name = 'The Desert''s Starless Nights' AND e.name = 'Nullification'"
        ),
        1
    );
    assert!(report
        .held_back_effects
        .iter()
        .any(|effect| effect.owner_name == "The Desert's Starless Nights" && effect.reason.contains("Nullification")));
    let names = db
        .prepare(
            "SELECT e.name FROM set_bonus_tier_effects link JOIN effects e ON e.id = link.effect_id
         JOIN set_bonus_tiers t ON t.id = link.tier_id JOIN set_bonuses s ON s.id = t.set_id
         WHERE s.name = 'Eminence of Summer' AND t.equipped_count = 4 ORDER BY e.name",
        )
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(names, ["Combat Mastery"]);
}

fn build_wrapper_fixture(
    groupings: &ddo_etl::groupings::Groupings,
    item_xml: &str,
    buff_definitions_xml: &str,
) -> Connection {
    let source = fixture::xml_source();
    let path = source.path().join("ItemBuffs.xml");
    let definitions = std::fs::read_to_string(&path).unwrap();
    std::fs::write(path, definitions.replace("</Buffs>", &format!("{buff_definitions_xml}</Buffs>"))).unwrap();
    std::fs::write(source.path().join("Items/Wrapper.item"), item_xml).unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    build_database(
        groupings,
        source.path(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    db
}

fn assert_wrapper_group(db: &Connection, item_name: &str, group_name: &str, members: &[&str], grants: &[(&str, i64)]) {
    let (group_id, is_group): (i64, bool) = db
        .query_row("SELECT id,is_group FROM effects WHERE name=?1", [group_name], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap();
    assert!(is_group, "{group_name} must be the group itself");
    let link_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM item_effects l JOIN items i ON i.id=l.item_id WHERE i.name=?1 AND l.effect_id=?2",
            params![item_name, group_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(link_count as usize, grants.len());
    let actual_members = db
        .prepare("SELECT s.name FROM effect_bonuses b JOIN effects s ON s.id=b.target_effect_id WHERE b.effect_id=?1 AND s.is_stat=1 AND b.amount_from=1 AND b.bonus_type_id IS NULL ORDER BY s.name")
        .unwrap()
        .query_map([group_id], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let mut expected_members = members.iter().map(|member| (*member).to_string()).collect::<Vec<_>>();
    expected_members.sort();
    assert_eq!(actual_members, expected_members);
    let actual = db
        .prepare("SELECT s.name,bt.name,b.amount FROM owner_bonuses b JOIN effects s ON s.id=b.stat_id JOIN bonus_types bt ON bt.id=b.bonus_type_id JOIN items i ON i.id=b.owner_id WHERE b.owner_kind='item' AND i.name=?1 AND b.via_effect_id=?2 ORDER BY s.name,bt.name,b.amount")
        .unwrap()
        .query_map(params![item_name, group_id], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let mut expected = members
        .iter()
        .flat_map(|member| {
            grants.iter().map(move |(bonus_type, amount)| ((*member).to_string(), (*bonus_type).to_string(), *amount))
        })
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(actual, expected);
}

fn assert_group_amount_on_item_link(db: &Connection, item_name: &str, group_name: &str, expected_amount: i64) {
    let amounts: (Option<i64>, Option<i64>, Option<i64>) = db.query_row(
        "SELECT l.value,e.default_value,e.default_value2 FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effects e ON e.id=l.effect_id WHERE i.name=?1 AND e.name=?2",
        params![item_name, group_name], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).unwrap();
    assert_eq!(amounts, (Some(expected_amount), None, None));
}

fn item_effect_alias(db: &Connection, item_name: &str, effect_name: &str) -> Option<String> {
    db.query_row("SELECT l.alias FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effects e ON e.id=l.effect_id WHERE i.name=?1 AND e.name=?2", params![item_name,effect_name], |row| row.get(0)).unwrap()
}

#[test]
fn radiance_buffs_link_five_spell_powers_at_each_owners_type() {
    let db = build_wrapper_fixture(
        fixture::groupings(),
        include_str!("fixtures/effect-groups/Radiance.item"),
        include_str!("fixtures/effect-groups/Radiance.Buffs.xml"),
    );
    assert_wrapper_group(
        &db,
        "Epic Sceptre of Radiance",
        "Radiance",
        &["Light Spell Power", "Chaos Spell Power", "Evil Spell Power", "Good Spell Power", "Law Spell Power"],
        &[("Equipment", 111), ("Insight", 55)],
    );
    assert_eq!(count(&db, "SELECT COUNT(*) FROM effects WHERE name='Light and Alignment Spell Power'"), 0);
}

#[test]
fn nullification_buffs_link_two_spell_powers_at_each_owners_type() {
    let db = build_wrapper_fixture(
        fixture::groupings(),
        include_str!("fixtures/effect-groups/Darksphere.item"),
        include_str!("fixtures/effect-groups/Darksphere.Buffs.xml"),
    );
    assert_wrapper_group(
        &db,
        "Darksphere",
        "Nullification",
        &["Negative Spell Power", "Poison Spell Power"],
        &[("Equipment", 115), ("Insight", 57)],
    );
    assert_eq!(count(&db, "SELECT COUNT(*) FROM effects WHERE name='Negative and Poison Spell Power'"), 0);
}

#[test]
fn void_lore_buffs_link_both_spell_lores_at_the_owners_type() {
    let items = include_str!("fixtures/effect-groups/Blightstaff.item").replace("</Items>", "")
        + &include_str!("fixtures/effect-groups/Darksphere.item").replace("<Items>", "");
    let definitions = include_str!("fixtures/effect-groups/Blightstaff.Buffs.xml").to_string()
        + include_str!("fixtures/effect-groups/Darksphere.Buffs.xml");
    let db = build_wrapper_fixture(fixture::groupings(), &items, &definitions);
    assert_wrapper_group(
        &db,
        "Darksphere",
        "Void Lore",
        &["Negative Spell Lore", "Poison Spell Lore"],
        &[("Equipment", 17)],
    );
    assert_eq!(count(&db, "SELECT COUNT(*) FROM effects WHERE name='Negative and Poison Spell Lore'"), 0);
}

#[test]
fn declared_ladder_buffs_keep_flat_bonuses_and_their_declaration() {
    let db = build_wrapper_fixture(
        fixture::grouping_cases(),
        include_str!("fixtures/effect-groups/VoidLoreRank.item"),
        include_str!("fixtures/effect-groups/VoidLoreRank.Buffs.xml"),
    );
    let targets = db.prepare("SELECT target.name FROM effect_bonuses b JOIN effects e ON e.id=b.effect_id JOIN effects target ON target.id=b.target_effect_id WHERE e.name='Void Lore VI' ORDER BY target.name")
        .unwrap().query_map([], |row| row.get::<_,String>(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(targets, ["Negative Spell Lore", "Poison Spell Lore"]);
    let declaration: (String, String) = db
        .query_row("SELECT stat_set_verdict,stat_set_reason FROM effects WHERE name='Void Lore VI'", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(
        declaration,
        (
            "distinct".to_string(),
            "steps of one ladder differ in amount; one-enchantment ladders are ddo-data#57".to_string()
        )
    );
    let bonuses = db.prepare("SELECT s.name,t.name,b.amount FROM owner_bonuses b JOIN effects s ON s.id=b.stat_id JOIN bonus_types t ON t.id=b.bonus_type_id JOIN items i ON i.id=b.owner_id WHERE b.owner_kind='item' AND i.name='Death''s Touch' ORDER BY s.name")
        .unwrap().query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(
        bonuses,
        [
            ("Negative Spell Lore".to_string(), "Equipment".to_string(), 14),
            ("Poison Spell Lore".to_string(), "Equipment".to_string(), 14)
        ]
    );
}

#[test]
fn exceptional_charisma_buffs_link_the_six_skills() {
    for (amount, expected) in [(None, 3), (Some(2), 2)] {
        let item_xml = include_str!("fixtures/effect-groups/CharismaSkills.item").replace(
            "<Value1>2</Value1>",
            &amount.map(|amount| format!("<Value1>{amount}</Value1>")).unwrap_or_default(),
        );
        let db = build_wrapper_fixture(
            fixture::grouping_cases(),
            &item_xml,
            include_str!("fixtures/effect-groups/CharismaSkills.Buffs.xml"),
        );
        assert_wrapper_group(
            &db,
            "Pink and Green Ioun Stone (unsuppressed)",
            "Charisma Skills",
            &["Bluff", "Diplomacy", "Haggle", "Intimidate", "Perform", "Use Magic Device"],
            &[("Exceptional", expected)],
        );
        assert_eq!(count(&db, "SELECT COUNT(*) FROM effects WHERE name='Charisma Skills - Exceptional Bonus'"), 0);
        assert_group_amount_on_item_link(&db, "Pink and Green Ioun Stone (unsuppressed)", "Charisma Skills", expected);
        let alias = item_effect_alias(&db, "Pink and Green Ioun Stone (unsuppressed)", "Charisma Skills");
        assert_eq!(alias.as_deref(), Some("Charisma Skills - Exceptional Bonus"));
    }
}

#[test]
fn explicit_zero_overrides_a_grouped_buffs_definition_default() {
    let item_xml =
        include_str!("fixtures/effect-groups/CharismaSkills.item").replace("<Value1>2</Value1>", "<Value1>0</Value1>");
    let db = build_wrapper_fixture(
        fixture::groupings(),
        &item_xml,
        include_str!("fixtures/effect-groups/CharismaSkills.Buffs.xml"),
    );
    let amounts: (Option<i64>, Option<i64>) = db.query_row("SELECT l.value,e.default_value FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effects e ON e.id=l.effect_id WHERE i.name='Pink and Green Ioun Stone (unsuppressed)' AND e.name='Charisma Skills'", [], |row| Ok((row.get(0)?,row.get(1)?))).unwrap();
    assert_eq!(amounts, (Some(0), None));
    assert_eq!(count(&db,"SELECT COUNT(*) FROM owner_bonuses b JOIN items i ON i.id=b.owner_id WHERE b.owner_kind='item' AND i.name='Pink and Green Ioun Stone (unsuppressed)'"), 0);
}

#[test]
fn routed_buff_chooses_its_source_family_alias() {
    let canonical = fixture::database(|root| {
        let path = root.join("effects/stat-groups/charisma-skills.toml");
        let original = std::fs::read_to_string(&path).unwrap();
        std::fs::write(path, format!("{original}\n[[alias]]\nname = 'Charisma Skills - Exceptional Bonus'\nverbose_name_template = 'Charisma Skills - %b1 Bonus +{{1}}'\n")).unwrap();
    });
    let groupings = ddo_etl::groupings::Groupings::from_database(&canonical).unwrap();
    let db = build_wrapper_fixture(
        &groupings,
        include_str!("fixtures/effect-groups/CharismaSkills.item"),
        include_str!("fixtures/effect-groups/CharismaSkills.Buffs.xml"),
    );
    let choice: (String, Option<String>, i64) = db.query_row(
        "SELECT e.name, a.name, l.value FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effects e ON e.id=l.effect_id LEFT JOIN effect_aliases a ON a.effect_id=l.effect_id AND a.name=l.alias WHERE i.name='Pink and Green Ioun Stone (unsuppressed)' AND e.name='Charisma Skills'",
        [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).unwrap();
    assert_eq!(choice, ("Charisma Skills".into(), Some("Charisma Skills - Exceptional Bonus".into()), 2));
    let stored: (String, String) = db.query_row("SELECT l.alias, typeof(l.alias) FROM item_effects l JOIN items i ON i.id=l.item_id WHERE i.name='Pink and Green Ioun Stone (unsuppressed)' AND l.alias IS NOT NULL", [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
    assert_eq!(stored, ("Charisma Skills - Exceptional Bonus".into(), "text".into()));
    assert_eq!(count(&db, "SELECT COUNT(*) FROM pragma_table_info('item_effects') WHERE name='alias_id'"), 0);
}

#[test]
fn search_only_aliases_do_not_choose_a_buff_line() {
    let canonical = fixture::database(|root| {
        let path = root.join("effects/stat-groups/charisma-skills.toml");
        let original = std::fs::read_to_string(&path).unwrap();
        std::fs::write(path, format!("{original}\n[[alias]]\nname = 'Charisma Skills - Exceptional Bonus'\n")).unwrap();
    });
    let groupings = ddo_etl::groupings::Groupings::from_database(&canonical).unwrap();
    let db = build_wrapper_fixture(
        &groupings,
        include_str!("fixtures/effect-groups/CharismaSkills.item"),
        include_str!("fixtures/effect-groups/CharismaSkills.Buffs.xml"),
    );
    let choice: Option<String> = db.query_row("SELECT l.alias FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effects e ON e.id=l.effect_id WHERE i.name='Pink and Green Ioun Stone (unsuppressed)' AND e.name='Charisma Skills'", [], |row| row.get(0)).unwrap();
    assert_eq!(choice, None);
}

#[test]
fn exceptional_dexterity_buff_links_five_skills_with_its_alias() {
    let db = build_wrapper_fixture(
        fixture::grouping_cases(),
        include_str!("fixtures/effect-groups/DexteritySkills.item"),
        include_str!("fixtures/effect-groups/DexteritySkills.Buffs.xml"),
    );
    assert_wrapper_group(
        &db,
        "Masterwork Gauntlet",
        "Dexterity Skills",
        &["Balance", "Hide", "Move Silently", "Open Lock", "Tumble"],
        &[("Exceptional", 6)],
    );
    assert_eq!(
        item_effect_alias(&db, "Masterwork Gauntlet", "Dexterity Skills").as_deref(),
        Some("Dexterity Skills - Exceptional Bonus")
    );
    assert_eq!(count(&db, "SELECT COUNT(*) FROM effects WHERE name='Dexterity Skills - Exceptional Bonus'"), 0);
}

#[test]
fn exceptional_intelligence_buff_links_four_skills_with_its_alias() {
    let db = build_wrapper_fixture(
        fixture::grouping_cases(),
        include_str!("fixtures/effect-groups/IntelligenceSkills.item"),
        include_str!("fixtures/effect-groups/IntelligenceSkills.Buffs.xml"),
    );
    assert_wrapper_group(
        &db,
        "Legendary Tiny Brain in a Tiny Jar",
        "Intelligence Skills",
        &["Disable Device", "Repair", "Search", "Spellcraft"],
        &[("Exceptional", 6)],
    );
    assert_eq!(
        item_effect_alias(&db, "Legendary Tiny Brain in a Tiny Jar", "Intelligence Skills").as_deref(),
        Some("Intelligence Skills - Exceptional Bonus")
    );
    assert_eq!(count(&db, "SELECT COUNT(*) FROM effects WHERE name='Intelligence Skills - Exceptional Bonus'"), 0);
}

#[test]
fn exceptional_wisdom_buff_keeps_its_alias_and_definition_default() {
    for (amount, expected) in [(None, 3), (Some(2), 2)] {
        let item_xml = include_str!("fixtures/effect-groups/WisdomSkills.item").replace(
            "<Value1>2</Value1>",
            &amount.map(|amount| format!("<Value1>{amount}</Value1>")).unwrap_or_default(),
        );
        let db = build_wrapper_fixture(
            fixture::grouping_cases(),
            &item_xml,
            include_str!("fixtures/effect-groups/WisdomSkills.Buffs.xml"),
        );
        assert_wrapper_group(
            &db,
            "Incandescent Blue Ioun Stone (unsuppressed)",
            "Wisdom Skills",
            &["Heal", "Listen", "Spot"],
            &[("Exceptional", expected)],
        );
        assert_eq!(count(&db, "SELECT COUNT(*) FROM effects WHERE name='Wisdom Skills - Exceptional Bonus'"), 0);
        assert_group_amount_on_item_link(&db, "Incandescent Blue Ioun Stone (unsuppressed)", "Wisdom Skills", expected);
        let alias = item_effect_alias(&db, "Incandescent Blue Ioun Stone (unsuppressed)", "Wisdom Skills");
        assert_eq!(alias.as_deref(), Some("Wisdom Skills - Exceptional Bonus"));
    }
}

#[test]
fn rune_arm_focus_buff_links_its_stat_with_its_alias() {
    let db = build_wrapper_fixture(
        fixture::grouping_cases(),
        include_str!("fixtures/effect-groups/RuneArmFocus.item"),
        include_str!("fixtures/effect-groups/RuneArmFocus.Buffs.xml"),
    );
    let links = db.prepare("SELECT e.name,e.is_stat,l.alias,l.value,bt.name FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effects e ON e.id=l.effect_id JOIN bonus_types bt ON bt.id=l.bonus_type_id WHERE i.name='Prototype IV: Discharge System' ORDER BY l.sort_order").unwrap()
        .query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,bool>(1)?,row.get::<_,Option<String>>(2)?,row.get::<_,i64>(3)?,row.get::<_,String>(4)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(links, [("Rune Arm Spell Focus".into(), true, Some("Rune Arm Focus".into()), 4, "Equipment".into())]);
    let bonuses = db.prepare("SELECT s.name,bt.name,b.amount FROM owner_bonuses b JOIN items i ON i.id=b.owner_id JOIN effects s ON s.id=b.stat_id JOIN bonus_types bt ON bt.id=b.bonus_type_id WHERE b.owner_kind='item' AND i.name='Prototype IV: Discharge System'").unwrap()
        .query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(bonuses, [("Rune Arm Spell Focus".into(), "Equipment".into(), 4)]);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM effects WHERE name='Rune Arm Focus'"), 0);
}

#[test]
fn strength_skill_wrappers_share_a_group_without_sharing_defaults() {
    let item_xml = format!(
        "{}{}",
        include_str!("fixtures/effect-groups/StrengthSkills.item").replace("</Items>", ""),
        include_str!("fixtures/effect-groups/MightySkills.item").replace("<Items>", "")
    );
    let definitions = format!(
        "{}{}",
        include_str!("fixtures/effect-groups/StrengthSkills.Buffs.xml"),
        include_str!("fixtures/effect-groups/MightySkills.Buffs.xml")
    );
    for (item_xml, exceptional, insight, quality) in [
        (item_xml.clone(), 2, 10, 5),
        (
            item_xml
                .replace("<Value1>2</Value1>", "")
                .replace("<Value1>10</Value1>", "")
                .replace("<Value1>5</Value1>", ""),
            3,
            5,
            5,
        ),
    ] {
        let db = build_wrapper_fixture(fixture::grouping_cases(), &item_xml, &definitions);
        assert_wrapper_group(
            &db,
            "Pale Blue Ioun Stone (unsuppressed)",
            "Strength Skills",
            &["Jump", "Swim"],
            &[("Exceptional", exceptional)],
        );
        assert_wrapper_group(
            &db,
            "The Repulsor Boots",
            "Strength Skills",
            &["Jump", "Swim"],
            &[("Insight", insight), ("Quality", quality)],
        );
        let aliases = db.prepare("SELECT l.alias FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effects e ON e.id=l.effect_id WHERE e.name='Strength Skills' AND i.name IN ('Pale Blue Ioun Stone (unsuppressed)','The Repulsor Boots') ORDER BY i.name,l.sort_order").unwrap().query_map([], |row| row.get::<_,String>(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        assert_eq!(aliases, ["Strength Skills - Exceptional Bonus", "Mighty Skills Bonus", "Mighty Skills Bonus"]);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM effects WHERE name IN ('Strength Skills - Exceptional Bonus','Mighty Skills Bonus')"), 0);
        let defaults: (Option<i64>, Option<i64>) = db
            .query_row("SELECT default_value,default_value2 FROM effects WHERE name='Strength Skills'", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(defaults, (None, None));
    }
}

#[test]
fn grouped_definition_defaults_survive_export_and_rebuild_on_owner_links() {
    let source = fixture::xml_source();
    let path = source.path().join("ItemBuffs.xml");
    let definitions = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        path,
        definitions.replace(
            "</Buffs>",
            &format!("{}</Buffs>", include_str!("fixtures/effect-groups/CharismaSkills.Buffs.xml")),
        ),
    )
    .unwrap();
    let item_xml = include_str!("fixtures/effect-groups/CharismaSkills.item");
    let missing_amount = item_xml
        .replace("<Value1>2</Value1>", "")
        .replace("Pink and Green Ioun Stone (unsuppressed)", "Default Charisma Skills");
    std::fs::write(source.path().join("Items/ExplicitCharismaSkills.item"), item_xml).unwrap();
    std::fs::write(source.path().join("Items/DefaultCharismaSkills.item"), missing_amount).unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    build_database(
        fixture::groupings(),
        source.path(),
        &WikiOverrides::default(),
        &fixture::fixture_icon_corrections(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    let exported = tempfile::tempdir().unwrap();
    ddo_etl::tree::export_database(&db, exported.path()).unwrap();
    let images = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data");
    for entry in walkdir::WalkDir::new(&images).into_iter().map(Result::unwrap).filter(|entry| {
        entry.file_type().is_file() && entry.path().extension().is_some_and(|extension| extension == "png")
    }) {
        let destination = exported.path().join(entry.path().strip_prefix(&images).unwrap());
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::copy(entry.path(), destination).unwrap();
    }
    let before = ddo_etl::tree::data_file_bytes(exported.path()).unwrap();
    let mut rebuilt = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(exported.path(), &mut rebuilt).unwrap();
    for (name, amount) in [("Default Charisma Skills", 3), ("Pink and Green Ioun Stone (unsuppressed)", 2)] {
        let linked_amount: Option<i64> = rebuilt.query_row("SELECT l.value FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effects e ON e.id=l.effect_id WHERE i.name=?1 AND e.name='Charisma Skills'", [name], |row| row.get(0)).unwrap();
        assert_eq!(linked_amount, Some(amount), "{name}");
    }
    ddo_etl::tree::export_database(&rebuilt, exported.path()).unwrap();
    assert_eq!(before, ddo_etl::tree::data_file_bytes(exported.path()).unwrap());
}

#[test]
fn potency_grants_each_named_spell_power_without_granting_universal_spell_power() {
    let (db, _) = built_fixture_db();
    assert_eq!(count(&db, "SELECT is_group FROM effects WHERE name = 'Potency'"), 1);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM effects WHERE name = 'Spell Powers'"), 0);
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM effect_bonuses b JOIN effects e ON e.id = b.target_effect_id WHERE b.effect_id = (SELECT id FROM effects WHERE name = 'Potency') AND e.is_stat = 1 AND b.amount_from = 1 AND b.bonus_type_id IS NULL"),
        15
    );
    let item = item_id(&db, "Band of Diani ir'Wynarn");
    let spell_powers: Vec<String> = db
        .prepare(
            "SELECT s.name FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
             WHERE ob.owner_kind = 'item' AND ob.owner_id = ?1
               AND ob.via_effect_id = (SELECT id FROM effects WHERE name = 'Potency') ORDER BY s.name",
        )
        .unwrap()
        .query_map([item], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(spell_powers.len(), 15);
    assert!(spell_powers.contains(&"Fire Spell Power".to_string()));
    assert!(!spell_powers.contains(&"Universal Spell Power".to_string()));
}

#[test]
fn an_augment_potency_effect_grants_each_spell_power_through_the_shared_family() {
    let (db, _) = built_fixture_db();
    let augment_id: i64 =
        db.query_row("SELECT id FROM augments WHERE name = 'Potency Probe'", [], |row| row.get(0)).unwrap();
    let linked_family: String = db
        .query_row(
            "SELECT e.name FROM augment_effects ae JOIN effects e ON e.id = ae.effect_id WHERE ae.augment_id = ?1",
            [augment_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(linked_family, "Potency");
    assert_eq!(count(&db, "SELECT is_group FROM effects WHERE name = 'Potency'"), 1);
    let spell_powers: Vec<(String, i64)> = db
        .prepare(
            "SELECT s.name, ob.amount FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
             WHERE ob.owner_kind = 'augment' AND ob.owner_id = ?1 ORDER BY s.name",
        )
        .unwrap()
        .query_map([augment_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(spell_powers.len(), 15);
    assert!(spell_powers.contains(&("Fire Spell Power".to_string(), 111)));
    assert!(!spell_powers.iter().any(|(name, _)| name == "Universal Spell Power"));
}

#[test]
fn a_wiki_item_links_potency_by_effect_and_keeps_its_description() {
    let (db, _) = built_fixture_db();
    let item = item_id(&db, "Battle Axe of the Oozing Hunger");
    let (linked_effect, description): (String, Option<String>) = db
        .query_row(
            "SELECT e.name, e.description_template FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
              WHERE ie.item_id = ?1 AND e.name = 'Potency'",
            [item],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(linked_effect, "Potency");
    assert!(description.is_some());
    let spell_powers: Vec<(String, i64, String)> = db
        .prepare(
            "SELECT s.name, ob.amount, bt.name FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
             JOIN bonus_types bt ON bt.id = ob.bonus_type_id
             WHERE ob.owner_kind = 'item' AND ob.owner_id = ?1 AND s.name LIKE '% Spell Power'",
        )
        .unwrap()
        .query_map([item], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(spell_powers.len(), 15);
    assert!(spell_powers.iter().all(|(_, amount, bonus_type)| *amount == 49 && bonus_type == "Equipment"));
}

#[test]
fn implement_spell_power_remains_universal() {
    let (db, _) = built_fixture_db();
    let bonuses: Vec<(String, String)> = db
        .prepare(
            "SELECT s.name, bt.name FROM owner_bonuses ob
             JOIN augments a ON a.id = ob.owner_id
             JOIN effects s ON s.id = ob.stat_id
             JOIN bonus_types bt ON bt.id = ob.bonus_type_id
             WHERE ob.owner_kind = 'augment' AND a.name = 'Implement Spell Power Probe'",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(bonuses, [("Universal Spell Power".to_string(), "Implement".to_string())]);
}

#[test]
fn writes_improved_deception_from_its_definition_as_a_typed_bluff_bonus() {
    let (db, report) = built_fixture_db();
    assert_eq!(report.effect_fallback_buff_count, 23);
    assert!(report.family_buff_count > 0);
    assert!(report.effect_buff_count > 0);
    let gloves = item_id(&db, "Backstabber's Gloves (Level 25)");
    let written_bonuses: Vec<(String, i64, String)> = db
        .prepare(
            "SELECT s.name, CASE es.amount_from WHEN 0 THEN es.constant WHEN 1 THEN ie.value ELSE ie.value2 END, bt.name
             FROM item_effects ie JOIN effect_bonuses es ON es.effect_id = ie.effect_id
             JOIN effects s ON s.id = es.target_effect_id JOIN bonus_types bt ON bt.id = COALESCE(es.bonus_type_id, ie.bonus_type_id)
             WHERE ie.item_id = ?1 AND ie.value = 5",
        )
        .unwrap()
        .query_map(params![gloves], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(written_bonuses.contains(&("Bluff".into(), 5, "Enhancement".into())));
}

#[test]
fn historic_deception_steps_share_the_bluff_ladder() {
    let (upstream, _) = built_fixture_db();
    let canonical = fixture::database(|_| {});
    for db in [&upstream, &canonical] {
        let steps: Vec<(String, i64)> = db
            .prepare(
                "SELECT e.name, e.tier FROM effects e
                  JOIN effect_tier_groups l ON l.id = e.tier_group_id
                  WHERE l.name = 'Deception' ORDER BY e.tier",
            )
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(steps, [("Deception +3".into(), 1), ("Improved Deception".into(), 2)]);
        let sneak_attack_tier: (Option<i64>, Option<i64>) = db
            .query_row("SELECT tier, tier_group_id FROM effects WHERE name = 'Deception'", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(sneak_attack_tier, (None, None));
    }
}

#[test]
fn effect_fallback_rows_stay_adjacent_and_distinct_descriptions_survive() {
    let (db, _) = built_fixture_db();
    let gauntlets = item_id(&db, "Alaric's Grim Gauntlets");
    let ordered_rows: Vec<(String, i64, i64)> = db
        .prepare(
            "SELECT s.name, ob.amount, ob.effect_link_order
             FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
             WHERE ob.owner_kind = 'item' AND ob.owner_id = ?1 ORDER BY ob.effect_link_order, ob.stat_id",
        )
        .unwrap()
        .query_map(params![gauntlets], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        &ordered_rows[4..],
        &[("Armor Class".into(), 5, 5), ("Saving Throws".into(), 5, 5), ("Bluff".into(), 3, 6)]
    );

    let bluff_rows: Vec<(String, String, String)> = db
        .prepare(
            "SELECT i.name, e.verbose_name_template || ': ' || e.description_template, bt.name FROM item_effects ie
             JOIN items i ON i.id = ie.item_id JOIN effects e ON e.id = ie.effect_id
             JOIN effect_bonuses es ON es.effect_id = e.id JOIN effects s ON s.id = es.target_effect_id
             JOIN bonus_types bt ON bt.id = COALESCE(es.bonus_type_id, ie.bonus_type_id)
             WHERE i.name IN ('Alaric''s Grim Gauntlets', 'Acrobat''s Ring') \
             AND s.name = 'Bluff' AND CASE es.amount_from WHEN 0 THEN es.constant WHEN 1 THEN ie.value ELSE ie.value2 END = 3
             ORDER BY i.name",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(bluff_rows.len(), 2);
    assert_eq!(bluff_rows[0].2, "Enhancement");
    assert_eq!(bluff_rows[1].2, "Enhancement");
    assert!(bluff_rows
        .iter()
        .any(|(item, description, _)| item == "Acrobat's Ring" && description.contains("Deception")));
    assert!(bluff_rows
        .iter()
        .any(|(item, description, _)| item == "Alaric's Grim Gauntlets" && description.contains("Improved Deception")));
}

#[test]
fn excludes_cosmetic_shields_from_items() {
    let (db, _) = built_fixture_db();
    assert_eq!(count(&db, "SELECT COUNT(*) FROM items WHERE name = 'Cosmetic Jackorb'"), 0);
    let reason: String = db
        .query_row("SELECT reason FROM excluded_items WHERE name = 'Cosmetic Jackorb'", [], |r| r.get(0))
        .expect("Cosmetic Jackorb is recorded in excluded_items");
    assert_eq!(reason, "cosmetic shield");
}

#[test]
fn builds_items_and_skips_cosmetics() {
    let (db, report) = built_fixture_db();
    assert_eq!(report.written_item_count, 59);
    assert_eq!(report.effect_count as i64, count(&db, "SELECT COUNT(*) FROM effects"));
    assert_eq!(report.bonus_count as i64, count(&db, "SELECT COUNT(*) FROM effect_bonuses"));
    assert_eq!(
        report.text_only_effect_count as i64,
        count(
            &db,
            "SELECT COUNT(*) FROM effects e WHERE e.is_stat = 0 AND NOT EXISTS (SELECT 1 FROM effect_bonuses b WHERE b.effect_id = e.id)"
        )
    );
    assert_eq!(report.skipped_cosmetic_item_count, 2, "the cosmetic helm and the cosmetic shield");
    assert_eq!(count(&db, "SELECT COUNT(*) FROM items WHERE provenance = 'maetrim'"), 59);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM items WHERE name = '17th Anniversary Dark Helm'"), 0);
    let reason: String = db
        .query_row("SELECT reason FROM excluded_items WHERE name = '17th Anniversary Dark Helm'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(reason, "cosmetic-only slots");
    assert_eq!(
        report.legacy_item_count, 3,
        "the two legacy-named fixture items and the axe that drops only in a retired Temple of Elemental Evil part"
    );
    for old_version_name in ["Ratkiller (legacy) (level 4)", "Allegiance (historic)"] {
        let is_legacy: bool = db
            .query_row("SELECT is_legacy FROM items WHERE name = ?1", params![old_version_name], |r| r.get(0))
            .unwrap_or_else(|e| panic!("{old_version_name} is written: {e}"));
        assert!(is_legacy, "{old_version_name}");
        assert_eq!(count(&db, &format!("SELECT COUNT(*) FROM excluded_items WHERE name = \"{old_version_name}\"")), 0);
    }
    assert_eq!(count(&db, "SELECT COUNT(*) FROM items WHERE is_legacy"), 3);
    let (sha, schema_version): (String, i64) = db
        .query_row("SELECT upstream_sha, (SELECT version FROM schema_version) FROM dataset_version", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(sha, "31ef0201");
    assert_eq!(schema_version, ddo_model::SCHEMA_VERSION);
}

#[test]
fn writes_item_core_columns() {
    let (db, _) = built_fixture_db();
    let id = item_id(&db, "Sireth, Spear of the Sky");
    let (slot, category, item_type, minimum_level, enhancement_bonus, material, icon, accepts_sentience, drop): (String, String, String, i64, i64, String, String, bool, String) = db
        .query_row(
            "SELECT es.name, i.item_category, i.item_type, i.minimum_level, i.enhancement_bonus, m.name, i.icon, i.accepts_sentience, i.drop_location
               FROM items i JOIN equipment_slots es ON es.id = i.slot_id LEFT JOIN item_materials m ON m.id = i.material_id
              WHERE i.id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?)),
        )
        .unwrap();
    assert_eq!(slot, "Main Hand");
    assert_eq!(category, "Weapon");
    assert_eq!(item_type, "Quarterstaff");
    assert_eq!(minimum_level, 23);
    assert_eq!(enhancement_bonus, 7);
    assert_eq!(material, "Steel");
    assert_eq!(icon, "Quarterstaff_6a");
    assert!(accepts_sentience);
    assert_eq!(drop, "Caught in the Web, End Chest");

    let docent = item_id(&db, "Docent of Defiance");
    let race: String =
        db.query_row("SELECT race_required FROM items WHERE id = ?1", params![docent], |r| r.get(0)).unwrap();
    assert_eq!(race, "Construct");
    let material_name: String = db
        .query_row(
            "SELECT m.name FROM items i JOIN item_materials m ON m.id = i.material_id WHERE i.id = ?1",
            params![docent],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(material_name, "Gem", "'(material)' suffix is stripped");
    let wiki: String = db.query_row("SELECT wiki_url FROM items WHERE id = ?1", params![id], |r| r.get(0)).unwrap();
    assert_eq!(wiki, "https://ddowiki.com/page/Item:Sireth,_Spear_of_the_Sky");
}

#[test]
fn writes_weapon_stats_with_rendered_display_strings() {
    let (db, _) = built_fixture_db();
    let id = item_id(&db, "Sireth, Spear of the Sky");
    let (weapon_type, damage, critical, handedness, damage_multiplier, critical_threat_range): (
        String,
        String,
        String,
        String,
        f64,
        i64,
    ) = db
        .query_row(
            "SELECT wt.name, s.damage, s.critical, s.handedness, s.damage_multiplier, s.critical_threat_range
               FROM item_weapon_stats s JOIN weapon_types wt ON wt.id = s.weapon_type_id WHERE s.item_id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .unwrap();
    assert_eq!(weapon_type, "Quarterstaff");
    assert_eq!(damage, "3.6[1d10] + 7 Good, Magic, Pierce, Slash");
    assert_eq!(critical, "16-20 / x2");
    assert_eq!(handedness, "Two-handed");
    assert_eq!(damage_multiplier, 3.6);
    assert_eq!(critical_threat_range, 5);
    assert_eq!(count(&db, &format!("SELECT COUNT(*) FROM item_dr_bypass WHERE item_id = {id}")), 4);

    let axe = item_id(&db, "+3 Combustion Scorched Battle Axe");
    let (damage, critical, handedness): (String, String, String) = db
        .query_row("SELECT damage, critical, handedness FROM item_weapon_stats WHERE item_id = ?1", params![axe], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert_eq!(damage, "1[1d8] + 3 Magic, Slash");
    assert_eq!(critical, "20 / x3");
    assert_eq!(handedness, "One-handed");

    let shield = item_id(&db, "+1 Starter Heavy Steel Shield");
    let (category, armor_type, handedness): (String, String, String) = db
        .query_row(
            "SELECT i.item_category, a.armor_type, w.handedness FROM items i JOIN item_armor_stats a ON a.item_id = i.id JOIN item_weapon_stats w ON w.item_id = i.id WHERE i.id = ?1",
            params![shield],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((category.as_str(), armor_type.as_str(), handedness.as_str()), ("Shield", "Shield", "Off-hand"));
}

#[test]
fn writes_armor_stats() {
    let (db, _) = built_fixture_db();
    let docent = item_id(&db, "Docent of Defiance");
    let (armor_type, bonus, mithral, adamantine): (String, i64, i64, i64) = db
        .query_row(
            "SELECT armor_type, armor_bonus, mithral_body, adamantine_body FROM item_armor_stats WHERE item_id = ?1",
            params![docent],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!((armor_type.as_str(), bonus, mithral, adamantine), ("Docent", -3, 5, 12));
    let heavy = item_id(&db, "Argenti's Armor");
    let armor_type: String = db
        .query_row("SELECT armor_type FROM item_armor_stats WHERE item_id = ?1", params![heavy], |r| r.get(0))
        .unwrap();
    assert_eq!(armor_type, "Heavy");
}

#[test]
fn splits_buffs_into_bonuses_and_effects() {
    let (db, _) = built_fixture_db();
    let cloak = item_id(&db, "Legendary Cloak of Winter");
    let bonuses: Vec<(String, String, Option<String>, i64)> = db
        .prepare(
            "SELECT s.name || ' +' || ob.amount,
                    s.name, bt.name, ob.effect_link_order FROM owner_bonuses ob
               JOIN effects s ON s.id = ob.stat_id LEFT JOIN bonus_types bt ON bt.id = ob.bonus_type_id
              WHERE ob.owner_kind = 'item' AND ob.owner_id = ?1 ORDER BY ob.effect_link_order, ob.stat_id",
        )
        .unwrap()
        .query_map(params![cloak], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        bonuses,
        vec![
            ("Cold Absorption +34".to_string(), "Cold Absorption".to_string(), Some("Enhancement".to_string()), 0),
            ("Hit Points +50".to_string(), "Hit Points".to_string(), Some("Enhancement".to_string()), 1),
            (
                "Negative Absorption +34".to_string(),
                "Negative Absorption".to_string(),
                Some("Enhancement".to_string()),
                3
            ),
        ]
    );
    let effects: Vec<(String, Option<i64>, Option<String>)> = db
        .prepare(
            "SELECT e.name, ie.value, e.verbose_name_template || ': ' || COALESCE(e.description_template, '')
                  FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
                  WHERE ie.item_id = ?1 AND e.is_stat = 0 AND NOT EXISTS (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)
                  ORDER BY ie.sort_order",
        )
        .unwrap()
        .query_map(params![cloak], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].0, "Legendary Ice Barrier");
    assert!(effects[0].2.as_deref().unwrap_or("").len() > 10, "effects carry ItemBuffs.xml text");

    let fire_family_count = count(
        &db,
        "SELECT COUNT(*) FROM effects e JOIN effect_bonuses es ON es.effect_id = e.id
         JOIN effects s ON s.id = es.target_effect_id WHERE s.name = 'Fire Spell Power' AND e.name = 'Combustion'",
    );
    assert_eq!(fire_family_count, 1);
    let description_template: String = db
        .query_row(
            "SELECT e.description_template FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
         JOIN effect_bonuses es ON es.effect_id = e.id JOIN effects s ON s.id = es.target_effect_id
         WHERE ie.item_id = ?1 AND s.name = 'Hit Points'",
            [cloak],
            |r| r.get(0),
        )
        .unwrap();
    assert!(description_template.contains("{1}"), "{description_template}");
}

#[test]
fn writes_tactical_dc_buffs_as_bonuses() {
    let (db, _) = built_fixture_db();
    let ring = item_id(&db, "Legendary Ring of Unbridled Might");
    let tactical_dc_bonuses: Vec<(String, Option<String>, i64)> = db
        .prepare(
            "SELECT s.name, bt.name, ob.amount
               FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
               LEFT JOIN bonus_types bt ON bt.id = ob.bonus_type_id
              WHERE ob.owner_kind = 'item' AND ob.owner_id = ?1 AND s.name LIKE '% DC' ORDER BY s.name",
        )
        .unwrap()
        .query_map(params![ring], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        tactical_dc_bonuses,
        vec![
            ("Stun DC".to_string(), Some("Insight".to_string()), 7),
            ("Sunder DC".to_string(), Some("Insight".to_string()), 7),
        ]
    );
    let shatter_effect_count = count(
        &db,
        &format!(
            "SELECT COUNT(*) FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
             WHERE ie.item_id = {ring} AND e.name = 'Shatter' AND NOT EXISTS
               (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)"
        ),
    );
    assert_eq!(shatter_effect_count, 0);
}

#[test]
fn writes_a_buff_without_a_value_at_the_amount_its_definition_fixes() {
    let (db, _) = built_fixture_db();
    let reign = item_id(&db, "Yeenoghu's Reign");
    let rage_bonuses: Vec<(Option<String>, Option<i64>)> = db
        .prepare(
            "SELECT bt.name, ob.amount
               FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
               LEFT JOIN bonus_types bt ON bt.id = ob.bonus_type_id
              WHERE ob.owner_kind = 'item' AND ob.owner_id = ?1 AND s.name = 'Rage Uses'",
        )
        .unwrap()
        .query_map(params![reign], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        rage_bonuses,
        vec![(Some("Equipment".to_string()), Some(5))],
        "Major Anger's definition carries one Simple ExtraRage effect of 5 Equipment"
    );
    let major_anger_effect_count = count(
        &db,
        &format!(
            "SELECT COUNT(*) FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
             WHERE ie.item_id = {reign} AND e.name = 'Major Anger' AND NOT EXISTS
               (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)"
        ),
    );
    assert_eq!(major_anger_effect_count, 0);
}

#[test]
fn command_keeps_the_hide_penalty_when_its_skill_type_comes_from_the_owner() {
    let source = fixture::xml_source();
    fixture::write_command_owners(source.path());
    let index = fixture::groupings();
    let correction = fixture::new_qualified_correction(
        "item_effect",
        "Ring of Power",
        "effect = 'Command'",
        "bonus_type",
        "\"Insight\"",
        "\"Competence\"",
    );
    let corrections = fixture::corrections_from(&[("ring-of-power.toml", &correction)]).unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        index,
        source.path(),
        &WikiOverrides::default(),
        &corrections,
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    assert!(report.stale_corrections.is_empty(), "{:?}", report.stale_corrections);
    for (owner, expected_type) in [("Grudgebearer's Plate", "Insight"), ("Ring of Power", "Competence")] {
        let link = db.query_row("SELECT l.value,l.value2,bt.name FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effects e ON e.id=l.effect_id JOIN bonus_types bt ON bt.id=l.bonus_type_id WHERE i.name=?1 AND e.name='Command'", [owner], |row| Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,String>(2)?))).unwrap();
        assert_eq!(link, (2, -6, expected_type.into()));
        let bonuses = db.prepare("SELECT s.name,bt.name,b.amount FROM owner_bonuses b JOIN items i ON i.id=b.owner_id JOIN effects e ON e.id=b.via_effect_id JOIN effects s ON s.id=b.stat_id JOIN bonus_types bt ON bt.id=b.bonus_type_id WHERE b.owner_kind='item' AND i.name=?1 AND e.name='Command' ORDER BY s.name").unwrap().query_map([owner], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        let expected =
            ["Bluff", "Diplomacy", "Haggle", "Hide", "Intimidate", "Perform", "Use Magic Device"].map(|stat| {
                if stat == "Hide" {
                    (stat.into(), "Penalty".into(), -6)
                } else {
                    (stat.into(), expected_type.into(), 2)
                }
            });
        assert_eq!(bonuses, expected, "{owner}");
    }
    for (owner, amount) in [("Brightbane Emerald", 5), ("Legendary Brightbane Emerald", 7)] {
        let link = db.query_row("SELECT e.name,l.value,l.value2,bt.name FROM augment_effects l JOIN augments a ON a.id=l.augment_id JOIN effects e ON e.id=l.effect_id JOIN bonus_types bt ON bt.id=l.bonus_type_id WHERE a.name=?1", [owner], |row| Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?,row.get::<_,String>(3)?))).unwrap();
        assert_eq!(link, ("Command".into(), amount, -6, "Insight".into()));
    }
}

#[test]
fn writes_buffs_named_after_a_stat_as_bonuses() {
    let (db, _) = built_fixture_db();
    let stat_bonuses_by_item = |item_name: &str, stat_name: &str| -> Vec<(Option<String>, Option<i64>, Option<i64>)> {
        db.prepare(
            "SELECT bt.name, ob.amount, ie.value2
               FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
               JOIN item_effects ie ON ie.item_id = ob.owner_id AND ie.sort_order = ob.effect_link_order
               LEFT JOIN bonus_types bt ON bt.id = ob.bonus_type_id
              WHERE ob.owner_kind = 'item' AND ob.owner_id = ?1 AND s.name = ?2 ORDER BY ob.effect_link_order, ob.stat_id",
        )
        .unwrap()
        .query_map(params![item_id(&db, item_name), stat_name], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
    };
    let insight = Some("Insight".to_string());
    let enhancement = Some("Enhancement".to_string());
    assert_eq!(
        stat_bonuses_by_item("Backstabber's Gloves (Level 25)", "Sneak Attack"),
        vec![(enhancement.clone(), Some(5), Some(8)), (insight.clone(), Some(3), Some(5))]
    );
    assert_eq!(
        stat_bonuses_by_item("Grudgebearer's Plate", "Bluff"),
        vec![(insight.clone(), Some(2), Some(-6))],
        "Command grants an Insight bonus to Charisma skill checks"
    );
    assert_eq!(
        stat_bonuses_by_item("Grudgebearer's Plate", "Hide"),
        vec![(Some("Penalty".to_string()), Some(-6), Some(-6))]
    );
    assert_eq!(
        stat_bonuses_by_item("Bold Trinket", "Damage Bonus"),
        vec![(Some("Competence".to_string()), Some(1), None)]
    );
    assert_eq!(
        stat_bonuses_by_item("The Stablestone", "Alignment Absorption"),
        vec![(enhancement.clone(), Some(22), None)]
    );
    assert_eq!(
        stat_bonuses_by_item("Cyran Guard (Level 27)", "Elemental Absorption"),
        vec![(enhancement.clone(), Some(19), None)]
    );
    assert_eq!(
        stat_bonuses_by_item("Visor of Fraz-Urb'luu", "Illusion Save"),
        vec![(Some("Resistance".to_string()), Some(5), None)]
    );
    assert_eq!(
        stat_bonuses_by_item("Celestial Emerald Ring", "Linguistics"),
        vec![(Some("Equipment".to_string()), Some(10), None)]
    );
    let linguistic_line: String = db
        .query_row(
            "SELECT e.verbose_name_template FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
         JOIN items i ON i.id = ie.item_id WHERE i.name = 'Celestial Emerald Ring' AND e.name = 'Linguistics'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(linguistic_line, "Linguistics +{1}%");
    assert_eq!(
        stat_bonuses_by_item("Echoes of Night", "Rune Arm Charge Rate"),
        vec![(enhancement.clone(), Some(5), None)]
    );
    let equipment = Some("Equipment".to_string());
    let resistance = Some("Resistance".to_string());
    assert_eq!(
        stat_bonuses_by_item("Alaric's Grim Gauntlets", "Dark Restoration Lore"),
        vec![(equipment.clone(), Some(23), None)]
    );
    assert_eq!(
        stat_bonuses_by_item("Epic Glimpse of the Soul", "Enchantment Save"),
        vec![(resistance.clone(), Some(6), None)]
    );
    assert_eq!(stat_bonuses_by_item("Epic Glimpse of the Soul", "Illusion Save"), vec![(resistance, Some(6), None)]);
    assert_eq!(stat_bonuses_by_item("Sightless", "Melee Power"), vec![(enhancement.clone(), Some(6), None)]);
    assert_eq!(stat_bonuses_by_item("Sightless", "Ranged Power"), vec![(enhancement, Some(6), None)]);
    assert_eq!(
        stat_bonuses_by_item("A Memento of Mori", "Sacred Ground Lore"),
        vec![(equipment.clone(), Some(22), None)]
    );
    assert_eq!(stat_bonuses_by_item("Darstil's Gloves", "Spell Lore"), vec![(equipment.clone(), Some(3), None)]);
    assert_eq!(
        stat_bonuses_by_item("Alarphon's Staff", "Spell Lore"),
        vec![(equipment.clone(), Some(6), None)],
        "a numeral <Item> names no school, so the lore is to every spell"
    );
    assert_eq!(
        stat_bonuses_by_item("Epic Ring of Master Artifice", "Repair Spell Lore"),
        vec![(equipment, Some(16), None)],
        "an <Item> naming a school narrows Spell Lore to that school's lore"
    );
    assert_eq!(stat_bonuses_by_item("Epic Ring of Master Artifice", "Spell Lore"), vec![]);
    let effects_named_after_a_stat: Vec<String> = db
        .prepare(
            "SELECT DISTINCT e.name FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
              WHERE e.is_stat = 0 AND lower(replace(e.name, ' ', '')) IN (SELECT lower(replace(name, ' ', '')) FROM effects WHERE is_stat = 1)
              AND e.is_stat = 0 AND NOT EXISTS (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)
              ORDER BY e.name",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(effects_named_after_a_stat.is_empty(), "{effects_named_after_a_stat:?}");
}

#[test]
fn one_stat_effect_uses_each_owners_bonus_type() {
    let (db, _) = built_fixture_db();
    let separate_effect_count: i64 =
        db.query_row("SELECT COUNT(*) FROM effects WHERE name = 'Extra Lay on Hands'", [], |row| row.get(0)).unwrap();
    assert_eq!(separate_effect_count, 0);
    let item: (i64, String) = db
        .query_row(
            "SELECT ob.amount, bt.name FROM owner_bonuses ob JOIN items i ON i.id = ob.owner_id
             JOIN effects s ON s.id = ob.stat_id JOIN bonus_types bt ON bt.id = ob.bonus_type_id
             WHERE ob.owner_kind = 'item' AND i.name = 'Lindal''s Mighty Belt' AND s.name = 'Lay on Hands Uses'
               AND ob.via_effect_id IS NULL",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(item, (3, "Enhancement".into()));
    let feat_types: Vec<String> = db
        .prepare(
            "SELECT bt.name FROM feat_effects fe JOIN effects e ON e.id = fe.effect_id
             JOIN bonus_types bt ON bt.id = fe.bonus_type_id WHERE e.name = 'Lay on Hands Uses'",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(!feat_types.is_empty());
    assert!(feat_types.iter().all(|bonus_type| bonus_type == "Feat"));
    let illusion_stat: (i64, String) = db
        .query_row("SELECT is_stat, verbose_name_template FROM effects WHERE name = 'Illusion Save'", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(illusion_stat, (1, "%b1 Illusion Save +{1}".into()));
    let illusion_named_effect_count: i64 = db
        .query_row("SELECT COUNT(*) FROM effects WHERE name = 'Illusion Save (variable type)'", [], |row| row.get(0))
        .unwrap();
    assert_eq!(illusion_named_effect_count, 0);

    let fixed_item_types: Vec<(String, String)> = db
        .prepare(
            "SELECT e.name, bt.name FROM owner_bonuses ob
             JOIN items i ON i.id = ob.owner_id JOIN effects e ON e.id = ob.via_effect_id
             JOIN bonus_types bt ON bt.id = ob.bonus_type_id
             WHERE ob.owner_kind = 'item' AND i.name = 'Lindal''s Mighty Belt'
               AND e.name IN ('Invisibility', 'Unwieldy') ORDER BY e.name",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(fixed_item_types, [("Invisibility".into(), "Deflection".into()), ("Unwieldy".into(), "Penalty".into())]);

    for (owner_kind, owner_table, owner_name, expected_type) in [
        ("item", "items", "Visor of Fraz-Urb'luu", "Resistance"),
        ("augment", "augments", "Storm's Bulwark", "Insight"),
        ("feat", "feats", "Adamantine Body", "Feat"),
    ] {
        let sql = format!(
            "SELECT bt.name FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
             JOIN bonus_types bt ON bt.id = ob.bonus_type_id
             WHERE ob.owner_kind = ?1 AND ob.owner_id = (SELECT id FROM {owner_table} WHERE name = ?2)
               AND s.name = 'Illusion Save'"
        );
        let served_type: String = db.query_row(&sql, params![owner_kind, owner_name], |row| row.get(0)).unwrap();
        assert_eq!(served_type, expected_type, "{owner_kind} {owner_name}");
    }
}

#[test]
fn skill_ability_item_links_the_group_without_a_wrapper() {
    let (db, _) = built_fixture_db();
    let linked: (i64, String) = db
        .query_row(
            "SELECT ie.value, bt.name FROM item_effects ie
         JOIN items i ON i.id = ie.item_id JOIN effects e ON e.id = ie.effect_id
         JOIN bonus_types bt ON bt.id = ie.bonus_type_id
         WHERE i.name = 'Lindal''s Mighty Belt' AND e.name = 'Charisma Skills' AND e.is_group = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(linked, (4, "Insight".into()));
    assert_eq!(count(&db, "SELECT COUNT(*) FROM effects WHERE name = 'Skills Charisma'"), 0);
    let augment_link: (i64, String) = db
        .query_row(
            "SELECT ae.value, bt.name FROM augment_effects ae
         JOIN augments a ON a.id = ae.augment_id JOIN effects e ON e.id = ae.effect_id
         JOIN bonus_types bt ON bt.id = ae.bonus_type_id
         WHERE a.name = 'Storm''s Bulwark' AND e.name = 'Charisma Skills' AND e.is_group = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(augment_link, (5, "Insight".into()));
    let description: String = db
        .query_row("SELECT description_template FROM effects WHERE name = 'Charisma Skills'", [], |row| row.get(0))
        .unwrap();
    assert_eq!(description, "{1} %b1 bonus to all Charisma based skills.");
}

#[test]
fn writes_augment_slots_and_presets() {
    let (db, _) = built_fixture_db();
    let cloak = item_id(&db, "Legendary Cloak of Winter");
    let label: String = db
        .query_row("SELECT t.label FROM item_augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.item_id = ?1", params![cloak], |r| r.get(0))
        .unwrap();
    assert_eq!(label, "green");
    let sireth = item_id(&db, "Sireth, Spear of the Sky");
    let rows: Vec<(String, Option<String>)> = db
        .prepare(
            "SELECT t.label, o.name FROM item_augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id
               LEFT JOIN item_augment_slot_options o ON o.item_id = s.item_id AND o.slot_order = s.sort_order
              WHERE s.item_id = ?1 ORDER BY s.sort_order, o.option_order",
        )
        .unwrap()
        .query_map(params![sireth], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(rows.len(), 4, "four slots, each with exactly one fixed option");
    assert_eq!(rows[0], ("crafting: attuned to heroism 1".to_string(), Some("Planar Conflux".to_string())));
    assert_eq!(rows[1].1.as_deref(), Some("+8 Enhancement Bonus"));
    assert_eq!(count(&db, &format!("SELECT COUNT(*) FROM item_augment_slot_options WHERE item_id = {cloak}")), 0);
}

fn option_id(db: &Connection, item_name: &str, slot_order: i64) -> i64 {
    db.query_row(
        "SELECT o.id FROM item_augment_slot_options o JOIN items i ON i.id = o.item_id
          WHERE i.name = ?1 AND o.slot_order = ?2 AND o.option_order = 0",
        params![item_name, slot_order],
        |r| r.get(0),
    )
    .unwrap_or_else(|e| panic!("{item_name} slot {slot_order}: {e}"))
}

fn option_strings(db: &Connection, sql: &str, option_id: i64) -> Vec<String> {
    db.prepare(sql).unwrap().query_map(params![option_id], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

const OPTION_GRANT_LABELS_SQL: &str = "SELECT t.label FROM item_augment_slot_option_grants g
    JOIN augment_slot_types t ON t.id = g.slot_id WHERE g.option_id = ?1 ORDER BY g.sort_order";

const OPTION_BONUSES_SQL: &str = "SELECT s.name || ' ' || bt.name || ' ' || ob.amount
     FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
     JOIN bonus_types bt ON bt.id = ob.bonus_type_id
     WHERE ob.owner_kind = 'item_augment_slot_option' AND ob.owner_id = ?1
     ORDER BY ob.effect_link_order, ob.stat_id";

#[test]
fn keeps_what_an_augment_slot_option_gives_on_the_option() {
    let (db, _) = built_fixture_db();
    let first_tier = option_id(&db, "+3 Combustion Scorched Battle Axe", 0);
    assert_eq!(
        option_strings(&db, OPTION_BONUSES_SQL, first_tier),
        ["Spell Penetration Equipment 1", "Armor Class Insight 1"],
        "the option's simple effects become typed bonuses on the option"
    );
    assert_eq!(
        option_strings(
            &db,
            "SELECT effect_type FROM modifiers WHERE source_kind = 'item_augment_slot_option' AND source_id = ?1 ORDER BY sort_order",
            first_tier
        ),
        ["SpellPenetrationBonus", "ACBonus"]
    );
    let second_tier = option_id(&db, "+3 Combustion Scorched Battle Axe", 1);
    assert_eq!(option_strings(&db, OPTION_GRANT_LABELS_SQL, second_tier), ["purple"]);
    assert_eq!(option_strings(&db, OPTION_BONUSES_SQL, second_tier), ["Fire Spell Lore Equipment 13"]);
    let axe = item_id(&db, "+3 Combustion Scorched Battle Axe");
    assert_eq!(count(&db, &format!("SELECT COUNT(*) FROM item_augment_slots WHERE item_id = {axe}")), 2);
    assert_eq!(
        count(
            &db,
            &format!(
                "SELECT COUNT(*) FROM item_effects ie JOIN effect_bonuses es ON es.effect_id = ie.effect_id
                 JOIN effects s ON s.id = es.target_effect_id WHERE ie.item_id = {axe}
                 AND s.name IN ('Spell Penetration', 'Fire Spell Lore')"
            )
        ),
        0,
        "a tier the player still unlocks gives the item none of its bonuses"
    );

    assert_eq!(option_strings(&db, OPTION_GRANT_LABELS_SQL, option_id(&db, "Sireth, Spear of the Sky", 3)), ["red"]);
    let planar_conflux = option_id(&db, "Sireth, Spear of the Sky", 0);
    let icon: Option<String> = db
        .query_row("SELECT icon FROM item_augment_slot_options WHERE id = ?1", params![planar_conflux], |r| r.get(0))
        .unwrap();
    assert_eq!(icon.as_deref(), Some("Heroism"));
    assert_eq!(
        option_strings(&db, OPTION_GRANT_LABELS_SQL, option_id(&db, "Baz'Morath, the Curator of Decay", 0)),
        ["purple"],
        "<AddAugment> adds a socket as <GrantAugment> does"
    );

    let fabricators_ingenuity = option_id(&db, "Fabricator's Gauntlets", 0);
    assert_eq!(
        option_strings(
            &db,
            "SELECT s.name FROM item_augment_slot_option_sets os JOIN set_bonuses s ON s.id = os.set_id WHERE os.option_id = ?1",
            fabricators_ingenuity
        ),
        ["Fabricator's Ingenuity"]
    );
    let gauntlets = item_id(&db, "Fabricator's Gauntlets");
    assert_eq!(count(&db, &format!("SELECT COUNT(*) FROM set_bonus_items WHERE item_id = {gauntlets}")), 0);
}

#[test]
fn links_items_to_quests_from_drop_location() {
    let (db, report) = built_fixture_db();
    assert_eq!(count(&db, "SELECT COUNT(*) FROM quests WHERE NOT is_challenge AND provenance = 'maetrim'"), 23);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM patrons"), 22);
    assert!(count(&db, "SELECT COUNT(*) FROM adventure_packs") >= 5);
    let (level, epic_level, is_raid, pack): (i64, Option<i64>, bool, String) = db
        .query_row(
            "SELECT q.level, q.epic_level, q.is_raid, p.name FROM quests q JOIN adventure_packs p ON p.id = q.pack_id WHERE q.name = 'The Chronoscope'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!((level, epic_level, is_raid, pack.as_str()), (6, Some(21), true, "Devil Assault"));

    let sireth = item_id(&db, "Sireth, Spear of the Sky");
    let (quest, loot): (String, String) = db
        .query_row(
            "SELECT q.name, ql.loot_type FROM sources ql JOIN quests q ON q.id = ql.quest_id WHERE ql.item_id = ?1",
            params![sireth],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((quest.as_str(), loot.as_str()), ("Caught in the Web", "raid"), "Caught in the Web is a raid");

    let docent = item_id(&db, "Docent of Defiance");
    let loot: String = db
        .query_row("SELECT loot_type FROM sources WHERE kind = 'quest' AND item_id = ?1", params![docent], |r| r.get(0))
        .unwrap();
    assert_eq!(loot, "chest", "'The Cursed Crypt, End Chest' is a non-raid chest drop");

    let axe = item_id(&db, "+3 Combustion Scorched Battle Axe");
    assert_eq!(count(&db, &format!("SELECT COUNT(*) FROM sources WHERE kind = 'quest' AND item_id = {axe}")), 0);
    let drop: String =
        db.query_row("SELECT drop_location FROM items WHERE id = ?1", params![axe], |r| r.get(0)).unwrap();
    assert!(drop.starts_with("Temple of Elemental Evil Part One"));
    assert!(report.quest_loot_link_count >= 4);
}

fn quest_loot_types(db: &Connection, quest: &str, item: &str) -> Vec<String> {
    quest_loot_chests(db, quest, item).into_iter().map(|(loot_type, _)| loot_type).collect()
}

fn quest_loot_chests(db: &Connection, quest: &str, item: &str) -> Vec<(String, Option<String>)> {
    let mut statement = db
        .prepare(
            "SELECT ql.loot_type, ql.chest FROM sources ql JOIN quests q ON q.id = ql.quest_id JOIN items i ON i.id = ql.item_id
              WHERE q.name = ?1 AND i.name = ?2 ORDER BY ql.loot_type",
        )
        .unwrap();
    statement.query_map(params![quest, item], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
}

#[test]
fn links_an_item_once_per_loot_type_the_quests_own_segment_names() {
    let (db, _) = built_fixture_db();
    assert_eq!(
        quest_loot_types(&db, "The Tide Turns", "Rusted Crown"),
        ["chest", "reward"],
        "'The Tide Turns, End Chest, End Reward' is both"
    );
    assert_eq!(
        quest_loot_chests(&db, "The Tide Turns", "Rusted Crown"),
        [("chest".to_string(), Some("end chest".to_string())), ("reward".to_string(), None)],
        "the chest row names only the chest, and a reward is not a chest"
    );
    assert_eq!(
        quest_loot_types(&db, "Project Nemesis", "Band of Diani ir'Wynarn"),
        ["raid"],
        "a saga's end reward in another segment is not the raid's own reward"
    );
}

#[test]
fn links_augments_to_the_quests_their_descriptions_name() {
    let (db, report) = built_fixture_db();
    let augment_links = |augment: &str| -> Vec<(String, String, bool, Option<String>)> {
        let mut statement = db
            .prepare(
                "SELECT q.name, qal.loot_type, qal.is_rare, qal.chest FROM sources qal
                   JOIN quests q ON q.id = qal.quest_id JOIN augments a ON a.id = qal.augment_id
                  WHERE a.name = ?1 ORDER BY q.name",
            )
            .unwrap();
        statement
            .query_map([augment], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert_eq!(
        augment_links("Solar Gem of Elemental Absorption (Heroic)"),
        [("Land of Lamordia".into(), "chest".into(), true, Some("vornir frosthelm's chest".into()))]
    );
    assert_eq!(
        augment_links("Lunar Gem of Magical Protection (Heroic)"),
        [("Book Burning".into(), "chest".into(), true, Some("end chest".into()))],
        "the quests his text names that the fixture lacks link nothing; the wiki fixture marks this one rare"
    );
    assert!(augment_links("Lunar Gem of Evocation (Heroic)").is_empty(), "his 'Drops in: ?' names no quest");
    assert_eq!((report.quest_augment_loot_link_count, report.drop_text_rare_augment_link_count), (6, 1));
    let description: String = db
        .query_row(
            "SELECT description FROM augments WHERE name = 'Lunar Gem of Magical Protection (Heroic)'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(description.ends_with("A Light in the Attic, secret area chest"), "the description keeps its drop text");
}

#[test]
fn records_the_chest_each_item_drops_from() {
    let (db, _) = built_fixture_db();
    let chest_of = |quest: &str, item: &str| -> Option<String> {
        db.query_row(
            "SELECT ql.chest FROM sources ql JOIN quests q ON q.id = ql.quest_id JOIN items i ON i.id = ql.item_id
              WHERE q.name = ?1 AND i.name = ?2",
            [quest, item],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(chest_of("The Cursed Crypt", "Docent of Defiance").as_deref(), Some("end chest"));
    assert_eq!(
        chest_of("Book Burning", "Buckler of the Golden Age").as_deref(),
        Some("end chest"),
        "rare marker trimmed"
    );
    assert_eq!(
        chest_of("Land of Lamordia", "Gravekeeper's Docent").as_deref(),
        Some("red-named rare encounter chests")
    );
}

#[test]
fn matches_quest_names_his_drop_text_capitalises_differently() {
    let (db, _) = built_fixture_db();
    let item_chests = |item: &str| -> Vec<(String, Option<String>)> {
        let mut statement = db
            .prepare(
                "SELECT q.name, ql.chest FROM sources ql JOIN quests q ON q.id = ql.quest_id
                   JOIN items i ON i.id = ql.item_id WHERE i.name = ?1 ORDER BY q.name",
            )
            .unwrap();
        statement.query_map([item], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
    };
    let end_chest = Some("end chest".to_string());
    assert_eq!(
        item_chests("Prismatic Cloak, Grey (Level 27)"),
        [
            ("A Break in the Ice".to_string(), end_chest.clone()),
            ("Breaking the Ranks".to_string(), end_chest.clone()),
            ("Lines of Supply".to_string(), end_chest.clone()),
            ("The Tracker's Trap".to_string(), end_chest.clone()),
            ("What Goes Up".to_string(), end_chest.clone()),
        ],
        "'A Break In the Ice' is the quest 'A Break in the Ice', not the chest of the quests before it"
    );
    let augment_quests = |augment: &str| -> Vec<String> {
        let mut statement = db
            .prepare(
                "SELECT q.name FROM sources qal JOIN quests q ON q.id = qal.quest_id
                   JOIN augments a ON a.id = qal.augment_id WHERE a.name = ?1",
            )
            .unwrap();
        statement.query_map([augment], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    };
    assert_eq!(augment_quests("Ruby of Fey Bane"), ["Wake Me Up Inside"]);
    assert_eq!(augment_quests("Storm's Bulwark"), ["The Knight Who Cried Windmill"]);
}

#[test]
fn matches_the_epic_name_a_quest_goes_by() {
    let (db, _) = built_fixture_db();
    assert_eq!(
        quest_loot_chests(&db, "The Prison of the Planes", "Kardin's Eye (Level 24)"),
        [("chest".to_string(), Some("xoriat room's chest".to_string()))],
        "'Return to Prison of the Planes' is the epic name of The Prison of the Planes"
    );
}
#[test]
fn matches_a_quest_name_his_drop_text_wraps_onto_the_next_line() {
    let (db, _) = built_fixture_db();
    let mut statement = db
        .prepare(
            "SELECT q.name, qal.is_rare, qal.chest FROM sources qal JOIN quests q ON q.id = qal.quest_id
               JOIN augments a ON a.id = qal.augment_id WHERE a.name = 'Ruby of Acid Blast' ORDER BY q.name",
        )
        .unwrap();
    let links: Vec<(String, bool, Option<String>)> =
        statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect();
    let rare_encounter_chests = Some("rare encounter chests".to_string());
    assert_eq!(
        links,
        [
            ("ToEE: First Level and Earth Temple".to_string(), false, rare_encounter_chests.clone()),
            ("ToEE: Lower Temple Complex".to_string(), false, rare_encounter_chests),
        ],
        "'ToEE: Lower\\nTemple Complex' is one quest name, not the chest 'and toee: lower'"
    );
}

#[test]
fn writes_quest_difficulties_and_epic_name() {
    let (db, _) = built_fixture_db();
    let quest = |name: &str| -> (String, Option<String>) {
        db.query_row("SELECT difficulties, epic_name FROM quests WHERE name = ?1", params![name], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap_or_else(|e| panic!("{name}: {e}"))
    };
    assert_eq!(quest("The Chronoscope"), (r#"["casual","normal","hard","elite","reaper"]"#.into(), None));
    assert_eq!(quest("The Grotto"), (r#"["solo"]"#.into(), None));
    assert_eq!(quest("Land of Lamordia"), ("[]".into(), None));
    assert_eq!(
        quest("Madstone Crater"),
        (r#"["normal","hard","elite","reaper"]"#.into(), Some("Return to Madstone Crater".into()))
    );
}

#[test]
fn diff_reports_coverage_against_a_legacy_database() {
    let (db, _) = built_fixture_db();
    let legacy_db = Connection::open_in_memory().unwrap();
    legacy_db
        .execute_batch(
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
             INSERT INTO items (name) VALUES ('Sireth, Spear of the Sky'), ('Docent of Defiance'), ('Kundarak Delving Boots'),
                                             ('Something Only The Wiki Had'), ('17th Anniversary Dark Helm');",
        )
        .unwrap();
    let coverage = item_coverage(&db, &legacy_db).unwrap();
    assert_eq!(coverage.matched_count, 3);
    assert_eq!(coverage.names_only_in_legacy, vec!["Something Only The Wiki Had".to_string()]);
    assert_eq!(
        coverage.names_excluded_by_design,
        vec!["17th Anniversary Dark Helm".to_string()],
        "cosmetics are not gaps"
    );
    assert_eq!(
        coverage.names_only_in_built.len(),
        57,
        "the wiki fixture item, Lindal's Mighty Belt, Ship Kickers and the legacy fixture items are only in the build"
    );
    assert!((coverage.coverage_ratio() - 0.75).abs() < 1e-9);
}

#[test]
fn writes_augments_with_slots_bonuses_and_modifiers() {
    let (db, report) = built_fixture_db();
    assert_eq!(report.augment_count, 18);
    let ruby: i64 =
        db.query_row("SELECT id FROM augments WHERE name = 'Ruby of Acid Damage'", [], |r| r.get(0)).unwrap();
    let (family, has_selectable_level, levels, values): (String, bool, String, String) = db
        .query_row(
            "SELECT family, choose_level, levels, level_values FROM augments WHERE id = ?1",
            params![ruby],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(family, "Ruby");
    assert!(has_selectable_level);
    assert_eq!(levels, "[1,4,8,12,16,20,24,28,32,36]");
    assert_eq!(values, "[1,2,3,4,5,6,7,8,9,10]");
    let slots: Vec<String> = db
        .prepare("SELECT t.label FROM augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.augment_id = ?1 ORDER BY t.label")
        .unwrap()
        .query_map(params![ruby], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(slots, vec!["orange", "purple", "red"]);
    assert_eq!(count(&db, &format!("SELECT COUNT(*) FROM augment_effects WHERE augment_id = {ruby}")), 0);
    let (modifier_count, dice_damage, dice_counts): (i64, String, String) = db
        .query_row(
            "SELECT COUNT(*), MIN(dice_damage), MIN(dice_number) FROM modifiers WHERE source_kind = 'augment' AND source_id = ?1",
            params![ruby],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((modifier_count, dice_damage.as_str(), dice_counts.as_str()), (2, "Acid", "[2]"));

    let silver: i64 = db.query_row("SELECT id FROM augments WHERE name = 'Silverscale'", [], |r| r.get(0)).unwrap();
    let bonuses: Vec<(String, String)> = db
        .prepare(
            "SELECT s.name || ' +' || ob.amount, bt.name FROM owner_bonuses ob
              JOIN effects s ON s.id = ob.stat_id JOIN bonus_types bt ON bt.id = ob.bonus_type_id
              WHERE ob.owner_kind = 'augment' AND ob.owner_id = ?1 ORDER BY ob.effect_link_order, ob.stat_id",
        )
        .unwrap()
        .query_map(params![silver], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        bonuses,
        vec![
            ("Healing Amplification +56".to_string(), "Competence".to_string()),
            ("Negative Healing Amplification +56".to_string(), "Profane".to_string()),
            ("Repair Amplification +56".to_string(), "Enhancement".to_string()),
        ]
    );
    let label: String = db
        .query_row(
            "SELECT t.label FROM augment_slots s JOIN augment_slot_types t ON t.id = s.slot_id WHERE s.augment_id = ?1",
            params![silver],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(label, "isle of dread: scale (armor)");

    let (added_augment, slot, description): (String, String, String) = db
        .query_row(
            "SELECT a.adds_augment, t.label, a.effect_description FROM augments a JOIN augment_slots s ON s.augment_id = a.id JOIN augment_slot_types t ON t.id = s.slot_id WHERE a.name = 'Fire I: Combustion'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(added_augment, "Legendary Alchemical Tier 2");
    assert_eq!(slot, "crafting: legendary alchemical tier 1");
    assert!(description.starts_with("Combustion 152") && description.contains("Fire Lore +21%"), "{description}");
    let fire: Vec<String> = db
        .prepare(
            "SELECT s.name || ' +' || ob.amount
                  FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
                  JOIN augments a ON a.id = ob.owner_id
                  WHERE ob.owner_kind = 'augment' AND a.name = 'Fire I: Combustion'
                  ORDER BY ob.effect_link_order, ob.stat_id",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(fire, vec!["Fire Spell Power +152", "Fire Spell Lore +21"]);

    let (suppresses_set_bonus, set): (bool, String) = db
        .query_row("SELECT suppress_set_bonus, set_bonus FROM augments WHERE name = 'Perfect Silence'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert!(suppresses_set_bonus);
    assert_eq!(set, "Perfect Silence");
}

#[test]
fn imported_cold_critical_damage_and_missile_deflection_keep_percent_units() {
    let source = fixture::xml_source();
    std::fs::create_dir_all(source.path().join("FiligreeSets")).unwrap();
    std::fs::create_dir_all(source.path().join("FiligreeImages")).unwrap();
    for (name, contents, icon) in [
        (
            "FrozenWanderer",
            include_str!("fixtures/effect-groups/FrozenWanderer.Filigree.xml"),
            include_bytes!("fixtures/effect-groups/icons/FrozenWanderer.png").as_slice(),
        ),
        (
            "DanceOfTheWind",
            include_str!("fixtures/effect-groups/DanceOfTheWind.Filigree.xml"),
            include_bytes!("fixtures/effect-groups/icons/DanceOfTheWind.png").as_slice(),
        ),
    ] {
        std::fs::write(source.path().join(format!("FiligreeSets/{name}.Filigree.xml")), contents).unwrap();
        std::fs::write(source.path().join(format!("FiligreeImages/{name}.png")), icon).unwrap();
    }
    let mut db = Connection::open_in_memory().unwrap();
    build_database(
        fixture::grouping_cases(),
        source.path(),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    let templates = db.prepare(
        "SELECT e.name,e.verbose_name_template,l.value FROM set_bonus_tier_effects l JOIN set_bonus_tiers t ON t.id=l.tier_id JOIN set_bonuses s ON s.id=t.set_id JOIN effects e ON e.id=l.effect_id WHERE s.name IN ('Frozen Wanderer','Dance of the Wind') AND t.equipped_count=2 ORDER BY e.name"
    ).unwrap().query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(
        templates,
        [
            ("Cold Spell Critical Damage".into(), "%b1 Cold Spell Critical Damage +{1}%".into(), 5),
            ("Missile Deflection".into(), "%b1 Missile Deflection +{1}%".into(), 5),
        ]
    );
}

#[test]
fn writes_sets_filigrees_and_their_items() {
    let (db, report) = built_fixture_db();
    assert_eq!(report.set_bonus_count, 10, "nine gear sets and one filigree set");
    let (icon, filigree): (String, bool) = db
        .query_row("SELECT icon, is_filigree_set FROM set_bonuses WHERE name = 'The Inevitable Grave'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert!(!icon.is_empty());
    assert!(filigree);
    let winter: i64 =
        db.query_row("SELECT id FROM set_bonuses WHERE name = 'Eminence of Winter'", [], |r| r.get(0)).unwrap();
    let tiers: Vec<(i64, String)> = db
        .prepare(
            "SELECT t.equipped_count, e.name FROM set_bonus_tiers t
                  JOIN set_bonus_tier_effects te ON te.tier_id = t.id AND te.sort_order = 0
                  JOIN effects e ON e.id = te.effect_id
                  WHERE t.set_id = ?1 ORDER BY t.equipped_count",
        )
        .unwrap()
        .query_map(params![winter], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(!tiers.is_empty());
    assert!(tiers.iter().all(|(n, _)| *n >= 2));
    let tiers_with_prose_and_structured_facts = count(
        &db,
        "SELECT COUNT(*) FROM set_bonus_tier_effects te JOIN effects prose ON prose.id = te.effect_id
         WHERE prose.is_stat = 0 AND NOT EXISTS (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = te.effect_id)
           AND EXISTS (SELECT 1 FROM set_bonus_tier_effects structured JOIN effects e ON e.id = structured.effect_id
                       WHERE structured.tier_id = te.tier_id AND (e.is_stat = 1 OR EXISTS
                       (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)))",
    );
    assert_eq!(tiers_with_prose_and_structured_facts, 0);
    let tier_modifier_count = count(&db, &format!("SELECT COUNT(*) FROM modifiers m JOIN set_bonus_tiers t ON t.id = m.source_id WHERE m.source_kind = 'set_bonus_tier' AND t.set_id = {winter}"));
    assert!(tier_modifier_count >= 1);
    let winter_tier_bonuses: Vec<(i64, String, i64)> = db
        .prepare(
            "SELECT t.equipped_count, s.name, ob.amount
               FROM set_bonus_tiers t JOIN owner_bonuses ob ON ob.owner_kind = 'set_bonus_tier' AND ob.owner_id = t.id
               JOIN effects s ON s.id = ob.stat_id
              WHERE t.set_id = ?1 ORDER BY t.equipped_count, ob.effect_link_order, ob.stat_id",
        )
        .unwrap()
        .query_map(params![winter], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(
        winter_tier_bonuses.contains(&(2, "Physical Resistance Rating".into(), 30))
            && winter_tier_bonuses.contains(&(6, "Constitution".into(), 3)),
        "a tier's plain stat effects derive bonuses as a feat's do: {winter_tier_bonuses:?}"
    );
    let members: Vec<String> = db
        .prepare("SELECT i.name FROM set_bonus_items sbi JOIN items i ON i.id = sbi.item_id WHERE sbi.set_id = ?1")
        .unwrap()
        .query_map(params![winter], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(members, vec!["Legendary Cloak of Winter"]);

    assert_eq!(count(&db, "SELECT COUNT(*) FROM filigrees"), 4);
    let (set_name, menu): (String, String) = db
        .query_row("SELECT s.name, f.menu FROM filigrees f JOIN set_bonuses s ON s.id = f.set_id LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(set_name, "The Inevitable Grave");
    assert!(!menu.is_empty());
    assert!(
        count(&db, "SELECT COUNT(*) FROM modifiers WHERE source_kind = 'filigree' AND is_rare = 1") >= 1,
        "rare filigree bonuses are flagged"
    );
}

#[test]
fn all_ability_set_tier_uses_one_family_with_six_stats() {
    let (db, _) = built_fixture_db();
    let families: Vec<(String, i64)> = db
        .prepare(
            "SELECT e.name, COUNT(es.target_effect_id) FROM set_bonus_tier_effects l
             JOIN set_bonus_tiers t ON t.id = l.tier_id
             JOIN set_bonuses sb ON sb.id = t.set_id
             JOIN effects e ON e.id = l.effect_id
             JOIN effect_bonuses es ON es.effect_id = e.id
             WHERE sb.name = 'Fried & Frozen Frenzy' AND t.equipped_count = 2
             GROUP BY e.id",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(families, [("All Ability Scores".to_string(), 6)]);
    let templates: (String, Option<String>) = db
        .query_row(
            "SELECT e.verbose_name_template, e.description_template FROM effects e
         WHERE e.name = 'All Ability Scores'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        templates,
        ("%b1 All Ability Scores +{1}".to_string(), Some("{1} %b1 bonus to all Ability Scores".to_string()))
    );
}

#[test]
fn set_tier_links_one_group_when_both_members_share_type_and_value() {
    let (db, _) = built_fixture_db();
    let tier_id: i64 = db
        .query_row(
            "SELECT t.id FROM set_bonus_tiers t JOIN set_bonuses s ON s.id = t.set_id
             WHERE s.name = 'Kundarak Delving Equipment' AND t.equipped_count = 3",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let links: Vec<(String, i64, String)> = db
        .prepare(
            "SELECT e.name, l.value, bt.name FROM set_bonus_tier_effects l
             JOIN effects e ON e.id = l.effect_id JOIN bonus_types bt ON bt.id = l.bonus_type_id
             WHERE l.tier_id = ?1 AND e.name IN ('Doublestrike', 'Doubleshot', 'Doublestrike and Doubleshot')",
        )
        .unwrap()
        .query_map([tier_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(links, [("Doublestrike and Doubleshot".to_string(), 5, "Artifact".to_string())]);
    let bonuses: Vec<(String, i64)> = db
        .prepare(
            "SELECT e.name, ob.amount FROM owner_bonuses ob JOIN effects e ON e.id = ob.stat_id
             WHERE ob.owner_kind = 'set_bonus_tier' AND ob.owner_id = ?1
               AND e.name IN ('Doublestrike', 'Doubleshot') ORDER BY e.name",
        )
        .unwrap()
        .query_map([tier_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(bonuses, [("Doubleshot".to_string(), 5), ("Doublestrike".to_string(), 5)]);
}

#[test]
fn links_an_item_to_every_set_it_names() {
    let (db, _) = built_fixture_db();
    let fried_sword_fish = item_id(&db, "Fried Sword Fish");
    let set_names: Vec<String> = db
        .prepare(
            "SELECT s.name FROM set_bonus_items sbi JOIN set_bonuses s ON s.id = sbi.set_id
               WHERE sbi.item_id = ?1 ORDER BY s.name",
        )
        .unwrap()
        .query_map([fried_sword_fish], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(set_names, ["Cooking By the Book", "Fried & Frozen Frenzy"]);
}

#[test]
fn links_sets_to_the_augments_that_grant_them() {
    let (db, _) = built_fixture_db();
    let rows: Vec<(String, String)> = db
        .prepare(
            "SELECT s.name, a.name FROM set_bonus_augments sba JOIN set_bonuses s ON s.id = sba.set_id
               JOIN augments a ON a.id = sba.augment_id WHERE a.provenance = 'maetrim' ORDER BY s.name, a.name",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(rows, vec![("Perfect Silence".to_string(), "Perfect Silence".to_string())]);
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM set_bonus_items sbi JOIN set_bonuses s ON s.id = sbi.set_id WHERE s.name = 'Perfect Silence'"),
        0,
        "no item names the set; only the augment grants it"
    );
}

#[test]
fn writes_clickies_and_item_level_effects() {
    let (db, report) = built_fixture_db();
    assert_eq!(report.clickie_count, 2);
    let alabaster = item_id(&db, "Alabaster of the Twelve");
    let name: String = db
        .query_row(
            "SELECT c.name FROM item_clickies ic JOIN clickies c ON c.id = ic.clickie_id WHERE ic.item_id = ?1",
            params![alabaster],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(name, "Cure Serious Wounds, Mass");
    let effect_types: Vec<String> = db
        .prepare("SELECT effect_type FROM modifiers WHERE source_kind = 'item' AND source_id = ?1 ORDER BY sort_order")
        .unwrap()
        .query_map(params![alabaster], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(effect_types.contains(&"ItemClickie".to_string()), "{effect_types:?}");
    let rune_arm = item_id(&db, "Acid Rune Arm");
    let (clickie_name, clickie_id): (String, Option<i64>) = db
        .query_row("SELECT name, clickie_id FROM item_clickies WHERE item_id = ?1", params![rune_arm], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(clickie_name, "Acid Shot");
    assert!(clickie_id.is_some(), "a name ItemClickies.xml defines resolves immediately");
    assert!(report.modifier_count > 20);
}

#[test]
fn parses_and_writes_sentient_gems() {
    let parsed_gems = sentient_gems::parse(&data_files_fixture_dir().join("Sentient.gems.xml")).unwrap();
    let names: Vec<&str> = parsed_gems.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(
        names,
        ["Sentient Jewel of the Hopeful", "Sentient Jewel of the Inquisitive", "Sentient Jewel of the Resolute"]
    );
    let (db, report) = built_fixture_db();
    assert_eq!(report.sentient_gem_count, 3);
    let (icon, description): (String, String) = db
        .query_row(
            "SELECT icon, description FROM sentient_gems WHERE name = 'Sentient Jewel of the Resolute'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((icon.as_str(), description.as_str()), ("SentientJewel_Blue", "Voiced by: Ally Murphy"));
}

#[test]
fn parses_challenges_with_their_level_range() {
    let parsed_challenges = challenges::parse(&data_files_fixture_dir().join("Challenges.xml")).unwrap();
    assert_eq!(parsed_challenges.len(), 3, "the grouping comments are skipped");
    let door = &parsed_challenges[0];
    assert_eq!(door.name, "Dr. Rushmore's Mansion - Behind the Door");
    assert_eq!(door.patron.as_deref(), Some("House Cannith"));
    assert_eq!(door.adventure_pack.as_deref(), Some("Free to Play"));
    assert_eq!(door.level_range, [4, 15]);
    assert_eq!(parsed_challenges[2].patron, None, "<Patron>None</Patron> is no patron");
}

#[test]
fn writes_challenges_into_quests() {
    let (db, report) = built_fixture_db();
    assert_eq!(report.challenge_count, 3);
    let challenge = |name: &str| -> (Option<String>, String, i64, i64, bool, Option<i64>) {
        db.query_row(
            "SELECT pt.name, p.name, q.level, q.max_level, q.is_challenge, q.epic_level
               FROM quests q JOIN adventure_packs p ON p.id = q.pack_id LEFT JOIN patrons pt ON pt.id = q.patron_id
              WHERE q.name = ?1",
            params![name],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"))
    };
    assert_eq!(
        challenge("Dr. Rushmore's Mansion - Moving Targets - EPIC"),
        (Some("House Cannith".into()), "Secrets of the Artificers".into(), 21, 25, true, None)
    );
    assert_eq!(
        challenge("Extraplanar Mining - Epic The Dragon's Horde"),
        (None, "Free to Play".into(), 15, 20, true, None)
    );
    let (is_challenge, max_level): (bool, Option<i64>) = db
        .query_row("SELECT is_challenge, max_level FROM quests WHERE name = 'The Grotto'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((is_challenge, max_level), (false, None), "regular quests are not challenges");
}

#[test]
fn builds_with_foreign_keys_enforced_on_every_insert() {
    let (db, _) = built_fixture_db();
    let is_enforced: bool = db.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
    assert!(is_enforced, "the build connection enforces foreign keys, so a row naming a missing one fails the build");
    assert_eq!(count(&db, "SELECT COUNT(*) FROM pragma_foreign_key_check()"), 0);
}
