use anyhow::Context as _;

use std::{fmt, path::Path};

use crate::item_tree::{
    file::{FileId, FileRecord, ParseDb},
    item::ItemNode,
    target::{TargetBuilder, TargetId, TargetIndex},
};

/// Parsed package, e.g. all the files, targets (lib.rs, main.rs, examples, integration
/// tests, etc), and parsed target representations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageIndex {
    /// Package name from `Cargo.toml`
    pub package_name: String,
    /// All parsed files known to this package index.
    pub files: Vec<FileRecord>,
    /// Per-target item trees built from target entrypoints.
    pub targets: Vec<TargetIndex>,
}

impl PackageIndex {
    /// Returns the path associated with a file id, if the id is valid.
    pub fn file_path(&self, file_id: FileId) -> Option<&Path> {
        self.files.get(file_id.0).map(|file| file.path.as_path())
    }

    /// Traverses each target and builds a package index.
    ///
    /// Each "root entrypoint" (e.g. `main.rs`, `lib.rs`, etc) is served as a root and all
    /// the items are recursively parsed from there.
    ///
    // Note: the same file in theory can be a module for multiple targets, e.g. if two
    // integration test will declare `mod utils`, both will have it.
    pub fn build(
        package_name: String,
        targets: Vec<cargo_metadata::Target>,
    ) -> anyhow::Result<Self> {
        let mut parse_db = ParseDb::default();
        let mut target_indexes = Vec::new();

        for (idx, target_input) in targets.into_iter().enumerate() {
            let target_id = TargetId(idx);
            let target_index = TargetBuilder::new(&mut parse_db)
                .build(target_id, target_input)
                .with_context(|| format!("while attempting to build target index {idx}"))?;
            target_indexes.push(target_index);
        }

        let files = parse_db.into_file_records();

        Ok(Self {
            package_name,
            files,
            targets: target_indexes,
        })
    }

    /// Formats one item subtree with indentation for human-readable output.
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

/// Renders a textual tree view of the package, targets, items, and parse diagnostics.
impl fmt::Display for PackageIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Package {}", self.package_name)?;

        for target in &self.targets {
            let root_path = self
                .file_path(target.root_file)
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<unknown>".to_string());
            let kinds = if !target.metadata.kind.is_empty() {
                format!("{:?}", target.metadata.kind)
            } else {
                "<unknown>".to_string()
            };

            writeln!(f)?;
            writeln!(
                f,
                "Target {} ({kinds}) | root {}",
                target.metadata.name, root_path
            )?;
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
