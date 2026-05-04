//! Cached workspace planning from normalized workspace and parse metadata.
//!
//! This module is the conversion boundary into the cache schema. Cargo/workspace metadata supplies
//! package identity and dependency edges, while parse metadata supplies the exact targets that were
//! analyzed and therefore must be present in a package artifact.

use std::path::Path;

use rg_workspace::{PackageSlot, WorkspaceMetadata};

use super::{
    CachedDependency, CachedPackage, CachedPackageId, CachedPackageSlot, CachedPackageSource,
    CachedPath, CachedRustEdition, CachedTarget, CachedTargetKind, Fingerprint, PackageCacheHeader,
    fingerprint,
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
    /// Builds cache metadata for the package targets actually analyzed by the current project.
    ///
    /// Cargo metadata can list dependency examples, tests, benches, and binaries that we do not
    /// parse for non-workspace packages. Package artifacts must describe the retained analysis
    /// payload, so their target list follows `ParseDb`, not raw Cargo metadata.
    pub fn build(workspace: &WorkspaceMetadata, parse: &rg_parse::ParseDb) -> Self {
        debug_assert_eq!(
            workspace.packages().len(),
            parse.packages().len(),
            "workspace and parse package slots should stay aligned",
        );

        let packages = workspace
            .packages()
            .iter()
            .zip(parse.packages())
            .enumerate()
            .map(|(package_slot, (package, parsed_package))| {
                debug_assert_eq!(
                    package.name,
                    parsed_package.package_name(),
                    "workspace and parse package slots should stay aligned",
                );

                CachedPackage {
                    package: CachedPackageSlot::from_workspace(PackageSlot(package_slot)),
                    package_id: CachedPackageId::from_workspace(&package.id),
                    name: package.name.clone(),
                    source: CachedPackageSource::from(package.source),
                    edition: CachedRustEdition::from(package.edition),
                    manifest_path: CachedPath::from_workspace_path(&package.manifest_path),
                    targets: parsed_package
                        .targets()
                        .iter()
                        .map(CachedTarget::from_parse_target)
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
                }
            })
            .collect();

        Self { packages }
    }

    /// Returns all cached packages in `WorkspaceMetadata::packages()` order.
    #[cfg(test)]
    pub(super) fn packages(&self) -> &[CachedPackage] {
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

    /// Returns the cache generation fingerprint for this workspace graph.
    ///
    /// Source edits keep this stable, while package/target/dependency metadata changes select a
    /// new artifact directory and make old generations eligible for cleanup.
    pub fn fingerprint(&self, workspace_root: &Path) -> Fingerprint {
        fingerprint::FingerprintBuilder::workspace_graph(workspace_root, self)
    }
}

impl CachedTarget {
    fn from_parse_target(target: &rg_parse::Target) -> Self {
        Self {
            name: target.name.clone(),
            kind: CachedTargetKind::from_workspace(&target.kind),
            src_path: CachedPath::from_workspace_path(&target.src_path),
        }
    }
}
