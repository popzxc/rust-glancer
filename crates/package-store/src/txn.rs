//! Read transaction handles for package-store payloads.

use std::ops::Deref;
use std::sync::Arc;

use rg_workspace::PackageSlot;

/// Read-only package-store view used by query transactions.
#[derive(Debug)]
pub struct PackageStoreReadTxn<'db, T> {
    packages: Vec<Option<Arc<T>>>,
    _marker: std::marker::PhantomData<&'db T>,
}

impl<'db, T> PackageStoreReadTxn<'db, T> {
    pub fn from_sparse_arcs(packages: Vec<Option<Arc<T>>>) -> Self {
        Self {
            packages,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn read(&self, package: PackageSlot) -> Option<PackageRead<'_, T>> {
        self.packages
            .get(package.0)
            .and_then(Option::as_ref)
            .map(|package| PackageRead::Resident(package.as_ref()))
    }

    /// Iterates over every materialized package together with its original package slot.
    pub fn packages_with_slots(
        &self,
    ) -> impl Iterator<Item = (PackageSlot, PackageRead<'_, T>)> + '_ {
        self.packages
            .iter()
            .enumerate()
            .filter_map(|(package_idx, package)| {
                package.as_ref().map(|package| {
                    (
                        PackageSlot(package_idx),
                        PackageRead::Resident(package.as_ref()),
                    )
                })
            })
    }
}

impl<T> Clone for PackageStoreReadTxn<'_, T> {
    fn clone(&self) -> Self {
        Self::from_sparse_arcs(self.packages.clone())
    }
}

/// One package payload read through a package-store transaction.
#[derive(Debug)]
pub enum PackageRead<'db, T> {
    Resident(&'db T),
}

impl<'db, T> PackageRead<'db, T> {
    pub fn into_ref(self) -> &'db T {
        match self {
            Self::Resident(package) => package,
        }
    }
}

impl<T> Clone for PackageRead<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for PackageRead<'_, T> {}

impl<T> Deref for PackageRead<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Resident(package) => package,
        }
    }
}
