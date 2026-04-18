use crate::test_utils::test_file;

use super::{
    item::{ItemKind, ItemNode, VisibilityLevel},
    package::PackageIndex,
    target::TargetInput,
};

fn flatten<'a>(items: &'a [ItemNode], output: &mut Vec<&'a ItemNode>) {
    for item in items {
        output.push(item);
        flatten(&item.children, output);
    }
}

fn single_target_index(path: &str) -> PackageIndex {
    PackageIndex::build(
        "fixture".to_string(),
        vec![TargetInput {
            name: "fixture".to_string(),
            kinds: vec!["lib".to_string()],
            root_file: test_file(path),
        }],
    )
    .expect("fixture crate should parse")
}

#[test]
fn parses_module_tree_and_impl_items() {
    let index = single_target_index("moderate_crate/src/lib.rs");
    let target = index.targets.first().expect("target should exist");
    let mut all_items = Vec::new();
    flatten(&target.root_items, &mut all_items);

    let model_module = all_items
        .iter()
        .find(|item| item.kind == ItemKind::Module && item.name.as_deref() == Some("model"))
        .expect("model module should exist");
    assert_eq!(model_module.visibility, VisibilityLevel::Public);

    let constructor = all_items
        .iter()
        .find(|item| {
            item.kind == ItemKind::AssociatedFunction && item.name.as_deref() == Some("new")
        })
        .expect("impl method should be collected");
    assert_eq!(constructor.visibility, VisibilityLevel::Public);
}

#[test]
fn keeps_macro_definitions_only() {
    let index = single_target_index("complex_crate/src/lib.rs");
    let target = index.targets.first().expect("target should exist");
    let mut all_items = Vec::new();
    flatten(&target.root_items, &mut all_items);

    let macro_def = all_items
        .iter()
        .find(|item| {
            item.kind == ItemKind::MacroDefinition && item.name.as_deref() == Some("label_result")
        })
        .expect("macro definition should exist");
    assert_eq!(macro_def.visibility, VisibilityLevel::Private);
}

#[test]
fn stores_offset_and_line_column_spans() {
    let index = single_target_index("simple_crate/src/lib.rs");
    let target = index.targets.first().expect("target should exist");
    let mut all_items = Vec::new();
    flatten(&target.root_items, &mut all_items);

    let function = all_items
        .iter()
        .find(|item| {
            item.kind == ItemKind::Function && item.name.as_deref() == Some("add_two_numbers")
        })
        .expect("function should exist");
    assert!(function.span.text.end > function.span.text.start);
    assert!(
        function.span.line_column.end.line >= function.span.line_column.start.line,
        "span end line should be after start line"
    );
}

#[test]
fn shows_import_paths_for_use_items() {
    let index = single_target_index("moderate_crate/src/main.rs");
    let target = index.targets.first().expect("target should exist");
    let mut all_items = Vec::new();
    flatten(&target.root_items, &mut all_items);

    let use_items = all_items
        .iter()
        .filter(|item| item.kind == ItemKind::Use)
        .collect::<Vec<_>>();
    assert!(
        use_items
            .iter()
            .any(|item| item.name.as_deref() == Some("std::path::PathBuf")),
        "use node should carry imported path"
    );
    assert!(
        use_items
            .iter()
            .any(|item| item.name.as_deref() == Some("moderate_crate::cli::run")),
        "second use should also carry imported path"
    );
}

#[test]
fn parses_shared_files_once_across_targets() {
    let root_file = test_file("simple_crate/src/lib.rs");
    let index = PackageIndex::build(
        "fixture".to_string(),
        vec![
            TargetInput {
                name: "a".to_string(),
                kinds: vec!["lib".to_string()],
                root_file: root_file.clone(),
            },
            TargetInput {
                name: "b".to_string(),
                kinds: vec!["bin".to_string()],
                root_file,
            },
        ],
    )
    .expect("fixture crate should parse");

    assert_eq!(index.files.len(), 1, "shared file should be parsed once");
    assert_eq!(index.targets.len(), 2, "all targets should be indexed");
}

#[test]
fn builds_independent_trees_for_lib_and_bin_targets() {
    let index = PackageIndex::build(
        "moderate_crate".to_string(),
        vec![
            TargetInput {
                name: "moderate_crate".to_string(),
                kinds: vec!["lib".to_string()],
                root_file: test_file("moderate_crate/src/lib.rs"),
            },
            TargetInput {
                name: "moderate_crate".to_string(),
                kinds: vec!["bin".to_string()],
                root_file: test_file("moderate_crate/src/main.rs"),
            },
        ],
    )
    .expect("fixture crate should parse");

    let lib_target = index
        .targets
        .iter()
        .find(|target| target.kinds.iter().any(|kind| kind == "lib"))
        .expect("lib target should exist");
    let bin_target = index
        .targets
        .iter()
        .find(|target| target.kinds.iter().any(|kind| kind == "bin"))
        .expect("bin target should exist");

    let mut lib_items = Vec::new();
    flatten(&lib_target.root_items, &mut lib_items);
    assert!(
        lib_items
            .iter()
            .any(|item| item.kind == ItemKind::Module && item.name.as_deref() == Some("cli")),
        "lib target should include module declarations"
    );

    let mut bin_items = Vec::new();
    flatten(&bin_target.root_items, &mut bin_items);
    assert!(
        bin_items
            .iter()
            .any(|item| item.kind == ItemKind::Function && item.name.as_deref() == Some("main")),
        "bin target should include bin entrypoint function"
    );
}
