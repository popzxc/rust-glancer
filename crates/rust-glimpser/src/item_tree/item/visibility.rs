use std::fmt;

use ra_syntax::{AstNode as _, ast};

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
