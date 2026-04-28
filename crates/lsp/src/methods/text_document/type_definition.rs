use tower_lsp_server::{
    jsonrpc::Result,
    ls_types::{request::*, *},
};

use crate::{
    backend::ServerContext,
    methods::{internal_error, uri_to_path},
};

pub(crate) async fn type_definition(
    ctx: &ServerContext,
    params: GotoTypeDefinitionParams,
) -> Result<Option<GotoTypeDefinitionResponse>> {
    let Some(path) = uri_to_path(&params.text_document_position_params.text_document.uri) else {
        return Ok(None);
    };

    let locations = ctx
        .engine
        .goto_type_definition(path, params.text_document_position_params.position)
        .await
        .map_err(internal_error)?;

    Ok(Some(GotoDefinitionResponse::Array(locations)))
}
