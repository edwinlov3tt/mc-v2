//! Stage 1: YAML bytes → `ParsedModel`.
//!
//! Two phases internally:
//!
//! 1. **Safe-subset prefilter.** A line-oriented scan rejects anchors
//!    (`&name`), aliases (`*name`), merge keys (`<<:`), and custom tags
//!    (`!Foo` / `!!Foo`). Per ADR-0004 Decision 1, these are banned
//!    *before* the YAML library sees them so an LLM-emitted model that
//!    uses anchors is rejected by name, not silently expanded. The scan
//!    is quote-and-comment aware: anything inside `'…'` or `"…"` is
//!    transparent, anything after an unquoted `#` is a comment.
//!
//! 2. **`serde_yaml` deserialization.** Standard `from_str` against
//!    `ParsedModel`. Span info (line/column) is extracted from the
//!    `serde_yaml::Error` location when available.
//!
//! Multiple safe-subset violations on different lines surface as
//! separate errors? No — for Phase 3A the parse stage returns the *first*
//! violation. The validator's all-errors-at-once contract starts at
//! stage 2 (semantic validation). This is consistent with how
//! `serde_yaml` itself returns single errors.

use std::path::PathBuf;

use crate::error::{ParseError, ParseErrorKind, Span};
use crate::schema::ParsedModel;

/// Parse a YAML string into a `ParsedModel`. Rejects safe-subset
/// violations before deserialization.
///
/// `source_label` is used as the `file` field of any `Span` produced;
/// pass `Some(path.display().to_string())` from `load`, or `None` when
/// loading from an in-memory string.
pub fn parse(yaml: &str, source_label: Option<String>) -> Result<ParsedModel, ParseError> {
    // Per ADR-0004 Decision 1: the YAML safe subset rejects anchors,
    // aliases, merge keys, and custom tags. Run the prefilter first.
    if let Some(violation) = safe_subset_scan(yaml) {
        let span = Span {
            file: source_label.as_ref().map(PathBuf::from),
            line: violation.line,
            column: violation.column,
        };
        return Err(ParseError::SafeSubset {
            span,
            kind: violation.kind,
        });
    }

    serde_yaml::from_str::<ParsedModel>(yaml).map_err(|e| {
        let location = e.location();
        let span = Span {
            file: source_label.as_ref().map(PathBuf::from),
            line: location.as_ref().map(|l| l.line()).unwrap_or(0),
            column: location.as_ref().map(|l| l.column()).unwrap_or(0),
        };
        ParseError::Syntax {
            span,
            message: e.to_string(),
        }
    })
}

#[derive(Debug)]
struct SafeSubsetViolation {
    line: usize,
    column: usize,
    kind: ParseErrorKind,
}

/// Line-by-line scan for forbidden YAML 1.2 features. Quote-and-comment
/// aware: `'…'` and `"…"` ranges are transparent; an unquoted `#` starts
/// a comment that runs to end-of-line.
///
/// Returns the FIRST violation found, with 1-based line + 1-based column.
fn safe_subset_scan(yaml: &str) -> Option<SafeSubsetViolation> {
    for (line_idx, raw_line) in yaml.lines().enumerate() {
        let line_no = line_idx + 1;
        if let Some((col, kind)) = scan_line(raw_line) {
            return Some(SafeSubsetViolation {
                line: line_no,
                column: col,
                kind,
            });
        }
    }
    None
}

/// Scan a single line for forbidden tokens. Returns `Some((col_1_based, kind))`
/// for the first violation, else `None`.
fn scan_line(line: &str) -> Option<(usize, ParseErrorKind)> {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    let mut state = LineState::Plain;
    while i < bytes.len() {
        let b = bytes[i];
        match state {
            LineState::Plain => match b {
                b'#' => return None, // rest is a comment — done with this line
                b'\'' => state = LineState::SingleQuoted,
                b'"' => state = LineState::DoubleQuoted,
                b'&' => return Some((col(i), ParseErrorKind::Anchor)),
                b'*' => {
                    // `*` is also YAML's flow-style sequence end-marker after
                    // a flow sequence — but our YAML uses block style. Any
                    // unquoted `*name` token is treated as alias.
                    // Disambiguate: only flag when the next char is a letter
                    // / digit / underscore (a valid alias-name start),
                    // otherwise ignore (e.g. multiplication-like patterns
                    // never appear unquoted in our schema, but we still want
                    // to be conservative).
                    if i + 1 < bytes.len() && is_anchor_name_start(bytes[i + 1]) {
                        return Some((col(i), ParseErrorKind::Alias));
                    }
                }
                b'<' => {
                    // Merge key: `<<:` (with optional whitespace before `:`).
                    if bytes.get(i + 1) == Some(&b'<') {
                        let mut j = i + 2;
                        while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                            j += 1;
                        }
                        if bytes.get(j) == Some(&b':') {
                            return Some((col(i), ParseErrorKind::MergeKey));
                        }
                    }
                }
                b'!' => {
                    // Custom tag: an unquoted `!` at the start of a value
                    // followed by a non-space identifier-start character.
                    // Examples we want to flag: `!Foo`, `!!Foo`, `!<tag:url>`.
                    // YAML doesn't use `!` for anything else outside quoted
                    // strings, so a flat reject is safe.
                    if i + 1 < bytes.len() {
                        let next = bytes[i + 1];
                        if next == b'!' || is_anchor_name_start(next) || next == b'<' {
                            return Some((col(i), ParseErrorKind::CustomTag));
                        }
                    }
                }
                _ => {}
            },
            LineState::SingleQuoted => {
                if b == b'\'' {
                    // YAML escapes a single quote inside single quotes by
                    // doubling: `''`. Detect and skip.
                    if bytes.get(i + 1) == Some(&b'\'') {
                        i += 1;
                    } else {
                        state = LineState::Plain;
                    }
                }
            }
            LineState::DoubleQuoted => match b {
                b'\\' => {
                    // Backslash-escaped char — skip the next byte.
                    i += 1;
                }
                b'"' => state = LineState::Plain,
                _ => {}
            },
        }
        i += 1;
    }
    None
}

#[derive(Clone, Copy, Debug)]
enum LineState {
    Plain,
    SingleQuoted,
    DoubleQuoted,
}

fn is_anchor_name_start(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn col(byte_offset: usize) -> usize {
    // 1-based column. We don't translate UTF-8 byte offsets to character
    // columns because `serde_yaml::Location::column` is also byte-based.
    byte_offset + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_anchor() {
        let y = "metadata:\n  name: &x \"hi\"\n";
        let err = parse(y, None).unwrap_err();
        assert!(matches!(
            err,
            ParseError::SafeSubset {
                kind: ParseErrorKind::Anchor,
                ..
            }
        ));
    }

    #[test]
    fn rejects_alias() {
        let y = "metadata:\n  name: *x\n";
        let err = parse(y, None).unwrap_err();
        assert!(matches!(
            err,
            ParseError::SafeSubset {
                kind: ParseErrorKind::Alias,
                ..
            }
        ));
    }

    #[test]
    fn rejects_merge_key() {
        let y = "metadata:\n  <<: { name: \"x\" }\n";
        let err = parse(y, None).unwrap_err();
        assert!(matches!(
            err,
            ParseError::SafeSubset {
                kind: ParseErrorKind::MergeKey,
                ..
            }
        ));
    }

    #[test]
    fn rejects_custom_tag() {
        let y = "metadata: !Foo\n  name: \"x\"\n";
        let err = parse(y, None).unwrap_err();
        assert!(matches!(
            err,
            ParseError::SafeSubset {
                kind: ParseErrorKind::CustomTag,
                ..
            }
        ));
    }

    #[test]
    fn allows_quoted_ampersand_etc() {
        // & * < ! inside quoted strings are fine.
        let y = "metadata:\n  name: \"a & b * c << d ! e\"\n  description: 'single & quote'\n";
        // We don't need this to deserialize cleanly into ParsedModel — the
        // top-level model has lots of required fields. We're only testing
        // that the safe-subset prefilter does NOT trigger.
        let err = parse(y, None).unwrap_err();
        assert!(matches!(err, ParseError::Syntax { .. }));
    }

    #[test]
    fn allows_comments_with_special_chars() {
        let y = "metadata:\n  # this comment has & and * and << and !\n  name: \"hi\"\n";
        let err = parse(y, None).unwrap_err();
        assert!(matches!(err, ParseError::Syntax { .. }));
    }
}
