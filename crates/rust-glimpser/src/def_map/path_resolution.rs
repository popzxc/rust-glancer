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
    DefId, ModuleId, ModuleRef, ModuleScope, ScopeEntry, TargetRef, collect::TargetState,
    data::Namespace,
};

/// Returns the subset of one module scope that is visible to the importing target.
///
/// The result keeps the same textual-name-to-`ScopeEntry` shape as `ModuleScope`, but filters out
/// bindings that are not visible from the caller's target.
pub(super) fn visible_module_scope_entry_set(
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
    let Some((first_segment, remaining_segments)) = path.segments.split_first() else {
        return Vec::new();
    };

    // The first segment is special because relative paths can start from local scope while
    // absolute paths can only start from implicit target roots.
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
            // Local scope wins over implicit roots for relative names.
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

/// Resolves every path segment after the first one.
///
/// At this point resolution can only continue through modules, so any non-module intermediate
/// definition is discarded.
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

/// Resolves one textual name inside one module scope across all namespaces.
///
/// The result is visibility-filtered from the perspective of the importing target, because
/// cross-target resolution is allowed to see only public bindings.
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

    // One textual name can contribute bindings from several namespaces, so we collect them all
    // into a deduplicated result set.
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

/// Checks whether a binding can be observed from the importing target.
///
/// This is intentionally simplified for the current scope of the project: inside one target all
/// bindings are treated as visible, and cross-target visibility is approximated as `pub` only.
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

/// Pushes one resolved definition unless it is already present in the result list.
fn push_unique_def(defs: &mut Vec<DefId>, def: DefId) {
    if !defs.contains(&def) {
        defs.push(def);
    }
}
