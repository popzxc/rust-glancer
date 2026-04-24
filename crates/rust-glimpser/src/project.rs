use std::fmt;

use anyhow::Context as _;

#[cfg(test)]
use crate::def_map::{DefMap, TargetRef};
use crate::{
    def_map::DefMapDb,
    item_tree::{ItemKind, ItemNode, ItemTreeDb, ModuleSource},
    parse::{self, ParseDb},
    workspace_metadata::WorkspaceMetadata,
};

/// Fully built project pipeline state.
#[derive(Debug, Clone)]
pub struct Project {
    workspace: WorkspaceMetadata,
    parse: ParseDb,
    item_tree: ItemTreeDb,
    def_map: DefMapDb,
}

impl Project {
    /// Builds the parse, item-tree, and def-map phases for one metadata graph.
    pub fn build(workspace: WorkspaceMetadata) -> anyhow::Result<Self> {
        let mut parse = ParseDb::build(&workspace).context("while attempting to build parse db")?;
        let item_tree =
            ItemTreeDb::build(&mut parse).context("while attempting to build item tree db")?;
        let def_map = DefMapDb::build(&workspace, &parse, &item_tree)
            .context("while attempting to build def map db")?;

        Ok(Self {
            workspace,
            parse,
            item_tree,
            def_map,
        })
    }

    /// Returns all parsed packages.
    #[cfg(test)]
    pub(crate) fn packages(&self) -> &[parse::Package] {
        self.parse.packages()
    }

    /// Returns lowered item trees for test snapshot rendering.
    #[cfg(test)]
    pub(crate) fn item_tree(&self) -> &ItemTreeDb {
        &self.item_tree
    }

    /// Returns the def map for one project-wide target reference.
    #[cfg(test)]
    pub(crate) fn def_map(&self, target: TargetRef) -> Option<&DefMap> {
        self.def_map.def_map(target)
    }

    fn fmt_item(
        &self,
        f: &mut fmt::Formatter<'_>,
        package: &parse::Package,
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
                for child in items {
                    self.fmt_item(f, package, child, depth + 1)?;
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

        let def_map_stats = self.def_map.stats();
        writeln!(
            f,
            "DefMaps {} targets (modules: {}, local defs: {}, imports: {}, unresolved imports: {})",
            def_map_stats.target_count,
            def_map_stats.module_count,
            def_map_stats.local_def_count,
            def_map_stats.import_count,
            def_map_stats.unresolved_import_count,
        )?;

        for (package_slot, package) in self.parse.packages().iter().enumerate() {
            writeln!(f)?;
            writeln!(f, "Package {} [{}]", package.package_name(), package.id())?;

            let Some(package_trees) = self.item_tree.package(package_slot) else {
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
                for item in &file_tree.items {
                    self.fmt_item(f, package, item, 0)?;
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
