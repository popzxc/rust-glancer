//! Resolution and very small type inference for Body IR.
//!
//! The resolver consumes the already-lowered body store. It answers only cheap questions:
//! local-vs-item path resolution and simple types that are already present in signatures.

use rg_def_map::{DefId, DefMapDb, ModuleRef, PackageSlot, Path, TargetRef};
use rg_item_tree::{FieldKey, TypeRef};
use rg_parse::TargetId;
use rg_semantic_ir::{
    FieldRef, FunctionRef, SemanticIrDb, SemanticTypePathResolution, TypeDefRef, TypePathContext,
};

use super::{
    data::{
        BindingKind, BodyData, BodyIrDb, BodyResolution, BodyTy, BodyTypePathResolution, ExprKind,
        ResolvedFieldRef, ResolvedFunctionRef,
    },
    ids::{
        BindingId, BodyFieldRef, BodyFunctionRef, BodyId, BodyImplId, BodyItemId, BodyItemRef,
        BodyRef, ExprId, ScopeId,
    },
};

pub(super) fn resolve_bodies(db: &mut BodyIrDb, def_map: &DefMapDb, semantic_ir: &SemanticIrDb) {
    for (package_idx, package) in db.packages_mut().iter_mut().enumerate() {
        for (target_idx, target) in package.targets_mut().iter_mut().enumerate() {
            let target_ref = TargetRef {
                package: PackageSlot(package_idx),
                target: TargetId(target_idx),
            };

            for (body_idx, body) in target.bodies_mut().iter_mut().enumerate() {
                BodyResolver {
                    def_map,
                    semantic_ir,
                    body_ref: BodyRef {
                        target: target_ref,
                        body: BodyId(body_idx),
                    },
                    body,
                }
                .resolve();
            }
        }
    }
}

pub(super) fn resolve_type_path_in_scope(
    db: &BodyIrDb,
    def_map: &DefMapDb,
    semantic_ir: &SemanticIrDb,
    body_ref: BodyRef,
    scope: ScopeId,
    path: &Path,
) -> BodyTypePathResolution {
    let Some(body) = db.body_data(body_ref) else {
        return BodyTypePathResolution::Unknown;
    };

    BodyTypePathResolver {
        def_map,
        semantic_ir,
        body_ref,
        body,
    }
    .resolve_in_scope(scope, path)
}

pub(super) fn ty_for_field(
    def_map: &DefMapDb,
    semantic_ir: &SemanticIrDb,
    field_ref: FieldRef,
) -> Option<BodyTy> {
    let field_data = semantic_ir.field_data(field_ref)?;
    Some(ty_from_type_ref_in_context(
        def_map,
        semantic_ir,
        &field_data.field.ty,
        TypePathContext::module(field_data.owner_module),
        BodyTy::Unknown,
    ))
}

struct BodyResolver<'db, 'body> {
    def_map: &'db DefMapDb,
    semantic_ir: &'db SemanticIrDb,
    body_ref: BodyRef,
    body: &'body mut BodyData,
}

impl<'db, 'body> BodyResolver<'db, 'body> {
    fn type_path_resolver(&self) -> BodyTypePathResolver<'db, '_> {
        BodyTypePathResolver {
            def_map: self.def_map,
            semantic_ir: self.semantic_ir,
            body_ref: self.body_ref,
            body: self.body,
        }
    }

    fn resolve(&mut self) {
        self.resolve_bindings();
        self.resolve_local_impls();

        for expr_idx in 0..self.body.exprs.len() {
            self.resolve_expr(ExprId(expr_idx));
        }
    }

    fn resolve_bindings(&mut self) {
        for binding_idx in 0..self.body.bindings.len() {
            let binding = BindingId(binding_idx);
            let ty = self.binding_ty(binding);
            self.body.bindings[binding.0].ty = ty;
        }
    }

    fn binding_ty(&self, binding: BindingId) -> BodyTy {
        let binding_data = &self.body.bindings[binding.0];
        if let Some(annotation) = &binding_data.annotation {
            return self
                .type_path_resolver()
                .ty_from_type_ref_in_scope(annotation, binding_data.scope);
        }

        if matches!(binding_data.kind, BindingKind::SelfParam)
            && binding_data.name.as_deref() == Some("self")
        {
            let self_tys = self
                .type_path_resolver()
                .self_tys_for_function(self.body.owner);
            if !self_tys.is_empty() {
                return BodyTy::SelfTy(self_tys);
            }
        }

        BodyTy::Unknown
    }

    fn resolve_local_impls(&mut self) {
        for impl_idx in 0..self.body.local_impls.len() {
            let impl_id = BodyImplId(impl_idx);
            let self_item = {
                let impl_data = &self.body.local_impls[impl_idx];
                self.type_path_resolver()
                    .local_item_from_type_ref_in_scope(&impl_data.self_ty, impl_data.scope)
            };

            if let Some(impl_data) = self.body.local_impl_mut(impl_id) {
                impl_data.self_item = self_item;
            }
        }
    }

    fn resolve_expr(&mut self, expr: ExprId) {
        let kind = self.body.exprs[expr.0].kind.clone();

        match kind {
            ExprKind::Path { path } => {
                let (resolution, ty) = self.resolve_path_expr(expr, &path);
                let data = &mut self.body.exprs[expr.0];
                data.resolution = resolution;
                data.ty = ty;
            }
            ExprKind::Call { callee, .. } => {
                self.body.exprs[expr.0].ty = self.call_ty(callee);
            }
            ExprKind::Block { tail, .. } => {
                self.body.exprs[expr.0].ty = tail
                    .map(|tail| self.body.exprs[tail.0].ty.clone())
                    .unwrap_or(BodyTy::Unit);
            }
            ExprKind::Field { base, field, .. } => {
                let (resolution, ty) = self.resolve_field_expr(base, field.as_ref());
                let data = &mut self.body.exprs[expr.0];
                data.resolution = resolution;
                data.ty = ty;
            }
            ExprKind::MethodCall {
                receiver,
                method_name,
                ..
            } => {
                let (resolution, ty) = self.resolve_method_call_expr(expr, receiver, &method_name);
                let data = &mut self.body.exprs[expr.0];
                data.resolution = resolution;
                data.ty = ty;
            }
            ExprKind::Literal { .. } | ExprKind::Unknown { .. } => {}
        }
    }

    fn resolve_path_expr(&self, expr: ExprId, path: &Path) -> (BodyResolution, BodyTy) {
        let scope = self.body.exprs[expr.0].scope;
        if let Some(name) = path.single_name() {
            if let Some(binding) = self.resolve_local_name(scope, name, expr) {
                let ty = self.body.bindings[binding.0].ty.clone();
                return (BodyResolution::Local(binding), ty);
            }
        }

        match self.type_path_resolver().resolve_in_scope(scope, path) {
            BodyTypePathResolution::BodyLocal(item_ref) => {
                return (
                    BodyResolution::LocalItem(item_ref),
                    BodyTy::LocalNominal(vec![item_ref]),
                );
            }
            BodyTypePathResolution::SelfType(types) => {
                return (BodyResolution::Unknown, BodyTy::SelfTy(types));
            }
            BodyTypePathResolution::TypeDefs(_)
            | BodyTypePathResolution::Traits(_)
            | BodyTypePathResolution::Unknown => {}
        }

        let result = self.def_map.resolve_path(self.body.owner_module, path);
        let ty = self.nominal_ty_from_defs(&result.resolved);
        (BodyResolution::Item(result.resolved), ty)
    }

    fn resolve_field_expr(
        &self,
        base: Option<ExprId>,
        field: Option<&FieldKey>,
    ) -> (BodyResolution, BodyTy) {
        let (Some(base), Some(field)) = (base, field) else {
            return (BodyResolution::Unknown, BodyTy::Unknown);
        };

        let mut fields = Vec::new();
        let mut field_tys = Vec::new();

        for item_ref in self.body.exprs[base.0].ty.local_items() {
            let Some(field_ref) = self.local_field_for_type(*item_ref, field) else {
                continue;
            };
            push_unique(&mut fields, ResolvedFieldRef::BodyLocal(field_ref));

            let Some(item) = self.body.local_item(field_ref.item.item) else {
                continue;
            };
            let Some(field_data) = item.field(field_ref.index) else {
                continue;
            };
            let field_ty = self
                .type_path_resolver()
                .ty_from_type_ref_in_scope(&field_data.ty, item.scope);
            push_unique(&mut field_tys, field_ty);
        }

        for ty in self.body.exprs[base.0].ty.type_defs() {
            let Some(field_ref) = self.semantic_ir.field_for_type(*ty, field) else {
                continue;
            };
            push_unique(&mut fields, ResolvedFieldRef::Semantic(field_ref));

            let Some(field_data) = self.semantic_ir.field_data(field_ref) else {
                continue;
            };
            let field_ty = self.type_path_resolver().ty_from_type_ref_in_context(
                &field_data.field.ty,
                TypePathContext::module(field_data.owner_module),
            );
            push_unique(&mut field_tys, field_ty);
        }

        let resolution = if !fields.is_empty() {
            BodyResolution::Field(fields)
        } else {
            BodyResolution::Unknown
        };

        let ty = if field_tys.len() == 1 {
            field_tys.pop().expect("one field type should exist")
        } else {
            BodyTy::Unknown
        };

        (resolution, ty)
    }

    fn resolve_method_call_expr(
        &self,
        _expr: ExprId,
        receiver: Option<ExprId>,
        method_name: &str,
    ) -> (BodyResolution, BodyTy) {
        let Some(receiver) = receiver else {
            return (BodyResolution::Unknown, BodyTy::Unknown);
        };

        let mut functions = Vec::new();
        let mut return_tys = Vec::new();
        let receiver_ty = &self.body.exprs[receiver.0].ty;

        for item_ref in receiver_ty.local_items() {
            for function_ref in self.local_functions_for_type(*item_ref) {
                let Some(function_data) = self.body.local_function(function_ref.function) else {
                    continue;
                };
                if function_data.name != method_name || !function_data.has_self_receiver() {
                    continue;
                }

                push_unique(&mut functions, ResolvedFunctionRef::BodyLocal(function_ref));
                push_unique(&mut return_tys, self.local_function_return_ty(function_ref));
            }
        }

        for ty in receiver_ty.type_defs() {
            for function_ref in self.semantic_functions_for_type(*ty) {
                let Some(function_data) = self.semantic_ir.function_data(function_ref) else {
                    continue;
                };
                if function_data.name != method_name || !function_data.has_self_receiver() {
                    continue;
                }

                push_unique(&mut functions, ResolvedFunctionRef::Semantic(function_ref));
                push_unique(
                    &mut return_tys,
                    self.semantic_function_return_ty(function_ref),
                );
            }
        }

        let resolution = if functions.is_empty() {
            BodyResolution::Unknown
        } else {
            BodyResolution::Method(functions)
        };
        let ty = if return_tys.len() == 1 {
            return_tys.pop().expect("one return type should exist")
        } else {
            BodyTy::Unknown
        };

        (resolution, ty)
    }

    fn local_field_for_type(&self, item_ref: BodyItemRef, key: &FieldKey) -> Option<BodyFieldRef> {
        let body = if item_ref.body == self.body_ref {
            &*self.body
        } else {
            return None;
        };
        let item = body.local_item(item_ref.item)?;
        let index = item.field_index(key)?;

        Some(BodyFieldRef {
            item: item_ref,
            index,
        })
    }

    fn local_functions_for_type(&self, item_ref: BodyItemRef) -> Vec<BodyFunctionRef> {
        if item_ref.body != self.body_ref {
            return Vec::new();
        }

        self.body
            .inherent_functions_for_local_type(self.body_ref, item_ref)
    }

    fn semantic_functions_for_type(&self, ty: TypeDefRef) -> Vec<FunctionRef> {
        let mut functions = self.semantic_ir.inherent_functions_for_type(ty);
        for function in self.semantic_ir.trait_functions_for_type(ty) {
            push_unique(&mut functions, function);
        }
        functions
    }

    fn local_function_return_ty(&self, function_ref: BodyFunctionRef) -> BodyTy {
        let Some(function_data) = self.body.local_function(function_ref.function) else {
            return BodyTy::Unknown;
        };
        let Some(ret_ty) = &function_data.declaration.ret_ty else {
            return BodyTy::Unit;
        };

        match function_data.owner {
            super::data::BodyFunctionOwner::LocalImpl(impl_id) => {
                self.ty_from_type_ref_for_local_impl(ret_ty, impl_id)
            }
        }
    }

    fn semantic_function_return_ty(&self, function_ref: FunctionRef) -> BodyTy {
        let Some(function_data) = self.semantic_ir.function_data(function_ref) else {
            return BodyTy::Unknown;
        };
        let Some(ret_ty) = &function_data.declaration.ret_ty else {
            return BodyTy::Unit;
        };

        self.type_path_resolver()
            .ty_from_type_ref_for_function(ret_ty, function_ref)
    }

    fn ty_from_type_ref_for_local_impl(&self, ty: &TypeRef, impl_id: BodyImplId) -> BodyTy {
        let Some(impl_data) = self.body.local_impl(impl_id) else {
            return BodyTy::Unknown;
        };

        if let TypeRef::Path(type_path) = ty {
            let path = Path::from_type_path(type_path);
            if path.is_self_type() {
                return impl_data
                    .self_item
                    .map(|item| BodyTy::LocalNominal(vec![item]))
                    .unwrap_or(BodyTy::Unknown);
            }
        }

        self.type_path_resolver()
            .ty_from_type_ref_in_scope(ty, impl_data.scope)
    }

    fn resolve_local_name(
        &self,
        mut scope: ScopeId,
        name: &str,
        expr: ExprId,
    ) -> Option<BindingId> {
        loop {
            let scope_data = self.body.scope(scope)?;
            for binding in scope_data.bindings.iter().rev() {
                if binding.0 >= self.body.exprs[expr.0].visible_bindings {
                    continue;
                }

                let binding_data = self.body.binding(*binding)?;
                if binding_data.name.as_deref() == Some(name) {
                    return Some(*binding);
                }
            }

            scope = scope_data.parent?;
        }
    }

    fn call_ty(&self, callee: Option<ExprId>) -> BodyTy {
        let Some(callee) = callee else {
            return BodyTy::Unknown;
        };
        let callee_data = &self.body.exprs[callee.0];

        if let BodyTy::Nominal(types) = &callee_data.ty {
            return BodyTy::Nominal(types.clone());
        }

        if let BodyTy::SelfTy(types) = &callee_data.ty {
            return BodyTy::SelfTy(types.clone());
        }

        if let BodyTy::LocalNominal(items) = &callee_data.ty {
            return BodyTy::LocalNominal(items.clone());
        }

        let BodyResolution::Item(defs) = &callee_data.resolution else {
            return BodyTy::Unknown;
        };

        let mut return_tys = Vec::new();
        for def in defs {
            let Some(function_ref) = self.function_ref_for_def(*def) else {
                continue;
            };
            let Some(function_data) = self.semantic_ir.function_data(function_ref) else {
                continue;
            };
            let Some(ret_ty) = &function_data.declaration.ret_ty else {
                push_unique(&mut return_tys, BodyTy::Unit);
                continue;
            };

            let ty = self
                .type_path_resolver()
                .ty_from_type_ref_for_function(ret_ty, function_ref);
            push_unique(&mut return_tys, ty);
        }

        if return_tys.len() == 1 {
            return_tys.pop().expect("one return type should exist")
        } else {
            BodyTy::Unknown
        }
    }

    fn nominal_ty_from_defs(&self, defs: &[DefId]) -> BodyTy {
        let mut type_defs = Vec::new();
        for def in defs {
            let DefId::Local(local_def) = def else {
                continue;
            };
            let Some(type_def) = self.semantic_ir.type_def_for_local_def(*local_def) else {
                continue;
            };
            push_unique(&mut type_defs, type_def);
        }

        if type_defs.is_empty() {
            BodyTy::Unknown
        } else {
            BodyTy::Nominal(type_defs)
        }
    }

    fn function_ref_for_def(&self, def: DefId) -> Option<FunctionRef> {
        let DefId::Local(local_def) = def else {
            return None;
        };
        self.semantic_ir.function_for_local_def(local_def)
    }
}

struct BodyTypePathResolver<'db, 'body> {
    def_map: &'db DefMapDb,
    semantic_ir: &'db SemanticIrDb,
    body_ref: BodyRef,
    body: &'body BodyData,
}

impl BodyTypePathResolver<'_, '_> {
    fn resolve_in_scope(&self, scope: ScopeId, path: &Path) -> BodyTypePathResolution {
        if let Some(name) = path.single_name() {
            if let Some(item) = self.resolve_local_type_item(scope, name) {
                return BodyTypePathResolution::BodyLocal(BodyItemRef {
                    body: self.body_ref,
                    item,
                });
            }
        }

        self.resolve_in_context(
            self.context_for_function(self.body.owner, self.body.owner_module),
            path,
        )
    }

    fn resolve_in_context(&self, context: TypePathContext, path: &Path) -> BodyTypePathResolution {
        resolve_type_path_in_context(self.def_map, self.semantic_ir, context, path)
    }

    fn ty_from_type_ref_in_scope(&self, ty: &TypeRef, scope: ScopeId) -> BodyTy {
        match ty {
            TypeRef::Path(type_path) => {
                let path = Path::from_type_path(type_path);
                self.ty_from_body_resolution(
                    self.resolve_in_scope(scope, &path),
                    BodyTy::Syntax(ty.clone()),
                )
            }
            _ => self.ty_from_type_ref_in_context(
                ty,
                self.context_for_function(self.body.owner, self.body.owner_module),
            ),
        }
    }

    fn local_item_from_type_ref_in_scope(
        &self,
        ty: &TypeRef,
        scope: ScopeId,
    ) -> Option<BodyItemRef> {
        let TypeRef::Path(type_path) = ty else {
            return None;
        };
        let path = Path::from_type_path(type_path);
        match self.resolve_in_scope(scope, &path) {
            BodyTypePathResolution::BodyLocal(item) => Some(item),
            BodyTypePathResolution::SelfType(_)
            | BodyTypePathResolution::TypeDefs(_)
            | BodyTypePathResolution::Traits(_)
            | BodyTypePathResolution::Unknown => None,
        }
    }

    fn ty_from_type_ref_for_function(&self, ty: &TypeRef, function: FunctionRef) -> BodyTy {
        self.ty_from_type_ref_in_context(
            ty,
            self.context_for_function(function, self.body.owner_module),
        )
    }

    fn ty_from_type_ref_in_context(&self, ty: &TypeRef, context: TypePathContext) -> BodyTy {
        ty_from_type_ref_in_context(
            self.def_map,
            self.semantic_ir,
            ty,
            context,
            BodyTy::Syntax(ty.clone()),
        )
    }

    fn self_tys_for_function(&self, function: FunctionRef) -> Vec<TypeDefRef> {
        let Some(impl_ref) = self
            .context_for_function(function, self.body.owner_module)
            .impl_ref
        else {
            return Vec::new();
        };

        self.semantic_ir
            .impl_data(impl_ref)
            .map(|impl_data| impl_data.resolved_self_tys.clone())
            .unwrap_or_default()
    }

    fn context_for_function(
        &self,
        function: FunctionRef,
        fallback_module: ModuleRef,
    ) -> TypePathContext {
        self.semantic_ir
            .type_path_context_for_function(function)
            .unwrap_or_else(|| TypePathContext::module(fallback_module))
    }

    fn resolve_local_type_item(&self, mut scope: ScopeId, name: &str) -> Option<BodyItemId> {
        loop {
            let scope_data = self.body.scope(scope)?;
            for item in scope_data.local_items.iter().rev() {
                let item_data = self.body.local_item(*item)?;
                if item_data.name == name {
                    return Some(*item);
                }
            }

            scope = scope_data.parent?;
        }
    }

    fn ty_from_body_resolution(
        &self,
        resolution: BodyTypePathResolution,
        fallback: BodyTy,
    ) -> BodyTy {
        ty_from_body_resolution(resolution, fallback)
    }
}

fn resolve_type_path_in_context(
    def_map: &DefMapDb,
    semantic_ir: &SemanticIrDb,
    context: TypePathContext,
    path: &Path,
) -> BodyTypePathResolution {
    match semantic_ir.resolve_type_path(def_map, context, path) {
        SemanticTypePathResolution::SelfType(types) => BodyTypePathResolution::SelfType(types),
        SemanticTypePathResolution::TypeDefs(types) => BodyTypePathResolution::TypeDefs(types),
        SemanticTypePathResolution::Traits(traits) => BodyTypePathResolution::Traits(traits),
        SemanticTypePathResolution::Unknown => BodyTypePathResolution::Unknown,
    }
}

fn ty_from_type_ref_in_context(
    def_map: &DefMapDb,
    semantic_ir: &SemanticIrDb,
    ty: &TypeRef,
    context: TypePathContext,
    unresolved_path_fallback: BodyTy,
) -> BodyTy {
    match ty {
        TypeRef::Unit => BodyTy::Unit,
        TypeRef::Never => BodyTy::Never,
        TypeRef::Path(type_path) => {
            let path = Path::from_type_path(type_path);
            ty_from_body_resolution(
                resolve_type_path_in_context(def_map, semantic_ir, context, &path),
                unresolved_path_fallback,
            )
        }
        TypeRef::Unknown(_) | TypeRef::Infer => BodyTy::Unknown,
        TypeRef::Tuple(types) if types.is_empty() => BodyTy::Unit,
        _ => BodyTy::Syntax(ty.clone()),
    }
}

fn ty_from_body_resolution(resolution: BodyTypePathResolution, fallback: BodyTy) -> BodyTy {
    match resolution {
        BodyTypePathResolution::BodyLocal(item) => BodyTy::LocalNominal(vec![item]),
        BodyTypePathResolution::SelfType(types) => BodyTy::SelfTy(types),
        BodyTypePathResolution::TypeDefs(types) => BodyTy::Nominal(types),
        BodyTypePathResolution::Traits(_) => fallback,
        BodyTypePathResolution::Unknown => fallback,
    }
}

fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}
