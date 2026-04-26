mod cursor;
mod data;
mod ids;
mod lower;
mod resolution;

#[cfg(test)]
mod tests;

pub use self::{
    cursor::SemanticCursorCandidate,
    data::SemanticIrDb,
    ids::{
        FieldRef, FunctionId, FunctionRef, ImplRef, ItemId, ItemOwner, TraitRef, TypeDefId,
        TypeDefRef,
    },
    resolution::{SemanticTypePathResolution, TypePathContext},
};
