use std::path::Path;

use anyhow::Context as _;

use rg_analysis::Analysis;
use rg_def_map::{PackageSlot, TargetRef};
use rg_parse::{FileId, ParseDb};

use super::{
    FileContext, demand::PackageDemand, inventory::ProjectInventory, state::ProjectState,
    stats::ProjectStats,
};

/// Immutable project view used to answer LSP-shaped queries.
#[derive(Debug, Clone, Copy)]
pub struct ProjectSnapshot<'a> {
    pub(super) state: &'a ProjectState,
}

impl<'a> ProjectSnapshot<'a> {
    /// Returns a full-project analysis view.
    pub fn full_analysis(&self) -> anyhow::Result<Analysis<'a>> {
        let txn = self.state.read_txn()?;
        Ok(self.state.analysis(&txn))
    }

    /// Returns an analysis view scoped to the package dependency closure of target queries.
    pub fn analysis_for_targets(&self, targets: &[TargetRef]) -> anyhow::Result<Analysis<'a>> {
        let demand = PackageDemand::targets(self.state.workspace(), targets);
        let txn = self.state.read_txn_for_demand(&demand)?;
        Ok(self.state.analysis(&txn))
    }

    /// Returns an analysis view over exactly the listed packages, without dependency expansion.
    ///
    /// This is only suitable for package-local metadata queries such as target/file ownership.
    /// Semantic queries should use a target-scoped analysis so dependencies are materialized too.
    pub(crate) fn shallow_analysis(
        &self,
        packages: &[PackageSlot],
    ) -> anyhow::Result<Analysis<'a>> {
        let demand = PackageDemand::package_slots(self.state.workspace(), packages);
        let txn = self.state.read_txn_for_demand(&demand)?;
        Ok(self.state.analysis(&txn))
    }

    pub fn parse_db(&self) -> &'a ParseDb {
        self.state.parse_db()
    }

    pub(crate) fn inventory(&self) -> ProjectInventory<'a> {
        self.state.inventory()
    }

    pub fn stats(&self) -> ProjectStats {
        self.state.stats()
    }

    /// Returns an approximate retained-memory total for the current immutable analysis graph.
    ///
    /// This is intended for observability, not correctness. Computing it walks the graph, so LSP
    /// callers should keep it behind explicit memory logging.
    pub fn retained_memory_bytes(&self) -> usize {
        use rg_memsize::MemorySize as _;

        self.state.memory_size()
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
        let candidates = self.inventory().file_refs_for_path(&canonical_path);

        let package_slots = candidates
            .iter()
            .map(|file| file.package)
            .collect::<Vec<_>>();
        let analysis = self.shallow_analysis(&package_slots)?;
        let mut contexts = Vec::new();

        for file in candidates {
            let targets = analysis.targets_for_file(file.package, file.file);
            if targets.is_empty() {
                continue;
            }

            contexts.push(FileContext {
                package: file.package,
                file: file.file,
                targets,
            });
        }

        Ok(contexts)
    }

    /// Returns target contexts whose module tree contains a package-local file.
    pub fn targets_for_file(
        &self,
        package: PackageSlot,
        file: FileId,
    ) -> anyhow::Result<Vec<TargetRef>> {
        let analysis = self.shallow_analysis(&[package])?;
        Ok(analysis.targets_for_file(package, file))
    }
}
