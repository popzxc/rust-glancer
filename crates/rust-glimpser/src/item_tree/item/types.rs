//! Types that power `ItemNode` but are not part of it.
// Potentially, as this module grows, we'll need to split it further.

use std::fmt;

use ra_syntax::{AstNode as _, ast};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    AsmExpr,
    AssociatedConst,
    AssociatedFunction,
    AssociatedTypeAlias,
    Const,
    Enum,
    ExternBlock,
    ExternCrate,
    Function,
    Impl,
    MacroDefinition,
    Module,
    Static,
    Struct,
    Trait,
    TypeAlias,
    Union,
    Use,
}

impl fmt::Display for ItemKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            ItemKind::AsmExpr => "asm",
            ItemKind::AssociatedConst => "associated_const",
            ItemKind::AssociatedFunction => "associated_fn",
            ItemKind::AssociatedTypeAlias => "associated_type_alias",
            ItemKind::Const => "const",
            ItemKind::Enum => "enum",
            ItemKind::ExternBlock => "extern_block",
            ItemKind::ExternCrate => "extern_crate",
            ItemKind::Function => "fn",
            ItemKind::Impl => "impl",
            ItemKind::MacroDefinition => "macro_definition",
            ItemKind::Module => "module",
            ItemKind::Static => "static",
            ItemKind::Struct => "struct",
            ItemKind::Trait => "trait",
            ItemKind::TypeAlias => "type_alias",
            ItemKind::Union => "union",
            ItemKind::Use => "use",
        };
        write!(f, "{value}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisibilityLevel {
    Private,
    Public,
    Crate,
    Super,
    Self_,
    Restricted(String),
    Unknown(String),
}

impl fmt::Display for VisibilityLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VisibilityLevel::Private => write!(f, "private"),
            VisibilityLevel::Public => write!(f, "pub"),
            VisibilityLevel::Crate => write!(f, "pub(crate)"),
            VisibilityLevel::Super => write!(f, "pub(super)"),
            VisibilityLevel::Self_ => write!(f, "pub(self)"),
            VisibilityLevel::Restricted(path) => write!(f, "pub(in {path})"),
            VisibilityLevel::Unknown(raw) => write!(f, "{raw}"),
        }
    }
}

impl VisibilityLevel {
    pub(crate) fn from_ast(visibility: Option<ast::Visibility>) -> Self {
        let Some(visibility) = visibility else {
            return Self::Private;
        };

        let Some(inner) = visibility.visibility_inner() else {
            return Self::Public;
        };

        let Some(path) = inner.path() else {
            return Self::Unknown(visibility.syntax().text().to_string());
        };
        let path_text = path.syntax().text().to_string();

        if inner.in_token().is_some() {
            return Self::Restricted(path_text);
        }

        match path_text.as_str() {
            "crate" => Self::Crate,
            "super" => Self::Super,
            "self" => Self::Self_,
            _ => Self::Unknown(visibility.syntax().text().to_string()),
        }
    }
}
