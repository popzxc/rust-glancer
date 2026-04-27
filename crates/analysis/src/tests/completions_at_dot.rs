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
fn completes_methods_for_bin_root_library_type() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_bin_completion"
version = "0.1.0"
edition = "2024"

[lib]
path = "src/lib.rs"

[[bin]]
name = "analysis-bin-completion"
path = "src/main.rs"

//- /src/lib.rs
pub struct Api;

impl Api {
    pub fn ping(&self) {}
    pub fn work(&self) {}
}

//- /src/main.rs
fn main() {
    let api: analysis_bin_completion::Api = todo!();
    api.$0;
}
"#,
        &[AnalysisQuery::complete("bin root completions", "0").in_bin("analysis_bin_completion")],
        expect![[r#"
            bin root completions
            - inherent_method ping
            - inherent_method work
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

#[test]
fn completes_methods_after_field_receiver() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_field_receiver_completions"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct Profile;

impl Profile {
    pub fn display(&self) {}
}

pub struct User {
    pub profile: Profile,
}

pub fn use_it(user: User) {
    user.profile.$0;
}
"#,
        &[AnalysisQuery::complete("field receiver completions", "0")],
        expect![[r#"
            field receiver completions
            - inherent_method display
        "#]],
    );
}

#[test]
fn completes_fields_at_dot() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_field_completions"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct Profile;

pub struct User {
    pub profile: Profile,
}

pub fn use_it(user: User) {
    user.$0;
}
"#,
        &[AnalysisQuery::complete("field completions", "0")],
        expect![[r#"
            field completions
            - field profile
        "#]],
    );
}

#[test]
fn completes_body_local_struct_fields_at_dot() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_body_local_field_completions"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User;

pub fn use_it() {
    struct User {
        id: UserId,
        profile: Profile,
    }
    struct Pair(UserId, Profile);
    struct UserId;
    struct Profile;

    let user: User;
    user.$0;

    let pair: Pair;
    pair.$tuple$;
}
"#,
        &[
            AnalysisQuery::complete("body-local field completions", "0"),
            AnalysisQuery::complete("body-local tuple field completions", "tuple"),
        ],
        expect![[r#"
            body-local field completions
            - field id
            - field profile

            body-local tuple field completions
            - field 0
            - field 1
        "#]],
    );
}

#[test]
fn completes_body_local_impl_methods_at_dot() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_body_local_impl_completions"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct GlobalId;

pub fn use_it() {
    struct User {
        id: GlobalId,
    }

    impl User {
        fn id(&self) -> GlobalId {
            missing()
        }

        fn associated() -> GlobalId {
            missing()
        }
    }

    let user: User;
    user.$0;
}
"#,
        &[AnalysisQuery::complete("body-local impl completions", "0")],
        expect![[r#"
            body-local impl completions
            - field id
            - inherent_method id
        "#]],
    );
}

#[test]
fn completes_body_local_impl_methods_from_nested_blocks() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_nested_body_local_impl_completions"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct GlobalId;

pub fn use_it() {
    struct User {
        id: GlobalId,
    }

    {
        impl User {
            fn id(&self) -> GlobalId {
                missing()
            }
        }
    }

    let user: User;
    user.$0;
}
"#,
        &[AnalysisQuery::complete(
            "nested body-local impl completions",
            "0",
        )],
        expect![[r#"
            nested body-local impl completions
            - field id
            - inherent_method id
        "#]],
    );
}

#[test]
fn completes_tuple_fields_at_dot() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_tuple_field_completions"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct Left;
pub struct Right;

pub struct Pair(pub Left, pub Right);

pub fn use_it(pair: Pair) {
    pair.$0;
}
"#,
        &[AnalysisQuery::complete("tuple field completions", "0")],
        expect![[r#"
            tuple field completions
            - field 0
            - field 1
        "#]],
    );
}
