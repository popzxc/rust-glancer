use std::fmt;

use crate::item_tree::{ImportAlias, UseImportKind, UsePath, UsePathSegment, VisibilityLevel};

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
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum ImportBinding {
    #[display("")]
    Inferred,
    #[display(" as {_0}")]
    Explicit(String),
    #[display(" as _")]
    Hidden,
}

impl ImportBinding {
    pub(super) fn from_alias(alias: &ImportAlias) -> Self {
        match alias {
            ImportAlias::Inferred => Self::Inferred,
            ImportAlias::Explicit(name) => Self::Explicit(name.clone()),
            ImportAlias::Hidden => Self::Hidden,
        }
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

impl ImportKind {
    pub(super) fn from_use_kind(kind: UseImportKind) -> Self {
        match kind {
            UseImportKind::Named => Self::Named,
            UseImportKind::SelfImport => Self::SelfImport,
            UseImportKind::Glob => Self::Glob,
        }
    }
}

/// Structured path used by def-map path resolution queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    pub absolute: bool,
    pub segments: Vec<PathSegment>,
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.absolute {
            write!(f, "::")?;
        }

        for (idx, segment) in self.segments.iter().enumerate() {
            if idx > 0 {
                write!(f, "::")?;
            }
            write!(f, "{segment}")?;
        }

        Ok(())
    }
}

/// Structured path used during import resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPath {
    pub absolute: bool,
    pub segments: Vec<PathSegment>,
}

impl ImportPath {
    pub(super) fn from_use_path(path: &UsePath) -> Self {
        Self {
            absolute: path.absolute,
            segments: path
                .segments
                .iter()
                .map(PathSegment::from_use_segment)
                .collect(),
        }
    }

    pub(super) fn last_name(&self) -> Option<String> {
        last_segment_name(&self.segments)
    }
}

impl From<&ImportPath> for Path {
    fn from(path: &ImportPath) -> Self {
        Self {
            absolute: path.absolute,
            segments: path.segments.clone(),
        }
    }
}

impl fmt::Display for ImportPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Path::from(self).fmt(f)
    }
}

/// One structured path segment.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum PathSegment {
    #[display("{_0}")]
    Name(String),
    #[display("self")]
    SelfKw,
    #[display("super")]
    SuperKw,
    #[display("crate")]
    CrateKw,
}

impl PathSegment {
    fn from_use_segment(segment: &UsePathSegment) -> Self {
        match segment {
            UsePathSegment::Name(name) => Self::Name(name.clone()),
            UsePathSegment::SelfKw => Self::SelfKw,
            UsePathSegment::SuperKw => Self::SuperKw,
            UsePathSegment::CrateKw => Self::CrateKw,
        }
    }
}

fn last_segment_name(segments: &[PathSegment]) -> Option<String> {
    match segments.last()? {
        PathSegment::Name(name) => Some(name.clone()),
        PathSegment::SelfKw => Some("self".to_string()),
        PathSegment::SuperKw => Some("super".to_string()),
        PathSegment::CrateKw => Some("crate".to_string()),
    }
}
