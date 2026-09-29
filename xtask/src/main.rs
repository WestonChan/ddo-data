mod comments;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use comments::{comments_in, without_comments};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;
use xtask::dataset::{build_database_file, wiki_overrides_from};
use xtask::response_examples::{write_response_examples, EXAMPLE_REQUESTS};
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
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Task::Lint => {
            deny_clippy_warnings()?;
            deny_comments()
        }
        Task::NoComments { fix: false } => deny_comments(),
        Task::NoComments { fix: true } => strip_workspace_comments(),
        Task::RefreshExamples { db: db_path, source: data_files_dir, out: examples_dir } => {
            refresh_response_examples(db_path.as_deref(), data_files_dir, examples_dir)
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
            build_database_file(&data_files_dir, &wiki_overrides_from(None)?, built_db_file.path())?;
            built_db_file.path()
        }
    };
    for written_example in write_response_examples(db_path, EXAMPLE_REQUESTS, &examples_dir)? {
        println!("{:<28} {:>6} bytes", written_example.file_name, written_example.size_bytes);
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
