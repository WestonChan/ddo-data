use anyhow::{Context, Result};
use ddo_etl::build::BuildReport;
use ddo_etl::corrections::Corrections;
use ddo_etl::wiki::WikiOverrides;
use rusqlite::Connection;
use std::path::Path;

pub fn wiki_overrides_from(wiki_dir: Option<&Path>) -> Result<WikiOverrides> {
    match wiki_dir {
        Some(dir) => WikiOverrides::from_dir(dir),
        None => WikiOverrides::embedded(),
    }
}

pub fn corrections_from(corrections_dir: Option<&Path>) -> Result<Corrections> {
    match corrections_dir {
        Some(dir) => Corrections::from_dir(dir),
        None => Corrections::embedded(),
    }
}

pub fn build_database_file(
    data_files_dir: &Path,
    wiki_overrides: &WikiOverrides,
    corrections: &Corrections,
    db_path: &Path,
) -> Result<BuildReport> {
    let mut db = Connection::open(db_path).with_context(|| format!("opening {}", db_path.display()))?;
    build_into(&mut db, data_files_dir, wiki_overrides, corrections)
}

fn build_into(
    db: &mut Connection,
    data_files_dir: &Path,
    wiki_overrides: &WikiOverrides,
    corrections: &Corrections,
) -> Result<BuildReport> {
    let dataset_version = ddo_etl::upstream::dataset_version(data_files_dir, None);
    ddo_etl::build::build_database(data_files_dir, wiki_overrides, corrections, db, &dataset_version)
        .with_context(|| format!("building the database from {}", data_files_dir.display()))
}

pub fn build_in_memory_database(
    data_files_dir: &Path,
    wiki_overrides: &WikiOverrides,
    corrections: &Corrections,
) -> Result<(Connection, BuildReport)> {
    let mut db = Connection::open_in_memory()?;
    let report = build_into(&mut db, data_files_dir, wiki_overrides, corrections)?;
    Ok((db, report))
}
