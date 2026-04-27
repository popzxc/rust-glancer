//! Cursor-oriented queries over semantic item signatures.
//!
//! Analysis owns the user-facing `SymbolAt` enum, but semantic IR owns the shape of item
//! signatures. Keeping this scan here prevents analysis from knowing how every semantic item stores
//! generic params, field types, impl headers, and associated function declarations.

use rg_def_map::{Path, TargetRef};
use rg_item_tree::{
    FieldList, GenericArg, GenericParams, TypeBound, TypePath, TypeRef, WherePredicate,
};
use rg_parse::{FileId, Span};

use crate::{FieldRef, FunctionRef, ItemOwner, SemanticIrDb, TypeDefRef, TypePathContext};

/// One semantic signature source node that can participate in cursor queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticCursorCandidate {
    Field {
        field: FieldRef,
        span: Span,
    },
    Function {
        function: FunctionRef,
        span: Span,
    },
    TypePath {
        context: TypePathContext,
        path: Path,
        span: Span,
    },
}

impl SemanticCursorCandidate {
    fn span(&self) -> Span {
        match self {
            Self::Field { span, .. }
            | Self::Function { span, .. }
            | Self::TypePath { span, .. } => *span,
        }
    }
}

impl SemanticIrDb {
    pub fn signature_cursor_candidates(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Vec<SemanticCursorCandidate> {
        let mut candidates = Vec::new();
        SignatureCursorScanner {
            semantic_ir: self,
            target,
            file_id,
            offset,
            candidates: &mut candidates,
        }
        .scan();

        candidates
    }
}

struct SignatureCursorScanner<'a> {
    semantic_ir: &'a SemanticIrDb,
    target: TargetRef,
    file_id: FileId,
    offset: u32,
    candidates: &'a mut Vec<SemanticCursorCandidate>,
}

impl SignatureCursorScanner<'_> {
    fn scan(&mut self) {
        self.scan_structs();
        self.scan_unions();
        self.scan_enums();
        self.scan_traits();
        self.scan_impls();
        self.scan_functions();
        self.scan_type_aliases();
        self.scan_consts();
        self.scan_statics();
    }

    fn scan_structs(&mut self) {
        for (ty, data) in self.semantic_ir.structs(self.target) {
            if data.source.file_id != self.file_id {
                continue;
            }
            let context = TypePathContext::module(data.owner);
            self.scan_generic_params(context, &data.generics);
            self.scan_field_list(ty, context, &data.fields);
        }
    }

    fn scan_unions(&mut self) {
        for (ty, data) in self.semantic_ir.unions(self.target) {
            if data.source.file_id != self.file_id {
                continue;
            }
            let context = TypePathContext::module(data.owner);
            self.scan_generic_params(context, &data.generics);
            for (field_idx, field) in data.fields.iter().enumerate() {
                self.push_field(
                    FieldRef {
                        owner: ty,
                        index: field_idx,
                    },
                    field.span,
                );
                self.push_type_ref(context, &field.ty);
            }
        }
    }

    fn scan_enums(&mut self) {
        for (_, data) in self.semantic_ir.enums(self.target) {
            if data.source.file_id != self.file_id {
                continue;
            }
            let context = TypePathContext::module(data.owner);
            self.scan_generic_params(context, &data.generics);
            for variant in &data.variants {
                self.scan_field_list_for_owner(context, &variant.fields);
            }
        }
    }

    fn scan_traits(&mut self) {
        for (_, data) in self.semantic_ir.traits(self.target) {
            if data.source.file_id != self.file_id {
                continue;
            }
            let context = TypePathContext::module(data.owner);
            self.scan_generic_params(context, &data.generics);
            self.scan_type_bounds(context, &data.super_traits);
        }
    }

    fn scan_impls(&mut self) {
        for (impl_ref, data) in self.semantic_ir.impls(self.target) {
            if data.source.file_id != self.file_id {
                continue;
            }
            let Some(context) = self.owner_context(ItemOwner::Impl(impl_ref.id)) else {
                continue;
            };
            self.scan_generic_params(context, &data.generics);
            if let Some(trait_ref) = &data.trait_ref {
                self.push_type_ref(context, trait_ref);
            }
            self.push_type_ref(context, &data.self_ty);
        }
    }

    fn scan_functions(&mut self) {
        for (function_ref, data) in self.semantic_ir.functions(self.target) {
            if data.source.file_id != self.file_id {
                continue;
            }
            if data.local_def.is_none() {
                let span = data.name_span.unwrap_or(data.span);
                self.push_function(function_ref, span);
            }
            let Some(context) = self.owner_context(data.owner) else {
                continue;
            };
            self.scan_generic_params(context, &data.declaration.generics);
            for param in &data.declaration.params {
                if let Some(ty) = &param.ty {
                    self.push_type_ref(context, ty);
                }
            }
            if let Some(ret_ty) = &data.declaration.ret_ty {
                self.push_type_ref(context, ret_ty);
            }
        }
    }

    fn scan_type_aliases(&mut self) {
        for (_, data) in self.semantic_ir.type_aliases(self.target) {
            if data.source.file_id != self.file_id {
                continue;
            }
            let Some(context) = self.owner_context(data.owner) else {
                continue;
            };
            self.scan_generic_params(context, &data.declaration.generics);
            self.scan_type_bounds(context, &data.declaration.bounds);
            if let Some(ty) = &data.declaration.aliased_ty {
                self.push_type_ref(context, ty);
            }
        }
    }

    fn scan_consts(&mut self) {
        for (_, data) in self.semantic_ir.consts(self.target) {
            if data.source.file_id != self.file_id {
                continue;
            }
            let Some(context) = self.owner_context(data.owner) else {
                continue;
            };
            self.scan_generic_params(context, &data.declaration.generics);
            if let Some(ty) = &data.declaration.ty {
                self.push_type_ref(context, ty);
            }
        }
    }

    fn scan_statics(&mut self) {
        for (_, data) in self.semantic_ir.statics(self.target) {
            if data.source.file_id != self.file_id {
                continue;
            }
            if let Some(ty) = &data.ty {
                self.push_type_ref(TypePathContext::module(data.owner), ty);
            }
        }
    }

    fn scan_field_list(&mut self, owner: TypeDefRef, context: TypePathContext, fields: &FieldList) {
        for (idx, field) in fields.fields().iter().enumerate() {
            self.push_field(FieldRef { owner, index: idx }, field.span);
            self.push_type_ref(context, &field.ty);
        }
    }

    fn scan_field_list_for_owner(&mut self, context: TypePathContext, fields: &FieldList) {
        for field in fields.fields() {
            self.push_type_ref(context, &field.ty);
        }
    }

    fn scan_generic_params(&mut self, context: TypePathContext, generics: &GenericParams) {
        for param in &generics.types {
            self.scan_type_bounds(context, &param.bounds);
            if let Some(default) = &param.default {
                self.push_type_ref(context, default);
            }
        }
        for param in &generics.consts {
            if let Some(ty) = &param.ty {
                self.push_type_ref(context, ty);
            }
        }
        for predicate in &generics.where_predicates {
            match predicate {
                WherePredicate::Type { ty, bounds } => {
                    self.push_type_ref(context, ty);
                    self.scan_type_bounds(context, bounds);
                }
                WherePredicate::Lifetime { .. } | WherePredicate::Unsupported(_) => {}
            }
        }
    }

    fn scan_type_bounds(&mut self, context: TypePathContext, bounds: &[TypeBound]) {
        for bound in bounds {
            match bound {
                TypeBound::Trait(ty) => self.push_type_ref(context, ty),
                TypeBound::Lifetime(_) | TypeBound::Unsupported(_) => {}
            }
        }
    }

    fn push_type_ref(&mut self, context: TypePathContext, ty: &TypeRef) {
        match ty {
            TypeRef::Path(path) => self.push_type_path(context, path),
            TypeRef::Tuple(types) => {
                for ty in types {
                    self.push_type_ref(context, ty);
                }
            }
            TypeRef::Reference { inner, .. }
            | TypeRef::RawPointer { inner, .. }
            | TypeRef::Slice(inner) => self.push_type_ref(context, inner),
            TypeRef::Array { inner, .. } => self.push_type_ref(context, inner),
            TypeRef::FnPointer { params, ret } => {
                for param in params {
                    self.push_type_ref(context, param);
                }
                self.push_type_ref(context, ret);
            }
            TypeRef::ImplTrait(bounds) | TypeRef::DynTrait(bounds) => {
                self.scan_type_bounds(context, bounds);
            }
            TypeRef::Unknown(_) | TypeRef::Never | TypeRef::Unit | TypeRef::Infer => {}
        }
    }

    fn push_type_path(&mut self, context: TypePathContext, path: &TypePath) {
        for (idx, segment) in path.segments.iter().enumerate() {
            if segment.span.touches(self.offset) {
                self.push_candidate(SemanticCursorCandidate::TypePath {
                    context,
                    path: Path::from_type_path_prefix(path, idx),
                    span: segment.span,
                });
            }

            for arg in &segment.args {
                self.push_generic_arg(context, arg);
            }
        }
    }

    fn push_generic_arg(&mut self, context: TypePathContext, arg: &GenericArg) {
        match arg {
            GenericArg::Type(ty) => self.push_type_ref(context, ty),
            GenericArg::AssocType { ty: Some(ty), .. } => self.push_type_ref(context, ty),
            GenericArg::Lifetime(_)
            | GenericArg::Const(_)
            | GenericArg::AssocType { ty: None, .. }
            | GenericArg::Unsupported(_) => {}
        }
    }

    fn push_field(&mut self, field: FieldRef, span: Span) {
        self.push_candidate(SemanticCursorCandidate::Field { field, span });
    }

    fn push_function(&mut self, function: FunctionRef, span: Span) {
        self.push_candidate(SemanticCursorCandidate::Function { function, span });
    }

    fn push_candidate(&mut self, candidate: SemanticCursorCandidate) {
        if candidate.span().touches(self.offset) {
            self.candidates.push(candidate);
        }
    }

    fn owner_context(&self, owner: ItemOwner) -> Option<TypePathContext> {
        self.semantic_ir
            .type_path_context_for_owner(self.target, owner)
    }
}
