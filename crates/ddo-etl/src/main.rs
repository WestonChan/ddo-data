use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "ddo-etl", about = "Build the DDO Tools game database from DDOBuilderV2 data files")]
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
        #[arg(long)]
        wiki: Option<PathBuf>,
        #[arg(long)]
        corrections: Option<PathBuf>,
    },
    Build {
        #[arg(long)]
        source: Option<PathBuf>,
        #[arg(long = "data", conflicts_with = "source")]
        data_dir: Option<PathBuf>,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        sha: Option<String>,
        #[arg(long)]
        wiki: Option<PathBuf>,
        #[arg(long)]
        corrections: Option<PathBuf>,
    },
    Export {
        #[arg(long)]
        source: Option<PathBuf>,
        #[arg(long, conflicts_with = "source")]
        db: Option<PathBuf>,
        #[arg(long)]
        out: PathBuf,
    },
    Find {
        text: String,
        #[arg(long = "data", default_value = "data")]
        data_dir: PathBuf,
    },
    Icons {
        #[arg(long)]
        source: Option<PathBuf>,
        #[arg(long = "data", conflicts_with = "source")]
        data_dir: Option<PathBuf>,
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
            println!("pruned {pruned} expired redirects and removed slugs");
        }
        CliCommand::ImportDdobuilder { source, previous, data_dir, sha, previous_sha, wiki, corrections } => {
            ddo_etl::import_ddobuilder::validate_previous_commit(&data_dir, &previous, previous_sha.clone())?;
            let wiki = match wiki {
                Some(path) => ddo_etl::wiki::WikiOverrides::from_dir(&path)?,
                None => ddo_etl::wiki::WikiOverrides::embedded()?,
            };
            let corrections = match corrections {
                Some(path) => ddo_etl::corrections::Corrections::from_dir(&path)?,
                None => ddo_etl::corrections::Corrections::from_data(&data_dir)?,
            };
            let report =
                ddo_etl::import_ddobuilder::import_ddobuilder(&ddo_etl::import_ddobuilder::DdobuilderImportOptions {
                    source_dir: source,
                    previous_dir: previous,
                    data_dir,
                    sha,
                    previous_sha,
                    wiki,
                    corrections,
                })?;
            println!("{report}");
            return Ok(if report.has_conflicts() { ExitCode::from(2) } else { ExitCode::SUCCESS });
        }
        CliCommand::Build {
            source: data_files_dir,
            data_dir,
            out: database_path,
            sha: upstream_sha,
            wiki: wiki_dir,
            corrections: corrections_dir,
        } => {
            if database_path.exists() {
                std::fs::remove_file(&database_path)
                    .with_context(|| format!("removing {}", database_path.display()))?;
            }
            if let Some(data_dir) = data_dir {
                let mut db = Connection::open(&database_path)?;
                let report = ddo_etl::tree::build_from_data(&data_dir, &mut db)?;
                println!("{report}");
                println!("dataset written to {}", database_path.display());
                return Ok(ExitCode::SUCCESS);
            }
            let data_files_dir = data_files_dir.unwrap_or_else(default_data_files_dir);
            let dataset_version = ddo_etl::upstream::dataset_version(&data_files_dir, upstream_sha);
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
        CliCommand::Export { source: data_files_dir, db: baseline_db, out: data_dir } => {
            if let Some(baseline_db) = baseline_db {
                let db = Connection::open(&baseline_db)?;
                let report = ddo_etl::tree::export_database(&db, &data_dir)?;
                println!("{report}");
                anyhow::ensure!(report.icon_actions.is_empty(), "export has HARD image layout failures");
                return Ok(ExitCode::SUCCESS);
            }
            let data_files_dir = data_files_dir.unwrap_or_else(default_data_files_dir);
            let wiki_overrides = ddo_etl::wiki::WikiOverrides::embedded()?;
            let corrections = ddo_etl::corrections::Corrections::embedded()?;
            let dataset_version = ddo_etl::upstream::dataset_version(&data_files_dir, None);
            let mut db = Connection::open_in_memory()?;
            ddo_etl::build::build_database(&data_files_dir, &wiki_overrides, &corrections, &mut db, &dataset_version)?;
            let report = ddo_etl::tree::export_ddobuilder_tree(&db, &data_files_dir, &data_dir)?;
            println!("{report}");
        }
        CliCommand::Find { text, data_dir } => print!("{}", ddo_etl::tree::find(&data_dir, &text)?),
        CliCommand::Icons { source: data_files_dir, data_dir, out: icons_dir } => {
            if let Some(data_dir) = data_dir {
                let report = ddo_etl::icons::copy_data_icons(&data_dir, &icons_dir)?;
                println!("{report:#?}");
                return Ok(ExitCode::SUCCESS);
            }
            let data_files_dir = data_files_dir.unwrap_or_else(default_data_files_dir);
            let report = ddo_etl::icons::export_icons(&data_files_dir, &icons_dir)?;
            ddo_etl::icons::link_corrected_icons(&ddo_etl::corrections::Corrections::embedded()?, &icons_dir)?;
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

fn default_data_files_dir() -> PathBuf {
    ddo_etl::upstream::default_data_files_dir(Path::new(""))
}
