test:
    cargo nextest run --workspace

lint:
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings

deny:
    cargo deny check

build:
    cargo build --workspace --release

bench:
    cargo bench -p rg_project --bench analysis_pipeline

build-client:
    npm --prefix editors/code ci
    npm --prefix editors/code run compile

check-client:
    npm --prefix editors/code run check

pr-ready: test lint deny build-client check-client
