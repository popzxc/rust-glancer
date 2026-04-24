use expect_test::Expect;

use crate::{
    Project,
    def_map::{DefId, ModuleId, ScopeBinding, ScopeEntry, TargetRef},
    item_tree::VisibilityLevel,
    parse::{Package, Target},
    test_utils::fixture_crate,
};

pub(super) fn check_project_def_map(fixture: &str, expect: Expect) {
    let actual = render_project_def_map(&fixture_crate!(fixture).project());
    let actual = format!("{}\n", actual.trim_end());
    expect.assert_eq(&actual);
}

fn render_project_def_map(project: &Project) -> String {
    let mut packages = project.packages().iter().enumerate().collect::<Vec<_>>();
    packages.sort_by(|left, right| left.1.package_name().cmp(right.1.package_name()));

    let package_dumps = packages
        .into_iter()
        .map(|(package_slot, package)| {
            let mut targets = package.targets().iter().collect::<Vec<_>>();
            targets.sort_by(|left, right| {
                (
                    left.kind.sort_order(),
                    left.name.as_str(),
                    left.src_path.as_path(),
                )
                    .cmp(&(
                        right.kind.sort_order(),
                        right.name.as_str(),
                        right.src_path.as_path(),
                    ))
            });

            let target_dumps = targets
                .into_iter()
                .map(|target| {
                    render_target_def_map(
                        project,
                        package,
                        target,
                        TargetRef {
                            package: crate::def_map::PackageSlot(package_slot),
                            target: target.id,
                        },
                    )
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

fn render_target_def_map(
    project: &Project,
    package: &Package,
    target: &Target,
    target_ref: TargetRef,
) -> String {
    let def_map = project
        .def_map(target_ref)
        .expect("target def map should exist while rendering snapshot");
    let mut modules = def_map
        .modules
        .iter()
        .enumerate()
        .map(|(idx, _)| {
            let module_id = ModuleId(idx);
            (module_path(project, target_ref, module_id), module_id)
        })
        .collect::<Vec<_>>();
    modules.sort_by(|left, right| left.0.cmp(&right.0));

    let mut dump = String::new();
    std::fmt::Write::write_fmt(
        &mut dump,
        format_args!("{} [{}]\n", package.package_name(), target.kind),
    )
    .expect("string writes should not fail");

    for (idx, (module_path, module_id)) in modules.into_iter().enumerate() {
        if idx > 0 {
            dump.push('\n');
        }

        std::fmt::Write::write_fmt(&mut dump, format_args!("{module_path}\n"))
            .expect("string writes should not fail");

        let module = def_map
            .module(module_id)
            .expect("module id should exist in def map dump");
        let mut names = module.scope.names.keys().cloned().collect::<Vec<_>>();
        names.sort();

        for name in names {
            let entry = module
                .scope
                .entry(&name)
                .expect("scope entry should exist while dumping");
            std::fmt::Write::write_fmt(
                &mut dump,
                format_args!("- {name} : {}\n", render_scope_entry(project, entry)),
            )
            .expect("string writes should not fail");
        }
    }

    dump
}

fn render_scope_entry(project: &Project, entry: &ScopeEntry) -> String {
    let mut parts = Vec::new();

    if !entry.types.is_empty() {
        parts.push(format!(
            "type [{}]",
            render_namespace_bindings(project, &entry.types)
        ));
    }

    if !entry.values.is_empty() {
        parts.push(format!(
            "value [{}]",
            render_namespace_bindings(project, &entry.values)
        ));
    }

    if !entry.macros.is_empty() {
        parts.push(format!(
            "macro [{}]",
            render_namespace_bindings(project, &entry.macros)
        ));
    }

    parts.join(" | ")
}

fn render_namespace_bindings(project: &Project, bindings: &[ScopeBinding]) -> String {
    let mut rendered = bindings
        .iter()
        .filter_map(|binding| binding_origin(project, binding))
        .map(|origin| origin.render())
        .collect::<Vec<_>>();
    rendered.sort();
    rendered.join("; ")
}

fn binding_origin<'a>(
    project: &'a Project,
    binding: &'a ScopeBinding,
) -> Option<BindingOrigin<'a>> {
    let target_ref = match binding.def {
        DefId::Module(module_ref) => module_ref.target,
        DefId::Local(local_def_ref) => local_def_ref.target,
    };
    project.packages().get(target_ref.package.0)?;
    project.def_map(target_ref)?;

    Some(BindingOrigin {
        project,
        def: binding.def,
        binding_visibility: &binding.visibility,
    })
}

struct BindingOrigin<'a> {
    project: &'a Project,
    def: DefId,
    binding_visibility: &'a VisibilityLevel,
}

impl<'a> BindingOrigin<'a> {
    fn render(&self) -> String {
        let visibility = Self::visibility_prefix(self.binding_visibility);

        match self.def {
            DefId::Module(module_ref) => {
                format!("{visibility}module {}", self.render_module_path(module_ref))
            }
            DefId::Local(local_def_ref) => {
                let local_def = self
                    .project
                    .def_map(local_def_ref.target)
                    .expect("target def map should exist while dumping")
                    .local_defs
                    .get(local_def_ref.local_def.0)
                    .expect("local def id should exist while dumping");
                let module_path = self.render_module_path(crate::def_map::ModuleRef {
                    target: local_def_ref.target,
                    module: local_def.module,
                });

                format!(
                    "{visibility}{} {}::{}",
                    local_def.kind, module_path, local_def.name
                )
            }
        }
    }

    fn render_module_path(&self, module_ref: crate::def_map::ModuleRef) -> String {
        let package = self
            .project
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
            module_path(self.project, module_ref.target, module_ref.module),
        )
    }

    fn visibility_prefix(visibility: &VisibilityLevel) -> String {
        match visibility {
            VisibilityLevel::Private => String::new(),
            _ => format!("{visibility} "),
        }
    }
}

fn module_path(project: &Project, target_ref: TargetRef, module_id: ModuleId) -> String {
    let module = project
        .def_map(target_ref)
        .expect("target def map should exist while building relative module path")
        .module(module_id)
        .expect("module id should exist while building relative module path");

    match module.parent {
        Some(parent) => {
            let parent_path = module_path(project, target_ref, parent);
            let name = module
                .name
                .as_deref()
                .expect("non-root modules should have names");
            format!("{parent_path}::{name}")
        }
        None => "crate".to_string(),
    }
}
