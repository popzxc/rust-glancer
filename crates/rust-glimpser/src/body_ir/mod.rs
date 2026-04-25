mod data;
mod ids;
mod lower;
mod resolution;

#[cfg(test)]
mod tests;

pub(crate) use self::{
    data::{BindingData, BodyData, BodyIrDb, BodyResolution, BodyTy, ExprData, ExprKind},
    ids::{BindingId, BodyId, BodyRef, ExprId},
};
