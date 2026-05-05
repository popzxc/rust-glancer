//! Resident def-map database and package-level query helpers.

use rg_item_tree::ItemTreeDb;
use rg_package_store::{PackageLoader, PackageStore, PackageSubset};
use rg_parse::{self, TargetId};
use rg_text::NameInterner;
use rg_workspace::WorkspaceMetadata;

use crate::{
    DefMap, DefMapReadTxn, ImportData, ImportId, ImportRef, LocalDefData, LocalDefId, LocalDefRef,
    LocalImplData, LocalImplId, LocalImplRef, ModuleData, ModuleId, ModuleRef, Package,
    PackageSlot, Path, ResidentTargetRef, ResolvePathResult, TargetRef, path_resolution, resolve,
};

/// Frozen def maps for all parsed packages and targets.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DefMapDb {
    pub(crate) packages: PackageStore<Package>,
}

impl DefMapDb {
    /// Builds target-local def maps from parsed project metadata and lowered item trees.
    pub fn build(
        workspace: &WorkspaceMetadata,
        parse: &rg_parse::ParseDb,
        item_tree: &ItemTreeDb,
    ) -> anyhow::Result<Self> {
        let mut interner = NameInterner::new();
        Self::build_with_interner(workspace, parse, item_tree, &mut interner)
    }

    /// Builds target-local def maps using a caller-retained name interner.
    pub fn build_with_interner(
        workspace: &WorkspaceMetadata,
        parse: &rg_parse::ParseDb,
        item_tree: &ItemTreeDb,
        interner: &mut NameInterner,
    ) -> anyhow::Result<Self> {
        let mut db = resolve::build_db(workspace, parse, item_tree, interner)?;
        db.shrink_to_fit();
        Ok(db)
    }

    /// Returns a new def-map snapshot with selected packages rebuilt against a logical old view.
    pub fn rebuild_packages_with_interner_and_read_txn(
        &self,
        old_read: &DefMapReadTxn<'_>,
        workspace: &WorkspaceMetadata,
        parse: &rg_parse::ParseDb,
        item_tree: &ItemTreeDb,
        packages: &[PackageSlot],
        interner: &mut NameInterner,
    ) -> anyhow::Result<Self> {
        let mut db = resolve::rebuild_packages(
            self, old_read, workspace, parse, item_tree, packages, interner,
        )?;
        db.shrink_packages(packages);
        Ok(db)
    }

    pub fn package_count(&self) -> usize {
        self.packages.len()
    }

    /// Iterates over every resident target def map together with a resident-only target reference.
    pub fn resident_target_maps(&self) -> impl Iterator<Item = (ResidentTargetRef, &DefMap)> {
        self.packages
            .raw_entries_with_slots()
            .filter_map(|(package_slot, entry)| {
                entry.as_resident().map(|package| (package_slot, package))
            })
            .flat_map(move |(package_slot, package)| {
                package
                    .targets()
                    .iter()
                    .enumerate()
                    .map(move |(target_idx, def_map)| {
                        let target_ref = ResidentTargetRef {
                            package: package_slot,
                            target: TargetId(target_idx),
                        };
                        (target_ref, def_map)
                    })
            })
    }

    /// Returns coarse DefMap totals for the current project report.
    pub fn stats(&self) -> DefMapStats {
        let mut stats = DefMapStats::default();

        for (_, target) in self.resident_target_maps() {
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

    /// Returns one resident package def-map set by package slot.
    pub fn resident_package(&self, package_slot: PackageSlot) -> Option<&Package> {
        self.packages
            .raw_entry(package_slot)
            .and_then(|entry| entry.as_resident())
    }

    pub fn read_txn<'db>(&'db self, loader: PackageLoader<'db, Package>) -> DefMapReadTxn<'db> {
        DefMapReadTxn::from_package_store(self.packages.read_txn(loader))
    }

    pub fn read_txn_for_subset<'db>(
        &'db self,
        loader: PackageLoader<'db, Package>,
        subset: &PackageSubset,
    ) -> DefMapReadTxn<'db> {
        DefMapReadTxn::from_package_store(self.packages.read_txn_for_subset(loader, subset))
    }

    pub fn replace_package(&mut self, package_slot: PackageSlot, package: Package) -> Option<()> {
        self.packages.replace(package_slot, package)
    }

    pub fn offload_package(&mut self, package_slot: PackageSlot) -> Option<()> {
        self.packages.offload(package_slot)
    }

    /// Returns one resident target def map by project-wide target reference.
    pub fn resident_def_map(&self, target: TargetRef) -> Option<&DefMap> {
        self.resident_package(target.package)?.target(target.target)
    }

    /// Iterates over one target's modules together with stable project-wide references.
    #[allow(dead_code)]
    pub fn modules(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (ModuleRef, &ModuleData)> + '_ {
        self.resident_def_map(target)
            .into_iter()
            .flat_map(move |def_map| {
                def_map
                    .modules()
                    .iter()
                    .enumerate()
                    .map(move |(module_idx, module)| {
                        (
                            ModuleRef {
                                target,
                                module: ModuleId(module_idx),
                            },
                            module,
                        )
                    })
            })
    }

    /// Returns one module by stable project-wide reference.
    pub fn module(&self, module: ModuleRef) -> Option<&ModuleData> {
        self.resident_def_map(module.target)?.module(module.module)
    }

    /// Iterates over one target's local definitions together with stable project-wide references.
    pub fn local_defs(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (LocalDefRef, &LocalDefData)> + '_ {
        self.resident_def_map(target)
            .into_iter()
            .flat_map(move |def_map| {
                def_map
                    .local_defs()
                    .iter()
                    .enumerate()
                    .map(move |(local_def_idx, local_def)| {
                        (
                            LocalDefRef {
                                target,
                                local_def: LocalDefId(local_def_idx),
                            },
                            local_def,
                        )
                    })
            })
    }

    /// Returns one local definition by stable project-wide reference.
    pub fn local_def(&self, local_def: LocalDefRef) -> Option<&LocalDefData> {
        self.resident_def_map(local_def.target)?
            .local_def(local_def.local_def)
    }

    /// Iterates over one target's impl blocks together with stable project-wide references.
    pub fn local_impls(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (LocalImplRef, &LocalImplData)> + '_ {
        self.resident_def_map(target)
            .into_iter()
            .flat_map(move |def_map| {
                def_map
                    .local_impls()
                    .iter()
                    .enumerate()
                    .map(move |(local_impl_idx, local_impl)| {
                        (
                            LocalImplRef {
                                target,
                                local_impl: LocalImplId(local_impl_idx),
                            },
                            local_impl,
                        )
                    })
            })
    }

    /// Returns one impl block by stable project-wide reference.
    #[allow(dead_code)]
    pub fn local_impl(&self, local_impl: LocalImplRef) -> Option<&LocalImplData> {
        self.resident_def_map(local_impl.target)?
            .local_impl(local_impl.local_impl)
    }

    /// Iterates over one target's imports together with stable project-wide references.
    pub fn imports(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (ImportRef, &ImportData)> + '_ {
        self.resident_def_map(target)
            .into_iter()
            .flat_map(move |def_map| {
                def_map
                    .imports()
                    .iter()
                    .enumerate()
                    .map(move |(import_idx, import)| {
                        (
                            ImportRef {
                                target,
                                import: ImportId(import_idx),
                            },
                            import,
                        )
                    })
            })
    }

    /// Returns one import by stable project-wide reference.
    #[allow(dead_code)]
    pub fn import(&self, import: ImportRef) -> Option<&ImportData> {
        self.resident_def_map(import.target)?.import(import.import)
    }

    /// Resolves a path from a module against the frozen def-map graph.
    #[allow(dead_code)]
    pub fn resolve_path(&self, from: ModuleRef, path: &Path) -> ResolvePathResult {
        path_resolution::resolve_path_in_db(self, from, path)
    }

    /// Resolves a path whose terminal segment is used in the type namespace.
    pub fn resolve_path_in_type_namespace(
        &self,
        from: ModuleRef,
        path: &Path,
    ) -> ResolvePathResult {
        path_resolution::resolve_path_in_type_namespace(self, from, path)
    }

    fn shrink_to_fit(&mut self) {
        self.packages.shrink_to_fit();
        for entry in self.packages.raw_entries_mut() {
            if let Some(package) = entry.as_resident_unique_mut() {
                package.shrink_to_fit();
            }
        }
    }

    fn shrink_packages(&mut self, packages: &[PackageSlot]) {
        for package in packages {
            if let Some(package) = self.packages.get_unique_mut(*package) {
                package.shrink_to_fit();
            }
        }
    }
}

/// Coarse totals for reporting that the DefMap phase produced useful data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DefMapStats {
    pub target_count: usize,
    pub module_count: usize,
    pub local_def_count: usize,
    pub local_impl_count: usize,
    pub import_count: usize,
    pub unresolved_import_count: usize,
}

#[cfg(test)]
mod tests {
    use rg_arena::Arena;

    use super::*;

    #[test]
    fn target_maps_preserve_package_slots_when_middle_package_is_offloaded() {
        let mut db = DefMapDb {
            packages: PackageStore::from_vec(vec![
                package_with_one_target("workspace"),
                package_with_one_target("offloaded"),
                package_with_one_target("dependency"),
            ]),
        };

        db.offload_package(PackageSlot(1))
            .expect("middle package should exist");

        let target_packages = db
            .resident_target_maps()
            .map(|(target, _)| target.package)
            .collect::<Vec<_>>();

        assert_eq!(target_packages, vec![PackageSlot(0), PackageSlot(2)]);
    }

    fn package_with_one_target(name: &str) -> Package {
        Package {
            name: name.to_string(),
            target_names: Arena::from_vec(vec![format!("{name}_lib")]),
            targets: Arena::from_vec(vec![DefMap::default()]),
        }
    }
}
