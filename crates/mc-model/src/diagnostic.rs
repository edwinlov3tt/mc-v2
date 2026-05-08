//! Phase 3B diagnostic types — the contract Phase 4 (LLM authoring) and
//! Phase 6 (UI editor) consume.
//!
//! Per [ADR-0005](../../../docs/decisions/0005-phase-3b-model-qa-linter-diagnostics.md)
//! Decision 7 + acceptance amendments #11 / #13 / #14:
//!
//! - **Stable diagnostic codes** namespaced as `MC{1|2|3|4}xxx`. Codes are
//!   `&'static str` and never repurposed. MC3008 is permanently retired
//!   (promoted to MC2011 — amendment #11); the slot stays vacant.
//! - **JSON envelope** `{ "schema_version": "1.0", "diagnostics": [...] }`.
//!   The `schema_version` field is **mandatory**, including in empty-
//!   diagnostic emissions (amendment #13).
//! - **Deterministic emission order** sorted by
//!   `(severity desc, code asc, yaml_pointer asc, message asc)` — applied
//!   uniformly across text, JSON, and library `Vec<Diagnostic>` outputs
//!   (amendment #14).
//!
//! ### Code registry (Phase 3B)
//!
//! | Range          | Category                                         |
//! |----------------|--------------------------------------------------|
//! | `MC1001..1002` | Parse errors (YAML syntax + safe-subset)         |
//! | `MC2001..2010` | Validation errors (Phase 3A's 10 ADR-0004 rules) |
//! | `MC2011`       | Validation error (Phase 3B promotion from lint)  |
//! | `MC3001..3007` | Lint warnings (descriptions, goldens, orphan, …) |
//! | `MC3008`       | **RETIRED** — promoted to `MC2011`               |
//! | `MC3009..3011` | Lint warnings (unused measures, root ambiguity)  |
//! | `MC3012+`      | Reserved for future lint additions               |
//! | `MC4xxx`       | Reserved (perf hints, security warnings, …)      |

use std::cmp::Ordering;
use std::path::PathBuf;

/// Stable diagnostic identifier. The `&'static str` form keeps it cheap
/// to copy and gives Rust's type system zero room to mistype a code.
pub type DiagnosticCode = &'static str;

/// Severity rank. The `#[repr(u8)]` representation makes the descending
/// sort (per ADR-0005 Decision 7's deterministic emission order) a
/// straightforward `u8` comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Severity {
    Info = 0,
    Warning = 1,
    Error = 2,
}

impl Severity {
    /// Lower-case-ascii label used in JSON output and the `severity:` line
    /// of text format. (We keep `Display` upper-cased for human-readable
    /// CLI prefixes; this is the machine-readable form.)
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "Error",
            Severity::Warning => "Warning",
            Severity::Info => "Info",
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// A `(line, column)` source position. Phase 3B emits this only for parse-
/// stage diagnostics where `serde_yaml` exposed a location. Validation and
/// lint diagnostics omit `span` (they read a typed `ParsedModel` that no
/// longer carries source coordinates).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// Structured pointer into the model. Two paths because the use cases
/// diverge: `yaml_pointer` is mechanical (RFC-6901, future UI-friendly);
/// `model_path` is name-resolved (LLM-friendly).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelPath {
    /// Path to the YAML file the diagnostic is rooted at. May be empty if
    /// the diagnostic was emitted against an in-memory model.
    pub file: PathBuf,
    /// Optional source position (`Some` for parse errors, `None` elsewhere).
    pub span: Option<Span>,
    /// RFC-6901-style pointer into the parsed YAML tree
    /// (`/dimensions/2`, `/measures/0/aggregation`).
    pub yaml_pointer: String,
    /// Schema-aware path
    /// (`dimensions.Time`, `measures.CPC.aggregation`,
    /// `rules.rule_clicks`).
    pub model_path: String,
}

impl ModelPath {
    /// Convenience: a path rooted at `file` with no span.
    pub fn new(
        file: impl Into<PathBuf>,
        yaml_pointer: impl Into<String>,
        model_path: impl Into<String>,
    ) -> Self {
        Self {
            file: file.into(),
            span: None,
            yaml_pointer: yaml_pointer.into(),
            model_path: model_path.into(),
        }
    }
}

/// One diagnostic. The five fields are the public contract — adding new
/// optional fields is allowed (and bumps `schema_version`); renaming or
/// repurposing existing fields is not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub path: ModelPath,
    pub message: String,
    pub suggestion: Option<String>,
}

impl Diagnostic {
    /// Convert to a `RichDiagnostic` from `mc-diagnostics`, optionally
    /// attaching a source span from the location map.
    ///
    /// Per ADR-0024 Decision 2: backward compatibility — the existing
    /// `Diagnostic` type is not deleted; it gains this conversion method.
    pub fn to_rich(
        &self,
        loc_map: Option<&crate::location::LocationMap>,
    ) -> mc_diagnostics::RichDiagnostic {
        let severity = match self.severity {
            Severity::Error => mc_diagnostics::DiagSeverity::Error,
            Severity::Warning => mc_diagnostics::DiagSeverity::Warning,
            Severity::Info => mc_diagnostics::DiagSeverity::Info,
        };

        let mut rich =
            mc_diagnostics::RichDiagnostic::new(self.code.to_string(), severity, &self.message);

        // Attach source span from location map if available
        // TODO(saphyr): replace with single-pass LocatedValue parsing.
        if let Some(map) = loc_map {
            if let Some(span) = map.get(&self.path.yaml_pointer) {
                rich.primary_span = Some(span.clone());
            }
        }

        // Carry over suggestion as help text
        if let Some(ref suggestion) = self.suggestion {
            rich = rich.with_help(suggestion.clone());
        }

        rich
    }
}

/// JSON envelope schema version. Bumps on breaking diagnostic shape changes
/// (renaming a field, changing a severity enum variant). Phase 3B ships at
/// `"1.0"`. Phase 4 + Phase 6 pin to this exact value.
pub const SCHEMA_VERSION: &str = "1.0";

/// Deterministic emission order — apply BEFORE any formatter runs.
///
/// Sort by:
/// 1. `severity` desc — errors first, then warnings, then info.
/// 2. `code` asc — within a severity, MC2001 before MC2002, etc.
/// 3. `yaml_pointer` asc — within a code, earlier YAML nodes first.
/// 4. `message` asc — final tiebreaker.
///
/// Per ADR-0005 amendment #14: lint rules iterating over `BTreeMap`/`Vec`
/// produce deterministic output by themselves, but cross-rule ordering
/// requires this sort. Apply uniformly to text, JSON, and library output.
pub fn sort_diagnostics(diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_by(|a, b| match (b.severity as u8).cmp(&(a.severity as u8)) {
        Ordering::Equal => match a.code.cmp(b.code) {
            Ordering::Equal => match a.path.yaml_pointer.cmp(&b.path.yaml_pointer) {
                Ordering::Equal => a.message.cmp(&b.message),
                other => other,
            },
            other => other,
        },
        other => other,
    });
}

/// Render a sorted slice of diagnostics as the human-readable text format.
///
/// Layout:
///
/// ```text
/// MC3001 [Warning] dimensions.Time: Dimension 'Time' has no description
///   in: <path>
///   pointer: /dimensions/2
///   suggestion: Add a one-line description …
/// ```
///
/// Empty slice produces an empty string. Trailing newline only when at
/// least one diagnostic was rendered.
pub fn diagnostics_to_text(diagnostics: &[Diagnostic]) -> String {
    let mut out = String::new();
    for (i, d) in diagnostics.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(d.code);
        out.push_str(" [");
        out.push_str(d.severity.label());
        out.push_str("] ");
        out.push_str(&d.path.model_path);
        out.push_str(": ");
        out.push_str(&d.message);
        out.push('\n');
        if !d.path.file.as_os_str().is_empty() {
            out.push_str("  in: ");
            out.push_str(&d.path.file.display().to_string());
            if let Some(span) = &d.path.span {
                out.push(':');
                out.push_str(&span.line.to_string());
                out.push(':');
                out.push_str(&span.column.to_string());
            }
            out.push('\n');
        }
        out.push_str("  pointer: ");
        out.push_str(&d.path.yaml_pointer);
        out.push('\n');
        if let Some(s) = &d.suggestion {
            out.push_str("  suggestion: ");
            out.push_str(s);
            out.push('\n');
        }
    }
    out
}

/// Render a sorted slice of diagnostics as the JSON envelope per ADR-0005
/// Decision 7 + amendment #13:
///
/// ```json
/// {
///   "schema_version": "1.0",
///   "diagnostics": [ … ]
/// }
/// ```
///
/// `schema_version` is emitted **unconditionally**, including in empty-
/// diagnostic cases.
pub fn diagnostics_to_json(diagnostics: &[Diagnostic]) -> String {
    let mut out = String::new();
    out.push_str("{\n  \"schema_version\": \"");
    out.push_str(SCHEMA_VERSION);
    out.push_str("\",\n  \"diagnostics\": [");
    if diagnostics.is_empty() {
        out.push_str("]\n}\n");
        return out;
    }
    out.push('\n');
    for (i, d) in diagnostics.iter().enumerate() {
        write_diagnostic_json(&mut out, d, /* indent */ 4);
        if i + 1 < diagnostics.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]\n}\n");
    out
}

fn write_diagnostic_json(out: &mut String, d: &Diagnostic, indent: usize) {
    let pad = " ".repeat(indent);
    let pad2 = " ".repeat(indent + 2);
    out.push_str(&pad);
    out.push_str("{\n");
    out.push_str(&pad2);
    out.push_str("\"code\": ");
    write_json_string(out, d.code);
    out.push_str(",\n");
    out.push_str(&pad2);
    out.push_str("\"severity\": ");
    write_json_string(out, d.severity.label());
    out.push_str(",\n");
    out.push_str(&pad2);
    out.push_str("\"path\": ");
    write_path_json(out, &d.path, indent + 2);
    out.push_str(",\n");
    out.push_str(&pad2);
    out.push_str("\"message\": ");
    write_json_string(out, &d.message);
    out.push_str(",\n");
    out.push_str(&pad2);
    out.push_str("\"suggestion\": ");
    match &d.suggestion {
        Some(s) => write_json_string(out, s),
        None => out.push_str("null"),
    }
    out.push('\n');
    out.push_str(&pad);
    out.push('}');
}

fn write_path_json(out: &mut String, p: &ModelPath, indent: usize) {
    let pad = " ".repeat(indent);
    let pad2 = " ".repeat(indent + 2);
    out.push_str("{\n");
    out.push_str(&pad2);
    out.push_str("\"file\": ");
    write_json_string(out, &p.file.display().to_string());
    out.push_str(",\n");
    out.push_str(&pad2);
    out.push_str("\"span\": ");
    match &p.span {
        Some(s) => {
            out.push_str("{\"line\": ");
            out.push_str(&s.line.to_string());
            out.push_str(", \"column\": ");
            out.push_str(&s.column.to_string());
            out.push('}');
        }
        None => out.push_str("null"),
    }
    out.push_str(",\n");
    out.push_str(&pad2);
    out.push_str("\"yaml_pointer\": ");
    write_json_string(out, &p.yaml_pointer);
    out.push_str(",\n");
    out.push_str(&pad2);
    out.push_str("\"model_path\": ");
    write_json_string(out, &p.model_path);
    out.push('\n');
    out.push_str(&pad);
    out.push('}');
}

/// Minimal JSON string encoder. Handles the escapes the JSON spec requires
/// (`\\`, `\"`, control chars `< 0x20`, plus `\b \f \n \r \t` shorthands).
/// Phase 3B avoids a `serde_json` dep — this keeps Cargo.lock + the
/// toolchain story unchanged.
pub fn write_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diag(
        code: &'static str,
        severity: Severity,
        pointer: &str,
        model_path: &str,
        message: &str,
    ) -> Diagnostic {
        Diagnostic {
            code,
            severity,
            path: ModelPath::new("acme.yaml", pointer, model_path),
            message: message.into(),
            suggestion: None,
        }
    }

    #[test]
    fn sort_orders_by_severity_desc_then_code_asc() {
        let mut v = vec![
            diag(
                "MC3001",
                Severity::Warning,
                "/dimensions/0",
                "dimensions.Time",
                "x",
            ),
            diag(
                "MC2001",
                Severity::Error,
                "/dimensions/0",
                "dimensions.Time",
                "x",
            ),
            diag("MC3010", Severity::Info, "/measures/0", "measures.X", "x"),
        ];
        sort_diagnostics(&mut v);
        assert_eq!(v[0].code, "MC2001");
        assert_eq!(v[1].code, "MC3001");
        assert_eq!(v[2].code, "MC3010");
    }

    #[test]
    fn sort_breaks_ties_by_yaml_pointer_then_message() {
        let mut v = vec![
            diag("MC3001", Severity::Warning, "/dimensions/2", "x", "z"),
            diag("MC3001", Severity::Warning, "/dimensions/0", "x", "z"),
            diag("MC3001", Severity::Warning, "/dimensions/1", "x", "y"),
        ];
        sort_diagnostics(&mut v);
        assert_eq!(v[0].path.yaml_pointer, "/dimensions/0");
        assert_eq!(v[1].path.yaml_pointer, "/dimensions/1");
        assert_eq!(v[2].path.yaml_pointer, "/dimensions/2");
    }

    #[test]
    fn json_envelope_emits_schema_version_for_empty_diagnostics() {
        let json = diagnostics_to_json(&[]);
        assert!(json.contains("\"schema_version\": \"1.0\""));
        assert!(json.contains("\"diagnostics\": []"));
    }

    #[test]
    fn json_string_escapes_special_chars() {
        let mut s = String::new();
        write_json_string(&mut s, "a\"b\\c\nd\te\x01");
        assert_eq!(s, "\"a\\\"b\\\\c\\nd\\te\\u0001\"");
    }
}
