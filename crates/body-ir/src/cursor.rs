//! Cursor-oriented queries over lowered function bodies.
//!
//! Analysis owns the public query vocabulary, but Body IR owns body source layout: expression
//! spans, binding spans, body-local item names, let annotations, and dot-completion receiver
//! ranges. Keeping those scans here makes the later crate boundary much less leaky.

use rg_def_map::{Path, TargetRef};
use rg_item_tree::{GenericArg, TypeBound, TypePath, TypeRef};
use rg_parse::{FileId, span::Span};

use crate::{
    BindingId, BodyData, BodyId, BodyIrDb, BodyItemId, BodyItemRef, BodyRef, BodyTy, ExprData,
    ExprId, ExprKind, ScopeId, StmtKind,
};

/// One body source node that can participate in cursor queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyCursorCandidate {
    Body {
        body: BodyRef,
        span: Span,
    },
    Binding {
        body: BodyRef,
        binding: BindingId,
        span: Span,
    },
    Expr {
        body: BodyRef,
        expr: ExprId,
        span: Span,
    },
    LocalItem {
        item: BodyItemRef,
        span: Span,
    },
    TypePath {
        body: BodyRef,
        scope: ScopeId,
        path: Path,
        span: Span,
    },
}

impl BodyCursorCandidate {
    pub fn span(&self) -> Span {
        match self {
            Self::Body { span, .. }
            | Self::Binding { span, .. }
            | Self::Expr { span, .. }
            | Self::LocalItem { span, .. }
            | Self::TypePath { span, .. } => *span,
        }
    }
}

impl BodyIrDb {
    /// Returns body-local cursor candidates at `offset`, including let-annotation type paths.
    pub fn cursor_candidates(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Vec<BodyCursorCandidate> {
        let Some(source_node) = self.source_node_at(target, file_id, offset) else {
            return Vec::new();
        };
        let Some(body) = self.body_data(source_node.body) else {
            return Vec::new();
        };

        let mut candidates = Vec::new();
        candidates.push(Self::candidate_for_source_node(body, source_node));
        TypePathCursorScanner {
            body_ref: source_node.body,
            body,
            file_id,
            offset,
            candidates: &mut candidates,
        }
        .scan();

        candidates
    }

    /// Returns the inferred receiver type for a dot-completion site.
    pub fn receiver_ty_at_dot(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Option<&BodyTy> {
        let (body_ref, receiver) = self.receiver_expr_at_dot(target, file_id, offset)?;
        self.body_data(body_ref)?
            .expr(receiver)
            .map(|expr| &expr.ty)
    }

    fn source_node_at(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Option<SourceNodeAt> {
        let target_bodies = self.target_bodies(target)?;
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
                expr: Self::smallest_expr_at(body, file_id, offset),
                binding: Self::smallest_binding_at(body, file_id, offset),
                local_item: Self::smallest_local_item_at(body, file_id, offset),
            });
        }

        best
    }

    fn smallest_expr_at(body: &BodyData, file_id: FileId, offset: u32) -> Option<ExprId> {
        body.exprs
            .iter()
            .enumerate()
            .filter(|(_, expr)| expr.source.file_id == file_id)
            .filter(|(_, expr)| expr.source.span.touches(offset))
            .min_by_key(|(_, expr)| expr.source.span.len())
            .map(|(idx, _)| ExprId(idx))
    }

    fn smallest_binding_at(body: &BodyData, file_id: FileId, offset: u32) -> Option<BindingId> {
        body.bindings
            .iter()
            .enumerate()
            .filter(|(_, binding)| binding.source.file_id == file_id)
            .filter(|(_, binding)| binding.source.span.touches(offset))
            .min_by_key(|(_, binding)| binding.source.span.len())
            .map(|(idx, _)| BindingId(idx))
    }

    fn smallest_local_item_at(body: &BodyData, file_id: FileId, offset: u32) -> Option<BodyItemId> {
        body.local_items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.name_source.file_id == file_id)
            .filter(|(_, item)| item.name_source.span.touches(offset))
            .min_by_key(|(_, item)| item.name_source.span.len())
            .map(|(idx, _)| BodyItemId(idx))
    }

    fn candidate_for_source_node(
        body: &BodyData,
        source_node: SourceNodeAt,
    ) -> BodyCursorCandidate {
        let mut candidates = Vec::new();
        if let Some(expr) = source_node.expr {
            if let Some(data) = body.expr(expr) {
                candidates.push((
                    data.source.span.len(),
                    BodyCursorCandidate::Expr {
                        body: source_node.body,
                        expr,
                        span: data.source.span,
                    },
                ));
            }
        }
        if let Some(binding) = source_node.binding {
            if let Some(data) = body.binding(binding) {
                candidates.push((
                    data.source.span.len(),
                    BodyCursorCandidate::Binding {
                        body: source_node.body,
                        binding,
                        span: data.source.span,
                    },
                ));
            }
        }
        if let Some(item) = source_node.local_item {
            if let Some(data) = body.local_item(item) {
                candidates.push((
                    data.name_source.span.len(),
                    BodyCursorCandidate::LocalItem {
                        item: BodyItemRef {
                            body: source_node.body,
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
            .map(|(_, candidate)| candidate)
            .unwrap_or(BodyCursorCandidate::Body {
                body: source_node.body,
                span: body.source.span,
            })
    }

    fn receiver_expr_at_dot(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Option<(BodyRef, ExprId)> {
        let target_bodies = self.target_bodies(target)?;
        let mut best = None::<(BodyRef, ExprId, u32)>;

        for (body_idx, body) in target_bodies.bodies().iter().enumerate() {
            if body.source.file_id != file_id || !body.source.span.contains(offset) {
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
                let len = expr.source.span.len();
                if best.is_none_or(|(_, _, best_len)| len < best_len) {
                    best = Some((body_ref, receiver, len));
                }
            }
        }

        best.map(|(body, receiver, _)| (body, receiver))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceNodeAt {
    body: BodyRef,
    expr: Option<ExprId>,
    binding: Option<BindingId>,
    local_item: Option<BodyItemId>,
}

struct TypePathCursorScanner<'a> {
    body_ref: BodyRef,
    body: &'a BodyData,
    file_id: FileId,
    offset: u32,
    candidates: &'a mut Vec<BodyCursorCandidate>,
}

impl TypePathCursorScanner<'_> {
    fn scan(&mut self) {
        for statement in &self.body.statements {
            if statement.source.file_id != self.file_id {
                continue;
            }
            let StmtKind::Let {
                scope,
                annotation: Some(annotation),
                ..
            } = &statement.kind
            else {
                continue;
            };
            self.scan_type_ref(*scope, annotation);
        }
    }

    fn scan_type_ref(&mut self, scope: ScopeId, ty: &TypeRef) {
        match ty {
            TypeRef::Path(path) => self.scan_type_path(scope, path),
            TypeRef::Tuple(types) => {
                for ty in types {
                    self.scan_type_ref(scope, ty);
                }
            }
            TypeRef::Reference { inner, .. }
            | TypeRef::RawPointer { inner, .. }
            | TypeRef::Slice(inner) => self.scan_type_ref(scope, inner),
            TypeRef::Array { inner, .. } => self.scan_type_ref(scope, inner),
            TypeRef::FnPointer { params, ret } => {
                for param in params {
                    self.scan_type_ref(scope, param);
                }
                self.scan_type_ref(scope, ret);
            }
            TypeRef::ImplTrait(bounds) | TypeRef::DynTrait(bounds) => {
                for bound in bounds {
                    if let TypeBound::Trait(ty) = bound {
                        self.scan_type_ref(scope, ty);
                    }
                }
            }
            TypeRef::Unknown(_) | TypeRef::Never | TypeRef::Unit | TypeRef::Infer => {}
        }
    }

    fn scan_type_path(&mut self, scope: ScopeId, path: &TypePath) {
        for (idx, segment) in path.segments.iter().enumerate() {
            if segment.span.touches(self.offset) {
                self.candidates.push(BodyCursorCandidate::TypePath {
                    body: self.body_ref,
                    scope,
                    path: Path::from_type_path_prefix(path, idx),
                    span: segment.span,
                });
            }

            for arg in &segment.args {
                self.scan_generic_arg(scope, arg);
            }
        }
    }

    fn scan_generic_arg(&mut self, scope: ScopeId, arg: &GenericArg) {
        match arg {
            GenericArg::Type(ty) => self.scan_type_ref(scope, ty),
            GenericArg::AssocType { ty: Some(ty), .. } => self.scan_type_ref(scope, ty),
            GenericArg::Lifetime(_)
            | GenericArg::Const(_)
            | GenericArg::AssocType { ty: None, .. }
            | GenericArg::Unsupported(_) => {}
        }
    }
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
        ExprKind::Field { field_span, .. } => *field_span,
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
