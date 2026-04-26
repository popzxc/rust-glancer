use anyhow::Context as _;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use ra_syntax::{Edition, SourceFile};

use crate::parse::{
    error::ParseError,
    span::{LineIndex, Span},
};

/// Stable identifier for a parsed source file inside `FileDb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(pub usize);

/// Internal parsed representation used by the parser cache.
#[derive(Debug, Clone)]
struct ParsedFileData {
    /// Canonical filesystem path for this source file.
    path: PathBuf,
    /// Parse diagnostics produced while parsing the file.
    parse_errors: Vec<ParseError>,
    /// Line-start index used to convert byte offsets into line/column coordinates.
    line_index: LineIndex,
    /// Parsed Rust syntax tree produced by `ra_syntax`.
    tree: SourceFile,
}

/// Borrowed view over one cached source file.
///
/// Later phases need syntax and source coordinates, but they should not know that parsing is backed
/// by a mutable file cache. This view is the stable boundary between `parse` and AST-consuming
/// phases.
#[derive(Debug, Clone, Copy)]
pub struct ParsedFile<'a> {
    data: &'a ParsedFileData,
}

impl<'a> ParsedFile<'a> {
    fn new(data: &'a ParsedFileData) -> Self {
        Self { data }
    }

    /// Returns the canonical path for this parsed source file.
    pub fn path(&self) -> &'a Path {
        self.data.path.as_path()
    }

    /// Returns parser diagnostics produced for this source file.
    pub fn parse_errors(&self) -> &'a [ParseError] {
        &self.data.parse_errors
    }

    /// Returns the line index used for byte-offset to line/column conversion.
    pub fn line_index(&self) -> &'a LineIndex {
        &self.data.line_index
    }

    /// Returns the parsed Rust syntax tree.
    pub fn syntax(&self) -> &'a SourceFile {
        &self.data.tree
    }
}

/// Shared parse cache that owns filesystem-backed source files and syntax trees.
///
/// `FileDb` deduplicates parsing across targets, so shared modules are parsed once
/// and reused during multiple target traversals.
#[derive(Default, Debug, Clone)]
pub struct FileDb {
    parsed_files: Vec<ParsedFileData>,
    file_ids_by_path: HashMap<PathBuf, FileId>,
}

impl FileDb {
    /// Returns an existing `FileId` for `file_path` or parses and caches the file.
    pub(super) fn get_or_parse_file(&mut self, file_path: &Path) -> anyhow::Result<FileId> {
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

        self.parsed_files.push(ParsedFileData {
            path: canonical_file_path.clone(),
            parse_errors,
            line_index,
            tree: parsed_file.tree(),
        });
        self.file_ids_by_path.insert(canonical_file_path, file_id);

        Ok(file_id)
    }

    /// Returns the cached parsed file for a previously known `FileId`.
    pub(super) fn parsed_file(&self, file_id: FileId) -> Option<ParsedFile<'_>> {
        self.parsed_files.get(file_id.0).map(ParsedFile::new)
    }

    /// Returns all cached parsed files.
    pub(super) fn parsed_files(&self) -> impl Iterator<Item = ParsedFile<'_>> {
        self.parsed_files.iter().map(ParsedFile::new)
    }

    /// Returns the canonical path associated with `file_id`.
    pub(super) fn file_path(&self, file_id: FileId) -> Option<&Path> {
        self.parsed_files
            .get(file_id.0)
            .map(|parsed_file| parsed_file.path.as_path())
    }
}
