//! Phase 3C strict-subset CSV parser.
//!
//! Hand-rolled per ADR-0006 Decision 1's "strict fixture-only subset" +
//! amendment (b). Real CSV actuals import (with quoted fields, escaped
//! commas, multi-line cells, encodings) is **Phase 5** — not 3C — so
//! this parser is deliberately tiny.
//!
//! Subset specification (anything not on this list is rejected):
//!
//! - **UTF-8** input. The `&str` argument enforces this; a BOM at byte 0
//!   is rejected (no upstream tool of ours emits one).
//! - **Required header row** as the first line; column names byte-exact
//!   match the caller-supplied `expected_columns` (same names, same
//!   order, no trailing whitespace). Mismatch → MC2024.
//! - **Comma-separated** field delimiter only. Tab / semicolon are not
//!   delimiters; they sit inside fields like any other character.
//! - **No quoted fields.** Any `"` anywhere is a row-format error
//!   surfaced as a generic `Schema` parse error.
//! - **No embedded commas / newlines** in field values; one row per
//!   line, separated by `\n` (or `\r\n` — `str::lines()` handles both).
//! - **No comments.**
//! - **Trailing newline** on the last data row is tolerated.
//! - **No empty rows** elsewhere.
//!
//! Output: `Vec<Vec<String>>` — one inner vec per data row, each row
//! `expected_columns.len()` strings long. Field values are returned
//! verbatim (no trimming) so downstream resolvers can detect whitespace-
//! padded values per their own rules.

use crate::error::ValidationError;

/// Parse a strict-subset CSV string into a vector of data rows.
///
/// `input_set_label` is a human-readable identifier for the input set
/// (the YAML key — e.g., `"canonical_inputs"` or
/// `"test_fixtures.aggressive_q1"`); used to populate the `input_set`
/// field of the diagnostic.
///
/// On success returns `Ok(rows)` where each `rows[i].len() ==
/// expected_columns.len()`. On failure returns `Err(diags)` carrying
/// every error encountered; callers append the result to a larger
/// `Vec<ValidationError>`.
pub fn parse_strict(
    content: &str,
    expected_columns: &[String],
    input_set_label: &str,
) -> Result<Vec<Vec<String>>, Vec<ValidationError>> {
    let mut diags: Vec<ValidationError> = Vec::new();

    // Per amendment (b): UTF-8 BOM at byte 0 is rejected. (`&str`
    // already guarantees the rest of the contents is UTF-8.)
    if content.starts_with('\u{FEFF}') {
        diags.push(ValidationError::Schema {
            message: format!(
                "input set {input_set_label:?}: CSV starts with a UTF-8 BOM (rejected)"
            ),
        });
        return Err(diags);
    }

    // `str::lines()` strips trailing `\n` and `\r\n` for free, which
    // implements ADR-0006 Decision 1's "trailing newline tolerated".
    // Line numbers are 1-based and match the position the line would
    // have in a numbered listing of the file.
    let mut iter = content.lines().enumerate();

    // Header (line 1).
    let header_line = match iter.next() {
        Some((_, l)) => l,
        None => {
            diags.push(ValidationError::Schema {
                message: format!("input set {input_set_label:?}: CSV is empty"),
            });
            return Err(diags);
        }
    };
    if header_line.contains('"') {
        diags.push(ValidationError::Schema {
            message: format!(
                "input set {input_set_label:?}: CSV header contains a quote \
                 (the strict subset rejects quoted fields)"
            ),
        });
        return Err(diags);
    }
    let header: Vec<&str> = header_line.split(',').collect();
    if header.len() != expected_columns.len()
        || header
            .iter()
            .zip(expected_columns)
            .any(|(h, e)| *h != e.as_str())
    {
        diags.push(ValidationError::FixtureCsvHeaderMismatch {
            input_set: input_set_label.to_string(),
            expected: expected_columns.to_vec(),
            actual: header.iter().map(|s| (*s).to_string()).collect(),
        });
        return Err(diags);
    }

    // Data rows.
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (idx, line) in iter {
        let line_no = idx + 1;
        if line.is_empty() {
            diags.push(ValidationError::Schema {
                message: format!(
                    "input set {input_set_label:?} CSV line {line_no}: empty row (rejected)"
                ),
            });
            continue;
        }
        if line.contains('"') {
            diags.push(ValidationError::Schema {
                message: format!(
                    "input set {input_set_label:?} CSV line {line_no}: \
                     field contains a quote (the strict subset rejects quoted fields)"
                ),
            });
            continue;
        }
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() != expected_columns.len() {
            diags.push(ValidationError::FixtureCsvRowColumnCountMismatch {
                input_set: input_set_label.to_string(),
                line: line_no,
                expected: expected_columns.len(),
                actual: fields.len(),
            });
            continue;
        }
        rows.push(fields.into_iter().map(String::from).collect());
    }

    if !diags.is_empty() {
        return Err(diags);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols() -> Vec<String> {
        vec!["A".into(), "B".into(), "C".into()]
    }

    #[test]
    fn parses_basic_csv() {
        let csv = "A,B,C\n1,2,3\n4,5,6\n";
        let rows = parse_strict(csv, &cols(), "test").expect("ok");
        assert_eq!(
            rows,
            vec![
                vec!["1".to_string(), "2".to_string(), "3".to_string()],
                vec!["4".to_string(), "5".to_string(), "6".to_string()],
            ]
        );
    }

    #[test]
    fn tolerates_missing_trailing_newline() {
        let csv = "A,B,C\n1,2,3";
        let rows = parse_strict(csv, &cols(), "test").expect("ok");
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn header_mismatch_emits_mc2024() {
        let csv = "A,B,X\n1,2,3\n";
        let err = parse_strict(csv, &cols(), "test").unwrap_err();
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].code(), "MC2024");
    }

    #[test]
    fn row_column_count_emits_mc2023() {
        let csv = "A,B,C\n1,2\n";
        let err = parse_strict(csv, &cols(), "test").unwrap_err();
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].code(), "MC2023");
    }

    #[test]
    fn quoted_field_rejected() {
        let csv = "A,B,C\n1,\"2\",3\n";
        let err = parse_strict(csv, &cols(), "test").unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(err[0].to_string().contains("quote"));
    }

    #[test]
    fn bom_rejected() {
        let csv = "\u{FEFF}A,B,C\n1,2,3\n";
        let err = parse_strict(csv, &cols(), "test").unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(err[0].to_string().contains("BOM"));
    }

    #[test]
    fn handles_crlf_line_endings() {
        let csv = "A,B,C\r\n1,2,3\r\n";
        let rows = parse_strict(csv, &cols(), "test").expect("ok");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], vec!["1", "2", "3"]);
    }

    #[test]
    fn empty_csv_rejected() {
        let err = parse_strict("", &cols(), "test").unwrap_err();
        assert_eq!(err.len(), 1);
    }

    #[test]
    fn internal_empty_row_rejected() {
        let csv = "A,B,C\n1,2,3\n\n4,5,6\n";
        let err = parse_strict(csv, &cols(), "test").unwrap_err();
        assert!(err.iter().any(|e| e.to_string().contains("empty row")));
    }
}
