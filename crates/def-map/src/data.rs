use std::collections::HashMap;

use rg_item_tree::{ItemTag, ItemTreeRef, VisibilityLevel};
use rg_parse::{FileId, Span};

use super::{DefId, ImportData, ImportId, LocalDefId, LocalImplId, ModuleId, ModuleRef};

/// Frozen namespace map for one analyzed target.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DefMap {
    root_module: Option<ModuleId>,
    // Currently means “implicit roots visible to this target,” including sibling lib roots
    extern_prelude: HashMap<String, ModuleRef>,
    // Standard prelude module selected for this target, if sysroot sources are available.
    prelude: Option<ModuleRef>,
    pub modules: Vec<ModuleData>,
    pub local_defs: Vec<LocalDefData>,
    pub local_impls: Vec<LocalImplData>,
    pub imports: Vec<ImportData>,
}

impl DefMap {
    /// Returns the root module of this target, if the map has been populated.
    pub fn root_module(&self) -> Option<ModuleId> {
        self.root_module
    }

    /// Returns the external root names visible from this target.
    pub fn extern_prelude(&self) -> &HashMap<String, ModuleRef> {
        &self.extern_prelude
    }

    /// Returns the standard prelude module visible from this target, if it was discovered.
    pub fn prelude(&self) -> Option<ModuleRef> {
        self.prelude
    }

    /// Returns all modules in stable module-id order.
    pub fn modules(&self) -> &[ModuleData] {
        &self.modules
    }

    /// Returns module data by id.
    pub fn module(&self, module_id: ModuleId) -> Option<&ModuleData> {
        self.modules.get(module_id.0)
    }

    /// Returns local definition data by id.
    pub fn local_def(&self, local_def: LocalDefId) -> Option<&LocalDefData> {
        self.local_defs.get(local_def.0)
    }

    /// Returns all local definitions in stable local-def-id order.
    pub fn local_defs(&self) -> &[LocalDefData] {
        &self.local_defs
    }

    /// Returns impl block data by id.
    #[allow(dead_code)]
    pub fn local_impl(&self, local_impl: LocalImplId) -> Option<&LocalImplData> {
        self.local_impls.get(local_impl.0)
    }

    /// Returns all impl blocks in stable local-impl-id order.
    pub fn local_impls(&self) -> &[LocalImplData] {
        &self.local_impls
    }

    /// Returns import data by id.
    #[allow(dead_code)]
    pub fn import(&self, import: ImportId) -> Option<&ImportData> {
        self.imports.get(import.0)
    }

    /// Returns all imports in stable import-id order.
    pub fn imports(&self) -> &[ImportData] {
        &self.imports
    }

    pub(super) fn set_root_module(&mut self, root_module: ModuleId) {
        self.root_module = Some(root_module);
    }

    pub(super) fn set_extern_prelude(&mut self, extern_prelude: HashMap<String, ModuleRef>) {
        self.extern_prelude = extern_prelude;
    }

    pub(super) fn set_prelude(&mut self, prelude: Option<ModuleRef>) {
        self.prelude = prelude;
    }
}

/// One module in the frozen namespace graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleData {
    pub name: Option<String>,
    pub name_span: Option<Span>,
    pub parent: Option<ModuleId>,
    pub children: Vec<(String, ModuleId)>,
    pub local_defs: Vec<LocalDefId>,
    pub impls: Vec<LocalImplId>,
    pub imports: Vec<ImportId>,
    pub unresolved_imports: Vec<ImportId>,
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

impl ModuleOrigin {
    /// Returns whether this module's source touches the requested file.
    pub fn contains_file(&self, file_id: FileId) -> bool {
        match self {
            Self::Root { file_id: root_file } => *root_file == file_id,
            Self::Inline {
                declaration_file, ..
            } => *declaration_file == file_id,
            Self::OutOfLine {
                declaration_file,
                definition_file,
                ..
            } => *declaration_file == file_id || *definition_file == Some(file_id),
        }
    }
}

/// One module-scope definition collected from source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDefData {
    pub module: ModuleId,
    pub name: String,
    pub kind: LocalDefKind,
    pub visibility: VisibilityLevel,
    pub source: ItemTreeRef,
    pub file_id: FileId,
    pub name_span: Option<Span>,
    pub span: Span,
}

/// One module-owned impl block collected from source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalImplData {
    pub module: ModuleId,
    pub source: ItemTreeRef,
    pub file_id: FileId,
    pub span: Span,
}

/// Module-scope definition kind that participates in def-map namespaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum LocalDefKind {
    #[display("const")]
    Const,
    #[display("enum")]
    Enum,
    #[display("fn")]
    Function,
    #[display("macro_definition")]
    MacroDefinition,
    #[display("static")]
    Static,
    #[display("struct")]
    Struct,
    #[display("trait")]
    Trait,
    #[display("type_alias")]
    TypeAlias,
    #[display("union")]
    Union,
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
        owner: ModuleRef,
    ) {
        for binding in &entry.types {
            self.insert_binding(
                name,
                Namespace::Types,
                ScopeBinding {
                    def: binding.def,
                    visibility: visibility.clone(),
                    owner,
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
                    owner,
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
                    owner,
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
    pub owner: ModuleRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Namespace {
    Types,
    Values,
    Macros,
}

impl LocalDefKind {
    pub(super) fn from_item_tag(tag: ItemTag) -> Option<Self> {
        match tag {
            ItemTag::Const => Some(Self::Const),
            ItemTag::Enum => Some(Self::Enum),
            ItemTag::Function => Some(Self::Function),
            ItemTag::MacroDefinition => Some(Self::MacroDefinition),
            ItemTag::Static => Some(Self::Static),
            ItemTag::Struct => Some(Self::Struct),
            ItemTag::Trait => Some(Self::Trait),
            ItemTag::TypeAlias => Some(Self::TypeAlias),
            ItemTag::Union => Some(Self::Union),
            ItemTag::AsmExpr
            | ItemTag::ExternBlock
            | ItemTag::ExternCrate
            | ItemTag::Impl
            | ItemTag::Module
            | ItemTag::Use => None,
        }
    }

    pub(super) fn namespace(self) -> Namespace {
        match self {
            Self::Const | Self::Function | Self::Static => Namespace::Values,
            Self::Enum | Self::Struct | Self::Trait | Self::TypeAlias | Self::Union => {
                Namespace::Types
            }
            Self::MacroDefinition => Namespace::Macros,
        }
    }
}
