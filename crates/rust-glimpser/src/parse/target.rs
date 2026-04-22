use crate::parse::file::FileId;

/// Stable identifier of a target within one parsed package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TargetId(pub usize);

/// Parsed target metadata.
///
/// A single Cargo package may define multiple targets, such as `lib.rs`,
/// `main.rs`, examples, or tests. This phase only records the target identity
/// and its root source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// Stable target id assigned during package parsing.
    pub id: TargetId,
    /// `cargo metadata` description of the target.
    pub cargo_target: cargo_metadata::Target,
    /// Entrypoint file id for this target.
    pub root_file: FileId,
}
