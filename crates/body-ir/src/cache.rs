//! Body IR package payload boundary for future cache artifacts.

use crate::PackageBodies;

/// One package worth of Body IR data as it will be serialized into an artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyIrPackageBundle {
    package: PackageBodies,
}

impl BodyIrPackageBundle {
    pub fn new(package: PackageBodies) -> Self {
        Self { package }
    }

    pub fn package(&self) -> &PackageBodies {
        &self.package
    }

    pub fn into_package(self) -> PackageBodies {
        self.package
    }
}
