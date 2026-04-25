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
