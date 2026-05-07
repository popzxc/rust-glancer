//! Builds and rebuilds Body IR snapshots.

mod lower;
mod resolve;

use std::{fmt, marker::PhantomData, sync::Arc};

use anyhow::Context as _;

use rg_def_map::{Package as DefMapPackage, PackageSlot};
use rg_package_store::{LoadPackage, PackageLoader, PackageStoreError, PackageSubset};
use rg_semantic_ir::PackageIr;
use rg_text::PackageNameInterners;

use crate::{BodyIrBuildPolicy, BodyIrDb};

/// Builder for a fresh Body IR snapshot.
pub struct BodyIrDbBuilder<'db, 'names> {
    parse: &'db rg_parse::ParseDb,
    def_map: &'db rg_def_map::DefMapDb,
    semantic_ir: &'db rg_semantic_ir::SemanticIrDb,
    policy: BodyIrBuildPolicy,
    interners: NameInternerSource<'names>,
}

impl<'db> BodyIrDbBuilder<'db, 'static> {
    pub(crate) fn new(
        parse: &'db rg_parse::ParseDb,
        def_map: &'db rg_def_map::DefMapDb,
        semantic_ir: &'db rg_semantic_ir::SemanticIrDb,
    ) -> Self {
        Self {
            parse,
            def_map,
            semantic_ir,
            policy: BodyIrBuildPolicy::default(),
            interners: NameInternerSource::Owned(PackageNameInterners::new(parse.package_count())),
        }
    }
}

impl<'db, 'names> BodyIrDbBuilder<'db, 'names> {
    pub fn name_interners(
        self,
        interners: &'names mut PackageNameInterners,
    ) -> BodyIrDbBuilder<'db, 'names> {
        BodyIrDbBuilder {
            parse: self.parse,
            def_map: self.def_map,
            semantic_ir: self.semantic_ir,
            policy: self.policy,
            interners: NameInternerSource::Borrowed(interners),
        }
    }

    pub fn policy(mut self, policy: BodyIrBuildPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn build(mut self) -> anyhow::Result<BodyIrDb> {
        let def_map_txn = self.def_map.read_txn(unexpected_package_loader());
        let semantic_ir_txn = self.semantic_ir.read_txn(unexpected_package_loader());
        let packages = lower::build_packages(
            self.parse,
            &semantic_ir_txn,
            self.semantic_ir.package_count(),
            self.policy,
            self.interners.as_mut(),
        )?;
        let mut db = BodyIrDb::from_packages(packages);
        {
            let mut mutator = db.mutator();
            resolve::resolve_bodies(&mut mutator, &def_map_txn, &semantic_ir_txn);
            mutator.shrink_to_fit();
        }
        Ok(db)
    }
}

enum NameInternerSource<'names> {
    Owned(PackageNameInterners),
    Borrowed(&'names mut PackageNameInterners),
}

impl NameInternerSource<'_> {
    fn as_mut(&mut self) -> &mut PackageNameInterners {
        match self {
            Self::Owned(interners) => interners,
            Self::Borrowed(interners) => interners,
        }
    }
}

/// Builder for a Body IR snapshot that replaces selected packages.
pub struct BodyIrDbPackageRebuilder<'db, 'names> {
    old: &'db BodyIrDb,
    parse: &'db rg_parse::ParseDb,
    def_map: &'db rg_def_map::DefMapDb,
    semantic_ir: &'db rg_semantic_ir::SemanticIrDb,
    policy: BodyIrBuildPolicy,
    packages: &'db [PackageSlot],
    interners: &'names mut PackageNameInterners,
    def_map_loader: PackageLoader<'db, DefMapPackage>,
    semantic_ir_loader: PackageLoader<'db, PackageIr>,
    subset: &'db PackageSubset,
}

impl<'db, 'names> BodyIrDbPackageRebuilder<'db, 'names> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        old: &'db BodyIrDb,
        parse: &'db rg_parse::ParseDb,
        def_map: &'db rg_def_map::DefMapDb,
        semantic_ir: &'db rg_semantic_ir::SemanticIrDb,
        packages: &'db [PackageSlot],
        interners: &'names mut PackageNameInterners,
        def_map_loader: PackageLoader<'db, DefMapPackage>,
        semantic_ir_loader: PackageLoader<'db, PackageIr>,
        subset: &'db PackageSubset,
    ) -> Self {
        Self {
            old,
            parse,
            def_map,
            semantic_ir,
            policy: BodyIrBuildPolicy::default(),
            packages,
            interners,
            def_map_loader,
            semantic_ir_loader,
            subset,
        }
    }

    pub fn policy(mut self, policy: BodyIrBuildPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn build(self) -> anyhow::Result<BodyIrDb> {
        let mut next = self.old.clone();
        let packages = normalized_package_slots(self.packages);
        let semantic_ir_txn = self
            .semantic_ir
            .read_txn_for_subset(self.semantic_ir_loader, self.subset);

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
                    self.parse,
                    &semantic_ir_txn,
                    self.policy,
                    *package,
                    target_count,
                    self.interners,
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

        let def_map_txn = self
            .def_map
            .read_txn_for_subset(self.def_map_loader, self.subset);
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
