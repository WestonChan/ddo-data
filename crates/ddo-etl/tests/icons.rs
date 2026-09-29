use ddo_etl::icons::export_icons;
use std::path::PathBuf;

#[test]
fn flattens_item_images_and_copies_the_rest() {
    let data_files_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles");
    let icons_dir = std::env::temp_dir().join(format!("ddo-icons-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&icons_dir);
    let report = export_icons(&data_files_dir, &icons_dir).unwrap();
    assert_eq!(report.copied_count_by_family.get("items"), Some(&2), "{report:?}");
    assert_eq!(report.copied_count_by_family.get("feats"), Some(&1));
    assert_eq!(report.copied_count_by_family.get("augments"), Some(&1));
    assert!(icons_dir.join("items/Quarterstaff_6a.png").is_file(), "nested item images are flattened to their stem");
    assert!(icons_dir.join("items/Docent_1a.png").is_file());
    assert!(icons_dir.join("feats/PowerAttack.png").is_file());
    assert!(!icons_dir.join("enhancements").exists(), "families absent from the source are skipped");
    let second_report = export_icons(&data_files_dir, &icons_dir).unwrap();
    assert_eq!(second_report.copied_count_by_family.get("items"), Some(&0), "re-running copies nothing new");
    let _ = std::fs::remove_dir_all(&icons_dir);
}
