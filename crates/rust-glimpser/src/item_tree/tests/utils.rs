use std::fmt::Write as _;

use expect_test::Expect;

use crate::{
    Project,
    item_tree::{ItemKind, ItemNode, ModuleSource},
    parse::{Package, Target},
    test_utils::fixture_crate,
};

pub(super) fn check_project_item_tree(fixture: &str, expect: Expect) {
    let actual = render_project_item_tree(&fixture_crate!(fixture).project(), false);
    let actual = format!("{}\n", actual.trim_end());
    expect.assert_eq(&actual);
}

pub(super) fn check_project_item_tree_with_spans(fixture: &str, expect: Expect) {
    let actual = render_project_item_tree(&fixture_crate!(fixture).project(), true);
    let actual = format!("{}\n", actual.trim_end());
    expect.assert_eq(&actual);
}

fn render_project_item_tree(project: &Project, include_spans: bool) -> String {
    let mut packages = project.packages().iter().enumerate().collect::<Vec<_>>();
    packages.sort_by(|left, right| left.1.package_name().cmp(right.1.package_name()));

    let package_dumps = packages
        .into_iter()
        .map(|(package_slot, package)| {
            let item_trees = project
                .item_tree
                .package(package_slot)
                .expect("package item trees should exist while rendering snapshot");
            let mut target_roots = item_trees.target_roots().iter().collect::<Vec<_>>();
            target_roots.sort_by(|left, right| {
                let left_target = package
                    .target(left.target)
                    .expect("parsed target should exist while sorting");
                let right_target = package
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

            let target_dumps = target_roots
                .into_iter()
                .map(|target_root| {
                    let target = package
                        .target(target_root.target)
                        .expect("parsed target should exist while rendering snapshot");
                    render_target_root(package, target, target_root.root_file)
                        .trim_end()
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("\n\n");

            let mut files = item_trees.files().collect::<Vec<_>>();
            files.sort_by(|left, right| {
                let left_path = package
                    .file_path(left.file)
                    .expect("item-tree file should exist while sorting");
                let right_path = package
                    .file_path(right.file)
                    .expect("item-tree file should exist while sorting");
                left_path.cmp(right_path)
            });

            let file_dumps = files
                .into_iter()
                .map(|file_tree| {
                    render_file_item_tree(package, file_tree.file, &file_tree.items, include_spans)
                        .trim_end()
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("\n\n");

            format!(
                "package {}\n\ntargets\n{target_dumps}\n\nfiles\n{file_dumps}",
                package.package_name()
            )
        })
        .collect::<Vec<_>>();

    package_dumps.join("\n\n")
}

fn render_target_root(
    package: &Package,
    target: &Target,
    root_file: crate::parse::FileId,
) -> String {
    let mut dump = String::new();
    let root_label = file_label(package, root_file);
    writeln!(
        &mut dump,
        "- {} [{}] -> {}",
        target.name, target.kind, root_label
    )
    .expect("string writes should not fail");

    dump
}

fn render_file_item_tree(
    package: &Package,
    file_id: crate::parse::FileId,
    items: &[ItemNode],
    include_spans: bool,
) -> String {
    let mut dump = String::new();
    writeln!(&mut dump, "file {}", file_label(package, file_id))
        .expect("string writes should not fail");

    for item in items {
        render_item(package, item, 0, include_spans, &mut dump);
    }

    dump
}

fn render_item(
    package: &Package,
    item: &ItemNode,
    depth: usize,
    include_spans: bool,
    dump: &mut String,
) {
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
        line.push_str(&format!(
            " [{}]",
            render_module_source(package, &module.source)
        ));
    }

    if let ItemKind::ExternCrate(extern_crate) = &item.kind {
        let name = extern_crate.name.as_deref().unwrap_or("<missing>");
        line.push_str(&format!(" [{name}{}]", extern_crate.alias));
    }

    if include_spans {
        line.push_str(&format!(
            " [{} {}:{}-{}:{} ({}..{})]",
            file_label(package, item.file_id),
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
                render_item(package, child, depth + 1, include_spans, dump);
            }
        }
    }
}

fn render_module_source(package: &Package, source: &ModuleSource) -> String {
    match source {
        ModuleSource::Inline { .. } => "inline".to_string(),
        ModuleSource::OutOfLine {
            definition_file: Some(file_id),
        } => {
            format!("out_of_line {}", file_label(package, *file_id))
        }
        ModuleSource::OutOfLine {
            definition_file: None,
        } => "out_of_line <missing>".to_string(),
    }
}

fn file_label(package: &Package, file_id: crate::parse::FileId) -> String {
    package
        .file_path(file_id)
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("<unknown>")
        .to_string()
}
