use anyhow::Context as _;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use ra_syntax::{Edition, SourceFile};

use crate::{
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
    file_id: FileId,
    data: &'a ParsedFileData,
}

impl<'a> ParsedFile<'a> {
    fn new(file_id: FileId, data: &'a ParsedFileData) -> Self {
        Self { file_id, data }
    }

    /// Returns the stable package-local id for this parsed source file.
    pub fn file_id(&self) -> FileId {
        self.file_id
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
pub(super) struct FileDb {
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

        let file_id = FileId(self.parsed_files.len());
        let source = Self::read_source(&canonical_file_path)?;

        self.parsed_files.push(Self::parse_source(
            file_id,
            canonical_file_path.clone(),
            &source,
        ));
        self.file_ids_by_path.insert(canonical_file_path, file_id);

        Ok(file_id)
    }

    /// Reparses an already known file from the saved filesystem snapshot.
    pub(super) fn reparse_file_from_disk(
        &mut self,
        file_path: &Path,
    ) -> anyhow::Result<Option<FileId>> {
        let Some(file_id) = self.file_ids_by_path.get(file_path).copied() else {
            return Ok(None);
        };

        let source = Self::read_source(file_path)?;
        self.parsed_files[file_id.0] =
            Self::parse_source(file_id, file_path.to_path_buf(), &source);
        Ok(Some(file_id))
    }

    /// Returns the cached parsed file for a previously known `FileId`.
    pub(super) fn parsed_file(&self, file_id: FileId) -> Option<ParsedFile<'_>> {
        self.parsed_files
            .get(file_id.0)
            .map(|data| ParsedFile::new(file_id, data))
    }

    /// Returns all cached parsed files.
    pub(super) fn parsed_files(&self) -> impl Iterator<Item = ParsedFile<'_>> {
        self.parsed_files
            .iter()
            .enumerate()
            .map(|(idx, data)| ParsedFile::new(FileId(idx), data))
    }

    /// Returns the canonical path associated with `file_id`.
    pub(super) fn file_path(&self, file_id: FileId) -> Option<&Path> {
        self.parsed_files
            .get(file_id.0)
            .map(|parsed_file| parsed_file.path.as_path())
    }

    fn read_source(file_path: &Path) -> anyhow::Result<String> {
        std::fs::read_to_string(file_path)
            .with_context(|| format!("while attempting to read {}", file_path.display()))
    }

    fn parse_source(file_id: FileId, path: PathBuf, source: &str) -> ParsedFileData {
        let line_index = LineIndex::new(source);
        let parsed_file = SourceFile::parse(source, Edition::CURRENT);
        let parse_errors = parsed_file
            .errors()
            .into_iter()
            .map(|error| ParseError {
                file_id,
                message: error.to_string(),
                span: Span::from_text_range(error.range(), &line_index),
            })
            .collect();

        ParsedFileData {
            path,
            parse_errors,
            line_index,
            tree: parsed_file.tree(),
        }
    }
}
