//! Diagnostic codes MC7001–MC7010 for the narrative template engine.
//!
//! These codes are reserved per the Phase 7A diagnostic namespace.
//! Phase 7A.1 allocates MC7001–MC7010; higher codes are reserved
//! for Phases 7A.2–7A.4.

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

    /// Template file I/O error.
    #[error("cannot read template file `{path}`: {detail}")]
    IoError { path: String, detail: String },

    /// Template YAML parse error.
    #[error("cannot parse template file `{path}`: {detail}")]
    ParseError { path: String, detail: String },
}
