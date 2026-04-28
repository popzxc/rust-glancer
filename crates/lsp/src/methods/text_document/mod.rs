pub(crate) mod completion;
pub(crate) mod definition;
pub(crate) mod did_change;
pub(crate) mod did_close;
pub(crate) mod did_open;
pub(crate) mod did_save;
pub(crate) mod document_symbol;
pub(crate) mod inlay_hint;
pub(crate) mod type_definition;

use std::path::Path;

use crate::backend::ServerContext;

pub(crate) async fn is_dirty(ctx: &ServerContext, path: &Path) -> bool {
    let dirty = ctx.documents.lock().await.is_dirty(path);
    if dirty {
        tracing::debug!(
            path = %path.display(),
            "returning empty result for dirty document"
        );
    }

    dirty
}
