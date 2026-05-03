use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};

use expect_test::Expect;
use rg_body_ir::{BodyIrPackageBundle, PackageBodies};
use rg_def_map::{DefMapPackageBundle, Package, PackageSlot};
use rg_semantic_ir::{PackageIr, SemanticIrPackageBundle};
use rg_workspace::WorkspaceMetadata;
use test_fixture::fixture_crate;

use crate::{
    CachedDependency, CachedPackage, CachedPackageId, CachedPackageSlot, CachedPackageSource,
    CachedPath, CachedRustEdition, CachedTarget, CachedTargetKind, CachedWorkspace,
    PackageCacheArtifact, PackageCacheBodyIrState, PackageCacheCodec, PackageCacheHeader,
    PackageCachePayload, PackageCacheStore, Project,
};

pub(super) fn check_cached_workspace(fixture: &str, expect: Expect) {
    let fixture = fixture_crate(fixture);
    let workspace = WorkspaceMetadata::from_cargo(fixture.metadata())
        .expect("fixture workspace metadata should normalize");
    let cached_workspace = CachedWorkspace::build(&workspace);
    let actual = render_cached_workspace(&workspace, &cached_workspace);

    expect.assert_eq(&format!("{}\n", actual.trim_end()));
}

pub(super) fn check_cache_store_paths(fixture: &str, expect: Expect) {
    let fixture = fixture_crate(fixture);
    let workspace = WorkspaceMetadata::from_cargo(fixture.metadata())
        .expect("fixture workspace metadata should normalize");
    let cached_workspace = CachedWorkspace::build(&workspace);

    let mut dump = String::new();
    render_cache_store(
        "workspace target",
        &workspace,
        &cached_workspace,
        &PackageCacheStore::for_workspace_with_target_dir(
            &workspace,
            workspace.workspace_root().join("target"),
        ),
        &mut dump,
    );
    writeln!(&mut dump).expect("string writes should not fail");
    render_cache_store(
        "custom target",
        &workspace,
        &cached_workspace,
        &PackageCacheStore::for_workspace_with_target_dir(
            &workspace,
            PathBuf::from("custom-target"),
        ),
        &mut dump,
    );

    expect.assert_eq(&format!("{}\n", dump.trim_end()));
}

pub(super) fn check_cache_header_codec(expect: Expect) {
    let header = PackageCacheHeader::new(CachedPackage {
        package: CachedPackageSlot(7),
        package_id: CachedPackageId::from_stable_text("path+file:///workspace#app@0.1.0"),
        name: "app".to_string(),
        source: CachedPackageSource::Workspace,
        edition: CachedRustEdition::Edition2024,
        manifest_path: CachedPath::from_stable_text("/workspace/Cargo.toml"),
        targets: vec![
            CachedTarget {
                name: "app".to_string(),
                kind: CachedTargetKind::Lib,
                src_path: CachedPath::from_stable_text("/workspace/src/lib.rs"),
            },
            CachedTarget {
                name: "app-cli".to_string(),
                kind: CachedTargetKind::Bin,
                src_path: CachedPath::from_stable_text("/workspace/src/main.rs"),
            },
        ],
        dependencies: vec![CachedDependency {
            package_id: CachedPackageId::from_stable_text("path+file:///workspace/dep#dep@0.1.0"),
            name: "dep".to_string(),
            is_normal: true,
            is_build: false,
            is_dev: false,
        }],
    });

    let bytes =
        PackageCacheCodec::encode_header(&header).expect("package cache header should serialize");
    let decoded =
        PackageCacheCodec::decode_header(&bytes).expect("package cache header should deserialize");
    assert_eq!(decoded, header);

    let mut dump = String::new();
    writeln!(&mut dump, "encoded header bytes {}", bytes.len())
        .expect("string writes should not fail");
    render_hex(&bytes, &mut dump);
    writeln!(&mut dump).expect("string writes should not fail");
    render_header("decoded header", &decoded, &mut dump);

    expect.assert_eq(&format!("{}\n", dump.trim_end()));
}

pub(super) fn check_minimal_cache_artifact_codec(expect: Expect) {
    let artifact = PackageCacheArtifact::new(
        PackageCacheHeader::new(CachedPackage {
            package: CachedPackageSlot(7),
            package_id: CachedPackageId::from_stable_text("path+file:///workspace#empty@0.1.0"),
            name: String::new(),
            source: CachedPackageSource::Workspace,
            edition: CachedRustEdition::Edition2024,
            manifest_path: CachedPath::from_stable_text("/workspace/Cargo.toml"),
            targets: Vec::new(),
            dependencies: Vec::new(),
        }),
        PackageCachePayload::new(
            DefMapPackageBundle::new(Package::default()),
            SemanticIrPackageBundle::new(PackageIr::default()),
            PackageCacheBodyIrState::Built(Box::new(BodyIrPackageBundle::new(
                PackageBodies::default(),
            ))),
        ),
    );

    let bytes = PackageCacheCodec::encode_artifact(&artifact)
        .expect("package cache artifact should serialize");
    let decoded = PackageCacheCodec::decode_artifact(&bytes)
        .expect("package cache artifact should deserialize");
    assert_eq!(decoded, artifact);

    let mut dump = String::new();
    writeln!(&mut dump, "encoded artifact bytes {}", bytes.len())
        .expect("string writes should not fail");
    render_hex(&bytes, &mut dump);
    writeln!(&mut dump).expect("string writes should not fail");
    render_artifact("decoded artifact", &decoded, &mut dump);

    expect.assert_eq(&format!("{}\n", dump.trim_end()));
}

pub(super) fn check_fixture_cache_artifact_codec(fixture: &str, expect: Expect) {
    let fixture = fixture_crate(fixture);
    let workspace = WorkspaceMetadata::from_cargo(fixture.metadata())
        .expect("fixture workspace metadata should normalize");
    let cached_workspace = CachedWorkspace::build(&workspace);
    let project = Project::build(workspace).expect("fixture project should build");
    let artifact = package_artifact_from_project(&cached_workspace, &project, PackageSlot(0));

    let bytes = PackageCacheCodec::encode_artifact(&artifact)
        .expect("package cache artifact should serialize");
    let decoded = PackageCacheCodec::decode_artifact(&bytes)
        .expect("package cache artifact should deserialize");
    assert_eq!(decoded, artifact);

    let mut dump = String::new();
    writeln!(&mut dump, "encoded artifact bytes {}", bytes.len())
        .expect("string writes should not fail");
    render_artifact("decoded artifact", &decoded, &mut dump);

    expect.assert_eq(&format!("{}\n", dump.trim_end()));
}

fn package_artifact_from_project(
    cached_workspace: &CachedWorkspace,
    project: &Project,
    package: PackageSlot,
) -> PackageCacheArtifact {
    let header = cached_workspace
        .artifact_header(package)
        .expect("cached fixture package should have an artifact header");
    let def_map = project
        .def_map
        .package(package)
        .expect("fixture package should have def-map data")
        .clone();
    let semantic_ir = project
        .semantic_ir
        .package(package)
        .expect("fixture package should have semantic IR data")
        .clone();
    let body_ir = project
        .body_ir
        .package(package)
        .expect("fixture package should have body IR data")
        .clone();

    PackageCacheArtifact::new(
        header,
        PackageCachePayload::new(
            DefMapPackageBundle::new(def_map),
            SemanticIrPackageBundle::new(semantic_ir),
            PackageCacheBodyIrState::Built(Box::new(BodyIrPackageBundle::new(body_ir))),
        ),
    )
}

fn render_cached_workspace(
    workspace: &WorkspaceMetadata,
    cached_workspace: &CachedWorkspace,
) -> String {
    let mut dump = String::new();
    writeln!(&mut dump, "cached workspace").expect("string writes should not fail");

    for package in cached_workspace.packages() {
        writeln!(&mut dump).expect("string writes should not fail");
        render_package(workspace, cached_workspace, package, &mut dump);
    }

    dump
}

fn render_cache_store(
    label: &str,
    workspace: &WorkspaceMetadata,
    cached_workspace: &CachedWorkspace,
    store: &PackageCacheStore,
    dump: &mut String,
) {
    writeln!(dump, "cache store `{label}`").expect("string writes should not fail");
    writeln!(
        dump,
        "root {}",
        cache_path(workspace, store.root().to_path_buf()),
    )
    .expect("string writes should not fail");
    writeln!(dump, "artifacts").expect("string writes should not fail");

    for package in cached_workspace.packages() {
        writeln!(
            dump,
            "- #{} {} {}",
            package.package.0,
            package.name,
            store.package_fingerprint(package),
        )
        .expect("string writes should not fail");
        writeln!(
            dump,
            "  {}",
            cache_path(workspace, store.package_artifact_path(package)),
        )
        .expect("string writes should not fail");
    }
}

fn render_package(
    workspace: &WorkspaceMetadata,
    cached_workspace: &CachedWorkspace,
    package: &CachedPackage,
    dump: &mut String,
) {
    // The header is rendered together with the cached package because artifact metadata is the
    // unit future cache readers will validate before loading any package payload.
    let header = cached_workspace
        .artifact_header(
            package
                .package
                .workspace_slot()
                .expect("cached package slots should fit into workspace slots"),
        )
        .expect("cached package should have an artifact header");

    writeln!(dump, "package #{} {}", package.package.0, package.name)
        .expect("string writes should not fail");
    writeln!(dump, "schema {}", header.schema_version.0).expect("string writes should not fail");
    writeln!(
        dump,
        "id {}",
        normalize_package_id(workspace.workspace_root(), package.package_id.as_str()),
    )
    .expect("string writes should not fail");
    writeln!(dump, "source {}", package.source).expect("string writes should not fail");
    writeln!(dump, "edition {}", package.edition).expect("string writes should not fail");
    writeln!(
        dump,
        "manifest {}",
        relative_path(workspace.workspace_root(), package.manifest_path.as_path())
    )
    .expect("string writes should not fail");

    render_targets(workspace, package, dump);
    render_dependencies(workspace, cached_workspace, package, dump);
}

fn render_header(label: &str, header: &PackageCacheHeader, dump: &mut String) {
    writeln!(dump, "{label}").expect("string writes should not fail");
    writeln!(dump, "schema {}", header.schema_version.0).expect("string writes should not fail");
    writeln!(
        dump,
        "package #{} {}",
        header.package.package.0, header.package.name,
    )
    .expect("string writes should not fail");
    writeln!(dump, "id {}", header.package.package_id).expect("string writes should not fail");
    writeln!(dump, "source {}", header.package.source).expect("string writes should not fail");
    writeln!(dump, "edition {}", header.package.edition).expect("string writes should not fail");
    writeln!(dump, "manifest {}", header.package.manifest_path)
        .expect("string writes should not fail");

    writeln!(dump, "targets").expect("string writes should not fail");
    for target in CachedTarget::sorted(&header.package.targets) {
        writeln!(
            dump,
            "- {} [{}] {}",
            target.name, target.kind, target.src_path,
        )
        .expect("string writes should not fail");
    }

    writeln!(dump, "dependencies").expect("string writes should not fail");
    for dependency in CachedDependency::sorted(&header.package.dependencies) {
        writeln!(
            dump,
            "- {} -> {} {}",
            dependency.name,
            dependency.package_id,
            render_dependency_kinds(dependency),
        )
        .expect("string writes should not fail");
    }
}

fn render_artifact(label: &str, artifact: &PackageCacheArtifact, dump: &mut String) {
    writeln!(dump, "{label}").expect("string writes should not fail");
    writeln!(dump, "schema {}", artifact.header.schema_version.0)
        .expect("string writes should not fail");
    writeln!(
        dump,
        "package #{} {}",
        artifact.header.package.package.0, artifact.header.package.name,
    )
    .expect("string writes should not fail");
    writeln!(
        dump,
        "header targets {}",
        artifact.header.package.targets.len()
    )
    .expect("string writes should not fail");
    writeln!(
        dump,
        "def-map package {} targets {}",
        artifact.payload.def_map.package().package_name(),
        artifact.payload.def_map.package().targets().len(),
    )
    .expect("string writes should not fail");
    writeln!(
        dump,
        "semantic IR targets {}",
        artifact.payload.semantic_ir.package().targets().len(),
    )
    .expect("string writes should not fail");

    match &artifact.payload.body_ir {
        PackageCacheBodyIrState::Built(bundle) => {
            writeln!(
                dump,
                "body IR built targets {}",
                bundle.package().targets().len()
            )
            .expect("string writes should not fail");
        }
        PackageCacheBodyIrState::SkippedByPolicy => {
            writeln!(dump, "body IR skipped by policy").expect("string writes should not fail");
        }
    }
}

fn render_targets(workspace: &WorkspaceMetadata, package: &CachedPackage, dump: &mut String) {
    writeln!(dump, "targets").expect("string writes should not fail");

    let targets = CachedTarget::sorted(&package.targets);

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
            relative_path(workspace.workspace_root(), target.src_path.as_path()),
        )
        .expect("string writes should not fail");
    }
}

fn render_dependencies(
    workspace: &WorkspaceMetadata,
    cached_workspace: &CachedWorkspace,
    package: &CachedPackage,
    dump: &mut String,
) {
    writeln!(dump, "dependencies").expect("string writes should not fail");

    if package.dependencies.is_empty() {
        writeln!(dump, "- <none>").expect("string writes should not fail");
        return;
    }

    let dependencies = CachedDependency::sorted(&package.dependencies);

    for dependency in dependencies {
        writeln!(
            dump,
            "- {} -> {} {}",
            dependency.name,
            render_dependency_package(workspace, cached_workspace, &dependency.package_id),
            render_dependency_kinds(dependency),
        )
        .expect("string writes should not fail");
    }
}

fn render_dependency_package(
    workspace: &WorkspaceMetadata,
    cached_workspace: &CachedWorkspace,
    package_id: &CachedPackageId,
) -> String {
    cached_workspace
        .packages()
        .iter()
        .find(|package| &package.package_id == package_id)
        .map(|package| format!("{} (#{})", package.name, package.package.0))
        .unwrap_or_else(|| normalize_package_id(workspace.workspace_root(), package_id.as_str()))
}

fn render_dependency_kinds(dependency: &CachedDependency) -> String {
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

fn normalize_package_id(root: &Path, package_id: &str) -> String {
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

fn cache_path(workspace: &WorkspaceMetadata, path: PathBuf) -> String {
    let path = relative_path(workspace.workspace_root(), &path);
    let workspace_name = workspace
        .workspace_root()
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| "workspace".into());

    path.replace(workspace_name.as_ref(), "<workspace>")
}

fn render_hex(bytes: &[u8], dump: &mut String) {
    for chunk in bytes.chunks(32) {
        for byte in chunk {
            write!(dump, "{byte:02x}").expect("string writes should not fail");
        }
        writeln!(dump).expect("string writes should not fail");
    }
}
