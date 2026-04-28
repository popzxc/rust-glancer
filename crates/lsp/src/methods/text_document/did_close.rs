use tower_lsp_server::ls_types::*;

use crate::{backend::ServerContext, methods::uri_to_path};

pub(crate) async fn did_close(ctx: &ServerContext, params: DidCloseTextDocumentParams) {
    let Some(path) = uri_to_path(&params.text_document.uri) else {
        return;
    };

    let mut documents = ctx.documents.lock().await;
    let freshness = documents.freshness(&path);
    documents.did_close(&path);
    drop(documents);

    tracing::debug!(path = %path.display(), "closed document");
    tracing::trace!(
        path = %path.display(),
        tracked = freshness.tracked(),
        version = ?freshness.version(),
        dirty = freshness.dirty(),
        saved_len = ?freshness.saved_len(),
        live_len = ?freshness.live_len(),
        saved_hash = ?freshness.saved_hash(),
        live_hash = ?freshness.live_hash(),
        "removed document freshness"
    );
}
