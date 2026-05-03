use std::{
    path::{Path, PathBuf},
    sync::{Arc, mpsc::Receiver},
    time::Instant,
};

use anyhow::Context as _;
use rg_analysis::TypeHint;
use rg_def_map::TargetRef;
use rg_parse::TextSpan;
use rg_project::{
    AnalysisHost, AnalysisSnapshot, FileContext, ProjectBuildOptions, SavedFileChange,
};
use rg_workspace::{SysrootSources, WorkspaceMetadata};
use tower_lsp_server::ls_types;

use crate::{
    engine::command::EngineCommand,
    memory::{MemoryControl, MemoryReporter},
    project_stats::{ProjectStats, log_retained_memory},
    proto::{completion, hover, inlay_hint, navigation, position, symbols},
};

#[derive(Debug)]
pub(super) struct EngineWorker {
    host: Option<AnalysisHost>,
    memory_control: Arc<dyn MemoryControl>,
}

impl EngineWorker {
    pub(super) fn new(memory_control: Arc<dyn MemoryControl>) -> Self {
        Self {
            host: None,
            memory_control,
        }
    }

    pub(super) fn run(mut self, receiver: Receiver<EngineCommand>) {
        tracing::debug!("LSP engine worker started");

        while let Ok(command) = receiver.recv() {
            match command {
                EngineCommand::Initialize {
                    root,
                    build_options,
                    respond_to,
                } => {
                    tracing::trace!(root = %root.display(), "engine command started: initialize");
                    let _ = respond_to.send(self.initialize(root, build_options));
                }
                EngineCommand::DidSave {
                    path,
                    text,
                    respond_to,
                } => {
                    tracing::trace!(
                        path = %path.display(),
                        has_text = text.is_some(),
                        text_len = ?text.as_ref().map(String::len),
                        "engine command started: did_save"
                    );
                    let _ = respond_to.send(self.did_save(path, text));
                }
                EngineCommand::GotoDefinition {
                    path,
                    position,
                    respond_to,
                } => {
                    tracing::trace!(
                        path = %path.display(),
                        line = position.line,
                        character = position.character,
                        "engine command started: goto_definition"
                    );
                    let _ =
                        respond_to.send(self.query_request("goto_definition", || {
                            self.goto_definition(path, position)
                        }));
                }
                EngineCommand::GotoTypeDefinition {
                    path,
                    position,
                    respond_to,
                } => {
                    tracing::trace!(
                        path = %path.display(),
                        line = position.line,
                        character = position.character,
                        "engine command started: goto_type_definition"
                    );
                    let _ = respond_to.send(self.query_request("goto_type_definition", || {
                        self.goto_type_definition(path, position)
                    }));
                }
                EngineCommand::Hover {
                    path,
                    position,
                    respond_to,
                } => {
                    tracing::trace!(
                        path = %path.display(),
                        line = position.line,
                        character = position.character,
                        "engine command started: hover"
                    );
                    let _ =
                        respond_to.send(self.query_request("hover", || self.hover(path, position)));
                }
                EngineCommand::Completion {
                    path,
                    position,
                    respond_to,
                } => {
                    tracing::trace!(
                        path = %path.display(),
                        line = position.line,
                        character = position.character,
                        "engine command started: completion"
                    );
                    let _ = respond_to
                        .send(self.query_request("completion", || self.completion(path, position)));
                }
                EngineCommand::DocumentSymbol { path, respond_to } => {
                    tracing::trace!(
                        path = %path.display(),
                        "engine command started: document_symbol"
                    );
                    let _ = respond_to
                        .send(self.query_request("document_symbol", || self.document_symbol(path)));
                }
                EngineCommand::InlayHint {
                    path,
                    range,
                    respond_to,
                } => {
                    tracing::trace!(
                        path = %path.display(),
                        start_line = range.start.line,
                        start_character = range.start.character,
                        end_line = range.end.line,
                        end_character = range.end.character,
                        "engine command started: inlay_hint"
                    );
                    let _ = respond_to
                        .send(self.query_request("inlay_hint", || self.inlay_hint(path, range)));
                }
                EngineCommand::WorkspaceSymbol { query, respond_to } => {
                    tracing::trace!(query = %query, "engine command started: workspace_symbol");
                    let _ = respond_to.send(
                        self.query_request("workspace_symbol", || self.workspace_symbol(&query)),
                    );
                }
                EngineCommand::Shutdown(respond_to) => {
                    tracing::info!("shutting down LSP engine worker");
                    let _ = respond_to.send(Ok(()));
                    break;
                }
            }
        }

        tracing::debug!("LSP engine worker stopped");
    }

    fn initialize(
        &mut self,
        root: PathBuf,
        build_options: ProjectBuildOptions,
    ) -> anyhow::Result<()> {
        let started = Instant::now();
        tracing::info!(
            root = %root.display(),
            package_residency = build_options.package_residency_policy.config_name(),
            "starting workspace indexing"
        );

        let manifest_path = root.join("Cargo.toml");
        if !manifest_path.exists() {
            anyhow::bail!(
                "workspace root {} does not contain Cargo.toml",
                root.display()
            );
        }

        let metadata_started = Instant::now();
        let metadata = cargo_metadata::MetadataCommand::new()
            .manifest_path(&manifest_path)
            .exec()
            .context("while attempting to run cargo metadata for LSP initialization")?;
        tracing::info!(
            package_count = metadata.packages.len(),
            elapsed_ms = metadata_started.elapsed().as_millis(),
            "cargo metadata finished"
        );

        let workspace = WorkspaceMetadata::from_cargo(metadata)
            .context("while attempting to normalize Cargo metadata")?;
        let workspace_root = workspace.workspace_root().to_path_buf();
        let sysroot = SysrootSources::discover(workspace.workspace_root());
        match &sysroot {
            Some(sysroot) => {
                tracing::info!(
                    library_root = %sysroot.library_root().display(),
                    "sysroot sources discovered"
                );
            }
            None => {
                tracing::info!("sysroot sources unavailable");
            }
        }

        let workspace = workspace.with_sysroot_sources(sysroot);
        let host = AnalysisHost::build_with_options(workspace, build_options)
            .context("while attempting to build LSP analysis host")?;
        let snapshot = host.snapshot();
        Self::post_project_build(self.memory_control.as_ref(), snapshot, "initial index");

        self.host = Some(host);
        tracing::info!(
            workspace_root = %workspace_root.display(),
            elapsed_ms = started.elapsed().as_millis(),
            "workspace indexing finished"
        );

        Ok(())
    }

    fn did_save(&mut self, path: PathBuf, text: Option<String>) -> anyhow::Result<()> {
        let started = Instant::now();
        let memory_control = Arc::clone(&self.memory_control);
        let host = self
            .host
            .as_mut()
            .context("LSP engine is not initialized")?;

        tracing::info!(
            path = %path.display(),
            notification_includes_text = text.is_some(),
            "processing saved file"
        );

        let summary = host
            .apply_change(SavedFileChange::new(&path))
            .context("while attempting to apply saved file change")?;
        tracing::info!(
            path = %path.display(),
            changed_files = summary.changed_files.len(),
            affected_packages = summary.affected_packages.len(),
            changed_targets = summary.changed_targets.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "saved file reindex finished"
        );
        Self::post_project_build(memory_control.as_ref(), host.snapshot(), "after save");

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
        let started = Instant::now();
        let snapshot = self.snapshot()?;
        let txn = snapshot.read_txn()?;
        let analysis = snapshot.analysis(&txn);
        let mut completions = Vec::new();

        for (context, target, offset) in self.target_offsets(snapshot, &path, position)? {
            for item in analysis.completions_at_dot(target, context.file, offset) {
                let item = completion::completion_item(item);
                if !completions.contains(&item) {
                    completions.push(item);
                }
            }
        }

        tracing::debug!(
            path = %path.display(),
            line = position.line,
            character = position.character,
            result_count = completions.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "completion query finished"
        );

        Ok(completions)
    }

    fn hover(
        &self,
        path: PathBuf,
        position: ls_types::Position,
    ) -> anyhow::Result<Option<ls_types::Hover>> {
        let started = Instant::now();
        let snapshot = self.snapshot()?;
        let txn = snapshot.read_txn()?;
        let analysis = snapshot.analysis(&txn);

        for (context, target, offset) in self.target_offsets(snapshot, &path, position)? {
            let Some(info) = analysis.hover(target, context.file, offset) else {
                continue;
            };
            let Some(package) = snapshot.parse_db().package(context.package.0) else {
                continue;
            };
            let Some(file) = package.parsed_file(context.file) else {
                continue;
            };
            let Some(hover) = hover::hover(info, file.line_index()) else {
                continue;
            };
            tracing::debug!(
                path = %path.display(),
                line = position.line,
                character = position.character,
                has_hover = true,
                elapsed_ms = started.elapsed().as_millis(),
                "hover query finished"
            );
            return Ok(Some(hover));
        }

        tracing::debug!(
            path = %path.display(),
            line = position.line,
            character = position.character,
            has_hover = false,
            elapsed_ms = started.elapsed().as_millis(),
            "hover query finished"
        );
        Ok(None)
    }

    fn document_symbol(&self, path: PathBuf) -> anyhow::Result<Vec<ls_types::DocumentSymbol>> {
        let started = Instant::now();
        let snapshot = self.snapshot()?;
        let txn = snapshot.read_txn()?;
        let analysis = snapshot.analysis(&txn);
        let mut lsp_symbols = Vec::new();

        for context in self.file_contexts(snapshot, &path)? {
            for target in context.targets {
                let symbols = analysis.document_symbols(target, context.file);
                for symbol in symbols {
                    let symbol =
                        symbols::document_symbol(snapshot.parse_db(), context.package.0, symbol)?;
                    if !lsp_symbols.contains(&symbol) {
                        lsp_symbols.push(symbol);
                    }
                }
            }
        }

        tracing::debug!(
            path = %path.display(),
            result_count = lsp_symbols.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "document symbol query finished"
        );

        Ok(lsp_symbols)
    }

    fn inlay_hint(
        &self,
        path: PathBuf,
        range: ls_types::Range,
    ) -> anyhow::Result<Vec<ls_types::InlayHint>> {
        let started = Instant::now();
        let snapshot = self.snapshot()?;
        let txn = snapshot.read_txn()?;
        let analysis = snapshot.analysis(&txn);
        let mut hints = Vec::<(rg_def_map::PackageSlot, TypeHint)>::new();

        for context in self.file_contexts(snapshot, &path)? {
            let Some(range) = self.text_span_for_context(snapshot, &context, range) else {
                continue;
            };

            for target in context.targets {
                for hint in analysis.type_hints(target, context.file, Some(range)) {
                    if !hints
                        .iter()
                        .any(|(_, existing_hint)| existing_hint == &hint)
                    {
                        hints.push((context.package, hint));
                    }
                }
            }
        }

        let mut lsp_hints = Vec::new();
        for (package, hint) in hints {
            let Some(hint) = inlay_hint::type_hint(snapshot.parse_db(), package.0, hint)? else {
                continue;
            };
            lsp_hints.push(hint);
        }

        tracing::debug!(
            path = %path.display(),
            result_count = lsp_hints.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "inlay hint query finished"
        );

        Ok(lsp_hints)
    }

    fn workspace_symbol(&self, query: &str) -> anyhow::Result<Vec<ls_types::WorkspaceSymbol>> {
        let started = Instant::now();
        let snapshot = self.snapshot()?;
        let txn = snapshot.read_txn()?;
        let analysis = snapshot.analysis(&txn);
        let mut lsp_symbols = Vec::new();

        for symbol in analysis.workspace_symbols(query) {
            let Some(symbol) = symbols::workspace_symbol(snapshot.parse_db(), symbol)? else {
                continue;
            };
            if !lsp_symbols.contains(&symbol) {
                lsp_symbols.push(symbol);
            }
        }

        tracing::debug!(
            query,
            result_count = lsp_symbols.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "workspace symbol query finished"
        );

        Ok(lsp_symbols)
    }

    fn navigation_query(
        &self,
        path: PathBuf,
        position: ls_types::Position,
        query: NavigationQuery,
    ) -> anyhow::Result<Vec<ls_types::Location>> {
        let started = Instant::now();
        let snapshot = self.snapshot()?;
        let txn = snapshot.read_txn()?;
        let analysis = snapshot.analysis(&txn);
        let mut locations = Vec::new();

        for (context, target, offset) in self.target_offsets(snapshot, &path, position)? {
            let targets = match query {
                NavigationQuery::Definition => {
                    analysis.goto_definition(target, context.file, offset)
                }
                NavigationQuery::TypeDefinition => {
                    analysis.goto_type_definition(target, context.file, offset)
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

        tracing::debug!(
            query = query.name(),
            path = %path.display(),
            line = position.line,
            character = position.character,
            result_count = locations.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "navigation query finished"
        );

        Ok(locations)
    }

    fn target_offsets(
        &self,
        snapshot: AnalysisSnapshot<'_>,
        path: &Path,
        position: ls_types::Position,
    ) -> anyhow::Result<Vec<(FileContext, TargetRef, u32)>> {
        let mut targets = Vec::new();

        let contexts = self.file_contexts(snapshot, path)?;
        for context in contexts {
            let Some(offset) = self.offset_for_context(snapshot, &context, position) else {
                tracing::trace!(
                    path = %path.display(),
                    line = position.line,
                    character = position.character,
                    package = ?context.package,
                    file = ?context.file,
                    "could not convert LSP position to file offset"
                );
                continue;
            };

            for target in &context.targets {
                targets.push((context.clone(), *target, offset));
            }
        }

        tracing::trace!(
            path = %path.display(),
            line = position.line,
            character = position.character,
            target_offset_count = targets.len(),
            "resolved request target offsets"
        );

        Ok(targets)
    }

    fn file_contexts(
        &self,
        snapshot: AnalysisSnapshot<'_>,
        path: &Path,
    ) -> anyhow::Result<Vec<FileContext>> {
        if !path.exists() {
            tracing::debug!(path = %path.display(), "query path does not exist");
            return Ok(Vec::new());
        }

        let contexts = snapshot.file_contexts_for_path(path)?;
        let target_count = contexts
            .iter()
            .map(|context| context.targets.len())
            .sum::<usize>();
        tracing::debug!(
            path = %path.display(),
            context_count = contexts.len(),
            target_count,
            "resolved file contexts"
        );
        tracing::trace!(
            path = %path.display(),
            context_count = contexts.len(),
            target_count,
            "resolved file contexts for query"
        );

        Ok(contexts)
    }

    fn offset_for_context(
        &self,
        snapshot: AnalysisSnapshot<'_>,
        context: &FileContext,
        position: ls_types::Position,
    ) -> Option<u32> {
        let package = snapshot.parse_db().package(context.package.0)?;
        let file = package.parsed_file(context.file)?;

        let offset = file
            .line_index()
            .offset_from_utf16_position(position::parse_position(position));
        tracing::trace!(
            package = ?context.package,
            file = ?context.file,
            line = position.line,
            character = position.character,
            offset = ?offset,
            "converted LSP position to file offset"
        );
        offset
    }

    fn text_span_for_context(
        &self,
        snapshot: AnalysisSnapshot<'_>,
        context: &FileContext,
        range: ls_types::Range,
    ) -> Option<TextSpan> {
        let package = snapshot.parse_db().package(context.package.0)?;
        let file = package.parsed_file(context.file)?;
        let start = file
            .line_index()
            .offset_from_utf16_position(position::parse_position(range.start))?;
        let end = file
            .line_index()
            .offset_from_utf16_position(position::parse_position(range.end))?;

        let span = TextSpan { start, end };
        tracing::trace!(
            package = ?context.package,
            file = ?context.file,
            start_line = range.start.line,
            start_character = range.start.character,
            end_line = range.end.line,
            end_character = range.end.character,
            span_start = span.start,
            span_end = span.end,
            "converted LSP range to text span"
        );
        Some(span)
    }

    fn snapshot(&self) -> anyhow::Result<AnalysisSnapshot<'_>> {
        self.host
            .as_ref()
            .map(AnalysisHost::snapshot)
            .context("LSP engine is not initialized")
    }

    /// Runs a read-only request and cleans up memory that eager offloaded transactions may leave
    /// behind after they are dropped.
    fn query_request<T>(
        &self,
        label: &'static str,
        query: impl FnOnce() -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        MemoryReporter::report_op(self.memory_control.as_ref(), label, query)
    }

    /// Hook for activities to be run after the project (re-)build.
    fn post_project_build(
        memory_control: &dyn MemoryControl,
        snapshot: AnalysisSnapshot<'_>,
        label: &'static str,
    ) {
        // Indexing can temporarily materialize most of the project. Once the snapshot is ready for
        // editor queries, purge allocator caches and report memory separately from project shape.
        MemoryReporter::report_current(memory_control, label);
        ProjectStats::capture(snapshot).log_info(label);
        log_retained_memory(snapshot, label);
    }
}

#[derive(Debug, Clone, Copy)]
enum NavigationQuery {
    Definition,
    TypeDefinition,
}

impl NavigationQuery {
    fn name(self) -> &'static str {
        match self {
            Self::Definition => "definition",
            Self::TypeDefinition => "type_definition",
        }
    }
}
