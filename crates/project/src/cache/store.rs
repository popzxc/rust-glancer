//! Filesystem storage for package cache artifacts.
//!
//! This module owns paths and atomic file replacement. Project-level code still owns invalidation:
//! the store can load bytes for an already-vetted header, but it does not decide whether a package
//! should be resident, rebuilt, or evicted.

use std::{
    ffi::OsStr,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use atomic_write_file::AtomicWriteFile;
use rg_workspace::WorkspaceMetadata;

use super::{
    CachedPackage, CachedWorkspace, Fingerprint, PackageCacheArtifact, PackageCacheCodec,
    PackageCacheHeader,
};

const CACHE_DIR_NAME: &str = "rust_glancer";
const CACHE_PACKAGES_DIR_NAME: &str = "packages";
const CACHE_GENERATION_DIR_PREFIX: &str = "graph-";
const PACKAGE_ARTIFACT_EXTENSION: &str = "rgpkg";

/// Root and naming policy for package cache artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageCacheStore {
    workspace_root: PathBuf,
    root: PathBuf,
    generation: Fingerprint,
}

impl PackageCacheStore {
    /// Plans cache paths for a workspace using Cargo's target directory convention.
    pub fn for_workspace(
        workspace: &WorkspaceMetadata,
        cached_workspace: &CachedWorkspace,
    ) -> Self {
        let target_dir = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| workspace.workspace_root().join("target"));

        Self::for_workspace_with_target_dir(workspace, cached_workspace, target_dir)
    }

    /// Plans cache paths under an explicit Cargo target directory.
    pub(super) fn for_workspace_with_target_dir(
        workspace: &WorkspaceMetadata,
        cached_workspace: &CachedWorkspace,
        target_dir: impl Into<PathBuf>,
    ) -> Self {
        let workspace_name = workspace
            .workspace_root()
            .file_name()
            .unwrap_or_else(|| OsStr::new("workspace"));

        Self {
            workspace_root: workspace.workspace_root().to_path_buf(),
            root: target_dir.into().join(CACHE_DIR_NAME).join(workspace_name),
            generation: cached_workspace.fingerprint(workspace.workspace_root()),
        }
    }

    #[cfg(test)]
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub fn package_artifact_path(&self, package: &CachedPackage) -> PathBuf {
        let fingerprint = self.package_fingerprint(package);
        let file_name = format!(
            "package-{}-{}-{}.{}",
            package.package.0, package.name, fingerprint, PACKAGE_ARTIFACT_EXTENSION,
        );

        self.generation_dir().join(file_name)
    }

    pub fn package_fingerprint(&self, package: &CachedPackage) -> Fingerprint {
        package.fingerprint(&self.workspace_root)
    }

    /// Removes cache data that cannot be reached through the current workspace graph generation.
    ///
    /// The store deliberately does not track individual artifacts. A source-only save rewrites the
    /// affected package files inside the same generation directory, while Cargo graph changes pick
    /// a new generation and make the older directories disposable.
    pub(crate) fn cleanup_stale_generations(&self) -> anyhow::Result<()> {
        let packages_dir = self.packages_dir();
        let entries = match fs::read_dir(&packages_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "while attempting to read package cache directory {}",
                        packages_dir.display(),
                    )
                });
            }
        };
        let current_generation = self.generation_dir_name();

        for entry in entries {
            let entry = entry.with_context(|| {
                format!(
                    "while attempting to inspect package cache directory {}",
                    packages_dir.display(),
                )
            })?;
            let path = entry.path();
            let file_name = entry.file_name();
            let Some(file_name) = file_name.to_str() else {
                continue;
            };
            let file_type = entry.file_type().with_context(|| {
                format!(
                    "while attempting to inspect package cache entry {}",
                    path.display(),
                )
            })?;

            if file_type.is_dir()
                && file_name.starts_with(CACHE_GENERATION_DIR_PREFIX)
                && file_name != current_generation
            {
                fs::remove_dir_all(&path).with_context(|| {
                    format!(
                        "while attempting to remove stale package cache generation {}",
                        path.display(),
                    )
                })?;
            }
        }

        Ok(())
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

        Self::write_artifact_bytes(&path, bytes.as_ref())
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

    fn write_artifact_bytes(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
        // Cache artifacts must appear atomically: readers either observe the previous complete
        // payload or the newly committed one, never a partially written file.
        let mut file = AtomicWriteFile::options().open(path).with_context(|| {
            format!(
                "while attempting to start atomic package cache write {}",
                path.display(),
            )
        })?;
        file.write_all(bytes).with_context(|| {
            format!(
                "while attempting to write package cache artifact {}",
                path.display(),
            )
        })?;
        file.commit().with_context(|| {
            format!(
                "while attempting to commit package cache artifact {}",
                path.display(),
            )
        })
    }

    fn packages_dir(&self) -> PathBuf {
        self.root.join(CACHE_PACKAGES_DIR_NAME)
    }

    fn generation_dir(&self) -> PathBuf {
        self.packages_dir().join(self.generation_dir_name())
    }

    fn generation_dir_name(&self) -> String {
        format!("{CACHE_GENERATION_DIR_PREFIX}{}", self.generation)
    }
}
