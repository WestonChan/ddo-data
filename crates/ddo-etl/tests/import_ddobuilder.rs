use ddo_etl::{build, corrections::Corrections, tree, wiki::WikiOverrides};
use ddo_model::DatasetVersion;
use rusqlite::Connection;
use std::path::Path;

#[test]
fn exported_ddobuilder_tree_builds_with_dc_icons_from_other_kinds() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import_ddobuilder/ba7e18e/DataFiles");
    let root = std::env::temp_dir().join(format!("ddo-import-dc-icons-{}", std::process::id()));
    let mut db = Connection::open_in_memory().unwrap();
    build::build_database(
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

struct ImportWorld {
    scratch: tempfile::TempDir,
    options: DdobuilderImportOptions,
}

impl ImportWorld {
    fn new(corrections: Corrections) -> Self {
        let scratch = tempfile::tempdir().unwrap();
        let previous_dir = scratch.path().join("previous");
        let source_dir = scratch.path().join("new");
        let data_dir = scratch.path().join("data");
        copy_tree(&fixture_root().join("ba7e18e/DataFiles"), &previous_dir);
        copy_tree(&previous_dir, &source_dir);
        copy_tree(&fixture_root().join("d1350a0c/DataFiles"), &source_dir);
        let mut db = Connection::open_in_memory().unwrap();
        build::build_database(
            &previous_dir,
            &WikiOverrides::default(),
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
                wiki: WikiOverrides::default(),
                corrections,
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

fn ravager_corrections() -> Corrections {
    Corrections::from_dir(&fixture_root().join("corrections")).unwrap()
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
    world.edit("INSERT INTO corrections (kind, name, qualifier, field, from_value, to_value, reason, source, read) SELECT 'enhancement', e.name, 'selection \"SLA: Vial of Acid\"', 'icon', '\"OldIcon\"', json_quote(s.icon), 'Reviewed icon', 'https://ddowiki.com/page/Bombardier', '2026-10-17' FROM enhancement_selections s JOIN enhancements e ON e.id = s.enhancement_id WHERE s.slug = 'bomspellvialselection-sla-vial-of-acid'");
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
    let mut world = ImportWorld::new(Corrections::default());
    let mut correction = ravager_corrections().entries.remove(0);
    correction.name = "Bombardier: Spellvial Selection II".into();
    correction.from = ddo_etl::corrections::CorrectionValue::Text("Previous prose".into());
    correction.to = ddo_etl::corrections::CorrectionValue::Text("Our prose".into());
    world.options.corrections.entries.push(correction);
    let before = world.bytes();
    let error = format!("{:#}", import_ddobuilder(&world.options).unwrap_err());
    assert!(error.contains("building the new DataFiles") && error.contains("Spellvial Selection II"), "{error}");
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
        .arg(world.options.sha.as_ref().unwrap())
        .arg("--wiki")
        .arg(fixture_root().join("wiki"));
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
fn import_cli_accepts_a_corrections_override() {
    let world = ImportWorld::new(ravager_corrections());
    let override_dir = world.scratch.path().join("corrections");
    std::fs::create_dir(&override_dir).unwrap();
    let original = std::fs::read_to_string(fixture_root().join("corrections/ravager.toml")).unwrap();
    let draconic = original.split("[[correction]]").nth(1).unwrap();
    std::fs::write(override_dir.join("ravager.toml"), format!("[[correction]]{draconic}")).unwrap();
    let output = cli(&world).arg("--corrections").arg(override_dir).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", String::from_utf8_lossy(&output.stderr));
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(report.contains("RESOLVED CORRECTION") && report.contains("Our researched Barbarian"), "{report}");
    assert!(!report.contains("CONFLICT enhancement \"Ravager: Barbarian Power Attack\""), "{report}");
    assert!(report.contains("enhancements/ravbarbarianpowerattack description"), "{report}");
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
fn import_rebases_both_sides_without_mutating_supplied_corrections() {
    let mut corrections = ravager_corrections();
    corrections.entries.retain(|entry| entry.name.contains("Barbarian"));
    let mut world = ImportWorld::new(corrections.clone());
    import_ddobuilder(&world.options).unwrap();
    assert_eq!(world.options.corrections, corrections);
    world.options.previous_dir = world.options.source_dir.clone();
    world.options.previous_sha = world.options.sha.clone();
    let before = world.bytes();
    let report = import_ddobuilder(&world.options).unwrap();
    assert!(!report.has_conflicts(), "{report}");
    assert_eq!(before, world.bytes());
    assert_eq!(world.options.corrections, corrections);
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
