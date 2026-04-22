use ra_syntax::ast::{self, HasName};

use crate::parse::item::VisibilityLevel;

use super::ModuleId;

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
    pub(super) fn binding_name(&self) -> Option<String> {
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
    pub(super) fn from_rename(rename: Option<ast::Rename>) -> Self {
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

    pub(super) fn resolve(&self, inferred_name: Option<String>) -> Option<String> {
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
    pub(super) fn empty() -> Self {
        Self {
            absolute: false,
            segments: Vec::new(),
        }
    }

    pub(super) fn joined(&self, suffix: &Self) -> Self {
        let mut segments = self.segments.clone();
        segments.extend(suffix.segments.clone());
        Self {
            absolute: self.absolute || suffix.absolute,
            segments,
        }
    }

    pub(super) fn without_trailing_self(&self) -> Self {
        let mut segments = self.segments.clone();
        if matches!(segments.last(), Some(PathSegment::SelfKw)) {
            segments.pop();
        }
        Self {
            absolute: self.absolute,
            segments,
        }
    }

    pub(super) fn ends_with_self(&self) -> bool {
        matches!(self.segments.last(), Some(PathSegment::SelfKw))
    }

    pub(super) fn last_name(&self) -> Option<String> {
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
