# Phase 3K Handoff — Model Authoring Ergonomics

**Status:** Proposed (next to start)
**Date:** 2026-05-26
**ADR:** [ADR-0030](../decisions/0030-model-authoring-ergonomics.md) (Proposed — accept before implementation)
**Estimated effort:** 1–2 sessions
**Crate:** `mc-model` only (no kernel changes, no API breakage)
**Branch:** `phase-3k/model-authoring-ergonomics`

---

## What this phase ships

Two improvements that eliminate the bulk of new-cube authoring friction:

1. **Auto-element population** — Standard/Time dimensions with empty `elements` are populated from matching `canonical_inputs` columns automatically.
2. **JSON schema generation** — `mc-model-schema` binary emits a JSON schema for editor autocomplete and inline validation.

Validated against a real session (MLB totals, 2,353 games) where these would have removed 8 of the 10 friction points hit.

---

## Phase 3K scope

| # | Feature |
|---|---|
| 1 | Auto-populate empty `Standard`/`Time` dimensions from `canonical_inputs` |
| 2 | New diagnostic MC1015 (info severity) reporting auto-population |
| 3 | `#[derive(JsonSchema)]` on `ParsedModel` and transitive types |
| 4 | `mc-model-schema` binary that emits the schema to stdout |
| 5 | Checked-in schema at `docs/specs/mosaic-model-schema.json` |
| 6 | Doc comments on schema fields for editor hover tooltips |
| 7 | CI drift check (schema must match generated output) |
| 8 | Update Acme example with the `$schema=` directive |

---

## Implementation path

### Step 1: Auto-element population

**Location:** `crates/mc-model/src/inputs.rs` or `crates/mc-model/src/lib.rs` — wherever `resolve_input_set` is called, add a post-resolution dimension fixup.

**Logic:**
```rust
// After resolve_input_set succeeds:
fn auto_populate_dimensions(
    parsed: &mut ParsedModel,
    inputs: &ResolvedInputSet,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for dim in &mut parsed.dimensions {
        // Skip if explicit elements declared
        if !dim.elements.is_empty() { continue; }
        // Skip Scenario/Version/Measure (semantic dimensions)
        match dim.kind {
            DimensionKind::Standard | DimensionKind::Time => {},
            _ => continue,
        }
        // Look up CSV column by dimension name
        let Some(column_values) = inputs.distinct_values_for_column(&dim.name) else {
            continue; // No matching column — fall through to existing missing-elements error
        };
        // Populate in first-seen order
        for value in column_values {
            dim.elements.push(ParsedElement {
                name: value.clone(),
                parent: None,
                // ... other fields default
            });
        }
        // Emit MC1015 info diagnostic
        diagnostics.push(Diagnostic::info(
            "MC1015",
            format!(
                "Dimension '{}' populated automatically from canonical_inputs ({} elements)",
                dim.name, dim.elements.len()
            ),
        ));
    }
}
```

**Key checks:**
- `dim.elements.is_empty()` — explicit declaration always wins
- `DimensionKind::Standard | DimensionKind::Time` — only these kinds get auto-populated
- Column name lookup is **case-sensitive exact match** to dimension name
- Element ordering matches CSV first-seen order (not sorted)
- The diagnostic is `info` severity, not `warning` — but MUST surface by default (per Amendment 4), not only with `--verbose`

**Helper needed:** `ResolvedInputSet::distinct_values_for_column(name) -> Option<Vec<String>>` — returns distinct values from the named column, in first-seen order. If the column doesn't exist, return `None`. Check if this already exists; if not, add it.

**Also add:** `ResolvedInputSet::find_column_case_insensitive(name) -> Option<&str>` — returns the actual column name (preserving original casing) if a case-insensitive match exists. Used by the case-mismatch hint in Step 1b.

### Step 1a: High-cardinality guardrail (per Amendment 2)

After auto-population succeeds, check the element count and emit escalating diagnostics:

```rust
let count = dim.elements.len();
if count <= 10_000 {
    diagnostics.push(Diagnostic::info("MC1015", format!(
        "Dimension '{}' populated automatically from canonical_inputs ({} elements)",
        dim.name, count
    )));
} else if count <= 100_000 {
    diagnostics.push(Diagnostic::warning("MC1016", format!(
        "High-cardinality auto-population: dimension '{}' has {} elements. \
         High-cardinality dimensions may indicate the data belongs as a fact \
         rather than a dimension. Consider whether '{}' should be modeled differently.",
        dim.name, count, dim.name
    )));
} else {
    diagnostics.push(Diagnostic::critical("MC1017", format!(
        "Very high-cardinality auto-population: dimension '{}' has {} elements. \
         This is almost certainly a modeling error — '{}' likely belongs as fact data, \
         not as a dimension. Auto-population proceeded but review the cube design.",
        dim.name, count, dim.name
    )));
}
```

**Critical:** even MC1017 does NOT block compilation — the author can opt out by declaring elements explicitly (explicit always wins). The diagnostic surfaces the problem; the author decides.

### Step 1b: Case-mismatch hint on fall-through (per Amendment 1)

When auto-population does NOT fire because no exact-case-match column exists, AND validation is about to emit the missing-elements error (MC1001 or equivalent), add a hint that detects case-insensitive matches:

```rust
// In the missing-elements validator path:
let hint = if let Some(actual_column) = inputs.find_column_case_insensitive(&dim.name) {
    Some(format!(
        "canonical_inputs has column '{}' (case differs from dimension '{}'). \
         Auto-population requires exact case match. Either rename the dimension to '{}', \
         rename the CSV column to '{}', or declare elements explicitly.",
        actual_column, dim.name, actual_column, dim.name
    ))
} else {
    None
};
// Attach hint to the diagnostic
```

Only fires for Standard/Time dimensions (kinds eligible for auto-population). Scenario/Version/Measure get the plain missing-elements error.

### Step 2: JSON schema generation

**Add dependency** to `crates/mc-model/Cargo.toml` (per Amendment 3 — EXACT pin required to prevent CI drift false positives):
```toml
[dependencies]
# Exact pin: schemars output formatting is not stable across minor versions.
# CI drift check depends on byte-for-byte stable output.
schemars = "=0.8.21"
```

Verify MSRV: `schemars 0.8.21` works at Rust 1.65+, well within our 1.78 pin. If the latest 0.8.x version requires a newer MSRV, pin the newest 0.8.x that compiles at 1.78. Document the version choice with a comment explaining why exact-pin matters.

**Add derives** to `crates/mc-model/src/schema.rs`:
```rust
use schemars::JsonSchema;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ParsedModel {
    pub model_format_version: u32,
    pub metadata: Option<ParsedMetadata>,
    pub dimensions: Vec<ParsedDimension>,
    // ...
}
```

Apply to every public type in the model schema: `ParsedModel`, `ParsedMetadata`, `ParsedDimension`, `ParsedElement`, `ParsedMeasure`, `ParsedRule`, `ParsedFittedModel`, `ParsedCoefficient`, `ParsedCalibrationMap`, `ParsedLookupTable`, `ParsedBenchmark`, `ParsedStatusThreshold`, `ParsedGoldenTest`, `ParsedTestFixture`, `ParsedParameter`, and all enums (`DimensionKind`, `MeasureRole`, `AggregationRule`, `CellDataType`, `Scope`, etc.).

**Add doc comments** to fields that lack them — these become hover tooltips in editors. Per Amendment 5, treat the historically-confusing fields (target_measure, declared_dependencies, method, coefficients structure, predict() body quoting) as PRIORITY targets for excellent doc comments — that's where the hover-tooltip pays for itself:

```rust
pub struct ParsedRule {
    /// Unique name for this rule (must be distinct across the model).
    /// Separate from `target_measure`: the name identifies the rule itself,
    /// target_measure identifies which measure receives the computed value.
    pub name: String,

    /// The measure this rule's body computes. SEPARATE from `name` — a rule
    /// named "compute_revenue" might target the "Revenue" measure. The same
    /// measure can only be targeted by one rule.
    pub target_measure: String,

    /// Evaluation scope — controls which leaves the rule fires at.
    /// Most common: `AllLeaves` (every leaf coordinate in the cube).
    pub scope: Scope,

    /// Formula expression. Examples:
    ///   "Customers * AOV"
    ///   "if(x > 0, x, 0)"
    ///   'predict("model_name", feature1, feature2)' — note model name MUST be quoted
    pub body: String,

    /// Measures this rule reads. Required for dependency-graph correctness —
    /// omitting a dependency that the body actually reads causes MC2030.
    /// Include every measure named in the body expression.
    pub declared_dependencies: Vec<String>,
}

pub struct ParsedFittedModel {
    /// Method identifier. NOTE: this field is called `method`, NOT `type`.
    /// Common values: "linear", "logistic", "lasso".
    pub method: String,

    /// Model intercept (bias term added before features × coefficients).
    pub intercept: f64,

    /// Per-feature coefficients as a list of {feature, weight} entries.
    /// NOTE: this is a LIST of objects, not a flat map.
    /// Example:
    ///   coefficients:
    ///     - { feature: "Weather_Temp", weight: 0.012 }
    ///     - { feature: "Wind_MPH", weight: 0.035 }
    pub coefficients: Vec<ParsedCoefficient>,
    // ...
}
```

The exact wording matters less than the explicit warning callouts for the fields that authors get wrong (NOTE: ...). These are the lines that convert hover-tooltip into friction reduction.

**Create binary** at `crates/mc-model/src/bin/mc-model-schema.rs`:
```rust
use mc_model::schema::ParsedModel;
use schemars::schema_for;

fn main() {
    let schema = schema_for!(ParsedModel);
    let json = serde_json::to_string_pretty(&schema)
        .expect("schema serialization");
    println!("{json}");
}
```

**Generate the schema once and commit:**
```bash
cargo run --bin mc-model-schema --quiet > docs/specs/mosaic-model-schema.json
git add docs/specs/mosaic-model-schema.json
```

### Step 3: CI drift check

Add to the project's existing CI script (or document for manual run):
```bash
# Schema drift check — fails if structs and committed schema diverge
diff <(cargo run --bin mc-model-schema --quiet) docs/specs/mosaic-model-schema.json
```

If diff produces output, the structs changed but the schema wasn't regenerated. Fail the build.

### Step 4: Update Acme example

In `crates/mc-model/examples/acme.yaml`, add to the very top (before `model_format_version`) — use the RELATIVE path (per Amendment 6, works offline, no network required):
```yaml
# yaml-language-server: $schema=../../docs/specs/mosaic-model-schema.json
model_format_version: 1
```

For external users, document the absolute URL in a README — but call it out as a DEVELOPMENT PLACEHOLDER, not the permanent answer:
```yaml
# Development placeholder — will be replaced with a stable versioned URL
# (e.g., schemas.mosaic.dev/v1/model.json) when Mosaic publishes a public
# schema endpoint.
# yaml-language-server: $schema=https://raw.githubusercontent.com/edwinlov3tt/mc-v2/main/docs/specs/mosaic-model-schema.json
```

### Step 5: Tests

**Unit tests** in `crates/mc-model/src/inputs.rs` (or a new test file):
```rust
#[test]
fn t_auto_populate_empty_standard_dim() {
    // Cube with Game dim elements=[] and canonical_inputs containing Game column
    // Assert: after load, Game dim has elements from CSV
}

#[test]
fn t_explicit_elements_wins_over_auto_populate() {
    // Cube with Game dim elements=[{name: "A"}] and canonical_inputs containing Game column with B, C, D
    // Assert: Game dim has only [A] — explicit wins, auto-populate skipped
}

#[test]
fn t_scenario_dim_not_auto_populated() {
    // Cube with Scenario dim elements=[] and canonical_inputs containing Scenario column
    // Assert: validation fails with missing-elements error (auto-populate doesn't apply to Scenario)
}

#[test]
fn t_mc1015_diagnostic_emitted() {
    // Cube with auto-populated dim, count <= 10K
    // Assert: diagnostics contain MC1015 with correct dimension name and count
}

#[test]
fn t_first_seen_ordering_preserved() {
    // CSV with column values [Z, A, M, A, Z, B] (duplicates)
    // Assert: dim elements are [Z, A, M, B] in that order (not sorted alphabetically)
}

#[test]
fn t_no_matching_column_falls_through() {
    // Cube with Game dim elements=[] and canonical_inputs WITHOUT a Game column
    // Assert: existing missing-elements error fires (not silent success)
}

#[test]
fn t_mlb_cube_validates_without_explicit_elements() {
    // Acceptance test: rewrite the MLB cube (or fixture equivalent) to omit
    // the Game dim elements list. Validate succeeds.
}

// ─── Amendment 1: case-mismatch hint ───
#[test]
fn t_case_mismatch_hint_in_fallthrough_error() {
    // Cube with Game dim elements=[] and canonical_inputs containing 'game' (lowercase)
    // Assert: MC1001 fires (no auto-populate due to casing)
    // Assert: diagnostic hint mentions 'game' as a case-insensitive match
    // Assert: hint suggests renaming dim, renaming column, OR explicit declaration
}

#[test]
fn t_no_case_hint_when_no_close_match() {
    // Cube with Game dim elements=[] and canonical_inputs with completely unrelated columns
    // Assert: MC1001 fires WITHOUT a case-mismatch hint (don't suggest spurious matches)
}

// ─── Amendment 2: high-cardinality guardrail ───
#[test]
fn t_high_cardinality_warning_above_10k() {
    // Build cube with canonical_inputs having 10,001 distinct dim values (synthetic)
    // Assert: MC1016 warning fires
    // Assert: auto-population still succeeds (warning, not error)
}

#[test]
fn t_critical_warning_above_100k() {
    // Synthetic test with 100,001 distinct values
    // Assert: MC1017 critical fires
    // Assert: auto-population still succeeds (critical, not block)
}

#[test]
fn t_no_warning_under_threshold() {
    // Cube with ~2,500 elements (MLB-sized)
    // Assert: only MC1015 fires, no MC1016 or MC1017
}

// ─── Amendment 4: MC1015 surfaces by default ───
#[test]
fn t_mc1015_in_default_diagnostic_stream() {
    // Run validate WITHOUT verbose flag on auto-populated cube
    // Assert: MC1015 appears in the diagnostic output stream
    // (Not just available via --verbose)
}
```

**Schema generation test:**
```rust
#[test]
fn t_schema_is_valid_json() {
    let schema = schemars::schema_for!(ParsedModel);
    let json = serde_json::to_value(&schema).expect("serialize");
    // Assert top-level keys exist
    assert!(json.get("$schema").is_some());
    assert!(json.get("title").is_some());
    assert!(json.get("properties").is_some());
}
```

### Step 6: Verify in an editor (manual)

Open `crates/mc-model/examples/acme.yaml` in VSCode with the YAML extension installed. Confirm:
- Autocomplete works on field names
- Hover on `target_measure` shows the doc comment
- Typing an invalid field name produces a red squiggle

---

## Files to modify

| File | Change |
|---|---|
| `crates/mc-model/Cargo.toml` | Add `schemars = "0.8"`, add `[[bin]]` entry for mc-model-schema |
| `crates/mc-model/src/schema.rs` | Add `#[derive(JsonSchema)]` to all public types, add doc comments to undocumented fields |
| `crates/mc-model/src/inputs.rs` | Add `auto_populate_dimensions()` function and call it after `resolve_input_set` |
| `crates/mc-model/src/lib.rs` | Wire auto-population into the load pipeline (if not done in inputs.rs) |
| `crates/mc-model/src/diagnostic.rs` | Add MC1015 to the diagnostic codes |
| `crates/mc-model/src/bin/mc-model-schema.rs` | NEW — binary that emits JSON schema |
| `docs/specs/mosaic-model-schema.json` | NEW — generated schema, committed |
| `crates/mc-model/examples/acme.yaml` | Add `$schema=` directive at top |
| `crates/mc-model/tests/` | NEW — auto-population test suite (7 tests above) |

---

## Acceptance criteria

1. Model with `elements: []` on a Standard dim + matching CSV column → validates and compiles
2. Model with explicit elements on a dim → those elements are NOT overridden by CSV values
3. Scenario/Version/Measure dimensions with empty elements + matching CSV column → still fail with missing-elements error (auto-population skipped)
4. MC1015 info diagnostic emitted with dimension name and element count
5. MC1015 surfaces in DEFAULT validate output, not only with `--verbose` (Amendment 4)
6. First-seen ordering preserved (not alphabetical sort)
7. Missing CSV column → existing missing-elements error fires (not silent success)
8. **Case mismatch produces hint** (Amendment 1): dim `Game` + column `game` → MC1001 with hint pointing to `game` and suggesting rename or explicit declaration
9. **High-cardinality warnings** (Amendment 2): MC1016 fires above 10K elements, MC1017 fires above 100K; auto-population still succeeds in both cases
10. **schemars exact-pinned** (Amendment 3): `Cargo.toml` uses `=0.8.X` form, not range
11. **Doc comments on confusing fields** (Amendment 5): `target_measure`, `declared_dependencies`, `method`, `coefficients`, `body` (predict quoting) all have explicit NOTE callouts in their doc comments
12. **Acme example uses relative `$schema=` path** (Amendment 6), not GitHub URL
13. `cargo run --bin mc-model-schema` outputs valid JSON Schema with `$schema`, `title`, `properties` keys
14. Committed `docs/specs/mosaic-model-schema.json` matches generated output (CI drift check passes)
15. VSCode with YAML extension applies the schema (manual verify — autocomplete + hover work)
16. MLB cube fixture (or equivalent) validates with Game dim elements omitted
17. `cargo test --workspace` passes
18. `cargo clippy --all-targets --workspace -- -D warnings` passes
19. `cargo fmt --check --all` clean
20. No changes to `mc-core`

---

## Cross-links

- **ADR-0030:** All binding decisions for this phase
- **ADR-0006 (Phase 3C):** canonical_inputs introduction — this builds on that infrastructure
- **MLB cube example:** `examples/sports-betting/mlb-totals.yaml` (the model that motivated this phase)
- **mc-model loader:** `crates/mc-model/src/lib.rs`
- **mc-model schema:** `crates/mc-model/src/schema.rs`
- **mc-model inputs:** `crates/mc-model/src/inputs.rs`

---

**End of handoff. Small surgical phase. Auto-population is ~80 lines + 7 tests. Schema is mostly `#[derive(JsonSchema)]` annotations + doc comments + a small binary.**
