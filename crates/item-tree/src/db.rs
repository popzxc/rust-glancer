//! Resident item-tree database and package-selection builders.

use anyhow::Context as _;

use rg_parse::ParseDb;
use rg_text::NameInterner;

use crate::{Package, lower};

/// Lowered item trees for all parsed packages.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemTreeDb {
    pub(crate) packages: Vec<Option<Package>>,
}

impl ItemTreeDb {
    /// Builds file-local item trees on top of the parsed source database.
    pub fn build(parse: &mut ParseDb) -> anyhow::Result<Self> {
        let mut interner = NameInterner::new();
        Self::build_with_interner(parse, &mut interner)
    }

    /// Builds file-local item trees using a caller-retained name interner.
    pub fn build_with_interner(
        parse: &mut ParseDb,
        interner: &mut NameInterner,
    ) -> anyhow::Result<Self> {
        let package_count = parse.package_count();
        let packages = (0..package_count).collect::<Vec<_>>();
        Self::build_packages_with_interner(parse, &packages, interner)
    }

    /// Builds item trees only for selected packages.
    ///
    /// Project rebuilds use this as a temporary lowering input: affected packages are populated,
    /// while unrelated packages stay absent so accidental cross-package item-tree access fails
    /// loudly instead of retaining the whole item-tree graph.
    pub fn build_packages(parse: &mut ParseDb, packages: &[usize]) -> anyhow::Result<Self> {
        let mut interner = NameInterner::new();
        Self::build_packages_with_interner(parse, packages, &mut interner)
    }

    /// Builds selected packages using a caller-retained name interner.
    pub fn build_packages_with_interner(
        parse: &mut ParseDb,
        packages: &[usize],
        interner: &mut NameInterner,
    ) -> anyhow::Result<Self> {
        let mut trees = Self {
            packages: vec![None; parse.package_count()],
        };
        for package_slot in normalized_package_slots(packages) {
            let package = parse.package_mut(package_slot).with_context(|| {
                format!("while attempting to fetch parsed package {package_slot}")
            })?;
            let lowered = lower::build_package(package, interner).with_context(|| {
                format!(
                    "while attempting to build item trees for package {}",
                    package.package_name()
                )
            })?;
            trees.packages[package_slot] = Some(lowered);
        }

        Ok(trees)
    }

    /// Returns one package tree set by slot.
    pub fn package(&self, package_slot: usize) -> Option<&Package> {
        self.packages.get(package_slot)?.as_ref()
    }
}

fn normalized_package_slots(packages: &[usize]) -> Vec<usize> {
    let mut packages = packages.to_vec();
    packages.sort_unstable();
    packages.dedup();
    packages
}
