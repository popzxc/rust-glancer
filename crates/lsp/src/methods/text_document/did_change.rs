use tower_lsp_server::ls_types::*;

use crate::{backend::ServerContext, methods::uri_to_path};

pub(crate) async fn did_change(ctx: &ServerContext, params: DidChangeTextDocumentParams) {
    let Some(path) = uri_to_path(&params.text_document.uri) else {
        return;
    };
    let full_text = params
        .content_changes
        .last()
        .and_then(|change| change.range.is_none().then_some(change.text.as_str()));
    let full_text_len = full_text.map(str::len);

    let mut documents = ctx.documents.lock().await;
    let change = documents.did_change(path.clone(), Some(params.text_document.version), full_text);
    let freshness = documents.freshness(&path);
    drop(documents);

    tracing::debug!(
        path = %path.display(),
        became_dirty = change.became_dirty,
        became_clean = change.became_clean,
        dirty = freshness.dirty(),
        "updated document freshness after change"
    );
    tracing::trace!(
        path = %path.display(),
        version = params.text_document.version,
        content_changes = params.content_changes.len(),
        full_text_len,
        tracked = freshness.tracked(),
        dirty = freshness.dirty(),
        saved_len = ?freshness.saved_len(),
        live_len = ?freshness.live_len(),
        saved_hash = ?freshness.saved_hash(),
        live_hash = ?freshness.live_hash(),
        "document freshness after change"
    );

    if change.became_dirty || change.became_clean {
        if let Err(error) = ctx.client.inlay_hint_refresh().await {
            tracing::debug!(
                error = %error,
                "failed to request inlay hint refresh after document freshness changed"
            );
        }
    }
}
