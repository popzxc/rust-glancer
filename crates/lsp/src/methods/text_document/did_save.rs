use tower_lsp_server::ls_types::*;

use crate::{
    backend::ServerContext,
    methods::{internal_error, uri_to_path},
};

pub(crate) async fn did_save(ctx: &ServerContext, params: DidSaveTextDocumentParams) {
    let Some(path) = uri_to_path(&params.text_document.uri) else {
        return;
    };

    if let Err(error) = ctx.engine.did_save(path.clone(), params.text).await {
        let error = internal_error(error);
        ctx.client
            .log_message(
                MessageType::ERROR,
                format!("failed to process saved file: {}", error.message),
            )
            .await;
        return;
    }

    ctx.documents.lock().await.did_save(path.clone());
    tracing::debug!(path = %path.display(), "marked document clean after save");

    if let Err(error) = ctx.client.inlay_hint_refresh().await {
        tracing::debug!(
            error = %error,
            "failed to request inlay hint refresh after save"
        );
    }
}
