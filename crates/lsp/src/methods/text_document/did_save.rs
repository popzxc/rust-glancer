use tower_lsp_server::ls_types::*;

use crate::{
    backend::ServerContext,
    methods::{internal_error, uri_to_path},
};

pub(crate) async fn did_save(ctx: &ServerContext, params: DidSaveTextDocumentParams) {
    let Some(path) = uri_to_path(&params.text_document.uri) else {
        return;
    };

    if let Err(error) = ctx.engine.did_save(path, params.text).await {
        let error = internal_error(error);
        ctx.client
            .log_message(
                MessageType::ERROR,
                format!("failed to process saved file: {}", error.message),
            )
            .await;
    }
}
