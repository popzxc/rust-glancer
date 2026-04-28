use tower_lsp_server::ls_types::*;

use crate::{backend::ServerContext, methods::uri_to_path};

pub(crate) async fn did_open(ctx: &ServerContext, params: DidOpenTextDocumentParams) {
    let Some(path) = uri_to_path(&params.text_document.uri) else {
        return;
    };

    let version = params.text_document.version;
    let text_len = params.text_document.text.len();
    ctx.documents
        .lock()
        .await
        .did_open(path.clone(), Some(version), &params.text_document.text);
    tracing::debug!(path = %path.display(), "opened clean document snapshot");
    tracing::trace!(
        path = %path.display(),
        version,
        text_len,
        "recorded open document freshness"
    );
}
