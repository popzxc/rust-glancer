mod data;
mod ids;
mod lower;
mod resolution;

#[cfg(test)]
mod tests;

pub(crate) use self::{
    data::{FunctionData, SemanticIrDb},
    ids::{FunctionId, FunctionRef, ImplRef, ItemId, ItemOwner, TraitRef, TypeDefRef},
};

#[cfg(test)]
pub(crate) use self::ids::TypeDefId;
