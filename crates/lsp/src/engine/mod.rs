mod command;
mod worker;

use std::{
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Sender},
    },
    thread,
};

use anyhow::Context as _;
use rg_project::PackageResidencyPolicy;
use rg_workspace::CargoMetadataConfig;
use tokio::sync::oneshot;
use tower_lsp_server::ls_types;

use self::{
    command::{EngineCommand, EngineResponse},
    worker::EngineWorker,
};
use crate::memory::MemoryControl;

#[derive(Clone, Debug)]
pub(crate) struct EngineHandle {
    sender: Sender<EngineCommand>,
}

impl EngineHandle {
    pub(crate) fn spawn(memory_control: Arc<dyn MemoryControl>) -> Self {
        let (sender, receiver) = mpsc::channel();

        thread::spawn(move || EngineWorker::new(memory_control).run(receiver));

        Self { sender }
    }

    pub(crate) async fn initialize(
        &self,
        root: PathBuf,
        package_residency_policy: PackageResidencyPolicy,
        cargo_metadata_config: CargoMetadataConfig,
    ) -> anyhow::Result<()> {
        self.request(|respond_to| EngineCommand::Initialize {
            root,
            package_residency_policy,
            cargo_metadata_config,
            respond_to,
        })
        .await
    }

    pub(crate) async fn did_save(&self, path: PathBuf, text: Option<String>) -> anyhow::Result<()> {
        self.request(|respond_to| EngineCommand::DidSave {
            path,
            text,
            respond_to,
        })
        .await
    }

    pub(crate) async fn goto_definition(
        &self,
        path: PathBuf,
        position: ls_types::Position,
    ) -> anyhow::Result<Vec<ls_types::Location>> {
        self.request(|respond_to| EngineCommand::GotoDefinition {
            path,
            position,
            respond_to,
        })
        .await
    }

    pub(crate) async fn goto_type_definition(
        &self,
        path: PathBuf,
        position: ls_types::Position,
    ) -> anyhow::Result<Vec<ls_types::Location>> {
        self.request(|respond_to| EngineCommand::GotoTypeDefinition {
            path,
            position,
            respond_to,
        })
        .await
    }

    pub(crate) async fn hover(
        &self,
        path: PathBuf,
        position: ls_types::Position,
    ) -> anyhow::Result<Option<ls_types::Hover>> {
        self.request(|respond_to| EngineCommand::Hover {
            path,
            position,
            respond_to,
        })
        .await
    }

    pub(crate) async fn completion(
        &self,
        path: PathBuf,
        position: ls_types::Position,
    ) -> anyhow::Result<Vec<ls_types::CompletionItem>> {
        self.request(|respond_to| EngineCommand::Completion {
            path,
            position,
            respond_to,
        })
        .await
    }

    pub(crate) async fn document_symbol(
        &self,
        path: PathBuf,
    ) -> anyhow::Result<Vec<ls_types::DocumentSymbol>> {
        self.request(|respond_to| EngineCommand::DocumentSymbol { path, respond_to })
            .await
    }

    pub(crate) async fn inlay_hint(
        &self,
        path: PathBuf,
        range: ls_types::Range,
    ) -> anyhow::Result<Vec<ls_types::InlayHint>> {
        self.request(|respond_to| EngineCommand::InlayHint {
            path,
            range,
            respond_to,
        })
        .await
    }

    pub(crate) async fn workspace_symbol(
        &self,
        query: String,
    ) -> anyhow::Result<Vec<ls_types::WorkspaceSymbol>> {
        self.request(|respond_to| EngineCommand::WorkspaceSymbol { query, respond_to })
            .await
    }

    pub(crate) async fn reindex_workspace(&self) -> anyhow::Result<()> {
        self.request(|respond_to| EngineCommand::ReindexWorkspace { respond_to })
            .await
    }

    pub(crate) async fn shutdown(&self) -> anyhow::Result<()> {
        self.request(EngineCommand::Shutdown).await
    }

    async fn request<T>(
        &self,
        build: impl FnOnce(EngineResponse<T>) -> EngineCommand,
    ) -> anyhow::Result<T>
    where
        T: Send + 'static,
    {
        let (respond_to, response) = oneshot::channel();
        self.sender
            .send(build(respond_to))
            .context("while attempting to send LSP engine command")?;

        response
            .await
            .context("while attempting to receive LSP engine response")?
    }
}
