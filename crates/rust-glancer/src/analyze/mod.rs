use std::path::PathBuf;

use anyhow::Context as _;
use rg_lsp::MemoryControl as _;
use rg_project::{BuildProfileOptions, Project};
use rg_workspace::{SysrootSources, WorkspaceMetadata};

mod fmt;

/// Runs project analysis for the Cargo manifest at `path` and prints a small build summary.
pub(super) fn analyze(path: PathBuf, include_memory: bool) -> anyhow::Result<()> {
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
    let memory_control = crate::runtime::memory_control();
    let (project, build_profile) = if include_memory {
        let options = BuildProfileOptions {
            retained_memory: true,
            resident_memory_sampler: Some(Box::new(move || memory_control.resident_bytes())),
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
    self::fmt::print_project_summary(&project);
    if include_memory {
        println!("allocator: {}", memory_control.allocator_name());
        if let Some(stats) = memory_control.allocator_stats() {
            self::fmt::print_allocator_stats(stats);
        }
        self::fmt::print_allocator_purge_after_build(&memory_control);
        if let Some(profile) = &build_profile {
            self::fmt::print_build_profile(profile);
        }
        self::fmt::print_memory_summary(&project);
    }

    Ok(())
}
