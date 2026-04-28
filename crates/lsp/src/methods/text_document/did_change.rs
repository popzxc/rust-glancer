use tower_lsp_server::ls_types::*;

use crate::{backend::ServerContext, methods::uri_to_path};

pub(crate) async fn did_change(ctx: &ServerContext, params: DidChangeTextDocumentParams) {
    let Some(path) = uri_to_path(&params.text_document.uri) else {
        return;
    };

    let became_dirty = ctx
        .documents
        .lock()
        .await
        .did_change(path.clone(), Some(params.text_document.version));
    tracing::debug!(path = %path.display(), "marked document dirty");

    if became_dirty {
        if let Err(error) = ctx.client.inlay_hint_refresh().await {
            tracing::debug!(
                error = %error,
                "failed to request inlay hint refresh after document became dirty"
            );
        }
    }
}
