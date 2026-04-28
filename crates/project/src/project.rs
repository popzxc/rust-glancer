use anyhow::Context as _;

use rg_analysis::Analysis;
use rg_body_ir::{BodyIrBuildPolicy, BodyIrDb};
use rg_def_map::{DefMapDb, PackageSlot};
use rg_item_tree::ItemTreeDb;
use rg_parse::ParseDb;
use rg_semantic_ir::SemanticIrDb;
use rg_workspace::WorkspaceMetadata;

/// Fully built project pipeline state.
#[derive(Debug, Clone)]
pub struct Project {
    workspace: WorkspaceMetadata,
    body_ir_policy: BodyIrBuildPolicy,
    parse: ParseDb,
    item_tree: ItemTreeDb,
    def_map: DefMapDb,
    semantic_ir: SemanticIrDb,
    body_ir: BodyIrDb,
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
        let (parse, item_tree, def_map, semantic_ir, body_ir) =
            Self::build_phases(&workspace, body_ir_policy)?;

        Ok(Self {
            workspace,
            body_ir_policy,
            parse,
            item_tree,
            def_map,
            semantic_ir,
            body_ir,
        })
    }

    pub(crate) fn rebuild_packages(&mut self, packages: &[PackageSlot]) -> anyhow::Result<()> {
        if packages.is_empty() {
            return Ok(());
        }

        let package_indices = packages.iter().map(|package| package.0).collect::<Vec<_>>();
        let item_tree = self
            .item_tree
            .rebuild_packages(&mut self.parse, &package_indices)
            .context("while attempting to rebuild affected item-tree packages")?;
        let def_map = self
            .def_map
            .rebuild_packages(&self.workspace, &self.parse, &item_tree, packages)
            .context("while attempting to rebuild affected def-map packages")?;
        let semantic_ir = self
            .semantic_ir
            .rebuild_packages(&item_tree, &def_map, packages)
            .context("while attempting to rebuild affected semantic IR packages")?;
        let body_ir = self
            .body_ir
            .rebuild_packages(
                &self.parse,
                &item_tree,
                &def_map,
                &semantic_ir,
                self.body_ir_policy,
                packages,
            )
            .context("while attempting to rebuild affected body IR packages")?;

        self.item_tree = item_tree;
        self.def_map = def_map;
        self.semantic_ir = semantic_ir;
        self.body_ir = body_ir;

        Ok(())
    }

    fn build_phases(
        workspace: &WorkspaceMetadata,
        body_ir_policy: BodyIrBuildPolicy,
    ) -> anyhow::Result<(ParseDb, ItemTreeDb, DefMapDb, SemanticIrDb, BodyIrDb)> {
        let mut parse = ParseDb::build(workspace).context("while attempting to build parse db")?;
        let item_tree =
            ItemTreeDb::build(&mut parse).context("while attempting to build item tree db")?;
        let def_map = DefMapDb::build(workspace, &parse, &item_tree)
            .context("while attempting to build def map db")?;
        let semantic_ir = SemanticIrDb::build(&item_tree, &def_map)
            .context("while attempting to build semantic ir db")?;
        let body_ir =
            BodyIrDb::build_with_policy(&parse, &item_tree, &def_map, &semantic_ir, body_ir_policy)
                .context("while attempting to build body ir db")?;

        Ok((parse, item_tree, def_map, semantic_ir, body_ir))
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

    /// Returns the item-tree database built for this project.
    pub fn item_tree_db(&self) -> &ItemTreeDb {
        &self.item_tree
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
