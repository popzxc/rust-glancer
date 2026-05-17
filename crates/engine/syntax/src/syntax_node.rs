//! Concrete syntax tree cursors and the parser-only tree builder.
//!
//! Public callers receive immutable nodes and tokens. The builder stays crate-private so the only
//! supported way to obtain syntax is by parsing source text.

use rowan::{GreenNodeBuilder, Language};

use crate::{SyntaxError, SyntaxKind, TextSize};

pub(crate) use rowan::GreenNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RustLanguage {}
impl Language for RustLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        SyntaxKind::from(raw.0)
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind.into())
    }
}

pub type SyntaxNode = rowan::SyntaxNode<RustLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<RustLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<RustLanguage>;
pub type SyntaxNodeChildren = rowan::SyntaxNodeChildren<RustLanguage>;
pub type SyntaxElementChildren = rowan::SyntaxElementChildren<RustLanguage>;
pub type PreorderWithTokens = rowan::api::PreorderWithTokens<RustLanguage>;

#[derive(Default)]
pub(crate) struct SyntaxTreeBuilder {
    errors: Vec<SyntaxError>,
    inner: GreenNodeBuilder<'static>,
}

impl SyntaxTreeBuilder {
    pub(crate) fn finish_raw(self) -> (GreenNode, Vec<SyntaxError>) {
        let green = self.inner.finish();
        (green, self.errors)
    }

    pub(crate) fn token(&mut self, kind: SyntaxKind, text: &str) {
        let kind = RustLanguage::kind_to_raw(kind);
        self.inner.token(kind, text);
    }

    pub(crate) fn start_node(&mut self, kind: SyntaxKind) {
        let kind = RustLanguage::kind_to_raw(kind);
        self.inner.start_node(kind);
    }

    pub(crate) fn finish_node(&mut self) {
        self.inner.finish_node();
    }

    pub(crate) fn error(&mut self, error: String, text_pos: TextSize) {
        self.errors
            .push(SyntaxError::new_at_offset(error, text_pos));
    }
}
