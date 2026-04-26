use std::fmt;

use anyhow::Context as _;

use crate::{
    analysis::Analysis,
    body_ir::BodyIrDb,
    def_map::DefMapDb,
    item_tree::{FileTree, ItemKind, ItemNode, ItemTreeDb, ModuleSource},
    parse::{self, ParseDb},
    semantic_ir::SemanticIrDb,
    workspace_metadata::WorkspaceMetadata,
};

/// Fully built project pipeline state.
#[derive(Debug, Clone)]
pub struct Project {
    workspace: WorkspaceMetadata,
    parse: ParseDb,
    item_tree: ItemTreeDb,
    def_map: DefMapDb,
    semantic_ir: SemanticIrDb,
    body_ir: BodyIrDb,
}

impl Project {
    /// Builds the parse, item-tree, and def-map phases for one metadata graph.
    pub fn build(workspace: WorkspaceMetadata) -> anyhow::Result<Self> {
        let mut parse = ParseDb::build(&workspace).context("while attempting to build parse db")?;
        let item_tree =
            ItemTreeDb::build(&mut parse).context("while attempting to build item tree db")?;
        let def_map = DefMapDb::build(&workspace, &parse, &item_tree)
            .context("while attempting to build def map db")?;
        let semantic_ir = SemanticIrDb::build(&item_tree, &def_map)
            .context("while attempting to build semantic ir db")?;
        let body_ir = BodyIrDb::build(&parse, &item_tree, &def_map, &semantic_ir)
            .context("while attempting to build body ir db")?;

        Ok(Self {
            workspace,
            parse,
            item_tree,
            def_map,
            semantic_ir,
            body_ir,
        })
    }

    /// Returns the parse database built for this project.
    pub(crate) fn parse_db(&self) -> &ParseDb {
        &self.parse
    }

    /// Returns the item-tree database built for this project.
    pub(crate) fn item_tree_db(&self) -> &ItemTreeDb {
        &self.item_tree
    }

    /// Returns the def-map database built for this project.
    pub(crate) fn def_map_db(&self) -> &DefMapDb {
        &self.def_map
    }

    /// Returns the semantic IR database built for this project.
    pub(crate) fn semantic_ir_db(&self) -> &SemanticIrDb {
        &self.semantic_ir
    }

    /// Returns the body IR database built for this project.
    pub(crate) fn body_ir_db(&self) -> &BodyIrDb {
        &self.body_ir
    }

    /// Returns the high-level query API for this frozen project analysis.
    #[allow(dead_code)]
    pub(crate) fn analysis(&self) -> Analysis<'_> {
        Analysis::new(self)
    }

    fn fmt_item(
        &self,
        f: &mut fmt::Formatter<'_>,
        package: &parse::Package,
        file_tree: &FileTree,
        item: &ItemNode,
        depth: usize,
    ) -> fmt::Result {
        let indent = "  ".repeat(depth);
        let name = item.name.as_deref().unwrap_or("<anonymous>");
        let file_path = package
            .file_path(item.file_id)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<unknown>".to_string());
        writeln!(
            f,
            "{indent}- {} {name} [{}] {}:{}:{}-{}:{} ({}..{})",
            item.kind,
            item.visibility,
            file_path,
            item.span.line_column.start.line + 1,
            item.span.line_column.start.column + 1,
            item.span.line_column.end.line + 1,
            item.span.line_column.end.column + 1,
            item.span.text.start,
            item.span.text.end,
        )?;

        if let ItemKind::Module(module) = &item.kind {
            if let ModuleSource::Inline { items } = &module.source {
                for child_id in items {
                    let child = file_tree
                        .item(*child_id)
                        .expect("inline child item id should exist while rendering project");
                    self.fmt_item(f, package, file_tree, child, depth + 1)?;
                }
            }
        }

        Ok(())
    }
}

impl fmt::Display for Project {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let workspace_member_count = self.workspace.workspace_packages().count();
        let dependency_count = self
            .workspace
            .packages()
            .len()
            .saturating_sub(workspace_member_count);
        writeln!(f, "Project {}", self.workspace.workspace_root().display())?;
        writeln!(
            f,
            "Packages {} (workspace members: {}, dependencies: {})",
            self.workspace.packages().len(),
            workspace_member_count,
            dependency_count,
        )?;

        let def_map_stats = self.def_map_db().stats();
        writeln!(
            f,
            "DefMaps {} targets (modules: {}, local defs: {}, impls: {}, imports: {}, unresolved imports: {})",
            def_map_stats.target_count,
            def_map_stats.module_count,
            def_map_stats.local_def_count,
            def_map_stats.local_impl_count,
            def_map_stats.import_count,
            def_map_stats.unresolved_import_count,
        )?;

        let semantic_ir_stats = self.semantic_ir_db().stats();
        writeln!(
            f,
            "SemanticIR {} targets (structs: {}, unions: {}, enums: {}, traits: {}, impls: {}, fns: {}, aliases: {}, consts: {}, statics: {})",
            semantic_ir_stats.target_count,
            semantic_ir_stats.struct_count,
            semantic_ir_stats.union_count,
            semantic_ir_stats.enum_count,
            semantic_ir_stats.trait_count,
            semantic_ir_stats.impl_count,
            semantic_ir_stats.function_count,
            semantic_ir_stats.type_alias_count,
            semantic_ir_stats.const_count,
            semantic_ir_stats.static_count,
        )?;

        let body_ir_stats = self.body_ir_db().stats();
        writeln!(
            f,
            "BodyIR {} targets (bodies: {}, scopes: {}, items: {}, bindings: {}, stmts: {}, exprs: {})",
            body_ir_stats.target_count,
            body_ir_stats.body_count,
            body_ir_stats.scope_count,
            body_ir_stats.local_item_count,
            body_ir_stats.binding_count,
            body_ir_stats.statement_count,
            body_ir_stats.expression_count,
        )?;

        for (package_slot, package) in self.parse_db().packages().iter().enumerate() {
            writeln!(f)?;
            writeln!(f, "Package {} [{}]", package.package_name(), package.id())?;

            let Some(package_trees) = self.item_tree_db().package(package_slot) else {
                continue;
            };

            for target in package_trees.target_roots() {
                let root_path = package
                    .file_path(target.root_file)
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "<unknown>".to_string());
                let parsed_target = package
                    .target(target.target)
                    .expect("target should exist while rendering project");

                writeln!(f)?;
                writeln!(
                    f,
                    "Target {} ({}) | root {}",
                    parsed_target.name, parsed_target.kind, root_path
                )?;
            }

            for file_tree in package_trees.files() {
                let file_path = package
                    .file_path(file_tree.file)
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "<unknown>".to_string());
                writeln!(f)?;
                writeln!(f, "File {file_path}")?;
                for item_id in &file_tree.top_level {
                    let item = file_tree
                        .item(*item_id)
                        .expect("top-level item id should exist while rendering project");
                    self.fmt_item(f, package, file_tree, item, 0)?;
                }
            }

            let has_errors = package
                .files
                .parsed_files()
                .iter()
                .any(|file| !file.parse_errors.is_empty());
            if has_errors {
                writeln!(f)?;
                writeln!(f, "Parser errors:")?;
                for file in package.files.parsed_files() {
                    for parse_error in &file.parse_errors {
                        writeln!(
                            f,
                            "- {}:{}:{} [{}..{}]: {}",
                            file.path.display(),
                            parse_error.span.line_column.start.line + 1,
                            parse_error.span.line_column.start.column + 1,
                            parse_error.span.text.start,
                            parse_error.span.text.end,
                            parse_error.message,
                        )?;
                    }
                }
            }
        }

        Ok(())
    }
}
