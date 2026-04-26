// The analysis layer is the intended LSP-facing query surface, but the LSP consumer is not wired
// yet. Snapshot tests exercise it until that production entrypoint exists.
#![allow(dead_code)]

use crate::{
    body_ir::{BodyIrDb, BodyTy},
    def_map::{DefMapDb, TargetRef},
    item_tree::ItemTreeDb,
    parse::FileId,
    semantic_ir::SemanticIrDb,
};

mod completion;
mod cursor;
mod data;
mod navigation;
mod symbol;
mod ty;

#[cfg(test)]
mod tests;

pub(crate) use self::data::{CompletionItem, NavigationTarget, SymbolAt};
#[allow(unused_imports)]
pub(crate) use self::data::{CompletionKind, CompletionTarget, NavigationTargetKind};

/// High-level query API over the frozen phase databases.
pub(crate) struct Analysis<'a> {
    item_tree: &'a ItemTreeDb,
    def_map: &'a DefMapDb,
    semantic_ir: &'a SemanticIrDb,
    body_ir: &'a BodyIrDb,
}

impl<'a> Analysis<'a> {
    pub(crate) fn new(
        item_tree: &'a ItemTreeDb,
        def_map: &'a DefMapDb,
        semantic_ir: &'a SemanticIrDb,
        body_ir: &'a BodyIrDb,
    ) -> Self {
        Self {
            item_tree,
            def_map,
            semantic_ir,
            body_ir,
        }
    }

    /// Returns the smallest known symbol under a source offset.
    pub(crate) fn symbol_at(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Option<SymbolAt> {
        symbol::SymbolFinder::new(self).symbol_at(target, file_id, offset)
    }

    /// Resolves a previously found symbol to navigation targets.
    pub(crate) fn resolve_symbol(&self, symbol: SymbolAt) -> Vec<NavigationTarget> {
        navigation::SymbolResolver::new(self).resolve_symbol(symbol)
    }

    /// Returns best-effort definitions for the symbol under a source offset.
    pub(crate) fn goto_definition(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Vec<NavigationTarget> {
        navigation::GotoResolver::new(self).goto_definition(target, file_id, offset)
    }

    /// Returns the best-effort Body IR type under a source offset.
    pub(crate) fn type_at(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Option<BodyTy> {
        ty::TypeResolver::new(self).type_at(target, file_id, offset)
    }

    /// Returns field and method completion candidates for a receiver before a dot.
    pub(crate) fn completions_at_dot(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Vec<CompletionItem> {
        completion::CompletionResolver::new(self).completions_at_dot(target, file_id, offset)
    }
}
