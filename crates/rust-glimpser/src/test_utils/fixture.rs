use std::{
    fs, io,
    path::PathBuf,
    process,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{Project, WorkspaceMetadata};

use super::query::FixtureProject;

/// Creates temporary on-disk crate fixtures from inline file contents.
///
/// Parser tests should exercise the same `cargo metadata` path as production code, but many of
/// them only need a tiny crate with one or two files. This helper lets those tests define the
/// exact crate layout they need without depending on the larger checked-in fixture projects.
pub(crate) struct CrateFixture {
    root: PathBuf,
}

impl CrateFixture {
    /// Materializes a crate fixture from the following syntax (inspired by rust-analyzer):
    ///
    /// ```text
    /// //- /Cargo.toml
    /// [package]
    /// name = "demo"
    ///
    /// //- /src/lib.rs
    /// pub fn work() {}
    /// ```
    ///
    /// Cargo metadata still remains the source of truth for package/target/dependency structure,
    /// so rust-analyzer header metadata such as `crate:` or `deps:` is intentionally not parsed.
    pub(crate) fn from_fixture_spec(spec: &str) -> Self {
        Self::materialize(Self::parse_fixture_spec(spec))
    }

    fn materialize<P, C>(files: impl IntoIterator<Item = (P, C)>) -> Self
    where
        P: AsRef<str>,
        C: AsRef<str>,
    {
        let root = Self::create_root_directory();

        for (relative_path, contents) in files {
            let path = root.join(relative_path.as_ref());
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("fixture directories should be created");
            }
            fs::write(path, contents.as_ref()).expect("fixture file should be written");
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

    /// Loads normalized workspace metadata for the fixture crate.
    pub(crate) fn workspace_metadata(&self) -> WorkspaceMetadata {
        WorkspaceMetadata::from_cargo(self.metadata())
    }

    /// Runs full project analysis for the fixture and exposes a test query API.
    pub(crate) fn analyze(&self) -> FixtureProject {
        FixtureProject {
            project: Project::build(self.workspace_metadata())
                .expect("fixture project should analyze"),
        }
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

    /// Builds the full project pipeline for the fixture crate.
    pub(crate) fn project(&self) -> Project {
        Project::build(self.workspace_metadata()).expect("fixture project should build")
    }

    fn manifest_path(&self) -> PathBuf {
        self.path("Cargo.toml")
    }

    fn parse_fixture_spec(spec: &str) -> Vec<(String, String)> {
        let spec = Self::trim_fixture_indent(spec);
        let mut files = Vec::new();
        let mut current_path = None::<String>;
        let mut current_contents = String::new();

        for line in spec.lines() {
            if let Some(header) = line.strip_prefix("//- ") {
                if let Some(path) = current_path.take() {
                    files.push((path, current_contents));
                    current_contents = String::new();
                }

                current_path = Some(Self::parse_fixture_header(header));
                continue;
            }

            if current_path.is_none() {
                if line.trim().is_empty() {
                    continue;
                }

                panic!(
                    "fixture content must start with `//- /path`; found `{}`",
                    line
                );
            }

            current_contents.push_str(line);
            current_contents.push('\n');
        }

        if let Some(path) = current_path {
            files.push((path, current_contents));
        }

        assert!(
            !files.is_empty(),
            "fixture specification should contain at least one `//- /path` header"
        );

        files
    }

    fn parse_fixture_header(header: &str) -> String {
        let (path, metadata) = header
            .split_once(char::is_whitespace)
            .unwrap_or((header, ""));
        assert!(
            metadata.trim().is_empty(),
            "fixture header metadata is not supported yet: `{}`",
            metadata.trim()
        );
        assert!(
            path.starts_with('/'),
            "fixture path should start with `/`: `{path}`"
        );

        let relative_path = path.trim_start_matches('/');
        assert!(
            !relative_path.is_empty(),
            "fixture path should not be empty"
        );
        relative_path.to_string()
    }

    fn trim_fixture_indent(spec: &str) -> String {
        let spec = spec.strip_prefix('\n').unwrap_or(spec);
        let min_indent = spec
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(Self::leading_indent)
            .min()
            .unwrap_or(0);

        let mut trimmed = String::new();

        for (idx, line) in spec.lines().enumerate() {
            if idx > 0 {
                trimmed.push('\n');
            }

            if line.trim().is_empty() {
                continue;
            }

            trimmed.push_str(&line[min_indent..]);
        }

        trimmed
    }

    fn leading_indent(line: &str) -> usize {
        line.as_bytes()
            .iter()
            .take_while(|byte| matches!(byte, b' ' | b'\t'))
            .count()
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

pub fn fixture_crate(fixture: &str) -> CrateFixture {
    CrateFixture::from_fixture_spec(fixture)
}
