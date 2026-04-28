use tower_lsp_server::{LspService, Server};

use crate::backend::Backend;

/// Starts the rust-glimpser LSP server over stdio.
pub fn run_stdio() -> anyhow::Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init();

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
