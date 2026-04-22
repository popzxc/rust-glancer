// Human note: this file must not grow into a big mess. If you anticipate that it will
// get too big to be readable, propose splitting it instead of blindly
// adding more tests here.

use std::path::PathBuf;

use crate::{
    Project,
    item_tree::{self, ItemKind, ItemNode, VisibilityLevel},
    test_utils::{CrateFixture, fixture_crate},
};

fn flatten<'a>(items: &'a [ItemNode], output: &mut Vec<&'a ItemNode>) {
    for item in items {
        output.push(item);
        flatten(&item.children, output);
    }
}

fn only_package(project: &Project) -> (&crate::parse::Package, &item_tree::Package) {
    let package = project
        .packages()
        .first()
        .expect("fixture package should exist");
    let package_trees = project
        .item_tree
        .package(0)
        .expect("item tree package should exist");

    (package, package_trees)
}

fn target_tree(
    project: &Project,
    expected_kind: cargo_metadata::TargetKind,
) -> (&crate::parse::Target, &item_tree::Target) {
    let (package, package_trees) = only_package(project);
    let parse_target = package
        .targets()
        .iter()
        .find(|target| target.cargo_target.is_kind(expected_kind.clone()))
        .unwrap_or_else(|| panic!("fixture target {:?} should exist", expected_kind));
    let tree_target = package_trees
        .target(parse_target.id)
        .expect("item tree target should exist");

    (parse_target, tree_target)
}

fn moderate_fixture() -> CrateFixture {
    fixture_crate!(
        r#"
//- /Cargo.toml
[package]
name = "moderate_crate"
version = "0.1.0"
edition = "2024"

[lib]
path = "src/lib.rs"

[[bin]]
name = "moderate_crate"
path = "src/main.rs"

//- /src/lib.rs
pub mod cli;
pub mod model;

//- /src/model.rs
pub struct Model;

impl Model {
    pub fn new() -> Self {
        Self
    }
}

//- /src/cli.rs
pub fn run() {}

//- /src/main.rs
use std::path::PathBuf;
use moderate_crate::cli::run;

fn main() {
    let _path = PathBuf::new();
    run();
}
"#
    )
}

fn simple_fixture() -> CrateFixture {
    fixture_crate!(
        r#"
//- /Cargo.toml
[package]
name = "simple_crate"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub fn add_two_numbers(left: i32, right: i32) -> i32 {
    left + right
}
"#
    )
}

fn macro_fixture() -> CrateFixture {
    fixture_crate!(
        r#"
//- /Cargo.toml
[package]
name = "complex_crate"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
macro_rules! label_result {
    ($value:expr) => {
        $value
    };
}

pub fn decorate(input: &str) -> &str {
    label_result!(input)
}
"#
    )
}

#[test]
fn parses_module_tree_and_impl_items() {
    let fixture = moderate_fixture();
    let project = fixture.project_for_target("src/lib.rs");
    let (_, target) = target_tree(&project, cargo_metadata::TargetKind::Lib);
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
    let fixture = macro_fixture();
    let project = fixture.project_for_target("src/lib.rs");
    let (_, target) = target_tree(&project, cargo_metadata::TargetKind::Lib);
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
    let fixture = simple_fixture();
    let project = fixture.project_for_target("src/lib.rs");
    let (_, target) = target_tree(&project, cargo_metadata::TargetKind::Lib);
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
    let fixture = moderate_fixture();
    let project = fixture.project_for_target("src/main.rs");
    let (_, target) = target_tree(&project, cargo_metadata::TargetKind::Bin);
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

fn mock_target(name: &str, kind: &[&str], root_file: PathBuf) -> cargo_metadata::Target {
    cargo_metadata::TargetBuilder::default()
        .name(name)
        .kind(
            kind.into_iter()
                .map(|&k| cargo_metadata::TargetKind::from(k))
                .collect::<Vec<_>>(),
        )
        .crate_types(
            kind.into_iter()
                .map(|&k| cargo_metadata::CrateType::from(k))
                .collect::<Vec<_>>(),
        )
        .src_path(root_file.to_str().expect("fixture path should be UTF-8"))
        .build()
        .expect("target fixture should be valid")
}

#[test]
fn parses_shared_files_once_across_targets() {
    let fixture = simple_fixture();
    let root_file = fixture.path("src/lib.rs");
    let project = fixture.project_with_targets(vec![
        mock_target("a", &["lib"], root_file.clone()),
        mock_target("b", &["bin"], root_file),
    ]);
    let (package, _) = only_package(&project);

    assert_eq!(
        package.files.parsed_files.len(),
        1,
        "shared file should be parsed once"
    );
    assert_eq!(package.targets().len(), 2, "all targets should be indexed");
}

#[test]
fn builds_independent_trees_for_lib_and_bin_targets() {
    let fixture = moderate_fixture();
    let project = fixture.project();
    let (_, lib_target) = target_tree(&project, cargo_metadata::TargetKind::Lib);
    let (_, bin_target) = target_tree(&project, cargo_metadata::TargetKind::Bin);

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
