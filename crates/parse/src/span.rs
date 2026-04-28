use ra_syntax::TextRange;

/// Span representation that keeps both byte offsets and line/column coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Span in UTF-8 byte offsets from the beginning of the file.
    pub text: TextSpan,
    /// Span in zero-based line and column coordinates.
    pub line_column: LineColumnSpan,
}

impl Span {
    /// Converts a syntax-level text range into the internal span representation.
    pub fn from_text_range(text_range: TextRange, line_index: &LineIndex) -> Self {
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

    /// Returns true when `offset` is inside the half-open text range.
    pub fn contains(self, offset: u32) -> bool {
        self.text.contains(offset)
    }

    /// Returns true when `offset` is inside the text range or exactly at its end.
    pub fn touches(self, offset: u32) -> bool {
        self.text.touches(offset)
    }

    /// Returns the byte length of the text range.
    pub fn len(self) -> u32 {
        self.text.len()
    }
}

/// A half-open byte-offset range within a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSpan {
    pub start: u32,
    pub end: u32,
}

impl TextSpan {
    /// Returns true when `offset` is inside the half-open range: `start <= offset < end`.
    pub fn contains(self, offset: u32) -> bool {
        self.start <= offset && offset < self.end
    }

    /// Returns true when `offset` is inside the range or exactly at its end.
    pub fn touches(self, offset: u32) -> bool {
        self.start <= offset && offset <= self.end
    }

    /// Returns the byte length of the range, saturating if invalid input ever appears.
    pub fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }
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

#[derive(Debug, Clone)]
pub struct LineIndex {
    line_starts: Vec<usize>,
    line_metrics: Vec<LineMetrics>,
}

impl LineIndex {
    /// Builds a fast line-start index for repeated offset-to-position lookups.
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (idx, byte) in source.as_bytes().iter().enumerate() {
            if *byte == b'\n' {
                line_starts.push(idx + 1);
            }
        }

        let line_metrics = line_starts
            .iter()
            .enumerate()
            .map(|(line_idx, line_start)| {
                let next_line_start = line_starts
                    .get(line_idx + 1)
                    .copied()
                    .unwrap_or(source.len());
                let line_end = Self::line_text_end(source.as_bytes(), *line_start, next_line_start);
                LineMetrics::new(&source[*line_start..line_end])
            })
            .collect();

        Self {
            line_starts,
            line_metrics,
        }
    }

    /// Converts a byte offset into a zero-based line/column position.
    pub fn position(&self, offset: u32) -> Position {
        let offset = usize::try_from(offset).expect("offset should fit into usize");
        let line_index = self.line_for_offset(offset);
        let line_start = self.line_starts[line_index];
        let column = offset.saturating_sub(line_start);

        Position {
            line: u32::try_from(line_index).expect("line index should fit into u32"),
            column: u32::try_from(column).expect("column should fit into u32"),
        }
    }

    /// Converts a byte offset into a zero-based line/UTF-16-column position.
    pub fn utf16_position(&self, offset: u32) -> Position {
        let offset = usize::try_from(offset).expect("offset should fit into usize");
        let line_index = self.line_for_offset(offset);
        let line_start = self.line_starts[line_index];
        let byte_column = offset.saturating_sub(line_start);

        Position {
            line: u32::try_from(line_index).expect("line index should fit into u32"),
            column: self.line_metrics[line_index].utf16_column_for_byte(byte_column),
        }
    }

    /// Converts a zero-based line/UTF-16-column position into a byte offset.
    pub fn offset_from_utf16_position(&self, position: Position) -> Option<u32> {
        let line_index = usize::try_from(position.line).ok()?;
        let line_start = *self.line_starts.get(line_index)?;
        let line_metrics = self.line_metrics.get(line_index)?;
        let byte_column = line_metrics.byte_column_for_utf16(position.column)?;
        let offset = line_start.checked_add(usize::try_from(byte_column).ok()?)?;

        u32::try_from(offset).ok()
    }

    fn line_for_offset(&self, offset: usize) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(line) => line.saturating_sub(1),
        }
    }

    fn line_text_end(bytes: &[u8], start: usize, next_line_start: usize) -> usize {
        let mut end = next_line_start;
        if end > start && bytes[end - 1] == b'\n' {
            end -= 1;
            if end > start && bytes[end - 1] == b'\r' {
                end -= 1;
            }
        }

        end
    }
}

/// Per-line mapping between UTF-8 byte columns and UTF-16 code-unit columns.
#[derive(Debug, Clone)]
struct LineMetrics {
    byte_len: u32,
    utf16_len: u32,
    char_offsets: Vec<LineCharOffset>,
}

impl LineMetrics {
    fn new(line_text: &str) -> Self {
        let mut utf16_offset = 0_u32;
        let mut char_offsets = Vec::new();

        for (byte_offset, ch) in line_text.char_indices() {
            char_offsets.push(LineCharOffset {
                byte: u32::try_from(byte_offset).expect("line byte offset should fit into u32"),
                utf16: utf16_offset,
            });
            utf16_offset +=
                u32::try_from(ch.len_utf16()).expect("UTF-16 width should fit into u32");
        }

        Self {
            byte_len: u32::try_from(line_text.len()).expect("line length should fit into u32"),
            utf16_len: utf16_offset,
            char_offsets,
        }
    }

    fn utf16_column_for_byte(&self, byte_column: usize) -> u32 {
        let byte_column = u32::try_from(byte_column).unwrap_or(u32::MAX);
        if byte_column >= self.byte_len {
            return self.utf16_len;
        }

        match self
            .char_offsets
            .binary_search_by_key(&byte_column, |offset| offset.byte)
        {
            Ok(idx) => self.char_offsets[idx].utf16,
            Err(0) => 0,
            Err(idx) => self.char_offsets[idx - 1].utf16,
        }
    }

    fn byte_column_for_utf16(&self, utf16_column: u32) -> Option<u32> {
        if utf16_column > self.utf16_len {
            return None;
        }
        if utf16_column == self.utf16_len {
            return Some(self.byte_len);
        }

        self.char_offsets
            .binary_search_by_key(&utf16_column, |offset| offset.utf16)
            .ok()
            .map(|idx| self.char_offsets[idx].byte)
    }
}

#[derive(Debug, Clone, Copy)]
struct LineCharOffset {
    byte: u32,
    utf16: u32,
}

#[cfg(test)]
mod tests {
    use super::{LineIndex, Position, TextSpan};

    #[test]
    fn checks_half_open_span_containment() {
        let cases = [
            ("before start", TextSpan { start: 10, end: 20 }, 9, false),
            ("at start", TextSpan { start: 10, end: 20 }, 10, true),
            ("inside", TextSpan { start: 10, end: 20 }, 15, true),
            ("at end", TextSpan { start: 10, end: 20 }, 20, false),
            ("after end", TextSpan { start: 10, end: 20 }, 21, false),
        ];

        for (label, span, offset, expected) in cases {
            assert_eq!(span.contains(offset), expected, "{label}");
        }
    }

    #[test]
    fn checks_cursor_friendly_span_touches() {
        let cases = [
            ("before start", TextSpan { start: 10, end: 20 }, 9, false),
            ("at start", TextSpan { start: 10, end: 20 }, 10, true),
            ("inside", TextSpan { start: 10, end: 20 }, 15, true),
            ("at end", TextSpan { start: 10, end: 20 }, 20, true),
            ("after end", TextSpan { start: 10, end: 20 }, 21, false),
            ("empty at start", TextSpan { start: 10, end: 10 }, 10, true),
        ];

        for (label, span, offset, expected) in cases {
            assert_eq!(span.touches(offset), expected, "{label}");
        }
    }

    #[test]
    fn reports_saturating_span_lengths() {
        let cases = [
            ("normal", TextSpan { start: 10, end: 20 }, 10),
            ("empty", TextSpan { start: 10, end: 10 }, 0),
            ("invalid", TextSpan { start: 20, end: 10 }, 0),
        ];

        for (label, span, expected) in cases {
            assert_eq!(span.len(), expected, "{label}");
        }
    }

    #[test]
    fn converts_ascii_offsets_to_utf16_positions() {
        let index = LineIndex::new("let user = User;\nuser.id();");
        let cases = [
            ("start", 0, Position { line: 0, column: 0 }),
            ("same line", 4, Position { line: 0, column: 4 }),
            ("next line", 17, Position { line: 1, column: 0 }),
            ("inside next line", 21, Position { line: 1, column: 4 }),
        ];

        for (label, offset, expected) in cases {
            assert_eq!(index.utf16_position(offset), expected, "{label}");
            assert_eq!(
                index.offset_from_utf16_position(expected),
                Some(offset),
                "{label}"
            );
        }
    }

    #[test]
    fn converts_non_ascii_offsets_to_utf16_positions() {
        let index = LineIndex::new("é\n𝄞a");
        let cases = [
            ("accent start", 0, Position { line: 0, column: 0 }),
            ("after accent", 2, Position { line: 0, column: 1 }),
            ("second line start", 3, Position { line: 1, column: 0 }),
            ("after surrogate pair", 7, Position { line: 1, column: 2 }),
            ("after ascii", 8, Position { line: 1, column: 3 }),
        ];

        for (label, offset, expected) in cases {
            assert_eq!(index.utf16_position(offset), expected, "{label}");
            assert_eq!(
                index.offset_from_utf16_position(expected),
                Some(offset),
                "{label}"
            );
        }
    }

    #[test]
    fn rejects_invalid_utf16_positions() {
        let index = LineIndex::new("𝄞a");
        let cases = [
            ("inside surrogate pair", Position { line: 0, column: 1 }),
            ("past line end", Position { line: 0, column: 4 }),
            ("past last line", Position { line: 1, column: 0 }),
        ];

        for (label, position) in cases {
            assert_eq!(index.offset_from_utf16_position(position), None, "{label}");
        }
    }

    #[test]
    fn treats_line_endings_as_line_boundaries() {
        let index = LineIndex::new("a\r\nbb\n");
        let cases = [
            ("first line end", Position { line: 0, column: 1 }, Some(1)),
            (
                "second line start",
                Position { line: 1, column: 0 },
                Some(3),
            ),
            ("second line end", Position { line: 1, column: 2 }, Some(5)),
            (
                "trailing empty line",
                Position { line: 2, column: 0 },
                Some(6),
            ),
        ];

        for (label, position, expected) in cases {
            assert_eq!(
                index.offset_from_utf16_position(position),
                expected,
                "{label}"
            );
        }
    }
}
