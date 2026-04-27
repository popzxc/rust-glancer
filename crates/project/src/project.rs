use anyhow::Context as _;

use rg_analysis::Analysis;
use rg_body_ir::{BodyIrBuildPolicy, BodyIrDb};
use rg_def_map::DefMapDb;
use rg_item_tree::ItemTreeDb;
use rg_parse::ParseDb;
use rg_semantic_ir::SemanticIrDb;
use rg_workspace::WorkspaceMetadata;

/// Fully built project pipeline state.
#[derive(Debug, Clone)]
pub struct Project {
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
        let mut parse = ParseDb::build(&workspace).context("while attempting to build parse db")?;
        let item_tree =
            ItemTreeDb::build(&mut parse).context("while attempting to build item tree db")?;
        let def_map = DefMapDb::build(&workspace, &parse, &item_tree)
            .context("while attempting to build def map db")?;
        let semantic_ir = SemanticIrDb::build(&item_tree, &def_map)
            .context("while attempting to build semantic ir db")?;
        let body_ir =
            BodyIrDb::build_with_policy(&parse, &item_tree, &def_map, &semantic_ir, body_ir_policy)
                .context("while attempting to build body ir db")?;

        Ok(Self {
            parse,
            item_tree,
            def_map,
            semantic_ir,
            body_ir,
        })
    }

    /// Returns the parse database built for this project.
    pub fn parse_db(&self) -> &ParseDb {
        &self.parse
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
