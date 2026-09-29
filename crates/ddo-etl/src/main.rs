use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ddo_model::DatasetVersion;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::process::Command;

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
        source: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        sha: Option<String>,
        #[arg(long)]
        wiki: Option<PathBuf>,
    },
    Icons {
        #[arg(long)]
        source: PathBuf,
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
        CliCommand::Build { source: data_files_dir, out: database_path, sha: upstream_sha, wiki: wiki_dir } => {
            let upstream_sha = upstream_sha
                .unwrap_or_else(|| head_commit_sha(&data_files_dir).unwrap_or_else(|| "unknown".to_string()));
            let dataset_version = DatasetVersion { upstream_sha, built_at: current_utc_timestamp() };
            if database_path.exists() {
                std::fs::remove_file(&database_path)
                    .with_context(|| format!("removing {}", database_path.display()))?;
            }
            let wiki_overrides = match &wiki_dir {
                Some(dir) => ddo_etl::wiki::WikiOverrides::from_dir(dir)?,
                None => ddo_etl::wiki::WikiOverrides::embedded()?,
            };
            let mut db = Connection::open(&database_path)?;
            let report = ddo_etl::build::build_database(&data_files_dir, &wiki_overrides, &mut db, &dataset_version)?;
            println!("{report:#?}");
            println!("dataset {} written to {}", dataset_version.upstream_sha, database_path.display());
        }
        CliCommand::Icons { source: data_files_dir, out: icons_dir } => {
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

fn head_commit_sha(repository_dir: &Path) -> Option<String> {
    let output = Command::new("git").arg("-C").arg(repository_dir).args(["rev-parse", "HEAD"]).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn current_utc_timestamp() -> String {
    let seconds_since_epoch =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days_since_epoch = (seconds_since_epoch / 86_400) as i64;
    let (hour, minute, second) =
        ((seconds_since_epoch % 86_400) / 3600, (seconds_since_epoch % 3600) / 60, seconds_since_epoch % 60);
    let days_since_era_zero = days_since_epoch + 719_468;
    let era = days_since_era_zero.div_euclid(146_097);
    let day_of_era = days_since_era_zero.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 { month_from_march + 3 } else { month_from_march - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}
