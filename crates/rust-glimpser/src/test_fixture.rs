use std::{
    fmt::Write as _,
    fs, io,
    path::PathBuf,
    process,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::parse::item::VisibilityLevel;
use crate::parse::{
    ProjectAnalysis,
    def_map::{DefId, ModuleId, ScopeBinding, ScopeEntry},
    package::PackageIndex,
    target::TargetIndex,
};

/// Creates temporary on-disk crate fixtures from inline file contents.
///
/// Parser tests should exercise the same `cargo metadata` path as production code, but many of
/// them only need a tiny crate with one or two files. This helper lets those tests define the
/// exact crate layout they need without depending on the larger checked-in fixture projects.
pub(crate) struct CrateFixture {
    root: PathBuf,
}

impl CrateFixture {
    /// Materializes a crate fixture from a rust-analyzer-style multi-file fixture string.
    ///
    /// Only the file-splitting syntax is supported right now:
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

    /// Runs full project analysis for the fixture and exposes a test query API.
    pub(crate) fn analyze(&self) -> FixtureProject {
        FixtureProject {
            analysis: ProjectAnalysis::build(self.metadata())
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

/// Project-level test query API built on top of one analyzed fixture.
///
/// The goal is to let tests assert on visible behavior without reaching through raw slot ids
/// or the exact storage layout of the parser and def map internals.
pub(crate) struct FixtureProject {
    analysis: ProjectAnalysis,
}

impl FixtureProject {
    /// Returns the library target for one package.
    pub(crate) fn lib(&self, package_name: &str) -> FixtureTarget<'_> {
        self.target(package_name, cargo_metadata::TargetKind::Lib)
    }

    /// Returns the binary target for one package.
    pub(crate) fn bin(&self, package_name: &str) -> FixtureTarget<'_> {
        self.target(package_name, cargo_metadata::TargetKind::Bin)
    }

    /// Renders a stable textual view of all analyzed target namespace maps.
    pub(crate) fn def_map_dump(&self) -> String {
        let mut packages = self.analysis.packages().iter().collect::<Vec<_>>();
        packages.sort_by(|left, right| left.package_name().cmp(right.package_name()));

        let package_dumps = packages
            .into_iter()
            .map(|package| {
                let mut targets = package.targets().iter().collect::<Vec<_>>();
                targets.sort_by(|left, right| {
                    (
                        target_kind_sort_order(left),
                        left.cargo_target.name.as_str(),
                        left.cargo_target.src_path.as_str(),
                    )
                        .cmp(&(
                            target_kind_sort_order(right),
                            right.cargo_target.name.as_str(),
                            right.cargo_target.src_path.as_str(),
                        ))
                });

                let target_dumps = targets
                    .into_iter()
                    .map(|target| {
                        FixtureTarget {
                            analysis: &self.analysis,
                            package,
                            target,
                        }
                        .def_map_dump()
                        .trim_end()
                        .to_string()
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n");

                format!("package {}\n\n{target_dumps}", package.package_name())
            })
            .collect::<Vec<_>>();

        package_dumps.join("\n\n")
    }

    fn target(
        &self,
        package_name: &str,
        expected_kind: cargo_metadata::TargetKind,
    ) -> FixtureTarget<'_> {
        let package = self
            .analysis
            .packages()
            .iter()
            .find(|package| package.package_name() == package_name)
            .unwrap_or_else(|| panic!("fixture package `{package_name}` should exist"));
        let target = package
            .targets()
            .iter()
            .find(|target| {
                target
                    .cargo_target
                    .kind
                    .iter()
                    .any(|target_kind| target_kind == &expected_kind)
            })
            .unwrap_or_else(|| {
                panic!(
                    "fixture package `{package_name}` should have a {:?} target",
                    expected_kind
                )
            });

        FixtureTarget {
            analysis: &self.analysis,
            package,
            target,
        }
    }
}

/// Target-scoped test query API.
///
/// `entry("name")` always means "lookup `name` in the root module scope of this target".
pub(crate) struct FixtureTarget<'a> {
    analysis: &'a ProjectAnalysis,
    package: &'a PackageIndex,
    target: &'a TargetIndex,
}

impl<'a> FixtureTarget<'a> {
    /// Looks up one textual name in the root module scope of this target.
    pub(crate) fn entry(&self, name: &str) -> FixtureEntry<'a> {
        FixtureEntry {
            analysis: self.analysis,
            package_name: self.package.package_name(),
            target: self.target,
            name: name.to_string(),
            entry: self.target.root_scope_entry(name),
        }
    }

    /// Renders a stable textual view of this target namespace map for snapshot tests.
    pub(crate) fn def_map_dump(&self) -> String {
        let mut modules = self
            .target
            .def_map
            .modules
            .iter()
            .enumerate()
            .map(|(idx, _)| {
                let module_id = ModuleId(idx);
                (self.module_path(module_id), module_id)
            })
            .collect::<Vec<_>>();
        modules.sort_by(|left, right| left.0.cmp(&right.0));

        let mut dump = String::new();
        writeln!(
            &mut dump,
            "{} [{}]",
            self.package.package_name(),
            target_kind_label(self.target)
        )
        .expect("string writes should not fail");

        for (idx, (module_path, module_id)) in modules.into_iter().enumerate() {
            if idx > 0 {
                dump.push('\n');
            }

            writeln!(&mut dump, "{module_path}").expect("string writes should not fail");

            let module = self
                .target
                .def_map
                .module(module_id)
                .expect("module id should exist in def map dump");
            let mut names = module.scope.names.keys().cloned().collect::<Vec<_>>();
            names.sort();

            for name in names {
                let entry = module
                    .scope
                    .entry(&name)
                    .expect("scope entry should exist while dumping");
                writeln!(&mut dump, "- {name} : {}", self.render_scope_entry(entry))
                    .expect("string writes should not fail");
            }
        }

        dump
    }

    fn render_scope_entry(&self, entry: &ScopeEntry) -> String {
        let mut parts = Vec::new();

        if !entry.types.is_empty() {
            parts.push(format!(
                "type [{}]",
                self.render_namespace_bindings(&entry.types)
            ));
        }

        if !entry.values.is_empty() {
            parts.push(format!(
                "value [{}]",
                self.render_namespace_bindings(&entry.values)
            ));
        }

        if !entry.macros.is_empty() {
            parts.push(format!(
                "macro [{}]",
                self.render_namespace_bindings(&entry.macros)
            ));
        }

        parts.join(" | ")
    }

    fn render_namespace_bindings(&self, bindings: &[ScopeBinding]) -> String {
        let mut rendered = bindings
            .iter()
            .filter_map(|binding| self.binding_origin(binding))
            .map(|origin| origin.render(&self.analysis))
            .collect::<Vec<_>>();
        rendered.sort();
        rendered.join("; ")
    }

    fn binding_origin(&self, binding: &'a ScopeBinding) -> Option<FixtureBindingOrigin<'a>> {
        let target_ref = match binding.def {
            DefId::Module(module_ref) => module_ref.target,
            DefId::Local(local_def_ref) => local_def_ref.target,
        };
        let package = self.analysis.packages().get(target_ref.package.0)?;
        let target = package.target(target_ref.target)?;

        Some(FixtureBindingOrigin {
            target,
            def: binding.def,
            binding_visibility: &binding.visibility,
        })
    }

    fn module_path(&self, module_id: ModuleId) -> String {
        let module = self
            .target
            .def_map
            .module(module_id)
            .expect("module id should exist while building module path");

        match module.parent {
            Some(parent) => {
                let parent_path = self.module_path(parent);
                let name = module
                    .name
                    .as_deref()
                    .expect("non-root modules should have names");
                format!("{parent_path}::{name}")
            }
            None => "crate".to_string(),
        }
    }
}

/// Root-scope entry assertion helper for one textual name.
pub(crate) struct FixtureEntry<'a> {
    analysis: &'a ProjectAnalysis,
    package_name: &'a str,
    target: &'a TargetIndex,
    name: String,
    entry: Option<&'a ScopeEntry>,
}

impl<'a> FixtureEntry<'a> {
    /// Asserts that the entry is absent from the root scope.
    pub(crate) fn assert_missing(&self, reason: &str) -> &Self {
        assert!(
            self.entry.is_none(),
            "{reason}: expected {} to be absent",
            self.context(),
        );
        self
    }

    /// Asserts that the entry has at least one visible type binding.
    pub(crate) fn assert_type_exists(&self, reason: &str) -> &Self {
        assert!(
            !self.scope_entry().types.is_empty(),
            "{reason}: expected {} to have a type binding",
            self.context(),
        );
        self
    }

    /// Asserts that the entry has at least one visible value binding.
    pub(crate) fn assert_value_exists(&self, reason: &str) -> &Self {
        assert!(
            !self.scope_entry().values.is_empty(),
            "{reason}: expected {} to have a value binding",
            self.context(),
        );
        self
    }

    /// Asserts that one type binding resolves to a module with the requested name.
    pub(crate) fn assert_module_named(&self, module_name: &str, reason: &str) -> &Self {
        assert!(
            self.scope_entry()
                .types
                .iter()
                .filter_map(|binding| self.binding_origin(binding))
                .any(|origin| origin.module_name() == Some(module_name)),
            "{reason}: expected {} to resolve to module `{module_name}`",
            self.context(),
        );
        self
    }

    fn context(&self) -> String {
        format!(
            "root scope entry `{}` in package `{}` target `{}` ({:?})",
            self.name,
            self.package_name,
            self.target.cargo_target.name,
            self.target.cargo_target.kind,
        )
    }

    fn scope_entry(&self) -> &ScopeEntry {
        self.entry.unwrap_or_else(|| {
            panic!(
                "expected {} to exist before asserting on its bindings",
                self.context()
            )
        })
    }

    fn binding_origin(&self, binding: &'a ScopeBinding) -> Option<FixtureBindingOrigin<'a>> {
        let target_ref = match binding.def {
            DefId::Module(module_ref) => module_ref.target,
            DefId::Local(local_def_ref) => local_def_ref.target,
        };
        let package = self.analysis.packages().get(target_ref.package.0)?;
        let target = package.target(target_ref.target)?;

        Some(FixtureBindingOrigin {
            target,
            def: binding.def,
            binding_visibility: &binding.visibility,
        })
    }
}

/// Project-relative view of one resolved binding origin.
struct FixtureBindingOrigin<'a> {
    target: &'a TargetIndex,
    def: DefId,
    binding_visibility: &'a VisibilityLevel,
}

impl<'a> FixtureBindingOrigin<'a> {
    fn module_name(&self) -> Option<&str> {
        let DefId::Module(module_ref) = self.def else {
            return None;
        };

        self.target
            .module(module_ref.module)
            .and_then(|module| module.name.as_deref())
    }

    fn render(&self, analysis: &ProjectAnalysis) -> String {
        let visibility = Self::visibility_prefix(self.binding_visibility);

        match self.def {
            DefId::Module(module_ref) => format!(
                "{visibility}module {}",
                self.render_module_path(analysis, module_ref)
            ),
            DefId::Local(local_def_ref) => {
                let local_def = self
                    .target
                    .def_map
                    .local_defs
                    .get(local_def_ref.local_def.0)
                    .expect("local def id should exist while dumping");
                let module_path = self.render_module_path(
                    analysis,
                    crate::parse::def_map::ModuleRef {
                        target: local_def_ref.target,
                        module: local_def.module,
                    },
                );

                format!(
                    "{visibility}{} {}::{}",
                    local_def.kind, module_path, local_def.name
                )
            }
        }
    }

    fn render_module_path(
        &self,
        analysis: &ProjectAnalysis,
        module_ref: crate::parse::def_map::ModuleRef,
    ) -> String {
        let package = analysis
            .packages()
            .get(module_ref.target.package.0)
            .expect("package slot should exist while dumping");
        let target = package
            .target(module_ref.target.target)
            .expect("target id should exist while dumping");

        format!(
            "{}[{}]::{}",
            package.package_name(),
            target_kind_label(target),
            Self::relative_module_path(target, module_ref.module),
        )
    }

    fn relative_module_path(target: &TargetIndex, module_id: ModuleId) -> String {
        let module = target
            .def_map
            .module(module_id)
            .expect("module id should exist while building relative module path");

        match module.parent {
            Some(parent) => {
                let parent_path = Self::relative_module_path(target, parent);
                let name = module
                    .name
                    .as_deref()
                    .expect("non-root modules should have names");
                format!("{parent_path}::{name}")
            }
            None => "crate".to_string(),
        }
    }

    fn visibility_prefix(visibility: &VisibilityLevel) -> String {
        match visibility {
            VisibilityLevel::Private => String::new(),
            _ => format!("{visibility} "),
        }
    }
}

fn target_kind_label(target: &TargetIndex) -> &'static str {
    if target.cargo_target.is_kind(cargo_metadata::TargetKind::Lib) {
        "lib"
    } else if target.cargo_target.is_kind(cargo_metadata::TargetKind::Bin) {
        "bin"
    } else if target
        .cargo_target
        .is_kind(cargo_metadata::TargetKind::Example)
    {
        "example"
    } else if target
        .cargo_target
        .is_kind(cargo_metadata::TargetKind::Test)
    {
        "test"
    } else if target
        .cargo_target
        .is_kind(cargo_metadata::TargetKind::Bench)
    {
        "bench"
    } else if target
        .cargo_target
        .is_kind(cargo_metadata::TargetKind::CustomBuild)
    {
        "custom-build"
    } else {
        "unknown"
    }
}

fn target_kind_sort_order(target: &TargetIndex) -> u8 {
    if target.cargo_target.is_kind(cargo_metadata::TargetKind::Lib) {
        0
    } else if target.cargo_target.is_kind(cargo_metadata::TargetKind::Bin) {
        1
    } else if target
        .cargo_target
        .is_kind(cargo_metadata::TargetKind::Example)
    {
        2
    } else if target
        .cargo_target
        .is_kind(cargo_metadata::TargetKind::Test)
    {
        3
    } else if target
        .cargo_target
        .is_kind(cargo_metadata::TargetKind::Bench)
    {
        4
    } else if target
        .cargo_target
        .is_kind(cargo_metadata::TargetKind::CustomBuild)
    {
        5
    } else {
        6
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
    ($fixture:expr $(,)?) => {{ $crate::test_fixture::CrateFixture::from_fixture_spec($fixture) }};
}

pub(crate) use fixture_crate;
