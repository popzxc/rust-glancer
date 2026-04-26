use std::path::PathBuf;

use crate::{parse::file::FileId, workspace_metadata::TargetKind};

/// Stable identifier of a target within one parsed package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TargetId(pub usize);

/// Parsed target metadata.
///
/// A single package may define multiple targets, such as `lib.rs`, `main.rs`, examples, or tests.
/// This phase keeps only the normalized target identity and its parsed root source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// Stable target id assigned during package parsing.
    pub id: TargetId,
    /// Normalized target name.
    pub name: String,
    /// Normalized target kind.
    pub kind: TargetKind,
    /// Target entrypoint path from workspace metadata.
    pub src_path: PathBuf,
    /// Entrypoint file id for this target.
    pub root_file: FileId,
}
