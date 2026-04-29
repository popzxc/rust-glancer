//! Resolves analysis cursor symbols into semantic/body identities.
//!
//! Navigation and hover need different presentation payloads, but they start from the same core
//! question: "what declaration-like entity does this cursor symbol denote?"

use rg_body_ir::{
    BodyData, BodyItemRef, BodyRef, BodyResolution, BodyTypePathResolution, ResolvedFieldRef,
    ResolvedFunctionRef, ScopeId,
};
use rg_def_map::{DefId, LocalDefRef, ModuleRef, Path};
use rg_semantic_ir::{
    ConstRef, EnumVariantRef, FunctionRef, ItemId, SemanticTypePathResolution, StaticRef, TraitRef,
    TypeAliasRef, TypeDefId, TypeDefRef,
};

use super::{Analysis, data::SymbolAt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ResolvedEntity {
    Module {
        module: ModuleRef,
        display_name: Option<String>,
    },
    TypeDef(TypeDefRef),
    Trait(TraitRef),
    Function(ResolvedFunctionRef),
    Field(ResolvedFieldRef),
    EnumVariant(EnumVariantRef),
    TypeAlias(TypeAliasRef),
    Const(ConstRef),
    Static(StaticRef),
    LocalBinding {
        body: BodyRef,
        binding: rg_body_ir::BindingId,
    },
    LocalItem(BodyItemRef),
    LocalDef(LocalDefRef),
}

pub(super) struct EntityResolver<'a, 'db>(&'a Analysis<'db>);

impl<'a, 'db> EntityResolver<'a, 'db> {
    pub(super) fn new(analysis: &'a Analysis<'db>) -> Self {
        Self(analysis)
    }

    pub(super) fn entities_for_symbol(&self, symbol: SymbolAt) -> Vec<ResolvedEntity> {
        match symbol {
            SymbolAt::Body { .. } => Vec::new(),
            SymbolAt::Binding { body, binding } => {
                vec![ResolvedEntity::LocalBinding { body, binding }]
            }
            SymbolAt::BodyPath {
                body, scope, path, ..
            } => self.entities_for_body_type_path(body, scope, &path),
            SymbolAt::BodyValuePath {
                body, scope, path, ..
            } => self.entities_for_body_value_path(body, scope, &path),
            SymbolAt::Def { def, .. } => self.entities_for_def(def),
            SymbolAt::Expr { body, expr } => self
                .body_data(body)
                .and_then(|body_data| {
                    body_data.expr(expr).map(|expr_data| {
                        self.entities_for_body_resolution(Some(body), &expr_data.resolution, None)
                    })
                })
                .unwrap_or_default(),
            SymbolAt::Field { field, .. } => {
                vec![ResolvedEntity::Field(ResolvedFieldRef::Semantic(field))]
            }
            SymbolAt::Function { function, .. } => vec![ResolvedEntity::Function(
                ResolvedFunctionRef::Semantic(function),
            )],
            SymbolAt::EnumVariant { variant, .. } => vec![ResolvedEntity::EnumVariant(variant)],
            SymbolAt::LocalItem { item, .. } => vec![ResolvedEntity::LocalItem(item)],
            SymbolAt::TypePath { context, path, .. } => {
                let resolution =
                    self.0
                        .semantic_ir
                        .resolve_type_path(self.0.def_map, context, &path);
                let entities = self.entities_for_semantic_type_path_resolution(resolution);
                if entities.is_empty() {
                    self.entities_for_use_path(context.module, &path)
                } else {
                    entities
                }
            }
            SymbolAt::UsePath { module, path, .. } => self.entities_for_use_path(module, &path),
        }
    }

    fn entities_for_def(&self, def: DefId) -> Vec<ResolvedEntity> {
        self.entities_for_def_with_module_display(def, None)
    }

    fn entities_for_def_with_module_display(
        &self,
        def: DefId,
        display_name: Option<String>,
    ) -> Vec<ResolvedEntity> {
        match def {
            DefId::Module(module) => vec![ResolvedEntity::Module {
                module,
                display_name,
            }],
            DefId::Local(local_def) => vec![
                self.entity_for_local_def(local_def)
                    .unwrap_or(ResolvedEntity::LocalDef(local_def)),
            ],
        }
    }

    fn entity_for_local_def(&self, local_def: LocalDefRef) -> Option<ResolvedEntity> {
        let item = self
            .0
            .semantic_ir
            .target_ir(local_def.target)?
            .item_for_local_def(local_def.local_def)?;

        let entity = match item {
            ItemId::Struct(id) => ResolvedEntity::TypeDef(TypeDefRef {
                target: local_def.target,
                id: TypeDefId::Struct(id),
            }),
            ItemId::Union(id) => ResolvedEntity::TypeDef(TypeDefRef {
                target: local_def.target,
                id: TypeDefId::Union(id),
            }),
            ItemId::Enum(id) => ResolvedEntity::TypeDef(TypeDefRef {
                target: local_def.target,
                id: TypeDefId::Enum(id),
            }),
            ItemId::Trait(id) => ResolvedEntity::Trait(TraitRef {
                target: local_def.target,
                id,
            }),
            ItemId::Function(id) => {
                ResolvedEntity::Function(ResolvedFunctionRef::Semantic(FunctionRef {
                    target: local_def.target,
                    id,
                }))
            }
            ItemId::TypeAlias(id) => ResolvedEntity::TypeAlias(TypeAliasRef {
                target: local_def.target,
                id,
            }),
            ItemId::Const(id) => ResolvedEntity::Const(ConstRef {
                target: local_def.target,
                id,
            }),
            ItemId::Static(id) => ResolvedEntity::Static(StaticRef {
                target: local_def.target,
                id,
            }),
        };
        Some(entity)
    }

    fn entities_for_use_path(&self, module: ModuleRef, path: &Path) -> Vec<ResolvedEntity> {
        let display_name = path.last_segment_label();
        self.0
            .def_map
            .resolve_path(module, path)
            .resolved
            .into_iter()
            .flat_map(|def| self.entities_for_def_with_module_display(def, display_name.clone()))
            .collect()
    }

    fn entities_for_body_type_path(
        &self,
        body_ref: BodyRef,
        scope: ScopeId,
        path: &Path,
    ) -> Vec<ResolvedEntity> {
        let resolution = self.0.body_ir.resolve_type_path_in_scope(
            self.0.def_map,
            self.0.semantic_ir,
            body_ref,
            scope,
            path,
        );

        let entities = self.entities_for_body_type_path_resolution(resolution);
        if !entities.is_empty() {
            return entities;
        }

        self.body_data(body_ref)
            .map(|body| self.entities_for_use_path(body.owner_module, path))
            .unwrap_or_default()
    }

    fn entities_for_body_value_path(
        &self,
        body_ref: BodyRef,
        scope: ScopeId,
        path: &Path,
    ) -> Vec<ResolvedEntity> {
        let (resolution, _) = self.0.body_ir.resolve_value_path_in_scope(
            self.0.def_map,
            self.0.semantic_ir,
            body_ref,
            scope,
            path,
        );
        self.entities_for_body_resolution(Some(body_ref), &resolution, path.last_segment_label())
    }

    fn entities_for_body_resolution(
        &self,
        body_ref: Option<BodyRef>,
        resolution: &BodyResolution,
        module_display_name: Option<String>,
    ) -> Vec<ResolvedEntity> {
        match resolution {
            BodyResolution::Local(binding) => body_ref
                .map(|body| ResolvedEntity::LocalBinding {
                    body,
                    binding: *binding,
                })
                .into_iter()
                .collect(),
            BodyResolution::LocalItem(item) => vec![ResolvedEntity::LocalItem(*item)],
            BodyResolution::Item(defs) => defs
                .iter()
                .flat_map(|def| {
                    self.entities_for_def_with_module_display(*def, module_display_name.clone())
                })
                .collect(),
            BodyResolution::Field(fields) => {
                fields.iter().copied().map(ResolvedEntity::Field).collect()
            }
            BodyResolution::Function(functions) | BodyResolution::Method(functions) => functions
                .iter()
                .copied()
                .map(ResolvedEntity::Function)
                .collect(),
            BodyResolution::EnumVariant(variants) => variants
                .iter()
                .copied()
                .map(ResolvedEntity::EnumVariant)
                .collect(),
            BodyResolution::Unknown => Vec::new(),
        }
    }

    fn entities_for_semantic_type_path_resolution(
        &self,
        resolution: SemanticTypePathResolution,
    ) -> Vec<ResolvedEntity> {
        match resolution {
            SemanticTypePathResolution::SelfType(types)
            | SemanticTypePathResolution::TypeDefs(types) => {
                types.into_iter().map(ResolvedEntity::TypeDef).collect()
            }
            SemanticTypePathResolution::Traits(traits) => {
                traits.into_iter().map(ResolvedEntity::Trait).collect()
            }
            SemanticTypePathResolution::Unknown => Vec::new(),
        }
    }

    fn entities_for_body_type_path_resolution(
        &self,
        resolution: BodyTypePathResolution,
    ) -> Vec<ResolvedEntity> {
        match resolution {
            BodyTypePathResolution::BodyLocal(item) => vec![ResolvedEntity::LocalItem(item)],
            BodyTypePathResolution::SelfType(types) | BodyTypePathResolution::TypeDefs(types) => {
                types.into_iter().map(ResolvedEntity::TypeDef).collect()
            }
            BodyTypePathResolution::Traits(traits) => {
                traits.into_iter().map(ResolvedEntity::Trait).collect()
            }
            BodyTypePathResolution::Unknown => Vec::new(),
        }
    }

    fn body_data(&self, body_ref: BodyRef) -> Option<&BodyData> {
        self.0.body_ir.body_data(body_ref)
    }
}
