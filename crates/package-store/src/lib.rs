//! Package-slot-indexed storage for retained analysis package data.
//!
//! Package payloads are retained behind `Arc` while resident, and selected slots can be marked as
//! offloaded after a durable package artifact is written by the project cache layer. The store does
//! not know where offloaded data lives; callers must materialize those packages into a read
//! transaction before running queries over them.

mod txn;

use std::sync::Arc;

use rg_memsize::{MemoryRecorder, MemorySize};
use rg_workspace::PackageSlot;

pub use self::txn::{PackageRead, PackageStoreReadTxn};

/// Package slot proven to come from a resident package-store entry.
///
/// This is intentionally distinct from `PackageSlot`: resident iterators describe storage state,
/// not the full project graph. Callers that really need to promote it back to a graph slot must do
/// so explicitly at the boundary where that choice is still visible in code review.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResidentPackageSlot(PackageSlot);

impl ResidentPackageSlot {
    fn new(package: PackageSlot) -> Self {
        Self(package)
    }

    pub fn expose_package_slot(self) -> PackageSlot {
        self.0
    }
}

/// Retained storage state for one package slot.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PackageEntry<T> {
    Resident(Arc<T>),
    Offloaded,
}

/// Package storage keyed by the stable package slots of one workspace snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageStore<T> {
    packages: Vec<PackageEntry<T>>,
}

impl<T> PackageStore<T> {
    /// Freezes freshly built package payloads into the retained store.
    pub fn from_vec(packages: Vec<T>) -> Self {
        Self {
            packages: packages
                .into_iter()
                .map(|package| PackageEntry::Resident(Arc::new(package)))
                .collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.packages.len()
    }

    pub fn shrink_to_fit(&mut self) {
        self.packages.shrink_to_fit();
    }

    /// Iterates over resident package payloads, skipping offloaded slots.
    pub fn resident_packages(&self) -> impl Iterator<Item = &T> + '_ {
        self.packages.iter().filter_map(PackageEntry::as_ref)
    }

    /// Iterates over resident package payloads together with their original package slots.
    pub fn resident_packages_with_slots(
        &self,
    ) -> impl Iterator<Item = (ResidentPackageSlot, &T)> + '_ {
        self.packages
            .iter()
            .enumerate()
            .filter_map(|(package_idx, package)| {
                package
                    .as_ref()
                    .map(|package| (ResidentPackageSlot::new(PackageSlot(package_idx)), package))
            })
    }

    pub fn get(&self, package: PackageSlot) -> Option<&T> {
        self.packages.get(package.0)?.as_ref()
    }

    pub fn get_arc(&self, package: PackageSlot) -> Option<Arc<T>> {
        self.packages.get(package.0)?.resident_arc()
    }

    /// Replaces one package payload while preserving all other cloned snapshot entries.
    pub fn replace(&mut self, package: PackageSlot, value: T) -> Option<()> {
        let slot = self.packages.get_mut(package.0)?;
        *slot = PackageEntry::Resident(Arc::new(value));
        Some(())
    }

    /// Drops one resident payload after a durable package artifact has been written.
    pub fn offload(&mut self, package: PackageSlot) -> Option<()> {
        let slot = self.packages.get_mut(package.0)?;
        *slot = PackageEntry::Offloaded;
        Some(())
    }

    pub fn is_resident(&self, package: PackageSlot) -> bool {
        self.packages
            .get(package.0)
            .is_some_and(PackageEntry::is_resident)
    }

    /// Returns mutable access only when this snapshot uniquely owns the package payload.
    pub fn get_unique_mut(&mut self, package: PackageSlot) -> Option<&mut T> {
        self.packages.get_mut(package.0)?.unique_mut()
    }

    /// Returns mutable access, cloning the package payload if another snapshot still shares it.
    pub fn make_mut(&mut self, package: PackageSlot) -> Option<&mut T>
    where
        T: Clone,
    {
        self.packages.get_mut(package.0)?.make_mut()
    }

    /// Iterates over package payloads that this snapshot uniquely owns.
    pub fn resident_packages_unique_mut(&mut self) -> impl Iterator<Item = &mut T> + '_ {
        self.packages
            .iter_mut()
            .filter_map(PackageEntry::unique_mut)
    }
}

impl<T> PackageEntry<T> {
    fn as_ref(&self) -> Option<&T> {
        match self {
            Self::Resident(package) => Some(package.as_ref()),
            Self::Offloaded => None,
        }
    }

    fn resident_arc(&self) -> Option<Arc<T>> {
        match self {
            Self::Resident(package) => Some(Arc::clone(package)),
            Self::Offloaded => None,
        }
    }

    fn is_resident(&self) -> bool {
        matches!(self, Self::Resident(_))
    }

    fn unique_mut(&mut self) -> Option<&mut T> {
        match self {
            Self::Resident(package) => Arc::get_mut(package),
            Self::Offloaded => None,
        }
    }

    fn make_mut(&mut self) -> Option<&mut T>
    where
        T: Clone,
    {
        match self {
            Self::Resident(package) => Some(Arc::make_mut(package)),
            Self::Offloaded => None,
        }
    }
}

impl<T> MemorySize for PackageStore<T>
where
    T: MemorySize,
{
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        for package in &self.packages {
            if let PackageEntry::Resident(package) = package {
                package.record_memory_children(recorder);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rg_workspace::PackageSlot;

    use crate::{PackageStore, PackageStoreReadTxn};

    #[test]
    fn cloned_stores_replace_packages_independently() {
        let original = PackageStore::from_vec(vec!["workspace", "dependency"]);
        let mut changed = original.clone();

        changed
            .replace(PackageSlot(1), "rebuilt")
            .expect("package slot should exist");

        assert_eq!(original.get(PackageSlot(0)), Some(&"workspace"));
        assert_eq!(original.get(PackageSlot(1)), Some(&"dependency"));
        assert_eq!(changed.get(PackageSlot(0)), Some(&"workspace"));
        assert_eq!(changed.get(PackageSlot(1)), Some(&"rebuilt"));
    }

    #[test]
    fn read_transactions_return_package_handles() {
        let txn = PackageStoreReadTxn::from_sparse_arcs(vec![Some(Arc::new("workspace"))]);

        let package = txn.read(PackageSlot(0)).expect("package slot should exist");

        assert_eq!(*package, "workspace");
        assert_eq!(package.into_ref(), &"workspace");
    }

    #[test]
    fn sparse_read_transactions_preserve_original_package_slots() {
        let txn = PackageStoreReadTxn::from_sparse_arcs(vec![
            Some(Arc::new("workspace")),
            None,
            Some(Arc::new("dependency")),
        ]);

        let packages_with_slots = txn
            .packages_with_slots()
            .map(|(slot, package)| (slot.0, *package))
            .collect::<Vec<_>>();

        assert!(txn.read(PackageSlot(1)).is_none());
        assert_eq!(
            packages_with_slots,
            vec![(0, "workspace"), (2, "dependency")]
        );
    }

    #[test]
    fn offloaded_packages_are_not_resident_until_materialized() {
        let mut store = PackageStore::from_vec(vec!["workspace", "dependency"]);

        store
            .offload(PackageSlot(1))
            .expect("package slot should exist");

        assert_eq!(store.get(PackageSlot(0)), Some(&"workspace"));
        assert_eq!(store.get(PackageSlot(1)), None);
        assert!(!store.is_resident(PackageSlot(1)));
    }

    #[test]
    fn resident_iterators_preserve_original_package_slots() {
        let mut store = PackageStore::from_vec(vec!["workspace", "offloaded", "dependency"]);

        store
            .offload(PackageSlot(1))
            .expect("package slot should exist");

        let residents = store.resident_packages().copied().collect::<Vec<_>>();
        let residents_with_slots = store
            .resident_packages_with_slots()
            .map(|(slot, package)| (slot.expose_package_slot().0, *package))
            .collect::<Vec<_>>();

        assert_eq!(residents, vec!["workspace", "dependency"]);
        assert_eq!(
            residents_with_slots,
            vec![(0, "workspace"), (2, "dependency")]
        );
    }
}
