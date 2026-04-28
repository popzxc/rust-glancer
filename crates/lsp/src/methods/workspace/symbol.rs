use tower_lsp_server::{jsonrpc::Result, ls_types::*};

use crate::{backend::ServerContext, methods::internal_error};

pub(crate) async fn symbol(
    ctx: &ServerContext,
    params: WorkspaceSymbolParams,
) -> Result<Option<WorkspaceSymbolResponse>> {
    let symbols = ctx
        .engine
        .workspace_symbol(params.query)
        .await
        .map_err(internal_error)?;

    Ok(Some(WorkspaceSymbolResponse::Nested(symbols)))
}
