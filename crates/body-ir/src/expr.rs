use rg_def_map::Path;
use rg_item_tree::FieldKey;
use rg_parse::Span;

use crate::{
    body::BodySource,
    ids::{ExprId, ScopeId, StmtId},
    resolved::BodyResolution,
    ty::BodyTy,
};

/// One lowered expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprData {
    pub source: BodySource,
    pub scope: ScopeId,
    /// Number of body-wide bindings that were visible at this expression's source location.
    ///
    /// Scope data is frozen after lowering, so the resolver needs this boundary to avoid letting a
    /// later `let x` shadow an earlier use of `x`.
    pub visible_bindings: usize,
    pub kind: ExprKind,
    pub resolution: BodyResolution,
    pub ty: BodyTy,
}

/// Expression forms that the first Body IR pass understands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprKind {
    Block {
        scope: ScopeId,
        statements: Vec<StmtId>,
        tail: Option<ExprId>,
    },
    Path {
        path: Path,
    },
    Call {
        callee: Option<ExprId>,
        args: Vec<ExprId>,
    },
    MethodCall {
        receiver: Option<ExprId>,
        dot_span: Option<Span>,
        method_name: String,
        method_name_span: Option<Span>,
        args: Vec<ExprId>,
    },
    Field {
        base: Option<ExprId>,
        dot_span: Option<Span>,
        field: Option<FieldKey>,
        field_span: Option<Span>,
    },
    Literal {
        text: String,
        kind: LiteralKind,
    },
    Unknown {
        text: String,
        children: Vec<ExprId>,
    },
}

/// Literal category used for display and future cheap inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum LiteralKind {
    #[display("bool")]
    Bool,
    #[display("char")]
    Char,
    #[display("float")]
    Float,
    #[display("int")]
    Int,
    #[display("string")]
    String,
    #[display("unknown")]
    Unknown,
}
