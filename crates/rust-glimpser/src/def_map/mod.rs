mod collect;
mod data;
mod ids;
mod import;
mod path_resolution;
mod resolve;

pub use self::{
    data::{
        DefMap, LocalDefData, LocalDefKind, ModuleData, ModuleOrigin, ModuleScope, ScopeBinding,
        ScopeEntry,
    },
    ids::{DefId, ImportId, LocalDefId, LocalDefRef, ModuleId, ModuleRef, PackageSlot, TargetRef},
    import::{ImportBinding, ImportData, ImportKind, ImportPath, PathSegment},
};
use crate::{item_tree::ItemTreeDb, parse};

/// Frozen def maps for all parsed packages and targets.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DefMapDb {
    packages: Vec<Package>,
}

impl DefMapDb {
    /// Builds target-local def maps from parsed project metadata and lowered item trees.
    pub(crate) fn build(parse: &parse::ParseDb, item_tree: &ItemTreeDb) -> anyhow::Result<Self> {
        resolve::build_db(parse, item_tree)
    }

    /// Returns one package def-map set by package slot.
    #[cfg(test)]
    pub(crate) fn package(&self, package_slot: usize) -> Option<&Package> {
        self.packages.get(package_slot)
    }

    /// Returns one target def map by project-wide target reference.
    #[cfg(test)]
    pub(crate) fn def_map(&self, target: TargetRef) -> Option<&DefMap> {
        self.package(target.package.0)?.target(target.target)
    }
}

/// Def maps for all targets inside one parsed package.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Package {
    targets: Vec<DefMap>,
}

impl Package {
    /// Returns one target def map by target id.
    #[cfg(test)]
    pub(crate) fn target(&self, target_id: crate::parse::TargetId) -> Option<&DefMap> {
        self.targets.get(target_id.0)
    }
}

#[cfg(test)]
mod tests;
