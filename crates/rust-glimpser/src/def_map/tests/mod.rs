mod utils;

use expect_test::expect;

use crate::test_utils::fixture_crate;

#[test]
fn dumps_workspace_resolution_flow() {
    utils::check_project_def_map(
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
mod hidden {
    pub struct Thing;
}

pub use hidden::Thing;
pub fn work() {}

//- /crates/app/Cargo.toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
dep = { path = "../dep" }

//- /crates/app/src/lib.rs
pub mod nested;

mod source {
    pub fn greet() {}
}

mod middle {
    pub use crate::source::*;
}

mod final_mod {
    pub use crate::middle::*;
}

extern crate dep as dep_alias;

use dep::Thing;
use dep_alias::work as dep_work;
use crate::nested::local_work;
use final_mod::greet;

pub fn make(_: Thing) {
    greet();
    dep_work();
    local_work();
}

//- /crates/app/src/nested.rs
pub fn local_work() {}

//- /crates/app/src/main.rs
use app::make;

fn main() {
    let _ = make;
}
"#,
        expect![[r#"
            package app

            app [lib]
            crate
            - Thing : type [struct dep[lib]::crate::hidden::Thing]
            - dep_alias : type [module dep[lib]::crate]
            - dep_work : value [fn dep[lib]::crate::work]
            - final_mod : type [module app[lib]::crate::final_mod]
            - greet : value [fn app[lib]::crate::source::greet]
            - local_work : value [fn app[lib]::crate::nested::local_work]
            - make : value [pub fn app[lib]::crate::make]
            - middle : type [module app[lib]::crate::middle]
            - nested : type [pub module app[lib]::crate::nested]
            - source : type [module app[lib]::crate::source]

            crate::final_mod
            - greet : value [pub fn app[lib]::crate::source::greet]

            crate::middle
            - greet : value [pub fn app[lib]::crate::source::greet]

            crate::nested
            - local_work : value [pub fn app[lib]::crate::nested::local_work]

            crate::source
            - greet : value [pub fn app[lib]::crate::source::greet]

            app [bin]
            crate
            - main : value [fn app[bin]::crate::main]
            - make : value [fn app[lib]::crate::make]

            package dep

            dep [lib]
            crate
            - Thing : type [pub struct dep[lib]::crate::hidden::Thing]
            - hidden : type [module dep[lib]::crate::hidden]
            - work : value [pub fn dep[lib]::crate::work]

            crate::hidden
            - Thing : type [pub struct dep[lib]::crate::hidden::Thing]
        "#]],
    );
}

#[test]
fn resolves_reexports_from_out_of_line_files_inside_inline_modules() {
    utils::check_project_def_map(
        r#"
//- /Cargo.toml
[package]
name = "nested_module_fixture"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub mod outer {
    pub mod child;
}

pub use outer::child::work;

//- /src/outer/child.rs
pub fn work() {}
"#,
        expect![[r#"
            package nested_module_fixture

            nested_module_fixture [lib]
            crate
            - outer : type [pub module nested_module_fixture[lib]::crate::outer]
            - work : value [pub fn nested_module_fixture[lib]::crate::outer::child::work]

            crate::outer
            - child : type [pub module nested_module_fixture[lib]::crate::outer::child]

            crate::outer::child
            - work : value [pub fn nested_module_fixture[lib]::crate::outer::child::work]
        "#]],
    );
}

#[test]
fn keeps_type_and_value_bindings_separate() {
    let fixture = fixture_crate!(
        r#"
//- /Cargo.toml
[package]
name = "namespace_fixture"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct Thing;

#[allow(non_snake_case)]
pub fn Thing() -> Thing {
    Thing
}
"#
    );
    let project = fixture.analyze();

    project
        .lib("namespace_fixture")
        .entry("Thing")
        .assert_type_exists("type namespace should keep the struct")
        .assert_value_exists("value namespace should keep the function");
}

#[test]
fn resolves_nested_self_imports_without_binding_literal_self() {
    let fixture = fixture_crate!(
        r#"
//- /Cargo.toml
[package]
name = "self_import_fixture"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
mod bar {
    pub mod foo {
        pub fn work() {}
    }
}

use bar::foo::{self, self as imported_foo, work};
"#
    );
    let project = fixture.analyze();
    let target = project.lib("self_import_fixture");

    target.entry("foo").assert_module_named(
        "foo",
        "nested self imports should bind the referenced module under its own name",
    );
    target.entry("imported_foo").assert_module_named(
        "foo",
        "aliased nested self imports should keep the referenced module under the alias",
    );
    target
        .entry("work")
        .assert_value_exists("nested self imports should not interfere with sibling imports");
    target
        .entry("self")
        .assert_missing("nested self imports should not leak a literal `self` binding");
}

#[test]
fn ignores_hidden_renames() {
    let fixture = fixture_crate!(
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
pub fn work() {}

//- /crates/app/Cargo.toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
dep = { path = "../dep" }

//- /crates/app/src/lib.rs
mod bar {
    pub fn work() {}
}

extern crate dep as _;
use bar::work as _;
"#
    );
    let project = fixture.analyze();
    let target = project.lib("app");

    target
        .entry("bar")
        .assert_type_exists("hidden renames should not remove unrelated local bindings");
    target
        .entry("dep")
        .assert_missing("hidden extern crate renames should not bind the dependency name");
    target
        .entry("work")
        .assert_missing("hidden use renames should not bind the imported item name");
}
