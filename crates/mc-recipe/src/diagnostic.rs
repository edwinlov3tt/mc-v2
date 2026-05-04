//! Recipe diagnostic envelope — the JSON shape Phase 4 (LLM authoring),
//! Phase 5B (LLM-assisted recipe authoring), and Phase 6 (UI editor)
//! consume.
//!
//! Per ADR-0010 Appendix B, MC5xxx diagnostics share the Phase 3B envelope
//! shape (`schema_version: "1.0"`, sorted `diagnostics` array) but carry
//! a **flat** `path` field — a JSON pointer into the recipe YAML — rather
//! than the structured `ModelPath` the model layer uses. This is
//! deliberate: recipe diagnostics never carry source-line spans (the
//! parser surfaces those via MC5001's message), so the simpler shape is
//! sufficient.
//!
//! Severity is emitted as **lowercase** (`"error"`) — distinct from
//! `mc-model` which emits uppercase (`"Error"`). Per the Stream B
//! handoff's diagnostic-shape spec.

use std::cmp::Ordering;

/// Stable diagnostic identifier (e.g., `"MC5004"`). The `&'static str`
/// form keeps codes cheap to copy and gives Rust's type system zero room
/// for typos.
pub type DiagnosticCode = &'static str;

/// JSON envelope schema version. Bumps on breaking diagnostic shape
/// changes. Phase 5A ships at `"1.0"`, matching the Phase 3B shape.
pub const SCHEMA_VERSION: &str = "1.0";

/// Severity rank. Phase 5A emits only [`Severity::Error`]; the lower
/// variants exist so future MC5xxx warnings (e.g., recipe-author lints)
/// can extend the enum without breaking the sort comparator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Severity {
    /// Informational note. Reserved for future use.
    Info = 0,
    /// Warning. Reserved for future use.
    Warning = 1,
    /// Error — the recipe cannot proceed without remediation.
    Error = 2,
}

impl Severity {
    /// Lowercase machine-readable label, per the Stream B diagnostic
    /// envelope specification. (Distinct from `mc-model::Severity::label`,
    /// which capitalizes for the human-readable text format.)
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// One MC5xxx diagnostic. Five fields: `code`, `severity`, `path`,
/// `message`. Fields are public — the JSON envelope writes them
/// verbatim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// Stable MC5xxx code.
    pub code: DiagnosticCode,
    /// Severity (Phase 5A always [`Severity::Error`]).
    pub severity: Severity,
    /// JSON pointer into the recipe YAML (e.g., `"/columns/2/dimension"`).
    pub path: String,
    /// Human-readable message. Free-form prose; named fields embedded
    /// via `Display` for stability.
    pub message: String,
}

/// Deterministic emission order — apply BEFORE any formatter runs.
///
/// Sort by:
///
/// 1. `severity` desc (errors first).
/// 2. `code` asc (`MC5001` before `MC5002`).
/// 3. `path` asc (earlier YAML nodes first).
/// 4. `message` asc (final tiebreaker).
///
/// This matches `mc-model::sort_diagnostics`'s policy so recipe and
/// model diagnostics interleave predictably when both layers report
/// against a single import attempt.
pub fn sort_diagnostics(diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_by(|a, b| match (b.severity as u8).cmp(&(a.severity as u8)) {
        Ordering::Equal => match a.code.cmp(b.code) {
            Ordering::Equal => match a.path.cmp(&b.path) {
                Ordering::Equal => a.message.cmp(&b.message),
                other => other,
            },
            other => other,
        },
        other => other,
    });
}

/// Render a sorted slice of diagnostics as the JSON envelope:
///
/// ```json
/// {
///   "schema_version": "1.0",
///   "diagnostics": [
///     {
///       "code": "MC5004",
///       "severity": "error",
///       "path": "/columns/2/dimension",
///       "message": "..."
///     }
///   ]
/// }
/// ```
///
/// `schema_version` is emitted **unconditionally**, including for
/// empty-diagnostic lists (matches Phase 3B amendment #13).
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
    write_json_string(out, &d.path);
    out.push_str(",\n");
    out.push_str(&pad2);
    out.push_str("\"message\": ");
    write_json_string(out, &d.message);
    out.push('\n');
    out.push_str(&pad);
    out.push('}');
}

/// Minimal JSON string encoder. Mirrors
/// `mc-model::diagnostic::write_json_string` so the two crates emit
/// identical escapes; Phase 5A avoids a `serde_json` dependency for the
/// same reason Phase 3B did (Cargo.lock + toolchain story unchanged).
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

    fn diag(code: &'static str, severity: Severity, path: &str, message: &str) -> Diagnostic {
        Diagnostic {
            code,
            severity,
            path: path.to_string(),
            message: message.to_string(),
        }
    }

    #[test]
    fn severity_labels_are_lowercase() {
        assert_eq!(Severity::Error.label(), "error");
        assert_eq!(Severity::Warning.label(), "warning");
        assert_eq!(Severity::Info.label(), "info");
    }

    #[test]
    fn sort_orders_by_severity_desc_then_code_asc() {
        let mut v = vec![
            diag("MC5004", Severity::Error, "/columns/0", "x"),
            diag("MC5001", Severity::Error, "/columns/0", "x"),
            diag("MC5018", Severity::Warning, "/columns/0", "x"),
        ];
        sort_diagnostics(&mut v);
        assert_eq!(v[0].code, "MC5001");
        assert_eq!(v[1].code, "MC5004");
        assert_eq!(v[2].code, "MC5018");
    }

    #[test]
    fn sort_breaks_ties_by_path_then_message() {
        let mut v = vec![
            diag("MC5004", Severity::Error, "/columns/2", "z"),
            diag("MC5004", Severity::Error, "/columns/0", "z"),
            diag("MC5004", Severity::Error, "/columns/1", "y"),
        ];
        sort_diagnostics(&mut v);
        assert_eq!(v[0].path, "/columns/0");
        assert_eq!(v[1].path, "/columns/1");
        assert_eq!(v[2].path, "/columns/2");
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

    #[test]
    fn json_envelope_renders_diagnostic_body() {
        let v = vec![diag(
            "MC5004",
            Severity::Error,
            "/columns/2/dimension",
            "column \"market_region\" references unknown dimension \"Region\"",
        )];
        let json = diagnostics_to_json(&v);
        assert!(json.contains("\"code\": \"MC5004\""));
        assert!(json.contains("\"severity\": \"error\""));
        assert!(json.contains("\"path\": \"/columns/2/dimension\""));
        assert!(json.contains("references unknown dimension"));
    }
}
