use ra_syntax::TextRange;

/// Span representation that keeps both byte offsets and line/column coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Span in UTF-8 byte offsets from the beginning of the file.
    pub text: TextSpan,
    /// Span in zero-based line and column coordinates.
    pub line_column: LineColumnSpan,
}

/// A half-open byte-offset range within a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSpan {
    pub start: u32,
    pub end: u32,
}

/// A half-open line/column range within a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineColumnSpan {
    pub start: Position,
    pub end: Position,
}

/// A zero-based line/column coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

impl Span {
    /// Converts a syntax-level text range into the internal span representation.
    pub(crate) fn from_text_range(text_range: TextRange, line_index: &LineIndex) -> Self {
        let start = u32::from(text_range.start());
        let end = u32::from(text_range.end());

        Self {
            text: TextSpan { start, end },
            line_column: LineColumnSpan {
                start: line_index.position(start),
                end: line_index.position(end),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    /// Builds a fast line-start index for repeated offset-to-position lookups.
    pub(crate) fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (idx, byte) in source.as_bytes().iter().enumerate() {
            if *byte == b'\n' {
                line_starts.push(idx + 1);
            }
        }
        Self { line_starts }
    }

    /// Converts a byte offset into a zero-based line/column position.
    pub(crate) fn position(&self, offset: u32) -> Position {
        let offset = usize::try_from(offset).expect("offset should fit into usize");
        let line_index = match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(line) => line.saturating_sub(1),
        };
        let line_start = self.line_starts[line_index];
        let column = offset.saturating_sub(line_start);

        Position {
            line: u32::try_from(line_index).expect("line index should fit into u32"),
            column: u32::try_from(column).expect("column should fit into u32"),
        }
    }
}
