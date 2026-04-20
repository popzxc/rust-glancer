use anyhow::Context as _;
use ra_syntax::{Edition, SourceFile};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::parse::{
    error::ParseError,
    span::{LineIndex, Span},
};

/// Stable identifier for a parsed source file inside `ParseDb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(pub usize);

/// Internal parsed representation used by the parser cache.
#[derive(Debug, Clone)]
pub(crate) struct ParsedFile {
    /// Numeric id assigned by `ParseDb`.
    pub id: FileId, // TODO: maybe remove if not proven useful.
    /// Canonical filesystem path for this source file.
    pub path: PathBuf,
    /// Parse diagnostics produced while parsing the file.
    pub parse_errors: Vec<ParseError>,
    /// Line-start index used to convert byte offsets into line/column coordinates.
    pub(crate) line_index: LineIndex,
    /// Parsed Rust syntax tree produced by `ra_syntax`.
    pub(crate) tree: SourceFile,
}

/// Shared parse cache that owns filesystem-backed source files and syntax trees.
///
/// `ParseDb` deduplicates parsing across targets, so shared modules are parsed once
/// and reused during multiple target traversals.
#[derive(Default, Debug, Clone)]
pub(crate) struct ParseDb {
    // TODO: `pub(crate)` only for tests, should be locked down
    pub(crate) parsed_files: Vec<ParsedFile>,
    file_ids_by_path: HashMap<PathBuf, FileId>,
}

impl ParseDb {
    /// Returns an existing `FileId` for `file_path` or parses and caches the file.
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

        self.parsed_files.push(ParsedFile {
            id: file_id,
            path: canonical_file_path.clone(),
            parse_errors,
            line_index,
            tree: parsed_file.tree(),
        });
        self.file_ids_by_path.insert(canonical_file_path, file_id);

        Ok(file_id)
    }

    /// Returns the cached parsed file for a previously known `FileId`.
    pub(crate) fn parsed_file(&self, file_id: FileId) -> Option<&ParsedFile> {
        self.parsed_files.get(file_id.0)
    }

    /// Returns the canonical path associated with `file_id`.
    pub(crate) fn file_path(&self, file_id: FileId) -> Option<&Path> {
        self.parsed_files
            .get(file_id.0)
            .map(|parsed_file| parsed_file.path.as_path())
    }
}
