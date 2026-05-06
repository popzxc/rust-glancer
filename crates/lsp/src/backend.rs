use std::sync::Arc;

use tokio::sync::Mutex;
use tower_lsp_server::{
    Client, LanguageServer,
    jsonrpc::Result,
    ls_types::{request::*, *},
};

use crate::{
    check::CheckHandle, documents::DocumentStore, engine::EngineHandle, memory::MemoryControl,
    methods,
};

#[derive(Debug)]
pub(crate) struct Backend {
    ctx: ServerContext,
}

impl Backend {
    pub(crate) fn new(client: Client, memory_control: Arc<dyn MemoryControl>) -> Self {
        let documents = Arc::new(Mutex::new(DocumentStore::default()));
        Self {
            ctx: ServerContext {
                check: CheckHandle::new(client.clone(), Arc::clone(&documents)),
                client,
                engine: EngineHandle::spawn(memory_control),
                documents,
            },
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ServerContext {
    pub(crate) client: Client,
    pub(crate) check: CheckHandle,
    pub(crate) engine: EngineHandle,
    pub(crate) documents: Arc<Mutex<DocumentStore>>,
}

impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        methods::initialize(&self.ctx, params).await
    }

    async fn initialized(&self, params: InitializedParams) {
        methods::initialized(&self.ctx, params).await;
    }

    async fn shutdown(&self) -> Result<()> {
        methods::shutdown(&self.ctx).await
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        methods::text_document::did_open::did_open(&self.ctx, params).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        methods::text_document::did_change::did_change(&self.ctx, params).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        methods::text_document::did_save::did_save(&self.ctx, params).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        methods::text_document::did_close::did_close(&self.ctx, params).await;
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        methods::text_document::definition::definition(&self.ctx, params).await
    }

    async fn goto_type_definition(
        &self,
        params: GotoTypeDefinitionParams,
    ) -> Result<Option<GotoTypeDefinitionResponse>> {
        methods::text_document::type_definition::type_definition(&self.ctx, params).await
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        methods::text_document::hover::hover(&self.ctx, params).await
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        methods::text_document::completion::completion(&self.ctx, params).await
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        methods::text_document::document_symbol::document_symbol(&self.ctx, params).await
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        methods::text_document::inlay_hint::inlay_hint(&self.ctx, params).await
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<WorkspaceSymbolResponse>> {
        methods::workspace::symbol::symbol(&self.ctx, params).await
    }
}
