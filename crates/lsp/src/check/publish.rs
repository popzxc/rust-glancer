use std::{collections::BTreeSet, path::PathBuf};

use tower_lsp_server::{
    Client,
    ls_types::{Diagnostic, Uri},
};

use crate::documents::DocumentStore;

use super::diagnostics::CheckDiagnostics;

/// Complete diagnostics publication for one cargo diagnostics run.
///
/// `file_diagnostics` is what should be sent to the client, while `published_paths` records
/// which files now have live cargo diagnostics so a later run can clear stale files.
pub(super) struct WorkspaceDiagnostics {
    file_diagnostics: Vec<FileDiagnostics>,
    published_paths: BTreeSet<PathBuf>,
}

impl WorkspaceDiagnostics {
    pub(super) fn new(
        diagnostics: CheckDiagnostics,
        documents: &DocumentStore,
        previous_paths: &BTreeSet<PathBuf>,
    ) -> Self {
        let mut file_diagnostics = Vec::new();
        let mut published_paths = BTreeSet::new();

        for (path, diagnostics) in diagnostics.into_inner() {
            let freshness = documents.freshness(&path);
            let version = freshness.tracked().then(|| freshness.version()).flatten();

            if freshness.dirty() {
                // Cargo diagnostics belong to the saved snapshot. Once the editor has a newer
                // dirty buffer, publishing those ranges would attach stale spans to live text.
                file_diagnostics.push(FileDiagnostics {
                    path,
                    diagnostics: Vec::new(),
                    version,
                });
                continue;
            }

            published_paths.insert(path.clone());
            file_diagnostics.push(FileDiagnostics {
                path,
                diagnostics,
                version,
            });
        }

        for stale_path in previous_paths.difference(&published_paths) {
            let freshness = documents.freshness(stale_path);
            let version = freshness.tracked().then(|| freshness.version()).flatten();
            file_diagnostics.push(FileDiagnostics {
                path: stale_path.clone(),
                diagnostics: Vec::new(),
                version,
            });
        }

        Self {
            file_diagnostics,
            published_paths,
        }
    }

    pub(super) fn take_published_paths(&mut self) -> BTreeSet<PathBuf> {
        std::mem::take(&mut self.published_paths)
    }

    pub(super) async fn publish(self, client: &Client) {
        for file_diagnostics in self.file_diagnostics {
            file_diagnostics.publish(client).await;
        }
    }
}

#[derive(Debug)]
struct FileDiagnostics {
    path: PathBuf,
    diagnostics: Vec<Diagnostic>,
    version: Option<i32>,
}

impl FileDiagnostics {
    async fn publish(self, client: &Client) {
        let Some(uri) = Uri::from_file_path(&self.path) else {
            tracing::debug!(
                path = %self.path.display(),
                "failed to convert diagnostics path to URI"
            );
            return;
        };

        client
            .publish_diagnostics(uri, self.diagnostics, self.version)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        path::PathBuf,
    };

    use tower_lsp_server::ls_types::{Diagnostic, Position, Range};

    use super::WorkspaceDiagnostics;
    use crate::{check::diagnostics::CheckDiagnostics, documents::DocumentStore};

    #[test]
    fn new_clears_stale_diagnostic_files() {
        let previous_paths = BTreeSet::from([
            PathBuf::from("/workspace/src/lib.rs"),
            PathBuf::from("/workspace/src/main.rs"),
        ]);
        let diagnostics = CheckDiagnostics::from_map(BTreeMap::from([(
            PathBuf::from("/workspace/src/main.rs"),
            vec![diagnostic("still broken")],
        )]));

        let documents = DocumentStore::default();
        let workspace_diagnostics =
            WorkspaceDiagnostics::new(diagnostics, &documents, &previous_paths);

        assert_eq!(workspace_diagnostics.file_diagnostics.len(), 2);
        assert_eq!(
            workspace_diagnostics.file_diagnostics[0].path,
            PathBuf::from("/workspace/src/main.rs")
        );
        assert_eq!(
            workspace_diagnostics.file_diagnostics[0].diagnostics.len(),
            1
        );
        assert_eq!(
            workspace_diagnostics.file_diagnostics[1].path,
            PathBuf::from("/workspace/src/lib.rs")
        );
        assert!(
            workspace_diagnostics.file_diagnostics[1]
                .diagnostics
                .is_empty()
        );
        assert_eq!(
            workspace_diagnostics.published_paths,
            [PathBuf::from("/workspace/src/main.rs")].into()
        );
    }

    #[test]
    fn new_clears_dirty_documents_instead_of_publishing_saved_ranges() {
        let path = PathBuf::from("/workspace/src/lib.rs");
        let diagnostics =
            CheckDiagnostics::from_map(BTreeMap::from([(path.clone(), vec![diagnostic("new")])]));
        let mut documents = DocumentStore::default();
        documents.did_open(path.clone(), Some(1), "fn main() {}\n");
        documents.did_change(path.clone(), Some(2), Some("fn main() {\n}\n"));

        let workspace_diagnostics =
            WorkspaceDiagnostics::new(diagnostics, &documents, &BTreeSet::new());

        assert_eq!(workspace_diagnostics.file_diagnostics.len(), 1);
        assert_eq!(workspace_diagnostics.file_diagnostics[0].path, path);
        assert!(
            workspace_diagnostics.file_diagnostics[0]
                .diagnostics
                .is_empty()
        );
        assert_eq!(workspace_diagnostics.file_diagnostics[0].version, Some(2));
        assert!(workspace_diagnostics.published_paths.is_empty());
    }

    fn diagnostic(message: &str) -> Diagnostic {
        Diagnostic {
            range: Range::new(Position::new(0, 0), Position::new(0, 1)),
            severity: None,
            code: None,
            code_description: None,
            source: None,
            message: message.to_string(),
            related_information: None,
            tags: None,
            data: None,
        }
    }
}
