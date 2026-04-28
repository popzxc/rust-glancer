use tower_lsp_server::{jsonrpc::Result, ls_types::*};

use crate::{
    backend::ServerContext,
    methods::{internal_error, uri_to_path},
};

pub(crate) async fn document_symbol(
    ctx: &ServerContext,
    params: DocumentSymbolParams,
) -> Result<Option<DocumentSymbolResponse>> {
    let Some(path) = uri_to_path(&params.text_document.uri) else {
        return Ok(None);
    };
    tracing::trace!(
        path = %path.display(),
        "document symbol request received"
    );

    let freshness = ctx.documents.lock().await.freshness(&path);
    if freshness.dirty() {
        // LSP has refresh requests for features like inlay hints, but not for document symbols.
        // Returning an empty symbol tree while the document is dirty can leave VS Code's Outline
        // empty after save, so document symbols intentionally use the last saved snapshot.
        // TODO: This can show stale ranges while the dirty buffer shifts item spans. VSCode has an
        // open issue to trigger outline refresh, it is not implemented still: see
        // https://github.com/microsoft/vscode/issues/108722
        tracing::trace!(
            path = %path.display(),
            tracked = freshness.tracked(),
            version = ?freshness.version(),
            saved_len = ?freshness.saved_len(),
            live_len = ?freshness.live_len(),
            saved_hash = ?freshness.saved_hash(),
            live_hash = ?freshness.live_hash(),
            "document symbol request is using saved snapshot for dirty document"
        );
    }

    let symbols = ctx
        .engine
        .document_symbol(path.clone())
        .await
        .map_err(internal_error)?;
    tracing::trace!(
        path = %path.display(),
        result_count = symbols.len(),
        "document symbol request answered"
    );

    Ok(Some(DocumentSymbolResponse::Nested(symbols)))
}
