//! Helpers for resolving import paths against the current def-map scope snapshot.
//!
//! Resolution here is intentionally narrow:
//! - it works only with the already-built module scopes
//! - it understands module navigation (`self`, `super`, `crate`)
//! - it can return multiple definitions because several namespaces may share one textual name
//!
//! This module does not mutate scopes. It is used by `resolve.rs` while building the next
//! fixed-point iteration.

use std::collections::HashMap;

use crate::item_tree::VisibilityLevel;

use super::{
    DefId, ModuleData, ModuleId, ModuleRef, ModuleScope, ScopeBinding, ScopeEntry, TargetRef,
    collect::TargetState, data::Namespace,
};

/// Returns the subset of one module scope that is visible to the importing target.
///
/// The result keeps the same textual-name-to-`ScopeEntry` shape as `ModuleScope`, but filters out
/// bindings that are not visible from the caller's target.
pub(super) fn visible_module_scope_entry_set(
    states: &[Vec<TargetState>],
    current_scopes: &[Vec<Vec<ModuleScope>>],
    importing_module: ModuleRef,
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
            if binding_is_visible(states, importing_module, binding) {
                visible_entry.insert_binding(Namespace::Types, binding.clone());
            }
        }

        for binding in &entry.values {
            if binding_is_visible(states, importing_module, binding) {
                visible_entry.insert_binding(Namespace::Values, binding.clone());
            }
        }

        for binding in &entry.macros {
            if binding_is_visible(states, importing_module, binding) {
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

    names
}

/// Resolves a path to the definitions it denotes in the current scope snapshot.
///
/// The return type is a list rather than a single value because one textual name may resolve in
/// multiple namespaces at once.
pub(super) fn resolve_path_to_defs(
    states: &[Vec<TargetState>],
    current_scopes: &[Vec<Vec<ModuleScope>>],
    importing_target: TargetRef,
    importing_module: ModuleId,
    path: &super::ImportPath,
) -> Vec<DefId> {
    let importing_module_ref = ModuleRef {
        target: importing_target,
        module: importing_module,
    };
    let Some((first_segment, remaining_segments)) = path.segments.split_first() else {
        return Vec::new();
    };

    // The first segment is special because relative paths can start from local scope while
    // absolute paths can only start from implicit target roots.
    let mut current_defs = resolve_first_segment(
        states,
        current_scopes,
        importing_module_ref,
        path.absolute,
        first_segment,
    );

    for segment in remaining_segments {
        current_defs = resolve_next_segment(
            states,
            current_scopes,
            importing_module_ref,
            current_defs,
            segment,
        );
    }

    current_defs
}

/// Resolves a path and keeps only module results.
///
/// This is used by glob imports, where the path must denote one or more source modules whose
/// contents will be copied into the importing scope.
pub(super) fn resolve_path_to_modules(
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

/// Resolves the first path segment, which decides the starting search space.
///
/// Relative names first try the current module scope and then fall back to implicit roots.
/// Absolute names skip local scope entirely and can only start from implicit roots.
fn resolve_first_segment(
    states: &[Vec<TargetState>],
    current_scopes: &[Vec<Vec<ModuleScope>>],
    importing_module: ModuleRef,
    absolute: bool,
    segment: &super::PathSegment,
) -> Vec<DefId> {
    if absolute {
        return match segment {
            super::PathSegment::Name(name) => states
                .get(importing_module.target.package.0)
                .and_then(|package_states| package_states.get(importing_module.target.target.0))
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
        super::PathSegment::SelfKw => vec![DefId::Module(importing_module)],
        super::PathSegment::SuperKw => {
            parent_module(states, importing_module.target, importing_module.module)
                .map(DefId::Module)
                .into_iter()
                .collect()
        }
        super::PathSegment::CrateKw => root_module_ref(states, importing_module.target)
            .map(DefId::Module)
            .into_iter()
            .collect(),
        super::PathSegment::Name(name) => {
            // Local scope wins over implicit roots for relative names.
            let local_defs = resolve_name_in_module(
                states,
                current_scopes,
                importing_module,
                importing_module,
                name,
            );
            if !local_defs.is_empty() {
                return local_defs;
            }

            states
                .get(importing_module.target.package.0)
                .and_then(|package_states| package_states.get(importing_module.target.target.0))
                .and_then(|state| state.implicit_roots.get(name))
                .copied()
                .map(|module_ref| vec![DefId::Module(module_ref)])
                .unwrap_or_default()
        }
    }
}

/// Resolves every path segment after the first one.
///
/// At this point resolution can only continue through modules, so any non-module intermediate
/// definition is discarded.
fn resolve_next_segment(
    states: &[Vec<TargetState>],
    current_scopes: &[Vec<Vec<ModuleScope>>],
    importing_module: ModuleRef,
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
                for resolved_def in resolve_name_in_module(
                    states,
                    current_scopes,
                    importing_module,
                    module_ref,
                    name,
                ) {
                    push_unique_def(&mut next_defs, resolved_def);
                }
            }
        }
    }

    next_defs
}

/// Resolves one textual name inside one module scope across all namespaces.
///
/// The result is visibility-filtered from the perspective of the importing target, because
/// cross-target resolution is allowed to see only public bindings.
fn resolve_name_in_module(
    states: &[Vec<TargetState>],
    current_scopes: &[Vec<Vec<ModuleScope>>],
    importing_module: ModuleRef,
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

    // One textual name can contribute bindings from several namespaces, so we collect them all
    // into a deduplicated result set.
    for binding in &scope_entry.types {
        if binding_is_visible(states, importing_module, binding) {
            push_unique_def(&mut defs, binding.def);
        }
    }

    for binding in &scope_entry.values {
        if binding_is_visible(states, importing_module, binding) {
            push_unique_def(&mut defs, binding.def);
        }
    }

    for binding in &scope_entry.macros {
        if binding_is_visible(states, importing_module, binding) {
            push_unique_def(&mut defs, binding.def);
        }
    }

    defs
}

/// Checks whether a binding can be observed from the importing module.
fn binding_is_visible(
    states: &[Vec<TargetState>],
    importing_module: ModuleRef,
    binding: &ScopeBinding,
) -> bool {
    if matches!(binding.visibility, VisibilityLevel::Public) {
        return true;
    }

    // Non-public visibility is always anchored to a module inside the target that introduced the
    // binding. Cross-target access therefore needs a public re-export first.
    if importing_module.target != binding.owner.target {
        return false;
    }

    match &binding.visibility {
        VisibilityLevel::Private | VisibilityLevel::Self_ => {
            module_is_descendant_of(states, importing_module, binding.owner)
        }
        VisibilityLevel::Crate => true,
        VisibilityLevel::Super => parent_module(states, binding.owner.target, binding.owner.module)
            .is_some_and(|visible_from| {
                module_is_descendant_of(states, importing_module, visible_from)
            }),
        VisibilityLevel::Restricted(path) => {
            restricted_visibility_owner(states, binding.owner, path).is_some_and(|visible_from| {
                module_is_descendant_of(states, importing_module, visible_from)
            })
        }
        VisibilityLevel::Public => true,
        VisibilityLevel::Unknown(_) => false,
    }
}

/// Resolves the module that anchors a `pub(in path)` visibility restriction.
fn restricted_visibility_owner(
    states: &[Vec<TargetState>],
    owner: ModuleRef,
    path: &str,
) -> Option<ModuleRef> {
    let mut segments = path.split("::");
    let first = segments.next()?;
    let mut current = match first {
        "crate" => root_module_ref(states, owner.target)?,
        "self" => owner,
        "super" => parent_module(states, owner.target, owner.module)?,
        _ => return None,
    };

    for segment in segments {
        let module = module_data(states, current)?;
        let child = module
            .children
            .iter()
            .find_map(|(name, child)| (name == segment).then_some(*child))?;
        current = ModuleRef {
            target: current.target,
            module: child,
        };
    }

    Some(current)
}

/// Returns whether `module` is the same as or nested inside `ancestor`.
fn module_is_descendant_of(
    states: &[Vec<TargetState>],
    module: ModuleRef,
    ancestor: ModuleRef,
) -> bool {
    if module.target != ancestor.target {
        return false;
    }

    let mut current = Some(module.module);
    while let Some(module_id) = current {
        if module_id == ancestor.module {
            return true;
        }

        current = module_data(
            states,
            ModuleRef {
                target: module.target,
                module: module_id,
            },
        )
        .and_then(|module| module.parent);
    }

    false
}

/// Maps a resolved definition to the namespace bucket it occupies in scope.
pub(super) fn namespace_for_def(states: &[Vec<TargetState>], def: DefId) -> Option<Namespace> {
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

/// Returns the root module reference for one target.
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

/// Returns the parent module reference, if this module is not the root.
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

fn module_data(states: &[Vec<TargetState>], module_ref: ModuleRef) -> Option<&ModuleData> {
    states
        .get(module_ref.target.package.0)?
        .get(module_ref.target.target.0)?
        .def_map
        .module(module_ref.module)
}

/// Pushes one resolved definition unless it is already present in the result list.
fn push_unique_def(defs: &mut Vec<DefId>, def: DefId) {
    if !defs.contains(&def) {
        defs.push(def);
    }
}
