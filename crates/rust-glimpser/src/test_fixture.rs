use std::{
    fs, io,
    path::PathBuf,
    process,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::parse::package::PackageIndex;

/// Creates temporary on-disk crate fixtures from inline file contents.
///
/// Parser tests should exercise the same `cargo metadata` path as production code, but many of
/// them only need a tiny crate with one or two files. This helper lets those tests define the
/// exact crate layout they need without depending on the larger checked-in fixture projects.
pub(crate) struct CrateFixture {
    root: PathBuf,
}

impl CrateFixture {
    /// Materializes a crate fixture under the system temp directory.
    pub(crate) fn new<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let root = Self::create_root_directory();

        for (relative_path, contents) in files {
            let path = root.join(relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("fixture directories should be created");
            }
            fs::write(path, contents).expect("fixture file should be written");
        }

        Self { root }
    }

    /// Resolves a relative path within the fixture root.
    pub(crate) fn path(&self, relative_path: &str) -> PathBuf {
        self.root.join(relative_path)
    }

    /// Loads cargo metadata for the fixture crate.
    pub(crate) fn metadata(&self) -> cargo_metadata::Metadata {
        cargo_metadata::MetadataCommand::new()
            .manifest_path(self.manifest_path())
            .exec()
            .expect("fixture metadata should load")
    }

    /// Returns the package described by the fixture's root manifest.
    pub(crate) fn package(&self) -> cargo_metadata::Package {
        let metadata = self.metadata();

        metadata
            .root_package()
            .cloned()
            .or_else(|| metadata.workspace_packages().into_iter().next().cloned())
            .expect("fixture package should be present in metadata")
    }

    /// Builds an index for the full fixture crate.
    pub(crate) fn package_index(&self) -> PackageIndex {
        PackageIndex::build(self.package(), true).expect("fixture crate should parse")
    }

    /// Builds an index that keeps only one target root.
    pub(crate) fn package_index_for_target(&self, relative_path: &str) -> PackageIndex {
        let root_file = self
            .path(relative_path)
            .canonicalize()
            .expect("fixture target path should resolve");

        self.package_index_matching_targets(|target| {
            target
                .src_path
                .as_std_path()
                .canonicalize()
                .expect("metadata target path should resolve")
                == root_file
        })
    }

    /// Builds an index after filtering the manifest target set.
    pub(crate) fn package_index_matching_targets(
        &self,
        keep_target: impl FnMut(&cargo_metadata::Target) -> bool,
    ) -> PackageIndex {
        let mut package = self.package();
        package.targets.retain(keep_target);

        PackageIndex::build(package, true).expect("fixture crate should parse")
    }

    /// Builds an index with a caller-provided target list.
    pub(crate) fn package_index_with_targets(
        &self,
        targets: Vec<cargo_metadata::Target>,
    ) -> PackageIndex {
        let mut package = self.package();
        package.targets = targets;

        PackageIndex::build(package, true).expect("fixture crate should parse")
    }

    fn manifest_path(&self) -> PathBuf {
        self.path("Cargo.toml")
    }

    fn create_root_directory() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let base = std::env::temp_dir().join("rust-glimpser-test-fixtures");
        fs::create_dir_all(&base).expect("fixture base directory should be created");

        for _ in 0..32 {
            let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos();
            let root = base.join(format!("crate-{}-{timestamp}-{sequence}", process::id()));

            match fs::create_dir(&root) {
                Ok(()) => return root,
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("fixture root directory should be created: {err}"),
            }
        }

        panic!("fixture root directory should be unique");
    }
}

impl Drop for CrateFixture {
    fn drop(&mut self) {
        if let Err(err) = fs::remove_dir_all(&self.root) {
            if err.kind() != io::ErrorKind::NotFound {
                panic!(
                    "fixture root directory should be removed on drop: {}",
                    self.root.display()
                );
            }
        }
    }
}

macro_rules! fixture_crate {
    ($($path:literal => $contents:expr),+ $(,)?) => {{
        $crate::test_fixture::CrateFixture::new([$(($path, $contents)),+])
    }};
}

pub(crate) use fixture_crate;
