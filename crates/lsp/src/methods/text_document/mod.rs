pub(crate) mod completion;
pub(crate) mod definition;
pub(crate) mod did_change;
pub(crate) mod did_close;
pub(crate) mod did_open;
pub(crate) mod did_save;
pub(crate) mod document_symbol;
pub(crate) mod hover;
pub(crate) mod inlay_hint;
pub(crate) mod type_definition;

use std::path::Path;

use crate::backend::ServerContext;

pub(crate) async fn is_dirty(ctx: &ServerContext, path: &Path) -> bool {
    let freshness = ctx.documents.lock().await.freshness(path);
    tracing::trace!(
        path = %path.display(),
        tracked = freshness.tracked(),
        version = ?freshness.version(),
        dirty = freshness.dirty(),
        saved_len = ?freshness.saved_len(),
        live_len = ?freshness.live_len(),
        saved_hash = ?freshness.saved_hash(),
        live_hash = ?freshness.live_hash(),
        "checked document freshness"
    );

    if freshness.dirty() {
        tracing::debug!(
            path = %path.display(),
            "returning empty result for dirty document"
        );
    }

    freshness.dirty()
}
