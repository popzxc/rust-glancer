use std::path::Path;

use rg_def_map::PackageSlot;
use rg_parse::{FileId, ParseDb};
use rg_workspace::WorkspaceMetadata;
use test_fixture::{fixture_crate, fixture_crate_with_markers};

use crate::{AnalysisHost, SavedFileChange};

#[test]
fn reparses_known_file_in_place() {
    let fixture = fixture_crate(
        r#"
//- /Cargo.toml
[package]
name = "host_update_fixture"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User;
"#,
    );
    let workspace = WorkspaceMetadata::from_cargo(fixture.metadata());
    let mut host = AnalysisHost::build(workspace).expect("analysis host should build");
    let lib_path = fixture.path("src/lib.rs");
    let before_file_id = file_id_for_path(host.snapshot().parse_db(), &lib_path);

    assert!(
        has_workspace_symbol(&host, "User"),
        "initial snapshot should contain the original type"
    );

    let summary = host
        .apply_change(SavedFileChange::new(&lib_path, "pub struct Account;\n"))
        .expect("source change should apply");
    let after_file_id = file_id_for_path(host.snapshot().parse_db(), &lib_path);

    assert_eq!(
        summary.changed_files.len(),
        1,
        "known file should be reported once"
    );
    assert_eq!(
        summary.changed_files[0].file, before_file_id,
        "known file should keep its stable FileId"
    );
    assert_eq!(
        summary.changed_targets.len(),
        1,
        "single-lib fixture should report one owning target"
    );
    assert_eq!(
        host.snapshot().targets_for_file(
            summary.changed_files[0].package,
            summary.changed_files[0].file
        ),
        summary.changed_targets,
        "snapshot target ownership should match the change summary"
    );
    assert_eq!(
        after_file_id, before_file_id,
        "reparse should replace the file in place"
    );
    assert!(
        has_workspace_symbol(&host, "Account"),
        "updated snapshot should use saved change text"
    );
    assert!(
        !has_workspace_symbol(&host, "User"),
        "updated snapshot should not keep stale semantic symbols"
    );
}

#[test]
fn uses_saved_change_text_for_modules_discovered_after_the_change() {
    let fixture = fixture_crate(
        r#"
//- /Cargo.toml
[package]
name = "host_new_module_fixture"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct Root;

//- /src/api.rs
pub struct DiskOnly;
"#,
    );
    let workspace = WorkspaceMetadata::from_cargo(fixture.metadata());
    let mut host = AnalysisHost::build(workspace).expect("analysis host should build");
    let lib_path = fixture.path("src/lib.rs");
    let api_path = fixture.path("src/api.rs");

    host.apply_changes([
        SavedFileChange::new(&api_path, "pub struct SavedOnly;\n"),
        SavedFileChange::new(&lib_path, "mod api;\n"),
    ])
    .expect("source changes should apply");

    assert!(
        has_workspace_symbol(&host, "SavedOnly"),
        "newly discovered module should be parsed from the saved change text"
    );
    assert!(
        !has_workspace_symbol(&host, "DiskOnly"),
        "newly discovered module should not fall back to stale disk contents"
    );
}

#[test]
fn reports_reverse_dependent_packages_as_affected() {
    let fixture = fixture_crate(
        r#"
//- /Cargo.toml
[workspace]
members = ["crates/dep", "crates/app"]
resolver = "3"

//- /crates/dep/Cargo.toml
[package]
name = "dep"
version = "0.1.0"
edition = "2024"

//- /crates/dep/src/lib.rs
pub struct Api;

//- /crates/app/Cargo.toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
dep = { path = "../dep" }

//- /crates/app/src/lib.rs
pub fn use_dep(_: dep::Api) {}
"#,
    );
    let workspace = WorkspaceMetadata::from_cargo(fixture.metadata());
    let mut host = AnalysisHost::build(workspace).expect("analysis host should build");
    let dep_path = fixture.path("crates/dep/src/lib.rs");

    let summary = host
        .apply_change(SavedFileChange::new(
            &dep_path,
            "pub struct Api;\npub struct Extra;\n",
        ))
        .expect("source change should apply");
    let affected = summary
        .affected_packages
        .iter()
        .map(|slot| {
            host.snapshot()
                .parse_db()
                .package(slot.0)
                .expect("affected package should exist")
                .package_name()
                .to_string()
        })
        .collect::<Vec<_>>();

    assert!(
        affected.iter().any(|name| name == "dep"),
        "changed package should be affected"
    );
    assert!(
        affected.iter().any(|name| name == "app"),
        "reverse dependent package should be affected"
    );
}

#[test]
fn rebuilds_reverse_dependent_packages_after_dependency_changes() {
    let (fixture, markers) = fixture_crate_with_markers(
        r#"
//- /Cargo.toml
[workspace]
members = ["crates/dep", "crates/app"]
resolver = "3"

//- /crates/dep/Cargo.toml
[package]
name = "dep"
version = "0.1.0"
edition = "2024"

//- /crates/dep/src/lib.rs
pub struct Api;

//- /crates/app/Cargo.toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
dep = { path = "../dep" }

//- /crates/app/src/lib.rs
pub fn use_dep(value: dep::Api) {
    let same = val$0ue;
}
"#,
    );
    let workspace = WorkspaceMetadata::from_cargo(fixture.metadata());
    let mut host = AnalysisHost::build(workspace).expect("analysis host should build");
    let marker = markers.position("0");
    let app_path = fixture.path(&marker.path);
    let dep_path = fixture.path("crates/dep/src/lib.rs");

    assert_eq!(
        nominal_type_names_at(&host, "app", &app_path, marker.offset),
        vec!["Api"],
        "initial app body should resolve the dependency type"
    );

    host.apply_change(SavedFileChange::new(&dep_path, "pub struct Renamed;\n"))
        .expect("dependency source change should apply");

    assert!(
        nominal_type_names_at(&host, "app", &app_path, marker.offset).is_empty(),
        "dependent package should be rebuilt against the updated dependency graph"
    );
}

#[test]
fn rebuilds_transitive_reverse_dependent_packages_after_dependency_changes() {
    let (fixture, markers) = fixture_crate_with_markers(
        r#"
//- /Cargo.toml
[workspace]
members = ["crates/dep", "crates/mid", "crates/app"]
resolver = "3"

//- /crates/dep/Cargo.toml
[package]
name = "dep"
version = "0.1.0"
edition = "2024"

//- /crates/dep/src/lib.rs
pub struct Api;

//- /crates/mid/Cargo.toml
[package]
name = "mid"
version = "0.1.0"
edition = "2024"

[dependencies]
dep = { path = "../dep" }

//- /crates/mid/src/lib.rs
pub fn make() -> dep::Api {
    loop {}
}

//- /crates/app/Cargo.toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
mid = { path = "../mid" }

//- /crates/app/src/lib.rs
pub fn use_mid() {
    let value = mid::make();
    let same = val$0ue;
}
"#,
    );
    let workspace = WorkspaceMetadata::from_cargo(fixture.metadata());
    let mut host = AnalysisHost::build(workspace).expect("analysis host should build");
    let marker = markers.position("0");
    let app_path = fixture.path(&marker.path);
    let dep_path = fixture.path("crates/dep/src/lib.rs");

    assert_eq!(
        nominal_type_names_at(&host, "app", &app_path, marker.offset),
        vec!["Api"],
        "initial app body should resolve through the middle crate"
    );

    let summary = host
        .apply_change(SavedFileChange::new(&dep_path, "pub struct Renamed;\n"))
        .expect("dependency source change should apply");
    let affected = affected_package_names(&host, &summary.affected_packages);

    assert_eq!(
        affected,
        vec!["app", "dep", "mid"],
        "changed dependency should rebuild all transitive reverse dependents"
    );
    assert!(
        nominal_type_names_at(&host, "app", &app_path, marker.offset).is_empty(),
        "transitive dependent package should be rebuilt against the updated dependency graph"
    );
}

fn has_workspace_symbol(host: &AnalysisHost, name: &str) -> bool {
    host.snapshot()
        .analysis()
        .workspace_symbols(name)
        .iter()
        .any(|symbol| symbol.name == name)
}

fn file_id_for_path(parse: &ParseDb, path: &Path) -> FileId {
    let canonical_path = path
        .canonicalize()
        .expect("fixture source path should canonicalize");

    parse
        .packages()
        .iter()
        .flat_map(|package| package.parsed_files())
        .find(|file| file.path() == canonical_path.as_path())
        .unwrap_or_else(|| panic!("fixture file {} should be parsed", path.display()))
        .file_id()
}

fn nominal_type_names_at(
    host: &AnalysisHost,
    package_name: &str,
    path: &Path,
    offset: u32,
) -> Vec<String> {
    let snapshot = host.snapshot();
    let package_slot = package_slot_by_name(snapshot.parse_db(), package_name);
    let file_id = file_id_for_path(snapshot.parse_db(), path);
    let target = snapshot
        .targets_for_file(package_slot, file_id)
        .into_iter()
        .next()
        .expect("fixture file should be owned by a target");
    let Some(ty) = snapshot.analysis().type_at(target, file_id, offset) else {
        return Vec::new();
    };

    ty.type_defs()
        .into_iter()
        .filter_map(|ty| snapshot.semantic_ir_db().local_def_for_type_def(ty))
        .filter_map(|local_def| snapshot.def_map_db().local_def(local_def))
        .map(|local_def| local_def.name.clone())
        .collect()
}

fn package_slot_by_name(parse: &ParseDb, package_name: &str) -> PackageSlot {
    parse
        .packages()
        .iter()
        .enumerate()
        .find_map(|(idx, package)| {
            (package.package_name() == package_name).then_some(PackageSlot(idx))
        })
        .unwrap_or_else(|| panic!("fixture package {package_name} should be parsed"))
}

fn affected_package_names(host: &AnalysisHost, packages: &[PackageSlot]) -> Vec<String> {
    let mut names = packages
        .iter()
        .map(|slot| {
            host.snapshot()
                .parse_db()
                .package(slot.0)
                .expect("affected package should exist")
                .package_name()
                .to_string()
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}
