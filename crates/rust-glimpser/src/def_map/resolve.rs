use std::collections::HashMap;

use anyhow::Context as _;

use crate::{
    item_tree::{ItemTreeDb, VisibilityLevel},
    parse::{self, package::Package},
};

use super::{
    DefId, DefMapDb, ModuleId, ModuleRef, ModuleScope, PackageSlot, ScopeBinding, ScopeEntry,
    TargetRef,
    collect::{TargetState, collect_target_states},
    data::Namespace,
};

pub(crate) fn build_db(parse: &parse::ParseDb, item_tree: &ItemTreeDb) -> anyhow::Result<DefMapDb> {
    let implicit_roots =
        build_implicit_roots(parse.metadata(), parse.packages(), parse.package_by_id())
            .context("while attempting to build implicit target roots")?;

    let mut target_states = collect_target_states(parse.packages(), item_tree, &implicit_roots)
        .context("while attempting to collect target definitions and imports")?;

    finalize_scopes(&mut target_states).context("while attempting to resolve target scopes")?;

    let packages = target_states
        .into_iter()
        .map(|package_states| super::Package {
            targets: package_states
                .into_iter()
                .map(|state| state.def_map)
                .collect::<Vec<_>>(),
        })
        .collect::<Vec<_>>();

    Ok(DefMapDb { packages })
}

fn build_implicit_roots(
    metadata: &cargo_metadata::Metadata,
    packages: &[Package],
    package_by_id: &HashMap<cargo_metadata::PackageId, usize>,
) -> anyhow::Result<Vec<Vec<HashMap<String, ModuleRef>>>> {
    let lib_targets = packages
        .iter()
        .enumerate()
        .map(|(package_slot, package)| {
            package
                .targets
                .iter()
                .find(|target| target.cargo_target.is_kind(cargo_metadata::TargetKind::Lib))
                .map(|target| TargetRef {
                    package: PackageSlot(package_slot),
                    target: target.id,
                })
        })
        .collect::<Vec<_>>();

    let resolve = metadata.resolve.as_ref();
    let mut roots = Vec::with_capacity(packages.len());

    for (package_slot, package) in packages.iter().enumerate() {
        let mut package_roots = Vec::with_capacity(package.targets.len());
        let package_node = resolve.map(|resolve| &resolve[package.id()]);

        for target in &package.targets {
            let mut target_roots = HashMap::new();

            if let Some(lib_target) = lib_targets[package_slot] {
                if lib_target.target != target.id {
                    let lib_name = package
                        .targets
                        .get(lib_target.target.0)
                        .expect("library target should exist")
                        .cargo_target
                        .name
                        .clone();
                    target_roots.insert(
                        lib_name,
                        ModuleRef {
                            target: lib_target,
                            module: ModuleId(0),
                        },
                    );
                }
            }

            if let Some(package_node) = package_node {
                for dependency in &package_node.deps {
                    if dependency.name.is_empty() || dependency_is_build_only(dependency) {
                        continue;
                    }

                    let Some(&dependency_slot) = package_by_id.get(&dependency.pkg) else {
                        continue;
                    };
                    let Some(lib_target) = lib_targets[dependency_slot] else {
                        continue;
                    };

                    target_roots.insert(
                        dependency.name.clone(),
                        ModuleRef {
                            target: lib_target,
                            module: ModuleId(0),
                        },
                    );
                }
            }

            package_roots.push(target_roots);
        }

        roots.push(package_roots);
    }

    Ok(roots)
}

fn dependency_is_build_only(dependency: &cargo_metadata::NodeDep) -> bool {
    !dependency.dep_kinds.is_empty()
        && dependency
            .dep_kinds
            .iter()
            .all(|kind| kind.kind == cargo_metadata::DependencyKind::Build)
}

fn finalize_scopes(states: &mut [Vec<TargetState>]) -> anyhow::Result<()> {
    let mut current_scopes = states
        .iter()
        .map(|package_states| {
            package_states
                .iter()
                .map(|state| state.base_scopes.clone())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    loop {
        let mut next_scopes = states
            .iter()
            .map(|package_states| {
                package_states
                    .iter()
                    .map(|state| state.base_scopes.clone())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        for package_states in states.iter() {
            for state in package_states {
                apply_imports(state, states, &current_scopes, &mut next_scopes).with_context(
                    || {
                        format!(
                            "while attempting to resolve imports for {}",
                            state.target_name
                        )
                    },
                )?;
            }
        }

        if next_scopes == current_scopes {
            for package_states in states.iter_mut() {
                for state in package_states {
                    let final_scopes = current_scopes
                        .get(state.target.package.0)
                        .and_then(|package_scopes| package_scopes.get(state.target.target.0))
                        .expect("final scopes should exist for every target");

                    for (module_id, scope) in final_scopes.iter().enumerate() {
                        state
                            .def_map
                            .modules
                            .get_mut(module_id)
                            .expect("module should exist for every final scope")
                            .scope = scope.clone();
                    }
                }
            }

            return Ok(());
        }

        current_scopes = next_scopes;
    }
}

fn apply_imports(
    state: &TargetState,
    states: &[Vec<TargetState>],
    current_scopes: &[Vec<Vec<ModuleScope>>],
    next_scopes: &mut [Vec<Vec<ModuleScope>>],
) -> anyhow::Result<()> {
    for import in &state.def_map.imports {
        match import.kind {
            super::ImportKind::Glob => {
                let source_modules = resolve_path_to_modules(
                    states,
                    current_scopes,
                    state.target,
                    import.module,
                    &import.path,
                );

                for source_module in source_modules {
                    let source_scope = visible_module_scope_entry_set(
                        states,
                        current_scopes,
                        state.target,
                        source_module,
                    );
                    let target_scope = next_scopes
                        .get_mut(state.target.package.0)
                        .and_then(|package_scopes| package_scopes.get_mut(state.target.target.0))
                        .and_then(|target_scopes| target_scopes.get_mut(import.module.0))
                        .expect("target scope should exist for every import");

                    for (name, entry) in source_scope {
                        target_scope.copy_visible_bindings(
                            &name,
                            &entry,
                            import.visibility.clone(),
                        );
                    }
                }
            }
            super::ImportKind::Named | super::ImportKind::SelfImport => {
                let resolved_defs = resolve_path_to_defs(
                    states,
                    current_scopes,
                    state.target,
                    import.module,
                    &import.path,
                );

                let Some(binding_name) = import.binding_name() else {
                    continue;
                };
                let target_scope = next_scopes
                    .get_mut(state.target.package.0)
                    .and_then(|package_scopes| package_scopes.get_mut(state.target.target.0))
                    .and_then(|target_scopes| target_scopes.get_mut(import.module.0))
                    .expect("target scope should exist for every import");

                for resolved_def in resolved_defs {
                    let Some(namespace) = namespace_for_def(states, resolved_def) else {
                        continue;
                    };
                    target_scope.insert_binding(
                        &binding_name,
                        namespace,
                        ScopeBinding {
                            def: resolved_def,
                            visibility: import.visibility.clone(),
                        },
                    );
                }
            }
        }
    }

    Ok(())
}

fn visible_module_scope_entry_set(
    states: &[Vec<TargetState>],
    current_scopes: &[Vec<Vec<ModuleScope>>],
    importing_target: TargetRef,
    source_module: ModuleRef,
) -> HashMap<String, ScopeEntry> {
    let Some(module_scope) = current_scopes
        .get(source_module.target.package.0)
        .and_then(|package_scopes| package_scopes.get(source_module.target.target.0))
        .and_then(|target_scopes| target_scopes.get(source_module.module.0))
    else {
        return HashMap::new();
    };

    let mut names = HashMap::new();

    for (name, entry) in &module_scope.names {
        let mut visible_entry = ScopeEntry::default();

        for binding in &entry.types {
            if binding_is_visible(importing_target, source_module.target, &binding.visibility) {
                visible_entry.insert_binding(Namespace::Types, binding.clone());
            }
        }

        for binding in &entry.values {
            if binding_is_visible(importing_target, source_module.target, &binding.visibility) {
                visible_entry.insert_binding(Namespace::Values, binding.clone());
            }
        }

        for binding in &entry.macros {
            if binding_is_visible(importing_target, source_module.target, &binding.visibility) {
                visible_entry.insert_binding(Namespace::Macros, binding.clone());
            }
        }

        if !visible_entry.types.is_empty()
            || !visible_entry.values.is_empty()
            || !visible_entry.macros.is_empty()
        {
            names.insert(name.clone(), visible_entry);
        }
    }

    let _ = states;

    names
}

fn resolve_path_to_defs(
    states: &[Vec<TargetState>],
    current_scopes: &[Vec<Vec<ModuleScope>>],
    importing_target: TargetRef,
    importing_module: ModuleId,
    path: &super::ImportPath,
) -> Vec<DefId> {
    let Some((first_segment, remaining_segments)) = path.segments.split_first() else {
        return Vec::new();
    };

    let mut current_defs = resolve_first_segment(
        states,
        current_scopes,
        importing_target,
        importing_module,
        path.absolute,
        first_segment,
    );

    for segment in remaining_segments {
        current_defs = resolve_next_segment(
            states,
            current_scopes,
            importing_target,
            current_defs,
            segment,
        );
    }

    current_defs
}

fn resolve_path_to_modules(
    states: &[Vec<TargetState>],
    current_scopes: &[Vec<Vec<ModuleScope>>],
    importing_target: TargetRef,
    importing_module: ModuleId,
    path: &super::ImportPath,
) -> Vec<ModuleRef> {
    let resolved_defs = resolve_path_to_defs(
        states,
        current_scopes,
        importing_target,
        importing_module,
        path,
    );

    let mut modules = Vec::new();
    for resolved_def in resolved_defs {
        if let DefId::Module(module_ref) = resolved_def {
            if !modules.contains(&module_ref) {
                modules.push(module_ref);
            }
        }
    }

    modules
}

fn resolve_first_segment(
    states: &[Vec<TargetState>],
    current_scopes: &[Vec<Vec<ModuleScope>>],
    importing_target: TargetRef,
    importing_module: ModuleId,
    absolute: bool,
    segment: &super::PathSegment,
) -> Vec<DefId> {
    if absolute {
        return match segment {
            super::PathSegment::Name(name) => states
                .get(importing_target.package.0)
                .and_then(|package_states| package_states.get(importing_target.target.0))
                .and_then(|state| state.implicit_roots.get(name))
                .copied()
                .map(|module_ref| vec![DefId::Module(module_ref)])
                .unwrap_or_default(),
            super::PathSegment::SelfKw
            | super::PathSegment::SuperKw
            | super::PathSegment::CrateKw => Vec::new(),
        };
    }

    match segment {
        super::PathSegment::SelfKw => vec![DefId::Module(ModuleRef {
            target: importing_target,
            module: importing_module,
        })],
        super::PathSegment::SuperKw => parent_module(states, importing_target, importing_module)
            .map(DefId::Module)
            .into_iter()
            .collect(),
        super::PathSegment::CrateKw => root_module_ref(states, importing_target)
            .map(DefId::Module)
            .into_iter()
            .collect(),
        super::PathSegment::Name(name) => {
            let current_module_ref = ModuleRef {
                target: importing_target,
                module: importing_module,
            };
            let local_defs =
                resolve_name_in_module(current_scopes, importing_target, current_module_ref, name);
            if !local_defs.is_empty() {
                return local_defs;
            }

            states
                .get(importing_target.package.0)
                .and_then(|package_states| package_states.get(importing_target.target.0))
                .and_then(|state| state.implicit_roots.get(name))
                .copied()
                .map(|module_ref| vec![DefId::Module(module_ref)])
                .unwrap_or_default()
        }
    }
}

fn resolve_next_segment(
    states: &[Vec<TargetState>],
    current_scopes: &[Vec<Vec<ModuleScope>>],
    importing_target: TargetRef,
    current_defs: Vec<DefId>,
    segment: &super::PathSegment,
) -> Vec<DefId> {
    let mut next_defs = Vec::new();

    for current_def in current_defs {
        let DefId::Module(module_ref) = current_def else {
            continue;
        };

        match segment {
            super::PathSegment::SelfKw => {
                push_unique_def(&mut next_defs, DefId::Module(module_ref));
            }
            super::PathSegment::SuperKw => {
                if let Some(parent) = parent_module(states, module_ref.target, module_ref.module) {
                    push_unique_def(&mut next_defs, DefId::Module(parent));
                }
            }
            super::PathSegment::CrateKw => {
                if let Some(root) = root_module_ref(states, module_ref.target) {
                    push_unique_def(&mut next_defs, DefId::Module(root));
                }
            }
            super::PathSegment::Name(name) => {
                for resolved_def in
                    resolve_name_in_module(current_scopes, importing_target, module_ref, name)
                {
                    push_unique_def(&mut next_defs, resolved_def);
                }
            }
        }
    }

    next_defs
}

fn resolve_name_in_module(
    current_scopes: &[Vec<Vec<ModuleScope>>],
    importing_target: TargetRef,
    module_ref: ModuleRef,
    name: &str,
) -> Vec<DefId> {
    let Some(scope_entry) = current_scopes
        .get(module_ref.target.package.0)
        .and_then(|package_scopes| package_scopes.get(module_ref.target.target.0))
        .and_then(|target_scopes| target_scopes.get(module_ref.module.0))
        .and_then(|scope| scope.entry(name))
    else {
        return Vec::new();
    };

    let mut defs = Vec::new();

    for binding in &scope_entry.types {
        if binding_is_visible(importing_target, module_ref.target, &binding.visibility) {
            push_unique_def(&mut defs, binding.def);
        }
    }

    for binding in &scope_entry.values {
        if binding_is_visible(importing_target, module_ref.target, &binding.visibility) {
            push_unique_def(&mut defs, binding.def);
        }
    }

    for binding in &scope_entry.macros {
        if binding_is_visible(importing_target, module_ref.target, &binding.visibility) {
            push_unique_def(&mut defs, binding.def);
        }
    }

    defs
}

fn binding_is_visible(
    importing_target: TargetRef,
    defining_target: TargetRef,
    visibility: &VisibilityLevel,
) -> bool {
    // TODO: for now, to not deal with actual resolution, we overly simplify the model
    // so that items within a single target all see each other. It is a temporary
    // solution to iterate faster and get to a working prototype. To be refined
    // later.
    if importing_target == defining_target {
        return true;
    }

    matches!(visibility, VisibilityLevel::Public)
}

fn namespace_for_def(states: &[Vec<TargetState>], def: DefId) -> Option<Namespace> {
    match def {
        DefId::Module(_) => Some(Namespace::Types),
        DefId::Local(local_def_ref) => {
            let local_def = states
                .get(local_def_ref.target.package.0)?
                .get(local_def_ref.target.target.0)?
                .def_map
                .local_defs
                .get(local_def_ref.local_def.0)?;
            Some(local_def.kind.namespace())
        }
    }
}

fn root_module_ref(states: &[Vec<TargetState>], target: TargetRef) -> Option<ModuleRef> {
    Some(ModuleRef {
        target,
        module: states
            .get(target.package.0)?
            .get(target.target.0)?
            .def_map
            .root_module()?,
    })
}

fn parent_module(
    states: &[Vec<TargetState>],
    target: TargetRef,
    module_id: ModuleId,
) -> Option<ModuleRef> {
    let module = states
        .get(target.package.0)?
        .get(target.target.0)?
        .def_map
        .module(module_id)?;

    Some(ModuleRef {
        target,
        module: module.parent?,
    })
}

fn push_unique_def(defs: &mut Vec<DefId>, def: DefId) {
    if !defs.contains(&def) {
        defs.push(def);
    }
}
