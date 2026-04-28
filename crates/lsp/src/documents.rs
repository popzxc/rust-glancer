use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// LSP-side document freshness state.
///
/// The analysis engine remains save-only. This store only records whether VS Code has told us a
/// file's live buffer has diverged from the saved snapshot, so position-sensitive requests can
/// avoid returning stale answers.
#[derive(Debug, Clone, Default)]
pub(crate) struct DocumentStore {
    documents: HashMap<PathBuf, DocumentState>,
}

impl DocumentStore {
    pub(crate) fn did_open(&mut self, path: PathBuf, version: Option<i32>) {
        self.documents.insert(
            path,
            DocumentState {
                version,
                dirty: false,
            },
        );
    }

    /// Marks an open document dirty and returns whether this was the clean-to-dirty transition.
    pub(crate) fn did_change(&mut self, path: PathBuf, version: Option<i32>) -> bool {
        let document = self.documents.entry(path).or_default();
        document.version = version;
        let was_dirty = document.dirty;
        document.dirty = true;
        !was_dirty
    }

    pub(crate) fn did_save(&mut self, path: PathBuf) {
        let document = self.documents.entry(path).or_default();
        document.dirty = false;
    }

    pub(crate) fn did_close(&mut self, path: &Path) {
        self.documents.remove(path);
    }

    pub(crate) fn is_dirty(&self, path: &Path) -> bool {
        self.documents
            .get(path)
            .is_some_and(|document| document.dirty)
    }
}

#[derive(Debug, Clone, Default)]
struct DocumentState {
    version: Option<i32>,
    dirty: bool,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::DocumentStore;

    #[test]
    fn tracks_clean_to_dirty_to_clean_document_lifecycle() {
        let path = PathBuf::from("/workspace/src/lib.rs");
        let mut store = DocumentStore::default();

        store.did_open(path.clone(), Some(1));
        assert!(!store.is_dirty(&path));

        assert!(store.did_change(path.clone(), Some(2)));
        assert!(store.is_dirty(&path));

        assert!(!store.did_change(path.clone(), Some(3)));
        assert!(store.is_dirty(&path));

        store.did_save(path.clone());
        assert!(!store.is_dirty(&path));

        store.did_close(&path);
        assert!(!store.is_dirty(&path));
    }
}
