use expect_test::expect;

use super::utils::{self, PathResolutionQuery};

#[test]
fn resolves_paths_against_frozen_def_map() {
    utils::check_project_path_resolution(
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
pub trait ExternalTrait {}

mod hidden {
    pub trait HiddenTrait {}
}

//- /crates/app/Cargo.toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
dep = { path = "../dep" }

//- /crates/app/src/lib.rs
use dep::ExternalTrait as ImportedTrait;

pub struct Root;

pub mod api {
    pub struct LocalType;
    struct PrivateType;
    pub const LOCAL_CONST: u8 = 0;

    pub mod child {
        pub struct Child;
    }
}
"#,
        &[
            PathResolutionQuery::lib("app", "crate", "dep::ExternalTrait"),
            PathResolutionQuery::lib("app", "crate", "::dep::ExternalTrait"),
            PathResolutionQuery::lib("app", "crate", "ImportedTrait"),
            PathResolutionQuery::lib("app", "crate", "crate::api::LOCAL_CONST"),
            PathResolutionQuery::lib("app", "crate::api::child", "self::Child"),
            PathResolutionQuery::lib("app", "crate::api::child", "super::LocalType"),
            PathResolutionQuery::lib("app", "crate::api::child", "super::PrivateType"),
            PathResolutionQuery::lib("app", "crate", "crate::api::PrivateType"),
            PathResolutionQuery::lib("app", "crate", "dep::hidden::HiddenTrait"),
            PathResolutionQuery::lib("app", "crate", "missing::Thing"),
            PathResolutionQuery::lib("app", "crate", "dep::missing::Thing"),
            PathResolutionQuery::lib("app", "crate", "crate::dep::ExternalTrait"),
        ],
        expect![[r#"
            app [lib] crate resolves dep::ExternalTrait -> trait dep[lib]::crate::ExternalTrait
            app [lib] crate resolves ::dep::ExternalTrait -> trait dep[lib]::crate::ExternalTrait
            app [lib] crate resolves ImportedTrait -> trait dep[lib]::crate::ExternalTrait
            app [lib] crate resolves crate::api::LOCAL_CONST -> const app[lib]::crate::api::LOCAL_CONST
            app [lib] crate::api::child resolves self::Child -> struct app[lib]::crate::api::child::Child
            app [lib] crate::api::child resolves super::LocalType -> struct app[lib]::crate::api::LocalType
            app [lib] crate::api::child resolves super::PrivateType -> struct app[lib]::crate::api::PrivateType
            app [lib] crate resolves crate::api::PrivateType -> <none> (unresolved at segment #2)
            app [lib] crate resolves dep::hidden::HiddenTrait -> <none> (unresolved at segment #1)
            app [lib] crate resolves missing::Thing -> <none> (unresolved at segment #0)
            app [lib] crate resolves dep::missing::Thing -> <none> (unresolved at segment #1)
            app [lib] crate resolves crate::dep::ExternalTrait -> <none> (unresolved at segment #1)
        "#]],
    );
}
