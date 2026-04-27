//! Resolution and very small type inference for Body IR.
//!
//! The resolver consumes the already-lowered body store. It answers only cheap questions:
//! local-vs-item path resolution and simple types that are already present in signatures.

use rg_def_map::{DefId, DefMapDb, ModuleRef, PackageSlot, Path, TargetRef};
use rg_item_tree::{FieldKey, GenericArg, GenericParams, TypeRef};
use rg_parse::TargetId;
use rg_semantic_ir::{
    FieldRef, FunctionRef, ImplRef, ItemOwner, SemanticIrDb, SemanticTypePathResolution,
    TypeDefRef, TypePathContext,
};

use super::{
    BodyIrDb,
    body::{BodyData, TargetBodiesStatus},
    expr::ExprKind,
    ids::{
        BindingId, BodyFieldRef, BodyFunctionRef, BodyId, BodyImplId, BodyItemId, BodyItemRef,
        BodyRef, ExprId, ScopeId,
    },
    item::BodyFunctionOwner,
    resolved::{BodyResolution, BodyTypePathResolution, ResolvedFieldRef, ResolvedFunctionRef},
    stmt::BindingKind,
    ty::{BodyGenericArg, BodyLocalNominalTy, BodyNominalTy, BodyTy},
};

pub(super) fn resolve_bodies(db: &mut BodyIrDb, def_map: &DefMapDb, semantic_ir: &SemanticIrDb) {
    for (package_idx, package) in db.packages_mut().iter_mut().enumerate() {
        for (target_idx, target) in package.targets_mut().iter_mut().enumerate() {
            if matches!(target.status(), TargetBodiesStatus::Skipped) {
                continue;
            }

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
        &TypeSubst::new(),
    ))
}

pub(super) fn semantic_function_applies_to_receiver(
    def_map: &DefMapDb,
    semantic_ir: &SemanticIrDb,
    function_ref: FunctionRef,
    receiver_ty: &BodyNominalTy,
) -> bool {
    let Some(function_data) = semantic_ir.function_data(function_ref) else {
        return false;
    };
    let ItemOwner::Impl(impl_id) = function_data.owner else {
        return true;
    };
    let impl_ref = ImplRef {
        target: function_ref.target,
        id: impl_id,
    };
    let Some(impl_data) = semantic_ir.impl_data(impl_ref) else {
        return false;
    };
    if !impl_data.resolved_self_tys.contains(&receiver_ty.def) {
        return false;
    }

    impl_self_args_match_receiver(def_map, semantic_ir, impl_ref, impl_data, receiver_ty)
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
                return BodyTy::SelfTy(self_tys.into_iter().map(BodyNominalTy::bare).collect());
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
                    BodyTy::LocalNominal(vec![BodyLocalNominalTy::bare(item_ref)]),
                );
            }
            BodyTypePathResolution::SelfType(types) => {
                return (
                    BodyResolution::Unknown,
                    BodyTy::SelfTy(types.into_iter().map(BodyNominalTy::bare).collect()),
                );
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

        for local_ty in self.body.exprs[base.0].ty.local_nominals() {
            let Some(field_ref) = self.local_field_for_type(local_ty.item, field) else {
                continue;
            };
            push_unique(&mut fields, ResolvedFieldRef::BodyLocal(field_ref));

            let Some(item) = self.body.local_item(field_ref.item.item) else {
                continue;
            };
            let Some(field_data) = item.field(field_ref.index) else {
                continue;
            };
            let subst = self.local_type_subst(local_ty);
            let field_ty = self
                .type_path_resolver()
                .ty_from_type_ref_in_scope_with_subst(&field_data.ty, item.scope, &subst);
            push_unique(&mut field_tys, field_ty);
        }

        for nominal_ty in self.body.exprs[base.0].ty.nominal_tys() {
            let Some(field_ref) = self.semantic_ir.field_for_type(nominal_ty.def, field) else {
                continue;
            };
            push_unique(&mut fields, ResolvedFieldRef::Semantic(field_ref));

            let Some(field_data) = self.semantic_ir.field_data(field_ref) else {
                continue;
            };
            let subst = self.semantic_type_subst(nominal_ty);
            let field_ty = self
                .type_path_resolver()
                .ty_from_type_ref_in_context_with_subst(
                    &field_data.field.ty,
                    TypePathContext::module(field_data.owner_module),
                    &subst,
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

        for local_ty in receiver_ty.local_nominals() {
            for function_ref in self.local_functions_for_type(local_ty.item) {
                let Some(function_data) = self.body.local_function(function_ref.function) else {
                    continue;
                };
                if function_data.name != method_name || !function_data.has_self_receiver() {
                    continue;
                }

                push_unique(&mut functions, ResolvedFunctionRef::BodyLocal(function_ref));
                push_unique(
                    &mut return_tys,
                    self.local_function_return_ty(function_ref, Some(local_ty)),
                );
            }
        }

        for nominal_ty in receiver_ty.nominal_tys() {
            for function_ref in self.semantic_functions_for_type(nominal_ty) {
                let Some(function_data) = self.semantic_ir.function_data(function_ref) else {
                    continue;
                };
                if function_data.name != method_name || !function_data.has_self_receiver() {
                    continue;
                }

                push_unique(&mut functions, ResolvedFunctionRef::Semantic(function_ref));
                push_unique(
                    &mut return_tys,
                    self.semantic_function_return_ty(function_ref, Some(nominal_ty)),
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

    fn semantic_functions_for_type(&self, ty: &BodyNominalTy) -> Vec<FunctionRef> {
        let mut functions = self.semantic_ir.inherent_functions_for_type(ty.def);
        functions.retain(|function| {
            semantic_function_applies_to_receiver(self.def_map, self.semantic_ir, *function, ty)
        });

        for function in self.semantic_ir.trait_functions_for_type(ty.def) {
            push_unique(&mut functions, function);
        }
        functions
    }

    fn local_type_subst(&self, ty: &BodyLocalNominalTy) -> TypeSubst {
        let Some(item) = self.body.local_item(ty.item.item) else {
            return TypeSubst::new();
        };

        subst_from_generics(&item.generics, &ty.args)
    }

    fn semantic_type_subst(&self, ty: &BodyNominalTy) -> TypeSubst {
        let Some(generics) = self.semantic_ir.generic_params_for_type_def(ty.def) else {
            return TypeSubst::new();
        };

        subst_from_generics(generics, &ty.args)
    }

    fn semantic_impl_self_subst(
        &self,
        function_ref: FunctionRef,
        receiver_ty: &BodyNominalTy,
    ) -> TypeSubst {
        let Some(function_data) = self.semantic_ir.function_data(function_ref) else {
            return TypeSubst::new();
        };
        let ItemOwner::Impl(impl_id) = function_data.owner else {
            return TypeSubst::new();
        };
        let Some(impl_data) = self.semantic_ir.impl_data(ImplRef {
            target: function_ref.target,
            id: impl_id,
        }) else {
            return TypeSubst::new();
        };
        let TypeRef::Path(self_ty) = &impl_data.self_ty else {
            return TypeSubst::new();
        };
        let Some(segment) = self_ty.segments.last() else {
            return TypeSubst::new();
        };

        let impl_type_params = impl_type_param_names(&impl_data.generics);
        let receiver_type_args = receiver_ty
            .args
            .iter()
            .filter_map(body_generic_arg_ty)
            .collect::<Vec<_>>();

        segment
            .args
            .iter()
            .filter_map(generic_arg_type_ref)
            .zip(receiver_type_args)
            .filter_map(|(impl_arg, receiver_arg)| {
                let name = type_param_name_from_type_ref(impl_arg)?;
                impl_type_params
                    .contains(&name.as_str())
                    .then_some((name, receiver_arg))
            })
            .collect()
    }

    fn local_function_return_ty(
        &self,
        function_ref: BodyFunctionRef,
        receiver_ty: Option<&BodyLocalNominalTy>,
    ) -> BodyTy {
        let Some(function_data) = self.body.local_function(function_ref.function) else {
            return BodyTy::Unknown;
        };
        let Some(ret_ty) = &function_data.declaration.ret_ty else {
            return BodyTy::Unit;
        };

        match function_data.owner {
            BodyFunctionOwner::LocalImpl(impl_id) => {
                self.ty_from_type_ref_for_local_impl(ret_ty, impl_id, receiver_ty)
            }
        }
    }

    fn semantic_function_return_ty(
        &self,
        function_ref: FunctionRef,
        receiver_ty: Option<&BodyNominalTy>,
    ) -> BodyTy {
        let Some(function_data) = self.semantic_ir.function_data(function_ref) else {
            return BodyTy::Unknown;
        };
        let Some(ret_ty) = &function_data.declaration.ret_ty else {
            return BodyTy::Unit;
        };

        if receiver_ty.is_some() && type_ref_is_self(ret_ty) {
            return receiver_ty
                .cloned()
                .map(|ty| BodyTy::Nominal(vec![ty]))
                .unwrap_or(BodyTy::Unknown);
        }

        let subst = receiver_ty
            .map(|ty| {
                let mut subst = self.semantic_type_subst(ty);
                subst.extend(self.semantic_impl_self_subst(function_ref, ty));
                subst
            })
            .unwrap_or_default();
        self.type_path_resolver()
            .ty_from_type_ref_for_function_with_subst(ret_ty, function_ref, &subst)
    }

    fn ty_from_type_ref_for_local_impl(
        &self,
        ty: &TypeRef,
        impl_id: BodyImplId,
        receiver_ty: Option<&BodyLocalNominalTy>,
    ) -> BodyTy {
        let Some(impl_data) = self.body.local_impl(impl_id) else {
            return BodyTy::Unknown;
        };

        if let TypeRef::Path(type_path) = ty {
            let path = Path::from_type_path(type_path);
            if path.is_self_type() {
                if let Some(receiver_ty) = receiver_ty {
                    return BodyTy::LocalNominal(vec![receiver_ty.clone()]);
                }
                return impl_data
                    .self_item
                    .map(|item| BodyTy::LocalNominal(vec![BodyLocalNominalTy::bare(item)]))
                    .unwrap_or(BodyTy::Unknown);
            }
        }

        let subst = receiver_ty
            .map(|ty| self.local_type_subst(ty))
            .unwrap_or_default();
        self.type_path_resolver()
            .ty_from_type_ref_in_scope_with_subst(ty, impl_data.scope, &subst)
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

        if matches!(
            callee_data.ty,
            BodyTy::Nominal(_) | BodyTy::SelfTy(_) | BodyTy::LocalNominal(_)
        ) {
            return callee_data.ty.clone();
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
            BodyTy::Nominal(type_defs.into_iter().map(BodyNominalTy::bare).collect())
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

type TypeSubst = Vec<(String, BodyTy)>;

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
        self.ty_from_type_ref_in_scope_with_subst(ty, scope, &TypeSubst::new())
    }

    fn ty_from_type_ref_in_scope_with_subst(
        &self,
        ty: &TypeRef,
        scope: ScopeId,
        subst: &TypeSubst,
    ) -> BodyTy {
        match ty {
            TypeRef::Path(type_path) => {
                let path = Path::from_type_path(type_path);
                if let Some(ty) = substitute_type_param(&path, subst) {
                    return ty;
                }

                let args = self.generic_args_from_type_path_in_scope(type_path, scope, subst);
                self.ty_from_body_resolution_with_args(
                    self.resolve_in_scope(scope, &path),
                    BodyTy::Syntax(ty.clone()),
                    args,
                )
            }
            _ => self.ty_from_type_ref_in_context(
                ty,
                self.context_for_function(self.body.owner, self.body.owner_module),
                subst,
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
        self.ty_from_type_ref_for_function_with_subst(ty, function, &TypeSubst::new())
    }

    fn ty_from_type_ref_for_function_with_subst(
        &self,
        ty: &TypeRef,
        function: FunctionRef,
        subst: &TypeSubst,
    ) -> BodyTy {
        self.ty_from_type_ref_in_context_with_subst(
            ty,
            self.context_for_function(function, self.body.owner_module),
            subst,
        )
    }

    fn ty_from_type_ref_in_context(
        &self,
        ty: &TypeRef,
        context: TypePathContext,
        subst: &TypeSubst,
    ) -> BodyTy {
        self.ty_from_type_ref_in_context_with_subst(ty, context, subst)
    }

    fn ty_from_type_ref_in_context_with_subst(
        &self,
        ty: &TypeRef,
        context: TypePathContext,
        subst: &TypeSubst,
    ) -> BodyTy {
        ty_from_type_ref_in_context(
            self.def_map,
            self.semantic_ir,
            ty,
            context,
            BodyTy::Syntax(ty.clone()),
            subst,
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

    fn ty_from_body_resolution_with_args(
        &self,
        resolution: BodyTypePathResolution,
        fallback: BodyTy,
        args: Vec<BodyGenericArg>,
    ) -> BodyTy {
        ty_from_body_resolution(resolution, fallback, args)
    }

    fn generic_args_from_type_path_in_scope(
        &self,
        type_path: &rg_item_tree::TypePath,
        scope: ScopeId,
        subst: &TypeSubst,
    ) -> Vec<BodyGenericArg> {
        type_path
            .segments
            .last()
            .map(|segment| {
                self.generic_args_from_item_tree_args_in_scope(&segment.args, scope, subst)
            })
            .unwrap_or_default()
    }

    fn generic_args_from_item_tree_args_in_scope(
        &self,
        args: &[GenericArg],
        scope: ScopeId,
        subst: &TypeSubst,
    ) -> Vec<BodyGenericArg> {
        args.iter()
            .map(|arg| self.generic_arg_from_item_tree_arg_in_scope(arg, scope, subst))
            .collect()
    }

    fn generic_arg_from_item_tree_arg_in_scope(
        &self,
        arg: &GenericArg,
        scope: ScopeId,
        subst: &TypeSubst,
    ) -> BodyGenericArg {
        match arg {
            GenericArg::Type(ty) => BodyGenericArg::Type(Box::new(
                self.ty_from_type_ref_in_scope_with_subst(ty, scope, subst),
            )),
            GenericArg::Lifetime(lifetime) => BodyGenericArg::Lifetime(lifetime.clone()),
            GenericArg::Const(value) => BodyGenericArg::Const(value.clone()),
            GenericArg::AssocType { name, ty } => BodyGenericArg::AssocType {
                name: name.clone(),
                ty: ty.as_ref().map(|ty| {
                    Box::new(self.ty_from_type_ref_in_scope_with_subst(ty, scope, subst))
                }),
            },
            GenericArg::Unsupported(text) => BodyGenericArg::Unsupported(text.clone()),
        }
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
    subst: &TypeSubst,
) -> BodyTy {
    match ty {
        TypeRef::Unit => BodyTy::Unit,
        TypeRef::Never => BodyTy::Never,
        TypeRef::Path(type_path) => {
            let path = Path::from_type_path(type_path);
            if let Some(ty) = substitute_type_param(&path, subst) {
                return ty;
            }

            let args = generic_args_from_type_path_in_context(
                def_map,
                semantic_ir,
                type_path,
                context,
                subst,
            );
            ty_from_body_resolution(
                resolve_type_path_in_context(def_map, semantic_ir, context, &path),
                unresolved_path_fallback,
                args,
            )
        }
        TypeRef::Unknown(_) | TypeRef::Infer => BodyTy::Unknown,
        TypeRef::Tuple(types) if types.is_empty() => BodyTy::Unit,
        _ => BodyTy::Syntax(ty.clone()),
    }
}

fn ty_from_body_resolution(
    resolution: BodyTypePathResolution,
    fallback: BodyTy,
    args: Vec<BodyGenericArg>,
) -> BodyTy {
    match resolution {
        BodyTypePathResolution::BodyLocal(item) => {
            BodyTy::LocalNominal(vec![BodyLocalNominalTy { item, args }])
        }
        BodyTypePathResolution::SelfType(types) => BodyTy::SelfTy(
            types
                .into_iter()
                .map(|def| BodyNominalTy {
                    def,
                    args: args.clone(),
                })
                .collect(),
        ),
        BodyTypePathResolution::TypeDefs(types) => BodyTy::Nominal(
            types
                .into_iter()
                .map(|def| BodyNominalTy {
                    def,
                    args: args.clone(),
                })
                .collect(),
        ),
        BodyTypePathResolution::Traits(_) => fallback,
        BodyTypePathResolution::Unknown => fallback,
    }
}

fn impl_self_args_match_receiver(
    def_map: &DefMapDb,
    semantic_ir: &SemanticIrDb,
    impl_ref: ImplRef,
    impl_data: &rg_semantic_ir::ImplData,
    receiver_ty: &BodyNominalTy,
) -> bool {
    let TypeRef::Path(self_ty) = &impl_data.self_ty else {
        return true;
    };
    let Some(segment) = self_ty.segments.last() else {
        return true;
    };

    let impl_type_args = segment
        .args
        .iter()
        .filter_map(generic_arg_type_ref)
        .collect::<Vec<_>>();
    if impl_type_args.is_empty() {
        return true;
    }

    let receiver_type_args = receiver_ty
        .args
        .iter()
        .filter_map(body_generic_arg_ty)
        .collect::<Vec<_>>();
    if impl_type_args.len() != receiver_type_args.len() {
        return false;
    }

    let impl_type_params = impl_type_param_names(&impl_data.generics);
    for (impl_arg, receiver_arg) in impl_type_args.into_iter().zip(receiver_type_args) {
        if type_param_name_from_type_ref(impl_arg)
            .as_deref()
            .is_some_and(|name| impl_type_params.contains(&name))
        {
            continue;
        }

        let context = TypePathContext {
            module: impl_data.owner,
            impl_ref: Some(impl_ref),
        };
        let impl_arg_ty = ty_from_type_ref_in_context(
            def_map,
            semantic_ir,
            impl_arg,
            context,
            BodyTy::Syntax(impl_arg.clone()),
            &TypeSubst::new(),
        );
        if impl_arg_ty != receiver_arg {
            return false;
        }
    }

    true
}

fn subst_from_generics(generics: &GenericParams, args: &[BodyGenericArg]) -> TypeSubst {
    let type_args = args.iter().filter_map(body_generic_arg_ty);

    generics
        .types
        .iter()
        .zip(type_args)
        .map(|(param, ty)| (param.name.clone(), ty))
        .collect()
}

fn body_generic_arg_ty(arg: &BodyGenericArg) -> Option<BodyTy> {
    match arg {
        BodyGenericArg::Type(ty) => Some((**ty).clone()),
        BodyGenericArg::Lifetime(_)
        | BodyGenericArg::Const(_)
        | BodyGenericArg::AssocType { .. }
        | BodyGenericArg::Unsupported(_) => None,
    }
}

fn impl_type_param_names(generics: &GenericParams) -> Vec<&str> {
    generics
        .types
        .iter()
        .map(|param| param.name.as_str())
        .collect()
}

fn generic_arg_type_ref(arg: &GenericArg) -> Option<&TypeRef> {
    match arg {
        GenericArg::Type(ty) => Some(ty),
        GenericArg::Lifetime(_)
        | GenericArg::Const(_)
        | GenericArg::AssocType { .. }
        | GenericArg::Unsupported(_) => None,
    }
}

fn type_param_name_from_type_ref(ty: &TypeRef) -> Option<String> {
    let TypeRef::Path(path) = ty else {
        return None;
    };

    Path::from_type_path(path)
        .single_name()
        .map(ToString::to_string)
}

fn substitute_type_param(path: &Path, subst: &TypeSubst) -> Option<BodyTy> {
    let name = path.single_name()?;
    subst
        .iter()
        .rev()
        .find_map(|(param, ty)| (param == name).then(|| ty.clone()))
}

fn type_ref_is_self(ty: &TypeRef) -> bool {
    Path::from_type_ref(ty).is_some_and(|path| path.is_self_type())
}

fn generic_args_from_type_path_in_context(
    def_map: &DefMapDb,
    semantic_ir: &SemanticIrDb,
    type_path: &rg_item_tree::TypePath,
    context: TypePathContext,
    subst: &TypeSubst,
) -> Vec<BodyGenericArg> {
    type_path
        .segments
        .last()
        .map(|segment| {
            generic_args_from_item_tree_args_in_context(
                def_map,
                semantic_ir,
                &segment.args,
                context,
                subst,
            )
        })
        .unwrap_or_default()
}

fn generic_args_from_item_tree_args_in_context(
    def_map: &DefMapDb,
    semantic_ir: &SemanticIrDb,
    args: &[GenericArg],
    context: TypePathContext,
    subst: &TypeSubst,
) -> Vec<BodyGenericArg> {
    args.iter()
        .map(|arg| {
            generic_arg_from_item_tree_arg_in_context(def_map, semantic_ir, arg, context, subst)
        })
        .collect()
}

fn generic_arg_from_item_tree_arg_in_context(
    def_map: &DefMapDb,
    semantic_ir: &SemanticIrDb,
    arg: &GenericArg,
    context: TypePathContext,
    subst: &TypeSubst,
) -> BodyGenericArg {
    match arg {
        GenericArg::Type(ty) => BodyGenericArg::Type(Box::new(ty_from_type_ref_in_context(
            def_map,
            semantic_ir,
            ty,
            context,
            BodyTy::Syntax(ty.clone()),
            subst,
        ))),
        GenericArg::Lifetime(lifetime) => BodyGenericArg::Lifetime(lifetime.clone()),
        GenericArg::Const(value) => BodyGenericArg::Const(value.clone()),
        GenericArg::AssocType { name, ty } => BodyGenericArg::AssocType {
            name: name.clone(),
            ty: ty.as_ref().map(|ty| {
                Box::new(ty_from_type_ref_in_context(
                    def_map,
                    semantic_ir,
                    ty,
                    context,
                    BodyTy::Syntax(ty.clone()),
                    subst,
                ))
            }),
        },
        GenericArg::Unsupported(text) => BodyGenericArg::Unsupported(text.clone()),
    }
}

fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}
