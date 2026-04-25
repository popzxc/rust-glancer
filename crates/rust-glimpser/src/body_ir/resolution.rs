//! Resolution and very small type inference for Body IR.
//!
//! The resolver consumes the already-lowered body store. It answers only cheap questions:
//! local-vs-item path resolution and simple types that are already present in signatures.

use crate::{
    def_map::{DefId, DefMapDb, Path, PathSegment},
    item_tree::TypeRef,
    semantic_ir::{FunctionRef, ImplRef, ItemId, ItemOwner, SemanticIrDb, TraitRef, TypeDefRef},
};

use super::{
    data::{BindingKind, BodyData, BodyIrDb, BodyResolution, BodyTy, ExprKind},
    ids::{BindingId, ExprId, ScopeId},
};

pub(super) fn resolve_bodies(db: &mut BodyIrDb, def_map: &DefMapDb, semantic_ir: &SemanticIrDb) {
    for package in db.packages_mut() {
        for target in package.targets_mut() {
            for body in target.bodies_mut() {
                BodyResolver {
                    def_map,
                    semantic_ir,
                    body,
                }
                .resolve();
            }
        }
    }
}

struct BodyResolver<'db, 'body> {
    def_map: &'db DefMapDb,
    semantic_ir: &'db SemanticIrDb,
    body: &'body mut BodyData,
}

impl<'db, 'body> BodyResolver<'db, 'body> {
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
            return self.ty_from_type_ref(annotation);
        }

        if matches!(binding_data.kind, BindingKind::SelfParam)
            && binding_data.name.as_deref() == Some("self")
        {
            let self_tys = self.impl_self_tys_for_function(self.body.owner);
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
            ExprKind::Literal { .. }
            | ExprKind::MethodCall { .. }
            | ExprKind::Field { .. }
            | ExprKind::Unknown { .. } => {}
        }
    }

    fn resolve_path_expr(&self, expr: ExprId, path: &Path) -> (BodyResolution, BodyTy) {
        if let Some(name) = local_name(path) {
            if let Some(binding) =
                self.resolve_local_name(self.body.exprs[expr.0].scope, name, expr)
            {
                let ty = self.body.bindings[binding.0].ty.clone();
                return (BodyResolution::Local(binding), ty);
            }
        }

        if is_self_type_path(path) {
            let self_tys = self.impl_self_tys_for_function(self.body.owner);
            if !self_tys.is_empty() {
                return (BodyResolution::Unknown, BodyTy::SelfTy(self_tys));
            }
        }

        let result = self.def_map.resolve_path(self.body.owner_module, path);
        let ty = self.nominal_ty_from_defs(&result.resolved);
        (BodyResolution::Item(result.resolved), ty)
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

            let owner_module = self
                .owner_module_for_function(function_ref)
                .unwrap_or(self.body.owner_module);
            let ty = self.ty_from_type_ref_in_module(ret_ty, owner_module, function_ref);
            push_unique(&mut return_tys, ty);
        }

        if return_tys.len() == 1 {
            return_tys.pop().expect("one return type should exist")
        } else {
            BodyTy::Unknown
        }
    }

    fn ty_from_type_ref(&self, ty: &TypeRef) -> BodyTy {
        self.ty_from_type_ref_in_module(ty, self.body.owner_module, self.body.owner)
    }

    fn ty_from_type_ref_in_module(
        &self,
        ty: &TypeRef,
        owner_module: crate::def_map::ModuleRef,
        owner_function: FunctionRef,
    ) -> BodyTy {
        match ty {
            TypeRef::Unit => BodyTy::Unit,
            TypeRef::Never => BodyTy::Never,
            TypeRef::Path(path) if path.segments.len() == 1 && path.segments[0].name == "Self" => {
                let self_tys = self.impl_self_tys_for_function(owner_function);
                if self_tys.is_empty() {
                    BodyTy::Syntax(ty.clone())
                } else {
                    BodyTy::SelfTy(self_tys)
                }
            }
            TypeRef::Path(_) => {
                let Some(path) = path_from_type_ref(ty) else {
                    return BodyTy::Syntax(ty.clone());
                };
                let type_defs =
                    self.semantic_ir
                        .type_defs_for_path(self.def_map, owner_module, &path);
                if type_defs.is_empty() {
                    BodyTy::Syntax(ty.clone())
                } else {
                    BodyTy::Nominal(type_defs)
                }
            }
            TypeRef::Unknown(_) | TypeRef::Infer => BodyTy::Unknown,
            TypeRef::Tuple(types) if types.is_empty() => BodyTy::Unit,
            _ => BodyTy::Syntax(ty.clone()),
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
        let item = self
            .semantic_ir
            .target_ir(local_def.target)?
            .item_for_local_def(local_def.local_def)?;
        let ItemId::Function(id) = item else {
            return None;
        };

        Some(FunctionRef {
            target: local_def.target,
            id,
        })
    }

    fn owner_module_for_function(
        &self,
        function: FunctionRef,
    ) -> Option<crate::def_map::ModuleRef> {
        let function_data = self.semantic_ir.function_data(function)?;
        match function_data.owner {
            ItemOwner::Module(module_ref) => Some(module_ref),
            ItemOwner::Trait(trait_id) => self
                .semantic_ir
                .trait_data(TraitRef {
                    target: function.target,
                    id: trait_id,
                })
                .map(|data| data.owner),
            ItemOwner::Impl(impl_id) => self
                .semantic_ir
                .impl_data(ImplRef {
                    target: function.target,
                    id: impl_id,
                })
                .map(|data| data.owner),
        }
    }

    fn impl_self_tys_for_function(&self, function: FunctionRef) -> Vec<TypeDefRef> {
        let Some(function_data) = self.semantic_ir.function_data(function) else {
            return Vec::new();
        };
        let ItemOwner::Impl(impl_id) = function_data.owner else {
            return Vec::new();
        };

        self.semantic_ir
            .impl_data(ImplRef {
                target: function.target,
                id: impl_id,
            })
            .map(|impl_data| impl_data.resolved_self_tys.clone())
            .unwrap_or_default()
    }
}

fn local_name(path: &Path) -> Option<&str> {
    if path.absolute || path.segments.len() != 1 {
        return None;
    }

    match path.segments.first()? {
        PathSegment::Name(name) => Some(name),
        PathSegment::SelfKw => Some("self"),
        PathSegment::SuperKw | PathSegment::CrateKw => None,
    }
}

fn is_self_type_path(path: &Path) -> bool {
    !path.absolute
        && path.segments.len() == 1
        && matches!(path.segments.first(), Some(PathSegment::Name(name)) if name == "Self")
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

fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}
