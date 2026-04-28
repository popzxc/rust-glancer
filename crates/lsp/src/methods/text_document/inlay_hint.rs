use tower_lsp_server::{jsonrpc::Result, ls_types::*};

use crate::{
    backend::ServerContext,
    methods::{internal_error, uri_to_path},
};

pub(crate) async fn inlay_hint(
    ctx: &ServerContext,
    params: InlayHintParams,
) -> Result<Option<Vec<InlayHint>>> {
    let Some(path) = uri_to_path(&params.text_document.uri) else {
        return Ok(None);
    };

    let hints = ctx
        .engine
        .inlay_hint(path, params.range)
        .await
        .map_err(internal_error)?;

    Ok(Some(hints))
}
