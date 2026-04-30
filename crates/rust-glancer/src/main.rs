use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod analyze;
mod runtime;

/// Command-line interface for the `rust-glancer` binary.
#[derive(Debug, Parser)]
#[command(name = "rust-glancer")]
#[command(about = "An incomplete-by-design Rust LSP implementation")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Top-level subcommands supported by the CLI.
#[derive(Debug, Subcommand)]
enum Command {
    /// Analyze the crate or workspace package located at `path`.
    Analyze {
        path: PathBuf,
        #[clap(short, long)]
        memory: bool,
    },
    /// Start the language server over stdio.
    Lsp,
}

/// Parses CLI arguments and dispatches to the selected command handler.
fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Analyze { path, memory } => analyze::analyze(path, memory),
        Command::Lsp => rg_lsp::run_stdio_with_memory_control(runtime::memory_control()),
    }
}
