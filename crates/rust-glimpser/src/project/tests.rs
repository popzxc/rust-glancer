use crate::{
    project::{DependencyScope, ProjectAnalysis, ProjectBuildOptions},
    test_utils::test_file,
};

fn test_metadata(path: &str) -> cargo_metadata::Metadata {
    cargo_metadata::MetadataCommand::new()
        .manifest_path(test_file(path).join("Cargo.toml"))
        .exec()
        .expect("fixture metadata should load")
}

fn package_id(metadata: &cargo_metadata::Metadata, name: &str) -> cargo_metadata::PackageId {
    metadata
        .packages
        .iter()
        .find(|package| package.name.as_ref() == name)
        .map(|package| package.id.clone())
        .expect("fixture package should exist")
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
fn supports_custom_roots_for_workspace_only_scope() {
    let metadata = test_metadata("moderate_workspace");
    let app_id = package_id(&metadata, "moderate_workspace_app");

    let analysis = ProjectAnalysis::build_with_options(
        metadata,
        ProjectBuildOptions {
            dependency_scope: DependencyScope::WorkspaceOnly,
            root_package_ids: vec![app_id.clone()],
        },
    )
    .expect("workspace fixture should parse");

    assert_eq!(
        analysis.slots.len(),
        1,
        "only selected root should be analyzed"
    );
    assert_eq!(
        analysis.slots[0].package_name(),
        "moderate_workspace_app",
        "selected root should map to app package"
    );
}

#[test]
fn traverses_path_dependencies_from_selected_root() {
    let metadata = test_metadata("moderate_workspace");
    let app_id = package_id(&metadata, "moderate_workspace_app");

    let analysis = ProjectAnalysis::build_with_options(
        metadata,
        ProjectBuildOptions {
            dependency_scope: DependencyScope::WorkspaceAndPathDependencies,
            root_package_ids: vec![app_id],
        },
    )
    .expect("workspace fixture should parse");

    let analyzed_names = analysis
        .slots
        .iter()
        .map(|package| package.package_name().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        analyzed_names.len(),
        3,
        "selected root should traverse both workspace path dependencies"
    );
    assert!(
        analyzed_names
            .iter()
            .any(|name| name == "moderate_workspace_app"),
        "root package should be included"
    );
    assert!(
        analyzed_names
            .iter()
            .any(|name| name == "moderate_workspace_math"),
        "path dependency should be included"
    );
    assert!(
        analyzed_names
            .iter()
            .any(|name| name == "moderate_workspace_text"),
        "second path dependency should be included"
    );
}

#[test]
fn supports_full_resolved_scope() {
    let metadata = test_metadata("moderate_workspace");
    let app_id = package_id(&metadata, "moderate_workspace_app");

    let analysis = ProjectAnalysis::build_with_options(
        metadata,
        ProjectBuildOptions {
            dependency_scope: DependencyScope::FullResolvedGraph,
            root_package_ids: vec![app_id],
        },
    )
    .expect("workspace fixture should parse");

    assert_eq!(
        analysis.slots.len(),
        3,
        "full scope should keep all reachable packages in this fixture"
    );
}
