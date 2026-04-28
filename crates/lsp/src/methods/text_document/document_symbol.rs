use tower_lsp_server::{jsonrpc::Result, ls_types::*};

use crate::{
    backend::ServerContext,
    methods::{internal_error, text_document, uri_to_path},
};

pub(crate) async fn document_symbol(
    ctx: &ServerContext,
    params: DocumentSymbolParams,
) -> Result<Option<DocumentSymbolResponse>> {
    let Some(path) = uri_to_path(&params.text_document.uri) else {
        return Ok(None);
    };
    if text_document::is_dirty(ctx, &path).await {
        return Ok(Some(DocumentSymbolResponse::Nested(Vec::new())));
    }

    let symbols = ctx
        .engine
        .document_symbol(path)
        .await
        .map_err(internal_error)?;

    Ok(Some(DocumentSymbolResponse::Nested(symbols)))
}
