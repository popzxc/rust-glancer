//! Rebuilds the whole analysis project after workspace graph changes.
//!
//! When Cargo metadata can change package, target, or dependency slots, partial reuse becomes more
//! dangerous than useful. This path reloads metadata and rebuilds every non-sysroot package so the
//! downstream phase databases return to a single consistent snapshot.

use anyhow::Context as _;

use rg_def_map::PackageSlot;
use rg_workspace::WorkspaceMetadata;

use crate::Project;

use super::{AnalysisChangeSummary, AnalysisHost, ChangedFile, SavedFileChange};

pub(super) fn rebuild_workspace_graph(
    host: &mut AnalysisHost,
    changes: &[SavedFileChange],
) -> anyhow::Result<AnalysisChangeSummary> {
    let manifest_path = host.project.workspace().workspace_root().join("Cargo.toml");
    let sysroot = host.project.workspace().sysroot_sources();
    let workspace = WorkspaceMetadata::from_manifest_path(&manifest_path)
        .with_context(|| format!("while attempting to load {}", manifest_path.display()))?
        .with_sysroot_sources(sysroot);
    let build_options = host.project.build_options;

    host.project = Project::build_with_options(workspace, build_options)
        .context("while attempting to build refreshed analysis project")?;

    let changed_files = changed_source_files_for_saved_paths(host, changes);
    let affected_packages = host
        .project
        .workspace()
        .packages()
        .iter()
        .enumerate()
        .filter_map(|(package_slot, package)| {
            (!package.origin.is_sysroot()).then_some(PackageSlot(package_slot))
        })
        .collect::<Vec<_>>();
    let changed_targets = host
        .project
        .def_map_db()
        .target_maps()
        .filter_map(|(target, _)| {
            affected_packages
                .contains(&target.package)
                .then_some(target)
        })
        .collect();

    Ok(AnalysisChangeSummary {
        changed_files,
        affected_packages,
        changed_targets,
    })
}

fn changed_source_files_for_saved_paths(
    host: &AnalysisHost,
    changes: &[SavedFileChange],
) -> Vec<ChangedFile> {
    let mut changed_files = Vec::new();

    for change in changes {
        for (package_slot, package) in host.project.parse_db().packages().iter().enumerate() {
            for parsed_file in package.parsed_files() {
                if parsed_file.path() != change.path {
                    continue;
                }

                let changed_file = ChangedFile {
                    package: PackageSlot(package_slot),
                    file: parsed_file.file_id(),
                };
                if !changed_files.contains(&changed_file) {
                    changed_files.push(changed_file);
                }
            }
        }
    }

    changed_files
}
