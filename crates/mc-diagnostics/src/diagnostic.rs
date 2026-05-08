//! Rich diagnostic types for Rust-style source-span rendering.
//!
//! Per [ADR-0024](../../../docs/decisions/0024-rich-diagnostic-rendering.md)
//! Decision 2: a unified diagnostic type used across all Mosaic surfaces.
//! Diagnostic codes remain in their respective crates; this module provides
//! the shared structural types.

use serde::Serialize;

use crate::SourceSpan;

/// A rich diagnostic with optional source spans, related locations, and
/// machine-applicable suggestions.
///
/// Constructed by diagnostic sites in `mc-model`, `mc-narrative`, etc.
/// Rendered by [`render_diagnostic`](crate::render_diagnostic).
#[derive(Debug, Clone, Serialize)]
pub struct RichDiagnostic {
    /// Stable diagnostic code (e.g., `"MC2015"`).
    pub code: String,
    /// Severity level.
    pub severity: DiagSeverity,
    /// Human-readable message.
    pub message: String,
    /// Primary source span (the error site).
    pub primary_span: Option<SourceSpan>,
    /// Additional related locations (e.g., MC7050 cross-template collision).
    pub related: Vec<RelatedSpan>,
    /// Informational notes appended after the source context.
    pub notes: Vec<String>,
    /// Help text appended after notes.
    pub help: Vec<String>,
    /// Optional machine-applicable suggestion.
    pub suggestion: Option<Suggestion>,
}

impl RichDiagnostic {
    /// Create a minimal diagnostic with just code, severity, and message.
    pub fn new(
        code: impl Into<String>,
        severity: DiagSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            severity,
            message: message.into(),
            primary_span: None,
            related: Vec::new(),
            notes: Vec::new(),
            help: Vec::new(),
            suggestion: None,
        }
    }

    /// Attach a primary source span.
    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.primary_span = Some(span);
        self
    }

    /// Add a related span.
    pub fn with_related(mut self, span: SourceSpan, label: impl Into<String>) -> Self {
        self.related.push(RelatedSpan {
            span,
            label: label.into(),
        });
        self
    }

    /// Add a note.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Add a help message.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help.push(help.into());
        self
    }

    /// Attach a suggestion.
    pub fn with_suggestion(mut self, suggestion: Suggestion) -> Self {
        self.suggestion = Some(suggestion);
        self
    }
}

/// A related source location with a label.
#[derive(Debug, Clone, Serialize)]
pub struct RelatedSpan {
    /// The source span of the related location.
    pub span: SourceSpan,
    /// Label describing why this location is relevant.
    pub label: String,
}

/// A suggestion for fixing the diagnostic.
#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    /// Human-readable description of the fix.
    pub message: String,
    /// The kind of suggestion.
    pub kind: SuggestionKind,
}

/// The kind of fix suggestion.
#[derive(Debug, Clone, Serialize)]
pub enum SuggestionKind {
    /// Free-form help text.
    Help(String),
    /// Machine-applicable: replace the span content with `replacement`.
    Replace {
        /// The span to replace.
        span: SourceSpan,
        /// The replacement text.
        replacement: String,
    },
}

/// Diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DiagSeverity {
    /// Fatal — the model cannot proceed.
    Error,
    /// Non-fatal — the model loads but has quality issues.
    Warning,
    /// Informational — no action required.
    Info,
}

impl DiagSeverity {
    /// Lower-case label for rendering.
    pub fn label(self) -> &'static str {
        match self {
            DiagSeverity::Error => "error",
            DiagSeverity::Warning => "warning",
            DiagSeverity::Info => "info",
        }
    }
}

impl std::fmt::Display for DiagSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}
