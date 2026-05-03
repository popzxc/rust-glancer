mod rebuild;
mod update;
mod workspace_graph;

use std::path::{Path, PathBuf};

use anyhow::Context as _;

use rg_analysis::Analysis;
use rg_body_ir::BodyIrDb;
use rg_def_map::{DefMapDb, PackageSlot, TargetRef};
use rg_parse::{FileId, ParseDb};
use rg_semantic_ir::SemanticIrDb;
use rg_workspace::WorkspaceMetadata;

use self::workspace_graph::WorkspaceGraphChanges;
use crate::{Project, ProjectBuildOptions, ProjectReadTxn};

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
    pub(crate) project: Project,
}

impl AnalysisHost {
    /// Builds a host using default project build options.
    pub fn build(workspace: WorkspaceMetadata) -> anyhow::Result<Self> {
        Self::build_with_options(workspace, ProjectBuildOptions::default())
    }

    /// Builds a host using explicit project build options.
    pub fn build_with_options(
        workspace: WorkspaceMetadata,
        build_options: ProjectBuildOptions,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            project: Project::build_with_options(workspace, build_options)
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
        let changes = canonicalize_changes(changes)?;
        let graph_changes = WorkspaceGraphChanges::check(
            self.project.workspace(),
            self.project.parse_db(),
            &changes,
        );

        match graph_changes {
            WorkspaceGraphChanges::Changed => rebuild::rebuild_workspace_graph(self, &changes)
                .context("while attempting to rebuild analysis project after workspace change"),
            WorkspaceGraphChanges::Unchanged => update::apply_source_changes(self, changes),
        }
    }
}

fn canonicalize_changes(
    changes: impl IntoIterator<Item = SavedFileChange>,
) -> anyhow::Result<Vec<SavedFileChange>> {
    changes
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
        .collect()
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
    /// Starts a read transaction over resident packages and materialized cache artifacts.
    pub fn read_txn(&self) -> anyhow::Result<ProjectReadTxn<'a>> {
        self.project.read_txn()
    }

    /// Returns the high-level frozen query API.
    pub fn analysis(&self, txn: &ProjectReadTxn<'a>) -> Analysis<'a> {
        self.project.analysis(txn)
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

    /// Returns an approximate retained-memory total for the current immutable analysis graph.
    ///
    /// This is intended for observability, not correctness. Computing it walks the graph, so LSP
    /// callers should keep it behind explicit memory logging.
    pub fn retained_memory_bytes(&self) -> usize {
        use rg_memsize::MemorySize as _;

        self.project.memory_size()
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
        let txn = self.read_txn()?;
        let analysis = self.analysis(&txn);

        let mut contexts = Vec::new();

        for (package_idx, package) in self.project.parse_db().packages().iter().enumerate() {
            let package_slot = PackageSlot(package_idx);

            for parsed_file in package.parsed_files() {
                if parsed_file.path() != canonical_path.as_path() {
                    continue;
                }

                let targets = analysis.targets_for_file(package_slot, parsed_file.file_id());
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
    pub fn targets_for_file(
        &self,
        package: PackageSlot,
        file: FileId,
    ) -> anyhow::Result<Vec<TargetRef>> {
        let txn = self.read_txn()?;
        Ok(self.analysis(&txn).targets_for_file(package, file))
    }
}
