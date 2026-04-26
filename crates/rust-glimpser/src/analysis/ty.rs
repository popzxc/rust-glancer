use crate::{
    body_ir::{BodyData, BodyItemRef, BodyRef, BodyTy, ScopeId},
    def_map::{DefId, ModuleRef, Path},
    item_tree::TypeRef,
    semantic_ir::{FieldRef, ImplRef, ItemOwner, TypeDefRef},
};

use super::{
    Analysis,
    data::{PathContext, PathRole, SymbolAt},
};

pub(super) struct TypeResolver<'a, 'project>(&'a Analysis<'project>);

impl<'a, 'project> TypeResolver<'a, 'project> {
    pub(super) fn new(analysis: &'a Analysis<'project>) -> Self {
        Self(analysis)
    }

    pub(super) fn type_at(
        &self,
        target: crate::def_map::TargetRef,
        file_id: crate::parse::FileId,
        offset: u32,
    ) -> Option<BodyTy> {
        match self.0.symbol_at(target, file_id, offset)? {
            SymbolAt::Expr { body, expr } => self
                .0
                .project
                .body_ir_db()
                .body_data(body)?
                .expr(expr)
                .map(|data| data.ty.clone()),
            SymbolAt::Binding { body, binding } => self
                .0
                .project
                .body_ir_db()
                .body_data(body)?
                .binding(binding)
                .map(|data| data.ty.clone()),
            SymbolAt::BodyPath {
                body,
                scope,
                path,
                role: PathRole::Type,
                ..
            } => Some(self.ty_for_body_type_path(body, scope, &path)),
            SymbolAt::BodyPath {
                role: PathRole::Use,
                ..
            } => None,
            SymbolAt::Def { def, .. } => self.ty_for_def(def),
            SymbolAt::Field { field, .. } => self.ty_for_field(field),
            SymbolAt::LocalItem { item, .. } => Some(BodyTy::LocalNominal(vec![item])),
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

    pub(super) fn ty_for_type_path(&self, context: PathContext, path: &Path) -> BodyTy {
        if path.is_self_type() {
            if let Some(impl_ref) = context.impl_ref {
                let self_tys = self.impl_self_tys(impl_ref);
                return if self_tys.is_empty() {
                    BodyTy::Unknown
                } else {
                    BodyTy::SelfTy(self_tys)
                };
            }
        }

        let type_defs = self.0.project.semantic_ir_db().type_defs_for_path(
            self.0.project.def_map_db(),
            context.module,
            path,
        );
        if type_defs.is_empty() {
            BodyTy::Unknown
        } else {
            BodyTy::Nominal(type_defs)
        }
    }

    pub(super) fn ty_for_body_type_path(
        &self,
        body_ref: BodyRef,
        scope: ScopeId,
        path: &Path,
    ) -> BodyTy {
        if let Some(item) = self.resolve_body_local_type_item(body_ref, scope, path) {
            return BodyTy::LocalNominal(vec![item]);
        }

        let Some(body) = self.0.project.body_ir_db().body_data(body_ref) else {
            return BodyTy::Unknown;
        };

        self.ty_for_type_path(self.path_context_for_body(body), path)
    }

    pub(super) fn ty_from_type_ref_in_module(
        &self,
        ty: &TypeRef,
        owner_module: ModuleRef,
    ) -> BodyTy {
        match ty {
            TypeRef::Unit => BodyTy::Unit,
            TypeRef::Never => BodyTy::Never,
            TypeRef::Path(_) => {
                let Some(path) = Path::from_type_ref(ty) else {
                    return BodyTy::Syntax(ty.clone());
                };
                self.ty_for_type_path(PathContext::module(owner_module), &path)
            }
            TypeRef::Unknown(_) | TypeRef::Infer => BodyTy::Unknown,
            TypeRef::Tuple(types) if types.is_empty() => BodyTy::Unit,
            _ => BodyTy::Syntax(ty.clone()),
        }
    }

    pub(super) fn impl_self_tys(&self, impl_ref: ImplRef) -> Vec<TypeDefRef> {
        self.0
            .project
            .semantic_ir_db()
            .impl_data(impl_ref)
            .map(|data| data.resolved_self_tys.clone())
            .unwrap_or_default()
    }

    pub(super) fn path_context_for_body(&self, body: &BodyData) -> PathContext {
        let impl_ref = self
            .0
            .project
            .semantic_ir_db()
            .function_data(body.owner)
            .and_then(|function| match function.owner {
                ItemOwner::Impl(id) => Some(ImplRef {
                    target: body.owner.target,
                    id,
                }),
                ItemOwner::Module(_) | ItemOwner::Trait(_) => None,
            });

        PathContext {
            module: body.owner_module,
            impl_ref,
        }
    }

    pub(super) fn resolve_body_local_type_item(
        &self,
        body_ref: BodyRef,
        mut scope: ScopeId,
        path: &Path,
    ) -> Option<BodyItemRef> {
        let name = path.single_name()?;
        let body = self.0.project.body_ir_db().body_data(body_ref)?;

        loop {
            let scope_data = body.scope(scope)?;
            for item in scope_data.local_items.iter().rev() {
                let item_data = body.local_item(*item)?;
                if item_data.name == name {
                    return Some(BodyItemRef {
                        body: body_ref,
                        item: *item,
                    });
                }
            }

            scope = scope_data.parent?;
        }
    }

    fn ty_for_def(&self, def: DefId) -> Option<BodyTy> {
        let DefId::Local(local_def) = def else {
            return None;
        };
        self.0
            .project
            .semantic_ir_db()
            .type_def_for_local_def(local_def)
            .map(|ty| BodyTy::Nominal(vec![ty]))
    }

    fn ty_for_field(&self, field: FieldRef) -> Option<BodyTy> {
        let field_data = self.0.project.semantic_ir_db().field_data(field)?;
        Some(self.ty_from_type_ref_in_module(&field_data.field.ty, field_data.owner_module))
    }
}

pub(super) fn type_defs_from_body_ty(ty: &BodyTy) -> Vec<TypeDefRef> {
    match ty {
        BodyTy::Nominal(types) | BodyTy::SelfTy(types) => types.clone(),
        BodyTy::Unit
        | BodyTy::Never
        | BodyTy::Syntax(_)
        | BodyTy::LocalNominal(_)
        | BodyTy::Unknown => Vec::new(),
    }
}
