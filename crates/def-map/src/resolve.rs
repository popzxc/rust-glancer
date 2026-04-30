//! Finalizes per-target def maps after the raw module/items/imports have already been collected.
//!
//! This module is responsible for two things:
//! 1. Building the implicit root names each target can start resolution from
//!    (workspace library roots and dependency library roots).
//! 2. Running a fixed-point import-resolution loop until module scopes stop changing.

use std::collections::HashMap;

use anyhow::Context as _;

use rg_item_tree::ItemTreeDb;
use rg_parse::{self, Package};
use rg_text::{Name, NameInterner};
use rg_workspace::WorkspaceMetadata;

use super::{
    DefMapDb, ImportData, ImportId, ImportPath, ModuleId, ModuleRef, PackageSlot, ScopeBinding,
    TargetRef,
    collect::{TargetState, collect_package_target_states, collect_target_states},
    path_resolution::{
        namespace_for_def, resolve_path_to_defs, resolve_path_to_modules,
        visible_module_scope_entry_set,
    },
    scope::ModuleScopeBuilder,
};

/// Builds the final `DefMapDb` from collected per-target states.
///
/// `collect_target_states` gives us module trees, local definitions, imports, and the initial
/// module scopes that contain only directly declared names. This phase adds the implicit
/// cross-target roots and repeatedly applies imports until the scopes stabilize.
pub fn build_db(
    workspace: &WorkspaceMetadata,
    parse: &rg_parse::ParseDb,
    item_tree: &ItemTreeDb,
    interner: &mut NameInterner,
) -> anyhow::Result<DefMapDb> {
    // First compute every implicit crate root from the complete package graph. These roots are
    // needed while collecting target states because extern prelude bindings can point across
    // packages and targets.
    let implicit_roots = build_implicit_roots(workspace, parse.packages(), interner)
        .context("while attempting to build implicit target roots")?;

    // A fresh build collects every target from item trees. At this point scopes contain only
    // directly declared names; imports and preludes are deliberately unresolved.
    let mut target_states = collect_target_states(parse.packages(), item_tree, &implicit_roots)
        .context("while attempting to collect target definitions and imports")?;

    let packages = finalize_and_freeze(workspace, parse.packages(), &mut target_states, interner)
        .context("while attempting to finish target states")?;

    Ok(DefMapDb {
        packages: rg_arena::Arena::from_vec(packages),
    })
}

/// Rebuilds selected package def maps against the previous frozen graph.
///
/// Fresh package states are collected for affected packages. Unaffected packages are represented
/// by their already-final scopes, so affected imports can still resolve through dependencies
/// without re-collecting or re-lowering the whole workspace.
pub fn rebuild_packages(
    old: &DefMapDb,
    workspace: &WorkspaceMetadata,
    parse: &rg_parse::ParseDb,
    item_tree: &ItemTreeDb,
    packages: &[PackageSlot],
    interner: &mut NameInterner,
) -> anyhow::Result<DefMapDb> {
    let packages = normalized_package_slots(packages);
    if packages.is_empty() {
        return Ok(old.clone());
    }

    // Implicit roots are still recomputed from metadata even for package-scoped source rebuilds,
    // because the rebuilt targets need the same cross-target root map shape as a clean build.
    let implicit_roots = build_implicit_roots(workspace, parse.packages(), interner)
        .context("while attempting to rebuild implicit target roots")?;

    // Unaffected packages become frozen target states: their final scopes are treated as base
    // scopes, and their imports are cleared so fixed-point import resolution will not reprocess
    // them. They remain visible as dependencies while affected packages are rebuilt.
    let mut target_states = frozen_target_states(old);

    // Replace only the affected packages with fresh target states collected from current item
    // trees. These states re-enter the normal prelude/import finalization pipeline below.
    for package_slot in &packages {
        let parse_package = parse.package(package_slot.0).with_context(|| {
            format!(
                "while attempting to fetch parsed package {}",
                package_slot.0
            )
        })?;
        let item_tree_package = item_tree.package(package_slot.0).with_context(|| {
            format!(
                "while attempting to fetch item-tree package {}",
                package_slot.0
            )
        })?;
        let package_states = collect_package_target_states(
            package_slot.0,
            parse_package,
            item_tree_package,
            &implicit_roots,
        )
        .with_context(|| {
            format!(
                "while attempting to rebuild target states for package {}",
                parse_package.package_name()
            )
        })?;

        let slot = target_states.get_mut(package_slot.0).with_context(|| {
            format!(
                "while attempting to replace target states for package {}",
                package_slot.0
            )
        })?;
        *slot = package_states;
    }

    let finalized_packages =
        finalize_and_freeze(workspace, parse.packages(), &mut target_states, interner)
            .context("while attempting to finish rebuilt target states")?;

    // Preserve the old snapshot shape and swap in only rebuilt package payloads. This keeps the DB
    // immutable from query consumers' point of view while avoiding a whole-workspace replacement.
    let mut next = old.clone();
    for package_slot in packages {
        let rebuilt = finalized_packages.get(package_slot.0).with_context(|| {
            format!(
                "while attempting to fetch rebuilt package {}",
                package_slot.0
            )
        })?;
        let package = next.packages.get_mut(package_slot).with_context(|| {
            format!(
                "while attempting to replace def-map package {}",
                package_slot.0
            )
        })?;
        *package = rebuilt.clone();
    }

    Ok(next)
}

/// Runs the common post-collection pipeline and converts mutable target states into frozen maps.
///
/// Target collection intentionally stops before two project-wide facts are known:
/// - which standard prelude module each target should use;
/// - what every import contributes after fixed-point resolution.
///
/// Both clean builds and package-scoped rebuilds must pass through this same finalization step so
/// query-time path resolution sees exactly the same frozen `DefMap` shape.
fn finalize_and_freeze(
    workspace: &WorkspaceMetadata,
    packages: &[Package],
    target_states: &mut [Vec<TargetState>],
    interner: &mut NameInterner,
) -> anyhow::Result<Vec<super::Package>> {
    // Prelude selection is separated from target collection because it resolves through the
    // package graph and the directly-declared module scopes, not through one target in isolation.
    select_preludes(workspace, packages, target_states, interner)
        .context("while attempting to select target preludes")?;

    // Imports can depend on each other across modules and targets. Resolve them to a fixed point
    // before freezing scopes into `ModuleData`.
    finalize_scopes(target_states).context("while attempting to resolve target scopes")?;

    Ok(target_states
        .iter()
        .zip(packages)
        .map(|(package_states, package)| super::Package {
            name: package.package_name().to_string(),
            target_names: rg_arena::Arena::from_vec(
                package_states
                    .iter()
                    .map(|state| state.target_name.clone())
                    .collect(),
            ),
            targets: rg_arena::Arena::from_vec(
                package_states
                    .iter()
                    .map(freeze_target_state)
                    .collect::<Vec<_>>(),
            ),
        })
        .collect())
}

fn freeze_target_state(state: &TargetState) -> super::DefMap {
    let mut def_map = state.def_map.clone();

    // The same implicit roots used by import resolution are still needed by later frozen path
    // queries. Keep them as an extern prelude rather than pretending they are child modules of the
    // crate root.
    def_map.set_extern_prelude(state.implicit_roots.clone());
    def_map.set_prelude(state.prelude);
    def_map
}

fn normalized_package_slots(packages: &[PackageSlot]) -> Vec<PackageSlot> {
    let mut slots = packages.to_vec();
    slots.sort_by_key(|slot| slot.0);
    slots.dedup();
    slots
}

fn frozen_target_states(old: &DefMapDb) -> Vec<Vec<TargetState>> {
    old.packages
        .iter()
        .enumerate()
        .map(|(package_idx, package)| {
            package
                .targets
                .iter()
                .enumerate()
                .map(|(target_idx, def_map)| {
                    let target_id = rg_parse::TargetId(target_idx);
                    let mut env_def_map = def_map.clone();
                    env_def_map.imports.clear();

                    TargetState {
                        target: TargetRef {
                            package: PackageSlot(package_idx),
                            target: target_id,
                        },
                        target_name: package
                            .target_name(target_id)
                            .map(ToOwned::to_owned)
                            .unwrap_or_else(|| {
                                format!("package {package_idx} target {target_idx}")
                            }),
                        base_scopes: def_map
                            .modules
                            .iter()
                            .map(|module| module.scope.to_builder())
                            .collect(),
                        implicit_roots: def_map.extern_prelude().clone(),
                        prelude: def_map.prelude(),
                        def_map: env_def_map,
                    }
                })
                .collect()
        })
        .collect()
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
    interner: &mut NameInterner,
) -> anyhow::Result<Vec<Vec<HashMap<Name, ModuleRef>>>> {
    let lib_targets = packages
        .iter()
        .enumerate()
        .filter_map(|(package_slot, package)| {
            package
                .targets()
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
        let mut package_roots = Vec::with_capacity(package.targets().len());
        let workspace_package = workspace.package(package.id()).with_context(|| {
            format!(
                "while attempting to fetch workspace metadata for package {}",
                package.id()
            )
        })?;

        for target in package.targets() {
            let mut target_roots = HashMap::new();

            // Cargo lets package targets refer to their sibling library by crate name, but build
            // scripts are separate crates and only see explicit build-dependencies.
            if let Some(&lib_target) = lib_targets.get(package.id()) {
                if lib_target.target != target.id && !target.kind.is_custom_build() {
                    let lib_name = package
                        .target(lib_target.target)
                        .expect("library target should exist")
                        .name
                        .clone();
                    target_roots.insert(
                        interner.intern(lib_name),
                        ModuleRef {
                            target: lib_target,
                            module: ModuleId(0),
                        },
                    );
                }
            }

            for dependency in &workspace_package.dependencies {
                if dependency.name().is_empty() || !dependency.applies_to_target(&target.kind) {
                    continue;
                }

                let Some(&lib_target) = lib_targets.get(dependency.package_id()) else {
                    continue;
                };

                target_roots.insert(
                    interner.intern(dependency.name()),
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

/// Selects the standard prelude module for each target before ordinary imports are resolved.
///
/// The prelude is not copied into every module scope. Instead, the frozen resolver treats it as a
/// final fallback layer for unqualified names. That keeps the scopes honest while still making
/// queries behave like editor users expect for names such as `Option` and `Vec`.
///
/// We resolve only the prelude module path here, not the prelude's contents. The module path is
/// structural (`std::prelude::rust_20xx`), so directly declared base scopes are enough. Imports
/// inside the prelude module are still handled by the normal fixed-point pass below.
fn select_preludes(
    workspace: &WorkspaceMetadata,
    packages: &[Package],
    states: &mut [Vec<TargetState>],
    interner: &mut NameInterner,
) -> anyhow::Result<()> {
    let base_scopes = states
        .iter()
        .map(|package_states| {
            package_states
                .iter()
                .map(|state| state.base_scopes.clone())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut selected_preludes = states
        .iter()
        .map(|package_states| vec![None; package_states.len()])
        .collect::<Vec<_>>();

    for (package_slot, package_states) in states.iter().enumerate() {
        let package = packages
            .get(package_slot)
            .expect("parse package should exist for every target state package");
        let workspace_package = workspace.package(package.id()).with_context(|| {
            format!(
                "while attempting to fetch workspace metadata for package {}",
                package.id()
            )
        })?;
        let prelude_path = ImportPath::standard_prelude(workspace_package.edition, interner);

        for (target_slot, state) in package_states.iter().enumerate() {
            let Some(root_module) = state.def_map.root_module() else {
                continue;
            };
            let Some(prelude_module) = resolve_path_to_modules(
                states,
                &base_scopes,
                state.target,
                root_module,
                &prelude_path,
            )
            .into_iter()
            .next() else {
                continue;
            };

            selected_preludes[package_slot][target_slot] = Some(prelude_module);
        }
    }

    for (package_slot, package_states) in states.iter_mut().enumerate() {
        for (target_slot, state) in package_states.iter_mut().enumerate() {
            state.prelude = selected_preludes[package_slot][target_slot];
        }
    }

    Ok(())
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

                    for (module_idx, scope) in final_scopes.iter().enumerate() {
                        let module = state
                            .def_map
                            .modules
                            .get_mut(ModuleId(module_idx))
                            .expect("module should exist for every final scope");
                        module.scope = scope.freeze();
                        module.unresolved_imports = final_unresolved_imports
                            .get(module_idx)
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
    final_scopes: &[Vec<Vec<ModuleScopeBuilder>>],
) -> Vec<Vec<Vec<Vec<ImportId>>>> {
    states
        .iter()
        .map(|package_states| {
            package_states
                .iter()
                .map(|state| {
                    let mut module_imports = vec![Vec::new(); state.def_map.modules.len()];

                    for (import_id, import) in state.def_map.imports.iter_with_ids() {
                        if import_is_unresolved(state, states, final_scopes, import) {
                            module_imports
                                .get_mut(import.module.0)
                                .expect("import module should exist while collecting unresolved imports")
                                .push(import_id);
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
    final_scopes: &[Vec<Vec<ModuleScopeBuilder>>],
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
    current_scopes: &[Vec<Vec<ModuleScopeBuilder>>],
    next_scopes: &mut [Vec<Vec<ModuleScopeBuilder>>],
) -> anyhow::Result<()> {
    for import in state.def_map.imports.iter() {
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
                    let import_owner = ModuleRef {
                        target: state.target,
                        module: import.module,
                    };
                    let source_scope = visible_module_scope_entry_set(
                        states,
                        current_scopes,
                        import_owner,
                        source_module,
                    );
                    let target_scope = next_scopes
                        .get_mut(state.target.package.0)
                        .and_then(|package_scopes| package_scopes.get_mut(state.target.target.0))
                        .and_then(|target_scopes| target_scopes.get_mut(import.module.0))
                        .expect("target scope should exist for every import");

                    // Visibility is attached to the binding introduced by the glob import, not to
                    // the original definition.
                    for (name, entry) in source_scope.entries() {
                        target_scope.copy_visible_bindings(
                            name,
                            entry,
                            import.visibility.clone(),
                            import_owner,
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
                            owner: ModuleRef {
                                target: state.target,
                                module: import.module,
                            },
                        },
                    );
                }
            }
        }
    }

    Ok(())
}
