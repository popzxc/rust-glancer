use crate::def_map::ModuleRef;

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
