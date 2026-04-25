// The analysis layer is the intended LSP-facing query surface, but the LSP consumer is not wired
// yet. Snapshot tests exercise it until that production entrypoint exists.
#![allow(dead_code)]

use crate::{
    Project,
    body_ir::{
        BindingId, BodyData, BodyId, BodyRef, BodyResolution, BodyTy, ExprData, ExprId, ExprKind,
    },
    def_map::{DefId, LocalDefRef, ModuleOrigin, ModuleRef, TargetRef},
    item_tree::ParamKind,
    parse::{FileId, span::Span},
    semantic_ir::{FunctionData, FunctionRef, TypeDefRef},
};

mod data;

#[cfg(test)]
mod tests;

use self::data::SourceNodeAt;
pub(crate) use self::data::{
    CompletionItem, CompletionKind, NavigationTarget, NavigationTargetKind, SymbolAt,
};

/// High-level query API over the frozen project analysis.
pub(crate) struct Analysis<'a> {
    project: &'a Project,
}

impl<'a> Analysis<'a> {
    pub(crate) fn new(project: &'a Project) -> Self {
        Self { project }
    }

    /// Returns the body symbol under a source offset, if Body IR owns that location.
    pub(crate) fn symbol_at(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Option<SymbolAt> {
        let node = self.source_node_at(target, file_id, offset)?;
        let body = self.body_data(node.body)?;
        Some(self.symbol_for_source_node(body, node))
    }

    /// Resolves a previously found body symbol to navigation targets.
    pub(crate) fn resolve_symbol(&self, symbol: SymbolAt) -> Vec<NavigationTarget> {
        match symbol {
            SymbolAt::Binding { body, binding } => self
                .body_data(body)
                .and_then(|body_data| body_data.binding(binding))
                .map(|binding_data| vec![NavigationTarget::from_binding(binding_data)])
                .unwrap_or_default(),
            SymbolAt::Expr { body, expr } => self
                .body_data(body)
                .and_then(|body_data| {
                    body_data.expr(expr).map(|expr_data| {
                        self.navigation_targets_for_resolution(body_data, &expr_data.resolution)
                    })
                })
                .unwrap_or_default(),
            SymbolAt::Body { .. } => Vec::new(),
        }
    }

    /// Returns best-effort definitions for the body symbol under a source offset.
    pub(crate) fn goto_definition(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Vec<NavigationTarget> {
        let Some(symbol) = self.symbol_at(target, file_id, offset) else {
            return Vec::new();
        };

        self.resolve_symbol(symbol)
    }

    /// Returns the best-effort Body IR type under a source offset.
    pub(crate) fn type_at(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Option<BodyTy> {
        match self.symbol_at(target, file_id, offset)? {
            SymbolAt::Expr { body, expr } => {
                self.body_data(body)?.expr(expr).map(|data| data.ty.clone())
            }
            SymbolAt::Binding { body, binding } => self
                .body_data(body)?
                .binding(binding)
                .map(|data| data.ty.clone()),
            SymbolAt::Body { .. } => None,
        }
    }

    /// Returns method-like completion candidates for a receiver before a dot.
    pub(crate) fn completions_at_dot(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Vec<CompletionItem> {
        let Some((body_ref, receiver)) = self.receiver_expr_at_dot(target, file_id, offset) else {
            return Vec::new();
        };
        let Some(receiver_data) = self
            .body_data(body_ref)
            .and_then(|body| body.expr(receiver))
        else {
            return Vec::new();
        };

        let mut completions = Vec::new();
        for ty in type_defs_from_body_ty(&receiver_data.ty) {
            self.push_type_function_completions(ty, &mut completions);
        }
        completions.sort_by(|left, right| {
            left.label
                .cmp(&right.label)
                .then(left.kind.cmp(&right.kind))
        });
        completions
    }

    fn source_node_at(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Option<SourceNodeAt> {
        let target_bodies = self.project.body_ir_db().target_bodies(target)?;
        let mut best = None;

        for (body_idx, body) in target_bodies.bodies().iter().enumerate() {
            if body.source.file_id != file_id || !contains_offset(body.source.span, offset) {
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
            });
        }

        best
    }

    fn smallest_expr_at(&self, body: &BodyData, file_id: FileId, offset: u32) -> Option<ExprId> {
        body.exprs
            .iter()
            .enumerate()
            .filter(|(_, expr)| expr.source.file_id == file_id)
            .filter(|(_, expr)| contains_offset_or_end(expr.source.span, offset))
            .min_by_key(|(_, expr)| span_len(expr.source.span))
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
            .filter(|(_, binding)| contains_offset_or_end(binding.source.span, offset))
            .min_by_key(|(_, binding)| span_len(binding.source.span))
            .map(|(idx, _)| BindingId(idx))
    }

    fn receiver_expr_at_dot(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Option<(BodyRef, ExprId)> {
        let target_bodies = self.project.body_ir_db().target_bodies(target)?;
        let mut best = None::<(BodyRef, ExprId, u32)>;

        for (body_idx, body) in target_bodies.bodies().iter().enumerate() {
            if body.source.file_id != file_id || !contains_offset(body.source.span, offset) {
                continue;
            }

            let body_ref = BodyRef {
                target,
                body: BodyId(body_idx),
            };
            for expr in &body.exprs {
                if expr.source.file_id != file_id || !offset_in_dot_expr(expr, body, offset) {
                    continue;
                }

                let Some(receiver) = receiver_expr(expr) else {
                    continue;
                };
                let len = span_len(expr.source.span);
                if best.is_none_or(|(_, _, best_len)| len < best_len) {
                    best = Some((body_ref, receiver, len));
                }
            }
        }

        best.map(|(body, receiver, _)| (body, receiver))
    }

    fn body_data(&self, body_ref: BodyRef) -> Option<&BodyData> {
        self.project.body_ir_db().body_data(body_ref)
    }

    fn symbol_for_source_node(&self, body: &BodyData, node: SourceNodeAt) -> SymbolAt {
        let expr = node.expr.and_then(|expr| {
            body.expr(expr)
                .map(|data| (expr, span_len(data.source.span)))
        });
        let binding = node.binding.and_then(|binding| {
            body.binding(binding)
                .map(|data| (binding, span_len(data.source.span)))
        });

        match (expr, binding) {
            (Some((expr, expr_len)), Some((_, binding_len))) if expr_len < binding_len => {
                SymbolAt::Expr {
                    body: node.body,
                    expr,
                }
            }
            (Some(_), Some((binding, _))) | (None, Some((binding, _))) => SymbolAt::Binding {
                body: node.body,
                binding,
            },
            (Some((expr, _)), None) => SymbolAt::Expr {
                body: node.body,
                expr,
            },
            (None, None) => SymbolAt::Body { body: node.body },
        }
    }

    fn navigation_targets_for_resolution(
        &self,
        body: &BodyData,
        resolution: &BodyResolution,
    ) -> Vec<NavigationTarget> {
        match resolution {
            BodyResolution::Local(binding) => body
                .binding(*binding)
                .map(NavigationTarget::from_binding)
                .into_iter()
                .collect(),
            BodyResolution::Item(defs) => defs
                .iter()
                .filter_map(|def| self.navigation_target_for_def(*def))
                .collect(),
            BodyResolution::Unknown => Vec::new(),
        }
    }

    fn navigation_target_for_def(&self, def: DefId) -> Option<NavigationTarget> {
        match def {
            DefId::Module(module_ref) => self.navigation_target_for_module(module_ref),
            DefId::Local(local_def) => self.navigation_target_for_local_def(local_def),
        }
    }

    fn navigation_target_for_module(&self, module_ref: ModuleRef) -> Option<NavigationTarget> {
        let module = self
            .project
            .def_map_db()
            .def_map(module_ref.target)?
            .module(module_ref.module)?;
        let (file_id, span) = match module.origin {
            ModuleOrigin::Root { file_id } => (file_id, None),
            ModuleOrigin::Inline {
                declaration_file,
                declaration_span,
            }
            | ModuleOrigin::OutOfLine {
                declaration_file,
                declaration_span,
                ..
            } => (declaration_file, Some(declaration_span)),
        };

        Some(NavigationTarget {
            kind: NavigationTargetKind::Module,
            name: module.name.clone().unwrap_or_else(|| "crate".to_string()),
            file_id,
            span,
        })
    }

    fn navigation_target_for_local_def(&self, local_def: LocalDefRef) -> Option<NavigationTarget> {
        let local_def_data = self
            .project
            .def_map_db()
            .def_map(local_def.target)?
            .local_defs()
            .get(local_def.local_def.0)?;

        Some(NavigationTarget {
            kind: NavigationTargetKind::from_local_def_kind(local_def_data.kind),
            name: local_def_data.name.clone(),
            file_id: local_def_data.file_id,
            span: Some(local_def_data.span),
        })
    }

    fn push_type_function_completions(
        &self,
        ty: TypeDefRef,
        completions: &mut Vec<CompletionItem>,
    ) {
        for function in self
            .project
            .semantic_ir_db()
            .inherent_functions_for_type(ty)
        {
            self.push_function_completion(function, CompletionKind::InherentMethod, completions);
        }

        for function in self.project.semantic_ir_db().trait_functions_for_type(ty) {
            self.push_function_completion(function, CompletionKind::TraitMethod, completions);
        }
    }

    fn push_function_completion(
        &self,
        function: FunctionRef,
        kind: CompletionKind,
        completions: &mut Vec<CompletionItem>,
    ) {
        let Some(data) = self.project.semantic_ir_db().function_data(function) else {
            return;
        };
        if !function_has_self_receiver(data) {
            return;
        }
        if completions
            .iter()
            .any(|completion| completion.function == function)
        {
            return;
        }

        completions.push(CompletionItem {
            label: data.name.clone(),
            kind,
            function,
        });
    }
}

fn type_defs_from_body_ty(ty: &BodyTy) -> Vec<TypeDefRef> {
    match ty {
        BodyTy::Nominal(types) | BodyTy::SelfTy(types) => types.clone(),
        BodyTy::Unit | BodyTy::Never | BodyTy::Syntax(_) | BodyTy::Unknown => Vec::new(),
    }
}

fn function_has_self_receiver(data: &FunctionData) -> bool {
    data.declaration
        .params
        .first()
        .is_some_and(|param| matches!(param.kind, ParamKind::SelfParam))
}

fn offset_in_dot_expr(expr: &ExprData, body: &BodyData, offset: u32) -> bool {
    let Some(receiver) = receiver_expr(expr) else {
        return false;
    };
    let Some(receiver_data) = body.expr(receiver) else {
        return false;
    };
    let Some(dot_span) = dot_span(expr) else {
        return false;
    };
    let completion_end = member_name_span(expr)
        .map(|span| span.text.end)
        .unwrap_or(expr.source.span.text.end);

    receiver_data.source.span.text.end <= dot_span.text.start
        && dot_span.text.end <= offset
        && offset <= completion_end
}

fn receiver_expr(expr: &ExprData) -> Option<ExprId> {
    match &expr.kind {
        ExprKind::MethodCall {
            receiver: Some(receiver),
            ..
        }
        | ExprKind::Field {
            base: Some(receiver),
            ..
        } => Some(*receiver),
        _ => None,
    }
}

fn member_name_span(expr: &ExprData) -> Option<Span> {
    match &expr.kind {
        ExprKind::MethodCall {
            method_name_span, ..
        } => *method_name_span,
        ExprKind::Field {
            field_name_span, ..
        } => *field_name_span,
        _ => None,
    }
}

fn dot_span(expr: &ExprData) -> Option<Span> {
    match &expr.kind {
        ExprKind::MethodCall { dot_span, .. } => *dot_span,
        ExprKind::Field { dot_span, .. } => *dot_span,
        _ => None,
    }
}

fn contains_offset(span: Span, offset: u32) -> bool {
    span.text.start <= offset && offset < span.text.end
}

fn contains_offset_or_end(span: Span, offset: u32) -> bool {
    span.text.start <= offset && offset <= span.text.end
}

fn span_len(span: Span) -> u32 {
    span.text.end.saturating_sub(span.text.start)
}
