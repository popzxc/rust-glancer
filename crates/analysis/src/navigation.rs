//! Converts found symbols and inferred types into editor navigation targets.
//!
//! Analysis can receive identities from def-map, semantic IR, or body IR. This adapter keeps the
//! public navigation shape uniform while preserving the source span each layer considers primary.

use rg_body_ir::{
    BodyData, BodyFieldRef, BodyItemRef, BodyRef, BodyResolution, BodyTy, BodyTypePathResolution,
    ResolvedFieldRef, ResolvedFunctionRef, ScopeId,
};
use rg_def_map::{DefId, LocalDefRef, ModuleOrigin, ModuleRef, Path, TargetRef};
use rg_parse::FileId;
use rg_semantic_ir::{
    EnumVariantRef, FieldRef, FunctionRef, SemanticTypePathResolution, TraitRef, TypeDefRef,
    TypePathContext,
};

use super::{
    Analysis,
    data::{NavigationTarget, NavigationTargetKind, SymbolAt},
};

pub(super) struct NavigationTargetResolver<'a, 'db>(&'a Analysis<'db>);

impl<'a, 'db> NavigationTargetResolver<'a, 'db> {
    pub(super) fn new(analysis: &'a Analysis<'db>) -> Self {
        Self(analysis)
    }

    fn navigation_target_for_def(&self, def: DefId) -> Option<NavigationTarget> {
        match def {
            DefId::Module(module_ref) => self.navigation_target_for_module(module_ref),
            DefId::Local(local_def) => self.navigation_target_for_local_def(local_def),
        }
    }

    fn navigation_target_for_module(&self, module_ref: ModuleRef) -> Option<NavigationTarget> {
        let module = self.0.def_map.module(module_ref)?;
        // Root modules have no declaration name to jump to, so they navigate to the owning file.
        // Named modules navigate to the `mod` declaration that introduced them.
        let (file_id, span) = match module.origin {
            ModuleOrigin::Root { file_id } => (file_id, None),
            ModuleOrigin::Inline {
                declaration_file,
                declaration_span,
            }
            | ModuleOrigin::OutOfLine {
                declaration_file,
                declaration_span,
                ..
            } => (declaration_file, Some(declaration_span)),
        };

        Some(NavigationTarget {
            target: module_ref.target,
            kind: NavigationTargetKind::Module,
            name: module
                .name
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "crate".to_string()),
            file_id,
            span,
        })
    }

    fn navigation_target_for_local_def(&self, local_def: LocalDefRef) -> Option<NavigationTarget> {
        let local_def_data = self.0.def_map.local_def(local_def)?;

        Some(NavigationTarget {
            target: local_def.target,
            kind: NavigationTargetKind::from_local_def_kind(local_def_data.kind),
            name: local_def_data.name.to_string(),
            file_id: local_def_data.file_id,
            // Goto should land on the declaration name rather than the whole item. The full item
            // span intentionally includes doc comments, which is useful for outline/hover-like
            // features but feels wrong as an editor cursor destination.
            span: Some(local_def_data.name_span.unwrap_or(local_def_data.span)),
        })
    }

    fn navigation_target_for_body_item(&self, item_ref: BodyItemRef) -> Option<NavigationTarget> {
        let item = self.body_data(item_ref.body)?.local_item(item_ref.item)?;

        Some(NavigationTarget {
            target: item_ref.body.target,
            kind: NavigationTargetKind::from_body_item_kind(item.kind),
            name: item.name.to_string(),
            file_id: item.source.file_id,
            span: Some(item.name_source.span),
        })
    }

    fn navigation_target_for_field(&self, field_ref: FieldRef) -> Option<NavigationTarget> {
        let field_data = self.0.semantic_ir.field_data(field_ref)?;
        let key = field_data.field.key.as_ref()?;
        Some(NavigationTarget {
            target: field_ref.owner.target,
            kind: NavigationTargetKind::Field,
            name: key.declaration_label(),
            file_id: field_data.file_id,
            span: Some(field_data.field.span),
        })
    }

    fn navigation_target_for_resolved_field(
        &self,
        field_ref: ResolvedFieldRef,
    ) -> Option<NavigationTarget> {
        match field_ref {
            ResolvedFieldRef::Semantic(field) => self.navigation_target_for_field(field),
            ResolvedFieldRef::BodyLocal(field) => self.navigation_target_for_local_field(field),
        }
    }

    fn navigation_target_for_local_field(
        &self,
        field_ref: BodyFieldRef,
    ) -> Option<NavigationTarget> {
        let field_data = self.0.body_ir.local_field_data(field_ref)?;
        let key = field_data.field.key.as_ref()?;

        Some(NavigationTarget {
            target: field_ref.item.body.target,
            kind: NavigationTargetKind::Field,
            name: key.declaration_label(),
            file_id: field_data.item.source.file_id,
            span: Some(field_data.field.span),
        })
    }

    fn navigation_target_for_function(
        &self,
        function_ref: FunctionRef,
    ) -> Option<NavigationTarget> {
        let function_data = self.0.semantic_ir.function_data(function_ref)?;

        Some(NavigationTarget {
            target: function_ref.target,
            kind: NavigationTargetKind::Function,
            name: function_data.name.to_string(),
            file_id: function_data.source.file_id,
            span: Some(function_data.name_span.unwrap_or(function_data.span)),
        })
    }

    fn navigation_target_for_resolved_function(
        &self,
        function_ref: ResolvedFunctionRef,
    ) -> Option<NavigationTarget> {
        match function_ref {
            ResolvedFunctionRef::Semantic(function) => {
                self.navigation_target_for_function(function)
            }
            ResolvedFunctionRef::BodyLocal(function) => {
                let data = self.0.body_ir.local_function_data(function)?;
                Some(NavigationTarget {
                    target: function.body.target,
                    kind: NavigationTargetKind::Function,
                    name: data.name.to_string(),
                    file_id: data.source.file_id,
                    span: Some(data.name_source.span),
                })
            }
        }
    }

    fn navigation_target_for_enum_variant(
        &self,
        variant_ref: EnumVariantRef,
    ) -> Option<NavigationTarget> {
        let data = self.0.semantic_ir.enum_variant_data(variant_ref)?;

        Some(NavigationTarget {
            target: variant_ref.target,
            kind: NavigationTargetKind::EnumVariant,
            name: data.variant.name.to_string(),
            file_id: data.file_id,
            span: Some(data.variant.name_span),
        })
    }

    fn navigation_target_for_trait(&self, trait_ref: TraitRef) -> Option<NavigationTarget> {
        let local_def = self.0.semantic_ir.trait_data(trait_ref)?.local_def;

        self.navigation_target_for_local_def(local_def)
    }

    fn navigation_target_for_type_def(&self, ty: TypeDefRef) -> Option<NavigationTarget> {
        let local_def = self.0.semantic_ir.local_def_for_type_def(ty)?;

        self.navigation_target_for_local_def(local_def)
    }

    fn navigation_targets_for_body_ty(&self, ty: &BodyTy) -> Vec<NavigationTarget> {
        let local_targets = ty
            .local_nominals()
            .iter()
            .filter_map(|ty| self.navigation_target_for_body_item(ty.item))
            .collect::<Vec<_>>();
        if !local_targets.is_empty() {
            return local_targets;
        }

        ty.nominal_tys()
            .iter()
            .filter_map(|ty| self.navigation_target_for_type_def(ty.def))
            .collect()
    }

    fn body_data(&self, body_ref: BodyRef) -> Option<&BodyData> {
        self.0.body_ir.body_data(body_ref)
    }
}

pub(super) struct SymbolResolver<'a, 'db>(&'a Analysis<'db>);

impl<'a, 'db> SymbolResolver<'a, 'db> {
    pub(super) fn new(analysis: &'a Analysis<'db>) -> Self {
        Self(analysis)
    }

    pub(super) fn resolve_symbol(&self, symbol: SymbolAt) -> Vec<NavigationTarget> {
        match symbol {
            SymbolAt::Binding { body, binding } => self
                .body_data(body)
                .and_then(|body_data| body_data.binding(binding))
                .map(|binding_data| vec![NavigationTarget::from_binding(body.target, binding_data)])
                .unwrap_or_default(),
            SymbolAt::BodyPath {
                body, scope, path, ..
            } => self.navigation_targets_for_body_type_path(body, scope, &path),
            SymbolAt::BodyValuePath {
                body, scope, path, ..
            } => self.navigation_targets_for_body_value_path(body, scope, &path),
            SymbolAt::Def { def, .. } => self
                .targets()
                .navigation_target_for_def(def)
                .into_iter()
                .collect(),
            SymbolAt::Expr { body, expr } => self
                .body_data(body)
                .and_then(|body_data| {
                    body_data.expr(expr).map(|expr_data| {
                        self.navigation_targets_for_resolution(body_data, &expr_data.resolution)
                    })
                })
                .unwrap_or_default(),
            SymbolAt::Field { field, .. } => self
                .targets()
                .navigation_target_for_field(field)
                .into_iter()
                .collect(),
            SymbolAt::Function { function, .. } => self
                .targets()
                .navigation_target_for_function(function)
                .into_iter()
                .collect(),
            SymbolAt::EnumVariant { variant, .. } => self
                .targets()
                .navigation_target_for_enum_variant(variant)
                .into_iter()
                .collect(),
            SymbolAt::LocalItem { item, .. } => self
                .targets()
                .navigation_target_for_body_item(item)
                .into_iter()
                .collect(),
            SymbolAt::TypePath { context, path, .. } => {
                self.navigation_targets_for_type_path(context, &path)
            }
            SymbolAt::UsePath { module, path, .. } => {
                self.navigation_targets_for_use_path(module, &path)
            }
            SymbolAt::Body { .. } => Vec::new(),
        }
    }

    fn body_data(&self, body_ref: BodyRef) -> Option<&BodyData> {
        self.0.body_ir.body_data(body_ref)
    }

    fn targets(&self) -> NavigationTargetResolver<'_, 'db> {
        NavigationTargetResolver::new(self.0)
    }

    fn navigation_targets_for_resolution(
        &self,
        body: &BodyData,
        resolution: &BodyResolution,
    ) -> Vec<NavigationTarget> {
        // Body resolution can point at lexical bindings, body-local items, or semantic items.
        // Normalize each source of identity into the same navigation payload.
        match resolution {
            BodyResolution::Local(binding) => body
                .binding(*binding)
                .map(|binding_data| NavigationTarget::from_binding(body.owner.target, binding_data))
                .into_iter()
                .collect(),
            BodyResolution::LocalItem(item) => self
                .targets()
                .navigation_target_for_body_item(*item)
                .into_iter()
                .collect(),
            BodyResolution::Item(defs) => defs
                .iter()
                .filter_map(|def| self.targets().navigation_target_for_def(*def))
                .collect(),
            BodyResolution::Field(fields) => fields
                .iter()
                .filter_map(|field| self.targets().navigation_target_for_resolved_field(*field))
                .collect(),
            BodyResolution::Function(functions) => functions
                .iter()
                .filter_map(|function| {
                    self.targets()
                        .navigation_target_for_resolved_function(*function)
                })
                .collect(),
            BodyResolution::EnumVariant(variants) => variants
                .iter()
                .filter_map(|variant| self.targets().navigation_target_for_enum_variant(*variant))
                .collect(),
            BodyResolution::Method(functions) => functions
                .iter()
                .filter_map(|function| {
                    self.targets()
                        .navigation_target_for_resolved_function(*function)
                })
                .collect(),
            BodyResolution::Unknown => Vec::new(),
        }
    }

    fn navigation_targets_for_type_path(
        &self,
        context: TypePathContext,
        path: &Path,
    ) -> Vec<NavigationTarget> {
        let resolution = self
            .0
            .semantic_ir
            .resolve_type_path(&self.0.def_map, context, path);

        let targets = self.navigation_targets_for_semantic_type_path_resolution(resolution);
        if targets.is_empty() {
            // A cursor can sit on a non-type prefix inside a type path, for example `helper` in
            // `helper::Tool`. Semantic type resolution correctly says "not a type", but editor
            // navigation should still use DefMap to jump to the module/crate prefix.
            self.navigation_targets_for_use_path(context.module, path)
        } else {
            targets
        }
    }

    fn navigation_targets_for_use_path(
        &self,
        module: ModuleRef,
        path: &Path,
    ) -> Vec<NavigationTarget> {
        self.0
            .def_map
            .resolve_path(module, path)
            .resolved
            .into_iter()
            .filter_map(|def| self.targets().navigation_target_for_def(def))
            .collect()
    }

    fn navigation_targets_for_body_type_path(
        &self,
        body_ref: BodyRef,
        scope: ScopeId,
        path: &Path,
    ) -> Vec<NavigationTarget> {
        let resolution = self.0.body_ir.resolve_type_path_in_scope(
            &self.0.def_map,
            &self.0.semantic_ir,
            body_ref,
            scope,
            path,
        );

        let targets = self.navigation_targets_for_body_type_path_resolution(resolution);
        if targets.is_empty() {
            // Body-local type resolution owns `Self` and local items. If that fails, the path may
            // still be a module/crate prefix selected by the cursor, so fall back to the owning
            // module's DefMap lookup.
            self.body_data(body_ref)
                .map(|body| self.navigation_targets_for_use_path(body.owner_module, path))
                .unwrap_or_default()
        } else {
            targets
        }
    }

    fn navigation_targets_for_body_value_path(
        &self,
        body_ref: BodyRef,
        scope: ScopeId,
        path: &Path,
    ) -> Vec<NavigationTarget> {
        let (resolution, _) = self.0.body_ir.resolve_value_path_in_scope(
            &self.0.def_map,
            &self.0.semantic_ir,
            body_ref,
            scope,
            path,
        );

        let Some(body_data) = self.body_data(body_ref) else {
            return Vec::new();
        };
        self.navigation_targets_for_resolution(body_data, &resolution)
    }

    fn navigation_targets_for_semantic_type_path_resolution(
        &self,
        resolution: SemanticTypePathResolution,
    ) -> Vec<NavigationTarget> {
        // Type paths can legally resolve to traits in bound positions, so goto-definition should
        // navigate to those traits instead of treating them as unknown.
        match resolution {
            SemanticTypePathResolution::SelfType(types)
            | SemanticTypePathResolution::TypeDefs(types) => types
                .into_iter()
                .filter_map(|ty| self.targets().navigation_target_for_type_def(ty))
                .collect(),
            SemanticTypePathResolution::Traits(traits) => traits
                .into_iter()
                .filter_map(|trait_ref| self.targets().navigation_target_for_trait(trait_ref))
                .collect(),
            SemanticTypePathResolution::Unknown => Vec::new(),
        }
    }

    fn navigation_targets_for_body_type_path_resolution(
        &self,
        resolution: BodyTypePathResolution,
    ) -> Vec<NavigationTarget> {
        match resolution {
            BodyTypePathResolution::BodyLocal(item) => self
                .targets()
                .navigation_target_for_body_item(item)
                .into_iter()
                .collect(),
            BodyTypePathResolution::SelfType(types) | BodyTypePathResolution::TypeDefs(types) => {
                types
                    .into_iter()
                    .filter_map(|ty| self.targets().navigation_target_for_type_def(ty))
                    .collect()
            }
            BodyTypePathResolution::Traits(traits) => traits
                .into_iter()
                .filter_map(|trait_ref| self.targets().navigation_target_for_trait(trait_ref))
                .collect(),
            BodyTypePathResolution::Unknown => Vec::new(),
        }
    }
}

pub(super) struct GotoResolver<'a, 'db>(&'a Analysis<'db>);

impl<'a, 'db> GotoResolver<'a, 'db> {
    pub(super) fn new(analysis: &'a Analysis<'db>) -> Self {
        Self(analysis)
    }

    pub(super) fn goto_definition(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Vec<NavigationTarget> {
        let Some(symbol) = self.0.symbol_at(target, file_id, offset) else {
            return Vec::new();
        };

        SymbolResolver::new(self.0).resolve_symbol(symbol)
    }
}

pub(super) struct TypeDefinitionResolver<'a, 'db>(&'a Analysis<'db>);

impl<'a, 'db> TypeDefinitionResolver<'a, 'db> {
    pub(super) fn new(analysis: &'a Analysis<'db>) -> Self {
        Self(analysis)
    }

    pub(super) fn goto_type_definition(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Vec<NavigationTarget> {
        let Some(ty) = super::ty::TypeResolver::new(self.0).type_at(target, file_id, offset) else {
            return Vec::new();
        };

        NavigationTargetResolver::new(self.0).navigation_targets_for_body_ty(&ty)
    }
}
