use std::path::PathBuf;

use clap::{Parser, Subcommand};

use rust_glimpser::analyze;

#[derive(Debug, Parser)]
#[command(name = "rust-glimpser")]
#[command(about = "An incomplete-by-design Rust LSP implementation")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Analyze { path: PathBuf },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Analyze { path } => analyze(path),
    }
}
