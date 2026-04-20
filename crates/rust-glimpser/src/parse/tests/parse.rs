// Human note: this file must not grow into a big mess. If you anticipate that it will
// get too big to be readable, propose splitting it instead of blindly
// adding more tests here.

use std::path::PathBuf;

use crate::parse::item::{ItemKind, ItemNode, VisibilityLevel};
use crate::test_fixture::{CrateFixture, fixture_crate};

fn flatten<'a>(items: &'a [ItemNode], output: &mut Vec<&'a ItemNode>) {
    for item in items {
        output.push(item);
        flatten(&item.children, output);
    }
}

fn moderate_fixture() -> CrateFixture {
    fixture_crate! {
        "Cargo.toml" => r#"
[package]
name = "moderate_crate"
version = "0.1.0"
edition = "2024"

[lib]
path = "src/lib.rs"

[[bin]]
name = "moderate_crate"
path = "src/main.rs"
"#,
        "src/lib.rs" => r#"
pub mod cli;
pub mod model;
"#,
        "src/model.rs" => r#"
pub struct Model;

impl Model {
    pub fn new() -> Self {
        Self
    }
}
"#,
        "src/cli.rs" => r#"
pub fn run() {}
"#,
        "src/main.rs" => r#"
use std::path::PathBuf;
use moderate_crate::cli::run;

fn main() {
    let _path = PathBuf::new();
    run();
}
"#,
    }
}

fn simple_fixture() -> CrateFixture {
    fixture_crate! {
        "Cargo.toml" => r#"
[package]
name = "simple_crate"
version = "0.1.0"
edition = "2024"
"#,
        "src/lib.rs" => r#"
pub fn add_two_numbers(left: i32, right: i32) -> i32 {
    left + right
}
"#,
    }
}

fn macro_fixture() -> CrateFixture {
    fixture_crate! {
        "Cargo.toml" => r#"
[package]
name = "complex_crate"
version = "0.1.0"
edition = "2024"
"#,
        "src/lib.rs" => r#"
macro_rules! label_result {
    ($value:expr) => {
        $value
    };
}

pub fn decorate(input: &str) -> &str {
    label_result!(input)
}
"#,
    }
}

#[test]
fn parses_module_tree_and_impl_items() {
    let fixture = moderate_fixture();
    let index = fixture.package_index_for_target("src/lib.rs");
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
    let fixture = macro_fixture();
    let index = fixture.package_index_for_target("src/lib.rs");
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
    let fixture = simple_fixture();
    let index = fixture.package_index_for_target("src/lib.rs");
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
    let fixture = moderate_fixture();
    let index = fixture.package_index_for_target("src/main.rs");
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
    let index = fixture.package_index_with_targets(vec![
        mock_target("a", &["lib"], root_file.clone()),
        mock_target("b", &["bin"], root_file),
    ]);

    assert_eq!(
        index.db.parsed_files.len(),
        1,
        "shared file should be parsed once"
    );
    assert_eq!(index.targets.len(), 2, "all targets should be indexed");
}

#[test]
fn builds_independent_trees_for_lib_and_bin_targets() {
    let fixture = moderate_fixture();
    let index = fixture.package_index();

    let lib_target = index
        .targets
        .iter()
        .find(|target| {
            target
                .metadata
                .kind
                .iter()
                .any(|kind| kind == &cargo_metadata::TargetKind::Lib)
        })
        .expect("lib target should exist");
    let bin_target = index
        .targets
        .iter()
        .find(|target| {
            target
                .metadata
                .kind
                .iter()
                .any(|kind| kind == &cargo_metadata::TargetKind::Bin)
        })
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
