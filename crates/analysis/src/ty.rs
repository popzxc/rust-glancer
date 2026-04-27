use rg_body_ir::{
    BodyLocalNominalTy, BodyNominalTy, BodyRef, BodyTy, BodyTypePathResolution, ScopeId,
};
use rg_def_map::{DefId, Path};
use rg_semantic_ir::{FieldRef, SemanticTypePathResolution, TypeDefRef, TypePathContext};

use super::{Analysis, data::SymbolAt};

pub(super) struct TypeResolver<'a, 'db>(&'a Analysis<'db>);

impl<'a, 'db> TypeResolver<'a, 'db> {
    pub(super) fn new(analysis: &'a Analysis<'db>) -> Self {
        Self(analysis)
    }

    pub(super) fn type_at(
        &self,
        target: rg_def_map::TargetRef,
        file_id: rg_parse::FileId,
        offset: u32,
    ) -> Option<BodyTy> {
        match self.0.symbol_at(target, file_id, offset)? {
            SymbolAt::Expr { body, expr } => self
                .0
                .body_ir
                .body_data(body)?
                .expr(expr)
                .map(|data| data.ty.clone()),
            SymbolAt::Binding { body, binding } => self
                .0
                .body_ir
                .body_data(body)?
                .binding(binding)
                .map(|data| data.ty.clone()),
            SymbolAt::BodyPath {
                body, scope, path, ..
            } => Some(self.ty_for_body_type_path(body, scope, &path)),
            SymbolAt::Def { def, .. } => self.ty_for_def(def),
            SymbolAt::Field { field, .. } => self.ty_for_field(field),
            SymbolAt::LocalItem { item, .. } => {
                Some(BodyTy::LocalNominal(vec![BodyLocalNominalTy::bare(item)]))
            }
            SymbolAt::TypePath { context, path, .. } => Some(self.ty_for_type_path(context, &path)),
            SymbolAt::UsePath { .. } | SymbolAt::Function { .. } => None,
            SymbolAt::Body { .. } => None,
        }
    }

    pub(super) fn ty_for_type_path(&self, context: TypePathContext, path: &Path) -> BodyTy {
        semantic_type_path_resolution_to_ty(self.0.semantic_ir.resolve_type_path(
            self.0.def_map,
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
        body_type_path_resolution_to_ty(self.0.body_ir.resolve_type_path_in_scope(
            self.0.def_map,
            self.0.semantic_ir,
            body_ref,
            scope,
            path,
        ))
    }

    fn ty_for_def(&self, def: DefId) -> Option<BodyTy> {
        let DefId::Local(local_def) = def else {
            return None;
        };
        self.0
            .semantic_ir
            .type_def_for_local_def(local_def)
            .map(|ty| BodyTy::Nominal(vec![BodyNominalTy::bare(ty)]))
    }

    fn ty_for_field(&self, field: FieldRef) -> Option<BodyTy> {
        self.0
            .body_ir
            .ty_for_field(self.0.def_map, self.0.semantic_ir, field)
    }
}

pub(super) fn semantic_type_path_resolution_to_ty(
    resolution: SemanticTypePathResolution,
) -> BodyTy {
    match resolution {
        SemanticTypePathResolution::SelfType(types) => {
            BodyTy::SelfTy(types.into_iter().map(BodyNominalTy::bare).collect())
        }
        SemanticTypePathResolution::TypeDefs(types) => {
            BodyTy::Nominal(types.into_iter().map(BodyNominalTy::bare).collect())
        }
        SemanticTypePathResolution::Traits(_) => BodyTy::Unknown,
        SemanticTypePathResolution::Unknown => BodyTy::Unknown,
    }
}

pub(super) fn body_type_path_resolution_to_ty(resolution: BodyTypePathResolution) -> BodyTy {
    match resolution {
        BodyTypePathResolution::BodyLocal(item) => {
            BodyTy::LocalNominal(vec![BodyLocalNominalTy::bare(item)])
        }
        BodyTypePathResolution::SelfType(types) => {
            BodyTy::SelfTy(types.into_iter().map(BodyNominalTy::bare).collect())
        }
        BodyTypePathResolution::TypeDefs(types) => {
            BodyTy::Nominal(types.into_iter().map(BodyNominalTy::bare).collect())
        }
        BodyTypePathResolution::Traits(_) => BodyTy::Unknown,
        BodyTypePathResolution::Unknown => BodyTy::Unknown,
    }
}

pub(super) fn type_defs_from_body_ty(ty: &BodyTy) -> Vec<TypeDefRef> {
    ty.type_defs()
}
