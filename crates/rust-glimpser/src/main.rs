use std::path::PathBuf;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use rg_project::Project;
use rg_workspace::{SysrootSources, WorkspaceMetadata};

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
    /// Start the language server over stdio.
    Lsp,
}

/// Parses CLI arguments and dispatches to the selected command handler.
fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Analyze { path } => analyze(path),
        Command::Lsp => rg_lsp::run_stdio(),
    }
}

/// Runs project analysis for the Cargo manifest at `path` and prints a small build summary.
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
    let sysroot = SysrootSources::discover(workspace.workspace_root());
    let workspace = workspace.with_sysroot_sources(sysroot);
    let project = Project::build(workspace).context("while attempting to build project")?;
    print_project_summary(&project);

    Ok(())
}

fn print_project_summary(project: &Project) {
    let workspace_package_count = project.parse_db().workspace_packages().count();
    let package_count = project.parse_db().package_count();
    let def_map_stats = project.def_map_db().stats();
    let semantic_ir_stats = project.semantic_ir_db().stats();
    let body_ir_stats = project.body_ir_db().stats();

    println!("rust-glimpser analysis built");
    println!("packages: {package_count} ({workspace_package_count} workspace)");
    println!(
        "def maps: {} targets, {} modules, {} unresolved imports",
        def_map_stats.target_count,
        def_map_stats.module_count,
        def_map_stats.unresolved_import_count
    );
    println!(
        "semantic IR: {} targets, {} type defs, {} traits, {} impls, {} functions",
        semantic_ir_stats.target_count,
        semantic_ir_stats.struct_count
            + semantic_ir_stats.enum_count
            + semantic_ir_stats.union_count,
        semantic_ir_stats.trait_count,
        semantic_ir_stats.impl_count,
        semantic_ir_stats.function_count
    );
    println!(
        "body IR: {} targets ({} built, {} skipped), {} bodies, {} expressions",
        body_ir_stats.target_count,
        body_ir_stats.built_target_count,
        body_ir_stats.skipped_target_count,
        body_ir_stats.body_count,
        body_ir_stats.expression_count
    );
}
