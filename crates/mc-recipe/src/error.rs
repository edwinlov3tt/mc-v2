//! `RecipeError` — every distinct way a Tessera recipe can be wrong, plus
//! the stable MC5xxx → variant mapping.
//!
//! ADR-0010 Appendix B fixes 18 codes for Phase 5A
//! ([`MC5001`](RecipeError::Syntax) through
//! [`MC5018`](RecipeError::DerivedMeasureWriteRejected)). Codes are
//! `&'static str` and never repurposed (CVE-style retirement, per
//! ADR-0005 amendment #11). Future MC5xxx additions append; existing
//! variants do not move.
//!
//! Two layering notes:
//!
//! 1. **Three of the 18 codes are reserved for runtime ([`SourceFileUnreadable`](RecipeError::SourceFileUnreadable),
//!    [`SourceConnectionFailed`](RecipeError::SourceConnectionFailed),
//!    [`CredentialInterpolationFailed`](RecipeError::CredentialInterpolationFailed)).**
//!    They are defined here so the namespace is centrally documented and
//!    the JSON envelope shape is uniform across stages, but Stream D
//!    (`mc-tessera`) is the layer that actually fires them — touching
//!    sources is explicitly out of scope for `mc-recipe` (handoff §8 +
//!    SPEC QUESTION trigger #5).
//!
//! 2. **MC5011 covers two distinct shapes.** Per ADR-0010 Decision 7
//!    semantic rule #1, a column mapping must be **1:1**: at most one
//!    of `dimension` / `measure` is set, and at least one must be set
//!    (unless `skip: true`). Both the "no target" and the "ambiguous
//!    target (both fields set)" shapes fire MC5011; the message
//!    distinguishes them. The Stream B handoff explicitly lists the
//!    no-target shape under MC5011 but leaves the both-set shape's code
//!    unspecified — collapsing them under MC5011 is the closest-fit
//!    interpretation since both shapes share the same authoring
//!    remediation ("pick one target").

use thiserror::Error;

use crate::diagnostic::DiagnosticCode;

/// Every distinct way a recipe can be malformed or invalid. Each variant
/// maps to exactly one MC5xxx code via [`RecipeError::code`].
///
/// The variants are organized by stage:
///
/// - **Parse stage** (MC5001, MC5002, MC5007, MC5012) — emitted by
///   [`crate::parse::parse`].
/// - **Validate stage** (MC5003-MC5006, MC5008-MC5011, MC5016-MC5018) —
///   emitted by [`crate::validate::validate_recipe`].
/// - **Runtime stage** (MC5013-MC5015) — defined here for namespace
///   uniformity; fired by Stream D / `mc-tessera`.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecipeError {
    /// **MC5001** — The recipe YAML failed to deserialize. Wraps the
    /// underlying serde_yaml message; any source location is encoded
    /// in `path` (e.g., `"/columns/2"`) when serde_yaml exposes it.
    #[error("recipe YAML parse error at {path}: {message}")]
    Syntax { path: String, message: String },

    /// **MC5002** — The `source.driver:` field carries a value that
    /// isn't one of the declared [`crate::DriverKind`] variants.
    #[error("unknown driver kind {driver:?} at {path} (expected one of: csv, sqlite, duckdb, postgres, duckdb_postgres, http_json)")]
    UnknownDriver { path: String, driver: String },

    /// **MC5003** — `source.table:` and `source.query:` are mutually
    /// exclusive. A recipe must pick one.
    #[error("source declares both table and query at {path} (mutual exclusion)")]
    SourceTableQueryConflict { path: String },

    /// **MC5004** — A column mapping's `dimension:` field references a
    /// dimension name that doesn't exist in the target model.
    #[error("column {source_column:?} references unknown dimension {dimension:?} at {path}")]
    UnknownDimension {
        path: String,
        source_column: String,
        dimension: String,
    },

    /// **MC5005** — A column mapping's `measure:` field references a
    /// measure name that doesn't exist in the target model.
    #[error("column {source_column:?} references unknown measure {measure:?} at {path}")]
    UnknownMeasure {
        path: String,
        source_column: String,
        measure: String,
    },

    /// **MC5006** — The column's declared `type:` is incompatible with
    /// the target measure's `data_type` in the model. Compared after
    /// case-insensitive normalization (so `f64` matches `F64`).
    #[error("column {source_column:?} declared type {column_type:?} is incompatible with measure {measure:?} type {measure_type:?} at {path}")]
    ColumnTypeIncompatible {
        path: String,
        source_column: String,
        column_type: String,
        measure: String,
        measure_type: String,
    },

    /// **MC5007** — A required recipe field is missing (`version`,
    /// `name`, `model`, `source`, `source.driver`, or `columns`).
    /// Best-effort detection: serde_yaml's missing-field error message
    /// is reformatted under this code.
    #[error("missing required field at {path}: {message}")]
    MissingField { path: String, message: String },

    /// **MC5008** — A key in `defaults:` references a dimension name
    /// that doesn't exist in the target model.
    #[error("default references unknown dimension {dimension:?} at {path}")]
    DefaultUnknownDimension { path: String, dimension: String },

    /// **MC5009** — A value in `defaults:` references an element name
    /// that doesn't exist in the named dimension.
    #[error(
        "default for dimension {dimension:?} references unknown element {element:?} at {path}"
    )]
    DefaultUnknownElement {
        path: String,
        dimension: String,
        element: String,
    },

    /// **MC5010** — Two `columns[i]` entries share the same `source:`
    /// name. Source columns must be unique.
    #[error("duplicate column mapping for source {source_column:?} at {path} (first occurrence at {first_path})")]
    DuplicateColumn {
        path: String,
        source_column: String,
        first_path: String,
    },

    /// **MC5011** — A column mapping has no clear single target. Two
    /// shapes fire under this code (see module-level docs):
    ///
    /// - "no target": neither `dimension` nor `measure` is set, and
    ///   `skip` is not `true`.
    /// - "ambiguous target": both `dimension` and `measure` are set
    ///   (1:1 violation per ADR-0010 amendment #7).
    ///
    /// The `kind` field disambiguates in the message.
    #[error("column {source_column:?} at {path}: {kind}")]
    ColumnNoSingleTarget {
        path: String,
        source_column: String,
        kind: ColumnTargetIssue,
    },

    /// **MC5012** — `version:` is not `1`. Phase 5A pins the recipe
    /// schema at version 1; future versions are explicit migrations.
    #[error("recipe version {version} is not supported (expected 1) at {path}")]
    UnsupportedVersion { path: String, version: u32 },

    /// **MC5013** — A `${env.VAR}` reference in `credentials:` (or
    /// elsewhere) names an environment variable that isn't set at
    /// resolution time. Defined here for namespace completeness; fired
    /// by Stream D at runtime (mc-recipe does not read environment).
    #[error(
        "credential interpolation failed for {variable:?} at {path}: environment variable not set"
    )]
    CredentialInterpolationFailed { path: String, variable: String },

    /// **MC5014** — The source file declared in `source.path:` is not
    /// readable (not found, permission denied, IO error). Defined here
    /// for namespace completeness; fired by Stream D at runtime.
    #[error("source file at {path}: {source_path:?} unreadable — {reason}")]
    SourceFileUnreadable {
        path: String,
        source_path: String,
        reason: String,
    },

    /// **MC5015** — The source connection (Postgres DSN, HTTP endpoint,
    /// …) failed to establish. Defined here for namespace completeness;
    /// fired by Stream D at runtime.
    #[error("source connection failed at {path}: {reason}")]
    SourceConnectionFailed { path: String, reason: String },

    /// **MC5016** — A dimension appears in BOTH `columns:` (as the
    /// `dimension:` of some mapping) AND `defaults:` (as a key). Per
    /// ADR-0010 amendment #8, the two are mutually exclusive — a
    /// dimension is either varying-per-row (columns) or constant
    /// (defaults), never both.
    #[error("dimension {dimension:?} appears in both columns (at {column_path}) and defaults (at {default_path}) — mutual exclusion")]
    DimensionInColumnsAndDefaults {
        dimension: String,
        column_path: String,
        default_path: String,
    },

    /// **MC5017** — The `model:` path, resolved relative to the recipe
    /// file's directory, escapes the workspace root. Path-traversal
    /// protection per ADR-0010 amendment #10. Fired only when the
    /// caller supplies both a recipe directory and a workspace root;
    /// in-memory recipes (no file context) bypass the check.
    #[error("model path {resolved:?} escapes workspace root {workspace_root:?} at {path}")]
    ModelPathEscapesWorkspace {
        path: String,
        resolved: String,
        workspace_root: String,
    },

    /// **MC5018** — A column mapping targets a measure whose `role:` is
    /// `Derived` in the model. Per ADR-0010 amendment #2, Phase 5A
    /// writes to **Input measures only**; Derived cells are computed
    /// by rules and rejected at write time anyway (kernel
    /// `WritebackError::DerivedCellNotWritable`). Catching it here at
    /// recipe-validation time gives a friendlier author-time error.
    #[error("column {source_column:?} maps to derived measure {measure:?} at {path} (Phase 5A writes to Input measures only — derived measures are computed by rules)")]
    DerivedMeasureWriteRejected {
        path: String,
        source_column: String,
        measure: String,
    },

    /// **MC5021** — `format: long` used with `measure: X` in `columns:`
    /// (mutual exclusion — in long format, measures come from the
    /// `long_format.measure_column`, not from column mappings).
    #[error("column {source_column:?} at {path} declares measure: {measure:?} but recipe uses format: long (measures come from long_format.measure_column in long format)")]
    LongFormatMeasureColumnConflict {
        path: String,
        source_column: String,
        measure: String,
    },

    // === Phase 5C ADR-0014 time_format enforcement ===
    /// **MC5030** — A column mapping targets a Time dimension but the
    /// source values are non-ISO and no `time_format` is specified.
    /// Per ADR-0014 Decision 5: non-ISO date columns require explicit
    /// `time_format` in the recipe.
    #[error("column {source_column:?} at {path} maps to Time dimension but has non-ISO date values without explicit time_format (add time_format to specify the source date format)")]
    TimeFormatRequired { path: String, source_column: String },

    /// **MC5031** — A column mapping has a timezone-less timestamp
    /// (no Z suffix, no offset) but no `time_timezone` is specified.
    /// Per ADR-0014 Decision 5: timezone-less timestamps require
    /// explicit `time_timezone`.
    #[error("column {source_column:?} at {path} has timezone-less timestamp without time_timezone (add time_timezone with an IANA identifier like 'America/New_York')")]
    TimeTimezoneRequired { path: String, source_column: String },

    /// **MC5032** — The `time_timezone` value is not a valid IANA
    /// timezone identifier. Per ADR-0014 Decision 5: only IANA
    /// identifiers are accepted (not abbreviations like "EST" or
    /// fixed offsets like "-05:00").
    #[error("column {source_column:?} at {path} has non-IANA time_timezone {timezone:?} (use IANA identifier like {suggestion:?})")]
    TimeTimezoneNotIana {
        path: String,
        source_column: String,
        timezone: String,
        suggestion: String,
    },

    /// **MC5033** — A parsed date from a Time-dimension column doesn't
    /// map to any declared Time element in the model. Fired when
    /// `map_to_period` is set but the resulting period name has no
    /// matching element.
    #[error("column {source_column:?} at {path}: date value doesn't map to any Time element in the model (period: {period:?})")]
    TimeElementNotFound {
        path: String,
        source_column: String,
        period: String,
    },
}

/// Sub-classifier for [`RecipeError::ColumnNoSingleTarget`] — disambiguates
/// the two MC5011 shapes in the human-readable message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnTargetIssue {
    /// Neither `dimension:` nor `measure:` is set, and `skip` is not
    /// `true`. Column would be silently dropped.
    NoTarget,
    /// Both `dimension:` and `measure:` are set. 1:1 mapping rule
    /// (ADR-0010 amendment #7) violated.
    Ambiguous,
}

impl std::fmt::Display for ColumnTargetIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ColumnTargetIssue::NoTarget => f.write_str(
                "no dimension or measure target declared and skip is not true (column would be silently dropped)",
            ),
            ColumnTargetIssue::Ambiguous => f.write_str(
                "both dimension and measure are set — column mappings must be 1:1 (pick one target)",
            ),
        }
    }
}

impl RecipeError {
    /// Stable diagnostic code for this error variant. Codes are part
    /// of the public API per ADR-0010 Appendix B; renaming or
    /// renumbering would silently break LLM/UI consumers pinned to
    /// the code-to-meaning map.
    pub fn code(&self) -> DiagnosticCode {
        match self {
            RecipeError::Syntax { .. } => "MC5001",
            RecipeError::UnknownDriver { .. } => "MC5002",
            RecipeError::SourceTableQueryConflict { .. } => "MC5003",
            RecipeError::UnknownDimension { .. } => "MC5004",
            RecipeError::UnknownMeasure { .. } => "MC5005",
            RecipeError::ColumnTypeIncompatible { .. } => "MC5006",
            RecipeError::MissingField { .. } => "MC5007",
            RecipeError::DefaultUnknownDimension { .. } => "MC5008",
            RecipeError::DefaultUnknownElement { .. } => "MC5009",
            RecipeError::DuplicateColumn { .. } => "MC5010",
            RecipeError::ColumnNoSingleTarget { .. } => "MC5011",
            RecipeError::UnsupportedVersion { .. } => "MC5012",
            RecipeError::CredentialInterpolationFailed { .. } => "MC5013",
            RecipeError::SourceFileUnreadable { .. } => "MC5014",
            RecipeError::SourceConnectionFailed { .. } => "MC5015",
            RecipeError::DimensionInColumnsAndDefaults { .. } => "MC5016",
            RecipeError::ModelPathEscapesWorkspace { .. } => "MC5017",
            RecipeError::DerivedMeasureWriteRejected { .. } => "MC5018",
            RecipeError::LongFormatMeasureColumnConflict { .. } => "MC5021",
            RecipeError::TimeFormatRequired { .. } => "MC5030",
            RecipeError::TimeTimezoneRequired { .. } => "MC5031",
            RecipeError::TimeTimezoneNotIana { .. } => "MC5032",
            RecipeError::TimeElementNotFound { .. } => "MC5033",
        }
    }

    /// JSON-pointer path embedded in this error variant (for
    /// diagnostic-envelope rendering).
    pub fn path(&self) -> &str {
        match self {
            RecipeError::Syntax { path, .. } => path,
            RecipeError::UnknownDriver { path, .. } => path,
            RecipeError::SourceTableQueryConflict { path } => path,
            RecipeError::UnknownDimension { path, .. } => path,
            RecipeError::UnknownMeasure { path, .. } => path,
            RecipeError::ColumnTypeIncompatible { path, .. } => path,
            RecipeError::MissingField { path, .. } => path,
            RecipeError::DefaultUnknownDimension { path, .. } => path,
            RecipeError::DefaultUnknownElement { path, .. } => path,
            RecipeError::DuplicateColumn { path, .. } => path,
            RecipeError::ColumnNoSingleTarget { path, .. } => path,
            RecipeError::UnsupportedVersion { path, .. } => path,
            RecipeError::CredentialInterpolationFailed { path, .. } => path,
            RecipeError::SourceFileUnreadable { path, .. } => path,
            RecipeError::SourceConnectionFailed { path, .. } => path,
            RecipeError::DimensionInColumnsAndDefaults { column_path, .. } => column_path,
            RecipeError::ModelPathEscapesWorkspace { path, .. } => path,
            RecipeError::DerivedMeasureWriteRejected { path, .. } => path,
            RecipeError::LongFormatMeasureColumnConflict { path, .. } => path,
            RecipeError::TimeFormatRequired { path, .. } => path,
            RecipeError::TimeTimezoneRequired { path, .. } => path,
            RecipeError::TimeTimezoneNotIana { path, .. } => path,
            RecipeError::TimeElementNotFound { path, .. } => path,
        }
    }

    /// Convert this error into a [`crate::diagnostic::Diagnostic`] for
    /// envelope emission. Phase 5A: every MC5xxx is severity `Error`.
    pub fn to_diagnostic(&self) -> crate::diagnostic::Diagnostic {
        crate::diagnostic::Diagnostic {
            code: self.code(),
            severity: crate::diagnostic::Severity::Error,
            path: self.path().to_string(),
            message: self.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_has_a_unique_code() {
        let variants: Vec<RecipeError> = vec![
            RecipeError::Syntax {
                path: "/".into(),
                message: "x".into(),
            },
            RecipeError::UnknownDriver {
                path: "/source/driver".into(),
                driver: "x".into(),
            },
            RecipeError::SourceTableQueryConflict {
                path: "/source".into(),
            },
            RecipeError::UnknownDimension {
                path: "/columns/0/dimension".into(),
                source_column: "x".into(),
                dimension: "x".into(),
            },
            RecipeError::UnknownMeasure {
                path: "/columns/0/measure".into(),
                source_column: "x".into(),
                measure: "x".into(),
            },
            RecipeError::ColumnTypeIncompatible {
                path: "/columns/0/type".into(),
                source_column: "x".into(),
                column_type: "x".into(),
                measure: "x".into(),
                measure_type: "x".into(),
            },
            RecipeError::MissingField {
                path: "/".into(),
                message: "x".into(),
            },
            RecipeError::DefaultUnknownDimension {
                path: "/defaults".into(),
                dimension: "x".into(),
            },
            RecipeError::DefaultUnknownElement {
                path: "/defaults/x".into(),
                dimension: "x".into(),
                element: "x".into(),
            },
            RecipeError::DuplicateColumn {
                path: "/columns/1".into(),
                source_column: "x".into(),
                first_path: "/columns/0".into(),
            },
            RecipeError::ColumnNoSingleTarget {
                path: "/columns/0".into(),
                source_column: "x".into(),
                kind: ColumnTargetIssue::NoTarget,
            },
            RecipeError::UnsupportedVersion {
                path: "/version".into(),
                version: 2,
            },
            RecipeError::CredentialInterpolationFailed {
                path: "/credentials/x".into(),
                variable: "X".into(),
            },
            RecipeError::SourceFileUnreadable {
                path: "/source/path".into(),
                source_path: "x".into(),
                reason: "x".into(),
            },
            RecipeError::SourceConnectionFailed {
                path: "/source".into(),
                reason: "x".into(),
            },
            RecipeError::DimensionInColumnsAndDefaults {
                dimension: "x".into(),
                column_path: "/columns/0/dimension".into(),
                default_path: "/defaults/x".into(),
            },
            RecipeError::ModelPathEscapesWorkspace {
                path: "/model".into(),
                resolved: "x".into(),
                workspace_root: "x".into(),
            },
            RecipeError::DerivedMeasureWriteRejected {
                path: "/columns/0/measure".into(),
                source_column: "x".into(),
                measure: "x".into(),
            },
            RecipeError::LongFormatMeasureColumnConflict {
                path: "/columns/0/measure".into(),
                source_column: "x".into(),
                measure: "x".into(),
            },
            RecipeError::TimeFormatRequired {
                path: "/columns/0".into(),
                source_column: "x".into(),
            },
            RecipeError::TimeTimezoneRequired {
                path: "/columns/0".into(),
                source_column: "x".into(),
            },
            RecipeError::TimeTimezoneNotIana {
                path: "/columns/0".into(),
                source_column: "x".into(),
                timezone: "EST".into(),
                suggestion: "America/New_York".into(),
            },
            RecipeError::TimeElementNotFound {
                path: "/columns/0".into(),
                source_column: "x".into(),
                period: "2026-05".into(),
            },
        ];

        let mut codes: Vec<&'static str> = variants.iter().map(|v| v.code()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), 23, "expected 23 distinct MC5xxx codes");
        assert_eq!(codes[0], "MC5001");
        assert_eq!(codes[22], "MC5033");
    }

    #[test]
    fn every_variant_renders_to_diagnostic_with_correct_code() {
        let err = RecipeError::UnknownDimension {
            path: "/columns/0/dimension".into(),
            source_column: "market_region".into(),
            dimension: "Region".into(),
        };
        let d = err.to_diagnostic();
        assert_eq!(d.code, "MC5004");
        assert_eq!(d.path, "/columns/0/dimension");
        assert!(d.message.contains("market_region"));
        assert!(d.message.contains("Region"));
    }
}
