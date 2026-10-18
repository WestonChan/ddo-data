use ddo_etl::upstream::data_files_dir_for;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn data_files_dir_comes_from_ddo_upstream_when_set() {
    let checkout_root = Path::new("/work/ddo-data");
    assert_eq!(
        data_files_dir_for(Some(OsString::from("/shared/upstream")), checkout_root),
        Path::new("/shared/upstream/Output/DataFiles")
    );
    assert_eq!(data_files_dir_for(None, checkout_root), Path::new("/work/ddo-data/upstream/Output/DataFiles"));
    assert_eq!(
        data_files_dir_for(Some(OsString::new()), checkout_root),
        Path::new("/work/ddo-data/upstream/Output/DataFiles")
    );
}

#[test]
fn build_and_icons_default_to_the_data_tree_without_upstream() {
    let work_dir = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(fixtures_dir().join("data"), work_dir.path().join("data")).unwrap();
    let db_path = work_dir.path().join("ddo.db");
    let build_output = Command::new(env!("CARGO_BIN_EXE_ddo-etl"))
        .args(["build", "--out"])
        .arg(&db_path)
        .current_dir(work_dir.path())
        .env("DDO_UPSTREAM", work_dir.path().join("absent-upstream"))
        .output()
        .unwrap();
    assert!(build_output.status.success(), "{}", String::from_utf8_lossy(&build_output.stderr));
    let db = rusqlite::Connection::open(&db_path).unwrap();
    let item_count: i64 = db.query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0)).unwrap();
    let mut expected = rusqlite::Connection::open_in_memory().unwrap();
    ddo_etl::tree::build_from_data(&fixtures_dir().join("data"), &mut expected).unwrap();
    let expected_count: i64 = expected.query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0)).unwrap();
    assert_eq!(item_count, expected_count);

    let icons_output = Command::new(env!("CARGO_BIN_EXE_ddo-etl"))
        .args(["icons", "--out"])
        .arg(work_dir.path().join("icons"))
        .current_dir(work_dir.path())
        .env("DDO_UPSTREAM", work_dir.path().join("absent-upstream"))
        .output()
        .unwrap();
    assert!(icons_output.status.success(), "{}", String::from_utf8_lossy(&icons_output.stderr));
    assert_eq!(
        std::fs::read(work_dir.path().join("icons/items/Quarterstaff_6a.png")).unwrap(),
        std::fs::read(fixtures_dir().join("data/icons/items/Quarterstaff_6a.png")).unwrap()
    );
}

#[test]
fn served_dataset_commands_expose_only_canonical_inputs() {
    for (command, absent) in [
        ("build", vec!["--source", "--wiki", "--corrections", "--sha"]),
        ("export", vec!["--source"]),
        ("icons", vec!["--source"]),
        ("import-ddobuilder", vec!["--wiki", "--corrections"]),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_ddo-etl")).args([command, "--help"]).output().unwrap();
        assert!(output.status.success());
        let help = String::from_utf8(output.stdout).unwrap();
        for flag in absent {
            assert!(!help.contains(flag), "{command} still accepts {flag}: {help}");
        }
    }
}
