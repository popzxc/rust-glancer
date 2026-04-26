use expect_test::expect;

use super::utils::{AnalysisQuery, check_analysis_queries};

#[test]
fn returns_body_expression_types() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_type_at"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User;

pub fn helper() -> User {
    User
}

pub fn use_it() {
    let local: User = helper();
    let _typed: User = loc$type_at$al;
}
"#,
        &[AnalysisQuery::ty("type at local", "type_at")],
        expect![[r#"
            type at local
            - nominal struct analysis_type_at[lib]::crate::User
        "#]],
    );
}

#[test]
fn returns_binding_declaration_types() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_binding_type"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User;

pub fn helper() -> User {
    User
}

pub fn use_it() {
    let typed$type_decl$: User = helper();
}
"#,
        &[AnalysisQuery::ty(
            "type at declaration binding",
            "type_decl",
        )],
        expect![[r#"
            type at declaration binding
            - nominal struct analysis_binding_type[lib]::crate::User
        "#]],
    );
}

#[test]
fn returns_field_access_types() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_field_type"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct Profile;

pub struct User {
    pub profile: Profile,
}

pub fn use_it(user: User) {
    let _typed: Profile = user.pro$type_field$file;
}
"#,
        &[AnalysisQuery::ty("type at field", "type_field")],
        expect![[r#"
            type at field
            - nominal struct analysis_field_type[lib]::crate::Profile
        "#]],
    );
}

#[test]
fn returns_tuple_field_access_types() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_tuple_field_type"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct Left;
pub struct Right;

pub struct Pair(pub Left, pub Right);

pub fn use_it(pair: Pair) {
    let _right: Right = pair.$type_tuple_field$1;
}
"#,
        &[AnalysisQuery::ty("type at tuple field", "type_tuple_field")],
        expect![[r#"
            type at tuple field
            - nominal struct analysis_tuple_field_type[lib]::crate::Right
        "#]],
    );
}

#[test]
fn returns_signature_path_and_field_declaration_types() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_signature_type_at"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct Profile;

pub struct User {
    pub pro$type_field_decl$file: Pro$type_field_path$file,
}

pub fn make(profile: Pro$type_param$file) -> Pro$type_ret$file {
    profile
}
"#,
        &[
            AnalysisQuery::ty("type at field declaration", "type_field_decl"),
            AnalysisQuery::ty("type at field type path", "type_field_path"),
            AnalysisQuery::ty("type at parameter type", "type_param"),
            AnalysisQuery::ty("type at return type", "type_ret"),
        ],
        expect![[r#"
            type at field declaration
            - nominal struct analysis_signature_type_at[lib]::crate::Profile

            type at field type path
            - nominal struct analysis_signature_type_at[lib]::crate::Profile

            type at parameter type
            - nominal struct analysis_signature_type_at[lib]::crate::Profile

            type at return type
            - nominal struct analysis_signature_type_at[lib]::crate::Profile
        "#]],
    );
}

#[test]
fn returns_self_type_in_impl_signatures() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_impl_self_signature_type"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User;

impl User {
    pub fn new() -> Se$type_impl_self_signature$lf {
        User
    }
}
"#,
        &[AnalysisQuery::ty(
            "type at impl signature Self",
            "type_impl_self_signature",
        )],
        expect![[r#"
            type at impl signature Self
            - Self struct analysis_impl_self_signature_type[lib]::crate::User
        "#]],
    );
}

#[test]
fn returns_body_local_struct_types_before_module_structs() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_local_struct_type"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User;

pub fn make() {
    struct User;
    let local$type_binding$: Us$type_local_path$er = User;
    let _again: User = loc$type_local_expr$al;
}

pub fn outside() {
    let outside$type_module_binding$: User = User;
}
"#,
        &[
            AnalysisQuery::ty("type at local binding", "type_binding"),
            AnalysisQuery::ty("type at local type path", "type_local_path"),
            AnalysisQuery::ty("type at local expr", "type_local_expr"),
            AnalysisQuery::ty("type at module binding", "type_module_binding"),
        ],
        expect![[r#"
            type at local binding
            - local nominal struct fn analysis_local_struct_type[lib]::crate::make::User

            type at local type path
            - local nominal struct fn analysis_local_struct_type[lib]::crate::make::User

            type at local expr
            - local nominal struct fn analysis_local_struct_type[lib]::crate::make::User

            type at module binding
            - nominal struct analysis_local_struct_type[lib]::crate::User
        "#]],
    );
}

#[test]
fn returns_body_let_annotation_types_with_body_context() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_body_annotation_type"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User;

impl User {
    pub fn capture(&self) {
        let _this: Se$type_body_self$lf = self;
    }
}

pub fn make() {
    struct User;
    let _: Us$type_wildcard_type$er = User;
    let (_left, _right): (Us$type_tuple_left$er, Us$type_tuple_right$er) = User;
}
"#,
        &[
            AnalysisQuery::ty("type at body Self annotation", "type_body_self"),
            AnalysisQuery::ty("type at wildcard annotation", "type_wildcard_type"),
            AnalysisQuery::ty("type at tuple annotation left", "type_tuple_left"),
            AnalysisQuery::ty("type at tuple annotation right", "type_tuple_right"),
        ],
        expect![[r#"
            type at body Self annotation
            - Self struct analysis_body_annotation_type[lib]::crate::User

            type at wildcard annotation
            - local nominal struct fn analysis_body_annotation_type[lib]::crate::make::User

            type at tuple annotation left
            - local nominal struct fn analysis_body_annotation_type[lib]::crate::make::User

            type at tuple annotation right
            - local nominal struct fn analysis_body_annotation_type[lib]::crate::make::User
        "#]],
    );
}
