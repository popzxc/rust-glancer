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
