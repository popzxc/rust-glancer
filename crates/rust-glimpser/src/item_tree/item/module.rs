use crate::parse::FileId;

/// Syntactic module facts attached to `ItemKind::Module`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleItem {
    pub source: ModuleSource,
}

/// How a module declaration obtains its item list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleSource {
    Inline,
    OutOfLine { definition_file: Option<FileId> },
}
