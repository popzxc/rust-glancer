use std::path::PathBuf;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use rg_memsize::{MemoryRecord, MemoryRecordKind, MemoryRecorder, MemorySize};
use rg_project::{BuildProfile, BuildProfileOptions, Project, RssSampler};
use rg_workspace::{SysrootSources, WorkspaceMetadata};

const TOP_MEMORY_ROWS: usize = 12;

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
        Command::Analyze { path, memory } => analyze(path, memory),
        Command::Lsp => rg_lsp::run_stdio(),
    }
}

/// Runs project analysis for the Cargo manifest at `path` and prints a small build summary.
fn analyze(path: PathBuf, include_memory: bool) -> anyhow::Result<()> {
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
    let (project, build_profile) = if include_memory {
        let options = BuildProfileOptions {
            retained_memory: true,
            rss_sampler: process_rss_sampler(),
        };
        let (project, profile) = Project::build_profiled(workspace, options)
            .context("while attempting to build profiled project")?;
        (project, Some(profile))
    } else {
        (
            Project::build(workspace).context("while attempting to build project")?,
            None,
        )
    };
    print_project_summary(&project);
    if include_memory {
        if let Some(profile) = &build_profile {
            print_build_profile(profile);
        }
        print_memory_summary(&project);
    }

    Ok(())
}

fn print_project_summary(project: &Project) {
    let workspace_package_count = project.parse_db().workspace_packages().count();
    let package_count = project.parse_db().package_count();
    let def_map_stats = project.def_map_db().stats();
    let semantic_ir_stats = project.semantic_ir_db().stats();
    let body_ir_stats = project.body_ir_db().stats();

    println!("rust-glancer analysis built");
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

fn print_build_profile(profile: &BuildProfile) {
    println!();
    println!("build profile:");
    println!(
        "  {:>10}  {:>12}  {:>12}  {:>12}  checkpoint",
        "elapsed", "sampled", "active", "rss"
    );

    for checkpoint in profile.checkpoints() {
        println!(
            "  {:>10}  {:>12}  {:>12}  {:>12}  {}",
            format_duration(checkpoint.elapsed),
            checkpoint
                .retained_bytes
                .map(format_bytes)
                .unwrap_or_else(|| "-".to_string()),
            checkpoint
                .active_retained_bytes
                .map(format_bytes)
                .unwrap_or_else(|| "-".to_string()),
            checkpoint
                .rss_bytes
                .map(format_bytes)
                .unwrap_or_else(|| "-".to_string()),
            checkpoint.label,
        );
    }
}

fn print_memory_summary(project: &Project) {
    let mut recorder = MemoryRecorder::new("project");
    project.record_memory_size(&mut recorder);
    let records = recorder.records();

    println!();
    println!(
        "memory: {} retained across {} aggregate buckets",
        format_bytes(recorder.total_bytes()),
        records.len()
    );

    print_memory_section("memory by phase", top_level_totals(&records), usize::MAX);
    print_memory_section("memory by kind", kind_totals(&records), usize::MAX);
    print_memory_section(
        "top memory paths",
        string_totals(
            records
                .iter()
                .map(|record| (record.path.as_str(), record.bytes)),
        ),
        TOP_MEMORY_ROWS,
    );
    print_memory_section(
        "top memory types",
        string_totals(
            records
                .iter()
                .map(|record| (record.type_name.as_str(), record.bytes)),
        ),
        TOP_MEMORY_ROWS,
    );
}

fn print_memory_section(title: &str, rows: Vec<(String, usize)>, limit: usize) {
    println!("{title}:");

    for (label, bytes) in rows.into_iter().take(limit) {
        println!("  {:>10}  {label}", format_bytes(bytes));
    }
}

fn top_level_totals(records: &[MemoryRecord]) -> Vec<(String, usize)> {
    string_totals(records.iter().map(|record| {
        let path = top_level_path(&record.path);
        (path, record.bytes)
    }))
}

fn top_level_path(path: &str) -> String {
    let mut parts = path.split('.');
    let Some(root) = parts.next() else {
        return path.to_string();
    };
    let Some(child) = parts.next() else {
        return root.to_string();
    };

    format!("{root}.{child}")
}

fn kind_totals(records: &[MemoryRecord]) -> Vec<(String, usize)> {
    string_totals(
        records
            .iter()
            .map(|record| (memory_kind_label(record.kind), record.bytes)),
    )
}

fn string_totals<S>(items: impl IntoIterator<Item = (S, usize)>) -> Vec<(String, usize)>
where
    S: Into<String>,
{
    let mut totals = std::collections::BTreeMap::<String, usize>::new();
    for (label, bytes) in items {
        *totals.entry(label.into()).or_default() += bytes;
    }

    let mut rows = totals.into_iter().collect::<Vec<_>>();
    rows.sort_by(|(left_label, left_bytes), (right_label, right_bytes)| {
        right_bytes
            .cmp(left_bytes)
            .then_with(|| left_label.cmp(right_label))
    });
    rows
}

fn format_duration(duration: std::time::Duration) -> String {
    let millis = duration.as_secs_f64() * 1000.0;
    if millis < 1000.0 {
        format!("{millis:.0} ms")
    } else {
        format!("{:.2} s", duration.as_secs_f64())
    }
}

fn memory_kind_label(kind: MemoryRecordKind) -> &'static str {
    match kind {
        MemoryRecordKind::Shallow => "shallow",
        MemoryRecordKind::Heap => "heap",
        MemoryRecordKind::SpareCapacity => "spare capacity",
        MemoryRecordKind::Approximate => "approximate",
    }
}

fn process_rss_sampler() -> Option<RssSampler> {
    #[cfg(unix)]
    {
        Some(Box::new(sample_process_rss_bytes))
    }

    #[cfg(not(unix))]
    {
        None
    }
}

#[cfg(unix)]
fn sample_process_rss_bytes() -> Option<usize> {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p"])
        .arg(std::process::id().to_string())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8(output.stdout).ok()?;
    let kib = text.trim().parse::<usize>().ok()?;
    Some(kib.saturating_mul(1024))
}

fn format_bytes(bytes: usize) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

    let mut value = bytes as f64;
    let mut unit = UNITS[0];
    for next_unit in UNITS.iter().skip(1) {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next_unit;
    }

    if unit == "B" {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {unit}")
    }
}
