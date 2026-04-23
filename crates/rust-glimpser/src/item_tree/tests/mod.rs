mod utils;

use expect_test::expect;

#[test]
fn dumps_lib_and_bin_item_trees() {
    utils::check_project_item_tree(
        r#"
//- /Cargo.toml
[package]
name = "moderate_crate"
version = "0.1.0"
edition = "2024"

[lib]
path = "src/lib.rs"

[[bin]]
name = "moderate_crate"
path = "src/main.rs"

//- /src/lib.rs
pub mod cli;
pub mod model;

//- /src/model.rs
pub struct Model;

impl Model {
    pub fn new() -> Self {
        Self
    }
}

//- /src/cli.rs
pub fn run() {}

//- /src/main.rs
use std::path::PathBuf;
use moderate_crate::cli::run;

fn main() {
    let _path = PathBuf::new();
    run();
}
"#,
        expect![[r#"
            package moderate_crate

            targets
            - moderate_crate [lib] -> lib.rs

            - moderate_crate [bin] -> main.rs

            files
            file cli.rs
            - pub fn run

            file lib.rs
            - pub module cli [out_of_line cli.rs]
            - pub module model [out_of_line model.rs]

            file main.rs
            - use std::path::PathBuf
              - import named std::path::PathBuf
            - use moderate_crate::cli::run
              - import named moderate_crate::cli::run
            - fn main

            file model.rs
            - pub struct Model
            - impl
        "#]],
    );
}

#[test]
fn dumps_import_payloads() {
    utils::check_project_item_tree(
        r#"
//- /Cargo.toml
[package]
name = "import_crate"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub mod bar {
    pub mod foo {}
}

extern crate self as current;
extern crate self as _;

use bar::foo::{self, self as imported_foo, work as _, *};
use crate::bar::foo::work as run;
use ::bar::foo;
"#,
        expect![[r#"
            package import_crate

            targets
            - import_crate [lib] -> lib.rs

            files
            file lib.rs
            - pub module bar [inline]
              - pub module foo [inline]
            - extern_crate self [self as current]
            - extern_crate self [self as _]
            - use bar::foo::{self, self as imported_foo, work as _, *}
              - import self bar::foo
              - import self bar::foo as imported_foo
              - import named bar::foo::work as _
              - import glob bar::foo
            - use crate::bar::foo::work as run
              - import named crate::bar::foo::work as run
            - use ::bar::foo
              - import named ::bar::foo
        "#]],
    );
}

#[test]
fn dumps_macro_item_trees() {
    utils::check_project_item_tree(
        r#"
//- /Cargo.toml
[package]
name = "complex_crate"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
macro_rules! label_result {
    ($value:expr) => {
        $value
    };
}

pub fn decorate(input: &str) -> &str {
    label_result!(input)
}
"#,
        expect![[r#"
            package complex_crate

            targets
            - complex_crate [lib] -> lib.rs

            files
            file lib.rs
            - macro_definition label_result
            - pub fn decorate
        "#]],
    );
}

#[test]
fn dumps_item_spans() {
    utils::check_project_item_tree_with_spans(
        r#"
//- /Cargo.toml
[package]
name = "simple_crate"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub fn add_two_numbers(left: i32, right: i32) -> i32 {
    left + right
}
"#,
        expect![[r#"
            package simple_crate

            targets
            - simple_crate [lib] -> lib.rs

            files
            file lib.rs
            - pub fn add_two_numbers [lib.rs 1:1-3:2 (0..73)]
        "#]],
    );
}
