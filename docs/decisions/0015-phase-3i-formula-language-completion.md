# ADR-0015: Phase 3I — Formula Language Completion

**Status:** Accepted
**Date:** 2026-05-06
**Deciders:** project owner
**Phase:** 3I (per [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md))

> Phase 3I closes the formula-language gaps surfaced by the post-6A audit. After 3I ships, the formula engine reaches full coverage for the marketing-finance + sports-betting + FP&A + demand-planning planning domains. This is the last formula-expansion phase before the roadmap pivots to data integration polish (Phase 5D) or UI (Phase 6B).

---

## Context

The post-6A audit (Sonnet × 3 lenses + Codex independent verification — see [`../audits/master-gap-report.md`](../audits/master-gap-report.md) and [`../audits/codex-phase-6a-followup.md`](../audits/codex-phase-6a-followup.md)) catalogued 48 candidate gaps across data-in, calculation, and data-out lenses. Of those, 12 mapped to the formula language. The audit grouped them into:

- **Clear-path additive** (no design needed): `is_element()`, math primitives (`pow`, `sqrt`, `ln`, `norm_inv`, etc.), `avg_over` family, `ifs`/`switch`, multi-key lookup tables, `predict()` arity validation, filter-formula parser unification, hyphen-tolerance in filter tokenizer.
- **Design-required** (needs ADR before scoping): `parameters:` blocks, `scenario_ref()` / `actual_ref(measure, fallback)`, `extrapolate_last_value()` / LOCF, general string-literal support beyond `is_element`, `Indicator` measure role, `output_bound` on fitted models, adstock + saturation transforms, advanced aggregation methods (Median, Variance, etc.), multi-frequency Time dimensions.

Real-world impact: ~170 lines of email-matchback Python (from `prepare_mmm_inputs.py` indicator generation, `prepare_v2_inputs.py` Q1 anchor pre-computation, multi-key seasonality lookups) come from gaps in the first bucket. The Tide MMM model needs the math primitives for NPV-style derivations. Sports-betting cartridges need `norm_inv` for Kelly criterion confidence intervals.

**The strategic decision:** ship the clear-path additive bucket as Phase 3I now; defer the design-required bucket to a future "Phase 3J" ADR. This is a deliberate scope-narrowing move (matches process-notes Rule 1's "When in doubt, default to ADR-first" principle: where decisions are not derivable from prior ADRs, defer rather than rush).

**Architectural importance.** Phase 3I establishes that **the formula language has a "completion line"** — a point where every common planning operation is expressible without Python pre-processing. After 3I, future formula additions are demand-driven (a real customer hits a gap → file an ADR → ship the addition), not speculative. This frees the project to invest in distribution (Phase 6C), UI (Phase 6B), and customer onboarding (Phase 7) rather than continued formula-engine expansion.

---

## Decisions

### Decision 1: Scope — 8 items in, 9 categories deferred

**In scope (binding — see [`../handoffs/phase-3i-formula-completion-handoff.md`](../handoffs/phase-3i-formula-completion-handoff.md) for implementation specs):**

1. `is_element(Dim, "Element")` returning numeric 0.0/1.0 (closes audit M-11).
2. 9 math primitives: `pow`, `sqrt`, `ln`, `log10`, `round`, `floor`, `ceil`, `mod`, `norm_inv` (closes audit M-15).
3. Multi-key `lookup_tables` via `key_dimensions: Vec<String>` (closes audit M-16).
4. `predict()` arity validation at load-time (closes audit M-17 narrowed).
5. `avg_over` / `min_over` / `max_over` / `wavg_over` family mirroring `sum_over` (closes audit M-18).
6. `ifs(c1, v1, c2, v2, ..., default)` and `switch(expr, m1, v1, m2, v2, ..., default)` desugaring to nested `If` (closes audit M-21).
7. Filter parser tokenizer accepts hyphens in identifier values (closes audit M-30; auto-closed by Decision 8).
8. Filter-formula parser unification — `query.rs::Filter::parse` becomes a wrapper around `mc_model::formula::parse_expression` (closes audit M-40 + research-notes §7I.8 commitment).

**Out of scope (deferred to Phase 3J or later — each requires its own ADR):**

| Deferred item | Reason | Future phase |
|---|---|---|
| `parameters:` block | Type system, override rules, lineage all undecided | Phase 3J ADR |
| `scenario_ref()` / `actual_ref(measure, fallback)` | 3 viable shapes; cross-coord dep-graph implications | Phase 3J ADR amendment |
| `extrapolate_last_value()` / LOCF | Past-gap vs future-gap semantics; needs `Scope` system extension (current scope is `AllLeaves` only — see audit S-1) | Phase 3J ADR |
| General string-literal support beyond `is_element()` | Kernel-adjacent (`ScalarValue::Str` propagation through `Cube::read`); design spike required | Phase 3J or 4+ ADR |
| `current_element(Dim) -> Str` (string-typed sibling of `is_element`) | Requires general string-literal support | Phase 3J ADR |
| `Indicator` measure role (declarative, not function) | Different shape than `is_element`; needs ADR for measure-role enum extension | Phase 3J ADR |
| `output_bound: {min: 0}` on fitted models (Amarillo -$5,706 case) | Phase 3H schema amendment, separate scoping | Phase 3H.1 amendment |
| Adstock + saturation transforms native to `fitted_models:` | Phase 3H.2 — biggest model-layer gap, kernel-adjacent | Phase 3H.2 |
| Aggregation methods beyond Sum/WeightedAvg/Min/Max (Median, Variance) | Requires mc-core consolidation change | New phase, ADR-required |
| Multi-frequency Time dimensions | High-cost change, relaxes MC2036 | New phase, ADR-required |

**Why this scope cut.** Each in-scope item has prior precedent (Phase 3E/3F/3G/3H established the "parser case + Expr variant + eval dispatch" pattern). Each deferred item has a real design question that wasn't pre-decided in prior ADRs.

### Decision 2: `is_element` is the narrow numeric form, NOT a string-literal entry point

**Binding choice (audit M-11).** `is_element(Dim, "Element")` returns numeric 0.0/1.0. The element name is parsed as a literal arg of this function and resolved at validate-time to an `ElementId`; the AST stores `Expr::IsElement(DimensionId, ElementId)`. **String literals in formula bodies are otherwise rejected** (parse error MC1024).

This avoids the kernel-adjacent change of propagating `ScalarValue::Str` through `Cube::read`, `Cube::write`, and the consolidation pipeline. The broader `current_element() -> Str` form is a Phase 3J question.

**Codex's audit explicitly recommended the narrow path.** Phase 3I implements that.

### Decision 3: 9 math primitives with explicit Null policies for edge cases

**Binding edge-case behavior (Null propagation, never error at eval-time):**

| Function | Edge case → Null |
|---|---|
| `pow(base, exp)` | `base < 0` and `exp` is non-integer |
| `sqrt(x)` | `x < 0` |
| `ln(x)` | `x ≤ 0` |
| `log10(x)` | `x ≤ 0` |
| `mod(a, b)` | `b == 0` |
| `norm_inv(p, mu, sigma)` | `p ≤ 0`, `p ≥ 1`, or `sigma ≤ 0` |
| `round` / `floor` / `ceil` | (no edge case) |

`norm_inv` uses the **Beasley-Springer-Moro algorithm** (~30 lines, accuracy ~1e-9) per process-notes Rule 5 (hand-rolled wins over deps). All 9 functions are pure-math; no I/O, no allocation beyond the result `ScalarValue`.

The AST shape is **one `Expr` variant per function** (e.g., `Expr::Pow(Box<Expr>, Box<Expr>)`), mirroring Phase 3H's `Exp(Box<Expr>)` and `NormCdf(Vec<Expr>)`. A unified `MathFunc(MathFuncKind, Vec<Expr>)` enum was rejected — see Alternatives.

### Decision 4: Multi-key `lookup_tables` are additive (backward-compatible) with explicit pipe separator

**Schema (binding):**

```yaml
lookup_tables:
  - name: "seasonality"
    key_dimensions: ["Market", "Time"]   # Vec<String>; new field
    values:
      "Houston|Jan_2026": 1.05
      "Houston|Feb_2026": 1.12
      ...
```

The existing single-key form (`key_dimension: String`) continues to work. Validator rules:

- Both `key_dimension` and `key_dimensions` set → MC2050 (mutually exclusive).
- Element name contains literal `|` → MC2051 (separator collision).
- Key arity in `values` doesn't match `key_dimensions.len()` → MC2052.

The `lookup()` formula function becomes variadic: `lookup(name, dim1)` for single-key (existing), `lookup(name, dim1, dim2)` for two-key, etc. Parse-time arity check against the table's declared key dimensions.

**No `schema_version` bump** — the new field is additive; existing parsers see it as an unknown-but-tolerated field (per the `#[serde(default)]` pattern already in `ParsedLookupTable`).

### Decision 5: `predict()` arity validation as MC2053 (validate-time)

**Bug context (audit M-17 narrowed by Codex).** Sonnet's report claimed `norm_cdf` with `sigma ≤ 0` produces NaN. Codex verified this is FALSE — runtime returns Null at `rule.rs:755-768`. The real (and smaller) gap is **load-time** validation: today's `validate.rs::check_fitted_model_blocks` does NOT cross-reference `predict()` call arity against the named model's coefficient count. Mismatch → silent runtime Null.

**Binding fix:** new validate-time check that walks all `predict(name, f1, f2, ...)` calls in rule bodies, looks up the named fitted model's coefficient count, errors with **MC2053** ("predict feature count does not match fitted model coefficient count") if mismatched.

**Code namespace:** MC2xxx (validate-time), NOT MC1xxx (parse-time). The check requires resolved fitted-model names, which only exist post-parse → validate. This supersedes the audit's earlier "MC1021 reserved for this" plan; MC1021 stays unused (per process-notes Rule 3, codes once shipped are forever, but unshipped reserved codes can be repurposed pre-acceptance — same window ADR-0006 used for MC2025).

### Decision 6: `avg_over` family — 4 new functions, Null-skipping semantics

**Binding (audit M-18):**

- `avg_over(measure, dim)` — arithmetic mean across leaf elements of `dim`; **skips Null cells** (Nulls don't count toward divisor).
- `min_over(measure, dim)` — min, skipping Nulls.
- `max_over(measure, dim)` — max, skipping Nulls.
- `wavg_over(measure, dim, weight_measure)` — weighted average; weights from `weight_measure` evaluated at each element. If all weights are zero or Null → return Null.

Each gets its own `Expr` variant (`AvgOver(DimensionId)`, etc.) and eval dispatch in `cube.rs::resolve_cross_coord_read`. Pattern mirrors Phase 3G's `SumOver`.

**Null-skipping semantics** matches Excel's `AVERAGE` and statistical convention, NOT TM1's "Null is zero" semantics. Mosaic's first-class Null handling extends naturally here.

### Decision 7: `ifs()` and `switch()` desugar to nested `If` at parse time

**Binding shape:**

- `ifs(c1, v1, c2, v2, ..., default)` — odd argument count (2N+1); default is mandatory.
- `switch(expr, m1, v1, m2, v2, ..., default)` — even argument count (2N+2); default is mandatory.

Both desugar at parse time to nested `Expr::If` chains. **No new `Expr` variants in mc-core.** The kernel doesn't know `ifs/switch` exist; it sees `If` chains.

**Mandatory default** — even-arg `ifs` or odd-arg `switch` is MC1004 (existing arity error). This forces explicit handling of the "no match" case, eliminating the silent-Null trap from Phase 3E's bare `if` chains.

`switch` works WITHOUT string literals: match values are numeric (e.g., `switch(period_index(), 0, 0.05, 1, 0.10, 0.02)`) or `is_element` calls. This stays inside Decision 2's narrow string-literal scope.

### Decision 8: Filter-formula parser unification — single source of truth

**Binding (audit M-40 + research-notes §7I.8).** `crates/mc-cli/src/query.rs::Filter::parse` is replaced by a wrapper around `mc_model::formula::parse_expression(&str) -> Result<Expr, ParseError>`. This requires **adding `parse_expression` as a public function in `mc-model`** — the only authorized public API addition to mc-model in Phase 3I.

**Filter restrictions (audit-confirmed):** the wrapper rejects ASTs containing cross-coord operators (`prev`, `lag`, `lead`, `cumsum`, `period_delta`, `actual_ref`, `predict`, `calibrate`, `lookup`, `bucket`, `period_index`, `is_past`, `is_current`, `is_future`, `sum_over`, `avg_over`, `min_over`, `max_over`, `wavg_over`). Filter ASTs are restricted to single-coord predicates: arithmetic, comparison, `if`/`ifs`/`switch`, `is_element`, math primitives, constants. Cross-coord ops in filters are a Phase 3J question.

**New code MC1025** ("cross-coord operator not allowed in filter expression") fires when a disallowed operator appears in a filter AST.

**Backward compat:** every existing `--where` invocation must continue to work. Tests verify ~15 representative filters from Phase 6A.2 + 6A.3 still match the same coords.

### Decision 9: New diagnostic codes — 8 total

**Reserved (binding per process-notes Rule 3 — codes are forever once shipped):**

| Code | Stage | Meaning |
|---|---|---|
| MC1022 | parse | `is_element` references unknown element |
| MC1023 | parse | `is_element` references unknown dimension |
| MC1024 | parse | string literal outside `is_element`'s second arg |
| MC1025 | parse | cross-coord operator in filter expression |
| MC2050 | validate | `lookup_table` has both `key_dimension` and `key_dimensions` set |
| MC2051 | validate | `lookup_table` element name contains separator (pipe) |
| MC2052 | validate | `lookup_table` key arity mismatch |
| MC2053 | validate | `predict()` feature count does not match fitted model coefficient count |

The previously-reserved-but-unshipped MC1021 stays unassigned (the predict-arity check is MC2053 per Decision 5).

### Decision 10: Handoff-first parallel flow

Per process-notes Rule 1, this ADR ships in **parallel with the implementation**, not before kickoff. The handoff at [`../handoffs/phase-3i-formula-completion-handoff.md`](../handoffs/phase-3i-formula-completion-handoff.md) is the binding implementation contract; this ADR is the audit-trail artifact.

Justification (Rule 1 self-test):
1. Kernel change? — **Yes, additive** (new Expr variants + eval dispatch). NOT a public API surface change in mc-core.
2. Runtime dep added? — No.
3. Contract surface change? — One: `mc_model::parse_expression` becomes public. This is intentional and bounded; the rest of mc-model's public API is unchanged.
4. Scope < ~1500 lines? — Borderline; 3I is the largest formula expansion to date but stays under that bound.
5. Strategic decisions derivable from prior ADRs? — Yes (3E/3F/3G/3H all establish the additive-formula-expansion pattern; the gap report and Codex audit pre-decide each item's binding shape).

Phase 3I is the second instance of handoff-first parallel flow (Phase 3D was the first). If the implementation surfaces a SPEC QUESTION that conflicts with this ADR, the project owner reconciles before merge.

---

## Out of scope

Beyond the 9 categories deferred in Decision 1, this phase explicitly does NOT:

- Add new public types or functions to `mc-core` beyond `Expr` enum extensions and eval dispatch (the kernel public API stays locked).
- Add new dependencies (norm_inv via Beasley-Springer-Moro hand-roll per process-notes Rule 5).
- Bump `schema_version` on any envelope (all changes are additive).
- Modify `mc-fixtures`, `mc-recipe`, `mc-drivers`, `mc-tessera`, or `mosaic-plugin/`.
- Change the toolchain (Rust 1.78 stays).
- Change `Cargo.lock` pins.
- Touch the consolidation engine (which would be required for Median / Variance aggregation methods — that's a separate phase).
- Touch the dependency graph (cross-coord operators in filters are explicitly rejected per Decision 8; `Indicator` measure role is deferred per Decision 1).

---

## Alternatives considered

### Alt 1: Broad `is_element` with general string-literal support

Considered for Decision 2. Would let users write `if(current_element(Channel) == "Email", ...)` directly. **Rejected** because it requires propagating `ScalarValue::Str` through `Cube::read`, `Cube::write`, consolidation, and writeback — a kernel-adjacent change with cascading implications (NaN-equivalence rules for strings, comparison semantics for mixed types, storage format changes). Codex's audit recommendation was to ship the narrow form first; the broad form becomes Phase 3J after a design spike.

### Alt 2: Single `MathFunc(MathFuncKind, Vec<Expr>)` enum for math primitives

Considered for Decision 3. A unified enum would reduce match-arm boilerplate in `eval_expr_unified`. **Rejected** because:
- Phase 3H shipped per-function variants (`Exp(Box<Expr>)`, `NormCdf(Vec<Expr>)`); switching style mid-arc creates inconsistency.
- Per-function variants are easier to grep and refactor (Phase 4 LLM consumers can pattern-match on specific variants).
- The `eval_expr_unified` match arm grows by 9 lines; not a real cost.

### Alt 3: `parameters:` block as a Phase 3I item

Was considered but rejected. The audit's M-14 entry has multiple viable shapes (read-only constants vs. tunable parameters vs. computed-derivations). Phase 3I would have to commit to one shape arbitrarily; deferring to Phase 3J ADR with proper alternatives analysis is the correct move.

### Alt 4: Filter parser stays separate; just fix hyphens

Considered for Decision 8. Would close audit M-30 cheaply without unifying parsers. **Rejected** because:
- The two-parser state is documented technical debt (research-notes §7I.8 explicitly commits to unification).
- Each future formula addition (3J's `parameters:` block; potential 4+ string-literal additions) would need duplicate filter-side implementation.
- Phase 6A.3's `sweep --metric-where` already reuses the filter parser; the unification benefits sweep too.

### Alt 5: Wait for Phase 3J ADR before shipping any of Phase 3I

The strict ADR-first interpretation. **Rejected** because the 8 in-scope items don't share design questions with the deferred items; bundling them together would delay ~50 regression tests of value for ~6 months while a `parameters:` block design spike completes. Splitting clean is the right move.

---

## Cross-links

- **Handoff (binding implementation contract):** [`../handoffs/phase-3i-formula-completion-handoff.md`](../handoffs/phase-3i-formula-completion-handoff.md)
- **Audit reports that surfaced the gaps:** [`../audits/master-gap-report.md`](../audits/master-gap-report.md), [`../audits/codex-phase-6a-followup.md`](../audits/codex-phase-6a-followup.md)
- **Research note with the Phase 3I commitment for filter unification:** [`../research-notes/formula-language-expansion.md`](../research-notes/formula-language-expansion.md) §7I.8
- **Prior formula expansion ADRs (the shape template):**
  - [`0011-phase-3e-conditionals-and-basic-operations.md`](0011-phase-3e-conditionals-and-basic-operations.md)
  - [`0012-phase-3f-time-series-operations.md`](0012-phase-3f-time-series-operations.md)
  - [`0013-phase-3g-reference-data-blocks.md`](0013-phase-3g-reference-data-blocks.md)
- **Cross-coord dependency-graph debt that affects Decision 8's filter restrictions:** [`../research-notes/cross-coord-dep-graph.md`](../research-notes/cross-coord-dep-graph.md)
- **Process rules:** [`../process-notes.md`](../process-notes.md) §1 (handoff-first parallel flow), §3 (diagnostic-code retirement), §5 (hand-rolled wins), §7 (backward compat), §11 (git workflow)

---

## Notes

**Phase 3 arc summary (post-Phase 3I):**

| Phase | What it added | Tag |
|---|---|---|
| 3A | YAML model definition + validator + 4-stage pipeline | `phase-3a-model-definition-layer` |
| 3B | Lint rules + diagnostic envelope | `phase-3b-lint-and-diagnostics` |
| 3C | `canonical_inputs` + `test_fixtures` schema | `phase-3c-fixtures-and-inputs` |
| 3D | Friendly formula syntax (string parser) | `phase-3d-friendly-formula-syntax` |
| 3E–3G | Conditionals + time-series + reference data | `phase-3e-3f-3g-formula-expansion` |
| 3H | Fitted-model evaluation (predict/calibrate/exp/norm_cdf) | `phase-3h-fitted-model-evaluation` |
| **3I** | **Math primitives + indicators + multi-key lookups + parser unification** | `phase-3i-formula-language-completion` (pending) |

**The "completion line."** After 3I ships, the formula engine is at full coverage for the four planning domains documented in the strategic positioning (marketing-finance, sports-betting, FP&A, demand-planning). Future formula additions are demand-driven — a real customer hits a gap → file an ADR → ship the addition. This is a deliberate stopping point.

**Roadmap implication.** With 3I complete, the next-phase decision (3J vs 5D vs 6B) becomes a customer-acquisition question, not a feature-completion question. The deferred items in Decision 1 stay queued; they get scoped when a real user surfaces the need.

**Acceptance amendment audit trail.** Per process-notes Rule 2, any project-owner amendments at acceptance land in a numbered table here. As of this ADR's authoring (handoff-first parallel flow), no amendments have been recorded — implementation is in progress on `phase-3i/formula-language-completion`. Any GPT/Desktop review feedback after handoff merge will be appended below as Acceptance Amendments §1+.

---

## Acceptance amendments

### Amendment §1 — MC2053 → MC2057 correction (2026-05-06)

**Source:** Phase 3I implementer self-audit (Section G — diagnostic-code namespace check).

**The error.** Decision 5 of this ADR (and the corresponding handoff at [`../handoffs/phase-3i-formula-completion-handoff.md`](../handoffs/phase-3i-formula-completion-handoff.md) item 4) specified **MC2053** for `predict()` arity validation. The implementer's audit verified against baseline `548eb6b` and discovered MC2053 was already shipped by Phase 3H for "duplicate fitted-artifact name" in `crates/mc-model/src/validate.rs::check_fitted_model_blocks` (5 occurrences at baseline). Per process-notes Rule 3 (CVE-style diagnostic-code retirement), shipped codes are forever locked; reusing MC2053 for a different rule would be a hard violation.

**The remediation (binding correction).** The `predict()` arity validation code is **MC2057**, not MC2053. Verified MC2057 was unassigned at baseline. Decision 5 of this ADR and Decision 9's code table are corrected accordingly.

**Updated code table (Decision 9):**

| Code | Stage | Meaning |
|---|---|---|
| MC1022 | parse | `is_element` references unknown element |
| MC1023 | parse | `is_element` references unknown dimension |
| MC1024 | parse | string literal outside `is_element`'s second arg |
| MC1025 | parse | cross-coord operator in filter expression |
| MC2050 | validate | `lookup_table` has both `key_dimension` and `key_dimensions` set |
| MC2051 | validate | `lookup_table` element name contains separator (pipe) |
| MC2052 | validate | `lookup_table` key arity mismatch |
| **MC2057** | **validate** | **`predict()` feature count does not match fitted model coefficient count** *(was MC2053 in original ADR; corrected)* |

MC2053 stays assigned to its Phase 3H meaning ("duplicate fitted-artifact name"). MC2054, MC2055, MC2056 are unassigned; the implementer chose the next free slot above MC2056 to make the audit-trail clear. Future codes start at MC2058.

**Process implication.** This is the first time a binding decision in a published ADR was caught and corrected by the implementer's self-audit. The audit pattern (process-notes Rule 10 + the per-phase audit prompt) worked exactly as designed — the implementer surfaced the collision, picked a safe alternative, and documented in their completion report rather than silently shipping the violation. **Future formula-expansion ADRs should sweep `git show <baseline>:crates/mc-model/src/validate.rs | grep -c "<proposed-code>"` for each new code in the proposal phase, not just the implementation phase.** PM (this ADR's author) takes responsibility for the original error; the audit pattern caught it before it shipped.
