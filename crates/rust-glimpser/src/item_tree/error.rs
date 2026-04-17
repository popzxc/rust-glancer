use crate::item_tree::{file::FileId, span::Span};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub file_id: FileId,
    pub message: String,
    pub span: Span,
}
