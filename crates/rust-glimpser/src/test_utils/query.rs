use crate::{
    Project,
    def_map::{DefId, ScopeBinding, ScopeEntry, TargetRef},
    parse::{Package, Target},
};

/// Project-level test query API built on top of one analyzed fixture.
///
/// The goal is to let tests assert on visible behavior without reaching through raw slot ids
/// or the exact storage layout of the parser and def map internals.
pub(crate) struct FixtureProject {
    pub(crate) project: Project,
}

impl FixtureProject {
    /// Returns the library target for one package.
    pub(crate) fn lib(&self, package_name: &str) -> FixtureTarget<'_> {
        self.target(package_name, cargo_metadata::TargetKind::Lib)
    }

    fn target(
        &self,
        package_name: &str,
        expected_kind: cargo_metadata::TargetKind,
    ) -> FixtureTarget<'_> {
        let (package_slot, package) = self
            .project
            .packages()
            .iter()
            .enumerate()
            .find(|(_, package)| package.package_name() == package_name)
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
            project: &self.project,
            package,
            target,
            target_ref: TargetRef {
                package: crate::def_map::PackageSlot(package_slot),
                target: target.id,
            },
        }
    }
}

/// Target-scoped test query API.
///
/// `entry("name")` always means "lookup `name` in the root module scope of this target".
pub(crate) struct FixtureTarget<'a> {
    project: &'a Project,
    package: &'a Package,
    target: &'a Target,
    target_ref: TargetRef,
}

impl<'a> FixtureTarget<'a> {
    /// Looks up one textual name in the root module scope of this target.
    pub(crate) fn entry(&self, name: &str) -> FixtureEntry<'a> {
        let entry = self
            .def_map()
            .root_module()
            .and_then(|root_module| self.def_map().module(root_module))
            .and_then(|module| module.scope.entry(name));
        FixtureEntry {
            project: self.project,
            package_name: self.package.package_name(),
            target: self.target,
            name: name.to_string(),
            entry,
        }
    }

    fn def_map(&self) -> &'a crate::def_map::DefMap {
        self.project
            .def_map(self.target_ref)
            .expect("target def map should exist in fixture project")
    }
}

/// Root-scope entry assertion helper for one textual name.
pub(crate) struct FixtureEntry<'a> {
    project: &'a Project,
    package_name: &'a str,
    target: &'a Target,
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
        self.project.packages().get(target_ref.package.0)?;
        self.project.def_map(target_ref)?;

        Some(FixtureBindingOrigin {
            project: self.project,
            def: binding.def,
        })
    }
}

/// Project-relative view of one resolved binding origin.
struct FixtureBindingOrigin<'a> {
    project: &'a Project,
    def: DefId,
}

impl<'a> FixtureBindingOrigin<'a> {
    fn module_name(&self) -> Option<&str> {
        let DefId::Module(module_ref) = self.def else {
            return None;
        };

        self.project
            .def_map(module_ref.target)?
            .module(module_ref.module)
            .and_then(|module| module.name.as_deref())
    }
}
