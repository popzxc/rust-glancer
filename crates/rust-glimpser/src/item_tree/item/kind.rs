use super::{
    import::{ExternCrateItem, UseItem},
    module::ModuleItem,
};

/// Payload-bearing item kind.
///
/// Unit variants are enough for plain local definitions. Variants whose syntax
/// matters to later lowering stages carry structured item-tree facts, boxed to
/// keep the enum size stable as those facts grow.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum ItemKind {
    #[display("asm")]
    AsmExpr,
    #[display("const")]
    Const,
    #[display("enum")]
    Enum,
    #[display("extern_block")]
    ExternBlock,
    #[display("extern_crate")]
    ExternCrate(Box<ExternCrateItem>),
    #[display("fn")]
    Function,
    #[display("impl")]
    Impl,
    #[display("macro_definition")]
    MacroDefinition,
    #[display("module")]
    Module(Box<ModuleItem>),
    #[display("static")]
    Static,
    #[display("struct")]
    Struct,
    #[display("trait")]
    Trait,
    #[display("type_alias")]
    TypeAlias,
    #[display("union")]
    Union,
    #[display("use")]
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

/// Payload-independent item classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum ItemTag {
    #[display("asm")]
    AsmExpr,
    #[display("const")]
    Const,
    #[display("enum")]
    Enum,
    #[display("extern_block")]
    ExternBlock,
    #[display("extern_crate")]
    ExternCrate,
    #[display("fn")]
    Function,
    #[display("impl")]
    Impl,
    #[display("macro_definition")]
    MacroDefinition,
    #[display("module")]
    Module,
    #[display("static")]
    Static,
    #[display("struct")]
    Struct,
    #[display("trait")]
    Trait,
    #[display("type_alias")]
    TypeAlias,
    #[display("union")]
    Union,
    #[display("use")]
    Use,
}
