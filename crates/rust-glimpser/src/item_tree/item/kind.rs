use std::fmt;

use super::{
    import::{ExternCrateItem, UseItem},
    module::ModuleItem,
};

/// Payload-bearing item kind.
///
/// Unit variants are enough for plain local definitions. Variants whose syntax
/// matters to later lowering stages carry structured item-tree facts, boxed to
/// keep the enum size stable as those facts grow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    AsmExpr,
    Const,
    Enum,
    ExternBlock,
    ExternCrate(Box<ExternCrateItem>),
    Function,
    Impl,
    MacroDefinition,
    Module(Box<ModuleItem>),
    Static,
    Struct,
    Trait,
    TypeAlias,
    Union,
    Use(Box<UseItem>),
}

impl ItemKind {
    /// Returns payload-independent item classification.
    pub(crate) fn tag(&self) -> ItemTag {
        match self {
            Self::AsmExpr => ItemTag::AsmExpr,
            Self::Const => ItemTag::Const,
            Self::Enum => ItemTag::Enum,
            Self::ExternBlock => ItemTag::ExternBlock,
            Self::ExternCrate(_) => ItemTag::ExternCrate,
            Self::Function => ItemTag::Function,
            Self::Impl => ItemTag::Impl,
            Self::MacroDefinition => ItemTag::MacroDefinition,
            Self::Module(_) => ItemTag::Module,
            Self::Static => ItemTag::Static,
            Self::Struct => ItemTag::Struct,
            Self::Trait => ItemTag::Trait,
            Self::TypeAlias => ItemTag::TypeAlias,
            Self::Union => ItemTag::Union,
            Self::Use(_) => ItemTag::Use,
        }
    }
}

impl fmt::Display for ItemKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.tag().fmt(f)
    }
}

/// Payload-independent item classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemTag {
    AsmExpr,
    Const,
    Enum,
    ExternBlock,
    ExternCrate,
    Function,
    Impl,
    MacroDefinition,
    Module,
    Static,
    Struct,
    Trait,
    TypeAlias,
    Union,
    Use,
}

impl fmt::Display for ItemTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::AsmExpr => "asm",
            Self::Const => "const",
            Self::Enum => "enum",
            Self::ExternBlock => "extern_block",
            Self::ExternCrate => "extern_crate",
            Self::Function => "fn",
            Self::Impl => "impl",
            Self::MacroDefinition => "macro_definition",
            Self::Module => "module",
            Self::Static => "static",
            Self::Struct => "struct",
            Self::Trait => "trait",
            Self::TypeAlias => "type_alias",
            Self::Union => "union",
            Self::Use => "use",
        };
        write!(f, "{value}")
    }
}
