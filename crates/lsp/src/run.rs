use tower_lsp_server::{LspService, Server};
use tracing_subscriber::EnvFilter;

use crate::backend::Backend;

/// Starts the rust-glimpser LSP server over stdio.
pub fn run_stdio() -> anyhow::Result<()> {
    let filter =
        EnvFilter::try_from_env("RUST_GLIMPSER_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init();

    tracing::info!("starting rust-glimpser LSP server over stdio");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let (service, socket) = LspService::new(Backend::new);

        Server::new(stdin, stdout, socket).serve(service).await;

        Ok(())
    })
}
