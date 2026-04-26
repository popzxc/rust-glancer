mod data;
mod ids;
mod lower;
mod resolution;

#[cfg(test)]
mod tests;

pub(crate) use self::{
    data::{
        BindingData, BodyData, BodyIrDb, BodyItemKind, BodyResolution, BodyTy, ExprData, ExprKind,
        StmtKind,
    },
    ids::{BindingId, BodyId, BodyItemId, BodyItemRef, BodyRef, ExprId, ScopeId},
};
