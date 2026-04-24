//! Finalizes per-target def maps after the raw module/items/imports have already been collected.
//!
//! This module is responsible for two things:
//! 1. Building the implicit root names each target can start resolution from
//!    (workspace library roots and dependency library roots).
//! 2. Running a fixed-point import-resolution loop until module scopes stop changing.

use std::collections::HashMap;

use anyhow::Context as _;

use crate::{
    item_tree::ItemTreeDb,
    parse::{self, package::Package},
    workspace_metadata::WorkspaceMetadata,
};

use super::{
    DefMapDb, ImportData, ImportId, ModuleId, ModuleRef, ModuleScope, PackageSlot, ScopeBinding,
    TargetRef,
    collect::{TargetState, collect_target_states},
    path_resolution::{
        namespace_for_def, resolve_path_to_defs, resolve_path_to_modules,
        visible_module_scope_entry_set,
    },
};

/// Builds the final `DefMapDb` from collected per-target states.
///
/// `collect_target_states` gives us module trees, local definitions, imports, and the initial
/// module scopes that contain only directly declared names. This phase adds the implicit
/// cross-target roots and repeatedly applies imports until the scopes stabilize.
pub(crate) fn build_db(
    workspace: &WorkspaceMetadata,
    parse: &parse::ParseDb,
    item_tree: &ItemTreeDb,
) -> anyhow::Result<DefMapDb> {
    let implicit_roots = build_implicit_roots(workspace, parse.packages())
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

/// Builds the per-target root-name map used as the first step of cross-target resolution.
///
/// Return shape:
/// - outer `Vec`: package slot
/// - inner `Vec`: target slot within that package
/// - `HashMap`: textual root name -> referenced root module
///
/// Within the workspace, non-library targets implicitly see their own package's library target.
/// All targets also see dependency library targets by dependency name.
fn build_implicit_roots(
    workspace: &WorkspaceMetadata,
    packages: &[Package],
) -> anyhow::Result<Vec<Vec<HashMap<String, ModuleRef>>>> {
    let lib_targets = packages
        .iter()
        .enumerate()
        .filter_map(|(package_slot, package)| {
            package
                .targets
                .iter()
                .find(|target| target.kind.is_lib())
                .map(|target| {
                    (
                        package.id().clone(),
                        TargetRef {
                            package: PackageSlot(package_slot),
                            target: target.id,
                        },
                    )
                })
        })
        .collect::<HashMap<_, _>>();
    let mut roots = Vec::with_capacity(packages.len());

    for package in packages {
        let mut package_roots = Vec::with_capacity(package.targets.len());
        let workspace_package = workspace.package(package.id()).with_context(|| {
            format!(
                "while attempting to fetch workspace metadata for package {}",
                package.id()
            )
        })?;

        for target in &package.targets {
            let mut target_roots = HashMap::new();

            // A workspace binary/example/test target can refer to its sibling library target by
            // the package library name without going through Cargo metadata dependencies.
            if let Some(&lib_target) = lib_targets.get(package.id()) {
                if lib_target.target != target.id {
                    let lib_name = package
                        .targets
                        .get(lib_target.target.0)
                        .expect("library target should exist")
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

            for dependency in &workspace_package.dependencies {
                // Step 1 deliberately ignores build-only dependencies in def-map resolution.
                if dependency.name.is_empty() || dependency.is_build_only {
                    continue;
                }

                let Some(&lib_target) = lib_targets.get(&dependency.package) else {
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

            package_roots.push(target_roots);
        }

        roots.push(package_roots);
    }

    Ok(roots)
}

/// Resolves imports until every target scope stops changing.
///
/// Imports can depend on names introduced by other imports, so one pass is not enough.
/// We therefore keep two copies of the scopes:
/// - `current_scopes`: the last fully computed state
/// - `next_scopes`: the state being produced by this iteration
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

        // Every iteration starts from the directly declared names, then layers import-derived
        // bindings on top of that snapshot.
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
            // Once the import graph reaches a fixed point, freeze the resolved scopes into the
            // public def-map payload and preserve unresolved imports for query consumers.
            let unresolved_imports = collect_unresolved_imports(states, &current_scopes);

            for package_states in states.iter_mut() {
                for state in package_states {
                    let final_scopes = current_scopes
                        .get(state.target.package.0)
                        .and_then(|package_scopes| package_scopes.get(state.target.target.0))
                        .expect("final scopes should exist for every target");
                    let final_unresolved_imports = unresolved_imports
                        .get(state.target.package.0)
                        .and_then(|package_imports| package_imports.get(state.target.target.0))
                        .expect("unresolved imports should exist for every target");

                    for (module_id, scope) in final_scopes.iter().enumerate() {
                        let module = state
                            .def_map
                            .modules
                            .get_mut(module_id)
                            .expect("module should exist for every final scope");
                        module.scope = scope.clone();
                        module.unresolved_imports = final_unresolved_imports
                            .get(module_id)
                            .expect("unresolved imports should exist for every module")
                            .clone();
                    }
                }
            }

            return Ok(());
        }

        current_scopes = next_scopes;
    }
}

/// Computes imports that still have no resolution after the fixed-point loop has stabilized.
fn collect_unresolved_imports(
    states: &[Vec<TargetState>],
    final_scopes: &[Vec<Vec<ModuleScope>>],
) -> Vec<Vec<Vec<Vec<ImportId>>>> {
    states
        .iter()
        .map(|package_states| {
            package_states
                .iter()
                .map(|state| {
                    let mut module_imports = vec![Vec::new(); state.def_map.modules.len()];

                    for (import_idx, import) in state.def_map.imports.iter().enumerate() {
                        if import_is_unresolved(state, states, final_scopes, import) {
                            module_imports
                                .get_mut(import.module.0)
                                .expect("import module should exist while collecting unresolved imports")
                                .push(ImportId(import_idx));
                        }
                    }

                    module_imports
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Checks whether one import failed to resolve, independent of whether it introduces a binding.
fn import_is_unresolved(
    state: &TargetState,
    states: &[Vec<TargetState>],
    final_scopes: &[Vec<Vec<ModuleScope>>],
    import: &ImportData,
) -> bool {
    match import.kind {
        super::ImportKind::Glob => resolve_path_to_modules(
            states,
            final_scopes,
            state.target,
            import.module,
            &import.path,
        )
        .is_empty(),
        super::ImportKind::Named | super::ImportKind::SelfImport => resolve_path_to_defs(
            states,
            final_scopes,
            state.target,
            import.module,
            &import.path,
        )
        .is_empty(),
    }
}

/// Applies one target's imports using the previously computed scope snapshot.
///
/// Named/self imports add a binding under one textual name. Glob imports copy every visible
/// binding from the source module into the target module.
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
                    let source_scope =
                        visible_module_scope_entry_set(current_scopes, state.target, source_module);
                    let target_scope = next_scopes
                        .get_mut(state.target.package.0)
                        .and_then(|package_scopes| package_scopes.get_mut(state.target.target.0))
                        .and_then(|target_scopes| target_scopes.get_mut(import.module.0))
                        .expect("target scope should exist for every import");

                    // Visibility is attached to the binding introduced by the glob import, not to
                    // the original definition.
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
                    // Resolution is namespace-aware, but the target textual name is shared across
                    // namespaces inside one scope entry.
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
