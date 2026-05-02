//! Type-path resolution with body-local scope awareness.
//!
//! Semantic IR can resolve module items, but body-local structs live in lexical scopes. This
//! resolver checks those scopes first and then falls back to the semantic/def-map context.

use rg_def_map::{ModuleRef, Path};
use rg_item_tree::{GenericArg, TypeRef};
use rg_semantic_ir::{FunctionRef, SemanticTypePathResolution, TypeDefRef, TypePathContext};

use crate::{
    body::BodyData,
    ids::{BodyItemId, BodyItemRef, BodyRef, ScopeId},
    query::{DefMapQuery, SemanticIrQuery},
    resolved::BodyTypePathResolution,
    ty::{BodyGenericArg, BodyTy},
};

use super::ty::{
    TypeSubst, substitute_type_param, ty_from_body_resolution, ty_from_type_ref_in_context,
};

pub(super) struct BodyTypePathResolver<'db, 'body, D, S>
where
    D: DefMapQuery,
    S: SemanticIrQuery,
{
    def_map: &'db D,
    semantic_ir: &'db S,
    body_ref: BodyRef,
    body: &'body BodyData,
}

impl<'db, 'body, D, S> BodyTypePathResolver<'db, 'body, D, S>
where
    D: DefMapQuery,
    S: SemanticIrQuery,
{
    pub(super) fn new(
        def_map: &'db D,
        semantic_ir: &'db S,
        body_ref: BodyRef,
        body: &'body BodyData,
    ) -> Self {
        Self {
            def_map,
            semantic_ir,
            body_ref,
            body,
        }
    }

    pub(super) fn resolve_in_scope(&self, scope: ScopeId, path: &Path) -> BodyTypePathResolution {
        // Body-local type names shadow module items inside their lexical scope. Qualified paths
        // skip this branch because local items cannot be named through module paths.
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

    pub(super) fn ty_from_type_ref_in_scope(&self, ty: &TypeRef, scope: ScopeId) -> BodyTy {
        self.ty_from_type_ref_in_scope_with_subst(ty, scope, &TypeSubst::new())
    }

    pub(super) fn ty_from_type_ref_in_scope_with_subst(
        &self,
        ty: &TypeRef,
        scope: ScopeId,
        subst: &TypeSubst,
    ) -> BodyTy {
        // Path types are the only type syntax we resolve structurally today. Other forms stay as
        // syntax unless they have a cheap built-in representation such as `()` or `!`.
        match ty {
            TypeRef::Path(type_path) => {
                let path = Path::from_type_path(type_path);
                if let Some(ty) = substitute_type_param(&path, subst) {
                    return ty;
                }

                let args = self.generic_args_from_type_path_in_scope(type_path, scope, subst);
                ty_from_body_resolution(
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

    pub(super) fn local_item_from_type_ref_in_scope(
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

    pub(super) fn ty_from_type_ref_for_function_with_subst(
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

    pub(super) fn ty_from_type_ref_in_context_with_subst(
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

    pub(super) fn self_tys_for_function(&self, function: FunctionRef) -> Vec<TypeDefRef> {
        // `self` parameters and explicit `Self` annotations need the enclosing impl owner, not
        // just the owner module. Semantic IR owns that function-to-owner mapping.
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
        // Walk from the innermost scope outward so body-local items shadow outer declarations in
        // the same way as ordinary lexical bindings.
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

pub(super) fn resolve_type_path_in_context(
    def_map: &impl DefMapQuery,
    semantic_ir: &impl SemanticIrQuery,
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
