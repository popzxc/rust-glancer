use expect_test::expect;

use super::utils::{AnalysisQuery, check_analysis_queries};

#[test]
fn finds_common_reference_subjects() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_references"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User {
    na$field_ref$me: Name,
}

pub struct Name;

pub fn helper(user: User) -> Name {
    user.name
}

pub fn use_it() {
    let loc$local_ref$al: Us$type_ref$er;
    let _again: User = local;
    let _name = hel$fn_ref$per(local);
}
"#,
        &[
            AnalysisQuery::references("type references", "type_ref"),
            AnalysisQuery::references_without_declaration(
                "type references without declaration",
                "type_ref",
            ),
            AnalysisQuery::references("field references", "field_ref"),
            AnalysisQuery::references("function references", "fn_ref"),
            AnalysisQuery::references("local references", "local_ref"),
        ],
        expect![[r#"
            type references
            - `User` @ src/lib.rs:1:12-1:16
            - `User` @ src/lib.rs:7:21-7:25
            - `User` @ src/lib.rs:12:16-12:20
            - `User` @ src/lib.rs:13:17-13:21

            type references without declaration
            - `User` @ src/lib.rs:7:21-7:25
            - `User` @ src/lib.rs:12:16-12:20
            - `User` @ src/lib.rs:13:17-13:21

            field references
            - `name` @ src/lib.rs:2:5-2:9
            - `name` @ src/lib.rs:8:10-8:14

            function references
            - `helper` @ src/lib.rs:7:8-7:14
            - `helper` @ src/lib.rs:14:17-14:23

            local references
            - `local` @ src/lib.rs:12:9-12:14
            - `local` @ src/lib.rs:13:24-13:29
            - `local` @ src/lib.rs:14:24-14:29
        "#]],
    );
}

#[test]
fn finds_body_local_method_references() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_body_local_method_references"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct Id;

pub fn use_it() {
    struct User;

    impl User {
        fn i$method_ref$d(&self) -> Id {
            Id
        }
    }

    let user: User;
    user.id();
}
"#,
        &[AnalysisQuery::references(
            "body-local method references",
            "method_ref",
        )],
        expect![[r#"
            body-local method references
            - `id` @ src/lib.rs:7:12-7:14
            - `id` @ src/lib.rs:13:10-13:12
        "#]],
    );
}
