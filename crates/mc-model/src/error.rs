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

/// Stage 1 errors: YAML syntax + safe-subset rejections + (Phase 3D)
/// formula-string parse errors.
///
/// Stable diagnostic codes:
///
/// | Code   | Variant                       | Phase  |
/// |--------|-------------------------------|--------|
/// | MC1001 | `Syntax`                      | 3B     |
/// | MC1002 | `SafeSubset`                  | 3B     |
/// | MC1003 | `FormulaUnbalancedParen`      | 3D     |
/// | MC1004 | `FormulaUnexpectedToken`      | 3D     |
/// | MC1005 | `FormulaExpectedExpression`   | 3D     |
/// | MC1006 | `FormulaInvalidNumber`        | 3D     |
///
/// Phase 3E carves out MC1007-MC1009 and MC1013 from the MC1004 catch-all.
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

    /// **MC1003** — A formula string has an unbalanced or unexpected
    /// parenthesis (missing close, stray close, etc.). `rule_name` is
    /// the rule whose body fired; `offset` is the byte offset within
    /// the formula text.
    #[error("rule {rule_name:?} formula at offset {offset}: {message}")]
    FormulaUnbalancedParen {
        span: Span,
        rule_name: String,
        offset: usize,
        message: String,
    },

    /// **MC1004** — A formula string has an unexpected token, including
    /// unknown function calls (per amendment #25, MC1004 is the catch-all
    /// for both shapes in Phase 3D).
    #[error("rule {rule_name:?} formula at offset {offset}: {message}")]
    FormulaUnexpectedToken {
        span: Span,
        rule_name: String,
        offset: usize,
        message: String,
    },

    /// **MC1005** — A formula string ends or breaks where an expression
    /// was expected (e.g., trailing operator: `Spend +`).
    #[error("rule {rule_name:?} formula at offset {offset}: {message}")]
    FormulaExpectedExpression {
        span: Span,
        rule_name: String,
        offset: usize,
        message: String,
    },

    /// **MC1006** — A formula string contains a malformed numeric
    /// literal (e.g., `1..5`, `1e`, `1.2.3`).
    #[error("rule {rule_name:?} formula at offset {offset}: {message}")]
    FormulaInvalidNumber {
        span: Span,
        rule_name: String,
        offset: usize,
        message: String,
    },

    /// **MC1007** — Unknown function call (Phase 3E: split from MC1004).
    #[error("rule {rule_name:?} formula at offset {offset}: {message}")]
    FormulaUnknownFunction {
        span: Span,
        rule_name: String,
        offset: usize,
        message: String,
    },

    /// **MC1008** — Wrong argument count or chained non-associative comparison.
    #[error("rule {rule_name:?} formula at offset {offset}: {message}")]
    FormulaWrongArgCount {
        span: Span,
        rule_name: String,
        offset: usize,
        message: String,
    },

    /// **MC1009** — `actual_ref` called with non-identifier argument.
    #[error("rule {rule_name:?} formula at offset {offset}: {message}")]
    FormulaActualRefNonIdentifier {
        span: Span,
        rule_name: String,
        offset: usize,
        message: String,
    },

    /// **MC1013** — Cross-coordinate function nesting.
    #[error("rule {rule_name:?} formula at offset {offset}: {message}")]
    FormulaCrossCoordNesting {
        span: Span,
        rule_name: String,
        offset: usize,
        message: String,
    },

    /// **MC1024** — String literal appears outside the second arg of
    /// `is_element()` (and the named-arg slot of
    /// benchmark/lookup/bucket/calibrate/predict). Phase 3I item 1 W4
    /// keeps `ScalarValue::Str` out of general expressions.
    #[error("rule {rule_name:?} formula at offset {offset}: {message}")]
    FormulaStringLiteralMisplaced {
        span: Span,
        rule_name: String,
        offset: usize,
        message: String,
    },

    /// **MC1025** — Cross-coord operator used inside a `--where` filter
    /// expression. Filters evaluate against single coordinates;
    /// cross-coord operators are deferred to Phase 3J+. Phase 3I item 8 W2.
    #[error("filter expression: {message}")]
    FormulaCrossCoordInFilter {
        span: Span,
        offset: usize,
        message: String,
    },
}

impl ParseError {
    /// Stable diagnostic code (`MC1xxx`). See the variant table above.
    pub fn code(&self) -> &'static str {
        match self {
            ParseError::Syntax { .. } => "MC1001",
            ParseError::SafeSubset { .. } => "MC1002",
            ParseError::FormulaUnbalancedParen { .. } => "MC1003",
            ParseError::FormulaUnexpectedToken { .. } => "MC1004",
            ParseError::FormulaExpectedExpression { .. } => "MC1005",
            ParseError::FormulaInvalidNumber { .. } => "MC1006",
            ParseError::FormulaUnknownFunction { .. } => "MC1007",
            ParseError::FormulaWrongArgCount { .. } => "MC1008",
            ParseError::FormulaActualRefNonIdentifier { .. } => "MC1009",
            ParseError::FormulaCrossCoordNesting { .. } => "MC1013",
            ParseError::FormulaStringLiteralMisplaced { .. } => "MC1024",
            ParseError::FormulaCrossCoordInFilter { .. } => "MC1025",
        }
    }

    /// Source span carried by the error. Always present — the parser
    /// synthesizes a zero-position span when `serde_yaml` returns no
    /// location.
    pub fn span(&self) -> &Span {
        match self {
            ParseError::Syntax { span, .. }
            | ParseError::SafeSubset { span, .. }
            | ParseError::FormulaUnbalancedParen { span, .. }
            | ParseError::FormulaUnexpectedToken { span, .. }
            | ParseError::FormulaExpectedExpression { span, .. }
            | ParseError::FormulaInvalidNumber { span, .. }
            | ParseError::FormulaUnknownFunction { span, .. }
            | ParseError::FormulaWrongArgCount { span, .. }
            | ParseError::FormulaActualRefNonIdentifier { span, .. }
            | ParseError::FormulaCrossCoordNesting { span, .. }
            | ParseError::FormulaStringLiteralMisplaced { span, .. }
            | ParseError::FormulaCrossCoordInFilter { span, .. } => span,
        }
    }
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

/// Stage 2 errors: semantic validation. One variant per Decision 6 row,
/// plus the Phase 3B promotion (`WeightedAverageMissingWeight` — MC2011)
/// and Phase 3C's fixture/input validators (MC2012–MC2025).
/// Multiple `ValidationError`s are returned at once from `validate` so a
/// user editing a 500-line YAML sees every problem in one pass.
///
/// Each variant carries a stable diagnostic code via [`ValidationError::code`]:
///
/// | Code   | Variant                                 |
/// |--------|-----------------------------------------|
/// | MC2001 | `DuplicateName`                         |
/// | MC2002 | `MissingDimension`                      |
/// | MC2003 | `InvalidHierarchyEdge`                  |
/// | MC2004 | `HierarchyCycle`                        |
/// | MC2005 | `RuleReferencesUnknownMeasure`          |
/// | MC2006 | `DerivedMeasureWithoutRule`             |
/// | MC2007 | `InputMeasureHasRule`                   |
/// | MC2008 | `RuleCycle`                             |
/// | MC2009 | `UnsupportedAggregation`                |
/// | MC2010 | `Schema`                                |
/// | MC2011 | `WeightedAverageMissingWeight` (Phase 3B promotion from MC3008) |
/// | MC2012 | `FixtureUnknownDimensionKey` (Phase 3C) |
/// | MC2013 | `FixtureUnknownElementValue` (Phase 3C) |
/// | MC2014 | `FixtureUnknownMeasure` (Phase 3C)      |
/// | MC2015 | `FixtureWritesDerivedMeasure` (Phase 3C)|
/// | MC2016 | `DuplicateFixtureName` (Phase 3C)       |
/// | MC2017 | `GoldenReferencesUnknownFixture` (Phase 3C) |
/// | MC2018 | `FixtureValueTypeMismatch` (Phase 3C)   |
/// | MC2019 | `FixtureMissingDimension` (Phase 3C)    |
/// | MC2020 | `FixtureWritesConsolidatedCell` (Phase 3C) |
/// | MC2021 | `FixtureValueIsNaN` (Phase 3C)          |
/// | MC2022 | `FixtureSourceUnreadable` (Phase 3C; includes path-escape variant) |
/// | MC2023 | `FixtureCsvRowColumnCountMismatch` (Phase 3C) |
/// | MC2024 | `FixtureCsvHeaderMismatch` (Phase 3C)   |
/// | MC2025 | `FixtureDuplicateCoordinate` (Phase 3C) |
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

    /// A measure declared `aggregation: WeightedAverage` did not declare a
    /// `weight_measure:`. Promoted from lint to validator in Phase 3B per
    /// ADR-0005 acceptance amendment #4 — code MC2011.
    #[error("measure {measure_name:?}: aggregation WeightedAverage requires weight_measure")]
    WeightedAverageMissingWeight { measure_name: String },

    /// Generic schema misshape: a required field was missing, a required
    /// type didn't match, a `model_format_version` other than 1, etc. The
    /// validator surfaces these alongside the targeted Decision 6 errors so
    /// authors don't bounce through a half-validated model.
    #[error("schema error: {message}")]
    Schema { message: String },

    // -----------------------------------------------------------------------
    // Phase 3C: fixture / input validators (MC2012–MC2025).
    //
    // These are emitted by the resolve-inputs stage rather than by
    // `validate()` itself, because they need filesystem access (CSV reads,
    // path canonicalization). They carry the `ValidationError` enum so the
    // diagnostic envelope shape is uniform — `mc model validate` reports
    // them via the same MC2xxx code namespace as the pure-validate errors.
    // -----------------------------------------------------------------------
    /// **MC2012** — A fixture / canonical_inputs column header (or coord
    /// key) names a dimension that isn't declared on the model. Catches
    /// typos like `Scenrio` instead of `Scenario`. Distinct from MC2013:
    /// MC2012 means the *column header* is wrong; MC2013 means the column
    /// is correct but the *row's value* names an unknown element.
    #[error("input set {input_set:?}: column {column:?} is not a declared dimension name")]
    FixtureUnknownDimensionKey { input_set: String, column: String },

    /// **MC2013** — A row in a fixture / canonical_inputs cites an element
    /// name (e.g., `Mar2026`) that isn't in the named dimension's element
    /// list. The column is correctly named; the value is wrong.
    #[error(
        "input set {input_set:?} row {row_index}: dimension {dim:?} has no element named {value:?}"
    )]
    FixtureUnknownElementValue {
        input_set: String,
        row_index: usize,
        dim: String,
        value: String,
    },

    /// **MC2014** — A row's `Measure` value (e.g., `Spnd`) doesn't match
    /// any measure declared in the model.
    #[error("input set {input_set:?} row {row_index}: unknown measure {measure:?}")]
    FixtureUnknownMeasure {
        input_set: String,
        row_index: usize,
        measure: String,
    },

    /// **MC2015** — A fixture / canonical_inputs row writes to a derived
    /// measure. Only `Input` measures are writable; derived cells are
    /// computed by rules (mirrors kernel `WritebackError::DerivedCellNotWritable`,
    /// caught here at load time for a friendlier file:row error message).
    #[error(
        "input set {input_set:?} row {row_index}: cannot write derived measure {measure:?} \
         (only Input measures are writable)"
    )]
    FixtureWritesDerivedMeasure {
        input_set: String,
        row_index: usize,
        measure: String,
    },

    /// **MC2016** — Two `test_fixtures` entries share the same `name:`.
    #[error("test_fixtures: duplicate fixture name {name:?}")]
    DuplicateFixtureName { name: String },

    /// **MC2017** — A `golden_test.fixture` field references a name that
    /// isn't in `test_fixtures`.
    #[error("golden_test {golden_name:?}: references unknown fixture {fixture_name:?}")]
    GoldenReferencesUnknownFixture {
        golden_name: String,
        fixture_name: String,
    },

    /// **MC2018** — A row's value column doesn't parse as the row's
    /// measure-declared type (e.g., `"abc"` for an `F64` measure).
    #[error(
        "input set {input_set:?} row {row_index}: value {value:?} is not a valid {data_type} \
         for measure {measure:?}"
    )]
    FixtureValueTypeMismatch {
        input_set: String,
        row_index: usize,
        measure: String,
        data_type: String,
        value: String,
    },

    /// **MC2019** — A fixture / canonical_inputs row is missing a column
    /// for one of the model's dimensions. Every leaf write must specify
    /// all dim coordinates; missing any is ambiguous.
    #[error(
        "input set {input_set:?}: columns {columns:?} are missing required dimension(s): {missing:?}"
    )]
    FixtureMissingDimension {
        input_set: String,
        columns: Vec<String>,
        missing: Vec<String>,
    },

    /// **MC2020** — A fixture / canonical_inputs row coordinates a
    /// consolidated element on at least one dimension. Only leaf
    /// coordinates are writable (mirrors kernel
    /// `WritebackError::ConsolidatedCellNotWritable`, caught here at
    /// load time for a friendlier file:row error message).
    #[error(
        "input set {input_set:?} row {row_index}: coordinate references consolidated element \
         {element:?} in dim {dim:?} (only leaves are writable)"
    )]
    FixtureWritesConsolidatedCell {
        input_set: String,
        row_index: usize,
        dim: String,
        element: String,
    },

    /// **MC2021** — A fixture / canonical_inputs row's value column is
    /// `NaN`. The kernel rejects NaN at write time anyway; catching it
    /// here yields a file:row error message instead of a kernel-side
    /// error after a partial write batch.
    #[error("input set {input_set:?} row {row_index}: value is NaN (rejected)")]
    FixtureValueIsNaN { input_set: String, row_index: usize },

    /// **MC2022** — The `source:` CSV file could not be read. The
    /// `reason` field disambiguates `not found` / `path escape` / `IO
    /// error`. ADR-0006 amendment #18 requires path-escape rejection
    /// (paths resolving outside the YAML model file's directory).
    #[error("input set {input_set:?}: source path {path:?} unreadable — {reason}")]
    FixtureSourceUnreadable {
        input_set: String,
        path: String,
        reason: String,
    },

    /// **MC2023** — A CSV data row's field count does not match the
    /// declared `columns:` length.
    #[error("input set {input_set:?} CSV line {line}: expected {expected} fields, got {actual}")]
    FixtureCsvRowColumnCountMismatch {
        input_set: String,
        line: usize,
        expected: usize,
        actual: usize,
    },

    /// **MC2024** — A CSV header row does not byte-exact match the
    /// declared `columns:` (per ADR-0006 amendment #19's columns
    /// contract).
    #[error("input set {input_set:?} CSV header mismatch: expected {expected:?}, got {actual:?}")]
    FixtureCsvHeaderMismatch {
        input_set: String,
        expected: Vec<String>,
        actual: Vec<String>,
    },

    /// **MC2025** — Two rows in the same `canonical_inputs` or single
    /// `test_fixtures` entry resolve to the exact same coordinate. The
    /// kernel would silently last-write-wins; catching it at load time
    /// surfaces an authoring mistake.
    #[error(
        "input set {input_set:?}: duplicate coordinate written by rows {first_row} and {second_row}"
    )]
    FixtureDuplicateCoordinate {
        input_set: String,
        first_row: usize,
        second_row: usize,
    },
}

impl ValidationError {
    /// Stable diagnostic code (`MC2xxx`). Phase 3B contract: this value is
    /// part of the public API; renaming or renumbering would silently
    /// break LLM/UI consumers pinned to the code-to-meaning map.
    pub fn code(&self) -> &'static str {
        match self {
            ValidationError::DuplicateName { .. } => "MC2001",
            ValidationError::MissingDimension { .. } => "MC2002",
            ValidationError::InvalidHierarchyEdge { .. } => "MC2003",
            ValidationError::HierarchyCycle { .. } => "MC2004",
            ValidationError::RuleReferencesUnknownMeasure { .. } => "MC2005",
            ValidationError::DerivedMeasureWithoutRule { .. } => "MC2006",
            ValidationError::InputMeasureHasRule { .. } => "MC2007",
            ValidationError::RuleCycle { .. } => "MC2008",
            ValidationError::UnsupportedAggregation { .. } => "MC2009",
            ValidationError::Schema { .. } => "MC2010",
            ValidationError::WeightedAverageMissingWeight { .. } => "MC2011",
            ValidationError::FixtureUnknownDimensionKey { .. } => "MC2012",
            ValidationError::FixtureUnknownElementValue { .. } => "MC2013",
            ValidationError::FixtureUnknownMeasure { .. } => "MC2014",
            ValidationError::FixtureWritesDerivedMeasure { .. } => "MC2015",
            ValidationError::DuplicateFixtureName { .. } => "MC2016",
            ValidationError::GoldenReferencesUnknownFixture { .. } => "MC2017",
            ValidationError::FixtureValueTypeMismatch { .. } => "MC2018",
            ValidationError::FixtureMissingDimension { .. } => "MC2019",
            ValidationError::FixtureWritesConsolidatedCell { .. } => "MC2020",
            ValidationError::FixtureValueIsNaN { .. } => "MC2021",
            ValidationError::FixtureSourceUnreadable { .. } => "MC2022",
            ValidationError::FixtureCsvRowColumnCountMismatch { .. } => "MC2023",
            ValidationError::FixtureCsvHeaderMismatch { .. } => "MC2024",
            ValidationError::FixtureDuplicateCoordinate { .. } => "MC2025",
        }
    }
}

/// Top-level error wrapper. `load`, `load_str`, and (Phase 3D onwards)
/// `validate` return `Vec<Error>` so the caller can still see all of
/// stage-2's accumulated errors when validation fails partway through
/// the pipeline.
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

impl Error {
    /// Stable diagnostic code (`MC1xxx` for parse, `MC2xxx` for
    /// validation, internal labels for IO / compile). Phase 3D
    /// convenience: callers iterating mixed `Vec<Error>` no longer have
    /// to match on each variant to recover the code.
    pub fn code(&self) -> &'static str {
        match self {
            Error::Io { .. } => "MC0001",
            Error::Parse(p) => p.code(),
            Error::Validation(v) => v.code(),
            Error::Compile(_) => "MC9001",
        }
    }

    /// If this error wraps a [`ValidationError`], return a reference to
    /// it. Phase 3D helper for tests that iterate `Vec<Error>` looking
    /// for specific MC2xxx variants.
    pub fn as_validation(&self) -> Option<&ValidationError> {
        match self {
            Error::Validation(v) => Some(v),
            _ => None,
        }
    }

    /// If this error wraps a [`ParseError`], return a reference to it.
    pub fn as_parse(&self) -> Option<&ParseError> {
        match self {
            Error::Parse(p) => Some(p),
            _ => None,
        }
    }
}
