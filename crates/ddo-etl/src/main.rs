use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "ddo-etl", about = "Build the DDO Tools game database from DDOBuilderV2 data files")]
struct Cli {
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand)]
enum CliCommand {
    Build {
        #[arg(long)]
        source: Option<PathBuf>,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        sha: Option<String>,
        #[arg(long)]
        wiki: Option<PathBuf>,
        #[arg(long)]
        corrections: Option<PathBuf>,
    },
    Icons {
        #[arg(long)]
        source: Option<PathBuf>,
        #[arg(long)]
        out: PathBuf,
    },
    Diff {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        legacy: PathBuf,
        #[arg(long)]
        verbose: bool,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        CliCommand::Build {
            source: data_files_dir,
            out: database_path,
            sha: upstream_sha,
            wiki: wiki_dir,
            corrections: corrections_dir,
        } => {
            let data_files_dir = data_files_dir.unwrap_or_else(default_data_files_dir);
            let dataset_version = ddo_etl::upstream::dataset_version(&data_files_dir, upstream_sha);
            if database_path.exists() {
                std::fs::remove_file(&database_path)
                    .with_context(|| format!("removing {}", database_path.display()))?;
            }
            let wiki_overrides = match &wiki_dir {
                Some(dir) => ddo_etl::wiki::WikiOverrides::from_dir(dir)?,
                None => ddo_etl::wiki::WikiOverrides::embedded()?,
            };
            let corrections = match &corrections_dir {
                Some(dir) => ddo_etl::corrections::Corrections::from_dir(dir)?,
                None => ddo_etl::corrections::Corrections::embedded()?,
            };
            let mut db = Connection::open(&database_path)?;
            let report = ddo_etl::build::build_database(
                &data_files_dir,
                &wiki_overrides,
                &corrections,
                &mut db,
                &dataset_version,
            )?;
            println!("{report:#?}");
            println!("dataset {} written to {}", dataset_version.upstream_sha, database_path.display());
        }
        CliCommand::Icons { source: data_files_dir, out: icons_dir } => {
            let data_files_dir = data_files_dir.unwrap_or_else(default_data_files_dir);
            let report = ddo_etl::icons::export_icons(&data_files_dir, &icons_dir)?;
            println!("{report:#?}");
        }
        CliCommand::Diff { db: built_db_path, legacy: legacy_db_path, verbose: lists_every_name } => {
            let built_db = Connection::open(&built_db_path)?;
            let legacy_db = Connection::open(&legacy_db_path)?;
            let coverage = ddo_etl::diff::item_coverage(&built_db, &legacy_db)?;
            println!(
                "matched {}  only in new {}  only in legacy {}  excluded by design {}  coverage {:.1}%",
                coverage.matched_count,
                coverage.names_only_in_built.len(),
                coverage.names_only_in_legacy.len(),
                coverage.names_excluded_by_design.len(),
                coverage.coverage_ratio() * 100.0
            );
            let listed_name_limit = if lists_every_name { usize::MAX } else { 20 };
            println!("\n-- only in legacy (first {}):", coverage.names_only_in_legacy.len().min(listed_name_limit));
            for name in coverage.names_only_in_legacy.iter().take(listed_name_limit) {
                println!("  {name}");
            }
            println!("\n-- only in new (first {}):", coverage.names_only_in_built.len().min(listed_name_limit));
            for name in coverage.names_only_in_built.iter().take(listed_name_limit) {
                println!("  {name}");
            }
        }
    }
    Ok(())
}

fn default_data_files_dir() -> PathBuf {
    ddo_etl::upstream::default_data_files_dir(Path::new(""))
}
