//! Main body-resolution pass.
//!
//! This module walks lowered bodies and fills resolution/type slots on bindings and expressions.
//! Specialized helpers live in sibling modules so this file can read like the pass itself.

use rg_def_map::{DefId, DefMapDb, Path};
use rg_item_tree::{FieldKey, TypeRef};
use rg_semantic_ir::{FunctionRef, SemanticIrDb, TypePathContext};

use crate::{
    body::BodyData,
    expr::ExprKind,
    ids::{
        BindingId, BodyFieldRef, BodyFunctionRef, BodyImplId, BodyItemRef, BodyRef, ExprId, ScopeId,
    },
    item::BodyFunctionOwner,
    resolved::{BodyResolution, BodyTypePathResolution, ResolvedFieldRef, ResolvedFunctionRef},
    stmt::BindingKind,
    ty::{BodyLocalNominalTy, BodyNominalTy, BodyTy},
};

use super::{
    method::{
        local_function_applies_to_receiver, local_impl_self_subst,
        semantic_function_applies_to_receiver, semantic_impl_self_subst,
        semantic_trait_function_candidates_for_receiver,
    },
    pat::PatternTypePropagator,
    push_unique,
    ty::{TypeSubst, subst_from_generics, type_ref_is_self},
    type_path::BodyTypePathResolver,
};

pub(super) struct BodyResolver<'db, 'body> {
    def_map: &'db DefMapDb,
    semantic_ir: &'db SemanticIrDb,
    body_ref: BodyRef,
    body: &'body mut BodyData,
}

impl<'db, 'body> BodyResolver<'db, 'body> {
    pub(super) fn new(
        def_map: &'db DefMapDb,
        semantic_ir: &'db SemanticIrDb,
        body_ref: BodyRef,
        body: &'body mut BodyData,
    ) -> Self {
        Self {
            def_map,
            semantic_ir,
            body_ref,
            body,
        }
    }

    fn type_path_resolver(&self) -> BodyTypePathResolver<'db, '_> {
        BodyTypePathResolver::new(self.def_map, self.semantic_ir, self.body_ref, self.body)
    }

    pub(super) fn resolve(&mut self) {
        self.resolve_bindings();
        self.resolve_local_impls();

        // Pattern propagation can unlock later expression types, and those expressions can then
        // unlock more patterns. Every successful pass should discover at least one new binding or
        // expression fact, so a body-sized cap is enough to avoid a hidden magic constant.
        let max_passes = self.body.exprs.len() + self.body.bindings.len() + 1;
        for _ in 0..max_passes {
            let mut changed = false;
            for expr_idx in 0..self.body.exprs.len() {
                changed |= self.resolve_expr(ExprId(expr_idx));
            }
            changed |= PatternTypePropagator::new(
                self.def_map,
                self.semantic_ir,
                self.body_ref,
                self.body,
            )
            .propagate();

            if !changed {
                break;
            }
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
        // Local impls are lowered before their `Self` type is known. Resolve that link once so
        // method lookup can match directly by body-local item identity.
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

    fn resolve_expr(&mut self, expr: ExprId) -> bool {
        let old_resolution = self.body.exprs[expr.0].resolution.clone();
        let old_ty = self.body.exprs[expr.0].ty.clone();
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
            ExprKind::Match { arms, .. } => {
                let mut arm_tys = Vec::new();
                for arm in arms {
                    if let Some(expr) = arm.expr {
                        push_unique(&mut arm_tys, self.body.exprs[expr.0].ty.clone());
                    }
                }
                self.body.exprs[expr.0].ty = if arm_tys.len() == 1 {
                    arm_tys.pop().expect("one arm type should exist")
                } else {
                    BodyTy::Unknown
                };
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
                let (resolution, ty) = self.resolve_method_call_expr(receiver, &method_name);
                let data = &mut self.body.exprs[expr.0];
                data.resolution = resolution;
                data.ty = ty;
            }
            ExprKind::Literal { .. } | ExprKind::Unknown { .. } => {}
        }

        self.body.exprs[expr.0].resolution != old_resolution || self.body.exprs[expr.0].ty != old_ty
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

        // Local and semantic fields use the same substitution idea, but local items need their
        // declaration scope so field types can mention body-local names.
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
        receiver: Option<ExprId>,
        method_name: &str,
    ) -> (BodyResolution, BodyTy) {
        let Some(receiver) = receiver else {
            return (BodyResolution::Unknown, BodyTy::Unknown);
        };

        let mut functions = Vec::new();
        let mut return_tys = Vec::new();
        let receiver_ty = &self.body.exprs[receiver.0].ty;

        // Method lookup is intentionally shallow: exact local item identity for body-local impls,
        // and nominal type plus lightweight impl-argument matching for semantic impls.
        for local_ty in receiver_ty.local_nominals() {
            for function_ref in self.local_functions_for_type(local_ty) {
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

    fn local_functions_for_type(&self, ty: &BodyLocalNominalTy) -> Vec<BodyFunctionRef> {
        if ty.item.body != self.body_ref {
            return Vec::new();
        }

        let mut functions = self
            .body
            .inherent_functions_for_local_type(self.body_ref, ty.item);
        functions.retain(|function| {
            local_function_applies_to_receiver(
                self.def_map,
                self.semantic_ir,
                self.body_ref,
                self.body,
                *function,
                ty,
            )
        });
        functions
    }

    fn semantic_functions_for_type(&self, ty: &BodyNominalTy) -> Vec<FunctionRef> {
        let mut functions = self.semantic_ir.inherent_functions_for_type(ty.def);
        functions.retain(|function| {
            semantic_function_applies_to_receiver(self.def_map, self.semantic_ir, *function, ty)
        });

        for (function, _) in
            semantic_trait_function_candidates_for_receiver(self.def_map, self.semantic_ir, ty)
        {
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
                self.ty_from_type_ref_for_local_impl(ret_ty, impl_id, function_ref, receiver_ty)
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
                // Receiver type args and impl self args both contribute substitutions. For
                // `impl<U> Wrapper<U>`, this maps `U` to the known receiver argument.
                let mut subst = self.semantic_type_subst(ty);
                subst.extend(semantic_impl_self_subst(self.semantic_ir, function_ref, ty));
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
        function_ref: BodyFunctionRef,
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
            .map(|ty| {
                // Receiver type args and impl self args both contribute substitutions. For
                // `impl<U> Wrapper<U>`, this maps `U` to the known receiver argument.
                let mut subst = self.local_type_subst(ty);
                subst.extend(local_impl_self_subst(self.body, function_ref, ty));
                subst
            })
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

        // Ordinary calls use explicit return types only. Generic function inference remains
        // outside the current intentionally-small Body IR model.
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
