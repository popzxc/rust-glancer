//! DefMap package payload boundary for future cache artifacts.

use crate::Package;

/// One package worth of DefMap data as it will be serialized into an artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefMapPackageBundle {
    package: Package,
}

impl DefMapPackageBundle {
    pub fn new(package: Package) -> Self {
        Self { package }
    }

    pub fn package(&self) -> &Package {
        &self.package
    }

    pub fn into_package(self) -> Package {
        self.package
    }
}
