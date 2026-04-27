use rg_item_tree::TypeRef;

use crate::{
    body::BodySource,
    ids::{BindingId, BodyImplId, BodyItemId, ExprId, PatId, ScopeId},
    ty::BodyTy,
};

/// One local binding introduced by a parameter or `let`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingData {
    pub source: BodySource,
    pub scope: ScopeId,
    pub kind: BindingKind,
    pub name: Option<String>,
    pub pat: String,
    pub annotation: Option<TypeRef>,
    pub ty: BodyTy,
}

/// Local binding category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum BindingKind {
    #[display("param")]
    Param,
    #[display("self_param")]
    SelfParam,
    #[display("let")]
    Let,
}

/// One lowered statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StmtData {
    pub source: BodySource,
    pub kind: StmtKind,
}

/// Statement forms that matter for the first Body IR pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StmtKind {
    Let {
        scope: ScopeId,
        pat: Option<PatId>,
        bindings: Vec<BindingId>,
        annotation: Option<TypeRef>,
        initializer: Option<ExprId>,
    },
    Expr {
        expr: ExprId,
        has_semicolon: bool,
    },
    Item {
        item: BodyItemId,
    },
    Impl {
        impl_id: BodyImplId,
    },
    ItemIgnored,
}
