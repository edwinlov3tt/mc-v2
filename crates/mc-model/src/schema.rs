//! `ParsedModel` and `ValidatedModel` — the two intermediate types
//! mandated by ADR-0004 Decision 9.
//!
//! `ParsedModel` mirrors the YAML 1:1 (owned strings + numbers + Vecs;
//! Option for optional fields). It is the surface Phase 4 (LLM authoring)
//! emits against — the LLM-emitted YAML is parsed into `ParsedModel`
//! exactly the same way a hand-authored YAML is.
//!
//! `ValidatedModel` is a `ParsedModel` that has passed every Decision 6
//! validator. Names are not yet resolved to `mc_core` IDs (the compile
//! stage does that); but element-name lookups within a dimension are
//! pre-built so the compile walk is O(N) rather than O(N²).

use std::collections::BTreeMap;

use serde::Deserialize;

// ---------------------------------------------------------------------------
// ParsedModel — YAML mirror, no semantic checking.
// ---------------------------------------------------------------------------

/// Top-level parsed YAML model. Mirrors the on-disk YAML 1:1.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedModel {
    pub model_format_version: i64,
    pub metadata: ParsedMetadata,
    pub dimensions: Vec<ParsedDimension>,
    #[serde(default)]
    pub hierarchies: Vec<ParsedHierarchy>,
    pub measures: Vec<ParsedMeasure>,
    pub rules: Vec<ParsedRule>,
    // -- Phase 3G: Reference-data blocks (optional) --
    /// Industry benchmarks with source attribution.
    #[serde(default)]
    pub benchmarks: Vec<ParsedBenchmark>,
    /// Lookup tables keyed by dimension element.
    #[serde(default)]
    pub lookup_tables: Vec<ParsedLookupTable>,
    /// Status threshold bands for bucket() evaluation.
    #[serde(default)]
    pub status_thresholds: Vec<ParsedStatusThreshold>,
    #[serde(default)]
    pub golden_tests: Vec<ParsedGoldenTest>,
    // -- Phase 3H: Fitted-model blocks (optional) --
    /// Pre-fitted model coefficients for `predict()` evaluation.
    #[serde(default)]
    pub fitted_models: Vec<ParsedFittedModel>,
    /// Calibration maps for `calibrate()` evaluation.
    #[serde(default)]
    pub calibration_maps: Vec<ParsedCalibrationMap>,
    /// Phase 3C: optional always-load input set (sibling CSV or inline
    /// rows). Replaces the `mc-cli/main.rs` Acme-name special case.
    /// Models without this block load identically to Phase 3B.
    #[serde(default)]
    pub canonical_inputs: Option<ParsedInputSet>,
    /// Phase 3C: optional named per-test input fixtures. Each fixture is
    /// referenced by `golden_tests[i].fixture` for override semantics.
    #[serde(default)]
    pub test_fixtures: Vec<ParsedFixture>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedMetadata {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub created: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedDimension {
    pub name: String,
    /// `"Standard"` | `"Measure"` | `"Scenario"` | `"Version"` | `"Time"`.
    pub kind: String,
    /// Optional human-readable description. Phase 3B's MC3001 lint fires
    /// when this is missing on a Standard / Measure / Scenario / Version
    /// dim. The field is additive over the ADR-0004 schema; Phase 3A
    /// models with no descriptions still parse cleanly.
    #[serde(default)]
    pub description: Option<String>,
    /// Phase 3E: for `kind: "Scenario"` dims, declares which element
    /// `actual_ref()` reads from. E.g., `actuals_element: "Actual"`.
    #[serde(default)]
    pub actuals_element: Option<String>,
    /// Phase 3F.1: declared granularity for Time dims.
    /// Legal values: `"day"` | `"week"` | `"month"` | `"quarter"` | `"year"`.
    #[serde(default)]
    pub granularity: Option<String>,
    /// Phase 3F.1: runtime "now" anchor element name. Anchor functions
    /// (`anchor_index`, `is_past`, etc.) reference this element.
    #[serde(default)]
    pub time_anchor: Option<String>,
    pub elements: Vec<ParsedElement>,
}

/// One element within a dimension. The optional fields are populated only
/// when the parent dim's kind matches:
///
/// - `version_state` → Version dim (`"Draft" | "Submitted" | "Approved" | "Archived"`)
/// - `scenario_meta` → Scenario dim (`"Default" | "NonDefault"`)
///
/// (Measure-dim elements are NOT modeled here — measures live under the
/// top-level `measures:` block per the schema; a Measure dim with no
/// elements declared inline is filled in from `measures:` by the validator.)
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedElement {
    pub name: String,
    #[serde(default)]
    pub version_state: Option<String>,
    #[serde(default)]
    pub scenario_meta: Option<String>,
    /// Phase 3F: optional ISO date metadata for Time-kind dimensions.
    /// Used by lint MC3010 (non-chronological order warning).
    #[serde(default)]
    pub date: Option<String>,
    /// Phase 3F.1: ISO 8601 period start date (YYYY-MM-DD).
    #[serde(default)]
    pub period_start: Option<String>,
    /// Phase 3F.1: ISO 8601 period end date, exclusive (YYYY-MM-DD).
    #[serde(default)]
    pub period_end_exclusive: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedHierarchy {
    /// Dimension name (must reference a declared dimension).
    pub dimension: String,
    pub name: String,
    pub edges: Vec<ParsedHierarchyEdge>,
    /// Optional flag: this hierarchy is the dimension's default. If no
    /// hierarchy under a given dimension is marked default, the first
    /// declared hierarchy becomes default (mirrors `DimensionBuilder`).
    #[serde(default)]
    pub default: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedHierarchyEdge {
    pub parent: String,
    pub child: String,
    pub weight: f64,
}

/// A measure declaration. The measure dimension's element list is
/// **derived from this section** during validation — measures appear
/// once, not duplicated in `dimensions[Measure].elements`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedMeasure {
    pub name: String,
    /// `"Input" | "Derived"`.
    pub role: String,
    /// `"F64" | "I64" | "Bool" | "Category"`. Phase 3A's Acme uses F64
    /// only; the others are forward surface.
    pub data_type: String,
    /// Optional human-readable description. Phase 3B's MC3002 lint fires
    /// when this is missing. Additive over ADR-0004's schema.
    #[serde(default)]
    pub description: Option<String>,
    /// Required when `data_type: "Category"`. Ignored otherwise.
    #[serde(default)]
    pub category_domain: Option<Vec<String>>,
    /// `"Sum" | "WeightedAverage" | "Min" | "Max"`.
    pub aggregation: String,
    /// Required when `aggregation: "WeightedAverage"`. References another
    /// measure name from this same `measures:` block.
    #[serde(default)]
    pub weight_measure: Option<String>,
}

/// A deterministic rule declaration. Phase 3D introduced the
/// [`ParsedRuleBodyForm`] wrapper so `body:` may be authored either as a
/// structured expression tree (`{ mul: [...] }`) or as a friendly formula
/// string (`"Customers * AOV"`). Both forms produce identical
/// [`ValidatedModel`]s — the validate stage parses the formula form into
/// the structured tree before any downstream processing sees it.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedRule {
    pub name: String,
    pub target_measure: String,
    /// `"AllLeaves"` (Phase 3A's only supported scope).
    pub scope: String,
    /// Optional human-readable description. Phase 3B's MC3003 lint fires
    /// when this is missing. Additive over ADR-0004's schema.
    #[serde(default)]
    pub description: Option<String>,
    pub body: ParsedRuleBodyForm,
    pub declared_dependencies: Vec<String>,
}

/// Phase 3D: the YAML-faithful authoring form for a rule body. `Formula`
/// holds the source text (`"Customers * AOV"`); `Structured` holds the
/// existing s-expression-shaped tree.
///
/// `serde(untagged)` dispatches by YAML node kind: a scalar string maps to
/// `Formula(_)`; a mapping maps to `Structured(_)`. Order matters — string
/// must come first so a YAML scalar never accidentally tries the
/// `Structured` mapping branch first. (Acceptance amendment per the
/// Phase 3D handoff §"Phase 3D scope" item 2.)
///
/// The wrapper lives in [`ParsedModel`] only; [`ValidatedModel`] flattens
/// to bare [`ParsedRuleBody`] so downstream stages (`compile`, `lint`,
/// `inspect`) have no awareness of formula authoring form.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum ParsedRuleBodyForm {
    /// `body: "Customers * AOV"`. Parsed by the formula parser at
    /// validate time; emits MC1003–MC1006 on syntax failure.
    Formula(String),
    /// `body: { mul: [{ ref: "Customers" }, { ref: "AOV" }] }`. The
    /// structured form predates Phase 3D and continues to load unchanged.
    Structured(ParsedRuleBody),
}

/// Structured expression-tree node. Each variant carries a distinguishing
/// field name so the YAML stays JSON-shaped (per ADR-0004 Decision 1's
/// safe subset bans on YAML tags). Serde's `untagged` enum dispatch
/// picks the variant by which field name is present.
///
/// Phase 3E adds comparison, logical, and function variants (17 new).
/// Phase 3F adds time-series variants (5 new).
/// Phase 3G adds reference-data variants (4 new).
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum ParsedRuleBody {
    /// `{ const: 1.0 }`
    Const(ParsedConstBody),
    /// `{ ref: "Spend" }`
    Ref(ParsedRefBody),
    /// `{ add: [lhs, rhs] }`
    Add(ParsedAddBody),
    /// `{ sub: [lhs, rhs] }`
    Sub(ParsedSubBody),
    /// `{ mul: [lhs, rhs] }`
    Mul(ParsedMulBody),
    /// `{ div: [lhs, rhs] }`
    Div(ParsedDivBody),
    /// `{ if_null: [primary, fallback] }`
    IfNull(ParsedIfNullBody),

    // -- Phase 3E: Comparisons (6) --
    /// `a > b`
    Gt(ParsedBinopBody),
    /// `a < b`
    Lt(ParsedBinopBody),
    /// `a >= b`
    Gte(ParsedBinopBody),
    /// `a <= b`
    Lte(ParsedBinopBody),
    /// `a == b`
    Eq(ParsedBinopBody),
    /// `a != b`
    Neq(ParsedBinopBody),

    // -- Phase 3E: Logical operators (3) --
    /// `a and b`
    And(ParsedBinopBody),
    /// `a or b`
    Or(ParsedBinopBody),
    /// `not x`
    Not(ParsedUnaryBody),

    // -- Phase 3E: Functions (8) --
    /// `if(condition, then_branch, else_branch)`
    If(ParsedIfBody),
    /// `min(a, b, ...)` — variadic, 2+ args
    Min(ParsedVarargBody),
    /// `max(a, b, ...)` — variadic, 2+ args
    Max(ParsedVarargBody),
    /// `abs(x)`
    Abs(ParsedUnaryBody),
    /// `safe_div(numerator, denominator, default)`
    SafeDiv(ParsedSafeDivBody),
    /// `clamp(value, lo, hi)`
    Clamp(ParsedClampBody),
    /// `coalesce(a, b, ...)` — variadic, 1+ args
    Coalesce(ParsedVarargBody),
    /// `actual_ref(Measure_Name)` — cross-coordinate read (Scenario shift)
    ActualRef(ParsedActualRefBody),

    // -- Phase 3F: Time-series (5) --
    /// `prev(measure)` — previous time-period value
    Prev(ParsedMeasureRefBody),
    /// `lag(measure, periods)` — n periods ago
    Lag(ParsedLagBody),
    /// `cumulative(measure)` — running sum
    Cumulative(ParsedMeasureRefBody),
    /// `rolling_avg(measure, window)` — moving average
    RollingAvg(ParsedRollingAvgBody),
    /// `period_index()` — 0-based position in Time dim.
    /// Wrapped in a struct to prevent serde's untagged dispatch from
    /// matching YAML null as this unit variant.
    PeriodIndex(ParsedPeriodIndexBody),
    /// `anchor_index()` — period_index of the time_anchor element.
    AnchorIndex(ParsedPeriodIndexBody),
    /// `is_past()` — 1.0 if period_index < anchor_index, else 0.0.
    IsPast(ParsedPeriodIndexBody),
    /// `is_current()` — 1.0 if period_index == anchor_index, else 0.0.
    IsCurrent(ParsedPeriodIndexBody),
    /// `is_future()` — 1.0 if period_index > anchor_index, else 0.0.
    IsFuture(ParsedPeriodIndexBody),
    /// `periods_since_anchor()` — period_index - anchor_index.
    PeriodsSinceAnchor(ParsedPeriodIndexBody),
    /// `periods_to_end()` — max_period_index - period_index.
    PeriodsToEnd(ParsedPeriodIndexBody),

    // -- Phase 3G: Reference-data (4) --
    /// `benchmark("name", key_expr)`
    Benchmark(ParsedBenchmarkRefBody),
    /// `lookup("table", key_expr)`
    Lookup(ParsedLookupRefBody),
    /// `bucket(value, "threshold_name")`
    Bucket(ParsedBucketBody),
    /// `sum_over(dimension, measure)`
    SumOver(ParsedSumOverBody),

    // -- Phase 3H: Fitted-model evaluation (4) --
    /// `predict("model_name", feature1, feature2, ...)`
    Predict(ParsedPredictBody),
    /// `calibrate(value, "map_name")`
    Calibrate(ParsedCalibrateBody),
    /// `exp(x)` — Euler's number raised to the power of x
    Exp(ParsedUnaryBody),
    /// `norm_cdf(x, mu, sigma)` — normal distribution CDF
    NormCdf(ParsedNormCdfBody),

    // -- Phase 3I: Math primitives (9) --
    /// `pow(base, exp)`
    Pow(ParsedPowBody),
    /// `sqrt(x)`
    Sqrt(ParsedUnaryBody),
    /// `ln(x)`
    Ln(ParsedUnaryBody),
    /// `log10(x)`
    Log10(ParsedUnaryBody),
    /// `round(x)`
    Round(ParsedUnaryBody),
    /// `floor(x)`
    Floor(ParsedUnaryBody),
    /// `ceil(x)`
    Ceil(ParsedUnaryBody),
    /// `mod(a, b)`
    Mod(ParsedModBody),
    /// `norm_inv(p, mu, sigma)` — inverse of standard normal CDF.
    NormInv(ParsedNormInvBody),

    // -- Phase 3I: is_element narrow numeric form --
    /// `is_element(Dim, "Element")` — returns 1.0 if current coord's element
    /// in `Dim` is `"Element"`, 0.0 otherwise.
    IsElement(ParsedIsElementBody),

    // -- Phase 3J item 1: string literal in eval --
    /// `{ str_literal: "Houston" }` (structured form) or `"Houston"`
    /// (formula form). Per ADR-0016 Decision 2, `Str` values exist only
    /// in expression evaluation; the validator rejects Str-typed bodies
    /// (MC2058), Str-in-arithmetic (MC1026), Str-in-numeric-comparison
    /// (MC1028), and Str-in-truthy-context (MC1027).
    StrLiteral(ParsedStrLiteralBody),

    // -- Phase 3J item 2: current_element(Dim) --
    /// `{ current_element: "Channel" }` (structured form) or
    /// `current_element(Channel)` (formula form). Returns the current
    /// coordinate's element name in `dim` as `ScalarValue::Str`. At
    /// consolidated coords, returns Null.
    CurrentElement(ParsedCurrentElementBody),

    // -- Phase 3I: cross-coord scans --
    /// `avg_over(measure, dim)` — mean across leaf elements of `dim`.
    AvgOver(ParsedSumOverBody),
    /// `min_over(measure, dim)` — minimum across leaf elements of `dim`.
    MinOver(ParsedSumOverBody),
    /// `max_over(measure, dim)` — maximum across leaf elements of `dim`.
    MaxOver(ParsedSumOverBody),
    /// `wavg_over(measure, dim, weight_measure)` — weighted average.
    WAvgOver(ParsedWAvgOverBody),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedConstBody {
    #[serde(rename = "const")]
    pub value: ParsedScalar,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedRefBody {
    #[serde(rename = "ref")]
    pub measure: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedAddBody {
    pub add: Vec<ParsedRuleBody>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedSubBody {
    pub sub: Vec<ParsedRuleBody>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedMulBody {
    pub mul: Vec<ParsedRuleBody>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedDivBody {
    pub div: Vec<ParsedRuleBody>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedIfNullBody {
    pub if_null: Vec<ParsedRuleBody>,
}

// ---------------------------------------------------------------------------
// Phase 3E body structs
// ---------------------------------------------------------------------------

/// Shared body for binary operators (comparisons + logical and/or).
/// The two children are boxed to allow recursive nesting.
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedBinopBody {
    pub left: Box<ParsedRuleBody>,
    pub right: Box<ParsedRuleBody>,
}

/// Shared body for unary operators (not, abs).
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedUnaryBody {
    pub operand: Box<ParsedRuleBody>,
}

/// `if(condition, then_branch, else_branch)`
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedIfBody {
    pub condition: Box<ParsedRuleBody>,
    pub then_branch: Box<ParsedRuleBody>,
    pub else_branch: Box<ParsedRuleBody>,
}

/// Variadic function body (min, max, coalesce).
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedVarargBody {
    pub args: Vec<ParsedRuleBody>,
}

/// `safe_div(numerator, denominator, default)`
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedSafeDivBody {
    pub numerator: Box<ParsedRuleBody>,
    pub denominator: Box<ParsedRuleBody>,
    pub default: Box<ParsedRuleBody>,
}

/// `clamp(value, lo, hi)`
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedClampBody {
    pub value: Box<ParsedRuleBody>,
    pub lo: Box<ParsedRuleBody>,
    pub hi: Box<ParsedRuleBody>,
}

/// `actual_ref(Measure_Name)` — cross-coordinate read (Scenario shift).
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedActualRefBody {
    pub measure: String,
}

// ---------------------------------------------------------------------------
// Phase 3F body structs
// ---------------------------------------------------------------------------

/// Shared body for time-series functions that take a bare measure name.
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedMeasureRefBody {
    pub measure: String,
}

/// `lag(measure, periods)` — the periods argument is an expression.
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedLagBody {
    pub measure: String,
    pub periods: Box<ParsedRuleBody>,
}

/// `rolling_avg(measure, window)` — the window argument is an expression.
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedRollingAvgBody {
    pub measure: String,
    pub window: Box<ParsedRuleBody>,
}

// ---------------------------------------------------------------------------
// Phase 3G body structs
// ---------------------------------------------------------------------------

/// Marker body for `period_index()` — a zero-field struct so the serde
/// untagged dispatch doesn't match YAML null as a unit variant.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ParsedPeriodIndexBody {
    // Intentionally empty — exists to prevent serde null matching.
    #[serde(skip)]
    _marker: (),
}

impl ParsedPeriodIndexBody {
    /// Construct the marker body (used by the formula parser).
    pub fn new() -> Self {
        Self::default()
    }
}

/// `benchmark("name", key_expr)`
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedBenchmarkRefBody {
    pub name: String,
    pub key_expr: Box<ParsedRuleBody>,
}

/// `lookup("table", key1, key2, ...)` — Phase 3I item 3 made the key
/// expression list variadic. A 1-element vec corresponds to the
/// original Phase 3G single-key shape.
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedLookupRefBody {
    pub table: String,
    pub key_exprs: Vec<Box<ParsedRuleBody>>,
}

/// `bucket(value, "threshold_name")`
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedBucketBody {
    pub value: Box<ParsedRuleBody>,
    pub threshold_name: String,
}

/// `sum_over(dimension, measure)`
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedSumOverBody {
    pub dimension: String,
    pub measure: String,
}

// ---------------------------------------------------------------------------
// Phase 3H body structs
// ---------------------------------------------------------------------------

/// `predict("model_name", feature1, feature2, ...)`
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedPredictBody {
    pub model_id: String,
    pub features: Vec<Box<ParsedRuleBody>>,
}

/// `calibrate(value, "map_name")`
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedCalibrateBody {
    pub value: Box<ParsedRuleBody>,
    pub map_id: String,
}

/// `norm_cdf(x, mu, sigma)` — normal distribution CDF
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedNormCdfBody {
    pub x: Box<ParsedRuleBody>,
    pub mu: Box<ParsedRuleBody>,
    pub sigma: Box<ParsedRuleBody>,
}

// ---------------------------------------------------------------------------
// Phase 3I body structs
// ---------------------------------------------------------------------------

/// `pow(base, exponent)`
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedPowBody {
    pub base: Box<ParsedRuleBody>,
    pub exponent: Box<ParsedRuleBody>,
}

/// `mod(dividend, divisor)`
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedModBody {
    pub dividend: Box<ParsedRuleBody>,
    pub divisor: Box<ParsedRuleBody>,
}

/// `norm_inv(p, mu, sigma)` — inverse of normal-distribution CDF
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedNormInvBody {
    pub p: Box<ParsedRuleBody>,
    pub mu: Box<ParsedRuleBody>,
    pub sigma: Box<ParsedRuleBody>,
}

/// `is_element(Dim, "Element")` — narrow numeric indicator.
///
/// `dimension` is a bare identifier (the dimension name); `element` is a
/// quoted string literal. Per Phase 3I item 1 W4, string literals are
/// allowed only as this second arg — a parse-time guarantee enforced by
/// the formula parser.
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedIsElementBody {
    pub dimension: String,
    pub element: String,
}

/// `wavg_over(value_measure, dim, weight_measure)` — weighted average
/// across leaf elements of `dim`. Phase 3I item 5.
#[derive(Clone, Debug, Deserialize)]
pub struct ParsedWAvgOverBody {
    pub dimension: String,
    pub value_measure: String,
    pub weight_measure: String,
}

/// Phase 3J item 1: string literal in expression evaluation. Authored as
/// `{ str_literal: "Houston" }` in the structured YAML form, or as
/// `"Houston"` in the friendly-formula form (the formula parser produces
/// a `StrLiteral` node when a quoted string appears in primary position).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedStrLiteralBody {
    pub str_literal: String,
}

/// Phase 3J item 2: `current_element(Dim)` — returns the current coord's
/// element name in `Dim` as `Str`. Structured form:
/// `{ current_element: "Channel" }`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedCurrentElementBody {
    pub current_element: String,
}

/// `Const` payload. `f64` and `i64` are the common shapes; `bool` is
/// included for forward-compat. Phase 3E adds `Null` so formulas can
/// reference it explicitly (e.g., `if(cond, value, Null)`).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ParsedScalar {
    Float(f64),
    Int(i64),
    Bool(bool),
    /// Explicit Null literal, parsed from formula text `Null`.
    /// Not reachable via serde deserialization (YAML null is not a scalar).
    #[serde(skip)]
    Null,
}

/// Inline golden test entry. `coord` is a flat map of dim-name → element-name.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedGoldenTest {
    pub name: String,
    pub coord: BTreeMap<String, String>,
    /// Either `expect: 11500.0` (exact) or
    /// `expect_within_epsilon: { value: ..., epsilon: ... }` (tolerant).
    /// Exactly one of `expect` / `expect_within_epsilon` must be set;
    /// validator enforces.
    #[serde(default)]
    pub expect: Option<f64>,
    #[serde(default)]
    pub expect_within_epsilon: Option<ParsedEpsilonExpect>,
    /// Phase 3C: optional reference to a `test_fixtures` entry by name.
    /// When set, the named fixture's rows are applied (override semantic)
    /// on top of `canonical_inputs` before this golden runs. When unset,
    /// only `canonical_inputs` apply.
    #[serde(default)]
    pub fixture: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedEpsilonExpect {
    pub value: f64,
    pub epsilon: f64,
}

// ---------------------------------------------------------------------------
// Phase 3G: reference-data block schema types
// ---------------------------------------------------------------------------

/// Industry benchmark with source attribution (Phase 3G).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedBenchmark {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub source: String,
    pub last_updated: String,
    pub key_dimension: String,
    pub values: BTreeMap<String, f64>,
}

/// Lookup table keyed by dimension element (Phase 3G).
///
/// Phase 3I item 3 made `key_dimension` optional and added the multi-key
/// `key_dimensions: Vec<String>` field. Exactly one of the two must be
/// set:
///   - `key_dimension: Market` — single-key (Phase 3G shape, unchanged).
///   - `key_dimensions: ["Market", "Time"]` — multi-key (Phase 3I).
///
/// MC2050 fires if both are set; MC2051 fires if any element name
/// contains the pipe separator; MC2052 fires if a key has the wrong
/// arity for the declared key_dimensions.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedLookupTable {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Single-key form. Backward compat with Phase 3G.
    #[serde(default)]
    pub key_dimension: Option<String>,
    /// Multi-key form. Phase 3I item 3.
    #[serde(default)]
    pub key_dimensions: Option<Vec<String>>,
    pub values: BTreeMap<String, f64>,
}

impl ParsedLookupTable {
    /// Phase 3I item 3: return the key dimensions as a slice regardless
    /// of which form (`key_dimension` vs `key_dimensions`) was used.
    /// Returns an empty slice if neither is set (validator catches that).
    pub fn key_dims(&self) -> Vec<&str> {
        match (&self.key_dimension, &self.key_dimensions) {
            (Some(d), None) => vec![d.as_str()],
            (None, Some(ds)) => ds.iter().map(String::as_str).collect(),
            (Some(_), Some(_)) | (None, None) => Vec::new(),
        }
    }
}

/// Status threshold configuration with ordered bands (Phase 3G).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedStatusThreshold {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub bands: Vec<ParsedThresholdBand>,
}

/// One band within a status threshold. The last band must have `max: None`
/// (unbounded above).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedThresholdBand {
    pub label: String,
    #[serde(default)]
    pub max: Option<f64>,
}

// ---------------------------------------------------------------------------
// Phase 3H: fitted-model + calibration-map schema types
// ---------------------------------------------------------------------------

/// Pre-fitted model coefficients for `predict()` evaluation (Phase 3H).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedFittedModel {
    pub name: String,
    /// `"linear"` | `"logistic"`
    pub method: String,
    pub intercept: f64,
    pub coefficients: Vec<ParsedFittedCoefficient>,
    #[serde(default)]
    pub standardization: Option<ParsedStandardizationConfig>,
    #[serde(default)]
    pub residual_std: Option<f64>,
    #[serde(default)]
    pub metadata: Option<ParsedFittedModelMetadata>,
}

/// One coefficient in a fitted model.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedFittedCoefficient {
    pub feature: String,
    pub weight: f64,
}

/// Standardization configuration (z-score normalization).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedStandardizationConfig {
    /// `"zscore"` (only supported method for now).
    pub method: String,
    pub params: Vec<ParsedStandardizationParam>,
}

/// Per-feature standardization parameters.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedStandardizationParam {
    pub feature: String,
    pub mean: f64,
    pub std: f64,
}

/// Optional metadata for fitted models.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedFittedModelMetadata {
    #[serde(default)]
    pub fitted_at: Option<String>,
    #[serde(default)]
    pub algorithm: Option<String>,
    #[serde(default)]
    pub alpha: Option<f64>,
    #[serde(default)]
    pub n_train: Option<usize>,
    #[serde(default)]
    pub holdout_mae: Option<f64>,
}

/// Calibration map for `calibrate()` evaluation (Phase 3H).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedCalibrationMap {
    pub name: String,
    /// `"pava"` | `"platt"`
    pub method: String,
    #[serde(default)]
    pub points: Option<Vec<ParsedCalibrationPoint>>,
    #[serde(default)]
    pub platt_params: Option<ParsedPlattParams>,
    #[serde(default)]
    pub metadata: Option<ParsedCalibrationMetadata>,
}

/// One point in a PAVA calibration map.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedCalibrationPoint {
    pub raw: f64,
    pub calibrated: f64,
}

/// Platt sigmoid parameters.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedPlattParams {
    pub a: f64,
    pub b: f64,
}

/// Optional metadata for calibration maps.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedCalibrationMetadata {
    #[serde(default)]
    pub fitted_at: Option<String>,
    #[serde(default)]
    pub sample_size: Option<usize>,
    #[serde(default)]
    pub raw_brier: Option<f64>,
    #[serde(default)]
    pub calibrated_brier: Option<f64>,
}

// ---------------------------------------------------------------------------
// Phase 3C: canonical_inputs + test_fixtures schema
// ---------------------------------------------------------------------------

/// One declared input set (`canonical_inputs:` or one entry of
/// `test_fixtures:`). The block declares the column layout once and then
/// either points at a sibling CSV file (`source:`) OR carries the rows
/// inline (`inline:`). Exactly one of `source` / `inline` must be set —
/// the resolve-inputs stage enforces this and emits a structural error
/// if both / neither are set.
///
/// `columns:` is required for both forms. The last column name is reserved
/// as the cell value (literal `"value"` per ADR-0006 amendment #19's
/// alternate-route flag); every other column must match a dimension
/// name declared in the model.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedInputSet {
    pub columns: Vec<String>,
    /// Sibling CSV file path, resolved relative to the YAML model file's
    /// directory. Path-escapes (`../`) are rejected per ADR-0006
    /// amendment #18 (MC2022 with a path-escape variant).
    #[serde(default)]
    pub source: Option<String>,
    /// Inline rows. Each inner Vec must have `columns.len()` entries.
    /// Per ADR-0006 Decision 1: tabular inline form only (per-row inline
    /// dropped pre-acceptance).
    #[serde(default)]
    pub inline: Option<ParsedInlineRows>,
}

/// Inline `rows:` payload for a `ParsedInputSet`. Each inner cell is a
/// [`ParsedRowCell`] (string OR number OR bool) so dim columns (string)
/// and the value column (numeric / bool) coexist on the same row.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedInlineRows {
    pub rows: Vec<Vec<ParsedRowCell>>,
}

/// One cell in an inline `canonical_inputs.rows[i]` / `test_fixtures[i].inline.rows[j]`.
/// Wider than `ParsedScalar` (which is for rule constants and excludes
/// strings on purpose) — inline rows mix string dim values with numeric
/// cell values.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ParsedRowCell {
    Float(f64),
    Int(i64),
    Bool(bool),
    Str(String),
}

/// One named per-test fixture under `test_fixtures:`. Fixtures inherit the
/// same source-XOR-inline shape as `canonical_inputs:`, but carry a
/// `name:` so golden tests can reference them.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedFixture {
    pub name: String,
    pub columns: Vec<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub inline: Option<ParsedInlineRows>,
}

// ---------------------------------------------------------------------------
// ValidatedModel — every Decision 6 check passed, name resolution baked in.
// ---------------------------------------------------------------------------

/// One rule, post-validate. Mirrors [`ParsedRule`] field-for-field with
/// one normalization: `body` is the flat [`ParsedRuleBody`] expression
/// tree, regardless of whether the rule was authored as a formula string
/// or a structured tree in YAML.
///
/// Per Phase 3D acceptance amendment #23: downstream stages
/// (`resolve_inputs`, `compile`, `inspect`, `lint`) consume
/// `ValidatedModel.rules[i].body` and have ZERO awareness of the
/// `ParsedRuleBodyForm` wrapper that lives upstream in `ParsedModel`.
#[derive(Clone, Debug)]
pub struct ValidatedRule {
    pub name: String,
    pub target_measure: String,
    pub scope: String,
    pub description: Option<String>,
    pub body: ParsedRuleBody,
    pub declared_dependencies: Vec<String>,
}

/// `ValidatedModel` is the contract Phase 4 (LLM authoring) and Phase 6
/// (UI editor) compile against. By construction:
///
/// - Every dimension referenced by a hierarchy / measure-weight / rule
///   is declared.
/// - Every element referenced by a hierarchy edge / rule body is declared.
/// - No duplicate names within a category.
/// - Hierarchies are acyclic.
/// - Rule dependency graph is acyclic.
/// - Every derived measure has exactly one rule; no input measure has a rule.
/// - Every rule body is a flat [`ParsedRuleBody`] (formula bodies have
///   been parsed; the [`ParsedRuleBodyForm`] wrapper is gone).
///
/// The compile stage walks this in dim order and allocates `mc_core` IDs.
#[derive(Clone, Debug)]
pub struct ValidatedModel {
    pub parsed: ParsedModel,
    /// Phase 3D: the rules with bodies normalized to flat
    /// [`ParsedRuleBody`]. Length and order match `parsed.rules`.
    /// Downstream stages read from here, not `parsed.rules`.
    pub rules: Vec<ValidatedRule>,
    /// Indices into `parsed.dimensions`, in the canonical dim order. For
    /// the Acme schema this is `[Scenario, Version, Time, Channel, Market,
    /// Measure]`; the validator enforces that order matches `mc_core`'s
    /// expectation when `kind: "Measure"` exists.
    pub dimension_order: Vec<usize>,
    /// Index of the Measure dimension in `parsed.dimensions`. Required.
    pub measure_dim_index: usize,
    /// Map dim-name → index into `parsed.dimensions`.
    pub dim_index_by_name: BTreeMap<String, usize>,
    /// For each dimension (by `parsed.dimensions[i]` index): map element
    /// name → element index within that dim's `elements` vec.
    pub element_index_by_name: Vec<BTreeMap<String, usize>>,
    /// Map measure-name → index into `parsed.measures`.
    pub measure_index_by_name: BTreeMap<String, usize>,
}
