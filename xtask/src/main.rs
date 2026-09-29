mod comments;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use comments::{comments_in, without_comments};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

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
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Task::Lint => {
            deny_clippy_warnings()?;
            deny_comments()
        }
        Task::NoComments { fix: false } => deny_comments(),
        Task::NoComments { fix: true } => strip_workspace_comments(),
    }
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

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}
