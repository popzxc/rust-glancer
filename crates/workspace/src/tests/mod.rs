mod utils;

use std::collections::BTreeSet;

use expect_test::expect;
use test_fixture::fixture_crate;

use crate::WorkspaceMetadata;

#[test]
fn dumps_normalized_workspace_metadata() {
    utils::check_workspace_metadata(
        r#"
//- /Cargo.toml
[workspace]
members = ["crates/app", "crates/dep"]
resolver = "3"

//- /crates/app/Cargo.toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
dep_alias = { path = "../dep", package = "dep" }

[build-dependencies]
build_support = { path = "../../vendor/build_helper", package = "build_helper" }

[dev-dependencies]
dev_support = { path = "../../vendor/dev_helper", package = "dev_helper" }

[[example]]
name = "demo"
path = "examples/demo.rs"

[[test]]
name = "smoke"
path = "tests/smoke.rs"

[[bench]]
name = "api"
path = "benches/api.rs"

//- /crates/app/build.rs
fn main() {}

//- /crates/app/src/lib.rs
pub fn work() {}

//- /crates/app/src/main.rs
fn main() {}

//- /crates/app/examples/demo.rs
fn main() {}

//- /crates/app/tests/smoke.rs
#[test]
fn smoke() {}

//- /crates/app/benches/api.rs
fn main() {}

//- /crates/dep/Cargo.toml
[package]
name = "dep"
version = "0.1.0"
edition = "2024"

[dependencies]
helper_tools = { path = "../../vendor/helper", package = "helper" }

//- /crates/dep/src/lib.rs
pub fn dep() {}

//- /vendor/helper/Cargo.toml
[package]
name = "helper"
version = "0.1.0"
edition = "2024"

//- /vendor/helper/src/lib.rs
pub fn helper() {}

//- /vendor/build_helper/Cargo.toml
[package]
name = "build_helper"
version = "0.1.0"
edition = "2024"

//- /vendor/build_helper/src/lib.rs
pub fn build_helper() {}

//- /vendor/dev_helper/Cargo.toml
[package]
name = "dev_helper"
version = "0.1.0"
edition = "2024"

//- /vendor/dev_helper/src/lib.rs
pub fn dev_helper() {}
"#,
        expect![[r#"
            workspace .

            package app [member]
            manifest crates/app/Cargo.toml
            edition 2024
            targets
            - app [lib] crates/app/src/lib.rs
            - app [bin] crates/app/src/main.rs
            - demo [example] crates/app/examples/demo.rs
            - smoke [test] crates/app/tests/smoke.rs
            - api [bench] crates/app/benches/api.rs
            - build-script-build [custom-build] crates/app/build.rs
            dependencies
            - build_support -> build_helper [build]
            - dep_alias -> dep
            - dev_support -> dev_helper [dev]

            package build_helper [member]
            manifest vendor/build_helper/Cargo.toml
            edition 2024
            targets
            - build_helper [lib] vendor/build_helper/src/lib.rs
            dependencies
            - <none>

            package dep [member]
            manifest crates/dep/Cargo.toml
            edition 2024
            targets
            - dep [lib] crates/dep/src/lib.rs
            dependencies
            - helper_tools -> helper

            package dev_helper [member]
            manifest vendor/dev_helper/Cargo.toml
            edition 2024
            targets
            - dev_helper [lib] vendor/dev_helper/src/lib.rs
            dependencies
            - <none>

            package helper [member]
            manifest vendor/helper/Cargo.toml
            edition 2024
            targets
            - helper [lib] vendor/helper/src/lib.rs
            dependencies
            - <none>
        "#]],
    );
}

#[test]
fn injects_sysroot_packages_as_normalized_dependencies() {
    utils::check_workspace_metadata_with_sysroot(
        r#"
//- /Cargo.toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct App;

//- /sysroot/library/core/src/lib.rs
pub mod marker {
    pub struct Core;
}

//- /sysroot/library/alloc/src/lib.rs
pub mod marker {
    pub struct Alloc;
}

//- /sysroot/library/std/src/lib.rs
pub mod marker {
    pub struct Std;
}
"#,
        expect![[r#"
            workspace .

            package alloc [sysroot]
            manifest sysroot/library/alloc/Cargo.toml
            edition 2024
            targets
            - alloc [lib] sysroot/library/alloc/src/lib.rs
            dependencies
            - core -> core

            package app [member]
            manifest Cargo.toml
            edition 2024
            targets
            - app [lib] src/lib.rs
            dependencies
            - alloc -> alloc [normal, build, dev]
            - core -> core [normal, build, dev]
            - std -> std [normal, build, dev]

            package core [sysroot]
            manifest sysroot/library/core/Cargo.toml
            edition 2024
            targets
            - core [lib] sysroot/library/core/src/lib.rs
            dependencies
            - <none>

            package std [sysroot]
            manifest sysroot/library/std/Cargo.toml
            edition 2024
            targets
            - std [lib] sysroot/library/std/src/lib.rs
            dependencies
            - alloc -> alloc
            - core -> core
        "#]],
    );
}

#[test]
fn computes_transitive_reverse_dependency_closure() {
    let fixture = fixture_crate(
        r#"
//- /Cargo.toml
[workspace]
members = ["crates/dep", "crates/mid", "crates/app", "crates/independent"]
resolver = "3"

//- /crates/dep/Cargo.toml
[package]
name = "dep"
version = "0.1.0"
edition = "2024"

//- /crates/dep/src/lib.rs
pub struct Dep;

//- /crates/mid/Cargo.toml
[package]
name = "mid"
version = "0.1.0"
edition = "2024"

[dependencies]
dep = { path = "../dep" }

//- /crates/mid/src/lib.rs
pub struct Mid(dep::Dep);

//- /crates/app/Cargo.toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
mid = { path = "../mid" }

//- /crates/app/src/lib.rs
pub struct App(mid::Mid);

//- /crates/independent/Cargo.toml
[package]
name = "independent"
version = "0.1.0"
edition = "2024"

//- /crates/independent/src/lib.rs
pub struct Independent;
"#,
    );
    let workspace = WorkspaceMetadata::from_cargo(fixture.metadata());
    let dep_id = workspace
        .packages()
        .iter()
        .find(|package| package.name == "dep")
        .expect("dep package should exist")
        .id
        .clone();
    let affected_names = workspace
        .reverse_dependency_closure(&[dep_id])
        .into_iter()
        .map(|slot| workspace.packages()[slot].name.clone())
        .collect::<BTreeSet<_>>();

    assert_eq!(
        affected_names,
        BTreeSet::from(["app".to_string(), "dep".to_string(), "mid".to_string()]),
        "reverse dependency closure should include transitive dependents only"
    );
}
