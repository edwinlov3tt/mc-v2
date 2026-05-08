//! Diagnostic codes MC7001–MC7044 for the narrative template engine.
//!
//! Phase 7A.1: MC7001–MC7010 (template validation).
//! Phase 7A.2: MC7020–MC7025 (interpretation ledger) — in `ledger.rs`.
//! Phase 7A.3: MC7030–MC7032 (cross-period analysis).
//! Phase 7A.4: MC7040–MC7044 (benchmark aggregation) — in `benchmark.rs`.

use thiserror::Error;

/// Narrative engine error — covers template loading, validation, and evaluation.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum NarrativeError {
    /// MC7001: Template references an unknown measure name.
    #[error("MC7001: template `{template_id}` references unknown measure `{measure}`")]
    UnknownMeasure {
        template_id: String,
        measure: String,
    },

    /// MC7002: Template references an unknown dimension name.
    #[error("MC7002: template `{template_id}` references unknown dimension `{dimension}`")]
    UnknownDimension {
        template_id: String,
        dimension: String,
    },

    /// MC7003: `when:` predicate has invalid syntax.
    #[error("MC7003: template `{template_id}` has invalid `when:` predicate: {detail}")]
    InvalidWhenPredicate { template_id: String, detail: String },

    /// MC7004: Format hint references an undefined formatter.
    #[error("MC7004: template `{template_id}` uses unknown format hint `{hint}`")]
    UnknownFormatHint { template_id: String, hint: String },

    /// MC7005: Template body has unresolved `{placeholder}` (binding not declared).
    #[error("MC7005: template `{template_id}` has unresolved placeholder `{{{placeholder}}}`")]
    UnresolvedPlaceholder {
        template_id: String,
        placeholder: String,
    },

    /// MC7006: Template severity is invalid (must be info|success|warning|critical).
    #[error("MC7006: template `{template_id}` has invalid severity `{severity}`")]
    InvalidSeverity {
        template_id: String,
        severity: String,
    },

    /// MC7007: Template family is undeclared.
    #[error("MC7007: template `{template_id}` declares unknown family `{family}`")]
    UnknownFamily { template_id: String, family: String },

    /// MC7008: Template ID collision (two templates with same ID).
    #[error("MC7008: duplicate template ID `{template_id}`")]
    DuplicateTemplateId { template_id: String },

    /// MC7009: Section reference in `reports:` is undefined.
    #[error("MC7009: report references undefined section `{section}`")]
    UndefinedSection { section: String },

    /// MC7010: notability_base outside [0, 1] range.
    #[error("MC7010: template `{template_id}` has notability_base {value} outside [0, 1]")]
    NotabilityOutOfRange { template_id: String, value: f64 },

    // ─── Phase 7A.3: Cross-period analysis (MC7030–MC7032) ─────────────
    /// MC7030: Cross-period template references itself (cycle detection).
    #[error("MC7030: template `{template_id}` ledger query references its own output (cycle)")]
    LedgerQueryCycle { template_id: String },

    /// MC7031: Ledger lookback exceeds available depth (warning-level).
    #[error(
        "MC7031: template `{template_id}` requested {requested} periods but ledger has only {available}"
    )]
    LedgerLookbackExceedsDepth {
        template_id: String,
        requested: usize,
        available: usize,
    },

    /// MC7032: Cross-period template references a template_id not in the template set.
    #[error("MC7032: trend template `{template_id}` references unknown template_id `{referenced}` in ledger query")]
    LedgerUnknownTemplateRef {
        template_id: String,
        referenced: String,
    },

    /// Template file I/O error.
    #[error("cannot read template file `{path}`: {detail}")]
    IoError { path: String, detail: String },

    /// Template YAML parse error.
    #[error("cannot parse template file `{path}`: {detail}")]
    ParseError { path: String, detail: String },
}
