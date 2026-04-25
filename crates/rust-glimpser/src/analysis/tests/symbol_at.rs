use expect_test::expect;

use super::utils::{AnalysisQuery, check_analysis_queries};

#[test]
fn finds_body_symbols_at_offsets() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_symbol_at"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User;

pub fn helper() -> User {
    User
}

pub fn use_it() {
    let loc$symbol_decl$al: User = helper();
    let _again: User = loc$symbol_local$al;
    let _made: User = hel$symbol_item$per();
}
"#,
        &[
            AnalysisQuery::symbol("symbol at declaration", "symbol_decl"),
            AnalysisQuery::symbol("symbol at local path", "symbol_local"),
            AnalysisQuery::symbol("symbol at item path", "symbol_item"),
        ],
        expect![[r#"
            symbol at declaration
            - binding let local @ 8:9-8:14

            symbol at local path
            - expr path local @ 9:24-9:29

            symbol at item path
            - expr path helper @ 10:23-10:29
        "#]],
    );
}
