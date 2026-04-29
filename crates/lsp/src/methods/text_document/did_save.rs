use tower_lsp_server::ls_types::*;

use crate::{
    backend::ServerContext,
    methods::{internal_error, uri_to_path},
};

pub(crate) async fn did_save(ctx: &ServerContext, params: DidSaveTextDocumentParams) {
    let Some(path) = uri_to_path(&params.text_document.uri) else {
        return;
    };

    // A save commits the editor buffer to the snapshot model. Mark it clean before rebuilding so
    // follow-up LSP queries queue behind the engine reindex instead of receiving an empty
    // "dirty document" response that VS Code may cache.
    let saved_text = params.text;
    let saved_text_len = saved_text.as_ref().map(String::len);
    let mut documents = ctx.documents.lock().await;
    documents.did_save(path.clone(), saved_text.as_deref());
    let freshness = documents.freshness(&path);
    drop(documents);

    tracing::debug!(path = %path.display(), "marked document clean before save reindex");
    tracing::trace!(
        path = %path.display(),
        saved_text_len,
        tracked = freshness.tracked(),
        version = ?freshness.version(),
        dirty = freshness.dirty(),
        saved_len = ?freshness.saved_len(),
        live_len = ?freshness.live_len(),
        saved_hash = ?freshness.saved_hash(),
        live_hash = ?freshness.live_hash(),
        "document freshness before save reindex"
    );

    ctx.check.run_on_save(path.clone()).await;

    if let Err(error) = ctx.engine.did_save(path.clone(), saved_text).await {
        let mut documents = ctx.documents.lock().await;
        documents.mark_dirty_after_failed_save(path.clone());
        let freshness = documents.freshness(&path);
        drop(documents);

        tracing::trace!(
            path = %path.display(),
            tracked = freshness.tracked(),
            version = ?freshness.version(),
            dirty = freshness.dirty(),
            saved_len = ?freshness.saved_len(),
            live_len = ?freshness.live_len(),
            saved_hash = ?freshness.saved_hash(),
            live_hash = ?freshness.live_hash(),
            "document freshness after failed save reindex"
        );
        let error = internal_error(error);
        ctx.client
            .log_message(
                MessageType::ERROR,
                format!("failed to process saved file: {}", error.message),
            )
            .await;
        return;
    }

    let freshness = ctx.documents.lock().await.freshness(&path);
    tracing::trace!(
        path = %path.display(),
        tracked = freshness.tracked(),
        version = ?freshness.version(),
        dirty = freshness.dirty(),
        saved_len = ?freshness.saved_len(),
        live_len = ?freshness.live_len(),
        saved_hash = ?freshness.saved_hash(),
        live_hash = ?freshness.live_hash(),
        "document freshness after save reindex"
    );

    if let Err(error) = ctx.client.inlay_hint_refresh().await {
        tracing::debug!(
            error = %error,
            "failed to request inlay hint refresh after save"
        );
    }
}
