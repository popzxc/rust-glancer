use std::{fmt::Write as _, path::Path};

use expect_test::Expect;
use rg_workspace::{PackageId, WorkspaceMetadata};
use test_fixture::fixture_crate;

use crate::{PackageCacheDependency, PackageCacheIdentity, PackageCachePlan, PackageCacheTarget};

pub(super) fn check_cache_plan(fixture: &str, expect: Expect) {
    let fixture = fixture_crate(fixture);
    let workspace = WorkspaceMetadata::from_cargo(fixture.metadata())
        .expect("fixture workspace metadata should normalize");
    let plan = PackageCachePlan::build(&workspace);
    let actual = render_cache_plan(&workspace, &plan);

    expect.assert_eq(&format!("{}\n", actual.trim_end()));
}

fn render_cache_plan(workspace: &WorkspaceMetadata, plan: &PackageCachePlan) -> String {
    let mut dump = String::new();
    writeln!(&mut dump, "package cache plan").expect("string writes should not fail");

    for package in plan.packages() {
        writeln!(&mut dump).expect("string writes should not fail");
        render_package(workspace, plan, package, &mut dump);
    }

    dump
}

fn render_package(
    workspace: &WorkspaceMetadata,
    plan: &PackageCachePlan,
    package: &PackageCacheIdentity,
    dump: &mut String,
) {
    // The header is rendered together with the identity because the artifact boundary is the unit
    // future cache readers will validate before loading any package payload.
    let header = plan
        .artifact_header(package.package)
        .expect("package from cache plan should have an artifact header");

    writeln!(dump, "package #{} {}", package.package.0, package.name)
        .expect("string writes should not fail");
    writeln!(dump, "schema {}", header.schema_version.0).expect("string writes should not fail");
    writeln!(
        dump,
        "id {}",
        normalize_package_id(workspace.workspace_root(), &package.package_id),
    )
    .expect("string writes should not fail");
    writeln!(dump, "source {}", package.source).expect("string writes should not fail");
    writeln!(dump, "edition {}", package.edition).expect("string writes should not fail");
    writeln!(
        dump,
        "manifest {}",
        relative_path(workspace.workspace_root(), &package.manifest_path)
    )
    .expect("string writes should not fail");

    render_targets(workspace, package, dump);
    render_dependencies(workspace, plan, package, dump);
}

fn render_targets(
    workspace: &WorkspaceMetadata,
    package: &PackageCacheIdentity,
    dump: &mut String,
) {
    writeln!(dump, "targets").expect("string writes should not fail");

    let mut targets = package.targets.iter().collect::<Vec<_>>();
    targets.sort_by(|left, right| target_sort_key(left).cmp(&target_sort_key(right)));

    if targets.is_empty() {
        writeln!(dump, "- <none>").expect("string writes should not fail");
        return;
    }

    for target in targets {
        writeln!(
            dump,
            "- {} [{}] {}",
            target.name,
            target.kind,
            relative_path(workspace.workspace_root(), &target.src_path),
        )
        .expect("string writes should not fail");
    }
}

fn target_sort_key(target: &PackageCacheTarget) -> (u8, &str, &Path) {
    (
        target.kind.sort_order(),
        target.name.as_str(),
        target.src_path.as_path(),
    )
}

fn render_dependencies(
    workspace: &WorkspaceMetadata,
    plan: &PackageCachePlan,
    package: &PackageCacheIdentity,
    dump: &mut String,
) {
    writeln!(dump, "dependencies").expect("string writes should not fail");

    if package.dependencies.is_empty() {
        writeln!(dump, "- <none>").expect("string writes should not fail");
        return;
    }

    let mut dependencies = package.dependencies.iter().collect::<Vec<_>>();
    dependencies.sort_by(|left, right| dependency_sort_key(left).cmp(&dependency_sort_key(right)));

    for dependency in dependencies {
        writeln!(
            dump,
            "- {} -> {} {}",
            dependency.name,
            render_dependency_package(workspace, plan, &dependency.package_id),
            render_dependency_kinds(dependency),
        )
        .expect("string writes should not fail");
    }
}

fn dependency_sort_key(dependency: &PackageCacheDependency) -> (&str, String, bool, bool, bool) {
    (
        dependency.name.as_str(),
        dependency.package_id.to_string(),
        dependency.is_normal,
        dependency.is_build,
        dependency.is_dev,
    )
}

fn render_dependency_package(
    workspace: &WorkspaceMetadata,
    plan: &PackageCachePlan,
    package_id: &PackageId,
) -> String {
    plan.packages()
        .iter()
        .find(|package| &package.package_id == package_id)
        .map(|package| format!("{} (#{})", package.name, package.package.0))
        .unwrap_or_else(|| normalize_package_id(workspace.workspace_root(), package_id))
}

fn render_dependency_kinds(dependency: &PackageCacheDependency) -> String {
    let mut kinds = Vec::new();

    if dependency.is_normal {
        kinds.push("normal");
    }
    if dependency.is_build {
        kinds.push("build");
    }
    if dependency.is_dev {
        kinds.push("dev");
    }

    format!("[{}]", kinds.join(", "))
}

fn normalize_package_id(root: &Path, package_id: &PackageId) -> String {
    let root_path = root.display().to_string();
    let mut root_paths = vec![root_path];

    // Cargo package IDs may preserve the non-canonical `/var` spelling on macOS while normalized
    // workspace paths point at `/private/var`. Treat both as the same fixture root in snapshots.
    let public_tmp_path = root_paths[0]
        .strip_prefix("/private/")
        .map(|path| format!("/{path}"));
    if let Some(public_tmp_path) = public_tmp_path {
        root_paths.push(public_tmp_path);
    }

    let mut package_id = package_id.to_string();
    for root_path in &root_paths {
        package_id = package_id.replace(&format!("file://{root_path}"), "file://./");
    }
    for root_path in root_paths {
        package_id = package_id.replace(&root_path, ".");
    }

    package_id.replace("file://.//", "file://./")
}

fn relative_path(root: &Path, path: &Path) -> String {
    let relative_path = path.strip_prefix(root).unwrap_or(path);

    if relative_path.as_os_str().is_empty() {
        ".".to_string()
    } else {
        relative_path.display().to_string()
    }
}
