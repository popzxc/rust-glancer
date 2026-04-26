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

#[test]
fn finds_item_and_signature_symbols_at_offsets() {
    check_analysis_queries(
        r#"
//- /Cargo.toml
[package]
name = "analysis_signature_symbol_at"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct Us$symbol_struct$er;

pub trait Na$symbol_trait$med {
    fn describe(&self) -> User;
}

pub fn make(user: Us$symbol_param$er) -> Us$symbol_ret$er {
    user
}
"#,
        &[
            AnalysisQuery::symbol("symbol at struct declaration", "symbol_struct"),
            AnalysisQuery::symbol("symbol at trait declaration", "symbol_trait"),
            AnalysisQuery::symbol("symbol at parameter type", "symbol_param"),
            AnalysisQuery::symbol("symbol at return type", "symbol_ret"),
        ],
        expect![[r#"
            symbol at struct declaration
            - struct User @ 1:12-1:16

            symbol at trait declaration
            - trait Named @ 3:11-3:16

            symbol at parameter type
            - path User @ 7:19-7:23

            symbol at return type
            - path User @ 7:28-7:32
        "#]],
    );
}
