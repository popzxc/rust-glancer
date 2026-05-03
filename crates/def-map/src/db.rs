//! Resident def-map database and package-level rebuild entrypoints.

use std::sync::Arc;

use rg_item_tree::ItemTreeDb;
use rg_package_store::PackageStore;
use rg_parse::{self, TargetId};
use rg_text::NameInterner;
use rg_workspace::WorkspaceMetadata;

use crate::{
    DefMap, DefMapReadTxn, ImportData, ImportId, ImportRef, LocalDefData, LocalDefId, LocalDefRef,
    LocalImplData, LocalImplId, LocalImplRef, ModuleData, ModuleId, ModuleRef, Package,
    PackageSlot, Path, ResolvePathResult, TargetRef, path_resolution, resolve,
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

    /// Returns a new def-map snapshot with selected packages rebuilt.
    pub fn rebuild_packages(
        &self,
        workspace: &WorkspaceMetadata,
        parse: &rg_parse::ParseDb,
        item_tree: &ItemTreeDb,
        packages: &[PackageSlot],
    ) -> anyhow::Result<Self> {
        let mut interner = NameInterner::new();
        self.rebuild_packages_with_interner(workspace, parse, item_tree, packages, &mut interner)
    }

    /// Returns a new def-map snapshot with selected packages rebuilt using retained names.
    pub fn rebuild_packages_with_interner(
        &self,
        workspace: &WorkspaceMetadata,
        parse: &rg_parse::ParseDb,
        item_tree: &ItemTreeDb,
        packages: &[PackageSlot],
        interner: &mut NameInterner,
    ) -> anyhow::Result<Self> {
        let mut db =
            resolve::rebuild_packages(self, workspace, parse, item_tree, packages, interner)?;
        db.shrink_packages(packages);
        Ok(db)
    }

    /// Starts a read transaction over package-level def-map data.
    pub fn read_txn(&self) -> DefMapReadTxn<'_> {
        DefMapReadTxn::new(self.packages.read_txn())
    }

    pub fn read_txn_from_package_arcs<'a>(packages: Vec<Arc<Package>>) -> DefMapReadTxn<'a> {
        DefMapReadTxn::from_package_arcs(packages)
    }

    /// Returns all package-level def-map sets.
    pub fn packages(&self) -> impl Iterator<Item = &Package> + '_ {
        self.packages.iter()
    }

    pub fn package_count(&self) -> usize {
        self.packages.len()
    }

    /// Iterates over every target def map together with its project-wide target reference.
    pub fn target_maps(&self) -> impl Iterator<Item = (TargetRef, &DefMap)> {
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

    /// Returns coarse DefMap totals for the current project report.
    pub fn stats(&self) -> DefMapStats {
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
    pub fn package(&self, package_slot: PackageSlot) -> Option<&Package> {
        self.packages.get(package_slot)
    }

    pub fn package_arc(&self, package_slot: PackageSlot) -> Option<Arc<Package>> {
        self.packages.get_arc(package_slot)
    }

    pub fn replace_package(&mut self, package_slot: PackageSlot, package: Package) -> Option<()> {
        self.packages.replace(package_slot, package)
    }

    pub fn offload_package(&mut self, package_slot: PackageSlot) -> Option<()> {
        self.packages.offload(package_slot)
    }

    /// Returns one target def map by project-wide target reference.
    pub fn def_map(&self, target: TargetRef) -> Option<&DefMap> {
        self.package(target.package)?.target(target.target)
    }

    /// Iterates over one target's modules together with stable project-wide references.
    #[allow(dead_code)]
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

    /// Iterates over one target's impl blocks together with stable project-wide references.
    pub fn local_impls(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (LocalImplRef, &LocalImplData)> + '_ {
        self.def_map(target).into_iter().flat_map(move |def_map| {
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

    /// Returns one import by stable project-wide reference.
    #[allow(dead_code)]
    pub fn import(&self, import: ImportRef) -> Option<&ImportData> {
        self.def_map(import.target)?.import(import.import)
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
        for package in self.packages.iter_unique_mut() {
            package.shrink_to_fit();
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
