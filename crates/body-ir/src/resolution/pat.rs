//! Pattern-directed binding type propagation.
//!
//! This pass stays deliberately narrow: it only pushes already-known expected types into pattern
//! bindings. Enum variants are matched against a known enum scrutinee/annotation type; patterns do
//! not infer the scrutinee type by themselves.

use rg_def_map::{DefMapDb, Path, PathSegment};
use rg_item_tree::{FieldItem, FieldKey, FieldList};
use rg_semantic_ir::{SemanticIrDb, TypeDefId, TypePathContext};

use crate::{
    body::BodyData,
    expr::ExprKind,
    ids::{BindingId, BodyRef, ExprId, PatId, ScopeId},
    pat::{PatKind, RecordPatField},
    path::BodyPath,
    stmt::StmtKind,
    ty::{BodyNominalTy, BodyTy},
};

use super::{
    push_unique,
    ty::{TypeSubst, subst_from_generics, ty_from_type_ref_in_context},
    type_path::BodyTypePathResolver,
};

pub(super) struct PatternTypePropagator<'db, 'body> {
    def_map: &'db DefMapDb,
    semantic_ir: &'db SemanticIrDb,
    body_ref: BodyRef,
    body: &'body mut BodyData,
}

impl<'db, 'body> PatternTypePropagator<'db, 'body> {
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

    pub(super) fn propagate(&mut self) -> bool {
        let mut changed = false;

        for statement_idx in 0..self.body.statements.len() {
            let StmtKind::Let {
                scope,
                pat: Some(pat),
                annotation,
                initializer,
                ..
            } = self.body.statements[statement_idx].kind.clone()
            else {
                continue;
            };

            let expected_ty = self.expected_ty_for_let(scope, annotation.as_ref(), initializer);
            changed |= self.propagate_pat(pat, &expected_ty);
        }

        for expr_idx in 0..self.body.exprs.len() {
            let ExprKind::Match { scrutinee, arms } = self.body.exprs[expr_idx].kind.clone() else {
                continue;
            };
            let Some(scrutinee) = scrutinee else {
                continue;
            };
            let expected_ty = self.body.exprs[scrutinee.0].ty.clone();
            for arm in arms {
                if let Some(pat) = arm.pat {
                    changed |= self.propagate_pat(pat, &expected_ty);
                }
            }
        }

        changed
    }

    fn expected_ty_for_let(
        &self,
        scope: ScopeId,
        annotation: Option<&rg_item_tree::TypeRef>,
        initializer: Option<ExprId>,
    ) -> BodyTy {
        if let Some(annotation) = annotation {
            let ty =
                BodyTypePathResolver::new(self.def_map, self.semantic_ir, self.body_ref, self.body)
                    .ty_from_type_ref_in_scope(annotation, scope);
            if !matches!(ty, BodyTy::Unknown) {
                return ty;
            }
        }

        initializer
            .map(|expr| self.body.exprs[expr.0].ty.clone())
            .unwrap_or(BodyTy::Unknown)
    }

    fn propagate_pat(&mut self, pat: PatId, expected_ty: &BodyTy) -> bool {
        if matches!(expected_ty, BodyTy::Unknown) {
            return false;
        }

        let Some(data) = self.body.pat(pat).cloned() else {
            return false;
        };

        match data.kind {
            PatKind::Binding { binding, subpat } => {
                let mut changed = binding
                    .map(|binding| self.set_binding_ty(binding, expected_ty.clone()))
                    .unwrap_or(false);
                if let Some(subpat) = subpat {
                    changed |= self.propagate_pat(subpat, expected_ty);
                }
                changed
            }
            PatKind::TupleStruct { path, fields } => {
                self.propagate_tuple_variant(path.as_ref(), &fields, expected_ty)
            }
            PatKind::Record { path, fields } => {
                self.propagate_record_variant(path.as_ref(), &fields, expected_ty)
            }
            PatKind::Or { pats } => pats
                .into_iter()
                .any(|pat| self.propagate_pat(pat, expected_ty)),
            PatKind::Ref { pat } | PatKind::Box { pat } => self.propagate_pat(pat, expected_ty),
            PatKind::Tuple { .. }
            | PatKind::Slice { .. }
            | PatKind::Path { .. }
            | PatKind::Wildcard
            | PatKind::Unsupported { .. } => false,
        }
    }

    fn propagate_tuple_variant(
        &mut self,
        path: Option<&BodyPath>,
        fields: &[PatId],
        expected_ty: &BodyTy,
    ) -> bool {
        let Some(variant_name) = variant_name(path.map(|path| &path.path)) else {
            return false;
        };

        let mut changed = false;
        for (idx, field_pat) in fields.iter().enumerate() {
            let field_key = FieldKey::Tuple(idx);
            if let Some(field_ty) = self.variant_field_ty(expected_ty, variant_name, &field_key) {
                changed |= self.propagate_pat(*field_pat, &field_ty);
            }
        }
        changed
    }

    fn propagate_record_variant(
        &mut self,
        path: Option<&BodyPath>,
        fields: &[RecordPatField],
        expected_ty: &BodyTy,
    ) -> bool {
        let Some(variant_name) = variant_name(path.map(|path| &path.path)) else {
            return false;
        };

        let mut changed = false;
        for field in fields {
            if let Some(field_ty) = self.variant_field_ty(expected_ty, variant_name, &field.key) {
                changed |= self.propagate_pat(field.pat, &field_ty);
            }
        }
        changed
    }

    fn variant_field_ty(
        &self,
        expected_ty: &BodyTy,
        variant_name: &str,
        field_key: &FieldKey,
    ) -> Option<BodyTy> {
        let mut candidates = Vec::new();
        for enum_ty in enum_ty_candidates(expected_ty) {
            let Some(field_ty) = self.variant_field_ty_for_enum(enum_ty, variant_name, field_key)
            else {
                continue;
            };
            push_unique(&mut candidates, field_ty);
        }

        match candidates.as_slice() {
            [ty] => Some(ty.clone()),
            [] | [_, ..] => None,
        }
    }

    fn variant_field_ty_for_enum(
        &self,
        enum_ty: &BodyNominalTy,
        variant_name: &str,
        field_key: &FieldKey,
    ) -> Option<BodyTy> {
        if !matches!(enum_ty.def.id, TypeDefId::Enum(_)) {
            return None;
        }

        let enum_data = self.semantic_ir.enum_data_for_type_def(enum_ty.def)?;
        let (_, variant) = self
            .semantic_ir
            .enum_variant_for_type_def(enum_ty.def, variant_name)?;
        let field = variant_field(&variant.fields, field_key)?;
        let subst = self
            .semantic_ir
            .generic_params_for_type_def(enum_ty.def)
            .map(|generics| subst_from_generics(generics, &enum_ty.args))
            .unwrap_or_else(TypeSubst::new);

        Some(ty_from_type_ref_in_context(
            self.def_map,
            self.semantic_ir,
            &field.ty,
            TypePathContext::module(enum_data.owner),
            BodyTy::Unknown,
            &subst,
        ))
    }

    fn set_binding_ty(&mut self, binding: BindingId, ty: BodyTy) -> bool {
        if matches!(ty, BodyTy::Unknown) {
            return false;
        }

        let Some(binding_data) = self.body.bindings.get_mut(binding.0) else {
            return false;
        };
        if !matches!(binding_data.ty, BodyTy::Unknown) {
            return false;
        }

        binding_data.ty = ty;
        true
    }
}

fn enum_ty_candidates(ty: &BodyTy) -> Vec<&BodyNominalTy> {
    match ty {
        BodyTy::Nominal(types) | BodyTy::SelfTy(types) => types
            .iter()
            .filter(|ty| matches!(ty.def.id, TypeDefId::Enum(_)))
            .collect(),
        BodyTy::Unit
        | BodyTy::Never
        | BodyTy::Syntax(_)
        | BodyTy::LocalNominal(_)
        | BodyTy::Unknown => Vec::new(),
    }
}

fn variant_field<'a>(fields: &'a FieldList, key: &FieldKey) -> Option<&'a FieldItem> {
    match key {
        FieldKey::Named(_) => fields
            .fields()
            .iter()
            .find(|field| field.key.as_ref() == Some(key)),
        FieldKey::Tuple(index) => fields
            .fields()
            .get(*index)
            .filter(|field| field.key.as_ref() == Some(key)),
    }
}

fn variant_name(path: Option<&Path>) -> Option<&str> {
    match path?.segments.last()? {
        PathSegment::Name(name) => Some(name),
        PathSegment::SelfKw | PathSegment::SuperKw | PathSegment::CrateKw => None,
    }
}
