//! Diagnostic codes MC7001–MC7055 for the narrative template engine.
//!
//! Phase 7A.1: MC7001–MC7010 (template validation).
//! Phase 7A.2: MC7020–MC7025 (interpretation ledger) — in `ledger.rs`.
//! Phase 7A.3: MC7030–MC7032 (cross-period analysis).
//! Phase 7A.4: MC7040–MC7044 (benchmark aggregation) — in `benchmark.rs`.
//! Phase 7A.5: MC7050–MC7055 (explanation chains + context events).

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

    // ─── Phase 7A.5: Explanation chains (MC7050–MC7055) ─────────────
    /// MC7050: Two templates share the same `finding_id` AND `explanation_priority`.
    /// Per ADR-0022 Decision 2: deterministic output requires deterministic order.
    #[error("MC7050: templates `{template_a}` and `{template_b}` share finding_id `{finding_id}` with same priority {priority}")]
    ExplanationPriorityCollision {
        finding_id: String,
        priority: u32,
        template_a: String,
        template_b: String,
    },

    /// MC7051: Context event references a period not present in any loaded cube.
    /// Per ADR-0022 Decision 10: warning-level.
    #[error(
        "MC7051: context event `{event_id}` references period `{period}` not in any loaded cube"
    )]
    ContextEventUnknownPeriod { event_id: String, period: String },

    /// MC7052: Context event `expires_at` is before its `period`.
    /// Per ADR-0022 Decision 10: warning-level.
    #[error(
        "MC7052: context event `{event_id}` expires_at `{expires_at}` is before period `{period}`"
    )]
    ContextEventExpiresBeforePeriod {
        event_id: String,
        period: String,
        expires_at: String,
    },

    /// MC7053: A `finding_id` group has no template with `explanation_priority >= 900`
    /// (missing fallback). Per ADR-0022 Decision 3: info-level nudge.
    #[error("MC7053: finding_id `{finding_id}` has no fallback template (priority >= 900)")]
    ExplanationMissingFallback { finding_id: String },

    /// MC7055: A `finding_id` is referenced by only one template (likely typo).
    #[error("MC7055: finding_id `{finding_id}` is referenced by only template `{template_id}` (likely typo — intended to be part of a group)")]
    ExplanationSingletonFindingId {
        finding_id: String,
        template_id: String,
    },

    /// Template file I/O error.
    #[error("cannot read template file `{path}`: {detail}")]
    IoError { path: String, detail: String },

    /// Template YAML parse error.
    #[error("cannot parse template file `{path}`: {detail}")]
    ParseError { path: String, detail: String },
}

impl NarrativeError {
    /// Stable diagnostic code for this error.
    pub fn code(&self) -> &'static str {
        match self {
            NarrativeError::UnknownMeasure { .. } => "MC7001",
            NarrativeError::UnknownDimension { .. } => "MC7002",
            NarrativeError::InvalidWhenPredicate { .. } => "MC7003",
            NarrativeError::UnknownFormatHint { .. } => "MC7004",
            NarrativeError::UnresolvedPlaceholder { .. } => "MC7005",
            NarrativeError::InvalidSeverity { .. } => "MC7006",
            NarrativeError::UnknownFamily { .. } => "MC7007",
            NarrativeError::DuplicateTemplateId { .. } => "MC7008",
            NarrativeError::UndefinedSection { .. } => "MC7009",
            NarrativeError::NotabilityOutOfRange { .. } => "MC7010",
            NarrativeError::LedgerQueryCycle { .. } => "MC7030",
            NarrativeError::LedgerLookbackExceedsDepth { .. } => "MC7031",
            NarrativeError::LedgerUnknownTemplateRef { .. } => "MC7032",
            NarrativeError::ExplanationPriorityCollision { .. } => "MC7050",
            NarrativeError::ContextEventUnknownPeriod { .. } => "MC7051",
            NarrativeError::ContextEventExpiresBeforePeriod { .. } => "MC7052",
            NarrativeError::ExplanationMissingFallback { .. } => "MC7053",
            NarrativeError::ExplanationSingletonFindingId { .. } => "MC7055",
            NarrativeError::IoError { .. } => "MC7098",
            NarrativeError::ParseError { .. } => "MC7099",
        }
    }

    /// Convert to a `RichDiagnostic` for rich rendering.
    ///
    /// Phase 7A.6: narrative errors gain rich diagnostic support. The
    /// MC7050 priority-collision case is the canonical multi-location
    /// diagnostic with two related spans.
    pub fn to_rich(&self) -> mc_diagnostics::RichDiagnostic {
        let severity = match self {
            NarrativeError::ContextEventUnknownPeriod { .. }
            | NarrativeError::ContextEventExpiresBeforePeriod { .. }
            | NarrativeError::LedgerLookbackExceedsDepth { .. }
            | NarrativeError::ExplanationMissingFallback { .. }
            | NarrativeError::ExplanationSingletonFindingId { .. } => {
                mc_diagnostics::DiagSeverity::Warning
            }
            _ => mc_diagnostics::DiagSeverity::Error,
        };

        mc_diagnostics::RichDiagnostic::new(self.code(), severity, self.to_string())
    }
}
