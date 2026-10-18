use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rusqlite::Connection;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "ddo-etl", about = "Build the DDO Tools game database from its data tree")]
struct Cli {
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand)]
enum CliCommand {
    PruneRedirects {
        #[arg(long = "data", default_value = "data")]
        data_dir: PathBuf,
        #[arg(long)]
        today: Option<String>,
    },
    ImportDdobuilder {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        previous: PathBuf,
        #[arg(long = "data", default_value = "data")]
        data_dir: PathBuf,
        #[arg(long)]
        sha: Option<String>,
        #[arg(long)]
        previous_sha: Option<String>,
    },
    Build {
        #[arg(long = "data", default_value = "data")]
        data_dir: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Export {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Find {
        text: String,
        #[arg(long = "data", default_value = "data")]
        data_dir: PathBuf,
    },
    Icons {
        #[arg(long = "data", default_value = "data")]
        data_dir: PathBuf,
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

fn main() -> Result<ExitCode> {
    match Cli::parse().command {
        CliCommand::PruneRedirects { data_dir, today } => {
            let pruned = ddo_etl::tree::redirects::prune_redirects(&data_dir, today.as_deref())?;
            println!("pruned {pruned} expired redirects and retired slugs");
        }
        CliCommand::ImportDdobuilder { source, previous, data_dir, sha, previous_sha } => {
            let report =
                ddo_etl::import_ddobuilder::import_ddobuilder(&ddo_etl::import_ddobuilder::DdobuilderImportOptions {
                    source_dir: source,
                    previous_dir: previous,
                    data_dir,
                    sha,
                    previous_sha,
                })?;
            println!("{report}");
            return Ok(if report.has_conflicts() { ExitCode::from(2) } else { ExitCode::SUCCESS });
        }
        CliCommand::Build { data_dir, out: database_path } => {
            if database_path.exists() {
                std::fs::remove_file(&database_path)
                    .with_context(|| format!("removing {}", database_path.display()))?;
            }
            let mut db = Connection::open(&database_path)?;
            let report = ddo_etl::tree::build_from_data(&data_dir, &mut db)?;
            println!("{report}");
            println!("dataset written to {}", database_path.display());
        }
        CliCommand::Export { db: baseline_db, out: data_dir } => {
            let db = Connection::open(&baseline_db)?;
            let report = ddo_etl::tree::export_database(&db, &data_dir)?;
            println!("{report}");
            anyhow::ensure!(report.icon_actions.is_empty(), "export has HARD image layout failures");
        }
        CliCommand::Find { text, data_dir } => print!("{}", ddo_etl::tree::find(&data_dir, &text)?),
        CliCommand::Icons { data_dir, out: icons_dir } => {
            let report = ddo_etl::icons::copy_data_icons(&data_dir, &icons_dir)?;
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
    Ok(ExitCode::SUCCESS)
}
