use std::path::PathBuf;

use crate::item_tree::error::ParseError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(pub usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRecord {
    pub id: FileId,
    pub path: PathBuf,
    pub parse_errors: Vec<ParseError>,
}
