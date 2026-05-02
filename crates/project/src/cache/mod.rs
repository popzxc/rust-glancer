//! Cache artifact identities planned from normalized workspace metadata.
//!
//! This module deliberately stores fingerprint inputs as readable fields. Project-level code owns
//! invalidation because only it can see Cargo metadata, workspace graph changes, and the selected
//! residency policy; lower storage layers should receive already-vetted artifact handles.

use std::path::{Path, PathBuf};

mod fingerprint;
mod store;

use rg_body_ir::BodyIrPackageBundle;
use rg_def_map::DefMapPackageBundle;
use rg_semantic_ir::SemanticIrPackageBundle;
use rg_workspace::{
    PackageId, PackageSlot, PackageSource, RustEdition, TargetKind, WorkspaceMetadata,
};

pub use self::{fingerprint::Fingerprint, store::PackageCacheStore};

#[cfg(test)]
mod tests;

/// Current on-disk package artifact schema.
///
/// This version is not consumed yet. It exists so the first serialized artifacts will have an
/// explicit compatibility boundary instead of retrofitting one after files appear on disk.
pub const CURRENT_PACKAGE_CACHE_SCHEMA_VERSION: PackageCacheSchemaVersion =
    PackageCacheSchemaVersion(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PackageCacheSchemaVersion(pub u32);

/// Body IR payload state for one package artifact.
///
/// `SkippedByPolicy` is valid only when the current Body IR build policy does not require bodies
/// for this package. If a later policy needs bodies, the whole package artifact should be rejected
/// and rebuilt through the normal project-level path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageCacheBodyIrState {
    Built(Box<BodyIrPackageBundle>),
    SkippedByPolicy,
}

/// Header shared by future package cache artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageCacheHeader {
    pub schema_version: PackageCacheSchemaVersion,
    pub package: PackageCacheIdentity,
}

impl PackageCacheHeader {
    pub fn new(package: PackageCacheIdentity) -> Self {
        Self {
            schema_version: CURRENT_PACKAGE_CACHE_SCHEMA_VERSION,
            package,
        }
    }
}

/// One package artifact containing every retained analysis phase we currently cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageCacheArtifact {
    pub header: PackageCacheHeader,
    pub payload: PackageCachePayload,
}

impl PackageCacheArtifact {
    pub fn new(header: PackageCacheHeader, payload: PackageCachePayload) -> Self {
        Self { header, payload }
    }
}

/// Retained package data stored together to avoid mismatched phase fragments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageCachePayload {
    pub def_map: DefMapPackageBundle,
    pub semantic_ir: SemanticIrPackageBundle,
    pub body_ir: PackageCacheBodyIrState,
}

impl PackageCachePayload {
    pub fn new(
        def_map: DefMapPackageBundle,
        semantic_ir: SemanticIrPackageBundle,
        body_ir: PackageCacheBodyIrState,
    ) -> Self {
        Self {
            def_map,
            semantic_ir,
            body_ir,
        }
    }
}

/// Per-package cache identities for one workspace metadata snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageCachePlan {
    pub(crate) packages: Vec<PackageCacheIdentity>,
}

impl PackageCachePlan {
    pub fn build(workspace: &WorkspaceMetadata) -> Self {
        let packages = workspace
            .packages()
            .iter()
            .enumerate()
            .map(|(package_slot, package)| PackageCacheIdentity {
                package: PackageSlot(package_slot),
                package_id: package.id.clone(),
                name: package.name.clone(),
                source: package.source,
                edition: package.edition,
                manifest_path: package.manifest_path.clone(),
                targets: package
                    .targets
                    .iter()
                    .map(|target| PackageCacheTarget {
                        name: target.name.clone(),
                        kind: target.kind.clone(),
                        src_path: target.src_path.clone(),
                    })
                    .collect(),
                dependencies: package
                    .dependencies
                    .iter()
                    .map(|dependency| PackageCacheDependency {
                        package_id: dependency.package_id().clone(),
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

    /// Returns all package identities in `WorkspaceMetadata::packages()` order.
    pub fn packages(&self) -> &[PackageCacheIdentity] {
        &self.packages
    }

    /// Returns one package identity by stable package slot.
    pub fn package(&self, package: PackageSlot) -> Option<&PackageCacheIdentity> {
        self.packages.get(package.0)
    }

    /// Builds an artifact header for one package bundle.
    pub fn artifact_header(&self, package: PackageSlot) -> Option<PackageCacheHeader> {
        Some(PackageCacheHeader::new(self.package(package)?.clone()))
    }
}

/// Conservative identity inputs for one package artifact.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageCacheIdentity {
    pub package: PackageSlot,
    pub package_id: PackageId,
    pub name: String,
    pub source: PackageSource,
    pub edition: RustEdition,
    pub manifest_path: PathBuf,
    pub targets: Vec<PackageCacheTarget>,
    pub dependencies: Vec<PackageCacheDependency>,
}

impl PackageCacheIdentity {
    /// Returns the canonical package identity fingerprint for one workspace root.
    ///
    /// The workspace root is explicit because Cargo package IDs and source paths can contain
    /// absolute workspace paths that should not become part of the stable cache key.
    pub fn fingerprint(&self, workspace_root: &Path) -> Fingerprint {
        fingerprint::FingerprintBuilder::package_identity(workspace_root, self)
    }
}

/// Target metadata that can affect package-local analysis artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageCacheTarget {
    pub name: String,
    pub kind: TargetKind,
    pub src_path: PathBuf,
}

impl PackageCacheTarget {
    /// Returns targets in the deterministic order used by cache fingerprints and snapshots.
    pub fn sorted(targets: &[Self]) -> Vec<&Self> {
        let mut targets = targets.iter().collect::<Vec<_>>();
        targets.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        targets
    }

    fn sort_key(&self) -> (u8, &str, &Path) {
        (
            self.kind.sort_order(),
            self.name.as_str(),
            self.src_path.as_path(),
        )
    }
}

/// Dependency edge metadata that can affect package-local path resolution.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageCacheDependency {
    pub package_id: PackageId,
    pub name: String,
    pub is_normal: bool,
    pub is_build: bool,
    pub is_dev: bool,
}

impl PackageCacheDependency {
    /// Returns dependencies in the deterministic order used by cache fingerprints and snapshots.
    pub fn sorted(dependencies: &[Self]) -> Vec<&Self> {
        let mut dependencies = dependencies.iter().collect::<Vec<_>>();
        dependencies.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        dependencies
    }

    fn sort_key(&self) -> (&str, String, bool, bool, bool) {
        (
            self.name.as_str(),
            self.package_id.to_string(),
            self.is_normal,
            self.is_build,
            self.is_dev,
        )
    }
}
