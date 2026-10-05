use ddo_etl::corrections::Corrections;
use ddo_etl::wiki::WikiOverrides;
use rusqlite::Connection;
use std::path::Path;
use xtask::dataset::build_database_file;
use xtask::golden_refresh::{golden_sample, SampleRequest};

#[test]
fn sample_reads_real_writer_links_for_targeted_names_and_effect_patterns() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/ddo-etl/tests/fixtures");
    let work_dir = tempfile::tempdir().unwrap();
    let db_path = work_dir.path().join("fixture.db");
    let wiki = WikiOverrides::from_dir(&fixtures.join("wiki")).unwrap();
    build_database_file(&fixtures.join("DataFiles"), &wiki, &Corrections::default(), &db_path).unwrap();
    let db = Connection::open(&db_path).unwrap();
    let (item_name, effect_name): (String, String) = db
        .query_row(
            "SELECT i.name, e.name FROM items i JOIN item_effects link ON link.item_id = i.id
         JOIN effects e ON e.id = link.effect_id WHERE i.minimum_level >= 29 AND i.is_legacy = 0
         ORDER BY i.name, link.sort_order LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let exact = golden_sample(
        &db_path,
        &SampleRequest {
            count: 1,
            fresh_days: 90,
            items: vec![item_name.clone()],
            augments: Vec::new(),
            sets: Vec::new(),
            effects: Vec::new(),
            effect_likes: Vec::new(),
        },
    )
    .unwrap();
    assert_eq!(exact["items"][0]["item"], item_name);
    assert!(exact["items"][0]["lines"].as_array().unwrap().iter().any(|line| line["effect"] == effect_name));
    let pattern = golden_sample(
        &db_path,
        &SampleRequest {
            count: 200,
            fresh_days: 90,
            items: Vec::new(),
            augments: Vec::new(),
            sets: Vec::new(),
            effects: Vec::new(),
            effect_likes: vec![effect_name],
        },
    )
    .unwrap();
    assert!(pattern["items"].as_array().unwrap().iter().any(|item| item["item"] == item_name));
}
