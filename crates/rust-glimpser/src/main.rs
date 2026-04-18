use std::path::PathBuf;

use clap::{Parser, Subcommand};

use rust_glimpser::analyze;

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
