mod fixture;

use ddo_etl::build::{build_database, BuildReport, StaleCorrection, StaleCorrectionCause};
use ddo_etl::corrections::{CorrectionValue, Corrections, DamageRow};
use ddo_etl::wiki::WikiOverrides;
use ddo_model::DatasetVersion;
use fixture::{new_correction, new_qualified_correction, CorrectionSet};
use rusqlite::{Connection, OptionalExtension};
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture_dataset_version() -> DatasetVersion {
    DatasetVersion { upstream_sha: "31ef0201".into(), built_at: "2026-09-20T00:00:00Z".into() }
}

fn validated_corrections(files: &[(&str, &CorrectionSet)]) -> Result<Corrections, String> {
    fixture::corrections_from(files)
}

fn committed_corrections() -> &'static Corrections {
    static COMMITTED: std::sync::OnceLock<Corrections> = std::sync::OnceLock::new();
    COMMITTED
        .get_or_init(|| Corrections::from_data(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap())
}

fn built_db_with_wiki(wiki: &WikiOverrides, corrections: &Corrections) -> Result<(Connection, BuildReport), String> {
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        fixture::groupings(),
        &fixtures_dir().join("DataFiles"),
        wiki,
        corrections,
        &mut db,
        &fixture_dataset_version(),
    )
    .map_err(|e| format!("{e:#}"))?;
    Ok((db, report))
}

fn built_db_with(correction_files: &[(&str, &CorrectionSet)]) -> Result<(Connection, BuildReport), String> {
    built_db_with_wiki(&WikiOverrides::default(), &validated_corrections(correction_files)?)
}

fn augment_min_levels(db: &Connection, name: &str) -> Vec<Option<i64>> {
    let mut statement = db.prepare("SELECT min_level FROM augments WHERE name = ?1 ORDER BY id").unwrap();
    statement.query_map([name], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

fn recorded_corrections(db: &Connection) -> Vec<(String, String, String, String, String)> {
    let mut statement = db
        .prepare("SELECT kind, name, field, from_value, to_value FROM corrections ORDER BY kind, name, field")
        .unwrap();
    statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn copy_fixture_icons(root: &std::path::Path) {
    let source = fixtures_dir().join("data");
    for entry in walkdir::WalkDir::new(&source).into_iter().map(Result::unwrap).filter(|entry| {
        entry.file_type().is_file() && entry.path().extension().is_some_and(|extension| extension == "png")
    }) {
        let destination = root.join(entry.path().strip_prefix(&source).unwrap());
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::copy(entry.path(), destination).unwrap();
    }
}

fn rebuilt_correction_notes(db: &Connection) -> Corrections {
    let mut corrections = Corrections::from_database(db).unwrap();
    corrections.entries.extend(fixture::fixture_icon_corrections().entries);
    let (normalized, _) = built_db_with_wiki(&WikiOverrides::default(), &corrections).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    ddo_etl::tree::export_database(&normalized, scratch.path()).unwrap();
    copy_fixture_icons(scratch.path());
    let mut rebuilt = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(scratch.path(), &mut rebuilt).unwrap();
    Corrections::from_database(&rebuilt).unwrap()
}

#[test]
fn item_effect_replacement_preserves_position_and_reports_a_missing_upstream_link() {
    let index = ddo_etl::groupings::Groupings::from_database(&fixture::riposte_rank_database()).unwrap();
    let replacement = new_qualified_correction(
        "item_effect",
        "Ship Kickers",
        "effect = 'Riposte V'",
        "effect",
        "\"Riposte\"",
        "\"Riposte V\"",
    );
    let corrections = validated_corrections(&[("ship-kickers.toml", &replacement)]).unwrap();
    assert_eq!(corrections.entries[0].qualifier(), "Riposte V");
    for present in [true, false] {
        let source = fixture::xml_source();
        fixture::write_ship_kickers(source.path());
        if !present {
            let path = source.path().join("Items/ShipKickers.item");
            let text = std::fs::read_to_string(&path).unwrap();
            let start = text.find("    <Buff>\n      <Type>Riposte</Type>").unwrap();
            let end = start + text[start..].find("    </Buff>").unwrap() + "    </Buff>".len();
            std::fs::write(path, format!("{}{}", &text[..start], &text[end..])).unwrap();
        }
        let mut db = Connection::open_in_memory().unwrap();
        let report = build_database(
            &index,
            source.path(),
            &WikiOverrides::default(),
            &corrections,
            &mut db,
            &fixture_dataset_version(),
        )
        .unwrap();
        if present {
            assert!(report.stale_corrections.is_empty(), "{:?}", report.stale_corrections);
            let links = db.prepare("SELECT e.name,l.sort_order,l.value,l.value2 FROM item_effects l JOIN effects e ON e.id=l.effect_id JOIN items i ON i.id=l.item_id WHERE i.name='Ship Kickers' ORDER BY l.sort_order").unwrap().query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?,row.get::<_,Option<i64>>(2)?,row.get::<_,Option<i64>>(3)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
            assert_eq!(
                links,
                [
                    ("Dexterity".into(), 0, Some(2), None),
                    ("Riposte V".into(), 1, None, None),
                    ("Balance".into(), 2, Some(15), None)
                ]
            );
        } else {
            assert_eq!(report.stale_corrections.len(), 1);
            assert!(
                matches!(&report.stale_corrections[0].cause, StaleCorrectionCause::ValueChanged { maetrim_value, .. } if maetrim_value == "\"no such bonus\""),
                "{:?}",
                report.stale_corrections
            );
        }
    }
}

#[test]
fn item_effect_replacement_needs_no_upstream_definition_in_the_data_tree() {
    let source = fixture::xml_source();
    fixture::write_ship_kickers(source.path());
    let replacement = new_qualified_correction(
        "item_effect",
        "Ship Kickers",
        "effect = 'Riposte V'",
        "effect",
        "'Riposte'",
        "'Riposte V'",
    ) + &CorrectionSet(fixture::fixture_icon_corrections().entries);
    let (db, report) =
        built_db_from_index(fixture::grouping_cases(), source.path(), &[("ship-kickers.toml", &replacement)]).unwrap();
    assert!(report.stale_corrections.is_empty());
    let exported = tempfile::tempdir().unwrap();
    ddo_etl::tree::export_database(&db, exported.path()).unwrap();
    copy_fixture_icons(exported.path());
    std::fs::remove_file(exported.path().join("effects/riposte.toml")).unwrap();
    let mut rebuilt = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(exported.path(), &mut rebuilt).unwrap();
    assert_eq!(recorded_corrections(&db), recorded_corrections(&rebuilt));
    assert_eq!(
        rebuilt.query_row("SELECT COUNT(*) FROM effects WHERE name='Riposte'", [], |row| row.get::<_, i64>(0)).unwrap(),
        0
    );
}

#[test]
fn effect_scale_and_rounding_corrections_target_the_definition() {
    let index = ddo_etl::groupings::Groupings::from_database(&fixture::riposte_rank_database()).unwrap();
    let rules = new_qualified_correction("effect", "Riposte", "stat = 'Armor Class'", "scale", "1.0", "0.5")
        + &new_qualified_correction("effect", "Riposte", "stat = 'Armor Class'", "rounding", "\"down\"", "\"up\"")
        + &new_qualified_correction("effect", "Riposte", "stat = 'Saving Throws'", "scale", "1.0", "0.5");
    let corrections = validated_corrections(&[("riposte.toml", &rules)]).unwrap();
    let source = fixture::xml_source();
    fixture::write_ship_kickers(source.path());
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        &index,
        source.path(),
        &WikiOverrides::default(),
        &corrections,
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    assert!(report.stale_corrections.is_empty(), "{:?}", report.stale_corrections);
    let bonuses = db.prepare("SELECT stat.name,b.amount FROM owner_bonuses b JOIN effects stat ON stat.id=b.stat_id JOIN effects e ON e.id=b.via_effect_id JOIN items i ON i.id=b.owner_id WHERE b.owner_kind='item' AND i.name='Ship Kickers' AND e.name='Riposte' ORDER BY stat.name").unwrap().query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(bonuses, [("Armor Class".into(), 2), ("Saving Throws".into(), 1)]);
}

#[test]
fn alias_correction_uses_the_link_alias_and_reports_a_missing_member() {
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
    let alias = new_qualified_correction(
        "item_bonus",
        "Legendary Keylock Ring",
        "stat = 'Spot'\nbonus_type = 'Competence'",
        "alias",
        "\"null\"",
        "\"Awareness\"",
    );
    let corrections = validated_corrections(&[("alias.toml", &alias)]).unwrap();
    for include_spot in [true, false] {
        let source = fixture::xml_source();
        fixture::write_search_and_spot_item(source.path(), 20, include_spot);
        let mut db = Connection::open_in_memory().unwrap();
        let report = build_database(
            &groupings,
            source.path(),
            &WikiOverrides::default(),
            &corrections,
            &mut db,
            &fixture_dataset_version(),
        )
        .unwrap();
        if include_spot {
            let chosen: String = db.query_row("SELECT a.name FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effect_aliases a ON a.effect_id=l.effect_id AND a.name=l.alias WHERE i.name='Legendary Keylock Ring'", [], |row| row.get(0)).unwrap();
            assert_eq!(chosen, "Awareness");
            assert!(report.stale_corrections.is_empty());
            assert!(recorded_corrections(&db).iter().any(|row| row.2 == "alias" && row.4 == "\"Awareness\""));
        } else {
            assert_eq!(report.stale_corrections.len(), 1);
            assert!(
                matches!(&report.stale_corrections[0].cause, StaleCorrectionCause::ValueChanged { maetrim_value, .. }
                if maetrim_value == "\"no such bonus\""),
                "{:?}",
                report.stale_corrections
            );
        }
    }
}

#[test]
fn correctable_fields_pin_each_kinds_names_columns_shapes_and_nullability() {
    use ddo_model::enums::CorrectionKind;
    let expected = [
        (CorrectionKind::Item, "accepts_sentience:accepts_sentience:Flag:false, description:description:Text:true, drop_location:drop_location:Text:true, enhancement_bonus:enhancement_bonus:Integer:true, icon:icon:Text:true, is_legacy:is_legacy:Flag:false, is_minor_artifact:is_minor_artifact:Flag:false, item_category:item_category:Text:false, item_type:item_type:Text:true, material:material_id:NamedReference { referenced_table: \"item_materials\" }:true, minimum_level:minimum_level:Integer:false, name:name:RowName:false, race_required:race_required:Text:true, remove:remove:Removal:false, set_bonus:set_bonus:SetName:true, slot:slot_id:NamedReference { referenced_table: \"equipment_slots\" }:false"),
        (CorrectionKind::Effect, "bonus_type:bonus_type_id:EffectBonusType:false, constant:constant:EffectBonusConstant:false, default_value:default_value:Integer:true, description_template:description_template:Text:true, name:name:RowName:false, remove:remove:EffectBonusRemoval:false, rounding:rounding:StatRounding:false, scale:scale:StatScale:false, verbose_name_template:verbose_name_template:Text:false"),
        (CorrectionKind::Augment, "description:description:Text:false, effect_description:effect_description:Text:true, family:family:Text:false, min_level:min_level:Integer:true, name:name:RowName:false, remove:remove:Removal:false"),
        (CorrectionKind::Quest, "epic_level:epic_level:Integer:true, favor:favor:Integer:true, is_raid:is_raid:Flag:false, level:level:Integer:true, name:name:RowName:false, pack:pack_id:NamedReference { referenced_table: \"adventure_packs\" }:false, patron:patron_id:NamedReference { referenced_table: \"patrons\" }:true, remove:remove:Removal:false"),
        (CorrectionKind::Feat, "description:description:Text:true, icon:icon:Text:true"),
        (CorrectionKind::FeatBonus, "add:add:BonusAddition:false, alias:alias:BonusAlias:true, bonus_type:bonus_type:BonusTypeName:false, lines:lines:BonusLines:false, remove:remove:BonusRemoval:false, value:value:Integer:true"),
        (CorrectionKind::SocketOptionBonus, "alias:alias:BonusAlias:true, lines:lines:BonusLines:false, remove:remove:BonusRemoval:false, value:value:Integer:true"),
        (CorrectionKind::Enhancement, "arrows:arrows:TextList:false, cooldown_seconds:cooldown_seconds:Integer:true, cost_per_rank:cost_per_rank:IntegerList:false, description:description:Text:false, duration_seconds:duration_seconds:Integer:true, icon:icon:Text:true, is_clickie:is_clickie:Flag:false, is_tier5:is_tier5:Flag:false, min_spent:min_spent:Integer:false, name:name:RowName:false, ranks:ranks:Integer:false, x:x:Integer:false, y:y:Integer:false"),
        (CorrectionKind::Spell, "description:description:Text:false, icon:icon:Text:true"),
        (CorrectionKind::EnhancementTree, "icon:icon:Text:false"),
        (CorrectionKind::OptionalBuff, "description:description:Text:false, icon:icon:Text:true"),
        (CorrectionKind::Clickie, "description:description:Text:false, icon:icon:Text:false"),
        (CorrectionKind::Filigree, "description:description:Text:false, icon:icon:Text:false"),
        (CorrectionKind::Race, "description:description:Text:false"),
        (CorrectionKind::Class, "description:description:Text:false"),
        (CorrectionKind::AdventurePack, "name:name:RowName:false"),
        (CorrectionKind::Patron, "name:name:RowName:false"),
        (CorrectionKind::SocketLabel, "name:label:RowName:false"),
        (CorrectionKind::SetBonus, "icon:icon:Text:true, name:name:RowName:false"),
        (CorrectionKind::AugmentBonus, "add:add:BonusAddition:false, alias:alias:BonusAlias:true, bonus_type:bonus_type:BonusTypeName:false, dedupe:dedupe:BonusDedupe:false, lines:lines:BonusLines:false, remove:remove:BonusRemoval:false, value:value:Integer:true"),
        (CorrectionKind::ItemBonus, "add:add:BonusAddition:false, alias:alias:BonusAlias:true, bonus_type:bonus_type:BonusTypeName:false, lines:lines:BonusLines:false, remove:remove:BonusRemoval:false, rounding:rounding:StatRounding:false, scale:scale:StatScale:false, value:value:Integer:true"),
        (CorrectionKind::SetTier, "add:add:TierAddition:false, description:description:Text:true, remove:remove:Removal:false"),
        (CorrectionKind::SetTierBonus, "add:add:BonusAddition:false, alias:alias:BonusAlias:true, bonus_type:bonus_type:BonusTypeName:false, lines:lines:BonusLines:false, remove:remove:BonusRemoval:false, rounding:rounding:StatRounding:false, scale:scale:StatScale:false, value:value:Integer:true"),
        (CorrectionKind::ItemEffect, "add:add:EffectAddition:false, bonus_type:bonus_type:BonusTypeName:false, effect:effect:EffectReplacement:false, remove:remove:Removal:false, value2:value2:Integer:true, value:value:Integer:true"),
        (CorrectionKind::EffectDamage, "add:add:DamageAddition:false, remove:remove:DamageRemoval:false"),
        (CorrectionKind::ItemSocket, "add:add:SocketAddition:false"),
    ];
    assert_eq!(expected.len(), CorrectionKind::ALL.len());
    for (kind, expected) in expected {
        let mut fields = ddo_etl::corrections::correctable_fields(kind)
            .iter()
            .map(|field| format!("{}:{}:{:?}:{}", field.name, field.column, field.shape, field.is_nullable))
            .collect::<Vec<_>>();
        fields.sort();
        assert_eq!(fields.join(", "), expected, "{kind:?}");
    }
    let text = new_correction("enhancement", "Kensei: Action Boost", "name", "\"Attack Boost\"", "\"Reviewed\"")
        + "selection = \"Attack Boost\"\n";
    let mut selection = validated_corrections(&[("selection.toml", &text)]).unwrap().entries.remove(0);
    for (name, shape, is_nullable) in [
        ("name", ddo_etl::corrections::FieldShape::RowName, false),
        ("cooldown_seconds", ddo_etl::corrections::FieldShape::Integer, true),
        ("cost_per_rank", ddo_etl::corrections::FieldShape::IntegerList, false),
        ("description", ddo_etl::corrections::FieldShape::Text, true),
        ("duration_seconds", ddo_etl::corrections::FieldShape::Integer, true),
        ("icon", ddo_etl::corrections::FieldShape::Text, false),
        ("is_clickie", ddo_etl::corrections::FieldShape::Flag, false),
        ("min_spent", ddo_etl::corrections::FieldShape::Integer, true),
        ("ranks", ddo_etl::corrections::FieldShape::Integer, true),
    ] {
        selection.field = name.into();
        assert_eq!(
            *selection.correctable_field().unwrap(),
            ddo_etl::corrections::CorrectableField { name, column: name, shape, is_nullable }
        );
    }
}

#[test]
fn required_item_level_cannot_be_corrected_to_null() {
    let correction = new_correction("item", "Five Rings", "minimum_level", "29", "\"null\"");
    let error = built_db_with(&[("level.toml", &correction)]).expect_err("a required level cannot be null");
    assert!(error.contains("minimum_level"), "{error}");
}

#[test]
fn required_flag_can_be_corrected_from_an_absent_upstream_value() {
    let correction = new_correction("item", "Five Rings", "accepts_sentience", "\"null\"", "1");
    validated_corrections(&[("flag.toml", &correction)]).unwrap();
}

#[test]
fn a_stance_icon_correction_is_scoped_to_its_spell_and_stance_name() {
    let correction = new_correction("spell", "Evolution", "icon", "\"intelligence\"", "\"Intelligence\"")
        + "stance = \"IntEvolution\"\n";
    let (db, report) = built_db_with(&[("icons.toml", &correction)]).unwrap();
    assert_eq!(report.correction_applied_count, 1);
    let icons = db.prepare("SELECT spell.name, stance.icon FROM stances stance JOIN spells spell ON stance.owner_kind = 'spell' AND stance.owner_id = spell.id WHERE stance.name = 'IntEvolution' ORDER BY spell.name").unwrap()
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(
        icons,
        [
            ("Evolution".into(), "Intelligence".into()),
            ("Greater Evolution".into(), "intelligence".into()),
            ("Lesser Evolution".into(), "intelligence".into())
        ]
    );
    let restored = Corrections::from_database(&db).unwrap();
    assert_eq!(restored.entries[0].qualifier(), "stance \"IntEvolution\"");
}

#[test]
fn icon_notes_cover_filigrees_and_every_existing_ability_owner() {
    let (baseline, _) = built_db_with(&[]).unwrap();
    let mut entries = Vec::new();
    for (table, owner_kind, owner_table, qualifier_key) in [
        ("stances", "feat", "feats", "stance"),
        ("stances", "enhancement", "enhancements", "stance"),
        ("dcs", "enhancement", "enhancements", "dc"),
    ] {
        let (owner, name, icon): (String, String, String) = baseline.query_row(
            &format!("SELECT owner.name, ability.name, ability.icon FROM {table} ability JOIN {owner_table} owner ON owner.id = ability.owner_id WHERE ability.owner_kind = ?1 ORDER BY ability.id LIMIT 1"),
            [owner_kind], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        ).unwrap();
        entries.push(
            new_correction(owner_kind, &owner, "icon", &format!("{icon:?}"), "\"CorrectedAbilityIcon\"")
                + format!("{qualifier_key} = {name:?}\n").as_str(),
        );
    }
    let (name, icon): (String, String) = baseline
        .query_row("SELECT name, icon FROM filigrees ORDER BY id LIMIT 1", [], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap();
    entries.push(new_correction("filigree", &name, "icon", &format!("{icon:?}"), "\"CorrectedFiligreeIcon\""));
    let icon_corrections = entries.iter().fold(CorrectionSet::default(), |all, entry| all + entry);
    let (db, report) = built_db_with(&[("icons.toml", &icon_corrections)]).unwrap();
    assert_eq!(report.correction_applied_count, 4);
    let output = std::env::temp_dir().join(format!("ddo-data-ability-icon-notes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&output);
    ddo_etl::tree::export_database(&db, &output).unwrap();
    fn corrected_notes(value: &toml::Value) -> usize {
        match value {
            toml::Value::Table(fields) => {
                let own = if fields
                    .get("icon")
                    .and_then(toml::Value::as_str)
                    .is_some_and(|icon| icon.starts_with("Corrected"))
                {
                    assert_eq!(fields["ddobuilder_correction"][0]["field"].as_str(), Some("icon"));
                    assert!(fields["ddobuilder_correction"][0].get("qualifier").is_none());
                    1
                } else {
                    0
                };
                own + fields.values().map(corrected_notes).sum::<usize>()
            }
            toml::Value::Array(rows) => rows.iter().map(corrected_notes).sum(),
            _ => 0,
        }
    }
    let count: usize = walkdir::WalkDir::new(&output)
        .into_iter()
        .map(Result::unwrap)
        .filter(|entry| entry.path().extension().is_some_and(|extension| extension == "toml"))
        .map(|entry| {
            corrected_notes(&toml::from_str::<toml::Value>(&std::fs::read_to_string(entry.path()).unwrap()).unwrap())
        })
        .sum();
    assert_eq!(count, 4);
    std::fs::remove_dir_all(output).unwrap();
}

#[test]
fn nested_icon_notes_remain_beside_the_corrected_rows() {
    let selection = new_correction(
        "enhancement",
        "Legendary Dreadnought: Dire Attack",
        "icon",
        "\"DireAttack\"",
        "\"SelectionIcon\"",
    ) + "selection = \"Dire Charge\"\n";
    let dc =
        new_correction("enhancement", "Legendary Dreadnought: Dire Attack", "icon", "\"DireAttack\"", "\"DcIcon\"")
            + "selection = \"Dire Charge\"\ndc = \"Dire Charge\"\n";
    let stance = new_correction("spell", "Evolution", "icon", "\"intelligence\"", "\"Intelligence\"")
        + "stance = \"IntEvolution\"\n";
    let (db, report) =
        built_db_with(&[("selection.toml", &selection), ("dc.toml", &dc), ("stance.toml", &stance)]).unwrap();
    assert_eq!(report.correction_applied_count, 3);
    let output = std::env::temp_dir().join(format!("ddo-data-nested-icon-notes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&output);
    ddo_etl::tree::export_database(&db, &output).unwrap();
    let tree: toml::Value = toml::from_str(
        &std::fs::read_to_string(output.join("characters/enhancements/legendary-dreadnought/tree.toml")).unwrap(),
    )
    .unwrap();
    let enhancement = tree["enhancement"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"].as_str() == Some("Legendary Dreadnought: Dire Attack"))
        .unwrap();
    assert!(enhancement.get("ddobuilder_correction").is_none());
    let selection = enhancement["selection"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"].as_str() == Some("Dire Charge"))
        .unwrap();
    for (row, icon) in [(selection, "SelectionIcon"), (&selection["dc"][0], "DcIcon")] {
        assert_eq!(row["icon"].as_str(), Some(icon));
        assert_eq!(row["ddobuilder_correction"][0]["upstream"].as_str(), Some("DireAttack"));
        assert!(row["ddobuilder_correction"][0].get("qualifier").is_none());
    }
    let restored = Corrections::from_database(&db).unwrap();
    assert_eq!(restored.entries.len(), 3);
    std::fs::remove_dir_all(output).unwrap();
}

#[test]
fn drop_location_correction_changes_the_source_before_links_are_written() {
    let correction = new_correction(
        "item",
        "Sireth, Spear of the Sky",
        "drop_location",
        "\"Caught in the Web, End Chest\"",
        "\"The Chronoscope, End Chest\"",
    );
    let (db, report) = built_db_with(&[("drop.toml", &correction)]).unwrap();
    assert_eq!(report.correction_applied_count, 1);
    assert_eq!(report.correction_stale_count, 0);
    let linked: Vec<String> = db
        .prepare(
            "SELECT q.name FROM sources s JOIN quests q ON q.id = s.quest_id
         JOIN items i ON i.id = s.item_id WHERE i.name = 'Sireth, Spear of the Sky' ORDER BY q.name",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(linked, ["The Chronoscope"]);
    let segment: String = db
        .query_row(
            "SELECT s.drop_text_segment FROM sources s JOIN items i ON i.id = s.item_id
         WHERE i.name = 'Sireth, Spear of the Sky'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(segment, "The Chronoscope, End Chest");
}

#[test]
fn prerequisite_quest_words_do_not_create_sources() {
    let correction = new_correction(
        "item",
        "Sireth, Spear of the Sky",
        "drop_location",
        "\"Caught in the Web, End Chest\"",
        "\"Caught in the Web, End Chest (rare, requires completion of The Cursed Crypt)\\nRedemption, end chest\"",
    );
    let (db, report) = built_db_with(&[("drop.toml", &correction)]).unwrap();
    assert_eq!(report.correction_applied_count, 1);
    let linked: Vec<String> = db
        .prepare(
            "SELECT q.name FROM sources s JOIN quests q ON q.id = s.quest_id
         JOIN items i ON i.id = s.item_id WHERE i.name = 'Sireth, Spear of the Sky' ORDER BY q.name",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(linked, ["Caught in the Web", "Redemption"]);
}

#[test]
fn item_effect_second_value_corrections_preserve_the_first_slot_and_track_staleness() {
    let correction = new_qualified_correction(
        "item_effect",
        "Grand Ringmaster's Plate (level 4)",
        "effect = 'Command'",
        "value2",
        "\"null\"",
        "-6",
    );
    let mut corrections = validated_corrections(&[("grand-ringmasters-plate-level-4.toml", &correction)]).unwrap();
    let ring_type = new_qualified_correction(
        "item_effect",
        "Ring of Power",
        "effect = 'Command'",
        "bonus_type",
        "\"Insight\"",
        "\"Competence\"",
    );
    corrections.entries.extend(validated_corrections(&[("ring-of-power.toml", &ring_type)]).unwrap().entries);
    corrections.entries.extend(fixture::fixture_icon_corrections().entries);
    for upstream in [None, Some(-4)] {
        let source = fixture::xml_source();
        fixture::write_command_owners(source.path());
        if let Some(amount) = upstream {
            let path = source.path().join("Items/GrandRingmastersPlate4.item");
            let text = std::fs::read_to_string(&path).unwrap();
            let original = "<Type>Command</Type>\n      <Value1>3</Value1>";
            assert!(text.contains(original));
            std::fs::write(path, text.replace(original, &format!("{original}\n      <Value2>{amount}</Value2>")))
                .unwrap();
        }
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
        let amounts = db.query_row("SELECT l.value,l.value2 FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effects e ON e.id=l.effect_id WHERE i.name=?1 AND e.name='Command'", [&corrections.entries[0].name], |row| Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?))).unwrap();
        assert_eq!(amounts, (3, upstream.unwrap_or(-6)));
        if upstream.is_none() {
            assert!(report.stale_corrections.is_empty());
            assert!(recorded_corrections(&db).iter().any(|row| row.2 == "value2" && row.4 == "-6"));
            let exported = tempfile::tempdir().unwrap();
            ddo_etl::tree::export_database(&db, exported.path()).unwrap();
            let icons = fixtures_dir().join("data");
            for entry in walkdir::WalkDir::new(&icons).into_iter().map(Result::unwrap).filter(|entry| {
                entry.file_type().is_file() && entry.path().extension().is_some_and(|extension| extension == "png")
            }) {
                let destination = exported.path().join(entry.path().strip_prefix(&icons).unwrap());
                std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
                std::fs::copy(entry.path(), destination).unwrap();
            }
            ddo_etl::icons::export_icons(source.path(), &exported.path().join("icons")).unwrap();
            assert!(ddo_etl::icons::complete_referenced_icons(exported.path()).unwrap().missing_icons.is_empty());
            let mut rebuilt = Connection::open_in_memory().unwrap();
            ddo_etl::tree::build_from_data(exported.path(), &mut rebuilt).unwrap();
            let restored = Corrections::from_database(&rebuilt).unwrap();
            let ring_type = restored.entries.iter().find(|entry| entry.name == "Ring of Power").unwrap();
            assert_eq!(ring_type.qualifier(), "effect \"Command\"");
            assert_eq!(ring_type.from, CorrectionValue::Text("Insight".into()));
            assert_eq!(ring_type.to, CorrectionValue::Text("Competence".into()));
            let restored = restored.entries.iter().find(|entry| entry.field == "value2").unwrap();
            assert_eq!(restored.from, CorrectionValue::Null);
            assert_eq!(restored.to, CorrectionValue::Integer(-6));
        } else {
            assert_eq!(report.stale_corrections.len(), 1);
            assert!(
                matches!(&report.stale_corrections[0].cause, StaleCorrectionCause::ValueChanged { maetrim_value, .. } if maetrim_value == "-4")
            );
        }
    }
}

#[test]
fn item_effect_corrections_change_a_text_line_type_or_remove_its_link() {
    let source = fixture::xml_source();
    fixture::write_ship_kickers(source.path());
    let type_correction = new_correction("item_effect", "Ship Kickers", "bonus_type", "\"Insight\"", "\"Quality\"")
        + "effect = \"Riposte\"\n";
    let (typed_db, typed_report) = built_db_from(source.path(), &[("type.toml", &type_correction)]).unwrap();
    assert_eq!(typed_report.correction_stale_count, 0);
    let typed: String = typed_db
        .query_row(
            "SELECT b.name FROM item_effects l JOIN items i ON i.id = l.item_id
             JOIN effects e ON e.id = l.effect_id JOIN bonus_types b ON b.id = l.bonus_type_id
             WHERE i.name = 'Ship Kickers' AND e.name = 'Riposte'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(typed, "Quality");

    let removal = new_correction("item_effect", "Ship Kickers", "remove", "0", "1") + "effect = \"Riposte\"\n";
    let (removed_db, removed_report) = built_db_from(source.path(), &[("remove.toml", &removal)]).unwrap();
    assert_eq!(removed_report.correction_stale_count, 0);
    assert!(!item_effect_names(&removed_db, "Ship Kickers").contains(&"Riposte".to_string()));
}

#[test]
fn committed_insight_skill_corrections_use_the_client_cap_through_the_writer() {
    let source = fixture::xml_source();
    std::fs::write(
        source.path().join("Augments/CannithAndRandomItem.Augments.xml"),
        include_str!("fixtures/effect-groups/InsightfulSkills.Augments.xml"),
    )
    .unwrap();
    std::fs::create_dir_all(source.path().join("AugmentImages")).unwrap();
    std::fs::write(
        source.path().join("AugmentImages/Cannith.png"),
        include_bytes!("fixtures/effect-groups/ElementalResistance/AugmentImages/Cannith.png"),
    )
    .unwrap();
    let names = ["Insightful Haggle", "Insightful Diplomacy", "Insightful Spellsight"];
    let mut corrections = committed_corrections().clone();
    corrections.entries.retain(|correction| names.contains(&correction.name.as_str()));
    assert_eq!(corrections.entries.iter().filter(|correction| correction.field == "value").count(), names.len());
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
    for name in names {
        let (amount, bonus_type): (i64, String) = db
            .query_row(
                "SELECT b.amount,t.name FROM owner_bonuses b JOIN augments a ON b.owner_kind='augment' AND b.owner_id=a.id JOIN bonus_types t ON t.id=b.bonus_type_id WHERE a.name=?1",
                [name],
                |row| Ok((row.get(0)?,row.get(1)?)),
            )
            .unwrap();
        assert_eq!((amount, bonus_type), (11, "Insight".into()), "{name}");
    }
    let alias: String = db
        .query_row(
            "SELECT l.alias FROM augment_effects l JOIN augments a ON a.id=l.augment_id WHERE a.name='Insightful Spellsight'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(alias, "Spellsight");
    assert_eq!(report.correction_applied_count, corrections.entries.len());
    assert!(report.stale_corrections.is_empty(), "{:?}", report.stale_corrections);
}

#[test]
fn committed_data_corrects_the_furys_rage_level() {
    let corrections = committed_corrections().clone();
    let furys_rage = corrections.entries.iter().find(|c| c.name == "The Fury's Rage").unwrap();
    assert_eq!(
        (furys_rage.from.clone(), furys_rage.to.clone()),
        (CorrectionValue::Integer(318), CorrectionValue::Integer(18))
    );
}

#[test]
fn rejects_a_field_outside_the_kinds_allow_list() {
    for (kind, field) in [("augment", "icon"), ("augment", "id"), ("item", "icon"), ("feat", "icon"), ("quest", "icon")]
    {
        let error =
            validated_corrections(&[("corrections.toml", &new_correction(kind, "Perfect Silence", field, "1", "2"))])
                .unwrap_err();
        assert!(error.contains("Perfect Silence") && error.contains(field), "{kind}.{field}: {error}");
    }
}

#[test]
fn allows_the_quest_facts_his_files_carry_and_not_the_wikis() {
    for (field, from, to) in [
        ("level", "1", "2"),
        ("epic_level", "\"null\"", "30"),
        ("favor", "1", "2"),
        ("is_raid", "0", "1"),
        ("pack", "\"Vault of Night\"", "\"Devil Assault\""),
        ("patron", "\"null\"", "\"The Twelve\""),
    ] {
        validated_corrections(&[("corrections.toml", &new_correction("quest", "Plane of Night", field, from, to))])
            .unwrap_or_else(|error| panic!("{field}: {error}"));
    }
    for wiki_field in ["is_free_to_play", "legendary_level"] {
        let error = validated_corrections(&[(
            "corrections.toml",
            &new_correction("quest", "Plane of Night", wiki_field, "0", "1"),
        )])
        .unwrap_err();
        assert!(error.contains(wiki_field) && error.contains("wiki"), "{error}");
    }
}

#[test]
fn rejects_a_correction_without_a_source_url_reason_or_real_read_date() {
    let correction = new_correction("augment", "Perfect Silence", "min_level", "30", "29");
    let broken = |edit: fn(&mut ddo_etl::corrections::Correction)| {
        let mut broken = correction.clone();
        edit(&mut broken.0[0]);
        broken
    };
    for (broken_correction, expected_text) in [
        (broken(|entry| entry.source = "the wiki".to_string().into()), "source"),
        (broken(|entry| entry.reason = String::new()), "reason"),
        (broken(|entry| entry.read = "2026-02-30".into()), "2026-02-30"),
        (broken(|entry| entry.to = CorrectionValue::Integer(30)), "to"),
    ] {
        let error = validated_corrections(&[("corrections.toml", &broken_correction)]).unwrap_err();
        assert!(error.contains("Perfect Silence") && error.contains(expected_text), "{expected_text}: {error}");
    }
}

#[test]
fn an_effect_rename_must_cite_a_wiki_page_but_a_template_may_cite_the_internal_check() {
    let internal_source = "internal:effect-name-and-description";
    let wiki_source = "https://ddowiki.com/page/Test";
    let correction = |field: &str, source: &str| {
        new_correction("effect", "Echoes of 2006", field, "\"Echoes of 2006\"", "\"Earthen Guard\"").sourced(source)
    };
    let error = validated_corrections(&[("corrections.toml", &correction("name", internal_source))]).unwrap_err();
    assert!(error.contains("Echoes of 2006") && error.contains("rename") && error.contains("wiki"), "{error}");
    assert!(validated_corrections(&[("corrections.toml", &correction("name", wiki_source))]).is_ok());
    for template_field in ["verbose_name_template", "description_template"] {
        assert!(validated_corrections(&[("corrections.toml", &correction(template_field, internal_source))]).is_ok());
    }
}

#[test]
fn build_fails_naming_a_row_his_files_lack() {
    let error =
        built_db_with(&[("corrections.toml", &new_correction("augment", "Missing Gem", "min_level", "30", "29"))])
            .unwrap_err();
    assert!(error.contains("Missing Gem") && error.contains("corrections.toml"), "{error}");
}

#[test]
fn applies_a_correction_whose_from_matches_and_records_it() {
    let (db, report) =
        built_db_with(&[("corrections.toml", &new_correction("augment", "Perfect Silence", "min_level", "30", "29"))])
            .unwrap();
    assert_eq!(augment_min_levels(&db, "Perfect Silence"), [Some(29)]);
    assert_eq!(
        recorded_corrections(&db),
        [("augment".into(), "Perfect Silence".into(), "min_level".into(), "30".into(), "29".into())]
    );
    let (reason, source, read): (String, String, String) = db
        .query_row("SELECT reason, source, read FROM corrections", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    assert_eq!(
        (reason.as_str(), source.as_str(), read.as_str()),
        ("Test reason.", "https://ddowiki.com/page/Test", "2026-09-29")
    );
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
    assert!(report.stale_corrections.is_empty());
}

#[test]
fn effect_template_and_owner_link_value_corrections_keep_one_effect() {
    let source = fixture::xml_source();
    fixture::write_ship_kickers(source.path());
    let effect_template =
        new_correction("effect", "Riposte", "verbose_name_template", "\"Riposte +{1}\"", "\"Riposte {1}\"");
    let owner_value = new_correction("item_effect", "Ship Kickers", "value", "3", "6") + "effect = \"Riposte\"\n";
    let (db, report) = built_db_from(source.path(), &[("effect.toml", &(effect_template + &owner_value))]).unwrap();
    assert_eq!(report.correction_applied_count, 2);
    let rows: Vec<(String, Option<i64>)> = db
        .prepare(
            "SELECT e.name, ie.value FROM item_effects ie JOIN effects e ON e.id = ie.effect_id
               JOIN items i ON i.id = ie.item_id WHERE i.name = 'Ship Kickers' AND e.name = 'Riposte'",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(rows, [("Riposte".to_string(), Some(6))]);
    let template: String =
        db.query_row("SELECT verbose_name_template FROM effects WHERE name = 'Riposte'", [], |row| row.get(0)).unwrap();
    assert_eq!(template, "Riposte {1}");
}

#[test]
fn lines_corrections_reject_legacy_effect_name_upstreams() {
    let rejected = [
        ("'Doublestrike and Doubleshot'", "['Doublestrike', 'Doubleshot']"),
        ("['Doublestrike', 'Doubleshot']", "'Doublestrike and Doubleshot'"),
    ]
    .map(|(upstream, corrected)| {
        let correction = new_qualified_correction(
            "augment_bonus",
            "Woeful Shadow (Legendary)",
            "stat = 'Doublestrike and Doubleshot'\nbonus_type = 'Profane'",
            "lines",
            upstream,
            corrected,
        );
        match validated_corrections(&[("legacy-lines.toml", &correction)]) {
            Ok(_) => false,
            Err(error) => {
                assert!(error.contains("legacy-lines.toml") && error.contains("upstream stat sets"), "{error}");
                true
            }
        }
    });
    assert_eq!(rejected, [true, true], "both legacy directions must be rejected");
}

#[test]
fn stat_set_splits_need_no_upstream_effect_definition() {
    let source = fixture::xml_source();
    fixture::write_stat_set_split_augments(source.path());
    let group_name = "Magical Resistance Rating Cap, Dodge Cap and Max Dex Bonus (Armor)";
    let canonical = fixture::grouping_case_database(|_| {});
    assert_eq!(
        canonical
            .query_row("SELECT COUNT(*) FROM effects WHERE name=?1", [group_name], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
    let index = ddo_etl::groupings::Groupings::from_database(&canonical).unwrap();
    let curse = new_qualified_correction(
        "augment_bonus",
        "Curse of Intangibility",
        "stat = 'Magical Resistance Rating Cap, Dodge Cap, Max Dex Bonus (Armor)'\nbonus_type = 'Fortune'",
        "lines",
        "[['Magical Resistance Rating Cap', 'Dodge Cap', 'Max Dex Bonus (Armor)']]",
        "['Magical Resistance Rating Cap', 'Dodge Cap', 'Max Dex Bonus (Armor)']",
    );
    let corrections =
        curse + &fixture::dolorous_stat_set_corrections() + &CorrectionSet(fixture::fixture_icon_corrections().entries);
    let (db, report) = built_db_from_index(&index, source.path(), &[("stat-set-splits.toml", &corrections)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (corrections.0.len(), 0));
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM effects WHERE name=?1", [group_name], |row| row.get::<_, i64>(0)).unwrap(),
        0
    );
    let links = |owner: &str| {
        db.prepare("SELECT e.name,l.value FROM augment_effects l JOIN effects e ON e.id=l.effect_id JOIN augments a ON a.id=l.augment_id WHERE a.name=?1 ORDER BY l.sort_order").unwrap().query_map([owner], |row| Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
    };
    assert_eq!(
        links("Curse of Intangibility"),
        [("Magical Resistance Rating Cap".into(), 2), ("Dodge Cap".into(), 2), ("Max Dex Bonus (Armor)".into(), 2)]
    );
    assert_eq!(
        links("Dolorous Invigorator (Legendary)"),
        [("Spell Focus Mastery".into(), 2), ("Combat Mastery".into(), 2), ("Assassinate".into(), 2)]
    );
    let exported = tempfile::tempdir().unwrap();
    ddo_etl::tree::export_database(&db, exported.path()).unwrap();
    copy_fixture_icons(exported.path());
    for name in ["DeckOfCurses.png", "Lamordia.png"] {
        std::fs::copy(
            source.path().join("AugmentImages").join(name),
            exported.path().join("icons/augments").join(name),
        )
        .unwrap();
    }
    let mut rebuilt = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(exported.path(), &mut rebuilt).unwrap();
    assert_eq!(recorded_corrections(&db), recorded_corrections(&rebuilt));
    let mut overlapping = corrections.clone();
    let mut exact_amount = overlapping.0[0].clone();
    exact_amount.bonus_value = Some(2);
    overlapping.0.push(exact_amount);
    let result = built_db_from_index(&index, source.path(), &[("overlapping-lines.toml", &overlapping)]);
    assert!(result.is_err(), "overlapping split corrections must fail");
    let message = result.err().unwrap();
    assert!(message.contains("overlapping split corrections"), "{message}");
    let path = walkdir::WalkDir::new(exported.path())
        .into_iter()
        .map(Result::unwrap)
        .find(|entry| entry.file_name() == "curse-of-intangibility.toml")
        .unwrap()
        .into_path();
    let original = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Value = toml::from_str(&original).unwrap();
    let mut annotated = parsed.clone();
    let annotation: toml::Value = toml::from_str(
        "kind = 'augment_bonus'\nfield = 'lines'\nqualifier = 'Magical Resistance Rating Cap, Dodge Cap, Max Dex Bonus (Armor) / Fortune'\nintended = [['Magical Resistance Rating Cap', 'Dodge Cap', 'Max Dex Bonus (Armor)']]\nreason = 'The game shows these as separate lines.'\nin_game = true\nread = '2026-10-28'"
    ).unwrap();
    annotated.as_table_mut().unwrap().insert("game_bug_correction".into(), toml::Value::Array(vec![annotation]));
    std::fs::write(&path, toml::to_string(&annotated).unwrap()).unwrap();
    let mut annotated_db = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(exported.path(), &mut annotated_db).unwrap();
    assert_eq!(
        annotated_db
            .query_row("SELECT COUNT(*) FROM note_lines WHERE note_kind='game_bug'", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        3
    );
    for (upstream, expected) in [
        (vec![vec!["Unknown Stat", "Dodge Cap", "Max Dex Bonus (Armor)"]], "unknown upstream stat"),
        (vec![vec!["Dodge Cap", "Max Dex Bonus (Armor)"]], "qualifier"),
        (vec![vec!["Magical Resistance Rating Cap", "Dodge Cap", "Dodge Cap"]], "duplicate"),
        (vec![Vec::new()], "empty"),
    ] {
        let mut edited = parsed.clone();
        edited["ddobuilder_correction"][0]["upstream"] = toml::Value::try_from(upstream).unwrap();
        std::fs::write(&path, toml::to_string(&edited).unwrap()).unwrap();
        let error =
            ddo_etl::tree::build_from_data(exported.path(), &mut Connection::open_in_memory().unwrap()).unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("curse-of-intangibility.toml") && message.contains(expected), "{message}");
    }
    let mut duplicated = parsed.clone();
    let duplicate = duplicated["effect"][1].clone();
    duplicated["effect"].as_array_mut().unwrap().push(duplicate);
    std::fs::write(&path, toml::to_string(&duplicated).unwrap()).unwrap();
    let error =
        ddo_etl::tree::build_from_data(exported.path(), &mut Connection::open_in_memory().unwrap()).unwrap_err();
    assert!(format!("{error:#}").contains("curse-of-intangibility.toml"), "{error:#}");
    let mut wrong_partition = parsed;
    wrong_partition["effect"][1]["name"] = toml::Value::String("Dodge".into());
    std::fs::write(&path, toml::to_string(&wrong_partition).unwrap()).unwrap();
    let error =
        ddo_etl::tree::build_from_data(exported.path(), &mut Connection::open_in_memory().unwrap()).unwrap_err();
    assert!(format!("{error:#}").contains("curse-of-intangibility.toml"), "{error:#}");
}

#[test]
fn stat_set_merges_match_the_expanded_buff_stats() {
    let source = fixture::xml_source();
    std::fs::write(
        source.path().join("Items/SkirmishersBracers.item"),
        include_str!("fixtures/effect-groups/SkirmishersBracers.item"),
    )
    .unwrap();
    std::fs::create_dir_all(source.path().join("ItemImages/Bracers")).unwrap();
    std::fs::write(
        source.path().join("ItemImages/Bracers/Bracers_13.png"),
        include_bytes!("fixtures/effect-groups/icons/Bracers_13.png"),
    )
    .unwrap();
    for amount in [None, Some(8), Some(9)] {
        let qualifier = format!(
            "stat = 'Doublestrike, Doubleshot'\nbonus_type = 'Enhancement'{}",
            amount.map(|value| format!("\nbonus_value = {value}")).unwrap_or_default()
        );
        let correction = new_qualified_correction(
            "item_bonus",
            "Skirmisher's Bracers (Level 26)",
            &qualifier,
            "lines",
            "[['Doublestrike'], ['Doubleshot']]",
            "'Doublestrike and Doubleshot'",
        );
        let (db, report) =
            built_db_from_index(fixture::grouping_cases(), source.path(), &[("skirmisher.toml", &correction)]).unwrap();
        let links = db.prepare("SELECT e.name,l.value FROM item_effects l JOIN effects e ON e.id=l.effect_id JOIN items i ON i.id=l.item_id WHERE i.name=?1 ORDER BY l.sort_order").unwrap().query_map(["Skirmisher's Bracers (Level 26)"], |row| Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        if amount == Some(9) {
            assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 1));
            assert_eq!(links, [("Dexterity".into(), 9), ("Doublestrike".into(), 8), ("Doubleshot".into(), 8)]);
        } else {
            assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
            assert_eq!(links, [("Dexterity".into(), 9), ("Doublestrike and Doubleshot".into(), 8)]);
        }
    }
}

#[test]
fn a_lines_merge_combines_item_buffs_and_reports_missing_members() {
    let source = fixture::xml_source();
    let correction = fixture::search_and_spot_merge();
    for amount in [20, 21, 0] {
        fixture::write_search_and_spot_item(source.path(), amount, true);
        let (separate, _) = built_db_from_index(fixture::grouping_cases(), source.path(), &[]).unwrap();
        assert_eq!(
            fixture::search_and_spot_lines(&separate),
            [("Search".into(), Some(amount)), ("Spot".into(), Some(amount))]
        );
        let (merged, report) =
            built_db_from_index(fixture::grouping_cases(), source.path(), &[("merged-lines.toml", &correction)])
                .unwrap();
        assert_eq!(fixture::search_and_spot_lines(&merged), [("Search and Spot".into(), Some(amount))]);
        assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
        let bonuses = |db: &Connection| {
            db.prepare("SELECT s.name,b.bonus_type_id,b.amount FROM owner_bonuses b JOIN effects s ON s.id=b.stat_id JOIN items i ON i.id=b.owner_id WHERE b.owner_kind='item' AND i.name='Legendary Keylock Ring' ORDER BY s.name")
                .unwrap().query_map([], |row| Ok((row.get::<_, String>(0)?,row.get::<_, i64>(1)?,row.get::<_, i64>(2)?)))
                .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };
        assert_eq!(bonuses(&merged), bonuses(&separate));
    }
    fixture::write_search_and_spot_item(source.path(), 20, true);
    let item_path = source.path().join("Items/SearchAndSpot.item");
    let original = std::fs::read_to_string(&item_path).unwrap();
    std::fs::write(&item_path, original.replacen("<Value1>20</Value1>", "<Value1>21</Value1>", 1)).unwrap();
    let error = built_db_from_index(fixture::grouping_cases(), source.path(), &[("merged-lines.toml", &correction)])
        .unwrap_err();
    assert!(error.contains("grouped lines have different values for slot 1"), "{error}");
    let search_buff = original.split_once("<Buff>").unwrap().1.split_once("</Buff>").unwrap().0;
    std::fs::write(
        &item_path,
        original.replace("</Item>\n</Items>", &format!("<Buff>{search_buff}</Buff></Item>\n</Items>")),
    )
    .unwrap();
    let error = built_db_from_index(fixture::grouping_cases(), source.path(), &[("merged-lines.toml", &correction)])
        .unwrap_err();
    assert!(error.contains("matches several links"), "{error}");
    std::fs::write(
        &item_path,
        original.replacen("<BonusType>Competence</BonusType>", "<BonusType>Insight</BonusType>", 1),
    )
    .unwrap();
    let (db, report) =
        built_db_from_index(fixture::grouping_cases(), source.path(), &[("merged-lines.toml", &correction)]).unwrap();
    assert_eq!(fixture::search_and_spot_lines(&db), [("Search".into(), Some(20)), ("Spot".into(), Some(20))]);
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 1));
    fixture::write_search_and_spot_item(source.path(), 20, false);
    let (db, report) =
        built_db_from_index(fixture::grouping_cases(), source.path(), &[("merged-lines.toml", &correction)]).unwrap();
    assert_eq!(fixture::search_and_spot_lines(&db), [("Search".into(), Some(20))]);
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 1));
    assert!(matches!(&report.stale_corrections[0].cause,
        StaleCorrectionCause::ValueChanged { maetrim_value, .. } if maetrim_value == "\"no such bonus\""));
}

#[test]
fn a_two_slot_lines_merge_preserves_each_slots_stat_amounts() {
    let source = fixture::xml_source();
    let correction = fixture::swiftness_merge();
    for movement in [30, 25, 0] {
        fixture::write_swiftness_augment(source.path(), movement);
        let (separate, _) = built_db_from_index(fixture::grouping_cases(), source.path(), &[]).unwrap();
        let (merged, report) =
            built_db_from_index(fixture::grouping_cases(), source.path(), &[("swiftness.toml", &correction)]).unwrap();
        assert_eq!(fixture::swiftness_links(&merged), [("Speed".into(), Some(movement), Some(15))]);
        assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
        let bonuses = |db: &Connection| {
            db.prepare("SELECT s.name,bt.name,b.amount FROM owner_bonuses b JOIN effects s ON s.id=b.stat_id JOIN bonus_types bt ON bt.id=b.bonus_type_id JOIN augments a ON a.id=b.owner_id WHERE b.owner_kind='augment' AND a.name='Topaz of Swiftness' ORDER BY s.name")
                .unwrap().query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?)))
                .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };
        assert_eq!(bonuses(&merged), bonuses(&separate));
        let expected = [
            ("Melee Attack Speed".into(), "Enhancement".into(), 15),
            ("Movement Speed".into(), "Enhancement".into(), movement),
            ("Ranged Attack Speed".into(), "Enhancement".into(), 15),
        ]
        .into_iter()
        .filter(|(_, _, amount)| *amount != 0)
        .collect::<Vec<_>>();
        assert_eq!(bonuses(&merged), expected);
    }
    fixture::write_swiftness_augment(source.path(), 30);
    let path = source.path().join("Augments/Topaz.Augments.xml");
    let original = std::fs::read_to_string(&path).unwrap();
    let attack = original
        .split("<Effect>")
        .find(|effect| effect.contains("<Type>Weapon_Alacrity</Type>"))
        .unwrap()
        .split_once("</Effect>")
        .unwrap()
        .0;
    let unequal = format!(
        "<Effect>{}</Effect><Effect>{}</Effect>",
        attack.replace("<Item>All</Item>", "<Item>Melee</Item>"),
        attack
            .replace("<Item>All</Item>", "<Item>Ranged</Item>")
            .replace("<Amount size=\"1\">15</Amount>", "<Amount size=\"1\">10</Amount>")
    );
    std::fs::write(&path, original.replace(&format!("<Effect>{attack}</Effect>"), &unequal)).unwrap();
    let mut separate_sources = correction.clone();
    separate_sources.0[0].from = CorrectionValue::StatSets(vec![
        vec!["Movement Speed".into()],
        vec!["Melee Attack Speed".into()],
        vec!["Ranged Attack Speed".into()],
    ]);
    let error = built_db_from_index(fixture::grouping_cases(), source.path(), &[("swiftness.toml", &separate_sources)])
        .unwrap_err();
    assert!(error.contains("slot 2") && error.contains("different values"), "{error}");
    std::fs::write(&path, original.replace(&format!("<Effect>{attack}</Effect>"), "")).unwrap();
    let (missing, report) =
        built_db_from_index(fixture::grouping_cases(), source.path(), &[("swiftness.toml", &correction)]).unwrap();
    assert_eq!(fixture::swiftness_links(&missing), [("Movement Speed".into(), Some(30), None)]);
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 1));
    assert!(
        matches!(&report.stale_corrections[0].cause, StaleCorrectionCause::ValueChanged { maetrim_value, .. } if maetrim_value=="\"no such bonus\"")
    );
}

#[test]
fn source_type_and_value_corrections_precede_additions_and_merges() {
    let source = fixture::xml_source();
    fixture::write_search_and_spot_item(source.path(), 20, true);
    let mut notes = fixture::search_and_spot_merge();
    notes.0[0].bonus_type = Some("Insight".into());
    for stat in ["Search", "Spot"] {
        let qualifier = format!("stat = {stat:?}\nbonus_type = 'Competence'");
        notes = notes
            + &fixture::new_qualified_correction(
                "item_bonus",
                "Legendary Keylock Ring",
                &qualifier,
                "value",
                "20",
                "21",
            );
        notes = notes
            + &fixture::new_qualified_correction(
                "item_bonus",
                "Legendary Keylock Ring",
                &qualifier,
                "bonus_type",
                "'Competence'",
                "'Insight'",
            );
    }
    notes = notes
        + &fixture::new_qualified_correction(
            "item_bonus",
            "Legendary Keylock Ring",
            "",
            "add",
            "'null'",
            "{stat = 'Spot', bonus_type = 'Insight', value = 21}",
        );
    let (db, report) =
        built_db_from_index(fixture::grouping_cases(), source.path(), &[("corrected-merge.toml", &notes)]).unwrap();
    assert_eq!(fixture::search_and_spot_lines(&db), [("Search and Spot".into(), Some(21))]);
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (5, 1));
    assert!(
        matches!(&report.stale_corrections[0].cause, StaleCorrectionCause::ValueChanged {maetrim_value, ..} if maetrim_value.contains("21"))
    );
}

#[test]
fn pending_corrections_resolve_group_targets_inside_named_effects() {
    let source = fixture::xml_source();
    fixture::write_command_owners(source.path());
    let corrections = fixture::new_qualified_correction(
        "augment_bonus",
        "Brightbane Emerald",
        "stat = 'Charisma Skills'\nbonus_type = 'Insight'",
        "value",
        "5",
        "4",
    ) + &fixture::new_qualified_correction(
        "augment_bonus",
        "Brightbane Emerald",
        "",
        "add",
        "'null'",
        "{stat='Charisma Skills',bonus_type='Insight',value=4}",
    );
    let (db, report) =
        built_db_from_index(fixture::grouping_cases(), source.path(), &[("brightbane.toml", &corrections)]).unwrap();
    let links = db.prepare("SELECT e.name,l.value,l.value2 FROM augment_effects l JOIN effects e ON e.id=l.effect_id JOIN augments a ON a.id=l.augment_id WHERE a.name='Brightbane Emerald'").unwrap().query_map([],|row| Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(links, [("Command".into(), 4, -6)]);
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 1));
}

#[test]
fn a_lines_merge_and_split_cannot_both_apply_to_one_line() {
    let qualifier = "equipped_count = 3\nstat = 'Doublestrike, Doubleshot'\nbonus_type = 'Artifact'";
    let merge = new_qualified_correction(
        "set_tier_bonus",
        "Kundarak Delving Equipment",
        qualifier,
        "lines",
        "[[\"Doublestrike\"], [\"Doubleshot\"]]",
        "\"Doublestrike and Doubleshot\"",
    );
    let split = new_qualified_correction(
        "set_tier_bonus",
        "Kundarak Delving Equipment",
        qualifier,
        "lines",
        "[[\"Doublestrike\", \"Doubleshot\"]]",
        "[\"Doublestrike\", \"Doubleshot\"]",
    );
    let error = built_db_with(&[("both-directions.toml", &(merge + &split))]).unwrap_err();
    assert!(error.contains("merge") && error.contains("split") && error.contains("Doublestrike"), "{error}");
}

#[test]
fn archons_resistances_stay_separate_without_a_lines_merge() {
    let source = fixture::xml_source();
    let buffs_path = source.path().join("ItemBuffs.xml");
    let buffs = std::fs::read_to_string(&buffs_path).unwrap();
    std::fs::write(
        &buffs_path,
        buffs.replace("</Buffs>", &format!("{}</Buffs>", include_str!("fixtures/effect-groups/Sheltering.Buff.xml"))),
    )
    .unwrap();
    let path = source.path().join("SetBonuses.xml");
    let original = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        original.replace(
            "</SetBonuses>",
            &format!("{}</SetBonuses>", include_str!("fixtures/effect-groups/Archons.SetBonus.xml")),
        ),
    )
    .unwrap();
    let names = |db: &Connection| {
        db.prepare("SELECT e.name FROM set_bonus_tier_effects l JOIN effects e ON e.id=l.effect_id JOIN set_bonus_tiers t ON t.id=l.tier_id JOIN set_bonuses s ON s.id=t.set_id WHERE s.name='Legendary Armaments of the Archons' AND t.equipped_count=3 ORDER BY l.sort_order")
            .unwrap().query_map([], |row| row.get::<_, String>(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
    };
    let (separate, _) = built_db_from(source.path(), &[]).unwrap();
    assert_eq!(
        names(&separate),
        ["Physical Resistance Rating", "Magical Resistance Rating", "Armor Class Percentage", "Maximum Hit Points"]
    );
}

#[test]
fn an_augment_lines_merge_accepts_a_fixed_type_grouping() {
    let source = fixture::xml_source();
    let original = include_str!("fixtures/effect-groups/WoefulShadow.Augments.xml");
    let first_effect = original.split_once("    <Effect>").unwrap().1.split_once("    </Effect>").unwrap().0;
    let separate_effects = ["Fire", "Electric"]
        .map(|target| {
            format!(
                "    <Effect>{}    </Effect>",
                first_effect
                    .replace(
                        "<Type>Doublestrike</Type>\n      <Type>Doubleshot</Type>",
                        &format!("<Type>SpellPower</Type><Item>{target}</Item>")
                    )
                    .replace("<Bonus>Profane</Bonus>", "<Bonus>Equipment</Bonus>")
            )
        })
        .join("\n");
    std::fs::write(
        source.path().join("Augments/WoefulShadow.Augments.xml"),
        original.replacen(&format!("    <Effect>{first_effect}    </Effect>"), &separate_effects, 1),
    )
    .unwrap();
    std::fs::copy(
        fixtures_dir().join("effect-groups/icons/Lamordia.png"),
        source.path().join("AugmentImages/Lamordia.png"),
    )
    .unwrap();
    let correction = new_qualified_correction(
        "augment_bonus",
        "Woeful Shadow (Legendary)",
        "stat = \"Fire Spell Power, Electric Spell Power\"\nbonus_type = 'Equipment'",
        "lines",
        "[[\"Fire Spell Power\"], [\"Electric Spell Power\"]]",
        "'Power of the Firestorm'",
    );
    let (merged, report) = built_db_from(source.path(), &[("fixed-merge.toml", &correction)]).unwrap();
    let lines = merged.prepare("SELECT e.name,l.bonus_type_id,l.value FROM augment_effects l JOIN effects e ON e.id=l.effect_id JOIN augments a ON a.id=l.augment_id WHERE a.name='Woeful Shadow (Legendary)' ORDER BY l.sort_order")
        .unwrap().query_map([], |row| Ok((row.get::<_, String>(0)?,row.get::<_, Option<i64>>(1)?,row.get::<_, Option<i64>>(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(lines[0], ("Power of the Firestorm".into(), None, Some(3)));
    assert_eq!(lines.len(), 3);
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
}

#[test]
fn a_lines_merge_uses_the_grouping_named_by_its_correction() {
    let source = fixture::xml_source();
    let original = include_str!("fixtures/effect-groups/WoefulShadow.Augments.xml");
    let first_effect = original.split_once("    <Effect>").unwrap().1.split_once("    </Effect>").unwrap().0;
    let separate_effects = ["Trip", "Sunder", "Stun"]
        .map(|target| {
            format!(
                "    <Effect>{}    </Effect>",
                first_effect.replace(
                    "<Type>Doublestrike</Type>\n      <Type>Doubleshot</Type>",
                    &format!("<Type>TacticalDC</Type><Item>{target}</Item>")
                )
            )
        })
        .join("\n");
    std::fs::write(
        source.path().join("Augments/WoefulShadow.Augments.xml"),
        original
            .replacen(&format!("    <Effect>{first_effect}    </Effect>"), &separate_effects, 1)
            .replace("Woeful Shadow (Legendary)", "Dolorous Invigorator (Legendary)"),
    )
    .unwrap();
    std::fs::copy(
        fixtures_dir().join("effect-groups/icons/Lamordia.png"),
        source.path().join("AugmentImages/Lamordia.png"),
    )
    .unwrap();
    let correction = new_qualified_correction(
        "augment_bonus",
        "Dolorous Invigorator (Legendary)",
        "stat = \"Trip DC, Sunder DC, Stun DC\"\nbonus_type = 'Profane'",
        "lines",
        "[[\"Trip DC\"], [\"Sunder DC\"], [\"Stun DC\"]]",
        "'Combat Mastery'",
    );
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        fixture::grouping_cases(),
        source.path(),
        &WikiOverrides::default(),
        &validated_corrections(&[("explicit-line.toml", &correction)]).unwrap(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    let name = db.query_row("SELECT e.name FROM augment_effects l JOIN effects e ON e.id=l.effect_id JOIN augments a ON a.id=l.augment_id WHERE a.name='Dolorous Invigorator (Legendary)' AND l.value=3", [], |row| row.get::<_, String>(0)).unwrap();
    assert_eq!(name, "Combat Mastery");
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
}

#[test]
fn item_line_splits_are_rejected_until_the_item_writer_applies_them() {
    let correction = new_qualified_correction(
        "item_bonus",
        "Lantern of the Abyss",
        "stat = \"Fire Spell Power, Electric Spell Power\"\nbonus_type = 'Equipment'",
        "lines",
        "[[\"Fire Spell Power\", \"Electric Spell Power\"]]",
        "[\"Fire Spell Power\", \"Electric Spell Power\"]",
    );
    let error = validated_corrections(&[("item-lines.toml", &correction)]).unwrap_err();
    assert!(error.contains("item_bonus") && error.contains("before inserting links"), "{error}");
}

#[test]
fn grouping_value_corrections_use_the_corrected_canonical_definition() {
    let source = fixture::xml_source();
    fixture::write_ship_kickers(source.path());
    let scale = new_qualified_correction("effect", "Riposte", "stat = 'Armor Class'", "scale", "1.0", "0.5");
    let (canonical, report) = built_db_from(source.path(), &[("riposte-scale.toml", &scale)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
    let groupings = ddo_etl::groupings::Groupings::from_database(&canonical).unwrap();
    let correction = new_qualified_correction(
        "item_bonus",
        "Ship Kickers",
        "stat = 'Riposte'\nbonus_type = 'Insight'",
        "value",
        "3",
        "6",
    );
    let error = built_db_from_index(&groupings, source.path(), &[("riposte-value.toml", &correction)]).unwrap_err();
    assert!(error.contains("Riposte") && error.contains("cannot take one value"), "{error}");
}

#[test]
fn grouping_definition_queries_do_not_grow_with_corrections() {
    use rusqlite::trace::{TraceEvent, TraceEventCodes};
    use std::cell::Cell;
    thread_local! { static READS: Cell<usize> = const { Cell::new(0) }; }
    fn trace(event: TraceEvent<'_>) {
        if let TraceEvent::Stmt(statement, _) = event {
            let sql = statement.sql();
            if sql.starts_with("SELECT e.id, e.name, e.slug, e.is_stat, e.is_group")
                || sql.starts_with("SELECT b.effect_id, b.target_effect_id, bt.name, b.amount_from")
            {
                READS.set(READS.get() + 1);
            }
        }
    }
    let index = fixture::groupings();
    let query_count = |count| {
        let source = fixture::xml_source();
        let path = source.path().join("SetBonuses.xml");
        let xml = std::fs::read_to_string(&path).unwrap();
        let start = xml.find("<Type>Kundarak Delving Equipment</Type>").unwrap();
        let opening = xml[..start].rfind("<SetBonus>").unwrap();
        let closing = start + xml[start..].find("</SetBonus>").unwrap() + "</SetBonus>".len();
        let template = &xml[opening..closing];
        let mut corrections = CorrectionSet::default();
        let mut additions = String::new();
        for index in 0..count {
            let name = format!("Kundarak Query Probe {index}");
            additions.push_str(&template.replace("Kundarak Delving Equipment", &name));
            corrections = corrections
                + &new_qualified_correction(
                    "set_tier_bonus",
                    &name,
                    "equipped_count = 3\nstat = 'Doublestrike and Doubleshot'\nbonus_type = 'Artifact'",
                    "value",
                    "5",
                    "6",
                );
        }
        std::fs::write(path, xml.replace("</SetBonuses>", &format!("{additions}</SetBonuses>"))).unwrap();
        let corrections = validated_corrections(&[("queries.toml", &corrections)]).unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        READS.set(0);
        db.trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(trace));
        let report = build_database(
            index,
            source.path(),
            &WikiOverrides::default(),
            &corrections,
            &mut db,
            &fixture_dataset_version(),
        )
        .unwrap();
        db.trace_v2(TraceEventCodes::empty(), None);
        assert_eq!(report.correction_applied_count, count);
        READS.get()
    };
    assert_eq!(query_count(1), query_count(50));
}

#[test]
fn a_split_preserves_named_single_stat_partitions() {
    let index = fixture::grouping_cases();
    let source = fixture::xml_source();
    fixture::write_eye_of_lamannia(source.path());
    let correction = new_qualified_correction(
        "augment_bonus",
        "The Eye of Lamannia",
        "stat = 'Force Spell Power, Positive Spell Power'\nbonus_type = 'Equipment'",
        "lines",
        "[['Force Spell Power', 'Positive Spell Power']]",
        "['Devotion', 'Impulse']",
    );
    let (before, _) = built_db_from_index(index, source.path(), &[]).unwrap();
    let (after, report) =
        built_db_from_index(index, source.path(), &[("the-eye-of-lamannia.toml", &correction)]).unwrap();
    let lines = after.prepare("SELECT e.name,l.value,bt.name FROM augment_effects l JOIN effects e ON e.id=l.effect_id JOIN bonus_types bt ON bt.id=l.bonus_type_id JOIN augments a ON a.id=l.augment_id WHERE a.name='The Eye of Lamannia' AND l.value=146 ORDER BY l.sort_order").unwrap()
        .query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?,row.get::<_,String>(2)?)))
        .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(lines, [("Devotion".into(), 146, "Equipment".into()), ("Impulse".into(), 146, "Equipment".into())]);
    let mut previous_bonuses = augment_bonus_rows(&before, "The Eye of Lamannia");
    let mut split_bonuses = augment_bonus_rows(&after, "The Eye of Lamannia");
    previous_bonuses.sort();
    split_bonuses.sort();
    assert_eq!(previous_bonuses, split_bonuses);
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
}

#[test]
fn a_named_single_stat_partition_must_accept_the_upstream_value_and_type() {
    let rejected = [
        ("amount_from = 0\nconstant = 146\n", "Devotion +146", "constant"),
        ("amount_from = 2\n", "Devotion +{2}", "second slot"),
        ("amount_from = 1\nbonus_type = 'Insight'\n", "Devotion +{1}", "fixed type"),
    ].map(|(definition, template, expected)| {
        let canonical = fixture::grouping_case_database(|root| {
            std::fs::write(root.join("effects/devotion.toml"),
                format!("name = 'Devotion'\nverbose_name_template = {template:?}\n[[bonus]]\n{definition}rounding = 'down'\nscale = 1.0\ntarget = 'Positive Spell Power'\n")
            ).unwrap();
        });
        let index = ddo_etl::groupings::Groupings::from_database(&canonical).unwrap();
        let source = fixture::xml_source();
        fixture::write_eye_of_lamannia(source.path());
        let correction = new_qualified_correction(
            "augment_bonus", "The Eye of Lamannia",
            "stat = 'Force Spell Power, Positive Spell Power'\nbonus_type = 'Equipment'", "lines",
            "[['Force Spell Power', 'Positive Spell Power']]", "['Devotion', 'Force Spell Power']",
        );
        let result = built_db_from_index(&index, source.path(), &[("the-eye-of-lamannia.toml", &correction)]);
        if let Err(error) = &result {
            assert!(error.contains("Devotion") && error.contains("cannot take one value at Equipment"), "{expected}: {error}");
        }
        result.is_err()
    });
    assert_eq!(rejected, [true, true, true], "constant, second slot and incompatible fixed type");
}

#[test]
fn a_split_uses_its_named_partition_even_when_other_effects_share_the_stats() {
    let canonical = fixture::grouping_case_database(|root| {
        std::fs::write(root.join("effects/stat-groups/cold-negative-and-poison-spell-power.toml"),
            "name = \"Cold, Negative and Poison Spell Power\"\ndescription_template = \"{1} %b1 bonus to Cold, Negative and Poison Spell Power.\"\nverbose_name_template = \"%b1 Cold, Negative and Poison Spell Power +{1}\"\n".to_string()
            + &["Cold Spell Power", "Negative Spell Power", "Poison Spell Power"].map(|name|
                format!("\n[[bonus]]\namount_from = 1\nrounding = \"down\"\nscale = 1.0\ntarget = {name:?}\n")
            ).join("")
        ).unwrap();
    });
    let index = ddo_etl::groupings::Groupings::from_database(&canonical).unwrap();
    let source = fixture::xml_source();
    std::fs::write(
        source.path().join("Augments/WoefulShadow.Augments.xml"),
        include_str!("fixtures/effect-groups/WoefulShadow.Augments.xml").replace(
            "<Type>Doublestrike</Type>\n      <Type>Doubleshot</Type>",
            "<Type>SpellPower</Type><Item>Cold</Item><Item>Negative</Item><Item>Poison</Item>",
        ),
    )
    .unwrap();
    std::fs::copy(
        fixtures_dir().join("effect-groups/icons/Lamordia.png"),
        source.path().join("AugmentImages/Lamordia.png"),
    )
    .unwrap();
    let correction = new_qualified_correction(
        "augment_bonus",
        "Woeful Shadow (Legendary)",
        "stat = 'Cold Spell Power, Negative Spell Power, Poison Spell Power'\nbonus_type = 'Profane'",
        "lines",
        "[[\"Cold Spell Power\", \"Negative Spell Power\", \"Poison Spell Power\"]]",
        "[\"Cold Spell Power\", \"Negative and Poison Spell Power\"]",
    );
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        &index,
        source.path(),
        &WikiOverrides::default(),
        &validated_corrections(&[("partition.toml", &correction)]).unwrap(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    let names = db.prepare("SELECT e.name FROM augment_effects l JOIN effects e ON e.id=l.effect_id JOIN augments a ON a.id=l.augment_id WHERE a.name='Woeful Shadow (Legendary)' AND l.value=3 ORDER BY e.name")
        .unwrap().query_map([], |row| row.get::<_, String>(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(names, ["Cold Spell Power", "Negative and Poison Spell Power"]);
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
}

#[test]
fn fixed_type_grouping_splits_into_stat_lines_at_its_resolved_type() {
    let source = fixture::xml_source();
    std::fs::write(
        source.path().join("Augments/WoefulShadow.Augments.xml"),
        include_str!("fixtures/effect-groups/WoefulShadow.Augments.xml")
            .replace(
                "<Type>Doublestrike</Type>\n      <Type>Doubleshot</Type>",
                "<Type>SpellPower</Type><Item>Fire</Item><Item>Electric</Item>",
            )
            .replace("<Bonus>Profane</Bonus>", "<Bonus>Equipment</Bonus>"),
    )
    .unwrap();
    std::fs::copy(
        fixtures_dir().join("effect-groups/icons/Lamordia.png"),
        source.path().join("AugmentImages/Lamordia.png"),
    )
    .unwrap();
    let correction = new_qualified_correction(
        "augment_bonus",
        "Woeful Shadow (Legendary)",
        "stat = \"Fire Spell Power, Electric Spell Power\"\nbonus_type = 'Equipment'",
        "lines",
        "[[\"Fire Spell Power\", \"Electric Spell Power\"]]",
        "[\"Fire Spell Power\", \"Electric Spell Power\"]",
    );
    let (db, report) = built_db_from(source.path(), &[("fixed-type-split.toml", &correction)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
    let lines = db.prepare("SELECT e.name,bt.name,l.value FROM augment_effects l JOIN effects e ON e.id=l.effect_id JOIN bonus_types bt ON bt.id=l.bonus_type_id JOIN augments a ON a.id=l.augment_id WHERE a.name='Woeful Shadow (Legendary)' AND l.value=3 ORDER BY e.name")
        .unwrap().query_map([], |row| Ok((row.get::<_, String>(0)?,row.get::<_, String>(1)?,row.get::<_, i64>(2)?)))
        .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(
        lines,
        [("Electric Spell Power".into(), "Equipment".into(), 3), ("Fire Spell Power".into(), "Equipment".into(), 3)]
    );
}

#[test]
fn socket_option_targets_are_loaded_once_for_one_and_fifty_owners() {
    use rusqlite::trace::{TraceEvent, TraceEventCodes};
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SELECTS: AtomicUsize = AtomicUsize::new(0);
    fn trace(event: TraceEvent<'_>) {
        if let TraceEvent::Stmt(statement, _) = event {
            if statement.sql().contains("slot_type.label, option.name") {
                SELECTS.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
    for (field, count) in [("lines", 1), ("lines", 50), ("value", 1), ("value", 50)] {
        let source = fixture::xml_source();
        fixture::write_spyglass_options(source.path());
        let captured = std::fs::read_to_string(source.path().join("Items/SpyglassOptions.item")).unwrap();
        let mut corrections = Corrections::default();
        for index in 0..count {
            let mut correction = fixture::spyglass_option_correction(
                field,
                if field == "lines" { serde_json::json!([["Search", "Spot"]]) } else { serde_json::json!(20) },
                if field == "lines" {
                    CorrectionValue::Texts(vec!["Search".into(), "Spot".into()])
                } else {
                    CorrectionValue::Integer(25)
                },
            )
            .0
            .remove(0);
            let renamed = format!("{} {index}", correction.name);
            std::fs::write(
                source.path().join(format!("Items/SpyglassOptions{index}.item")),
                captured.replace(&format!("<Name>{}</Name>", correction.name), &format!("<Name>{renamed}</Name>")),
            )
            .unwrap();
            correction.name = renamed;
            corrections.entries.push(correction);
        }
        std::fs::remove_file(source.path().join("Items/SpyglassOptions.item")).unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        db.trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(trace));
        SELECTS.store(0, Ordering::SeqCst);
        let report = build_database(
            fixture::grouping_cases(),
            source.path(),
            &WikiOverrides::default(),
            &corrections,
            &mut db,
            &fixture_dataset_version(),
        )
        .unwrap();
        db.trace_v2(TraceEventCodes::empty(), None);
        assert_eq!(report.correction_applied_count, count);
        assert_eq!(SELECTS.load(Ordering::SeqCst), 1, "{field}: {count} owners");
    }
}

#[test]
fn socket_option_member_removal_precedes_grouping_and_value_correction() {
    let source = fixture::xml_source();
    fixture::write_spyglass_options(source.path());
    let mut removal = fixture::spyglass_option_correction("remove", serde_json::json!(20), CorrectionValue::Null);
    removal.0[0].stat = Some("Spot".into());
    let mut value = fixture::spyglass_option_correction("value", serde_json::json!(20), CorrectionValue::Integer(25));
    value.0[0].stat = Some("Search".into());
    let error =
        built_db_from_index(fixture::grouping_cases(), source.path(), &[("option-value.toml", &value)]).err().unwrap();
    assert!(error.contains("behind grouping") && error.contains("Search and Spot"), "{error}");
    let (db, report) = built_db_from_index(
        fixture::grouping_cases(),
        source.path(),
        &[("option-remove.toml", &removal), ("option-value.toml", &value)],
    )
    .unwrap();
    assert_eq!(fixture::spyglass_option_lines(&db), [("Search".into(), 25)]);
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (2, 0));
}

#[test]
fn socket_option_lines_split_and_merge_through_the_qualified_writer() {
    let source = fixture::xml_source();
    fixture::write_spyglass_options(source.path());
    let split = fixture::spyglass_option_correction(
        "lines",
        serde_json::json!([["Search", "Spot"]]),
        CorrectionValue::Texts(vec!["Search".into(), "Spot".into()]),
    );
    let (db, report) =
        built_db_from_index(fixture::grouping_cases(), source.path(), &[("option-split.toml", &split)]).unwrap();
    assert_eq!(fixture::spyglass_option_lines(&db), [("Search".into(), 20), ("Spot".into(), 20)]);
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
    let path = source.path().join("Items/SpyglassOptions.item");
    let original = std::fs::read_to_string(&path).unwrap();
    let separate = original.replace("<Item>Search</Item>\n          <Item>Spot</Item>",
        "<Item>Search</Item><AType>Simple</AType><Amount size=\"1\">20</Amount></Effect><Effect><Type>SkillBonus</Type><Bonus>Competence</Bonus><Item>Spot</Item>");
    assert_ne!(separate, original);
    std::fs::write(path, separate).unwrap();
    let merge = fixture::spyglass_option_correction(
        "lines",
        serde_json::json!([["Search"], ["Spot"]]),
        CorrectionValue::Text("Search and Spot".into()),
    );
    let (db, report) =
        built_db_from_index(fixture::grouping_cases(), source.path(), &[("option-merge.toml", &merge)]).unwrap();
    assert_eq!(fixture::spyglass_option_lines(&db), [("Search and Spot".into(), 20)]);
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
}

#[test]
fn socket_option_corrections_reject_every_ambiguous_match() {
    let source = fixture::xml_source();
    fixture::write_spyglass_options(source.path());
    let path = source.path().join("Items/SpyglassOptions.item");
    let original = std::fs::read_to_string(&path).unwrap();
    let socket = original.split_once("    <ItemAugment>").unwrap().1.split_once("    </ItemAugment>").unwrap().0;
    std::fs::write(
        path,
        original.replace("  </Item>", &format!("    <ItemAugment>{socket}    </ItemAugment>\n  </Item>")),
    )
    .unwrap();
    let split = fixture::spyglass_option_correction(
        "lines",
        serde_json::json!([["Search", "Spot"]]),
        CorrectionValue::Texts(vec!["Search".into(), "Spot".into()]),
    );
    let error = built_db_from_index(fixture::grouping_cases(), source.path(), &[("ambiguous-option.toml", &split)])
        .err()
        .unwrap();
    for expected in
        ["ambiguous-option.toml", "ambiguous", "socket order 0, option order 0", "socket order 2, option order 0"]
    {
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn rank_option_merges_preserve_constant_and_slot_amounts() {
    let source = fixture::xml_source();
    fixture::write_rank_options(source.path());
    let corrections = fixture::rank_option_merge("Riposte V") + &fixture::rank_option_merge("Parrying I");
    let (db, report) =
        built_db_from_index(fixture::grouping_cases(), source.path(), &[("rank-options.toml", &corrections)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (2, 0));
    let lines = db.prepare("SELECT o.name,e.name,l.value,l.value2 FROM item_augment_slot_option_effects l JOIN effects e ON e.id=l.effect_id JOIN item_augment_slot_options o ON o.id=l.option_id WHERE o.name IN ('Riposte V','Parrying I') ORDER BY o.name").unwrap().query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,Option<i64>>(2)?,row.get::<_,Option<i64>>(3)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(
        lines,
        [
            ("Parrying I".into(), "Parrying I".into(), Some(1), None),
            ("Riposte V".into(), "Riposte V".into(), None, None)
        ]
    );
    let bonuses = db.prepare("SELECT o.name,e.name,b.amount FROM owner_bonuses b JOIN item_augment_slot_options o ON b.owner_kind='item_augment_slot_option' AND b.owner_id=o.id JOIN effects e ON e.id=b.stat_id WHERE o.name IN ('Riposte V','Parrying I') ORDER BY o.name,e.name").unwrap().query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(
        bonuses,
        [
            ("Parrying I".into(), "Armor Class".into(), 1),
            ("Parrying I".into(), "Saving Throws".into(), 1),
            ("Riposte V".into(), "Armor Class".into(), 3),
            ("Riposte V".into(), "Saving Throws".into(), 2)
        ]
    );
    let path = source.path().join("Items/SpyglassOptions.item");
    let original = std::fs::read_to_string(&path).unwrap();
    let start = original.find("        <Name>Riposte V</Name>").unwrap();
    let end = start + original[start..].find("      </Augment>").unwrap();
    let option = &original[start..end];
    let saves_start = option.find("        <Effect>\n          <Type>SaveBonus</Type>").unwrap();
    for missing in [false, true] {
        let changed = if missing {
            option[..saves_start].to_string()
        } else {
            option.replace("<Amount size=\"1\">2</Amount>", "<Amount size=\"1\">4</Amount>")
        };
        std::fs::write(&path, format!("{}{}{}", &original[..start], changed, &original[end..])).unwrap();
        let merge = fixture::rank_option_merge("Riposte V");
        let result = built_db_from_index(fixture::grouping_cases(), source.path(), &[("rank-options.toml", &merge)]);
        if missing {
            let (_, report) = result.unwrap();
            assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 1));
            assert!(
                matches!(&report.stale_corrections[0].cause, StaleCorrectionCause::ValueChanged { maetrim_value, .. } if maetrim_value == "\"no such bonus\""),
                "{:?}",
                report.stale_corrections
            );
        } else {
            let error = result.err().unwrap();
            assert!(error.contains("merged values disagree with the target constants"), "{error}");
        }
    }
}

#[test]
fn socket_option_description_changes_only_the_qualified_option() {
    let source = fixture::xml_source();
    fixture::write_rank_options(source.path());
    let correction = fixture::rank_option_description_correction(source.path());
    let descriptions = |db: &Connection| {
        db.prepare("SELECT i.name,o.name,o.description FROM item_augment_slot_options o JOIN items i ON i.id=o.item_id ORDER BY i.name,o.slot_order,o.option_order").unwrap().query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,Option<String>>(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
    };
    let (before, _) = built_db_from_index(fixture::grouping_cases(), source.path(), &[]).unwrap();
    let mut expected = descriptions(&before);
    let selected = expected
        .iter_mut()
        .find(|(owner, option, _)| owner == &correction.0[0].name && option == "Riposte VI")
        .unwrap();
    selected.2 = correction.0[0].to.as_text().map(str::to_string);
    let (after, report) =
        built_db_from_index(fixture::grouping_cases(), source.path(), &[("option-description.toml", &correction)])
            .unwrap();
    assert_eq!(descriptions(&after), expected);
    let item_descriptions = |db: &Connection| {
        db.prepare("SELECT name,description FROM items ORDER BY name")
            .unwrap()
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    assert_eq!(item_descriptions(&before), item_descriptions(&after));
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
}

#[test]
fn socket_option_description_rejects_ambiguous_pairs_with_the_file() {
    let source = fixture::xml_source();
    fixture::write_rank_options(source.path());
    let correction = fixture::rank_option_description_correction(source.path());
    let path = source.path().join("Items/SpyglassOptions.item");
    let item = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        path,
        item.replace("  </Item>", &format!("{}  </Item>", include_str!("fixtures/effect-groups/RankOptions.xml"))),
    )
    .unwrap();
    let error =
        built_db_from_index(fixture::grouping_cases(), source.path(), &[("option-description.toml", &correction)])
            .err()
            .unwrap();
    for expected in [
        "option-description.toml",
        "ambiguous socket-option target",
        "socket order 2, option order 1",
        "socket order 3, option order 1",
    ] {
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn socket_option_description_is_stale_when_the_option_is_gone() {
    let source = fixture::xml_source();
    fixture::write_rank_options(source.path());
    let correction = fixture::rank_option_description_correction(source.path());
    fixture::write_spyglass_options(source.path());
    let (_, report) =
        built_db_from_index(fixture::grouping_cases(), source.path(), &[("option-description.toml", &correction)])
            .unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 1));
    assert!(
        matches!(&report.stale_corrections[0].cause, StaleCorrectionCause::ValueChanged { maetrim_value, .. } if maetrim_value == "\"no such bonus\""),
        "{:?}",
        report.stale_corrections
    );
}

#[test]
fn rank_option_merge_applies_a_source_value_correction_first() {
    let source = fixture::xml_source();
    fixture::write_rank_options(source.path());
    let corrections = fixture::rank_option_merge("Riposte VI") + &fixture::rank_option_value_correction();
    let (db, report) =
        built_db_from_index(fixture::grouping_cases(), source.path(), &[("rank-options.toml", &corrections)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (2, 0));
    let bonuses = db.prepare("SELECT e.name,b.amount FROM owner_bonuses b JOIN item_augment_slot_options o ON b.owner_kind='item_augment_slot_option' AND b.owner_id=o.id JOIN effects e ON e.id=b.stat_id WHERE o.name='Riposte VI' ORDER BY e.name").unwrap().query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(bonuses, [("Armor Class".into(), 3), ("Saving Throws".into(), 3)]);
    assert_eq!(db.query_row("SELECT COUNT(*) FROM item_augment_slot_option_effects l JOIN item_augment_slot_options o ON o.id=l.option_id JOIN effects e ON e.id=l.effect_id WHERE o.name='Riposte VI' AND e.name='Riposte VI' AND l.value IS NULL AND l.value2 IS NULL", [], |row| row.get::<_,i64>(0)).unwrap(), 1);
}

#[test]
fn socket_option_alias_round_trips_and_a_removed_option_is_stale() {
    let canonical = fixture::grouping_case_database(|root| {
        let path = root.join("effects/stat-groups/search-and-spot.toml");
        let original = std::fs::read_to_string(&path).unwrap();
        std::fs::write(
            path,
            format!("{original}\n[[alias]]\nname = 'Perception'\nverbose_name_template = '%b1 Perception +{{1}}'\n"),
        )
        .unwrap();
    });
    let index = ddo_etl::groupings::Groupings::from_database(&canonical).unwrap();
    let alias = fixture::spyglass_option_correction(
        "alias",
        serde_json::Value::Null,
        CorrectionValue::Text("Perception".into()),
    );
    let mut corrections = validated_corrections(&[("option-alias.toml", &alias)]).unwrap();
    corrections.entries.extend(fixture::fixture_icon_corrections().entries);
    for removed in [false, true] {
        let source = fixture::xml_source();
        fixture::write_spyglass_options(source.path());
        if removed {
            let path = source.path().join("Items/SpyglassOptions.item");
            let original = std::fs::read_to_string(&path).unwrap();
            let start = original.find("    <ItemAugment>").unwrap();
            let end = start + original[start..].find("    </ItemAugment>").unwrap() + "    </ItemAugment>".len();
            std::fs::write(path, format!("{}{}", &original[..start], &original[end..])).unwrap();
        }
        let mut db = Connection::open_in_memory().unwrap();
        let report = build_database(
            &index,
            source.path(),
            &WikiOverrides::default(),
            &corrections,
            &mut db,
            &fixture_dataset_version(),
        )
        .unwrap();
        if removed {
            assert_eq!(report.stale_corrections.len(), 1);
            assert!(
                matches!(&report.stale_corrections[0].cause, StaleCorrectionCause::ValueChanged { maetrim_value, .. } if maetrim_value == "\"no such bonus\"")
            );
        } else {
            assert_eq!(
                (report.correction_applied_count, report.correction_stale_count),
                (corrections.entries.len(), 0)
            );
            let chosen: String = db
                .query_row("SELECT alias FROM item_augment_slot_option_effects WHERE alias IS NOT NULL", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(chosen, "Perception");
            let exported = tempfile::tempdir().unwrap();
            ddo_etl::tree::export_database(&db, exported.path()).unwrap();
            let icons = fixtures_dir().join("data");
            for entry in walkdir::WalkDir::new(&icons).into_iter().map(Result::unwrap).filter(|entry| {
                entry.file_type().is_file() && entry.path().extension().is_some_and(|extension| extension == "png")
            }) {
                let destination = exported.path().join(entry.path().strip_prefix(&icons).unwrap());
                std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
                std::fs::copy(entry.path(), destination).unwrap();
            }
            std::fs::write(
                exported.path().join("icons/items/Trinket_17.png"),
                include_bytes!("fixtures/effect-groups/icons/Trinket_17.png"),
            )
            .unwrap();
            let mut rebuilt = Connection::open_in_memory().unwrap();
            ddo_etl::tree::build_from_data(exported.path(), &mut rebuilt).unwrap();
            let restored = Corrections::from_database(&rebuilt).unwrap();
            let restored_alias = restored.entries.iter().find(|correction| correction.field == "alias").unwrap();
            assert!(
                restored_alias.file_name.ends_with("epic-treasure-hunters-spyglass-legacy-level-20.toml"),
                "{}",
                restored_alias.file_name
            );
            assert_eq!(restored_alias.qualifier(), corrections.entries[0].qualifier());
            assert_eq!(restored_alias.to, CorrectionValue::Text("Perception".into()));
            let second = tempfile::tempdir().unwrap();
            ddo_etl::tree::export_database(&rebuilt, second.path()).unwrap();
            assert_eq!(
                ddo_etl::tree::data_file_bytes(exported.path()).unwrap(),
                ddo_etl::tree::data_file_bytes(second.path()).unwrap()
            );
            let name = "epic-treasure-hunters-spyglass-legacy-level-20.toml";
            let path = walkdir::WalkDir::new(exported.path())
                .into_iter()
                .map(Result::unwrap)
                .find(|entry| entry.file_name() == name)
                .unwrap()
                .into_path();
            let mut item: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let sockets = item["socket"].as_array_mut().unwrap();
            sockets.push(sockets[0].clone());
            std::fs::write(&path, toml::to_string(&item).unwrap()).unwrap();
            let error = format!(
                "{:#}",
                ddo_etl::tree::build_from_data(exported.path(), &mut Connection::open_in_memory().unwrap())
                    .unwrap_err()
            );
            for expected in [name, "ambiguous", "socket order 0, option order 0", "socket order 2, option order 0"] {
                assert!(error.contains(expected), "{error}");
            }
        }
    }
}

#[test]
fn socket_option_qualifiers_require_both_nonblank_selectors() {
    let valid = fixture::spyglass_option_correction(
        "alias",
        serde_json::Value::Null,
        CorrectionValue::Text("Perception".into()),
    )
    .0
    .remove(0);
    assert!(valid.validate().is_ok());
    for selectors in [
        (None, valid.option.clone()),
        (valid.socket.clone(), None),
        (Some(" ".into()), valid.option.clone()),
        (valid.socket.clone(), Some("".into())),
    ] {
        let mut invalid = valid.clone();
        invalid.socket = selectors.0;
        invalid.option = selectors.1;
        assert!(invalid.validate().is_err());
    }
    let mut wrong_kind = valid;
    wrong_kind.kind = ddo_model::enums::CorrectionKind::ItemBonus;
    assert!(wrong_kind.validate().is_err());
}

#[test]
fn feat_lines_split_and_merge_follow_the_real_writer() {
    let source = fixture::xml_source();
    fixture::write_scion_feat(source.path());
    let correction = fixture::scion_dodge_split();
    let (split, report) =
        built_db_from_index(fixture::grouping_cases(), source.path(), &[("feat-split.toml", &correction)]).unwrap();
    assert_eq!(fixture::scion_dodge_lines(&split), [("Dodge".into(), 4), ("Dodge Cap".into(), 4)]);
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
    let path = source.path().join("Feats.xml");
    let original = std::fs::read_to_string(&path).unwrap();
    let separate = original.replace(
        "<Type>DodgeBonus</Type>\n        <Type>DodgeCapBonus</Type>",
        "<Type>DodgeBonus</Type><Bonus>Feat</Bonus><AType>Simple</AType><Amount size=\"1\">4</Amount></Effect><Effect><Type>DodgeCapBonus</Type>",
    );
    assert_ne!(separate, original);
    std::fs::write(&path, separate).unwrap();
    let merge = new_qualified_correction(
        "feat_bonus",
        "Scion of the Astral Plane",
        "stat = \"Dodge, Dodge Cap\"\nbonus_type = 'Feat'",
        "lines",
        "[[\"Dodge\"], [\"Dodge Cap\"]]",
        "\"Dodge and Dodge Cap\"",
    );
    let (merged, report) =
        built_db_from_index(fixture::grouping_cases(), source.path(), &[("feat-merge.toml", &merge)]).unwrap();
    assert_eq!(fixture::scion_dodge_lines(&merged), [("Dodge and Dodge Cap".into(), 4)]);
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
}

#[test]
fn a_feat_addition_feeds_a_merge_and_reports_upstream_values() {
    let source = fixture::xml_source();
    let path = source.path().join("Feats.xml");
    let original = std::fs::read_to_string(&path).unwrap();
    let captured = include_str!("fixtures/effect-groups/ScionOfTheFeywild.Feat.xml");
    std::fs::create_dir_all(source.path().join("FeatImages")).unwrap();
    std::fs::write(
        source.path().join("FeatImages/ScionOfTheFeywild.png"),
        include_bytes!("fixtures/effect-groups/icons/ScionOfTheFeywild.png"),
    )
    .unwrap();
    let addition = new_correction(
        "feat_bonus",
        "Scion of the Feywild",
        "add",
        "\"null\"",
        "{ stat = 'Force Spell Power', bonus_type = 'Feat', value = 10 }",
    );
    let merge = new_qualified_correction(
        "feat_bonus",
        "Scion of the Feywild",
        "stat = 'Sonic Spell Power, Force Spell Power'\nbonus_type = 'Feat'",
        "lines",
        "[[\"Sonic Spell Power\"], [\"Force Spell Power\"]]",
        "\"Sonic and Force Spell Power\"",
    );
    let corrections = addition + &merge;
    let bonuses = |db: &Connection| {
        db.prepare("SELECT s.name,bt.name,ob.amount FROM owner_bonuses ob JOIN feats f ON ob.owner_kind='feat' AND ob.owner_id=f.id JOIN effects s ON s.id=ob.stat_id JOIN bonus_types bt ON bt.id=ob.bonus_type_id WHERE f.name='Scion of the Feywild' ORDER BY s.name,bt.name,ob.amount").unwrap()
            .query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?)))
            .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
    };
    let effect_start = captured.find("      <Effect>").unwrap();
    let effect_end = effect_start + captured[effect_start..].find("      </Effect>").unwrap() + "      </Effect>".len();
    for upstream_amount in [None, Some(10), Some(20)] {
        let extra = upstream_amount
            .map(|amount| {
                captured[effect_start..effect_end]
                    .replace("<Item>Sonic</Item>", "<Item>Force</Item>")
                    .replace("<Amount size=\"1\">10</Amount>", &format!("<Amount size=\"1\">{amount}</Amount>"))
            })
            .unwrap_or_default();
        let changed = captured.replace("    </Feat>", &format!("{extra}\n    </Feat>"));
        std::fs::write(&path, original.replace("</Feats>", &format!("{changed}</Feats>"))).unwrap();
        let (before, _) = built_db_from_index(fixture::grouping_cases(), source.path(), &[]).unwrap();
        let (after, report) =
            built_db_from_index(fixture::grouping_cases(), source.path(), &[("feywild.toml", &corrections)]).unwrap();
        let mut expected = bonuses(&before);
        if upstream_amount.is_none() {
            expected.push(("Force Spell Power".into(), "Feat".into(), 10));
            expected.sort();
        }
        assert_eq!(bonuses(&after), expected);
        if upstream_amount.is_none() {
            let mut export_corrections = Corrections::from_database(&after).unwrap();
            export_corrections.entries.extend(fixture::fixture_icon_corrections().entries);
            let mut normalized = Connection::open_in_memory().unwrap();
            build_database(
                fixture::grouping_cases(),
                source.path(),
                &WikiOverrides::default(),
                &export_corrections,
                &mut normalized,
                &fixture_dataset_version(),
            )
            .unwrap();
            let exported = tempfile::tempdir().unwrap();
            ddo_etl::tree::export_database(&normalized, exported.path()).unwrap();
            copy_fixture_icons(exported.path());
            std::fs::write(
                exported.path().join("icons/feats/ScionOfTheFeywild.png"),
                include_bytes!("fixtures/effect-groups/icons/ScionOfTheFeywild.png"),
            )
            .unwrap();
            let mut rebuilt = Connection::open_in_memory().unwrap();
            ddo_etl::tree::build_from_data(exported.path(), &mut rebuilt).unwrap();
            assert_eq!(bonuses(&rebuilt), expected);
            assert_eq!(recorded_corrections(&normalized), recorded_corrections(&rebuilt));
            let second = tempfile::tempdir().unwrap();
            ddo_etl::tree::export_database(&rebuilt, second.path()).unwrap();
            assert_eq!(
                ddo_etl::tree::data_file_bytes(exported.path()).unwrap(),
                ddo_etl::tree::data_file_bytes(second.path()).unwrap()
            );
        }
        let lines = after.prepare("SELECT e.name,l.value FROM feat_effects l JOIN effects e ON e.id=l.effect_id JOIN feats f ON f.id=l.feat_id WHERE f.name='Scion of the Feywild' AND e.name IN ('Sonic Spell Power','Force Spell Power','Sonic and Force Spell Power') ORDER BY l.sort_order").unwrap()
            .query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?)))
            .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        if upstream_amount == Some(20) {
            assert_eq!(lines, [("Sonic Spell Power".into(), 10), ("Force Spell Power".into(), 20)]);
            assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 2));
            assert!(report.stale_corrections.iter().any(|correction| matches!(&correction.cause,
                StaleCorrectionCause::ValueChanged { maetrim_value, .. } if maetrim_value == "\"no such bonus\"")));
        } else {
            assert_eq!(lines, [("Sonic and Force Spell Power".into(), 10)]);
            assert_eq!(
                (report.correction_applied_count, report.correction_stale_count),
                if upstream_amount.is_none() { (2, 0) } else { (1, 1) }
            );
        }
    }
}

#[test]
fn a_feat_bonus_type_correction_preserves_amounts_and_tracks_upstream_types() {
    let source = fixture::xml_source();
    let path = source.path().join("Feats.xml");
    let original = std::fs::read_to_string(&path).unwrap();
    let captured = include_str!("fixtures/effect-groups/ShieldDeflection.Feat.xml");
    std::fs::create_dir_all(source.path().join("FeatImages")).unwrap();
    std::fs::write(
        source.path().join("FeatImages/ShieldDeflection.png"),
        include_bytes!("fixtures/effect-groups/icons/ShieldDeflection.png"),
    )
    .unwrap();
    let correction = new_qualified_correction(
        "feat_bonus",
        "Shield Deflection",
        "stat = 'Fire, Cold, Electric and Acid Absorption'\nbonus_type = 'Enhancement'",
        "bonus_type",
        "\"Enhancement\"",
        "\"Competence\"",
    );
    let shield_lines = |db: &Connection| {
        db.prepare("SELECT b.name,l.value FROM feat_effects l JOIN feats f ON f.id=l.feat_id JOIN bonus_types b ON b.id=l.bonus_type_id WHERE f.name='Shield Deflection' ORDER BY l.sort_order").unwrap()
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
            .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
    };
    for upstream_type in ["Enhancement", "Competence", "Quality"] {
        let changed = captured.replace("<Bonus>Enhancement</Bonus>", &format!("<Bonus>{upstream_type}</Bonus>"));
        std::fs::write(&path, original.replace("</Feats>", &format!("{changed}</Feats>"))).unwrap();
        let (before, _) = built_db_from(source.path(), &[]).unwrap();
        assert_eq!(shield_lines(&before), [20, 25, 30, 40].map(|amount| (upstream_type.into(), amount)));
        let (after, report) = built_db_from(source.path(), &[("shield-deflection.toml", &correction)]).unwrap();
        let expected_type = if upstream_type == "Enhancement" { "Competence" } else { upstream_type };
        assert_eq!(shield_lines(&after), [20, 25, 30, 40].map(|amount| (expected_type.into(), amount)));
        let bonuses = after.prepare("SELECT s.name,b.name,ob.amount FROM owner_bonuses ob JOIN feats f ON ob.owner_kind='feat' AND f.id=ob.owner_id JOIN effects s ON s.id=ob.stat_id JOIN bonus_types b ON b.id=ob.bonus_type_id WHERE f.name='Shield Deflection' ORDER BY s.name,ob.amount").unwrap()
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)))
            .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        let expected = ["Acid Absorption", "Cold Absorption", "Electric Absorption", "Fire Absorption"]
            .into_iter()
            .flat_map(|stat| [20, 25, 30, 40].map(|amount| (stat.to_string(), expected_type.to_string(), amount)))
            .collect::<Vec<_>>();
        assert_eq!(bonuses, expected);
        if upstream_type == "Enhancement" {
            assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
        } else {
            assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 1));
            assert!(
                matches!(&report.stale_corrections[0].cause, StaleCorrectionCause::ValueChanged { maetrim_value, .. } if maetrim_value == &format!("\"{upstream_type}\"")),
                "{:?}",
                report.stale_corrections
            );
        }
    }
    std::fs::write(&path, original.replace("</Feats>", &format!("{captured}</Feats>"))).unwrap();
    let member = new_qualified_correction(
        "feat_bonus",
        "Shield Deflection",
        "stat = 'Fire Absorption'\nbonus_type = 'Enhancement'",
        "bonus_type",
        "\"Enhancement\"",
        "\"Competence\"",
    );
    let error = built_db_from(source.path(), &[("shield-member.toml", &member)]).unwrap_err();
    assert!(error.contains("behind grouping") && error.contains("Fire, Cold, Electric and Acid Absorption"), "{error}");
    let effects_start = captured.find("    <Effect>").unwrap();
    let effects_end = captured.rfind("    </Effect>").unwrap() + "    </Effect>".len();
    let removed = format!("{}{}", &captured[..effects_start], &captured[effects_end..]);
    std::fs::write(&path, original.replace("</Feats>", &format!("{removed}</Feats>"))).unwrap();
    let (without_bonuses, report) = built_db_from(source.path(), &[("shield-deflection.toml", &correction)]).unwrap();
    assert!(shield_lines(&without_bonuses).is_empty());
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 1));
    assert!(
        matches!(&report.stale_corrections[0].cause, StaleCorrectionCause::ValueChanged { maetrim_value, .. } if maetrim_value == "\"no such bonus\""),
        "{:?}",
        report.stale_corrections
    );
}

#[test]
fn blighted_wolf_uses_the_committed_attack_speed_correction() {
    let source = fixture::xml_source();
    let path = source.path().join("Feats.xml");
    let original = std::fs::read_to_string(&path).unwrap();
    let captured = include_str!("fixtures/effect-groups/BlightedWolf.Feat.xml");
    std::fs::create_dir_all(source.path().join("FeatImages")).unwrap();
    std::fs::write(
        source.path().join("FeatImages/WildShapeBlightedWolf.png"),
        include_bytes!("fixtures/effect-groups/icons/WildShapeBlightedWolf.png"),
    )
    .unwrap();
    let corrections = Corrections {
        entries: committed_corrections()
            .entries
            .iter()
            .filter(|correction| correction.name == "Wild Shape: Blighted Wolf")
            .cloned()
            .collect(),
    };
    for upstream in [20, 10, 25] {
        let changed =
            captured.replace("<Amount size=\"1\">20</Amount>", &format!("<Amount size=\"1\">{upstream}</Amount>"));
        std::fs::write(&path, original.replace("</Feats>", &format!("{changed}</Feats>"))).unwrap();
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
        let expected = if upstream == 20 { 10 } else { upstream };
        let bonuses = db.prepare("SELECT stat.name,bt.name,ob.amount FROM owner_bonuses ob JOIN feats f ON ob.owner_kind='feat' AND ob.owner_id=f.id JOIN effects stat ON stat.id=ob.stat_id JOIN bonus_types bt ON bt.id=ob.bonus_type_id WHERE f.name='Wild Shape: Blighted Wolf' AND bt.name='Combat Style' ORDER BY stat.name")
            .unwrap().query_map([], |row| Ok((row.get::<_, String>(0)?,row.get::<_, String>(1)?,row.get::<_, i64>(2)?)))
            .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        assert_eq!(
            bonuses,
            [
                ("Melee Attack Speed".into(), "Combat Style".into(), expected),
                ("Ranged Attack Speed".into(), "Combat Style".into(), expected)
            ]
        );
        assert_eq!(corrections.entries.len(), 1);
        assert_eq!(
            (report.correction_applied_count, report.correction_stale_count),
            if upstream == 20 { (1, 0) } else { (0, 1) }
        );
        if upstream != 20 {
            assert!(
                matches!(&report.stale_corrections[0].cause, StaleCorrectionCause::ValueChanged { maetrim_value, .. } if maetrim_value == &upstream.to_string())
            );
        }
    }
}

#[test]
fn feat_bonus_removal_and_plain_stat_value_apply_independently() {
    let source = fixture::xml_source();
    fixture::write_plague_wolf_feat(source.path());
    let corrections = fixture::plague_wolf_corrections();
    let removal = CorrectionSet(vec![corrections.0[0].clone()]);
    let (db, report) = built_db_from(source.path(), &[("removal.toml", &removal)]).unwrap();
    assert_eq!(fixture::plague_wolf_penalties(&db), [("Charisma".into(), -2, "Penalty".into(), true)]);
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
    let path = source.path().join("Feats.xml");
    let text = std::fs::read_to_string(&path).unwrap();
    let target = "<Item>Intelligence</Item>\n        <Item>Charisma</Item>";
    assert!(text.contains(target));
    std::fs::write(path, text.replace(target, "<Item>Charisma</Item>")).unwrap();
    let value = CorrectionSet(vec![corrections.0[1].clone()]);
    let (db, report) = built_db_from(source.path(), &[("value.toml", &value)]).unwrap();
    assert_eq!(fixture::plague_wolf_penalties(&db), [("Charisma".into(), -4, "Penalty".into(), true)]);
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
    let (_, report) = built_db_from(source.path(), &[("removed-upstream.toml", &removal)]).unwrap();
    assert_eq!(report.correction_stale_count, 1);
    assert!(
        matches!(&report.stale_corrections[0].cause, StaleCorrectionCause::ValueChanged { maetrim_value, .. } if maetrim_value == "\"no such bonus\"")
    );
}

#[test]
fn a_feat_member_value_cannot_change_a_surviving_group() {
    let source = fixture::xml_source();
    fixture::write_plague_wolf_feat(source.path());
    let value = CorrectionSet(vec![fixture::plague_wolf_corrections().0[1].clone()]);
    let error = built_db_from(source.path(), &[("member-value.toml", &value)]).unwrap_err();
    assert!(error.contains("behind grouping") && error.contains("Intelligence and Charisma"), "{error}");
}

#[test]
fn a_lines_split_preserves_the_default_and_follows_value_changes() {
    let source = fixture::xml_source();
    let augment = source.path().join("Augments/WoefulShadow.Augments.xml");
    std::fs::write(&augment, include_str!("fixtures/effect-groups/WoefulShadow.Augments.xml")).unwrap();
    std::fs::copy(
        fixtures_dir().join("effect-groups/icons/Lamordia.png"),
        source.path().join("AugmentImages/Lamordia.png"),
    )
    .unwrap();
    let correction = new_qualified_correction(
        "augment_bonus",
        "Woeful Shadow (Legendary)",
        "stat = \"Doublestrike, Doubleshot\"\nbonus_type = 'Profane'\n",
        "lines",
        "[[\"Doublestrike\", \"Doubleshot\"]]",
        "[\"Doubleshot\", \"Doublestrike\"]",
    );
    let line_names = |db: &Connection| {
        db.prepare("SELECT e.name FROM augment_effects l JOIN effects e ON e.id=l.effect_id JOIN augments a ON a.id=l.augment_id WHERE a.name='Woeful Shadow (Legendary)' AND l.value IN (3,4) ORDER BY e.name").unwrap()
            .query_map([], |row| row.get::<_, String>(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
    };
    let (default_db, _) = built_db_from(source.path(), &[]).unwrap();
    assert_eq!(line_names(&default_db), ["Doublestrike and Doubleshot"]);
    let (split_db, report) = built_db_from(source.path(), &[("woeful-shadow.toml", &correction)]).unwrap();
    assert_eq!(line_names(&split_db), ["Doubleshot", "Doublestrike"]);
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
    assert_eq!(report.line_splits.len(), 1);
    assert!(report.line_splits[0].reason.ends_with(".lines"));
    let exported = tempfile::tempdir().unwrap();
    let mut export_corrections = Corrections::from_database(&split_db).unwrap();
    export_corrections.entries.extend(fixture::fixture_icon_corrections().entries);
    let mut normalized = Connection::open_in_memory().unwrap();
    build_database(
        fixture::groupings(),
        source.path(),
        &WikiOverrides::default(),
        &export_corrections,
        &mut normalized,
        &fixture_dataset_version(),
    )
    .unwrap();
    ddo_etl::tree::export_database(&normalized, exported.path()).unwrap();
    for entry in walkdir::WalkDir::new(fixtures_dir().join("data")).into_iter().map(Result::unwrap).filter(|entry| {
        entry.file_type().is_file() && entry.path().extension().is_some_and(|extension| extension == "png")
    }) {
        let destination = exported.path().join(entry.path().strip_prefix(fixtures_dir().join("data")).unwrap());
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::copy(entry.path(), destination).unwrap();
    }
    std::fs::copy(
        fixtures_dir().join("effect-groups/icons/Lamordia.png"),
        exported.path().join("icons/augments/Lamordia.png"),
    )
    .unwrap();
    let mut rebuilt = Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(exported.path(), &mut rebuilt).unwrap();
    assert_eq!(recorded_corrections(&normalized), recorded_corrections(&rebuilt));
    let path = walkdir::WalkDir::new(exported.path())
        .into_iter()
        .map(Result::unwrap)
        .find(|entry| entry.file_name() == "woeful-shadow-legendary.toml")
        .unwrap()
        .into_path();
    let contents = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, contents.replace("name = \"Doubleshot\"", "name = \"Ranged Power\"")).unwrap();
    let error =
        ddo_etl::tree::build_from_data(exported.path(), &mut Connection::open_in_memory().unwrap()).unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("woeful-shadow-legendary.toml") && message.contains(".lines"), "{message}");
    let annotation = contents.split_once("[[ddobuilder_correction]]").unwrap().1.split("\n[").next().unwrap();
    std::fs::write(&path, format!("{contents}\n[[ddobuilder_correction]]{annotation}")).unwrap();
    let duplicate =
        ddo_etl::tree::build_from_data(exported.path(), &mut Connection::open_in_memory().unwrap()).unwrap_err();
    assert!(format!("{duplicate:#}").contains("conflicting notes"), "{duplicate:#}");
    std::fs::write(
        &augment,
        include_str!("fixtures/effect-groups/WoefulShadow.Augments.xml").replace(">3</Amount>", ">4</Amount>"),
    )
    .unwrap();
    let (changed, report) = built_db_from(source.path(), &[("woeful-shadow.toml", &correction)]).unwrap();
    assert_eq!(line_names(&changed), ["Doubleshot", "Doublestrike"]);
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
}

#[test]
fn a_lines_split_survives_the_set_tier_group_fallback() {
    let correction = new_qualified_correction(
        "set_tier_bonus",
        "Kundarak Delving Equipment",
        "equipped_count = 3\nstat = \"Doublestrike, Doubleshot\"\nbonus_type = 'Artifact'\n",
        "lines",
        "[[\"Doublestrike\", \"Doubleshot\"]]",
        "[\"Doubleshot\", \"Doublestrike\"]",
    );
    let (db, report) = built_db_with(&[("kundarak-split.toml", &correction)]).unwrap();
    let names = db.prepare("SELECT e.name FROM set_bonus_tier_effects l JOIN effects e ON e.id=l.effect_id JOIN set_bonus_tiers t ON t.id=l.tier_id JOIN set_bonuses s ON s.id=t.set_id WHERE s.name='Kundarak Delving Equipment' AND e.name IN ('Doublestrike','Doubleshot','Doublestrike and Doubleshot') ORDER BY e.name").unwrap()
        .query_map([], |row| row.get::<_, String>(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(names, ["Doubleshot", "Doublestrike"]);
    assert_eq!(report.line_splits.len(), 1);
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
}

#[test]
fn duplicate_bonus_correction_names_the_exact_augment_stat_type_and_value() {
    let correction = new_correction("augment_bonus", "Dolorous Quality Combat Mastery (Legendary)", "dedupe", "2", "1")
        + "stat = \"Stun DC\"\nbonus_type = \"Quality\"\nbonus_value = 3\n";
    let parsed = validated_corrections(&[("dedupe.toml", &correction)]).unwrap();
    assert_eq!(parsed.entries[0].qualifier(), "Stun DC / Quality / 3");
}

#[test]
fn stale_corrections_keep_the_qualified_child_identity() {
    let correction =
        new_correction("enhancement", "Legendary Dreadnought: Dire Attack", "icon", "\"OldIcon\"", "\"SelectionIcon\"")
            + "selection = \"Dire Charge\"\n";
    let (_, report) = built_db_with(&[("selection.toml", &correction)]).unwrap();
    assert_eq!(report.stale_corrections.len(), 1);
    assert_eq!(report.stale_corrections[0].qualifier, "selection \"Dire Charge\"");
}

#[test]
fn correction_json_values_round_trip_the_real_writer() {
    for correction in committed_corrections().clone().entries {
        for original in [correction.from, correction.to] {
            assert_eq!(CorrectionValue::from_json(&original.to_json()).unwrap(), original);
        }
    }
    for original in [
        CorrectionValue::Integers(vec![]),
        CorrectionValue::Integers(vec![i64::MIN, 0, i64::MAX]),
        CorrectionValue::Texts(vec!["null".into(), "line\nbreak".into()]),
        CorrectionValue::Integer(i64::MIN),
        CorrectionValue::Integer(i64::MAX),
        CorrectionValue::Float(0.5),
        CorrectionValue::Text("null".into()),
        CorrectionValue::Text("line\nbreak \"quoted\"".into()),
        CorrectionValue::Null,
    ] {
        assert_eq!(CorrectionValue::from_json(&original.to_json()).unwrap(), original);
    }
    assert_eq!(CorrectionValue::from_json("true").unwrap(), CorrectionValue::Integer(1));
    for malformed in ["", "[1.5]", "[1,\"text\"]", "{", "{\"unknown\":1}"] {
        assert!(CorrectionValue::from_json(malformed).is_err(), "{malformed}");
    }
}

#[test]
fn an_added_grouping_can_come_only_from_the_canonical_index() {
    let addition = new_qualified_correction(
        "set_tier_bonus",
        "Fabricator's Ingenuity",
        "equipped_count = 2",
        "add",
        "\"null\"",
        "{ stat = \"Elemental Resonance\", bonus_type = \"Artifact\", value = 3 }",
    );
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        fixture::grouping_cases(),
        &fixtures_dir().join("DataFiles"),
        &WikiOverrides::default(),
        &validated_corrections(&[("add.toml", &addition)]).unwrap(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
    let bonuses = set_tier_bonus_rows(&db, "Fabricator's Ingenuity", 2);
    for stat in ["Acid Spell Power", "Cold Spell Power", "Electric Spell Power", "Fire Spell Power"] {
        assert!(bonuses.contains(&(stat.into(), "Artifact".into(), 3)), "{bonuses:?}");
    }
}

#[test]
fn member_value_correction_accepts_a_group_with_only_one_stat() {
    let source = fixture::xml_source();
    let path = source.path().join("Augments/Named.Augments.xml");
    let contents = std::fs::read_to_string(&path).unwrap();
    std::fs::write(path, contents.replace("<Item>Charisma</Item>", "<Item>Constitution</Item>")).unwrap();
    let correction = new_qualified_correction(
        "augment_bonus",
        "Storm's Bulwark",
        "stat = \"Concentration\"\nbonus_type = \"Insight\"",
        "value",
        "5",
        "6",
    );
    let (db, report) = built_db_from(source.path(), &[("one-member.toml", &correction)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
    let amount: i64 = db.query_row(
        "SELECT ob.amount FROM owner_bonuses ob JOIN augments a ON a.id=ob.owner_id JOIN effects s ON s.id=ob.stat_id
         WHERE ob.owner_kind='augment' AND a.name='Storm''s Bulwark' AND s.name='Concentration'",
        [], |row| row.get(0),
    ).unwrap();
    assert_eq!(amount, 6);
}

#[test]
fn member_guard_ignores_a_grouping_at_another_value() {
    let source = fixture::xml_source();
    let path = source.path().join("SetBonuses.xml");
    let original = std::fs::read_to_string(&path).unwrap();
    let sets =
        include_str!("fixtures/effect-groups/MemberRemovalSets.xml").replace("<Type>FortificationBypass</Type>", "");
    std::fs::write(&path, original.replace("</SetBonuses>", &format!("{sets}</SetBonuses>"))).unwrap();
    let correction = new_qualified_correction(
        "set_tier_bonus",
        "Devil's Infernal Dance",
        "equipped_count = 3\nstat = \"Doubleshot\"\nbonus_type = \"Artifact\"\nbonus_value = 10",
        "value",
        "10",
        "11",
    );
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        fixture::groupings(),
        source.path(),
        &WikiOverrides::default(),
        &validated_corrections(&[("value.toml", &correction)]).unwrap(),
        &mut db,
        &fixture_dataset_version(),
    )
    .unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
    let bonuses = set_tier_bonus_rows(&db, "Devil's Infernal Dance", 3);
    assert!(bonuses.contains(&("Doubleshot".into(), "Artifact".into(), 5)));
    assert!(bonuses.contains(&("Doubleshot".into(), "Artifact".into(), 11)));
}

#[test]
fn member_removal_precedes_grouping_and_matches_only_its_value() {
    let source = fixture::xml_source();
    let path = source.path().join("SetBonuses.xml");
    let original = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        original.replace(
            "</SetBonuses>",
            &format!("{}</SetBonuses>", include_str!("fixtures/effect-groups/MemberRemovalSets.xml")),
        ),
    )
    .unwrap();
    let corrections = Corrections {
        entries: committed_corrections()
            .entries
            .iter()
            .filter(|entry| {
                entry.kind == ddo_model::enums::CorrectionKind::SetTierBonus
                    && entry.field == "remove"
                    && ["Devil's Infernal Dance", "Celerity"].contains(&entry.name.as_str())
            })
            .cloned()
            .collect(),
    };
    assert_eq!(corrections.entries.len(), 2);
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
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (2, 0));
    let bonuses = set_tier_bonus_rows(&db, "Devil's Infernal Dance", 3);
    assert!(bonuses.contains(&("Fortification Bypass".into(), "Artifact".into(), 10)));
    assert!(bonuses.contains(&("Doubleshot".into(), "Artifact".into(), 5)));
    assert!(!bonuses.contains(&("Doubleshot".into(), "Artifact".into(), 10)));
    assert_eq!(row_count(&db, "SELECT COUNT(*) FROM effects WHERE name = 'Doubleshot and Fortification Bypass'"), 0);
    assert_eq!(
        set_tier_bonus_rows(&db, "Celerity", 2),
        [("Doublestrike".into(), "Stacking".into(), 2), ("Doubleshot".into(), "Stacking".into(), 2)]
    );
    assert!(!set_tier_bonus_rows(&db, "Celerity", 3).iter().any(|(name, _, _)| name == "Movement Speed"));
    let restored = Corrections::from_database(&db).unwrap();
    for expected in &corrections.entries {
        let actual = restored.entries.iter().find(|entry| entry.name == expected.name).unwrap();
        assert_eq!(actual.qualifier(), expected.qualifier());
        assert_eq!(actual.from, expected.from);
        assert_eq!(actual.to, expected.to);
    }
    let unchanged_source = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, unchanged_source.replace("<Amount size=\"1\">10</Amount>", "<Amount size=\"1\">11</Amount>"))
        .unwrap();
    let mut changed_db = Connection::open_in_memory().unwrap();
    let changed = build_database(
        fixture::groupings(),
        source.path(),
        &WikiOverrides::default(),
        &corrections,
        &mut changed_db,
        &fixture_dataset_version(),
    )
    .unwrap();
    assert_eq!((changed.correction_applied_count, changed.correction_stale_count), (1, 1));
    assert_eq!(changed.stale_corrections[0].name, "Devil's Infernal Dance");
    std::fs::write(&path, unchanged_source).unwrap();
    let mut remove_all = corrections.clone();
    let mut second_removal =
        remove_all.entries.iter().find(|entry| entry.name == "Devil's Infernal Dance").unwrap().clone();
    second_removal.stat = Some("Fortification Bypass".into());
    remove_all.entries.push(second_removal);
    let mut empty_db = Connection::open_in_memory().unwrap();
    let empty = build_database(
        fixture::groupings(),
        source.path(),
        &WikiOverrides::default(),
        &remove_all,
        &mut empty_db,
        &fixture_dataset_version(),
    )
    .unwrap();
    assert_eq!((empty.correction_applied_count, empty.correction_stale_count), (3, 0));
    assert!(!set_tier_bonus_rows(&empty_db, "Devil's Infernal Dance", 3)
        .iter()
        .any(|(name, _, amount)| name == "Fortification Bypass" || (name == "Doubleshot" && *amount == 10)));
    assert_eq!(
        row_count(&empty_db, "SELECT COUNT(*) FROM effects WHERE name = '+10% Artifact Fortification Bypass'"),
        0
    );
}

#[test]
fn grouping_names_with_commas_survive_correction_exports() {
    let correction = new_qualified_correction(
        "set_tier_bonus",
        "Legendary Hruit's Influence",
        "equipped_count = 3\nstat = \"Fire, Cold, Electric and Positive Spell Power\"\nbonus_type = \"Artifact\"",
        "value",
        "30",
        "31",
    );
    let (db, report) = built_db_with(&[("group-value.toml", &correction)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
    let restored = rebuilt_correction_notes(&db);
    let grouping = restored.entries.iter().find(|entry| entry.name == "Legendary Hruit's Influence").unwrap();
    assert_eq!(grouping.stat.as_deref(), Some("Fire, Cold, Electric and Positive Spell Power"));
    assert_eq!(grouping.equipped_count, Some(3));
    assert_eq!(grouping.from, CorrectionValue::Integer(30));
    assert_eq!(grouping.to, CorrectionValue::Integer(31));
    let addition = new_qualified_correction(
        "set_tier_bonus",
        "Fabricator's Ingenuity",
        "equipped_count = 2",
        "add",
        "\"null\"",
        "{ stat = \"Fire, Cold, Electric and Positive Spell Power\", bonus_type = \"Artifact\", value = 2 }",
    );
    let (db, report) = built_db_with(&[("group-add.toml", &addition)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
    let restored = rebuilt_correction_notes(&db);
    let added = restored.entries.iter().find(|entry| entry.name == "Fabricator's Ingenuity").unwrap();
    assert_eq!(
        added.to,
        CorrectionValue::Bonus(ddo_etl::corrections::BonusAddition {
            stat: "Fire, Cold, Electric and Positive Spell Power".into(),
            bonus_type: "Artifact".into(),
            value: 2,
        })
    );
}

#[test]
fn leaves_his_value_and_reports_a_correction_whose_from_is_stale() {
    let (db, report) =
        built_db_with(&[("corrections.toml", &new_correction("augment", "Perfect Silence", "min_level", "318", "18"))])
            .unwrap();
    assert_eq!(augment_min_levels(&db, "Perfect Silence"), [Some(30)]);
    assert!(recorded_corrections(&db).is_empty());
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 1));
    assert_eq!(
        report.stale_corrections,
        [StaleCorrection {
            qualifier: String::new(),
            kind: "augment".into(),
            name: "Perfect Silence".into(),
            field: "min_level".into(),
            cause: StaleCorrectionCause::ValueChanged { expected_value: "318".into(), maetrim_value: "30".into() },
            file_name: "corrections.toml".into(),
        }]
    );
}

#[test]
fn matches_and_writes_null_through_the_literal_string() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(new_correction("augment", "Ruby of Acid Damage", "min_level", "\"null\"", "5")
            + &new_correction("augment", "Perfect Silence", "min_level", "30", "\"null\"")),
    )])
    .unwrap();
    assert_eq!(augment_min_levels(&db, "Ruby of Acid Damage"), [Some(5)]);
    assert_eq!(augment_min_levels(&db, "Perfect Silence"), [None]);
    assert_eq!(report.correction_applied_count, 2);
    assert!(recorded_corrections(&db)
        .iter()
        .any(|(_, name, _, from, _)| name == "Ruby of Acid Damage" && from == "null"));
}

#[test]
fn applies_a_repeated_name_to_every_row_with_that_name() {
    assert_eq!(augment_min_levels(&built_db_with(&[]).unwrap().0, "+5 Fortitude Save"), [Some(11), Some(11)]);
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &new_correction("augment", "+5 Fortitude Save", "min_level", "11", "12"),
    )])
    .unwrap();
    assert_eq!(augment_min_levels(&db, "+5 Fortitude Save"), [Some(12), Some(12)]);
    assert_eq!(report.correction_applied_count, 1);
    assert_eq!(recorded_corrections(&db).len(), 1);
}

#[test]
fn resolves_reference_fields_by_name() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(new_correction("item", "Docent of Defiance", "slot", "\"Body\"", "\"Waist\"")
            + &new_correction("item", "Docent of Defiance", "material", "\"Gem\"", "\"Steel\"")
            + &new_correction("quest", "Plane of Night", "pack", "\"Vault of Night\"", "\"Devil Assault\"")
            + &new_correction("quest", "Plane of Night", "patron", "\"House Kundarak\"", "\"The Twelve\"")),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 4, "{:?}", report.stale_corrections);
    let (slot, material): (String, String) = db
        .query_row(
            "SELECT s.name, m.name FROM items i JOIN equipment_slots s ON s.id = i.slot_id JOIN item_materials m ON m.id = i.material_id
              WHERE i.name = 'Docent of Defiance'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((slot.as_str(), material.as_str()), ("Waist", "Steel"));
    let (pack, patron): (String, String) = db
        .query_row(
            "SELECT p.name, pa.name FROM quests q JOIN adventure_packs p ON p.id = q.pack_id JOIN patrons pa ON pa.id = q.patron_id
              WHERE q.name = 'Plane of Night'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((pack.as_str(), patron.as_str()), ("Devil Assault", "The Twelve"));
}

#[test]
fn cited_cannith_challenge_corrections_use_their_own_pack_and_patron() {
    let dragon = "Extraplanar Mining - Epic The Dragon's Horde";
    let corrections = new_correction("quest", dragon, "pack", "\"Free to Play\"", "\"Vaults of the Artificers\"")
        + &new_correction("quest", dragon, "patron", "\"null\"", "\"House Cannith\"")
        + &new_correction(
            "quest",
            dragon,
            "name",
            "\"Extraplanar Mining - Epic The Dragon's Horde\"",
            "\"Extraplanar Mining - Epic The Dragon's Hoard\"",
        )
        + &new_correction(
            "quest",
            "Dr. Rushmore's Mansion - Moving Targets - EPIC",
            "pack",
            "\"Secrets of the Artificers\"",
            "\"Vaults of the Artificers\"",
        );
    let (db, report) = built_db_with(&[("cannith.toml", &corrections)]).unwrap();
    assert_eq!(report.correction_applied_count, 4);
    for name in ["Extraplanar Mining - Epic The Dragon's Hoard", "Dr. Rushmore's Mansion - Moving Targets - EPIC"] {
        let (pack, patron): (String, String) = db
            .query_row(
                "SELECT p.name, pa.name FROM quests q JOIN adventure_packs p ON p.id = q.pack_id JOIN patrons pa ON pa.id = q.patron_id WHERE q.name = ?1",
                [name],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((pack.as_str(), patron.as_str()), ("Vaults of the Artificers", "House Cannith"));
    }
}

#[test]
fn build_fails_naming_a_reference_his_files_lack() {
    let error = built_db_with(&[(
        "corrections.toml",
        &new_correction("item", "Docent of Defiance", "material", "\"Gem\"", "\"Unobtainium\""),
    )])
    .unwrap_err();
    assert!(error.contains("Unobtainium") && error.contains("Docent of Defiance"), "{error}");
}

#[test]
fn renames_a_pack_and_refuses_a_name_already_taken() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &new_correction("adventure_pack", "Vault of Night", "name", "\"Vault of Night\"", "\"The Vault of Night\""),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 1);
    let pack: String = db
        .query_row(
            "SELECT p.name FROM quests q JOIN adventure_packs p ON p.id = q.pack_id WHERE q.name = 'Plane of Night'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(pack, "The Vault of Night");
    let error = built_db_with(&[(
        "corrections.toml",
        &new_correction("patron", "The Twelve", "name", "\"The Twelve\"", "\"House Cannith\""),
    )])
    .unwrap_err();
    assert!(error.contains("House Cannith") && error.contains("The Twelve"), "{error}");
}

#[test]
fn renaming_a_set_renames_what_his_items_and_augments_call_it() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(new_correction(
            "set_bonus",
            "Kundarak Delving Equipment",
            "name",
            "\"Kundarak Delving Equipment\"",
            "\"Kundarak Delving Gear\"",
        ) + &new_correction("set_bonus", "Perfect Silence", "name", "\"Perfect Silence\"", "\"Perfect Quiet\"")),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 2);
    let (item_set_name, linked_set_name): (String, String) = db
        .query_row(
            "SELECT i.set_bonus, s.name FROM items i JOIN set_bonus_items si ON si.item_id = i.id JOIN set_bonuses s ON s.id = si.set_id
              WHERE i.name = 'Kundarak Delving Boots'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((item_set_name.as_str(), linked_set_name.as_str()), ("Kundarak Delving Gear", "Kundarak Delving Gear"));
    let augment_set_name: String =
        db.query_row("SELECT set_bonus FROM augments WHERE name = 'Perfect Silence'", [], |r| r.get(0)).unwrap();
    assert_eq!(augment_set_name, "Perfect Quiet");
}

#[test]
fn correcting_an_items_set_moves_its_set_link() {
    let (db, _) = built_db_with(&[(
        "corrections.toml",
        &new_correction(
            "item",
            "Kundarak Delving Boots",
            "set_bonus",
            "\"Kundarak Delving Equipment\"",
            "\"Eminence of Winter\"",
        ),
    )])
    .unwrap();
    let linked_set_names: Vec<String> = db
        .prepare(
            "SELECT s.name FROM set_bonus_items si JOIN set_bonuses s ON s.id = si.set_id JOIN items i ON i.id = si.item_id
              WHERE i.name = 'Kundarak Delving Boots'",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(linked_set_names, ["Eminence of Winter"]);
    let error = built_db_with(&[(
        "corrections.toml",
        &new_correction(
            "item",
            "Kundarak Delving Boots",
            "set_bonus",
            "\"Kundarak Delving Equipment\"",
            "\"No Such Set\"",
        ),
    )])
    .unwrap_err();
    assert!(error.contains("No Such Set"), "{error}");
}

#[test]
fn corrects_only_his_items_not_the_wikis() {
    let wiki = WikiOverrides::from_database(&fixture::database(|_| {})).unwrap();
    let corrections = validated_corrections(&[(
        "corrections.toml",
        &new_correction("item", "Battle Axe of the Oozing Hunger", "minimum_level", "29", "30"),
    )])
    .unwrap();
    let error = built_db_with_wiki(&wiki, &corrections).unwrap_err();
    assert!(error.contains("Battle Axe of the Oozing Hunger"), "{error}");
}

#[test]
fn corrects_only_his_quests_not_the_wikis() {
    let wiki = WikiOverrides::from_database(&fixture::database(|_| {})).unwrap();
    let corrections = validated_corrections(&[(
        "corrections.toml",
        &new_correction("quest", "Ghosts of Perdition", "level", "32", "31"),
    )])
    .unwrap();
    let error = built_db_with_wiki(&wiki, &corrections).unwrap_err();
    assert!(error.contains("Ghosts of Perdition"), "{error}");
}

#[test]
fn corrects_only_his_augments_not_the_wikis() {
    let wiki = WikiOverrides::from_database(&fixture::database(|_| {})).unwrap();
    for correction_text in [
        new_correction("augment", "Test Gem of Oozing Resistance", "min_level", "29", "30"),
        new_qualified_correction(
            "augment_bonus",
            "Test Gem of Oozing Resistance",
            "stat = \"Strength\"\nbonus_type = \"Insight\"",
            "value",
            "3",
            "4",
        ),
    ] {
        let corrections = validated_corrections(&[("corrections.toml", &correction_text)]).unwrap();
        let error = built_db_with_wiki(&wiki, &corrections).unwrap_err();
        assert!(error.contains("Test Gem of Oozing Resistance"), "{correction_text:?}: {error}");
    }
}

#[test]
fn runs_before_the_wiki_merge_so_a_corrected_description_is_his() {
    let wiki = WikiOverrides::from_database(&fixture::database(|_| {})).unwrap();
    let corrections = validated_corrections(&[(
        "corrections.toml",
        &new_correction("item", "+1 Ember Repeating Light Crossbow", "description", "\"null\"", "\"Corrected text.\""),
    )])
    .unwrap();
    let (db, report) = built_db_with_wiki(&wiki, &corrections).unwrap();
    assert_eq!(report.correction_applied_count, 1);
    let description: Option<String> = db
        .query_row("SELECT description FROM items WHERE name = '+1 Ember Repeating Light Crossbow'", [], |r| r.get(0))
        .optional()
        .unwrap()
        .flatten();
    assert_eq!(description.as_deref(), Some("Corrected text."));
}

#[test]
fn a_boolean_field_takes_zero_or_one() {
    let (db, _) =
        built_db_with(&[("corrections.toml", &new_correction("quest", "Plane of Night", "is_raid", "1", "0"))])
            .unwrap();
    let is_raid: bool =
        db.query_row("SELECT is_raid FROM quests WHERE name = 'Plane of Night'", [], |r| r.get(0)).unwrap();
    assert!(!is_raid);
    let error =
        validated_corrections(&[("corrections.toml", &new_correction("quest", "Plane of Night", "is_raid", "1", "2"))])
            .unwrap_err();
    assert!(error.contains("is_raid") && error.contains('2'), "{error}");
}

#[test]
fn flags_an_item_legacy_and_counts_it() {
    let (db, report) =
        built_db_with(&[("corrections.toml", &new_correction("item", "Docent of Defiance", "is_legacy", "0", "1"))])
            .unwrap();
    let is_legacy: bool =
        db.query_row("SELECT is_legacy FROM items WHERE name = 'Docent of Defiance'", [], |r| r.get(0)).unwrap();
    assert!(is_legacy);
    assert_eq!(report.legacy_item_count, 4, "the corrected item and the three legacy fixture items");
}

#[test]
fn committed_data_flags_the_tempests_spine_armours_the_wiki_says_no_longer_drop() {
    let corrections = committed_corrections().clone();
    for (old_name, replacement_name) in [
        ("Mithral Breastplate of the Elements", "Elemental Mithral Breastplate"),
        ("Platemail of Giants", "Full Plate of Giants"),
        ("Robe of Arcane Power", "Robe of Arcane Puissance"),
    ] {
        let legacy_flag = corrections
            .entries
            .iter()
            .find(|c| c.name == old_name && c.field == "is_legacy")
            .unwrap_or_else(|| panic!("{old_name} has no is_legacy correction"));
        assert_eq!(
            (legacy_flag.from.clone(), legacy_flag.to.clone()),
            (CorrectionValue::Integer(0), CorrectionValue::Integer(1))
        );
        assert!(legacy_flag.reason.contains(replacement_name), "{}", legacy_flag.reason);
    }
}

#[test]
fn a_member_correction_cannot_split_a_two_slot_grouping() {
    let correction = new_qualified_correction(
        "item_bonus",
        "Kundarak Delving Boots",
        "stat = \"Movement Speed\"\nbonus_type = \"Enhancement\"",
        "scale",
        "1.0",
        "0.5",
    );
    let error = built_db_with(&[("speed-scale.toml", &correction)]).unwrap_err();
    assert!(error.contains("Movement Speed") && error.contains("grouping \"Speed\""), "{error}");
}

fn item_names(db: &Connection) -> Vec<String> {
    db.prepare("SELECT name FROM items ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn augment_bonus_rows(db: &Connection, augment_name: &str) -> Vec<(String, Option<String>, Option<i64>)> {
    db.prepare(
        "SELECT s.name, bt.name, ob.amount
           FROM augments a JOIN owner_bonuses ob ON ob.owner_kind = 'augment' AND ob.owner_id = a.id
           JOIN effects s ON s.id = ob.stat_id
           LEFT JOIN bonus_types bt ON bt.id = ob.bonus_type_id
          WHERE a.name = ?1 ORDER BY a.id, ob.effect_link_order, ob.stat_id",
    )
    .unwrap()
    .query_map([augment_name], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

fn item_socket_labels(db: &Connection, item_name: &str) -> Vec<String> {
    db.prepare(
        "SELECT t.label FROM items i JOIN item_augment_slots s ON s.item_id = i.id JOIN augment_slot_types t ON t.id = s.slot_id
          WHERE i.name = ?1 ORDER BY s.sort_order",
    )
    .unwrap()
    .query_map([item_name], |r| r.get(0))
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

fn row_count(db: &Connection, sql: &str) -> i64 {
    db.query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn renames_an_item_augment_and_quest_after_their_other_corrections() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(new_correction("item", "Docent of Defiance", "name", "\"Docent of Defiance\"", "\"Docent of the Defiant\"")
            + &new_correction("item", "Docent of Defiance", "minimum_level", "10", "11")
            + &new_correction("augment", "Voidscale", "name", "\"Voidscale\"", "\"Void Scale\"")
            + &new_correction("augment", "Voidscale", "min_level", "31", "30")
            + &new_correction("quest", "Plane of Night", "name", "\"Plane of Night\"", "\"The Plane of Night\"")),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 5, "{:?}", report.stale_corrections);
    let (minimum_level, wiki_url): (i64, String) = db
        .query_row("SELECT minimum_level, wiki_url FROM items WHERE name = 'Docent of the Defiant'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((minimum_level, wiki_url.as_str()), (11, "https://ddowiki.com/page/Item:Docent_of_the_Defiant"));
    assert!(!item_names(&db).contains(&"Docent of Defiance".to_string()));
    assert_eq!(augment_min_levels(&db, "Void Scale"), [Some(30)]);
    assert_eq!(row_count(&db, "SELECT COUNT(*) FROM quests WHERE name = 'The Plane of Night'"), 1);
}

#[test]
fn a_renamed_quest_links_the_loot_his_drop_text_names_by_the_new_name() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &new_correction(
            "quest",
            "ToEE: First Level and Earth Temple",
            "name",
            "\"ToEE: First Level and Earth Temple\"",
            "\"Temple of Elemental Evil Part One\"",
        ),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 1, "{:?}", report.stale_corrections);
    let linked_items: Vec<String> = db
        .prepare(
            "SELECT items.name FROM sources JOIN quests ON quests.id = sources.quest_id
               JOIN items ON items.id = sources.item_id
              WHERE quests.name = 'Temple of Elemental Evil Part One'",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(linked_items, ["+3 Combustion Scorched Battle Axe"]);
    let is_legacy: bool = db
        .query_row("SELECT is_legacy FROM items WHERE name = '+3 Combustion Scorched Battle Axe'", [], |r| r.get(0))
        .unwrap();
    assert!(!is_legacy, "a segment naming a quest of his is current, even when it also names a legacy source");
}

#[test]
fn a_rename_his_files_already_carry_goes_stale_for_every_renamable_kind() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(new_correction("item", "Docent of Defiant", "name", "\"Docent of Defiant\"", "\"Docent of Defiance\"")
            + &new_correction("augment", "Void Scale", "name", "\"Void Scale\"", "\"Voidscale\"")
            + &new_correction("quest", "Plane of Nite", "name", "\"Plane of Nite\"", "\"Plane of Night\"")
            + &new_correction("adventure_pack", "Vault of Nite", "name", "\"Vault of Nite\"", "\"Vault of Night\"")
            + &new_correction("patron", "The Twelfe", "name", "\"The Twelfe\"", "\"The Twelve\"")
            + &new_correction("set_bonus", "Perfect Silense", "name", "\"Perfect Silense\"", "\"Perfect Silence\"")
            + &new_correction("socket_label", "redd", "name", "\"redd\"", "\"red\"")),
    )])
    .unwrap();
    assert_eq!(
        (report.correction_applied_count, report.correction_stale_count),
        (0, 7),
        "{:?}",
        report.stale_corrections
    );
    assert!(recorded_corrections(&db).is_empty());
    assert_eq!(
        report.stale_corrections.iter().find(|stale_correction| stale_correction.kind == "item").unwrap(),
        &StaleCorrection {
            qualifier: String::new(),
            kind: "item".into(),
            name: "Docent of Defiant".into(),
            field: "name".into(),
            cause: StaleCorrectionCause::RenameDoneUpstream { new_name: "Docent of Defiance".into() },
            file_name: "corrections.toml".into(),
        }
    );
    assert!(item_names(&db).contains(&"Docent of Defiance".to_string()));
}

#[test]
fn a_rename_whose_name_and_to_both_match_nothing_fails() {
    let error = built_db_with(&[(
        "corrections.toml",
        &new_correction("quest", "Plane of Nite", "name", "\"Plane of Nite\"", "\"Plain of Night\""),
    )])
    .unwrap_err();
    assert!(
        error.contains("Plane of Nite") && error.contains("Plain of Night") && error.contains("corrections.toml"),
        "{error}"
    );
}

#[test]
fn renaming_an_item_or_quest_to_a_name_already_taken_fails() {
    let error = built_db_with(&[(
        "corrections.toml",
        &new_correction("item", "Docent of Defiance", "name", "\"Docent of Defiance\"", "\"Five Rings\""),
    )])
    .unwrap_err();
    assert!(error.contains("Five Rings") && error.contains("Docent of Defiance"), "{error}");
}

#[test]
fn a_wiki_fact_naming_the_old_spelling_fails_the_build() {
    let wiki = WikiOverrides::from_database(&fixture::database(|_| {})).unwrap();
    let corrections = validated_corrections(&[(
        "corrections.toml",
        &new_correction("augment", "Minor Fire Guard", "name", "\"Minor Fire Guard\"", "\"Lesser Fire Guard\""),
    )])
    .unwrap();
    let error = built_db_with_wiki(&wiki, &corrections).unwrap_err();
    assert!(error.contains("Minor Fire Guard"), "{error}");
}

#[test]
fn a_family_narrows_an_augment_correction() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(new_qualified_correction(
            "augment",
            "+5 Fortitude Save",
            "family = \"Greensteel_Heroic\"",
            "min_level",
            "11",
            "12",
        ) + &new_qualified_correction(
            "augment",
            "+5 Fortitude Save",
            "family = \"Greensteel_Heroic\"",
            "name",
            "\"+5 Fortitude Save\"",
            "\"+5 Fortitude\"",
        )),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 2, "{:?}", report.stale_corrections);
    assert_eq!(augment_min_levels(&db, "+5 Fortitude"), [Some(12), Some(12)]);
    let qualifiers: Vec<String> = db
        .prepare("SELECT qualifier FROM corrections ORDER BY field")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(qualifiers, ["family \"Greensteel_Heroic\"", "family \"Greensteel_Heroic\""]);
    let error = built_db_with(&[(
        "corrections.toml",
        &new_qualified_correction("augment", "+5 Fortitude Save", "family = \"Ruby\"", "min_level", "11", "12"),
    )])
    .unwrap_err();
    assert!(error.contains("+5 Fortitude Save") && error.contains("Ruby"), "{error}");
    let error = validated_corrections(&[(
        "corrections.toml",
        &new_qualified_correction("item", "Five Rings", "family = \"Ruby\"", "minimum_level", "1", "2"),
    )])
    .unwrap_err();
    assert!(error.contains("family") && error.contains("Five Rings"), "{error}");
}

#[test]
fn removes_an_item_with_its_child_rows() {
    let item_ids: Vec<i64> = {
        let db = built_db_with(&[]).unwrap().0;
        ["Legendary Cloak of Winter", "Acid Rune Arm"]
            .iter()
            .map(|name| db.query_row("SELECT id FROM items WHERE name = ?1", [name], |r| r.get(0)).unwrap())
            .collect()
    };
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(new_correction("item", "Legendary Cloak of Winter", "remove", "0", "1")
            + &new_correction("item", "Acid Rune Arm", "remove", "0", "1")),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 2, "{:?}", report.stale_corrections);
    assert!(!item_names(&db).iter().any(|name| name == "Legendary Cloak of Winter" || name == "Acid Rune Arm"));
    let id_list = format!("({}, {})", item_ids[0], item_ids[1]);
    for child_table in ["item_effects", "item_augment_slots", "sources", "set_bonus_items", "item_clickies"] {
        assert_eq!(
            row_count(&db, &format!("SELECT COUNT(*) FROM {child_table} WHERE item_id IN {id_list}")),
            0,
            "{child_table}"
        );
    }
    assert_eq!(
        row_count(
            &db,
            &format!("SELECT COUNT(*) FROM modifiers WHERE source_kind = 'item' AND source_id IN {id_list}")
        ),
        0
    );
    assert_eq!(
        recorded_corrections(&db)[0],
        ("item".into(), "Acid Rune Arm".into(), "remove".into(), "0".into(), "1".into())
    );
    let error =
        validated_corrections(&[("corrections.toml", &new_correction("item", "Acid Rune Arm", "remove", "1", "0"))])
            .unwrap_err();
    assert!(error.contains("remove") && error.contains("Acid Rune Arm"), "{error}");
}

#[test]
fn removes_an_augment_with_its_child_rows() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(new_correction("augment", "Perfect Silence", "remove", "0", "1")
            + &new_correction("augment", "Ruby of Acid Damage", "remove", "0", "1")),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 2, "{:?}", report.stale_corrections);
    assert_eq!(
        row_count(&db, "SELECT COUNT(*) FROM augments WHERE name IN ('Perfect Silence', 'Ruby of Acid Damage')"),
        0
    );
    for orphan_sql in [
        "SELECT COUNT(*) FROM augment_slots WHERE augment_id NOT IN (SELECT id FROM augments)",
        "SELECT COUNT(*) FROM augment_effects WHERE augment_id NOT IN (SELECT id FROM augments)",
        "SELECT COUNT(*) FROM set_bonus_augments WHERE augment_id NOT IN (SELECT id FROM augments)",
        "SELECT COUNT(*) FROM modifiers WHERE source_kind = 'augment' AND source_id NOT IN (SELECT id FROM augments)",
    ] {
        assert_eq!(row_count(&db, orphan_sql), 0, "{orphan_sql}");
    }
}

#[test]
fn corrects_an_augment_bonus_value_and_type_without_touching_the_shared_bonus() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(new_qualified_correction(
            "augment_bonus",
            "Silverscale",
            "stat = \"Healing Amplification\"\nbonus_type = \"Competence\"",
            "value",
            "56",
            "60",
        ) + &new_qualified_correction(
            "augment_bonus",
            "Silverscale",
            "stat = \"Repair Amplification\"\nbonus_type = \"Enhancement\"",
            "bonus_type",
            "\"Enhancement\"",
            "\"Competence\"",
        ) + &new_qualified_correction(
            "augment_bonus",
            "Silverscale",
            "stat = \"Negative Healing Amplification\"\nbonus_type = \"Profane\"",
            "value",
            "55",
            "57",
        )),
    )])
    .unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (2, 1));
    assert_eq!(
        augment_bonus_rows(&db, "Silverscale"),
        [
            ("Healing Amplification".into(), Some("Competence".into()), Some(60)),
            ("Negative Healing Amplification".into(), Some("Profane".into()), Some(56)),
            ("Repair Amplification".into(), Some("Competence".into()), Some(56)),
        ]
    );
    assert!(
        matches!(&report.stale_corrections[0].cause, StaleCorrectionCause::ValueChanged { maetrim_value, .. } if maetrim_value == "56"),
        "{:?}",
        report.stale_corrections
    );
    assert_eq!(
        row_count(
            &db,
            "SELECT COUNT(*) FROM effects e WHERE e.is_stat = 0 AND NOT EXISTS (SELECT 1 FROM item_effects r WHERE r.effect_id = e.id)
               AND NOT EXISTS (SELECT 1 FROM item_augment_slot_option_effects r WHERE r.effect_id = e.id)
               AND NOT EXISTS (SELECT 1 FROM augment_effects r WHERE r.effect_id = e.id)
               AND NOT EXISTS (SELECT 1 FROM feat_effects r WHERE r.effect_id = e.id)
               AND NOT EXISTS (SELECT 1 FROM set_bonus_tier_effects r WHERE r.effect_id = e.id)
               AND NOT EXISTS (SELECT 1 FROM effect_bonuses r WHERE r.target_effect_id = e.id)"
        ),
        0,
        "a bonus the corrections leave nothing carrying is deleted"
    );
    assert_eq!(report.bonus_count as i64, row_count(&db, "SELECT COUNT(*) FROM effect_bonuses"));
    let qualifiers: Vec<String> = db
        .prepare("SELECT qualifier FROM corrections ORDER BY qualifier")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(qualifiers, ["Healing Amplification / Competence", "Repair Amplification / Enhancement"]);
}

#[test]
fn adds_a_bonus_an_augment_lacks_and_goes_stale_once_he_carries_it() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(new_correction(
            "augment_bonus",
            "Voidscale",
            "add",
            "\"null\"",
            "{ stat = \"Physical Resistance Rating\", bonus_type = \"Exceptional\", value = 2 }",
        ) + &new_correction(
            "augment_bonus",
            "Voidscale",
            "add",
            "\"null\"",
            "{ stat = \"Universal Spell Lore\", bonus_type = \"Exceptional\", value = 5 }",
        )),
    )])
    .unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 1));
    assert_eq!(
        augment_bonus_rows(&db, "Voidscale"),
        [
            ("Universal Spell Lore".into(), Some("Exceptional".into()), Some(5)),
            ("Physical Resistance Rating".into(), Some("Exceptional".into()), Some(2)),
        ]
    );
    let to_value: String = db.query_row("SELECT to_value FROM corrections", [], |r| r.get(0)).unwrap();
    assert_eq!(to_value, r#"{"bonus_type":"Exceptional","stat":"Physical Resistance Rating","value":2}"#);
    let error = built_db_with(&[(
        "corrections.toml",
        &new_correction(
            "augment_bonus",
            "Voidscale",
            "add",
            "\"null\"",
            "{ stat = \"Physical Resistence\", bonus_type = \"Exceptional\", value = 2 }",
        ),
    )])
    .unwrap_err();
    assert!(error.contains("Physical Resistence"), "{error}");
    let error = validated_corrections(&[(
        "corrections.toml",
        &new_correction("augment_bonus", "Voidscale", "value", "5", "6"),
    )])
    .unwrap_err();
    assert!(error.contains("stat") && error.contains("Voidscale"), "{error}");
}

fn item_bonus_rows(db: &Connection, item_name: &str) -> Vec<(String, Option<String>, Option<i64>)> {
    db.prepare(
        "SELECT s.name, bt.name, ob.amount
           FROM items i JOIN owner_bonuses ob ON ob.owner_kind = 'item' AND ob.owner_id = i.id
           JOIN effects s ON s.id = ob.stat_id
           LEFT JOIN bonus_types bt ON bt.id = ob.bonus_type_id
          WHERE i.name = ?1 ORDER BY ob.effect_link_order, ob.stat_id",
    )
    .unwrap()
    .query_map([item_name], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

fn set_tier_bonus_rows(db: &Connection, set_name: &str, equipped_count: i64) -> Vec<(String, String, i64)> {
    db.prepare(
        "SELECT stats.name, bonus_types.name, owner_bonuses.amount
         FROM set_bonuses
         JOIN set_bonus_tiers ON set_bonus_tiers.set_id = set_bonuses.id
         JOIN owner_bonuses ON owner_bonuses.owner_kind = 'set_bonus_tier' AND owner_bonuses.owner_id = set_bonus_tiers.id
         JOIN effects stats ON stats.id = owner_bonuses.stat_id
         JOIN bonus_types ON bonus_types.id = owner_bonuses.bonus_type_id
         WHERE set_bonuses.name = ?1 AND set_bonus_tiers.equipped_count = ?2
         ORDER BY owner_bonuses.effect_link_order, owner_bonuses.stat_id",
    )
    .unwrap()
    .query_map(rusqlite::params![set_name, equipped_count], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

#[test]
fn corrects_set_tier_bonuses_and_creates_a_missing_tier() {
    let corrections = new_qualified_correction(
        "set_tier_bonus",
        "Eminence of Winter",
        "equipped_count = 2\nstat = \"Physical Resistance Rating\"\nbonus_type = \"Artifact\"",
        "value",
        "30",
        "31",
    ) + &new_qualified_correction(
        "set_tier_bonus",
        "Eminence of Winter",
        "equipped_count = 2",
        "add",
        "\"null\"",
        "{ stat = \"Magical Resistance Rating\", bonus_type = \"Artifact\", value = 31 }",
    ) + &new_qualified_correction(
        "set_tier",
        "Eminence of Winter",
        "equipped_count = 8",
        "add",
        "\"null\"",
        "{ equipped_count = 8, description = \"A new tier.\" }",
    ) + &new_qualified_correction(
        "set_tier_bonus",
        "Eminence of Winter",
        "equipped_count = 8",
        "add",
        "\"null\"",
        "{ stat = \"Constitution\", bonus_type = \"Artifact\", value = 4 }",
    );
    let (db, report) = built_db_with(&[("corrections.toml", &corrections)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (4, 0));
    assert_eq!(
        set_tier_bonus_rows(&db, "Eminence of Winter", 2),
        [
            ("Physical Resistance Rating".into(), "Artifact".into(), 31),
            ("Magical Resistance Rating".into(), "Artifact".into(), 31),
        ]
    );
    assert_eq!(set_tier_bonus_rows(&db, "Eminence of Winter", 8), [("Constitution".into(), "Artifact".into(), 4)]);
}

#[test]
fn set_tier_add_goes_stale_when_the_tier_exists() {
    let correction = new_qualified_correction(
        "set_tier",
        "Eminence of Winter",
        "equipped_count = 2",
        "add",
        "\"null\"",
        "{ equipped_count = 2, description = \"Already here.\" }",
    );
    let (_, report) = built_db_with(&[("corrections.toml", &correction)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 1));
}

#[test]
fn refuses_a_set_tier_bonus_without_a_type() {
    let correction = new_qualified_correction(
        "set_tier_bonus",
        "Eminence of Winter",
        "equipped_count = 2",
        "add",
        "\"null\"",
        "{ stat = \"Constitution\", bonus_type = \"null\", value = 1 }",
    );
    assert!(validated_corrections(&[("corrections.toml", &correction)]).is_err());
    for kind in ["item_bonus", "augment_bonus"] {
        let correction = new_correction(
            kind,
            "Owner",
            "add",
            "\"null\"",
            "{ stat = \"Constitution\", bonus_type = \"null\", value = 1 }",
        );
        assert!(validated_corrections(&[("corrections.toml", &correction)]).is_err(), "{kind}");
    }
}

#[test]
fn missing_set_tiers_name_the_set_and_equipped_count() {
    for kind in ["set_tier", "set_tier_bonus"] {
        let (field, from, to) = if kind == "set_tier" {
            ("description", "\"null\"", "\"Missing tier\"")
        } else {
            ("add", "\"null\"", "{ stat = \"Constitution\", bonus_type = \"Artifact\", value = 1 }")
        };
        let correction = new_qualified_correction(kind, "Eminence of Winter", "equipped_count = 99", field, from, to);
        let error = built_db_with(&[("corrections.toml", &correction)]).unwrap_err();
        assert!(error.contains("set \"Eminence of Winter\" has no tier with equipped_count 99"), "{error}");
        assert!(!error.contains("set_bonus_tiers.name"), "{error}");
    }
    let correction = new_qualified_correction(
        "set_tier",
        "Unknown set",
        "equipped_count = 2",
        "add",
        "\"null\"",
        "{ equipped_count = 2, description = \"A new tier\" }",
    );
    let error = built_db_with(&[("corrections.toml", &correction)]).unwrap_err();
    assert!(error.contains("set_bonuses.name") && !error.contains("set_bonus_tiers.name"), "{error}");
}

#[test]
fn removes_only_the_qualified_bonus_link_for_each_owner_kind() {
    for (kind, name, owner_qualifier) in [
        ("item_bonus", "Docent of Defiance", ""),
        ("augment_bonus", "Silverscale", "family = \"DinosaurBone\""),
        ("set_tier_bonus", "Eminence of Winter", "equipped_count = 2"),
    ] {
        let source = fixture::xml_source();
        let other_item = source.path().join("Items/Acid Rune Arm.item");
        let contents = std::fs::read_to_string(&other_item).unwrap();
        std::fs::write(other_item, contents.replace("</Item>\n</Items>", "<Buff><Type>AbilityBonus</Type><Value1>3</Value1><BonusType>Quality</BonusType><Item>Strength</Item></Buff></Item>\n</Items>")).unwrap();
        let (path, marker, closing) = match kind {
            "item_bonus" => (
                source.path().join("Items/Docent of Defiance.item"),
                "<Name>Docent of Defiance</Name>",
                "</Item>\n</Items>",
            ),
            "augment_bonus" => {
                (source.path().join("Augments/DinosaurBone.Augments.xml"), "<Name>Silverscale</Name>", "</Augment>")
            }
            _ => (source.path().join("SetBonuses.xml"), "<Type>Eminence of Winter</Type>", "</Buff>"),
        };
        let original = std::fs::read_to_string(&path).unwrap();
        let target = original.find(marker).unwrap();
        let position = target + original[target..].find(closing).unwrap();
        let write_bonuses = |bonuses: &[(&str, i64)]| {
            let mut xml = original.clone();
            let additions = bonuses.iter().map(|(bonus_type, amount)| {
                if kind == "item_bonus" {
                    format!("<Buff><Type>AbilityBonus</Type><Value1>{amount}</Value1><BonusType>{bonus_type}</BonusType><Item>Strength</Item></Buff>")
                } else {
                    format!("<Effect><Type>AbilityBonus</Type><Bonus>{bonus_type}</Bonus><AType>Simple</AType><Amount size=\"1\">{amount}</Amount><Item>Strength</Item></Effect>")
                }
            }).collect::<String>();
            xml.insert_str(position, &additions);
            std::fs::write(&path, xml).unwrap();
        };
        let qualifier = format!("{owner_qualifier}\nstat = \"Strength\"\nbonus_type = \"Quality\"");
        for (selector, from, applied, stale) in
            [("", "3", 1, 0), ("\nbonus_value = 3", "3", 1, 0), ("", "2", 0, 1), ("\nbonus_value = 2", "2", 0, 1)]
        {
            write_bonuses(&[("Quality", 3), ("Artifact", 2)]);
            let removal =
                new_qualified_correction(kind, name, &format!("{qualifier}{selector}"), "remove", from, "\"null\"");
            let (db, report) = built_db_from(source.path(), &[("removal.toml", &removal)]).unwrap();
            assert_eq!((report.correction_applied_count, report.correction_stale_count), (applied, stale), "{kind}");
            assert!(item_bonus_rows(&db, "Acid Rune Arm").contains(&(
                "Strength".into(),
                Some("Quality".into()),
                Some(3)
            )));
            let rows = match kind {
                "item_bonus" => item_bonus_rows(&db, name),
                "augment_bonus" => augment_bonus_rows(&db, name),
                _ => set_tier_bonus_rows(&db, name, 2)
                    .into_iter()
                    .map(|(stat, bonus_type, amount)| (stat, Some(bonus_type), Some(amount)))
                    .collect(),
            };
            assert_eq!(rows.contains(&("Strength".into(), Some("Quality".into()), Some(3))), stale == 1, "{kind}");
            assert!(rows.contains(&("Strength".into(), Some("Artifact".into()), Some(2))), "{kind}");
        }
        write_bonuses(&[]);
        let removal = new_qualified_correction(kind, name, &qualifier, "remove", "3", "\"null\"");
        let (_, report) = built_db_from(source.path(), &[("removal.toml", &removal)]).unwrap();
        assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 1));
        for amounts in [[3, 2], [3, 3]] {
            write_bonuses(&[("Quality", amounts[0]), ("Quality", amounts[1])]);
            let error = built_db_from(source.path(), &[("removal.toml", &removal)]).unwrap_err();
            assert!(error.contains("specify bonus_value"), "{kind}: {error}");
        }
        write_bonuses(&[("Quality", 3), ("Quality", 2)]);
        let narrowed =
            new_qualified_correction(kind, name, &format!("{qualifier}\nbonus_value = 3"), "remove", "3", "\"null\"");
        let (db, report) = built_db_from(source.path(), &[("removal.toml", &narrowed)]).unwrap();
        assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
        let remaining = match kind {
            "item_bonus" => item_bonus_rows(&db, name),
            "augment_bonus" => augment_bonus_rows(&db, name),
            _ => set_tier_bonus_rows(&db, name, 2)
                .into_iter()
                .map(|(stat, bonus_type, amount)| (stat, Some(bonus_type), Some(amount)))
                .collect(),
        };
        assert!(remaining.contains(&("Strength".into(), Some("Quality".into()), Some(2))));
        assert!(!remaining.contains(&("Strength".into(), Some("Quality".into()), Some(3))));
    }
}

#[test]
fn bonus_removal_amounts_survive_the_data_file_round_trip() {
    for (kind, name, qualifier, amount) in [
        ("item_bonus", "Docent of Defiance", "stat = 'Fire Resistance'\nbonus_type = 'Enhancement'", "20"),
        (
            "augment_bonus",
            "Silverscale",
            "family = 'DinosaurBone'\nstat = 'Healing Amplification'\nbonus_type = 'Competence'",
            "56",
        ),
        (
            "set_tier_bonus",
            "Eminence of Winter",
            "equipped_count = 2\nstat = 'Physical Resistance Rating'\nbonus_type = 'Artifact'",
            "30",
        ),
    ] {
        let removal = new_qualified_correction(kind, name, qualifier, "remove", amount, "'null'");
        let (db, report) = built_db_with(&[("removal.toml", &removal)]).unwrap();
        assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0), "{kind}");
        let rebuilt = rebuilt_correction_notes(&db);
        let note = rebuilt.entries.iter().find(|entry| entry.name == name && entry.field == "remove").unwrap();
        assert_eq!(note.from, CorrectionValue::Integer(amount.parse().unwrap()), "{kind}");
        assert_eq!(note.to, CorrectionValue::Null, "{kind}");
    }
}

#[test]
fn bonus_removal_requires_qualifiers_a_current_integer_and_a_null_destination() {
    for (kind, owner_qualifier) in [("item_bonus", ""), ("augment_bonus", ""), ("set_tier_bonus", "equipped_count = 2")]
    {
        for (bonus_qualifier, from, to) in [
            ("", "3", "\"null\""),
            ("stat = \"Strength\"", "3", "\"null\""),
            ("stat = \"Strength\"\nbonus_type = \"Quality\"", "3", "1"),
            ("stat = \"Strength\"\nbonus_type = \"Quality\"", "3.5", "\"null\""),
            ("stat = \"Strength\"\nbonus_type = \"Quality\"", "\"3\"", "\"null\""),
        ] {
            let correction = new_qualified_correction(
                kind,
                "Owner",
                &format!("{owner_qualifier}\n{bonus_qualifier}"),
                "remove",
                from,
                to,
            );
            assert!(validated_corrections(&[("corrections.toml", &correction)]).is_err(), "{correction:?}");
        }
    }
}

#[test]
fn corrects_a_set_tier_bonus_type_and_value() {
    let corrections = new_qualified_correction(
        "set_tier_bonus",
        "Eminence of Winter",
        "equipped_count = 2\nstat = \"Physical Resistance Rating\"\nbonus_type = \"Artifact\"",
        "bonus_type",
        "\"Artifact\"",
        "\"Profane\"",
    ) + &new_qualified_correction(
        "set_tier_bonus",
        "Eminence of Winter",
        "equipped_count = 2\nstat = \"Physical Resistance Rating\"\nbonus_type = \"Profane\"",
        "value",
        "30",
        "31",
    ) + &new_qualified_correction(
        "set_tier_bonus",
        "Eminence of Winter",
        "equipped_count = 2",
        "add",
        "\"null\"",
        "{ stat = \"Physical Resistance Rating\", bonus_type = \"Profane\", value = 99 }",
    );
    let (db, report) = built_db_with(&[("corrections.toml", &corrections)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (2, 1));
    assert_eq!(
        set_tier_bonus_rows(&db, "Eminence of Winter", 2),
        [("Physical Resistance Rating".into(), "Profane".into(), 31),]
    );
}

#[test]
fn corrects_a_set_tier_description_and_removes_an_unlisted_tier() {
    let corrections =
        new_qualified_correction(
            "set_tier",
            "Eminence of Winter",
            "equipped_count = 2",
            "description",
            "\"+30 Artifact bonus to Physical Resistance Rating\"",
            "\"+31 Artifact PRR\"",
        ) + &new_qualified_correction("set_tier", "Eminence of Winter", "equipped_count = 3", "remove", "0", "1");
    let (db, report) = built_db_with(&[("corrections.toml", &corrections)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (2, 0));
    assert_eq!(row_count(&db, "SELECT COUNT(*) FROM set_bonus_tiers t JOIN set_bonuses s ON s.id = t.set_id WHERE s.name = 'Eminence of Winter' AND t.equipped_count = 3"), 0);
    assert_eq!(
        row_count(
            &db,
            "SELECT COUNT(*) FROM set_bonus_tier_effects WHERE tier_id NOT IN (SELECT id FROM set_bonus_tiers)"
        ),
        0
    );
    let (family_template, recorded_description): (String, String) = db
        .query_row(
            "SELECT e.verbose_name_template, c.to_value FROM set_bonus_tiers t JOIN set_bonuses s ON s.id = t.set_id
         JOIN set_bonus_tier_effects te ON te.tier_id = t.id JOIN effects e ON e.id = te.effect_id
         JOIN corrections c ON c.name = s.name AND c.kind = 'set_tier' AND c.field = 'description'
         WHERE s.name = 'Eminence of Winter' AND t.equipped_count = 2
         ORDER BY te.sort_order LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(family_template, "%b1 Physical Resistance Rating +{1}");
    assert_eq!(recorded_description, "\"+31 Artifact PRR\"");
}

#[test]
fn removing_a_set_tier_deletes_its_unowned_prose_family() {
    let data_files_dir = data_files_with_untyped("remove-prose-family", &[]);
    let set_file = data_files_dir.join("SetBonuses.xml");
    let set_xml = std::fs::read_to_string(&set_file).unwrap();
    let extra_set = "<SetBonus><Type>Removable Family Set</Type><Buff><EquippedCount>2</EquippedCount><Description>Unshared removable prose</Description></Buff></SetBonus></SetBonuses>";
    std::fs::write(&set_file, set_xml.replace("</SetBonuses>", extra_set)).unwrap();
    let correction =
        new_qualified_correction("set_tier", "Removable Family Set", "equipped_count = 2", "remove", "0", "1");
    let (db, report) = built_db_from(&data_files_dir, &[("corrections.toml", &correction)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
    assert_eq!(row_count(&db, "SELECT COUNT(*) FROM effects WHERE name = 'Unshared removable prose'"), 0);
}

#[test]
fn set_tier_qualifiers_must_be_complete_and_match_the_added_tier() {
    for correction in [
        new_correction("set_tier", "Eminence of Winter", "remove", "0", "1"),
        new_qualified_correction("set_tier", "Eminence of Winter", "equipped_count = 0", "remove", "0", "1"),
        new_qualified_correction(
            "set_tier",
            "Eminence of Winter",
            "equipped_count = 3",
            "add",
            "\"null\"",
            "{ equipped_count = 4, description = \"Wrong tier\" }",
        ),
    ] {
        assert!(validated_corrections(&[("corrections.toml", &correction)]).is_err(), "{correction:?}");
    }
}

#[test]
fn committed_data_set_corrections_keep_only_fact_changes() {
    let corrections = committed_corrections().clone();
    let set_bonus_additions = corrections
        .entries
        .iter()
        .filter(|correction| correction.kind.as_str() == "set_tier_bonus" && correction.field == "add");
    let original_additions = 50;
    let superseded_prose_additions = 28;
    let superseded_adamancy_additions = 2;
    let dread_isle_additions = 2;
    let consolidated_conduit_additions = 3 - 1;
    let consolidated_shaman_lore_additions = 4 - 1;
    assert_eq!(
        set_bonus_additions.count(),
        original_additions
            - superseded_prose_additions
            - superseded_adamancy_additions
            - dread_isle_additions
            - consolidated_conduit_additions
            - consolidated_shaman_lore_additions
    );
    assert!(corrections.entries.iter().any(|correction| {
        correction.kind.as_str() == "set_tier_bonus"
            && correction.name == "Heart of Blades"
            && correction.equipped_count == Some(3)
            && correction.stat.as_deref() == Some("Melee and Ranged Power")
            && correction.bonus_type.as_deref() == Some("Artifact")
            && correction.field == "value"
            && correction.from == CorrectionValue::Integer(5)
            && correction.to == CorrectionValue::Integer(10)
    }));
}

#[test]
fn committed_data_set_corrections_fix_feather_falling_and_remove_only_celeritys_movement_bonus() {
    let corrections = committed_corrections().clone();
    assert!(corrections.entries.iter().any(|correction| {
        correction.kind.as_str() == "set_tier"
            && correction.name == "Tharne's Wrath"
            && correction.equipped_count == Some(3)
            && correction.field == "description"
            && correction.from == CorrectionValue::Text("Father Falling".into())
            && correction.to == CorrectionValue::Text("Feather Falling".into())
    }));
    assert!(corrections.entries.iter().any(|correction| {
        correction.kind.as_str() == "set_tier_bonus"
            && correction.name == "Celerity"
            && correction.equipped_count == Some(3)
            && correction.field == "remove"
            && correction.stat.as_deref() == Some("Movement Speed")
            && correction.bonus_type.as_deref() == Some("Enhancement")
            && correction.from == CorrectionValue::Integer(32)
            && correction.to == CorrectionValue::Null
    }));
    assert!(!corrections.entries.iter().any(|correction| {
        correction.kind.as_str() == "set_tier"
            && correction.name == "The Wreath of Flame"
            && correction.equipped_count == Some(3)
            && correction.field == "description"
    }));
}

fn item_effect_names(db: &Connection, item_name: &str) -> Vec<String> {
    db.prepare(
        "SELECT CASE WHEN INSTR(e.name, ' — ') > 0 THEN SUBSTR(e.name, 1, INSTR(e.name, ' — ') - 1) ELSE e.name END
           FROM items i JOIN item_effects ie ON ie.item_id = i.id JOIN effects e ON e.id = ie.effect_id
          WHERE i.name = ?1 AND e.is_stat = 0 AND NOT EXISTS (SELECT 1 FROM effect_bonuses es WHERE es.effect_id = e.id)
          ORDER BY ie.sort_order",
    )
    .unwrap()
    .query_map([item_name], |r| r.get(0))
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

#[test]
fn corrects_a_named_grouping_link_without_changing_its_definition() {
    let corrections = new_qualified_correction(
        "item_bonus",
        "Band of Diani ir'Wynarn",
        "stat = \"Potency\"\nbonus_type = \"Quality\"",
        "bonus_type",
        "\"Quality\"",
        "\"Artifact\"",
    ) + &new_qualified_correction(
        "item_bonus",
        "Band of Diani ir'Wynarn",
        "stat = \"Potency\"\nbonus_type = \"Artifact\"",
        "value",
        "26",
        "30",
    );
    let (db, report) = built_db_with(&[("corrections.toml", &corrections)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (2, 0));
    let bonuses: Vec<(String, i64)> = db
        .prepare(
            "SELECT bt.name, b.amount FROM owner_bonuses b JOIN items i ON i.id = b.owner_id
         JOIN effects e ON e.id = b.via_effect_id JOIN bonus_types bt ON bt.id = b.bonus_type_id
         WHERE b.owner_kind = 'item' AND i.name = 'Band of Diani ir''Wynarn' AND e.name = 'Potency'",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(bonuses, vec![("Artifact".to_string(), 30); 15]);
    assert_eq!(db.query_row("SELECT COUNT(*) FROM effect_bonuses WHERE effect_id = (SELECT id FROM effects WHERE name = 'Potency') AND bonus_type_id IS NULL AND amount_from = 1", [], |row| row.get::<_, i64>(0)).unwrap(), 15);
    let restored = rebuilt_correction_notes(&db);
    assert!(restored.entries.iter().any(|correction| correction.stat.as_deref() == Some("Potency")
        && correction.field == "value"
        && correction.to == CorrectionValue::Integer(30)));
}

#[test]
fn corrects_a_fixed_type_grouping_value_without_adding_a_link_type() {
    let correction = new_qualified_correction(
        "item_bonus",
        "Lantern of the Abyss",
        "stat = \"Power of the Firestorm\"\nbonus_type = \"Equipment\"",
        "value",
        "86",
        "90",
    );
    let (db, report) = built_db_with(&[("corrections.toml", &correction)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
    let link: (i64, Option<i64>) = db.query_row(
        "SELECT link.value, link.bonus_type_id FROM item_effects link JOIN items i ON i.id = link.item_id
         JOIN effects e ON e.id = link.effect_id WHERE i.name = 'Lantern of the Abyss' AND e.name = 'Power of the Firestorm'",
        [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
    assert_eq!(link, (90, None));
    let restored = rebuilt_correction_notes(&db);
    assert!(restored
        .entries
        .iter()
        .any(|entry| entry.name == "Lantern of the Abyss" && entry.to == CorrectionValue::Integer(90)));
}

#[test]
fn adds_one_link_for_a_stat_or_grouping() {
    for (name, bonus_type) in [
        ("Acid Resistance", "Artifact"),
        ("Doublestrike and Doubleshot", "Artifact"),
        ("Potency", "Artifact"),
        ("Power of the Firestorm", "Equipment"),
    ] {
        let correction = new_correction(
            "item_bonus",
            "Docent of Defiance",
            "add",
            "\"null\"",
            &format!("{{ stat = \"{name}\", bonus_type = \"{bonus_type}\", value = 10 }}"),
        );
        let (db, report) = built_db_with(&[("corrections.toml", &correction)]).unwrap();
        assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
        assert_eq!(db.query_row("SELECT COUNT(*) FROM item_effects link JOIN items i ON i.id = link.item_id JOIN effects e ON e.id = link.effect_id WHERE i.name = 'Docent of Defiance' AND e.name = ?1 AND link.value = 10", [name], |row| row.get::<_, i64>(0)).unwrap(), 1);
        let restored = rebuilt_correction_notes(&db);
        assert!(
            restored.entries.iter().any(|entry| entry.name == "Docent of Defiance"
                && entry.field == "add"
                && entry.to == correction.0[0].to),
            "{:?}",
            restored.entries
        );
    }
}

#[test]
fn grouping_additions_reject_two_slots_and_incompatible_fixed_types() {
    for name in ["Deception", "Power of the Firestorm"] {
        let correction = new_correction(
            "item_bonus",
            "Docent of Defiance",
            "add",
            "\"null\"",
            &format!("{{ stat = \"{name}\", bonus_type = \"Artifact\", value = 10 }}"),
        );
        let error = built_db_with(&[("corrections.toml", &correction)]).unwrap_err();
        assert!(error.contains(name) && error.contains("cannot take one value"), "{error}");
    }
}

#[test]
fn adds_a_bonus_an_item_lacks_and_goes_stale_once_he_carries_it() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(new_correction(
            "item_bonus",
            "Docent of Defiance",
            "add",
            "\"null\"",
            "{ stat = \"Acid Resistance\", bonus_type = \"Enhancement\", value = 20 }",
        ) + &new_correction(
            "item_bonus",
            "Docent of Defiance",
            "add",
            "\"null\"",
            "{ stat = \"Fire Resistance\", bonus_type = \"Enhancement\", value = 30 }",
        )),
    )])
    .unwrap();
    assert_eq!(
        (report.correction_applied_count, report.correction_stale_count),
        (1, 1),
        "{:?}",
        report.stale_corrections
    );
    assert_eq!(
        item_bonus_rows(&db, "Docent of Defiance"),
        [
            ("Fire Resistance".into(), Some("Enhancement".into()), Some(20)),
            ("Electric Resistance".into(), Some("Enhancement".into()), Some(20)),
            ("Cold Resistance".into(), Some("Enhancement".into()), Some(20)),
            ("Acid Resistance".into(), Some("Enhancement".into()), Some(20)),
        ]
    );
    let (qualifier, to_value): (String, String) =
        db.query_row("SELECT qualifier, to_value FROM corrections", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(qualifier, "Acid Resistance / Enhancement");
    assert_eq!(to_value, r#"{"bonus_type":"Enhancement","stat":"Acid Resistance","value":20}"#);
    let error = built_db_with(&[(
        "corrections.toml",
        &new_correction(
            "item_bonus",
            "Docent of Defiance",
            "add",
            "\"null\"",
            "{ stat = \"Acid Resistence\", bonus_type = \"Enhancement\", value = 20 }",
        ),
    )])
    .unwrap_err();
    assert!(error.contains("Acid Resistence") && error.contains("Docent of Defiance"), "{error}");
}

#[test]
fn adds_an_effect_an_item_lacks_reusing_his_effect_row_and_goes_stale_once_he_carries_it() {
    let effect_count_without_corrections = row_count(&built_db_with(&[]).unwrap().0, "SELECT COUNT(*) FROM effects");
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(new_correction("item_effect", "Docent of Defiance", "add", "\"null\"", "\"Feather Falling\"")
            + &new_correction("item_effect", "Docent of Defiance", "add", "\"null\"", "\"Book Shot\"")
            + &new_correction("item_effect", "Kundarak Delving Boots", "add", "\"null\"", "\"freedom of movement\"")),
    )])
    .unwrap();
    assert_eq!(
        (report.correction_applied_count, report.correction_stale_count),
        (2, 1),
        "{:?}",
        report.stale_corrections
    );
    assert_eq!(
        item_effect_names(&db, "Docent of Defiance"),
        ["Hidden Effect - Cursed Defiance", "Feather Falling", "Book Shot"]
    );
    assert_eq!(item_effect_names(&db, "Kundarak Delving Boots"), ["Freedom of Movement"]);
    assert_eq!(row_count(&db, "SELECT COUNT(*) FROM effects"), effect_count_without_corrections + 1);
    let qualifiers: Vec<String> = db
        .prepare("SELECT qualifier FROM corrections ORDER BY qualifier")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(qualifiers, ["Book Shot", "Feather Falling"]);
    let error = validated_corrections(&[(
        "corrections.toml",
        &new_correction("item_effect", "Docent of Defiance", "add", "\"null\"", "3"),
    )])
    .unwrap_err();
    assert!(error.contains("Docent of Defiance") && error.contains("effect"), "{error}");
}

fn effect_description(db: &Connection, effect_name: &str) -> Option<String> {
    db.query_row("SELECT description_template FROM effects WHERE name = ?1", [effect_name], |r| r.get(0)).unwrap()
}

#[test]
fn an_added_effect_writes_its_description_only_on_the_effect_it_creates() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(new_correction(
            "item_effect",
            "Docent of Defiance",
            "add",
            "\"null\"",
            "{ name = \"Book Shot\", description = \"Hurls books at enemies.\" }",
        ) + &new_correction(
            "item_effect",
            "Docent of Defiance",
            "add",
            "\"null\"",
            "{ name = \"Feather Falling\", description = \"Not his text.\" }",
        ) + &new_correction(
            "item_effect",
            "Kundarak Delving Boots",
            "add",
            "\"null\"",
            "{ name = \"freedom of movement\", description = \"Not his text either.\" }",
        )),
    )])
    .unwrap();
    assert_eq!(
        (report.correction_applied_count, report.correction_stale_count),
        (2, 1),
        "{:?}",
        report.stale_corrections
    );
    assert_eq!(
        item_effect_names(&db, "Docent of Defiance"),
        ["Hidden Effect - Cursed Defiance", "Book Shot", "Feather Falling"]
    );
    assert_eq!(effect_description(&db, "Book Shot").as_deref(), Some("Hurls books at enemies."));
    assert!(
        effect_description(&db, "Feather Falling").unwrap().starts_with("This item"),
        "his description wins on the effect row the correction reuses"
    );
    let recorded: Vec<(String, String)> = db
        .prepare("SELECT qualifier, to_value FROM corrections WHERE qualifier = 'Book Shot'")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        recorded,
        [("Book Shot".to_string(), r#"{"description":"Hurls books at enemies.","name":"Book Shot"}"#.to_string())]
    );
}

#[test]
fn adds_a_socket_an_item_lacks_and_goes_stale_once_he_carries_it() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(new_correction("item_socket", "Docent of Defiance", "add", "\"null\"", "\"red\"")
            + &new_correction(
                "item_socket",
                "Docent of Defiance",
                "add",
                "\"null\"",
                "\"crafting: slavelords extra\"",
            )
            + &new_correction("item_socket", "Buckler of the Golden Age", "add", "\"null\"", "\"red\"")),
    )])
    .unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (2, 1));
    assert_eq!(item_socket_labels(&db, "Docent of Defiance"), ["red", "crafting: slavelords extra"]);
    assert_eq!(item_socket_labels(&db, "Buckler of the Golden Age"), ["red"]);
    let error = built_db_with(&[(
        "corrections.toml",
        &new_correction("item_socket", "Docent of Defiance", "add", "\"null\"", "\"crafting: no such socket\""),
    )])
    .unwrap_err();
    assert!(error.contains("crafting: no such socket"), "{error}");
}

#[test]
fn distinct_quest_names_with_colliding_slugs_are_not_silently_skipped() {
    let directory = data_files_with_untyped("quest-slug-collision", &[]);
    let path = directory.join("Quests.xml");
    let quests = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        quests
            .replace("<Name>Land of Lamordia</Name>", "<Name>Slug Probe</Name>")
            .replace("<Name>Redemption</Name>", "<Name>Slug-Probe</Name>"),
    )
    .unwrap();
    let error = built_db_from(&directory, &[]).unwrap_err();
    assert!(error.contains("quests.slug"), "{error}");
    std::fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}

#[test]
fn colliding_socket_label_slugs_apply_the_recorded_rename_in_either_input_order() {
    for reversed in [false, true] {
        let directory = data_files_with_untyped(&format!("socket-slug-{reversed}"), &[]);
        let path = directory.join("Items/Sireth, Spear of the Sky.item");
        let original = std::fs::read_to_string(&path).unwrap();
        let labels = if reversed {
            ["Upgradeable Tier 2", "Upgradeable - Tier 2"]
        } else {
            ["Upgradeable - Tier 2", "Upgradeable Tier 2"]
        };
        std::fs::write(
            &path,
            original
                .replace("<Type>Attuned to Heroism 1</Type>", &format!("<Type>{}</Type>", labels[0]))
                .replace("<Type>Attuned to Heroism 2</Type>", &format!("<Type>{}</Type>", labels[1])),
        )
        .unwrap();
        let correction = new_correction(
            "socket_label",
            "crafting: upgradeable - tier 2",
            "name",
            "\"crafting: upgradeable - tier 2\"",
            "\"crafting: upgradeable tier 2\"",
        );
        let (db, report) = built_db_from(&directory, &[("labels.toml", &correction)]).unwrap();
        assert_eq!(
            &item_socket_labels(&db, "Sireth, Spear of the Sky")[..2],
            ["crafting: upgradeable tier 2", "crafting: upgradeable tier 2"]
        );
        assert_eq!(report.correction_applied_count, 1);
        assert!(report.stale_corrections.is_empty());
        assert_eq!(recorded_corrections(&db).len(), 1);
        assert_eq!(
            row_count(&db, "SELECT COUNT(*) FROM augment_slot_types WHERE slug = 'crafting-upgradeable-tier-2'"),
            1
        );
        std::fs::remove_dir_all(directory.parent().unwrap()).unwrap();
    }
}

fn merge_of(kind: &str, spellings: &[&str], target: &str) -> CorrectionSet {
    fixture::correction_between(
        kind,
        spellings[0],
        "name",
        CorrectionValue::Texts(spellings.iter().map(|spelling| spelling.to_string()).collect()),
        CorrectionValue::Text(target.into()),
    )
}

#[test]
fn a_socket_label_merge_lists_two_distinct_spellings_and_only_socket_labels_merge() {
    let valid = merge_of(
        "socket_label",
        &["crafting: attuned to heroism 1", "crafting: attuned to heroism tier 1"],
        "crafting: attuned by heroism: tier 1",
    );
    assert!(validated_corrections(&[("corrections.toml", &valid)]).is_ok());
    for (merge, expected_text) in [
        (merge_of("adventure_pack", &["Vault of Night", "Vault of Nite"], "The Vault of Night"), "socket_label"),
        (merge_of("socket_label", &["crafting: one"], "crafting: two"), "two distinct spellings"),
        (merge_of("socket_label", &["crafting: one", "crafting: one"], "crafting: two"), "two distinct spellings"),
        (merge_of("socket_label", &["crafting: one", "crafting: two"], "crafting: two"), "none of them its to"),
    ] {
        let error = validated_corrections(&[("corrections.toml", &merge)]).unwrap_err();
        assert!(error.contains(expected_text), "{expected_text}: {error}");
    }
}

#[test]
fn a_socket_label_merge_renames_every_listed_spelling_into_one_row() {
    let merge = merge_of(
        "socket_label",
        &["crafting: attuned to heroism 1", "crafting: attuned to heroism 2"],
        "crafting: attuned by heroism: tier 1",
    );
    let (db, report) = built_db_with(&[("corrections.toml", &merge)]).unwrap();
    assert_eq!(
        (report.correction_applied_count, report.correction_stale_count),
        (1, 0),
        "{:?}",
        report.stale_corrections
    );
    assert_eq!(
        &item_socket_labels(&db, "Sireth, Spear of the Sky")[..2],
        ["crafting: attuned by heroism: tier 1", "crafting: attuned by heroism: tier 1"]
    );
    assert_eq!(
        row_count(&db, "SELECT COUNT(*) FROM augment_slot_types WHERE label IN ('crafting: attuned to heroism 1', 'crafting: attuned to heroism 2')"),
        0
    );
    assert_eq!(
        recorded_corrections(&db),
        [(
            "socket_label".into(),
            "crafting: attuned to heroism 1".into(),
            "name".into(),
            r#"["crafting: attuned to heroism 1","crafting: attuned to heroism 2"]"#.into(),
            r#""crafting: attuned by heroism: tier 1""#.into()
        )]
    );
}

#[test]
fn renames_a_socket_label_everywhere_and_merges_into_an_existing_one() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &(new_correction(
            "socket_label",
            "crafting: attuned to heroism 1",
            "name",
            "\"crafting: attuned to heroism 1\"",
            "\"crafting: attuned by heroism: tier 1\"",
        ) + &new_correction(
            "socket_label",
            "crafting: attuned to heroism 3",
            "name",
            "\"crafting: attuned to heroism 3\"",
            "\"crafting: attuned to heroism 2\"",
        )),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 2, "{:?}", report.stale_corrections);
    assert_eq!(
        item_socket_labels(&db, "Sireth, Spear of the Sky"),
        [
            "crafting: attuned by heroism: tier 1",
            "crafting: attuned to heroism 2",
            "crafting: attuned to heroism 2",
            "crafting: attuned to heroism 4"
        ]
    );
    assert_eq!(
        row_count(&db, "SELECT COUNT(*) FROM augment_slot_types WHERE label LIKE 'crafting: attuned to heroism 3'"),
        0
    );
    let (family, variant): (String, String) = db
        .query_row(
            "SELECT family, variant FROM augment_slot_types WHERE label = 'crafting: attuned by heroism: tier 1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((family.as_str(), variant.as_str()), ("crafting", "attuned by heroism: tier 1"));
    assert_eq!(report.augment_slot_type_count, row_count(&db, "SELECT COUNT(*) FROM augment_slot_types") as usize);
}

fn copied_directory(source_dir: &std::path::Path, copy_dir: &std::path::Path) {
    std::fs::create_dir_all(copy_dir).unwrap();
    for entry in std::fs::read_dir(source_dir).unwrap() {
        let entry = entry.unwrap();
        let copy_path = copy_dir.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copied_directory(&entry.path(), &copy_path);
        } else {
            std::fs::copy(entry.path(), copy_path).unwrap();
        }
    }
}

const UNTYPED_ITEMS: (&str, &str) = ("untyped_items", "Items");
const UNTYPED_AUGMENTS: (&str, &str) = ("untyped_augments", "Augments");
const UNTYPED_ITEM_AUGMENT_SLOT_OPTIONS: (&str, &str) = ("untyped_item_augment_slot_options", "Items");

fn data_files_with_untyped(test_name: &str, untyped_fixture_dirs: &[(&str, &str)]) -> PathBuf {
    let data_files_dir =
        std::env::temp_dir().join(format!("ddo-etl-untyped-{test_name}-{}", std::process::id())).join("DataFiles");
    let _ = std::fs::remove_dir_all(&data_files_dir);
    copied_directory(&fixtures_dir().join("DataFiles"), &data_files_dir);
    for (fixture_dir_name, data_files_subdir) in untyped_fixture_dirs {
        copied_directory(&fixtures_dir().join(fixture_dir_name), &data_files_dir.join(data_files_subdir));
    }
    data_files_dir
}

fn built_db_from(
    data_files_dir: &std::path::Path,
    correction_files: &[(&str, &CorrectionSet)],
) -> Result<(Connection, BuildReport), String> {
    built_db_from_index(fixture::groupings(), data_files_dir, correction_files)
}

fn built_db_from_index(
    groupings: &ddo_etl::groupings::Groupings,
    source: &std::path::Path,
    correction_files: &[(&str, &CorrectionSet)],
) -> Result<(Connection, BuildReport), String> {
    let mut db = Connection::open_in_memory().unwrap();
    let report = build_database(
        groupings,
        source,
        &WikiOverrides::default(),
        &validated_corrections(correction_files)?,
        &mut db,
        &fixture_dataset_version(),
    )
    .map_err(|e| format!("{e:#}"))?;
    Ok((db, report))
}

#[test]
fn a_definition_correction_cannot_change_a_grouping_answer() {
    let scale = new_qualified_correction(
        "set_tier_bonus",
        "Anger of the Avalanche",
        "equipped_count = 2\nstat = \"Power of the Frozen Thunderstorm\"\nbonus_type = \"Artifact\"",
        "scale",
        "1.0",
        "0.5",
    );
    let bonus_type = new_qualified_correction(
        "effect",
        "Power of the Frozen Thunderstorm",
        "stat = 'Cold Spell Power'",
        "bonus_type",
        "\"null\"",
        "\"Equipment\"",
    );
    let removal = new_qualified_correction(
        "effect",
        "Power of the Frozen Thunderstorm",
        "stat = 'Cold Spell Power'",
        "remove",
        "{ target = 'Cold Spell Power', amount_from = 1, scale = 1.0, rounding = 'down' }",
        "'null'",
    );
    for correction in [scale, bonus_type, removal] {
        let error = built_db_with(&[("grouping-definition.toml", &correction)]).unwrap_err();
        assert!(error.contains("Power of the Frozen Thunderstorm") && error.contains("lookup answer"), "{error}");
    }
}

#[test]
fn effect_bonus_type_correction_changes_one_fixed_row_and_tracks_his_type() {
    let correction = new_qualified_correction(
        "effect",
        "Lifeblood of the Undead Prince",
        "stat = 'Repair Amplification'",
        "bonus_type",
        "\"Competence\"",
        "\"Enhancement\"",
    );
    for upstream_type in ["Competence", "Insight", "Enhancement"] {
        let source = fixture::xml_source();
        fixture::write_diadem_item(source.path());
        let path = source.path().join("ItemBuffs.xml");
        let original = std::fs::read_to_string(&path).unwrap();
        let lifeblood = include_str!("fixtures/effect-groups/TheDiademOfRot.Buffs.xml");
        std::fs::write(
            path,
            original.replace(
                lifeblood,
                &lifeblood.replace("<Bonus>Competence</Bonus>", &format!("<Bonus>{upstream_type}</Bonus>")),
            ),
        )
        .unwrap();
        let (db, report) = built_db_from(source.path(), &[("lifeblood.toml", &correction)]).unwrap();
        let bonuses = db
            .prepare("SELECT target.name, bt.name, b.amount_from FROM effect_bonuses b JOIN effects e ON e.id=b.effect_id JOIN effects target ON target.id=b.target_effect_id JOIN bonus_types bt ON bt.id=b.bonus_type_id WHERE e.name='Lifeblood of the Undead Prince' ORDER BY target.name")
            .unwrap().query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)))
            .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        let repair_type = if upstream_type == "Competence" { "Enhancement" } else { upstream_type };
        assert_eq!(
            bonuses,
            [
                ("Healing Amplification".into(), upstream_type.into(), 1),
                ("Negative Healing Amplification".into(), "Profane".into(), 1),
                ("Repair Amplification".into(), repair_type.into(), 1),
            ]
        );
        if upstream_type == "Competence" {
            assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
            let restored = Corrections::from_database(&db).unwrap();
            assert_eq!(restored.entries[0].qualifier(), "stat \"Repair Amplification\"");
            assert_eq!(restored.entries[0].to, CorrectionValue::Text("Enhancement".into()));
        } else {
            assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 1));
            assert!(
                matches!(&report.stale_corrections[0].cause,
                StaleCorrectionCause::ValueChanged { expected_value, maetrim_value }
                if expected_value == "\"Competence\"" && maetrim_value == &format!("\"{upstream_type}\"")),
                "{:?}",
                report.stale_corrections
            );
        }
    }
}

#[test]
fn effect_bonus_type_correction_rejects_an_absent_stat_and_names_its_file() {
    let source = fixture::xml_source();
    fixture::write_diadem_item(source.path());
    let correction = new_qualified_correction(
        "effect",
        "Lifeblood of the Undead Prince",
        "stat = 'Strength'",
        "bonus_type",
        "\"Competence\"",
        "\"Enhancement\"",
    );
    let error = built_db_from(source.path(), &[("lifeblood-missing-stat.toml", &correction)]).unwrap_err();
    for expected in ["lifeblood-missing-stat.toml", "Lifeblood of the Undead Prince", "Strength", "exactly once"] {
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn effect_bonus_removal_deletes_only_its_row_and_reports_a_missing_member() {
    let upstream = "{ target = 'Repair Amplification', bonus_type = 'Competence', amount_from = 1, scale = 1.0, rounding = 'down' }";
    let correction = new_qualified_correction(
        "effect",
        "Lifeblood of the Undead Prince",
        "stat = 'Repair Amplification'",
        "remove",
        upstream,
        "'null'",
    );
    for has_repair in [true, false] {
        let source = fixture::xml_source();
        fixture::write_diadem_item(source.path());
        if !has_repair {
            let path = source.path().join("ItemBuffs.xml");
            let original = std::fs::read_to_string(&path).unwrap();
            std::fs::write(path, original.replace("      <Type>RepairAmplification</Type>\n", "")).unwrap();
        }
        let (db, report) = built_db_from(source.path(), &[("effect-removal.toml", &correction)]).unwrap();
        let bonuses = db.prepare("SELECT target.name, bt.name, b.amount_from, b.constant, b.scale, b.rounding FROM effect_bonuses b JOIN effects e ON e.id=b.effect_id JOIN effects target ON target.id=b.target_effect_id JOIN bonus_types bt ON bt.id=b.bonus_type_id WHERE e.name='Lifeblood of the Undead Prince' ORDER BY target.name")
            .unwrap().query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, Option<i64>>(3)?, row.get::<_, f64>(4)?, row.get::<_, String>(5)?)))
            .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        assert_eq!(
            bonuses,
            [
                ("Healing Amplification".into(), "Competence".into(), 1, None, 1.0, "down".into()),
                ("Negative Healing Amplification".into(), "Profane".into(), 1, None, 1.0, "down".into()),
            ]
        );
        if has_repair {
            assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
            let restored = Corrections::from_database(&db).unwrap();
            assert_eq!(restored.entries[0].from, fixture::correction_value(upstream));
            assert_eq!(restored.entries[0].to, CorrectionValue::Null);
        } else {
            assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 1));
            assert!(
                matches!(&report.stale_corrections[0].cause, StaleCorrectionCause::ValueChanged { maetrim_value, .. } if maetrim_value == "\"no such bonus\"")
            );
        }
    }
}

#[test]
fn effect_bonus_removal_tracks_changes_to_constant_amounts() {
    let correction = new_qualified_correction(
        "effect",
        "Lifeblood of the Undead Prince",
        "stat = 'Repair Amplification'",
        "remove",
        "58",
        "'null'",
    );
    for amount in [58, 59] {
        let source = fixture::xml_source();
        fixture::write_diadem_item(source.path());
        let path = source.path().join("ItemBuffs.xml");
        let original = std::fs::read_to_string(&path).unwrap();
        let definitions = include_str!("fixtures/effect-groups/TheDiademOfRot.Buffs.xml");
        let updated = definitions.replace("%v1", "58").replace(">58</Amount>", &format!(">{amount}</Amount>"));
        std::fs::write(path, original.replace(definitions, &updated)).unwrap();
        let (db, report) = built_db_from(source.path(), &[("constant-removal.toml", &correction)]).unwrap();
        let remaining = db.query_row("SELECT b.constant FROM effect_bonuses b JOIN effects e ON e.id=b.effect_id JOIN effects target ON target.id=b.target_effect_id WHERE e.name='Lifeblood of the Undead Prince' AND target.name='Repair Amplification'", [], |row| row.get::<_, i64>(0)).optional().unwrap();
        if amount == 58 {
            assert_eq!(remaining, None);
            assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
        } else {
            assert_eq!(remaining, Some(59));
            assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 1));
            assert!(
                matches!(&report.stale_corrections[0].cause, StaleCorrectionCause::ValueChanged { maetrim_value, .. } if maetrim_value == "59")
            );
        }
    }
}

#[test]
fn effect_bonus_removal_rejects_unknown_stats_and_malformed_qualifiers() {
    let source = fixture::xml_source();
    fixture::write_diadem_item(source.path());
    for (qualifier, expected) in [
        ("stat = 'Imaginary Amplification'", "unknown stat"),
        ("", "exactly one stat"),
        ("stat = 'Repair Amplification'\nbonus_type = 'Competence'", "exactly one stat"),
    ] {
        let correction =
            new_qualified_correction("effect", "Lifeblood of the Undead Prince", qualifier, "remove", "58", "'null'");
        let error = built_db_from(source.path(), &[("invalid-effect-removal.toml", &correction)]).unwrap_err();
        assert!(error.contains("invalid-effect-removal.toml") && error.contains(expected), "{error}");
    }
    let correction = new_qualified_correction(
        "effect",
        "Lifeblood of the Undead Prince",
        "stat = 'Strength'",
        "remove",
        "58",
        "'null'",
    );
    let (_, report) = built_db_from(source.path(), &[("absent-effect-member.toml", &correction)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 1));
    assert!(
        matches!(&report.stale_corrections[0].cause, StaleCorrectionCause::ValueChanged { maetrim_value, .. } if maetrim_value == "\"no such bonus\"")
    );
}

#[test]
fn periapt_of_dexterity_replaces_the_skill_group_with_insightful_seeker() {
    let mut corrections = committed_corrections().clone();
    corrections.entries.retain(|correction| correction.name == "Periapt of Dexterity");
    for upstream_corrected in [false, true] {
        let source = fixture::xml_source();
        let definitions = source.path().join("ItemBuffs.xml");
        let original = std::fs::read_to_string(&definitions).unwrap();
        std::fs::write(
            definitions,
            original.replace(
                "</Buffs>",
                &format!("{}</Buffs>", include_str!("fixtures/effect-groups/PeriaptOfDexterity.Buffs.xml")),
            ),
        )
        .unwrap();
        let mut item = include_str!("fixtures/effect-groups/PeriaptOfDexterity.item").to_string();
        if upstream_corrected {
            item = item.replace("<Type>Dexterity Skills - Exceptional Bonus</Type>\n      <Value1>2</Value1>\n      <BonusType>Exceptional</BonusType>", "<Type>Seeker</Type>\n      <Value1>3</Value1>\n      <BonusType>Insight</BonusType>");
        }
        std::fs::write(source.path().join("Items/PeriaptOfDexterity.item"), item).unwrap();
        let icon = source.path().join("ItemImages/Trinkets/Trinket_101.png");
        std::fs::create_dir_all(icon.parent().unwrap()).unwrap();
        std::fs::write(icon, include_bytes!("fixtures/effect-groups/icons/Trinket_101.png")).unwrap();
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
        let bonuses = db.prepare("SELECT stat.name, bt.name, ob.amount FROM owner_bonuses ob JOIN items i ON ob.owner_kind='item' AND ob.owner_id=i.id JOIN effects stat ON stat.id=ob.stat_id JOIN bonus_types bt ON bt.id=ob.bonus_type_id WHERE i.name='Periapt of Dexterity' ORDER BY stat.name").unwrap()
            .query_map([], |row| Ok((row.get::<_,String>(0)?, row.get::<_,String>(1)?, row.get::<_,i64>(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        assert_eq!(
            bonuses,
            [
                ("Dexterity".into(), "Enhancement".into(), 7),
                ("Dodge".into(), "Quality".into(), 1),
                ("Reflex Save".into(), "Resistance".into(), 7),
                ("Seeker".into(), "Insight".into(), 3)
            ]
        );
        let lines: Vec<(String, Option<String>)> = db.prepare("SELECT e.name,l.alias FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effects e ON e.id=l.effect_id WHERE i.name='Periapt of Dexterity' ORDER BY e.name").unwrap()
            .query_map([], |row| Ok((row.get(0)?,row.get(1)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
        assert_eq!(
            lines,
            [("Dexterity".into(), None), ("Dodge".into(), None), ("Reflex Save".into(), None), ("Seeker".into(), None)]
        );
        if upstream_corrected {
            assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 2));
            assert!(report.stale_corrections.iter().any(|stale| matches!(&stale.cause, StaleCorrectionCause::ValueChanged { maetrim_value, .. } if maetrim_value == "\"no such bonus\"")));
        } else {
            assert_eq!((report.correction_applied_count, report.correction_stale_count), (2, 0));
        }
    }
    let unknown = new_qualified_correction(
        "item_bonus",
        "Docent of Defiance",
        "stat = 'Imaginary Skills'\nbonus_type = 'Exceptional'",
        "remove",
        "2",
        "'null'",
    );
    let error = built_db_with(&[("misspelled-removal.toml", &unknown)]).unwrap_err();
    assert!(
        error.contains("misspelled-removal.toml")
            && error.contains("Imaginary Skills")
            && error.contains("not an effect"),
        "{error}"
    );
    let unknown_type = new_qualified_correction(
        "item_bonus",
        "Docent of Defiance",
        "stat = 'Dexterity Skills'\nbonus_type = 'Imaginary Type'",
        "remove",
        "2",
        "'null'",
    );
    let error = built_db_from_index(
        fixture::grouping_cases(),
        &fixtures_dir().join("DataFiles"),
        &[("misspelled-type.toml", &unknown_type)],
    )
    .unwrap_err();
    assert!(error.contains("misspelled-type.toml") && error.contains("Imaginary Type"), "{error}");
}

#[test]
fn deaths_touch_uses_its_cited_void_lore_rank_without_changing_the_bonus() {
    let canonical = fixture::grouping_case_database(|root| {
        let original = std::fs::read_to_string(root.join("effects/void-lore-vi.toml")).unwrap();
        std::fs::write(root.join("effects/void-lore-iv.toml"), original.replace("Void Lore VI", "Void Lore IV"))
            .unwrap();
    });
    let groupings = ddo_etl::groupings::Groupings::from_database(&canonical).unwrap();
    let mut corrections = committed_corrections().clone();
    corrections.entries.retain(|correction| correction.name == "Death's Touch");
    for rank in ["VI", "IV"] {
        let source = fixture::xml_source();
        let definitions = source.path().join("ItemBuffs.xml");
        let original = std::fs::read_to_string(&definitions).unwrap();
        std::fs::write(
            definitions,
            original.replace(
                "</Buffs>",
                &format!("{}</Buffs>", include_str!("fixtures/effect-groups/DeathsTouch.Buffs.xml")),
            ),
        )
        .unwrap();
        std::fs::write(
            source.path().join("Items/DeathsTouch.item"),
            include_str!("fixtures/effect-groups/DeathsTouch.item")
                .replace("<Description1>VI</Description1>", &format!("<Description1>{rank}</Description1>")),
        )
        .unwrap();
        let icon = source.path().join("ItemImages/Weapon_Sickle/Sickle_4.png");
        std::fs::create_dir_all(icon.parent().unwrap()).unwrap();
        std::fs::write(icon, include_bytes!("fixtures/effect-groups/icons/Sickle_4.png")).unwrap();
        let (before, _) = built_db_from_index(&groupings, source.path(), &[]).unwrap();
        let mut after = Connection::open_in_memory().unwrap();
        let report = build_database(
            &groupings,
            source.path(),
            &WikiOverrides::default(),
            &corrections,
            &mut after,
            &fixture_dataset_version(),
        )
        .unwrap();
        let links = after.prepare("SELECT e.name, l.value, b.name FROM item_effects l JOIN items i ON i.id=l.item_id JOIN effects e ON e.id=l.effect_id JOIN bonus_types b ON b.id=l.bonus_type_id WHERE i.name='Death''s Touch' AND e.name LIKE 'Void Lore%' ORDER BY l.sort_order").unwrap()
            .query_map([], |row| Ok((row.get::<_, String>(0)?,row.get::<_, i64>(1)?,row.get::<_, String>(2)?)))
            .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        assert_eq!(links, [("Void Lore IV".into(), 14, "Equipment".into())]);
        let bonuses = |db: &Connection| {
            db.prepare("SELECT owner_kind,owner_id,stat_id,bonus_type_id,amount FROM owner_bonuses ORDER BY owner_kind,owner_id,stat_id,bonus_type_id,amount").unwrap()
                .query_map([], |row| Ok((row.get::<_, String>(0)?,row.get::<_, i64>(1)?,row.get::<_, i64>(2)?,row.get::<_, i64>(3)?,row.get::<_, i64>(4)?)))
                .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };
        assert_eq!(bonuses(&before), bonuses(&after));
        if rank == "VI" {
            assert_eq!((report.correction_applied_count, report.correction_stale_count), (2, 0));
        } else {
            assert_eq!((report.correction_applied_count, report.correction_stale_count), (0, 2));
            assert!(report.stale_corrections.iter().any(|stale| matches!(&stale.cause, StaleCorrectionCause::ValueChanged { maetrim_value, .. } if maetrim_value == "\"no such bonus\"")));
        }
    }
}

#[test]
fn lore_definition_corrections_keep_only_the_wiki_stats() {
    let source = fixture::xml_source();
    fixture::write_lore_items(source.path());
    let corrections = ["Chaos", "Evil", "Good", "Law"]
        .into_iter()
        .map(|alignment| {
            let stat = format!("{alignment} Spell Lore");
            new_qualified_correction(
                "effect",
                "Purifying Flame Lore",
                &format!("stat = {stat:?}"),
                "remove",
                &format!("{{ target = {stat:?}, amount_from = 1, scale = 1.0, rounding = 'down' }}"),
                "'null'",
            )
        })
        .reduce(|corrections, correction| corrections + &correction)
        .unwrap();
    let (db, report) = built_db_from(source.path(), &[("purifying-flame-lore.toml", &corrections)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (4, 0));
    for (effect, expected, amount) in [
        ("Purifying Flame Lore", vec!["Fire Spell Lore", "Light Spell Lore"], 14),
        (
            "Radiance Lore",
            vec!["Chaos Spell Lore", "Evil Spell Lore", "Good Spell Lore", "Law Spell Lore", "Light Spell Lore"],
            15,
        ),
    ] {
        let rows = db.prepare("SELECT stat.name, ob.amount, bt.name FROM owner_bonuses ob JOIN effects e ON e.id=ob.via_effect_id JOIN effects stat ON stat.id=ob.stat_id JOIN bonus_types bt ON bt.id=ob.bonus_type_id WHERE ob.owner_kind='item' AND e.name=?1 ORDER BY stat.name")
            .unwrap().query_map([effect], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?)))
            .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        assert_eq!(
            rows,
            expected.into_iter().map(|stat| (stat.to_string(), amount, "Equipment".to_string())).collect::<Vec<_>>(),
            "{effect}"
        );
    }
}

#[test]
fn owner_bonuses_resolve_owner_default_and_constant_amounts() {
    let data_files_dir = data_files_with_untyped("owner-bonus-sources", &[]);
    fixture::write_ship_kickers(&data_files_dir);
    let buffs_file = data_files_dir.join("ItemBuffs.xml");
    let buffs_source = std::fs::read_to_string(&buffs_file).unwrap();
    let real_effect_sources = r#"  <Buff>
    <Type>Riposte</Type>
    <DisplayText>Riposte %v1: This item enchants its user with incredible awareness and speed, granting a %v1 %b1 bonus to AC and to Saves.</DisplayText>
    <Ignore>Insightful</Ignore>
    <Effect><Type>ACBonus</Type><Bonus>Not Set</Bonus><AType>Simple</AType><Amount size="1">2</Amount></Effect>
    <Effect><Type>SaveBonus</Type><Bonus>Not Set</Bonus><AType>Simple</AType><Amount size="1">2</Amount><Item>All</Item></Effect>
  </Buff>
  <Buff>
    <Type>Ghostly</Type>
    <DisplayText>Ghostly: Equipping this item causes you to become partially incorporeal. Your melee attacks do not roll a miss chance for Incorporeal targets. Enemy attacks have a 10% chance to miss you due to incorporeality. You receive a +5 enhancement bonus to your Hide and Move Silently skills.</DisplayText>
    <Effect><Type>GhostTouch</Type><Bonus>Equipment</Bonus><AType>Simple</AType><Amount size="1">1</Amount><Item>All</Item></Effect>
    <Effect><Type>Incorporeality</Type><Bonus>Equipment</Bonus><AType>Simple</AType><Amount size="1">10</Amount><Item>All</Item></Effect>
    <Effect><Type>SkillBonus</Type><Bonus>Enhancement</Bonus><AType>Simple</AType><Amount size="1">5</Amount><Item>Hide</Item><Item>Move Silently</Item></Effect>
  </Buff>
"#;
    std::fs::write(&buffs_file, buffs_source.replace("</Buffs>", &format!("{real_effect_sources}</Buffs>"))).unwrap();
    let cloak_file = data_files_dir.join("Items/Legendary Cloak of Winter.item");
    let cloak_source = std::fs::read_to_string(&cloak_file).unwrap();
    assert!(cloak_source.contains("<Type>FalseLife</Type>\n      <Value1>50</Value1>"));
    std::fs::write(
        &cloak_file,
        cloak_source.replace("<Type>FalseLife</Type>\n      <Value1>50</Value1>", "<Type>FalseLife</Type>"),
    )
    .unwrap();
    let (db, _) = built_db_from(&data_files_dir, &[]).unwrap();
    let resolved: Vec<(String, i64, String, f64)> = db
        .prepare(
            "SELECT s.name, ob.amount, ob.amount_source, ob.scale FROM owner_bonuses ob
             JOIN effects s ON s.id = ob.stat_id JOIN items i ON i.id = ob.owner_id
             WHERE ob.owner_kind = 'item' AND ((i.name IN ('Ship Kickers','Epic Ethereal Bracers') AND s.name IN
             ('Armor Class', 'Saving Throws', 'Hide')) OR (i.name = 'Legendary Cloak of Winter'
             AND s.name = 'Hit Points')) ORDER BY i.name, s.name",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(resolved.contains(&("Armor Class".into(), 3, "owner".into(), 1.0)));
    assert!(resolved.contains(&("Saving Throws".into(), 3, "owner".into(), 1.0)));
    assert!(resolved.contains(&("Hide".into(), 5, "constant".into(), 1.0)));
    assert!(resolved.contains(&("Hit Points".into(), 10, "default".into(), 1.0)));
}

#[test]
fn a_stacked_critical_range_feat_stays_text_without_a_value() {
    let data_files_dir = data_files_with_untyped("stacked-critical-feat", &[]);
    let feats_file = data_files_dir.join("Feats.xml");
    let feats_source = std::fs::read_to_string(&feats_file).unwrap();
    let piercing_feat = r#"  <Feat>
    <Name>Improved Critical: Piercing Weapons</Name>
    <Description>Doubles the base critical hit threat range of any piercing weapon you use.</Description>
    <Group>Standard</Group><Acquire>Train</Acquire><Icon>ImprovedCriticalPiercing</Icon>
    <Effect><DisplayName>Swashbuckler: Swashbuckling</DisplayName><Type>Weapon_CriticalRange</Type>
      <Bonus>Competence</Bonus><AType>Stacks</AType><Amount size="3">0 1 2</Amount>
      <ApplyAsItemEffect/><Item>Dagger</Item></Effect>
  </Feat>
"#;
    std::fs::write(&feats_file, feats_source.replace("</Feats>", &format!("{piercing_feat}</Feats>"))).unwrap();
    let (db, _) = built_db_from(&data_files_dir, &[]).unwrap();
    let (effect_name, value, bonus_rows): (String, Option<i64>, i64) = db
        .query_row(
            "SELECT e.name, fe.value, (SELECT COUNT(*) FROM effect_bonuses b WHERE b.effect_id = e.id)
             FROM feat_effects fe JOIN feats f ON f.id = fe.feat_id JOIN effects e ON e.id = fe.effect_id
             WHERE f.name = 'Improved Critical: Piercing Weapons' AND e.name LIKE 'Critical Threat Range (%'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(effect_name, "Critical Threat Range (Dagger)");
    assert_eq!(value, None);
    assert_eq!(bonus_rows, 0);
}

#[test]
fn a_merged_stat_keeps_each_owners_source_bonus_type() {
    let data_files_dir = data_files_with_untyped("illusion-save-types", &[]);
    let augments_file = data_files_dir.join("Augments/Named.Augments.xml");
    let augments_source = std::fs::read_to_string(&augments_file).unwrap();
    let illusion_augments = r#"  <Augment><Name>Illusion Resistance</Name><Description>Resistance to illusions.</Description>
    <MinLevel>1</MinLevel><Type>Blue</Type><Icon>Cannith</Icon>
    <Effect><Type>SaveBonus</Type><Bonus>Resistance</Bonus><AType>Simple</AType>
      <Amount size="1">11</Amount><Item>Illusion</Item></Effect></Augment>
  <Augment><Name>Insightful Illusion Resistance</Name><Description>Insight into illusions.</Description>
    <MinLevel>1</MinLevel><Type>Blue</Type><Icon>Cannith</Icon>
    <Effect><Type>SaveBonus</Type><Bonus>Insightful</Bonus><AType>Simple</AType>
      <Amount size="1">11</Amount><Item>Illusion</Item></Effect></Augment>
"#;
    std::fs::write(&augments_file, augments_source.replace("</Augments>", &format!("{illusion_augments}</Augments>")))
        .unwrap();
    let (db, _) = built_db_from(&data_files_dir, &[]).unwrap();
    let rows: Vec<(String, String, i64)> = db
        .prepare(
            "SELECT a.name, bt.name, ob.amount FROM owner_bonuses ob
             JOIN augments a ON a.id = ob.owner_id JOIN effects s ON s.id = ob.stat_id
             JOIN bonus_types bt ON bt.id = ob.bonus_type_id
             WHERE ob.owner_kind = 'augment' AND s.name = 'Illusion Save'
               AND a.name IN ('Illusion Resistance', 'Insightful Illusion Resistance')
             ORDER BY a.name",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        rows,
        [
            ("Illusion Resistance".into(), "Resistance".into(), 11),
            ("Insightful Illusion Resistance".into(), "Insight".into(), 11)
        ]
    );
    let visor_type: String = db
        .query_row(
            "SELECT bt.name FROM owner_bonuses ob JOIN items i ON i.id = ob.owner_id
             JOIN effects s ON s.id = ob.stat_id JOIN bonus_types bt ON bt.id = ob.bonus_type_id
             WHERE ob.owner_kind = 'item' AND i.name = 'Visor of Fraz-Urb''luu' AND s.name = 'Illusion Save'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(visor_type, "Resistance");
}

#[test]
fn acid_damage_correction_uses_the_real_buff_and_item_writer() {
    let data_files_dir = data_files_with_untyped("acid-damage", &[]);
    let buffs_file = data_files_dir.join("ItemBuffs.xml");
    let buffs_source = std::fs::read_to_string(&buffs_file).unwrap();
    let acid_buff = r#"  <Buff>
    <Type>AcidII</Type><ApplyToWeaponOnly/>
    <DisplayText>Acid II: On Hit: 2 to 8 Acid Damage.</DisplayText>
    <Effect><Type>WeaponOtherDamageBonus</Type><Type>WeaponOtherDamageBonusCritical</Type>
      <Bonus>Not Set</Bonus><AType>NotNeeded</AType><Dice><Number size="1">2</Number>
      <Sides size="1">4</Sides><Damage>Acid</Damage></Dice><Item>All</Item></Effect>
  </Buff>
"#;
    std::fs::write(&buffs_file, buffs_source.replace("</Buffs>", &format!("{acid_buff}</Buffs>"))).unwrap();
    std::fs::write(
        data_files_dir.join("Items/Stoneworker's Hammer.item"),
        r#"<Items><Item><Name>Stoneworker's Hammer</Name><Icon>LightHammer_1j</Icon>
        <Description/><DropLocation>Advance to level 15, End reward</DropLocation><MinLevel>15</MinLevel>
        <EquipmentSlot><Weapon1/><Weapon2/></EquipmentSlot><Weapon>Light Hammer</Weapon>
        <Buff><Type>AcidII</Type><BonusType>Enhancement</BonusType></Buff></Item></Items>"#,
    )
    .unwrap();
    let correction_file = fixture::correction_between(
        "effect_damage",
        "Acid II",
        "add",
        CorrectionValue::Null,
        CorrectionValue::Damage(DamageRow {
            trigger: "On Hit".into(),
            damage_type: "Acid".into(),
            dice_number: 1,
            dice_sides: 7,
            dice_bonus: 1,
            amount_from: 0,
            scale: 1.0,
            sort_order: 0,
        }),
    )
    .cited(
        "Stoneworker's Hammer deals 2 to 8 Acid damage on hit.",
        "https://ddowiki.com/page/Item:Stoneworker%27s_Hammer",
        "2026-10-04",
    );
    let (db, report) = built_db_from(&data_files_dir, &[("damage.toml", &correction_file)]).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (1, 0));
    let damage: (String, String, i64, i64, i64, i64, f64) = db
        .query_row(
            "SELECT t.name, dt.name, ed.dice_number, ed.dice_sides, ed.dice_bonus, ed.amount_from, ed.scale
             FROM effect_damage ed JOIN effects e ON e.id = ed.effect_id JOIN triggers t ON t.id = ed.trigger_id
             JOIN damage_types dt ON dt.id = ed.damage_type_id
             JOIN item_effects ie ON ie.effect_id = e.id JOIN items i ON i.id = ie.item_id
             WHERE i.name = 'Stoneworker''s Hammer' AND e.name = 'Acid II'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
        )
        .unwrap();
    assert_eq!(damage, ("On Hit".into(), "Acid".into(), 1, 7, 1, 0, 1.0));
}

#[test]
fn a_set_tier_correction_shares_a_family_with_the_same_effect_on_another_set() {
    let data_files_dir = data_files_with_untyped("shared-set-family", &[]);
    let set_file = data_files_dir.join("SetBonuses.xml");
    let source = std::fs::read_to_string(&set_file).unwrap();
    let added_sets = include_str!("fixtures/effect-groups/ShamansFury.xml");
    std::fs::write(&set_file, source.replace("</SetBonuses>", &format!("{added_sets}</SetBonuses>"))).unwrap();
    let corrections = new_qualified_correction(
        "set_tier_bonus",
        "Shaman's Fury",
        "equipped_count = 2\nstat = \"Elemental Resonance\"\nbonus_type = \"Equipment\"",
        "bonus_type",
        "\"Equipment\"",
        "\"Artifact\"",
    ) + &new_qualified_correction(
        "set_tier_bonus",
        "Shaman's Fury",
        "equipped_count = 2\nstat = \"Elemental Resonance\"\nbonus_type = \"Artifact\"",
        "value",
        "55",
        "10",
    ) + &new_qualified_correction(
        "set_tier_bonus",
        "Shaman's Fury",
        "equipped_count = 2",
        "add",
        "\"null\"",
        "{ stat = \"Fire, Cold, Electric and Acid Spell Lore\", bonus_type = \"Artifact\", value = 2 }",
    );
    let build = |corrections: &CorrectionSet| {
        let mut db = Connection::open_in_memory().unwrap();
        let report = build_database(
            fixture::grouping_cases(),
            &data_files_dir,
            &WikiOverrides::default(),
            &validated_corrections(&[("corrections.toml", corrections)]).unwrap(),
            &mut db,
            &fixture_dataset_version(),
        )?;
        Ok::<_, anyhow::Error>((db, report))
    };
    let (db, report) = build(&corrections).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (3, 0));
    let member_correction = new_qualified_correction(
        "set_tier_bonus",
        "Shaman's Fury",
        "equipped_count = 2\nstat = \"Fire Spell Power\"\nbonus_type = \"Equipment\"",
        "value",
        "55",
        "10",
    );
    let error = format!("{:#}", build(&member_correction).unwrap_err());
    assert!(error.contains("Elemental Resonance") && error.contains("grouping"), "{error}");
    let mut first_family_id = None;
    for (set_name, expected_value) in [("Shaman's Fury", 10), ("Epic Shaman's Fury", 20)] {
        let rows = set_tier_bonus_rows(&db, set_name, 2);
        assert_eq!(rows.len(), 8, "{set_name}: {rows:?}");
        assert!(
            rows.iter().all(|(stat, bonus_type, value)| bonus_type == "Artifact"
                && *value
                    == if stat.ends_with("Spell Lore") {
                        if set_name == "Shaman's Fury" {
                            2
                        } else {
                            4
                        }
                    } else {
                        expected_value
                    }),
            "{set_name}: {rows:?}"
        );
        let family: (i64, String, String, Option<i64>) = db
            .query_row(
                "SELECT e.id, e.name, e.verbose_name_template, te.value FROM set_bonus_tiers t JOIN set_bonuses s ON s.id = t.set_id
             JOIN set_bonus_tier_effects te ON te.tier_id = t.id JOIN effects e ON e.id = te.effect_id
             WHERE s.name = ?1 AND t.equipped_count = 2 AND e.name = 'Elemental Resonance'",
                [set_name],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(family.1, "Elemental Resonance", "{family:?}");
        assert_eq!(family.2, "%b1 Elemental Resonance +{1}", "{family:?}");
        assert_eq!(family.3, Some(expected_value));
        if let Some(first_id) = first_family_id {
            assert_eq!(family.0, first_id);
        } else {
            first_family_id = Some(family.0);
        }
    }
    let changed = std::fs::read_to_string(&set_file)
        .unwrap()
        .replace("<Amount size=\"1\">55</Amount>", "<Amount size=\"1\">56</Amount>");
    std::fs::write(&set_file, changed).unwrap();
    let (_, report) = build(&corrections).unwrap();
    assert_eq!((report.correction_applied_count, report.correction_stale_count), (2, 1));
}

#[test]
fn types_an_item_bonus_his_files_leave_untyped_and_retypes_a_typed_one() {
    let data_files_dir = data_files_with_untyped("item-bonus-type", &[UNTYPED_ITEMS]);
    let (db, report) = built_db_from(
        &data_files_dir,
        &[(
            "corrections.toml",
            &(new_qualified_correction(
                "item_bonus",
                "Embrace of the Spider Queen",
                "stat = \"Fortification\"\nbonus_type = \"null\"\nbonus_value = 100",
                "bonus_type",
                "\"null\"",
                "\"Enhancement\"",
            ) + &new_qualified_correction(
                "item_bonus",
                "Embrace of the Spider Queen",
                "stat = \"Fortification\"\nbonus_type = \"null\"\nbonus_value = 10",
                "bonus_type",
                "\"null\"",
                "\"Insight\"",
            ) + &new_qualified_correction(
                "item_bonus",
                "Docent of Defiance",
                "stat = \"Fire Resistance\"\nbonus_type = \"Enhancement\"",
                "bonus_type",
                "\"Enhancement\"",
                "\"Competence\"",
            ) + &new_qualified_correction(
                "item_bonus",
                "Docent of Defiance",
                "stat = \"Acid Resistance\"\nbonus_type = \"null\"",
                "bonus_type",
                "\"null\"",
                "\"Enhancement\"",
            )),
        )],
    )
    .unwrap();
    assert_eq!(
        (report.correction_applied_count, report.correction_stale_count),
        (3, 1),
        "{:?}",
        report.stale_corrections
    );
    assert_eq!(
        item_bonus_rows(&db, "Embrace of the Spider Queen"),
        [
            ("Fortification".into(), Some("Enhancement".into()), Some(100)),
            ("Fortification".into(), Some("Insight".into()), Some(10)),
        ]
    );
    assert_eq!(
        item_bonus_rows(&db, "Docent of Defiance")[0],
        ("Fire Resistance".into(), Some("Competence".into()), Some(20))
    );
    let qualifiers: Vec<String> = db
        .prepare("SELECT qualifier FROM corrections ORDER BY qualifier")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        qualifiers,
        ["Fire Resistance / Enhancement", "Fortification / null / 10", "Fortification / null / 100"]
    );
    let error = validated_corrections(&[(
        "corrections.toml",
        &new_qualified_correction(
            "item_bonus",
            "Embrace of the Spider Queen",
            "stat = \"Fortification\"\nbonus_type = \"null\"",
            "bonus_type",
            "\"Enhancement\"",
            "\"Insight\"",
        ),
    )])
    .unwrap_err();
    assert!(error.contains("from must be the bonus_type"), "{error}");
}

#[test]
fn corrects_an_item_bonus_value_without_touching_the_shared_bonus() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &new_qualified_correction(
            "item_bonus",
            "Docent of Defiance",
            "stat = \"Cold Resistance\"\nbonus_type = \"Enhancement\"",
            "value",
            "20",
            "25",
        ),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 1, "{:?}", report.stale_corrections);
    let docent_bonuses = item_bonus_rows(&db, "Docent of Defiance");
    assert!(
        docent_bonuses.contains(&("Cold Resistance".into(), Some("Enhancement".into()), Some(25))),
        "{docent_bonuses:?}"
    );
    assert!(
        docent_bonuses.contains(&("Fire Resistance".into(), Some("Enhancement".into()), Some(20))),
        "{docent_bonuses:?}"
    );
}

#[test]
fn refuses_to_write_an_item_bonus_without_a_type() {
    let data_files_dir = data_files_with_untyped("untyped-item", &[UNTYPED_ITEMS]);
    let error = built_db_from(&data_files_dir, &[]).unwrap_err();
    for expected_text in
        ["item \"Embrace of the Spider Queen\"", "buff \"Fortification\"", "bonus target \"Fortification\""]
    {
        assert!(error.contains(expected_text), "{expected_text} missing from {error}");
    }
}

#[test]
fn refuses_to_write_an_augment_bonus_without_a_type_unless_a_correction_types_it() {
    let data_files_dir = data_files_with_untyped("untyped-augment", &[UNTYPED_AUGMENTS]);
    let error = built_db_from(&data_files_dir, &[]).unwrap_err();
    for expected_text in
        ["augment \"Dolorous Invigorator (Heroic)\"", "effect \"TacticalDC\"", "bonus target \"Trip DC\""]
    {
        assert!(error.contains(expected_text), "{expected_text} missing from {error}");
    }
    let profane_corrections: CorrectionSet = ["Trip DC", "Sunder DC", "Stun DC", "Assassinate DC"]
        .iter()
        .map(|stat_name| {
            new_qualified_correction(
                "augment_bonus",
                "Dolorous Invigorator (Heroic)",
                &format!("stat = {stat_name:?}\nbonus_type = \"null\""),
                "bonus_type",
                "\"null\"",
                "\"Profane\"",
            )
        })
        .collect();
    let (db, report) = built_db_from(&data_files_dir, &[("corrections.toml", &profane_corrections)]).unwrap();
    assert_eq!(
        (report.correction_applied_count, report.correction_stale_count),
        (4, 0),
        "{:?}",
        report.stale_corrections
    );
    let dolorous_bonuses = augment_bonus_rows(&db, "Dolorous Invigorator (Heroic)");
    for stat_name in ["Trip DC", "Sunder DC", "Stun DC", "Assassinate DC"] {
        assert!(
            dolorous_bonuses.contains(&(stat_name.into(), Some("Profane".into()), Some(1))),
            "{dolorous_bonuses:?}"
        );
    }
    assert!(dolorous_bonuses.contains(&("Spell DCs".into(), Some("Profane".into()), Some(1))), "{dolorous_bonuses:?}");
    assert_eq!(dolorous_bonuses.len(), 5);
    let lines: Vec<String> = db
        .prepare(
            "SELECT e.name FROM augment_effects ae JOIN augments a ON a.id = ae.augment_id
             JOIN effects e ON e.id = ae.effect_id WHERE a.name = 'Dolorous Invigorator (Heroic)'
             ORDER BY ae.sort_order",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(lines, ["Spell Focus Mastery", "Trip, Sunder, Stun and Assassinate DC"]);
    assert_eq!(row_count(&db, "SELECT COUNT(*) FROM corrections WHERE kind = 'augment_bonus'"), 4);
}

#[test]
fn removing_an_item_removes_its_augment_slot_options_and_their_modifiers() {
    let (db, report) = built_db_with(&[(
        "corrections.toml",
        &new_correction("item", "+3 Combustion Scorched Battle Axe", "remove", "0", "1"),
    )])
    .unwrap();
    assert_eq!(report.correction_applied_count, 1, "{:?}", report.stale_corrections);
    assert_eq!(
        row_count(
            &db,
            "SELECT COUNT(*) FROM modifiers m WHERE m.source_kind = 'item_augment_slot_option'
              AND NOT EXISTS (SELECT 1 FROM item_augment_slot_options o WHERE o.id = m.source_id)"
        ),
        0
    );
}

#[test]
fn merging_a_socket_label_moves_the_sockets_options_grant() {
    let (db, report) =
        built_db_with(&[("corrections.toml", &new_correction("socket_label", "red", "name", "\"red\"", "\"purple\""))])
            .unwrap();
    assert_eq!(report.correction_applied_count, 1, "{:?}", report.stale_corrections);
    let granted_labels: Vec<String> = db
        .prepare(
            "SELECT t.label FROM item_augment_slot_option_grants g JOIN augment_slot_types t ON t.id = g.slot_id
               JOIN item_augment_slot_options o ON o.id = g.option_id JOIN items i ON i.id = o.item_id
              WHERE i.name = 'Sireth, Spear of the Sky'",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(granted_labels, ["purple"]);
}

#[test]
fn refuses_an_untyped_augment_slot_option_bonus_unless_an_item_bonus_correction_types_it() {
    let data_files_dir = data_files_with_untyped("untyped-slot-option", &[UNTYPED_ITEM_AUGMENT_SLOT_OPTIONS]);
    let error = built_db_from(&data_files_dir, &[]).unwrap_err();
    for expected_text in [
        "augment slot option of item \"Epic Bracers of Wind\"",
        "effect \"SpellLore\"",
        "bonus target \"Electric Spell Lore\"",
        "item_bonus bonus_type correction",
    ] {
        assert!(error.contains(expected_text), "{expected_text} missing from {error}");
    }
    let equipment_corrections: CorrectionSet = [17, 18]
        .iter()
        .map(|bonus_value| {
            new_qualified_correction(
                "item_bonus",
                "Epic Bracers of Wind",
                &format!("stat = \"Electric Spell Lore\"\nbonus_type = \"null\"\nbonus_value = {bonus_value}"),
                "bonus_type",
                "\"null\"",
                "\"Equipment\"",
            )
        })
        .collect();
    let (db, report) = built_db_from(&data_files_dir, &[("corrections.toml", &equipment_corrections)]).unwrap();
    assert_eq!(
        (report.correction_applied_count, report.correction_stale_count),
        (2, 0),
        "{:?}",
        report.stale_corrections
    );
    let lore_bonuses: Vec<(String, i64)> = db
        .prepare(
            "SELECT bt.name, ob.amount
               FROM owner_bonuses ob JOIN effects s ON s.id = ob.stat_id
               JOIN bonus_types bt ON bt.id = ob.bonus_type_id
               JOIN item_augment_slot_options o ON o.id = ob.owner_id JOIN items i ON i.id = o.item_id
              WHERE ob.owner_kind = 'item_augment_slot_option' AND i.name = 'Epic Bracers of Wind'
                AND s.name = 'Electric Spell Lore' ORDER BY 2",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(lore_bonuses, [("Equipment".to_string(), 17), ("Equipment".to_string(), 18)]);
}
