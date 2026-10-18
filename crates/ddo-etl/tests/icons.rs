use ddo_etl::icons::export_icons;
use std::path::PathBuf;

#[test]
fn flattens_item_images_and_copies_the_rest() {
    let data_files_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles");
    let icons_dir = std::env::temp_dir().join(format!("ddo-icons-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&icons_dir);
    let report = export_icons(&data_files_dir, &icons_dir).unwrap();
    assert_eq!(report.copied_count_by_kind.get("items"), Some(&2), "{report:?}");
    assert_eq!(report.copied_count_by_kind.get("feats"), Some(&1));
    assert_eq!(report.copied_count_by_kind.get("augments"), Some(&1));
    assert!(icons_dir.join("items/Quarterstaff_6a.png").is_file(), "nested item images are flattened to their stem");
    assert!(icons_dir.join("items/Docent_1a.png").is_file());
    assert!(icons_dir.join("feats/PowerAttack.png").is_file());
    assert!(!icons_dir.join("enhancements").exists(), "kinds absent from the source are skipped");
    let second_report = export_icons(&data_files_dir, &icons_dir).unwrap();
    assert_eq!(second_report.copied_count_by_kind.get("items"), Some(&0), "re-running copies nothing new");
    let _ = std::fs::remove_dir_all(&icons_dir);
}

#[test]
fn export_cli_fails_without_deleting_unowned_images() {
    let root = std::env::temp_dir().join(format!("ddo-export-hard-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let database = root.join("fixture.db");
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut db = rusqlite::Connection::open(&database).unwrap();
    ddo_etl::tree::build_from_data(&fixtures.join("data"), &mut db).unwrap();
    drop(db);
    let tree = root.join("tree");
    let stale = tree.join("characters/enhancements/obsolete/icons/Unowned.png");
    std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
    std::fs::write(&stale, b"unowned").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ddo-etl"))
        .arg("export")
        .arg("--db")
        .arg(&database)
        .arg("--out")
        .arg(&tree)
        .output()
        .unwrap();
    assert!(!output.status.success(), "HARD export must fail");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("HARD") && stdout.contains("Unowned.png"), "{stdout}");
    assert_eq!(std::fs::read(&stale).unwrap(), b"unowned");
    std::fs::remove_file(stale).unwrap();
    assert!(std::process::Command::new(env!("CARGO_BIN_EXE_ddo-etl"))
        .arg("export")
        .arg("--db")
        .arg(&database)
        .arg("--out")
        .arg(&tree)
        .output()
        .unwrap()
        .status
        .success());
    std::fs::remove_dir_all(root).unwrap();
}
