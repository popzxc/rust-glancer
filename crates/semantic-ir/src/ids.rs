use rg_def_map::{ModuleRef, TargetRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnionId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraitId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImplId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeAliasId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StaticId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeDefId {
    Struct(StructId),
    Enum(EnumId),
    Union(UnionId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeDefRef {
    pub target: TargetRef,
    pub id: TypeDefId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraitRef {
    pub target: TargetRef,
    pub id: TraitId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImplRef {
    pub target: TargetRef,
    pub id: ImplId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionRef {
    pub target: TargetRef,
    pub id: FunctionId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeAliasRef {
    pub target: TargetRef,
    pub id: TypeAliasId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstRef {
    pub target: TargetRef,
    pub id: ConstId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StaticRef {
    pub target: TargetRef,
    pub id: StaticId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldRef {
    pub owner: TypeDefRef,
    pub index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraitImplRef {
    pub impl_ref: ImplRef,
    pub trait_ref: TraitRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemId {
    Struct(StructId),
    Union(UnionId),
    Enum(EnumId),
    Trait(TraitId),
    Function(FunctionId),
    TypeAlias(TypeAliasId),
    Const(ConstId),
    Static(StaticId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssocItemId {
    Function(FunctionId),
    TypeAlias(TypeAliasId),
    Const(ConstId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemOwner {
    Module(ModuleRef),
    Trait(TraitId),
    Impl(ImplId),
}
