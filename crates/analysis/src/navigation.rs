use rg_body_ir::{BodyData, BodyItemRef, BodyRef, BodyResolution, BodyTypePathResolution, ScopeId};
use rg_def_map::{DefId, LocalDefRef, ModuleOrigin, ModuleRef, Path, TargetRef};
use rg_parse::FileId;
use rg_semantic_ir::{
    FieldRef, FunctionRef, SemanticTypePathResolution, TraitRef, TypeDefRef, TypePathContext,
};

use super::{
    Analysis,
    data::{NavigationTarget, NavigationTargetKind, SymbolAt},
};

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
                .map(|binding_data| vec![NavigationTarget::from_binding(binding_data)])
                .unwrap_or_default(),
            SymbolAt::BodyPath {
                body, scope, path, ..
            } => self.navigation_targets_for_body_type_path(body, scope, &path),
            SymbolAt::Def { def, .. } => self.navigation_target_for_def(def).into_iter().collect(),
            SymbolAt::Expr { body, expr } => self
                .body_data(body)
                .and_then(|body_data| {
                    body_data.expr(expr).map(|expr_data| {
                        self.navigation_targets_for_resolution(body_data, &expr_data.resolution)
                    })
                })
                .unwrap_or_default(),
            SymbolAt::Field { field, .. } => self
                .navigation_target_for_field(field)
                .into_iter()
                .collect(),
            SymbolAt::Function { function, .. } => self
                .navigation_target_for_function(function)
                .into_iter()
                .collect(),
            SymbolAt::LocalItem { item, .. } => self
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

    fn navigation_targets_for_resolution(
        &self,
        body: &BodyData,
        resolution: &BodyResolution,
    ) -> Vec<NavigationTarget> {
        match resolution {
            BodyResolution::Local(binding) => body
                .binding(*binding)
                .map(NavigationTarget::from_binding)
                .into_iter()
                .collect(),
            BodyResolution::LocalItem(item) => self
                .navigation_target_for_body_item(*item)
                .into_iter()
                .collect(),
            BodyResolution::Item(defs) => defs
                .iter()
                .filter_map(|def| self.navigation_target_for_def(*def))
                .collect(),
            BodyResolution::Field(fields) => fields
                .iter()
                .filter_map(|field| self.navigation_target_for_field(*field))
                .collect(),
            BodyResolution::Unknown => Vec::new(),
        }
    }

    fn navigation_target_for_def(&self, def: DefId) -> Option<NavigationTarget> {
        match def {
            DefId::Module(module_ref) => self.navigation_target_for_module(module_ref),
            DefId::Local(local_def) => self.navigation_target_for_local_def(local_def),
        }
    }

    fn navigation_target_for_module(&self, module_ref: ModuleRef) -> Option<NavigationTarget> {
        let module = self.0.def_map.module(module_ref)?;
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
            kind: NavigationTargetKind::Module,
            name: module.name.clone().unwrap_or_else(|| "crate".to_string()),
            file_id,
            span,
        })
    }

    fn navigation_target_for_local_def(&self, local_def: LocalDefRef) -> Option<NavigationTarget> {
        let local_def_data = self.0.def_map.local_def(local_def)?;

        Some(NavigationTarget {
            kind: NavigationTargetKind::from_local_def_kind(local_def_data.kind),
            name: local_def_data.name.clone(),
            file_id: local_def_data.file_id,
            span: Some(local_def_data.span),
        })
    }

    fn navigation_target_for_body_item(&self, item_ref: BodyItemRef) -> Option<NavigationTarget> {
        let item = self.body_data(item_ref.body)?.local_item(item_ref.item)?;

        Some(NavigationTarget {
            kind: NavigationTargetKind::from_body_item_kind(item.kind),
            name: item.name.clone(),
            file_id: item.source.file_id,
            span: Some(item.source.span),
        })
    }

    fn navigation_target_for_field(&self, field_ref: FieldRef) -> Option<NavigationTarget> {
        let field_data = self.0.semantic_ir.field_data(field_ref)?;
        let key = field_data.field.key.as_ref()?;
        Some(NavigationTarget {
            kind: NavigationTargetKind::Field,
            name: key.declaration_label(),
            file_id: field_data.file_id,
            span: Some(field_data.field.span),
        })
    }

    fn navigation_target_for_function(
        &self,
        function_ref: FunctionRef,
    ) -> Option<NavigationTarget> {
        let function_data = self.0.semantic_ir.function_data(function_ref)?;

        Some(NavigationTarget {
            kind: NavigationTargetKind::Function,
            name: function_data.name.clone(),
            file_id: function_data.source.file_id,
            span: Some(function_data.span),
        })
    }

    fn navigation_targets_for_type_path(
        &self,
        context: TypePathContext,
        path: &Path,
    ) -> Vec<NavigationTarget> {
        let resolution = self
            .0
            .semantic_ir
            .resolve_type_path(self.0.def_map, context, path);

        self.navigation_targets_for_semantic_type_path_resolution(resolution)
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
            .filter_map(|def| self.navigation_target_for_def(def))
            .collect()
    }

    fn navigation_targets_for_body_type_path(
        &self,
        body_ref: BodyRef,
        scope: ScopeId,
        path: &Path,
    ) -> Vec<NavigationTarget> {
        let resolution = self.0.body_ir.resolve_type_path_in_scope(
            self.0.def_map,
            self.0.semantic_ir,
            body_ref,
            scope,
            path,
        );

        self.navigation_targets_for_body_type_path_resolution(resolution)
    }

    fn navigation_targets_for_semantic_type_path_resolution(
        &self,
        resolution: SemanticTypePathResolution,
    ) -> Vec<NavigationTarget> {
        match resolution {
            SemanticTypePathResolution::SelfType(types)
            | SemanticTypePathResolution::TypeDefs(types) => types
                .into_iter()
                .filter_map(|ty| self.navigation_target_for_type_def(ty))
                .collect(),
            SemanticTypePathResolution::Traits(traits) => traits
                .into_iter()
                .filter_map(|trait_ref| self.navigation_target_for_trait(trait_ref))
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
                .navigation_target_for_body_item(item)
                .into_iter()
                .collect(),
            BodyTypePathResolution::SelfType(types) | BodyTypePathResolution::TypeDefs(types) => {
                types
                    .into_iter()
                    .filter_map(|ty| self.navigation_target_for_type_def(ty))
                    .collect()
            }
            BodyTypePathResolution::Traits(traits) => traits
                .into_iter()
                .filter_map(|trait_ref| self.navigation_target_for_trait(trait_ref))
                .collect(),
            BodyTypePathResolution::Unknown => Vec::new(),
        }
    }

    fn navigation_target_for_trait(&self, trait_ref: TraitRef) -> Option<NavigationTarget> {
        let local_def = self.0.semantic_ir.trait_data(trait_ref)?.local_def;

        self.navigation_target_for_local_def(local_def)
    }

    fn navigation_target_for_type_def(&self, ty: TypeDefRef) -> Option<NavigationTarget> {
        let local_def = self.0.semantic_ir.local_def_for_type_def(ty)?;

        self.navigation_target_for_local_def(local_def)
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
