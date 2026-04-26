use std::path::PathBuf;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use rg_project::Project;
use rg_workspace::WorkspaceMetadata;

/// Command-line interface for the `rust-glimpser` binary.
#[derive(Debug, Parser)]
#[command(name = "rust-glimpser")]
#[command(about = "An incomplete-by-design Rust LSP implementation")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Top-level subcommands supported by the CLI.
#[derive(Debug, Subcommand)]
enum Command {
    /// Analyze the crate or workspace package located at `path`.
    Analyze { path: PathBuf },
}

/// Parses CLI arguments and dispatches to the selected command handler.
fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Analyze { path } => analyze(path),
    }
}

/// Runs project analysis for the Cargo manifest at `path` and prints the current analysis report.
fn analyze(path: PathBuf) -> anyhow::Result<()> {
    if !path.exists() {
        anyhow::bail!("folder {} does not exist", path.display());
    }

    let cargo_manifest = path.join("Cargo.toml");
    if !cargo_manifest.exists() {
        anyhow::bail!("folder {} does not have Cargo.toml in it", path.display());
    }

    let metadata: cargo_metadata::Metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(cargo_manifest)
        .exec()
        .context("cargo metadata failed")?;

    let workspace = WorkspaceMetadata::from_cargo(metadata);
    let project = Project::build(workspace).context("while attempting to build project")?;
    println!("{project}");

    Ok(())
}
