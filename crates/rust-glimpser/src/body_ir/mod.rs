mod cursor;
mod data;
mod ids;
mod lower;
mod resolution;

#[cfg(test)]
mod tests;

pub(crate) use self::{
    cursor::BodyCursorCandidate,
    data::{
        BindingData, BodyData, BodyIrDb, BodyItemKind, BodyResolution, BodyTy,
        BodyTypePathResolution, ExprData, ExprKind, StmtKind,
    },
    ids::{BindingId, BodyId, BodyItemId, BodyItemRef, BodyRef, ExprId, ScopeId},
};
