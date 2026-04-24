use anyhow::Context as _;

use crate::{
    def_map::{
        DefMap, DefMapDb, LocalDefId, LocalDefRef, LocalImplId, LocalImplRef, ModuleRef, TargetRef,
    },
    item_tree::{
        ConstItem, ImplItem, ItemKind, ItemNode, ItemTreeDb, ItemTreeRef,
        Package as ItemTreePackage, StaticItem, TraitItem, TypeAliasItem,
    },
    parse::TargetId,
};

use super::{
    data::{
        ConstData, EnumData, FunctionData, ImplData, PackageIr, SemanticIrDb, StaticData,
        StructData, TargetIr, TraitData, TypeAliasData, UnionData,
    },
    ids::{
        AssocItemId, ConstId, FunctionId, ImplId, ItemId, ItemOwner, StaticId, TraitId, TypeAliasId,
    },
};

pub(super) fn build_db(item_tree: &ItemTreeDb, def_map: &DefMapDb) -> anyhow::Result<SemanticIrDb> {
    let mut packages = Vec::with_capacity(def_map.packages().len());

    for (package_idx, package) in def_map.packages().iter().enumerate() {
        let item_tree_package = item_tree.package(package_idx).with_context(|| {
            format!("while attempting to fetch item tree package {package_idx}")
        })?;
        let mut targets = Vec::with_capacity(package.targets().len());

        for (target_idx, target_def_map) in package.targets().iter().enumerate() {
            let target_ref = TargetRef {
                package: crate::def_map::PackageSlot(package_idx),
                target: TargetId(target_idx),
            };
            targets.push(
                TargetLowering::new(item_tree_package, target_ref, target_def_map)
                    .lower()
                    .with_context(|| {
                        format!("while attempting to lower semantic IR for target {target_idx}")
                    })?,
            );
        }

        packages.push(PackageIr::new(targets));
    }

    Ok(SemanticIrDb::new(packages))
}

struct TargetLowering<'a> {
    item_tree: &'a ItemTreePackage,
    target: TargetRef,
    def_map: &'a DefMap,
    target_ir: TargetIr,
}

impl<'a> TargetLowering<'a> {
    fn new(item_tree: &'a ItemTreePackage, target: TargetRef, def_map: &'a DefMap) -> Self {
        Self {
            item_tree,
            target,
            def_map,
            target_ir: TargetIr::new(def_map.local_defs().len()),
        }
    }

    fn lower(mut self) -> anyhow::Result<TargetIr> {
        for (local_def_idx, local_def) in self.def_map.local_defs().iter().enumerate() {
            let local_def_ref = LocalDefRef {
                target: self.target,
                local_def: LocalDefId(local_def_idx),
            };
            let item = self.item(local_def.source)?;
            let owner = ModuleRef {
                target: self.target,
                module: local_def.module,
            };

            if let Some(item_id) =
                self.lower_local_item(local_def_ref, local_def.source, owner, item)
            {
                self.target_ir
                    .set_local_item(LocalDefId(local_def_idx), item_id);
            }
        }

        for (local_impl_idx, local_impl) in self.def_map.local_impls().iter().enumerate() {
            let local_impl_ref = LocalImplRef {
                target: self.target,
                local_impl: LocalImplId(local_impl_idx),
            };
            let item = self.item(local_impl.source)?;
            let owner = ModuleRef {
                target: self.target,
                module: local_impl.module,
            };

            if let ItemKind::Impl(impl_item) = &item.kind {
                let impl_id = self.lower_impl(local_impl_ref, local_impl.source, owner, impl_item);
                self.target_ir.push_local_impl(impl_id);
            }
        }

        Ok(self.target_ir)
    }

    fn item(&self, item_ref: ItemTreeRef) -> anyhow::Result<&'a ItemNode> {
        self.item_tree.item(item_ref).with_context(|| {
            format!(
                "while attempting to fetch item-tree node {:?} in {:?}",
                item_ref.item, item_ref.file_id
            )
        })
    }

    fn lower_local_item(
        &mut self,
        local_def: LocalDefRef,
        source: ItemTreeRef,
        owner: ModuleRef,
        item: &ItemNode,
    ) -> Option<ItemId> {
        match &item.kind {
            ItemKind::Const(const_item) => Some(ItemId::Const(self.lower_const(
                Some(local_def),
                source,
                ItemOwner::Module(owner),
                item,
                const_item,
            ))),
            ItemKind::Enum(enum_item) => {
                let id = self.target_ir.items_mut().alloc_enum(EnumData {
                    local_def,
                    source,
                    owner,
                    name: item.name.clone()?,
                    visibility: item.visibility.clone(),
                    generics: enum_item.generics.clone(),
                    variants: enum_item.variants.clone(),
                });
                Some(ItemId::Enum(id))
            }
            ItemKind::Function(function_item) => Some(ItemId::Function(self.lower_function(
                Some(local_def),
                source,
                ItemOwner::Module(owner),
                item,
                function_item,
            ))),
            ItemKind::Static(static_item) => Some(ItemId::Static(self.lower_static(
                local_def,
                source,
                owner,
                item,
                static_item,
            ))),
            ItemKind::Struct(struct_item) => {
                let id = self.target_ir.items_mut().alloc_struct(StructData {
                    local_def,
                    source,
                    owner,
                    name: item.name.clone()?,
                    visibility: item.visibility.clone(),
                    generics: struct_item.generics.clone(),
                    fields: struct_item.fields.clone(),
                });
                Some(ItemId::Struct(id))
            }
            ItemKind::Trait(trait_item) => Some(ItemId::Trait(
                self.lower_trait(local_def, source, owner, item, trait_item),
            )),
            ItemKind::TypeAlias(type_alias) => Some(ItemId::TypeAlias(self.lower_type_alias(
                Some(local_def),
                source,
                ItemOwner::Module(owner),
                item,
                type_alias,
            ))),
            ItemKind::Union(union_item) => {
                let id = self.target_ir.items_mut().alloc_union(UnionData {
                    local_def,
                    source,
                    owner,
                    name: item.name.clone()?,
                    visibility: item.visibility.clone(),
                    generics: union_item.generics.clone(),
                    fields: union_item.fields.clone(),
                });
                Some(ItemId::Union(id))
            }
            _ => None,
        }
    }

    fn lower_trait(
        &mut self,
        local_def: LocalDefRef,
        source: ItemTreeRef,
        owner: ModuleRef,
        item: &ItemNode,
        trait_item: &TraitItem,
    ) -> TraitId {
        let trait_id = self.target_ir.items_mut().alloc_trait(TraitData {
            local_def,
            source,
            owner,
            name: item.name.clone().unwrap_or_else(|| "<missing>".to_string()),
            visibility: item.visibility.clone(),
            generics: trait_item.generics.clone(),
            super_traits: trait_item.super_traits.clone(),
            items: Vec::new(),
            is_unsafe: trait_item.is_unsafe,
        });
        let assoc_items = self.lower_assoc_items(
            source.file_id,
            &trait_item.items,
            ItemOwner::Trait(trait_id),
        );
        self.target_ir.items_mut().traits[trait_id.0].items = assoc_items;
        trait_id
    }

    fn lower_impl(
        &mut self,
        local_impl: LocalImplRef,
        source: ItemTreeRef,
        owner: ModuleRef,
        impl_item: &ImplItem,
    ) -> ImplId {
        let impl_id = self.target_ir.items_mut().alloc_impl(ImplData {
            local_impl,
            source,
            owner,
            generics: impl_item.generics.clone(),
            trait_ref: impl_item.trait_ref.clone(),
            self_ty: impl_item.self_ty.clone(),
            items: Vec::new(),
            is_unsafe: impl_item.is_unsafe,
        });
        let assoc_items =
            self.lower_assoc_items(source.file_id, &impl_item.items, ItemOwner::Impl(impl_id));
        self.target_ir.items_mut().impls[impl_id.0].items = assoc_items;
        impl_id
    }

    fn lower_assoc_items(
        &mut self,
        file_id: crate::parse::FileId,
        item_ids: &[crate::item_tree::ItemTreeId],
        owner: ItemOwner,
    ) -> Vec<AssocItemId> {
        let mut assoc_items = Vec::new();

        for item_id in item_ids {
            let source = ItemTreeRef {
                file_id,
                item: *item_id,
            };
            let Some(item) = self.item_tree.item(source) else {
                continue;
            };

            match &item.kind {
                ItemKind::Const(const_item) => {
                    assoc_items.push(AssocItemId::Const(
                        self.lower_const(None, source, owner, item, const_item),
                    ));
                }
                ItemKind::Function(function_item) => {
                    assoc_items.push(AssocItemId::Function(self.lower_function(
                        None,
                        source,
                        owner,
                        item,
                        function_item,
                    )));
                }
                ItemKind::TypeAlias(type_alias) => {
                    assoc_items.push(AssocItemId::TypeAlias(
                        self.lower_type_alias(None, source, owner, item, type_alias),
                    ));
                }
                _ => {}
            }
        }

        assoc_items
    }

    fn lower_function(
        &mut self,
        local_def: Option<LocalDefRef>,
        source: ItemTreeRef,
        owner: ItemOwner,
        item: &ItemNode,
        declaration: &crate::item_tree::FunctionItem,
    ) -> FunctionId {
        self.target_ir.items_mut().alloc_function(FunctionData {
            local_def,
            source,
            owner,
            name: item.name.clone().unwrap_or_else(|| "<missing>".to_string()),
            visibility: item.visibility.clone(),
            declaration: declaration.clone(),
        })
    }

    fn lower_type_alias(
        &mut self,
        local_def: Option<LocalDefRef>,
        source: ItemTreeRef,
        owner: ItemOwner,
        item: &ItemNode,
        declaration: &TypeAliasItem,
    ) -> TypeAliasId {
        self.target_ir.items_mut().alloc_type_alias(TypeAliasData {
            local_def,
            source,
            owner,
            name: item.name.clone().unwrap_or_else(|| "<missing>".to_string()),
            visibility: item.visibility.clone(),
            declaration: declaration.clone(),
        })
    }

    fn lower_const(
        &mut self,
        local_def: Option<LocalDefRef>,
        source: ItemTreeRef,
        owner: ItemOwner,
        item: &ItemNode,
        declaration: &ConstItem,
    ) -> ConstId {
        self.target_ir.items_mut().alloc_const(ConstData {
            local_def,
            source,
            owner,
            name: item.name.clone().unwrap_or_else(|| "<missing>".to_string()),
            visibility: item.visibility.clone(),
            declaration: declaration.clone(),
        })
    }

    fn lower_static(
        &mut self,
        local_def: LocalDefRef,
        source: ItemTreeRef,
        owner: ModuleRef,
        item: &ItemNode,
        declaration: &StaticItem,
    ) -> StaticId {
        self.target_ir.items_mut().alloc_static(StaticData {
            local_def,
            source,
            owner,
            name: item.name.clone().unwrap_or_else(|| "<missing>".to_string()),
            visibility: item.visibility.clone(),
            ty: declaration.ty.clone(),
            mutability: declaration.mutability,
        })
    }
}
