use crate::{
    body_ir::{BindingId, BodyData, BodyId, BodyItemId, BodyItemRef, BodyRef, ExprId},
    def_map::TargetRef,
    parse::{FileId, span::Span},
};

use super::{
    Analysis, cursor,
    data::{SourceNodeAt, SymbolAt, SymbolCandidate},
};

pub(super) struct SymbolFinder<'a, 'project>(&'a Analysis<'project>);

impl<'a, 'project> SymbolFinder<'a, 'project> {
    pub(super) fn new(analysis: &'a Analysis<'project>) -> Self {
        Self(analysis)
    }

    pub(super) fn symbol_at(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Option<SymbolAt> {
        let mut candidates = Vec::new();
        candidates.extend(self.body_symbol_candidates(target, file_id, offset));
        candidates.extend(cursor::item_signature_candidates(
            self.0.project,
            target,
            file_id,
            offset,
        ));

        candidates
            .into_iter()
            .min_by_key(|candidate| candidate.span.len())
            .map(|candidate| candidate.symbol)
    }

    fn body_symbol_candidates(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Vec<SymbolCandidate> {
        let Some(node) = self.source_node_at(target, file_id, offset) else {
            return Vec::new();
        };
        let Some(body) = self.body_data(node.body) else {
            return Vec::new();
        };

        let mut candidates = Vec::new();
        let symbol = self.symbol_for_source_node(body, node);
        if let Some(span) = self.body_symbol_span(&symbol) {
            candidates.push(SymbolCandidate { symbol, span });
        }
        candidates.extend(cursor::body_type_path_candidates(
            node.body, body, file_id, offset,
        ));
        candidates
    }

    fn source_node_at(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Option<SourceNodeAt> {
        let target_bodies = self.0.project.body_ir_db().target_bodies(target)?;
        let mut best = None;

        for (body_idx, body) in target_bodies.bodies().iter().enumerate() {
            if body.source.file_id != file_id || !body.source.span.contains(offset) {
                continue;
            }

            let body_ref = BodyRef {
                target,
                body: BodyId(body_idx),
            };
            best = Some(SourceNodeAt {
                body: body_ref,
                expr: self.smallest_expr_at(body, file_id, offset),
                binding: self.smallest_binding_at(body, file_id, offset),
                local_item: self.smallest_local_item_at(body, file_id, offset),
            });
        }

        best
    }

    fn smallest_expr_at(&self, body: &BodyData, file_id: FileId, offset: u32) -> Option<ExprId> {
        body.exprs
            .iter()
            .enumerate()
            .filter(|(_, expr)| expr.source.file_id == file_id)
            .filter(|(_, expr)| expr.source.span.touches(offset))
            .min_by_key(|(_, expr)| expr.source.span.len())
            .map(|(idx, _)| ExprId(idx))
    }

    fn smallest_binding_at(
        &self,
        body: &BodyData,
        file_id: FileId,
        offset: u32,
    ) -> Option<BindingId> {
        body.bindings
            .iter()
            .enumerate()
            .filter(|(_, binding)| binding.source.file_id == file_id)
            .filter(|(_, binding)| binding.source.span.touches(offset))
            .min_by_key(|(_, binding)| binding.source.span.len())
            .map(|(idx, _)| BindingId(idx))
    }

    fn smallest_local_item_at(
        &self,
        body: &BodyData,
        file_id: FileId,
        offset: u32,
    ) -> Option<BodyItemId> {
        body.local_items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.name_source.file_id == file_id)
            .filter(|(_, item)| item.name_source.span.touches(offset))
            .min_by_key(|(_, item)| item.name_source.span.len())
            .map(|(idx, _)| BodyItemId(idx))
    }

    fn body_data(&self, body_ref: BodyRef) -> Option<&BodyData> {
        self.0.project.body_ir_db().body_data(body_ref)
    }

    fn symbol_for_source_node(&self, body: &BodyData, node: SourceNodeAt) -> SymbolAt {
        let mut candidates = Vec::new();
        if let Some(expr) = node.expr {
            if let Some(data) = body.expr(expr) {
                candidates.push((
                    data.source.span.len(),
                    SymbolAt::Expr {
                        body: node.body,
                        expr,
                    },
                ));
            }
        }
        if let Some(binding) = node.binding {
            if let Some(data) = body.binding(binding) {
                candidates.push((
                    data.source.span.len(),
                    SymbolAt::Binding {
                        body: node.body,
                        binding,
                    },
                ));
            }
        }
        if let Some(item) = node.local_item {
            if let Some(data) = body.local_item(item) {
                candidates.push((
                    data.name_source.span.len(),
                    SymbolAt::LocalItem {
                        item: BodyItemRef {
                            body: node.body,
                            item,
                        },
                        span: data.name_source.span,
                    },
                ));
            }
        }

        candidates
            .into_iter()
            .min_by_key(|(len, _)| *len)
            .map(|(_, symbol)| symbol)
            .unwrap_or(SymbolAt::Body { body: node.body })
    }

    fn body_symbol_span(&self, symbol: &SymbolAt) -> Option<Span> {
        match symbol {
            SymbolAt::Body { body } => self.body_data(*body).map(|data| data.source.span),
            SymbolAt::Binding { body, binding } => self
                .body_data(*body)?
                .binding(*binding)
                .map(|data| data.source.span),
            SymbolAt::Expr { body, expr } => self
                .body_data(*body)?
                .expr(*expr)
                .map(|data| data.source.span),
            SymbolAt::Def { span, .. }
            | SymbolAt::Field { span, .. }
            | SymbolAt::Function { span, .. }
            | SymbolAt::LocalItem { span, .. }
            | SymbolAt::BodyPath { span, .. }
            | SymbolAt::Path { span, .. } => Some(*span),
        }
    }
}
