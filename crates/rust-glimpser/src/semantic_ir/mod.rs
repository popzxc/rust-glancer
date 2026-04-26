mod cursor;
mod data;
mod ids;
mod lower;
mod resolution;

#[cfg(test)]
mod tests;

pub(crate) use self::{
    cursor::SemanticCursorCandidate,
    data::SemanticIrDb,
    ids::{FieldRef, FunctionId, FunctionRef, ImplRef, ItemOwner, TraitRef, TypeDefRef},
    resolution::{SemanticTypePathResolution, TypePathContext},
};

#[cfg(test)]
pub(crate) use self::ids::{ItemId, TypeDefId};
