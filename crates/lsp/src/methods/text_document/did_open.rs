use tower_lsp_server::ls_types::*;

use crate::{backend::ServerContext, methods::uri_to_path};

pub(crate) async fn did_open(ctx: &ServerContext, params: DidOpenTextDocumentParams) {
    let Some(path) = uri_to_path(&params.text_document.uri) else {
        return;
    };

    ctx.documents
        .lock()
        .await
        .did_open(path.clone(), Some(params.text_document.version));
    tracing::debug!(path = %path.display(), "opened clean document snapshot");
}
