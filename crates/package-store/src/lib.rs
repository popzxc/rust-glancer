//! Package-slot-indexed storage for retained analysis package data.
//!
//! The first implementation is intentionally resident-only: every package payload stays in memory
//! behind an `Arc`. The important boundary is that phase databases no longer expose their storage
//! container directly, so later cache work can replace selected payloads with disk-backed entries
//! without rewriting every query API.

mod txn;

use std::sync::Arc;

use rg_memsize::{MemoryRecorder, MemorySize};
use rg_workspace::PackageSlot;

pub use self::txn::{PackageRead, PackageStoreReadTxn};

/// Resident package storage keyed by the stable package slots of one workspace snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageStore<T> {
    packages: Vec<Arc<T>>,
}

impl<T> PackageStore<T> {
    /// Freezes freshly built package payloads into the retained store.
    pub fn from_vec(packages: Vec<T>) -> Self {
        Self {
            packages: packages.into_iter().map(Arc::new).collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.packages.len()
    }

    pub fn shrink_to_fit(&mut self) {
        self.packages.shrink_to_fit();
    }

    /// Starts a read transaction over this store.
    pub fn read_txn(&self) -> PackageStoreReadTxn<'_, T> {
        PackageStoreReadTxn::new(self)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &T> + '_ {
        self.packages.iter().map(Arc::as_ref)
    }

    pub fn get(&self, package: PackageSlot) -> Option<&T> {
        self.packages.get(package.0).map(Arc::as_ref)
    }

    /// Replaces one package payload while preserving all other cloned snapshot entries.
    pub fn replace(&mut self, package: PackageSlot, value: T) -> Option<()> {
        let slot = self.packages.get_mut(package.0)?;
        *slot = Arc::new(value);
        Some(())
    }

    /// Returns mutable access only when this snapshot uniquely owns the package payload.
    pub fn get_unique_mut(&mut self, package: PackageSlot) -> Option<&mut T> {
        self.packages.get_mut(package.0).and_then(Arc::get_mut)
    }

    /// Returns mutable access, cloning the package payload if another snapshot still shares it.
    pub fn make_mut(&mut self, package: PackageSlot) -> Option<&mut T>
    where
        T: Clone,
    {
        self.packages.get_mut(package.0).map(Arc::make_mut)
    }

    /// Iterates over package payloads that this snapshot uniquely owns.
    pub fn iter_unique_mut(&mut self) -> impl Iterator<Item = &mut T> + '_ {
        self.packages.iter_mut().filter_map(Arc::get_mut)
    }
}

impl<T> MemorySize for PackageStore<T>
where
    T: MemorySize,
{
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        self.packages.record_memory_children(recorder);
    }
}

#[cfg(test)]
mod tests {
    use rg_workspace::PackageSlot;

    use crate::PackageStore;

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
        let store = PackageStore::from_vec(vec!["workspace"]);
        let txn = store.read_txn();

        let package = txn.read(PackageSlot(0)).expect("package slot should exist");

        assert_eq!(*package, "workspace");
        assert_eq!(package.into_ref(), &"workspace");
    }
}
