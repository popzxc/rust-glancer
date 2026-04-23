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

            target moderate_crate [lib]
            - pub module cli
              - pub fn run
            - pub module model
              - pub struct Model
              - impl
                - pub associated_fn new

            target moderate_crate [bin]
            - use std::path::PathBuf
            - use moderate_crate::cli::run
            - fn main
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

            target complex_crate [lib]
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

            target simple_crate [lib]
            - pub fn add_two_numbers [lib.rs 1:1-3:2 (0..73)]
        "#]],
    );
}
