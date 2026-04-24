use std::fmt::Write as _;

use expect_test::Expect;

use crate::{
    Project,
    item_tree::{ItemKind, ItemNode, ModuleSource},
    parse::{Package, Target},
    test_utils::fixture_crate,
};

pub(super) fn check_project_item_tree(fixture: &str, expect: Expect) {
    let project = fixture_crate!(fixture).project();
    let actual = ProjectItemTreeSnapshot::new(&project, false).render();
    let actual = format!("{}\n", actual.trim_end());
    expect.assert_eq(&actual);
}

pub(super) fn check_project_item_tree_with_spans(fixture: &str, expect: Expect) {
    let project = fixture_crate!(fixture).project();
    let actual = ProjectItemTreeSnapshot::new(&project, true).render();
    let actual = format!("{}\n", actual.trim_end());
    expect.assert_eq(&actual);
}

/// Project-level item-tree snapshot context.
/// Renders package sections such as `package demo`.
struct ProjectItemTreeSnapshot<'a> {
    project: &'a Project,
    include_spans: bool,
}

impl<'a> ProjectItemTreeSnapshot<'a> {
    fn new(project: &'a Project, include_spans: bool) -> Self {
        Self {
            project,
            include_spans,
        }
    }

    fn render(&self) -> String {
        let mut packages = self
            .project
            .parse_db()
            .packages()
            .iter()
            .enumerate()
            .collect::<Vec<_>>();
        packages.sort_by(|left, right| left.1.package_name().cmp(right.1.package_name()));

        let package_dumps = packages
            .into_iter()
            .map(|(package_slot, package)| {
                let item_trees = self
                    .project
                    .item_tree_db()
                    .package(package_slot)
                    .expect("package item trees should exist while rendering snapshot");
                PackageItemTreeSnapshot {
                    package,
                    item_trees,
                    include_spans: self.include_spans,
                }
                .render()
            })
            .collect::<Vec<_>>();

        package_dumps.join("\n\n")
    }
}

/// Package-level item-tree snapshot context with file-label access.
/// Renders target/file sections such as `file lib.rs`.
struct PackageItemTreeSnapshot<'a> {
    package: &'a Package,
    item_trees: &'a crate::item_tree::Package,
    include_spans: bool,
}

impl<'a> PackageItemTreeSnapshot<'a> {
    fn render(&self) -> String {
        let target_dumps = self
            .sorted_target_roots()
            .into_iter()
            .map(|target_root| {
                let target = self
                    .package
                    .target(target_root.target)
                    .expect("parsed target should exist while rendering snapshot");
                self.render_target_root(target, target_root.root_file)
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let file_dumps = self
            .sorted_files()
            .into_iter()
            .map(|file_tree| {
                self.render_file_item_tree(file_tree.file, &file_tree.items)
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        format!(
            "package {}\n\ntargets\n{target_dumps}\n\nfiles\n{file_dumps}",
            self.package.package_name()
        )
    }

    fn sorted_target_roots(&self) -> Vec<&crate::item_tree::TargetRoot> {
        let mut target_roots = self.item_trees.target_roots().iter().collect::<Vec<_>>();
        target_roots.sort_by(|left, right| {
            let left_target = self
                .package
                .target(left.target)
                .expect("parsed target should exist while sorting");
            let right_target = self
                .package
                .target(right.target)
                .expect("parsed target should exist while sorting");

            (
                left_target.kind.sort_order(),
                left_target.name.as_str(),
                left_target.src_path.as_path(),
            )
                .cmp(&(
                    right_target.kind.sort_order(),
                    right_target.name.as_str(),
                    right_target.src_path.as_path(),
                ))
        });
        target_roots
    }

    fn sorted_files(&self) -> Vec<&crate::item_tree::FileTree> {
        let mut files = self.item_trees.files().collect::<Vec<_>>();
        files.sort_by(|left, right| {
            let left_path = self
                .package
                .file_path(left.file)
                .expect("item-tree file should exist while sorting");
            let right_path = self
                .package
                .file_path(right.file)
                .expect("item-tree file should exist while sorting");
            left_path.cmp(right_path)
        });
        files
    }

    fn render_target_root(&self, target: &Target, root_file: crate::parse::FileId) -> String {
        let mut dump = String::new();
        writeln!(
            &mut dump,
            "- {} [{}] -> {}",
            target.name,
            target.kind,
            self.file_label(root_file)
        )
        .expect("string writes should not fail");

        dump
    }

    fn render_file_item_tree(&self, file_id: crate::parse::FileId, items: &[ItemNode]) -> String {
        let mut dump = String::new();
        writeln!(&mut dump, "file {}", self.file_label(file_id))
            .expect("string writes should not fail");

        for item in items {
            self.render_item(item, 0, &mut dump);
        }

        dump
    }

    fn render_item(&self, item: &ItemNode, depth: usize, dump: &mut String) {
        let indent = "  ".repeat(depth);
        let mut line = format!("{indent}- ");

        if item.visibility != crate::item_tree::VisibilityLevel::Private {
            line.push_str(&format!("{} ", item.visibility));
        }

        line.push_str(&item.kind.to_string());

        if let Some(name) = &item.name {
            line.push(' ');
            line.push_str(name);
        }

        if let ItemKind::Module(module) = &item.kind {
            line.push_str(&format!(" [{}]", self.render_module_source(&module.source)));
        }

        if let ItemKind::ExternCrate(extern_crate) = &item.kind {
            let name = extern_crate.name.as_deref().unwrap_or("<missing>");
            line.push_str(&format!(" [{name}{}]", extern_crate.alias));
        }

        if self.include_spans {
            line.push_str(&format!(
                " [{} {}:{}-{}:{} ({}..{})]",
                self.file_label(item.file_id),
                item.span.line_column.start.line + 1,
                item.span.line_column.start.column + 1,
                item.span.line_column.end.line + 1,
                item.span.line_column.end.column + 1,
                item.span.text.start,
                item.span.text.end,
            ));
        }

        writeln!(dump, "{line}").expect("string writes should not fail");

        if let ItemKind::Use(use_item) = &item.kind {
            for import in &use_item.imports {
                let path = import.path.to_string();
                let path = if path.is_empty() {
                    "<empty>".to_string()
                } else {
                    path
                };

                writeln!(
                    dump,
                    "{}  - import {} {}{}",
                    indent, import.kind, path, import.alias
                )
                .expect("string writes should not fail");
            }
        }

        if let ItemKind::Module(module) = &item.kind {
            if let ModuleSource::Inline { items } = &module.source {
                for child in items {
                    self.render_item(child, depth + 1, dump);
                }
            }
        }
    }

    fn render_module_source(&self, source: &ModuleSource) -> String {
        match source {
            ModuleSource::Inline { .. } => "inline".to_string(),
            ModuleSource::OutOfLine {
                definition_file: Some(file_id),
            } => {
                format!("out_of_line {}", self.file_label(*file_id))
            }
            ModuleSource::OutOfLine {
                definition_file: None,
            } => "out_of_line <missing>".to_string(),
        }
    }

    fn file_label(&self, file_id: crate::parse::FileId) -> String {
        self.package
            .file_path(file_id)
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("<unknown>")
            .to_string()
    }
}
