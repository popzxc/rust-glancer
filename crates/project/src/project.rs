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
    BuildProfile, BuildProfileOptions, CachedWorkspace, PackageCacheStore, PackageResidencyPlan,
    PackageResidencyPolicy, cache::integration, profile::BuildProfiler, txn::ProjectReadTxn,
};

/// Configuration that affects how a project snapshot is built and retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProjectBuildOptions {
    pub body_ir_policy: BodyIrBuildPolicy,
    pub package_residency_policy: PackageResidencyPolicy,
}

/// Fully built project pipeline state.
#[derive(Debug, Clone)]
pub struct Project {
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

impl Project {
    /// Builds every analysis phase for one metadata graph.
    pub fn build(workspace: WorkspaceMetadata) -> anyhow::Result<Self> {
        Self::build_with_options(workspace, ProjectBuildOptions::default())
    }

    /// Builds every analysis phase using explicit project build options.
    pub fn build_with_options(
        workspace: WorkspaceMetadata,
        build_options: ProjectBuildOptions,
    ) -> anyhow::Result<Self> {
        let mut profiler = BuildProfiler::disabled();
        let (names, parse, def_map, semantic_ir, body_ir) =
            Self::build_phases(&workspace, build_options.body_ir_policy, &mut profiler)?;
        let package_residency =
            PackageResidencyPlan::build(&workspace, build_options.package_residency_policy);
        let cached_workspace = CachedWorkspace::build(&workspace, &parse);
        let cache_store = PackageCacheStore::for_workspace(&workspace);

        let mut project = Self {
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
        };
        integration::apply_residency(&mut project)
            .context("while attempting to apply package cache residency")?;

        Ok(project)
    }

    /// Builds every analysis phase and returns coarse build-time profiling checkpoints.
    pub fn build_profiled(
        workspace: WorkspaceMetadata,
        build_options: ProjectBuildOptions,
        options: BuildProfileOptions,
    ) -> anyhow::Result<(Self, BuildProfile)> {
        let mut profiler = BuildProfiler::new(options);
        let (names, parse, def_map, semantic_ir, body_ir) =
            Self::build_phases(&workspace, build_options.body_ir_policy, &mut profiler)?;
        let package_residency =
            PackageResidencyPlan::build(&workspace, build_options.package_residency_policy);
        let cached_workspace = CachedWorkspace::build(&workspace, &parse);
        let cache_store = PackageCacheStore::for_workspace(&workspace);

        let mut project = Self {
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
        };
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

        integration::materialize_project(self)
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
    pub fn workspace(&self) -> &WorkspaceMetadata {
        &self.workspace
    }

    /// Returns package residency decisions for this project snapshot.
    pub fn package_residency_plan(&self) -> &PackageResidencyPlan {
        &self.package_residency
    }

    /// Returns the parse database built for this project.
    pub fn parse_db(&self) -> &ParseDb {
        &self.parse
    }

    pub(crate) fn parse_db_mut(&mut self) -> &mut ParseDb {
        &mut self.parse
    }

    /// Returns the def-map database built for this project.
    pub fn def_map_db(&self) -> &DefMapDb {
        &self.def_map
    }

    /// Returns the semantic IR database built for this project.
    pub fn semantic_ir_db(&self) -> &SemanticIrDb {
        &self.semantic_ir
    }

    /// Returns the body IR database built for this project.
    pub fn body_ir_db(&self) -> &BodyIrDb {
        &self.body_ir
    }

    /// Starts a read transaction over resident packages and materialized cache artifacts.
    pub fn read_txn(&self) -> anyhow::Result<ProjectReadTxn<'_>> {
        ProjectReadTxn::new(self)
    }

    /// Returns the high-level query API for this frozen project analysis.
    #[allow(dead_code)]
    pub fn analysis<'a>(&self, txn: &ProjectReadTxn<'a>) -> Analysis<'a> {
        Analysis::new(txn.analysis())
    }
}
