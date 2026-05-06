//! Builds and updates Body IR snapshots.

mod lower;
mod resolve;

use std::{fmt, marker::PhantomData, sync::Arc};

use anyhow::Context as _;

use rg_def_map::{Package as DefMapPackage, PackageSlot};
use rg_package_store::{LoadPackage, PackageLoader, PackageStoreError, PackageSubset};
use rg_semantic_ir::PackageIr;
use rg_text::NameInterner;

use crate::{BodyIrBuildPolicy, BodyIrDb};

pub(crate) struct BodyIrDbBuilder;

impl BodyIrDbBuilder {
    pub(crate) fn build_with_policy_and_interner<'db>(
        parse: &rg_parse::ParseDb,
        def_map: &'db rg_def_map::DefMapDb,
        semantic_ir: &'db rg_semantic_ir::SemanticIrDb,
        policy: BodyIrBuildPolicy,
        interner: &mut NameInterner,
    ) -> anyhow::Result<BodyIrDb> {
        let def_map_txn = def_map.read_txn(unexpected_package_loader());
        let semantic_ir_txn = semantic_ir.read_txn(unexpected_package_loader());
        let packages = lower::build_packages(
            parse,
            &semantic_ir_txn,
            semantic_ir.package_count(),
            policy,
            interner,
        )?;
        let mut db = BodyIrDb::from_packages(packages);
        {
            let mut mutator = db.mutator();
            resolve::resolve_bodies(&mut mutator, &def_map_txn, &semantic_ir_txn);
            mutator.shrink_to_fit();
        }
        Ok(db)
    }

    pub(crate) fn rebuild_packages_with_interner_and_loaders<'db>(
        old: &'db BodyIrDb,
        parse: &rg_parse::ParseDb,
        def_map: &'db rg_def_map::DefMapDb,
        semantic_ir: &'db rg_semantic_ir::SemanticIrDb,
        policy: BodyIrBuildPolicy,
        packages: &[PackageSlot],
        interner: &mut NameInterner,
        def_map_loader: PackageLoader<'db, DefMapPackage>,
        semantic_ir_loader: PackageLoader<'db, PackageIr>,
        subset: &PackageSubset,
    ) -> anyhow::Result<BodyIrDb> {
        let mut next = old.clone();
        let packages = normalized_package_slots(packages);
        let semantic_ir_txn = semantic_ir.read_txn_for_subset(semantic_ir_loader, subset);

        {
            let mut mutator = next.mutator();
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
                mutator
                    .replace_package(*package, rebuilt)
                    .with_context(|| {
                        format!("while attempting to replace body IR package {}", package.0)
                    })?;
            }
        }

        let def_map_txn = def_map.read_txn_for_subset(def_map_loader, subset);
        {
            let mut mutator = next.mutator();
            resolve::resolve_bodies_for_packages(
                &mut mutator,
                &def_map_txn,
                &semantic_ir_txn,
                &packages,
            )
            .context("while attempting to resolve rebuilt body IR packages")?;
            mutator.shrink_packages(&packages);
        }
        Ok(next)
    }
}

fn normalized_package_slots(packages: &[PackageSlot]) -> Vec<PackageSlot> {
    let mut slots = packages.to_vec();
    slots.sort_by_key(|slot| slot.0);
    slots.dedup();
    slots
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
            "resident body IR build should not load offloaded package {}",
            package.0,
        )
    }
}
