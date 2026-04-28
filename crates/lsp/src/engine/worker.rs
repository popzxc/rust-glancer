use std::{
    path::{Path, PathBuf},
    sync::mpsc::Receiver,
};

use anyhow::Context as _;
use rg_def_map::TargetRef;
use rg_project::{AnalysisHost, AnalysisSnapshot, FileContext, SavedFileChange};
use rg_workspace::{SysrootSources, WorkspaceMetadata};
use tower_lsp_server::ls_types;

use crate::{
    engine::command::EngineCommand,
    proto::{completion, navigation, position, symbols},
};

#[derive(Debug, Default)]
pub(super) struct EngineWorker {
    host: Option<AnalysisHost>,
}

impl EngineWorker {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn run(mut self, receiver: Receiver<EngineCommand>) {
        while let Ok(command) = receiver.recv() {
            match command {
                EngineCommand::Initialize { root, respond_to } => {
                    let _ = respond_to.send(self.initialize(root));
                }
                EngineCommand::DidSave {
                    path,
                    text,
                    respond_to,
                } => {
                    let _ = respond_to.send(self.did_save(path, text));
                }
                EngineCommand::GotoDefinition {
                    path,
                    position,
                    respond_to,
                } => {
                    let _ = respond_to.send(self.goto_definition(path, position));
                }
                EngineCommand::GotoTypeDefinition {
                    path,
                    position,
                    respond_to,
                } => {
                    let _ = respond_to.send(self.goto_type_definition(path, position));
                }
                EngineCommand::Completion {
                    path,
                    position,
                    respond_to,
                } => {
                    let _ = respond_to.send(self.completion(path, position));
                }
                EngineCommand::DocumentSymbol { path, respond_to } => {
                    let _ = respond_to.send(self.document_symbol(path));
                }
                EngineCommand::WorkspaceSymbol { query, respond_to } => {
                    let _ = respond_to.send(self.workspace_symbol(&query));
                }
                EngineCommand::Shutdown(respond_to) => {
                    let _ = respond_to.send(Ok(()));
                    break;
                }
            }
        }
    }

    fn initialize(&mut self, root: PathBuf) -> anyhow::Result<()> {
        let manifest_path = root.join("Cargo.toml");
        if !manifest_path.exists() {
            anyhow::bail!(
                "workspace root {} does not contain Cargo.toml",
                root.display()
            );
        }

        let metadata = cargo_metadata::MetadataCommand::new()
            .manifest_path(&manifest_path)
            .exec()
            .context("while attempting to run cargo metadata for LSP initialization")?;
        let workspace = WorkspaceMetadata::from_cargo(metadata);
        let sysroot = SysrootSources::discover(workspace.workspace_root());
        let workspace = workspace.with_sysroot_sources(sysroot);
        let host = AnalysisHost::build(workspace)
            .context("while attempting to build LSP analysis host")?;

        self.host = Some(host);

        Ok(())
    }

    fn did_save(&mut self, path: PathBuf, text: Option<String>) -> anyhow::Result<()> {
        let host = self
            .host
            .as_mut()
            .context("LSP engine is not initialized")?;

        // Save notifications are the only source-update signal rust-glimpser currently supports.
        // If the client does not include text, we fall back to the saved file on disk and keep the
        // same committed-save semantics.
        let text = match text {
            Some(text) => text,
            None => std::fs::read_to_string(&path).with_context(|| {
                format!("while attempting to read saved file {}", path.display())
            })?,
        };

        host.apply_change(SavedFileChange::new(path, text))
            .context("while attempting to apply saved file change")?;

        Ok(())
    }

    fn goto_definition(
        &self,
        path: PathBuf,
        position: ls_types::Position,
    ) -> anyhow::Result<Vec<ls_types::Location>> {
        self.navigation_query(path, position, NavigationQuery::Definition)
    }

    fn goto_type_definition(
        &self,
        path: PathBuf,
        position: ls_types::Position,
    ) -> anyhow::Result<Vec<ls_types::Location>> {
        self.navigation_query(path, position, NavigationQuery::TypeDefinition)
    }

    fn completion(
        &self,
        path: PathBuf,
        position: ls_types::Position,
    ) -> anyhow::Result<Vec<ls_types::CompletionItem>> {
        let snapshot = self.snapshot()?;
        let mut completions = Vec::new();

        for (context, target, offset) in self.target_offsets(snapshot, &path, position)? {
            let analysis = snapshot.analysis();
            for item in analysis.completions_at_dot(target, context.file, offset) {
                let item = completion::completion_item(item);
                if !completions.contains(&item) {
                    completions.push(item);
                }
            }
        }

        Ok(completions)
    }

    fn document_symbol(&self, path: PathBuf) -> anyhow::Result<Vec<ls_types::DocumentSymbol>> {
        let snapshot = self.snapshot()?;
        let mut lsp_symbols = Vec::new();

        for context in self.file_contexts(snapshot, &path)? {
            for target in context.targets {
                let symbols = snapshot.analysis().document_symbols(target, context.file);
                for symbol in symbols {
                    let symbol =
                        symbols::document_symbol(snapshot.parse_db(), context.package.0, symbol)?;
                    if !lsp_symbols.contains(&symbol) {
                        lsp_symbols.push(symbol);
                    }
                }
            }
        }

        Ok(lsp_symbols)
    }

    fn workspace_symbol(&self, query: &str) -> anyhow::Result<Vec<ls_types::WorkspaceSymbol>> {
        let snapshot = self.snapshot()?;
        let mut lsp_symbols = Vec::new();

        for symbol in snapshot.analysis().workspace_symbols(query) {
            let Some(symbol) = symbols::workspace_symbol(snapshot.parse_db(), symbol)? else {
                continue;
            };
            if !lsp_symbols.contains(&symbol) {
                lsp_symbols.push(symbol);
            }
        }

        Ok(lsp_symbols)
    }

    fn navigation_query(
        &self,
        path: PathBuf,
        position: ls_types::Position,
        query: NavigationQuery,
    ) -> anyhow::Result<Vec<ls_types::Location>> {
        let snapshot = self.snapshot()?;
        let mut locations = Vec::new();

        for (context, target, offset) in self.target_offsets(snapshot, &path, position)? {
            let targets = match query {
                NavigationQuery::Definition => {
                    snapshot
                        .analysis()
                        .goto_definition(target, context.file, offset)
                }
                NavigationQuery::TypeDefinition => {
                    snapshot
                        .analysis()
                        .goto_type_definition(target, context.file, offset)
                }
            };

            for target in targets {
                let Some(location) = navigation::location_for_target(snapshot.parse_db(), &target)?
                else {
                    continue;
                };
                if !locations.contains(&location) {
                    locations.push(location);
                }
            }
        }

        Ok(locations)
    }

    fn target_offsets(
        &self,
        snapshot: AnalysisSnapshot<'_>,
        path: &Path,
        position: ls_types::Position,
    ) -> anyhow::Result<Vec<(FileContext, TargetRef, u32)>> {
        let mut targets = Vec::new();

        for context in self.file_contexts(snapshot, path)? {
            let Some(offset) = self.offset_for_context(snapshot, &context, position) else {
                continue;
            };

            for target in &context.targets {
                targets.push((context.clone(), *target, offset));
            }
        }

        Ok(targets)
    }

    fn file_contexts(
        &self,
        snapshot: AnalysisSnapshot<'_>,
        path: &Path,
    ) -> anyhow::Result<Vec<FileContext>> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        snapshot.file_contexts_for_path(path)
    }

    fn offset_for_context(
        &self,
        snapshot: AnalysisSnapshot<'_>,
        context: &FileContext,
        position: ls_types::Position,
    ) -> Option<u32> {
        let package = snapshot.parse_db().package(context.package.0)?;
        let file = package.parsed_file(context.file)?;

        file.line_index()
            .offset_from_utf16_position(position::parse_position(position))
    }

    fn snapshot(&self) -> anyhow::Result<AnalysisSnapshot<'_>> {
        self.host
            .as_ref()
            .map(AnalysisHost::snapshot)
            .context("LSP engine is not initialized")
    }
}

#[derive(Debug, Clone, Copy)]
enum NavigationQuery {
    Definition,
    TypeDefinition,
}
