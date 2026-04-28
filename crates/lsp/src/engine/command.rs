use std::path::PathBuf;

use tokio::sync::oneshot;
use tower_lsp_server::ls_types;

pub(super) type EngineResponse<T> = oneshot::Sender<anyhow::Result<T>>;

#[derive(Debug)]
pub(super) enum EngineCommand {
    Initialize {
        root: PathBuf,
        respond_to: EngineResponse<()>,
    },
    DidSave {
        path: PathBuf,
        text: Option<String>,
        respond_to: EngineResponse<()>,
    },
    GotoDefinition {
        path: PathBuf,
        position: ls_types::Position,
        respond_to: EngineResponse<Vec<ls_types::Location>>,
    },
    GotoTypeDefinition {
        path: PathBuf,
        position: ls_types::Position,
        respond_to: EngineResponse<Vec<ls_types::Location>>,
    },
    Completion {
        path: PathBuf,
        position: ls_types::Position,
        respond_to: EngineResponse<Vec<ls_types::CompletionItem>>,
    },
    DocumentSymbol {
        path: PathBuf,
        respond_to: EngineResponse<Vec<ls_types::DocumentSymbol>>,
    },
    InlayHint {
        path: PathBuf,
        range: ls_types::Range,
        respond_to: EngineResponse<Vec<ls_types::InlayHint>>,
    },
    WorkspaceSymbol {
        query: String,
        respond_to: EngineResponse<Vec<ls_types::WorkspaceSymbol>>,
    },
    Shutdown(EngineResponse<()>),
}
