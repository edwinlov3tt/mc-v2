//! Byte-offset source spans for diagnostic rendering.
//!
//! Per [ADR-0024](../../../docs/decisions/0024-rich-diagnostic-rendering.md)
//! Decision 1: spans use byte offsets, not line/column. Line/column is
//! computed lazily on render from bytes + source text.

use serde::Serialize;

/// A span in a source file, identified by byte offsets.
///
/// Line and column are computed on render via [`resolve_position`](Self::resolve_position).
/// Byte offsets compose naturally with inner-grammar positions (e.g., a
/// formula parse error within a YAML string) via [`with_inner_offset`](Self::with_inner_offset).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceSpan {
    /// Relative path to the source file.
    pub file: String,
    /// Byte offset of the span start (inclusive, file-absolute).
    pub start_byte: usize,
    /// Byte offset of the span end (exclusive, file-absolute).
    pub end_byte: usize,
}

impl SourceSpan {
    /// Create a new span.
    pub fn new(file: impl Into<String>, start: usize, end: usize) -> Self {
        Self {
            file: file.into(),
            start_byte: start,
            end_byte: end,
        }
    }

    /// Compose with an inner offset to lift an inner-grammar position to
    /// file-absolute coordinates.
    ///
    /// Used to map a formula-parser byte offset (relative to the formula
    /// string) back to the YAML file position. The caller provides the
    /// inner offset within the string and the length of the inner span.
    ///
    /// # Example
    ///
    /// ```
    /// # use mc_diagnostics::SourceSpan;
    /// // YAML string "Custmers * AOV" starts at byte 100 in the file.
    /// let yaml_span = SourceSpan::new("model.yaml", 100, 114);
    /// // The formula parser reports "Custmers" at offset 0, length 8.
    /// let inner = yaml_span.with_inner_offset(0, 8);
    /// assert_eq!(inner.start_byte, 100);
    /// assert_eq!(inner.end_byte, 108);
    /// ```
    pub fn with_inner_offset(&self, inner_start: usize, inner_len: usize) -> Self {
        Self {
            file: self.file.clone(),
            start_byte: self.start_byte + inner_start,
            end_byte: self.start_byte + inner_start + inner_len,
        }
    }

    /// Resolve the start position to 1-based `(line, column)` given source text.
    ///
    /// Returns `(1, 1)` if the offset is out of range.
    pub fn resolve_position(&self, source: &str) -> (usize, usize) {
        byte_offset_to_line_col(source, self.start_byte)
    }
}

/// Convert a byte offset to 1-based (line, column) in the given source text.
///
/// Tabs are counted as 1 column each at this level; the renderer handles
/// visual tab expansion separately.
pub fn byte_offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(source.len());
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_inner_offset_composes_correctly() {
        let outer = SourceSpan::new("model.yaml", 100, 120);
        let inner = outer.with_inner_offset(5, 8);
        assert_eq!(inner.file, "model.yaml");
        assert_eq!(inner.start_byte, 105);
        assert_eq!(inner.end_byte, 113);
    }

    #[test]
    fn resolve_position_first_line() {
        let src = "hello world";
        let span = SourceSpan::new("f", 6, 11);
        assert_eq!(span.resolve_position(src), (1, 7));
    }

    #[test]
    fn resolve_position_multiline() {
        let src = "line1\nline2\nline3";
        let span = SourceSpan::new("f", 12, 17);
        // byte 12 = 'l' of "line3" → line 3, col 1
        assert_eq!(span.resolve_position(src), (3, 1));
    }

    #[test]
    fn resolve_position_out_of_range() {
        let src = "short";
        let span = SourceSpan::new("f", 999, 1000);
        // clamps to end
        assert_eq!(span.resolve_position(src), (1, 6));
    }

    #[test]
    fn with_inner_offset_zero_len() {
        let outer = SourceSpan::new("f", 50, 60);
        let inner = outer.with_inner_offset(3, 0);
        assert_eq!(inner.start_byte, 53);
        assert_eq!(inner.end_byte, 53);
    }
}
