use crate::{file::FileId, span::Span};

/// A parse diagnostic attached to a specific source file and span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Identifier of the file that produced this diagnostic.
    pub file_id: FileId,
    /// Human-readable parse error message.
    pub message: String,
    /// Location of the parse error in both byte and line/column space.
    pub span: Span,
}

impl ParseError {
    pub(crate) fn shrink_to_fit(&mut self) {
        self.message.shrink_to_fit();
    }
}
