//! Read transactions over frozen def-map package data.

use std::sync::Arc;

use rg_package_store::{PackageRead, PackageStoreReadTxn};
use rg_parse::TargetId;

use crate::{
    DefMap, ImportData, ImportId, ImportRef, LocalDefData, LocalDefId, LocalDefRef, LocalImplData,
    LocalImplRef, ModuleData, ModuleId, ModuleRef, Package, PackageSlot, Path, ResolvePathResult,
    TargetRef, path_resolution,
};

/// Read-only def-map access for one query transaction.
#[derive(Debug, Clone)]
pub struct DefMapReadTxn<'db> {
    packages: PackageStoreReadTxn<'db, Package>,
}

impl<'db> DefMapReadTxn<'db> {
    pub(crate) fn new(packages: PackageStoreReadTxn<'db, Package>) -> Self {
        Self { packages }
    }

    pub fn from_package_arcs(packages: Vec<Arc<Package>>) -> Self {
        Self {
            packages: PackageStoreReadTxn::from_arcs(packages),
        }
    }

    /// Returns all package-level def-map sets available in this transaction.
    pub fn packages(&self) -> impl ExactSizeIterator<Item = PackageRead<'_, Package>> + '_ {
        self.packages.iter()
    }

    /// Returns one package by package slot.
    pub fn package(&self, package_slot: PackageSlot) -> Option<PackageRead<'_, Package>> {
        self.packages.read(package_slot)
    }

    /// Returns one target def map by project-wide target reference.
    pub fn def_map(&self, target: TargetRef) -> Option<&DefMap> {
        self.package(target.package)?
            .into_ref()
            .target(target.target)
    }

    /// Iterates over every target def map together with its project-wide target reference.
    pub fn target_maps(&self) -> impl Iterator<Item = (TargetRef, &DefMap)> + '_ {
        self.packages()
            .enumerate()
            .flat_map(move |(package_idx, package)| {
                (0..package.targets().len()).filter_map(move |target_idx| {
                    let target_ref = TargetRef {
                        package: PackageSlot(package_idx),
                        target: TargetId(target_idx),
                    };
                    self.def_map(target_ref)
                        .map(|def_map| (target_ref, def_map))
                })
            })
    }

    /// Iterates over one target's modules together with stable project-wide references.
    pub fn modules(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (ModuleRef, &ModuleData)> + '_ {
        self.def_map(target).into_iter().flat_map(move |def_map| {
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
        self.def_map(module.target)?.module(module.module)
    }

    /// Iterates over one target's local definitions together with stable project-wide references.
    pub fn local_defs(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (LocalDefRef, &LocalDefData)> + '_ {
        self.def_map(target).into_iter().flat_map(move |def_map| {
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
        self.def_map(local_def.target)?
            .local_def(local_def.local_def)
    }

    /// Returns one impl block by stable project-wide reference.
    pub fn local_impl(&self, local_impl: LocalImplRef) -> Option<&LocalImplData> {
        self.def_map(local_impl.target)?
            .local_impl(local_impl.local_impl)
    }

    /// Iterates over one target's imports together with stable project-wide references.
    pub fn imports(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (ImportRef, &ImportData)> + '_ {
        self.def_map(target).into_iter().flat_map(move |def_map| {
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

    /// Resolves a value-position path from one module against this transaction.
    pub fn resolve_path(&self, from: ModuleRef, path: &Path) -> ResolvePathResult {
        path_resolution::resolve_path_in_txn(self, from, path)
    }

    /// Resolves a type-position path from one module against this transaction.
    pub fn resolve_path_in_type_namespace(
        &self,
        from: ModuleRef,
        path: &Path,
    ) -> ResolvePathResult {
        path_resolution::resolve_path_in_type_namespace_txn(self, from, path)
    }
}
