use tower_lsp_server::{jsonrpc::Result, ls_types::*};

use crate::{
    backend::ServerContext,
    methods::{internal_error, text_document, uri_to_path},
};

pub(crate) async fn definition(
    ctx: &ServerContext,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let Some(path) = uri_to_path(&params.text_document_position_params.text_document.uri) else {
        return Ok(None);
    };
    let position = params.text_document_position_params.position;
    tracing::trace!(
        path = %path.display(),
        line = position.line,
        character = position.character,
        "definition request received"
    );
    if text_document::is_dirty(ctx, &path).await {
        tracing::trace!(
            path = %path.display(),
            line = position.line,
            character = position.character,
            result_count = 0usize,
            reason = "dirty",
            "definition request suppressed"
        );
        return Ok(Some(GotoDefinitionResponse::Array(Vec::new())));
    }

    let locations = ctx
        .engine
        .goto_definition(path.clone(), position)
        .await
        .map_err(internal_error)?;
    tracing::trace!(
        path = %path.display(),
        line = position.line,
        character = position.character,
        result_count = locations.len(),
        "definition request answered"
    );

    Ok(Some(GotoDefinitionResponse::Array(locations)))
}
