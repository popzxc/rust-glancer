use std::sync::Arc;

use rg_item_tree::ItemTreeDb;
use rg_parse::ParseDb;
use rg_workspace::WorkspaceMetadata;
use test_fixture::fixture_crate;

use crate::{DefMapDb, PackageSlot, TargetRef};

#[test]
fn rebuild_resolves_dirty_imports_through_frozen_packages_without_replacing_them() {
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
pub mod api {
    pub struct Api;
}

//- /crates/app/Cargo.toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
dep = { path = "../dep" }

//- /crates/app/src/lib.rs
pub use dep::api::Api as Before;
"#,
    );
    let workspace = WorkspaceMetadata::from_cargo(fixture.metadata())
        .expect("fixture workspace metadata should build");
    let mut parse = ParseDb::build(&workspace).expect("fixture parse db should build");
    let item_tree = ItemTreeDb::build(&mut parse).expect("fixture item-tree db should build");
    let old =
        DefMapDb::build(&workspace, &parse, &item_tree).expect("fixture def-map db should build");

    fixture.write_fixture_files(
        r#"
//- /crates/app/src/lib.rs
pub use dep::api::Api as Renamed;
"#,
    );

    let mut parse = ParseDb::build(&workspace).expect("updated fixture parse db should build");
    let item_tree =
        ItemTreeDb::build(&mut parse).expect("updated fixture item-tree db should build");

    let mut app_slot = None;
    let mut dep_slot = None;
    for (package_idx, package) in parse.packages().iter().enumerate() {
        match package.package_name() {
            "app" => app_slot = Some(PackageSlot(package_idx)),
            "dep" => dep_slot = Some(PackageSlot(package_idx)),
            _ => {}
        }
    }
    let app_slot = app_slot.expect("fixture app package should exist");
    let dep_slot = dep_slot.expect("fixture dep package should exist");

    let rebuilt = old
        .rebuild_packages(&workspace, &parse, &item_tree, &[app_slot])
        .expect("fixture def-map package rebuild should succeed");

    // The dependency is not dirty, so it should remain the exact frozen package payload from the
    // previous snapshot while the dirty package resolves imports through it.
    let old_dep = old
        .packages
        .get(dep_slot)
        .expect("old dependency package should exist");
    let rebuilt_dep = rebuilt
        .packages
        .get(dep_slot)
        .expect("rebuilt dependency package should exist");
    assert!(
        Arc::ptr_eq(old_dep, rebuilt_dep),
        "clean dependency package should stay shared across package rebuilds"
    );

    let old_app = old
        .packages
        .get(app_slot)
        .expect("old app package should exist");
    let rebuilt_app = rebuilt
        .packages
        .get(app_slot)
        .expect("rebuilt app package should exist");
    assert!(
        !Arc::ptr_eq(old_app, rebuilt_app),
        "dirty app package should be replaced by the rebuild"
    );

    let app_package = parse
        .package(app_slot.0)
        .expect("fixture app package should exist after rebuild");
    let app_lib = app_package
        .targets()
        .iter()
        .find(|target| target.kind.is_lib())
        .expect("fixture app package should have a library target");
    let app_target = TargetRef {
        package: app_slot,
        target: app_lib.id,
    };
    let app_def_map = rebuilt
        .def_map(app_target)
        .expect("rebuilt app def-map should exist");
    let root_module = app_def_map
        .root_module()
        .expect("rebuilt app def-map should have a root module");
    let root = app_def_map
        .module(root_module)
        .expect("rebuilt app root module should exist");
    let renamed_entry = root
        .scope
        .entry("Renamed")
        .expect("rebuilt app root should contain the renamed import");

    assert!(
        !renamed_entry.types().is_empty(),
        "dirty app import should resolve through the clean frozen dependency package"
    );
    assert!(
        root.unresolved_imports.is_empty(),
        "dirty app import through the clean dependency should not be recorded as unresolved"
    );
}
