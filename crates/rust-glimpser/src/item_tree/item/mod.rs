use ra_syntax::TextRange;

use crate::parse::{
    FileId,
    span::{LineIndex, Span},
};

pub(crate) use self::{
    import::{
        ExternCrateItem, ImportAlias, UseImport, UseImportKind, UseItem, UsePath, UsePathSegment,
    },
    kind::{ItemKind, ItemTag},
    module::{ModuleItem, ModuleSource},
    visibility::VisibilityLevel,
};

mod import;
mod kind;
mod module;
mod visibility;

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
