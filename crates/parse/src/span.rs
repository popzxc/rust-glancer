use ra_syntax::TextRange;

/// Span representation in UTF-8 byte offsets from the beginning of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub text: TextSpan,
}

impl Span {
    /// Converts a syntax-level text range into the internal span representation.
    pub fn from_text_range(text_range: TextRange) -> Self {
        let start = u32::from(text_range.start());
        let end = u32::from(text_range.end());

        Self {
            text: TextSpan { start, end },
        }
    }

    /// Converts this byte span into zero-based line/column coordinates on demand.
    pub fn line_column(self, line_index: &LineIndex) -> LineColumnSpan {
        LineColumnSpan {
            start: line_index.position(self.text.start),
            end: line_index.position(self.text.end),
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
    pub(crate) line_starts: Vec<u32>,
    pub(crate) line_byte_lens: Vec<u32>,
    pub(crate) non_ascii_lines: Vec<LineUtf16Metrics>,
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

        let mut line_byte_lens = Vec::with_capacity(line_starts.len());
        let mut non_ascii_lines = Vec::new();
        for (line_idx, line_start) in line_starts.iter().enumerate() {
            let next_line_start = line_starts
                .get(line_idx + 1)
                .copied()
                .unwrap_or(source.len());
            let line_end = Self::line_text_end(source.as_bytes(), *line_start, next_line_start);
            let line_text = &source[*line_start..line_end];

            line_byte_lens
                .push(u32::try_from(line_text.len()).expect("line length should fit into u32"));
            if let Some(metrics) = LineUtf16Metrics::new(
                u32::try_from(line_idx).expect("line index should fit into u32"),
                line_text,
            ) {
                non_ascii_lines.push(metrics);
            }
        }

        let line_starts = line_starts
            .iter()
            .map(|start| u32::try_from(*start).expect("source offsets should fit into u32"))
            .collect();

        Self {
            line_starts,
            line_byte_lens,
            non_ascii_lines,
        }
    }

    /// Converts a byte offset into a zero-based line/column position.
    pub fn position(&self, offset: u32) -> Position {
        let offset = usize::try_from(offset).expect("offset should fit into usize");
        let line_index = self.line_for_offset(offset);
        let line_start = usize::try_from(self.line_starts[line_index])
            .expect("line start should fit into usize");
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
        let line_start = usize::try_from(self.line_starts[line_index])
            .expect("line start should fit into usize");
        let byte_column = offset.saturating_sub(line_start);
        let byte_column = u32::try_from(byte_column).unwrap_or(u32::MAX);
        let line_byte_len = self.line_byte_lens[line_index];

        Position {
            line: u32::try_from(line_index).expect("line index should fit into u32"),
            column: self
                .utf16_metrics(line_index)
                .map(|metrics| metrics.utf16_column_for_byte(byte_column))
                .unwrap_or_else(|| byte_column.min(line_byte_len)),
        }
    }

    /// Converts a zero-based line/UTF-16-column position into a byte offset.
    pub fn offset_from_utf16_position(&self, position: Position) -> Option<u32> {
        let line_index = usize::try_from(position.line).ok()?;
        let line_start = *self.line_starts.get(line_index)?;
        let line_byte_len = *self.line_byte_lens.get(line_index)?;
        let byte_column = match self.utf16_metrics(line_index) {
            Some(metrics) => metrics.byte_column_for_utf16(position.column)?,
            None if position.column <= line_byte_len => position.column,
            None => return None,
        };

        line_start.checked_add(byte_column)
    }

    fn line_for_offset(&self, offset: usize) -> usize {
        let offset = u32::try_from(offset).unwrap_or(u32::MAX);
        match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(line) => line.saturating_sub(1),
        }
    }

    fn utf16_metrics(&self, line_index: usize) -> Option<&LineUtf16Metrics> {
        let line_index = u32::try_from(line_index).ok()?;
        self.non_ascii_lines
            .binary_search_by_key(&line_index, |metrics| metrics.line)
            .ok()
            .map(|idx| &self.non_ascii_lines[idx])
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

/// Sparse per-line mapping between UTF-8 byte columns and UTF-16 code-unit columns.
#[derive(Debug, Clone)]
pub(crate) struct LineUtf16Metrics {
    pub(crate) line: u32,
    pub(crate) utf16_len: u32,
    pub(crate) non_ascii_ranges: Vec<LineCharRange>,
}

impl LineUtf16Metrics {
    fn new(line: u32, line_text: &str) -> Option<Self> {
        let mut utf16_offset = 0_u32;
        let mut non_ascii_ranges = Vec::new();

        for (byte_offset, ch) in line_text.char_indices() {
            let byte_start =
                u32::try_from(byte_offset).expect("line byte offset should fit into u32");
            let byte_width = u32::try_from(ch.len_utf8()).expect("UTF-8 width should fit into u32");
            let utf16_width =
                u32::try_from(ch.len_utf16()).expect("UTF-16 width should fit into u32");

            if byte_width != utf16_width {
                non_ascii_ranges.push(LineCharRange {
                    byte_start,
                    byte_end: byte_start + byte_width,
                    utf16_start: utf16_offset,
                    utf16_end: utf16_offset + utf16_width,
                });
            }

            utf16_offset += utf16_width;
        }

        (!non_ascii_ranges.is_empty()).then_some(Self {
            line,
            utf16_len: utf16_offset,
            non_ascii_ranges,
        })
    }

    fn utf16_column_for_byte(&self, byte_column: u32) -> u32 {
        if byte_column >= self.byte_len() {
            return self.utf16_len;
        }

        let mut adjustment = 0;
        for range in &self.non_ascii_ranges {
            if byte_column < range.byte_start {
                return byte_column.saturating_sub(adjustment);
            }
            if byte_column < range.byte_end {
                return range.utf16_start;
            }

            adjustment += range.byte_width().saturating_sub(range.utf16_width());
        }

        byte_column.saturating_sub(adjustment)
    }

    fn byte_column_for_utf16(&self, utf16_column: u32) -> Option<u32> {
        if utf16_column > self.utf16_len {
            return None;
        }
        if utf16_column == self.utf16_len {
            return Some(self.byte_len());
        }

        let mut adjustment = 0;
        for range in &self.non_ascii_ranges {
            if utf16_column < range.utf16_start {
                return Some(utf16_column + adjustment);
            }
            if utf16_column < range.utf16_end {
                return (utf16_column == range.utf16_start).then_some(range.byte_start);
            }

            adjustment += range.byte_width().saturating_sub(range.utf16_width());
        }

        Some(utf16_column + adjustment)
    }

    fn byte_len(&self) -> u32 {
        let adjustment = self
            .non_ascii_ranges
            .iter()
            .map(|range| range.byte_width().saturating_sub(range.utf16_width()))
            .sum::<u32>();
        self.utf16_len + adjustment
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LineCharRange {
    pub(crate) byte_start: u32,
    pub(crate) byte_end: u32,
    pub(crate) utf16_start: u32,
    pub(crate) utf16_end: u32,
}

impl LineCharRange {
    fn byte_width(self) -> u32 {
        self.byte_end - self.byte_start
    }

    fn utf16_width(self) -> u32 {
        self.utf16_end - self.utf16_start
    }
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

    #[test]
    fn clamps_utf16_positions_inside_line_endings_to_line_end() {
        let index = LineIndex::new("a\r\nbb\n");
        let cases = [
            ("at carriage return", 1, Position { line: 0, column: 1 }),
            ("at newline", 2, Position { line: 0, column: 1 }),
            ("at trailing newline", 5, Position { line: 1, column: 2 }),
        ];

        for (label, offset, expected) in cases {
            assert_eq!(index.utf16_position(offset), expected, "{label}");
        }
    }
}
