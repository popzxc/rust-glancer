use expect_test::expect;

use super::utils::{AnalysisQuery, check_analysis_queries};

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

#[test]
fn completes_methods_at_bare_dot() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_bare_dot_completions"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub trait Named {
    fn trait_name(&self);
}

pub struct User;

impl User {
    pub fn id(&self) {}

    pub fn touch(&mut self) {}
}

impl Named for User {
    fn trait_name(&self) {}
}

pub fn use_it(user: User) {
    user.$0;
}
"#,
        &[AnalysisQuery::complete("bare dot completions", "0")],
        expect![[r#"
            bare dot completions
            - inherent_method id
            - inherent_method touch
            - trait_method trait_name
        "#]],
    );
}

#[test]
fn does_not_trigger_inside_method_arguments() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_completion_dot_range"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User;

impl User {
    pub fn id(&self, _value: u8) {}

    pub fn touch(&self) {}
}

pub fn use_it(user: User) {
    user.id($inside_arg$0);
}
"#,
        &[AnalysisQuery::complete(
            "completion inside method argument",
            "inside_arg",
        )],
        expect![[r#"
            completion inside method argument
            - <none>
        "#]],
    );
}

#[test]
fn preserves_distinct_same_name_candidates() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_completion_duplicates"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub trait Named {
    fn label(&self);
}

pub trait Displayed {
    fn label(&self);
}

pub struct User;

impl User {
    pub fn label(&self) {}
}

impl Named for User {
    fn label(&self) {}
}

impl Displayed for User {
    fn label(&self) {}
}

pub fn use_it(user: User) {
    user.$0label();
}
"#,
        &[AnalysisQuery::complete("same-name completions", "0")],
        expect![[r#"
            same-name completions
            - inherent_method label
            - trait_method label
            - trait_method label
        "#]],
    );
}
