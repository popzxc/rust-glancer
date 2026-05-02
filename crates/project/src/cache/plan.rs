//! Cached workspace planning from normalized workspace metadata.
//!
//! This module is the conversion boundary between Cargo/workspace metadata and the cache schema.
//! The resulting cached packages are readable inputs for later path planning, fingerprinting, and
//! artifact validation.

use rg_workspace::{PackageSlot, WorkspaceMetadata};

use super::{
    CachedDependency, CachedPackage, CachedPackageId, CachedPackageSlot, CachedPackageSource,
    CachedPath, CachedRustEdition, CachedTarget, CachedTargetKind, PackageCacheHeader,
};

/// Cache-schema view of one workspace metadata snapshot.
// Note: It is `CachedWorkspace` as it currently represents a set of all the workspace
// cached packages; but it is not going to be cached itself. Probably deserves a better
// name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedWorkspace {
    pub(crate) packages: Vec<CachedPackage>,
}

impl CachedWorkspace {
    pub fn build(workspace: &WorkspaceMetadata) -> Self {
        let packages = workspace
            .packages()
            .iter()
            .enumerate()
            .map(|(package_slot, package)| CachedPackage {
                package: CachedPackageSlot::from_workspace(PackageSlot(package_slot)),
                package_id: CachedPackageId::from_workspace(&package.id),
                name: package.name.clone(),
                source: CachedPackageSource::from(package.source),
                edition: CachedRustEdition::from(package.edition),
                manifest_path: CachedPath::from_workspace_path(&package.manifest_path),
                targets: package
                    .targets
                    .iter()
                    .map(|target| CachedTarget {
                        name: target.name.clone(),
                        kind: CachedTargetKind::from_workspace(&target.kind),
                        src_path: CachedPath::from_workspace_path(&target.src_path),
                    })
                    .collect(),
                dependencies: package
                    .dependencies
                    .iter()
                    .map(|dependency| CachedDependency {
                        package_id: CachedPackageId::from_workspace(dependency.package_id()),
                        name: dependency.name().to_string(),
                        is_normal: dependency.is_normal(),
                        is_build: dependency.is_build(),
                        is_dev: dependency.is_dev(),
                    })
                    .collect(),
            })
            .collect();

        Self { packages }
    }

    /// Returns all cached packages in `WorkspaceMetadata::packages()` order.
    pub fn packages(&self) -> &[CachedPackage] {
        &self.packages
    }

    /// Returns one cached package by stable package slot.
    pub fn package(&self, package: PackageSlot) -> Option<&CachedPackage> {
        self.packages.get(package.0)
    }

    /// Builds an artifact header for one package bundle.
    pub fn artifact_header(&self, package: PackageSlot) -> Option<PackageCacheHeader> {
        Some(PackageCacheHeader::new(self.package(package)?.clone()))
    }
}
