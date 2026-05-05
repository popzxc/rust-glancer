//! Resident semantic IR database and item-query helpers.

use anyhow::Context as _;

use rg_def_map::{Package as DefMapPackage, PackageSlot};
use rg_package_store::{PackageLoader, PackageStore, PackageSubset};

use crate::{ImplData, ImplRef, PackageIr, SemanticIrReadTxn, SemanticIrStats, lower, resolution};

/// Semantic item graph for all analyzed packages and targets.
///
/// Semantic IR is the signature layer: it keeps named items, fields, impl headers, function
/// signatures, and enough resolution metadata to answer LSP-shaped questions without parsing AST
/// again. Bodies live in `rg_body_ir`; this layer intentionally stops at item/signature facts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SemanticIrDb {
    pub(crate) packages: PackageStore<PackageIr>,
}

impl SemanticIrDb {
    /// Builds semantic IR from already-collected item trees and frozen name-resolution maps.
    pub fn build(
        item_tree: &rg_item_tree::ItemTreeDb,
        def_map: &rg_def_map::DefMapDb,
    ) -> anyhow::Result<Self> {
        let mut db = lower::build_db(item_tree, def_map)?;
        resolution::resolve_impl_headers(&mut db, def_map)
            .context("while attempting to resolve semantic IR impl headers")?;
        db.shrink_to_fit();
        Ok(db)
    }

    /// Returns a new semantic-IR snapshot with selected packages rebuilt against lazy read views.
    pub fn rebuild_packages_with_loaders<'db>(
        &'db self,
        item_tree: &rg_item_tree::ItemTreeDb,
        def_map: &'db rg_def_map::DefMapDb,
        packages: &[PackageSlot],
        def_map_loader: PackageLoader<'db, DefMapPackage>,
        semantic_ir_loader: PackageLoader<'db, PackageIr>,
        subset: &PackageSubset,
    ) -> anyhow::Result<Self> {
        let mut next = self.clone();
        let packages = normalized_package_slots(packages);

        for package in &packages {
            let rebuilt = lower::build_package(item_tree, def_map, *package)?;
            next.packages.replace(*package, rebuilt).with_context(|| {
                format!(
                    "while attempting to replace semantic IR package {}",
                    package.0
                )
            })?;
        }

        let def_map_txn = def_map.read_txn_for_subset(def_map_loader, subset);
        let semantic_ir_txn = next.read_txn_for_subset(semantic_ir_loader, subset);
        let impl_resolutions = resolution::impl_header_resolutions_for_packages(
            &semantic_ir_txn,
            &def_map_txn,
            &packages,
        )
        .context("while attempting to resolve rebuilt semantic IR impl headers")?;
        drop(semantic_ir_txn);
        resolution::apply_impl_header_resolutions(&mut next, impl_resolutions);

        next.shrink_packages(&packages);
        Ok(next)
    }

    pub(crate) fn new(packages: Vec<PackageIr>) -> Self {
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

    /// Returns coarse item counts for status output and smoke checks.
    pub fn stats(&self) -> SemanticIrStats {
        let mut stats = SemanticIrStats::default();

        for entry in self.packages.raw_entries() {
            let Some(package) = entry.as_resident() else {
                continue;
            };
            for target in package.targets() {
                let items = target.items();
                stats.target_count += 1;
                stats.struct_count += items.structs.len();
                stats.union_count += items.unions.len();
                stats.enum_count += items.enums.len();
                stats.trait_count += items.traits.len();
                stats.impl_count += items.impls.len();
                stats.function_count += items.functions.len();
                stats.type_alias_count += items.type_aliases.len();
                stats.const_count += items.consts.len();
                stats.static_count += items.statics.len();
            }
        }

        stats
    }

    /// Returns resident package-level semantic IR sets, skipping offloaded packages.
    pub fn package_count(&self) -> usize {
        self.packages.len()
    }

    /// Returns one resident package by package slot.
    pub fn resident_package(&self, package: PackageSlot) -> Option<&PackageIr> {
        self.packages
            .raw_entry(package)
            .and_then(|entry| entry.as_resident())
    }

    pub fn read_txn<'db>(
        &'db self,
        loader: PackageLoader<'db, PackageIr>,
    ) -> SemanticIrReadTxn<'db> {
        SemanticIrReadTxn::from_package_store(self.packages.read_txn(loader))
    }

    pub fn read_txn_for_subset<'db>(
        &'db self,
        loader: PackageLoader<'db, PackageIr>,
        subset: &PackageSubset,
    ) -> SemanticIrReadTxn<'db> {
        SemanticIrReadTxn::from_package_store(self.packages.read_txn_for_subset(loader, subset))
    }

    pub fn offload_package(&mut self, package: PackageSlot) -> Option<()> {
        self.packages.offload(package)
    }

    pub(crate) fn impl_data_mut(&mut self, impl_ref: ImplRef) -> Option<&mut ImplData> {
        self.package_mut(impl_ref.target.package)?
            .target_mut(impl_ref.target.target)?
            .items_mut()
            .impls
            .get_mut(impl_ref.id)
    }

    fn package_mut(&mut self, package: PackageSlot) -> Option<&mut PackageIr> {
        self.packages.make_mut(package)
    }
}

fn normalized_package_slots(packages: &[PackageSlot]) -> Vec<PackageSlot> {
    let mut slots = packages.to_vec();
    slots.sort_by_key(|slot| slot.0);
    slots.dedup();
    slots
}
