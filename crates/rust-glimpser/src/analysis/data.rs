use crate::{
    body_ir::{
        BindingData, BindingId, BodyItemId, BodyItemKind, BodyItemRef, BodyRef, ExprId, ScopeId,
    },
    def_map::{DefId, LocalDefKind, ModuleRef, Path},
    parse::{FileId, span::Span},
    semantic_ir::{FieldRef, FunctionRef, TypePathContext},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SourceNodeAt {
    pub(super) body: BodyRef,
    pub(super) expr: Option<ExprId>,
    pub(super) binding: Option<BindingId>,
    pub(super) local_item: Option<BodyItemId>,
}

pub(super) struct SymbolCandidate {
    pub(super) symbol: SymbolAt,
    pub(super) span: Span,
}

/// Symbol found at one source offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SymbolAt {
    Body {
        body: BodyRef,
    },
    Binding {
        body: BodyRef,
        binding: BindingId,
    },
    BodyPath {
        body: BodyRef,
        scope: ScopeId,
        path: Path,
        span: Span,
    },
    Def {
        def: DefId,
        span: Span,
    },
    Expr {
        body: BodyRef,
        expr: ExprId,
    },
    Field {
        field: FieldRef,
        span: Span,
    },
    Function {
        function: FunctionRef,
        span: Span,
    },
    LocalItem {
        item: BodyItemRef,
        span: Span,
    },
    TypePath {
        context: TypePathContext,
        path: Path,
        span: Span,
    },
    UsePath {
        module: ModuleRef,
        path: Path,
        span: Span,
    },
}

/// One goto-definition destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NavigationTarget {
    pub(crate) kind: NavigationTargetKind,
    pub(crate) name: String,
    pub(crate) file_id: FileId,
    pub(crate) span: Option<Span>,
}

impl NavigationTarget {
    pub(super) fn from_binding(binding: &BindingData) -> Self {
        Self {
            kind: NavigationTargetKind::LocalBinding,
            name: binding
                .name
                .clone()
                .unwrap_or_else(|| "<unsupported>".to_string()),
            file_id: binding.source.file_id,
            span: Some(binding.source.span),
        }
    }
}

/// Navigation target category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, derive_more::Display)]
pub(crate) enum NavigationTargetKind {
    #[display("local")]
    LocalBinding,
    #[display("module")]
    Module,
    #[display("const")]
    Const,
    #[display("enum")]
    Enum,
    #[display("field")]
    Field,
    #[display("fn")]
    Function,
    #[display("macro")]
    Macro,
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

impl NavigationTargetKind {
    pub(super) fn from_local_def_kind(kind: LocalDefKind) -> Self {
        match kind {
            LocalDefKind::Const => Self::Const,
            LocalDefKind::Enum => Self::Enum,
            LocalDefKind::Function => Self::Function,
            LocalDefKind::MacroDefinition => Self::Macro,
            LocalDefKind::Static => Self::Static,
            LocalDefKind::Struct => Self::Struct,
            LocalDefKind::Trait => Self::Trait,
            LocalDefKind::TypeAlias => Self::TypeAlias,
            LocalDefKind::Union => Self::Union,
        }
    }

    pub(super) fn from_body_item_kind(kind: BodyItemKind) -> Self {
        match kind {
            BodyItemKind::Struct => Self::Struct,
        }
    }
}

/// One completion item produced from the current frozen analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompletionItem {
    pub(crate) label: String,
    pub(crate) kind: CompletionKind,
    pub(crate) target: CompletionTarget,
}

/// Stable analysis identity behind one completion row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompletionTarget {
    Field(FieldRef),
    Function(FunctionRef),
}

/// Completion source category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, derive_more::Display)]
pub(crate) enum CompletionKind {
    #[display("field")]
    Field,
    #[display("inherent_method")]
    InherentMethod,
    #[display("trait_method")]
    TraitMethod,
}
