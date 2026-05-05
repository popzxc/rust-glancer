//! Resident Body IR database and body-query helpers.

use std::{fmt, marker::PhantomData, sync::Arc};

use anyhow::Context as _;

use rg_def_map::{Package as DefMapPackage, PackageSlot};
use rg_package_store::{
    LoadPackage, PackageLoader, PackageStore, PackageStoreError, PackageSubset,
};
use rg_semantic_ir::PackageIr;
use rg_text::NameInterner;

use crate::{
    BodyIrBuildPolicy, BodyIrReadTxn, BodyIrStats, PackageBodies, TargetBodiesStatus, lower,
    resolution,
};

/// Body-level IR for all analyzed packages and targets.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BodyIrDb {
    pub(crate) packages: PackageStore<PackageBodies>,
}

impl BodyIrDb {
    /// Builds Body IR using the default editor-oriented policy.
    ///
    /// By default we lower bodies only for workspace packages. Dependency signatures remain
    /// available through Semantic IR, but dependency body internals are skipped to keep the eager
    /// analysis cheaper.
    pub fn build<'db>(
        parse: &rg_parse::ParseDb,
        def_map: &'db rg_def_map::DefMapDb,
        semantic_ir: &'db rg_semantic_ir::SemanticIrDb,
    ) -> anyhow::Result<Self> {
        let mut interner = NameInterner::new();
        Self::build_with_policy_and_interner(
            parse,
            def_map,
            semantic_ir,
            BodyIrBuildPolicy::default(),
            &mut interner,
        )
    }

    /// Builds Body IR using an explicit package selection policy.
    pub fn build_with_policy<'db>(
        parse: &rg_parse::ParseDb,
        def_map: &'db rg_def_map::DefMapDb,
        semantic_ir: &'db rg_semantic_ir::SemanticIrDb,
        policy: BodyIrBuildPolicy,
    ) -> anyhow::Result<Self> {
        let mut interner = NameInterner::new();
        Self::build_with_policy_and_interner(parse, def_map, semantic_ir, policy, &mut interner)
    }

    /// Builds Body IR using an explicit package selection policy and retained name interner.
    pub fn build_with_policy_and_interner<'db>(
        parse: &rg_parse::ParseDb,
        def_map: &'db rg_def_map::DefMapDb,
        semantic_ir: &'db rg_semantic_ir::SemanticIrDb,
        policy: BodyIrBuildPolicy,
        interner: &mut NameInterner,
    ) -> anyhow::Result<Self> {
        let def_map_txn = def_map.read_txn(unexpected_package_loader());
        let semantic_ir_txn = semantic_ir.read_txn(unexpected_package_loader());
        let mut db = lower::build_db(
            parse,
            &semantic_ir_txn,
            semantic_ir.package_count(),
            policy,
            interner,
        )?;
        resolution::resolve_bodies(&mut db, &def_map_txn, &semantic_ir_txn);
        db.shrink_to_fit();
        Ok(db)
    }

    /// Returns a new Body IR snapshot with selected packages rebuilt against lazy read views.
    pub fn rebuild_packages_with_interner_and_loaders<'db>(
        &'db self,
        parse: &rg_parse::ParseDb,
        def_map: &'db rg_def_map::DefMapDb,
        semantic_ir: &'db rg_semantic_ir::SemanticIrDb,
        policy: BodyIrBuildPolicy,
        packages: &[PackageSlot],
        interner: &mut NameInterner,
        def_map_loader: PackageLoader<'db, DefMapPackage>,
        semantic_ir_loader: PackageLoader<'db, PackageIr>,
        subset: &PackageSubset,
    ) -> anyhow::Result<Self> {
        let mut next = self.clone();
        let packages = normalized_package_slots(packages);
        let semantic_ir_txn = semantic_ir.read_txn_for_subset(semantic_ir_loader, subset);

        for package in &packages {
            let package_ir = semantic_ir_txn.package(*package).with_context(|| {
                format!(
                    "while attempting to fetch semantic IR package {}",
                    package.0
                )
            })?;
            let target_count = package_ir.into_ref().targets().len();
            let rebuilt = lower::build_package(
                parse,
                &semantic_ir_txn,
                policy,
                *package,
                target_count,
                interner,
            )
            .with_context(|| {
                format!(
                    "while attempting to lower rebuilt body IR package {}",
                    package.0
                )
            })?;
            next.packages.replace(*package, rebuilt).with_context(|| {
                format!("while attempting to replace body IR package {}", package.0)
            })?;
        }

        let def_map_txn = def_map.read_txn_for_subset(def_map_loader, subset);
        resolution::resolve_bodies_for_packages(
            &mut next,
            &def_map_txn,
            &semantic_ir_txn,
            &packages,
        )
        .context("while attempting to resolve rebuilt body IR packages")?;
        next.shrink_packages(&packages);
        Ok(next)
    }

    pub(crate) fn new(packages: Vec<PackageBodies>) -> Self {
        Self {
            packages: PackageStore::from_vec(packages),
        }
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

    pub fn stats(&self) -> BodyIrStats {
        let mut stats = BodyIrStats::default();

        for entry in self.packages.raw_entries() {
            let Some(package) = entry.as_resident() else {
                continue;
            };
            for target in package.targets() {
                stats.target_count += 1;
                match target.status() {
                    TargetBodiesStatus::Built => stats.built_target_count += 1,
                    TargetBodiesStatus::Skipped => stats.skipped_target_count += 1,
                }
                stats.body_count += target.bodies().len();
                for body in target.bodies() {
                    stats.scope_count += body.scopes.len();
                    stats.local_item_count += body.local_items.len();
                    stats.local_impl_count += body.local_impls.len();
                    stats.local_function_count += body.local_functions.len();
                    stats.binding_count += body.bindings.len();
                    stats.statement_count += body.statements.len();
                    stats.expression_count += body.exprs.len();
                }
            }
        }

        stats
    }

    /// Returns resident package-level body IR sets, skipping offloaded packages.
    pub fn package_count(&self) -> usize {
        self.packages.len()
    }

    /// Returns one resident package by package slot.
    pub fn resident_package(&self, package: PackageSlot) -> Option<&PackageBodies> {
        self.packages
            .raw_entry(package)
            .and_then(|entry| entry.as_resident())
    }

    pub fn read_txn<'db>(
        &'db self,
        loader: PackageLoader<'db, PackageBodies>,
    ) -> BodyIrReadTxn<'db> {
        BodyIrReadTxn::from_package_store(self.packages.read_txn(loader))
    }

    pub fn read_txn_for_subset<'db>(
        &'db self,
        loader: PackageLoader<'db, PackageBodies>,
        subset: &PackageSubset,
    ) -> BodyIrReadTxn<'db> {
        BodyIrReadTxn::from_package_store(self.packages.read_txn_for_subset(loader, subset))
    }

    pub fn offload_package(&mut self, package: PackageSlot) -> Option<()> {
        self.packages.offload(package)
    }

    pub(crate) fn package_mut(&mut self, package: PackageSlot) -> Option<&mut PackageBodies> {
        self.packages.make_mut(package)
    }
}

fn unexpected_package_loader<T: 'static>() -> PackageLoader<'static, T> {
    PackageLoader::new(UnexpectedPackageLoader(PhantomData))
}

struct UnexpectedPackageLoader<T>(PhantomData<fn() -> T>);

impl<T> fmt::Debug for UnexpectedPackageLoader<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnexpectedPackageLoader").finish()
    }
}

impl<T> LoadPackage<T> for UnexpectedPackageLoader<T> {
    fn load(&self, package: PackageSlot) -> Result<Arc<T>, PackageStoreError> {
        panic!(
            "resident body IR query should not load offloaded package {}",
            package.0,
        )
    }
}

fn normalized_package_slots(packages: &[PackageSlot]) -> Vec<PackageSlot> {
    let mut slots = packages.to_vec();
    slots.sort_by_key(|slot| slot.0);
    slots.dedup();
    slots
}
