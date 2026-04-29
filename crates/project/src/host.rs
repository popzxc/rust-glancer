use std::path::{Path, PathBuf};

use anyhow::Context as _;

use rg_analysis::Analysis;
use rg_body_ir::{BodyIrBuildPolicy, BodyIrDb};
use rg_def_map::{DefMapDb, PackageSlot, TargetRef};
use rg_parse::{FileId, ParseDb};
use rg_semantic_ir::SemanticIrDb;
use rg_workspace::WorkspaceMetadata;

use crate::Project;

/// Mutable owner for the current analysis state.
///
/// `AnalysisHost` is the future LSP-facing state container: it accepts saved file changes,
/// refreshes the derived phase databases, and hands out immutable snapshots for queries.
///
/// The host intentionally follows a rebuild-on-save model. It does not track arbitrary unsaved
/// editor buffers; callers should provide text only for committed save events, or read the saved
/// file from disk and pass that content through the same API.
#[derive(Debug, Clone)]
pub struct AnalysisHost {
    project: Project,
}

impl AnalysisHost {
    /// Builds a host using the default editor-oriented Body IR policy.
    pub fn build(workspace: WorkspaceMetadata) -> anyhow::Result<Self> {
        Self::build_with_body_ir_policy(workspace, BodyIrBuildPolicy::default())
    }

    /// Builds a host using an explicit Body IR policy.
    pub fn build_with_body_ir_policy(
        workspace: WorkspaceMetadata,
        body_ir_policy: BodyIrBuildPolicy,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            project: Project::build_with_body_ir_policy(workspace, body_ir_policy)
                .context("while attempting to build analysis host")?,
        })
    }

    /// Returns an immutable query view of the current project state.
    pub fn snapshot(&self) -> AnalysisSnapshot<'_> {
        AnalysisSnapshot {
            project: &self.project,
        }
    }

    /// Applies one saved file replacement and refreshes derived analysis state.
    pub fn apply_change(
        &mut self,
        change: SavedFileChange,
    ) -> anyhow::Result<AnalysisChangeSummary> {
        self.apply_changes([change])
    }

    /// Applies a batch of saved file replacements and refreshes derived analysis state once.
    pub fn apply_changes(
        &mut self,
        changes: impl IntoIterator<Item = SavedFileChange>,
    ) -> anyhow::Result<AnalysisChangeSummary> {
        let changes = changes
            .into_iter()
            .map(|change| {
                let path = change.path.canonicalize().with_context(|| {
                    format!(
                        "while attempting to canonicalize changed file {}",
                        change.path.display()
                    )
                })?;
                Ok(SavedFileChange { path })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let mut changed_files = Vec::new();
        let mut fallback_package_roots = Vec::new();
        let mut fallback_saved_paths = Vec::new();

        for change in changes {
            let changed = self
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
                // package lets item-tree lowering rediscover any newly materialized `mod foo;`
                // files through the normal Rust module rules.
                for package_slot in self
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

        let affected_packages = self.affected_packages(&changed_files, &fallback_package_roots);
        if !affected_packages.is_empty() {
            self.project
                .rebuild_packages(&affected_packages)
                .context("while attempting to rebuild affected analysis packages")?;
        }
        self.promote_discovered_fallback_files(
            &fallback_saved_paths,
            &fallback_package_roots,
            &mut changed_files,
        );
        let changed_targets = self.targets_for_changed_files(&changed_files);

        Ok(AnalysisChangeSummary {
            changed_files,
            affected_packages,
            changed_targets,
        })
    }

    fn affected_packages(
        &self,
        changed_files: &[ChangedFile],
        fallback_package_roots: &[PackageSlot],
    ) -> Vec<PackageSlot> {
        let mut changed_package_ids = changed_files
            .iter()
            .filter_map(|changed_file| {
                self.project
                    .workspace()
                    .packages()
                    .get(changed_file.package.0)
                    .map(|package| package.id.clone())
            })
            .collect::<Vec<_>>();

        for package_slot in fallback_package_roots {
            let Some(package) = self.project.workspace().packages().get(package_slot.0) else {
                continue;
            };
            if !changed_package_ids.contains(&package.id) {
                changed_package_ids.push(package.id.clone());
            }
        }

        self.project
            .workspace()
            .reverse_dependency_closure(&changed_package_ids)
            .into_iter()
            .map(PackageSlot)
            .collect()
    }

    fn promote_discovered_fallback_files(
        &self,
        fallback_saved_paths: &[PathBuf],
        fallback_package_roots: &[PackageSlot],
        changed_files: &mut Vec<ChangedFile>,
    ) {
        for saved_path in fallback_saved_paths {
            for package_slot in fallback_package_roots {
                let Some(package) = self.project.parse_db().package(package_slot.0) else {
                    continue;
                };

                // Unknown saved files only become target/file diagnostics candidates after a
                // package rebuild proves they are actually part of the parsed module graph.
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

    fn targets_for_changed_files(&self, changed_files: &[ChangedFile]) -> Vec<TargetRef> {
        let mut targets = Vec::new();

        for changed_file in changed_files {
            for (target_ref, def_map) in self.project.def_map_db().target_maps() {
                if target_ref.package != changed_file.package {
                    continue;
                }

                let owns_file = def_map
                    .modules()
                    .iter()
                    .any(|module| module.origin.contains_file(changed_file.file));
                if owns_file && !targets.contains(&target_ref) {
                    targets.push(target_ref);
                }
            }
        }

        targets
    }
}

/// One source file saved on disk.
///
/// The analysis host treats the filesystem as the source of truth. This keeps save handling aligned
/// with the project's rebuild-on-save model and avoids retaining editor buffer text in analysis
/// caches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedFileChange {
    pub path: PathBuf,
}

impl SavedFileChange {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

/// Summary of what a change batch touched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisChangeSummary {
    pub changed_files: Vec<ChangedFile>,
    pub affected_packages: Vec<PackageSlot>,
    pub changed_targets: Vec<TargetRef>,
}

/// One known package-local source file that was reparsed in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChangedFile {
    pub package: PackageSlot,
    pub file: FileId,
}

/// Analysis-ready context for one filesystem path.
///
/// The same file can be reachable from more than one target, for example when a package library
/// and binary both declare `mod shared;`. Unreachable parsed-cache files are intentionally omitted
/// by path lookups, because LSP queries need a current target context to answer semantic questions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileContext {
    pub package: PackageSlot,
    pub file: FileId,
    pub targets: Vec<TargetRef>,
}

/// Immutable project view used to answer LSP-shaped queries.
#[derive(Debug, Clone, Copy)]
pub struct AnalysisSnapshot<'a> {
    project: &'a Project,
}

impl<'a> AnalysisSnapshot<'a> {
    /// Returns the high-level frozen query API.
    pub fn analysis(&self) -> Analysis<'a> {
        self.project.analysis()
    }

    pub fn parse_db(&self) -> &'a ParseDb {
        self.project.parse_db()
    }

    pub fn def_map_db(&self) -> &'a DefMapDb {
        self.project.def_map_db()
    }

    pub fn semantic_ir_db(&self) -> &'a SemanticIrDb {
        self.project.semantic_ir_db()
    }

    pub fn body_ir_db(&self) -> &'a BodyIrDb {
        self.project.body_ir_db()
    }

    /// Returns current analysis contexts for a saved filesystem path.
    pub fn file_contexts_for_path(
        &self,
        path: impl AsRef<Path>,
    ) -> anyhow::Result<Vec<FileContext>> {
        let path = path.as_ref();
        let canonical_path = path
            .canonicalize()
            .with_context(|| format!("while attempting to canonicalize {}", path.display()))?;

        let mut contexts = Vec::new();

        for (package_idx, package) in self.project.parse_db().packages().iter().enumerate() {
            let package_slot = PackageSlot(package_idx);

            for parsed_file in package.parsed_files() {
                if parsed_file.path() != canonical_path.as_path() {
                    continue;
                }

                let targets = self.targets_for_file(package_slot, parsed_file.file_id());
                if targets.is_empty() {
                    continue;
                }

                contexts.push(FileContext {
                    package: package_slot,
                    file: parsed_file.file_id(),
                    targets,
                });
            }
        }

        Ok(contexts)
    }

    /// Returns target contexts whose module tree contains a package-local file.
    pub fn targets_for_file(&self, package: PackageSlot, file: FileId) -> Vec<TargetRef> {
        let mut targets = Vec::new();

        for (target_ref, def_map) in self.project.def_map_db().target_maps() {
            if target_ref.package != package {
                continue;
            }

            let owns_file = def_map
                .modules()
                .iter()
                .any(|module| module.origin.contains_file(file));
            if owns_file {
                targets.push(target_ref);
            }
        }

        targets
    }
}
