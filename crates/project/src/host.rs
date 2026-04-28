use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::Context as _;

use rg_analysis::Analysis;
use rg_body_ir::{BodyIrBuildPolicy, BodyIrDb};
use rg_def_map::{DefMapDb, PackageSlot, TargetRef};
use rg_item_tree::ItemTreeDb;
use rg_parse::{FileId, ParseDb};
use rg_semantic_ir::SemanticIrDb;
use rg_workspace::WorkspaceMetadata;

use crate::Project;

/// Mutable owner for the current analysis state.
///
/// `AnalysisHost` is the future LSP-facing state container: it accepts editor file changes,
/// refreshes the derived phase databases, and hands out immutable snapshots for queries. The
/// internal rebuild is intentionally centralized here so package-level replacement can be added
/// behind this boundary without changing the query API.
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

    /// Applies one in-memory file replacement and refreshes derived analysis state.
    pub fn apply_change(&mut self, change: FileChange) -> anyhow::Result<AnalysisChangeSummary> {
        self.apply_changes([change])
    }

    /// Applies a batch of in-memory file replacements and refreshes derived analysis state once.
    pub fn apply_changes(
        &mut self,
        changes: impl IntoIterator<Item = FileChange>,
    ) -> anyhow::Result<AnalysisChangeSummary> {
        let changes = changes.into_iter().collect::<Vec<_>>();
        for change in &changes {
            change.path.canonicalize().with_context(|| {
                format!(
                    "while attempting to canonicalize changed file {}",
                    change.path.display()
                )
            })?;
        }

        let mut changed_files = Vec::new();

        for change in changes {
            let changed = self
                .project
                .parse_db_mut()
                .set_file_text(&change.path, &change.text)
                .with_context(|| {
                    format!(
                        "while attempting to apply source change for {}",
                        change.path.display()
                    )
                })?;

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

        let affected_packages = self.affected_packages(&changed_files);
        if !changed_files.is_empty() {
            // TODO: replace whole-project derived rebuilding with package/target replacement.
            // The host boundary and stable parse cache are in place; the next performance step is
            // making each phase expose a package-scoped rebuild API and calling it from here.
            self.project
                .rebuild_derived()
                .context("while attempting to rebuild analysis after source changes")?;
        }
        let changed_targets = self.targets_for_changed_files(&changed_files);

        Ok(AnalysisChangeSummary {
            changed_files,
            affected_packages,
            changed_targets,
        })
    }

    fn affected_packages(&self, changed_files: &[ChangedFile]) -> Vec<PackageSlot> {
        let mut affected_ids = changed_files
            .iter()
            .filter_map(|changed_file| {
                self.project
                    .workspace()
                    .packages()
                    .get(changed_file.package.0)
                    .map(|package| package.id.clone())
            })
            .collect::<HashSet<_>>();

        // A package's exported surface can affect every reverse dependent. We intentionally use
        // package granularity here: it is coarse, predictable, and avoids pretending that we track
        // fine-grained item dependencies before the LSP has real-world pressure on it.
        loop {
            let previous_len = affected_ids.len();

            for package in self.project.workspace().packages() {
                if affected_ids.contains(&package.id) {
                    continue;
                }

                if package
                    .dependencies
                    .iter()
                    .any(|dependency| affected_ids.contains(dependency.package_id()))
                {
                    affected_ids.insert(package.id.clone());
                }
            }

            if affected_ids.len() == previous_len {
                break;
            }
        }

        self.project
            .workspace()
            .packages()
            .iter()
            .enumerate()
            .filter_map(|(package_slot, package)| {
                affected_ids
                    .contains(&package.id)
                    .then_some(PackageSlot(package_slot))
            })
            .collect()
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

/// One source file replacement from an editor buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: PathBuf,
    pub text: String,
}

impl FileChange {
    pub fn new(path: impl AsRef<Path>, text: impl Into<String>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            text: text.into(),
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

    pub fn item_tree_db(&self) -> &'a ItemTreeDb {
        self.project.item_tree_db()
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
