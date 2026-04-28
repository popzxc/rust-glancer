use tower_lsp_server::ls_types::*;

use crate::{backend::ServerContext, methods::uri_to_path};

pub(crate) async fn did_close(ctx: &ServerContext, params: DidCloseTextDocumentParams) {
    let Some(path) = uri_to_path(&params.text_document.uri) else {
        return;
    };

    ctx.documents.lock().await.did_close(&path);
    tracing::debug!(path = %path.display(), "closed document");
}
