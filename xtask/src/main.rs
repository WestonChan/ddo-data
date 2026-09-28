mod comments;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Lint,
    NoComments {
        #[arg(long)]
        fix: bool,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Cmd::Lint => {
            run_clippy()?;
            no_comments(false)
        }
        Cmd::NoComments { fix } => no_comments(fix),
    }
}

fn run_clippy() -> Result<()> {
    let status = Command::new("cargo")
        .args(["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])
        .status()
        .context("running cargo clippy")?;
    if !status.success() {
        bail!("clippy failed");
    }
    Ok(())
}

fn no_comments(fix: bool) -> Result<()> {
    let mut offenders = 0;
    for path in rust_sources(workspace_root()) {
        let source = std::fs::read_to_string(&path)?;
        let found = comments::find_comments(&source);
        if found.is_empty() {
            continue;
        }
        if fix {
            std::fs::write(&path, comments::strip_comments(&source))?;
            println!("stripped {} comment(s) from {}", found.len(), path.display());
        } else {
            for comment in &found {
                println!("{}:{}: comment not allowed", path.display(), comment.line);
            }
            offenders += found.len();
        }
    }
    if offenders > 0 {
        bail!("{offenders} comment(s) found; run `cargo xtask no-comments --fix`");
    }
    Ok(())
}

fn rust_sources(root: PathBuf) -> impl Iterator<Item = PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !matches!(e.file_name().to_str(), Some("target" | "upstream" | ".git")))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
        .map(|e| e.into_path())
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}
