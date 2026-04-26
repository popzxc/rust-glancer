use crate::{
    Project,
    body_ir::{BodyData, BodyRef, ScopeId, StmtKind},
    def_map::{DefId, ModuleRef, Path, TargetRef},
    item_tree::{
        FieldList, GenericArg, GenericParams, ItemKind, ItemNode, ItemTreeRef, TypeBound, TypePath,
        TypeRef, UsePath, WherePredicate,
    },
    parse::{FileId, span::Span},
    semantic_ir::{
        FieldRef, FunctionId, FunctionRef, ImplId, ItemOwner, StructId, TypeDefId, TypeDefRef,
        TypePathContext, UnionId,
    },
};

use super::data::{SymbolAt, SymbolCandidate};

pub(super) fn body_type_path_candidates(
    body_ref: BodyRef,
    body: &BodyData,
    file_id: FileId,
    offset: u32,
) -> Vec<SymbolCandidate> {
    let mut candidates = Vec::new();
    for statement in &body.statements {
        if statement.source.file_id != file_id {
            continue;
        }
        let StmtKind::Let {
            scope,
            annotation: Some(annotation),
            ..
        } = &statement.kind
        else {
            continue;
        };
        BodyTypeCursorScanner {
            body: body_ref,
            scope: *scope,
            offset,
            candidates: &mut candidates,
        }
        .scan_type_ref(annotation);
    }

    candidates
}

pub(super) fn item_signature_candidates(
    project: &Project,
    target: TargetRef,
    file_id: FileId,
    offset: u32,
) -> Vec<SymbolCandidate> {
    let mut candidates = Vec::new();

    CursorScanner {
        project,
        target,
        file_id,
        offset,
        candidates: &mut candidates,
    }
    .scan();

    candidates
}

struct BodyTypeCursorScanner<'a> {
    body: BodyRef,
    scope: ScopeId,
    offset: u32,
    candidates: &'a mut Vec<SymbolCandidate>,
}

impl BodyTypeCursorScanner<'_> {
    fn scan_type_ref(&mut self, ty: &TypeRef) {
        match ty {
            TypeRef::Path(path) => self.scan_type_path(path),
            TypeRef::Tuple(types) => {
                for ty in types {
                    self.scan_type_ref(ty);
                }
            }
            TypeRef::Reference { inner, .. }
            | TypeRef::RawPointer { inner, .. }
            | TypeRef::Slice(inner) => self.scan_type_ref(inner),
            TypeRef::Array { inner, .. } => self.scan_type_ref(inner),
            TypeRef::FnPointer { params, ret } => {
                for param in params {
                    self.scan_type_ref(param);
                }
                self.scan_type_ref(ret);
            }
            TypeRef::ImplTrait(bounds) | TypeRef::DynTrait(bounds) => {
                for bound in bounds {
                    if let TypeBound::Trait(ty) = bound {
                        self.scan_type_ref(ty);
                    }
                }
            }
            TypeRef::Unknown(_) | TypeRef::Never | TypeRef::Unit | TypeRef::Infer => {}
        }
    }

    fn scan_type_path(&mut self, path: &TypePath) {
        for (idx, segment) in path.segments.iter().enumerate() {
            if segment.span.touches(self.offset) {
                let path = Path::from_type_path_prefix(path, idx);
                self.candidates.push(SymbolCandidate {
                    symbol: SymbolAt::BodyPath {
                        body: self.body,
                        scope: self.scope,
                        path,
                        span: segment.span,
                    },
                    span: segment.span,
                });
            }

            for arg in &segment.args {
                self.scan_generic_arg(arg);
            }
        }
    }

    fn scan_generic_arg(&mut self, arg: &GenericArg) {
        match arg {
            GenericArg::Type(ty) => self.scan_type_ref(ty),
            GenericArg::AssocType { ty: Some(ty), .. } => self.scan_type_ref(ty),
            GenericArg::Lifetime(_)
            | GenericArg::Const(_)
            | GenericArg::AssocType { ty: None, .. }
            | GenericArg::Unsupported(_) => {}
        }
    }
}

struct CursorScanner<'a> {
    project: &'a Project,
    target: TargetRef,
    file_id: FileId,
    offset: u32,
    candidates: &'a mut Vec<SymbolCandidate>,
}

impl CursorScanner<'_> {
    fn scan(&mut self) {
        self.scan_local_definitions();
        self.scan_import_paths();
        self.scan_semantic_items();
    }

    fn scan_local_definitions(&mut self) {
        let Some(def_map) = self.project.def_map_db().def_map(self.target) else {
            return;
        };

        for (local_def_idx, local_def) in def_map.local_defs().iter().enumerate() {
            if local_def.file_id != self.file_id {
                continue;
            }

            let span = self
                .item(local_def.source)
                .and_then(|item| item.name_span)
                .unwrap_or(local_def.span);
            if !span.touches(self.offset) {
                continue;
            }

            self.candidates.push(SymbolCandidate {
                symbol: SymbolAt::Def {
                    def: DefId::Local(crate::def_map::LocalDefRef {
                        target: self.target,
                        local_def: crate::def_map::LocalDefId(local_def_idx),
                    }),
                    span,
                },
                span,
            });
        }
    }

    fn scan_import_paths(&mut self) {
        let Some(def_map) = self.project.def_map_db().def_map(self.target) else {
            return;
        };

        for import in def_map.imports() {
            if import.source.file_id != self.file_id {
                continue;
            }

            let Some(source_import) = self
                .item(import.source)
                .and_then(|item| match &item.kind {
                    ItemKind::Use(use_item) => use_item.imports.get(import.import_index),
                    _ => None,
                })
                .cloned()
            else {
                continue;
            };
            let module = ModuleRef {
                target: self.target,
                module: import.module,
            };
            self.push_use_path(module, &source_import.path);
            if let crate::item_tree::ImportAlias::Explicit { span, .. } = source_import.alias {
                if span.touches(self.offset) {
                    self.push_use_path_candidate(
                        module,
                        Path::from_use_path(&source_import.path),
                        span,
                    );
                }
            }
        }
    }

    fn scan_semantic_items(&mut self) {
        let Some(target_ir) = self.project.semantic_ir_db().target_ir(self.target) else {
            return;
        };
        let items = target_ir.items();

        for (idx, data) in items.structs.iter().enumerate() {
            if data.source.file_id != self.file_id {
                continue;
            }
            let ty = TypeDefRef {
                target: self.target,
                id: TypeDefId::Struct(StructId(idx)),
            };
            let context = TypePathContext::module(data.owner);
            self.scan_generic_params(context, &data.generics);
            self.scan_field_list(ty, context, &data.fields);
        }

        for (idx, data) in items.unions.iter().enumerate() {
            if data.source.file_id != self.file_id {
                continue;
            }
            let ty = TypeDefRef {
                target: self.target,
                id: TypeDefId::Union(UnionId(idx)),
            };
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

        for data in &items.enums {
            if data.source.file_id != self.file_id {
                continue;
            }
            let context = TypePathContext::module(data.owner);
            self.scan_generic_params(context, &data.generics);
            for variant in &data.variants {
                self.scan_field_list_for_owner(context, &variant.fields);
            }
        }

        for data in &items.traits {
            if data.source.file_id != self.file_id {
                continue;
            }
            let context = TypePathContext::module(data.owner);
            self.scan_generic_params(context, &data.generics);
            self.scan_type_bounds(context, &data.super_traits);
        }

        for (idx, data) in items.impls.iter().enumerate() {
            if data.source.file_id != self.file_id {
                continue;
            }
            let Some(context) = self.owner_context(ItemOwner::Impl(ImplId(idx))) else {
                continue;
            };
            self.scan_generic_params(context, &data.generics);
            if let Some(trait_ref) = &data.trait_ref {
                self.push_type_ref(context, trait_ref);
            }
            self.push_type_ref(context, &data.self_ty);
        }

        for (idx, data) in items.functions.iter().enumerate() {
            if data.source.file_id != self.file_id {
                continue;
            }
            if data.local_def.is_none() {
                let span = self
                    .item(data.source)
                    .and_then(|item| item.name_span)
                    .unwrap_or_else(|| {
                        self.item(data.source)
                            .map(|item| item.span)
                            .expect("function source item should exist")
                    });
                self.push_function(
                    FunctionRef {
                        target: self.target,
                        id: FunctionId(idx),
                    },
                    span,
                );
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

        for data in &items.type_aliases {
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

        for data in &items.consts {
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

        for data in &items.statics {
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
                self.push_type_path_candidate(
                    context,
                    Path::from_type_path_prefix(path, idx),
                    segment.span,
                );
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

    fn push_use_path(&mut self, module: ModuleRef, path: &UsePath) {
        for (idx, segment) in path.segments.iter().enumerate() {
            if segment.span.touches(self.offset) {
                self.push_use_path_candidate(
                    module,
                    Path::from_use_path_prefix(path, idx),
                    segment.span,
                );
            }
        }
    }

    fn push_type_path_candidate(&mut self, context: TypePathContext, path: Path, span: Span) {
        self.candidates.push(SymbolCandidate {
            symbol: SymbolAt::TypePath {
                context,
                path,
                span,
            },
            span,
        });
    }

    fn push_use_path_candidate(&mut self, module: ModuleRef, path: Path, span: Span) {
        self.candidates.push(SymbolCandidate {
            symbol: SymbolAt::UsePath { module, path, span },
            span,
        });
    }

    fn push_field(&mut self, field: FieldRef, span: Span) {
        if !span.touches(self.offset) {
            return;
        }

        self.candidates.push(SymbolCandidate {
            symbol: SymbolAt::Field { field, span },
            span,
        });
    }

    fn push_function(&mut self, function: FunctionRef, span: Span) {
        if !span.touches(self.offset) {
            return;
        }

        self.candidates.push(SymbolCandidate {
            symbol: SymbolAt::Function { function, span },
            span,
        });
    }

    fn owner_context(&self, owner: ItemOwner) -> Option<TypePathContext> {
        self.project
            .semantic_ir_db()
            .type_path_context_for_owner(self.target, owner)
    }

    fn item(&self, source: ItemTreeRef) -> Option<&ItemNode> {
        self.project
            .item_tree_db()
            .package(self.target.package.0)?
            .item(source)
    }
}
