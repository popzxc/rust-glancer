use std::fmt;

use crate::item_tree::{TypePath, TypeRef, UsePath, UsePathSegment, UsePathSegmentKind};

/// Structured path used by def-map path resolution queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    pub absolute: bool,
    pub segments: Vec<PathSegment>,
}

impl Path {
    pub(crate) fn from_type_ref(ty: &TypeRef) -> Option<Self> {
        let TypeRef::Path(path) = ty else {
            return None;
        };

        Some(Self::from_type_path(path))
    }

    pub(crate) fn from_type_path(path: &TypePath) -> Self {
        Self {
            absolute: path.absolute,
            segments: path
                .segments
                .iter()
                .map(|segment| PathSegment::from_type_segment_name(&segment.name))
                .collect(),
        }
    }

    pub(crate) fn from_type_path_prefix(path: &TypePath, end_idx: usize) -> Self {
        Self {
            absolute: path.absolute,
            segments: path
                .segments
                .iter()
                .take(end_idx.saturating_add(1))
                .map(|segment| PathSegment::from_type_segment_name(&segment.name))
                .collect(),
        }
    }

    pub(crate) fn from_use_path(path: &UsePath) -> Self {
        Self {
            absolute: path.absolute,
            segments: path
                .segments
                .iter()
                .map(PathSegment::from_use_segment)
                .collect(),
        }
    }

    pub(crate) fn from_use_path_prefix(path: &UsePath, end_idx: usize) -> Self {
        Self {
            absolute: path.absolute,
            segments: path
                .segments
                .iter()
                .take(end_idx.saturating_add(1))
                .map(PathSegment::from_use_segment)
                .collect(),
        }
    }

    /// Returns the name of a single-segment relative path that can participate in local lookup.
    pub(crate) fn single_name(&self) -> Option<&str> {
        if self.absolute || self.segments.len() != 1 {
            return None;
        }

        match self.segments.first()? {
            PathSegment::Name(name) => Some(name),
            PathSegment::SelfKw => Some("self"),
            PathSegment::SuperKw | PathSegment::CrateKw => None,
        }
    }

    pub(crate) fn is_self_type(&self) -> bool {
        self.is_plain_ident("Self")
    }

    pub(crate) fn is_plain_ident(&self, ident: &str) -> bool {
        !self.absolute
            && self.segments.len() == 1
            && matches!(self.segments.first(), Some(PathSegment::Name(name)) if name == ident)
    }
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
    fn from_type_segment_name(name: &str) -> Self {
        match name {
            "self" => Self::SelfKw,
            "super" => Self::SuperKw,
            "crate" => Self::CrateKw,
            name => Self::Name(name.to_string()),
        }
    }

    fn from_use_segment(segment: &UsePathSegment) -> Self {
        match &segment.kind {
            UsePathSegmentKind::Name(name) => Self::Name(name.clone()),
            UsePathSegmentKind::SelfKw => Self::SelfKw,
            UsePathSegmentKind::SuperKw => Self::SuperKw,
            UsePathSegmentKind::CrateKw => Self::CrateKw,
        }
    }
}

pub(super) fn last_segment_name(segments: &[PathSegment]) -> Option<String> {
    match segments.last()? {
        PathSegment::Name(name) => Some(name.clone()),
        PathSegment::SelfKw => Some("self".to_string()),
        PathSegment::SuperKw => Some("super".to_string()),
        PathSegment::CrateKw => Some("crate".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        item_tree::{
            TypePath, TypePathSegment, TypeRef, UsePath, UsePathSegment, UsePathSegmentKind,
        },
        parse::span::{LineColumnSpan, Position, Span, TextSpan},
    };

    use super::{Path, PathSegment};

    #[test]
    fn builds_paths_from_type_paths() {
        let cases = [
            (
                "relative keywords and names",
                type_path(false, &["crate", "super", "self", "User", "Self"]),
                "crate::super::self::User::Self",
            ),
            (
                "absolute path",
                type_path(true, &["api", "User"]),
                "::api::User",
            ),
        ];

        for (label, path, expected) in cases {
            assert_eq!(Path::from_type_path(&path).to_string(), expected, "{label}");
        }
    }

    #[test]
    fn builds_paths_from_type_refs() {
        let cases = [
            (
                "path type",
                TypeRef::Path(type_path(false, &["User"])),
                Some("User"),
            ),
            ("non-path type", TypeRef::Infer, None),
        ];

        for (label, ty, expected) in cases {
            let actual = Path::from_type_ref(&ty).map(|path| path.to_string());
            assert_eq!(actual.as_deref(), expected, "{label}");
        }
    }

    #[test]
    fn builds_paths_from_use_paths() {
        let cases = [
            (
                "relative keywords and names",
                use_path(
                    false,
                    &[
                        UsePathSegmentKind::CrateKw,
                        UsePathSegmentKind::SuperKw,
                        UsePathSegmentKind::SelfKw,
                        UsePathSegmentKind::Name("User".to_string()),
                    ],
                ),
                "crate::super::self::User",
            ),
            (
                "absolute path",
                use_path(
                    true,
                    &[
                        UsePathSegmentKind::Name("api".to_string()),
                        UsePathSegmentKind::Name("User".to_string()),
                    ],
                ),
                "::api::User",
            ),
        ];

        for (label, path, expected) in cases {
            assert_eq!(Path::from_use_path(&path).to_string(), expected, "{label}");
        }
    }

    #[test]
    fn builds_prefix_paths() {
        let type_path = type_path(false, &["api", "User", "Id"]);
        let use_path = use_path(
            true,
            &[
                UsePathSegmentKind::Name("api".to_string()),
                UsePathSegmentKind::Name("User".to_string()),
                UsePathSegmentKind::Name("Id".to_string()),
            ],
        );

        assert_eq!(
            Path::from_type_path_prefix(&type_path, 1).to_string(),
            "api::User"
        );
        assert_eq!(
            Path::from_use_path_prefix(&use_path, 1).to_string(),
            "::api::User"
        );
    }

    #[test]
    fn classifies_single_segment_paths() {
        let cases = [
            (
                "plain name",
                path(false, vec![PathSegment::Name("User".to_string())]),
                Some("User"),
            ),
            (
                "self keyword",
                path(false, vec![PathSegment::SelfKw]),
                Some("self"),
            ),
            (
                "super keyword",
                path(false, vec![PathSegment::SuperKw]),
                None,
            ),
            (
                "crate keyword",
                path(false, vec![PathSegment::CrateKw]),
                None,
            ),
            (
                "absolute name",
                path(true, vec![PathSegment::Name("User".to_string())]),
                None,
            ),
            (
                "qualified name",
                path(
                    false,
                    vec![
                        PathSegment::Name("api".to_string()),
                        PathSegment::Name("User".to_string()),
                    ],
                ),
                None,
            ),
        ];

        for (label, path, expected) in cases {
            assert_eq!(path.single_name(), expected, "{label}");
        }
    }

    #[test]
    fn classifies_plain_identifier_paths() {
        let cases = [
            (
                "Self type",
                path(false, vec![PathSegment::Name("Self".to_string())]),
                false,
                true,
                true,
            ),
            (
                "self keyword",
                path(false, vec![PathSegment::SelfKw]),
                false,
                false,
                false,
            ),
            (
                "other plain ident",
                path(false, vec![PathSegment::Name("User".to_string())]),
                true,
                false,
                false,
            ),
            (
                "absolute Self",
                path(true, vec![PathSegment::Name("Self".to_string())]),
                false,
                false,
                false,
            ),
        ];

        for (label, path, is_user, is_self_ident, is_self_type) in cases {
            assert_eq!(path.is_plain_ident("User"), is_user, "{label}");
            assert_eq!(path.is_plain_ident("Self"), is_self_ident, "{label}");
            assert_eq!(path.is_self_type(), is_self_type, "{label}");
        }
    }

    fn type_path(absolute: bool, names: &[&str]) -> TypePath {
        TypePath {
            absolute,
            segments: names
                .iter()
                .map(|name| TypePathSegment {
                    name: (*name).to_string(),
                    args: Vec::new(),
                    span: span(),
                })
                .collect(),
        }
    }

    fn use_path(absolute: bool, kinds: &[UsePathSegmentKind]) -> UsePath {
        UsePath {
            absolute,
            segments: kinds
                .iter()
                .cloned()
                .map(|kind| UsePathSegment { kind, span: span() })
                .collect(),
        }
    }

    fn path(absolute: bool, segments: Vec<PathSegment>) -> Path {
        Path { absolute, segments }
    }

    fn span() -> Span {
        Span {
            text: TextSpan { start: 0, end: 0 },
            line_column: LineColumnSpan {
                start: Position { line: 0, column: 0 },
                end: Position { line: 0, column: 0 },
            },
        }
    }
}
