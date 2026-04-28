mod command;
mod worker;

use std::{
    path::PathBuf,
    sync::mpsc::{self, Sender},
    thread,
};

use anyhow::Context as _;
use tokio::sync::oneshot;
use tower_lsp_server::ls_types;

use self::{
    command::{EngineCommand, EngineResponse},
    worker::EngineWorker,
};

#[derive(Clone, Debug)]
pub(crate) struct EngineHandle {
    sender: Sender<EngineCommand>,
}

impl EngineHandle {
    pub(crate) fn spawn() -> Self {
        let (sender, receiver) = mpsc::channel();

        thread::spawn(move || EngineWorker::new().run(receiver));

        Self { sender }
    }

    pub(crate) async fn initialize(&self, root: PathBuf) -> anyhow::Result<()> {
        self.request(|respond_to| EngineCommand::Initialize { root, respond_to })
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

    pub(crate) async fn workspace_symbol(
        &self,
        query: String,
    ) -> anyhow::Result<Vec<ls_types::WorkspaceSymbol>> {
        self.request(|respond_to| EngineCommand::WorkspaceSymbol { query, respond_to })
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
