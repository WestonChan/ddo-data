use ddo_etl::upstream::data_files_dir_for;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fresh_work_dir(test_name: &str) -> PathBuf {
    let work_dir = std::env::temp_dir().join(format!("ddo-etl-{test_name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work_dir);
    std::fs::create_dir_all(&work_dir).unwrap();
    work_dir
}

fn upstream_checkout_linking_fixtures(work_dir: &Path) -> PathBuf {
    let upstream_checkout_dir = work_dir.join("upstream-checkout");
    std::fs::create_dir_all(upstream_checkout_dir.join("Output")).unwrap();
    std::os::unix::fs::symlink(fixtures_dir().join("DataFiles"), upstream_checkout_dir.join("Output/DataFiles"))
        .unwrap();
    upstream_checkout_dir
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
fn build_and_icons_read_ddo_upstream_when_source_is_absent() {
    let work_dir = fresh_work_dir("ddo-upstream");
    let upstream_checkout_dir = upstream_checkout_linking_fixtures(&work_dir);
    let db_path = work_dir.join("ddo.db");

    let build_output = Command::new(env!("CARGO_BIN_EXE_ddo-etl"))
        .args(["build", "--sha", "fixture-sha", "--out"])
        .arg(&db_path)
        .arg("--wiki")
        .arg(fixtures_dir().join("wiki"))
        .arg("--corrections")
        .arg(fixtures_dir().join("corrections"))
        .env("DDO_UPSTREAM", &upstream_checkout_dir)
        .output()
        .unwrap();
    assert!(build_output.status.success(), "{}", String::from_utf8_lossy(&build_output.stderr));
    let item_count: i64 = rusqlite::Connection::open(&db_path)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM items WHERE provenance = 'maetrim'", [], |row| row.get(0))
        .unwrap();
    assert_eq!(item_count, 58);

    let icons_output = Command::new(env!("CARGO_BIN_EXE_ddo-etl"))
        .args(["icons", "--out"])
        .arg(work_dir.join("icons"))
        .env("DDO_UPSTREAM", &upstream_checkout_dir)
        .output()
        .unwrap();
    assert!(icons_output.status.success(), "{}", String::from_utf8_lossy(&icons_output.stderr));
    assert!(work_dir.join("icons/items/Quarterstaff_6a.png").is_file());
    let _ = std::fs::remove_dir_all(&work_dir);
}
