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
    },
    ids::{BindingId, BodyId, BodyItemId, BodyItemRef, BodyRef, ExprId, ScopeId},
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
            ExprKind::Literal { .. } | ExprKind::MethodCall { .. } | ExprKind::Unknown { .. } => {}
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

        for ty in type_defs_from_body_ty(&self.body.exprs[base.0].ty) {
            let Some(field_ref) = self.semantic_ir.field_for_type(ty, field) else {
                continue;
            };
            push_unique(&mut fields, field_ref);

            let Some(field_data) = self.semantic_ir.field_data(field_ref) else {
                continue;
            };
            let field_ty = self.type_path_resolver().ty_from_type_ref_in_context(
                &field_data.field.ty,
                TypePathContext::module(field_data.owner_module),
            );
            push_unique(&mut field_tys, field_ty);
        }

        let resolution = if fields.is_empty() {
            BodyResolution::Unknown
        } else {
            BodyResolution::Field(fields)
        };

        let ty = if field_tys.len() == 1 {
            field_tys.pop().expect("one field type should exist")
        } else {
            BodyTy::Unknown
        };

        (resolution, ty)
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

fn type_defs_from_body_ty(ty: &BodyTy) -> Vec<TypeDefRef> {
    match ty {
        BodyTy::Nominal(types) | BodyTy::SelfTy(types) => types.clone(),
        BodyTy::Unit
        | BodyTy::Never
        | BodyTy::Syntax(_)
        | BodyTy::LocalNominal(_)
        | BodyTy::Unknown => Vec::new(),
    }
}

fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}
