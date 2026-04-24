use std::path::PathBuf;

use crate::{
    parse::ParseDb,
    test_utils::{CrateFixture, fixture_crate, test_file},
    workspace_metadata::WorkspaceMetadata,
};

fn test_metadata(path: &str) -> cargo_metadata::Metadata {
    cargo_metadata::MetadataCommand::new()
        .manifest_path(test_file(path).join("Cargo.toml"))
        .exec()
        .expect("fixture metadata should load")
}

#[test]
fn analyzes_all_workspace_members() {
    let analysis = ParseDb::build(&WorkspaceMetadata::from_cargo(test_metadata(
        "moderate_workspace",
    )))
    .expect("workspace fixture should parse");

    assert_eq!(
        analysis.workspace_packages().count(),
        3,
        "all workspace members should be represented"
    );
    assert_eq!(
        analysis.packages().len(),
        3,
        "workspace fixture should not include external dependencies"
    );
}

#[test]
fn resolves_project() {
    let metadata = WorkspaceMetadata::from_cargo(test_metadata("moderate_workspace"));
    let analysis = ParseDb::build(&metadata).expect("workspace fixture should parse");

    assert_eq!(
        analysis.packages().len(),
        3,
        "full scope should keep all reachable packages in this fixture"
    );
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

fn mock_target(name: &str, kind: &[&str], root_file: PathBuf) -> cargo_metadata::Target {
    cargo_metadata::TargetBuilder::default()
        .name(name)
        .kind(
            kind.iter()
                .map(|&k| cargo_metadata::TargetKind::from(k))
                .collect::<Vec<_>>(),
        )
        .crate_types(
            kind.iter()
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
    let mut package = fixture.package();
    package.targets = vec![
        mock_target("a", &["lib"], root_file.clone()),
        mock_target("b", &["bin"], root_file),
    ];

    let mut metadata = fixture.metadata();
    if let Some(metadata_package) = metadata.packages.iter_mut().find(|it| it.id == package.id) {
        *metadata_package = package;
    }

    let parse = ParseDb::build(&WorkspaceMetadata::from_cargo(metadata))
        .expect("fixture metadata should parse");
    let package = parse
        .packages()
        .first()
        .expect("fixture package should exist");

    assert_eq!(
        package.files.parsed_files().len(),
        1,
        "shared file should be parsed once"
    );
    assert_eq!(package.targets().len(), 2, "all targets should be indexed");
}
