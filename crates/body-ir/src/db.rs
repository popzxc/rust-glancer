//! Body IR package store and transaction entry points.

use rg_def_map::{Package as DefMapPackage, PackageSlot};
use rg_memsize::{MemoryRecorder, MemorySize};
use rg_package_store::{PackageLoader, PackageStore, PackageSubset};
use rg_semantic_ir::PackageIr;
use rg_text::NameInterner;

use crate::{
    BodyIrBuildPolicy, BodyIrReadTxn, BodyIrStats, PackageBodies, TargetBodiesStatus,
    build::BodyIrDbBuilder,
};

/// Body-level IR for all analyzed packages and targets.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BodyIrDb {
    packages: PackageStore<PackageBodies>,
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
        BodyIrDbBuilder::build_with_policy_and_interner(
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
        BodyIrDbBuilder::build_with_policy_and_interner(
            parse,
            def_map,
            semantic_ir,
            policy,
            &mut interner,
        )
    }

    /// Builds Body IR using an explicit package selection policy and retained name interner.
    pub fn build_with_policy_and_interner<'db>(
        parse: &rg_parse::ParseDb,
        def_map: &'db rg_def_map::DefMapDb,
        semantic_ir: &'db rg_semantic_ir::SemanticIrDb,
        policy: BodyIrBuildPolicy,
        interner: &mut NameInterner,
    ) -> anyhow::Result<Self> {
        BodyIrDbBuilder::build_with_policy_and_interner(
            parse,
            def_map,
            semantic_ir,
            policy,
            interner,
        )
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
        BodyIrDbBuilder::rebuild_packages_with_interner_and_loaders(
            self,
            parse,
            def_map,
            semantic_ir,
            policy,
            packages,
            interner,
            def_map_loader,
            semantic_ir_loader,
            subset,
        )
    }

    pub(crate) fn from_packages(packages: Vec<PackageBodies>) -> Self {
        Self {
            packages: PackageStore::from_vec(packages),
        }
    }

    pub(crate) fn mutator(&mut self) -> BodyIrDbMutator<'_> {
        BodyIrDbMutator { db: self }
    }

    pub(crate) fn record_packages_memory_children(&self, recorder: &mut MemoryRecorder) {
        self.packages.record_memory_children(recorder);
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

    /// Returns the number of package slots tracked by this snapshot.
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
}

pub(crate) struct BodyIrDbMutator<'db> {
    db: &'db mut BodyIrDb,
}

impl BodyIrDbMutator<'_> {
    pub(crate) fn package_count(&self) -> usize {
        self.db.package_count()
    }

    pub(crate) fn replace_package(
        &mut self,
        package: PackageSlot,
        bodies: PackageBodies,
    ) -> Option<()> {
        self.db.packages.replace(package, bodies)
    }

    pub(crate) fn package_mut(&mut self, package: PackageSlot) -> Option<&mut PackageBodies> {
        self.db.packages.make_mut(package)
    }

    pub(crate) fn shrink_to_fit(&mut self) {
        self.db.packages.shrink_to_fit();
        for entry in self.db.packages.raw_entries_mut() {
            if let Some(package) = entry.as_resident_unique_mut() {
                package.shrink_to_fit();
            }
        }
    }

    pub(crate) fn shrink_packages(&mut self, packages: &[PackageSlot]) {
        for package in packages {
            if let Some(package) = self.db.packages.get_unique_mut(*package) {
                package.shrink_to_fit();
            }
        }
    }
}
