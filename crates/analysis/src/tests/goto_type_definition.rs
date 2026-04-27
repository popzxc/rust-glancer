use expect_test::expect;

use super::utils::{AnalysisQuery, check_analysis_queries};

#[test]
fn resolves_body_expression_type_definitions() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_goto_type_body"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User;
pub struct Profile;

pub struct Account {
    profile: Profile,
}

pub fn make_user() -> User {
    User
}

pub fn use_it(account: Account) {
    let local$goto_binding_type$: User = make_user();
    let _again: User = loc$goto_local_type$al;
    let _profile: Profile = account.pro$goto_field_type$file;
    let _made = make_user($goto_call_type$);
}
"#,
        &[
            AnalysisQuery::goto_type("goto type from binding declaration", "goto_binding_type"),
            AnalysisQuery::goto_type("goto type from local usage", "goto_local_type"),
            AnalysisQuery::goto_type("goto type from field access", "goto_field_type"),
            AnalysisQuery::goto_type("goto type from call expression", "goto_call_type"),
        ],
        expect![[r#"
            goto type from binding declaration
            - struct User @ 1:1-1:17

            goto type from local usage
            - struct User @ 1:1-1:17

            goto type from field access
            - struct Profile @ 2:1-2:20

            goto type from call expression
            - struct User @ 1:1-1:17
        "#]],
    );
}

#[test]
fn resolves_signature_type_definitions() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_goto_type_signature"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User;
pub struct Profile;

pub struct Account {
    profile$goto_field_decl_type$: Pro$goto_field_path_type$file,
}

pub fn make(user: Us$goto_param_type$er) -> Us$goto_ret_type$er {
    user
}

impl User {
    pub fn new() -> Se$goto_self_type$lf {
        User
    }
}
"#,
        &[
            AnalysisQuery::goto_type("goto type from field declaration", "goto_field_decl_type"),
            AnalysisQuery::goto_type("goto type from field type path", "goto_field_path_type"),
            AnalysisQuery::goto_type("goto type from parameter", "goto_param_type"),
            AnalysisQuery::goto_type("goto type from return", "goto_ret_type"),
            AnalysisQuery::goto_type("goto type from Self", "goto_self_type"),
        ],
        expect![[r#"
            goto type from field declaration
            - struct Profile @ 2:1-2:20

            goto type from field type path
            - struct Profile @ 2:1-2:20

            goto type from parameter
            - struct User @ 1:1-1:17

            goto type from return
            - struct User @ 1:1-1:17

            goto type from Self
            - struct User @ 1:1-1:17
        "#]],
    );
}

#[test]
fn resolves_body_local_type_definitions() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_goto_type_body_local"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User;

pub fn use_it() {
    struct User;

    let local$goto_local_decl_type$: User = User;
    let _again: User = loc$goto_local_usage_type$al;
}
"#,
        &[
            AnalysisQuery::goto_type(
                "goto type from body-local binding declaration",
                "goto_local_decl_type",
            ),
            AnalysisQuery::goto_type("goto type from body-local usage", "goto_local_usage_type"),
        ],
        expect![[r#"
            goto type from body-local binding declaration
            - struct User @ 4:5-4:17

            goto type from body-local usage
            - struct User @ 4:5-4:17
        "#]],
    );
}

#[test]
fn resolves_body_local_generic_impl_method_return_type_definitions() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_goto_type_body_local_impl_generic"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub fn use_it() {
    struct User;
    struct Wrapper<T> {
        value: T,
    }

    impl<U> Wrapper<U> {
        fn get(&self) -> U {
            missing()
        }
    }

    let wrapper: Wrapper<User>;
    let _value = wrapper.get($goto_type$);
}
"#,
        &[AnalysisQuery::goto_type(
            "goto type from body-local generic impl method",
            "goto_type",
        )],
        expect![[r#"
            goto type from body-local generic impl method
            - struct User @ 2:5-2:17
        "#]],
    );
}
