mod utils;

use expect_test::expect;

#[test]
fn plans_cache_artifacts_from_workspace_metadata() {
    utils::check_cache_plan(
        r#"
//- /Cargo.toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
dep_alias = { path = "dep", package = "dep-pkg" }

[build-dependencies]
build_support = { path = "build-helper", package = "build-helper" }

[dev-dependencies]
dev_support = { path = "dev-helper", package = "dev-helper" }

[[example]]
name = "demo"
path = "examples/demo.rs"

[[test]]
name = "smoke"
path = "tests/smoke.rs"

//- /build.rs
fn main() {}

//- /src/lib.rs
pub struct App;

//- /src/main.rs
fn main() {}

//- /examples/demo.rs
fn main() {}

//- /tests/smoke.rs
#[test]
fn smoke() {}

//- /dep/Cargo.toml
[package]
name = "dep-pkg"
version = "0.1.0"
edition = "2021"

//- /dep/src/lib.rs
pub struct Dep;

//- /build-helper/Cargo.toml
[package]
name = "build-helper"
version = "0.1.0"
edition = "2021"

//- /build-helper/src/lib.rs
pub struct BuildHelper;

//- /dev-helper/Cargo.toml
[package]
name = "dev-helper"
version = "0.1.0"
edition = "2018"

//- /dev-helper/src/lib.rs
pub struct DevHelper;
"#,
        expect![[r#"
            package cache plan

            package #0 app
            schema 1
            id path+file://./#app@0.1.0
            source workspace
            edition 2024
            manifest Cargo.toml
            targets
            - app [lib] src/lib.rs
            - app [bin] src/main.rs
            - demo [example] examples/demo.rs
            - smoke [test] tests/smoke.rs
            - build-script-build [custom-build] build.rs
            dependencies
            - build_support -> build-helper (#1) [build]
            - dep_alias -> dep-pkg (#2) [normal]
            - dev_support -> dev-helper (#3) [dev]

            package #1 build-helper
            schema 1
            id path+file://./build-helper#0.1.0
            source path
            edition 2021
            manifest build-helper/Cargo.toml
            targets
            - build_helper [lib] build-helper/src/lib.rs
            dependencies
            - <none>

            package #2 dep-pkg
            schema 1
            id path+file://./dep#dep-pkg@0.1.0
            source path
            edition 2021
            manifest dep/Cargo.toml
            targets
            - dep_pkg [lib] dep/src/lib.rs
            dependencies
            - <none>

            package #3 dev-helper
            schema 1
            id path+file://./dev-helper#0.1.0
            source path
            edition 2018
            manifest dev-helper/Cargo.toml
            targets
            - dev_helper [lib] dev-helper/src/lib.rs
            dependencies
            - <none>
        "#]],
    );
}

#[test]
fn plans_package_artifact_paths_from_cache_store() {
    utils::check_cache_store_paths(
        r#"
//- /Cargo.toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
dep_alias = { path = "dep", package = "dep-pkg" }

//- /src/lib.rs
pub struct App;

//- /src/main.rs
fn main() {}

//- /dep/Cargo.toml
[package]
name = "dep-pkg"
version = "0.1.0"
edition = "2021"

//- /dep/src/lib.rs
pub struct Dep;
"#,
        expect![[r#"
            cache store `workspace target`
            root target/rust_glancer/<workspace>
            artifacts
            - #0 app a64a418c3750f4192bf6c1c07e4b4053307a5e7e58cd8d1de0a74ca571c59b9b
              target/rust_glancer/<workspace>/packages/package-0-app-a64a418c3750f4192bf6c1c07e4b4053307a5e7e58cd8d1de0a74ca571c59b9b.rgpkg
            - #1 dep-pkg 4fab8a4495a92cf24f5756ab41dd3167f5c05a54961703e0988b5361e86ed651
              target/rust_glancer/<workspace>/packages/package-1-dep-pkg-4fab8a4495a92cf24f5756ab41dd3167f5c05a54961703e0988b5361e86ed651.rgpkg

            cache store `custom target`
            root custom-target/rust_glancer/<workspace>
            artifacts
            - #0 app a64a418c3750f4192bf6c1c07e4b4053307a5e7e58cd8d1de0a74ca571c59b9b
              custom-target/rust_glancer/<workspace>/packages/package-0-app-a64a418c3750f4192bf6c1c07e4b4053307a5e7e58cd8d1de0a74ca571c59b9b.rgpkg
            - #1 dep-pkg 4fab8a4495a92cf24f5756ab41dd3167f5c05a54961703e0988b5361e86ed651
              custom-target/rust_glancer/<workspace>/packages/package-1-dep-pkg-4fab8a4495a92cf24f5756ab41dd3167f5c05a54961703e0988b5361e86ed651.rgpkg
        "#]],
    );
}
