//! Workspace-level diagnostic types (MC5xxx namespace).
//!
//! Follows the same diagnostic patterns as `mc-model` (MC1xxx–MC4xxx)
//! per ADR-0005. The MC5xxx range is reserved for workspace-level issues.

use thiserror::Error;

/// Workspace-level error. Covers parse failures, missing references,
/// broken links, and consistency warnings.
///
/// | Code   | Variant                           | Severity |
/// |--------|-----------------------------------|----------|
/// | MC5001 | `ManifestParseFailure`            | Error    |
/// | MC5002 | `CubeFileNotFound`                | Error    |
/// | MC5003 | `RefTargetNotFound`               | Error    |
/// | MC5004 | `LinkReferencesNonexistent`        | Error    |
/// | MC5005 | `NamingInconsistency`             | Warning  |
/// | MC5006 | `UnusedSharedCatalog`             | Warning  |
/// | MC5007 | `CubeNotInManifest`               | Warning  |
/// | MC5008 | `NoGoldenSuites`                  | Info     |
/// | MC5009 | `DuplicateCubePath`               | Error    |
/// | MC5010 | `DimensionNamingDrift`            | Warning  |
/// | MC5011 | `MeasureNameCollisionNoLink`      | Warning  |
/// | MC5012 | `CubeNoDescription`               | Info     |
/// | MC5013 | `PathEscapesWorkspace`            | Error    |
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum WorkspaceError {
    /// **MC5001** — workspace.yaml or org.yaml failed to parse.
    #[error("manifest parse failure: {message}")]
    ManifestParseFailure { message: String },

    /// **MC5002** — A cube entry's path doesn't resolve to a file.
    #[error("cube file not found: {path}")]
    CubeFileNotFound { path: String },

    /// **MC5003** — A `$ref` target ID is not in the workspace's
    /// shared resources.
    #[error("$ref target {ref_id:?} not found in workspace shared resources")]
    RefTargetNotFound { ref_id: String, cube: String },

    /// **MC5004** — A link references a cube or measure that doesn't exist.
    #[error("link references nonexistent {kind}: {name:?} in cube {cube:?}")]
    LinkReferencesNonexistent {
        kind: String,
        name: String,
        cube: String,
    },

    /// **MC5005** — Same concept has different names across cubes.
    #[error("naming inconsistency: {message}")]
    NamingInconsistency { message: String },

    /// **MC5006** — A shared catalog is declared but no cube references it.
    #[error("shared catalog {catalog_id:?} is not referenced by any cube")]
    UnusedSharedCatalog { catalog_id: String },

    /// **MC5007** — A cube YAML exists in the workspace dir but is not
    /// listed in the manifest.
    #[error("cube file {path:?} not listed in workspace manifest")]
    CubeNotInManifest { path: String },

    /// **MC5008** — The workspace has no golden suites defined.
    #[error("workspace has no golden_suites defined")]
    NoGoldenSuites,

    /// **MC5009** — Two cube entries in the manifest share the same path.
    #[error("duplicate cube path: {path:?}")]
    DuplicateCubePath { path: String },

    /// **MC5013** — A cube or catalog path resolves outside the workspace
    /// directory boundary. Per ADR-0026 Decision 5: scope violations fail
    /// closed.
    #[error("path {path:?} escapes workspace directory")]
    PathEscapesWorkspace { path: String },

    /// **MC5010** — Dimension naming drift across cubes.
    #[error("dimension naming drift: {message}")]
    DimensionNamingDrift { message: String },

    /// **MC5011** — Measure name collision across cubes without explicit link.
    #[error("measure {measure:?} appears in cubes {cubes:?} without an explicit link declaration")]
    MeasureNameCollisionNoLink { measure: String, cubes: Vec<String> },

    /// **MC5012** — Cube has no description field.
    #[error("cube {cube:?} has no description")]
    CubeNoDescription { cube: String },
}

impl WorkspaceError {
    /// Stable diagnostic code (MC5xxx).
    pub fn code(&self) -> &'static str {
        match self {
            WorkspaceError::ManifestParseFailure { .. } => "MC5001",
            WorkspaceError::CubeFileNotFound { .. } => "MC5002",
            WorkspaceError::RefTargetNotFound { .. } => "MC5003",
            WorkspaceError::LinkReferencesNonexistent { .. } => "MC5004",
            WorkspaceError::NamingInconsistency { .. } => "MC5005",
            WorkspaceError::UnusedSharedCatalog { .. } => "MC5006",
            WorkspaceError::CubeNotInManifest { .. } => "MC5007",
            WorkspaceError::NoGoldenSuites => "MC5008",
            WorkspaceError::DuplicateCubePath { .. } => "MC5009",
            WorkspaceError::DimensionNamingDrift { .. } => "MC5010",
            WorkspaceError::MeasureNameCollisionNoLink { .. } => "MC5011",
            WorkspaceError::CubeNoDescription { .. } => "MC5012",
            WorkspaceError::PathEscapesWorkspace { .. } => "MC5013",
        }
    }

    /// Severity per the handoff spec table.
    pub fn severity(&self) -> Severity {
        match self {
            WorkspaceError::ManifestParseFailure { .. }
            | WorkspaceError::CubeFileNotFound { .. }
            | WorkspaceError::RefTargetNotFound { .. }
            | WorkspaceError::LinkReferencesNonexistent { .. }
            | WorkspaceError::DuplicateCubePath { .. }
            | WorkspaceError::PathEscapesWorkspace { .. } => Severity::Error,
            WorkspaceError::NamingInconsistency { .. }
            | WorkspaceError::UnusedSharedCatalog { .. }
            | WorkspaceError::CubeNotInManifest { .. }
            | WorkspaceError::DimensionNamingDrift { .. }
            | WorkspaceError::MeasureNameCollisionNoLink { .. } => Severity::Warning,
            WorkspaceError::NoGoldenSuites | WorkspaceError::CubeNoDescription { .. } => {
                Severity::Info
            }
        }
    }
}

/// Severity levels matching mc-model's diagnostic system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Severity {
    Info = 0,
    Warning = 1,
    Error = 2,
}

impl Severity {
    /// Lower-case label for output.
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

/// One workspace diagnostic. Same shape as `mc_model::Diagnostic` but
/// carries workspace-specific context.
#[derive(Clone, Debug)]
pub struct WorkspaceDiagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
}

impl WorkspaceDiagnostic {
    /// Build from a `WorkspaceError`.
    pub fn from_error(err: &WorkspaceError) -> Self {
        Self {
            code: err.code(),
            severity: err.severity(),
            message: err.to_string(),
        }
    }
}

/// Render diagnostics as human-readable text.
pub fn diagnostics_to_text(diags: &[WorkspaceDiagnostic]) -> String {
    let mut out = String::new();
    for (i, d) in diags.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(d.code);
        out.push_str(" [");
        out.push_str(d.severity.label());
        out.push_str("] ");
        out.push_str(&d.message);
        out.push('\n');
    }
    out
}

/// Render diagnostics as JSON envelope matching mc-model's schema.
pub fn diagnostics_to_json(diags: &[WorkspaceDiagnostic]) -> String {
    let mut out = String::new();
    out.push_str("{\n  \"schema_version\": \"1.0\",\n  \"diagnostics\": [");
    if diags.is_empty() {
        out.push_str("]\n}\n");
        return out;
    }
    out.push('\n');
    for (i, d) in diags.iter().enumerate() {
        out.push_str("    {\"code\": ");
        write_json_str(&mut out, d.code);
        out.push_str(", \"severity\": ");
        write_json_str(&mut out, d.severity.label());
        out.push_str(", \"message\": ");
        write_json_str(&mut out, &d.message);
        out.push('}');
        if i + 1 < diags.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]\n}\n");
    out
}

/// Sort diagnostics by severity (desc), code (asc), message (asc).
pub fn sort_diagnostics(diags: &mut [WorkspaceDiagnostic]) {
    diags.sort_by(|a, b| {
        (b.severity as u8)
            .cmp(&(a.severity as u8))
            .then_with(|| a.code.cmp(b.code))
            .then_with(|| a.message.cmp(&b.message))
    });
}

fn write_json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}
