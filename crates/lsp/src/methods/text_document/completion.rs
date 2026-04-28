use tower_lsp_server::{jsonrpc::Result, ls_types::*};

use crate::{
    backend::ServerContext,
    methods::{internal_error, text_document, uri_to_path},
};

pub(crate) async fn completion(
    ctx: &ServerContext,
    params: CompletionParams,
) -> Result<Option<CompletionResponse>> {
    let Some(path) = uri_to_path(&params.text_document_position.text_document.uri) else {
        return Ok(None);
    };
    if text_document::is_dirty(ctx, &path).await {
        return Ok(Some(CompletionResponse::Array(Vec::new())));
    }

    let completions = ctx
        .engine
        .completion(path, params.text_document_position.position)
        .await
        .map_err(internal_error)?;

    Ok(Some(CompletionResponse::Array(completions)))
}
