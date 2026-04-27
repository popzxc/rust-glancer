use rg_def_map::DefId;
use rg_semantic_ir::{FieldRef, FunctionRef, TraitRef, TypeDefRef};

use crate::ids::{BindingId, BodyFieldRef, BodyFunctionRef, BodyItemRef};

/// Stable field identity across module-level Semantic IR and body-local declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResolvedFieldRef {
    Semantic(FieldRef),
    BodyLocal(BodyFieldRef),
}

/// Stable function identity across module-level Semantic IR and body-local declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResolvedFunctionRef {
    Semantic(FunctionRef),
    BodyLocal(BodyFunctionRef),
}

/// Best-effort semantic resolution attached to body expressions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum BodyResolution {
    Local(BindingId),
    LocalItem(BodyItemRef),
    Item(Vec<DefId>),
    Field(Vec<ResolvedFieldRef>),
    Method(Vec<ResolvedFunctionRef>),
    #[default]
    Unknown,
}

/// Body-scoped type path resolution result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyTypePathResolution {
    BodyLocal(BodyItemRef),
    SelfType(Vec<TypeDefRef>),
    TypeDefs(Vec<TypeDefRef>),
    Traits(Vec<TraitRef>),
    Unknown,
}
