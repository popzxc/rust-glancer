mod utils;

use expect_test::expect;

use self::utils::{HostFixture, HostObservation};

#[test]
fn reparses_known_file_in_place() {
    let mut fixture = HostFixture::build(
        r#"
//- /Cargo.toml
[package]
name = "host_update_fixture"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User;
"#,
    );
    let before_file_id = fixture.file_id_for_path("src/lib.rs");

    fixture.check(
        &[HostObservation::workspace_symbols("User")],
        expect![[r#"
            workspace symbols `User`
            - struct User @ host_update_fixture[lib] src/lib.rs
        "#]],
    );

    fixture.check_save(
        r#"
//- /src/lib.rs
pub struct Account;
"#,
        &[
            HostObservation::workspace_symbols("Account"),
            HostObservation::workspace_symbols("User"),
        ],
        expect![[r#"
            changed files
            - host_update_fixture src/lib.rs

            affected packages
            - host_update_fixture

            changed targets
            - host_update_fixture[lib]

            workspace symbols `Account`
            - struct Account @ host_update_fixture[lib] src/lib.rs

            workspace symbols `User`
            - <none>
        "#]],
    );

    let after_file_id = fixture.file_id_for_path("src/lib.rs");
    assert_eq!(
        after_file_id, before_file_id,
        "known file reparses should keep the package-local FileId stable"
    );
}

#[test]
fn reads_saved_disk_text_for_modules_discovered_after_the_change() {
    let mut fixture = HostFixture::build(
        r#"
//- /Cargo.toml
[package]
name = "host_new_module_fixture"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct Root;

//- /src/api.rs
pub struct DiskOnly;
"#,
    );

    fixture.check_save(
        r#"
//- /src/api.rs
pub struct SavedOnly;

//- /src/lib.rs
mod api;
"#,
        &[
            HostObservation::workspace_symbols("SavedOnly"),
            HostObservation::workspace_symbols("DiskOnly"),
        ],
        expect![[r#"
            changed files
            - host_new_module_fixture src/api.rs
            - host_new_module_fixture src/lib.rs

            affected packages
            - host_new_module_fixture

            changed targets
            - host_new_module_fixture[lib]

            workspace symbols `SavedOnly`
            - struct SavedOnly @ host_new_module_fixture[lib] src/api.rs

            workspace symbols `DiskOnly`
            - <none>
        "#]],
    );
}

#[test]
fn resolves_lsp_file_contexts_from_paths() {
    let fixture = HostFixture::build(
        r#"
//- /Cargo.toml
[package]
name = "file_context_fixture"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
mod shared;

//- /src/main.rs
mod shared;

fn main() {}

//- /src/shared.rs
pub struct Shared;

//- /src/orphan.rs
pub struct Orphan;
"#,
    );

    fixture.check(
        &[
            HostObservation::file_contexts("lib root", "src/lib.rs"),
            HostObservation::file_contexts("bin root", "src/main.rs"),
            HostObservation::file_contexts("shared module", "src/shared.rs"),
            HostObservation::file_contexts("orphan file", "src/orphan.rs"),
        ],
        expect![[r#"
            file contexts `lib root`
            - file_context_fixture src/lib.rs -> file_context_fixture[lib]

            file contexts `bin root`
            - file_context_fixture src/main.rs -> file_context_fixture[bin]

            file contexts `shared module`
            - file_context_fixture src/shared.rs -> file_context_fixture[bin], file_context_fixture[lib]

            file contexts `orphan file`
            - <none>
        "#]],
    );
}

#[test]
fn rebuilds_package_roots_for_new_saved_module_files() {
    let mut fixture = HostFixture::build(
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
pub struct Root;

//- /crates/app/Cargo.toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
dep = { path = "../dep" }

//- /crates/app/src/lib.rs
pub fn use_dep(value: dep::api::Api) {
    let same = val$0ue;
}
"#,
    );

    fixture.check_save(
        r#"
//- /crates/dep/src/lib.rs
pub mod api;
pub struct Root;
"#,
        &[HostObservation::type_names_at("app marker 0", "app", "0")],
        expect![[r#"
            changed files
            - dep crates/dep/src/lib.rs

            affected packages
            - app
            - dep

            changed targets
            - dep[lib]

            type names at `app marker 0`
            - <none>
        "#]],
    );

    fixture.check_save(
        r#"
//- /crates/dep/src/api.rs
pub struct Api;
"#,
        &[HostObservation::type_names_at("app marker 0", "app", "0")],
        expect![[r#"
            changed files
            - dep crates/dep/src/api.rs

            affected packages
            - app
            - dep

            changed targets
            - dep[lib]

            type names at `app marker 0`
            - Api
        "#]],
    );
}

#[test]
fn removes_modules_from_index_after_mod_declarations_are_removed() {
    let mut fixture = HostFixture::build(
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
pub mod api;
pub struct Root;

//- /crates/dep/src/api.rs
pub struct Api;

//- /crates/app/Cargo.toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
dep = { path = "../dep" }

//- /crates/app/src/lib.rs
pub fn use_dep(value: dep::api::Api) {
    let same = val$0ue;
}
"#,
    );

    fixture.check(
        &[
            HostObservation::type_names_at("app marker 0", "app", "0"),
            HostObservation::workspace_symbols("Api"),
        ],
        expect![[r#"
            type names at `app marker 0`
            - Api

            workspace symbols `Api`
            - module api @ dep[lib] crates/dep/src/lib.rs
            - struct Api @ dep[lib] crates/dep/src/api.rs
        "#]],
    );

    fixture.check_save(
        r#"
//- /crates/dep/src/lib.rs
pub struct Root;
"#,
        &[
            HostObservation::type_names_at("app marker 0", "app", "0"),
            HostObservation::workspace_symbols("Api"),
        ],
        expect![[r#"
            changed files
            - dep crates/dep/src/lib.rs

            affected packages
            - app
            - dep

            changed targets
            - dep[lib]

            type names at `app marker 0`
            - <none>

            workspace symbols `Api`
            - <none>
        "#]],
    );
}

#[test]
fn reports_reverse_dependent_packages_as_affected() {
    let mut fixture = HostFixture::build(
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
pub struct Api;

//- /crates/app/Cargo.toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
dep = { path = "../dep" }

//- /crates/app/src/lib.rs
pub fn use_dep(_: dep::Api) {}
"#,
    );

    fixture.check_save(
        r#"
//- /crates/dep/src/lib.rs
pub struct Api;
pub struct Extra;
"#,
        &[],
        expect![[r#"
            changed files
            - dep crates/dep/src/lib.rs

            affected packages
            - app
            - dep

            changed targets
            - dep[lib]
        "#]],
    );
}

#[test]
fn rebuilds_reverse_dependent_packages_after_dependency_changes() {
    let mut fixture = HostFixture::build(
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
pub struct Api;

//- /crates/app/Cargo.toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
dep = { path = "../dep" }

//- /crates/app/src/lib.rs
pub fn use_dep(value: dep::Api) {
    let same = val$0ue;
}
"#,
    );

    fixture.check(
        &[HostObservation::type_names_at("app marker 0", "app", "0")],
        expect![[r#"
            type names at `app marker 0`
            - Api
        "#]],
    );

    fixture.check_save(
        r#"
//- /crates/dep/src/lib.rs
pub struct Renamed;
"#,
        &[HostObservation::type_names_at("app marker 0", "app", "0")],
        expect![[r#"
            changed files
            - dep crates/dep/src/lib.rs

            affected packages
            - app
            - dep

            changed targets
            - dep[lib]

            type names at `app marker 0`
            - <none>
        "#]],
    );
}

#[test]
fn rebuilds_transitive_reverse_dependent_packages_after_dependency_changes() {
    let mut fixture = HostFixture::build(
        r#"
//- /Cargo.toml
[workspace]
members = ["crates/dep", "crates/mid", "crates/app"]
resolver = "3"

//- /crates/dep/Cargo.toml
[package]
name = "dep"
version = "0.1.0"
edition = "2024"

//- /crates/dep/src/lib.rs
pub struct Api;

//- /crates/mid/Cargo.toml
[package]
name = "mid"
version = "0.1.0"
edition = "2024"

[dependencies]
dep = { path = "../dep" }

//- /crates/mid/src/lib.rs
pub fn make() -> dep::Api {
    loop {}
}

//- /crates/app/Cargo.toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[dependencies]
mid = { path = "../mid" }

//- /crates/app/src/lib.rs
pub fn use_mid() {
    let value = mid::make();
    let same = val$0ue;
}
"#,
    );

    fixture.check(
        &[HostObservation::type_names_at("app marker 0", "app", "0")],
        expect![[r#"
            type names at `app marker 0`
            - Api
        "#]],
    );

    fixture.check_save(
        r#"
//- /crates/dep/src/lib.rs
pub struct Renamed;
"#,
        &[HostObservation::type_names_at("app marker 0", "app", "0")],
        expect![[r#"
            changed files
            - dep crates/dep/src/lib.rs

            affected packages
            - app
            - dep
            - mid

            changed targets
            - dep[lib]

            type names at `app marker 0`
            - <none>
        "#]],
    );
}
