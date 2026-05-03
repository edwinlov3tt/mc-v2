//! Error types for the three pipeline stages.
//!
//! Each stage has distinct blame semantics (ADR-0004 Decision 9):
//!
//! - [`ParseError`] blames YAML syntax (anchor / merge-key / unparseable
//!   structure / unexpected EOF). Surfaced with file:line:column when
//!   `serde_yaml` exposes a span.
//! - [`ValidationError`] blames model semantics (duplicate names, unknown
//!   measure references, hierarchy cycles, …). Decision 6 enumerates the
//!   full validator surface.
//! - The compile stage forwards `mc_core::EngineError` as a string; the
//!   only way it should fire is `EngineError::Internal`-class problems.

use std::path::PathBuf;

use thiserror::Error;

/// Source span carried by parse + validation errors. Optional because some
/// `serde_yaml` errors do not carry a location, and validators that synthesize
/// errors from in-memory `ParsedModel`s have no source location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub file: Option<PathBuf>,
    pub line: usize,
    pub column: usize,
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.file {
            Some(p) => write!(f, "{}:{}:{}", p.display(), self.line, self.column),
            None => write!(f, "<input>:{}:{}", self.line, self.column),
        }
    }
}

/// Stage 1 errors: YAML syntax + safe-subset rejections.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ParseError {
    /// YAML document failed to deserialize. The inner `serde_yaml::Error`
    /// is reformatted as a string; the span (if available) is extracted
    /// into `span` so callers don't depend on `serde_yaml` internals.
    #[error("yaml syntax error at {span}: {message}")]
    Syntax { span: Span, message: String },

    /// One of the safe-subset prohibitions tripped. Distinguished from
    /// `Syntax` so authors can tell "your YAML is invalid" from "your
    /// YAML is valid but uses a feature we banned."
    #[error("yaml safe-subset violation at {span}: {kind}")]
    SafeSubset { span: Span, kind: ParseErrorKind },
}

/// Why a safe-subset prefilter rejected the YAML. ADR-0004 Decision 1
/// bans anchors, aliases, merge keys, and custom tags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseErrorKind {
    Anchor,
    Alias,
    MergeKey,
    CustomTag,
}

impl std::fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseErrorKind::Anchor => write!(f, "anchors (`&name`) are not allowed"),
            ParseErrorKind::Alias => write!(f, "aliases (`*name`) are not allowed"),
            ParseErrorKind::MergeKey => write!(f, "merge keys (`<<:`) are not allowed"),
            ParseErrorKind::CustomTag => {
                write!(f, "custom tags (`!Foo` / `!!Foo`) are not allowed")
            }
        }
    }
}

/// Stage 2 errors: semantic validation. One variant per Decision 6 row.
/// Multiple `ValidationError`s are returned at once from `validate` so a
/// user editing a 500-line YAML sees every problem in one pass.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValidationError {
    /// Two dimensions / two elements within a dim / two measures / two rules
    /// share a name.
    #[error("duplicate {kind} name {name:?} (first defined elsewhere; redefined here)")]
    DuplicateName { kind: String, name: String },

    /// A hierarchy / measure / rule references a dimension name that isn't
    /// declared at the top level.
    #[error("dimension {name:?} referenced by {referenced_by} but not declared")]
    MissingDimension { name: String, referenced_by: String },

    /// A hierarchy edge endpoint isn't an element of the parent dim.
    #[error("hierarchy edge in dim {dim:?} references unknown element {element:?}")]
    InvalidHierarchyEdge { dim: String, element: String },

    /// A → B → A cycle in a hierarchy. Path is rendered as `a -> b -> a`.
    #[error("hierarchy cycle in dim {dim:?}: {path}")]
    HierarchyCycle { dim: String, path: String },

    /// `body` references a measure name that isn't declared in `measures:`.
    #[error("rule {rule_name:?} references unknown measure {measure_name:?}")]
    RuleReferencesUnknownMeasure {
        rule_name: String,
        measure_name: String,
    },

    /// A measure declared `role: derived` has no rule targeting it. Without
    /// this the cell is permanently `Null` (silent kernel failure).
    #[error("derived measure {measure_name:?} has no rule targeting it")]
    DerivedMeasureWithoutRule { measure_name: String },

    /// A measure declared `role: input` has a rule targeting it.
    #[error("input measure {measure_name:?} cannot be the target of rule {rule_name:?}")]
    InputMeasureHasRule {
        measure_name: String,
        rule_name: String,
    },

    /// Cycle detected in the rule dependency graph: rule R1 reads M2 →
    /// rule R2 targets M2 reads M3 → rule R3 targets M3 reads M1 (the
    /// measure R1 targets).
    #[error("rule dependency cycle: {path}")]
    RuleCycle { path: String },

    /// A measure declared an aggregation method that the kernel's
    /// `AggregationRule` enum doesn't implement.
    #[error("measure {measure_name:?} declared unsupported aggregation {method:?}")]
    UnsupportedAggregation {
        measure_name: String,
        method: String,
    },

    /// Generic schema misshape: a required field was missing, a required
    /// type didn't match, a `model_format_version` other than 1, etc. The
    /// validator surfaces these alongside the targeted Decision 6 errors so
    /// authors don't bounce through a half-validated model.
    #[error("schema error: {message}")]
    Schema { message: String },
}

/// Top-level error wrapper. `load` and `load_str` return `Vec<Error>` so
/// the caller can still see all of stage-2's accumulated errors when
/// validation fails partway through the pipeline.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("io error reading {path}: {message}")]
    Io { path: String, message: String },

    #[error(transparent)]
    Parse(#[from] ParseError),

    #[error(transparent)]
    Validation(#[from] ValidationError),

    /// `mc_core::EngineError` rendered as a string — the compile stage
    /// should not normally fail (a `ValidatedModel` is by construction
    /// buildable); when it does, the kernel error propagates here.
    #[error("compile error (kernel): {0}")]
    Compile(String),
}
