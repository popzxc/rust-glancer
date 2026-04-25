use expect_test::Expect;

use crate::{
    Project,
    def_map::{
        DefId, DefMap, ImportData, ImportKind, ModuleId, ModuleRef, Path, PathSegment,
        ResolvePathResult, ScopeBinding, ScopeEntry, TargetRef,
    },
    item_tree::VisibilityLevel,
    parse::{Package, Target},
    test_utils::{fixture_crate, snapshot},
    workspace_metadata::TargetKind,
};

pub(super) fn check_project_def_map(fixture: &str, expect: Expect) {
    let project = fixture_crate(fixture).project();
    let actual = ProjectDefMapSnapshot::new(&project).render();
    let actual = format!("{}\n", actual.trim_end());
    expect.assert_eq(&actual);
}

pub(super) fn check_project_path_resolution(
    fixture: &str,
    queries: &[PathResolutionQuery],
    expect: Expect,
) {
    let project = fixture_crate(fixture).project();
    let actual = ProjectPathResolutionSnapshot::new(&project, queries).render();
    let actual = format!("{}\n", actual.trim_end());
    expect.assert_eq(&actual);
}

pub(super) struct PathResolutionQuery {
    package_name: &'static str,
    target_kind: TargetKind,
    module_path: &'static str,
    path: &'static str,
}

impl PathResolutionQuery {
    pub(super) fn lib(
        package_name: &'static str,
        module_path: &'static str,
        path: &'static str,
    ) -> Self {
        Self {
            package_name,
            target_kind: TargetKind::Lib,
            module_path,
            path,
        }
    }
}

/// Project-level DefMap snapshot context.
/// Renders package sections such as `package app`.
struct ProjectDefMapSnapshot<'a> {
    project: &'a Project,
}

impl<'a> ProjectDefMapSnapshot<'a> {
    fn new(project: &'a Project) -> Self {
        Self { project }
    }

    fn render(&self) -> String {
        let package_dumps = snapshot::sorted_packages(self.project)
            .into_iter()
            .map(|(package_slot, package)| {
                PackageDefMapSnapshot {
                    project: self.project,
                    package_slot,
                    package,
                }
                .render()
            })
            .collect::<Vec<_>>();

        package_dumps.join("\n\n")
    }
}

/// Project-level path-resolution snapshot context.
struct ProjectPathResolutionSnapshot<'a> {
    project: &'a Project,
    queries: &'a [PathResolutionQuery],
}

impl<'a> ProjectPathResolutionSnapshot<'a> {
    fn new(project: &'a Project, queries: &'a [PathResolutionQuery]) -> Self {
        Self { project, queries }
    }

    fn render(&self) -> String {
        self.queries
            .iter()
            .map(|query| self.render_query(query))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn render_query(&self, query: &PathResolutionQuery) -> String {
        let (target_ref, target) = self.target_ref(query);
        let module_id = self.module_id(target_ref, query.module_path);
        let path = Self::parse_path(query.path);
        let result = self.project.def_map_db().resolve_path(
            ModuleRef {
                target: target_ref,
                module: module_id,
            },
            &path,
        );

        format!(
            "{} [{}] {} resolves {} -> {}",
            query.package_name,
            target.kind,
            query.module_path,
            path,
            self.render_result(&result),
        )
    }

    fn target_ref(&self, query: &PathResolutionQuery) -> (TargetRef, &'a Target) {
        let (package_slot, package) = self
            .project
            .parse_db()
            .packages()
            .iter()
            .enumerate()
            .find(|(_, package)| package.package_name() == query.package_name)
            .unwrap_or_else(|| panic!("fixture package `{}` should exist", query.package_name));
        let target = package
            .targets()
            .iter()
            .find(|target| target.kind == query.target_kind)
            .unwrap_or_else(|| {
                panic!(
                    "fixture package `{}` should have a {} target",
                    query.package_name, query.target_kind
                )
            });

        (
            TargetRef {
                package: crate::def_map::PackageSlot(package_slot),
                target: target.id,
            },
            target,
        )
    }

    fn module_id(&self, target_ref: TargetRef, module_path: &str) -> ModuleId {
        let def_map = self
            .project
            .def_map_db()
            .def_map(target_ref)
            .expect("target def map should exist while resolving path snapshot query");

        def_map
            .modules()
            .iter()
            .enumerate()
            .find_map(|(module_idx, _)| {
                let module_id = ModuleId(module_idx);
                (self.module_path(target_ref, module_id) == module_path).then_some(module_id)
            })
            .unwrap_or_else(|| panic!("module `{module_path}` should exist in fixture target"))
    }

    fn module_path(&self, target_ref: TargetRef, module_id: ModuleId) -> String {
        let module = self
            .project
            .def_map_db()
            .def_map(target_ref)
            .expect("target def map should exist while building module path")
            .module(module_id)
            .expect("module id should exist while building module path");

        match module.parent {
            Some(parent) => {
                let parent_path = self.module_path(target_ref, parent);
                let name = module
                    .name
                    .as_deref()
                    .expect("non-root modules should have names");
                format!("{parent_path}::{name}")
            }
            None => "crate".to_string(),
        }
    }

    fn parse_path(text: &str) -> Path {
        let (absolute, text) = match text.strip_prefix("::") {
            Some(stripped) => (true, stripped),
            None => (false, text),
        };
        let segments = text
            .split("::")
            .filter(|segment| !segment.is_empty())
            .map(|segment| match segment {
                "self" => PathSegment::SelfKw,
                "super" => PathSegment::SuperKw,
                "crate" => PathSegment::CrateKw,
                name => PathSegment::Name(name.to_string()),
            })
            .collect::<Vec<_>>();

        Path { absolute, segments }
    }

    fn render_result(&self, result: &ResolvePathResult) -> String {
        let mut resolved = result
            .resolved
            .iter()
            .map(|def| {
                ResolvedDefOrigin {
                    project: self.project,
                    def: *def,
                }
                .render()
            })
            .collect::<Vec<_>>();
        resolved.sort();

        let mut rendered = if resolved.is_empty() {
            "<none>".to_string()
        } else {
            resolved.join("; ")
        };

        if let Some(unresolved_at) = result.unresolved_at {
            rendered.push_str(&format!(" (unresolved at segment #{unresolved_at})"));
        }

        rendered
    }
}

/// Package-level DefMap snapshot context.
/// Renders target sections such as `app [lib]`.
struct PackageDefMapSnapshot<'a> {
    project: &'a Project,
    package_slot: usize,
    package: &'a Package,
}

impl<'a> PackageDefMapSnapshot<'a> {
    fn render(&self) -> String {
        let target_dumps = snapshot::sorted_targets(self.package)
            .into_iter()
            .map(|target| {
                let target_ref = TargetRef {
                    package: crate::def_map::PackageSlot(self.package_slot),
                    target: target.id,
                };
                TargetDefMapSnapshot {
                    project: self.project,
                    package: self.package,
                    target,
                    target_ref,
                }
                .render()
                .trim_end()
                .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        format!("package {}\n\n{target_dumps}", self.package.package_name())
    }
}

/// Target-level DefMap snapshot context with access to resolved module paths.
/// Renders module scopes such as `crate::nested`.
struct TargetDefMapSnapshot<'a> {
    project: &'a Project,
    package: &'a Package,
    target: &'a Target,
    target_ref: TargetRef,
}

impl<'a> TargetDefMapSnapshot<'a> {
    fn render(&self) -> String {
        let def_map = self.def_map();
        let mut dump = format!("{} [{}]\n", self.package.package_name(), self.target.kind);

        for (idx, (module_path, module_id)) in self.sorted_modules().into_iter().enumerate() {
            if idx > 0 {
                dump.push('\n');
            }

            dump.push_str(&module_path);
            dump.push('\n');

            let module = def_map
                .module(module_id)
                .expect("module id should exist in def map dump");

            for name in self.sorted_scope_names(&module.scope) {
                let entry = module
                    .scope
                    .entry(&name)
                    .expect("scope entry should exist while dumping");
                dump.push_str(&format!("- {name} : {}\n", self.render_scope_entry(entry)));
            }

            if !module.unresolved_imports.is_empty() {
                dump.push_str("unresolved imports\n");

                for import_id in &module.unresolved_imports {
                    let import = def_map
                        .imports
                        .get(import_id.0)
                        .expect("unresolved import id should exist while dumping");
                    dump.push_str(&format!("- {}\n", self.render_unresolved_import(import)));
                }
            }

            if !module.impls.is_empty() {
                dump.push_str("impls\n");

                for impl_id in &module.impls {
                    let local_impl = def_map
                        .local_impls()
                        .get(impl_id.0)
                        .expect("local impl id should exist while dumping");
                    dump.push_str(&format!(
                        "- impl {}\n",
                        self.render_item_tree_ref(local_impl.source)
                    ));
                }
            }
        }

        dump
    }

    fn def_map(&self) -> &'a DefMap {
        self.project
            .def_map_db()
            .def_map(self.target_ref)
            .expect("target def map should exist while rendering snapshot")
    }

    fn sorted_modules(&self) -> Vec<(String, ModuleId)> {
        let mut modules = self
            .def_map()
            .modules
            .iter()
            .enumerate()
            .map(|(idx, _)| {
                let module_id = ModuleId(idx);
                (self.module_path(self.target_ref, module_id), module_id)
            })
            .collect::<Vec<_>>();
        modules.sort_by(|left, right| left.0.cmp(&right.0));
        modules
    }

    fn sorted_scope_names(&self, scope: &crate::def_map::ModuleScope) -> Vec<String> {
        let mut names = scope.names.keys().cloned().collect::<Vec<_>>();
        names.sort();
        names
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
            .map(|origin| origin.render())
            .collect::<Vec<_>>();
        rendered.sort();
        rendered.join("; ")
    }

    fn binding_origin(&self, binding: &'a ScopeBinding) -> Option<BindingOrigin<'a>> {
        let target_ref = match binding.def {
            DefId::Module(module_ref) => module_ref.target,
            DefId::Local(local_def_ref) => local_def_ref.target,
        };
        self.project
            .parse_db()
            .packages()
            .get(target_ref.package.0)?;
        self.project.def_map_db().def_map(target_ref)?;

        Some(BindingOrigin {
            project: self.project,
            def: binding.def,
            binding_visibility: &binding.visibility,
        })
    }

    fn render_unresolved_import(&self, import: &ImportData) -> String {
        let visibility = match &import.visibility {
            VisibilityLevel::Private => String::new(),
            visibility => format!("{visibility} "),
        };
        let path = match import.kind {
            ImportKind::Glob => format!("{}::*", import.path),
            ImportKind::Named | ImportKind::SelfImport => import.path.to_string(),
        };

        format!("{visibility}use {path}{}", import.binding)
    }

    fn render_item_tree_ref(&self, item_ref: crate::item_tree::ItemTreeRef) -> String {
        let file_label = snapshot::file_label(self.package, item_ref.file_id);
        format!("{file_label}#{}", item_ref.item.0)
    }

    fn module_path(&self, target_ref: TargetRef, module_id: ModuleId) -> String {
        let module = self
            .project
            .def_map_db()
            .def_map(target_ref)
            .expect("target def map should exist while building relative module path")
            .module(module_id)
            .expect("module id should exist while building relative module path");

        match module.parent {
            Some(parent) => {
                let parent_path = self.module_path(target_ref, parent);
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

/// Snapshot-only view of where a resolved scope binding came from.
/// Renders origins such as `pub fn app[lib]::crate::make`.
struct BindingOrigin<'a> {
    project: &'a Project,
    def: DefId,
    binding_visibility: &'a VisibilityLevel,
}

impl BindingOrigin<'_> {
    fn render(&self) -> String {
        let visibility = Self::visibility_prefix(self.binding_visibility);
        let origin = ResolvedDefOrigin {
            project: self.project,
            def: self.def,
        }
        .render();

        format!("{visibility}{origin}")
    }

    fn visibility_prefix(visibility: &VisibilityLevel) -> String {
        match visibility {
            VisibilityLevel::Private => String::new(),
            _ => format!("{visibility} "),
        }
    }
}

/// Snapshot-only view of one resolved definition.
struct ResolvedDefOrigin<'a> {
    project: &'a Project,
    def: DefId,
}

impl ResolvedDefOrigin<'_> {
    fn render(&self) -> String {
        match self.def {
            DefId::Module(module_ref) => {
                format!("module {}", self.render_module_path(module_ref))
            }
            DefId::Local(local_def_ref) => {
                let local_def = self
                    .project
                    .def_map_db()
                    .def_map(local_def_ref.target)
                    .expect("target def map should exist while dumping")
                    .local_defs
                    .get(local_def_ref.local_def.0)
                    .expect("local def id should exist while dumping");
                let module_path = self.render_module_path(crate::def_map::ModuleRef {
                    target: local_def_ref.target,
                    module: local_def.module,
                });

                format!("{} {}::{}", local_def.kind, module_path, local_def.name)
            }
        }
    }

    fn render_module_path(&self, module_ref: crate::def_map::ModuleRef) -> String {
        let package = self
            .project
            .parse_db()
            .packages()
            .get(module_ref.target.package.0)
            .expect("package slot should exist while dumping");
        let target = package
            .target(module_ref.target.target)
            .expect("target id should exist while dumping");

        format!(
            "{}[{}]::{}",
            package.package_name(),
            target.kind,
            self.module_path(module_ref.target, module_ref.module),
        )
    }

    fn module_path(&self, target_ref: TargetRef, module_id: ModuleId) -> String {
        let module = self
            .project
            .def_map_db()
            .def_map(target_ref)
            .expect("target def map should exist while building relative module path")
            .module(module_id)
            .expect("module id should exist while building relative module path");

        match module.parent {
            Some(parent) => {
                let parent_path = self.module_path(target_ref, parent);
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
