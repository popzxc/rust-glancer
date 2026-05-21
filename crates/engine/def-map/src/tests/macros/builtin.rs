use super::super::utils;

#[test]
fn include_macro_splices_real_source_items() {
    let project = utils::DefMapFixtureDb::build(
        r#"
//- /Cargo.toml
[package]
name = "include_macro_fixture"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
include!("included.rs");

make_included!();

//- /src/included.rs
pub struct Included;

macro_rules! make_included {
    () => {
        pub struct FromIncludedMacro;
    };
}
"#,
    );
    let target = project.lib("include_macro_fixture");

    target
        .entry("Included")
        .assert_type_exists("include should splice item definitions into the caller module")
        .assert_type_source_file("included.rs", "included items should keep real file spans");
    target.entry("FromIncludedMacro").assert_type_exists(
        "macro_rules definitions from included files should be visible to later calls",
    );
}

#[test]
fn local_include_macro_shadows_builtin_include() {
    let project = utils::DefMapFixtureDb::build(
        r#"
//- /Cargo.toml
[package]
name = "include_shadow_fixture"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
macro_rules! include {
    ($path:literal) => {
        pub struct FromMacro;
    };
}

include!("included.rs");

//- /src/included.rs
pub struct FromFile;
"#,
    );
    let target = project.lib("include_shadow_fixture");

    target
        .entry("FromMacro")
        .assert_type_exists("local macro_rules definitions should shadow builtin include");
    target
        .entry("FromFile")
        .assert_missing("shadowed include calls should not splice the referenced file");
}
