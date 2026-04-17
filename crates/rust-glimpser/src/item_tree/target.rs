use std::{collections::HashSet, path::PathBuf};

use crate::item_tree::{file::FileId, item::ItemNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TargetId(pub usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetInput {
    pub name: String,
    pub kinds: Vec<String>,
    pub root_file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIndex {
    pub id: TargetId,
    pub name: String,
    pub kinds: Vec<String>,
    pub root_file: FileId,
    pub root_items: Vec<ItemNode>,
}

#[derive(Default)]
pub(crate) struct TargetBuildState {
    pub(crate) active_stack: HashSet<FileId>,
}
