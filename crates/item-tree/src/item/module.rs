use rg_parse::FileId;

use super::ItemTreeId;

/// Syntactic module facts attached to `ItemKind::Module`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleItem {
    pub source: ModuleSource,
}

/// How a module declaration obtains its item list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleSource {
    Inline { items: Vec<ItemTreeId> },
    OutOfLine { definition_file: Option<FileId> },
}
