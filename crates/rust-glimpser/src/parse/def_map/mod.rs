mod collect;
mod data;
mod ids;
mod import;
mod resolve;

pub(crate) use self::resolve::populate_project_scopes;
pub use self::{
    data::{DefMap, LocalDefData, ModuleData, ModuleOrigin, ModuleScope, ScopeBinding, ScopeEntry},
    ids::{DefId, ImportId, LocalDefId, LocalDefRef, ModuleId, ModuleRef, PackageSlot, TargetRef},
    import::{ImportBinding, ImportData, ImportKind, ImportPath, PathSegment},
};

#[cfg(test)]
mod tests;
