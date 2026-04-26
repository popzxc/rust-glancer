use std::{
    collections::BTreeMap,
    fs, io,
    path::PathBuf,
    process,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const CURSOR_MARKER_NAME: &str = "0";

/// Parsed fixture files and source markers before they are materialized on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureSpec {
    files: Vec<FixtureFile>,
    markers: FixtureMarkers,
}

impl FixtureSpec {
    pub fn parse(spec: &str) -> Self {
        let spec = Self::trim_fixture_indent(spec);
        let mut files = Vec::new();
        let mut markers = FixtureMarkers::default();
        let mut current_path = None::<String>;
        let mut current_contents = String::new();
        let mut current_offset = 0_u32;

        for line in spec.lines() {
            if let Some(header) = line.strip_prefix("//- ") {
                if let Some(path) = current_path.take() {
                    files.push(FixtureFile {
                        relative_path: path,
                        contents: current_contents,
                    });
                    current_contents = String::new();
                }

                current_path = Some(Self::parse_fixture_header(header));
                current_offset = 0;
                continue;
            }

            let Some(path) = &current_path else {
                if line.trim().is_empty() {
                    continue;
                }

                panic!("fixture content must start with `//- /path`; found `{line}`");
            };

            let cleaned = Self::strip_markers_from_line(line, path, current_offset, &mut markers);
            current_offset +=
                u32::try_from(cleaned.len() + 1).expect("fixture line length should fit into u32");
            current_contents.push_str(&cleaned);
            current_contents.push('\n');
        }

        if let Some(path) = current_path {
            files.push(FixtureFile {
                relative_path: path,
                contents: current_contents,
            });
        }

        assert!(
            !files.is_empty(),
            "fixture specification should contain at least one `//- /path` header"
        );

        Self { files, markers }
    }

    pub fn files(&self) -> &[FixtureFile] {
        &self.files
    }

    pub fn markers(&self) -> &FixtureMarkers {
        &self.markers
    }

    fn into_files(self) -> Vec<FixtureFile> {
        self.files
    }

    fn strip_markers_from_line(
        line: &str,
        path: &str,
        line_offset: u32,
        markers: &mut FixtureMarkers,
    ) -> String {
        let mut cleaned = String::new();
        let mut idx = 0;

        while idx < line.len() {
            let rest = &line[idx..];
            if let Some(stripped) = rest.strip_prefix(r"\$") {
                if stripped.starts_with('0') {
                    cleaned.push_str("$0");
                    idx += 3;
                    continue;
                }
                if let Some(marker_name) = Self::take_named_marker(stripped) {
                    cleaned.push('$');
                    cleaned.push_str(marker_name);
                    cleaned.push('$');
                    idx += marker_name.len() + 3;
                    continue;
                }
            }

            if let Some(stripped) = rest.strip_prefix('$') {
                if stripped.starts_with('0') {
                    markers.push(
                        CURSOR_MARKER_NAME,
                        FixtureMarker {
                            path: path.to_string(),
                            offset: line_offset
                                + u32::try_from(cleaned.len())
                                    .expect("fixture line length should fit into u32"),
                        },
                    );
                    idx += 2;
                    continue;
                }
                if let Some(marker_name) = Self::take_named_marker(stripped) {
                    markers.push(
                        marker_name,
                        FixtureMarker {
                            path: path.to_string(),
                            offset: line_offset
                                + u32::try_from(cleaned.len())
                                    .expect("fixture line length should fit into u32"),
                        },
                    );
                    idx += marker_name.len() + 2;
                    continue;
                }
            }

            let ch = rest
                .chars()
                .next()
                .expect("non-empty line rest should have a first char");
            cleaned.push(ch);
            idx += ch.len_utf8();
        }

        cleaned
    }

    fn take_named_marker(text: &str) -> Option<&str> {
        let end = text.find('$')?;
        let name = &text[..end];
        let mut chars = name.chars();
        let first = chars.next()?;
        if !(first == '_' || first.is_ascii_alphabetic()) {
            return None;
        }
        if chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric()) {
            Some(name)
        } else {
            None
        }
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
            "fixture path should start with `/`: {path}"
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureFile {
    relative_path: String,
    contents: String,
}

impl FixtureFile {
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub fn contents(&self) -> &str {
        &self.contents
    }
}

/// Source marker metadata stripped from a parsed fixture.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FixtureMarkers {
    markers: BTreeMap<String, Vec<FixtureMarker>>,
}

impl FixtureMarkers {
    /// Returns one cursor marker by name.
    ///
    /// `$0` is exposed as marker name `"0"`. Named markers use `$name$`.
    pub fn position(&self, name: &str) -> &FixtureMarker {
        let positions = self
            .markers
            .get(name)
            .unwrap_or_else(|| panic!("marker `{name}` should exist in fixture"));
        assert_eq!(
            positions.len(),
            1,
            "marker `{name}` should appear exactly once for an offset query"
        );

        &positions[0]
    }

    fn push(&mut self, name: impl Into<String>, marker: FixtureMarker) {
        self.markers.entry(name.into()).or_default().push(marker);
    }
}

/// One stripped source marker position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureMarker {
    pub path: String,
    pub offset: u32,
}

/// Creates temporary on-disk crate fixtures from inline file contents.
///
/// Parser tests should exercise the same `cargo metadata` path as production code, but many of
/// them only need a tiny crate with one or two files. This helper lets those tests define the
/// exact crate layout they need without depending on the larger checked-in fixture projects.
pub struct CrateFixture {
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
    pub fn from_fixture_spec(spec: &str) -> Self {
        Self::from_parsed_fixture(FixtureSpec::parse(spec))
    }

    fn from_parsed_fixture(fixture: FixtureSpec) -> Self {
        Self::materialize(
            fixture
                .into_files()
                .into_iter()
                .map(|file| (file.relative_path, file.contents)),
        )
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
    pub fn path(&self, relative_path: &str) -> PathBuf {
        self.root.join(relative_path)
    }

    /// Loads cargo metadata for the fixture crate.
    pub fn metadata(&self) -> cargo_metadata::Metadata {
        cargo_metadata::MetadataCommand::new()
            .manifest_path(self.manifest_path())
            .exec()
            .expect("fixture metadata should load")
    }

    /// Returns the package described by the fixture's root manifest.
    pub fn package(&self) -> cargo_metadata::Package {
        let metadata = self.metadata();

        metadata
            .root_package()
            .cloned()
            .or_else(|| metadata.workspace_packages().into_iter().next().cloned())
            .expect("fixture package should be present in metadata")
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

pub fn fixture_crate(fixture: &str) -> CrateFixture {
    CrateFixture::from_fixture_spec(fixture)
}

pub fn fixture_crate_with_markers(fixture: &str) -> (CrateFixture, FixtureMarkers) {
    let parsed = FixtureSpec::parse(fixture);
    let markers = parsed.markers().clone();
    (CrateFixture::from_parsed_fixture(parsed), markers)
}

#[cfg(test)]
mod tests {
    use crate::test_fixture::FixtureSpec;

    #[test]
    fn strips_shared_source_markers_from_fixture_files() {
        let fixture = FixtureSpec::parse(
            r#"
//- /Cargo.toml
[package]
name = "marker_fixture"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub fn use_it(user: User) {
    let local = loc$goto$al;
    user.$0id();
    let escaped = "\$0 and \$name$";
}
"#,
        );
        let source = fixture
            .files()
            .iter()
            .find(|file| file.relative_path() == "src/lib.rs")
            .expect("source file should exist in parsed fixture");

        assert!(source.contents().contains("let local = local;"));
        assert!(source.contents().contains("user.id();"));
        assert!(source.contents().contains(r#""$0 and $name$""#));

        let goto = fixture.markers.position("goto");
        assert_eq!(goto.path, "src/lib.rs");
        assert_eq!(
            goto.offset,
            u32::try_from(
                source
                    .contents()
                    .find("local;")
                    .expect("local should be present")
                    + 3
            )
            .expect("fixture offset should fit into u32")
        );

        let cursor = fixture.markers.position("0");
        assert_eq!(cursor.path, "src/lib.rs");
        assert_eq!(
            cursor.offset,
            u32::try_from(
                source
                    .contents()
                    .find("id();")
                    .expect("method name should be present")
            )
            .expect("fixture offset should fit into u32")
        );
    }
}
