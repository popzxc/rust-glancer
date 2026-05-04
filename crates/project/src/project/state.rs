use anyhow::Context as _;

use rg_analysis::Analysis;
use rg_body_ir::{BodyIrBuildPolicy, BodyIrDb};
use rg_def_map::{DefMapDb, PackageSlot};
use rg_item_tree::ItemTreeDb;
use rg_parse::ParseDb;
use rg_semantic_ir::SemanticIrDb;
use rg_text::NameInterner;
use rg_workspace::WorkspaceMetadata;

use crate::{
    BuildProfile, BuildProfileOptions, PackageResidencyPlan, PackageResidencyPolicy,
    cache::{CachedWorkspace, PackageCacheStore, integration},
    profile::BuildProfiler,
};

use super::{
    demand::PackageDemand, inventory::ProjectInventory, stats::ProjectStats, txn::ProjectReadTxn,
};

/// Configuration that affects how a project snapshot is built and retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProjectBuildOptions {
    pub body_ir_policy: BodyIrBuildPolicy,
    pub package_residency_policy: PackageResidencyPolicy,
}

/// Fully built project pipeline state.
#[derive(Debug, Clone)]
pub(crate) struct ProjectState {
    pub(crate) workspace: WorkspaceMetadata,
    pub(crate) cached_workspace: CachedWorkspace,
    pub(crate) cache_store: PackageCacheStore,
    pub(crate) build_options: ProjectBuildOptions,
    pub(crate) package_residency: PackageResidencyPlan,
    pub(crate) names: NameInterner,
    pub(crate) parse: ParseDb,
    pub(crate) def_map: DefMapDb,
    pub(crate) semantic_ir: SemanticIrDb,
    pub(crate) body_ir: BodyIrDb,
}

impl ProjectState {
    /// Builds every analysis phase using explicit project build options.
    pub(crate) fn build_with_options(
        workspace: WorkspaceMetadata,
        build_options: ProjectBuildOptions,
    ) -> anyhow::Result<Self> {
        let mut profiler = BuildProfiler::disabled();
        let mut project =
            Self::build_resident_with_profiler(workspace, build_options, &mut profiler)
                .context("while attempting to build resident analysis project")?;
        integration::apply_residency(&mut project)
            .context("while attempting to apply package cache residency")?;

        Ok(project)
    }

    /// Builds every analysis phase and returns coarse build-time profiling checkpoints.
    pub(crate) fn build_profiled(
        workspace: WorkspaceMetadata,
        build_options: ProjectBuildOptions,
        options: BuildProfileOptions,
    ) -> anyhow::Result<(Self, BuildProfile)> {
        let mut profiler = BuildProfiler::new(options);
        let mut project =
            Self::build_resident_with_profiler(workspace, build_options, &mut profiler)
                .context("while attempting to build resident analysis project")?;
        integration::apply_residency(&mut project)
            .context("while attempting to apply package cache residency")?;

        let process_memory = profiler.sample_process_memory();
        let project_bytes = profiler.measure(&project);
        profiler.record(
            "after project",
            project_bytes,
            project_bytes,
            process_memory,
        );

        Ok((project, profiler.finish()))
    }

    pub(crate) fn rebuild_packages(&mut self, packages: &[PackageSlot]) -> anyhow::Result<()> {
        if packages.is_empty() {
            return Ok(());
        }

        // Rebuilding one package can resolve names through its dependencies, but unrelated
        // packages should stay offloaded so save handling does not recreate full-project spikes.
        let materialized_packages =
            PackageDemand::packages_with_dependencies(&self.workspace, packages);
        integration::materialize_packages(self, &materialized_packages)
            .context("while attempting to materialize package cache before package rebuild")?;

        let package_indices = packages.iter().map(|package| package.0).collect::<Vec<_>>();
        let item_tree = ItemTreeDb::build_packages_with_interner(
            &mut self.parse,
            &package_indices,
            &mut self.names,
        )
        .context("while attempting to rebuild affected item-tree packages")?;
        let def_map = self
            .def_map
            .rebuild_packages_with_interner(
                &self.workspace,
                &self.parse,
                &item_tree,
                packages,
                &mut self.names,
            )
            .context("while attempting to rebuild affected def-map packages")?;
        let semantic_ir = self
            .semantic_ir
            .rebuild_packages(&item_tree, &def_map, packages)
            .context("while attempting to rebuild affected semantic IR packages")?;
        let body_ir = self
            .body_ir
            .rebuild_packages_with_interner(
                &self.parse,
                &def_map,
                &semantic_ir,
                self.build_options.body_ir_policy,
                packages,
                &mut self.names,
            )
            .context("while attempting to rebuild affected body IR packages")?;

        // ItemTree is a transient rebuild input. Drop it before pruning the weak interner so names
        // that did not survive into retained DBs are no longer treated as live.
        drop(item_tree);

        self.parse.evict_syntax_trees();
        self.parse.shrink_to_fit();
        self.def_map = def_map;
        self.semantic_ir = semantic_ir;
        self.body_ir = body_ir;
        self.names.shrink_to_fit();
        integration::restore_residency_after_rebuild(self, packages)
            .context("while attempting to apply package cache residency after package rebuild")?;

        Ok(())
    }

    /// Replaces the project with a fully resident rebuild from the same workspace metadata.
    ///
    /// This is the mutable cache recovery path: after an artifact disappears or becomes invalid,
    /// package rebuilds need source-built phase payloads before residency can be restored.
    pub(crate) fn rebuild_resident_from_source(&mut self) -> anyhow::Result<()> {
        let workspace = self.workspace.clone();
        let build_options = self.build_options;
        let cache_store = self.cache_store.clone();
        let mut profiler = BuildProfiler::disabled();
        let mut rebuilt =
            Self::build_resident_with_profiler(workspace, build_options, &mut profiler)
                .context("while attempting to rebuild resident analysis project")?;

        // Keep the original cache namespace. Recovery can happen while the process is alive, and
        // the environment that selected the target directory may have changed since initialization.
        rebuilt.cache_store = cache_store;
        *self = rebuilt;

        Ok(())
    }

    /// Builds all analysis phases without applying the package residency policy.
    ///
    /// Cache recovery needs a fully resident project first, because writing replacement artifacts
    /// requires the retained phase payloads to be available in memory.
    pub(crate) fn build_resident_with_options(
        workspace: WorkspaceMetadata,
        build_options: ProjectBuildOptions,
    ) -> anyhow::Result<Self> {
        let mut profiler = BuildProfiler::disabled();
        Self::build_resident_with_profiler(workspace, build_options, &mut profiler)
    }

    fn build_resident_with_profiler(
        workspace: WorkspaceMetadata,
        build_options: ProjectBuildOptions,
        profiler: &mut BuildProfiler,
    ) -> anyhow::Result<Self> {
        let (names, parse, def_map, semantic_ir, body_ir) =
            Self::build_phases(&workspace, build_options.body_ir_policy, profiler)?;
        let package_residency =
            PackageResidencyPlan::build(&workspace, build_options.package_residency_policy);
        let cached_workspace = CachedWorkspace::build(&workspace, &parse);
        let cache_store = PackageCacheStore::for_workspace(&workspace, &cached_workspace);

        Ok(Self {
            workspace,
            cached_workspace,
            cache_store,
            build_options,
            package_residency,
            names,
            parse,
            def_map,
            semantic_ir,
            body_ir,
        })
    }

    fn build_phases(
        workspace: &WorkspaceMetadata,
        body_ir_policy: BodyIrBuildPolicy,
        profiler: &mut BuildProfiler,
    ) -> anyhow::Result<(NameInterner, ParseDb, DefMapDb, SemanticIrDb, BodyIrDb)> {
        let mut names = NameInterner::new();
        let mut parse = ParseDb::build(workspace).context("while attempting to build parse db")?;
        let process_memory = profiler.sample_process_memory();
        let parse_bytes = profiler.measure(&parse);
        profiler.record("after parse", parse_bytes, parse_bytes, process_memory);

        let item_tree = ItemTreeDb::build_with_interner(&mut parse, &mut names)
            .context("while attempting to build item tree db")?;
        let process_memory = profiler.sample_process_memory();
        let names_bytes = profiler.measure(&names);
        let parse_bytes = profiler.measure(&parse);
        let item_tree_bytes = profiler.measure(&item_tree);
        profiler.record(
            "after item-tree",
            item_tree_bytes,
            profiler.sum_retained(&[names_bytes, parse_bytes, item_tree_bytes]),
            process_memory,
        );

        let def_map = DefMapDb::build_with_interner(workspace, &parse, &item_tree, &mut names)
            .context("while attempting to build def map db")?;
        let process_memory = profiler.sample_process_memory();
        let names_bytes = profiler.measure(&names);
        let def_map_bytes = profiler.measure(&def_map);
        profiler.record(
            "after def-map",
            def_map_bytes,
            profiler.sum_retained(&[names_bytes, parse_bytes, item_tree_bytes, def_map_bytes]),
            process_memory,
        );

        let semantic_ir = SemanticIrDb::build(&item_tree, &def_map)
            .context("while attempting to build semantic ir db")?;
        let process_memory = profiler.sample_process_memory();
        let names_bytes = profiler.measure(&names);
        let semantic_ir_bytes = profiler.measure(&semantic_ir);
        profiler.record(
            "after semantic-ir",
            semantic_ir_bytes,
            profiler.sum_retained(&[
                names_bytes,
                parse_bytes,
                item_tree_bytes,
                def_map_bytes,
                semantic_ir_bytes,
            ]),
            process_memory,
        );

        // ItemTree is a lowering input, not retained project state. Dropping it here makes the
        // following process-only checkpoint useful for separating transient build pressure from
        // final retained memory.
        drop(item_tree);
        let process_memory = profiler.sample_process_memory();
        let names_bytes = profiler.measure(&names);
        profiler.record(
            "after item-tree drop",
            None,
            profiler.sum_retained(&[names_bytes, parse_bytes, def_map_bytes, semantic_ir_bytes]),
            process_memory,
        );

        let body_ir = BodyIrDb::build_with_policy_and_interner(
            &parse,
            &def_map,
            &semantic_ir,
            body_ir_policy,
            &mut names,
        )
        .context("while attempting to build body ir db")?;
        let process_memory = profiler.sample_process_memory();
        let names_bytes = profiler.measure(&names);
        let body_ir_bytes = profiler.measure(&body_ir);
        profiler.record(
            "after body-ir",
            body_ir_bytes,
            profiler.sum_retained(&[
                names_bytes,
                parse_bytes,
                def_map_bytes,
                semantic_ir_bytes,
                body_ir_bytes,
            ]),
            process_memory,
        );

        parse.evict_syntax_trees();
        parse.shrink_to_fit();
        let process_memory = profiler.sample_process_memory();
        names.shrink_to_fit();
        let names_bytes = profiler.measure(&names);
        let parse_bytes = profiler.measure(&parse);
        profiler.record(
            "after parse syntax eviction",
            parse_bytes,
            profiler.sum_retained(&[
                names_bytes,
                parse_bytes,
                def_map_bytes,
                semantic_ir_bytes,
                body_ir_bytes,
            ]),
            process_memory,
        );

        Ok((names, parse, def_map, semantic_ir, body_ir))
    }

    /// Returns the normalized workspace metadata this project was built from.
    pub(crate) fn workspace(&self) -> &WorkspaceMetadata {
        &self.workspace
    }

    /// Returns package residency decisions for this project snapshot.
    pub(crate) fn package_residency_plan(&self) -> &PackageResidencyPlan {
        &self.package_residency
    }

    /// Returns the parse database built for this project.
    pub(crate) fn parse_db(&self) -> &ParseDb {
        &self.parse
    }

    /// Returns residency-independent package, target, and parsed-file metadata.
    pub(crate) fn inventory(&self) -> ProjectInventory<'_> {
        ProjectInventory::new(&self.workspace, &self.parse)
    }

    /// Returns coarse status counters without exposing raw phase databases.
    pub(crate) fn stats(&self) -> ProjectStats {
        ProjectStats::capture(self)
    }

    pub(crate) fn parse_db_mut(&mut self) -> &mut ParseDb {
        &mut self.parse
    }

    /// Starts a read transaction over resident packages and materialized cache artifacts.
    pub(crate) fn read_txn(&self) -> anyhow::Result<ProjectReadTxn<'_>> {
        ProjectReadTxn::new(self)
    }

    pub(crate) fn read_txn_for_demand(
        &self,
        demand: &PackageDemand,
    ) -> anyhow::Result<ProjectReadTxn<'_>> {
        ProjectReadTxn::for_demand(self, demand)
    }

    /// Returns the high-level query API for this frozen project analysis.
    #[allow(dead_code)]
    pub(crate) fn analysis<'a>(&self, txn: &ProjectReadTxn<'a>) -> Analysis<'a> {
        Analysis::new(txn.analysis())
    }
}
