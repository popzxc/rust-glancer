//! Semantic IR package payload boundary for future cache artifacts.

use crate::PackageIr;

/// One package worth of Semantic IR data as it will be serialized into an artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticIrPackageBundle {
    package: PackageIr,
}

impl SemanticIrPackageBundle {
    pub fn new(package: PackageIr) -> Self {
        Self { package }
    }

    pub fn package(&self) -> &PackageIr {
        &self.package
    }

    pub fn into_package(self) -> PackageIr {
        self.package
    }
}
