mod fixture;

use ddo_etl::{
    build,
    corrections::{CorrectionValue, Corrections},
    tree,
    wiki::WikiOverrides,
};
use ddo_model::DatasetVersion;
use rusqlite::Connection;
use std::path::Path;

#[test]
fn exported_ddobuilder_tree_builds_with_dc_icons_from_other_kinds() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import_ddobuilder/ba7e18e/DataFiles");
    let root = std::env::temp_dir().join(format!("ddo-import-dc-icons-{}", std::process::id()));
    let mut db = Connection::open_in_memory().unwrap();
    build::build_database(
        fixture::groupings(),
        &source,
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &DatasetVersion {
            upstream_sha: "ba7e18eecafb1bb5fd6b6cf52cc0b6395bbeb20c".to_string(),
            built_at: "2026-10-17T00:00:00Z".to_string(),
        },
    )
    .unwrap();
    tree::export_ddobuilder_tree(&db, &source, &root).unwrap();
    let mut rebuilt = Connection::open_in_memory().unwrap();
    tree::build_from_data(&root, &mut rebuilt).unwrap();
    for (destination, original) in [
        ("characters/enhancements/half-orc/icons/Knock.png", "FeatImages/Knock.png"),
        ("icons/feats/Intimidate.png", "UIImages/Intimidate.png"),
        ("icons/feats/Diplomacy.png", "UIImages/Diplomacy.png"),
        ("icons/feats/Bluff.png", "UIImages/Bluff.png"),
    ] {
        assert_eq!(std::fs::read(root.join(destination)).unwrap(), std::fs::read(source.join(original)).unwrap());
    }
    let completed = ddo_etl::icons::complete_referenced_icons(&root).unwrap();
    assert_eq!(completed, ddo_etl::icons::IconCompletionReport::default());
    tree::export_ddobuilder_tree(&db, &source, &root).unwrap();
    tree::build_from_data(&root, &mut Connection::open_in_memory().unwrap()).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

use ddo_etl::import_ddobuilder::{import_ddobuilder, DdobuilderImportOptions};
use std::path::PathBuf;

const PREVIOUS_SHA: &str = "ba7e18eecafb1bb5fd6b6cf52cc0b6395bbeb20c";
const NEW_SHA: &str = "d1350a0c6ec396a65dc02680fa9801a14a79ebf3";

fn abjuration_home_import() -> ImportWorld {
    ImportWorld::with_sources(fixture::groupings(), Corrections::default(), WikiOverrides::default(), |source| {
        let path = source.join("ItemBuffs.xml");
        let original = std::fs::read_to_string(&path).unwrap();
        std::fs::write(
            path,
            original.replace(
                "</Buffs>",
                &format!("{}\n</Buffs>", include_str!("fixtures/effect-groups/AbjurationFocus.Buffs.xml")),
            ),
        )
        .unwrap();
        for (bonus_type, count) in [("Insight", 4), ("Equipment", 3)] {
            for index in 0..count {
                write_abjuration_carrier(source, bonus_type, index);
            }
        }
    })
}

fn write_abjuration_carrier(source: &Path, bonus_type: &str, index: usize) {
    std::fs::create_dir_all(source.join("Items")).unwrap();
    let item = include_str!("fixtures/effect-groups/AbjurationFocus.item")
        .replace("Abjuration Focus Carrier", &format!("Home {bonus_type} {index}"))
        .replace("<BonusType>Insight</BonusType>", &format!("<BonusType>{bonus_type}</BonusType>"));
    std::fs::write(source.join(format!("Items/Home{bonus_type}{index}.item")), item).unwrap();
}

#[test]
fn a_new_named_effect_reports_its_inferred_home() {
    let world = ImportWorld::new(Corrections::default());
    let path = world.options.source_dir.join("ItemBuffs.xml");
    let original = std::fs::read_to_string(&path).unwrap();
    let definition = include_str!("fixtures/effect-groups/AbjurationFocus.Buffs.xml")
        .replace("SchoolFocusNumber", "NewWillWard")
        .replace(
            "%b1 %i1 Focus %v1: %v1 %b1 bonus to the DC of %i1 spells.",
            "%b1 New Will Ward %v1: %v1 %b1 bonus to Will saves.",
        )
        .replace("SpellDC", "SaveBonus")
        .replace("Abjuration", "Will");
    std::fs::write(path, original.replace("</Buffs>", &format!("{definition}</Buffs>"))).unwrap();
    let item = include_str!("fixtures/effect-groups/AbjurationFocus.item")
        .replace("SchoolFocusNumber", "NewWillWard")
        .replace("Abjuration Focus Carrier", "New Ward Carrier")
        .replace("<Item>Abjuration</Item>", "")
        .replace("Insight", "Resistance");
    std::fs::create_dir_all(world.options.source_dir.join("Items")).unwrap();
    std::fs::write(world.options.source_dir.join("Items/NewWard.item"), item).unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    let home: String = world
        .database()
        .query_row(
            "SELECT t.name FROM effects e JOIN bonus_types t ON t.id=e.home_bonus_type_id WHERE e.name='New Will Ward'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(home, "Resistance");
    assert!(
        report.entries.iter().any(|entry| entry.category == "NEW EFFECT HOME"
            && entry.detail.contains("New Will Ward")
            && entry.detail.contains("Resistance")),
        "{report}"
    );
    assert!(report.to_string().contains("Named effect home types\n"));
}

#[test]
fn import_preserves_declared_stat_homes_and_reports_a_different_majority() {
    let world = abjuration_home_import();
    for index in 3..5 {
        write_abjuration_carrier(&world.options.source_dir, "Equipment", index);
    }
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    let actual: Option<String> = world.database().query_row("SELECT bt.name FROM effects e LEFT JOIN bonus_types bt ON bt.id=e.home_bonus_type_id WHERE e.name='Abjuration Focus'", [], |row| row.get(0)).unwrap();
    assert_eq!(actual, None);
    let entry = report
        .entries
        .iter()
        .find(|entry| entry.category == "STAT HOME MAJORITY" && entry.detail.contains("Abjuration Focus"))
        .unwrap_or_else(|| panic!("{report}"));
    assert!(entry.detail.contains("Equipment") && entry.detail.contains("none"), "{entry:?}");
    assert!(report.to_string().contains("Stat home types\n"));
}

#[test]
fn missing_seeded_stat_reports_its_majority_without_a_home_or_recreating_its_deleted_file() {
    for (stat, slug, expected) in
        [("Abjuration Focus", "abjuration-focus", "Equipment"), ("Divination Focus", "divination-focus", "none")]
    {
        let world = abjuration_home_import();
        world.edit(&format!("DELETE FROM items WHERE name LIKE 'Home %'; DELETE FROM effects WHERE name={stat:?}"));
        let carrier_path = world.options.source_dir.join("Items/HomeInsight3.item");
        let carrier = std::fs::read_to_string(&carrier_path).unwrap();
        std::fs::write(
            carrier_path,
            carrier.replace("<BonusType>Insight</BonusType>", "<BonusType>Equipment</BonusType>"),
        )
        .unwrap();
        let relative = format!("effects/stats/{slug}.toml");
        let path = world.options.data_dir.join(&relative);
        assert!(!path.exists());
        let upstream = world.scratch.path().join("upstream-home");
        let normalized = ddo_etl::import_ddobuilder::normalized_upstream_database(
            &world.options.source_dir,
            &world.database(),
            &DatasetVersion { upstream_sha: NEW_SHA.into(), built_at: "2026-10-28T00:00:00Z".into() },
            false,
            &upstream,
        )
        .unwrap();
        let home: Option<String> = normalized
            .db
            .query_row(
                "SELECT bt.name FROM effects e LEFT JOIN bonus_types bt ON bt.id=e.home_bonus_type_id WHERE e.name=?1",
                [stat],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(home, None);
        let declared = std::fs::read_to_string(upstream.join(&relative)).unwrap();
        assert!(!declared.contains("home_bonus_type"), "{declared}");
        let report = import_ddobuilder(&world.options).unwrap();
        assert!(!report.has_conflicts(), "{report}");
        assert!(
            report.entries.iter().any(|entry| entry.category == "NEW STAT HOME"
                && entry.detail.contains(stat)
                && entry.detail.contains(expected)),
            "{report}"
        );
        assert!(!path.exists());
        assert_eq!(
            world
                .database()
                .query_row("SELECT COUNT(*) FROM effects WHERE name=?1", [stat], |row| row.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

fn woeful_split_import() -> ImportWorld {
    woeful_split_import_with_sources(|_| {})
}

fn woeful_split_import_with_sources(add_augments: impl FnOnce(&Path)) -> ImportWorld {
    let correction = fixture::new_qualified_correction(
        "augment_bonus",
        "Woeful Shadow (Legendary)",
        "stat = 'Doublestrike, Doubleshot'\nbonus_type = 'Profane'\n",
        "lines",
        "[['Doublestrike', 'Doubleshot']]",
        "[\"Doubleshot\", \"Doublestrike\"]",
    );
    ImportWorld::with_sources(
        fixture::groupings(),
        fixture::corrections_from(&[("woeful-shadow.toml", &correction)]).unwrap(),
        WikiOverrides::default(),
        |source| {
            std::fs::create_dir_all(source.join("Augments")).unwrap();
            std::fs::create_dir_all(source.join("AugmentImages")).unwrap();
            std::fs::write(
                source.join("Augments/WoefulShadow.Augments.xml"),
                include_str!("fixtures/effect-groups/WoefulShadow.Augments.xml"),
            )
            .unwrap();
            std::fs::write(
                source.join("AugmentImages/Lamordia.png"),
                include_bytes!("fixtures/effect-groups/icons/Lamordia.png"),
            )
            .unwrap();
            add_augments(source);
        },
    )
}

fn woeful_lines(db: &Connection) -> Vec<(String, i64)> {
    db.prepare("SELECT e.name,l.value FROM augment_effects l JOIN effects e ON e.id=l.effect_id JOIN augments a ON a.id=l.augment_id JOIN bonus_types bt ON bt.id=l.bonus_type_id WHERE a.name='Woeful Shadow (Legendary)' AND bt.name='Profane' ORDER BY e.name").unwrap()
        .query_map([], |row| Ok((row.get(0)?,row.get(1)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap()
}

fn alias_import_world() -> ImportWorld {
    alias_import_world_with_sources(|_| {})
}

fn alias_import_world_with_sources(add_items: impl FnOnce(&Path)) -> ImportWorld {
    charisma_bonus_import_world("alias", "\"null\"", "\"Charisma Observance\"", add_items)
}

fn charisma_bonus_import_world(field: &str, from: &str, to: &str, add_items: impl FnOnce(&Path)) -> ImportWorld {
    let canonical = fixture::database(|root| {
        let path = root.join("effects/stat-groups/charisma-skills.toml");
        let original = std::fs::read_to_string(&path).unwrap();
        std::fs::write(path, format!("{original}\n[[alias]]\nname = 'Charisma Skills - Exceptional Bonus'\nverbose_name_template = 'Charisma Skills - %b1 Bonus +{{1}}'\n[[alias]]\nname = 'Charisma Observance'\nverbose_name_template = '%b1 Charisma Observance +{{1}}'\n")).unwrap();
    });
    let groupings = ddo_etl::groupings::Groupings::from_database(&canonical).unwrap();
    let item = include_str!("fixtures/effect-groups/CharismaSkills.item");
    let correction = fixture::new_qualified_correction(
        "item_bonus",
        "Pink and Green Ioun Stone (unsuppressed)",
        "stat = 'Charisma Skills'\nbonus_type = 'Exceptional'",
        field,
        from,
        to,
    );
    let corrections = fixture::corrections_from(&[("charisma.toml", &correction)]).unwrap();
    ImportWorld::with_sources(&groupings, corrections, WikiOverrides::default(), |source| {
        let path = source.join("ItemBuffs.xml");
        let original = std::fs::read_to_string(&path).unwrap();
        std::fs::write(
            path,
            original.replace(
                "</Buffs>",
                &format!("{}\n</Buffs>", include_str!("fixtures/effect-groups/CharismaSkills.Buffs.xml")),
            ),
        )
        .unwrap();
        std::fs::create_dir_all(source.join("Items")).unwrap();
        std::fs::write(
            source.join("Items/CharismaSkills.item"),
            item.replace(
                "<Type>Charisma Skills - Exceptional</Type>",
                "<Type>SkillBonusAbility</Type><Item>Charisma</Item>",
            ),
        )
        .unwrap();
        add_items(source);
    })
}

#[test]
fn an_upstream_alias_change_rebases_the_existing_alias_correction() {
    let world = alias_import_world();
    let item = include_str!("fixtures/effect-groups/CharismaSkills.item");
    std::fs::write(world.options.source_dir.join("Items/CharismaSkills.item"), item).unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(report.has_conflicts(), "{report}");
    let db = world.database();
    let chosen: String = db.query_row("SELECT l.alias FROM item_effects l JOIN items i ON i.id=l.item_id WHERE i.name='Pink and Green Ioun Stone (unsuppressed)'", [], |row| row.get(0)).unwrap();
    assert_eq!(chosen, "Charisma Observance");
    let upstream: String =
        db.query_row("SELECT from_value FROM corrections WHERE field='alias'", [], |row| row.get(0)).unwrap();
    assert_eq!(upstream, "\"Charisma Skills - Exceptional Bonus\"");
}

fn remove_alias_bonus(world: &ImportWorld) -> String {
    let path = world.options.source_dir.join("Items/CharismaSkills.item");
    let original = std::fs::read_to_string(&path).unwrap();
    let start = original.find("    <Buff>").unwrap();
    let end = start + original[start..].find("    </Buff>").unwrap() + "    </Buff>".len();
    std::fs::write(path, format!("{}{}", &original[..start], &original[end..])).unwrap();
    original
}

#[test]
fn an_alias_missing_target_repeats_without_a_report() {
    let mut world = alias_import_world();
    remove_alias_bonus(&world);
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(report.has_conflicts() && report.to_string().contains("no such bonus"), "{report}");
    assert_missing_targets_repeat_cleanly(&mut world.options);
}

#[test]
fn an_alias_missing_target_repeats_when_another_item_keeps_the_effect() {
    let mut world = alias_import_world_with_sources(|source| {
        let item = include_str!("fixtures/effect-groups/CharismaSkills.item")
            .replace("Pink and Green Ioun Stone (unsuppressed)", "Other Charisma Carrier");
        std::fs::write(source.join("Items/OtherCharismaCarrier.item"), item).unwrap();
    });
    remove_alias_bonus(&world);
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(report.has_conflicts() && report.to_string().contains("no such bonus"), "{report}");
    assert!(world.options.data_dir.join("effects/stat-groups/charisma-skills.toml").exists());
    assert_missing_targets_repeat_cleanly(&mut world.options);
}

fn charisma_bonus_type_import_world(keeps_other_bonus: bool) -> ImportWorld {
    charisma_bonus_import_world("bonus_type", "'Exceptional'", "'Insight'", |source| {
        if keeps_other_bonus {
            let path = source.join("Items/CharismaSkills.item");
            let item = std::fs::read_to_string(&path).unwrap();
            std::fs::write(path, item.replace("  </Item>", "    <Buff><Type>AbilityBonus</Type><Value1>1</Value1><BonusType>Enhancement</BonusType><Item>Strength</Item></Buff>\n  </Item>")).unwrap();
        }
    })
}

fn assert_bonus_type_missing_target_repeats(keeps_other_bonus: bool) {
    let mut world = charisma_bonus_type_import_world(keeps_other_bonus);
    remove_alias_bonus(&world);
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(
        report.entries.iter().any(|entry| entry.category == "CONFLICT"
            && entry.detail.contains(".bonus_type")
            && entry.detail.contains("new=\"no such bonus\"")),
        "{report}"
    );
    assert!(
        report.entries.iter().any(|entry| entry.category == "REMOVED" && entry.detail == "effects/charisma-skills"),
        "{report}"
    );
    assert_missing_targets_repeat_cleanly(&mut world.options);
}

#[test]
fn a_bonus_type_missing_target_repeats_when_its_owner_has_no_other_links() {
    assert_bonus_type_missing_target_repeats(false);
}

#[test]
fn a_bonus_type_missing_target_repeats_when_its_owner_keeps_another_link() {
    assert_bonus_type_missing_target_repeats(true);
}

#[test]
fn restoring_a_target_whose_correction_lost_its_value_stops_without_writing() {
    let mut failures = Vec::new();
    for (field, owner, current) in [
        ("alias", "Pink and Green Ioun Stone (unsuppressed)", "null"),
        ("bonus_type", "Pink and Green Ioun Stone (unsuppressed)", "\"Exceptional\""),
        ("label", "Sireth, Spear of the Sky", "\"crafting: attuned to heroism 1\""),
    ] {
        let mut world = match field {
            "alias" => alias_import_world(),
            "bonus_type" => charisma_bonus_type_import_world(false),
            _ => {
                let (scratch, mut options, _) = relabelled_socket_import(true);
                let source = scratch.path().join("changed");
                copy_tree(&options.source_dir, &source);
                options.source_dir = source;
                options.sha = Some(NEW_SHA.into());
                ImportWorld { scratch, options }
            }
        };
        let path = world.options.source_dir.join(if field == "label" {
            "Items/Sireth, Spear of the Sky.item"
        } else {
            "Items/CharismaSkills.item"
        });
        let original = if field == "label" {
            let original = std::fs::read_to_string(&path).unwrap();
            std::fs::write(
                &path,
                original.replace("<Name>Planar Conflux</Name>", "<Name>Upstream replacement option</Name>"),
            )
            .unwrap();
            original
        } else {
            remove_alias_bonus(&world)
        };
        let report = import_ddobuilder(&world.options).unwrap();
        assert!(report.has_conflicts(), "{field}: {report}");
        let missing = world.scratch.path().join("missing-target");
        copy_tree(&world.options.source_dir, &missing);
        std::fs::write(&path, original).unwrap();
        world.options.previous_dir = missing;
        world.options.previous_sha = world.options.sha.clone();
        world.options.sha = Some("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".into());
        let before = world.bytes();
        let error = import_ddobuilder(&world.options).unwrap_err();
        assert_eq!(world.bytes(), before, "{field}: failed restore wrote files");
        let message = format!("{error:#}");
        for expected in [
            owner,
            &format!(".{field}"),
            "Maetrim's files",
            &format!("current value: {current}"),
            "recorded it as missing",
            "delete the correction, import, then add it again if it is still wanted",
        ] {
            if !message.contains(expected) {
                failures.push(format!("{field}: missing {expected:?}: {message}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn an_effect_stat_removal_missing_target_repeats_without_a_report() {
    let correction = fixture::new_qualified_correction(
        "effect", "Lifeblood of the Undead Prince", "stat = 'Repair Amplification'", "remove",
        "{ target = 'Repair Amplification', bonus_type = 'Competence', amount_from = 1, scale = 1.0, rounding = 'down' }",
        "'null'",
    );
    let corrections = fixture::corrections_from(&[("effect-removal.toml", &correction)]).unwrap();
    let mut world = ImportWorld::with_sources(
        fixture::grouping_cases(),
        corrections,
        WikiOverrides::default(),
        fixture::write_diadem_item,
    );
    let path = world.options.source_dir.join("ItemBuffs.xml");
    let original = std::fs::read_to_string(&path).unwrap();
    let changed = original.replace("      <Type>RepairAmplification</Type>\n", "");
    assert_ne!(changed, original);
    std::fs::write(path, changed).unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(report.has_conflicts() && report.to_string().contains("no such bonus"), "{report}");
    assert_missing_targets_repeat_cleanly(&mut world.options);
}

#[test]
fn changing_the_upstream_buff_route_clears_its_alias_without_a_conflict() {
    let canonical = fixture::database(|root| {
        let path = root.join("effects/stat-groups/charisma-skills.toml");
        let original = std::fs::read_to_string(&path).unwrap();
        std::fs::write(path, format!("{original}\n[[alias]]\nname = 'Charisma Skills - Exceptional Bonus'\nverbose_name_template = 'Charisma Skills - %b1 Bonus +{{1}}'\n")).unwrap();
    });
    let groupings = ddo_etl::groupings::Groupings::from_database(&canonical).unwrap();
    let item = include_str!("fixtures/effect-groups/CharismaSkills.item");
    let world = ImportWorld::with_sources(&groupings, Corrections::default(), WikiOverrides::default(), |source| {
        let path = source.join("ItemBuffs.xml");
        let original = std::fs::read_to_string(&path).unwrap();
        std::fs::write(
            path,
            original.replace(
                "</Buffs>",
                &format!("{}\n</Buffs>", include_str!("fixtures/effect-groups/CharismaSkills.Buffs.xml")),
            ),
        )
        .unwrap();
        std::fs::create_dir_all(source.join("Items")).unwrap();
        std::fs::write(source.join("Items/CharismaSkills.item"), item).unwrap();
    });
    let link = |db: &Connection| -> (String, Option<String>, i64) {
        db.query_row("SELECT e.name,l.alias,l.value FROM item_effects l JOIN effects e ON e.id=l.effect_id JOIN items i ON i.id=l.item_id WHERE i.name='Pink and Green Ioun Stone (unsuppressed)'", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap()
    };
    assert_eq!(
        link(&world.database()),
        ("Charisma Skills".into(), Some("Charisma Skills - Exceptional Bonus".into()), 2)
    );
    std::fs::write(
        world.options.source_dir.join("Items/CharismaSkills.item"),
        item.replace(
            "<Type>Charisma Skills - Exceptional</Type>",
            "<Type>SkillBonusAbility</Type><Item>Charisma</Item>",
        ),
    )
    .unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!(link(&world.database()), ("Charisma Skills".into(), None, 2));
}

#[test]
fn alias_choices_and_declarations_survive_import_and_round_trip() {
    let canonical = fixture::database(|root| {
        let path = root.join("effects/stats/spot.toml");
        let original = std::fs::read_to_string(&path).unwrap();
        std::fs::write(
            path,
            format!("{original}\n[[alias]]\nname = 'Awareness'\nverbose_name_template = '%b1 Awareness +{{1}}'\n"),
        )
        .unwrap();
    });
    let groupings = ddo_etl::groupings::Groupings::from_database(&canonical).unwrap();
    let alias = fixture::new_qualified_correction(
        "item_bonus",
        "Legendary Keylock Ring",
        "stat = 'Spot'\nbonus_type = 'Competence'",
        "alias",
        "\"null\"",
        "\"Awareness\"",
    );
    let corrections = fixture::corrections_from(&[("alias.toml", &alias)]).unwrap();
    let world = ImportWorld::with_sources(&groupings, corrections, WikiOverrides::default(), |source| {
        fixture::write_search_and_spot_item(source, 20, true);
    });
    fixture::write_search_and_spot_item(&world.options.source_dir, 21, true);
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    let db = world.database();
    let chosen: (String, i64) = db.query_row("SELECT a.name,l.value FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effect_aliases a ON a.effect_id=l.effect_id AND a.name=l.alias WHERE i.name='Legendary Keylock Ring'", [], |row| Ok((row.get(0)?,row.get(1)?))).unwrap();
    assert_eq!(chosen, ("Awareness".into(), 21));
    let before = tree::data_file_bytes(&world.options.data_dir).unwrap();
    tree::export_database(&db, &world.options.data_dir).unwrap();
    assert_eq!(before, tree::data_file_bytes(&world.options.data_dir).unwrap());
}

#[test]
fn a_riposte_rank_change_updates_the_link_without_conflict() {
    let canonical = fixture::riposte_rank_database();
    let index = ddo_etl::groupings::Groupings::from_database(&canonical).unwrap();
    let world = ImportWorld::with_sources(&index, Corrections::default(), WikiOverrides::default(), |source| {
        let captured = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles");
        std::fs::create_dir_all(source.join("Items")).unwrap();
        std::fs::copy(
            captured.join("Items/Epic Ethereal Bracers.item"),
            source.join("Items/Epic Ethereal Bracers.item"),
        )
        .unwrap();
        std::fs::copy(captured.join("ItemBuffs.xml"), source.join("ItemBuffs.xml")).unwrap();
        copy_tree(&captured.join("ItemImages"), &source.join("ItemImages"));
        copy_tree(&captured.join("AugmentImages"), &source.join("AugmentImages"));
        std::fs::create_dir_all(source.join("ItemImages/Bracers")).unwrap();
        std::fs::create_dir_all(source.join("AugmentImages")).unwrap();
        std::fs::write(
            source.join("ItemImages/Bracers/Bracers_1.png"),
            include_bytes!("fixtures/data/icons/items/Bracers_1.png"),
        )
        .unwrap();
        std::fs::write(
            source.join("AugmentImages/Heroism.png"),
            include_bytes!("fixtures/effect-groups/icons/Heroism.png"),
        )
        .unwrap();
    });
    for (directory, rank) in [("data", "ix"), ("effect-groups/data", "vii")] {
        let name = format!("riposte-{rank}.toml");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(directory).join("effects").join(&name),
            world.options.data_dir.join("effects").join(name),
        )
        .unwrap();
    }
    let path = world.options.source_dir.join("Items/Epic Ethereal Bracers.item");
    let previous = std::fs::read_to_string(&path).unwrap();
    assert!(previous.contains("<Item>IX</Item>"));
    std::fs::write(path, previous.replace("<Item>IX</Item>", "<Item>VII</Item>")).unwrap();
    let rank_paths = ["riposte-iv.toml", "riposte-v.toml", "riposte-vii.toml", "riposte-ix.toml"];
    let before = rank_paths.map(|name| std::fs::read(world.options.data_dir.join("effects").join(name)).unwrap());
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert!(!report.to_string().lines().any(|line| line.contains("riposte-") && line.contains("citation")), "{report}");
    let after = rank_paths.map(|name| std::fs::read(world.options.data_dir.join("effects").join(name)).unwrap());
    assert_eq!(before, after);
    let db = world.database();
    let link: (String, Option<i64>, Option<i64>,i64) = db.query_row("SELECT e.name,l.value,l.value2,l.sort_order FROM item_effects l JOIN effects e ON e.id=l.effect_id JOIN items i ON i.id=l.item_id WHERE i.name='Epic Ethereal Bracers' AND e.name LIKE 'Riposte%'", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).unwrap();
    assert_eq!(link, ("Riposte VII".into(), None, None, 1));
    let bonuses = db.prepare("SELECT stat.name,b.amount FROM owner_bonuses b JOIN effects stat ON stat.id=b.stat_id JOIN items i ON i.id=b.owner_id JOIN effects e ON e.id=b.via_effect_id WHERE b.owner_kind='item' AND i.name='Epic Ethereal Bracers' AND e.name='Riposte VII' ORDER BY stat.name").unwrap().query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(bonuses, [("Armor Class".into(), 4), ("Saving Throws".into(), 3)]);
}

fn ship_kickers_replacement_world() -> ImportWorld {
    ship_kickers_effect_import_world("effect", "Riposte V", "\"Riposte\"", "\"Riposte V\"")
}

fn ship_kickers_effect_import_world(field: &str, effect: &str, from: &str, to: &str) -> ImportWorld {
    let index = ddo_etl::groupings::Groupings::from_database(&fixture::riposte_rank_database()).unwrap();
    let correction = fixture::new_qualified_correction(
        "item_effect",
        "Ship Kickers",
        &format!("effect = {effect:?}"),
        field,
        from,
        to,
    );
    let corrections = fixture::corrections_from(&[("ship-kickers.toml", &correction)]).unwrap();
    ImportWorld::with_sources(&index, corrections, WikiOverrides::default(), |source| {
        fixture::write_ship_kickers(source);
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles/ItemBuffs.xml"),
            source.join("ItemBuffs.xml"),
        )
        .unwrap();
    })
}

#[test]
fn an_effect_replacement_import_reports_a_removed_link_and_keeps_the_correction() {
    let mut world = ship_kickers_replacement_world();
    let path = world.options.source_dir.join("Items/ShipKickers.item");
    let previous = std::fs::read_to_string(&path).unwrap();
    let start = previous.find("    <Buff>\n      <Type>Riposte</Type>").unwrap();
    let end = start + previous[start..].find("    </Buff>").unwrap() + "    </Buff>".len();
    std::fs::write(path, format!("{}{}", &previous[..start], &previous[end..])).unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    let destination_exists = world.options.data_dir.join("effects/riposte-v.toml").exists();
    let destination_removed =
        report.entries.iter().any(|entry| entry.category == "REMOVED" && entry.detail == "effects/riposte-v");
    assert!(destination_exists && !destination_removed, "{report}");
    assert!(report.has_conflicts() && report.to_string().contains("no such bonus"), "{report}");
    let db = world.database();
    let retained: String = db
        .query_row("SELECT from_value FROM corrections WHERE name='Ship Kickers' AND field='effect'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(retained, "\"no such bonus\"");
    let links: i64 = db.query_row("SELECT COUNT(*) FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effects e ON e.id=l.effect_id WHERE i.name='Ship Kickers' AND e.name LIKE 'Riposte%'", [], |row| row.get(0)).unwrap();
    assert_eq!(links, 0);
    assert_missing_targets_repeat_cleanly(&mut world.options);
}

#[test]
fn an_effect_replacement_conflicts_when_upstream_adds_the_destination_beside_the_source() {
    assert_replacement_conflicts_persist("<Buff><Type>Riposte</Type><Item>V</Item></Buff>");
}

#[test]
fn an_effect_replacement_with_repeated_sources_conflicts_on_consecutive_imports() {
    assert_replacement_conflicts_persist(
        "<Buff><Type>Riposte</Type><Value1>4</Value1><Value2>3</Value2><BonusType>Insightful</BonusType></Buff>",
    );
}

fn assert_replacement_conflicts_persist(additional_buff: &str) {
    let mut world = ship_kickers_replacement_world();
    let path = world.options.source_dir.join("Items/ShipKickers.item");
    let previous = std::fs::read_to_string(&path).unwrap();
    std::fs::write(path, previous.replace("  </Item>", &format!("    {additional_buff}\n  </Item>"))).unwrap();
    for pass in 0..2 {
        let report = import_ddobuilder(&world.options).unwrap();
        let text = report.to_string();
        assert!(report.has_conflicts(), "pass {pass}: {report}");
        assert!(text.contains("Riposte") && text.contains("Riposte V"), "{report}");
        assert!(!text.contains("no such bonus"), "the source exists: {report}");
        let db = world.database();
        let retained: String = db
            .query_row("SELECT from_value FROM corrections WHERE name='Ship Kickers' AND field='effect'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(retained, "\"Riposte\"", "retain the actual source until the collision is settled");
        let names = db.prepare("SELECT e.name FROM item_effects l JOIN effects e ON e.id=l.effect_id JOIN items i ON i.id=l.item_id WHERE i.name='Ship Kickers' AND e.name LIKE 'Riposte%' ORDER BY e.name").unwrap().query_map([], |row| row.get::<_, String>(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        assert_eq!(names, ["Riposte V"], "an ambiguous replacement keeps our corrected link");
        world.options.previous_dir = world.options.source_dir.clone();
        world.options.previous_sha = world.options.sha.clone();
        world.options.sha = Some("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".into());
    }
}

fn will_save_value_import_world() -> ImportWorld {
    let correction = fixture::new_qualified_correction(
        "augment_bonus",
        "+1 Exceptional Will Save",
        "stat = 'Will Save'\nbonus_type = 'Exceptional'",
        "value",
        "1",
        "3",
    );
    let corrections = fixture::corrections_from(&[("will-twins.toml", &correction)]).unwrap();
    ImportWorld::with_sources(fixture::groupings(), corrections, WikiOverrides::default(), |source| {
        std::fs::create_dir_all(source.join("Augments")).unwrap();
        let twins = include_str!("fixtures/effect-groups/GreensteelWillTwins.Augments.xml")
            .replace("      <Icon>ShardOfGreatPower</Icon>\n", "");
        std::fs::write(source.join("Augments/WillTwins.Augments.xml"), twins).unwrap();
    })
}

#[test]
fn a_bonus_correction_conflicts_atomically_when_same_named_twins_diverge() {
    let world = will_save_value_import_world();
    let path = world.options.source_dir.join("Augments/WillTwins.Augments.xml");
    let source = std::fs::read_to_string(&path).unwrap();
    std::fs::write(path, source.replacen("<Amount size=\"1\">1</Amount>", "<Amount size=\"1\">2</Amount>", 1)).unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(report.has_conflicts(), "{report}");
    let db = world.database();
    let corrections: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM corrections WHERE name='+1 Exceptional Will Save' AND field='value'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(corrections, 1);
    let values = db.prepare("SELECT l.value FROM augment_effects l JOIN augments a ON a.id=l.augment_id WHERE a.name='+1 Exceptional Will Save' ORDER BY a.id").unwrap().query_map([], |row| row.get::<_, i64>(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(values, [3, 3], "a conflict keeps both of our corrected twins");
}

#[test]
fn removing_a_bonus_under_a_value_correction_stops_without_writing() {
    let world = will_save_value_import_world();
    let path = world.options.source_dir.join("Augments/WillTwins.Augments.xml");
    let original = std::fs::read_to_string(&path).unwrap();
    let removed = original.replace("<Item>Will</Item>", "<Item>Fortitude</Item>");
    assert_ne!(removed, original);
    std::fs::write(path, removed).unwrap();
    let before = world.bytes();
    let error = import_ddobuilder(&world.options).unwrap_err();
    assert_eq!(world.bytes(), before);
    let message = format!("{error:#}");
    assert!(
        message.contains("augment_bonus \"+1 Exceptional Will Save\".value") && message.contains("no such bonus"),
        "{message}"
    );
}

#[test]
fn removing_a_bonus_under_a_remove_correction_stops_without_writing() {
    let (original, correction) = fixture::bonus_correction_twins("remove", false);
    let corrections = fixture::corrections_from(&[("woeful-twins.toml", &correction)]).unwrap();
    let world = ImportWorld::with_sources(fixture::groupings(), corrections, WikiOverrides::default(), |source| {
        std::fs::create_dir_all(source.join("Augments")).unwrap();
        std::fs::write(source.join("Augments/WoefulTwins.Augments.xml"), &original).unwrap();
    });
    let removed = original.replace("      <Type>Doubleshot</Type>\n", "");
    assert_ne!(removed, original);
    std::fs::write(world.options.source_dir.join("Augments/WoefulTwins.Augments.xml"), removed).unwrap();
    let before = world.bytes();
    let error = import_ddobuilder(&world.options).unwrap_err();
    assert_eq!(world.bytes(), before);
    let message = format!("{error:#}");
    assert!(
        message.contains("augment_bonus \"Woeful Shadow (Legendary)\".remove")
            && message.contains("a bonus remove takes"),
        "{message}"
    );
}

#[test]
fn removing_an_item_effect_under_a_bonus_type_correction_stops_without_writing() {
    assert_removed_item_effect_stops_without_writing("bonus_type", "\"Insight\"", "\"Quality\"");
}

#[test]
fn removing_an_item_effect_under_a_remove_correction_stops_without_writing() {
    assert_removed_item_effect_stops_without_writing("remove", "0", "1");
}

fn assert_removed_item_effect_stops_without_writing(field: &str, from: &str, to: &str) {
    let world = ship_kickers_effect_import_world(field, "Riposte", from, to);
    let path = world.options.source_dir.join("Items/ShipKickers.item");
    let original = std::fs::read_to_string(&path).unwrap();
    let start = original.find("    <Buff>\n      <Type>Riposte</Type>").unwrap();
    let end = start + original[start..].find("    </Buff>").unwrap() + "    </Buff>".len();
    std::fs::write(path, format!("{}{}", &original[..start], &original[end..])).unwrap();
    let before = world.bytes();
    let error = import_ddobuilder(&world.options).unwrap_err();
    assert_eq!(world.bytes(), before);
    let message = format!("{error:#}");
    assert!(message.contains("effect \"Riposte\""), "{message}");
}

fn assert_structural_twin_conflicts_persist(case: &str) {
    let (original, correction) = fixture::bonus_correction_twins(case, false);
    let corrections = fixture::corrections_from(&[("woeful-twins.toml", &correction)]).unwrap();
    let mut world = ImportWorld::with_sources(fixture::groupings(), corrections, WikiOverrides::default(), |source| {
        std::fs::create_dir_all(source.join("Augments")).unwrap();
        std::fs::write(source.join("Augments/WoefulTwins.Augments.xml"), original).unwrap();
    });
    let links = |db: &Connection| {
        db.prepare("SELECT a.slug,e.name,t.name,l.value,l.value2 FROM augment_effects l JOIN augments a ON a.id=l.augment_id JOIN effects e ON e.id=l.effect_id LEFT JOIN bonus_types t ON t.id=l.bonus_type_id WHERE a.name='Woeful Shadow (Legendary)' ORDER BY a.slug,l.sort_order").unwrap().query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,Option<String>>(2)?,row.get::<_,Option<i64>>(3)?,row.get::<_,Option<i64>>(4)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
    };
    let expected = links(&world.database());
    let (changed, _) = fixture::bonus_correction_twins(case, true);
    std::fs::write(world.options.source_dir.join("Augments/WoefulTwins.Augments.xml"), changed).unwrap();
    for pass in 0..2 {
        let report = import_ddobuilder(&world.options).unwrap();
        assert!(report.has_conflicts(), "{case}, pass {pass}: {report}");
        let db = world.database();
        assert_eq!(links(&db), expected, "{case}, pass {pass}: keep both of our corrected twins");
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM corrections WHERE name='Woeful Shadow (Legendary)'", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        world.options.previous_dir = world.options.source_dir.clone();
        world.options.previous_sha = world.options.sha.clone();
        world.options.sha = Some("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".into());
    }
}

#[test]
fn divergent_bonus_additions_conflict_on_consecutive_imports() {
    assert_structural_twin_conflicts_persist("add");
}

#[test]
fn ambiguous_bonus_additions_conflict_on_consecutive_imports() {
    assert_structural_twin_conflicts_persist("ambiguous_add");
}

#[test]
fn ambiguous_line_merges_conflict_on_consecutive_imports() {
    assert_structural_twin_conflicts_persist("ambiguous_merge");
}

#[test]
fn divergent_bonus_removals_conflict_on_consecutive_imports() {
    assert_structural_twin_conflicts_persist("remove");
}

#[test]
fn divergent_line_splits_conflict_on_consecutive_imports() {
    assert_structural_twin_conflicts_persist("split");
}

#[test]
fn divergent_writer_holds_conflict_on_consecutive_imports() {
    assert_structural_twin_conflicts_persist("held_split");
}

#[test]
fn held_line_splits_keep_their_cause_on_consecutive_imports() {
    let (original, correction) = fixture::bonus_correction_twins("held_split", false);
    let corrections = fixture::corrections_from(&[("woeful-held-split.toml", &correction)]).unwrap();
    let mut world = ImportWorld::with_sources(fixture::groupings(), corrections, WikiOverrides::default(), |source| {
        std::fs::create_dir_all(source.join("Augments")).unwrap();
        std::fs::write(source.join("Augments/WoefulTwins.Augments.xml"), &original).unwrap();
    });
    std::fs::write(
        world.options.source_dir.join("Augments/WoefulTwins.Augments.xml"),
        original.replace("<Type>Doubleshot</Type>", "<Type>Doubleshot</Type><Type>Weapon_AttackAndDamage</Type>"),
    )
    .unwrap();
    for pass in 0..2 {
        let report = import_ddobuilder(&world.options).unwrap();
        let rendered = report.to_string();
        assert!(report.has_conflicts(), "pass {pass}: {rendered}");
        let conflict =
            rendered.lines().find(|line| line.contains("CONFLICT augment_bonus") && line.contains("].lines ")).unwrap();
        assert!(conflict.contains("held: effect types without derived stats: Weapon_AttackAndDamage"), "{conflict}");
        assert!(!conflict.contains("no such bonus"), "{conflict}");
        let db = world.database();
        let upstream: String = db
            .query_row(
                "SELECT from_value FROM corrections WHERE name='Woeful Shadow (Legendary)' AND field='lines'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&upstream).unwrap(),
            serde_json::json!([["Doublestrike", "Doubleshot"]])
        );
        assert_eq!(db.query_row(
            "SELECT COUNT(*) FROM augment_effects l JOIN augments a ON a.id=l.augment_id JOIN effects e ON e.id=l.effect_id WHERE a.name='Woeful Shadow (Legendary)' AND e.name IN ('Doublestrike','Doubleshot')", [], |row| row.get::<_, i64>(0),
        ).unwrap(), 4);
        world.options.previous_dir = world.options.source_dir.clone();
        world.options.previous_sha = world.options.sha.clone();
        world.options.sha = Some("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".into());
    }
}

#[test]
fn divergent_grouping_removals_conflict_on_consecutive_imports() {
    assert_structural_twin_conflicts_persist("group_remove");
}

#[test]
fn ambiguous_bonus_removals_conflict_on_consecutive_imports() {
    assert_structural_twin_conflicts_persist("ambiguous_remove");
}

#[test]
fn divergent_line_merges_conflict_on_consecutive_imports() {
    assert_structural_twin_conflicts_persist("merge");
}

#[test]
fn an_effect_replacement_resolves_when_upstream_names_the_destination() {
    let world = ship_kickers_replacement_world();
    let path = world.options.source_dir.join("Items/ShipKickers.item");
    let previous = std::fs::read_to_string(&path).unwrap();
    assert!(previous.contains("<Type>Riposte</Type>"));
    std::fs::write(path, previous.replace("<Type>Riposte</Type>", "<Type>Riposte</Type><Item>V</Item>")).unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts() && report.to_string().contains("RESOLVED CORRECTION"), "{report}");
    let db = world.database();
    let correction: (String, String) = db
        .query_row(
            "SELECT from_value,to_value FROM corrections WHERE name=?1 AND field=?2",
            ["Ship Kickers", "effect"],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(correction, ("\"Riposte\"".into(), "\"Riposte V\"".into()));
    let link: (String, Option<i64>, Option<i64>) = db.query_row("SELECT e.name,l.value,l.value2 FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effects e ON e.id=l.effect_id WHERE i.name=?1 AND e.name=?2", ["Ship Kickers","Riposte V"], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    assert_eq!(link, ("Riposte V".into(), None, None));
}

#[test]
fn a_merge_import_accepts_a_changed_amount_without_conflict() {
    let corrections = fixture::corrections_from(&[("keylock-merge.toml", &fixture::search_and_spot_merge())]).unwrap();
    let world = ImportWorld::with_sources(fixture::grouping_cases(), corrections, WikiOverrides::default(), |source| {
        fixture::write_search_and_spot_item(source, 20, true);
    });
    fixture::write_search_and_spot_item(&world.options.source_dir, 21, true);
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    let db = world.database();
    assert_eq!(fixture::search_and_spot_lines(&db), [("Search and Spot".into(), Some(21))]);
    let upstream: String = db
        .query_row(
            "SELECT from_value FROM corrections WHERE name='Legendary Keylock Ring' AND field='lines'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&upstream).unwrap(),
        serde_json::json!([["Search"], ["Spot"]])
    );
}

#[test]
fn a_data_build_rejects_an_add_governed_by_the_same_owners_split() {
    let notes = fixture::new_qualified_correction(
        "augment_bonus",
        "+2 Melee/Ranged and Spell Power",
        "stat = 'Melee Power, Ranged Power'\nbonus_type = 'Reaper'",
        "lines",
        "[['Melee Power', 'Ranged Power']]",
        "['Melee Power', 'Ranged Power']",
    );
    let corrections = fixture::corrections_from(&[("reaper-necklace.toml", &notes)]).unwrap();
    let world = ImportWorld::with_sources(fixture::grouping_cases(), corrections, WikiOverrides::default(), |source| {
        fixture::write_reaper_augment(source, None)
    });
    let file = world.options.data_dir.join("packs/shared/augments/reaper/2-melee-ranged-and-spell-power.toml");
    let original = std::fs::read_to_string(&file).unwrap();
    let note = "\n[[ddobuilder_correction]]\nkind = 'augment_bonus'\nfield = 'add'\nqualifier = 'Melee Power / Reaper'\nreason = 'The cited bonus is absent upstream.'\nurl = 'https://ddowiki.com/page/Reaper_Forge'\nread = '2026-10-29'\n";
    std::fs::write(&file, format!("{original}{note}")).unwrap();
    let error = ddo_etl::tree::build_from_data(&world.options.data_dir, &mut Connection::open_in_memory().unwrap())
        .unwrap_err();
    let message = format!("{error:#}");
    assert!(
        message.contains("2-melee-ranged-and-spell-power.toml") && message.contains("add") && message.contains("split"),
        "{message}"
    );
    std::fs::write(
        &file,
        format!("{original}{}", note.replace("Melee Power / Reaper", "Universal Spell Power / Reaper")),
    )
    .unwrap();
    ddo_etl::tree::build_from_data(&world.options.data_dir, &mut Connection::open_in_memory().unwrap()).unwrap();
}

#[test]
fn added_merge_members_resolve_when_upstream_supplies_them() {
    let notes = fixture::reaper_additions() + &fixture::reaper_merge();
    let corrections = fixture::corrections_from(&[("reaper-necklace.toml", &notes)]).unwrap();
    let world = ImportWorld::with_sources(fixture::grouping_cases(), corrections, WikiOverrides::default(), |source| {
        fixture::write_reaper_augment(source, None);
    });
    let before = fixture::reaper_bonuses(&world.database());
    fixture::write_reaper_augment(&world.options.source_dir, Some((2, 2)));
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!(
        report.to_string().matches("RESOLVED CORRECTION augment_bonus \"+2 Melee/Ranged and Spell Power\"").count(),
        2,
        "{report}"
    );
    let db = world.database();
    assert_eq!(fixture::reaper_lines(&db), [("Reaper Necklace Enchantment".into(), Some(2))]);
    assert_eq!(fixture::reaper_bonuses(&db), before);
}

#[test]
fn an_added_merge_members_changed_value_conflicts_without_duplicating_it() {
    let notes = fixture::reaper_additions() + &fixture::reaper_merge();
    let corrections = fixture::corrections_from(&[("reaper-necklace.toml", &notes)]).unwrap();
    let world = ImportWorld::with_sources(fixture::grouping_cases(), corrections, WikiOverrides::default(), |source| {
        fixture::write_reaper_augment(source, None);
    });
    fixture::write_reaper_augment(&world.options.source_dir, Some((3, 2)));
    let report = import_ddobuilder(&world.options).unwrap();
    let text = report.to_string();
    assert!(
        report.has_conflicts() && text.contains("Physical Resistance Rating") && text.contains("no such bonus"),
        "{text}"
    );
    let bonuses = fixture::reaper_bonuses(&world.database());
    assert_eq!(bonuses.len(), 5, "{bonuses:?}");
    assert_eq!(bonuses.iter().find(|(name, _, _)| name == "Physical Resistance Rating").unwrap().2, 3);
}

#[test]
fn an_unaccounted_merge_partition_reports_its_file_and_missing_bonus() {
    let notes = fixture::reaper_additions() + &fixture::reaper_merge();
    let corrections = fixture::corrections_from(&[("reaper-necklace.toml", &notes)]).unwrap();
    let world = ImportWorld::with_sources(fixture::grouping_cases(), corrections, WikiOverrides::default(), |source| {
        fixture::write_reaper_augment(source, None)
    });
    let path = world.options.source_dir.join("Augments/Reaper.Augments.xml");
    let xml = std::fs::read_to_string(&path).unwrap();
    std::fs::write(path, xml.replace("<Type>SpellPower</Type>", "<Type>PRR</Type>")).unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    let text = report.to_string();
    assert!(
        report.has_conflicts()
            && text.contains("2-melee-ranged-and-spell-power.toml")
            && text.contains("no such bonus"),
        "{text}"
    );
    assert_eq!(fixture::reaper_bonuses(&world.database()).len(), 4);
}

#[test]
fn a_two_slot_merge_import_updates_only_the_changed_slot() {
    let corrections = fixture::corrections_from(&[("swiftness.toml", &fixture::swiftness_merge())]).unwrap();
    let world = ImportWorld::with_sources(fixture::grouping_cases(), corrections, WikiOverrides::default(), |source| {
        fixture::write_swiftness_augment(source, 30)
    });
    assert_eq!(fixture::swiftness_links(&world.database()), [("Speed".into(), Some(30), Some(15))]);
    fixture::write_swiftness_augment(&world.options.source_dir, 25);
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!(fixture::swiftness_links(&world.database()), [("Speed".into(), Some(25), Some(15))]);
    let path = world.options.data_dir.join("packs/shared/augments/topaz/topaz-of-swiftness.toml");
    let original = std::fs::read_to_string(&path).unwrap();
    assert!(original.contains("value2 = 15"));
    std::fs::write(&path, original.replace("value2 = 15\n", "")).unwrap();
    let error = tree::build_from_data(&world.options.data_dir, &mut Connection::open_in_memory().unwrap()).unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("topaz-of-swiftness.toml") && message.contains(".lines"), "{message}");
}

#[test]
fn socket_option_bonus_types_import_from_the_authored_carrier_notes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let mut authored = Connection::open_in_memory().unwrap();
    tree::build_from_data(&root, &mut authored).unwrap();
    let mut corrections = Corrections::from_database(&authored).unwrap();
    corrections.entries.retain(|note| note.name.contains("bracers-of-wind") || note.name == "Epic Bracers of Wind");
    assert_eq!(corrections.entries.len(), 4);
    let wiki = WikiOverrides {
        per_item_slot_types: WikiOverrides::from_database(&authored).unwrap().per_item_slot_types,
        ..WikiOverrides::default()
    };
    let mut world = ImportWorld::with_sources(fixture::groupings(), corrections, wiki, |source| {
        std::fs::create_dir_all(source.join("Items")).unwrap();
        std::fs::write(
            source.join("Items/Epic Bracers of Wind.item"),
            include_str!("fixtures/untyped_socket_options/Epic Bracers of Wind.item"),
        )
        .unwrap();
        let path = source.join("ItemBuffs.xml");
        let existing = ddo_etl::xml::item_buffs::parse(&path).unwrap();
        let mut buffs = std::fs::read_to_string(&path).unwrap();
        for definition in include_str!("fixtures/DataFiles/ItemBuffs.xml").split("<Buff>").skip(1) {
            let kind = definition.split_once("<Type>").unwrap().1.split_once("</Type>").unwrap().0;
            if !existing.contains_key(kind) {
                buffs = buffs.replace("</Buffs>", &format!("<Buff>{definition}</Buffs>"));
            }
        }
        std::fs::write(path, buffs).unwrap();
        for (folder, kind, name) in
            [("ItemImages", "items", "Bracers_10.png"), ("AugmentImages", "augments", "Upgrade.png")]
        {
            std::fs::create_dir_all(source.join(folder)).unwrap();
            std::fs::copy(root.join("icons").join(kind).join(name), source.join(folder).join(name)).unwrap();
        }
    });
    world.options.source_dir = world.options.previous_dir.clone();
    world.options.sha = world.options.previous_sha.clone();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    let db = world.database();
    let paths = tree::DataFilePaths::from_database(&db).unwrap();
    for (tier, amount) in [(2, 17), (3, 18)] {
        let (id, kind): (i64, String) = db.query_row("SELECT s.id,b.name FROM socket_option_sets s JOIN socket_options o ON o.set_id=s.id JOIN socket_option_effects e ON e.option_id=o.id JOIN effects stat ON stat.id=e.effect_id JOIN bonus_types b ON b.id=e.bonus_type_id WHERE s.slug=?1 AND stat.name='Electric Spell Lore' AND e.value=?2", rusqlite::params![format!("upgrade-tier-{tier}-vaults-of-the-artificers-epic-bracers-of-wind"),amount], |row| Ok((row.get(0)?,row.get(1)?))).unwrap();
        assert_eq!(kind, "Equipment");
        let file = std::fs::read_to_string(world.options.data_dir.join(paths.path("socket_option_sets", id).unwrap()))
            .unwrap();
        assert!(file.contains("kind = \"socket_option_bonus\"") && file.contains("field = \"bonus_type\""), "{file}");
    }
}

#[test]
fn renamed_item_carriers_keep_their_set_identity_through_two_imports() {
    let original = "Epic Treasure Hunter's Spyglass (legacy) (level 20)";
    let rename = fixture::new_correction("item", original, "name", &format!("{original:?}"), "\"Reviewed Spyglass\"")
        .cited("Exercise carrier identity through an item rename.", "https://ddowiki.com/page/Test", "2026-11-10");
    let corrections = fixture::corrections_from(&[("rename.toml", &rename)]).unwrap();
    let mut world = ImportWorld::with_sources(
        fixture::grouping_cases(),
        corrections,
        WikiOverrides::default(),
        fixture::write_spyglass_options,
    );
    let identities = |db: &Connection| {
        db.prepare("SELECT s.slug,s.name,s.variant FROM socket_option_sets s JOIN item_augment_slots socket ON socket.option_set_id=s.id JOIN items i ON i.id=socket.item_id WHERE i.name='Reviewed Spyglass' ORDER BY s.slug").unwrap()
            .query_map([],|row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,Option<String>>(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
    };
    let expected = identities(&world.database());
    assert!(!expected.is_empty());
    let source = world.options.source_dir.join("Items/SpyglassOptions.item");
    let xml = std::fs::read_to_string(&source).unwrap();
    let changed = xml.replace("Spot +20: Passive:", "Updated spot description. Spot +20: Passive:");
    assert_ne!(changed, xml);
    std::fs::write(&source, changed).unwrap();
    for _ in 0..2 {
        let report = import_ddobuilder(&world.options).unwrap();
        assert!(!report.has_conflicts(), "{report}");
        let db = world.database();
        assert_eq!(identities(&db), expected);
        let changed_options: i64 = db.query_row("SELECT COUNT(*) FROM socket_options o JOIN socket_option_sets s ON s.id=o.set_id JOIN item_augment_slots socket ON socket.option_set_id=s.id JOIN items i ON i.id=socket.item_id WHERE i.name='Reviewed Spyglass' AND o.description LIKE 'Updated spot description.%'", [], |row| row.get(0)).unwrap();
        assert_eq!(changed_options, 1);
        world.options.previous_dir = world.options.source_dir.clone();
        world.options.previous_sha = world.options.sha.clone();
    }
}

#[test]
fn sets_keep_their_identity_through_corrections_and_reimports() {
    let mut notes = fixture::rank_option_merge("Riposte VII");
    notes.0.clear();
    for level in [26, 27] {
        let mut correction = fixture::rank_option_merge("Riposte VII").0.remove(0);
        correction.name = format!("crafting-variant-shield-of-morning-level-{level}");
        notes.0.push(correction);
    }
    let corrections = fixture::corrections_from(&[("shield-of-morning.toml", &notes)]).unwrap();
    let mut world =
        ImportWorld::with_sources(fixture::grouping_cases(), corrections, WikiOverrides::default(), |source| {
            let captured = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/effect-groups/ShieldOfMorning");
            std::fs::create_dir_all(source.join("Items")).unwrap();
            for level in [26, 27, 28] {
                let name = format!("Shield of Morning (Level {level}).item");
                std::fs::copy(captured.join(&name), source.join("Items").join(name)).unwrap();
            }
            for (folder, name) in [("ItemImages", "Shield_13.png"), ("AugmentImages", "Variant.png")] {
                std::fs::create_dir_all(source.join(folder)).unwrap();
                std::fs::copy(captured.join(name), source.join(folder).join(name)).unwrap();
            }
            let path = source.join("ItemBuffs.xml");
            let existing = ddo_etl::xml::item_buffs::parse(&path).unwrap();
            let mut buffs = std::fs::read_to_string(&path).unwrap();
            for definition in
                include_str!("fixtures/effect-groups/ShieldOfMorning/ItemBuffs.xml").split("<Buff>").skip(1)
            {
                let kind = definition.split_once("<Type>").unwrap().1.split_once("</Type>").unwrap().0;
                if !existing.contains_key(kind) {
                    buffs = buffs.replace("</Buffs>", &format!("<Buff>{definition}</Buffs>"));
                }
            }
            std::fs::write(path, buffs).unwrap();
        });
    world.options.source_dir = world.options.previous_dir.clone();
    world.options.sha = world.options.previous_sha.clone();
    let identities = |db: &Connection| {
        db.prepare("SELECT slug,variant FROM socket_option_sets WHERE name LIKE 'Shield of Morning%' ORDER BY slug")
            .unwrap()
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    let expected = [26, 27].map(|level| (format!("crafting-variant-shield-of-morning-level-{level}"), None));
    assert_eq!(identities(&world.database()), expected);
    let before = world.bytes();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!(world.bytes(), before);
    let db = world.database();
    let id: i64 =
        db.query_row("SELECT id FROM socket_option_sets WHERE slug=?1", [&expected[0].0], |row| row.get(0)).unwrap();
    let path = world
        .options
        .data_dir
        .join(tree::DataFilePaths::from_database(&db).unwrap().path("socket_option_sets", id).unwrap());
    let mut set: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let option = set["option"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|option| option["name"].as_str() == Some("Riposte VII"))
        .unwrap();
    let upstream = option["description"].clone();
    option["description"] = "Reviewed Riposte VII description.".into();
    let mut note: toml::Value = toml::from_str("kind='socket_option'\nfield='description'\nqualifier='option \"Riposte VII\"'\nreason='Check stable ownership through a new correction.'\nurl='https://ddowiki.com/page/Item:Shield_of_Morning_(level_26)'\nread='2026-11-08'\n").unwrap();
    note.as_table_mut().unwrap().insert("upstream".into(), upstream);
    set["ddobuilder_correction"].as_array_mut().unwrap().push(note);
    std::fs::write(path, toml::to_string(&set).unwrap()).unwrap();
    assert_eq!(identities(&world.database()), expected);
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!(identities(&world.database()), expected);

    let db = world.database();
    let paths = tree::DataFilePaths::from_database(&db).unwrap();
    let shared_id: i64 =
        db.query_row("SELECT id FROM socket_option_sets WHERE slug=?1", [&expected[0].0], |row| row.get(0)).unwrap();
    let shared_path = world.options.data_dir.join(paths.path("socket_option_sets", shared_id).unwrap());
    let mut separate: toml::Value = toml::from_str(&std::fs::read_to_string(&shared_path).unwrap()).unwrap();
    separate["name"] = "Shield of Morning (Level 28)".into();
    let option = separate["option"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|option| option["name"].as_str() == Some("Riposte VII"))
        .unwrap();
    option["description"] = "A separate reviewed description.".into();
    std::fs::write(shared_path.with_file_name("shield-of-morning-level-28.toml"), toml::to_string(&separate).unwrap())
        .unwrap();
    let item_id: i64 =
        db.query_row("SELECT id FROM items WHERE name='Shield of Morning (Level 28)'", [], |row| row.get(0)).unwrap();
    let item_path = world.options.data_dir.join(paths.path("items", item_id).unwrap());
    let mut item: toml::Value = toml::from_str(&std::fs::read_to_string(&item_path).unwrap()).unwrap();
    let socket = item["socket"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|socket| socket["slot"].as_str() == Some("crafting: variant"))
        .unwrap();
    socket["set"] = "Shield of Morning (Level 28)".into();
    std::fs::write(item_path, toml::to_string(&item).unwrap()).unwrap();
    world.edit("");
    let separated = identities(&world.database());
    assert_eq!(separated.len(), 3);
    let before = world.bytes();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!(identities(&world.database()), separated);
    assert_eq!(world.bytes(), before);
}

#[test]
fn a_rank_option_import_preserves_the_merge_and_resolves_corrected_prose() {
    let source = fixture::xml_source();
    fixture::write_rank_options(source.path());
    let description = fixture::rank_option_description_correction(source.path());
    let notes = fixture::rank_option_merge("Riposte VI") + &fixture::rank_option_value_correction() + &description;
    let corrections = fixture::corrections_from(&[("rank-option.toml", &notes)]).unwrap();
    let world = ImportWorld::with_sources(
        fixture::grouping_cases(),
        corrections,
        WikiOverrides::default(),
        fixture::write_rank_options,
    );
    let path = world.options.source_dir.join("Items/SpyglassOptions.item");
    let original = std::fs::read_to_string(&path).unwrap();
    let corrected = original.replace(description.0[0].from.as_text().unwrap(), description.0[0].to.as_text().unwrap());
    assert_ne!(corrected, original);
    std::fs::write(path, corrected).unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts() && report.to_string().contains("RESOLVED CORRECTION"), "{report}");
    let db = world.database();
    let served: (String, Option<i64>, Option<i64>, String) = db.query_row("SELECT e.name,l.value,l.value2,o.description FROM socket_option_effects l JOIN effects e ON e.id=l.effect_id JOIN socket_options o ON o.id=l.option_id WHERE o.name='Riposte VI'", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).unwrap();
    assert_eq!(served, ("Riposte VI".into(), None, None, description.0[0].to.as_text().unwrap().into()));
    let before = tree::data_file_bytes(&world.options.data_dir).unwrap();
    tree::export_database(&db, &world.options.data_dir).unwrap();
    assert_eq!(before, tree::data_file_bytes(&world.options.data_dir).unwrap());
}

#[test]
fn a_merge_import_reports_a_missing_member_and_keeps_the_correction() {
    let corrections = fixture::corrections_from(&[("keylock-merge.toml", &fixture::search_and_spot_merge())]).unwrap();
    let mut world =
        ImportWorld::with_sources(fixture::grouping_cases(), corrections, WikiOverrides::default(), |source| {
            fixture::write_search_and_spot_item(source, 20, true);
        });
    fixture::write_search_and_spot_item(&world.options.source_dir, 20, false);
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(report.has_conflicts() && report.to_string().contains("no such bonus"), "{report}");
    let db = world.database();
    assert_eq!(fixture::search_and_spot_lines(&db), [("Search".into(), Some(20))]);
    let upstream: String = db
        .query_row(
            "SELECT from_value FROM corrections WHERE name='Legendary Keylock Ring' AND field='lines'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(upstream, "\"no such bonus\"");
    let missing = ddo_etl::corrections::corrections_recording_missing_targets(&db).unwrap();
    assert_eq!(missing.len(), 1);
    assert_eq!(
        (missing[0].kind.as_str(), missing[0].name.as_str(), missing[0].qualifier.as_str(), missing[0].field.as_str()),
        ("item_bonus", "Legendary Keylock Ring", "Search, Spot / Competence", "lines")
    );
    let correction_id: i64 = db
        .query_row("SELECT id FROM corrections WHERE name='Legendary Keylock Ring' AND field='lines'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(missing[0].id, correction_id);
    assert_missing_targets_repeat_cleanly(&mut world.options);
}

#[test]
fn verification_requests_remain_ours_through_import_and_export() {
    let world = ImportWorld::new(Corrections::default());
    let path = world.options.data_dir.join("characters/enhancements/bombardier/tree.toml");
    let original = std::fs::read_to_string(&path).unwrap();
    let request = "\n[[verification_request]]\nquestion = 'Is this tree current?'\nassumed = 'The recorded tree'\nraised = '2026-10-23'\n[[verification_request.source]]\nurl = 'https://ddowiki.com/page/Enhancements'\nread = '2026-10-23'\nsays = 'The typed tree'\n[[verification_request.source]]\nin_game = true\nread = '2026-10-23'\nsays = 'The observed tree'\n";
    std::fs::write(&path, format!("{original}{request}")).unwrap();
    let before = world.database();
    for (source, previous, sha) in
        [(&world.options.previous_dir, true, PREVIOUS_SHA), (&world.options.source_dir, false, NEW_SHA)]
    {
        let normalized = ddo_etl::import_ddobuilder::normalized_upstream_database(
            source,
            &before,
            &DatasetVersion { upstream_sha: sha.into(), built_at: "2026-10-23T00:00:00Z".into() },
            previous,
            &world.scratch.path().join(if previous { "A" } else { "B" }),
        )
        .unwrap();
        for table in ["verification_requests", "verification_request_sources", "note_lines"] {
            assert_eq!(
                normalized
                    .db
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert!(!report.to_string().contains("verification_request"), "{report}");
    let restored: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let expected: toml::Value = toml::from_str(request).unwrap();
    assert_eq!(restored["verification_request"], expected["verification_request"]);
    let after = world.database();
    assert_eq!(
        after.query_row("SELECT COUNT(*) FROM verification_request_sources", [], |row| row.get::<_, i64>(0)).unwrap(),
        2
    );
}

#[test]
fn plague_wolf_import_preserves_game_bug_evidence_when_upstream_adopts_the_value() {
    for adopted in [false, true] {
        let corrections =
            fixture::corrections_from(&[("plague-wolf.toml", &fixture::plague_wolf_corrections())]).unwrap();
        let world = ImportWorld::with_sources(
            fixture::groupings(),
            corrections,
            WikiOverrides::default(),
            fixture::write_plague_wolf_feat,
        );
        let path = world.options.data_dir.join("characters/feats/wild-shape.toml");
        let mut document: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let feat = document["row"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|row| row["name"].as_str() == Some("Wild Shape: Plague Wolf"))
            .unwrap();
        let bugs: toml::Value = toml::from_str("[[game_bug_correction]]\nkind = 'feat_bonus'\nfield = 'value'\nqualifier = 'Charisma / Penalty'\nintended = -2\nreason = '(Bug: gives -4 Charisma)'\nin_game = true\nread = '2026-10-23'\n[[game_bug_correction]]\nkind = 'feat_bonus'\nfield = 'remove'\nqualifier = 'Intelligence / Penalty'\nintended = -2\nreason = '(Bug: gives -4 Charisma); no Intelligence penalty in game.'\nin_game = true\nread = '2026-10-23'\n").unwrap();
        feat.as_table_mut().unwrap().insert("game_bug_correction".into(), bugs["game_bug_correction"].clone());
        std::fs::write(&path, toml::to_string(&document).unwrap()).unwrap();
        let before = world.database();
        let first = world.scratch.path().join("first-export");
        let second = world.scratch.path().join("second-export");
        tree::export_database(&before, &first).unwrap();
        for entry in walkdir::WalkDir::new(&world.options.data_dir)
            .into_iter()
            .map(Result::unwrap)
            .filter(|entry| entry.path().extension().is_some_and(|extension| extension == "png"))
        {
            let destination = first.join(entry.path().strip_prefix(&world.options.data_dir).unwrap());
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), destination).unwrap();
        }
        let mut rebuilt = Connection::open_in_memory().unwrap();
        tree::build_from_data(&first, &mut rebuilt).unwrap();
        tree::export_database(&rebuilt, &second).unwrap();
        assert_eq!(tree::data_file_bytes(&first).unwrap(), tree::data_file_bytes(&second).unwrap());
        if adopted {
            let xml = world.options.source_dir.join("Feats.xml");
            let original = std::fs::read_to_string(&xml).unwrap();
            let target =
                "<Amount size=\"1\">-2</Amount>\n        <Item>Intelligence</Item>\n        <Item>Charisma</Item>";
            assert!(original.contains(target));
            let split = "<Amount size=\"1\">-2</Amount>\n        <Item>Intelligence</Item></Effect><Effect><Type>AbilityBonus</Type><Bonus>Penalty</Bonus><AType>Simple</AType><Amount size=\"1\">-4</Amount><Item>Charisma</Item>";
            std::fs::write(xml, original.replace(target, split)).unwrap();
        }
        for (source, is_previous, sha) in
            [(&world.options.previous_dir, true, PREVIOUS_SHA), (&world.options.source_dir, false, NEW_SHA)]
        {
            let normalized = ddo_etl::import_ddobuilder::normalized_upstream_database(
                source,
                &before,
                &DatasetVersion { upstream_sha: sha.into(), built_at: "2026-10-23T00:00:00Z".into() },
                is_previous,
                &world.scratch.path().join(if is_previous { "A" } else { "B" }),
            )
            .unwrap();
            assert_eq!(
                normalized
                    .db
                    .query_row("SELECT COUNT(*) FROM game_bug_corrections", [], |row| row.get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
        let report = import_ddobuilder(&world.options).unwrap();
        let text = report.to_string();
        assert!(!report.has_conflicts(), "{text}");
        assert_eq!(text.contains("RESOLVED CORRECTION"), adopted, "{text}");
        assert!(
            report
                .entries
                .iter()
                .all(|entry| !entry.detail.contains("game_bug") && !entry.detail.contains("(Bug: gives")),
            "{text}"
        );
        assert!(!text.contains("game_bug"), "{text}");
        if !adopted {
            assert!(!text.contains("Wild Shape: Plague Wolf"), "{text}");
        }
        let after = world.database();
        assert_eq!(fixture::plague_wolf_penalties(&after), [("Charisma".into(), -4, "Penalty".into(), true)]);
        let restored: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let feat = restored["row"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"].as_str() == Some("Wild Shape: Plague Wolf"))
            .unwrap();
        assert_eq!(feat["game_bug_correction"], bugs["game_bug_correction"]);
    }
}

#[test]
fn a_stale_socket_removal_keeps_its_correction_as_a_conflict() {
    let removal = fixture::new_qualified_correction(
        "item_socket",
        "Epic Treasure Hunter's Spyglass (legacy) (level 20)",
        "socket = 'upgrade: tier 2'",
        "remove",
        "0",
        "1",
    );
    let corrections = fixture::corrections_from(&[("spyglass.toml", &removal)]).unwrap();
    let mut world = ImportWorld::with_sources(
        fixture::grouping_cases(),
        corrections,
        WikiOverrides::default(),
        fixture::write_spyglass_options,
    );
    let path = world.options.source_dir.join("Items/SpyglassOptions.item");
    let mut original = std::fs::read_to_string(&path).unwrap();
    let start = original.find("    <ItemAugment>").unwrap();
    let end = start + original[start..].find("    </ItemAugment>").unwrap() + "    </ItemAugment>".len();
    original.replace_range(start..end, "");
    std::fs::write(path, original).unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(report.has_conflicts(), "{report}");
    assert!(
        report.entries.iter().any(|entry| entry.category == "CONFLICT"
            && entry.detail.contains("remove")
            && entry.detail.contains("no such bonus")),
        "{report}"
    );
    let upstream: String = world
        .database()
        .query_row("SELECT from_value FROM corrections WHERE kind='item_socket' AND field='remove'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(upstream, "\"no such bonus\"");
    assert_missing_targets_repeat_cleanly(&mut world.options);
}

#[test]
fn socket_option_lines_import_follows_amounts_and_reports_changed_stats() {
    for change in ["amount", "stats", "option"] {
        let split = fixture::spyglass_option_correction(
            "lines",
            serde_json::json!([["Search", "Spot"]]),
            ddo_etl::corrections::CorrectionValue::Texts(vec!["Search".into(), "Spot".into()]),
        );
        let corrections = fixture::corrections_from(&[("option-split.toml", &split)]).unwrap();
        let mut world = ImportWorld::with_sources(
            fixture::grouping_cases(),
            corrections,
            WikiOverrides::default(),
            fixture::write_spyglass_options,
        );
        let path = world.options.source_dir.join("Items/SpyglassOptions.item");
        let original = std::fs::read_to_string(&path).unwrap();
        let edited = if change == "stats" {
            original
                .replace("<Item>Search</Item>\n          <Item>Spot</Item>", "<Item>Search</Item><Item>Listen</Item>")
        } else if change == "option" {
            let start = original.find("    <ItemAugment>").unwrap();
            let end = start + original[start..].find("    </ItemAugment>").unwrap() + "    </ItemAugment>".len();
            format!("{}{}", &original[..start], &original[end..])
        } else {
            original.replace(">20</Amount>", ">25</Amount>")
        };
        assert_ne!(edited, original);
        std::fs::write(path, edited).unwrap();
        let report = import_ddobuilder(&world.options).unwrap();
        assert_eq!(report.has_conflicts(), change != "amount", "{report}");
        let db = world.database();
        if change != "amount" {
            let from: String = db
                .query_row(
                    "SELECT from_value FROM corrections WHERE kind='socket_option_bonus' AND field='lines'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            if change == "option" {
                assert_eq!(from, serde_json::json!([["Search", "Spot"]]).to_string());
                assert!(report.to_string().contains("DECLINED REMOVAL socket_option_sets/"), "{report}");
                assert!(report.to_string().contains("KEPT CORRECTION socket_option_bonus"), "{report}");
            } else {
                assert_eq!(from, "\"no such bonus\"", "{change}: {report}");
                assert_missing_targets_repeat_cleanly(&mut world.options);
            }
        } else {
            assert_eq!(fixture::spyglass_option_lines(&db), [("Search".into(), 25), ("Spot".into(), 25)]);
        }
    }
}

#[test]
fn feat_lines_import_follows_amounts_and_reports_changed_stats() {
    for changed_stats in [false, true] {
        let corrections = fixture::corrections_from(&[("feat-split.toml", &fixture::scion_dodge_split())]).unwrap();
        let mut world = ImportWorld::with_sources(
            fixture::grouping_cases(),
            corrections,
            WikiOverrides::default(),
            fixture::write_scion_feat,
        );
        let path = world.options.source_dir.join("Feats.xml");
        let original = std::fs::read_to_string(&path).unwrap();
        let edited = if changed_stats {
            original.replace("<Type>DodgeCapBonus</Type>", "<Type>RangedPower</Type>")
        } else {
            original.replace(">4</Amount>", ">5</Amount>")
        };
        assert_ne!(edited, original);
        std::fs::write(path, edited).unwrap();
        let report = import_ddobuilder(&world.options).unwrap();
        assert_eq!(report.has_conflicts(), changed_stats, "{report}");
        let db = world.database();
        if changed_stats {
            let from: String = db
                .query_row("SELECT from_value FROM corrections WHERE kind='feat_bonus' AND field='lines'", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(from, "\"no such bonus\"");
            assert_eq!(fixture::scion_dodge_lines(&db), [("Ranged Power and Dodge".into(), 4)]);
            assert_missing_targets_repeat_cleanly(&mut world.options);
        } else {
            assert_eq!(fixture::scion_dodge_lines(&db), [("Dodge".into(), 5), ("Dodge Cap".into(), 5)]);
        }
    }
}

#[test]
fn a_stat_set_split_import_reports_a_removed_assassinate_stat() {
    let corrections =
        fixture::corrections_from(&[("dolorous.toml", &fixture::dolorous_stat_set_corrections())]).unwrap();
    let mut world = ImportWorld::with_sources(
        fixture::grouping_cases(),
        corrections,
        WikiOverrides::default(),
        fixture::write_stat_set_split_augments,
    );
    let path = world.options.source_dir.join("Augments/DolorousInvigorator.Augments.xml");
    let xml = std::fs::read_to_string(&path).unwrap();
    std::fs::write(path, xml.replace("      <Item>Assassinate</Item>\n", "")).unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(
        report
            .to_string()
            .lines()
            .any(|line| line.contains("CONFLICT") && line.contains("no such bonus") && line.contains("lines")),
        "{report}"
    );
    let db = world.database();
    let upstream: String = db
        .query_row(
            "SELECT from_value FROM corrections WHERE name='Dolorous Invigorator (Legendary)' AND field='lines'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(upstream, "\"no such bonus\"");
    let types: Vec<(String, String)> = db.prepare("SELECT name,from_value FROM corrections WHERE field='bonus_type' AND qualifier='Assassinate DC / null' ORDER BY name").unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert_eq!(types.len(), 2);
    assert!(types.iter().all(|(_, upstream)| upstream == "\"no such bonus\""));
    let missing = ddo_etl::corrections::corrections_recording_missing_targets(&db).unwrap();
    assert_eq!(
        missing.iter().filter(|row| row.field == "bonus_type" && row.qualifier == "Assassinate DC / null").count(),
        2
    );
    assert_missing_targets_repeat_cleanly(&mut world.options);
}

#[test]
fn a_stat_set_split_import_reports_a_changed_qualified_amount() {
    let split = fixture::new_qualified_correction(
        "augment_bonus",
        "Curse of Intangibility",
        "stat = 'Magical Resistance Rating Cap, Dodge Cap, Max Dex Bonus (Armor)'\nbonus_type = 'Fortune'\nbonus_value = 2",
        "lines",
        "[['Magical Resistance Rating Cap', 'Dodge Cap', 'Max Dex Bonus (Armor)']]",
        "['Magical Resistance Rating Cap', 'Dodge Cap', 'Max Dex Bonus (Armor)']",
    );
    let corrections = fixture::corrections_from(&[(
        "curse-of-intangibility.toml",
        &(split + &fixture::dolorous_stat_set_corrections()),
    )])
    .unwrap();
    let mut world = ImportWorld::with_sources(
        fixture::grouping_cases(),
        corrections,
        WikiOverrides::default(),
        fixture::write_stat_set_split_augments,
    );
    let path = world.options.source_dir.join("Augments/CurseOfIntangibility.Augments.xml");
    let xml = std::fs::read_to_string(&path).unwrap();
    std::fs::write(path, xml.replace("<Amount size=\"1\">2</Amount>", "<Amount size=\"1\">3</Amount>")).unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(
        report.to_string().lines().any(|line| {
            line.contains("CONFLICT") && line.contains("no such bonus") && line.contains("Curse of Intangibility")
        }),
        "{report}"
    );
    let db = world.database();
    let upstream: String = db
        .query_row(
            "SELECT from_value FROM corrections WHERE name='Curse of Intangibility' AND field='lines'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(upstream, "\"no such bonus\"");
    assert_missing_targets_repeat_cleanly(&mut world.options);
}

#[test]
fn a_split_import_accepts_a_changed_amount_without_conflict() {
    let world = woeful_split_import();
    let path = world.options.source_dir.join("Augments/WoefulShadow.Augments.xml");
    let xml = std::fs::read_to_string(&path).unwrap();
    std::fs::write(path, xml.replace(">3</Amount>", ">4</Amount>")).unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!(woeful_lines(&world.database()), [("Doubleshot".into(), 4), ("Doublestrike".into(), 4)]);
}

#[test]
fn a_split_import_reports_changed_stats_and_takes_the_new_lines() {
    let mut world = woeful_split_import();
    let path = world.options.source_dir.join("Augments/WoefulShadow.Augments.xml");
    let xml = std::fs::read_to_string(&path).unwrap();
    std::fs::write(path, xml.replace("<Type>Doubleshot</Type>", "<Type>RangedPower</Type>")).unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    let text = report.to_string();
    assert!(
        text.lines().any(|line| line.contains("CONFLICT") && line.contains("no such bonus") && line.contains("lines")),
        "{text}"
    );
    let rebuilt = world.database();
    assert_eq!(woeful_lines(&rebuilt), [("Ranged Power and Doublestrike".into(), 3)]);
    let upstream: String = rebuilt
        .query_row(
            "SELECT from_value FROM corrections WHERE name='Woeful Shadow (Legendary)' AND field='lines'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(upstream, "\"no such bonus\"");
    assert_missing_targets_repeat_cleanly(&mut world.options);
}

#[test]
fn a_lines_missing_target_repeats_after_its_parent_effect_file_is_removed() {
    let mut world = woeful_split_import_with_sources(|source| {
        let path = source.join("Augments/WoefulShadow.Augments.xml");
        let original = std::fs::read_to_string(&path).unwrap();
        let carrier = original.replace("Woeful Shadow (Legendary)", "Uncorrected Woeful Shadow");
        std::fs::write(path, original.replace("</Augments>", &carrier.replace("<Augments>", ""))).unwrap();
    });
    let parent = world.options.data_dir.join("effects/stat-groups/doublestrike-and-doubleshot.toml");
    assert!(parent.exists());
    let path = world.options.source_dir.join("Augments/WoefulShadow.Augments.xml");
    let original = std::fs::read_to_string(&path).unwrap();
    let start = original.find("    <Effect>").unwrap();
    let end = start + original[start..].find("</Effect>").unwrap() + "</Effect>".len();
    std::fs::write(path, original.replace(&original[start..end], "")).unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(report.has_conflicts(), "{report}");
    assert!(!parent.exists());
    assert!(
        report
            .entries
            .iter()
            .any(|entry| entry.category == "REMOVED" && entry.detail == "effects/doublestrike-and-doubleshot"),
        "{report}"
    );
    assert_missing_targets_repeat_cleanly(&mut world.options);
}

#[test]
fn a_split_import_reports_a_changed_type_or_removed_effect() {
    for removed in [false, true] {
        let mut world = woeful_split_import();
        let path = world.options.source_dir.join("Augments/WoefulShadow.Augments.xml");
        let xml = std::fs::read_to_string(&path).unwrap();
        let changed = if removed {
            let start = xml.find("    <Effect>").unwrap();
            let end = start + xml[start..].find("</Effect>").unwrap() + "</Effect>".len();
            format!("{}{}", &xml[..start], &xml[end..])
        } else {
            xml.replace("<Bonus>Profane</Bonus>", "<Bonus>Artifact</Bonus>")
        };
        std::fs::write(path, changed).unwrap();
        let report = import_ddobuilder(&world.options).unwrap();
        assert!(report.has_conflicts(), "{report}");
        assert!(report.to_string().contains("no such bonus"), "{report}");
        let rebuilt = world.database();
        assert!(woeful_lines(&rebuilt).is_empty());
        let count: i64 = rebuilt.query_row("SELECT COUNT(*) FROM augment_effects l JOIN effects e ON e.id=l.effect_id JOIN augments a ON a.id=l.augment_id WHERE a.name='Woeful Shadow (Legendary)' AND e.name='Doublestrike and Doubleshot'", [], |row| row.get(0)).unwrap();
        assert_eq!(count, i64::from(!removed));
        assert_missing_targets_repeat_cleanly(&mut world.options);
    }
}

fn assert_only_standing_audits<'a>(
    entries: impl IntoIterator<Item = &'a ddo_etl::import_ddobuilder::ImportReportEntry>,
) {
    for entry in entries {
        assert!(
            matches!(entry.category, "PARTIALLY MAPPED EFFECT" | "UNCLASSIFIED BUFF TYPE" | "STAT HOME MAJORITY"),
            "unexpected repeat report entry: {entry:?}"
        );
    }
}

fn assert_missing_targets_repeat_cleanly(options: &mut DdobuilderImportOptions) {
    let database = |root: &Path| {
        let mut db = Connection::open_in_memory().unwrap();
        tree::build_from_data(root, &mut db).unwrap();
        db
    };
    let before = database(&options.data_dir);
    Corrections::from_database(&before).unwrap();
    let missing = ddo_etl::corrections::corrections_recording_missing_targets(&before).unwrap();
    assert!(!missing.is_empty());
    options.previous_dir = options.source_dir.clone();
    options.previous_sha = options.sha.clone();
    let mut previous_entries = None;
    for _ in 0..2 {
        let report = import_ddobuilder(options).unwrap();
        assert!(!report.has_conflicts() && !report.needs_settlement(), "{report}");
        assert_eq!((report.files.written, report.files.removed), (0, 0), "{report}");
        assert_only_standing_audits(&report.entries);
        let entries = report
            .entries
            .iter()
            .map(|entry| (entry.path.clone(), entry.category, entry.detail.clone()))
            .collect::<Vec<_>>();
        if let Some(previous) = &previous_entries {
            assert_eq!(&entries, previous);
        }
        previous_entries = Some(entries);
        let after = database(&options.data_dir);
        assert_eq!(ddo_etl::corrections::corrections_recording_missing_targets(&after).unwrap(), missing);
        for correction in &missing {
            let upstream: String = after
                .query_row("SELECT from_value FROM corrections WHERE id=?1", [correction.id], |row| row.get(0))
                .unwrap();
            assert_eq!(upstream, r#""no such bonus""#);
        }
    }
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import_ddobuilder")
}

fn copy_tree(source: &Path, destination: &Path) {
    for entry in walkdir::WalkDir::new(source) {
        let entry = entry.unwrap();
        if entry.file_type().is_file() {
            let target = destination.join(entry.path().strip_prefix(source).unwrap());
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn copy_data_icons_into_source(root: &Path, source: &Path, scratch: &Path) {
    let icons = scratch.join("icons");
    ddo_etl::icons::copy_data_icons(root, &icons).unwrap();
    for (kind, folder) in ddo_etl::icons::ICON_KIND_SOURCE_FOLDERS {
        if icons.join(kind).is_dir() {
            copy_tree(&icons.join(kind), &source.join(folder));
        }
    }
}

fn priced_socket_set_import() -> (tempfile::TempDir, DdobuilderImportOptions) {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().join("data");
    let previous = scratch.path().join("previous");
    let source = scratch.path().join("source");
    copy_tree(&fixtures.join("data"), &root);
    copy_tree(&fixtures.join("DataFiles"), &previous);
    copy_data_icons_into_source(&root, &previous, scratch.path());
    copy_tree(&previous, &source);
    let options = DdobuilderImportOptions {
        previous_dir: previous,
        source_dir: source,
        data_dir: root,
        previous_sha: Some("fixture".into()),
        sha: Some("fixture-next".into()),
    };
    (scratch, options)
}

#[test]
fn the_import_binary_exits_two_after_reporting_a_declined_set_removal() {
    let (_scratch, options) = priced_socket_set_import();
    std::fs::remove_file(options.source_dir.join("Items/Fabricator's Gauntlets.item")).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ddo-etl"))
        .arg("import-ddobuilder")
        .arg("--previous")
        .arg(&options.previous_dir)
        .arg("--source")
        .arg(&options.source_dir)
        .arg("--data")
        .arg(&options.data_dir)
        .args(["--previous-sha", "fixture", "--sha", "fixture-next"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    for expected in [
        "table | added | removed | changed | conflicts | declined removals | resolved corrections",
        "DECLINED REMOVAL socket_option_sets/",
        "0 conflicts",
    ] {
        assert!(stdout.contains(expected), "{stdout}\n{stderr}");
    }
    assert!(!stdout.contains("SKIPPED WIKI FACT"), "{stdout}");
    assert_eq!(output.status.code(), Some(2), "{stdout}\n{stderr}");
    let mut rebuilt = Connection::open_in_memory().unwrap();
    tree::build_from_data(&options.data_dir, &mut rebuilt).unwrap();
}

#[test]
fn a_priced_set_survives_when_upstream_removes_its_last_carrier() {
    for removed_from_new in [true, false] {
        let (_scratch, options) = priced_socket_set_import();
        let root = options.data_dir.clone();
        let previous = options.previous_dir.clone();
        let source = options.source_dir.clone();
        if removed_from_new {
            let carrier = source.join("Items/Fabricator's Gauntlets.item");
            let xml = std::fs::read_to_string(&carrier).unwrap();
            std::fs::write(
                &carrier,
                xml.replace("<Name>Fabricator's Ingenuity</Name>", "<Name>Unpriced option</Name>"),
            )
            .unwrap();
            let error = import_ddobuilder(&DdobuilderImportOptions {
                previous_dir: previous.clone(),
                source_dir: source.clone(),
                data_dir: root.clone(),
                previous_sha: Some("fixture".into()),
                sha: Some("fixture-next".into()),
            })
            .expect_err("a missing option in an existing set stays a hard error");
            let message = format!("{error:#}");
            assert!(
                message.contains("recipe set option")
                    && message.contains("Fabricator's Ingenuity")
                    && message.contains("matched 0 rows"),
                "{message}"
            );
            std::fs::write(carrier, xml).unwrap();
        }
        let removed_from = if removed_from_new { &source } else { &previous };
        std::fs::remove_file(removed_from.join("Items/Fabricator's Gauntlets.item")).unwrap();
        let report = import_ddobuilder(&DdobuilderImportOptions {
            previous_dir: previous,
            source_dir: source.clone(),
            data_dir: root.clone(),
            previous_sha: Some("fixture".into()),
            sha: Some("fixture-next".into()),
        })
        .unwrap();
        let rendered = report.to_string();
        if removed_from_new {
            let declined =
                report.entries.iter().filter(|entry| entry.category == "DECLINED REMOVAL").collect::<Vec<_>>();
            assert!(
                declined.iter().any(|entry| entry.detail.contains("Fabricator's Ingenuity")
                    && entry.detail.contains("fabricators-gauntlets")),
                "{rendered}"
            );
        }
        assert!(!report.entries.iter().any(|entry| entry.category == "SKIPPED WIKI FACT"), "{rendered}");
        let mut rebuilt = Connection::open_in_memory().unwrap();
        tree::build_from_data(&root, &mut rebuilt).unwrap();
        let (id, links): (i64, i64) = rebuilt.query_row("SELECT s.id,COUNT(*) FROM crafting_recipe_set_options link JOIN socket_options o ON o.id=link.option_id JOIN socket_option_sets s ON s.id=o.set_id JOIN crafting_recipes r ON r.id=link.recipe_id WHERE r.option='Fabricator''s Ingenuity' GROUP BY s.id", [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
        assert_eq!(links, 1);
        let carrierless = rebuilt
            .prepare(ddo_etl::build::CARRIERLESS_SOCKET_OPTION_SETS)
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, Option<String>>(2)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(carrierless.len(), usize::from(removed_from_new), "{carrierless:?}");
        if removed_from_new {
            assert_eq!((carrierless[0].0.as_str(), carrierless[0].1), ("Fabricator's Gauntlets", id));
            let detail = carrierless[0].2.as_deref().unwrap();
            assert!(detail.contains("Nearly Finished") && detail.contains("Fabricator's Ingenuity"), "{detail}");
        }
        if removed_from_new {
            let paths = tree::DataFilePaths::from_database(&rebuilt).unwrap();
            assert!(paths.path("socket_option_sets", id).unwrap().starts_with("packs/shared/sockets"));
        }
        let again = import_ddobuilder(&DdobuilderImportOptions {
            previous_dir: source.clone(),
            source_dir: source,
            data_dir: root.clone(),
            previous_sha: Some("fixture-next".into()),
            sha: Some("fixture-next".into()),
        })
        .unwrap();
        assert!(!again.has_conflicts(), "{again}");
        assert!(!again.needs_settlement(), "{again}");
        assert_eq!((again.files.written, again.files.removed), (0, 0), "{again}");
        assert!(!again.entries.iter().any(|entry| entry.category == "SKIPPED WIKI FACT"), "{again}");
    }
}

#[test]
fn canonical_wiki_rows_do_not_appear_in_an_unrelated_import_delta() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().join("data");
    let previous = scratch.path().join("previous");
    let source = scratch.path().join("source");
    copy_tree(&fixtures.join("data"), &root);
    copy_tree(&fixtures.join("DataFiles"), &previous);
    copy_data_icons_into_source(&root, &previous, scratch.path());
    copy_tree(&previous, &source);
    let item = source.join("Items/Bold Trinket.item");
    let contents = std::fs::read_to_string(&item).unwrap();
    assert!(contents.contains("<MinLevel>4</MinLevel>"));
    std::fs::write(item, contents.replace("<MinLevel>4</MinLevel>", "<MinLevel>5</MinLevel>")).unwrap();
    let report = import_ddobuilder(&DdobuilderImportOptions {
        previous_dir: previous,
        source_dir: source,
        data_dir: root.clone(),
        previous_sha: Some("fixture".into()),
        sha: Some("fixture-next".into()),
    })
    .unwrap();
    assert!(!report.has_conflicts(), "{report}");
    let rendered = report.to_string();
    assert_eq!(report.tables["items"].changed, 1, "only his Bold Trinket edit belongs in the delta: {rendered}");
    for table in ["vendors", "crafting_systems"] {
        assert_eq!(report.tables[table].changed, 0, "{rendered}");
    }
    for slug in ["battle-axe-of-the-oozing-hunger", "morten-edgewright", "heroic-green-steel"] {
        assert!(!rendered.contains(slug), "{rendered}");
    }
    let mut rebuilt = Connection::open_in_memory().unwrap();
    tree::build_from_data(&root, &mut rebuilt).unwrap();
    assert_eq!(
        rebuilt
            .query_row("SELECT minimum_level FROM items WHERE name='Bold Trinket'", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        5
    );
}

#[test]
fn import_reports_a_text_only_prose_entry_that_stopped_matching() {
    for changed in [false, true] {
        let world = ImportWorld::with_sources(
            fixture::grouping_cases(),
            Corrections::default(),
            WikiOverrides::default(),
            fixture::write_prose_sets,
        );
        fixture::write_prose_sets(&world.options.source_dir);
        if changed {
            let path = world.options.source_dir.join("SetBonuses.xml");
            let original = std::fs::read_to_string(&path).unwrap();
            assert!(original.contains("<Description>Underwater action</Description>"));
            std::fs::write(
                path,
                original.replace(
                    "<Description>Underwater action</Description>",
                    "<Description>Underwater breathing</Description>",
                ),
            )
            .unwrap();
        }
        let report = import_ddobuilder(&world.options).unwrap();
        assert!(!report.has_conflicts(), "{report}");
        let audits = report
            .entries
            .iter()
            .filter(|entry| {
                entry.category == "UNMATCHED SET TIER PROSE BONUS" && entry.detail.contains("Corsair's Cunning")
            })
            .collect::<Vec<_>>();
        assert_eq!(audits.len(), usize::from(changed), "{report}");
        if changed {
            assert!(
                audits[0].detail.contains("2 pieces") && audits[0].detail.contains("Underwater action"),
                "{:?}",
                audits[0]
            );
        }
    }
}

#[test]
fn import_reports_a_valued_prose_entry_that_stopped_matching() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().join("data");
    let previous = scratch.path().join("previous");
    let source = scratch.path().join("source");
    copy_tree(&fixtures.join("data"), &root);
    copy_tree(&fixtures.join("DataFiles"), &previous);
    copy_data_icons_into_source(&root, &previous, scratch.path());
    copy_tree(&previous, &source);
    let path = source.join("SetBonuses.xml");
    let original = std::fs::read_to_string(&path).unwrap();
    assert!(original.contains("+2 Artifact bonus to attack and damage."));
    std::fs::write(
        path,
        original.replace("+2 Artifact bonus to attack and damage.", "+3 Artifact bonus to Attack and Damage"),
    )
    .unwrap();
    let report = import_ddobuilder(&DdobuilderImportOptions {
        previous_dir: previous,
        source_dir: source,
        data_dir: root,
        previous_sha: Some("fixture".into()),
        sha: Some("fixture-next".into()),
    })
    .unwrap();
    let audit = report
        .entries
        .iter()
        .find(|entry| entry.category == "UNMATCHED SET TIER PROSE BONUS")
        .expect("missing prose audit");
    assert!(
        audit.detail.contains("Fabricator's Ingenuity")
            && audit.detail.contains("2 pieces")
            && audit.detail.contains("+2 Artifact bonus to attack and damage."),
        "{audit:?}"
    );
}

#[test]
fn import_reports_unmapped_damage_targets_without_losing_existing_targets() {
    let world = ImportWorld::with_sources(
        fixture::grouping_cases(),
        Corrections::default(),
        WikiOverrides::default(),
        |source| {
            std::fs::create_dir_all(source.join("Augments")).unwrap();
            std::fs::write(
                source.join("Augments/ThunderforgedDamageTargets.Augments.xml"),
                include_str!("fixtures/effect-groups/ThunderforgedDamageTargets.Augments.xml")
                    .replace("<Damage>Acid</Damage>", "<Damage>Unknown</Damage>"),
            )
            .unwrap();
        },
    );
    let report = ddo_etl::import_ddobuilder::import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    let audits: Vec<_> = report.entries.iter().filter(|entry| entry.category == "UNMAPPED DAMAGE TARGET").collect();
    assert_eq!(audits.len(), 2, "{report}");
    for effect_type in ["EnergyResistance", "SpellLore"] {
        assert!(
            audits.iter().any(|entry| entry.detail.contains(effect_type) && entry.detail.contains("Unknown")),
            "{report}"
        );
    }
    let db = world.database();
    let stats: Vec<String> = db.prepare("SELECT e.name FROM owner_bonuses b JOIN effects e ON e.id=b.stat_id JOIN augments a ON a.id=b.owner_id WHERE b.owner_kind='augment' AND a.name='Blinding Fear' ORDER BY e.name").unwrap().query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert_eq!(
        stats,
        ["Cold Resistance", "Electric Resistance", "Fire Resistance", "Saving Throws", "Sonic Resistance"]
    );
}

#[test]
fn import_reports_mapping_audits_without_changing_success_status() {
    let world = ImportWorld::new(Corrections::default());
    let path = world.options.source_dir.join("ItemBuffs.xml");
    let definitions = std::fs::read_to_string(&path).unwrap();
    let probes = "<Buff><Type>ProbeFamily</Type><DisplayText>Probe Family: test</DisplayText><Effect><Type>UnclassifiedProbe</Type><AType>NotNeeded</AType><Item>Strength</Item><Item>Dexterity</Item></Effect></Buff><Buff><Type>UnusedMappingProbe</Type><DisplayText>Unused mapping probe +1</DisplayText><Effect><Type>AbilityBonus</Type><Bonus>UnrecognizedAuditBonus</Bonus><AType>Simple</AType><Amount>1</Amount><Item>Strength</Item><Item>Dexterity</Item></Effect></Buff>";
    assert!(definitions.contains("</Buffs>"));
    std::fs::write(path, definitions.replace("</Buffs>", &format!("{probes}</Buffs>"))).unwrap();
    let items = world.options.source_dir.join("Items");
    std::fs::create_dir_all(&items).unwrap();
    std::fs::write(items.join("Probe.item"), "<Items><Item><Name>Probe</Name><MinLevel>1</MinLevel><EquipmentSlot><Trinket/></EquipmentSlot><Description>A mapping audit fixture.</Description><Buff><Type>ProbeFamily</Type></Buff></Item></Items>").unwrap();
    let output = cli(&world).output().unwrap();
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success(), "{report}\n{}", String::from_utf8_lossy(&output.stderr));
    assert!(report.contains("crates/ddo-etl/data/effect_map.toml: UNCLASSIFIED BUFF TYPE"), "{report}");
    assert!(report.contains("UnclassifiedProbe: 1 family; 1 item; ProbeFamily"), "{report}");
    assert!(report.contains("ProbeFamily: 2 source targets, 0 mapped stats"), "{report}");
    assert!(report.contains("UnusedMappingProbe") && report.contains("UnrecognizedAuditBonus"), "{report}");
    assert_eq!(
        world
            .database()
            .query_row("SELECT COUNT(*) FROM items WHERE name='Probe'", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn fieldless_citation_does_not_make_an_upstream_item_wiki_only() {
    let scratch = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles");
    let mut db = Connection::open_in_memory().unwrap();
    let corrections = fixture::fixture_icon_corrections();
    build::build_database(
        fixture::groupings(),
        &source,
        &WikiOverrides::default(),
        &corrections,
        &mut db,
        &DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-18T00:00:00Z".into() },
    )
    .unwrap();
    db.execute("INSERT INTO citations (owner_kind, owner_id, field, url, read) SELECT 'items', id, '', 'https://ddowiki.com/page/Item:Reviewed', '2026-10-18' FROM items WHERE id = (SELECT MIN(id) FROM items)", []).unwrap();
    export_fixture_with_icons(&db, &source, scratch.path());
    let mut rebuilt = Connection::open_in_memory().unwrap();
    tree::build_from_data(scratch.path(), &mut rebuilt).unwrap();
    let provenance: String = rebuilt.query_row(
        "SELECT provenance FROM items JOIN citations ON owner_kind = 'items' AND owner_id = items.id WHERE citations.url = 'https://ddowiki.com/page/Item:Reviewed'", [], |row| row.get(0)
    ).unwrap();
    assert_eq!(provenance, "maetrim");
}

fn export_fixture_with_icons(db: &Connection, source: &Path, root: &Path) {
    tree::export_database(db, root).unwrap();
    ddo_etl::icons::export_icons(source, &root.join("icons")).unwrap();
    let icons = ddo_etl::icons::complete_referenced_icons(root).unwrap();
    for missing in icons.missing_icons {
        let path = root.join("icons").join(missing);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::copy(source.join("FeatImages/PowerAttack.png"), path).unwrap();
    }
    ddo_etl::icons::place_enhancement_icons_beside_trees(root).unwrap();
}

fn assert_overlay_tables_match(normalized: &Connection, ours: &Connection) {
    let mut delta = ddo_etl::delta::database_delta(normalized, ours).unwrap();
    delta.arrays.retain(|key, change| {
        if key.array != "citation" {
            return true;
        }
        assert!(
            change.previous.iter().all(|citation| change.new.contains(citation)),
            "XML citations must be a subset of C: {key:?}: {change:?}"
        );
        false
    });
    delta
        .entities
        .retain(|_, changes| !(changes.added.is_empty() && changes.removed.is_empty() && changes.changed.is_empty()));
    assert!(delta.is_empty(), "{delta:#?}");
    for table in ddo_etl::database_tables::table_names(ours).unwrap() {
        if table == "citations" {
            continue;
        }
        let columns = ddo_etl::database_tables::table_columns(ours, &table).unwrap();
        let stable_rows = |db: &Connection| {
            ddo_etl::database_tables::table_rows(db, &table)
                .unwrap()
                .into_iter()
                .map(|row| {
                    row.into_iter()
                        .zip(&columns)
                        .filter_map(|(value, column)| {
                            (column != "built_at" && !(table == "schema_version" && column == "applied_at"))
                                .then_some(value)
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(stable_rows(normalized), stable_rows(ours), "{table}");
    }
}

#[test]
fn item_placeholder_marks_survive_both_xml_builds_and_a_no_op_import() {
    for marked in [true, false] {
        let ours = fixture::database(|root| {
            let path = root.join("packs/shared/crafting/catalyst-crafting/ingredients/abyssal-gem.toml");
            let contents = std::fs::read_to_string(&path).unwrap();
            if marked {
                std::fs::write(path, format!("is_item_placeholder = true\n{contents}")).unwrap();
            }
        });
        let scratch = tempfile::tempdir().unwrap();
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let source = scratch.path().join("source");
        copy_tree(&fixtures.join("DataFiles"), &source);
        copy_data_icons_into_source(&fixtures.join("data"), &source, scratch.path());
        let version = ours
            .query_row("SELECT upstream_sha,built_at FROM dataset_version", [], |row| {
                Ok(DatasetVersion { upstream_sha: row.get(0)?, built_at: row.get(1)? })
            })
            .unwrap();
        let marks = |db: &Connection| {
            db.prepare("SELECT slug FROM crafting_ingredients WHERE is_item_placeholder ORDER BY slug")
                .unwrap()
                .query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        let expected = if marked { vec!["abyssal-gem".to_string()] } else { Vec::new() };
        assert_eq!(marks(&ours), expected);
        for previous in [true, false] {
            let normalized = ddo_etl::import_ddobuilder::normalized_upstream_database(
                &source,
                &ours,
                &version,
                previous,
                &scratch.path().join(if previous { "previous" } else { "new" }),
            )
            .unwrap();
            assert_eq!(marks(&normalized.db), expected, "previous={previous}, marked={marked}");
        }
        let root = scratch.path().join("data");
        export_fixture_with_icons(&ours, &source, &root);
        let ingredient =
            std::fs::read_to_string(root.join("packs/shared/crafting/catalyst-crafting/ingredients/abyssal-gem.toml"))
                .unwrap();
        assert_eq!(ingredient.contains("is_item_placeholder = true"), marked);
        assert!(!ingredient.contains("is_item_placeholder = false"));
        let world = ImportWorld {
            scratch,
            options: DdobuilderImportOptions {
                previous_dir: source.clone(),
                source_dir: source,
                data_dir: root,
                previous_sha: Some(version.upstream_sha.clone()),
                sha: Some(version.upstream_sha),
            },
        };
        let before = world.bytes();
        let report = import_ddobuilder(&world.options).unwrap();
        assert!(!report.has_conflicts(), "{report}");
        assert_eq!((report.files.written, report.files.removed), (0, 0), "{report}");
        assert_eq!(world.bytes(), before);
        assert_eq!(marks(&world.database()), expected);
    }
}

#[test]
fn crafting_file_and_xml_builds_preserve_the_same_authored_rows() {
    let ours = fixture::database(|root| {
        let directory = root.join("packs/shared/crafting/ingredients");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("thread-of-fate.toml"),
            "name='Thread of Fate'\ntier='any'\ndrop_location='Fixture chest'\ncitation=[]\n",
        )
        .unwrap();
        let path = root.join("packs/shared/crafting/catalyst-crafting/crafting.toml");
        let system: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        std::fs::write(path,format!("{}\n[[station]]\nname='Test Altar Keeper'\nnote='Use this keeper.'\n[[unlock]]\nkind='kept_item'\ncrafting_ingredient='Thread of Fate'\ntext='Keep the thread.'\n[[step]]\nname='Create'\nis_random=true\n[[step.effect]]\neffect='Constitution'\nbonus_type='Enhancement'\nvalue=10\nequipment_slot='Off Hand'\n[[step.modifier]]\neffect_type='Weapon_BaseDamage'\namounts=[0.5]\n[[step.cost]]\ntier='heroic'\nitem=\"Acrobat's Ring\"\nquantity=2\n[[step.cost]]\ntier='legendary'\ningredient='Thread of Fate'\nquantity=3\n[[recipe]]\noption='Create thread'\nstep='Create'\ntier='heroic'\nyields_ingredient='Thread of Fate'\n[[recipe.unlock]]\nkind='crafting_level'\nvalue=20\ntext='Requires crafting level 20.'\n",toml::to_string(&system).unwrap())).unwrap();
        let path = root.join("packs/shared/crafting/test-upgrade-altar/crafting.toml");
        let original = std::fs::read_to_string(&path).unwrap();
        std::fs::write(path,format!("{original}\n[[recipe]]\noption='Use thread'\ntier='any'\n[[recipe.ingredient]]\ningredient='Thread of Fate'\nquantity=1\n")).unwrap();
        let path = root.join("packs/shared/crafting/stations.toml");
        let original = std::fs::read_to_string(&path).unwrap();
        std::fs::write(path, format!("{original}\n[[row.unlock]]\nkind='other'\ntext='Speak to the keeper.'\n"))
            .unwrap();
    });
    let scratch = tempfile::tempdir().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let source = scratch.path().join("source");
    copy_tree(&fixtures.join("DataFiles"), &source);
    copy_data_icons_into_source(&fixtures.join("data"), &source, scratch.path());
    let version = ours
        .query_row("SELECT upstream_sha,built_at FROM dataset_version", [], |row| {
            Ok(DatasetVersion { upstream_sha: row.get(0)?, built_at: row.get(1)? })
        })
        .unwrap();
    for previous in [true, false] {
        let normalized = ddo_etl::import_ddobuilder::normalized_upstream_database(
            &source,
            &ours,
            &version,
            previous,
            &scratch.path().join(if previous { "previous" } else { "new" }),
        )
        .unwrap();
        assert_overlay_tables_match(&normalized.db, &ours);
    }
}

#[test]
fn database_overlay_preserves_authored_array_order_and_effect_titles() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let scratch = tempfile::tempdir().unwrap();
    let mut base = Connection::open_in_memory().unwrap();
    tree::build_from_data(&fixtures.join("data"), &mut base).unwrap();
    let mut wiki = WikiOverrides::from_database(&base).unwrap();
    let item = wiki.items.iter_mut().find(|item| item.name == "Battle Axe of the Oozing Hunger").unwrap();
    item.weapon.as_mut().unwrap().dr_bypass = vec!["Slash".into(), "Magic".into()];
    item.effects.push(ddo_etl::wiki::WikiItemEffect {
        name: "Fixture Charge — 5 Charges".into(),
        description: Some("Caster level: 14".into()),
        value: None,
        target: None,
    });
    item.effects.push(ddo_etl::wiki::WikiItemEffect {
        name: "Major Anger".into(),
        description: base
            .query_row("SELECT description_template FROM effects WHERE name = ?1", ["Major Anger"], |row| row.get(0))
            .unwrap(),
        value: None,
        target: None,
    });
    wiki.crafting_systems[0].families = vec!["Ruby".into(), "Greensteel_Heroic".into()];
    let mut corrections = Corrections::from_database(&base).unwrap();
    let mut correction = corrections.entries[0].clone();
    correction.kind = ddo_model::enums::CorrectionKind::ItemEffect;
    correction.name = "Buckler of the Golden Age".into();
    correction.field = "add".into();
    correction.effect = None;
    correction.family = None;
    correction.selection = None;
    correction.from = ddo_etl::corrections::CorrectionValue::Null;
    correction.to = ddo_etl::corrections::CorrectionValue::Effect(ddo_etl::corrections::EffectAddition {
        name: "Fixture Charge".into(),
        description: "5 Charges: Caster level: 14".into(),
        value: None,
    });
    corrections.entries.push(correction);
    let mut written = Connection::open_in_memory().unwrap();
    build::build_database(
        fixture::groupings(),
        &fixtures.join("DataFiles"),
        &wiki,
        &corrections,
        &mut written,
        &DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-18T00:00:00Z".into() },
    )
    .unwrap();
    let root = scratch.path().join("data");
    export_fixture_with_icons(&written, &fixtures.join("DataFiles"), &root);
    let mut ours = Connection::open_in_memory().unwrap();
    tree::build_from_data(&root, &mut ours).unwrap();
    let reconstructed = WikiOverrides::from_database(&ours).unwrap();
    let item = reconstructed.items.iter().find(|item| item.name == "Battle Axe of the Oozing Hunger").unwrap();
    assert_eq!(item.weapon.as_ref().unwrap().dr_bypass, ["Slash", "Magic"]);
    assert!(item.effects.iter().any(|effect| effect.name == "Fixture Charge — 5 Charges"));
    assert!(item.effects.iter().any(|effect| effect.name == "Major Anger"));
    assert_eq!(
        reconstructed
            .crafting_systems
            .iter()
            .find(|system| system.name == wiki.crafting_systems[0].name)
            .unwrap()
            .families,
        ["Ruby", "Greensteel_Heroic"]
    );
    let mut rebuilt = Connection::open_in_memory().unwrap();
    build::build_database(
        fixture::groupings(),
        &fixtures.join("DataFiles"),
        &reconstructed,
        &Corrections::from_database(&ours).unwrap(),
        &mut rebuilt,
        &DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-18T00:00:00Z".into() },
    )
    .unwrap();
    let normalized_root = scratch.path().join("normalized");
    export_fixture_with_icons(&rebuilt, &fixtures.join("DataFiles"), &normalized_root);
    let mut normalized = Connection::open_in_memory().unwrap();
    tree::build_from_data(&normalized_root, &mut normalized).unwrap();
    for query in [
        "SELECT verbose_name_template FROM effects WHERE name='Fixture Charge'",
        "SELECT damage FROM item_weapon_stats WHERE item_id=(SELECT id FROM items WHERE name='Battle Axe of the Oozing Hunger')",
        "SELECT to_value FROM corrections WHERE kind='item_effect' AND name='Buckler of the Golden Age' AND qualifier='Fixture Charge' AND field='add'",
    ] {
        let value = |db: &Connection| db.query_row(query, [], |row| row.get::<_, String>(0)).unwrap();
        assert_eq!(value(&normalized), value(&ours), "{query}");
    }
    let value: String = normalized.query_row("SELECT to_value FROM corrections WHERE kind='item_effect' AND name='Buckler of the Golden Age' AND qualifier='Fixture Charge' AND field='add'", [], |row| row.get(0)).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&value).unwrap()["description"],
        "5 Charges: Caster level: 14"
    );
}

#[test]
fn overlay_catalog_identity_survives_insertion_order() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut fixture = Connection::open_in_memory().unwrap();
    tree::build_from_data(&fixtures.join("data"), &mut fixture).unwrap();
    let mut wiki = WikiOverrides::from_database(&fixture).unwrap();
    let mut system = wiki.crafting_systems.iter().find(|system| system.name == "Catalyst Crafting").unwrap().clone();
    system.name = "Ritual Table".into();
    system.families.clear();
    system.recipes.clear();
    wiki.crafting_systems.push(system);
    for reversed in [false, true] {
        if reversed {
            wiki.crafting_systems.reverse();
        }
        let mut built = Connection::open_in_memory().unwrap();
        build::build_database(
            fixture::groupings(),
            &fixtures.join("DataFiles"),
            &wiki,
            &Corrections::from_database(&fixture).unwrap(),
            &mut built,
            &DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-18T00:00:00Z".into() },
        )
        .unwrap();
        let id: i64 =
            built.query_row("SELECT id FROM crafting_systems WHERE name='Ritual Table'", [], |row| row.get(0)).unwrap();
        assert_eq!(id, 28, "the XML identity catalog owns Ritual Table's identity");
        assert_eq!(built.query_row("SELECT COUNT(*) FROM removed_entities WHERE owner_kind='crafting_systems' AND owner_id=?1 AND name='Ritual Table'", [id], |row| row.get::<_, i64>(0)).unwrap(), 1);
    }
}

const HEROISM_MERGES: [([&str; 2], &str); 2] = [
    (["Attuned to Heroism 1", "Attuned to Heroism 2"], "Reviewed Tier 1"),
    (["Attuned to Heroism 3", "Attuned to Heroism 4"], "Reviewed Tier 2"),
];

fn crafting_label(upstream_type: &str) -> String {
    format!("crafting: {}", upstream_type.to_lowercase())
}

fn heroism_merges() -> Vec<([String; 2], String)> {
    HEROISM_MERGES.iter().map(|(types, target)| (types.map(crafting_label), crafting_label(target))).collect()
}

struct MergedSocketLabelImport {
    report: String,
    previous: Connection,
    rebuilt: Connection,
    recipe_count: i64,
}

#[derive(Clone, Copy, PartialEq)]
enum HisAdoption {
    Nothing,
    EverySpelling,
    FirstSpellingOnly,
}

fn import_with_merged_socket_labels(adoption: HisAdoption) -> MergedSocketLabelImport {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let scratch = tempfile::tempdir().unwrap();
    let mut fixture = Connection::open_in_memory().unwrap();
    tree::build_from_data(&fixtures.join("data"), &mut fixture).unwrap();
    let mut wiki = WikiOverrides::from_database(&fixture).unwrap();
    let mut corrections = Corrections::from_database(&fixture).unwrap();
    for (spellings, target) in heroism_merges() {
        corrections.entries.extend(
            fixture::correction_between(
                "socket_label",
                &spellings[0],
                "name",
                CorrectionValue::Texts(spellings.to_vec()),
                CorrectionValue::Text(target),
            )
            .0,
        );
    }
    let item = wiki.items.iter_mut().find(|item| item.name == "Battle Axe of the Oozing Hunger").unwrap();
    item.augment_slots = vec!["crafting: reviewed tier 1".into(), "crafting: reviewed tier 2".into()];
    let system = wiki.crafting_systems.iter_mut().find(|system| system.name == "Mikrom Sum").unwrap();
    let step_template = system.steps[0].clone();
    system.steps = heroism_merges()
        .into_iter()
        .map(|(_, label)| {
            let mut step = step_template.clone();
            step.name = label.clone();
            step.slot = Some(label);
            step
        })
        .collect();
    system.recipes.push(ddo_etl::wiki::CraftingRecipe {
        tier: "any".into(),
        step: Some("crafting: reviewed tier 1".into()),
        yields_ingredient: None,
        yields_variant: None,
        option: "Reviewed socket grant".into(),
        augments: vec![],
        note: None,
        grants_slot: Some("crafting: reviewed tier 2".into()),
        cost: vec![],
        unlocks: vec![],
        set_options: vec![],
    });
    let recipe_count = system.recipes.len() as i64;
    let source = scratch.path().join("DataFiles");
    copy_tree(&fixtures.join("DataFiles"), &source);
    let root = scratch.path().join("data");
    let mut previous = Connection::open_in_memory().unwrap();
    let report = build::build_database(
        fixture::groupings(),
        &source,
        &wiki,
        &corrections,
        &mut previous,
        &DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-18T00:00:00Z".into() },
    )
    .unwrap();
    assert!(report.stale_corrections.is_empty(), "{:?}", report.stale_corrections);
    export_fixture_with_icons(&previous, &source, &root);
    copy_data_icons_into_source(&root, &source, scratch.path());
    let new_source = scratch.path().join("new");
    copy_tree(&source, &new_source);
    if adoption != HisAdoption::Nothing {
        let item = new_source.join("Items/Sireth, Spear of the Sky.item");
        let mut contents = std::fs::read_to_string(&item).unwrap();
        for (types, adopted) in HEROISM_MERGES {
            let adopted_spellings = if adoption == HisAdoption::EverySpelling { 2 } else { 1 };
            for upstream_type in &types[..adopted_spellings] {
                let element = format!("<Type>{upstream_type}</Type>");
                assert!(contents.contains(&element), "{element}");
                contents = contents.replace(&element, &format!("<Type>{adopted}</Type>"));
            }
        }
        std::fs::write(item, contents).unwrap();
    }
    let report = import_ddobuilder(&DdobuilderImportOptions {
        previous_dir: source,
        source_dir: new_source,
        data_dir: root.clone(),
        previous_sha: Some(PREVIOUS_SHA.into()),
        sha: Some(NEW_SHA.into()),
    })
    .unwrap();
    assert!(!report.has_conflicts(), "{report}");
    let mut rebuilt = Connection::open_in_memory().unwrap();
    tree::build_from_data(&root, &mut rebuilt).unwrap();
    MergedSocketLabelImport { report: report.to_string(), previous, rebuilt, recipe_count }
}

#[test]
fn merged_socket_labels_follow_the_writer_and_have_no_import_delta() {
    let import = import_with_merged_socket_labels(HisAdoption::Nothing);
    assert!(!import.report.contains("CORRECTION"), "{}", import.report);
    assert!(!import.report.contains("reviewed tier"), "{}", import.report);
    for db in [&import.previous, &import.rebuilt] {
        let steps = db
            .prepare("SELECT t.label FROM crafting_steps s JOIN crafting_systems c ON c.id=s.system_id JOIN augment_slot_types t ON t.id=s.slot_id WHERE c.name='Mikrom Sum' ORDER BY s.sort_order")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(steps, ["crafting: reviewed tier 1", "crafting: reviewed tier 2"]);
        let labels = db.prepare("SELECT s.label FROM item_augment_slots l JOIN items i ON i.id=l.item_id JOIN augment_slot_types s ON s.id=l.slot_id WHERE i.name='Battle Axe of the Oozing Hunger' ORDER BY l.sort_order").unwrap().query_map([], |row| row.get::<_, String>(0)).unwrap().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(labels, ["crafting: reviewed tier 1", "crafting: reviewed tier 2"]);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM crafting_recipes r JOIN crafting_steps step ON step.id=r.step_id JOIN augment_slot_types s ON s.id=step.slot_id JOIN augment_slot_types g ON g.id=r.grants_slot_id WHERE s.label='crafting: reviewed tier 1' AND g.label='crafting: reviewed tier 2'", [], |row| row.get::<_, i64>(0)).unwrap(), import.recipe_count);
        let sireth_labels = db
            .prepare("SELECT s.label FROM item_augment_slots l JOIN items i ON i.id=l.item_id JOIN augment_slot_types s ON s.id=l.slot_id WHERE i.name='Sireth, Spear of the Sky' AND s.label LIKE 'crafting: %' ORDER BY l.sort_order")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            &sireth_labels[..4],
            [
                "crafting: reviewed tier 1",
                "crafting: reviewed tier 1",
                "crafting: reviewed tier 2",
                "crafting: reviewed tier 2"
            ]
        );
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM augment_slot_types WHERE label LIKE 'crafting: attuned to heroism%'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        let merges = db
            .prepare("SELECT name, from_value, to_value FROM corrections WHERE kind = 'socket_label' AND to_value LIKE '%reviewed tier%' ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let expected: Vec<(String, String, String)> = heroism_merges()
            .iter()
            .map(|(spellings, target)| {
                (
                    spellings[0].clone(),
                    serde_json::to_string(spellings).unwrap(),
                    serde_json::to_string(target).unwrap(),
                )
            })
            .collect();
        assert_eq!(merges, expected);
    }
    assert_eq!(
        Corrections::from_database(&import.previous)
            .unwrap()
            .entries
            .iter()
            .filter(|entry| matches!(entry.from, CorrectionValue::Texts(_)))
            .count(),
        2
    );
}

#[test]
fn merged_socket_labels_resolve_when_his_files_adopt_the_name_for_every_spelling() {
    let import = import_with_merged_socket_labels(HisAdoption::EverySpelling);
    assert_eq!(import.report.matches("RESOLVED CORRECTION").count(), 2, "{}", import.report);
    assert!(import.report.contains("renamed upstream"), "{}", import.report);
}

#[test]
fn a_merge_is_not_resolved_while_one_listed_spelling_remains_in_his_files() {
    let import = import_with_merged_socket_labels(HisAdoption::FirstSpellingOnly);
    assert!(!import.report.contains("RESOLVED CORRECTION"), "{}", import.report);
    let db = &import.rebuilt;
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM augment_slot_types WHERE label LIKE 'crafting: attuned to heroism%'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM corrections WHERE kind = 'socket_label' AND to_value LIKE '%reviewed tier%'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
}

#[test]
fn separate_corrections_with_one_target_fail_naming_both_on_every_kind() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut fixture = Connection::open_in_memory().unwrap();
    tree::build_from_data(&fixtures.join("data"), &mut fixture).unwrap();
    for (kind, originals, target) in [
        ("adventure_pack", ["Masterminds of Sharn", "Chill of Ravenloft"], "Converging pack"),
        ("socket_label", ["crafting: attuned to heroism 1", "crafting: attuned to heroism 2"], "crafting: converging"),
    ] {
        let mut corrections = Corrections::from_database(&fixture).unwrap();
        for original in originals {
            corrections.entries.extend(
                fixture::correction_between(
                    kind,
                    original,
                    "name",
                    CorrectionValue::Text(original.into()),
                    CorrectionValue::Text(target.into()),
                )
                .0,
            );
        }
        let error = build::build_database(
            fixture::groupings(),
            &fixtures.join("DataFiles"),
            &WikiOverrides::default(),
            &corrections,
            &mut Connection::open_in_memory().unwrap(),
            &DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-18T00:00:00Z".into() },
        )
        .unwrap_err();
        let message = format!("{error:#}");
        for name in originals.into_iter().chain([target]) {
            assert!(message.contains(name), "{kind}: {message}");
        }
    }
}

#[test]
fn overlay_references_follow_only_renames_active_in_each_xml_build() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let scratch = tempfile::tempdir().unwrap();
    let mut initial = Connection::open_in_memory().unwrap();
    tree::build_from_data(&fixtures.join("data"), &mut initial).unwrap();
    initial.execute("UPDATE adventure_packs SET name = 'Reviewed Masterminds of Sharn', slug = 'reviewed-masterminds-of-sharn' WHERE name = 'Masterminds of Sharn'", []).unwrap();
    initial.execute("INSERT INTO corrections (kind,name,field,from_value,to_value,reason,source,read) VALUES ('adventure_pack','Masterminds of Sharn','name','\"Masterminds of Sharn\"','\"Reviewed Masterminds of Sharn\"','Exercise the overlay reference contract','https://ddowiki.com/page/Masterminds_of_Sharn','2026-10-18')", []).unwrap();
    initial.execute("UPDATE vendors SET pack_id = (SELECT id FROM adventure_packs WHERE name = 'Reviewed Masterminds of Sharn') WHERE id = (SELECT MIN(id) FROM vendors)", []).unwrap();
    tree::rederive_data_build_tables(&initial).unwrap();
    let data_root = scratch.path().join("data");
    export_fixture_with_icons(&initial, &fixtures.join("DataFiles"), &data_root);
    let mut ours = Connection::open_in_memory().unwrap();
    tree::build_from_data(&data_root, &mut ours).unwrap();
    let wiki = WikiOverrides::from_database(&ours).unwrap();
    let corrections = Corrections::from_database(&ours).unwrap();
    let version = DatasetVersion { upstream_sha: "fixture".into(), built_at: "2026-10-18T00:00:00Z".into() };
    for (state, adopted) in [("previous", false), ("new-unchanged", false), ("new-adopted", true)] {
        let source = scratch.path().join(state);
        copy_tree(&fixtures.join("DataFiles"), &source);
        if adopted {
            let quests = source.join("Quests.xml");
            let contents = std::fs::read_to_string(&quests)
                .unwrap()
                .replace("Masterminds of Sharn", "Reviewed Masterminds of Sharn");
            std::fs::write(quests, contents).unwrap();
        }
        let mut built = Connection::open_in_memory().unwrap();
        let report =
            build::build_database(fixture::groupings(), &source, &wiki, &corrections, &mut built, &version).unwrap();
        let resolved = report
            .stale_corrections
            .iter()
            .filter(|correction| correction.kind == "adventure_pack")
            .collect::<Vec<_>>();
        assert_eq!(resolved.len(), usize::from(adopted), "{state}: {resolved:?}");
        if adopted {
            assert!(matches!(resolved[0].cause, build::StaleCorrectionCause::RenameDoneUpstream { .. }));
        }
        let pack: String = built.query_row("SELECT p.name FROM sagas s JOIN adventure_packs p ON p.id=s.pack_id WHERE s.name='Masterminds of Sharn'", [], |row| row.get(0)).unwrap();
        assert_eq!(pack, "Reviewed Masterminds of Sharn", "{state}");
        if !adopted {
            ddo_etl::import_ddobuilder::fill_cells_from_ours(&built, &ours).unwrap();
            let export = scratch.path().join(format!("{state}-data"));
            export_fixture_with_icons(&built, &source, &export);
            let mut normalized = Connection::open_in_memory().unwrap();
            tree::build_from_data(&export, &mut normalized).unwrap();
            assert_overlay_tables_match(&normalized, &ours);
        }
    }
    let mut unknown = wiki;
    let saga = unknown.sagas.iter_mut().find(|saga| saga.name == "Masterminds of Sharn").unwrap();
    saga.pack = Some("A name neither XML nor a correction carries".into());
    let owner_file = saga.file_name.clone();
    let error = build::build_database(
        fixture::groupings(),
        &fixtures.join("DataFiles"),
        &unknown,
        &corrections,
        &mut Connection::open_in_memory().unwrap(),
        &version,
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains(&owner_file), "{error:#}");
}

#[test]
fn canonical_fixture_reproduces_all_tables_with_xml_citations_a_subset_of_ours() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut ours = Connection::open_in_memory().unwrap();
    tree::build_from_data(&fixtures.join("data"), &mut ours).unwrap();
    let corrections = Corrections::from_database(&ours).unwrap();
    assert!(corrections.entries.iter().any(|correction| correction.family.is_some() && correction.field == "name"));
    assert!(corrections.entries.iter().any(|correction| correction.selection.is_some() && correction.field == "name"));
    let wiki = WikiOverrides::from_database(&ours).unwrap();
    let mut upstream = Connection::open_in_memory().unwrap();
    let report = build::build_database(
        fixture::groupings(),
        &fixtures.join("DataFiles"),
        &wiki,
        &corrections,
        &mut upstream,
        &DatasetVersion { upstream_sha: "fixture".into(), built_at: "2026-10-06T00:00:00Z".into() },
    )
    .unwrap();
    assert!(report.stale_corrections.is_empty(), "{:?}", report.stale_corrections);
    let ordered_corrections = |mut corrections: Corrections| {
        corrections.entries.sort_by_key(|correction| {
            (
                correction.kind.as_str().to_string(),
                correction.name.clone(),
                correction.field.clone(),
                correction.qualifier(),
            )
        });
        corrections
    };
    assert_eq!(
        ordered_corrections(Corrections::from_database(&upstream).unwrap()),
        ordered_corrections(corrections.clone())
    );
    ddo_etl::import_ddobuilder::fill_cells_from_ours(&upstream, &ours).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    export_fixture_with_icons(&upstream, &fixtures.join("DataFiles"), scratch.path());
    let mut normalized = Connection::open_in_memory().unwrap();
    tree::build_from_data(scratch.path(), &mut normalized).unwrap();
    assert_overlay_tables_match(&normalized, &ours);
}

#[test]
fn source_citations_do_not_change_drop_provenance() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data");
    let scratch = tempfile::tempdir().unwrap();
    copy_tree(&fixtures, scratch.path());
    let item_path = scratch.path().join("packs/shared/gear/battle-axe-of-the-oozing-hunger.toml");
    let mut item: toml::Table = toml::from_str(&std::fs::read_to_string(&item_path).unwrap()).unwrap();
    let provenance = |root: &Path| {
        let mut db = Connection::open_in_memory().unwrap();
        tree::build_from_data(root, &mut db).unwrap();
        db.query_row("SELECT d.drop_text_segment, d.source_match FROM quest_drops d JOIN items i ON d.owner_kind = 'item' AND d.owner_id = i.id JOIN quests q ON q.id = d.quest_id WHERE i.name = 'Battle Axe of the Oozing Hunger' AND q.name = 'The Grotto'", [], |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?))).unwrap()
    };
    let with_citation = provenance(scratch.path());
    item.get_mut("citation")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .retain(|citation| citation.get("field").and_then(toml::Value::as_str) != Some("sources"));
    std::fs::write(&item_path, toml::to_string(&item).unwrap()).unwrap();
    let without_citation = provenance(scratch.path());
    assert_eq!(without_citation, (Some("Test source: The Grotto, end chest".into()), Some("The Grotto".into())));
    assert_eq!(with_citation, without_citation, "citations only record checks");
}

#[test]
fn database_wiki_facts_ignore_item_citation_sources_and_use_constant_queries() {
    use rusqlite::trace::{TraceEvent, TraceEventCodes};
    use std::sync::atomic::{AtomicUsize, Ordering};
    static QUERY_COUNT: AtomicUsize = AtomicUsize::new(0);
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let scratch = tempfile::tempdir().unwrap();
    copy_tree(&fixtures.join("data"), scratch.path());
    let item_path = scratch.path().join("packs/shared/gear/battle-axe-of-the-oozing-hunger.toml");
    let original: toml::Table = toml::from_str(&std::fs::read_to_string(&item_path).unwrap()).unwrap();
    let mut previous_facts = None;
    let mut previous_queries = None;
    for annotation in
        [None, Some("url = \"https://ddowiki.com/page/Item:Battle_Axe_of_the_Oozing_Hunger\""), Some("in_game = true")]
    {
        let mut item = original.clone();
        item.remove("citation");
        if let Some(annotation) = annotation {
            let citation: toml::Table =
                toml::from_str(&format!("{annotation}\nread = \"2026-10-18\"\nnote = \"Checked in the source\""))
                    .unwrap();
            item.insert("citation".into(), toml::Value::Array(vec![toml::Value::Table(citation)]));
        }
        std::fs::write(&item_path, toml::to_string(&item).unwrap()).unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        tree::build_from_data(scratch.path(), &mut db).unwrap();
        db.trace_v2(
            TraceEventCodes::SQLITE_TRACE_STMT,
            Some(|event| {
                if let TraceEvent::Stmt(_, _) = event {
                    QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
                }
            }),
        );
        QUERY_COUNT.store(0, Ordering::SeqCst);
        let wiki = WikiOverrides::from_database(&db).unwrap();
        let queries = QUERY_COUNT.load(Ordering::SeqCst);
        if let Some(previous) = &previous_facts {
            assert_eq!(&wiki.items, previous);
        }
        if let Some(previous) = previous_queries {
            assert_eq!(queries, previous);
        }
        previous_facts = Some(wiki.items);
        previous_queries = Some(queries);
    }
    let mut added_item = original;
    added_item.insert("name".into(), "Additional Wiki Axe".into());
    added_item.remove("citation");
    std::fs::write(
        scratch.path().join("packs/shared/gear/additional-wiki-axe.toml"),
        toml::to_string(&added_item).unwrap(),
    )
    .unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    tree::build_from_data(scratch.path(), &mut db).unwrap();
    db.trace_v2(
        TraceEventCodes::SQLITE_TRACE_STMT,
        Some(|event| {
            if let TraceEvent::Stmt(_, _) = event {
                QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
            }
        }),
    );
    QUERY_COUNT.store(0, Ordering::SeqCst);
    let wiki = WikiOverrides::from_database(&db).unwrap();
    assert_eq!(wiki.items.len(), previous_facts.unwrap().len() + 1);
    assert_eq!(
        QUERY_COUNT.load(Ordering::SeqCst),
        previous_queries.unwrap(),
        "wiki reconstruction must not query per entity"
    );
}

#[test]
fn database_wiki_reference_failure_names_the_owning_data_file() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut ours = Connection::open_in_memory().unwrap();
    tree::build_from_data(&fixtures.join("data"), &mut ours).unwrap();
    let wiki = WikiOverrides::from_database(&ours).unwrap();
    let corrections = Corrections::from_database(&ours).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    copy_tree(&fixtures.join("DataFiles"), scratch.path());
    std::fs::remove_file(scratch.path().join("Items/Buckler of the Golden Age.item")).unwrap();
    let error = build::build_database(
        fixture::groupings(),
        scratch.path(),
        &wiki,
        &corrections,
        &mut Connection::open_in_memory().unwrap(),
        &DatasetVersion { upstream_sha: "fixture".into(), built_at: "2026-10-18T00:00:00Z".into() },
    )
    .unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("Buckler of the Golden Age"), "{message}");
    assert!(message.contains("packs/magic-of-myth-drannor/quests/book-burning/quest.toml"), "{message}");
}

#[test]
fn xml_overlay_does_not_recreate_citations_owned_by_the_data_tree() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut db = Connection::open_in_memory().unwrap();
    build::build_database(
        fixture::groupings(),
        &fixtures.join("DataFiles"),
        &WikiOverrides::from_database(&fixture::database(|_| {})).unwrap(),
        &Corrections::default(),
        &mut db,
        &DatasetVersion { upstream_sha: "fixture".into(), built_at: "2026-10-18T00:00:00Z".into() },
    )
    .unwrap();
    let count: i64 = db.query_row("SELECT COUNT(*) FROM citations WHERE url = 'https://ddowiki.com/page/Item:Battle_Axe_of_the_Oozing_Hunger'", [], |row| row.get(0)).unwrap();
    assert_eq!(count, 0, "wiki facts must not recreate C's citations");
}

#[test]
fn every_entity_preserves_citations_with_notes_through_the_data_tree() {
    let scratch = tempfile::tempdir().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let source = fixtures.join("DataFiles");
    let mut db = Connection::open_in_memory().unwrap();
    tree::build_from_data(&fixtures.join("data"), &mut db).unwrap();
    let mut expected = Vec::new();
    for (table, merge) in ddo_etl::delta::TABLE_MERGES {
        if !matches!(merge, ddo_etl::delta::TableMerge::Entity { .. }) {
            continue;
        }
        let columns = db
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let identity_column = if columns.iter().any(|column| column == "slug") { "slug" } else { "name" };
        let limit = if matches!(*table, "stances" | "effects") { "" } else { " LIMIT 1" };
        let rows = db
            .prepare(&format!("SELECT id, {identity_column} FROM {table} ORDER BY {identity_column}{limit}"))
            .unwrap()
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert!(!rows.is_empty(), "fixture has no {table}");
        for (id, slug) in rows {
            for in_game in [false, true] {
                let note = format!("Checked {table}/{slug}; in_game={in_game}.");
                let url = (!in_game).then_some("https://ddowiki.com/page/Review");
                db.execute("INSERT INTO citations (owner_kind, owner_id, field, url, in_game, read, note) VALUES (?1, ?2, '', ?3, ?4, '2026-10-18', ?5)", rusqlite::params![table, id, url, in_game, note]).unwrap();
                expected.push((table, identity_column, slug.clone(), note, url, in_game));
            }
        }
    }
    let stance_requirements = |db: &Connection| {
        db.prepare("SELECT s.slug, r.group_kind, r.group_index, r.sort_order, r.req_type, r.items, r.value FROM requirements r JOIN stances s ON r.owner_kind = 'stance' AND r.owner_id = s.id ORDER BY s.slug, r.group_index, r.sort_order").unwrap()
            .query_map([], |row| (0..7).map(|column| row.get::<_, rusqlite::types::Value>(column)).collect::<rusqlite::Result<Vec<_>>>()).unwrap()
            .collect::<rusqlite::Result<Vec<_>>>().unwrap()
    };
    let expected_requirements = stance_requirements(&db);
    assert!(!expected_requirements.is_empty());
    export_fixture_with_icons(&db, &source, scratch.path());
    let before = tree::data_file_bytes(scratch.path()).unwrap();
    let mut rebuilt = Connection::open_in_memory().unwrap();
    tree::build_from_data(scratch.path(), &mut rebuilt).unwrap();
    assert_eq!(expected_requirements, stance_requirements(&rebuilt));
    let annotated = tree::annotated_entities(&rebuilt).unwrap();
    for (table, identity_column, slug, note, url, in_game) in expected {
        let id: i64 = rebuilt
            .query_row(&format!("SELECT id FROM {table} WHERE {identity_column} = ?1"), [&slug], |row| row.get(0))
            .unwrap();
        let count: i64 = rebuilt
            .query_row(
                "SELECT COUNT(*) FROM citations WHERE owner_kind = ?1 AND owner_id = ?2 AND note = ?3 AND url IS ?4 AND in_game = ?5",
                rusqlite::params![table, id, note, url, in_game],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "{table}/{slug}");
        assert!(annotated.contains(&(table.to_string(), id)), "{table}/{slug}");
    }
    tree::export_database(&rebuilt, scratch.path()).unwrap();
    assert_eq!(before, tree::data_file_bytes(scratch.path()).unwrap());
}

const PLACEHOLDER_AUGMENT: &str = "Lunar Gem of Evocation (Heroic)";
const PLACEHOLDER_AUGMENT_FILE: &str = "Augments/LunarGemEvocation.Augments.xml";
const WIKI_DROPS_LINE: &str = "Drops in: A Blood Pact, end chest\nFlocked Together, optional chest";

fn placeholder_augment_description(drops: &str) -> String {
    format!("+2 Profane Bonus to Evocation DCs\nDrops in: {drops}")
}

fn write_placeholder_augment(xml_root: &Path, minimum_level: u32, drops: &str) {
    let path = xml_root.join(PLACEHOLDER_AUGMENT_FILE);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        path,
        format!(
            "<?xml version='1.0' encoding='utf-8'?>\n<Augments>\n  <Augment>\n    <Name>{PLACEHOLDER_AUGMENT}</Name>\n    <Description>{}</Description>\n    <MinLevel>{minimum_level}</MinLevel>\n    <Type>Moon</Type>\n    <Icon>HistoryBookWhite</Icon>\n    <Effect>\n      <Type>SpellDC</Type>\n      <Bonus>Profane</Bonus>\n      <AType>Simple</AType>\n      <Amount size=\"1\">2</Amount>\n      <Item>Evocation</Item>\n    </Effect>\n  </Augment>\n</Augments>\n",
            placeholder_augment_description(drops)
        ),
    )
    .unwrap();
}

fn placeholder_augment_correction() -> Corrections {
    fixture::corrections_from(&[(
        "placeholder.toml",
        &fixture::correction_between(
            "augment",
            PLACEHOLDER_AUGMENT,
            "description",
            CorrectionValue::Text(placeholder_augment_description("?")),
            CorrectionValue::Text(placeholder_augment_description(WIKI_DROPS_LINE.strip_prefix("Drops in: ").unwrap())),
        )
        .cited(
            "Maetrim's description gives no drop locations; the wiki page lists them.",
            "https://ddowiki.com/page/Item:Lunar_Gem_of_Evocation_(Heroic)",
            "2026-10-18",
        ),
    )])
    .unwrap()
}

fn placeholder_augment_state(world: &ImportWorld) -> (String, i64, Vec<String>) {
    let db = world.database();
    let (description, minimum_level): (String, i64) = db
        .query_row("SELECT description, min_level FROM augments WHERE name = ?1", [PLACEHOLDER_AUGMENT], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    let sources = db
        .prepare(
            "SELECT q.name FROM quest_drops d JOIN quests q ON q.id = d.quest_id
             WHERE d.owner_kind = 'augment' AND d.owner_id = (SELECT id FROM augments WHERE name = ?1) ORDER BY q.name",
        )
        .unwrap()
        .query_map([PLACEHOLDER_AUGMENT], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    (description, minimum_level, sources)
}

#[test]
fn a_placeholder_augment_keeps_its_correction_and_sources_when_he_edits_another_field() {
    let world = ImportWorld::with_placeholder_augment();
    let before = placeholder_augment_state(&world);
    assert_eq!(before.0, placeholder_augment_description(WIKI_DROPS_LINE.strip_prefix("Drops in: ").unwrap()));
    assert_eq!(before.2, ["A Blood Pact", "Flocked Together"]);
    write_placeholder_augment(&world.options.source_dir, 2, "?");
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert!(!report.to_string().contains("CORRECTION"), "{report}");
    assert_eq!(placeholder_augment_state(&world), (before.0, 2, before.2));
    let kept = Corrections::from_database(&world.database()).unwrap();
    assert_eq!(kept.entries.len(), 1);
    assert_eq!(kept.entries[0].to, placeholder_augment_correction().entries[0].to);
}

#[test]
fn a_placeholder_augment_reports_our_correction_when_he_fills_in_its_description() {
    let world = ImportWorld::with_placeholder_augment();
    let before = placeholder_augment_state(&world);
    write_placeholder_augment(&world.options.source_dir, 1, "Back to the Abyss, end chest");
    let report = import_ddobuilder(&world.options).unwrap();
    let rendered = report.to_string();
    assert_eq!(report.conflict_count(), 1, "{rendered}");
    let conflict = rendered.lines().find(|line| line.contains(": CONFLICT")).unwrap();
    assert!(
        conflict.contains("CONFLICT augment \"Lunar Gem of Evocation (Heroic)\"")
            && conflict.contains(".description")
            && conflict.contains("Back to the Abyss, end chest"),
        "{rendered}"
    );
    assert_eq!(placeholder_augment_state(&world), before);
    let kept = Corrections::from_database(&world.database()).unwrap();
    assert_eq!(kept.entries[0].to, placeholder_augment_correction().entries[0].to);
    assert_eq!(
        kept.entries[0].from,
        CorrectionValue::Text(placeholder_augment_description("Back to the Abyss, end chest"))
    );
}

struct ImportWorld {
    scratch: tempfile::TempDir,
    options: DdobuilderImportOptions,
}

impl ImportWorld {
    fn new(corrections: Corrections) -> Self {
        Self::with_wiki(corrections, WikiOverrides::default())
    }

    fn with_wiki(corrections: Corrections, wiki: WikiOverrides) -> Self {
        Self::with_sources(fixture::groupings(), corrections, wiki, |_| {})
    }

    fn with_placeholder_augment() -> Self {
        Self::with_sources(
            fixture::groupings(),
            placeholder_augment_correction(),
            WikiOverrides::default(),
            |previous_dir| {
                write_placeholder_augment(previous_dir, 1, "?");
                let icons = previous_dir.join("AugmentImages");
                std::fs::create_dir_all(&icons).unwrap();
                std::fs::copy(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/data/icons/augments/HistoryBookWhite.png"),
                    icons.join("HistoryBookWhite.png"),
                )
                .unwrap();
            },
        )
    }

    fn with_sources(
        groupings: &ddo_etl::groupings::Groupings,
        corrections: Corrections,
        wiki: WikiOverrides,
        add_to_previous: impl FnOnce(&Path),
    ) -> Self {
        let scratch = tempfile::tempdir().unwrap();
        let previous_dir = scratch.path().join("previous");
        let source_dir = scratch.path().join("new");
        let data_dir = scratch.path().join("data");
        copy_tree(&fixture_root().join("ba7e18e/DataFiles"), &previous_dir);
        add_to_previous(&previous_dir);
        copy_tree(&previous_dir, &source_dir);
        copy_tree(&fixture_root().join("d1350a0c/DataFiles"), &source_dir);
        let mut db = Connection::open_in_memory().unwrap();
        build::build_database(
            groupings,
            &previous_dir,
            &wiki,
            &corrections,
            &mut db,
            &DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-17T00:00:00Z".into() },
        )
        .unwrap();
        tree::export_ddobuilder_tree(&db, &previous_dir, &data_dir).unwrap();
        Self {
            scratch,
            options: DdobuilderImportOptions {
                source_dir,
                previous_dir,
                data_dir,
                previous_sha: Some(PREVIOUS_SHA.into()),
                sha: Some(NEW_SHA.into()),
            },
        }
    }

    fn database(&self) -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        tree::build_from_data(&self.options.data_dir, &mut db).unwrap();
        db
    }

    fn edit(&self, sql: &str) {
        let db = self.database();
        db.execute_batch(sql).unwrap();
        tree::rederive_data_build_tables(&db).unwrap();
        tree::export_database(&db, &self.options.data_dir).unwrap();
        ddo_etl::icons::place_enhancement_icons_beside_trees(&self.options.data_dir).unwrap();
    }

    fn cost(&self, internal_name: &str) -> String {
        self.database()
            .query_row("SELECT cost_per_rank FROM enhancements WHERE internal_name = ?1", [internal_name], |row| {
                row.get(0)
            })
            .unwrap()
    }

    fn bytes(&self) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        walkdir::WalkDir::new(&self.options.data_dir)
            .into_iter()
            .map(Result::unwrap)
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| {
                (
                    entry.path().strip_prefix(&self.options.data_dir).unwrap().to_path_buf(),
                    std::fs::read(entry.path()).unwrap(),
                )
            })
            .collect()
    }
}

#[test]
fn previous_xml_skips_later_named_wiki_facts_but_new_xml_stays_strict() {
    let mut world = ImportWorld::new(Corrections::default());
    let db = world.database();
    db.execute("UPDATE adventure_packs SET name='Later Terror of Demogorgon',slug='later-terror-of-demogorgon' WHERE name='Terror of Demogorgon'", []).unwrap();
    db.execute("INSERT INTO vendors (name,slug,location,pack_id,provenance,wiki_url) SELECT 'Later Vendor','later-vendor','Fixture location',id,'wiki','https://ddowiki.com/page/Vendor' FROM adventure_packs WHERE name='Later Terror of Demogorgon'", []).unwrap();
    tree::rederive_data_build_tables(&db).unwrap();
    world.options.data_dir = world.scratch.path().join("later-data");
    tree::export_ddobuilder_tree(&db, &world.options.previous_dir, &world.options.data_dir).unwrap();
    let ours = world.database();
    let wiki = WikiOverrides::from_database(&ours).unwrap();
    let file = wiki.vendors.iter().find(|vendor| vendor.name == "Later Vendor").unwrap().file_name.clone();
    let source = world.scratch.path().join("later-source");
    copy_tree(&world.options.previous_dir, &source);
    let quests = source.join("Quests.xml");
    std::fs::write(
        &quests,
        std::fs::read_to_string(&quests).unwrap().replace("Terror of Demogorgon", "Later Terror of Demogorgon"),
    )
    .unwrap();
    world.options.source_dir = source;
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert!(!report.entries.iter().any(|entry| entry.category == "SKIPPED WIKI FACT"), "{report}");
    assert_eq!(
        world
            .database()
            .query_row(
                "SELECT p.name FROM vendors v JOIN adventure_packs p ON p.id=v.pack_id WHERE v.name='Later Vendor'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "Later Terror of Demogorgon"
    );
    world.options.source_dir = world.options.previous_dir.clone();
    world.options.previous_sha = Some(NEW_SHA.into());
    let before = world.bytes();
    let error = import_ddobuilder(&world.options).unwrap_err();
    assert!(format!("{error:#}").contains(&file), "{error:#}");
    assert_eq!(before, world.bytes());
}

#[test]
fn import_reports_an_adopted_pack_rename_and_keeps_an_unchanged_one() {
    for adopted in [false, true] {
        let mut world = ImportWorld::new(Corrections::default());
        let db = world.database();
        db.execute("UPDATE adventure_packs SET name='Reviewed Terror of Demogorgon', slug='reviewed-terror-of-demogorgon' WHERE name='Terror of Demogorgon'", []).unwrap();
        db.execute("INSERT INTO corrections (kind,name,field,from_value,to_value,reason,source,read) VALUES ('adventure_pack','Terror of Demogorgon','name','\"Terror of Demogorgon\"','\"Reviewed Terror of Demogorgon\"','Exercise the rename lifecycle','https://ddowiki.com/page/Terror_of_Demogorgon','2026-10-18')", []).unwrap();
        db.execute("INSERT INTO vendors (name,slug,location,pack_id,provenance,wiki_url) SELECT 'Reviewed Vendor','reviewed-vendor','Fixture location',id,'wiki','https://ddowiki.com/page/Vendor' FROM adventure_packs WHERE name='Reviewed Terror of Demogorgon'", []).unwrap();
        tree::rederive_data_build_tables(&db).unwrap();
        world.options.data_dir = world.scratch.path().join("reviewed-data");
        tree::export_ddobuilder_tree(&db, &world.options.previous_dir, &world.options.data_dir).unwrap();
        let source = world.scratch.path().join("rename-source");
        copy_tree(&world.options.previous_dir, &source);
        if adopted {
            let quests = source.join("Quests.xml");
            let contents = std::fs::read_to_string(&quests)
                .unwrap()
                .replace("Terror of Demogorgon", "Reviewed Terror of Demogorgon");
            std::fs::write(quests, contents).unwrap();
        }
        world.options.source_dir = source;
        let report = import_ddobuilder(&world.options).unwrap();
        assert!(!report.has_conflicts(), "{report}");
        let text = report.to_string();
        assert_eq!(text.contains("RESOLVED CORRECTION"), adopted, "{text}");
        assert_eq!(text.contains("renamed upstream"), adopted, "{text}");
        let result = world.database();
        let pack: String = result
            .query_row(
                "SELECT p.name FROM vendors v JOIN adventure_packs p ON p.id=v.pack_id WHERE v.name='Reviewed Vendor'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pack, "Reviewed Terror of Demogorgon");
    }
}

#[test]
fn import_of_the_recorded_commit_changes_nothing() {
    let mut world = ImportWorld::new(Corrections::default());
    world.options.source_dir = world.options.previous_dir.clone();
    world.options.sha = world.options.previous_sha.clone();
    let before = world.bytes();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!(before, world.bytes());
}

#[test]
fn import_supersedes_wiki_only_item_without_a_conflict() {
    let world = ImportWorld::new(Corrections::default());
    let path = world.options.data_dir.join("packs/unsourced/gear/reviewed-trinket.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        r#"
name = "Reviewed Trinket"
is_wiki_only = true
slot = "Trinket"
item_category = "Jewelry"
minimum_level = 1
description = "Wiki description"
drop_location = "Unknown"

[[citation]]
url = "https://ddowiki.com/page/Item:Reviewed_Trinket"
read = "2026-10-18"
"#,
    )
    .unwrap();
    let provenance = |db: &Connection| -> String {
        db.query_row("SELECT provenance FROM items WHERE name = 'Reviewed Trinket'", [], |row| row.get(0)).unwrap()
    };
    assert_eq!(provenance(&world.database()), "wiki");
    let items = world.options.source_dir.join("Items");
    std::fs::create_dir_all(&items).unwrap();
    std::fs::write(items.join("Reviewed Trinket.item"), "<Items><Item><Name>Reviewed Trinket</Name><MinLevel>1</MinLevel><EquipmentSlot><Trinket/></EquipmentSlot><Description>His description</Description></Item></Items>").unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    let db = world.database();
    assert_eq!(provenance(&db), "maetrim");
    let citations: i64 = db.query_row("SELECT COUNT(*) FROM citations WHERE owner_kind = 'items' AND owner_id = (SELECT id FROM items WHERE name = 'Reviewed Trinket') AND field = ''", [], |row| row.get(0)).unwrap();
    assert_eq!(citations, 1);
    let path = world.options.data_dir.join("packs/unsourced/gear/reviewed-trinket.toml");
    assert!(!std::fs::read_to_string(path).unwrap().contains("is_wiki_only"));
}

#[test]
fn import_takes_his_value_on_a_field_we_did_not_change() {
    let world = ImportWorld::new(Corrections::default());
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!(world.cost("ATShockGauntlet"), "[1]");
}

#[test]
fn import_keeps_our_value_and_lists_the_conflict() {
    let world = ImportWorld::new(Corrections::default());
    world.edit("UPDATE enhancements SET cost_per_rank = '[3]' WHERE internal_name = 'SDDefenseBoost'");
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(report.has_conflicts());
    assert_eq!(world.cost("SDDefenseBoost"), "[3]");
    let text = report.to_string();
    for expected in
        ["CONFLICT", "characters/enhancements/stalwart-defender/tree.toml", "cost_per_rank", "[2]", "[1]", "[3]"]
    {
        assert!(text.contains(expected), "{text}");
    }
}

#[test]
fn import_takes_no_action_when_we_already_made_his_change() {
    let world = ImportWorld::new(Corrections::default());
    world.edit("UPDATE enhancements SET cost_per_rank = '[1]' WHERE internal_name = 'SDDefenseBoost'");
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!(world.cost("SDDefenseBoost"), "[1]");
}

#[test]
fn import_adds_his_new_file_where_the_layout_puts_it() {
    let world = ImportWorld::new(Corrections::default());
    let report = import_ddobuilder(&world.options).unwrap();
    assert_eq!(report.tables["filigrees"].added, 8);
    assert_eq!(report.tables["filigrees"].changed, 0);
    assert_eq!(report.tables["set_bonuses"].added, 1);
    assert_eq!(report.tables["set_bonuses"].changed, 0);
    assert!(report.to_string().contains("characters/enhancements/bombardier/tree.toml: REMOVED"));
    assert!(world.options.data_dir.join("packs/shared/filigrees/night-revels-gifts.toml").is_file());
    assert_eq!(
        std::fs::read(world.options.data_dir.join("icons/filigrees/NightRevelsGifts.png")).unwrap(),
        std::fs::read(world.options.source_dir.join("FiligreeImages/NightRevelsGifts.png")).unwrap()
    );
}

#[test]
fn import_refuses_a_previous_that_is_not_the_recorded_commit() {
    let mut world = ImportWorld::new(Corrections::default());
    world.options.previous_sha = Some("wrong".into());
    let before = world.bytes();
    let error = import_ddobuilder(&world.options).unwrap_err().to_string();
    assert!(error.contains("wrong") && error.contains(PREVIOUS_SHA), "{error}");
    assert_eq!(before, world.bytes());
}

fn relabelled_socket_import(keeps_upstream_label: bool) -> (tempfile::TempDir, DdobuilderImportOptions, Connection) {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let scratch = tempfile::tempdir().unwrap();
    let source = scratch.path().join("DataFiles");
    copy_tree(&fixtures.join("DataFiles"), &source);
    if keeps_upstream_label {
        let xml = std::fs::read_to_string(source.join("Items/Sireth, Spear of the Sky.item")).unwrap();
        std::fs::write(
            source.join("Items/UncorrectedCarrier.item"),
            xml.replace("Sireth, Spear of the Sky", "Uncorrected Carrier"),
        )
        .unwrap();
    }
    let mut corrections = fixture::fixture_icon_corrections();
    corrections.entries.extend(
        fixture::new_qualified_correction(
            "item_socket",
            "Sireth, Spear of the Sky",
            "option='Planar Conflux'",
            "label",
            "\"crafting: attuned to heroism 1\"",
            "\"crafting: attuned to heroism 2\"",
        )
        .0,
    );
    let mut written = Connection::open_in_memory().unwrap();
    build::build_database(
        fixture::groupings(),
        &source,
        &WikiOverrides::default(),
        &corrections,
        &mut written,
        &DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-11-11T00:00:00Z".into() },
    )
    .unwrap();
    let root = scratch.path().join("data");
    export_fixture_with_icons(&written, &source, &root);
    copy_data_icons_into_source(&root, &source, scratch.path());
    let options = DdobuilderImportOptions {
        previous_dir: source.clone(),
        source_dir: source,
        data_dir: root,
        previous_sha: Some(PREVIOUS_SHA.into()),
        sha: Some(PREVIOUS_SHA.into()),
    };
    (scratch, options, written)
}

#[test]
fn per_item_slot_types_exclude_uncorrected_upstream_carriers_but_allow_wiki_carriers() {
    let world = per_item_socket_import();
    let db = world.database();
    let assignments = WikiOverrides::from_database(&db).unwrap();
    assert_eq!(assignments.per_item_slot_types.len(), 1);
    assert_eq!(assignments.retired_socket_labels.into_iter().collect::<Vec<_>>(), ["crafting: attuned to heroism 1"]);
    let carrier: i64 = db
        .query_row(
            "SELECT id FROM items WHERE name<>'Sireth, Spear of the Sky' AND provenance<>'wiki' AND drop_location IS NOT NULL ORDER BY slug LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    db.execute(
        "INSERT INTO item_augment_slots(item_id,sort_order,slot_id) SELECT ?1,COALESCE((SELECT MAX(sort_order)+1 FROM item_augment_slots WHERE item_id=?1),0),id FROM augment_slot_types WHERE label='crafting: attuned by heroism: tier 1 (mikrom sum)'",
        [carrier],
    ).unwrap();
    tree::export_database(&db, &world.options.data_dir).unwrap();
    assert!(WikiOverrides::from_database(&world.database()).unwrap().per_item_slot_types.is_empty());
    world.edit(&format!("UPDATE items SET provenance='wiki' WHERE id={carrier}"));
    assert_eq!(WikiOverrides::from_database(&world.database()).unwrap().per_item_slot_types.len(), 1);
}

#[test]
fn a_per_item_slot_type_has_no_import_delta() {
    let world = per_item_socket_import();
    let before = world.bytes();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!((report.files.written, report.files.removed), (0, 0), "{report}");
    assert_eq!(world.bytes(), before);
    let label: String = world.database().query_row(
        "SELECT t.label FROM item_augment_slots s JOIN augment_slot_types t ON t.id=s.slot_id JOIN items i ON i.id=s.item_id WHERE i.name='Sireth, Spear of the Sky' AND s.sort_order=0",
        [], |row| row.get(0),
    ).unwrap();
    assert_eq!(label, "crafting: attuned by heroism: tier 1 (mikrom sum)");
}

fn per_item_socket_import() -> ImportWorld {
    let (scratch, mut options, written) = relabelled_socket_import(false);
    written.execute_batch(
        "BEGIN;
         PRAGMA defer_foreign_keys=ON;
         INSERT INTO augment_slot_types(label,family,variant,slug)
         VALUES ('crafting: attuned by heroism: tier 1 (mikrom sum)','crafting','attuned by heroism: tier 1 (mikrom sum)','crafting-attuned-by-heroism-tier-1-mikrom-sum');
         UPDATE socket_option_sets SET slot_id=(SELECT id FROM augment_slot_types WHERE slug='crafting-attuned-by-heroism-tier-1-mikrom-sum')
         WHERE id=(SELECT s.option_set_id FROM item_augment_slots s JOIN items i ON i.id=s.item_id WHERE i.name='Sireth, Spear of the Sky' AND s.sort_order=0);
         UPDATE item_augment_slots SET slot_id=(SELECT id FROM augment_slot_types WHERE slug='crafting-attuned-by-heroism-tier-1-mikrom-sum')
         WHERE item_id=(SELECT id FROM items WHERE name='Sireth, Spear of the Sky') AND sort_order=0;
         DELETE FROM augment_slot_types WHERE label='crafting: attuned to heroism 1' AND NOT EXISTS (SELECT 1 FROM item_augment_slots WHERE slot_id=augment_slot_types.id);
         COMMIT;",
    ).unwrap();
    written
        .execute(
            "UPDATE corrections SET to_value=?1 WHERE kind='item_socket' AND field='label'",
            [serde_json::to_string("crafting: attuned by heroism: tier 1 (mikrom sum)").unwrap()],
        )
        .unwrap();
    tree::export_database(&written, &options.data_dir).unwrap();
    let new_source = scratch.path().join("new");
    copy_tree(&options.source_dir, &new_source);
    options.source_dir = new_source;
    ImportWorld { scratch, options }
}

fn add_uncorrected_socket_carrier(world: &ImportWorld, name: &str) {
    let source = world.options.source_dir.join("Items/Sireth, Spear of the Sky.item");
    let xml = std::fs::read_to_string(source).unwrap();
    std::fs::write(
        world.options.source_dir.join(format!("Items/{name}.item")),
        xml.replace("Sireth, Spear of the Sky", name),
    )
    .unwrap();
}

#[test]
fn retired_socket_labels_report_every_new_carrier_after_writing() {
    let mut world = per_item_socket_import();
    world.options.sha = Some(NEW_SHA.into());
    let before = world.bytes();
    let names = ["First Uncorrected Carrier", "Second Uncorrected Carrier"];
    for name in names {
        add_uncorrected_socket_carrier(&world, name);
    }
    let output = cli(&world).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(2), "{stdout}\n{stderr}");
    assert!(!stdout.contains(": CONFLICT "), "{stdout}");
    let db = world.database();
    let paths = tree::DataFilePaths::from_database(&db).unwrap();
    let mut expected_files = std::collections::BTreeSet::from([
        std::path::PathBuf::from("upstream/commit.toml"),
        std::path::PathBuf::from("effects/augment-slot-types.toml"),
    ]);
    for name in names {
        let line = stdout
            .lines()
            .find(|line| line.contains("NEEDS SLOT TYPE") && line.contains(name))
            .unwrap_or_else(|| panic!("{stdout}"));
        for expected in ["crafting: attuned to heroism 1", "item_socket", "recipe", "source"] {
            assert!(line.contains(expected), "{line}");
        }
        let label: String = db.query_row("SELECT t.label FROM item_augment_slots s JOIN items i ON i.id=s.item_id JOIN augment_slot_types t ON t.id=s.slot_id WHERE i.name=?1 AND s.sort_order=0", [name], |row| row.get(0)).unwrap();
        assert_eq!(label, "crafting: attuned to heroism 1");
        let (item, options): (i64, i64) = db.query_row(
            "SELECT i.id,s.option_set_id FROM items i JOIN item_augment_slots s ON s.item_id=i.id WHERE i.name=?1 AND s.sort_order=0",
            [name], |row| Ok((row.get(0)?,row.get(1)?)),
        ).unwrap();
        expected_files.insert(paths.path("items", item).unwrap());
        expected_files.insert(paths.path("socket_option_sets", options).unwrap());
    }
    let after = world.bytes();
    let written_files = after
        .iter()
        .filter(|(path, contents)| before.get(*path) != Some(*contents))
        .map(|(path, _)| path.clone())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(written_files, expected_files);
    let written = written_files.len();
    let removed = before.keys().filter(|path| !after.contains_key(*path)).count();
    assert!(stdout.contains(&format!("files: {written} written, {removed} removed")), "{stdout}");
    assert_eq!(removed, 0);
}

#[test]
fn a_renamed_option_keeps_the_split_socket_and_reports_each_conflict() {
    let mut world = per_item_socket_import();
    world.options.sha = Some(NEW_SHA.into());
    let original_db = world.database();
    let item_id: i64 = original_db
        .query_row("SELECT id FROM items WHERE name='Sireth, Spear of the Sky'", [], |row| row.get(0))
        .unwrap();
    let item_path = tree::DataFilePaths::from_database(&original_db).unwrap().path("items", item_id).unwrap();
    let original = std::fs::read(world.options.data_dir.join(&item_path)).unwrap();
    let source = world.options.source_dir.join("Items/Sireth, Spear of the Sky.item");
    let xml = std::fs::read_to_string(&source).unwrap();
    std::fs::write(source, xml.replace("<Name>Planar Conflux</Name>", "<Name>Replacement option</Name>")).unwrap();
    for pass in 0..2 {
        let before = world.bytes();
        let output = cli(&world).output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(2), "{stdout}\n{stderr}");
        let conflicts = stdout.lines().filter(|line| line.contains(": CONFLICT ")).collect::<Vec<_>>();
        assert_eq!(conflicts.len(), 2, "{stdout}");
        assert!(conflicts.iter().any(|line| line.contains("item_socket \"Sireth, Spear of the Sky\"")), "{stdout}");
        assert!(conflicts.iter().any(|line| line.contains("items/sireth-spear-of-the-sky socket")), "{stdout}");
        for conflict in conflicts {
            for label in ["previous=", "new=", "ours="] {
                assert!(conflict.contains(label), "{conflict}");
            }
        }
        assert_eq!(stdout.contains("DECLINED REMOVAL"), pass == 0, "{stdout}");
        assert_eq!(std::fs::read(world.options.data_dir.join(&item_path)).unwrap(), original, "{stdout}");
        let db = world.database();
        let socket: (String, String) = db.query_row(
            "SELECT t.label,o.name FROM item_augment_slots s JOIN items i ON i.id=s.item_id JOIN augment_slot_types t ON t.id=s.slot_id JOIN socket_options o ON o.set_id=s.option_set_id WHERE i.name='Sireth, Spear of the Sky' AND s.sort_order=0",
            [], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(socket, ("crafting: attuned by heroism: tier 1 (mikrom sum)".into(), "Planar Conflux".into()));
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM augment_slot_types WHERE label='crafting: attuned to heroism 1'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM socket_options WHERE name='Replacement option'", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        if pass == 1 {
            assert_eq!(world.bytes(), before);
            assert!(stdout.contains("files: 0 written, 0 removed"), "{stdout}");
        }
        world.options.previous_dir = world.options.source_dir.clone();
        world.options.previous_sha = world.options.sha.clone();
    }
}

#[test]
fn retired_socket_conflicts_report_new_carriers_and_keep_their_sockets() {
    let mut world = per_item_socket_import();
    world.options.sha = Some(NEW_SHA.into());
    for name in ["First Uncorrected Carrier", "Second Uncorrected Carrier"] {
        add_uncorrected_socket_carrier(&world, name);
    }
    let source = world.options.source_dir.join("Items/Sireth, Spear of the Sky.item");
    let xml = std::fs::read_to_string(&source).unwrap();
    std::fs::write(source, xml.replace("<Name>Planar Conflux</Name>", "<Name>Replacement option</Name>")).unwrap();
    let output = cli(&world).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(2), "{stdout}\n{stderr}");
    assert_eq!(stdout.lines().filter(|line| line.contains(": CONFLICT ")).count(), 2, "{stdout}");
    assert_eq!(stdout.lines().filter(|line| line.contains(": NEEDS SLOT TYPE ")).count(), 2, "{stdout}");
    let db = world.database();
    let carriers = db.prepare("SELECT i.name FROM item_augment_slots s JOIN items i ON i.id=s.item_id JOIN augment_slot_types t ON t.id=s.slot_id WHERE t.label='crafting: attuned to heroism 1' ORDER BY i.name").unwrap()
        .query_map([], |row| row.get::<_,String>(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(carriers, ["First Uncorrected Carrier", "Second Uncorrected Carrier"]);
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM socket_options WHERE name='Replacement option'", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(db.query_row("SELECT COUNT(*) FROM socket_options o JOIN item_augment_slots s ON s.option_set_id=o.set_id JOIN items i ON i.id=s.item_id WHERE i.name='Sireth, Spear of the Sky' AND o.name='Planar Conflux'", [], |row| row.get::<_,i64>(0)).unwrap(), 1);
}

#[test]
fn a_relabelled_socket_has_no_import_delta() {
    let (_scratch, options, written) = relabelled_socket_import(false);
    let sockets = |db: &Connection| {
        db.prepare(
        "SELECT a.sort_order,t.label,s.slug FROM item_augment_slots a JOIN items i ON i.id=a.item_id JOIN augment_slot_types t ON t.id=a.slot_id LEFT JOIN socket_option_sets s ON s.id=a.option_set_id WHERE i.name='Sireth, Spear of the Sky' ORDER BY a.sort_order"
    ).unwrap().query_map([], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?))).unwrap().collect::<Result<Vec<_>,_>>().unwrap()
    };
    let expected = sockets(&written);
    assert_eq!(expected[0].1, "crafting: attuned to heroism 2");
    for _ in 0..2 {
        let report = import_ddobuilder(&options).unwrap();
        assert!(!report.has_conflicts(), "{report}");
        assert_eq!((report.files.written, report.files.removed), (0, 0), "{report}");
        let mut rebuilt = Connection::open_in_memory().unwrap();
        tree::build_from_data(&options.data_dir, &mut rebuilt).unwrap();
        assert_eq!(sockets(&rebuilt), expected);
    }
}

#[test]
fn a_relabelled_socket_with_a_renamed_option_has_no_import_delta() {
    let scratch = tempfile::tempdir().unwrap();
    let source = scratch.path().join("DataFiles");
    copy_tree(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles"), &source);
    let written = fixture::renamed_socket_option_database(
        &source,
        &DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-11-11T00:00:00Z".into() },
    );
    let root = scratch.path().join("data");
    export_fixture_with_icons(&written, &source, &root);
    copy_data_icons_into_source(&root, &source, scratch.path());
    let world = ImportWorld {
        scratch,
        options: DdobuilderImportOptions {
            previous_dir: source.clone(),
            source_dir: source,
            data_dir: root,
            previous_sha: Some(PREVIOUS_SHA.into()),
            sha: Some(PREVIOUS_SHA.into()),
        },
    };
    let before = world.bytes();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!((report.files.written, report.files.removed), (0, 0), "{report}");
    assert!(!report.to_string().contains("STALE"), "{report}");
    assert_eq!(world.bytes(), before);
}

#[test]
fn a_socket_relabel_import_keeps_the_missing_target_correction_and_his_replacement() {
    let (scratch, mut options, _) = relabelled_socket_import(true);
    let changed = scratch.path().join("changed");
    copy_tree(&options.source_dir, &changed);
    let item = changed.join("Items/Sireth, Spear of the Sky.item");
    let xml = std::fs::read_to_string(&item).unwrap();
    std::fs::write(item, xml.replace("<Name>Planar Conflux</Name>", "<Name>Upstream replacement option</Name>"))
        .unwrap();
    options.source_dir = changed;
    options.sha = Some(NEW_SHA.into());
    let report = import_ddobuilder(&options).unwrap();
    assert!(report.has_conflicts(), "{report}");
    let text = report.to_string();
    let conflicts = text.lines().filter(|line| line.contains(": CONFLICT ")).collect::<Vec<_>>();
    assert_eq!(conflicts.len(), 1, "{report}");
    for expected in ["item_socket", "Sireth, Spear of the Sky", "label", "no such bonus"] {
        assert!(conflicts[0].contains(expected), "{report}");
    }
    for pass in 0..2 {
        if pass == 1 {
            assert_missing_targets_repeat_cleanly(&mut options);
        }
        let mut rebuilt = Connection::open_in_memory().unwrap();
        tree::build_from_data(&options.data_dir, &mut rebuilt).unwrap();
        let upstream: String = rebuilt.query_row(
            "SELECT from_value FROM corrections WHERE kind='item_socket' AND name='Sireth, Spear of the Sky' AND field='label'",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(upstream, r#""no such bonus""#);
        let missing = ddo_etl::corrections::corrections_recording_missing_targets(&rebuilt).unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(
            (
                missing[0].kind.as_str(),
                missing[0].name.as_str(),
                missing[0].qualifier.as_str(),
                missing[0].field.as_str()
            ),
            ("item_socket", "Sireth, Spear of the Sky", r#"option "Planar Conflux""#, "label")
        );
        let correction_id: i64 = rebuilt.query_row("SELECT id FROM corrections WHERE kind='item_socket' AND name='Sireth, Spear of the Sky' AND field='label'", [], |row| row.get(0)).unwrap();
        assert_eq!(missing[0].id, correction_id);

        let socket: (String,String) = rebuilt.query_row(
            "SELECT t.label,o.name FROM item_augment_slots a JOIN items i ON i.id=a.item_id JOIN augment_slot_types t ON t.id=a.slot_id JOIN socket_options o ON o.set_id=a.option_set_id WHERE i.name='Sireth, Spear of the Sky' AND a.sort_order=0",
            [], |row| Ok((row.get(0)?,row.get(1)?)),
        ).unwrap();
        assert_eq!(socket, ("crafting: attuned to heroism 1".into(), "Upstream replacement option".into()));
    }
}

#[test]
fn import_twice_changes_nothing_the_second_time() {
    let mut world = ImportWorld::new(Corrections::default());
    import_ddobuilder(&world.options).unwrap();
    world.options.previous_dir = world.options.source_dir.clone();
    world.options.previous_sha = world.options.sha.clone();
    let before = world.bytes();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!(before, world.bytes());
}

#[test]
fn augment_family_rename_preserves_the_upstream_correction_name() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let source = fixtures.join("DataFiles");
    let wiki = WikiOverrides::from_database(&fixture::database(|_| {})).unwrap();
    let mut corrections = fixture::fixture_icon_corrections();
    let version = DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-18T00:00:00Z".into() };
    let mut baseline = Connection::open_in_memory().unwrap();
    build::build_database(fixture::groupings(), &source, &wiki, &corrections, &mut baseline, &version).unwrap();
    let (name, family): (String, String) = baseline
        .query_row("SELECT name, family FROM augments ORDER BY id LIMIT 1", [], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap();
    let renamed = format!("{name} reviewed");
    let rename = fixture::new_correction("augment", &name, "name", &format!("{name:?}"), &format!("{renamed:?}"))
        + format!("family = {family:?}").as_str();
    corrections
        .entries
        .extend(rename.cited("Reviewed family rename.", "https://ddowiki.com/page/Test", "2026-10-18").0);
    let mut corrected = Connection::open_in_memory().unwrap();
    build::build_database(fixture::groupings(), &source, &wiki, &corrections, &mut corrected, &version).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    export_fixture_with_icons(&corrected, &source, scratch.path());
    let mut rebuilt = Connection::open_in_memory().unwrap();
    tree::build_from_data(scratch.path(), &mut rebuilt).unwrap();
    let recorded_name: String = rebuilt
        .query_row("SELECT name FROM corrections WHERE reason = 'Reviewed family rename.'", [], |row| row.get(0))
        .unwrap();
    assert_eq!(recorded_name, name);
}

#[test]
fn selection_and_parent_renames_keep_the_upstream_parent_in_both_builders() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let source = fixtures.join("DataFiles");
    let wiki = WikiOverrides::from_database(&fixture::database(|_| {})).unwrap();
    let version = DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-18T00:00:00Z".into() };
    for parent_first in [true, false] {
        let mut corrections = fixture::fixture_icon_corrections();
        let mut renames = [
            ("", "Kensei: Action Boost", "Kensei: Action Boost Reviewed"),
            ("selection = \"Attack Boost\"\n", "Attack Boost", "Attack Boost Reviewed"),
        ];
        if !parent_first {
            renames.reverse();
        }
        for (qualifier, before, after) in renames {
            let rename = fixture::new_correction(
                "enhancement",
                "Kensei: Action Boost",
                "name",
                &format!("{before:?}"),
                &format!("{after:?}"),
            ) + qualifier;
            corrections.entries.extend(rename.cited("Paired rename.", "https://ddowiki.com/page/Test", "2026-10-18").0);
        }
        let mut corrected = Connection::open_in_memory().unwrap();
        let report =
            build::build_database(fixture::groupings(), &source, &wiki, &corrections, &mut corrected, &version)
                .unwrap();
        assert!(report.stale_corrections.is_empty());
        let scratch = tempfile::tempdir().unwrap();
        export_fixture_with_icons(&corrected, &source, scratch.path());
        let mut rebuilt = Connection::open_in_memory().unwrap();
        tree::build_from_data(scratch.path(), &mut rebuilt).unwrap();
        for db in [&corrected, &rebuilt] {
            let mut query = db
                .prepare("SELECT name, qualifier FROM corrections WHERE reason = 'Paired rename.' ORDER BY qualifier")
                .unwrap();
            let keys = query
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(
                keys,
                vec![
                    ("Kensei: Action Boost".into(), "".into()),
                    ("Kensei: Action Boost".into(), "selection \"Attack Boost\"".into())
                ]
            );
            let names: (String, String) = db.query_row("SELECT e.name, s.name FROM enhancements e JOIN enhancement_selections s ON s.enhancement_id = e.id WHERE s.name = 'Attack Boost Reviewed'", [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
            assert_eq!(names, ("Kensei: Action Boost Reviewed".into(), "Attack Boost Reviewed".into()));
        }
    }
}

#[test]
fn every_declared_enhancement_field_agrees_between_builders() {
    use ddo_etl::corrections::{correctable_fields, CorrectionValue};
    use ddo_model::enums::CorrectionKind;
    let expected = [
        "arrows",
        "cooldown_seconds",
        "cost_per_rank",
        "description",
        "duration_seconds",
        "icon",
        "is_clickie",
        "is_tier5",
        "min_spent",
        "name",
        "ranks",
        "x",
        "y",
    ];
    let mut declared =
        correctable_fields(CorrectionKind::Enhancement).iter().map(|field| field.name).collect::<Vec<_>>();
    declared.sort();
    assert_eq!(declared, expected);
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let source = fixtures.join("DataFiles");
    let wiki = WikiOverrides::from_database(&fixture::database(|_| {})).unwrap();
    let baseline_corrections = fixture::fixture_icon_corrections();
    let version = DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-18T00:00:00Z".into() };
    let mut baseline = Connection::open_in_memory().unwrap();
    build::build_database(fixture::groupings(), &source, &wiki, &baseline_corrections, &mut baseline, &version)
        .unwrap();
    for selection in [false, true] {
        for field in expected {
            for empty_list in [false, true] {
                if empty_list && !["cost_per_rank", "arrows"].contains(&field) {
                    continue;
                }
                if selection && ["arrows", "is_tier5", "x", "y"].contains(&field) {
                    continue;
                }
                let (table, row_name) = if selection {
                    ("enhancement_selections", "Attack Boost")
                } else {
                    ("enhancements", "Kensei: Action Boost")
                };
                let original: rusqlite::types::Value = baseline
                    .query_row(&format!("SELECT {field} FROM {table} WHERE name = ?1"), [row_name], |row| row.get(0))
                    .unwrap();
                let original = match original {
                    rusqlite::types::Value::Text(encoded) if ["cost_per_rank", "arrows"].contains(&field) => {
                        CorrectionValue::from_json(&encoded).unwrap()
                    }
                    value => CorrectionValue::from_sql(value),
                };
                let mut replacement = match field {
                    "name" => serde_json::json!(format!("{row_name} reviewed")),
                    "description" => serde_json::json!("Reviewed enhancement description."),
                    "icon" => serde_json::json!("PowerAttack"),
                    "arrows" => serde_json::json!(["ArrowUp", "ArrowRight"]),
                    "cost_per_rank" => serde_json::json!([2, 3]),
                    "is_clickie" => serde_json::json!(0),
                    "is_tier5" => serde_json::json!(1),
                    _ => serde_json::json!(7),
                };
                if empty_list {
                    replacement = serde_json::json!([]);
                }
                let from = if original == CorrectionValue::Null { "\"null\"".into() } else { original.to_json() };
                let qualifier = if selection { "selection = \"Attack Boost\"\n" } else { "" };
                let correction = fixture::new_correction(
                    "enhancement",
                    "Kensei: Action Boost",
                    field,
                    &from,
                    &replacement.to_string(),
                ) + qualifier;
                let mut corrections = baseline_corrections.clone();
                corrections
                    .entries
                    .extend(correction.cited("Reviewed in test.", "https://ddowiki.com/page/Test", "2026-10-18").0);
                if selection && field == "name" {
                    let sibling =
                        fixture::new_correction("enhancement", "Kensei: Action Boost", "cost_per_rank", "[1]", "[4]")
                            + "selection = \"Attack Boost\"\n";
                    corrections.entries.extend(
                        sibling.cited("Sibling of renamed selection.", "https://ddowiki.com/page/Test", "2026-10-18").0,
                    );
                }
                let mut corrected = Connection::open_in_memory().unwrap();
                build::build_database(fixture::groupings(), &source, &wiki, &corrections, &mut corrected, &version)
                    .unwrap_or_else(|error| panic!("{table}.{field}: {error:#}"));
                let scratch = tempfile::tempdir().unwrap();
                export_fixture_with_icons(&corrected, &source, scratch.path());
                let mut rebuilt = Connection::open_in_memory().unwrap();
                tree::build_from_data(scratch.path(), &mut rebuilt)
                    .unwrap_or_else(|error| panic!("{table}.{field}: {error:#}"));
                for db in [&corrected, &rebuilt] {
                    let actual: String = db.query_row(
                    "SELECT to_value FROM corrections WHERE kind = 'enhancement' AND field = ?1 AND reason = 'Reviewed in test.'",
                    [field], |row| row.get(0),
                ).unwrap_or_else(|error| panic!("{table}.{field}: {error}"));
                    assert_eq!(actual, replacement.to_string(), "{table}.{field}");
                    if selection && field == "name" {
                        let sibling: (String, String) = db.query_row(
                        "SELECT qualifier, to_value FROM corrections WHERE reason = 'Sibling of renamed selection.'",
                        [], |row| Ok((row.get(0)?, row.get(1)?)),
                    ).unwrap();
                        assert_eq!(sibling, ("selection \"Attack Boost\"".into(), "[4]".into()));
                    }
                }
            }
        }
    }
}

#[test]
fn enhancement_notes_reject_identity_fields_and_fractional_upstream_costs() {
    for (field, upstream, expected) in [
        ("internal_name", toml::Value::String("OldIdentity".into()), "allowed fields"),
        ("cost_per_rank", toml::Value::Array(vec![toml::Value::Float(1.5)]), "integer"),
    ] {
        let world = ImportWorld::new(Corrections::default());
        let path = world.options.data_dir.join("characters/enhancements/ravager-fighter/tree.toml");
        let mut tree: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let note = toml::Table::from_iter([
            ("kind".into(), toml::Value::String("enhancement".into())),
            ("field".into(), toml::Value::String(field.into())),
            ("upstream".into(), upstream),
            ("reason".into(), toml::Value::String("Invalid correction test.".into())),
            ("url".into(), toml::Value::String("https://ddowiki.com/page/Test".into())),
            ("read".into(), toml::Value::String("2026-10-18".into())),
        ]);
        tree["enhancement"][0]
            .as_table_mut()
            .unwrap()
            .insert("ddobuilder_correction".into(), toml::Value::Array(vec![toml::Value::Table(note)]));
        std::fs::write(&path, toml::to_string_pretty(&tree).unwrap()).unwrap();
        let error =
            tree::build_from_data(&world.options.data_dir, &mut Connection::open_in_memory().unwrap()).unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("ravager-fighter/tree.toml") && message.contains(expected), "{message}");
    }
}

#[test]
fn annotation_sources_and_correction_kinds_are_typed() {
    let source_shape = serde_json::json!([
        {"required": ["url"], "not": {"required": ["in_game"]}},
        {"required": ["in_game"], "not": {"required": ["url"]}},
    ]);
    for schema in [
        serde_json::to_value(schemars::schema_for!(tree::Citation)).unwrap(),
        serde_json::to_value(schemars::schema_for!(tree::DdobuilderCorrection)).unwrap(),
    ] {
        assert_eq!(schema["oneOf"], source_shape);
        assert_eq!(schema["properties"]["in_game"]["const"], true);
    }
    let schema = serde_json::to_value(schemars::schema_for!(tree::DdobuilderCorrection)).unwrap();
    let expected_kinds = ddo_model::enums::CorrectionKind::ALL.iter().map(|kind| kind.as_str()).collect::<Vec<_>>();
    assert_eq!(schema["$defs"]["CorrectionKind"]["enum"], serde_json::json!(expected_kinds));
    for source in ["url = \"https://ddowiki.com/page/Test\"", "in_game = true"] {
        let text = format!("{source}\nread = \"2026-10-18\"\nnote = \"Observed cost.\"\n");
        let citation: tree::Citation = toml::from_str(&text).unwrap();
        let encoded = toml::to_string(&citation).unwrap();
        let decoded: tree::Citation = toml::from_str(&encoded).unwrap();
        assert_eq!(toml::to_string(&decoded).unwrap(), encoded);
        let text = format!("kind = \"enhancement\"\nfield = \"cost_per_rank\"\nreason = \"Observed cost.\"\n{source}\nread = \"2026-10-18\"\n");
        let correction: tree::DdobuilderCorrection = toml::from_str(&text).unwrap();
        let encoded = toml::to_string(&correction).unwrap();
        assert_eq!(toml::to_string(&toml::from_str::<tree::DdobuilderCorrection>(&encoded).unwrap()).unwrap(), encoded);
        for kind in ["release", "slug_collision"] {
            assert!(toml::from_str::<tree::DdobuilderCorrection>(
                &text.replace("\"enhancement\"", &format!("{kind:?}"))
            )
            .is_err());
        }
    }
    for source in ["", "in_game = false", "url = \"https://ddowiki.com/page/Test\"\nin_game = true"] {
        assert!(toml::from_str::<tree::Citation>(&format!("{source}\nread = \"2026-10-18\"\n")).is_err());
        for (array, fields) in [
            ("citation", ""),
            (
                "enhancement.ddobuilder_correction",
                "kind = \"enhancement\"\nfield = \"cost_per_rank\"\nreason = \"Observed cost.\"\nupstream = [2]\n",
            ),
        ] {
            let world = ImportWorld::new(Corrections::default());
            let path = world.options.data_dir.join("characters/enhancements/ravager-fighter/tree.toml");
            let original = std::fs::read_to_string(&path).unwrap();
            std::fs::write(&path, format!("{original}\n[[{array}]]\n{fields}{source}\nread = \"2026-10-18\"\n"))
                .unwrap();
            let error =
                tree::build_from_data(&world.options.data_dir, &mut Connection::open_in_memory().unwrap()).unwrap_err();
            assert!(format!("{error:#}").contains("ravager-fighter/tree.toml"), "{error:#}");
        }
        assert!(toml::from_str::<tree::DdobuilderCorrection>(&format!("kind = \"enhancement\"\nfield = \"cost_per_rank\"\nreason = \"Observed cost.\"\n{source}\nread = \"2026-10-18\"\n")).is_err());
    }
}

#[test]
fn import_reports_enhancement_cost_resolution_conflict_and_unchanged_values() {
    let cost_correction =
        fixture::new_correction("enhancement", "Ravager: Draconic Power Attack", "cost_per_rank", "[1]", "[2]").cited(
            "Reviewed enhancement cost.",
            "https://ddowiki.com/page/Test",
            "2026-10-18",
        );
    for in_game in [false, true] {
        let mut correction = cost_correction.clone();
        if in_game {
            correction.0[0].source = ddo_etl::AnnotationSource::InGame;
        }
        for (new_cost, expected) in [(1, None), (2, Some("RESOLVED CORRECTION")), (3, Some("CONFLICT"))] {
            let world = ImportWorld::new(fixture::corrections_from(&[("cost.toml", &correction)]).unwrap());
            let path = world.options.source_dir.join("EnhancementTrees/Fighter_Ravager.tree.xml");
            let xml = std::fs::read_to_string(&path).unwrap();
            assert!(xml.contains("<CostPerRank size=\"1\">1</CostPerRank>"));
            std::fs::write(
                path,
                xml.replace(
                    "<CostPerRank size=\"1\">1</CostPerRank>",
                    &format!("<CostPerRank size=\"1\">{new_cost}</CostPerRank>"),
                ),
            )
            .unwrap();
            let report = import_ddobuilder(&world.options).unwrap().to_string();
            let lines = report.lines().filter(|line| line.contains("cost_per_rank")).collect::<Vec<_>>();
            if let Some(expected) = expected {
                assert!(lines.iter().any(|line| line.contains(expected)), "{new_cost}: {report}");
            } else {
                assert!(lines.is_empty(), "{report}");
            }
            let cost: String = world
                .database()
                .query_row(
                    "SELECT cost_per_rank FROM enhancements WHERE name = 'Ravager: Draconic Power Attack'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(cost, "[2]");
        }
    }
}

fn ravager_corrections() -> Corrections {
    let page = "https://ddowiki.com/page/Ravager_enhancements";
    let description = |name: &str, from: &str, to: &str| {
        fixture::correction_between(
            "enhancement",
            name,
            "description",
            CorrectionValue::Text(from.into()),
            CorrectionValue::Text(to.into()),
        )
        .cited("Fixture wiki research.", page, "2026-10-17")
    };
    fixture::corrections_from(&[(
        "ravager.toml",
        &(description(
            "Ravager: Draconic Power Attack",
            "Your Power Attack feat does [1/2/3] additional point(s) of bonus damage.\nNote: Stacks with Improved Power Attack. Damage is doubled when Two Handed\nFighting or fighting in an animal form.",
            "Your Power Attack feat does 3 additional point(s) of bonus damage.\nNote: Stacks with Improved Power Attack. Damage is doubled when Two Handed\nFighting or fighting in an animal form.",
        ) + &description(
            "Ravager: Barbarian Power Attack",
            "Your Power Attack feat does [1/2/3] additional point(s) of bonus damage.\nDamage is doubled when Two Handed Fighting or fighting in an animal form..",
            "Our researched Barbarian Power Attack description.",
        )),
    )])
    .unwrap()
}

const DRACONIC_TREE: &str = "EnhancementTrees/Fighter_Ravager.tree.xml";

fn blank_draconic_description(xml_root: &Path) {
    let path = xml_root.join(DRACONIC_TREE);
    let xml = std::fs::read_to_string(&path).unwrap();
    let start = xml.find("<Description>Your Power Attack feat does").unwrap();
    let end = start + xml[start..].find("</Description>").unwrap() + "</Description>".len();
    std::fs::write(path, format!("{}<Description></Description>{}", &xml[..start], &xml[end..])).unwrap();
}

fn draconic_description(world: &ImportWorld) -> Option<String> {
    world
        .database()
        .query_row("SELECT description FROM enhancements WHERE internal_name = 'DLRavDraconicPowerAttack'", [], |row| {
            row.get(0)
        })
        .unwrap()
}

#[test]
fn his_blank_never_erases_ours_and_the_import_says_so() {
    let world = ImportWorld::new(Corrections::default());
    let before = draconic_description(&world).unwrap();
    assert!(before.contains("[1/2/3]"));
    blank_draconic_description(&world.options.source_dir);
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    let text = report.to_string();
    assert!(
        text.lines().any(|line| line.contains("BLANKED UPSTREAM, OURS KEPT")
            && line.contains("ravager-fighter/tree.toml")
            && line.contains("description")),
        "{text}"
    );
    assert_eq!(draconic_description(&world).as_deref(), Some(before.as_str()));
}

#[test]
fn his_added_description_applies_where_ours_had_filled_a_blank() {
    let world = ImportWorld::new(Corrections::default());
    blank_draconic_description(&world.options.previous_dir);
    world.edit(
        "UPDATE enhancements SET description = 'Our researched text.' WHERE internal_name = 'DLRavDraconicPowerAttack'",
    );
    assert_eq!(draconic_description(&world).as_deref(), Some("Our researched text."));
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert!(!report.to_string().contains("BLANKED UPSTREAM"), "{report}");
    assert!(draconic_description(&world).unwrap().contains("does 3 additional point(s)"));
}

fn remove_quest(xml_root: &Path, quest: &str) {
    let path = xml_root.join("Quests.xml");
    let xml = std::fs::read_to_string(&path).unwrap();
    let name = xml.find(&format!("<Name>{quest}</Name>")).unwrap();
    let start = xml[..name].rfind("<Quest>").unwrap();
    let end = name + xml[name..].find("</Quest>").unwrap() + "</Quest>".len();
    std::fs::write(path, format!("{}{}", &xml[..start], &xml[end..])).unwrap();
}

#[test]
fn a_quest_he_adds_that_we_already_hold_keeps_the_facts_only_we_carry() {
    let world = ImportWorld::new(Corrections::default());
    let quest = "A Long Way to Mushrooms";
    remove_quest(&world.options.previous_dir, quest);
    world.edit(&format!(
        "UPDATE quests SET is_free_to_play = 1, legendary_level = 31, zone = 'Our zone', bestowed_by = 'Our giver', flagging = 'Our flagging' WHERE name = '{quest}'"
    ));
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert!(!report.to_string().contains("BLANKED UPSTREAM"), "{report}");
    let facts: (i64, Option<i64>, Option<String>, Option<String>, Option<String>) = world
        .database()
        .query_row(
            "SELECT is_free_to_play, legendary_level, zone, bestowed_by, flagging FROM quests WHERE name = ?1",
            [quest],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .unwrap();
    assert_eq!(facts, (1, Some(31), Some("Our zone".into()), Some("Our giver".into()), Some("Our flagging".into())));
}

#[test]
fn title_omission_declarations_remain_ours_through_import() {
    let world =
        ImportWorld::with_sources(fixture::groupings(), Corrections::default(), WikiOverrides::default(), |source| {
            let captured = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles");
            std::fs::copy(captured.join("ItemBuffs.xml"), source.join("ItemBuffs.xml")).unwrap();
            std::fs::create_dir_all(source.join("Items")).unwrap();
            std::fs::copy(
                captured.join("Items/Grudgebearer's Plate.item"),
                source.join("Items/GrudgebearersPlate.item"),
            )
            .unwrap();
            std::fs::create_dir_all(source.join("ItemImages/Armor_Heavy")).unwrap();
            std::fs::write(
                source.join("ItemImages/Armor_Heavy/HeavyArmor_1a.png"),
                include_bytes!("fixtures/effect-groups/icons/HeavyArmor_1a.png"),
            )
            .unwrap();
        });
    world.edit("UPDATE effects SET omits_bonus_type=1 WHERE name='Command'; INSERT INTO effect_aliases (effect_id,name,slug,verbose_name_template,omits_bonus_type,sort_order) SELECT id,'Quiet Command','quiet-command','Quiet Command +{1}',1,0 FROM effects WHERE name='Command';");
    let normalized = ddo_etl::import_ddobuilder::normalized_upstream_database(
        &world.options.source_dir,
        &world.database(),
        &DatasetVersion { upstream_sha: NEW_SHA.into(), built_at: "2026-10-28T00:00:00Z".into() },
        false,
        &world.scratch.path().join("marker-normalized"),
    )
    .unwrap();
    assert!(normalized
        .db
        .query_row("SELECT omits_bonus_type FROM effects WHERE name='Command'", [], |row| row.get::<_, bool>(0))
        .unwrap());
    assert!(ddo_etl::delta::OURS_ONLY_COLUMNS.contains(&("effect_aliases", "omits_bonus_type")));
    assert!(normalized
        .db
        .query_row("SELECT omits_bonus_type FROM effect_aliases WHERE name='Quiet Command'", [], |row| row
            .get::<_, bool>(0))
        .unwrap());
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    let db = world.database();
    let markers: (bool,bool) = db.query_row("SELECT e.omits_bonus_type,a.omits_bonus_type FROM effects e JOIN effect_aliases a ON a.effect_id=e.id WHERE e.name='Command'", [], |row| Ok((row.get(0)?,row.get(1)?))).unwrap();
    assert_eq!(markers, (true, true));
    let before = tree::data_file_bytes(&world.options.data_dir).unwrap();
    tree::export_database(&db, &world.options.data_dir).unwrap();
    assert_eq!(before, tree::data_file_bytes(&world.options.data_dir).unwrap());
}

#[test]
fn his_xml_never_carries_a_column_declared_ours_only() {
    let mut his = Connection::open_in_memory().unwrap();
    build::build_database(
        fixture::groupings(),
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles"),
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut his,
        &DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-18T00:00:00Z".into() },
    )
    .unwrap();
    for (table, column) in ddo_etl::delta::OURS_ONLY_COLUMNS {
        let rows: i64 = his.query_row(&format!("SELECT COUNT(*) FROM \"{table}\""), [], |row| row.get(0)).unwrap();
        let carried: i64 = his
            .query_row(
                &format!("SELECT COUNT(*) FROM \"{table}\" WHERE \"{column}\" IS NOT NULL AND \"{column}\" <> 0"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(rows > 0, "{table}");
        assert_eq!(carried, 0, "{table}.{column} is filled by his XML build");
    }
}

#[test]
fn import_keeps_the_generated_templates() {
    let world = ImportWorld::new(Corrections::default());
    let templates = world.options.data_dir.join(".templates");
    let before = std::fs::read(templates.join("item.toml")).unwrap();
    let schema_before = std::fs::read(world.options.data_dir.join(".schemas/item.schema.json")).unwrap();
    import_ddobuilder(&world.options).unwrap();
    assert_eq!(std::fs::read(templates.join("item.toml")).unwrap(), before);
    assert_eq!(std::fs::read(world.options.data_dir.join(".schemas/item.schema.json")).unwrap(), schema_before);
    let actual = std::fs::read_dir(&templates)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<std::collections::BTreeSet<_>>();
    let expected = tree::schemas::kind_templates().unwrap().into_iter().map(|template| template.file_name).collect();
    assert_eq!(actual, expected);
}

#[test]
fn import_reports_a_note_his_value_now_matches() {
    let mut corrections = ravager_corrections();
    corrections.entries.retain(|entry| entry.name.contains("Draconic"));
    let world = ImportWorld::new(corrections.clone());
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    let text = report.to_string();
    assert!(text.contains("RESOLVED CORRECTION") && text.contains("ravager-fighter/tree.toml"), "{text}");
    let mut expected = corrections;
    expected.entries[0].file_name = "data".into();
    assert_eq!(Corrections::from_database(&world.database()).unwrap(), expected);
}

#[test]
fn import_keeps_a_corrected_value_he_changed_again() {
    let mut corrections = ravager_corrections();
    corrections.entries.retain(|entry| entry.name.contains("Barbarian"));
    let world = ImportWorld::new(corrections.clone());
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(report.has_conflicts(), "{report}");
    let result = Corrections::from_database(&world.database()).unwrap();
    assert_eq!(result.entries[0].to, corrections.entries[0].to);
    let parsed =
        ddo_etl::xml::trees::parse(&world.options.source_dir.join("EnhancementTrees/Barbarian_Ravager.tree.xml"))
            .unwrap();
    assert_eq!(
        result.entries[0].from,
        ddo_etl::corrections::CorrectionValue::Text(parsed.enhancements[0].description.clone().unwrap())
    );
    assert!(report.to_string().contains("ravager-barbarian/tree.toml"));
}

#[test]
fn import_places_his_new_tree_icons_beside_the_tree() {
    let world = ImportWorld::new(Corrections::default());
    import_ddobuilder(&world.options).unwrap();
    let overlay = fixture_root().join("d1350a0c/DataFiles/EnhancementImages");
    let mut checked = 0;
    for entry in std::fs::read_dir(overlay).unwrap() {
        let entry = entry.unwrap();
        let destination =
            world.options.data_dir.join("characters/enhancements/bombardier/icons").join(entry.file_name());
        assert_eq!(std::fs::read(destination).unwrap(), std::fs::read(entry.path()).unwrap());
        checked += 1;
    }
    assert_eq!(checked, 10);
}

#[test]
fn import_follows_his_icon_reference_swap() {
    let world = ImportWorld::new(Corrections::default());
    import_ddobuilder(&world.options).unwrap();
    for (internal, expected) in [("BomBurningAmbition", "BomAugmentation"), ("BomAugmentation", "BomBurningAmbition")] {
        let icon: String = world
            .database()
            .query_row("SELECT icon FROM enhancements WHERE internal_name = ?1", [internal], |row| row.get(0))
            .unwrap();
        assert_eq!(icon, expected);
        assert!(world
            .options
            .data_dir
            .join(format!("characters/enhancements/bombardier/icons/{expected}.png"))
            .exists());
    }
}

#[test]
fn import_removes_an_entity_we_left_alone() {
    let world = ImportWorld::new(Corrections::default());
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert!(report.to_string().contains("characters/enhancements/bombardier/tree.toml: REMOVED enhancement_selections/bomspellvialselection-sla-vial-of-acid"), "{report}");
    let count: i64 = world
        .database()
        .query_row("SELECT count(*) FROM enhancements WHERE internal_name = 'BomSpellvialSelectionII'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn import_declines_removing_an_annotated_entity() {
    let world = ImportWorld::new(Corrections::default());
    world.edit("INSERT INTO citations (owner_kind, owner_id, field, url, read) SELECT 'enhancements', id, '', 'https://ddowiki.com/page/Bombardier', '2026-10-17' FROM enhancements WHERE internal_name = 'BomSpellvialSelectionII'");
    let report = import_ddobuilder(&world.options).unwrap();
    let count: i64 = world.database().query_row("SELECT count(*) FROM enhancement_selections WHERE enhancement_id IN (SELECT id FROM enhancements WHERE internal_name = 'BomSpellvialSelectionII')", [], |row| row.get(0)).unwrap();
    assert_eq!(count, 5);
    assert!(report.to_string().contains("DECLINED REMOVAL"));
}

#[test]
fn import_treats_a_renamed_selection_as_a_removal_and_an_addition() {
    let world = ImportWorld::new(Corrections::default());
    world.edit("INSERT INTO citations (owner_kind,owner_id,field,url,read,note) SELECT 'enhancement_selections',id,'icon','https://ddowiki.com/page/Bombardier','2026-10-17','Reviewed icon' FROM enhancement_selections WHERE slug='bomspellvialselection-sla-vial-of-acid'");
    let report = import_ddobuilder(&world.options).unwrap();
    let db = world.database();
    let mut query = db.prepare("SELECT slug, sort_order FROM enhancement_selections WHERE enhancement_id IN (SELECT id FROM enhancements WHERE internal_name = 'BomSpellVialSelection') ORDER BY sort_order").unwrap();
    let selections = query
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let old_position =
        selections.iter().position(|(slug, _)| slug == "bomspellvialselection-sla-vial-of-acid").unwrap();
    assert!(old_position > 0);
    assert!(selections.iter().any(|(slug, _)| slug == "bomspellvialselection-sla-vial-smash-acid"));
    assert_eq!(
        selections.iter().map(|(_, position)| *position).collect::<Vec<_>>(),
        (0..selections.len() as i64).collect::<Vec<_>>()
    );
    assert!(report.to_string().contains("position was renumbered"), "{report}");
}

#[test]
fn import_takes_his_new_icon_bytes_on_a_file_we_left_alone() {
    let world = ImportWorld::new(Corrections::default());
    import_ddobuilder(&world.options).unwrap();
    assert_eq!(
        std::fs::read(world.options.data_dir.join("icons/feats/Alertness.png")).unwrap(),
        std::fs::read(world.options.source_dir.join("FeatImages/Alertness.png")).unwrap()
    );
}

#[test]
fn import_keeps_our_icon_bytes_and_lists_the_conflict() {
    let world = ImportWorld::new(Corrections::default());
    let ours = std::fs::read(world.options.previous_dir.join("FeatImages/Knock.png")).unwrap();
    std::fs::write(world.options.data_dir.join("icons/feats/Alertness.png"), &ours).unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(report.has_conflicts(), "{report}");
    assert!(report.to_string().contains("icons/feats/Alertness.png: ICON CONFLICT"));
    assert_eq!(std::fs::read(world.options.data_dir.join("icons/feats/Alertness.png")).unwrap(), ours);
}

#[test]
fn import_fails_without_writing_when_his_build_fails() {
    let world = ImportWorld::new(ravager_corrections());
    let path = world.options.source_dir.join("EnhancementTrees/Fighter_Ravager.tree.xml");
    let contents = std::fs::read_to_string(&path).unwrap();
    assert!(contents.contains("<Name>Ravager: Draconic Power Attack</Name>"));
    std::fs::write(
        &path,
        contents.replace("<Name>Ravager: Draconic Power Attack</Name>", "<Name>Missing corrected target</Name>"),
    )
    .unwrap();
    let before = world.bytes();
    let error = format!("{:#}", import_ddobuilder(&world.options).unwrap_err());
    assert!(error.contains("building the new DataFiles") && error.contains("Draconic Power Attack"), "{error}");
    assert_eq!(before, world.bytes());
}

#[test]
fn import_fails_without_writing_when_his_image_is_missing() {
    let world = ImportWorld::new(Corrections::default());
    std::fs::remove_file(world.options.source_dir.join("FeatImages/Alertness.png")).unwrap();
    let before = world.bytes();
    let error = format!("{:#}", import_ddobuilder(&world.options).unwrap_err());
    assert!(error.contains("missing image files"), "{error}");
    assert_eq!(before, world.bytes());
}

#[test]
fn import_removes_a_file_he_removed() {
    let mut world = ImportWorld::new(Corrections::default());
    import_ddobuilder(&world.options).unwrap();
    std::mem::swap(&mut world.options.previous_dir, &mut world.options.source_dir);
    std::mem::swap(&mut world.options.previous_sha, &mut world.options.sha);
    import_ddobuilder(&world.options).unwrap();
    assert!(!world.options.data_dir.join("packs/shared/filigrees/night-revels-gifts.toml").exists());
    assert!(!world.options.data_dir.join("icons/filigrees/NightRevelsGifts.png").exists());
}

#[test]
fn import_lists_his_change_to_an_entity_we_removed() {
    let world = ImportWorld::new(Corrections::default());
    world.edit("PRAGMA foreign_keys = OFF; DELETE FROM enhancements WHERE internal_name = 'ATShockGauntlet'; DELETE FROM requirements WHERE owner_kind = 'enhancement' AND owner_id NOT IN (SELECT id FROM enhancements); DELETE FROM modifiers WHERE source_kind = 'enhancement' AND source_id NOT IN (SELECT id FROM enhancements); DELETE FROM entity_identity");
    std::fs::remove_file(
        world.options.data_dir.join("characters/enhancements/arcanotechnician/icons/ATShockGauntlet.png"),
    )
    .unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(report.to_string().contains("CHANGED UPSTREAM, ABSENT IN OURS"), "{report}");
    let count: i64 = world
        .database()
        .query_row("SELECT count(*) FROM enhancements WHERE internal_name = 'ATShockGauntlet'", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

fn cli(world: &ImportWorld) -> std::process::Command {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_ddo-etl"));
    command
        .arg("import-ddobuilder")
        .arg("--previous")
        .arg(&world.options.previous_dir)
        .arg("--source")
        .arg(&world.options.source_dir)
        .arg("--data")
        .arg(&world.options.data_dir)
        .arg("--previous-sha")
        .arg(world.options.previous_sha.as_ref().unwrap())
        .arg("--sha")
        .arg(world.options.sha.as_ref().unwrap());
    command
}

#[test]
fn import_cli_exits_two_on_a_conflict_and_records_the_new_commit() {
    let world = ImportWorld::new(Corrections::default());
    world.edit("UPDATE enhancements SET cost_per_rank = '[3]' WHERE internal_name = 'SDDefenseBoost'");
    let output = cli(&world).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("CONFLICT"));
    let sha: String =
        world.database().query_row("SELECT upstream_sha FROM dataset_version", [], |row| row.get(0)).unwrap();
    assert_eq!(sha, NEW_SHA);
}

#[test]
fn import_cli_uses_the_data_files_corrections_by_default() {
    let mut corrections = ravager_corrections();
    corrections.entries.retain(|entry| entry.name.contains("Barbarian"));
    let world = ImportWorld::new(corrections);
    let output = cli(&world).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Our researched Barbarian"));
}

#[test]
fn import_refuses_missing_and_empty_data_directories() {
    let mut world = ImportWorld::new(Corrections::default());
    for path in [world.scratch.path().join("absent"), world.scratch.path().join("empty")] {
        if path.ends_with("empty") {
            std::fs::create_dir(&path).unwrap();
        }
        world.options.data_dir = path;
        assert!(import_ddobuilder(&world.options).is_err());
    }
}

#[test]
fn import_lists_one_conflict_for_an_array_both_sides_changed() {
    let world = ImportWorld::new(Corrections::default());
    world.edit("UPDATE modifiers SET amounts = '[7]' WHERE source_kind = 'enhancement' AND source_id IN (SELECT id FROM enhancements WHERE internal_name = 'RavBarbarianPowerAttack')");
    let report = import_ddobuilder(&world.options).unwrap();
    let conflicts: Vec<_> = report.entries.iter().filter(|entry| entry.category == "CONFLICT").collect();
    assert_eq!(conflicts.len(), 1, "{report}");
    assert!(conflicts[0].detail.contains("modifier"));
    let db = world.database();
    let amounts: Vec<String> = db.prepare("SELECT amounts FROM modifiers WHERE source_kind = 'enhancement' AND source_id IN (SELECT id FROM enhancements WHERE internal_name = 'RavBarbarianPowerAttack') ORDER BY sort_order").unwrap().query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert_eq!(amounts, vec!["[7]", "[7]"]);
}

#[test]
fn import_replaces_an_array_he_changed_and_we_did_not() {
    let world = ImportWorld::new(Corrections::default());
    let parsed =
        ddo_etl::xml::trees::parse(&world.options.source_dir.join("EnhancementTrees/Fighter_Ravager.tree.xml"))
            .unwrap();
    let expected: Vec<_> = parsed.enhancements[0].effects.iter().map(|effect| effect.amounts.clone()).collect();
    import_ddobuilder(&world.options).unwrap();
    let db = world.database();
    let amounts: Vec<String> = db.prepare("SELECT amounts FROM modifiers WHERE source_kind = 'enhancement' AND source_id IN (SELECT id FROM enhancements WHERE internal_name = 'DLRavDraconicPowerAttack') ORDER BY sort_order").unwrap().query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    let actual: Vec<Vec<f64>> = amounts.iter().map(|encoded| serde_json::from_str(encoded).unwrap()).collect();
    assert_eq!(actual, expected);
}

#[test]
fn import_lists_an_added_pack_as_needing_research() {
    let mut world = ImportWorld::new(Corrections::default());
    std::fs::copy(
        fixture_root().join("quests-without-terror-of-demogorgon/Quests.xml"),
        world.options.previous_dir.join("Quests.xml"),
    )
    .unwrap();
    let empty_data = world.scratch.path().join("without-pack");
    let mut db = Connection::open_in_memory().unwrap();
    build::build_database(
        fixture::groupings(),
        &world.options.previous_dir,
        &WikiOverrides::default(),
        &Corrections::default(),
        &mut db,
        &DatasetVersion { upstream_sha: PREVIOUS_SHA.into(), built_at: "2026-10-17T00:00:00Z".into() },
    )
    .unwrap();
    tree::export_ddobuilder_tree(&db, &world.options.previous_dir, &empty_data).unwrap();
    world.options.data_dir = empty_data;
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(report.to_string().contains("packs/terror-of-demogorgon/pack.toml: NEEDS RESEARCH"), "{report}");
}

#[test]
fn import_preserves_unmanaged_files_and_removes_stale_managed_files() {
    let world = ImportWorld::new(Corrections::default());
    std::fs::write(world.options.data_dir.join("README.md"), "Our local guide").unwrap();
    std::fs::write(world.options.data_dir.join(".schemas/stale.json"), "{}").unwrap();
    import_ddobuilder(&world.options).unwrap();
    assert_eq!(std::fs::read_to_string(world.options.data_dir.join("README.md")).unwrap(), "Our local guide");
    assert!(!world.options.data_dir.join(".schemas/stale.json").exists());
}

#[test]
fn import_cli_exits_two_on_an_icon_conflict() {
    let world = ImportWorld::new(Corrections::default());
    std::fs::copy(
        world.options.previous_dir.join("FeatImages/Knock.png"),
        world.options.data_dir.join("icons/feats/Alertness.png"),
    )
    .unwrap();
    let output = cli(&world).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("ICON CONFLICT"));
}

#[test]
fn import_rebases_both_sides_without_mutating_our_correction_values() {
    let mut corrections = ravager_corrections();
    corrections.entries.retain(|entry| entry.name.contains("Barbarian"));
    let mut world = ImportWorld::new(corrections.clone());
    let expected = Corrections::from_database(&world.database()).unwrap();
    import_ddobuilder(&world.options).unwrap();
    let rebased = Corrections::from_database(&world.database()).unwrap();
    let corrected_values = |corrections: &Corrections| {
        corrections
            .entries
            .iter()
            .map(|entry| (entry.kind, entry.name.clone(), entry.qualifier(), entry.field.clone(), entry.to.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(corrected_values(&rebased), corrected_values(&expected));
    assert_ne!(rebased.entries[0].from, expected.entries[0].from);
    world.options.previous_dir = world.options.source_dir.clone();
    world.options.previous_sha = world.options.sha.clone();
    let before = world.bytes();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!(before, world.bytes());
    assert_eq!(Corrections::from_database(&world.database()).unwrap(), rebased);
}

#[test]
fn import_icon_conflicts_name_the_final_tree_local_path() {
    let world = ImportWorld::new(Corrections::default());
    let old_icon = world.options.previous_dir.join("EnhancementImages/BomBurningAmbition.png");
    let ours = world.options.data_dir.join("characters/enhancements/bombardier/icons/BomBurningAmbition.png");
    let incoming = world.options.source_dir.join("EnhancementImages/BomBurningAmbition.png");
    let ours_bytes = std::fs::read(world.options.previous_dir.join("FeatImages/Knock.png")).unwrap();
    let incoming_bytes = std::fs::read(world.options.source_dir.join("FeatImages/Alertness.png")).unwrap();
    assert_ne!(ours_bytes, incoming_bytes);
    assert_ne!(std::fs::read(old_icon).unwrap(), incoming_bytes);
    std::fs::write(&ours, &ours_bytes).unwrap();
    std::fs::write(incoming, incoming_bytes).unwrap();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(
        report.to_string().contains("characters/enhancements/bombardier/icons/BomBurningAmbition.png: ICON CONFLICT"),
        "{report}"
    );
    assert_eq!(std::fs::read(ours).unwrap(), ours_bytes);
}
