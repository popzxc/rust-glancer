# small_app

This fixture is a small quasi-real Rust application workspace for analysis benchmarks.

It is intentionally handwritten rather than generated. The code models a compact service
with a domain crate, an Axum API crate, and a Clap/Tokio CLI crate. Dependencies are
exact-version requirements and `Cargo.lock` is checked in, so the target is stable while
still exercising normal third-party dependency graphs.

The fixture is useful as a baseline for modest early-stage application workspaces where
most complexity comes from common dependencies and realistic module boundaries, not from
huge generated bodies.
