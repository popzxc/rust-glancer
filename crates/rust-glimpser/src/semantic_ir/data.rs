use crate::{
    def_map::{
        DefMapDb, LocalDefId, LocalDefRef, LocalImplRef, ModuleRef, PackageSlot, Path, TargetRef,
    },
    item_tree::{
        ConstItem, FieldItem, FieldKey, FieldList, FunctionItem, GenericParams, ItemTreeRef,
        Mutability, ParamKind, TypeAliasItem, TypeBound, TypeRef, VisibilityLevel,
    },
    parse::{FileId, TargetId, span::Span},
};

use super::{
    ids::{
        AssocItemId, ConstId, ConstRef, EnumId, FieldRef, FunctionId, FunctionRef, ImplId, ImplRef,
        ItemId, ItemOwner, StaticId, StaticRef, StructId, TraitId, TraitImplRef, TraitRef,
        TypeAliasId, TypeAliasRef, TypeDefId, TypeDefRef, UnionId,
    },
    lower,
    resolution::{self, SemanticTypePathResolution, TypePathContext},
};

/// Semantic item graph for all analyzed packages and targets.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SemanticIrDb {
    packages: Vec<PackageIr>,
}

impl SemanticIrDb {
    pub(crate) fn build(
        item_tree: &crate::item_tree::ItemTreeDb,
        def_map: &crate::def_map::DefMapDb,
    ) -> anyhow::Result<Self> {
        let mut db = lower::build_db(item_tree, def_map)?;
        resolution::resolve_impl_headers(&mut db, def_map);
        Ok(db)
    }

    pub(crate) fn new(packages: Vec<PackageIr>) -> Self {
        Self { packages }
    }

    pub(crate) fn stats(&self) -> SemanticIrStats {
        let mut stats = SemanticIrStats::default();

        for package in &self.packages {
            for target in package.targets() {
                stats.target_count += 1;
                stats.struct_count += target.items.structs.len();
                stats.union_count += target.items.unions.len();
                stats.enum_count += target.items.enums.len();
                stats.trait_count += target.items.traits.len();
                stats.impl_count += target.items.impls.len();
                stats.function_count += target.items.functions.len();
                stats.type_alias_count += target.items.type_aliases.len();
                stats.const_count += target.items.consts.len();
                stats.static_count += target.items.statics.len();
            }
        }

        stats
    }
}

// The semantic IR query surface is intentionally introduced with the layer. The first production
// consumer only needs coarse stats today; snapshot tests exercise these methods until LSP/query
// consumers arrive.
#[allow(dead_code)]
impl SemanticIrDb {
    /// Returns all package-level semantic IR sets.
    pub(crate) fn packages(&self) -> &[PackageIr] {
        &self.packages
    }

    /// Returns one package by package slot.
    pub(crate) fn package(&self, package: PackageSlot) -> Option<&PackageIr> {
        self.packages.get(package.0)
    }

    /// Returns one target semantic IR by project-wide target reference.
    pub(crate) fn target_ir(&self, target: TargetRef) -> Option<&TargetIr> {
        self.package(target.package)?.target(target.target)
    }

    /// Iterates over every target IR together with its project-wide target reference.
    pub(crate) fn target_irs(&self) -> impl Iterator<Item = (TargetRef, &TargetIr)> {
        self.packages
            .iter()
            .enumerate()
            .flat_map(|(package_idx, package)| {
                package
                    .targets()
                    .iter()
                    .enumerate()
                    .map(move |(target_idx, target_ir)| {
                        (
                            TargetRef {
                                package: PackageSlot(package_idx),
                                target: TargetId(target_idx),
                            },
                            target_ir,
                        )
                    })
            })
    }

    /// Iterates over one target's structs together with stable project-wide references.
    pub(crate) fn structs(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (TypeDefRef, &StructData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .structs
                    .iter()
                    .enumerate()
                    .map(move |(idx, data)| {
                        (
                            TypeDefRef {
                                target,
                                id: TypeDefId::Struct(StructId(idx)),
                            },
                            data,
                        )
                    })
            })
    }

    /// Iterates over one target's unions together with stable project-wide references.
    pub(crate) fn unions(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (TypeDefRef, &UnionData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .unions
                    .iter()
                    .enumerate()
                    .map(move |(idx, data)| {
                        (
                            TypeDefRef {
                                target,
                                id: TypeDefId::Union(UnionId(idx)),
                            },
                            data,
                        )
                    })
            })
    }

    /// Iterates over one target's enums together with stable project-wide references.
    pub(crate) fn enums(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (TypeDefRef, &EnumData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .enums
                    .iter()
                    .enumerate()
                    .map(move |(idx, data)| {
                        (
                            TypeDefRef {
                                target,
                                id: TypeDefId::Enum(EnumId(idx)),
                            },
                            data,
                        )
                    })
            })
    }

    /// Iterates over one target's traits together with stable project-wide references.
    pub(crate) fn traits(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (TraitRef, &TraitData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .traits
                    .iter()
                    .enumerate()
                    .map(move |(idx, data)| {
                        (
                            TraitRef {
                                target,
                                id: TraitId(idx),
                            },
                            data,
                        )
                    })
            })
    }

    /// Iterates over one target's impls together with stable project-wide references.
    pub(crate) fn impls(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (ImplRef, &ImplData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .impls
                    .iter()
                    .enumerate()
                    .map(move |(idx, data)| {
                        (
                            ImplRef {
                                target,
                                id: ImplId(idx),
                            },
                            data,
                        )
                    })
            })
    }

    /// Iterates over one target's functions together with stable project-wide references.
    pub(crate) fn functions(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (FunctionRef, &FunctionData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .functions
                    .iter()
                    .enumerate()
                    .map(move |(idx, data)| {
                        (
                            FunctionRef {
                                target,
                                id: FunctionId(idx),
                            },
                            data,
                        )
                    })
            })
    }

    /// Returns the number of function slots in one target.
    pub(crate) fn function_count(&self, target: TargetRef) -> usize {
        self.functions(target).count()
    }

    /// Iterates over one target's type aliases together with stable project-wide references.
    pub(crate) fn type_aliases(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (TypeAliasRef, &TypeAliasData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .type_aliases
                    .iter()
                    .enumerate()
                    .map(move |(idx, data)| {
                        (
                            TypeAliasRef {
                                target,
                                id: TypeAliasId(idx),
                            },
                            data,
                        )
                    })
            })
    }

    /// Iterates over one target's consts together with stable project-wide references.
    pub(crate) fn consts(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (ConstRef, &ConstData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .consts
                    .iter()
                    .enumerate()
                    .map(move |(idx, data)| {
                        (
                            ConstRef {
                                target,
                                id: ConstId(idx),
                            },
                            data,
                        )
                    })
            })
    }

    /// Iterates over one target's statics together with stable project-wide references.
    pub(crate) fn statics(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (StaticRef, &StaticData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .statics
                    .iter()
                    .enumerate()
                    .map(move |(idx, data)| {
                        (
                            StaticRef {
                                target,
                                id: StaticId(idx),
                            },
                            data,
                        )
                    })
            })
    }

    pub(crate) fn type_defs_for_path(
        &self,
        def_map: &DefMapDb,
        from: ModuleRef,
        path: &Path,
    ) -> Vec<TypeDefRef> {
        resolution::resolve_type_defs_for_path(self, def_map, from, path)
    }

    pub(crate) fn resolve_type_path(
        &self,
        def_map: &DefMapDb,
        context: TypePathContext,
        path: &Path,
    ) -> SemanticTypePathResolution {
        resolution::resolve_type_path(self, def_map, context, path)
    }

    pub(crate) fn type_path_context_for_function(
        &self,
        function_ref: FunctionRef,
    ) -> Option<TypePathContext> {
        let function_data = self.function_data(function_ref)?;
        self.type_path_context_for_owner(function_ref.target, function_data.owner)
    }

    pub(crate) fn type_path_context_for_owner(
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

    pub(crate) fn type_def_for_local_def(&self, def: LocalDefRef) -> Option<TypeDefRef> {
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

    pub(crate) fn trait_for_local_def(&self, def: LocalDefRef) -> Option<TraitRef> {
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

    pub(crate) fn function_for_local_def(&self, def: LocalDefRef) -> Option<FunctionRef> {
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

    pub(crate) fn local_def_for_type_def(&self, ty: TypeDefRef) -> Option<LocalDefRef> {
        let target_ir = self.target_ir(ty.target)?;
        match ty.id {
            TypeDefId::Struct(id) => Some(target_ir.items().struct_data(id)?.local_def),
            TypeDefId::Enum(id) => Some(target_ir.items().enum_data(id)?.local_def),
            TypeDefId::Union(id) => Some(target_ir.items().union_data(id)?.local_def),
        }
    }

    pub(crate) fn impl_data(&self, impl_ref: ImplRef) -> Option<&ImplData> {
        self.target_ir(impl_ref.target)?
            .items()
            .impl_data(impl_ref.id)
    }

    pub(crate) fn trait_data(&self, trait_ref: TraitRef) -> Option<&TraitData> {
        self.target_ir(trait_ref.target)?
            .items()
            .trait_data(trait_ref.id)
    }

    pub(crate) fn function_data(&self, function_ref: FunctionRef) -> Option<&FunctionData> {
        self.target_ir(function_ref.target)?
            .items()
            .function_data(function_ref.id)
    }

    pub(crate) fn type_alias_data(&self, type_alias_ref: TypeAliasRef) -> Option<&TypeAliasData> {
        self.target_ir(type_alias_ref.target)?
            .items()
            .type_alias_data(type_alias_ref.id)
    }

    pub(crate) fn const_data(&self, const_ref: ConstRef) -> Option<&ConstData> {
        self.target_ir(const_ref.target)?
            .items()
            .const_data(const_ref.id)
    }

    pub(crate) fn static_data(&self, static_ref: StaticRef) -> Option<&StaticData> {
        self.target_ir(static_ref.target)?
            .items()
            .static_data(static_ref.id)
    }

    pub(crate) fn fields_for_type(&self, ty: TypeDefRef) -> Vec<FieldRef> {
        let Some(field_count) = self.field_count_for_type(ty) else {
            return Vec::new();
        };

        (0..field_count)
            .map(|index| FieldRef { owner: ty, index })
            .collect()
    }

    pub(crate) fn field_for_type(&self, ty: TypeDefRef, key: &FieldKey) -> Option<FieldRef> {
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

    pub(crate) fn field_data(&self, field_ref: FieldRef) -> Option<FieldData<'_>> {
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

    pub(crate) fn impls_for_type(&self, ty: TypeDefRef) -> Vec<ImplRef> {
        self.impl_refs()
            .into_iter()
            .filter(|impl_ref| {
                self.impl_data(*impl_ref)
                    .is_some_and(|data| data.resolved_self_tys.contains(&ty))
            })
            .collect()
    }

    pub(crate) fn inherent_impls_for_type(&self, ty: TypeDefRef) -> Vec<ImplRef> {
        self.impls_for_type(ty)
            .into_iter()
            .filter(|impl_ref| {
                self.impl_data(*impl_ref)
                    .is_some_and(|data| data.trait_ref.is_none())
            })
            .collect()
    }

    pub(crate) fn trait_impls_for_type(&self, ty: TypeDefRef) -> Vec<TraitImplRef> {
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

    pub(crate) fn traits_for_type(&self, ty: TypeDefRef) -> Vec<TraitRef> {
        let mut traits = Vec::new();

        for trait_impl in self.trait_impls_for_type(ty) {
            push_unique(&mut traits, trait_impl.trait_ref);
        }

        traits
    }

    pub(crate) fn inherent_functions_for_type(&self, ty: TypeDefRef) -> Vec<FunctionRef> {
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

    pub(crate) fn trait_functions_for_type(&self, ty: TypeDefRef) -> Vec<FunctionRef> {
        let mut functions = Vec::new();

        for trait_ref in self.traits_for_type(ty) {
            let Some(data) = self.trait_data(trait_ref) else {
                continue;
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
        }

        functions
    }

    pub(crate) fn trait_impl_functions_for_type(&self, ty: TypeDefRef) -> Vec<FunctionRef> {
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct SemanticIrStats {
    pub(crate) target_count: usize,
    pub(crate) struct_count: usize,
    pub(crate) union_count: usize,
    pub(crate) enum_count: usize,
    pub(crate) trait_count: usize,
    pub(crate) impl_count: usize,
    pub(crate) function_count: usize,
    pub(crate) type_alias_count: usize,
    pub(crate) const_count: usize,
    pub(crate) static_count: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FieldData<'a> {
    pub(crate) owner_module: ModuleRef,
    pub(crate) file_id: FileId,
    pub(crate) field: &'a FieldItem,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageIr {
    targets: Vec<TargetIr>,
}

impl PackageIr {
    pub(crate) fn new(targets: Vec<TargetIr>) -> Self {
        Self { targets }
    }

    pub(crate) fn targets(&self) -> &[TargetIr] {
        &self.targets
    }
}

#[allow(dead_code)]
impl PackageIr {
    pub(crate) fn target(&self, target: TargetId) -> Option<&TargetIr> {
        self.targets.get(target.0)
    }
}

impl SemanticIrDb {
    pub(super) fn impl_refs(&self) -> Vec<ImplRef> {
        self.target_irs()
            .flat_map(|(target, _)| self.impls(target).map(|(impl_ref, _)| impl_ref))
            .collect()
    }

    pub(super) fn impl_data_mut(&mut self, impl_ref: ImplRef) -> Option<&mut ImplData> {
        self.package_mut(impl_ref.target.package)?
            .target_mut(impl_ref.target.target)?
            .items_mut()
            .impls
            .get_mut(impl_ref.id.0)
    }

    fn package_mut(&mut self, package: PackageSlot) -> Option<&mut PackageIr> {
        self.packages.get_mut(package.0)
    }
}

impl PackageIr {
    fn target_mut(&mut self, target: TargetId) -> Option<&mut TargetIr> {
        self.targets.get_mut(target.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIr {
    local_items: Vec<Option<ItemId>>,
    local_impls: Vec<ImplId>,
    items: ItemStore,
}

impl TargetIr {
    pub(crate) fn new(local_def_count: usize) -> Self {
        Self {
            local_items: vec![None; local_def_count],
            local_impls: Vec::new(),
            items: ItemStore::default(),
        }
    }
}

#[allow(dead_code)]
impl TargetIr {
    pub(crate) fn item_for_local_def(&self, local_def: LocalDefId) -> Option<ItemId> {
        self.local_items.get(local_def.0).copied().flatten()
    }

    pub(crate) fn impls(&self) -> &[ImplId] {
        &self.local_impls
    }

    pub(crate) fn items(&self) -> &ItemStore {
        &self.items
    }
}

impl TargetIr {
    pub(super) fn set_local_item(&mut self, local_def: LocalDefId, item: ItemId) {
        let slot = self
            .local_items
            .get_mut(local_def.0)
            .expect("local item slot should exist while building semantic IR");
        *slot = Some(item);
    }

    pub(super) fn push_local_impl(&mut self, impl_id: ImplId) {
        self.local_impls.push(impl_id);
    }

    pub(super) fn items_mut(&mut self) -> &mut ItemStore {
        &mut self.items
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemStore {
    pub(crate) structs: Vec<StructData>,
    pub(crate) unions: Vec<UnionData>,
    pub(crate) enums: Vec<EnumData>,
    pub(crate) traits: Vec<TraitData>,
    pub(crate) impls: Vec<ImplData>,
    pub(crate) functions: Vec<FunctionData>,
    pub(crate) type_aliases: Vec<TypeAliasData>,
    pub(crate) consts: Vec<ConstData>,
    pub(crate) statics: Vec<StaticData>,
}

#[allow(dead_code)]
impl ItemStore {
    pub(crate) fn struct_data(&self, id: StructId) -> Option<&StructData> {
        self.structs.get(id.0)
    }

    pub(crate) fn union_data(&self, id: UnionId) -> Option<&UnionData> {
        self.unions.get(id.0)
    }

    pub(crate) fn enum_data(&self, id: EnumId) -> Option<&EnumData> {
        self.enums.get(id.0)
    }

    pub(crate) fn trait_data(&self, id: TraitId) -> Option<&TraitData> {
        self.traits.get(id.0)
    }

    pub(crate) fn impl_data(&self, id: ImplId) -> Option<&ImplData> {
        self.impls.get(id.0)
    }

    pub(crate) fn function_data(&self, id: FunctionId) -> Option<&FunctionData> {
        self.functions.get(id.0)
    }

    pub(crate) fn type_alias_data(&self, id: TypeAliasId) -> Option<&TypeAliasData> {
        self.type_aliases.get(id.0)
    }

    pub(crate) fn const_data(&self, id: ConstId) -> Option<&ConstData> {
        self.consts.get(id.0)
    }

    pub(crate) fn static_data(&self, id: StaticId) -> Option<&StaticData> {
        self.statics.get(id.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructData {
    pub local_def: LocalDefRef,
    pub source: ItemTreeRef,
    pub owner: ModuleRef,
    pub name: String,
    pub visibility: VisibilityLevel,
    pub generics: GenericParams,
    pub fields: FieldList,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnionData {
    pub local_def: LocalDefRef,
    pub source: ItemTreeRef,
    pub owner: ModuleRef,
    pub name: String,
    pub visibility: VisibilityLevel,
    pub generics: GenericParams,
    pub fields: Vec<crate::item_tree::FieldItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumData {
    pub local_def: LocalDefRef,
    pub source: ItemTreeRef,
    pub owner: ModuleRef,
    pub name: String,
    pub visibility: VisibilityLevel,
    pub generics: GenericParams,
    pub variants: Vec<crate::item_tree::EnumVariantItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitData {
    pub local_def: LocalDefRef,
    pub source: ItemTreeRef,
    pub owner: ModuleRef,
    pub name: String,
    pub visibility: VisibilityLevel,
    pub generics: GenericParams,
    pub super_traits: Vec<TypeBound>,
    pub items: Vec<AssocItemId>,
    pub is_unsafe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplData {
    pub local_impl: LocalImplRef,
    pub source: ItemTreeRef,
    pub owner: ModuleRef,
    pub generics: GenericParams,
    pub trait_ref: Option<TypeRef>,
    pub self_ty: TypeRef,
    pub resolved_self_tys: Vec<TypeDefRef>,
    pub resolved_trait_refs: Vec<TraitRef>,
    pub items: Vec<AssocItemId>,
    pub is_unsafe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionData {
    pub local_def: Option<LocalDefRef>,
    pub source: ItemTreeRef,
    pub span: Span,
    pub name_span: Option<Span>,
    pub owner: ItemOwner,
    pub name: String,
    pub visibility: VisibilityLevel,
    pub declaration: FunctionItem,
}

impl FunctionData {
    pub(crate) fn has_self_receiver(&self) -> bool {
        self.declaration
            .params
            .first()
            .is_some_and(|param| matches!(param.kind, ParamKind::SelfParam))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAliasData {
    pub local_def: Option<LocalDefRef>,
    pub source: ItemTreeRef,
    pub owner: ItemOwner,
    pub name: String,
    pub visibility: VisibilityLevel,
    pub declaration: TypeAliasItem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstData {
    pub local_def: Option<LocalDefRef>,
    pub source: ItemTreeRef,
    pub owner: ItemOwner,
    pub name: String,
    pub visibility: VisibilityLevel,
    pub declaration: ConstItem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticData {
    pub local_def: LocalDefRef,
    pub source: ItemTreeRef,
    pub owner: ModuleRef,
    pub name: String,
    pub visibility: VisibilityLevel,
    pub ty: Option<TypeRef>,
    pub mutability: Mutability,
}

impl ItemStore {
    pub(super) fn alloc_struct(&mut self, data: StructData) -> StructId {
        let id = StructId(self.structs.len());
        self.structs.push(data);
        id
    }

    pub(super) fn alloc_union(&mut self, data: UnionData) -> UnionId {
        let id = UnionId(self.unions.len());
        self.unions.push(data);
        id
    }

    pub(super) fn alloc_enum(&mut self, data: EnumData) -> EnumId {
        let id = EnumId(self.enums.len());
        self.enums.push(data);
        id
    }

    pub(super) fn alloc_trait(&mut self, data: TraitData) -> TraitId {
        let id = TraitId(self.traits.len());
        self.traits.push(data);
        id
    }

    pub(super) fn alloc_impl(&mut self, data: ImplData) -> ImplId {
        let id = ImplId(self.impls.len());
        self.impls.push(data);
        id
    }

    pub(super) fn alloc_function(&mut self, data: FunctionData) -> FunctionId {
        let id = FunctionId(self.functions.len());
        self.functions.push(data);
        id
    }

    pub(super) fn alloc_type_alias(&mut self, data: TypeAliasData) -> TypeAliasId {
        let id = TypeAliasId(self.type_aliases.len());
        self.type_aliases.push(data);
        id
    }

    pub(super) fn alloc_const(&mut self, data: ConstData) -> ConstId {
        let id = ConstId(self.consts.len());
        self.consts.push(data);
        id
    }

    pub(super) fn alloc_static(&mut self, data: StaticData) -> StaticId {
        let id = StaticId(self.statics.len());
        self.statics.push(data);
        id
    }
}

fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}
