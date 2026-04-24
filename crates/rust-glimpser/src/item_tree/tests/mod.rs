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
fn resolves_out_of_line_files_inside_inline_modules() {
    utils::check_project_item_tree(
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

            targets
            - nested_module_fixture [lib] -> lib.rs

            files
            file lib.rs
            - pub module outer [inline]
              - pub module child [out_of_line child.rs]
            - pub use outer::child::work
              - import named outer::child::work

            file child.rs
            - pub fn work
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
fn dumps_declaration_payloads() {
    utils::check_project_item_tree_with_declarations(
        r#"
//- /Cargo.toml
[package]
name = "declaration_fixture"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User<T>
where
    T: Clone,
{
    pub id: UserId,
    payload: Option<T>,
}

pub enum LoadState<E> {
    Empty,
    Loaded(User),
    Failed { error: E },
}

pub trait Repository<T>: Send
where
    T: Clone,
{
    type Error;
    const KIND: &'static str;
    fn get(&self, id: UserId) -> Result<T, Self::Error>;
}

impl<T> Repository<T> for DbRepository<T>
where
    T: Clone,
{
    type Error = DbError;
    const KIND: &'static str = "db";
    fn get(&self, id: UserId) -> Result<T, DbError> {
        todo!()
    }
}

pub type UserResult<T> = Result<User<T>, DbError>;
pub const DEFAULT_ID: UserId = UserId(0);
pub static mut CACHE_READY: bool = false;
"#,
        expect![[r#"
            package declaration_fixture

            targets
            - declaration_fixture [lib] -> lib.rs

            files
            file lib.rs
            - pub struct User
              - generics <T> where T: Clone
              - pub field id: UserId
              - field payload: Option<T>
            - pub enum LoadState
              - generics <E>
              - variant Empty
              - variant Loaded
                - field #0: User
              - variant Failed
                - field error: E
            - pub trait Repository
              - generics <T> where T: Clone
              - supertraits Send
              - type_alias Error
              - const KIND
                - ty &'static str
              - fn get
                - params (&self, id: UserId)
                - ret Result<T, Self::Error>
            - impl
              - generics <T> where T: Clone
              - trait Repository<T>
              - self DbRepository<T>
              - type_alias Error
                - aliased DbError
              - const KIND
                - ty &'static str
              - fn get
                - params (&self, id: UserId)
                - ret Result<T, DbError>
            - pub type_alias UserResult
              - generics <T>
              - aliased Result<User<T>, DbError>
            - pub const DEFAULT_ID
              - ty UserId
            - pub static CACHE_READY
              - ty bool
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
