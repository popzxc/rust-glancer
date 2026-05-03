//! Filesystem storage for package cache artifacts.
//!
//! This module owns paths and atomic file replacement. Project-level code still owns invalidation:
//! the store can load bytes for an already-vetted header, but it does not decide whether a package
//! should be resident, rebuilt, or evicted.

use std::{
    ffi::OsStr,
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use rg_workspace::WorkspaceMetadata;

use super::{
    CachedPackage, Fingerprint, PackageCacheArtifact, PackageCacheCodec, PackageCacheHeader,
};

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

    pub fn package_artifact_path(&self, package: &CachedPackage) -> PathBuf {
        let fingerprint = self.package_fingerprint(package);
        let file_name = format!(
            "package-{}-{}-{}.{}",
            package.package.0, package.name, fingerprint, PACKAGE_ARTIFACT_EXTENSION,
        );

        self.root.join("packages").join(file_name)
    }

    pub fn package_fingerprint(&self, package: &CachedPackage) -> Fingerprint {
        package.fingerprint(&self.workspace_root)
    }

    pub fn write_artifact(&self, artifact: &PackageCacheArtifact) -> anyhow::Result<()> {
        let bytes = PackageCacheCodec::encode_artifact(artifact)?;
        let path = self.package_artifact_path(&artifact.header.package);
        let package_dir = path
            .parent()
            .expect("package cache artifact paths should always have a parent directory");

        fs::create_dir_all(package_dir).with_context(|| {
            format!(
                "while attempting to create package cache directory {}",
                package_dir.display(),
            )
        })?;

        // Write beside the destination and rename over it. This keeps readers from observing a
        // partially-written artifact if the process exits during serialization or disk I/O.
        let temp_path = self.write_temp_artifact(package_dir, &path, &bytes)?;

        if let Err(error) = fs::rename(&temp_path, &path) {
            let _ = fs::remove_file(&temp_path);
            return Err(error).with_context(|| {
                format!(
                    "while attempting to replace package cache artifact {}",
                    path.display(),
                )
            });
        }

        Ok(())
    }

    pub fn read_artifact(
        &self,
        header: &PackageCacheHeader,
    ) -> anyhow::Result<Option<PackageCacheArtifact>> {
        let path = self.package_artifact_path(&header.package);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "while attempting to read package cache artifact {}",
                        path.display(),
                    )
                });
            }
        };

        let artifact = PackageCacheCodec::decode_artifact(&bytes).with_context(|| {
            format!(
                "while attempting to decode package cache artifact {}",
                path.display(),
            )
        })?;

        if artifact.header != *header {
            anyhow::bail!(
                "package cache artifact {} has header for package #{} `{}`, expected package #{} `{}`",
                path.display(),
                artifact.header.package.package.0,
                artifact.header.package.name,
                header.package.package.0,
                header.package.name,
            );
        }

        Ok(Some(artifact))
    }

    /// Removes this workspace's cache namespace.
    ///
    /// This intentionally never reaches outside `<target>/rust_glancer/<workspace>`; callers can
    /// use it after schema or deserialization failures without touching Cargo's own build output.
    pub fn invalidate_workspace_cache(&self) -> anyhow::Result<()> {
        match fs::remove_dir_all(&self.root) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).with_context(|| {
                format!(
                    "while attempting to remove package cache namespace {}",
                    self.root.display(),
                )
            }),
        }
    }

    fn write_temp_artifact(
        &self,
        package_dir: &Path,
        artifact_path: &Path,
        bytes: &[u8],
    ) -> anyhow::Result<PathBuf> {
        let file_name = artifact_path
            .file_name()
            .expect("package cache artifact paths should always have a file name")
            .to_string_lossy();

        // TODO: Cringe, do it better; leaving it here for now just to get things working first
        for attempt in 0..100 {
            let temp_path = package_dir.join(format!(
                ".{}.{}.{}.tmp",
                file_name,
                std::process::id(),
                attempt,
            ));

            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)
            {
                Ok(mut file) => {
                    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
                        let _ = fs::remove_file(&temp_path);
                        return Err(error).with_context(|| {
                            format!(
                                "while attempting to write package cache artifact {}",
                                temp_path.display(),
                            )
                        });
                    }

                    return Ok(temp_path);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!(
                            "while attempting to create temporary package cache artifact {}",
                            temp_path.display(),
                        )
                    });
                }
            }
        }

        anyhow::bail!(
            "failed to allocate a temporary package cache artifact next to {}",
            artifact_path.display(),
        );
    }
}
