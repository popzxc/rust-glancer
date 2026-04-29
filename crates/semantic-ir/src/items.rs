use rg_def_map::{LocalDefRef, LocalImplRef, ModuleRef};
use rg_item_tree::{
    ConstItem, Documentation, EnumVariantItem, FieldItem, FieldList, FunctionItem, GenericParams,
    ItemTreeRef, Mutability, ParamKind, TypeAliasItem, TypeBound, TypeRef, VisibilityLevel,
};
use rg_parse::{FileId, Span};

use crate::ids::{
    AssocItemId, ConstId, EnumId, FunctionId, ImplId, ItemOwner, StaticId, StructId, TraitId,
    TraitRef, TypeAliasId, TypeDefRef, UnionId,
};

/// Target-local storage for semantic items.
///
/// Semantic ids are dense indexes into these vectors. Keeping all item families in one store lets
/// lowering allocate ids cheaply while the public query surface exposes stable typed references.
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

impl ItemStore {
    pub fn struct_data(&self, id: StructId) -> Option<&StructData> {
        self.structs.get(id.0)
    }

    pub fn union_data(&self, id: UnionId) -> Option<&UnionData> {
        self.unions.get(id.0)
    }

    pub fn enum_data(&self, id: EnumId) -> Option<&EnumData> {
        self.enums.get(id.0)
    }

    pub fn trait_data(&self, id: TraitId) -> Option<&TraitData> {
        self.traits.get(id.0)
    }

    pub fn impl_data(&self, id: ImplId) -> Option<&ImplData> {
        self.impls.get(id.0)
    }

    pub fn function_data(&self, id: FunctionId) -> Option<&FunctionData> {
        self.functions.get(id.0)
    }

    pub fn type_alias_data(&self, id: TypeAliasId) -> Option<&TypeAliasData> {
        self.type_aliases.get(id.0)
    }

    pub fn const_data(&self, id: ConstId) -> Option<&ConstData> {
        self.consts.get(id.0)
    }

    pub fn static_data(&self, id: StaticId) -> Option<&StaticData> {
        self.statics.get(id.0)
    }
}

impl ItemStore {
    pub(crate) fn alloc_struct(&mut self, data: StructData) -> StructId {
        let id = StructId(self.structs.len());
        self.structs.push(data);
        id
    }

    pub(crate) fn alloc_union(&mut self, data: UnionData) -> UnionId {
        let id = UnionId(self.unions.len());
        self.unions.push(data);
        id
    }

    pub(crate) fn alloc_enum(&mut self, data: EnumData) -> EnumId {
        let id = EnumId(self.enums.len());
        self.enums.push(data);
        id
    }

    pub(crate) fn alloc_trait(&mut self, data: TraitData) -> TraitId {
        let id = TraitId(self.traits.len());
        self.traits.push(data);
        id
    }

    pub(crate) fn alloc_impl(&mut self, data: ImplData) -> ImplId {
        let id = ImplId(self.impls.len());
        self.impls.push(data);
        id
    }

    pub(crate) fn alloc_function(&mut self, data: FunctionData) -> FunctionId {
        let id = FunctionId(self.functions.len());
        self.functions.push(data);
        id
    }

    pub(crate) fn alloc_type_alias(&mut self, data: TypeAliasData) -> TypeAliasId {
        let id = TypeAliasId(self.type_aliases.len());
        self.type_aliases.push(data);
        id
    }

    pub(crate) fn alloc_const(&mut self, data: ConstData) -> ConstId {
        let id = ConstId(self.consts.len());
        self.consts.push(data);
        id
    }

    pub(crate) fn alloc_static(&mut self, data: StaticData) -> StaticId {
        let id = StaticId(self.statics.len());
        self.statics.push(data);
        id
    }
}

/// Borrowed view over one field plus the semantic owner facts needed by analysis.
#[derive(Debug, Clone, Copy)]
pub struct FieldData<'a> {
    pub owner_module: ModuleRef,
    pub file_id: FileId,
    pub field: &'a FieldItem,
}

/// Borrowed view over one enum variant plus the owning enum facts needed by analysis.
///
/// The owner data is repeated here so callers do not have to re-open the enum just to answer
/// editor questions such as "what type does this variant construct?".
#[derive(Debug, Clone, Copy)]
pub struct EnumVariantData<'a> {
    pub owner: TypeDefRef,
    pub owner_module: ModuleRef,
    pub file_id: FileId,
    pub variant: &'a EnumVariantItem,
}

/// Nominal struct lowered from a module item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructData {
    pub local_def: LocalDefRef,
    pub source: ItemTreeRef,
    pub owner: ModuleRef,
    pub name: String,
    pub visibility: VisibilityLevel,
    pub docs: Option<Documentation>,
    pub generics: GenericParams,
    pub fields: FieldList,
}

/// Nominal union lowered from a module item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnionData {
    pub local_def: LocalDefRef,
    pub source: ItemTreeRef,
    pub owner: ModuleRef,
    pub name: String,
    pub visibility: VisibilityLevel,
    pub docs: Option<Documentation>,
    pub generics: GenericParams,
    pub fields: Vec<FieldItem>,
}

/// Enum definition together with variant payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumData {
    pub local_def: LocalDefRef,
    pub source: ItemTreeRef,
    pub owner: ModuleRef,
    pub name: String,
    pub visibility: VisibilityLevel,
    pub docs: Option<Documentation>,
    pub generics: GenericParams,
    pub variants: Vec<EnumVariantItem>,
}

/// Trait signature and associated items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitData {
    pub local_def: LocalDefRef,
    pub source: ItemTreeRef,
    pub owner: ModuleRef,
    pub name: String,
    pub visibility: VisibilityLevel,
    pub docs: Option<Documentation>,
    pub generics: GenericParams,
    pub super_traits: Vec<TypeBound>,
    pub items: Vec<AssocItemId>,
    pub is_unsafe: bool,
}

/// Impl block header and associated items.
///
/// `resolved_*` fields are intentionally lossy and optimistic: they record all type/trait targets
/// that our current path resolver can identify, without attempting a real trait solver.
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

/// Function signature and source identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionData {
    pub local_def: Option<LocalDefRef>,
    pub source: ItemTreeRef,
    pub span: Span,
    pub name_span: Option<Span>,
    pub owner: ItemOwner,
    pub name: String,
    pub visibility: VisibilityLevel,
    pub docs: Option<Documentation>,
    pub declaration: FunctionItem,
}

impl FunctionData {
    pub fn has_self_receiver(&self) -> bool {
        self.declaration
            .params
            .first()
            .is_some_and(|param| matches!(param.kind, ParamKind::SelfParam))
    }
}

/// Type alias signature and optional aliased type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAliasData {
    pub local_def: Option<LocalDefRef>,
    pub source: ItemTreeRef,
    pub span: Span,
    pub name_span: Option<Span>,
    pub owner: ItemOwner,
    pub name: String,
    pub visibility: VisibilityLevel,
    pub docs: Option<Documentation>,
    pub declaration: TypeAliasItem,
}

/// Const signature and optional value body owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstData {
    pub local_def: Option<LocalDefRef>,
    pub source: ItemTreeRef,
    pub span: Span,
    pub name_span: Option<Span>,
    pub owner: ItemOwner,
    pub name: String,
    pub visibility: VisibilityLevel,
    pub docs: Option<Documentation>,
    pub declaration: ConstItem,
}

/// Module-level static item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticData {
    pub local_def: LocalDefRef,
    pub source: ItemTreeRef,
    pub span: Span,
    pub name_span: Option<Span>,
    pub owner: ModuleRef,
    pub name: String,
    pub visibility: VisibilityLevel,
    pub docs: Option<Documentation>,
    pub ty: Option<TypeRef>,
    pub mutability: Mutability,
}
