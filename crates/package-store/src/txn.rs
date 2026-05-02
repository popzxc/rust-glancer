//! Read transaction handles for package-store payloads.

use std::ops::Deref;

use rg_workspace::PackageSlot;

use crate::PackageStore;

/// Read-only package-store view used by query transactions.
#[derive(Debug)]
pub struct PackageStoreReadTxn<'db, T> {
    store: &'db PackageStore<T>,
}

impl<'db, T> PackageStoreReadTxn<'db, T> {
    pub(crate) fn new(store: &'db PackageStore<T>) -> Self {
        Self { store }
    }

    pub fn read(&self, package: PackageSlot) -> Option<PackageRead<'db, T>> {
        self.store
            .packages
            .get(package.0)
            .map(|package| PackageRead::Resident(package.as_ref()))
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = PackageRead<'db, T>> + '_ {
        self.store
            .packages
            .iter()
            .map(|package| PackageRead::Resident(package.as_ref()))
    }
}

impl<T> Clone for PackageStoreReadTxn<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for PackageStoreReadTxn<'_, T> {}

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
