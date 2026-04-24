use crate::{
    def_map::{LocalDefId, LocalDefRef, LocalImplRef, ModuleRef, PackageSlot, TargetRef},
    item_tree::{
        ConstItem, FieldList, FunctionItem, GenericParams, ItemTreeRef, Mutability, TypeAliasItem,
        TypeBound, TypeRef, VisibilityLevel,
    },
    parse::TargetId,
};

use super::{
    ids::{
        AssocItemId, ConstId, EnumId, FunctionId, ImplId, ItemId, ItemOwner, StaticId, StructId,
        TraitId, TypeAliasId, UnionId,
    },
    lower,
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
        lower::build_db(item_tree, def_map)
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
    pub items: Vec<AssocItemId>,
    pub is_unsafe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionData {
    pub local_def: Option<LocalDefRef>,
    pub source: ItemTreeRef,
    pub owner: ItemOwner,
    pub name: String,
    pub visibility: VisibilityLevel,
    pub declaration: FunctionItem,
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
