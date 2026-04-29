//! Builds hover payloads from resolved analysis entities.

use rg_body_ir::{BodyTy, ResolvedFieldRef, ResolvedFunctionRef};
use rg_def_map::{LocalDefRef, ModuleRef, TargetRef};
use rg_parse::{FileId, Span};
use rg_semantic_ir::{
    ConstRef, Documentation, StaticRef, TraitRef, TypeAliasRef, TypeDefId, TypeDefRef,
};

use super::{
    Analysis,
    data::{HoverBlock, HoverInfo, SymbolAt, SymbolKind},
    entity::{EntityResolver, ResolvedEntity},
    path_render::PathRenderer,
    signature::SignatureRenderer,
    ty::TypeResolver,
};

pub(super) struct HoverResolver<'a, 'db>(&'a Analysis<'db>);

impl<'a, 'db> HoverResolver<'a, 'db> {
    pub(super) fn new(analysis: &'a Analysis<'db>) -> Self {
        Self(analysis)
    }

    pub(super) fn hover(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Option<HoverInfo> {
        let symbol = self.0.symbol_at(target, file_id, offset)?;
        let range = self.symbol_range(&symbol);
        let entities = EntityResolver::new(self.0).entities_for_symbol(symbol.clone());
        let mut blocks = Vec::new();

        for entity in entities {
            let Some(block) = self.hover_for_entity(entity) else {
                continue;
            };
            if !blocks.contains(&block) {
                blocks.push(block);
            }
        }

        if blocks.is_empty()
            && let Some(block) = TypeResolver::new(self.0)
                .type_at(target, file_id, offset)
                .and_then(|ty| self.hover_for_ty(&ty))
        {
            blocks.push(block);
        }

        (!blocks.is_empty()).then_some(HoverInfo { range, blocks })
    }

    fn hover_for_entity(&self, entity: ResolvedEntity) -> Option<HoverBlock> {
        match entity {
            ResolvedEntity::Module {
                module,
                display_name,
            } => self.hover_for_module(module, display_name),
            ResolvedEntity::TypeDef(ty) => self.hover_for_type_def(ty),
            ResolvedEntity::Trait(trait_ref) => self.hover_for_trait(trait_ref),
            ResolvedEntity::Function(function) => self.hover_for_function(function),
            ResolvedEntity::Field(field) => self.hover_for_field(field),
            ResolvedEntity::EnumVariant(variant) => {
                let data = self.0.semantic_ir.enum_variant_data(variant)?;
                Some(HoverBlock {
                    kind: SymbolKind::EnumVariant,
                    path: PathRenderer::new(self.0).enum_variant_path(variant),
                    signature: Some(SignatureRenderer::new(self.0).enum_variant_signature(data)),
                    ty: None,
                    docs: docs_text(data.variant.docs.as_ref()),
                })
            }
            ResolvedEntity::TypeAlias(type_alias_ref) => self.hover_for_type_alias(type_alias_ref),
            ResolvedEntity::Const(const_ref) => self.hover_for_const(const_ref),
            ResolvedEntity::Static(static_ref) => self.hover_for_static(static_ref),
            ResolvedEntity::LocalBinding { body, binding } => {
                let body_data = self.0.body_ir.body_data(body)?;
                let binding_data = body_data.binding(binding)?;
                Some(HoverBlock {
                    kind: SymbolKind::Variable,
                    path: None,
                    signature: Some(SignatureRenderer::new(self.0).binding_signature(binding_data)),
                    ty: None,
                    docs: None,
                })
            }
            ResolvedEntity::LocalItem(item_ref) => {
                let body_data = self.0.body_ir.body_data(item_ref.body)?;
                let item = body_data.local_item(item_ref.item)?;
                Some(HoverBlock {
                    kind: SymbolKind::from_body_item_kind(item.kind),
                    path: None,
                    signature: Some(SignatureRenderer::new(self.0).local_item_signature(item)),
                    ty: None,
                    docs: docs_text(item.docs.as_ref()),
                })
            }
            ResolvedEntity::LocalDef(local_def) => self.hover_for_local_def(local_def),
        }
    }

    fn hover_for_type_def(&self, ty: TypeDefRef) -> Option<HoverBlock> {
        let target_ir = self.0.semantic_ir.target_ir(ty.target)?;
        let renderer = SignatureRenderer::new(self.0);
        let path = PathRenderer::new(self.0).type_def_path(ty);
        match ty.id {
            TypeDefId::Struct(id) => {
                let data = target_ir.items().struct_data(id)?;
                Some(HoverBlock {
                    kind: SymbolKind::Struct,
                    path,
                    signature: Some(renderer.struct_signature(data)),
                    ty: None,
                    docs: docs_text(data.docs.as_ref()),
                })
            }
            TypeDefId::Enum(id) => {
                let data = target_ir.items().enum_data(id)?;
                Some(HoverBlock {
                    kind: SymbolKind::Enum,
                    path,
                    signature: Some(renderer.enum_signature(data)),
                    ty: None,
                    docs: docs_text(data.docs.as_ref()),
                })
            }
            TypeDefId::Union(id) => {
                let data = target_ir.items().union_data(id)?;
                Some(HoverBlock {
                    kind: SymbolKind::Union,
                    path,
                    signature: Some(renderer.union_signature(data)),
                    ty: None,
                    docs: docs_text(data.docs.as_ref()),
                })
            }
        }
    }

    fn hover_for_trait(&self, trait_ref: TraitRef) -> Option<HoverBlock> {
        let data = self.0.semantic_ir.trait_data(trait_ref)?;
        Some(HoverBlock {
            kind: SymbolKind::Trait,
            path: PathRenderer::new(self.0).trait_path(trait_ref),
            signature: Some(SignatureRenderer::new(self.0).trait_signature(data)),
            ty: None,
            docs: docs_text(data.docs.as_ref()),
        })
    }

    fn hover_for_function(&self, function: ResolvedFunctionRef) -> Option<HoverBlock> {
        match function {
            ResolvedFunctionRef::Semantic(function_ref) => {
                let data = self.0.semantic_ir.function_data(function_ref)?;
                Some(HoverBlock {
                    kind: function_kind(data.owner),
                    path: PathRenderer::new(self.0).function_path(function_ref),
                    signature: Some(SignatureRenderer::new(self.0).function_signature(data)),
                    ty: None,
                    docs: docs_text(data.docs.as_ref()),
                })
            }
            ResolvedFunctionRef::BodyLocal(function_ref) => {
                let data = self.0.body_ir.local_function_data(function_ref)?;
                Some(HoverBlock {
                    kind: SymbolKind::Method,
                    path: None,
                    signature: Some(SignatureRenderer::new(self.0).local_function_signature(data)),
                    ty: None,
                    docs: docs_text(data.docs.as_ref()),
                })
            }
        }
    }

    fn hover_for_field(&self, field: ResolvedFieldRef) -> Option<HoverBlock> {
        match field {
            ResolvedFieldRef::Semantic(field_ref) => {
                let data = self.0.semantic_ir.field_data(field_ref)?;
                Some(HoverBlock {
                    kind: SymbolKind::Field,
                    path: PathRenderer::new(self.0).type_def_path(field_ref.owner),
                    signature: SignatureRenderer::new(self.0).field_signature(data),
                    ty: None,
                    docs: docs_text(data.field.docs.as_ref()),
                })
            }
            ResolvedFieldRef::BodyLocal(field_ref) => {
                let data = self.0.body_ir.local_field_data(field_ref)?;
                Some(HoverBlock {
                    kind: SymbolKind::Field,
                    path: None,
                    signature: SignatureRenderer::new(self.0).local_field_signature(data),
                    ty: None,
                    docs: docs_text(data.field.docs.as_ref()),
                })
            }
        }
    }

    fn hover_for_type_alias(&self, type_alias_ref: TypeAliasRef) -> Option<HoverBlock> {
        let data = self.0.semantic_ir.type_alias_data(type_alias_ref)?;
        Some(HoverBlock {
            kind: SymbolKind::TypeAlias,
            path: PathRenderer::new(self.0).type_alias_path(type_alias_ref),
            signature: Some(SignatureRenderer::new(self.0).type_alias_signature(data)),
            ty: None,
            docs: docs_text(data.docs.as_ref()),
        })
    }

    fn hover_for_const(&self, const_ref: ConstRef) -> Option<HoverBlock> {
        let data = self.0.semantic_ir.const_data(const_ref)?;
        Some(HoverBlock {
            kind: SymbolKind::Const,
            path: PathRenderer::new(self.0).const_path(const_ref),
            signature: Some(SignatureRenderer::new(self.0).const_signature(data)),
            ty: None,
            docs: docs_text(data.docs.as_ref()),
        })
    }

    fn hover_for_static(&self, static_ref: StaticRef) -> Option<HoverBlock> {
        let data = self.0.semantic_ir.static_data(static_ref)?;
        Some(HoverBlock {
            kind: SymbolKind::Static,
            path: PathRenderer::new(self.0).static_path(static_ref),
            signature: Some(SignatureRenderer::new(self.0).static_signature(data)),
            ty: None,
            docs: docs_text(data.docs.as_ref()),
        })
    }

    fn hover_for_module(
        &self,
        module_ref: ModuleRef,
        display_name: Option<String>,
    ) -> Option<HoverBlock> {
        let module = self.0.def_map.module(module_ref)?;
        let name = display_name
            .as_deref()
            .or(module.name.as_deref())
            .unwrap_or("crate");
        Some(HoverBlock {
            kind: SymbolKind::Module,
            path: PathRenderer::new(self.0).module_path(module_ref),
            signature: Some(format!("mod {name}")),
            ty: None,
            docs: docs_text(module.docs.as_ref()),
        })
    }

    fn hover_for_local_def(&self, local_def: LocalDefRef) -> Option<HoverBlock> {
        let data = self.0.def_map.local_def(local_def)?;
        Some(HoverBlock {
            kind: SymbolKind::from_local_def_kind(data.kind),
            path: PathRenderer::new(self.0)
                .module_path(ModuleRef {
                    target: local_def.target,
                    module: data.module,
                })
                .map(|module| format!("{module}::{}", data.name)),
            signature: Some(format!("{} {}", data.kind, data.name)),
            ty: None,
            docs: None,
        })
    }

    fn hover_for_ty(&self, ty: &BodyTy) -> Option<HoverBlock> {
        let signature = SignatureRenderer::new(self.0).ty_signature(ty)?;
        Some(HoverBlock {
            kind: SymbolKind::TypeAlias,
            path: None,
            signature: None,
            ty: Some(signature),
            docs: None,
        })
    }

    fn symbol_range(&self, symbol: &SymbolAt) -> Option<Span> {
        match symbol {
            SymbolAt::Body { body } => self
                .0
                .body_ir
                .body_data(*body)
                .map(|body_data| body_data.source.span),
            SymbolAt::Binding { body, binding } => self
                .0
                .body_ir
                .body_data(*body)?
                .binding(*binding)
                .map(|binding| binding.source.span),
            SymbolAt::BodyPath { span, .. }
            | SymbolAt::BodyValuePath { span, .. }
            | SymbolAt::Def { span, .. }
            | SymbolAt::Field { span, .. }
            | SymbolAt::Function { span, .. }
            | SymbolAt::EnumVariant { span, .. }
            | SymbolAt::LocalItem { span, .. }
            | SymbolAt::TypePath { span, .. }
            | SymbolAt::UsePath { span, .. } => Some(*span),
            SymbolAt::Expr { body, expr } => self
                .0
                .body_ir
                .body_data(*body)?
                .expr(*expr)
                .map(|expr| expr.source.span),
        }
    }
}

fn function_kind(owner: rg_semantic_ir::ItemOwner) -> SymbolKind {
    match owner {
        rg_semantic_ir::ItemOwner::Module(_) => SymbolKind::Function,
        rg_semantic_ir::ItemOwner::Trait(_) | rg_semantic_ir::ItemOwner::Impl(_) => {
            SymbolKind::Method
        }
    }
}

fn docs_text(docs: Option<&Documentation>) -> Option<String> {
    docs.map(|docs| docs.as_str().to_string())
}
