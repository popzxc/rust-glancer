use std::path::Path;

use rg_parse::{FileId, ParseDb};
use rg_workspace::WorkspaceMetadata;
use test_fixture::fixture_crate;

use crate::{AnalysisHost, FileChange};

#[test]
fn reparses_known_file_in_place() {
    let fixture = fixture_crate(
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
    let workspace = WorkspaceMetadata::from_cargo(fixture.metadata());
    let mut host = AnalysisHost::build(workspace).expect("analysis host should build");
    let lib_path = fixture.path("src/lib.rs");
    let before_file_id = file_id_for_path(host.snapshot().parse_db(), &lib_path);

    assert!(
        has_workspace_symbol(&host, "User"),
        "initial snapshot should contain the original type"
    );

    let summary = host
        .apply_change(FileChange::new(&lib_path, "pub struct Account;\n"))
        .expect("source change should apply");
    let after_file_id = file_id_for_path(host.snapshot().parse_db(), &lib_path);

    assert_eq!(
        summary.changed_files.len(),
        1,
        "known file should be reported once"
    );
    assert_eq!(
        summary.changed_files[0].file, before_file_id,
        "known file should keep its stable FileId"
    );
    assert_eq!(
        summary.changed_targets.len(),
        1,
        "single-lib fixture should report one owning target"
    );
    assert_eq!(
        host.snapshot().targets_for_file(
            summary.changed_files[0].package,
            summary.changed_files[0].file
        ),
        summary.changed_targets,
        "snapshot target ownership should match the change summary"
    );
    assert_eq!(
        after_file_id, before_file_id,
        "reparse should replace the file in place"
    );
    assert!(
        has_workspace_symbol(&host, "Account"),
        "updated snapshot should use in-memory source"
    );
    assert!(
        !has_workspace_symbol(&host, "User"),
        "updated snapshot should not keep stale semantic symbols"
    );
}

#[test]
fn uses_buffer_text_for_modules_discovered_after_the_change() {
    let fixture = fixture_crate(
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
    let workspace = WorkspaceMetadata::from_cargo(fixture.metadata());
    let mut host = AnalysisHost::build(workspace).expect("analysis host should build");
    let lib_path = fixture.path("src/lib.rs");
    let api_path = fixture.path("src/api.rs");

    host.apply_changes([
        FileChange::new(&api_path, "pub struct BufferOnly;\n"),
        FileChange::new(&lib_path, "mod api;\n"),
    ])
    .expect("source changes should apply");

    assert!(
        has_workspace_symbol(&host, "BufferOnly"),
        "newly discovered module should be parsed from the editor buffer"
    );
    assert!(
        !has_workspace_symbol(&host, "DiskOnly"),
        "newly discovered module should not fall back to stale disk contents"
    );
}

#[test]
fn reports_reverse_dependent_packages_as_affected() {
    let fixture = fixture_crate(
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
    let workspace = WorkspaceMetadata::from_cargo(fixture.metadata());
    let mut host = AnalysisHost::build(workspace).expect("analysis host should build");
    let dep_path = fixture.path("crates/dep/src/lib.rs");

    let summary = host
        .apply_change(FileChange::new(
            &dep_path,
            "pub struct Api;\npub struct Extra;\n",
        ))
        .expect("source change should apply");
    let affected = summary
        .affected_packages
        .iter()
        .map(|slot| {
            host.snapshot()
                .parse_db()
                .package(slot.0)
                .expect("affected package should exist")
                .package_name()
                .to_string()
        })
        .collect::<Vec<_>>();

    assert!(
        affected.iter().any(|name| name == "dep"),
        "changed package should be affected"
    );
    assert!(
        affected.iter().any(|name| name == "app"),
        "reverse dependent package should be affected"
    );
}

fn has_workspace_symbol(host: &AnalysisHost, name: &str) -> bool {
    host.snapshot()
        .analysis()
        .workspace_symbols(name)
        .iter()
        .any(|symbol| symbol.name == name)
}

fn file_id_for_path(parse: &ParseDb, path: &Path) -> FileId {
    let canonical_path = path
        .canonicalize()
        .expect("fixture source path should canonicalize");

    parse
        .packages()
        .iter()
        .flat_map(|package| package.parsed_files())
        .find(|file| file.path() == canonical_path.as_path())
        .unwrap_or_else(|| panic!("fixture file {} should be parsed", path.display()))
        .file_id()
}
