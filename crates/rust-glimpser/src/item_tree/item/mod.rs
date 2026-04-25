use ra_syntax::TextRange;

use crate::parse::{
    FileId,
    span::{LineIndex, Span},
};

pub(crate) use self::{
    decl::{
        ConstItem, EnumItem, EnumVariantItem, FieldItem, FieldKey, FieldList, FunctionItem,
        GenericParams, ImplItem, ParamKind, StaticItem, StructItem, TraitItem, TypeAliasItem,
        UnionItem,
    },
    import::{
        ExternCrateItem, ImportAlias, UseImport, UseImportKind, UseItem, UsePath, UsePathSegment,
    },
    kind::{ItemKind, ItemTag},
    module::{ModuleItem, ModuleSource},
    type_ref::{Mutability, TypeBound, TypeRef},
    visibility::VisibilityLevel,
};

mod decl;
mod import;
mod kind;
mod module;
mod type_ref;
mod visibility;

#[cfg(test)]
pub(crate) use self::decl::ParamItem;

/// Stable file-local identifier for one lowered item-tree node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ItemTreeId(pub usize);

/// Stable project-local reference to one item-tree node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ItemTreeRef {
    pub file_id: FileId,
    pub item: ItemTreeId,
}

/// AST-independent item-tree node used by later lowering stages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemNode {
    pub kind: ItemKind,
    /// Name (when applicable), e.g. for functions or structs.
    pub name: Option<String>,
    pub visibility: VisibilityLevel,
    /// File where this item is declared.
    pub file_id: FileId,
    /// Source span of the declaration.
    pub span: Span,
}

impl ItemNode {
    /// Creates a fully-populated item node from already-lowered parts.
    pub(super) fn new(
        kind: ItemKind,
        name: Option<String>,
        visibility: VisibilityLevel,
        text_range: TextRange,
        file_id: FileId,
        line_index: &LineIndex,
    ) -> Self {
        Self {
            kind,
            name,
            visibility,
            file_id,
            span: Span::from_text_range(text_range, line_index),
        }
    }
}

pub(crate) fn normalized_syntax(node: &impl ra_syntax::AstNode) -> String {
    node.syntax()
        .text()
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
