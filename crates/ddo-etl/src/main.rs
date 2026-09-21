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
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Parse a DataFiles directory and write a fresh SQLite database.
    Build {
        /// Path to DDOBuilderV2's Output/DataFiles directory.
        #[arg(long)]
        source: PathBuf,
        /// Output database path. Replaced if it exists.
        #[arg(long)]
        out: PathBuf,
        /// Upstream commit SHA to record. Defaults to `git rev-parse HEAD` in the source checkout.
        #[arg(long)]
        sha: Option<String>,
    },
    /// Report item-name coverage of a new database against a previous one.
    Diff {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        legacy: PathBuf,
        /// Print every unmatched name rather than the first twenty of each.
        #[arg(long)]
        verbose: bool,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Cmd::Build { source, out, sha } => {
            let sha = sha.unwrap_or_else(|| git_sha(&source).unwrap_or_else(|| "unknown".to_string()));
            let version = DatasetVersion { upstream_sha: sha, built_at: now_utc() };
            if out.exists() {
                std::fs::remove_file(&out).with_context(|| format!("removing {}", out.display()))?;
            }
            let mut conn = Connection::open(&out)?;
            let report = ddo_etl::build::build(&source, &mut conn, &version)?;
            println!("{report:#?}");
            println!("dataset {} written to {}", version.upstream_sha, out.display());
        }
        Cmd::Diff { db, legacy, verbose } => {
            let new = Connection::open(&db)?;
            let old = Connection::open(&legacy)?;
            let report = ddo_etl::diff::compare(&new, &old)?;
            println!(
                "matched {}  only in new {}  only in legacy {}  excluded by design {}  coverage {:.1}%",
                report.matched,
                report.only_new.len(),
                report.only_legacy.len(),
                report.excluded_by_design.len(),
                report.coverage() * 100.0
            );
            let limit = if verbose { usize::MAX } else { 20 };
            println!("\n-- only in legacy (first {}):", report.only_legacy.len().min(limit));
            for n in report.only_legacy.iter().take(limit) {
                println!("  {n}");
            }
            println!("\n-- only in new (first {}):", report.only_new.len().min(limit));
            for n in report.only_new.iter().take(limit) {
                println!("  {n}");
            }
        }
    }
    Ok(())
}

fn git_sha(source: &Path) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(source).args(["rev-parse", "HEAD"]).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// ISO-8601 UTC timestamp without a chrono dependency (civil-from-days, Howard Hinnant).
fn now_utc() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let (h, m, s) = ((secs % 86_400) / 3600, (secs % 3600) / 60, secs % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}
