use anyhow::Context as _;

use rg_analysis::Analysis;
use rg_body_ir::{BodyIrBuildPolicy, BodyIrDb};
use rg_def_map::{DefMapDb, PackageSlot};
use rg_item_tree::ItemTreeDb;
use rg_parse::ParseDb;
use rg_semantic_ir::SemanticIrDb;
use rg_text::NameInterner;
use rg_workspace::WorkspaceMetadata;

use crate::{BuildProfile, BuildProfileOptions, profile::BuildProfiler};

/// Fully built project pipeline state.
#[derive(Debug, Clone)]
pub struct Project {
    pub(crate) workspace: WorkspaceMetadata,
    pub(crate) body_ir_policy: BodyIrBuildPolicy,
    pub(crate) names: NameInterner,
    pub(crate) parse: ParseDb,
    pub(crate) def_map: DefMapDb,
    pub(crate) semantic_ir: SemanticIrDb,
    pub(crate) body_ir: BodyIrDb,
}

impl Project {
    /// Builds every analysis phase for one metadata graph.
    pub fn build(workspace: WorkspaceMetadata) -> anyhow::Result<Self> {
        Self::build_with_body_ir_policy(workspace, BodyIrBuildPolicy::default())
    }

    /// Builds every analysis phase using an explicit Body IR lowering policy.
    pub fn build_with_body_ir_policy(
        workspace: WorkspaceMetadata,
        body_ir_policy: BodyIrBuildPolicy,
    ) -> anyhow::Result<Self> {
        let mut profiler = BuildProfiler::disabled();
        let (names, parse, def_map, semantic_ir, body_ir) =
            Self::build_phases(&workspace, body_ir_policy, &mut profiler)?;

        Ok(Self {
            workspace,
            body_ir_policy,
            names,
            parse,
            def_map,
            semantic_ir,
            body_ir,
        })
    }

    /// Builds every analysis phase and returns coarse build-time profiling checkpoints.
    pub fn build_profiled(
        workspace: WorkspaceMetadata,
        options: BuildProfileOptions,
    ) -> anyhow::Result<(Self, BuildProfile)> {
        Self::build_profiled_with_body_ir_policy(workspace, BodyIrBuildPolicy::default(), options)
    }

    /// Builds every analysis phase with explicit Body IR policy and profiling options.
    pub fn build_profiled_with_body_ir_policy(
        workspace: WorkspaceMetadata,
        body_ir_policy: BodyIrBuildPolicy,
        options: BuildProfileOptions,
    ) -> anyhow::Result<(Self, BuildProfile)> {
        let mut profiler = BuildProfiler::new(options);
        let (names, parse, def_map, semantic_ir, body_ir) =
            Self::build_phases(&workspace, body_ir_policy, &mut profiler)?;

        let project = Self {
            workspace,
            body_ir_policy,
            names,
            parse,
            def_map,
            semantic_ir,
            body_ir,
        };
        let resident_bytes = profiler.sample_resident_memory();
        let project_bytes = profiler.measure(&project);
        profiler.record(
            "after project",
            project_bytes,
            project_bytes,
            resident_bytes,
        );

        Ok((project, profiler.finish()))
    }

    pub(crate) fn rebuild_packages(&mut self, packages: &[PackageSlot]) -> anyhow::Result<()> {
        if packages.is_empty() {
            return Ok(());
        }

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
                self.body_ir_policy,
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

        Ok(())
    }

    fn build_phases(
        workspace: &WorkspaceMetadata,
        body_ir_policy: BodyIrBuildPolicy,
        profiler: &mut BuildProfiler,
    ) -> anyhow::Result<(NameInterner, ParseDb, DefMapDb, SemanticIrDb, BodyIrDb)> {
        let mut names = NameInterner::new();
        let mut parse = ParseDb::build(workspace).context("while attempting to build parse db")?;
        let resident_bytes = profiler.sample_resident_memory();
        let parse_bytes = profiler.measure(&parse);
        profiler.record("after parse", parse_bytes, parse_bytes, resident_bytes);

        let item_tree = ItemTreeDb::build_with_interner(&mut parse, &mut names)
            .context("while attempting to build item tree db")?;
        let resident_bytes = profiler.sample_resident_memory();
        let names_bytes = profiler.measure(&names);
        let parse_bytes = profiler.measure(&parse);
        let item_tree_bytes = profiler.measure(&item_tree);
        profiler.record(
            "after item-tree",
            item_tree_bytes,
            profiler.sum_retained(&[names_bytes, parse_bytes, item_tree_bytes]),
            resident_bytes,
        );

        let def_map = DefMapDb::build_with_interner(workspace, &parse, &item_tree, &mut names)
            .context("while attempting to build def map db")?;
        let resident_bytes = profiler.sample_resident_memory();
        let names_bytes = profiler.measure(&names);
        let def_map_bytes = profiler.measure(&def_map);
        profiler.record(
            "after def-map",
            def_map_bytes,
            profiler.sum_retained(&[names_bytes, parse_bytes, item_tree_bytes, def_map_bytes]),
            resident_bytes,
        );

        let semantic_ir = SemanticIrDb::build(&item_tree, &def_map)
            .context("while attempting to build semantic ir db")?;
        let resident_bytes = profiler.sample_resident_memory();
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
            resident_bytes,
        );

        // ItemTree is a lowering input, not retained project state. Dropping it here makes the
        // following process-only checkpoint useful for separating transient build pressure from
        // final retained memory.
        drop(item_tree);
        let resident_bytes = profiler.sample_resident_memory();
        let names_bytes = profiler.measure(&names);
        profiler.record(
            "after item-tree drop",
            None,
            profiler.sum_retained(&[names_bytes, parse_bytes, def_map_bytes, semantic_ir_bytes]),
            resident_bytes,
        );

        let body_ir = BodyIrDb::build_with_policy_and_interner(
            &parse,
            &def_map,
            &semantic_ir,
            body_ir_policy,
            &mut names,
        )
        .context("while attempting to build body ir db")?;
        let resident_bytes = profiler.sample_resident_memory();
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
            resident_bytes,
        );

        parse.evict_syntax_trees();
        parse.shrink_to_fit();
        let resident_bytes = profiler.sample_resident_memory();
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
            resident_bytes,
        );

        Ok((names, parse, def_map, semantic_ir, body_ir))
    }

    /// Returns the normalized workspace metadata this project was built from.
    pub fn workspace(&self) -> &WorkspaceMetadata {
        &self.workspace
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

    /// Returns the high-level query API for this frozen project analysis.
    #[allow(dead_code)]
    pub fn analysis(&self) -> Analysis<'_> {
        Analysis::new(self.def_map_db(), self.semantic_ir_db(), self.body_ir_db())
    }
}
