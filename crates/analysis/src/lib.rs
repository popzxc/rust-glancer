// The analysis layer is the intended LSP-facing query surface, but the LSP consumer is not wired
// yet. Snapshot tests exercise it until that production entrypoint exists.
#![allow(dead_code)]

use rg_body_ir::{BodyIrReadTxn, BodyTy};
use rg_def_map::{DefMapReadTxn, TargetRef};
use rg_parse::FileId;
use rg_semantic_ir::SemanticIrReadTxn;

mod completion;
mod cursor;
mod data;
mod entity;
mod hints;
mod hover;
mod navigation;
mod path_render;
mod signature;
mod symbol;
mod symbols;
mod txn;
mod ty;
mod type_render;

#[cfg(test)]
mod tests;

pub use self::data::{
    CompletionApplicability, CompletionItem, DocumentSymbol, HoverBlock, HoverInfo,
    NavigationTarget, SymbolAt, TypeHint, WorkspaceSymbol,
};
#[allow(unused_imports)]
pub use self::data::{CompletionKind, CompletionTarget, NavigationTargetKind, SymbolKind};
pub use self::txn::AnalysisReadTxn;

/// High-level query API over the frozen phase databases.
pub struct Analysis<'a> {
    def_map: DefMapReadTxn<'a>,
    semantic_ir: SemanticIrReadTxn<'a>,
    body_ir: BodyIrReadTxn<'a>,
}

impl<'a> Analysis<'a> {
    pub fn new(txn: &AnalysisReadTxn<'a>) -> Self {
        Self {
            def_map: txn.def_map().clone(),
            semantic_ir: txn.semantic_ir().clone(),
            body_ir: txn.body_ir().clone(),
        }
    }

    /// Returns the smallest known symbol under a source offset.
    pub fn symbol_at(&self, target: TargetRef, file_id: FileId, offset: u32) -> Option<SymbolAt> {
        symbol::SymbolFinder::new(self).symbol_at(target, file_id, offset)
    }

    /// Resolves a previously found symbol to navigation targets.
    pub fn resolve_symbol(&self, symbol: SymbolAt) -> Vec<NavigationTarget> {
        navigation::SymbolResolver::new(self).resolve_symbol(symbol)
    }

    /// Returns best-effort definitions for the symbol under a source offset.
    pub fn goto_definition(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Vec<NavigationTarget> {
        navigation::GotoResolver::new(self).goto_definition(target, file_id, offset)
    }

    /// Returns best-effort type definitions for the symbol under a source offset.
    pub fn goto_type_definition(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Vec<NavigationTarget> {
        navigation::TypeDefinitionResolver::new(self).goto_type_definition(target, file_id, offset)
    }

    /// Returns the best-effort Body IR type under a source offset.
    pub fn type_at(&self, target: TargetRef, file_id: FileId, offset: u32) -> Option<BodyTy> {
        ty::TypeResolver::new(self).type_at(target, file_id, offset)
    }

    /// Returns best-effort inferred type hints for local bindings in one file.
    pub fn type_hints(
        &self,
        target: TargetRef,
        file_id: FileId,
        range: Option<rg_parse::TextSpan>,
    ) -> Vec<TypeHint> {
        hints::TypeHintCollector::new(self).type_hints(target, file_id, range)
    }

    /// Returns best-effort hover information for the symbol under a source offset.
    pub fn hover(&self, target: TargetRef, file_id: FileId, offset: u32) -> Option<HoverInfo> {
        hover::HoverResolver::new(self).hover(target, file_id, offset)
    }

    /// Returns field and method completion candidates for a receiver before a dot.
    pub fn completions_at_dot(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Vec<CompletionItem> {
        completion::CompletionResolver::new(self).completions_at_dot(target, file_id, offset)
    }

    /// Returns a hierarchical outline for one file under the selected target context.
    pub fn document_symbols(&self, target: TargetRef, file_id: FileId) -> Vec<DocumentSymbol> {
        symbols::SymbolCollector::new(self).document_symbols(target, file_id)
    }

    /// Returns flat, best-effort symbols matching a case-insensitive workspace query.
    pub fn workspace_symbols(&self, query: &str) -> Vec<WorkspaceSymbol> {
        symbols::SymbolCollector::new(self).workspace_symbols(query)
    }
}
