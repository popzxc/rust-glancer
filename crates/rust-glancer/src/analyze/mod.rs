use std::path::PathBuf;

use anyhow::Context as _;
use rg_lsp::MemoryControl as _;
use rg_project::{BuildProcessMemory, PackageResidencyPolicy, Project};
use rg_workspace::{SysrootSources, WorkspaceMetadata};

mod fmt;

/// Runs project analysis for the Cargo manifest at `path` and prints a small build summary.
pub(super) fn analyze(
    path: PathBuf,
    include_memory: bool,
    package_residency_policy: PackageResidencyPolicy,
) -> anyhow::Result<()> {
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

    let workspace = WorkspaceMetadata::from_cargo(metadata)
        .context("while attempting to normalize Cargo metadata")?;
    let sysroot = SysrootSources::discover(workspace.workspace_root());
    let workspace = workspace.with_sysroot_sources(sysroot);
    let memory_control = crate::runtime::memory_control();
    let project_build = if include_memory {
        Project::builder(workspace)
            .package_residency_policy(package_residency_policy)
            .measure_retained_memory(true)
            .process_memory_sampler(move || {
                memory_control
                    .allocator_stats()
                    .map(|stats| BuildProcessMemory {
                        allocated_bytes: stats.allocated_bytes,
                        active_bytes: stats.active_bytes,
                        resident_bytes: stats.resident_bytes,
                    })
            })
            .build()
            .context("while attempting to build profiled project")?
    } else {
        Project::builder(workspace)
            .package_residency_policy(package_residency_policy)
            .build()
            .context("while attempting to build project")?
    };
    let (project, build_profile) = project_build.into_parts();
    self::fmt::print_project_summary(&project);
    if include_memory {
        println!("allocator: {}", memory_control.allocator_name());
        if let Some(stats) = memory_control.allocator_stats() {
            self::fmt::print_allocator_stats(stats);
        }
        let purge = self::fmt::purge_allocator_after_build(&memory_control);
        if let Some(purge) = &purge {
            self::fmt::print_allocator_purge_after_build(purge);
        }
        if let Some(profile) = &build_profile {
            self::fmt::print_build_profile(profile, purge.as_ref());
        }
        self::fmt::print_memory_summary(&project);
    }

    Ok(())
}
