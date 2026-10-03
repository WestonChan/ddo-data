use ddo_etl::build::build_database;
use ddo_etl::corrections::Corrections;
use ddo_etl::wiki::WikiOverrides;
use ddo_model::DatasetVersion;
use rusqlite::types::ValueRef;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

const TABLE_ROWS: &[(&str, &str)] = &[
    (
        "bonuses",
        "SELECT s.name, bt.name, b.value, b.value2, b.name, b.description FROM bonuses b \
         JOIN stats s ON s.id = b.stat_id JOIN bonus_types bt ON bt.id = b.bonus_type_id",
    ),
    (
        "item_bonuses",
        "SELECT i.name, s.name, bt.name, b.value, b.value2, b.name, b.description, ib.sort_order \
         FROM item_bonuses ib JOIN items i ON i.id = ib.item_id JOIN bonuses b ON b.id = ib.bonus_id \
         JOIN stats s ON s.id = b.stat_id JOIN bonus_types bt ON bt.id = b.bonus_type_id",
    ),
    ("effects", "SELECT name, description FROM effects"),
    (
        "item_effects",
        "SELECT i.name, e.name, ie.sort_order, ie.value, ie.target FROM item_effects ie \
         JOIN items i ON i.id = ie.item_id JOIN effects e ON e.id = ie.effect_id",
    ),
    (
        "modifiers",
        "SELECT source_kind, source_id, sort_order, effect_type, extra_types, bonus, bonus_type_id, \
         amount_type, amounts, targets, value, dice_number, dice_sides, dice_bonus, dice_damage, damage, \
         percent, rank, cap, stack_source, display_name, apply_as_item_effect, is_item_specific, is_rare \
         FROM modifiers",
    ),
];

fn fixture_database() -> Connection {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut db = Connection::open_in_memory().unwrap();
    build_database(
        &fixture.join("DataFiles"),
        &WikiOverrides::from_dir(&fixture.join("wiki")).unwrap(),
        &Corrections::default(),
        &mut db,
        &DatasetVersion { upstream_sha: "test".into(), built_at: "2026-10-03T00:00:00Z".into() },
    )
    .unwrap();
    db
}

fn table_rows(db: &Connection, sql: &str) -> Vec<Vec<Value>> {
    let mut statement = db.prepare(sql).unwrap();
    let column_count = statement.column_count();
    statement
        .query_map([], |row| {
            (0..column_count)
                .map(|column| {
                    let cell = match row.get_ref(column)? {
                        ValueRef::Null => Value::Null,
                        ValueRef::Integer(number) => json!(number),
                        ValueRef::Real(number) => json!(number),
                        ValueRef::Text(text) => json!(String::from_utf8_lossy(text).as_ref()),
                        ValueRef::Blob(_) => panic!("the compared tables carry no blobs"),
                    };
                    Ok(cell)
                })
                .collect()
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn row_counts(rows: &[Vec<Value>]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for row in rows {
        *counts.entry(serde_json::to_string(row).unwrap()).or_default() += 1;
    }
    counts
}

fn difference(left: &[Vec<Value>], right: &[Vec<Value>]) -> Vec<Vec<Value>> {
    let left_counts = row_counts(left);
    let right_counts = row_counts(right);
    let mut different_rows = Vec::new();
    for (row, count) in left_counts {
        let unmatched_count = count.saturating_sub(right_counts.get(&row).copied().unwrap_or_default());
        different_rows.extend((0..unmatched_count).map(|_| serde_json::from_str(&row).unwrap()));
    }
    different_rows
}

#[test]
fn fixture_tables_change_only_for_named_fallback_bonuses_and_distinct_descriptions() {
    let before: BTreeMap<String, Vec<Vec<Value>>> =
        serde_json::from_str(include_str!("fixtures/enchantment_baseline.json")).unwrap();
    let after = fixture_database();
    let differences: BTreeMap<_, _> = TABLE_ROWS
        .iter()
        .map(|(table, sql)| {
            let previous_rows = &before[*table];
            let written_rows = table_rows(&after, sql);
            (*table, (difference(previous_rows, &written_rows), difference(&written_rows, previous_rows)))
        })
        .collect();
    assert_eq!(differences["effects"], (vec![vec![json!("Improved Deception"), Value::Null]], vec![]));
    assert_eq!(
        differences["item_effects"],
        (
            vec![vec![
                json!("Backstabber's Gloves (Level 25)"),
                json!("Improved Deception"),
                json!(3),
                json!(5),
                Value::Null
            ]],
            vec![]
        )
    );
    assert!(differences["modifiers"].0.is_empty());
    assert!(differences["modifiers"].1.is_empty());
    let (removed_bonuses, added_bonuses) = &differences["bonuses"];
    assert!(removed_bonuses.is_empty());
    assert_eq!(added_bonuses.len(), 7);
    let added_bonus_signatures: Vec<_> = added_bonuses
        .iter()
        .map(|row| {
            (
                row[0].clone(),
                row[1].clone(),
                row[2].clone(),
                row[5].as_str().map(|description| description.split(':').next().unwrap().to_string()),
            )
        })
        .collect();
    for expected in [
        (json!("Armor Class"), json!("Insight"), json!(5), Some("Insight Parrying ".to_string())),
        (json!("Bluff"), json!("Competence"), json!(20), None),
        (json!("Bluff"), json!("Enhancement"), json!(3), Some("Deception".to_string())),
        (json!("Bluff"), json!("Enhancement"), json!(3), Some("Enhancement Improved Deception 3".to_string())),
        (json!("Bluff"), json!("Enhancement"), json!(5), Some("Enhancement Improved Deception 5".to_string())),
        (json!("Diplomacy"), json!("Competence"), json!(20), None),
        (json!("Saving Throws"), json!("Insight"), json!(5), Some("Insight Parrying ".to_string())),
    ] {
        assert!(added_bonus_signatures.contains(&expected), "{expected:?}");
    }
    let (removed_item_bonuses, added_item_bonuses) = &differences["item_bonuses"];
    assert!(removed_item_bonuses.is_empty());
    let added_item_signatures: Vec<_> = added_item_bonuses
        .iter()
        .map(|row| (row[0].clone(), row[1].clone(), row[2].clone(), row[3].clone(), row[7].clone()))
        .collect();
    assert_eq!(
        added_item_signatures,
        [
            (json!("Acrobat's Ring"), json!("Bluff"), json!("Enhancement"), json!(3), json!(3)),
            (json!("Alaric's Grim Gauntlets"), json!("Armor Class"), json!("Insight"), json!(5), json!(5)),
            (json!("Alaric's Grim Gauntlets"), json!("Bluff"), json!("Enhancement"), json!(3), json!(7)),
            (json!("Alaric's Grim Gauntlets"), json!("Saving Throws"), json!("Insight"), json!(5), json!(6)),
            (json!("Backstabber's Gloves (Level 25)"), json!("Bluff"), json!("Enhancement"), json!(5), json!(3)),
        ]
    );
}
