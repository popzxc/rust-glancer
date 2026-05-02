//! Filesystem path planning for future package cache artifacts.
//!
//! This module is intentionally inert: it decides where artifacts would live and how package
//! identities are fingerprinted, but it does not read, write, or validate serialized bytes yet.

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use super::{Fingerprint, PackageCacheIdentity};
use rg_workspace::WorkspaceMetadata;

const CACHE_DIR_NAME: &str = "rust_glancer";
const PACKAGE_ARTIFACT_EXTENSION: &str = "rgpkg";

/// Root and naming policy for package cache artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageCacheStore {
    workspace_root: PathBuf,
    root: PathBuf,
}

impl PackageCacheStore {
    /// Plans cache paths for a workspace using Cargo's target directory convention.
    pub fn for_workspace(workspace: &WorkspaceMetadata) -> Self {
        let target_dir = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| workspace.workspace_root().join("target"));

        Self::for_workspace_with_target_dir(workspace, target_dir)
    }

    /// Plans cache paths under an explicit Cargo target directory.
    pub(super) fn for_workspace_with_target_dir(
        workspace: &WorkspaceMetadata,
        target_dir: impl Into<PathBuf>,
    ) -> Self {
        let workspace_name = workspace
            .workspace_root()
            .file_name()
            .unwrap_or_else(|| OsStr::new("workspace"));

        Self {
            workspace_root: workspace.workspace_root().to_path_buf(),
            root: target_dir.into().join(CACHE_DIR_NAME).join(workspace_name),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn package_artifact_path(&self, package: &PackageCacheIdentity) -> PathBuf {
        let fingerprint = self.package_fingerprint(package);
        let file_name = format!(
            "package-{}-{}-{}.{}",
            package.package.0, package.name, fingerprint, PACKAGE_ARTIFACT_EXTENSION,
        );

        self.root.join("packages").join(file_name)
    }

    pub fn package_fingerprint(&self, package: &PackageCacheIdentity) -> Fingerprint {
        package.fingerprint(&self.workspace_root)
    }
}
