//! Cargo-backed diagnostics for the LSP server.
//!
//! This module runs `cargo check`/`cargo clippy` outside the synchronous analysis engine and
//! publishes compiler diagnostics when saved-file checks complete.

mod config;
mod diagnostics;
mod runner;

pub(crate) use self::{config::CheckConfig, runner::CheckHandle};
