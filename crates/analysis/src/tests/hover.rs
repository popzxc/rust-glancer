use expect_test::expect;

use super::utils::{AnalysisQuery, check_analysis_queries};

#[test]
fn hovers_over_documented_items_and_usages() {
    check_analysis_queries(
        r#"
        //- /Cargo.toml
        [package]
        name = "analysis_hover"
        version = "0.0.0"
        edition = "2024"

        //- /src/lib.rs
        /// User account stored by the service.
        pub struct User {
            /// Display name shown in the UI.
            pub name: Profile,
        }

        #[doc = "Public profile data."]
        pub struct Profile;

        /// Builds a user.
        pub fn make_user() -> User {
            User { name: Profile }
        }

        pub fn demo() {
            let user = make_u$fn_hover$ser();
            let _name = user.na$field_hover$me;
            let _typed: Us$type_hover$er = user;
            let lo$local_hover$cal = Profile;
        }
        "#,
        &[
            AnalysisQuery::hover("hover function", "fn_hover"),
            AnalysisQuery::hover("hover field", "field_hover"),
            AnalysisQuery::hover("hover type", "type_hover"),
            AnalysisQuery::hover("hover local", "local_hover"),
        ],
        expect![[r#"
            hover function
            - kind: fn
            - signature: pub fn make_user() -> User
            - type: User
            - docs:
              Builds a user.

            hover field
            - kind: field
            - signature: pub name: Profile
            - type: Profile
            - docs:
              Display name shown in the UI.

            hover type
            - kind: struct
            - signature: pub struct User
            - docs:
              User account stored by the service.

            hover local
            - kind: variable
            - signature: let local: Profile
            - type: Profile
        "#]],
    );
}

#[test]
fn hovers_over_enum_variants_and_body_local_items() {
    check_analysis_queries(
        r#"
        //- /Cargo.toml
        [package]
        name = "analysis_hover_locals"
        version = "0.0.0"
        edition = "2024"

        //- /src/lib.rs
        pub enum Event {
            /// Event has started.
            Sta$variant_decl_hover$rted,
        }

        pub fn demo() {
            /// Request scoped to this function.
            struct Request {
                /// Request identifier.
                id: Event,
            }

            impl Request {
                /// Returns the request id.
                fn id(&self) -> Event {
                    Event::Started
                }
            }

            let request: Request;
            let _id = request.i$method_hover$d();
            let _field = request.i$local_field_hover$d;
            let _event = Event::Sta$variant_hover$rted;
            let _typed: Re$local_type_hover$quest = request;
        }
        "#,
        &[
            AnalysisQuery::hover("hover body-local method", "method_hover"),
            AnalysisQuery::hover("hover body-local field", "local_field_hover"),
            AnalysisQuery::hover("hover enum variant declaration", "variant_decl_hover"),
            AnalysisQuery::hover("hover enum variant", "variant_hover"),
            AnalysisQuery::hover("hover body-local type", "local_type_hover"),
        ],
        expect![[r#"
            hover body-local method
            - kind: method
            - signature: fn id(&self) -> Event
            - type: Event
            - docs:
              Returns the request id.

            hover body-local field
            - kind: field
            - signature: id: Event
            - type: Event
            - docs:
              Request identifier.

            hover enum variant declaration
            - kind: variant
            - signature: variant Event::Started
            - docs:
              Event has started.

            hover enum variant
            - kind: variant
            - signature: variant Event::Started
            - docs:
              Event has started.

            hover body-local type
            - kind: struct
            - signature: struct Request
            - docs:
              Request scoped to this function.
        "#]],
    );
}

#[test]
fn hovers_over_documented_module_declarations() {
    check_analysis_queries(
        r#"
        //- /Cargo.toml
        [package]
        name = "analysis_hover_modules"
        version = "0.0.0"
        edition = "2024"

        //- /src/lib.rs
        pub mod a$out_of_line_module_hover$pi;

        /// Inline helpers.
        pub mod he$inline_module_hover$lpers {
            //! Inline helper internals.
            pub struct Helper;
        }

        //- /src/api.rs
        //! Public API surface.
        pub struct Api;
        "#,
        &[
            AnalysisQuery::hover("hover out-of-line module", "out_of_line_module_hover"),
            AnalysisQuery::hover("hover inline module", "inline_module_hover"),
        ],
        expect![[r#"
            hover out-of-line module
            - kind: module
            - signature: mod api
            - docs:
              Public API surface.

            hover inline module
            - kind: module
            - signature: mod helpers
            - docs:
              Inline helpers.
              Inline helper internals.
        "#]],
    );
}

#[test]
fn hovers_over_crate_root_path_names_and_docs() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[workspace]
members = ["crates/dep", "crates/app"]
resolver = "3"

//- /crates/dep/Cargo.toml
[package]
name = "dep"
version = "0.1.0"
edition = "2024"

//- /crates/dep/src/lib.rs
//! Dependency crate docs.
pub struct Thing;

//- /crates/app/Cargo.toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
dep = { path = "../dep" }

//- /crates/app/src/lib.rs
//! Application crate docs.
pub struct Api;

pub fn use_roots(_: cra$crate_root_hover$te::Api, _: de$dep_root_hover$p::Thing) {}
"#,
        &[
            AnalysisQuery::hover("hover crate root path", "crate_root_hover").in_lib("app"),
            AnalysisQuery::hover("hover dependency root path", "dep_root_hover").in_lib("app"),
        ],
        expect![[r#"
            hover crate root path
            - kind: module
            - signature: mod crate
            - docs:
              Application crate docs.

            hover dependency root path
            - kind: module
            - signature: mod dep
            - docs:
              Dependency crate docs.
        "#]],
    );
}
