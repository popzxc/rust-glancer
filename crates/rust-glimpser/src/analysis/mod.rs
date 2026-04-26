// The analysis layer is the intended LSP-facing query surface, but the LSP consumer is not wired
// yet. Snapshot tests exercise it until that production entrypoint exists.
#![allow(dead_code)]

use crate::{
    Project,
    body_ir::{
        BindingId, BodyData, BodyId, BodyRef, BodyResolution, BodyTy, ExprData, ExprId, ExprKind,
    },
    def_map::{DefId, LocalDefRef, ModuleOrigin, ModuleRef, Path, PathSegment, TargetRef},
    item_tree::{ParamKind, TypeRef},
    parse::{FileId, span::Span},
    semantic_ir::{FieldRef, FunctionData, FunctionRef, ImplRef, TypeDefRef},
};

mod cursor;
mod data;

#[cfg(test)]
mod tests;

pub(crate) use self::data::{
    CompletionItem, CompletionKind, CompletionTarget, NavigationTarget, NavigationTargetKind,
    SymbolAt,
};
use self::data::{PathContext, PathRole, SourceNodeAt, SymbolCandidate};

/// High-level query API over the frozen project analysis.
pub(crate) struct Analysis<'a> {
    project: &'a Project,
}

impl<'a> Analysis<'a> {
    pub(crate) fn new(project: &'a Project) -> Self {
        Self { project }
    }

    /// Returns the smallest known symbol under a source offset.
    pub(crate) fn symbol_at(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Option<SymbolAt> {
        let mut candidates = Vec::new();
        if let Some(candidate) = self.body_symbol_candidate(target, file_id, offset) {
            candidates.push(candidate);
        }
        candidates.extend(cursor::item_signature_candidates(
            self.project,
            target,
            file_id,
            offset,
        ));

        candidates
            .into_iter()
            .min_by_key(|candidate| span_len(candidate.span))
            .map(|candidate| candidate.symbol)
    }

    /// Resolves a previously found symbol to navigation targets.
    pub(crate) fn resolve_symbol(&self, symbol: SymbolAt) -> Vec<NavigationTarget> {
        match symbol {
            SymbolAt::Binding { body, binding } => self
                .body_data(body)
                .and_then(|body_data| body_data.binding(binding))
                .map(|binding_data| vec![NavigationTarget::from_binding(binding_data)])
                .unwrap_or_default(),
            SymbolAt::Def { def, .. } => self.navigation_target_for_def(def).into_iter().collect(),
            SymbolAt::Expr { body, expr } => self
                .body_data(body)
                .and_then(|body_data| {
                    body_data.expr(expr).map(|expr_data| {
                        self.navigation_targets_for_resolution(body_data, &expr_data.resolution)
                    })
                })
                .unwrap_or_default(),
            SymbolAt::Field { field, .. } => self
                .navigation_target_for_field(field)
                .into_iter()
                .collect(),
            SymbolAt::Function { function, .. } => self
                .navigation_target_for_function(function)
                .into_iter()
                .collect(),
            SymbolAt::Path { context, path, .. } => {
                self.navigation_targets_for_path(context, &path)
            }
            SymbolAt::Body { .. } => Vec::new(),
        }
    }

    /// Returns best-effort definitions for the symbol under a source offset.
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
            SymbolAt::Def { def, .. } => self.ty_for_def(def),
            SymbolAt::Field { field, .. } => self.ty_for_field(field),
            SymbolAt::Path {
                context,
                path,
                role: PathRole::Type,
                ..
            } => Some(self.ty_for_type_path(context, &path)),
            SymbolAt::Path {
                role: PathRole::Use,
                ..
            }
            | SymbolAt::Function { .. } => None,
            SymbolAt::Body { .. } => None,
        }
    }

    /// Returns field and method completion candidates for a receiver before a dot.
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
            self.push_type_completions(ty, &mut completions);
        }
        completions.sort_by(|left, right| {
            left.label
                .cmp(&right.label)
                .then(left.kind.cmp(&right.kind))
        });
        completions
    }

    fn body_symbol_candidate(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Option<SymbolCandidate> {
        let node = self.source_node_at(target, file_id, offset)?;
        let body = self.body_data(node.body)?;
        let symbol = self.symbol_for_source_node(body, node);
        let span = self.body_symbol_span(&symbol)?;
        Some(SymbolCandidate { symbol, span })
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
            | SymbolAt::Path { span, .. } => Some(*span),
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
            BodyResolution::Field(fields) => fields
                .iter()
                .filter_map(|field| self.navigation_target_for_field(*field))
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

    fn navigation_target_for_field(&self, field_ref: FieldRef) -> Option<NavigationTarget> {
        let field_data = self.project.semantic_ir_db().field_data(field_ref)?;
        let key = field_data.field.key.as_ref()?;
        Some(NavigationTarget {
            kind: NavigationTargetKind::Field,
            name: key.declaration_label(),
            file_id: field_data.file_id,
            span: Some(field_data.field.span),
        })
    }

    fn navigation_target_for_function(
        &self,
        function_ref: FunctionRef,
    ) -> Option<NavigationTarget> {
        let function_data = self.project.semantic_ir_db().function_data(function_ref)?;
        let item = self
            .project
            .item_tree_db()
            .package(function_ref.target.package.0)?
            .item(function_data.source)?;

        Some(NavigationTarget {
            kind: NavigationTargetKind::Function,
            name: function_data.name.clone(),
            file_id: function_data.source.file_id,
            span: Some(item.span),
        })
    }

    fn navigation_targets_for_path(
        &self,
        context: PathContext,
        path: &Path,
    ) -> Vec<NavigationTarget> {
        if is_self_type_path(path) {
            if let Some(impl_ref) = context.impl_ref {
                return self
                    .impl_self_tys(impl_ref)
                    .into_iter()
                    .filter_map(|ty| self.navigation_target_for_type_def(ty))
                    .collect();
            }
        }

        self.project
            .def_map_db()
            .resolve_path(context.module, path)
            .resolved
            .into_iter()
            .filter_map(|def| self.navigation_target_for_def(def))
            .collect()
    }

    fn navigation_target_for_type_def(&self, ty: TypeDefRef) -> Option<NavigationTarget> {
        let target_ir = self.project.semantic_ir_db().target_ir(ty.target)?;
        let local_def = match ty.id {
            crate::semantic_ir::TypeDefId::Struct(id) => {
                target_ir.items().struct_data(id)?.local_def
            }
            crate::semantic_ir::TypeDefId::Enum(id) => target_ir.items().enum_data(id)?.local_def,
            crate::semantic_ir::TypeDefId::Union(id) => target_ir.items().union_data(id)?.local_def,
        };

        self.navigation_target_for_local_def(local_def)
    }

    fn ty_for_def(&self, def: DefId) -> Option<BodyTy> {
        let DefId::Local(local_def) = def else {
            return None;
        };
        self.project
            .semantic_ir_db()
            .type_def_for_local_def(local_def)
            .map(|ty| BodyTy::Nominal(vec![ty]))
    }

    fn ty_for_field(&self, field: FieldRef) -> Option<BodyTy> {
        let field_data = self.project.semantic_ir_db().field_data(field)?;
        Some(self.ty_from_type_ref_in_module(&field_data.field.ty, field_data.owner_module))
    }

    fn ty_for_type_path(&self, context: PathContext, path: &Path) -> BodyTy {
        if is_self_type_path(path) {
            if let Some(impl_ref) = context.impl_ref {
                let self_tys = self.impl_self_tys(impl_ref);
                return if self_tys.is_empty() {
                    BodyTy::Unknown
                } else {
                    BodyTy::SelfTy(self_tys)
                };
            }
        }

        let type_defs = self.project.semantic_ir_db().type_defs_for_path(
            self.project.def_map_db(),
            context.module,
            path,
        );
        if type_defs.is_empty() {
            BodyTy::Unknown
        } else {
            BodyTy::Nominal(type_defs)
        }
    }

    fn ty_from_type_ref_in_module(&self, ty: &TypeRef, owner_module: ModuleRef) -> BodyTy {
        match ty {
            TypeRef::Unit => BodyTy::Unit,
            TypeRef::Never => BodyTy::Never,
            TypeRef::Path(_) => {
                let Some(path) = path_from_type_ref(ty) else {
                    return BodyTy::Syntax(ty.clone());
                };
                self.ty_for_type_path(PathContext::module(owner_module), &path)
            }
            TypeRef::Unknown(_) | TypeRef::Infer => BodyTy::Unknown,
            TypeRef::Tuple(types) if types.is_empty() => BodyTy::Unit,
            _ => BodyTy::Syntax(ty.clone()),
        }
    }

    fn impl_self_tys(&self, impl_ref: ImplRef) -> Vec<TypeDefRef> {
        self.project
            .semantic_ir_db()
            .impl_data(impl_ref)
            .map(|data| data.resolved_self_tys.clone())
            .unwrap_or_default()
    }

    fn push_type_completions(&self, ty: TypeDefRef, completions: &mut Vec<CompletionItem>) {
        for field in self.project.semantic_ir_db().fields_for_type(ty) {
            self.push_field_completion(field, completions);
        }

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

    fn push_field_completion(&self, field: FieldRef, completions: &mut Vec<CompletionItem>) {
        let Some(data) = self.project.semantic_ir_db().field_data(field) else {
            return;
        };
        let Some(key) = data.field.key.as_ref() else {
            return;
        };
        let target = CompletionTarget::Field(field);
        if completions
            .iter()
            .any(|completion| completion.target == target)
        {
            return;
        }

        completions.push(CompletionItem {
            label: key.to_string(),
            kind: CompletionKind::Field,
            target,
        });
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
            .any(|completion| completion.target == CompletionTarget::Function(function))
        {
            return;
        }

        completions.push(CompletionItem {
            label: data.name.clone(),
            kind,
            target: CompletionTarget::Function(function),
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

fn path_from_type_ref(ty: &TypeRef) -> Option<Path> {
    let TypeRef::Path(path) = ty else {
        return None;
    };

    Some(Path {
        absolute: path.absolute,
        segments: path
            .segments
            .iter()
            .map(|segment| match segment.name.as_str() {
                "self" => PathSegment::SelfKw,
                "super" => PathSegment::SuperKw,
                "crate" => PathSegment::CrateKw,
                name => PathSegment::Name(name.to_string()),
            })
            .collect(),
    })
}

fn is_self_type_path(path: &Path) -> bool {
    !path.absolute
        && path.segments.len() == 1
        && matches!(path.segments.first(), Some(PathSegment::Name(name)) if name == "Self")
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
