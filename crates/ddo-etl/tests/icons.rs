use ddo_etl::icons::export_icons;
use std::path::PathBuf;

#[test]
fn flattens_item_images_and_copies_the_rest() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataFiles");
    let out = std::env::temp_dir().join(format!("ddo-icons-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let report = export_icons(&source, &out).unwrap();
    assert_eq!(report.copied.get("items"), Some(&2), "{report:?}");
    assert_eq!(report.copied.get("feats"), Some(&1));
    assert_eq!(report.copied.get("augments"), Some(&1));
    assert!(out.join("items/Quarterstaff_6a.png").is_file(), "nested item images are flattened to their stem");
    assert!(out.join("items/Docent_1a.png").is_file());
    assert!(out.join("feats/PowerAttack.png").is_file());
    assert!(!out.join("enhancements").exists(), "families absent from the source are skipped");
    let again = export_icons(&source, &out).unwrap();
    assert_eq!(again.copied.get("items"), Some(&0), "re-running copies nothing new");
    let _ = std::fs::remove_dir_all(&out);
}
