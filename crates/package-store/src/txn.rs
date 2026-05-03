//! Read transaction handles for package-store payloads.

use std::ops::Deref;
use std::sync::Arc;

use rg_workspace::PackageSlot;

use crate::{PackageEntry, PackageStore};

/// Read-only package-store view used by query transactions.
#[derive(Debug)]
pub struct PackageStoreReadTxn<'db, T> {
    packages: Vec<Arc<T>>,
    _marker: std::marker::PhantomData<&'db T>,
}

impl<'db, T> PackageStoreReadTxn<'db, T> {
    pub(crate) fn from_resident_store(store: &'db PackageStore<T>) -> Self {
        let packages = store
            .packages
            .iter()
            .map(|package| match package {
                PackageEntry::Resident(package) => Arc::clone(package),
                PackageEntry::Offloaded => {
                    panic!("offloaded packages must be materialized before starting a read txn")
                }
            })
            .collect();
        Self::from_arcs(packages)
    }

    pub fn from_arcs(packages: Vec<Arc<T>>) -> Self {
        Self {
            packages,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn read(&self, package: PackageSlot) -> Option<PackageRead<'_, T>> {
        self.packages
            .get(package.0)
            .map(|package| PackageRead::Resident(package.as_ref()))
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = PackageRead<'_, T>> + '_ {
        self.packages
            .iter()
            .map(|package| PackageRead::Resident(package.as_ref()))
    }
}

impl<T> Clone for PackageStoreReadTxn<'_, T> {
    fn clone(&self) -> Self {
        Self::from_arcs(self.packages.clone())
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
