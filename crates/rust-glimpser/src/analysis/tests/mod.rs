mod utils;

use expect_test::expect;

use self::utils::{AnalysisQuery, check_analysis_queries};

#[test]
fn resolves_goto_definition_and_type_queries_inside_bodies() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_queries"
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
    let _typed: User = loc$type_at$al;
}
"#,
        &[
            AnalysisQuery::goto("goto local", "goto_local"),
            AnalysisQuery::goto("goto item", "goto_item"),
            AnalysisQuery::ty("type at local", "type_at"),
        ],
        expect![[r#"
            goto local
            - local local @ 8:9-8:14

            goto item
            - fn helper @ 3:1-5:2

            type at local
            - nominal struct analysis_queries[lib]::crate::User
        "#]],
    );
}

#[test]
fn completes_inherent_and_trait_methods_at_dot() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_completions"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub trait Named {
    fn trait_name(&self);
    fn associated() {}
}

pub struct User;

impl User {
    pub fn new() -> Self {
        User
    }

    pub fn id(&self) {}

    pub fn touch(&mut self) {}
}

impl Named for User {
    fn trait_name(&self) {}
}

pub fn use_it(user: User) {
    user.$0id();
}
"#,
        &[AnalysisQuery::complete("dot completions", "0")],
        expect![[r#"
            dot completions
            - inherent_method id
            - inherent_method touch
            - trait_method trait_name
        "#]],
    );
}
