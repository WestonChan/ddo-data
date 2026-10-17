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
