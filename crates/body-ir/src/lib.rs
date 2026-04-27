mod cursor;
mod data;
mod ids;
mod lower;
mod resolution;

#[cfg(test)]
mod tests;

pub use self::{
    cursor::{BodyCursorCandidate, DotReceiver},
    data::{
        BindingData, BodyData, BodyFieldData, BodyFunctionData, BodyFunctionOwner, BodyImplData,
        BodyIrDb, BodyItemKind, BodyResolution, BodyTy, BodyTypePathResolution, ExprData, ExprKind,
        ResolvedFieldRef, ResolvedFunctionRef, StmtKind,
    },
    ids::{
        BindingId, BodyFieldRef, BodyFunctionId, BodyFunctionRef, BodyId, BodyImplId, BodyItemId,
        BodyItemRef, BodyRef, ExprId, ScopeId,
    },
};
