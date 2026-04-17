use ra_syntax::{
    TextRange,
    ast::{self, AstNode},
};

use crate::item_tree::{
    file::FileId,
    span::{LineIndex, Span},
};

pub(crate) use self::types::{ItemKind, VisibilityLevel};

mod types;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemNode {
    pub kind: ItemKind,
    pub name: Option<String>,
    pub visibility: VisibilityLevel,
    pub file_id: FileId,
    pub span: Span,
    pub children: Vec<ItemNode>,
}

impl ItemNode {
    pub(crate) fn new(
        kind: ItemKind,
        name: Option<String>,
        visibility: VisibilityLevel,
        text_range: TextRange,
        file_id: FileId,
        line_index: &LineIndex,
        children: Vec<ItemNode>,
    ) -> Self {
        Self {
            kind,
            name,
            visibility,
            file_id,
            span: Span::from_text_range(text_range, line_index),
            children,
        }
    }

    pub(crate) fn use_name(use_item: &ast::Use) -> Option<String> {
        let use_tree = use_item.use_tree()?;
        let text = use_tree.syntax().text().to_string();
        Some(Self::collapse_whitespace(&text))
    }

    fn collapse_whitespace(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}
