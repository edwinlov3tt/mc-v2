# Phase 7A.1 Handoff — Narrative Engine Productionization

> **Audience:** the Claude Code instance that implements Phase 7A.1.
> **You inherit `main` at `5d173ea` (926 / 0 / 5 tests + demo server).
> You'll work on the branch `phase-7a-1/narrative-engine`.**
>
> **This phase extracts Phase 6D's demo narrative engine into a
> permanent `mc-narrative` crate, upgrades the expression evaluator
> to use `mc_model::parse_expression` (the real formula engine),
> and ships `mc model narrate` as a first-class CLI verb.**
>
> **The binding design doc is [`docs/decisions/0020-phase-7a-narrative-engine-plan.md`](../decisions/0020-phase-7a-narrative-engine-plan.md).**
> It has the strategic framing, scope split, architectural decisions
> (Q1-Q8 answered), audit-driven requirements (6 items), diagnostic
> codes, and success criteria. Read it in full before starting.

---

## The one paragraph you must internalize

Phase 6D proved the concept: YAML templates + deterministic evaluation
= instant marketing reports. But the demo's evaluator is a hack —
`count_where` conditions are pre-computed as literal HashMap keys,
bindings only resolve 1 level deep, `NOT` isn't supported, dimension
names are guessed from table types. **Phase 7A.1 makes it real** by
integrating with `mc_model::parse_expression` (the formula engine
that already handles arbitrary expressions, shipped in Phase 3I with
912+ tests). The narrative engine becomes a CONSUMER of the formula
engine, not a parallel implementation. Adding a template with ANY
formula expression (that the formula engine supports) Just Works.

---

## The 6 binding requirements (from Phase 6D audit)

These are the specific gaps 7A.1 closes. Each was surfaced by the
Phase 6D implementer's honest self-audit:

| # | Gap | 7A.1 fix |
|---|---|---|
| 1 | `count_where`/`any_where` pre-computed, not generic | **Integrate with `mc_model::parse_expression`** — evaluate arbitrary predicates against real cube data at runtime |
| 2 | Dedup hardcoded by template ID in Rust | **`deduplicate: true` field in YAML schema** — engine checks, not Rust match arms |
| 3 | Two-pass binding resolution (max 1 level of reference) | **DAG-ordered resolution** — topological sort; bindings reference other bindings to any depth (cycle detection) |
| 4 | `NOT` operator missing | **Comes free with formula engine** (already has `not()`) |
| 5 | Dimension names guessed from table type string | **Read from `Cube::dimensions()`** — the cube knows its own dims |
| 6 | UTF-8 byte-vs-character indexing bugs | **Moot with formula parser** (already handles UTF-8 correctly) |

---

## Architecture (what gets built)

```
crates/mc-narrative/          ← NEW CRATE (extracted from mc-demo-server)
├── Cargo.toml               (deps: mc-core, mc-model, serde, serde_yaml)
├── src/
│   ├── lib.rs               (public API: evaluate_templates, load_templates)
│   ├── schema.rs            (TemplateDefinition, TemplateFile, Severity, FormatHint)
│   ├── context.rs           (build EvalContext from cube data for formula engine)
│   ├── evaluator.rs         (evaluate when: + bindings: via mc_model::parse_expression)
│   ├── renderer.rs          (template string substitution + format hints)
│   ├── composition.rs       (per-cell → per-section → per-report assembly)
│   └── error.rs             (MC7001-MC7010 diagnostic codes)
└── tests/
    ├── template_loading.rs  (YAML parse + validation)
    ├── expression_eval.rs   (formula engine integration tests)
    ├── rendering.rs         (format hints, placeholder substitution)
    └── regression.rs        (all 14 Phase 6D templates produce same output)

crates/mc-demo-server/src/narrative.rs
  → Becomes a thin wrapper: imports mc_narrative::evaluate_templates
  → Existing demo behavior unchanged (regression check)

crates/mc-cli/src/narrate.rs  ← NEW
  → mc model narrate <model.yaml> --period <p> [--format json|text|markdown]
  → Uses mc_narrative::evaluate_templates against a compiled cube

crates/mc-cli/src/mcp.rs
  → New tool: mosaic.model.narrate (parallels mosaic.model.query)
```

### The key integration: `mc_model::parse_expression` → `mc_core::eval_expr`

Phase 3I shipped `mc_model::parse_expression(&str) -> Result<ParsedRuleBody, FormulaError>`. Phase 3J extended it to handle `current_element`, `is_element`, string comparisons. The formula engine evaluates these against a cube context.

**For narrative templates, the integration is:**

```rust
// In mc-narrative/src/evaluator.rs:

use mc_model::parse_expression;
use mc_core::{Cube, eval_expr_unified};

/// Evaluate a template's `when:` predicate against a cube.
pub fn evaluate_when(expr_str: &str, cube: &mut Cube, coord: &CellCoordinate, refs: &Refs) -> bool {
    let parsed = parse_expression(expr_str).ok()?;
    let result = eval_expr_unified(&parsed, cube, coord, refs).ok()?;
    match result {
        ScalarValue::F64(v) => v.abs() > 1e-9,  // truthy
        ScalarValue::Null => false,
        _ => false,
    }
}

/// Evaluate a template binding (produces a value for substitution).
pub fn evaluate_binding(expr_str: &str, cube: &mut Cube, coord: &CellCoordinate, refs: &Refs) -> Option<Value> {
    let parsed = parse_expression(expr_str).ok()?;
    let result = eval_expr_unified(&parsed, cube, coord, refs).ok()?;
    match result {
        ScalarValue::F64(v) => Some(Value::Num(v)),
        ScalarValue::Str(s) => Some(Value::Str(s)),
        ScalarValue::Null => None,
    }
}
```

**This means ANY formula function the engine supports works in templates:**
- `prev(Measure)`, `lag(Measure, N)` — time-series comparisons
- `avg_over(Measure, Dim)`, `max_over(Measure, Dim)` — cross-coord aggregates
- `is_element(Dim, "Element")` — conditional on coordinate
- `if(cond, then, else)`, `ifs(...)` — branching
- `pow()`, `sqrt()`, `ln()`, `norm_cdf()` — math primitives
- ALL Phase 3J additions (`current_element`, `scenario_ref`, `param()`, etc.)

No custom evaluator needed. The formula engine IS the evaluator.

### What about `count_where` / `any_where`?

These become **new formula functions** in `mc-narrative` (not in mc-core):

```rust
// Narrative-specific aggregate functions evaluated at the narrative layer:
// They iterate over a dimension's elements and apply a predicate.

fn count_where(predicate: &ParsedRuleBody, dim: &Dimension, cube: &mut Cube, base_coord: &CellCoordinate, refs: &Refs) -> f64 {
    dim.leaf_elements().iter()
        .filter(|elem| {
            let coord = base_coord.with_element(dim.id(), elem.id());
            evaluate_when_at_coord(predicate, cube, &coord, refs)
        })
        .count() as f64
}
```

These are NOT mc-core formula functions (they don't belong in the kernel). They're narrative-layer aggregates that the narrative engine evaluates using the formula engine as a building block.

---

## Implementation steps (5 sessions estimated)

### Session 1 (~3-4h): Crate extraction + public API

**Goal:** `crates/mc-narrative/` exists as a proper crate with the
public API, template YAML loading, and a regression test proving
Phase 6D output is unchanged.

**Deliverables:**
- New `crates/mc-narrative/Cargo.toml` (deps: mc-core, mc-model, serde, serde_yaml, thiserror)
- `src/lib.rs` — public API: `pub fn load_templates(path)`, `pub fn evaluate_templates(templates, cube, refs)`
- `src/schema.rs` — `TemplateDefinition`, `TemplateFile`, `Severity`, `FormatHint` (lift from existing YAML shape)
- `src/error.rs` — MC7001-MC7010 error types
- `tests/regression.rs` — load `demo/narratives/display-like.yaml`, evaluate against Scotts RV sample data, assert same 14 outputs as Phase 6D
- Update `mc-demo-server` to depend on `mc-narrative` instead of inline code

**Key decision: How does mc-demo-server use mc-narrative?**

The demo server currently has its own `narrative.rs` with a pre-computed context + mini evaluator. Session 1 replaces that with:
```rust
// mc-demo-server/src/narrative.rs becomes:
use mc_narrative::{load_templates, evaluate_templates};

pub fn evaluate_all(cubes: &[IngestedCube]) -> Vec<NarrativeOutput> {
    let templates = load_templates("demo/narratives/");
    // Convert IngestedCube → mc_core::Cube (may need a bridge)
    evaluate_templates(&templates, &cubes_as_mc_core, &refs)
}
```

**Wall to expect:** `IngestedCube` (the demo server's type) is NOT the same as `mc_core::Cube`. The demo server populates cubes via `CubeBuilder` directly. You need to either:
- A. Have mc-narrative accept `mc_core::Cube` (and the demo server passes its already-built cubes through) — **preferred**
- B. Have mc-narrative define its own cube abstraction — **wrong direction**

**Binding choice: A.** mc-narrative's `evaluate_templates` takes `&mut Cube` (mc-core's type). The demo server already builds `mc_core::Cube` instances in `ingest.rs`. Wire them through.

---

### Session 2 (~4-5h): Formula engine integration (the hard session)

**Goal:** Replace the pre-computed context HashMap with real
`mc_model::parse_expression` + `mc_core::eval_expr` calls.

**Deliverables:**
- `src/evaluator.rs` — `evaluate_when()` and `evaluate_binding()` using the formula engine
- `src/context.rs` — builds the evaluation context that the formula engine needs (coordinate, refs, cube state)
- All `when:` predicates in `display-like.yaml` now evaluate via the formula engine
- All `bindings:` expressions evaluate via the formula engine
- `count_where`, `any_where`, `names_where`, `first_where` as narrative-layer aggregate functions (iterating dims + calling formula engine per element)
- `NOT` operator works (formula engine already supports `not()`)
- Dimension names read from cube schema (not guessed from table type)

**This is the hardest session.** The formula engine's eval expects:
- A `Cube` reference (you have this — from Session 1)
- A `CellCoordinate` (the "current" coordinate being evaluated)
- A `Refs` (the compiled model refs — measure IDs, dim IDs, etc.)

For narrative templates, the "current coordinate" might be:
- The "latest time period at the campaign-total level" (for time-series templates)
- Each element in a dimension (for `count_where` iteration)
- A synthetic "all-time aggregate" coordinate (for benchmark comparison)

**Binding approach:** build a `NarrativeCoord` that represents "evaluate this expression at this specific cube state." For simple cases (campaign-total), it's the leaf coordinate at the latest time period. For aggregate cases (`avg_over`), the formula engine already handles that internally.

**Regression check:** all 14 templates still produce the same output as Phase 6D. The formula engine should produce identical numeric results to the pre-computed approach (because the formulas are the same — just evaluated by a different evaluator).

---

### Session 3 (~3-4h): Template composition + `mc model narrate` CLI verb

**Goal:** Templates compose into sections and reports. New CLI verb
works end-to-end.

**Deliverables:**
- `src/composition.rs` — `ReportDefinition` (optional): sections with `include_narratives` filters (severity, section, limit, ordering)
- If no `ReportDefinition` exists, flat list of all narratives (backward compat with Phase 6D behavior)
- `crates/mc-cli/src/narrate.rs` — new verb:
  ```
  mc model narrate <model.yaml> [--templates <dir>] [--period <p>] [--format json|text|markdown]
  ```
- Verb loads model → compiles cube → populates from canonical_inputs → loads templates → evaluates → renders per format
- MCP tool: `mosaic.model.narrate` in `mcp.rs` (same pattern as existing tools)
- JSON output matches the planning doc's "structured output as contract" shape

**Key decision:** The `mc model narrate` verb needs a populated cube. Today, cubes are populated via `mc model test` (canonical inputs) or via Tessera. For `narrate`, the verb should:
1. Compile the model
2. Apply canonical_inputs (same as `mc model test`)
3. Optionally apply write-log (if `--include-writes` per LoadPolicy)
4. Load templates from `--templates <dir>` (default: `./narratives/` relative to model)
5. Evaluate templates
6. Output

This matches the existing Phase 6A `mc model query` flow but adds template evaluation at the end.

---

### Session 4 (~3-4h): Format hints + notability + deduplicate + polish

**Goal:** Production-quality template rendering with proper format
hints, notability filtering, and dedup.

**Deliverables:**
- `src/renderer.rs` — format hint implementation:
  - `currency` → $11,500
  - `percent_0/1/2` → 23% / 23.4% / 23.41%
  - `count` → 8,420
  - `count_short` → 8.4K
  - `delta_signed` → +47 / -312
  - `date_short` → Mar 2026
  - `decimal_2` → 0.42
- `deduplicate: true` in YAML schema — engine tracks which template IDs have fired; skips duplicates
- DAG-ordered binding resolution — topological sort of binding references; cycle detection
- `on_null: skip | placeholder` per binding (planning doc Q4 answer)
- `sort_order` field respected (data_sufficiency fires first at sort_order: -10)
- MC7001-MC7010 validator: catches invalid templates at load time (unknown measures, bad format hints, undefined placeholders)

---

### Session 5 (~2-3h): Plugin skill + tests + migration verification

**Goal:** Ship. Plugin skill teaches LLMs to author templates.
Full regression suite. Phase 6D demo still works unchanged.

**Deliverables:**
- `mosaic-plugin/skills/narratives/SKILL.md` — teaches the LLM:
  - What narrative templates are and how they work
  - The YAML schema with examples
  - How to inspect a cube's measures/dims and identify narrative candidates
  - How to test a template against canonical inputs
  - Best practices (severity ladder, format hints, notability thresholds)
- `mc-demo-server` regression: still produces 17 narratives, same content, same timing
- `mc model narrate` regression against the Scotts RV sample data
- All existing 926 tests pass + new mc-narrative tests
- MC7001-MC7010 codes swept against main (pre-flight per Rule 3)

---

## Hard Rules (binding)

1. **`mc-core` is NOT modified.** mc-narrative CONSUMES mc-core; doesn't extend it. No new Expr variants, no new public functions in mc-core.
2. **`mc-model` is NOT modified** (except: if `parse_expression` needs a minor signature tweak to accept a context-without-cube, surface as SPEC QUESTION).
3. **`mc-fixtures`, `mc-recipe`, `mc-drivers`, `mc-tessera` all locked.**
4. **Template YAML schema is the source of truth.** Adding a template = adding YAML. Zero Rust changes for new templates. This is the binding proof from Phase 6D — don't regress on it.
5. **Formula engine is the expression evaluator.** Don't build a parallel expression language. If a formula function is missing, add it to the formula engine (via proper Phase 3J.1 ADR), not to mc-narrative.
6. **Phase 6D demo still works.** `mc-demo-server` depends on `mc-narrative`; the demo server's behavior is unchanged after migration.
7. **Per-session commits (Rule 11).** 5 commits minimum.

---

## Acceptance Gates (lean)

- [ ] `cargo fmt --check --all` exits 0.
- [ ] `cargo clippy --all-targets --workspace -- -D warnings` exits 0.
- [ ] `cargo build --release --workspace` zero warnings.
- [ ] `cargo test --workspace` passes (926 + new mc-narrative tests).
- [ ] `mc model narrate` produces structured JSON output for the Scotts RV sample data.
- [ ] Phase 6D demo (`mc start`) still produces 17 narratives unchanged.
- [ ] Adding a new template to the YAML (without recompilation) produces new narrative output via `mc model narrate`.
- [ ] MC7001-MC7010 codes pre-flight swept FREE.
- [ ] Locked surfaces: zero diff on mc-core, mc-model, mc-fixtures, mc-recipe, mc-drivers, mc-tessera.
- [ ] Processing time < 200ms (formula-engine eval should be as fast or faster than the pre-computed approach).

---

## SPEC QUESTION candidates

- Session 2: Does `mc_model::parse_expression` need the cube's `Refs` at parse time, or only at eval time? If parse-time resolution is required, the narrative engine needs access to the compiled model's `ModelRefs` — which means templates are model-specific (not generic). If eval-time only, templates can be loaded once and evaluated against any cube.
- Session 2: How to evaluate `avg_over(CTR, Device)` in narrative context — the formula engine's `AvgOver` takes a DimensionId, which requires the cube's dimension registry. Does mc-narrative pass the cube's dimensions to the formula engine, or does it iterate manually?
- Session 3: Should `mc model narrate` require a `--templates` path, or auto-discover `./narratives/` relative to the model file? (Recommendation: auto-discover with explicit override.)

---

*End of handoff. Phase 7A.1 turns the demo into a real product
feature. The formula engine (912+ tests, Phase 3 complete) is the
evaluation substrate; mc-narrative is the orchestration layer that
connects templates to cubes to output. After 7A.1 ships, the
narrative engine is permanent infrastructure — not demo code.*
