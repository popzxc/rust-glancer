use std::fmt::Write as _;

use expect_test::Expect;

use crate::{
    Project,
    item_tree::{ItemKind, ItemNode, ModuleSource},
    parse::{Package, Target},
    test_utils::{TestTargetExt, fixture_crate},
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
            let mut targets = item_trees.targets().iter().collect::<Vec<_>>();
            targets.sort_by(|left, right| {
                let left_target = package
                    .target(left.target)
                    .expect("parsed target should exist while sorting");
                let right_target = package
                    .target(right.target)
                    .expect("parsed target should exist while sorting");

                (
                    left_target.cargo_target.sort_order(),
                    left_target.cargo_target.name.as_str(),
                    left_target.cargo_target.src_path.as_str(),
                )
                    .cmp(&(
                        right_target.cargo_target.sort_order(),
                        right_target.cargo_target.name.as_str(),
                        right_target.cargo_target.src_path.as_str(),
                    ))
            });

            let target_dumps = targets
                .into_iter()
                .map(|target_tree| {
                    let target = package
                        .target(target_tree.target)
                        .expect("parsed target should exist while rendering snapshot");
                    render_target_item_tree(package, target, &target_tree.root_items, include_spans)
                        .trim_end()
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("\n\n");

            format!("package {}\n\n{target_dumps}", package.package_name())
        })
        .collect::<Vec<_>>();

    package_dumps.join("\n\n")
}

fn render_target_item_tree(
    package: &Package,
    target: &Target,
    items: &[ItemNode],
    include_spans: bool,
) -> String {
    let mut dump = String::new();
    writeln!(
        &mut dump,
        "target {} [{}]",
        target.cargo_target.name,
        target.cargo_target.kind_label()
    )
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
        let file_path = package
            .file_path(item.file_id)
            .expect("item file should exist while rendering spans");
        let file_label = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("<unknown>");
        line.push_str(&format!(
            " [{} {}:{}-{}:{} ({}..{})]",
            file_label,
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

    for child in &item.children {
        render_item(package, child, depth + 1, include_spans, dump);
    }
}

fn render_module_source(package: &Package, source: &ModuleSource) -> String {
    match source {
        ModuleSource::Inline => "inline".to_string(),
        ModuleSource::OutOfLine {
            definition_file: Some(file_id),
        } => {
            let file_path = package
                .file_path(*file_id)
                .expect("out-of-line module file should exist while rendering snapshot");
            let file_label = file_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("<unknown>");
            format!("out_of_line {file_label}")
        }
        ModuleSource::OutOfLine {
            definition_file: None,
        } => "out_of_line <missing>".to_string(),
    }
}
