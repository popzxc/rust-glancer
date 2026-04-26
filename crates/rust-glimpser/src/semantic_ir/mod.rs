mod data;
mod ids;
mod lower;
mod resolution;

#[cfg(test)]
mod tests;

pub(crate) use self::{
    data::{FunctionData, SemanticIrDb},
    ids::{
        FieldRef, FunctionId, FunctionRef, ImplId, ImplRef, ItemId, ItemOwner, StructId, TraitRef,
        TypeDefId, TypeDefRef, UnionId,
    },
    resolution::{SemanticTypePathResolution, TypePathContext},
};
