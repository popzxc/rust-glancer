use std::fmt;

use ra_syntax::ast::{self, HasName};

/// Syntactic `extern crate` facts attached to `ItemKind::ExternCrate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternCrateItem {
    pub name: Option<String>,
    pub alias: ImportAlias,
}

impl ExternCrateItem {
    pub(crate) fn from_ast(item: &ast::ExternCrate) -> Self {
        Self {
            name: item.name_ref().map(|name_ref| name_ref.text().to_string()),
            alias: ImportAlias::from_rename(item.rename()),
        }
    }
}

/// Syntactic `use` facts attached to `ItemKind::Use`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseItem {
    pub imports: Vec<UseImport>,
}

impl UseItem {
    pub(crate) fn from_ast(item: &ast::Use) -> Self {
        let mut imports = Vec::new();

        if let Some(use_tree) = item.use_tree() {
            Self::lower_use_tree(&mut imports, &UsePath::empty(), use_tree);
        }

        Self { imports }
    }

    fn lower_use_tree(imports: &mut Vec<UseImport>, prefix: &UsePath, use_tree: ast::UseTree) {
        let path = match use_tree.path() {
            Some(path) => {
                let Some(path) = UsePath::from_ast(&path) else {
                    return;
                };
                prefix.joined(&path)
            }
            None => prefix.clone(),
        };

        if let Some(use_tree_list) = use_tree.use_tree_list() {
            for child_use_tree in use_tree_list.use_trees() {
                Self::lower_use_tree(imports, &path, child_use_tree);
            }
            return;
        }

        let (kind, path) = if use_tree.star_token().is_some() {
            (UseImportKind::Glob, path)
        } else if path.ends_with_self() {
            (UseImportKind::SelfImport, path.without_trailing_self())
        } else {
            (UseImportKind::Named, path)
        };

        imports.push(UseImport {
            kind,
            path,
            alias: ImportAlias::from_rename(use_tree.rename()),
        });
    }
}

/// One leaf import produced by a potentially nested use tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseImport {
    pub kind: UseImportKind,
    pub path: UsePath,
    pub alias: ImportAlias,
}

/// Import form before name resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum UseImportKind {
    #[display("named")]
    Named,
    #[display("self")]
    SelfImport,
    #[display("glob")]
    Glob,
}

/// Explicit import alias, including `as _`.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum ImportAlias {
    #[display("")]
    Inferred,
    #[display(" as {_0}")]
    Explicit(String),
    #[display(" as _")]
    Hidden,
}

impl ImportAlias {
    pub(crate) fn from_rename(rename: Option<ast::Rename>) -> Self {
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
}

/// Structured path used before semantic resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsePath {
    pub absolute: bool,
    pub segments: Vec<UsePathSegment>,
}

impl UsePath {
    fn empty() -> Self {
        Self {
            absolute: false,
            segments: Vec::new(),
        }
    }

    fn from_ast(path: &ast::Path) -> Option<Self> {
        let mut segments = Vec::new();

        for segment in path.segments() {
            let lowered_segment = match segment.kind()? {
                ast::PathSegmentKind::Name(name_ref) => {
                    UsePathSegment::Name(name_ref.text().to_string())
                }
                ast::PathSegmentKind::SelfKw => UsePathSegment::SelfKw,
                ast::PathSegmentKind::SuperKw => UsePathSegment::SuperKw,
                ast::PathSegmentKind::CrateKw => UsePathSegment::CrateKw,
                ast::PathSegmentKind::SelfTypeKw | ast::PathSegmentKind::Type { .. } => {
                    return None;
                }
            };
            segments.push(lowered_segment);
        }

        Some(Self {
            absolute: path
                .first_segment()
                .is_some_and(|segment| segment.coloncolon_token().is_some()),
            segments,
        })
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
        if matches!(segments.last(), Some(UsePathSegment::SelfKw)) {
            segments.pop();
        }
        Self {
            absolute: self.absolute,
            segments,
        }
    }

    fn ends_with_self(&self) -> bool {
        matches!(self.segments.last(), Some(UsePathSegment::SelfKw))
    }
}

impl fmt::Display for UsePath {
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

/// One structured path segment.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum UsePathSegment {
    #[display("{_0}")]
    Name(String),
    #[display("self")]
    SelfKw,
    #[display("super")]
    SuperKw,
    #[display("crate")]
    CrateKw,
}
