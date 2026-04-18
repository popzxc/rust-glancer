use crate::{parse::ProjectAnalysis, test_utils::test_file};

mod parse;

fn test_metadata(path: &str) -> cargo_metadata::Metadata {
    cargo_metadata::MetadataCommand::new()
        .manifest_path(test_file(path).join("Cargo.toml"))
        .exec()
        .expect("fixture metadata should load")
}

#[test]
fn analyzes_all_workspace_members() {
    let analysis = ProjectAnalysis::build(test_metadata("moderate_workspace"))
        .expect("workspace fixture should parse");

    assert_eq!(
        analysis.workspace_packages().count(),
        3,
        "all workspace members should be represented"
    );
    assert_eq!(
        analysis.slots.len(),
        3,
        "workspace fixture should not include external dependencies"
    );
}

#[test]
fn resolves_project() {
    let metadata = test_metadata("moderate_workspace");

    let analysis = ProjectAnalysis::build(metadata).expect("workspace fixture should parse");

    assert_eq!(
        analysis.slots.len(),
        3,
        "full scope should keep all reachable packages in this fixture"
    );
}
