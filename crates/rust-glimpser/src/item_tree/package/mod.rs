use anyhow::Context as _;

use std::{fmt, path::Path};

use crate::item_tree::{
    file::{FileId, FileRecord},
    item::ItemNode,
    target::{TargetId, TargetIndex, TargetInput},
};

use self::builder::PackageBuilder;

mod builder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageIndex {
    pub package_name: String,
    pub files: Vec<FileRecord>,
    pub targets: Vec<TargetIndex>,
}

impl PackageIndex {
    pub fn file_path(&self, file_id: FileId) -> Option<&Path> {
        self.files.get(file_id.0).map(|file| file.path.as_path())
    }

    pub fn build(package_name: String, targets: Vec<TargetInput>) -> anyhow::Result<Self> {
        let mut builder = PackageBuilder::default();
        let mut target_indexes = Vec::new();

        for (ordinal, target_input) in targets.into_iter().enumerate() {
            let target_id = TargetId(ordinal);
            let target_index = builder
                .build_target(target_id, target_input)
                .with_context(|| format!("while attempting to build target index {ordinal}"))?;
            target_indexes.push(target_index);
        }

        let files = builder
            .parsed_files
            .into_iter()
            .map(|parsed_file| parsed_file.record)
            .collect();

        Ok(Self {
            package_name,
            files,
            targets: target_indexes,
        })
    }

    fn fmt_item(&self, f: &mut fmt::Formatter<'_>, item: &ItemNode, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        let name = item.name.as_deref().unwrap_or("<anonymous>");
        let file_path = self
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
            self.fmt_item(f, child, depth + 1)?;
        }

        Ok(())
    }
}

impl fmt::Display for PackageIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Package {}", self.package_name)?;

        for target in &self.targets {
            let root_path = self
                .file_path(target.root_file)
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<unknown>".to_string());
            let kinds = if target.kinds.is_empty() {
                "<unknown>".to_string()
            } else {
                target.kinds.join(",")
            };

            writeln!(f)?;
            writeln!(f, "Target {} ({kinds}) | root {}", target.name, root_path)?;
            for item in &target.root_items {
                self.fmt_item(f, item, 0)?;
            }
        }

        let has_errors = self.files.iter().any(|file| !file.parse_errors.is_empty());
        if has_errors {
            writeln!(f)?;
            writeln!(f, "Parser errors:")?;
            for file in &self.files {
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

        Ok(())
    }
}
