use anyhow::Context as _;
use ra_syntax::{Edition, SourceFile};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::item_tree::{
    error::ParseError,
    span::{LineIndex, Span},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(pub usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRecord {
    pub id: FileId,
    pub path: PathBuf,
    pub parse_errors: Vec<ParseError>,
}

pub(crate) struct ParsedFile {
    pub(crate) record: FileRecord,
    pub(crate) line_index: LineIndex,
    pub(crate) tree: SourceFile,
}

#[derive(Default)]
pub(crate) struct ParseDb {
    parsed_files: Vec<ParsedFile>,
    file_ids_by_path: HashMap<PathBuf, FileId>,
}

impl ParseDb {
    pub(crate) fn get_or_parse_file(&mut self, file_path: &Path) -> anyhow::Result<FileId> {
        let canonical_file_path = file_path
            .canonicalize()
            .with_context(|| format!("while attempting to canonicalize {}", file_path.display()))?;

        if let Some(file_id) = self.file_ids_by_path.get(&canonical_file_path) {
            return Ok(*file_id);
        }

        let source = std::fs::read_to_string(&canonical_file_path).with_context(|| {
            format!("while attempting to read {}", canonical_file_path.display())
        })?;
        let line_index = LineIndex::new(&source);
        let parsed_file = SourceFile::parse(&source, Edition::CURRENT);

        let file_id = FileId(self.parsed_files.len());
        let parse_errors = parsed_file
            .errors()
            .into_iter()
            .map(|error| ParseError {
                file_id,
                message: error.to_string(),
                span: Span::from_text_range(error.range(), &line_index),
            })
            .collect();

        let record = FileRecord {
            id: file_id,
            path: canonical_file_path.clone(),
            parse_errors,
        };
        self.parsed_files.push(ParsedFile {
            record,
            line_index,
            tree: parsed_file.tree(),
        });
        self.file_ids_by_path.insert(canonical_file_path, file_id);

        Ok(file_id)
    }

    pub(crate) fn parsed_file(&self, file_id: FileId) -> anyhow::Result<&ParsedFile> {
        self.parsed_files
            .get(file_id.0)
            .with_context(|| format!("while attempting to look up parsed file {:?}", file_id))
    }

    pub(crate) fn file_path(&self, file_id: FileId) -> anyhow::Result<&Path> {
        self.parsed_files
            .get(file_id.0)
            .map(|parsed_file| parsed_file.record.path.as_path())
            .with_context(|| format!("while attempting to look up path for file {:?}", file_id))
    }

    pub(crate) fn into_file_records(self) -> Vec<FileRecord> {
        self.parsed_files
            .into_iter()
            .map(|parsed_file| parsed_file.record)
            .collect()
    }
}
