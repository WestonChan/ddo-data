use anyhow::{ensure, Context, Result};
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn toml_files(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| match entry {
            Ok(entry)
                if entry.file_type().is_file()
                    && entry.path().extension().is_some_and(|extension| extension == "toml") =>
            {
                Some(Ok(entry))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .map(|entry| {
            let entry = entry?;
            Ok((entry.path().strip_prefix(root)?.to_path_buf(), std::fs::read(entry.path())?))
        })
        .collect()
}

fn copy_data_images(source_dir: &Path, destination_dir: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(source_dir) {
        let entry = entry?;
        if !entry.file_type().is_file()
            || !entry.path().extension().is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
        {
            continue;
        }
        let target = destination_dir.join(entry.path().strip_prefix(source_dir)?);
        std::fs::create_dir_all(target.parent().context("icon has no parent folder")?)?;
        std::fs::copy(entry.path(), target)?;
    }
    Ok(())
}

pub fn verify_round_trip(data_dir: &Path) -> Result<()> {
    let start = Instant::now();
    let scratch = tempfile::tempdir()?;
    let first_db_path = scratch.path().join("first.db");
    let second_db_path = scratch.path().join("second.db");
    let exported_files = scratch.path().join("data");
    let mut first_db = Connection::open(&first_db_path)?;
    ddo_etl::tree::build_from_data(data_dir, &mut first_db)?;
    let first_build_time = start.elapsed();
    ddo_etl::tree::export_database(&first_db, &exported_files)?;
    let first_files = toml_files(data_dir)?;
    let second_files = toml_files(&exported_files)?;
    for file in first_files.keys().chain(second_files.keys()) {
        ensure!(first_files.get(file) == second_files.get(file), "data round trip changed {}", file.display());
    }
    let export_time = start.elapsed() - first_build_time;
    copy_data_images(data_dir, &exported_files)?;
    let mut second_db = Connection::open(&second_db_path)?;
    ddo_etl::tree::build_from_data(&exported_files, &mut second_db)?;
    drop(first_db);
    drop(second_db);
    let second_build_time = start.elapsed() - first_build_time - export_time;
    let report = crate::diff_db::diff_databases_by_slug(&first_db_path, &second_db_path)?;
    ensure!(report.is_equal(), "{report}");
    println!(
        "round trip: {} TOML files byte-equal; PNG files copied separately; {report}; first build {:.2}s, export {:.2}s, second build {:.2}s, comparison {:.2}s",
        first_files.len(),
        first_build_time.as_secs_f64(),
        export_time.as_secs_f64(),
        second_build_time.as_secs_f64(),
        (start.elapsed() - first_build_time - export_time - second_build_time).as_secs_f64()
    );
    Ok(())
}
