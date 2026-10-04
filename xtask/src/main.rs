mod comments;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use comments::{comments_in, without_comments};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;
use xtask::dataset::{build_database_file, corrections_from, wiki_overrides_from};
use xtask::integrity::{integrity_report, IntegrityOptions};
use xtask::response_examples::{write_response_examples, write_v1_detail_snapshot, EXAMPLE_REQUESTS};
use xtask::wiki_tools::{wiki_check_report, write_wiki_batch};
use xtask::workspace_root;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Task,
}

#[derive(Subcommand)]
enum Task {
    Lint,
    NoComments {
        #[arg(long)]
        fix: bool,
    },
    RefreshExamples {
        #[arg(long)]
        db: Option<PathBuf>,
        #[arg(long)]
        source: Option<PathBuf>,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    SnapshotV1Details {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    WikiCheck {
        #[arg(long)]
        wiki: Option<PathBuf>,
        #[arg(long)]
        corrections: Option<PathBuf>,
        #[arg(long)]
        source: Option<PathBuf>,
    },
    CheckDb {
        db: PathBuf,
        #[arg(long)]
        source: Option<PathBuf>,
        #[arg(long)]
        corrections: Option<PathBuf>,
        #[arg(long = "allow-empty-table")]
        allowed_empty_tables: Vec<String>,
    },
    WikiBatch {
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        wiki: Option<PathBuf>,
        #[arg(long)]
        corrections: Option<PathBuf>,
        #[arg(long)]
        source: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Task::Lint => {
            deny_clippy_warnings()?;
            corrections_from(None)?;
            deny_api_contract_or_scale_regressions()?;
            deny_comments()
        }
        Task::NoComments { fix: false } => deny_comments(),
        Task::NoComments { fix: true } => strip_workspace_comments(),
        Task::RefreshExamples { db: db_path, source: data_files_dir, out: examples_dir } => {
            refresh_response_examples(db_path.as_deref(), data_files_dir, examples_dir)
        }
        Task::SnapshotV1Details { db, out } => {
            println!("wrote {} v1 details to {}", write_v1_detail_snapshot(&db, &out)?, out.display());
            Ok(())
        }
        Task::WikiCheck { wiki: wiki_dir, corrections: corrections_dir, source: data_files_dir } => {
            let data_files_dir = data_files_dir.unwrap_or_else(default_data_files_dir);
            println!("{}", wiki_check_report(&data_files_dir, wiki_dir.as_deref(), corrections_dir.as_deref())?);
            Ok(())
        }
        Task::CheckDb { db: db_path, source: data_files_dir, corrections: corrections_dir, allowed_empty_tables } => {
            check_database_integrity(&db_path, data_files_dir, corrections_dir.as_deref(), allowed_empty_tables)
        }
        Task::WikiBatch { out: batch_dir, wiki: wiki_dir, corrections: corrections_dir, source: data_files_dir } => {
            let data_files_dir = data_files_dir.unwrap_or_else(default_data_files_dir);
            let batch_dir = batch_dir.unwrap_or_else(|| workspace_root().join("target/wiki-batch"));
            for written_path in
                write_wiki_batch(&data_files_dir, wiki_dir.as_deref(), corrections_dir.as_deref(), &batch_dir)?
            {
                println!("wrote {written_path}");
            }
            Ok(())
        }
    }
}

fn refresh_response_examples(
    db_path: Option<&Path>,
    data_files_dir: Option<PathBuf>,
    examples_dir: Option<PathBuf>,
) -> Result<()> {
    let examples_dir = examples_dir.unwrap_or_else(|| workspace_root().join("crates/ddo-api/docs/examples/v1"));
    let built_db_file;
    let db_path = match db_path {
        Some(db_path) => db_path,
        None => {
            built_db_file = tempfile::NamedTempFile::new()?;
            let data_files_dir = data_files_dir.unwrap_or_else(default_data_files_dir);
            println!("building {} into {}", data_files_dir.display(), built_db_file.path().display());
            build_database_file(
                &data_files_dir,
                &wiki_overrides_from(None)?,
                &corrections_from(None)?,
                built_db_file.path(),
            )?;
            built_db_file.path()
        }
    };
    for written_example in write_response_examples(db_path, EXAMPLE_REQUESTS, &examples_dir)? {
        println!("{:<28} {:>6} bytes", written_example.file_name, written_example.size_bytes);
    }
    Ok(())
}

fn check_database_integrity(
    db_path: &Path,
    data_files_dir: Option<PathBuf>,
    corrections_dir: Option<&Path>,
    allowed_empty_tables: Vec<String>,
) -> Result<()> {
    let db = rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening {}", db_path.display()))?;
    let options = IntegrityOptions {
        corrections: corrections_from(corrections_dir)?,
        allowed_empty_tables,
        data_files_dir: data_files_dir.unwrap_or_else(default_data_files_dir),
    };
    let report = integrity_report(&db, &options)?;
    println!("{report}");
    let failed_check_names = report.failed_hard_check_names();
    if !failed_check_names.is_empty() {
        bail!("{} HARD integrity check(s) failed: {}", failed_check_names.len(), failed_check_names.join(", "));
    }
    Ok(())
}

fn default_data_files_dir() -> PathBuf {
    ddo_etl::upstream::default_data_files_dir(&workspace_root())
}

fn deny_clippy_warnings() -> Result<()> {
    let clippy_status = Command::new("cargo")
        .args(["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])
        .status()
        .context("running cargo clippy")?;
    if !clippy_status.success() {
        bail!("clippy failed");
    }
    Ok(())
}

fn deny_api_contract_or_scale_regressions() -> Result<()> {
    for test_name in [
        "list_parameter_docs_share_descriptions_and_match_runtime_sort_fields",
        "item_pack_query_count_does_not_grow_with_rows_or_sources",
        "typed_response_schemas_resolve_every_reference_and_describe_fields",
        "every_json_route_response_matches_its_openapi_schema",
    ] {
        let test_status = Command::new("cargo")
            .args(["test", "-p", "ddo-api", "--test", "api", test_name, "--", "--exact"])
            .status()
            .with_context(|| format!("checking API contract or scale guard: {test_name}"))?;
        if !test_status.success() {
            bail!("API contract or scale guard failed: {test_name}");
        }
    }
    Ok(())
}

fn deny_comments() -> Result<()> {
    let mut comment_count = 0;
    for path in rust_source_paths(workspace_root()) {
        let source_code = std::fs::read_to_string(&path)?;
        for comment in comments_in(&source_code) {
            println!("{}:{}: comment not allowed", path.display(), comment.line_number);
            comment_count += 1;
        }
    }
    if comment_count > 0 {
        bail!("{comment_count} comment(s) found; run `cargo xtask no-comments --fix`");
    }
    Ok(())
}

fn strip_workspace_comments() -> Result<()> {
    for path in rust_source_paths(workspace_root()) {
        let source_code = std::fs::read_to_string(&path)?;
        let comment_count = comments_in(&source_code).len();
        if comment_count == 0 {
            continue;
        }
        std::fs::write(&path, without_comments(&source_code))?;
        println!("stripped {comment_count} comment(s) from {}", path.display());
    }
    Ok(())
}

fn rust_source_paths(search_root: PathBuf) -> impl Iterator<Item = PathBuf> {
    WalkDir::new(search_root)
        .into_iter()
        .filter_entry(|entry| !matches!(entry.file_name().to_str(), Some("target" | "upstream" | ".git")))
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|extension| extension == "rs"))
        .map(|entry| entry.into_path())
}
