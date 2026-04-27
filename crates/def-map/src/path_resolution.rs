//! Helpers for resolving paths against def-map scopes.
//!
//! Resolution here is intentionally narrow:
//! - it works only with already-built module scopes
//! - it understands module navigation (`self`, `super`, `crate`)
//! - it can return multiple definitions because several namespaces may share one textual name
//!
//! During def-map construction this module reads from the fixed-point scope snapshot. After
//! construction, the same path-walking logic reads from frozen `DefMapDb` data.

use std::collections::HashMap;

use rg_item_tree::VisibilityLevel;

use super::{
    DefId, DefMapDb, ModuleData, ModuleId, ModuleRef, ModuleScope, Path, ScopeBinding, ScopeEntry,
    TargetRef, collect::TargetState, data::Namespace,
};

/// Result of resolving a path against the frozen def-map graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvePathResult {
    pub resolved: Vec<DefId>,
    pub unresolved_at: Option<usize>,
}

/// Minimal scope graph required by the path resolver.
trait PathResolutionEnv {
    fn extern_root(&self, target: TargetRef, name: &str) -> Option<ModuleRef>;

    fn prelude_module(&self, target: TargetRef) -> Option<ModuleRef>;

    fn root_module(&self, target: TargetRef) -> Option<ModuleRef>;

    fn module_data(&self, module_ref: ModuleRef) -> Option<&ModuleData>;

    fn module_scope(&self, module_ref: ModuleRef) -> Option<&ModuleScope>;

    fn parent_module(&self, target: TargetRef, module_id: ModuleId) -> Option<ModuleRef> {
        let module = self.module_data(ModuleRef {
            target,
            module: module_id,
        })?;

        Some(ModuleRef {
            target,
            module: module.parent?,
        })
    }
}

/// Resolution environment used while imports are being fixed up.
struct BuildResolutionEnv<'a> {
    states: &'a [Vec<TargetState>],
    current_scopes: &'a [Vec<Vec<ModuleScope>>],
}

impl<'a> BuildResolutionEnv<'a> {
    fn new(states: &'a [Vec<TargetState>], current_scopes: &'a [Vec<Vec<ModuleScope>>]) -> Self {
        Self {
            states,
            current_scopes,
        }
    }

    fn target_state(&self, target: TargetRef) -> Option<&'a TargetState> {
        self.states.get(target.package.0)?.get(target.target.0)
    }
}

impl PathResolutionEnv for BuildResolutionEnv<'_> {
    fn extern_root(&self, target: TargetRef, name: &str) -> Option<ModuleRef> {
        self.target_state(target)?.implicit_roots.get(name).copied()
    }

    fn prelude_module(&self, target: TargetRef) -> Option<ModuleRef> {
        self.target_state(target)?.prelude
    }

    fn root_module(&self, target: TargetRef) -> Option<ModuleRef> {
        Some(ModuleRef {
            target,
            module: self.target_state(target)?.def_map.root_module()?,
        })
    }

    fn module_data(&self, module_ref: ModuleRef) -> Option<&ModuleData> {
        self.target_state(module_ref.target)?
            .def_map
            .module(module_ref.module)
    }

    fn module_scope(&self, module_ref: ModuleRef) -> Option<&ModuleScope> {
        self.current_scopes
            .get(module_ref.target.package.0)?
            .get(module_ref.target.target.0)?
            .get(module_ref.module.0)
    }
}

/// Resolution environment used by frozen query APIs.
struct FrozenResolutionEnv<'a> {
    db: &'a DefMapDb,
}

impl<'a> FrozenResolutionEnv<'a> {
    fn new(db: &'a DefMapDb) -> Self {
        Self { db }
    }
}

impl PathResolutionEnv for FrozenResolutionEnv<'_> {
    fn extern_root(&self, target: TargetRef, name: &str) -> Option<ModuleRef> {
        self.db.def_map(target)?.extern_prelude().get(name).copied()
    }

    fn prelude_module(&self, target: TargetRef) -> Option<ModuleRef> {
        self.db.def_map(target)?.prelude()
    }

    fn root_module(&self, target: TargetRef) -> Option<ModuleRef> {
        Some(ModuleRef {
            target,
            module: self.db.def_map(target)?.root_module()?,
        })
    }

    fn module_data(&self, module_ref: ModuleRef) -> Option<&ModuleData> {
        self.db
            .def_map(module_ref.target)?
            .module(module_ref.module)
    }

    fn module_scope(&self, module_ref: ModuleRef) -> Option<&ModuleScope> {
        self.module_data(module_ref).map(|module| &module.scope)
    }
}

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
    let env = BuildResolutionEnv::new(states, current_scopes);
    let Some(module_scope) = env.module_scope(source_module) else {
        return HashMap::new();
    };

    let mut names = HashMap::new();

    for (name, entry) in &module_scope.names {
        let mut visible_entry = ScopeEntry::default();

        for binding in &entry.types {
            if binding_is_visible(&env, importing_module, binding) {
                visible_entry.insert_binding(Namespace::Types, binding.clone());
            }
        }

        for binding in &entry.values {
            if binding_is_visible(&env, importing_module, binding) {
                visible_entry.insert_binding(Namespace::Values, binding.clone());
            }
        }

        for binding in &entry.macros {
            if binding_is_visible(&env, importing_module, binding) {
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
    let env = BuildResolutionEnv::new(states, current_scopes);
    let result = resolve_path_with_env(
        &env,
        ModuleRef {
            target: importing_target,
            module: importing_module,
        },
        path.absolute,
        &path.segments,
    );

    result.resolved
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

/// Resolves a path against the already-frozen def maps.
pub fn resolve_path_in_db(
    db: &DefMapDb,
    importing_module: ModuleRef,
    path: &Path,
) -> ResolvePathResult {
    let env = FrozenResolutionEnv::new(db);
    resolve_path_with_env(&env, importing_module, path.absolute, &path.segments)
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

/// Walks a path through one resolution environment.
fn resolve_path_with_env(
    env: &impl PathResolutionEnv,
    importing_module: ModuleRef,
    absolute: bool,
    segments: &[super::PathSegment],
) -> ResolvePathResult {
    let Some((first_segment, remaining_segments)) = segments.split_first() else {
        return ResolvePathResult {
            resolved: Vec::new(),
            unresolved_at: Some(0),
        };
    };

    let mut current_defs = resolve_first_segment(
        env,
        importing_module,
        absolute,
        first_segment,
        !remaining_segments.is_empty(),
    );

    if current_defs.is_empty() {
        return ResolvePathResult {
            resolved: current_defs,
            unresolved_at: Some(0),
        };
    }

    for (segment_idx, segment) in remaining_segments.iter().enumerate() {
        current_defs = resolve_next_segment(
            env,
            importing_module,
            current_defs,
            segment,
            segment_idx + 1 < remaining_segments.len(),
        );

        if current_defs.is_empty() {
            return ResolvePathResult {
                resolved: current_defs,
                unresolved_at: Some(segment_idx + 1),
            };
        }
    }

    ResolvePathResult {
        resolved: current_defs,
        unresolved_at: None,
    }
}

/// Resolves the first path segment, which decides the starting search space.
///
/// Relative names first try the current module scope, then extern roots, then the standard
/// prelude. Absolute names skip local scope and prelude fallback entirely.
fn resolve_first_segment(
    env: &impl PathResolutionEnv,
    importing_module: ModuleRef,
    absolute: bool,
    segment: &super::PathSegment,
    path_prefix: bool,
) -> Vec<DefId> {
    if absolute {
        return match segment {
            super::PathSegment::Name(name) => env
                .extern_root(importing_module.target, name)
                .map(|module_ref| vec![DefId::Module(module_ref)])
                .unwrap_or_default(),
            super::PathSegment::SelfKw
            | super::PathSegment::SuperKw
            | super::PathSegment::CrateKw => Vec::new(),
        };
    }

    match segment {
        super::PathSegment::SelfKw => vec![DefId::Module(importing_module)],
        super::PathSegment::SuperKw => env
            .parent_module(importing_module.target, importing_module.module)
            .map(DefId::Module)
            .into_iter()
            .collect(),
        super::PathSegment::CrateKw => env
            .root_module(importing_module.target)
            .map(DefId::Module)
            .into_iter()
            .collect(),
        super::PathSegment::Name(name) => {
            // Local type-namespace bindings shadow extern roots for qualified paths. Value and
            // macro bindings do not, because they cannot be used as a `foo::bar` prefix.
            let local_defs = resolve_name_in_module(
                env,
                importing_module,
                importing_module,
                name,
                NameResolutionFilter::for_path_prefix(path_prefix),
            );
            if !local_defs.is_empty() {
                return local_defs;
            }

            if let Some(module_ref) = env.extern_root(importing_module.target, name) {
                return vec![DefId::Module(module_ref)];
            }

            let Some(prelude_module) = env.prelude_module(importing_module.target) else {
                return Vec::new();
            };

            resolve_name_in_module(
                env,
                importing_module,
                prelude_module,
                name,
                NameResolutionFilter::for_path_prefix(path_prefix),
            )
        }
    }
}

/// Resolves every path segment after the first one.
///
/// At this point resolution can only continue through modules, so any non-module intermediate
/// definition is discarded.
fn resolve_next_segment(
    env: &impl PathResolutionEnv,
    importing_module: ModuleRef,
    current_defs: Vec<DefId>,
    segment: &super::PathSegment,
    path_prefix: bool,
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
                if let Some(parent) = env.parent_module(module_ref.target, module_ref.module) {
                    push_unique_def(&mut next_defs, DefId::Module(parent));
                }
            }
            super::PathSegment::CrateKw => {
                if let Some(root) = env.root_module(module_ref.target) {
                    push_unique_def(&mut next_defs, DefId::Module(root));
                }
            }
            super::PathSegment::Name(name) => {
                for resolved_def in resolve_name_in_module(
                    env,
                    importing_module,
                    module_ref,
                    name,
                    NameResolutionFilter::for_path_prefix(path_prefix),
                ) {
                    push_unique_def(&mut next_defs, resolved_def);
                }
            }
        }
    }

    next_defs
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NameResolutionFilter {
    AllNamespaces,
    TypesOnly,
}

impl NameResolutionFilter {
    fn for_path_prefix(path_prefix: bool) -> Self {
        if path_prefix {
            Self::TypesOnly
        } else {
            Self::AllNamespaces
        }
    }
}

/// Resolves one textual name inside one module scope.
///
/// The result is visibility-filtered from the perspective of the importing target, because
/// cross-target resolution is allowed to see only public bindings. Qualified path prefixes use only
/// the type namespace; terminal segments use every namespace.
fn resolve_name_in_module(
    env: &impl PathResolutionEnv,
    importing_module: ModuleRef,
    module_ref: ModuleRef,
    name: &str,
    filter: NameResolutionFilter,
) -> Vec<DefId> {
    let Some(scope_entry) = env
        .module_scope(module_ref)
        .and_then(|scope| scope.entry(name))
    else {
        return Vec::new();
    };

    let mut defs = Vec::new();

    // One textual name can contribute bindings from several namespaces, so we collect them all
    // into a deduplicated result set.
    for binding in &scope_entry.types {
        if binding_is_visible(env, importing_module, binding) {
            push_unique_def(&mut defs, binding.def);
        }
    }

    if matches!(filter, NameResolutionFilter::TypesOnly) {
        return defs;
    }

    for binding in &scope_entry.values {
        if binding_is_visible(env, importing_module, binding) {
            push_unique_def(&mut defs, binding.def);
        }
    }

    for binding in &scope_entry.macros {
        if binding_is_visible(env, importing_module, binding) {
            push_unique_def(&mut defs, binding.def);
        }
    }

    defs
}

/// Checks whether a binding can be observed from the importing module.
fn binding_is_visible(
    env: &impl PathResolutionEnv,
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
            module_is_descendant_of(env, importing_module, binding.owner)
        }
        VisibilityLevel::Crate => true,
        VisibilityLevel::Super => env
            .parent_module(binding.owner.target, binding.owner.module)
            .is_some_and(|visible_from| {
                module_is_descendant_of(env, importing_module, visible_from)
            }),
        VisibilityLevel::Restricted(path) => restricted_visibility_owner(env, binding.owner, path)
            .is_some_and(|visible_from| {
                module_is_descendant_of(env, importing_module, visible_from)
            }),
        VisibilityLevel::Public => true,
        VisibilityLevel::Unknown(_) => false,
    }
}

/// Resolves the module that anchors a `pub(in path)` visibility restriction.
fn restricted_visibility_owner(
    env: &impl PathResolutionEnv,
    owner: ModuleRef,
    path: &str,
) -> Option<ModuleRef> {
    let mut segments = path.split("::");
    let first = segments.next()?;
    let mut current = match first {
        "crate" => env.root_module(owner.target)?,
        "self" => owner,
        "super" => env.parent_module(owner.target, owner.module)?,
        _ => return None,
    };

    for segment in segments {
        let module = env.module_data(current)?;
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
    env: &impl PathResolutionEnv,
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

        current = env
            .module_data(ModuleRef {
                target: module.target,
                module: module_id,
            })
            .and_then(|module| module.parent);
    }

    false
}

/// Pushes one resolved definition unless it is already present in the result list.
fn push_unique_def(defs: &mut Vec<DefId>, def: DefId) {
    if !defs.contains(&def) {
        defs.push(def);
    }
}
