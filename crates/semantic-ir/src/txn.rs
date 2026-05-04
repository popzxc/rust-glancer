//! Read transactions over frozen Semantic IR package data.

use std::sync::Arc;

use rg_def_map::{DefMapReadTxn, LocalDefRef, ModuleRef, PackageSlot, Path, TargetRef};
use rg_item_tree::FieldKey;
use rg_package_store::{PackageRead, PackageStoreReadTxn};
use rg_parse::TargetId;

use crate::{
    AssocItemId, ConstData, ConstRef, EnumData, EnumVariantData, EnumVariantRef, FieldData,
    FieldRef, FunctionData, FunctionRef, ImplData, ImplRef, ItemId, ItemOwner, PackageIr,
    SemanticTypePathResolution, StaticData, StaticRef, StructData, TargetIr, TraitData,
    TraitImplRef, TraitRef, TypeAliasData, TypeAliasRef, TypeDefId, TypeDefRef, TypePathContext,
    UnionData, push_unique,
};

/// Read-only semantic IR access for one query transaction.
#[derive(Debug, Clone)]
pub struct SemanticIrReadTxn<'db> {
    packages: PackageStoreReadTxn<'db, PackageIr>,
}

impl<'db> SemanticIrReadTxn<'db> {
    pub fn from_sparse_package_arcs(packages: Vec<Option<Arc<PackageIr>>>) -> Self {
        Self {
            packages: PackageStoreReadTxn::from_sparse_arcs(packages),
        }
    }

    pub fn package(&self, package: PackageSlot) -> Option<PackageRead<'_, PackageIr>> {
        self.packages.read(package)
    }

    pub fn target_ir(&self, target: TargetRef) -> Option<&TargetIr> {
        self.package(target.package)?
            .into_ref()
            .target(target.target)
    }

    pub fn target_irs(&self) -> impl Iterator<Item = (TargetRef, &TargetIr)> + '_ {
        self.packages
            .packages_with_slots()
            .flat_map(|(package_slot, package)| {
                let package = package.into_ref();
                package
                    .targets()
                    .iter()
                    .enumerate()
                    .map(move |(target_idx, target_ir)| {
                        (
                            TargetRef {
                                package: package_slot,
                                target: TargetId(target_idx),
                            },
                            target_ir,
                        )
                    })
            })
    }

    pub fn structs(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (TypeDefRef, &StructData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .structs
                    .iter_with_ids()
                    .map(move |(id, data)| {
                        (
                            TypeDefRef {
                                target,
                                id: TypeDefId::Struct(id),
                            },
                            data,
                        )
                    })
            })
    }

    pub fn unions(&self, target: TargetRef) -> impl Iterator<Item = (TypeDefRef, &UnionData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .unions
                    .iter_with_ids()
                    .map(move |(id, data)| {
                        (
                            TypeDefRef {
                                target,
                                id: TypeDefId::Union(id),
                            },
                            data,
                        )
                    })
            })
    }

    pub fn enums(&self, target: TargetRef) -> impl Iterator<Item = (TypeDefRef, &EnumData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .enums
                    .iter_with_ids()
                    .map(move |(id, data)| {
                        (
                            TypeDefRef {
                                target,
                                id: TypeDefId::Enum(id),
                            },
                            data,
                        )
                    })
            })
    }

    pub fn traits(&self, target: TargetRef) -> impl Iterator<Item = (TraitRef, &TraitData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .traits
                    .iter_with_ids()
                    .map(move |(id, data)| (TraitRef { target, id }, data))
            })
    }

    pub fn impls(&self, target: TargetRef) -> impl Iterator<Item = (ImplRef, &ImplData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .impls
                    .iter_with_ids()
                    .map(move |(id, data)| (ImplRef { target, id }, data))
            })
    }

    pub fn functions(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (FunctionRef, &FunctionData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .functions
                    .iter_with_ids()
                    .map(move |(id, data)| (FunctionRef { target, id }, data))
            })
    }

    pub fn type_aliases(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (TypeAliasRef, &TypeAliasData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .type_aliases
                    .iter_with_ids()
                    .map(move |(id, data)| (TypeAliasRef { target, id }, data))
            })
    }

    pub fn consts(&self, target: TargetRef) -> impl Iterator<Item = (ConstRef, &ConstData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .consts
                    .iter_with_ids()
                    .map(move |(id, data)| (ConstRef { target, id }, data))
            })
    }

    pub fn statics(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (StaticRef, &StaticData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .statics
                    .iter_with_ids()
                    .map(move |(id, data)| (StaticRef { target, id }, data))
            })
    }

    pub fn resolve_type_path(
        &self,
        def_map: &DefMapReadTxn<'db>,
        context: TypePathContext,
        path: &Path,
    ) -> SemanticTypePathResolution {
        if path.is_self_type() {
            let Some(impl_ref) = context.impl_ref else {
                return SemanticTypePathResolution::Unknown;
            };
            let types = self
                .impl_data(impl_ref)
                .map(|data| data.resolved_self_tys.clone())
                .unwrap_or_default();
            return if types.is_empty() {
                SemanticTypePathResolution::Unknown
            } else {
                SemanticTypePathResolution::SelfType(types)
            };
        }

        let type_defs = self.type_defs_for_path(def_map, context.module, path);
        if type_defs.is_empty() {
            let traits = self.traits_for_path(def_map, context.module, path);
            if traits.is_empty() {
                SemanticTypePathResolution::Unknown
            } else {
                SemanticTypePathResolution::Traits(traits)
            }
        } else {
            SemanticTypePathResolution::TypeDefs(type_defs)
        }
    }

    pub fn type_defs_for_path(
        &self,
        def_map: &DefMapReadTxn<'db>,
        from: ModuleRef,
        path: &Path,
    ) -> Vec<TypeDefRef> {
        self.resolve_path(def_map, from, path, |db, def| {
            let rg_def_map::DefId::Local(local_def) = def else {
                return None;
            };

            db.type_def_for_local_def(local_def)
        })
    }

    pub fn traits_for_path(
        &self,
        def_map: &DefMapReadTxn<'db>,
        from: ModuleRef,
        path: &Path,
    ) -> Vec<TraitRef> {
        self.resolve_path(def_map, from, path, |db, def| {
            let rg_def_map::DefId::Local(local_def) = def else {
                return None;
            };

            db.trait_for_local_def(local_def)
        })
    }

    fn resolve_path<T: PartialEq>(
        &self,
        def_map: &DefMapReadTxn<'db>,
        owner: ModuleRef,
        path: &Path,
        map_def: impl Fn(&Self, rg_def_map::DefId) -> Option<T>,
    ) -> Vec<T> {
        let mut resolved_items = Vec::new();
        for def in def_map.resolve_path_in_type_namespace(owner, path).resolved {
            let Some(item) = map_def(self, def) else {
                continue;
            };
            push_unique(&mut resolved_items, item);
        }

        resolved_items
    }

    pub fn type_path_context_for_function(
        &self,
        function_ref: FunctionRef,
    ) -> Option<TypePathContext> {
        let function_data = self.function_data(function_ref)?;
        self.type_path_context_for_owner(function_ref.target, function_data.owner)
    }

    pub fn type_path_context_for_owner(
        &self,
        target: TargetRef,
        owner: ItemOwner,
    ) -> Option<TypePathContext> {
        match owner {
            ItemOwner::Module(module_ref) => Some(TypePathContext::module(module_ref)),
            ItemOwner::Trait(id) => self
                .trait_data(TraitRef { target, id })
                .map(|data| TypePathContext::module(data.owner)),
            ItemOwner::Impl(id) => {
                let impl_ref = ImplRef { target, id };
                self.impl_data(impl_ref).map(|data| TypePathContext {
                    module: data.owner,
                    impl_ref: Some(impl_ref),
                })
            }
        }
    }

    pub fn type_def_for_local_def(&self, def: LocalDefRef) -> Option<TypeDefRef> {
        let item = self
            .target_ir(def.target)?
            .item_for_local_def(def.local_def)?;
        let id = match item {
            ItemId::Struct(id) => TypeDefId::Struct(id),
            ItemId::Enum(id) => TypeDefId::Enum(id),
            ItemId::Union(id) => TypeDefId::Union(id),
            ItemId::Trait(_)
            | ItemId::Function(_)
            | ItemId::TypeAlias(_)
            | ItemId::Const(_)
            | ItemId::Static(_) => return None,
        };

        Some(TypeDefRef {
            target: def.target,
            id,
        })
    }

    pub fn trait_for_local_def(&self, def: LocalDefRef) -> Option<TraitRef> {
        let item = self
            .target_ir(def.target)?
            .item_for_local_def(def.local_def)?;
        let ItemId::Trait(id) = item else {
            return None;
        };

        Some(TraitRef {
            target: def.target,
            id,
        })
    }

    pub fn function_for_local_def(&self, def: LocalDefRef) -> Option<FunctionRef> {
        let item = self
            .target_ir(def.target)?
            .item_for_local_def(def.local_def)?;
        let ItemId::Function(id) = item else {
            return None;
        };

        Some(FunctionRef {
            target: def.target,
            id,
        })
    }

    pub fn local_def_for_type_def(&self, ty: TypeDefRef) -> Option<LocalDefRef> {
        let target_ir = self.target_ir(ty.target)?;
        match ty.id {
            TypeDefId::Struct(id) => Some(target_ir.items().struct_data(id)?.local_def),
            TypeDefId::Enum(id) => Some(target_ir.items().enum_data(id)?.local_def),
            TypeDefId::Union(id) => Some(target_ir.items().union_data(id)?.local_def),
        }
    }

    pub fn generic_params_for_type_def(
        &self,
        ty: TypeDefRef,
    ) -> Option<&rg_item_tree::GenericParams> {
        let target_ir = self.target_ir(ty.target)?;
        match ty.id {
            TypeDefId::Struct(id) => Some(&target_ir.items().struct_data(id)?.generics),
            TypeDefId::Enum(id) => Some(&target_ir.items().enum_data(id)?.generics),
            TypeDefId::Union(id) => Some(&target_ir.items().union_data(id)?.generics),
        }
    }

    pub fn type_def_name(&self, ty: TypeDefRef) -> Option<&str> {
        let target_ir = self.target_ir(ty.target)?;
        match ty.id {
            TypeDefId::Struct(id) => Some(target_ir.items().struct_data(id)?.name.as_str()),
            TypeDefId::Enum(id) => Some(target_ir.items().enum_data(id)?.name.as_str()),
            TypeDefId::Union(id) => Some(target_ir.items().union_data(id)?.name.as_str()),
        }
    }

    pub fn enum_data_for_type_def(&self, ty: TypeDefRef) -> Option<&EnumData> {
        let target_ir = self.target_ir(ty.target)?;
        let TypeDefId::Enum(id) = ty.id else {
            return None;
        };
        target_ir.items().enum_data(id)
    }

    pub fn enum_variant_for_type_def(
        &self,
        ty: TypeDefRef,
        variant_name: &str,
    ) -> Option<(usize, &rg_item_tree::EnumVariantItem)> {
        let data = self.enum_data_for_type_def(ty)?;
        data.variants
            .iter()
            .enumerate()
            .find(|(_, variant)| variant.name == variant_name)
    }

    pub fn enum_variant_ref_for_type_def(
        &self,
        ty: TypeDefRef,
        variant_name: &str,
    ) -> Option<EnumVariantRef> {
        let TypeDefId::Enum(enum_id) = ty.id else {
            return None;
        };
        let (index, _) = self.enum_variant_for_type_def(ty, variant_name)?;
        Some(EnumVariantRef {
            target: ty.target,
            enum_id,
            index,
        })
    }

    pub fn enum_variant_data(&self, variant_ref: EnumVariantRef) -> Option<EnumVariantData<'_>> {
        let target_ir = self.target_ir(variant_ref.target)?;
        let data = target_ir.items().enum_data(variant_ref.enum_id)?;
        let variant = data.variants.get(variant_ref.index)?;
        Some(EnumVariantData {
            owner: TypeDefRef {
                target: variant_ref.target,
                id: TypeDefId::Enum(variant_ref.enum_id),
            },
            owner_module: data.owner,
            file_id: data.source.file_id,
            variant,
        })
    }

    pub fn impl_data(&self, impl_ref: ImplRef) -> Option<&ImplData> {
        self.target_ir(impl_ref.target)?
            .items()
            .impl_data(impl_ref.id)
    }

    pub fn trait_data(&self, trait_ref: TraitRef) -> Option<&TraitData> {
        self.target_ir(trait_ref.target)?
            .items()
            .trait_data(trait_ref.id)
    }

    pub fn function_data(&self, function_ref: FunctionRef) -> Option<&FunctionData> {
        self.target_ir(function_ref.target)?
            .items()
            .function_data(function_ref.id)
    }

    pub fn type_alias_data(&self, type_alias_ref: TypeAliasRef) -> Option<&TypeAliasData> {
        self.target_ir(type_alias_ref.target)?
            .items()
            .type_alias_data(type_alias_ref.id)
    }

    pub fn const_data(&self, const_ref: ConstRef) -> Option<&ConstData> {
        self.target_ir(const_ref.target)?
            .items()
            .const_data(const_ref.id)
    }

    pub fn static_data(&self, static_ref: StaticRef) -> Option<&StaticData> {
        self.target_ir(static_ref.target)?
            .items()
            .static_data(static_ref.id)
    }

    pub fn fields_for_type(&self, ty: TypeDefRef) -> Vec<FieldRef> {
        let Some(field_count) = self.field_count_for_type(ty) else {
            return Vec::new();
        };

        (0..field_count)
            .map(|index| FieldRef { owner: ty, index })
            .collect()
    }

    pub fn field_for_type(&self, ty: TypeDefRef, key: &FieldKey) -> Option<FieldRef> {
        match key {
            FieldKey::Named(_) => self.fields_for_type(ty).into_iter().find(|field_ref| {
                self.field_data(*field_ref)
                    .is_some_and(|data| data.field.key.as_ref() == Some(key))
            }),
            FieldKey::Tuple(index) => {
                let field_ref = FieldRef {
                    owner: ty,
                    index: *index,
                };
                self.field_data(field_ref)
                    .is_some_and(|data| data.field.key.as_ref() == Some(key))
                    .then_some(field_ref)
            }
        }
    }

    pub fn field_data(&self, field_ref: FieldRef) -> Option<FieldData<'_>> {
        let target_ir = self.target_ir(field_ref.owner.target)?;
        match field_ref.owner.id {
            TypeDefId::Struct(id) => {
                let data = target_ir.items().struct_data(id)?;
                let field = data.fields.fields().get(field_ref.index)?;
                Some(FieldData {
                    owner_module: data.owner,
                    file_id: data.source.file_id,
                    field,
                })
            }
            TypeDefId::Union(id) => {
                let data = target_ir.items().union_data(id)?;
                let field = data.fields.get(field_ref.index)?;
                Some(FieldData {
                    owner_module: data.owner,
                    file_id: data.source.file_id,
                    field,
                })
            }
            TypeDefId::Enum(_) => None,
        }
    }

    pub fn impls_for_type(&self, ty: TypeDefRef) -> Vec<ImplRef> {
        self.impl_refs()
            .into_iter()
            .filter(|impl_ref| {
                self.impl_data(*impl_ref)
                    .is_some_and(|data| data.resolved_self_tys.contains(&ty))
            })
            .collect()
    }

    pub fn inherent_impls_for_type(&self, ty: TypeDefRef) -> Vec<ImplRef> {
        self.impls_for_type(ty)
            .into_iter()
            .filter(|impl_ref| {
                self.impl_data(*impl_ref)
                    .is_some_and(|data| data.trait_ref.is_none())
            })
            .collect()
    }

    pub fn trait_impls_for_type(&self, ty: TypeDefRef) -> Vec<TraitImplRef> {
        let mut trait_impls = Vec::new();

        for impl_ref in self.impls_for_type(ty) {
            let Some(data) = self.impl_data(impl_ref) else {
                continue;
            };

            for trait_ref in &data.resolved_trait_refs {
                push_unique(
                    &mut trait_impls,
                    TraitImplRef {
                        impl_ref,
                        trait_ref: *trait_ref,
                    },
                );
            }
        }

        trait_impls
    }

    pub fn traits_for_type(&self, ty: TypeDefRef) -> Vec<TraitRef> {
        let mut traits = Vec::new();

        for trait_impl in self.trait_impls_for_type(ty) {
            push_unique(&mut traits, trait_impl.trait_ref);
        }

        traits
    }

    pub fn trait_functions(&self, trait_ref: TraitRef) -> Vec<FunctionRef> {
        let mut functions = Vec::new();
        let Some(data) = self.trait_data(trait_ref) else {
            return functions;
        };

        for item in &data.items {
            if let AssocItemId::Function(id) = item {
                push_unique(
                    &mut functions,
                    FunctionRef {
                        target: trait_ref.target,
                        id: *id,
                    },
                );
            }
        }

        functions
    }

    pub fn inherent_functions_for_type(&self, ty: TypeDefRef) -> Vec<FunctionRef> {
        let mut functions = Vec::new();

        for impl_ref in self.inherent_impls_for_type(ty) {
            let Some(data) = self.impl_data(impl_ref) else {
                continue;
            };

            for item in &data.items {
                if let AssocItemId::Function(id) = item {
                    push_unique(
                        &mut functions,
                        FunctionRef {
                            target: impl_ref.target,
                            id: *id,
                        },
                    );
                }
            }
        }

        functions
    }

    pub fn trait_functions_for_type(&self, ty: TypeDefRef) -> Vec<FunctionRef> {
        let mut functions = Vec::new();

        for trait_ref in self.traits_for_type(ty) {
            for function in self.trait_functions(trait_ref) {
                push_unique(&mut functions, function);
            }
        }

        functions
    }

    pub fn trait_impl_functions_for_type(&self, ty: TypeDefRef) -> Vec<FunctionRef> {
        let mut functions = Vec::new();

        for trait_impl in self.trait_impls_for_type(ty) {
            let Some(data) = self.impl_data(trait_impl.impl_ref) else {
                continue;
            };

            for item in &data.items {
                if let AssocItemId::Function(id) = item {
                    push_unique(
                        &mut functions,
                        FunctionRef {
                            target: trait_impl.impl_ref.target,
                            id: *id,
                        },
                    );
                }
            }
        }

        functions
    }

    fn field_count_for_type(&self, ty: TypeDefRef) -> Option<usize> {
        let target_ir = self.target_ir(ty.target)?;
        match ty.id {
            TypeDefId::Struct(id) => Some(target_ir.items().struct_data(id)?.fields.fields().len()),
            TypeDefId::Union(id) => Some(target_ir.items().union_data(id)?.fields.len()),
            TypeDefId::Enum(_) => None,
        }
    }

    fn impl_refs(&self) -> Vec<ImplRef> {
        self.target_irs()
            .flat_map(|(target, _)| self.impls(target).map(|(impl_ref, _)| impl_ref))
            .collect()
    }
}
