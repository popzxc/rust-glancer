use super::{DefId, PackageSlot, ScopeEntry};
use crate::{
    parse::{ProjectAnalysis, package::PackageIndex, target::TargetIndex},
    test_fixture::{CrateFixture, fixture_crate},
};

fn analyze_fixture(fixture: &CrateFixture) -> ProjectAnalysis {
    ProjectAnalysis::build(fixture.metadata()).expect("fixture project should analyze")
}

fn package_by_name<'a>(analysis: &'a ProjectAnalysis, package_name: &str) -> &'a PackageIndex {
    analysis
        .packages
        .iter()
        .find(|package| package.package_name() == package_name)
        .expect("package should exist")
}

fn package_slot_by_name(analysis: &ProjectAnalysis, package_name: &str) -> PackageSlot {
    PackageSlot(
        analysis
            .packages
            .iter()
            .position(|package| package.package_name() == package_name)
            .expect("package should exist"),
    )
}

fn target_by_kind<'a>(
    package: &'a PackageIndex,
    kind: cargo_metadata::TargetKind,
) -> &'a TargetIndex {
    package
        .targets
        .iter()
        .find(|target| {
            target
                .cargo_target
                .kind
                .iter()
                .any(|target_kind| target_kind == &kind)
        })
        .expect("target should exist")
}

fn root_scope_entry<'a>(target: &'a TargetIndex, name: &str) -> &'a ScopeEntry {
    let root_module = target
        .def_map
        .root_module()
        .expect("target should have a root module");
    target
        .module(root_module)
        .and_then(|module| module.scope.entry(name))
        .expect("root scope entry should exist")
}

fn root_scope_has_name(target: &TargetIndex, name: &str) -> bool {
    let root_module = target
        .def_map
        .root_module()
        .expect("target should have a root module");
    target
        .module(root_module)
        .and_then(|module| module.scope.entry(name))
        .is_some()
}

#[test]
fn keeps_type_and_value_bindings_separate() {
    let fixture = fixture_crate! {
        "Cargo.toml" => r#"
[package]
name = "namespace_fixture"
version = "0.1.0"
edition = "2024"
"#,
        "src/lib.rs" => r#"
pub struct Thing;

#[allow(non_snake_case)]
pub fn Thing() -> Thing {
    Thing
}
"#,
    };

    let analysis = analyze_fixture(&fixture);
    let package = package_by_name(&analysis, "namespace_fixture");
    let target = target_by_kind(package, cargo_metadata::TargetKind::Lib);
    let entry = root_scope_entry(target, "Thing");

    assert_eq!(
        entry.types.len(),
        1,
        "type namespace should keep the struct"
    );
    assert_eq!(
        entry.values.len(),
        1,
        "value namespace should keep the function"
    );
}

#[test]
fn resolves_transitive_glob_imports_to_fixed_point() {
    let fixture = fixture_crate! {
        "Cargo.toml" => r#"
[package]
name = "glob_fixture"
version = "0.1.0"
edition = "2024"
"#,
        "src/lib.rs" => r#"
mod source {
    pub fn greet() {}
}

mod middle {
    pub use crate::source::*;
}

mod final_mod {
    pub use crate::middle::*;
}

use final_mod::greet;
"#,
    };

    let analysis = analyze_fixture(&fixture);
    let package = package_by_name(&analysis, "glob_fixture");
    let target = target_by_kind(package, cargo_metadata::TargetKind::Lib);
    let entry = root_scope_entry(target, "greet");

    assert_eq!(
        entry.values.len(),
        1,
        "transitive glob imports should eventually expose the function"
    );
}

#[test]
fn resolves_imports_through_out_of_line_modules() {
    let fixture = fixture_crate! {
        "Cargo.toml" => r#"
[package]
name = "outline_fixture"
version = "0.1.0"
edition = "2024"
"#,
        "src/lib.rs" => r#"
pub mod nested;

use crate::nested::work;
"#,
        "src/nested.rs" => r#"
pub fn work() {}
"#,
    };

    let analysis = analyze_fixture(&fixture);
    let package = package_by_name(&analysis, "outline_fixture");
    let target = target_by_kind(package, cargo_metadata::TargetKind::Lib);
    let entry = root_scope_entry(target, "work");

    assert_eq!(
        entry.values.len(),
        1,
        "out-of-line module items should participate in import resolution"
    );
}

#[test]
fn resolves_nested_self_imports_under_the_module_name() {
    let fixture = fixture_crate! {
        "Cargo.toml" => r#"
[package]
name = "self_import_fixture"
version = "0.1.0"
edition = "2024"
"#,
        "src/lib.rs" => r#"
mod bar {
    pub mod foo {
        pub fn work() {}
    }
}

use bar::foo::{self, work};
"#,
    };

    let analysis = analyze_fixture(&fixture);
    let package = package_by_name(&analysis, "self_import_fixture");
    let target = target_by_kind(package, cargo_metadata::TargetKind::Lib);
    let foo_entry = root_scope_entry(target, "foo");
    let work_entry = root_scope_entry(target, "work");

    assert!(
        matches!(
            foo_entry.types.first().map(|binding| binding.def),
            Some(DefId::Module(module_ref))
                if target
                    .module(module_ref.module)
                    .and_then(|module| module.name.as_deref())
                    == Some("foo")
        ),
        "nested self imports should bind the imported module under its module name"
    );
    assert_eq!(
        work_entry.values.len(),
        1,
        "self imports should still coexist with sibling named imports"
    );
    assert!(
        !root_scope_has_name(target, "self"),
        "nested self imports should not introduce a literal `self` binding"
    );
}

#[test]
fn resolves_aliased_nested_self_imports() {
    let fixture = fixture_crate! {
        "Cargo.toml" => r#"
[package]
name = "self_import_alias_fixture"
version = "0.1.0"
edition = "2024"
"#,
        "src/lib.rs" => r#"
mod bar {
    pub mod foo {
        pub fn work() {}
    }
}

use bar::foo::{self as imported_foo};
"#,
    };

    let analysis = analyze_fixture(&fixture);
    let package = package_by_name(&analysis, "self_import_alias_fixture");
    let target = target_by_kind(package, cargo_metadata::TargetKind::Lib);
    let alias_entry = root_scope_entry(target, "imported_foo");

    assert!(
        matches!(
            alias_entry.types.first().map(|binding| binding.def),
            Some(DefId::Module(module_ref))
                if target
                    .module(module_ref.module)
                    .and_then(|module| module.name.as_deref())
                    == Some("foo")
        ),
        "aliased self imports should keep the imported module and bind it under the alias"
    );
    assert!(
        !root_scope_has_name(target, "self"),
        "aliased self imports should not leak a literal `self` binding"
    );
}

#[test]
fn ignores_hidden_use_renames() {
    let fixture = fixture_crate! {
        "Cargo.toml" => r#"
[package]
name = "hidden_use_fixture"
version = "0.1.0"
edition = "2024"
"#,
        "src/lib.rs" => r#"
mod bar {
    pub fn work() {}
}

use bar::work as _;
"#,
    };

    let analysis = analyze_fixture(&fixture);
    let package = package_by_name(&analysis, "hidden_use_fixture");
    let target = target_by_kind(package, cargo_metadata::TargetKind::Lib);

    assert!(
        !root_scope_has_name(target, "work"),
        "hidden use renames should not introduce a visible binding"
    );
}

#[test]
fn ignores_hidden_extern_crate_renames() {
    let fixture = fixture_crate! {
        "Cargo.toml" => r#"
[workspace]
members = ["crates/dep", "crates/app"]
resolver = "3"
"#,
        "crates/dep/Cargo.toml" => r#"
[package]
name = "dep"
version = "0.1.0"
edition = "2024"
"#,
        "crates/dep/src/lib.rs" => r#"
pub fn work() {}
"#,
        "crates/app/Cargo.toml" => r#"
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
dep = { path = "../dep" }
"#,
        "crates/app/src/lib.rs" => r#"
extern crate dep as _;
"#,
    };

    let analysis = analyze_fixture(&fixture);
    let app_package = package_by_name(&analysis, "app");
    let app_target = target_by_kind(app_package, cargo_metadata::TargetKind::Lib);

    assert!(
        !root_scope_has_name(app_target, "dep"),
        "hidden extern crate renames should not introduce a visible binding"
    );
}

#[test]
fn resolves_same_package_library_root_from_bin_target() {
    let fixture = fixture_crate! {
        "Cargo.toml" => r#"
[package]
name = "same_package_fixture"
version = "0.1.0"
edition = "2024"

[lib]
path = "src/lib.rs"

[[bin]]
name = "same_package_fixture"
path = "src/main.rs"
"#,
        "src/lib.rs" => r#"
pub fn build_report() {}
"#,
        "src/main.rs" => r#"
use same_package_fixture::build_report;

fn main() {
    build_report();
}
"#,
    };

    let analysis = analyze_fixture(&fixture);
    let package = package_by_name(&analysis, "same_package_fixture");
    let lib_target = target_by_kind(package, cargo_metadata::TargetKind::Lib);
    let bin_target = target_by_kind(package, cargo_metadata::TargetKind::Bin);
    let entry = root_scope_entry(bin_target, "build_report");

    assert!(
        matches!(
            entry.values.first().map(|binding| binding.def),
            Some(DefId::Local(local_def_ref)) if local_def_ref.target.target == lib_target.id
        ),
        "bin target should resolve imports through the same-package library target"
    );
}

#[test]
fn resolves_public_reexports_from_dependency_targets() {
    let fixture = fixture_crate! {
        "Cargo.toml" => r#"
[workspace]
members = ["crates/dep", "crates/app"]
resolver = "3"
"#,
        "crates/dep/Cargo.toml" => r#"
[package]
name = "dep"
version = "0.1.0"
edition = "2024"
"#,
        "crates/dep/src/lib.rs" => r#"
mod hidden {
    pub struct Thing;
}

pub use hidden::Thing;
"#,
        "crates/app/Cargo.toml" => r#"
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
dep = { path = "../dep" }
"#,
        "crates/app/src/lib.rs" => r#"
use dep::Thing;

pub fn make(_: Thing) {}
"#,
    };

    let analysis = analyze_fixture(&fixture);
    let dep_slot = package_slot_by_name(&analysis, "dep");
    let dep_package = package_by_name(&analysis, "dep");
    let dep_target = target_by_kind(dep_package, cargo_metadata::TargetKind::Lib);
    let app_package = package_by_name(&analysis, "app");
    let app_target = target_by_kind(app_package, cargo_metadata::TargetKind::Lib);
    let entry = root_scope_entry(app_target, "Thing");

    assert!(
        matches!(
            entry.types.first().map(|binding| binding.def),
            Some(DefId::Local(local_def_ref))
                if local_def_ref.target.package == dep_slot
                    && local_def_ref.target.target == dep_target.id
        ),
        "public reexports from dependencies should be visible to dependents"
    );
}

#[test]
fn supports_extern_crate_aliases() {
    let fixture = fixture_crate! {
        "Cargo.toml" => r#"
[workspace]
members = ["crates/dep", "crates/app"]
resolver = "3"
"#,
        "crates/dep/Cargo.toml" => r#"
[package]
name = "dep"
version = "0.1.0"
edition = "2024"
"#,
        "crates/dep/src/lib.rs" => r#"
pub fn work() {}
"#,
        "crates/app/Cargo.toml" => r#"
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
dep = { path = "../dep" }
"#,
        "crates/app/src/lib.rs" => r#"
extern crate dep as dep_alias;

use dep_alias::work;
"#,
    };

    let analysis = analyze_fixture(&fixture);
    let dep_slot = package_slot_by_name(&analysis, "dep");
    let dep_package = package_by_name(&analysis, "dep");
    let dep_target = target_by_kind(dep_package, cargo_metadata::TargetKind::Lib);
    let app_package = package_by_name(&analysis, "app");
    let app_target = target_by_kind(app_package, cargo_metadata::TargetKind::Lib);
    let entry = root_scope_entry(app_target, "work");

    assert!(
        matches!(
            entry.values.first().map(|binding| binding.def),
            Some(DefId::Local(local_def_ref))
                if local_def_ref.target.package == dep_slot
                    && local_def_ref.target.target == dep_target.id
        ),
        "extern crate aliases should seed names that later imports can resolve through"
    );
}
