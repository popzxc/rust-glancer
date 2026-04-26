use crate::{
    body_ir::{BodyRef, BodyTy, BodyTypePathResolution, ScopeId},
    def_map::{DefId, ModuleRef, Path},
    item_tree::TypeRef,
    semantic_ir::{FieldRef, SemanticTypePathResolution, TypeDefRef, TypePathContext},
};

use super::{
    Analysis,
    data::{PathRole, SymbolAt},
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

    pub(super) fn ty_for_type_path(&self, context: TypePathContext, path: &Path) -> BodyTy {
        semantic_type_path_resolution_to_ty(self.0.project.semantic_ir_db().resolve_type_path(
            self.0.project.def_map_db(),
            context,
            path,
        ))
    }

    pub(super) fn ty_for_body_type_path(
        &self,
        body_ref: BodyRef,
        scope: ScopeId,
        path: &Path,
    ) -> BodyTy {
        body_type_path_resolution_to_ty(self.0.project.body_ir_db().resolve_type_path_in_scope(
            self.0.project.def_map_db(),
            self.0.project.semantic_ir_db(),
            body_ref,
            scope,
            path,
        ))
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
                self.ty_for_type_path(TypePathContext::module(owner_module), &path)
            }
            TypeRef::Unknown(_) | TypeRef::Infer => BodyTy::Unknown,
            TypeRef::Tuple(types) if types.is_empty() => BodyTy::Unit,
            _ => BodyTy::Syntax(ty.clone()),
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

pub(super) fn semantic_type_path_resolution_to_ty(
    resolution: SemanticTypePathResolution,
) -> BodyTy {
    match resolution {
        SemanticTypePathResolution::SelfType(types) => BodyTy::SelfTy(types),
        SemanticTypePathResolution::TypeDefs(types) => BodyTy::Nominal(types),
        SemanticTypePathResolution::Traits(_) => BodyTy::Unknown,
        SemanticTypePathResolution::Unknown => BodyTy::Unknown,
    }
}

pub(super) fn body_type_path_resolution_to_ty(resolution: BodyTypePathResolution) -> BodyTy {
    match resolution {
        BodyTypePathResolution::BodyLocal(item) => BodyTy::LocalNominal(vec![item]),
        BodyTypePathResolution::SelfType(types) => BodyTy::SelfTy(types),
        BodyTypePathResolution::TypeDefs(types) => BodyTy::Nominal(types),
        BodyTypePathResolution::Traits(_) => BodyTy::Unknown,
        BodyTypePathResolution::Unknown => BodyTy::Unknown,
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
