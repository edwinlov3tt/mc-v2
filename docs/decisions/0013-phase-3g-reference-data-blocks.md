# ADR-0013: Phase 3G — Reference-Data Blocks

**Status:** Accepted
**Date:** 2026-05-04
**Deciders:** project owner
**Phase:** 3G (per [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md))

> Phase 3E adds conditionals; Phase 3F adds time-series. Phase 3G introduces **structured reference data** — industry benchmarks, lookup tables, and status thresholds — as first-class YAML blocks with formula functions to read them. This is the "domain-knowledge unlock" that moves external constants into the model with source attribution and validation.

---

## Context

Real-world models are full of reference data that doesn't belong in the cell grid:
- Industry benchmarks ("average B2B SaaS CPC is $5.50 for Paid Search" — source: WordStream 2025)
- Lookup tables (tax rates by state, seasonal factors by month, territory-to-region mappings)
- Threshold bands (CPC < $3 = "Good", $3-$7 = "Warning", > $7 = "Critical")

Currently, users encode these as input measures with hardcoded values. This works but loses:
- **Provenance:** where did that $5.50 come from? When was it last updated?
- **Separation of concerns:** reference data mixed into the cell grid is indistinguishable from operational data
- **Updateability:** changing a benchmark requires finding the right cells across all coordinates, not editing one YAML block
- **Validation:** stale benchmarks (12+ months old) go undetected

Phase 3G introduces three new top-level YAML blocks that declare reference data with metadata, plus formula functions that read from them at eval time. This is the first model-format extension that adds new YAML blocks since Phase 3C's `canonical_inputs:` and `test_fixtures:`.

**Architectural importance:** Phase 3G establishes the **"reference-data-in-YAML" pattern** that Phase 3H reuses for `fitted_models:` and `calibration_maps:`. The schema types, validator, and eval-time lookup machinery designed here serve both phases.

---

## Decisions

### Decision 1: Schema shape for new YAML blocks

**`benchmarks:` block:**

```yaml
benchmarks:
  - name: "industry_cpc"
    description: "Average B2B SaaS cost-per-click by channel"
    source: "WordStream 2025 Industry Benchmark Report"
    last_updated: "2025-03-15"
    key_dimension: "Channel"
    values:
      Paid_Search: 5.50
      Paid_Social: 3.20
      Display: 1.80
      Email: 0.0
```

Schema type:
```rust
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedBenchmark {
    pub name: String,
    pub description: Option<String>,
    pub source: String,                    // attribution (required)
    pub last_updated: String,              // ISO date (required)
    pub key_dimension: String,             // which dim the keys reference
    pub values: BTreeMap<String, f64>,     // element_name -> value
}
```

**`lookup_tables:` block:**

```yaml
lookup_tables:
  - name: "tax_rate"
    description: "Corporate tax rate by market"
    key_dimension: "Market"
    values:
      Florida: 0.055
      Georgia: 0.0575
      North_Carolina: 0.025
      New_York: 0.085

  - name: "seasonal_factor"
    description: "Monthly seasonality index (1.0 = average month)"
    key_dimension: "Time"
    values:
      Jan_2026: 0.75
      Feb_2026: 0.80
      Mar_2026: 0.90
      # ... etc.
```

Schema type:
```rust
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedLookupTable {
    pub name: String,
    pub description: Option<String>,
    pub key_dimension: String,             // which dim the keys reference
    pub values: BTreeMap<String, f64>,     // element_name -> value
}
```

**`status_thresholds:` block:**

```yaml
status_thresholds:
  - name: "cpc_health"
    description: "CPC health bands for dashboard traffic-light display"
    bands:
      - { label: "Good", max: 3.0 }
      - { label: "Warning", max: 7.0 }
      - { label: "Critical" }           # unbounded above (no max)
```

Schema type:
```rust
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedStatusThreshold {
    pub name: String,
    pub description: Option<String>,
    pub bands: Vec<ParsedThresholdBand>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedThresholdBand {
    pub label: String,
    pub max: Option<f64>,              // None = unbounded (final band)
}
```

**Exhaustive bands requirement:** Threshold bands MUST be exhaustive — every possible numeric input must fall into exactly one band. Gaps between bands are rejected at validation with MC5025 ("status threshold has gap between bands"). Overlapping bands are rejected with MC5026. The validator checks that each band's `max` equals the next band's `min` (with the first band's min defaulting to negative infinity and the last band's max defaulting to positive infinity).

### Decision 2: Block placement in model YAML

All three blocks are **top-level, optional, after `rules:` and before `golden_tests:`**.

```yaml
model_format_version: 1
metadata: { ... }
dimensions: [ ... ]
hierarchies: [ ... ]
measures: [ ... ]
rules: [ ... ]
benchmarks: [ ... ]         # Phase 3G (optional)
lookup_tables: [ ... ]      # Phase 3G (optional)
status_thresholds: [ ... ]  # Phase 3G (optional)
golden_tests: [ ... ]
canonical_inputs: { ... }
test_fixtures: [ ... ]
```

**Schema addition to `ParsedModel`:**
```rust
#[serde(default)]
pub benchmarks: Vec<ParsedBenchmark>,
#[serde(default)]
pub lookup_tables: Vec<ParsedLookupTable>,
#[serde(default)]
pub status_thresholds: Vec<ParsedStatusThreshold>,
```

All three are `#[serde(default)]` — existing models without them parse unchanged.

### Decision 3: Validation rules

| Rule | Diagnostic | Severity |
|---|---|---|
| Benchmark/table name must be unique across all reference blocks | MC2037 | Error |
| `key_dimension` must reference a declared dimension | MC2038 | Error |
| Value keys must be valid element names in the referenced dimension | MC2039 | Error |
| Threshold bands must have at least 2 entries | MC2040 | Error |
| Threshold bands must have ascending `max` values | MC2041 | Error |
| Last threshold band must have `max: null` (unbounded) | MC2042 | Error |
| Threshold bands have gaps (each band's max must equal next band's min) | MC5025 | Error |
| Threshold bands overlap | MC5026 | Error |
| Benchmark `last_updated` > 12 months from current date | MC2030 | Warning (lint) |
| Lookup table referenced by no formula | MC2031 | Warning (lint) |
| Benchmark `source` field is empty string | MC3013 | Warning (lint) |

### Decision 4: `sum_over` — included in 3G with performance lint

**Decision: `sum_over(dimension, measure)` ships in Phase 3G.**

```yaml
- name: "rule_spend_share"
  target_measure: "Spend_Share"
  body: "safe_div(Spend, sum_over(Channel, Spend), 0)"
  declared_dependencies: ["Spend"]
```

**Semantics:** `sum_over(Channel, Spend)` evaluates to the sum of `Spend` across all leaf elements of the Channel dimension, at the CURRENT coordinate for all other dimensions. It's equivalent to reading the consolidated value at the top of the Channel hierarchy — but available even without a hierarchy declared.

**Performance rule:** Each eval of a cell containing `sum_over(dim, measure)` triggers N reads (where N = number of leaf elements in the named dimension). For the Acme cube (5 channels), this is 5 reads per cell eval. Manageable.

**Leaf-only semantics:** `sum_over(dimension, measure)` sums across ALL LEAF elements of the named dimension at the current coordinate (holding all other dimensions constant). It does NOT sum consolidated/parent elements — that would double-count. This matches the semantics of "what % of total is this element?" which is the primary use case.

**Lint warning:** MC3011 fires when `sum_over` is used on a dimension with > 50 leaf elements. This warns about potential performance impact (50 cell reads per eval point). At > 10,000 leaf elements, MC3011 escalates to Error (hard cap; requires explicit opt-in via a `#[allow_large_aggregation]` model-level annotation if the user deliberately wants this).

**Dep-graph implication:** `sum_over(Channel, Spend)` means that writing `Spend` at ANY channel dirties `Spend_Share` at EVERY channel (because the total changed). This is an N-to-N fan-out within one dimension. For small dimensions (5-20 elements) this is acceptable. The lint prevents pathological cases.

### Decision 5: Schema versioning — no version bump

Adding optional top-level blocks does NOT require `model_format_version: 2`. This follows the established precedent:
- Phase 3B added `description:` fields (no version bump)
- Phase 3C added `canonical_inputs:` and `test_fixtures:` (no version bump)
- Phase 3D added formula-form `body:` (no version bump)

All additions are optional (`#[serde(default)]`). Existing models parse unchanged. The version bump is reserved for backwards-INCOMPATIBLE changes (removing required fields, changing semantics of existing fields).

### Decision 6: `bucket()` returns zero-based band index

`bucket()` returns a **zero-based band index** as f64 (0.0, 1.0, 2.0, ...). The threshold block is the authoritative mapping from band_index to human label. UI, inspect, and reporting layers resolve the label from the threshold definition when rendering. Band index 0 is the FIRST band declared; band index N-1 is the last.

`bucket(CPC, "cpc_health")` returns:
- `0.0` if CPC falls in the first band ("Good": CPC <= 3.0)
- `1.0` if CPC falls in the second band ("Warning": 3.0 < CPC <= 7.0)
- `2.0` if CPC falls in the third band ("Critical": CPC > 7.0)
- `Null` if CPC is Null

Band ordering is part of the model's semantic contract — reordering bands changes the numeric output. Lint MC3013 warns if bands are declared with overlapping or gap regions.

**Rationale:** `ScalarValue` is f64 in Phases 1-3I. Returning a string label would require either (a) a string-valued ScalarValue variant (kernel change, deferred to 3J+) or (b) encoding strings as some kind of numeric ID (confusing). Numeric ranks are directly usable in downstream formulas (`if(bucket(CPC, "cpc_health") >= 2, ...)`) and the string labels are a display concern (Phase 6 UI reads the threshold definition to render "Good"/"Warning"/"Critical").

### Decision 7: Formula functions — AST nodes

| Name | Signature | AST node |
|---|---|---|
| `benchmark` | `benchmark("name", dim_ref)` | `Benchmark { name: String, key_expr: Box<ParsedRuleBody> }` |
| `lookup` | `lookup("table", dim_ref)` | `Lookup { table: String, key_expr: Box<ParsedRuleBody> }` |
| `bucket` | `bucket(value, "threshold")` | `Bucket { value: Box<ParsedRuleBody>, threshold_name: String }` |
| `sum_over` | `sum_over(dim_name, measure)` | `SumOver { dimension: String, measure: String }` |

**Key expression for `benchmark` and `lookup`:** The second argument is a dimension reference that resolves to the current element name in that dimension at eval time. For example, `benchmark("industry_cpc", Channel)` resolves `Channel` to the current Channel element name (e.g., "Paid_Search"), then looks up "Paid_Search" in the benchmark's values map.

**`lookup()` is exact-match only in Phase 3G.** If the key doesn't match any entry, returns Null. Interpolated lookups (linear interpolation between breakpoints for continuous key ranges like tax brackets) are a Phase 3G.1 candidate: `lookup_interp(table, key)`. The distinction matters because interpolation requires sorted numeric keys + a declared interpolation method (linear, step, cubic); exact-match works with any key type.

**Implementation note:** The `key_expr` for benchmark/lookup in practice is always a bare dimension reference (identifier). The parser accepts it as a general expression for forward-compat, but validation can warn if it's anything other than a dimension name (MC3014 lint: "benchmark key is a complex expression; expected a dimension name").

### Decision 8: Diagnostic codes

| Code | Fires when |
|---|---|
| **MC1013** | Formula references unknown benchmark name |
| **MC1014** | Formula references unknown lookup table name |
| **MC1015** | Formula references unknown threshold name |
| **MC1016** | `sum_over` first argument is not a declared dimension name |
| **MC2030** | Benchmark `last_updated` is > 12 months old (lint warning) |
| **MC2031** | Reference data block (benchmark/table/threshold) is unreferenced by any formula (lint) |
| **MC2037** | Duplicate reference-data name across blocks |
| **MC2038** | `key_dimension` references undeclared dimension |
| **MC2039** | Value key is not a valid element in the key dimension |
| **MC2040** | Threshold has fewer than 2 bands |
| **MC2041** | Threshold bands have non-ascending max values |
| **MC2042** | Last threshold band has a max (should be unbounded) |
| **MC3011** | `sum_over` on dimension with > 50 elements (performance lint) |
| **MC3013** | Benchmark `source` field is empty (provenance lint) |
| **MC3014** | `benchmark`/`lookup` key argument is a complex expression, not a dimension name (lint) |
| **MC3015** | Benchmark `last_updated` date is more than 12 months in the past. Suggestion: "Benchmark [name] was last updated [date]; consider refreshing from [source]." Prevents models from shipping stale industry standards without awareness. |

---

## Out of scope

| Out of scope | Phase / disposition |
|---|---|
| Fitted model coefficients (`fitted_models:` block) | Phase 3H |
| Calibration maps (`calibration_maps:` block) | Phase 3H |
| Multi-key lookup tables (composite key across 2+ dimensions) | Future extension if demand surfaces |
| Interpolated lookups (`lookup_interp(table, key)` — linear interpolation between breakpoints for continuous key ranges like tax brackets) | Phase 3G.1 candidate. Requires sorted numeric keys + a declared interpolation method (linear, step, cubic); exact-match works with any key type. |
| String-valued lookup results (e.g., territory → region name mapping) | Requires string ScalarValue; 3J+ |
| Dynamic reference data (values that change per scenario/version) | Not reference data — use input measures |
| `avg_over`, `min_over`, `max_over` (other aggregations over dims) | Future extension; `sum_over` is the MVP |
| External data source for benchmarks (API fetch at model load) | Phase 5+ integration |
| Benchmark version history (track changes over time) | Future — source attribution is sufficient for 3G |
| String operations (`concat`, `lower`, `contains`) in formulas | Deferred indefinitely. The intended substitute is Tessera recipe-time conditional dimension derivation (e.g., "if source column contains 'brand', assign to Brand_Awareness channel"). Note: Phase 5A's current recipe schema does NOT support conditional derivation; this capability must be added in Phase 5C or later for the deferral to hold. If Phase 5C does not deliver it, string operations may need reconsideration for Phase 3I. |

---

## Alternatives considered

1. **Encode benchmarks as input measures with hardcoded canonical_inputs values.** Rejected — loses provenance (source, date), mixes reference data with operational data, and cannot lint for staleness.

2. **Put reference data in a separate YAML file (not inline in the model).** Rejected for Phase 3G — adds file-management complexity. The reference data is small (tens of entries, not thousands). If models grow to need large reference datasets, a future phase can add `source: ./benchmarks.yaml` file-reference syntax (same pattern as `canonical_inputs.source`).

3. **Make `benchmark()` a general `ref_data(type, name, key)` function.** Rejected — separate functions (`benchmark`, `lookup`, `bucket`) are clearer in intent and enable targeted diagnostics. A generic function would need per-type error messages anyway.

4. **Return string labels from `bucket()`.** Rejected — ScalarValue is f64 through Phase 3I. Numeric ranks are usable in formulas; string labels are a UI display concern.

5. **Defer `sum_over` to Phase 3I due to performance risk.** Rejected — the performance risk is bounded (lint warns at > 50 elements) and the user value is immediate (share-of-total is a top-5 requested pattern). The dep-graph fan-out is manageable for typical cube sizes.

6. **Require `model_format_version: 2` for models using reference-data blocks.** Rejected — the blocks are additive and optional. Version bumps are for breaking changes only.

7. **Support weighted benchmarks (benchmark value differs by multiple dimensions).** Rejected for 3G — single key_dimension is sufficient for common cases. Multi-key lookups are a future extension.

8. **Allow threshold bands to be open-ended on BOTH sides (no minimum for first band).** Decided: first band implicitly starts at negative infinity. The schema only declares `max` per band; the first band catches everything below its `max`. This is simpler than requiring both `min` and `max` per band with overlap validation.

---

## Cross-links

- [`0011-phase-3e-conditionals-and-basic-operations.md`](0011-phase-3e-conditionals-and-basic-operations.md) — Phase 3E (prerequisite; `safe_div` used in benchmark-comparison patterns)
- [`0012-phase-3f-time-series-operations.md`](0012-phase-3f-time-series-operations.md) — Phase 3F (time-keyed lookups benefit from time-series infrastructure)
- [`../research-notes/formula-language-expansion.md`](../research-notes/formula-language-expansion.md) — full expansion research (3E through 3J)
- [`../../crates/mc-model/src/schema.rs`](../../crates/mc-model/src/schema.rs) — `ParsedModel` struct (3 new Vec fields + new schema types)
- [`../../mosaic-plugin/skills/schema-design/SKILL.md`](../../mosaic-plugin/skills/schema-design/SKILL.md) — schema documentation (updated at 3G ship)
- [`0010-phase-5-tessera-architecture.md`](0010-phase-5-tessera-architecture.md) — Phase 5 Tessera (reference data may eventually be importable via Tessera recipes)

---

## Notes

Phase 3G is the architectural foundation for Phase 3H. The design of the reference-data blocks, the validator, and the eval-time lookup machinery should be built with 3H in mind:

- The `ParsedBenchmark` / `ParsedLookupTable` / `ParsedStatusThreshold` types are instances of a pattern. Phase 3H adds `ParsedFittedModel` and `ParsedCalibrationMap` as additional instances.
- The eval-time lookup path (`eval_benchmark(name, key) -> f64`) is a trait-like dispatch. Phase 3H adds `eval_predict(model_id, features) -> f64` using the same dispatcher infrastructure.
- The lint/validation machinery (stale-date checks, unreferenced-block checks) generalizes across all reference-data types.

**Do not build Phase 3G's infrastructure as specific to benchmarks/lookups/thresholds.** Build it as a generic "named reference data with typed eval" pattern, then instantiate it three times in 3G. Phase 3H instantiates it two more times with zero architectural changes.

**The "fitted artifact" insight:** All five reference-data types (3G's three + 3H's two) share the same lifecycle: declared in YAML at authoring time, validated at model-load time, read at formula-eval time, attributed with metadata for provenance. The only difference is eval semantics. This is the architectural keystone of Phases 3G-3H.
