use std::fmt;

use anyhow::Context as _;

#[cfg(test)]
use crate::def_map::{DefMap, TargetRef};
use crate::{
    def_map::DefMapDb,
    item_tree::{ItemNode, ItemTreeDb},
    parse::{self, ParseDb},
};

/// Fully built project pipeline state.
#[derive(Debug, Clone)]
pub struct Project {
    pub parse: ParseDb,
    pub item_tree: ItemTreeDb,
    pub def_map: DefMapDb,
}

impl Project {
    /// Builds the parse, item-tree, and def-map phases for one metadata graph.
    pub fn build(metadata: cargo_metadata::Metadata) -> anyhow::Result<Self> {
        let mut parse = ParseDb::build(metadata).context("while attempting to build parse db")?;
        let item_tree =
            ItemTreeDb::build(&mut parse).context("while attempting to build item tree db")?;
        let def_map =
            DefMapDb::build(&mut parse).context("while attempting to build def map db")?;

        Ok(Self {
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

        for child in &item.children {
            self.fmt_item(f, package, child, depth + 1)?;
        }

        Ok(())
    }
}

impl fmt::Display for Project {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let workspace_member_count = self.parse.workspace_packages().count();
        let dependency_count = self
            .parse
            .packages()
            .len()
            .saturating_sub(workspace_member_count);
        writeln!(f, "Project {}", self.parse.metadata().workspace_root)?;
        writeln!(
            f,
            "Packages {} (workspace members: {}, dependencies: {})",
            self.parse.packages().len(),
            workspace_member_count,
            dependency_count,
        )?;

        for (package_slot, package) in self.parse.packages().iter().enumerate() {
            writeln!(f)?;
            writeln!(f, "Package {} [{}]", package.package_name(), package.id())?;

            let Some(package_trees) = self.item_tree.package(package_slot) else {
                continue;
            };

            for target in package_trees.targets() {
                let root_path = package
                    .file_path(
                        package
                            .target(target.target)
                            .expect("target should exist while rendering project")
                            .root_file,
                    )
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "<unknown>".to_string());
                let parsed_target = package
                    .target(target.target)
                    .expect("target should exist while rendering project");
                let kinds = if !parsed_target.cargo_target.kind.is_empty() {
                    format!("{:?}", parsed_target.cargo_target.kind)
                } else {
                    "<unknown>".to_string()
                };

                writeln!(f)?;
                writeln!(
                    f,
                    "Target {} ({kinds}) | root {}",
                    parsed_target.cargo_target.name, root_path
                )?;
                for item in &target.root_items {
                    self.fmt_item(f, package, item, 0)?;
                }
            }

            let has_errors = package
                .files
                .parsed_files
                .iter()
                .any(|file| !file.parse_errors.is_empty());
            if has_errors {
                writeln!(f)?;
                writeln!(f, "Parser errors:")?;
                for file in &package.files.parsed_files {
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
