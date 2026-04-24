mod collect;
mod data;
mod ids;
mod import;
mod path_resolution;
mod resolve;

pub use self::{
    data::{
        DefMap, LocalDefData, LocalDefKind, LocalImplData, ModuleData, ModuleOrigin, ModuleScope,
        ScopeBinding, ScopeEntry,
    },
    ids::{
        DefId, ImportId, LocalDefId, LocalDefRef, LocalImplId, LocalImplRef, ModuleId, ModuleRef,
        PackageSlot, TargetRef,
    },
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
    pub(crate) fn build(
        workspace: &crate::workspace_metadata::WorkspaceMetadata,
        parse: &parse::ParseDb,
        item_tree: &ItemTreeDb,
    ) -> anyhow::Result<Self> {
        resolve::build_db(workspace, parse, item_tree)
    }

    /// Returns all package-level def-map sets.
    pub(crate) fn packages(&self) -> &[Package] {
        &self.packages
    }

    /// Iterates over every target def map together with its project-wide target reference.
    pub(crate) fn target_maps(&self) -> impl Iterator<Item = (TargetRef, &DefMap)> {
        self.packages()
            .iter()
            .enumerate()
            .flat_map(move |(package_idx, package)| {
                (0..package.targets().len()).filter_map(move |target_idx| {
                    let target_ref = TargetRef {
                        package: PackageSlot(package_idx),
                        target: crate::parse::TargetId(target_idx),
                    };
                    self.def_map(target_ref)
                        .map(|def_map| (target_ref, def_map))
                })
            })
    }

    /// Returns coarse DefMap totals for the current project report.
    pub(crate) fn stats(&self) -> DefMapStats {
        let mut stats = DefMapStats::default();

        for (_, target) in self.target_maps() {
            stats.target_count += 1;
            stats.module_count += target.modules().len();
            stats.local_def_count += target.local_defs().len();
            stats.local_impl_count += target.local_impls().len();
            stats.import_count += target.imports().len();
            stats.unresolved_import_count += target
                .modules()
                .iter()
                .map(|module| module.unresolved_imports.len())
                .sum::<usize>();
        }

        stats
    }

    /// Returns one package def-map set by package slot.
    pub(crate) fn package(&self, package_slot: PackageSlot) -> Option<&Package> {
        self.packages.get(package_slot.0)
    }

    /// Returns one target def map by project-wide target reference.
    pub(crate) fn def_map(&self, target: TargetRef) -> Option<&DefMap> {
        self.package(target.package)?.target(target.target)
    }
}

/// Coarse totals for reporting that the DefMap phase produced useful data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct DefMapStats {
    pub(crate) target_count: usize,
    pub(crate) module_count: usize,
    pub(crate) local_def_count: usize,
    pub(crate) local_impl_count: usize,
    pub(crate) import_count: usize,
    pub(crate) unresolved_import_count: usize,
}

/// Def maps for all targets inside one parsed package.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Package {
    targets: Vec<DefMap>,
}

impl Package {
    /// Returns all target def maps in target-id order.
    pub(crate) fn targets(&self) -> &[DefMap] {
        &self.targets
    }

    /// Returns one target def map by target id.
    pub(crate) fn target(&self, target_id: crate::parse::TargetId) -> Option<&DefMap> {
        self.targets.get(target_id.0)
    }
}

#[cfg(test)]
mod tests;
