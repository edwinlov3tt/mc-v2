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
    #[serde(default)]
    pub golden_tests: Vec<ParsedGoldenTest>,
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
    /// `"Standard"` | `"Measure"` | `"Scenario"` | `"Version"`.
    pub kind: String,
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

/// A deterministic rule declaration. The body is a structured expression
/// tree per ADR-0004 Decision 4 — friendly formula strings are Phase 3C.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedRule {
    pub name: String,
    pub target_measure: String,
    /// `"AllLeaves"` (Phase 3A's only supported scope).
    pub scope: String,
    pub body: ParsedRuleBody,
    pub declared_dependencies: Vec<String>,
}

/// Structured expression-tree node. Each variant carries a distinguishing
/// field name so the YAML stays JSON-shaped (per ADR-0004 Decision 1's
/// safe subset bans on YAML tags). Serde's `untagged` enum dispatch
/// picks the variant by which field name is present.
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

/// `Const` payload. `f64` and `i64` are the common shapes; `bool` is
/// included for forward-compat. We deliberately do NOT support `Null` as
/// a constant value — that would conflict with §7's null-poison policy.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ParsedScalar {
    Float(f64),
    Int(i64),
    Bool(bool),
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
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedEpsilonExpect {
    pub value: f64,
    pub epsilon: f64,
}

// ---------------------------------------------------------------------------
// ValidatedModel — every Decision 6 check passed, name resolution baked in.
// ---------------------------------------------------------------------------

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
///
/// The compile stage walks this in dim order and allocates `mc_core` IDs.
#[derive(Clone, Debug)]
pub struct ValidatedModel {
    pub parsed: ParsedModel,
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
