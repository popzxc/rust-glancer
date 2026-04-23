use std::collections::HashMap;

use crate::{
    item_tree::{ItemTag, VisibilityLevel},
    parse::{file::FileId, span::Span},
};

use super::{DefId, ImportData, ImportId, LocalDefId, ModuleId};

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

    pub(super) fn set_root_module(&mut self, root_module: ModuleId) {
        self.root_module = Some(root_module);
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
    pub kind: ItemTag,
    pub visibility: VisibilityLevel,
    pub file_id: FileId,
    pub span: Span,
}

/// Module scope with separate namespaces stored under one textual name map.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModuleScope {
    pub names: HashMap<String, ScopeEntry>,
}

impl ModuleScope {
    pub(super) fn insert_binding(
        &mut self,
        name: &str,
        namespace: Namespace,
        binding: ScopeBinding,
    ) -> bool {
        let entry = self.names.entry(name.to_string()).or_default();
        entry.insert_binding(namespace, binding)
    }

    pub(super) fn copy_visible_bindings(
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
    pub(super) fn insert_binding(&mut self, namespace: Namespace, binding: ScopeBinding) -> bool {
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
pub(super) enum Namespace {
    Types,
    Values,
    Macros,
}

pub(super) fn namespace_for_local_kind(kind: ItemTag) -> Option<Namespace> {
    match kind {
        ItemTag::Const | ItemTag::Function | ItemTag::Static => Some(Namespace::Values),
        ItemTag::Enum | ItemTag::Struct | ItemTag::Trait | ItemTag::TypeAlias | ItemTag::Union => {
            Some(Namespace::Types)
        }
        ItemTag::MacroDefinition => Some(Namespace::Macros),
        ItemTag::AsmExpr
        | ItemTag::AssociatedConst
        | ItemTag::AssociatedFunction
        | ItemTag::AssociatedTypeAlias
        | ItemTag::ExternBlock
        | ItemTag::ExternCrate
        | ItemTag::Impl
        | ItemTag::Module
        | ItemTag::Use => None,
    }
}
