use std::collections::{HashMap, HashSet};

use anyhow::Context as _;
use ra_syntax::{
    AstNode as _,
    ast::{self, HasModuleItem, HasName, HasVisibility},
};

use crate::parse::{
    file::{FileId, ParseDb},
    item::{ItemKind, VisibilityLevel},
    package::PackageIndex,
    span::{LineIndex, Span},
    target::{TargetId, TargetIndex, resolve_module_file},
};

// =============================================================================
// Stable References
// =============================================================================

/// Stable identifier of one module inside a target map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModuleId(pub usize);

/// Stable identifier of one local definition inside a target map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalDefId(pub usize);

/// Stable identifier of one lowered import inside a target map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImportId(pub usize);

/// Stable identifier of one analyzed package inside a project analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PackageSlot(pub usize);

/// Stable reference to one target across the whole project analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TargetRef {
    pub package: PackageSlot,
    pub target: TargetId,
}

/// Stable reference to one module across the whole project analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModuleRef {
    pub target: TargetRef,
    pub module: ModuleId,
}

/// Stable reference to one local definition across the whole project analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalDefRef {
    pub target: TargetRef,
    pub local_def: LocalDefId,
}

/// Namespace-resolved target-level definition reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DefId {
    Module(ModuleRef),
    Local(LocalDefRef),
}

// =============================================================================
// Final Data
// =============================================================================

/// Frozen namespace map for one analyzed target.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DefMap {
    root_module: Option<ModuleId>,
    pub modules: Vec<ModuleData>,
    pub local_defs: Vec<LocalDefData>,
    pub imports: Vec<ImportData>,
}

impl DefMap {
    /// Returns the root module of this target, if the map has been populated.
    pub fn root_module(&self) -> Option<ModuleId> {
        self.root_module
    }

    /// Returns module data by id.
    pub fn module(&self, module_id: ModuleId) -> Option<&ModuleData> {
        self.modules.get(module_id.0)
    }
}

/// One module in the frozen namespace graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleData {
    pub name: Option<String>,
    pub parent: Option<ModuleId>,
    pub children: Vec<(String, ModuleId)>,
    pub local_defs: Vec<LocalDefId>,
    pub imports: Vec<ImportId>,
    pub scope: ModuleScope,
    pub origin: ModuleOrigin,
}

/// Where a module came from in source code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleOrigin {
    Root {
        file_id: FileId,
    },
    Inline {
        declaration_file: FileId,
        declaration_span: Span,
    },
    OutOfLine {
        declaration_file: FileId,
        declaration_span: Span,
        definition_file: Option<FileId>,
    },
}

/// One module-scope definition collected from source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDefData {
    pub module: ModuleId,
    pub name: String,
    pub kind: ItemKind,
    pub visibility: VisibilityLevel,
    pub file_id: FileId,
    pub span: Span,
}

/// One lowered import declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportData {
    pub module: ModuleId,
    pub visibility: VisibilityLevel,
    pub kind: ImportKind,
    pub path: ImportPath,
    pub binding: ImportBinding,
}

impl ImportData {
    /// Returns the binding name introduced by this import when it is not a glob import.
    fn binding_name(&self) -> Option<String> {
        let inferred_name = match self.kind {
            ImportKind::Named => self.path.last_name(),
            ImportKind::SelfImport => self.path.last_name(),
            ImportKind::Glob => None,
        };

        self.binding.resolve(inferred_name)
    }
}

/// Binding strategy for one lowered import or extern crate item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportBinding {
    Inferred,
    Explicit(String),
    Hidden,
}

impl ImportBinding {
    fn from_rename(rename: Option<ast::Rename>) -> Self {
        let Some(rename) = rename else {
            return Self::Inferred;
        };

        if rename.underscore_token().is_some() {
            return Self::Hidden;
        }

        rename
            .name()
            .map(|name| Self::Explicit(name.text().to_string()))
            .unwrap_or(Self::Inferred)
    }

    fn resolve(&self, inferred_name: Option<String>) -> Option<String> {
        match self {
            Self::Inferred => inferred_name,
            Self::Explicit(name) => Some(name.clone()),
            Self::Hidden => None,
        }
    }
}

/// Import form that matters for scope propagation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    Named,
    SelfImport,
    Glob,
}

/// Structured path used during import resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPath {
    pub absolute: bool,
    pub segments: Vec<PathSegment>,
}

impl ImportPath {
    fn empty() -> Self {
        Self {
            absolute: false,
            segments: Vec::new(),
        }
    }

    fn joined(&self, suffix: &Self) -> Self {
        let mut segments = self.segments.clone();
        segments.extend(suffix.segments.clone());
        Self {
            absolute: self.absolute || suffix.absolute,
            segments,
        }
    }

    fn without_trailing_self(&self) -> Self {
        let mut segments = self.segments.clone();
        if matches!(segments.last(), Some(PathSegment::SelfKw)) {
            segments.pop();
        }
        Self {
            absolute: self.absolute,
            segments,
        }
    }

    fn ends_with_self(&self) -> bool {
        matches!(self.segments.last(), Some(PathSegment::SelfKw))
    }

    fn last_name(&self) -> Option<String> {
        match self.segments.last()? {
            PathSegment::Name(name) => Some(name.clone()),
            PathSegment::SelfKw => Some("self".to_string()),
            PathSegment::SuperKw => Some("super".to_string()),
            PathSegment::CrateKw => Some("crate".to_string()),
        }
    }
}

/// One structured path segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSegment {
    Name(String),
    SelfKw,
    SuperKw,
    CrateKw,
}

/// Module scope with separate namespaces stored under one textual name map.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModuleScope {
    pub names: HashMap<String, ScopeEntry>,
}

impl ModuleScope {
    fn insert_binding(&mut self, name: &str, namespace: Namespace, binding: ScopeBinding) -> bool {
        let entry = self.names.entry(name.to_string()).or_default();
        entry.insert_binding(namespace, binding)
    }

    fn copy_visible_bindings(
        &mut self,
        name: &str,
        entry: &ScopeEntry,
        visibility: VisibilityLevel,
    ) {
        for binding in &entry.types {
            self.insert_binding(
                name,
                Namespace::Types,
                ScopeBinding {
                    def: binding.def,
                    visibility: visibility.clone(),
                },
            );
        }

        for binding in &entry.values {
            self.insert_binding(
                name,
                Namespace::Values,
                ScopeBinding {
                    def: binding.def,
                    visibility: visibility.clone(),
                },
            );
        }

        for binding in &entry.macros {
            self.insert_binding(
                name,
                Namespace::Macros,
                ScopeBinding {
                    def: binding.def,
                    visibility: visibility.clone(),
                },
            );
        }
    }

    /// Returns the scope entry for one textual name, if present.
    pub fn entry(&self, name: &str) -> Option<&ScopeEntry> {
        self.names.get(name)
    }
}

/// All namespace slots for one textual name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScopeEntry {
    pub types: Vec<ScopeBinding>,
    pub values: Vec<ScopeBinding>,
    pub macros: Vec<ScopeBinding>,
}

impl ScopeEntry {
    fn insert_binding(&mut self, namespace: Namespace, binding: ScopeBinding) -> bool {
        let bucket = match namespace {
            Namespace::Types => &mut self.types,
            Namespace::Values => &mut self.values,
            Namespace::Macros => &mut self.macros,
        };

        if bucket.contains(&binding) {
            return false;
        }

        bucket.push(binding);
        true
    }
}

/// One definition together with the visibility of the binding that introduced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeBinding {
    pub def: DefId,
    pub visibility: VisibilityLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Namespace {
    Types,
    Values,
    Macros,
}

// =============================================================================
// Project Builder
// =============================================================================

pub(crate) fn populate_project_scopes(
    metadata: &cargo_metadata::Metadata,
    packages: &mut [PackageIndex],
    package_by_id: &HashMap<cargo_metadata::PackageId, usize>,
) -> anyhow::Result<()> {
    let implicit_roots = build_implicit_roots(metadata, packages, package_by_id)
        .context("while attempting to build implicit target roots")?;

    let mut target_states = collect_target_states(packages, &implicit_roots)
        .context("while attempting to collect target definitions and imports")?;

    finalize_scopes(&mut target_states).context("while attempting to resolve target scopes")?;

    for (package_slot, package) in packages.iter_mut().enumerate() {
        for (target_slot, target) in package.targets.iter_mut().enumerate() {
            let state = target_states
                .get_mut(package_slot)
                .and_then(|package_states| package_states.get_mut(target_slot))
                .expect("target state should exist for every parsed target");
            target.def_map = std::mem::take(&mut state.def_map);
        }
    }

    Ok(())
}

fn build_implicit_roots(
    metadata: &cargo_metadata::Metadata,
    packages: &[PackageIndex],
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

fn collect_target_states(
    packages: &mut [PackageIndex],
    implicit_roots: &[Vec<HashMap<String, ModuleRef>>],
) -> anyhow::Result<Vec<Vec<TargetState>>> {
    let mut states = Vec::with_capacity(packages.len());

    for (package_slot, package) in packages.iter_mut().enumerate() {
        let mut package_states = Vec::with_capacity(package.targets.len());

        for target in &package.targets {
            let target_ref = TargetRef {
                package: PackageSlot(package_slot),
                target: target.id,
            };
            let target_roots = implicit_roots
                .get(package_slot)
                .and_then(|package_roots| package_roots.get(target.id.0))
                .expect("implicit roots should exist for every parsed target");

            let collector = TargetScopeCollector::new(target_ref, &mut package.db, target_roots);
            let state = collector.collect(target).with_context(|| {
                format!(
                    "while attempting to collect target scope for {}",
                    target.cargo_target.name
                )
            })?;
            package_states.push(state);
        }

        states.push(package_states);
    }

    Ok(states)
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
            ImportKind::Glob => {
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
            ImportKind::Named | ImportKind::SelfImport => {
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

    // Keep the parameter list symmetric with the rest of the resolver helpers.
    let _ = states;

    names
}

fn resolve_path_to_defs(
    states: &[Vec<TargetState>],
    current_scopes: &[Vec<Vec<ModuleScope>>],
    importing_target: TargetRef,
    importing_module: ModuleId,
    path: &ImportPath,
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
    path: &ImportPath,
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
    segment: &PathSegment,
) -> Vec<DefId> {
    if absolute {
        return match segment {
            PathSegment::Name(name) => states
                .get(importing_target.package.0)
                .and_then(|package_states| package_states.get(importing_target.target.0))
                .and_then(|state| state.implicit_roots.get(name))
                .copied()
                .map(|module_ref| vec![DefId::Module(module_ref)])
                .unwrap_or_default(),
            PathSegment::SelfKw | PathSegment::SuperKw | PathSegment::CrateKw => Vec::new(),
        };
    }

    match segment {
        PathSegment::SelfKw => vec![DefId::Module(ModuleRef {
            target: importing_target,
            module: importing_module,
        })],
        PathSegment::SuperKw => parent_module(states, importing_target, importing_module)
            .map(DefId::Module)
            .into_iter()
            .collect(),
        PathSegment::CrateKw => root_module_ref(states, importing_target)
            .map(DefId::Module)
            .into_iter()
            .collect(),
        PathSegment::Name(name) => {
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
    segment: &PathSegment,
) -> Vec<DefId> {
    let mut next_defs = Vec::new();

    for current_def in current_defs {
        let DefId::Module(module_ref) = current_def else {
            continue;
        };

        match segment {
            PathSegment::SelfKw => {
                push_unique_def(&mut next_defs, DefId::Module(module_ref));
            }
            PathSegment::SuperKw => {
                if let Some(parent) = parent_module(states, module_ref.target, module_ref.module) {
                    push_unique_def(&mut next_defs, DefId::Module(parent));
                }
            }
            PathSegment::CrateKw => {
                if let Some(root) = root_module_ref(states, module_ref.target) {
                    push_unique_def(&mut next_defs, DefId::Module(root));
                }
            }
            PathSegment::Name(name) => {
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
    // Cross-target visibility matters immediately because dependency-facing resolution depends
    // on exported names. Same-target privacy is intentionally deferred for now, so intra-target
    // lookups can continue to see every collected binding.
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
            namespace_for_local_kind(local_def.kind)
        }
    }
}

fn namespace_for_local_kind(kind: ItemKind) -> Option<Namespace> {
    match kind {
        ItemKind::Const | ItemKind::Function | ItemKind::Static => Some(Namespace::Values),
        ItemKind::Enum
        | ItemKind::Struct
        | ItemKind::Trait
        | ItemKind::TypeAlias
        | ItemKind::Union => Some(Namespace::Types),
        ItemKind::MacroDefinition => Some(Namespace::Macros),
        ItemKind::AsmExpr
        | ItemKind::AssociatedConst
        | ItemKind::AssociatedFunction
        | ItemKind::AssociatedTypeAlias
        | ItemKind::ExternBlock
        | ItemKind::ExternCrate
        | ItemKind::Impl
        | ItemKind::Module
        | ItemKind::Use => None,
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

// =============================================================================
// Target Collector
// =============================================================================

struct TargetState {
    target: TargetRef,
    target_name: String,
    def_map: DefMap,
    base_scopes: Vec<ModuleScope>,
    implicit_roots: HashMap<String, ModuleRef>,
}

struct TargetScopeCollector<'db> {
    target: TargetRef,
    parse_db: &'db mut ParseDb,
    implicit_roots: &'db HashMap<String, ModuleRef>,
    active_files: HashSet<FileId>,
    def_map: DefMap,
    base_scopes: Vec<ModuleScope>,
}

impl<'db> TargetScopeCollector<'db> {
    fn new(
        target: TargetRef,
        parse_db: &'db mut ParseDb,
        implicit_roots: &'db HashMap<String, ModuleRef>,
    ) -> Self {
        Self {
            target,
            parse_db,
            implicit_roots,
            active_files: HashSet::default(),
            def_map: DefMap::default(),
            base_scopes: Vec::new(),
        }
    }

    fn collect(mut self, target: &TargetIndex) -> anyhow::Result<TargetState> {
        let root_module = self.alloc_module(
            None,
            None,
            ModuleOrigin::Root {
                file_id: target.root_file,
            },
        );
        self.def_map.root_module = Some(root_module);

        self.collect_file_items(root_module, target.root_file)
            .with_context(|| {
                format!(
                    "while attempting to collect source items for {}",
                    target.cargo_target.name
                )
            })?;

        Ok(TargetState {
            target: self.target,
            target_name: target.cargo_target.name.clone(),
            def_map: self.def_map,
            base_scopes: self.base_scopes,
            implicit_roots: self.implicit_roots.clone(),
        })
    }

    fn alloc_module(
        &mut self,
        parent: Option<ModuleId>,
        name: Option<String>,
        origin: ModuleOrigin,
    ) -> ModuleId {
        let module_id = ModuleId(self.def_map.modules.len());
        self.def_map.modules.push(ModuleData {
            name,
            parent,
            children: Vec::new(),
            local_defs: Vec::new(),
            imports: Vec::new(),
            scope: ModuleScope::default(),
            origin,
        });
        self.base_scopes.push(ModuleScope::default());
        module_id
    }

    fn collect_file_items(&mut self, module_id: ModuleId, file_id: FileId) -> anyhow::Result<()> {
        if !self.active_files.insert(file_id) {
            return Ok(());
        }

        let (items, line_index) = {
            let parsed_file = self
                .parse_db
                .parsed_file(file_id)
                .with_context(|| format!("while attempting to fetch parsed file {:?}", file_id))?;
            (
                parsed_file.tree.items().collect::<Vec<_>>(),
                parsed_file.line_index.clone(),
            )
        };

        self.collect_items(module_id, items, file_id, &line_index)
            .with_context(|| format!("while attempting to collect file items for {:?}", file_id))?;

        self.active_files.remove(&file_id);
        Ok(())
    }

    fn collect_items(
        &mut self,
        module_id: ModuleId,
        items: Vec<ast::Item>,
        file_id: FileId,
        line_index: &LineIndex,
    ) -> anyhow::Result<()> {
        for item in items {
            match item {
                ast::Item::AsmExpr(_) => {}
                ast::Item::Const(item) => self.collect_local_def(
                    module_id,
                    ItemKind::Const,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::Enum(item) => self.collect_local_def(
                    module_id,
                    ItemKind::Enum,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::ExternBlock(_) => {}
                ast::Item::ExternCrate(item) => self.collect_extern_crate(module_id, item),
                ast::Item::Fn(item) => self.collect_local_def(
                    module_id,
                    ItemKind::Function,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::Impl(_) => {}
                ast::Item::MacroCall(_) => {}
                ast::Item::MacroDef(item) => self.collect_local_def(
                    module_id,
                    ItemKind::MacroDefinition,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::MacroRules(item) => self.collect_local_def(
                    module_id,
                    ItemKind::MacroDefinition,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::Module(item) => self
                    .collect_module(module_id, item, file_id, line_index)
                    .context("while attempting to collect module item")?,
                ast::Item::Static(item) => self.collect_local_def(
                    module_id,
                    ItemKind::Static,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::Struct(item) => self.collect_local_def(
                    module_id,
                    ItemKind::Struct,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::Trait(item) => self.collect_local_def(
                    module_id,
                    ItemKind::Trait,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::TypeAlias(item) => self.collect_local_def(
                    module_id,
                    ItemKind::TypeAlias,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::Union(item) => self.collect_local_def(
                    module_id,
                    ItemKind::Union,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::Use(item) => self.collect_use(module_id, item),
            }
        }

        Ok(())
    }

    fn collect_local_def(
        &mut self,
        module_id: ModuleId,
        kind: ItemKind,
        name: Option<String>,
        visibility: VisibilityLevel,
        file_id: FileId,
        span: Span,
    ) {
        let Some(name) = name else {
            return;
        };
        let Some(namespace) = namespace_for_local_kind(kind) else {
            return;
        };

        let local_def_id = LocalDefId(self.def_map.local_defs.len());
        self.def_map.local_defs.push(LocalDefData {
            module: module_id,
            name: name.clone(),
            kind,
            visibility: visibility.clone(),
            file_id,
            span,
        });
        self.def_map
            .modules
            .get_mut(module_id.0)
            .expect("module should exist for collected local definition")
            .local_defs
            .push(local_def_id);
        self.base_scopes
            .get_mut(module_id.0)
            .expect("base scope should exist for collected local definition")
            .insert_binding(
                &name,
                namespace,
                ScopeBinding {
                    def: DefId::Local(LocalDefRef {
                        target: self.target,
                        local_def: local_def_id,
                    }),
                    visibility,
                },
            );
    }

    fn collect_module(
        &mut self,
        parent_module: ModuleId,
        item: ast::Module,
        file_id: FileId,
        line_index: &LineIndex,
    ) -> anyhow::Result<()> {
        let Some(module_name) = item.name().map(|name| name.text().to_string()) else {
            return Ok(());
        };
        let visibility = VisibilityLevel::from_ast(item.visibility());
        let declaration_span = Span::from_text_range(item.syntax().text_range(), line_index);

        if let Some(item_list) = item.item_list() {
            let child_module = self.alloc_module(
                Some(parent_module),
                Some(module_name.clone()),
                ModuleOrigin::Inline {
                    declaration_file: file_id,
                    declaration_span,
                },
            );
            self.link_child_module(parent_module, child_module, &module_name, visibility);

            let inline_items = item_list.items().collect::<Vec<_>>();
            self.collect_items(child_module, inline_items, file_id, line_index)
                .with_context(|| {
                    format!("while attempting to collect inline module {module_name}")
                })?;
            return Ok(());
        }

        let current_file_path = self
            .parse_db
            .file_path(file_id)
            .with_context(|| format!("while attempting to resolve current file {:?}", file_id))?;
        let module_file_path = resolve_module_file(current_file_path, &module_name);
        let definition_file = if let Some(module_file_path) = module_file_path {
            Some(
                self.parse_db
                    .get_or_parse_file(&module_file_path)
                    .with_context(|| {
                        format!(
                            "while attempting to parse module file {}",
                            module_file_path.display()
                        )
                    })?,
            )
        } else {
            None
        };

        let child_module = self.alloc_module(
            Some(parent_module),
            Some(module_name.clone()),
            ModuleOrigin::OutOfLine {
                declaration_file: file_id,
                declaration_span,
                definition_file,
            },
        );
        self.link_child_module(parent_module, child_module, &module_name, visibility);

        if let Some(definition_file) = definition_file {
            self.collect_file_items(child_module, definition_file)
                .with_context(|| {
                    format!("while attempting to collect out-of-line module {module_name}")
                })?;
        }

        Ok(())
    }

    fn link_child_module(
        &mut self,
        parent_module: ModuleId,
        child_module: ModuleId,
        module_name: &str,
        visibility: VisibilityLevel,
    ) {
        self.def_map
            .modules
            .get_mut(parent_module.0)
            .expect("parent module should exist for child link")
            .children
            .push((module_name.to_string(), child_module));
        self.base_scopes
            .get_mut(parent_module.0)
            .expect("base scope should exist for child link")
            .insert_binding(
                module_name,
                Namespace::Types,
                ScopeBinding {
                    def: DefId::Module(ModuleRef {
                        target: self.target,
                        module: child_module,
                    }),
                    visibility,
                },
            );
    }

    fn collect_use(&mut self, module_id: ModuleId, item: ast::Use) {
        let visibility = VisibilityLevel::from_ast(item.visibility());
        let Some(use_tree) = item.use_tree() else {
            return;
        };

        self.lower_use_tree(module_id, &ImportPath::empty(), visibility, use_tree);
    }

    fn lower_use_tree(
        &mut self,
        module_id: ModuleId,
        prefix: &ImportPath,
        visibility: VisibilityLevel,
        use_tree: ast::UseTree,
    ) {
        let path = use_tree
            .path()
            .and_then(|path| lower_path(&path))
            .unwrap_or_else(ImportPath::empty);
        let path = prefix.joined(&path);

        if let Some(use_tree_list) = use_tree.use_tree_list() {
            for child_use_tree in use_tree_list.use_trees() {
                self.lower_use_tree(module_id, &path, visibility.clone(), child_use_tree);
            }
            return;
        }

        let binding = ImportBinding::from_rename(use_tree.rename());

        let (kind, path) = if use_tree.star_token().is_some() {
            (ImportKind::Glob, path)
        } else if path.ends_with_self() {
            // `use foo::{self}` should resolve the `foo` module and bind it under `foo`
            // (or an explicit alias), so the lowered import keeps the module path.
            (ImportKind::SelfImport, path.without_trailing_self())
        } else {
            (ImportKind::Named, path)
        };

        if path.segments.is_empty() {
            return;
        }

        let import_id = ImportId(self.def_map.imports.len());
        self.def_map.imports.push(ImportData {
            module: module_id,
            visibility,
            kind,
            path,
            binding,
        });
        self.def_map
            .modules
            .get_mut(module_id.0)
            .expect("module should exist for lowered import")
            .imports
            .push(import_id);
    }

    fn collect_extern_crate(&mut self, module_id: ModuleId, item: ast::ExternCrate) {
        let Some(extern_name) = item.name_ref().map(|name_ref| name_ref.text().to_string()) else {
            return;
        };
        let Some(binding_name) =
            ImportBinding::from_rename(item.rename()).resolve(Some(extern_name.clone()))
        else {
            return;
        };

        let module_ref = if extern_name == "self" {
            ModuleRef {
                target: self.target,
                module: self
                    .def_map
                    .root_module()
                    .expect("root module should exist before extern crate collection"),
            }
        } else {
            let Some(module_ref) = self.implicit_roots.get(&extern_name).copied() else {
                return;
            };
            module_ref
        };

        let visibility = VisibilityLevel::from_ast(item.visibility());
        self.base_scopes
            .get_mut(module_id.0)
            .expect("base scope should exist for extern crate binding")
            .insert_binding(
                &binding_name,
                Namespace::Types,
                ScopeBinding {
                    def: DefId::Module(module_ref),
                    visibility,
                },
            );
    }
}

fn lower_path(path: &ast::Path) -> Option<ImportPath> {
    let mut segments = Vec::new();

    for segment in path.segments() {
        let lowered_segment = match segment.kind()? {
            ast::PathSegmentKind::Name(name_ref) => PathSegment::Name(name_ref.text().to_string()),
            ast::PathSegmentKind::SelfKw => PathSegment::SelfKw,
            ast::PathSegmentKind::SuperKw => PathSegment::SuperKw,
            ast::PathSegmentKind::CrateKw => PathSegment::CrateKw,
            ast::PathSegmentKind::SelfTypeKw | ast::PathSegmentKind::Type { .. } => return None,
        };
        segments.push(lowered_segment);
    }

    Some(ImportPath {
        absolute: path
            .first_segment()
            .is_some_and(|segment| segment.coloncolon_token().is_some()),
        segments,
    })
}

#[cfg(test)]
mod tests;
