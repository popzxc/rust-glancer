use expect_test::expect;

use super::utils::{AnalysisQuery, check_analysis_queries};

#[test]
fn resolves_body_references_to_definition_targets() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_goto_definition"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User;

pub fn helper() -> User {
    User
}

pub fn use_it() {
    let local: User = helper();
    let _again: User = loc$goto_local$al;
    let _made: User = hel$goto_item$per();
}
"#,
        &[
            AnalysisQuery::goto("goto local", "goto_local"),
            AnalysisQuery::goto("goto item", "goto_item"),
        ],
        expect![[r#"
            goto local
            - local local @ 8:9-8:14

            goto item
            - fn helper @ 3:1-5:2
        "#]],
    );
}

#[test]
fn resolves_binding_declarations_to_themselves() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_binding_goto"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User;

pub fn helper() -> User {
    User
}

pub fn use_it() {
    let loc$goto_decl$al: User = helper();
}
"#,
        &[AnalysisQuery::goto("goto declaration binding", "goto_decl")],
        expect![[r#"
            goto declaration binding
            - local local @ 8:9-8:14
        "#]],
    );
}

#[test]
fn resolves_field_accesses_to_field_declarations() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_field_goto"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct Profile;

pub struct User {
    pub profile: Profile,
}

pub fn use_it(user: User) {
    let _profile: Profile = user.pro$goto_field$file;
}
"#,
        &[AnalysisQuery::goto("goto field", "goto_field")],
        expect![[r#"
            goto field
            - field profile @ 4:9-4:16
        "#]],
    );
}

#[test]
fn resolves_tuple_field_accesses_to_tuple_field_declarations() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_tuple_field_goto"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct Left;
pub struct Right;

pub struct Pair(pub Left, pub Right);

pub fn use_it(pair: Pair) {
    let _left: Left = pair.$goto_tuple_field$0;
}
"#,
        &[AnalysisQuery::goto("goto tuple field", "goto_tuple_field")],
        expect![[r#"
            goto tuple field
            - field #0 @ 4:17-4:25
        "#]],
    );
}

#[test]
fn resolves_use_and_signature_paths_to_definitions() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_signature_goto"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
mod api {
    pub trait Named {}
    pub struct User;
}

use a$goto_use_module$pi::Us$goto_use$er;

impl api::Na$goto_impl_trait$med for Us$goto_impl_self$er {}

pub fn make(user: Us$goto_param$er) -> Us$goto_ret$er {
    user
}
"#,
        &[
            AnalysisQuery::goto("goto use module", "goto_use_module"),
            AnalysisQuery::goto("goto use path", "goto_use"),
            AnalysisQuery::goto("goto impl trait", "goto_impl_trait"),
            AnalysisQuery::goto("goto impl self type", "goto_impl_self"),
            AnalysisQuery::goto("goto parameter type", "goto_param"),
            AnalysisQuery::goto("goto return type", "goto_ret"),
        ],
        expect![[r#"
            goto use module
            - module api @ 1:1-4:2

            goto use path
            - struct User @ 3:5-3:21

            goto impl trait
            - trait Named @ 2:5-2:23

            goto impl self type
            - struct User @ 3:5-3:21

            goto parameter type
            - struct User @ 3:5-3:21

            goto return type
            - struct User @ 3:5-3:21
        "#]],
    );
}

#[test]
fn resolves_field_declarations_and_field_type_paths() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_field_signature_goto"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct Profile;

pub struct User {
    pub pro$goto_field_decl$file: Pro$goto_field_type$file,
}
"#,
        &[
            AnalysisQuery::goto("goto field declaration", "goto_field_decl"),
            AnalysisQuery::goto("goto field type", "goto_field_type"),
        ],
        expect![[r#"
            goto field declaration
            - field profile @ 4:9-4:16

            goto field type
            - struct Profile @ 1:1-1:20
        "#]],
    );
}

#[test]
fn resolves_import_alias_cursors_to_imported_definitions() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_import_alias_goto"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
mod api {
    pub struct User;
}

use api::User as Acc$goto_import_alias$ount;
"#,
        &[AnalysisQuery::goto(
            "goto import alias",
            "goto_import_alias",
        )],
        expect![[r#"
            goto import alias
            - struct User @ 2:5-2:21
        "#]],
    );
}

#[test]
fn resolves_self_in_impl_signatures_to_impl_self_type() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_impl_self_signature_goto"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User;

impl User {
    pub fn new() -> Se$goto_impl_self_signature$lf {
        User
    }
}
"#,
        &[AnalysisQuery::goto(
            "goto impl signature Self",
            "goto_impl_self_signature",
        )],
        expect![[r#"
            goto impl signature Self
            - struct User @ 1:1-1:17
        "#]],
    );
}
