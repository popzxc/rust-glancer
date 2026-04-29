use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

use crate::documents::DocumentStore;
use anyhow::Context as _;
use tokio::{process::Command, sync::Mutex, task::JoinHandle};
use tower_lsp_server::{
    Client,
    ls_types::{Diagnostic, NumberOrString, ProgressToken, Uri},
};

use super::{config::CheckConfig, diagnostics::CheckDiagnostics};

/// Runs Cargo diagnostics independently from the synchronous analysis engine.
#[derive(Clone, Debug)]
pub(crate) struct CheckHandle {
    client: Client,
    documents: Arc<Mutex<DocumentStore>>,
    state: Arc<Mutex<CheckState>>,
    current: Arc<Mutex<Option<CurrentCheck>>>,
}

impl CheckHandle {
    pub(crate) fn new(client: Client, documents: Arc<Mutex<DocumentStore>>) -> Self {
        Self {
            client,
            documents,
            state: Arc::default(),
            current: Arc::default(),
        }
    }

    pub(crate) async fn configure(&self, workspace_root: PathBuf, config: CheckConfig) {
        let mut state = self.state.lock().await;
        state.workspace_root = Some(workspace_root);
        state.config = config;
    }

    pub(crate) async fn run_on_save(&self, saved_path: PathBuf) {
        let snapshot = {
            let mut state = self.state.lock().await;
            if !state.config.on_save {
                return;
            }
            let Some(workspace_root) = state.workspace_root.clone() else {
                tracing::debug!("cargo diagnostics requested before workspace configuration");
                return;
            };

            state.generation += 1;
            CheckSnapshot {
                generation: state.generation,
                workspace_root,
                config: state.config.clone(),
            }
        };

        if let Some(current) = self.current.lock().await.take() {
            current.task.abort();
            Self::finish_progress(
                &self.client,
                current.progress_token,
                Some("Cancelled".to_string()),
            )
            .await;
            tracing::debug!("cancelled previous cargo diagnostics run");
        }

        let client = self.client.clone();
        let documents = Arc::clone(&self.documents);
        let state = Arc::clone(&self.state);
        let current = Arc::clone(&self.current);
        let progress_token =
            ProgressToken::String(format!("rust-glancer/check/{}", snapshot.generation));
        let task_progress_token = progress_token.clone();
        let task = tokio::spawn(async move {
            let generation = snapshot.generation;
            Self::begin_progress(
                &client,
                task_progress_token.clone(),
                snapshot.config.user_facing_command(),
            )
            .await;
            let result = CheckRun::new(snapshot, saved_path).run().await;

            let publish = match result {
                Ok(diagnostics) => {
                    let mut state = state.lock().await;
                    if state.generation != generation {
                        Self::finish_progress(
                            &client,
                            task_progress_token,
                            Some("Superseded".to_string()),
                        )
                        .await;
                        return;
                    }
                    let documents = documents.lock().await;
                    state.publish_plan(diagnostics, &documents)
                }
                Err(error) => {
                    tracing::error!(
                        generation,
                        error = %error,
                        "cargo diagnostics run failed"
                    );
                    client
                        .log_message(
                            tower_lsp_server::ls_types::MessageType::ERROR,
                            format!("cargo diagnostics failed: {error}"),
                        )
                        .await;
                    current.lock().await.take();
                    Self::finish_progress(&client, task_progress_token, Some("Failed".to_string()))
                        .await;
                    return;
                }
            };

            for publication in publish {
                let path = publication.path;
                let Some(uri) = Uri::from_file_path(&path) else {
                    tracing::debug!(
                        path = %path.display(),
                        "failed to convert diagnostics path to URI"
                    );
                    continue;
                };
                client
                    .publish_diagnostics(uri, publication.diagnostics, publication.version)
                    .await;
            }

            let mut current = current.lock().await;
            if current
                .as_ref()
                .is_some_and(|current| current.progress_token == task_progress_token)
            {
                current.take();
            }
            Self::finish_progress(&client, task_progress_token, Some("Finished".to_string())).await;
        });

        *self.current.lock().await = Some(CurrentCheck {
            task,
            progress_token,
        });
    }

    pub(crate) async fn shutdown(&self) {
        if let Some(current) = self.current.lock().await.take() {
            current.task.abort();
            Self::finish_progress(
                &self.client,
                current.progress_token,
                Some("Cancelled".to_string()),
            )
            .await;
        }
    }

    async fn begin_progress(client: &Client, token: ProgressToken, command: String) {
        if let Err(error) = client.create_work_done_progress(token.clone()).await {
            tracing::debug!(
                error = %error,
                "failed to create cargo diagnostics progress token"
            );
            return;
        }

        let _ = client
            .progress(token, "Cargo diagnostics")
            .with_message(command)
            .begin()
            .await;
    }

    async fn finish_progress(client: &Client, token: ProgressToken, message: Option<String>) {
        client
            .send_notification::<tower_lsp_server::ls_types::notification::Progress>(
                tower_lsp_server::ls_types::ProgressParams {
                    token,
                    value: tower_lsp_server::ls_types::ProgressParamsValue::WorkDone(
                        tower_lsp_server::ls_types::WorkDoneProgress::End(
                            tower_lsp_server::ls_types::WorkDoneProgressEnd { message },
                        ),
                    ),
                },
            )
            .await;
    }
}

#[derive(Debug)]
struct CurrentCheck {
    task: JoinHandle<()>,
    progress_token: NumberOrString,
}

#[derive(Debug)]
struct CheckState {
    workspace_root: Option<PathBuf>,
    config: CheckConfig,
    generation: u64,
    published_paths: BTreeSet<PathBuf>,
}

impl Default for CheckState {
    fn default() -> Self {
        Self {
            workspace_root: None,
            config: CheckConfig::default(),
            generation: 0,
            published_paths: BTreeSet::new(),
        }
    }
}

impl CheckState {
    fn publish_plan(
        &mut self,
        diagnostics: CheckDiagnostics,
        documents: &DocumentStore,
    ) -> Vec<DiagnosticPublication> {
        let mut publish = Vec::new();
        let mut next_paths = BTreeSet::new();

        for (path, diagnostics) in diagnostics.into_inner() {
            let freshness = documents.freshness(&path);
            let version = freshness.tracked().then(|| freshness.version()).flatten();

            if freshness.dirty() {
                // Cargo diagnostics belong to the saved snapshot. Once the editor has a newer
                // dirty buffer, publishing those ranges would attach stale spans to live text.
                publish.push(DiagnosticPublication {
                    path,
                    diagnostics: Vec::new(),
                    version,
                });
                continue;
            }

            next_paths.insert(path.clone());
            publish.push(DiagnosticPublication {
                path,
                diagnostics,
                version,
            });
        }

        for stale_path in self.published_paths.difference(&next_paths) {
            let freshness = documents.freshness(stale_path);
            let version = freshness.tracked().then(|| freshness.version()).flatten();
            publish.push(DiagnosticPublication {
                path: stale_path.clone(),
                diagnostics: Vec::new(),
                version,
            });
        }

        self.published_paths = next_paths;
        publish
    }
}

#[derive(Debug)]
struct DiagnosticPublication {
    path: PathBuf,
    diagnostics: Vec<Diagnostic>,
    version: Option<i32>,
}

#[derive(Debug)]
struct CheckSnapshot {
    generation: u64,
    workspace_root: PathBuf,
    config: CheckConfig,
}

struct CheckRun {
    snapshot: CheckSnapshot,
    saved_path: PathBuf,
}

impl CheckRun {
    fn new(snapshot: CheckSnapshot, saved_path: PathBuf) -> Self {
        Self {
            snapshot,
            saved_path,
        }
    }

    async fn run(self) -> anyhow::Result<CheckDiagnostics> {
        let started = std::time::Instant::now();
        let source = format!("cargo {}", self.snapshot.config.command);
        tracing::info!(
            generation = self.snapshot.generation,
            saved_path = %self.saved_path.display(),
            command = %self.snapshot.config.user_facing_command(),
            "starting cargo diagnostics"
        );

        let mut command = Command::new("cargo");
        command
            .arg(&self.snapshot.config.command)
            .arg("--message-format=json")
            .args(&self.snapshot.config.arguments)
            .current_dir(&self.snapshot.workspace_root)
            .kill_on_drop(true);

        let output = command
            .output()
            .await
            .with_context(|| format!("while attempting to run {}", source))?;
        let diagnostics = CheckDiagnostics::parse(
            &self.snapshot.workspace_root,
            &source,
            &output.stdout,
            &output.stderr,
        );

        if !output.status.success() && diagnostics.is_empty() {
            anyhow::bail!(
                "{} exited with {} and did not produce JSON diagnostics",
                source,
                output.status
            );
        }

        tracing::info!(
            generation = self.snapshot.generation,
            success = output.status.success(),
            diagnostic_files = diagnostics.paths().len(),
            elapsed_ms = started.elapsed().as_millis(),
            "cargo diagnostics finished"
        );

        Ok(diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, path::PathBuf};

    use tower_lsp_server::ls_types::{Diagnostic, Position, Range};

    use super::{CheckDiagnostics, CheckState};
    use crate::documents::DocumentStore;

    #[test]
    fn publish_plan_clears_stale_diagnostic_files() {
        let mut state = CheckState::default();
        state
            .published_paths
            .insert(PathBuf::from("/workspace/src/lib.rs"));
        state
            .published_paths
            .insert(PathBuf::from("/workspace/src/main.rs"));

        let diagnostics = CheckDiagnostics::from_map(BTreeMap::from([(
            PathBuf::from("/workspace/src/main.rs"),
            vec![diagnostic("still broken")],
        )]));

        let documents = DocumentStore::default();
        let publish = state.publish_plan(diagnostics, &documents);

        assert_eq!(publish.len(), 2);
        assert_eq!(publish[0].path, PathBuf::from("/workspace/src/main.rs"));
        assert_eq!(publish[0].diagnostics.len(), 1);
        assert_eq!(publish[1].path, PathBuf::from("/workspace/src/lib.rs"));
        assert!(publish[1].diagnostics.is_empty());
        assert_eq!(
            state.published_paths,
            [PathBuf::from("/workspace/src/main.rs")].into()
        );
    }

    #[test]
    fn publish_plan_clears_dirty_documents_instead_of_publishing_saved_ranges() {
        let mut state = CheckState::default();
        let path = PathBuf::from("/workspace/src/lib.rs");
        let diagnostics =
            CheckDiagnostics::from_map(BTreeMap::from([(path.clone(), vec![diagnostic("new")])]));
        let mut documents = DocumentStore::default();
        documents.did_open(path.clone(), Some(1), "fn main() {}\n");
        documents.did_change(path.clone(), Some(2), Some("fn main() {\n}\n"));

        let publish = state.publish_plan(diagnostics, &documents);

        assert_eq!(publish.len(), 1);
        assert_eq!(publish[0].path, path);
        assert!(publish[0].diagnostics.is_empty());
        assert_eq!(publish[0].version, Some(2));
        assert!(state.published_paths.is_empty());
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
