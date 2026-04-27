use rg_parse::TargetId;

use crate::TargetIr;

/// Semantic IR for one Cargo package.
///
/// Packages keep target IRs in the same stable order as parse/def-map packages, so a
/// `TargetRef { package, target }` can address every phase without an extra translation table.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageIr {
    targets: Vec<TargetIr>,
}

impl PackageIr {
    pub(crate) fn new(targets: Vec<TargetIr>) -> Self {
        Self { targets }
    }

    /// Returns all target IRs for this package in target-id order.
    pub fn targets(&self) -> &[TargetIr] {
        &self.targets
    }

    /// Returns one target IR by package-local target id.
    pub fn target(&self, target: TargetId) -> Option<&TargetIr> {
        self.targets.get(target.0)
    }

    pub(crate) fn target_mut(&mut self, target: TargetId) -> Option<&mut TargetIr> {
        self.targets.get_mut(target.0)
    }
}
