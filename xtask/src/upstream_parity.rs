use anyhow::Result;
use ddo_etl::delta::IdentityIndex;
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::Path;

const EFFECT_TABLES: [&str; 7] = [
    "effects",
    "effect_bonuses",
    "item_effects",
    "augment_effects",
    "set_bonus_tier_effects",
    "feat_effects",
    "item_augment_slot_option_effects",
];

pub fn upstream_differences(upstream: &Connection, ours: &Connection) -> Result<Vec<String>> {
    let mut upstream_index = IdentityIndex::from_db(upstream)?;
    let mut our_index = IdentityIndex::from_db(ours)?;
    let mut differences = Vec::new();
    for table in EFFECT_TABLES {
        let mut row_counts: BTreeMap<Vec<String>, (usize, usize)> = BTreeMap::new();
        for row in upstream_index.normalized_rows(upstream, table)? {
            row_counts.entry(row).or_default().0 += 1;
        }
        for row in our_index.normalized_rows(ours, table)? {
            row_counts.entry(row).or_default().1 += 1;
        }
        for (row, (upstream_count, our_count)) in row_counts {
            if upstream_count != our_count {
                differences.push(format!("{table}: upstream={upstream_count} ours={our_count} {row:?}"));
            }
        }
    }
    Ok(differences)
}

pub fn upstream_parity(source: &Path, data_dir: &Path, write_db: Option<&Path>) -> Result<Vec<String>> {
    let scratch = tempfile::tempdir()?;
    let mut ours = Connection::open_in_memory()?;
    ddo_etl::tree::build_from_data(data_dir, &mut ours)?;
    let upstream = ddo_etl::import_ddobuilder::normalized_upstream_database(
        source,
        &ours,
        &ddo_etl::upstream::dataset_version(source, None),
        false,
        &scratch.path().join("upstream"),
    )?;
    if let Some(destination) = write_db {
        upstream.db.execute("VACUUM INTO ?1", [destination.to_string_lossy().as_ref()])?;
    }
    for split in upstream.line_splits {
        println!("LINES SPLIT: {split:?}");
    }
    for split in upstream.held_back_effects {
        println!("HELD BACK EFFECT: {split:?}");
    }
    upstream_differences(&upstream.db, &ours)
}
