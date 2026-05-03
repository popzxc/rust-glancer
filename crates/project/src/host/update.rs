//! Applies ordinary source-file saves without invalidating the workspace graph.
//!
//! This path keeps package and target slots stable. It reparses saved files, rebuilds only affected
//! packages and their reverse dependents, and reports changed targets from the updated def-map
//! snapshot.

use std::path::PathBuf;

use anyhow::Context as _;

use rg_def_map::{PackageSlot, TargetRef};

use super::{AnalysisChangeSummary, AnalysisHost, ChangedFile, SavedFileChange};

pub(super) fn apply_source_changes(
    host: &mut AnalysisHost,
    changes: Vec<SavedFileChange>,
) -> anyhow::Result<AnalysisChangeSummary> {
    let mut changed_files = Vec::new();
    let mut fallback_package_roots = Vec::new();
    let mut fallback_saved_paths = Vec::new();

    for change in changes {
        let changed = host
            .project
            .parse_db_mut()
            .reparse_saved_file(&change.path)
            .with_context(|| {
                format!(
                    "while attempting to apply saved file change for {}",
                    change.path.display()
                )
            })?;

        if changed.is_empty() {
            if !fallback_saved_paths.contains(&change.path) {
                fallback_saved_paths.push(change.path.clone());
            }

            // A saved file can be new to the graph even though it now exists on disk. In that
            // case, package roots are the coarse ownership boundary: rebuilding the containing
            // package lets item-tree lowering rediscover any newly materialized `mod foo;` files
            // through the normal Rust module rules.
            for package_slot in host
                .project
                .workspace()
                .package_slots_containing_path(&change.path)
            {
                let package_slot = PackageSlot(package_slot);
                if !fallback_package_roots.contains(&package_slot) {
                    fallback_package_roots.push(package_slot);
                }
            }
        }

        for changed_file in changed {
            let changed_file = ChangedFile {
                package: PackageSlot(changed_file.package),
                file: changed_file.file,
            };
            if !changed_files.contains(&changed_file) {
                changed_files.push(changed_file);
            }
        }
    }

    let affected_packages = affected_packages(host, &changed_files, &fallback_package_roots);
    if !affected_packages.is_empty() {
        host.project
            .rebuild_packages(&affected_packages)
            .context("while attempting to rebuild affected analysis packages")?;
    }
    promote_discovered_fallback_files(
        host,
        &fallback_saved_paths,
        &fallback_package_roots,
        &mut changed_files,
    );
    let changed_targets = targets_for_changed_files(host, &changed_files)
        .context("while attempting to report changed analysis targets")?;

    Ok(AnalysisChangeSummary {
        changed_files,
        affected_packages,
        changed_targets,
    })
}

fn affected_packages(
    host: &AnalysisHost,
    changed_files: &[ChangedFile],
    fallback_package_roots: &[PackageSlot],
) -> Vec<PackageSlot> {
    let mut changed_package_ids = changed_files
        .iter()
        .filter_map(|changed_file| {
            host.project
                .workspace()
                .packages()
                .get(changed_file.package.0)
                .map(|package| package.id.clone())
        })
        .collect::<Vec<_>>();

    for package_slot in fallback_package_roots {
        let Some(package) = host.project.workspace().packages().get(package_slot.0) else {
            continue;
        };
        if !changed_package_ids.contains(&package.id) {
            changed_package_ids.push(package.id.clone());
        }
    }

    host.project
        .workspace()
        .reverse_dependency_closure(&changed_package_ids)
        .into_iter()
        .map(PackageSlot)
        .collect()
}

fn promote_discovered_fallback_files(
    host: &AnalysisHost,
    fallback_saved_paths: &[PathBuf],
    fallback_package_roots: &[PackageSlot],
    changed_files: &mut Vec<ChangedFile>,
) {
    for saved_path in fallback_saved_paths {
        for package_slot in fallback_package_roots {
            let Some(package) = host.project.parse_db().package(package_slot.0) else {
                continue;
            };

            // Unknown saved files only become target/file diagnostics candidates after a package
            // rebuild proves they are actually part of the parsed module graph.
            for parsed_file in package.parsed_files() {
                if parsed_file.path() != saved_path {
                    continue;
                }

                let changed_file = ChangedFile {
                    package: *package_slot,
                    file: parsed_file.file_id(),
                };
                if !changed_files.contains(&changed_file) {
                    changed_files.push(changed_file);
                }
            }
        }
    }
}

fn targets_for_changed_files(
    host: &AnalysisHost,
    changed_files: &[ChangedFile],
) -> anyhow::Result<Vec<TargetRef>> {
    let txn = host.project.read_txn()?;
    let analysis = host.project.analysis(&txn);
    let mut targets = Vec::new();

    for changed_file in changed_files {
        for target_ref in analysis.targets_for_file(changed_file.package, changed_file.file) {
            if !targets.contains(&target_ref) {
                targets.push(target_ref);
            }
        }
    }

    Ok(targets)
}
