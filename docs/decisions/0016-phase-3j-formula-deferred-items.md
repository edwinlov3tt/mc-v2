# ADR-0016: Phase 3J — Formula Authoring Deferred Items

**Status:** Accepted (with amendments §1–§6 from GPT review and §11–§12 from Claude Desktop review; see Acceptance amendments at end of file)
**Date:** 2026-05-06 (Proposed); accepted same day with amendments
**Deciders:** project owner (with reviews from GPT + Claude Desktop)
**Phase:** 3J (per [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md))

> Phase 3J closes the formula-authoring deferred queue from ADR-0015 Decision 1. After 3J ships, every formula-language item the post-6A audit surfaced is either implemented or has a separate ADR. This is the **definitive close-out of formula-engine deferrals**; future formula additions are demand-driven (real customer hits a gap → ADR → ship). Fitted-model deferred items (`output_bound`, adstock/saturation) ship separately as Phase 3H.1 — they're a different concern (model evaluation layer, not formula authoring).

---

## Context

ADR-0015 Decision 1 deferred 9 items to "Phase 3J or later" because each required design work that prior ADRs didn't pre-decide. The post-Phase-3I review confirmed the deferral list is the right scope cut: each item independently solves a real Python workaround in the email-matchback codebase.

Per project-owner Option B (2026-05-06): split the 9 items into two phases by concern:

- **Phase 3J — formula authoring (this ADR):** 7 items in clusters A/B/C/D (string-literal foundation, scope system extension, parameters block, Indicator measure role, scenario_ref, actual_ref fallback, extrapolate_last_value/LOCF).
- **Phase 3H.1 — fitted-model amendments (separate ADR-0017):** 2 items (`output_bound: {min: 0}`, native adstock/saturation transforms in `fitted_models:`). Independent of 3J; can ship in parallel or after.

After both ship, the deferred queue from ADR-0015 is empty.

**Why ADR-first (per process-notes Rule 1).** Phase 3J makes 4 kernel-adjacent changes (`ScalarValue::Str` first-class promotion through eval, `Scope` enum extension, `MeasureRole` enum extension, comparison operators on strings) and introduces 2 new top-level YAML schema blocks (`parameters:`) plus 1 new measure role. Rule 1's self-test fails on "kernel change?" and "contract surface change?"; ADR-first is mandatory. Per Rule 2 amendment-trail convention, GPT/Desktop review feedback after this ADR's authoring lands as numbered amendments.

**Architectural importance.** The string-literal foundation (Cluster A) is the load-bearing kernel change. Once shipped, future formula features that need string semantics ride on the same infrastructure. Done wrong, every later feature becomes a kernel touch; done right, future formula additions stay additive. Decisions 2-4 below carefully bound the string-literal semantics to keep `ScalarValue::Str` from leaking into storage, consolidation, or the dirty tracker — strings exist only in expression evaluation.

---

## Decisions

### Decision 1: Scope — 7 items in 4 clusters

**In scope (binding):**

| Cluster | Item | Closes audit | Effort |
|---|---|---|---|
| **A — String-literal foundation** | (1) `ScalarValue::Str` first-class in eval (was transient lookup-key only); (2) string equality/inequality operators (`==`, `!=`); (3) `current_element(Dim) -> Str` | M-11 (broad) | High |
| **B — Scope system extension** | (4) `Scope::FutureLeaves` / `PastLeaves` / `CurrentLeaves` variants (kernel `Scope` enum extension) | S-1, M-12 | Medium |
| **C — Schema additions** | (5) `parameters:` block (named scalar constants); (6) `Indicator` measure role | M-14, M-11 (decl) | Medium |
| **D — Formula function additions** (depends on A, B) | (7) `scenario_ref(measure, "Scenario")`; (8) `actual_ref(measure, fallback)` (extends existing 1-arg form); (9) `extrapolate_last_value(measure)` / LOCF (depends on Cluster B) | M-13, M-12 | Medium |

That's 9 sub-items mapping to 7 user-facing capabilities (Decisions 2-9 below treat them as 7 numbered items where the cluster groupings collapse).

**Out of scope (Phase 3H.1 — separate ADR-0017):**

| Item | Why deferred from 3J | Future phase |
|---|---|---|
| `output_bound: {min: 0, max: 1}` on fitted models | Fitted-model evaluation layer concern, not formula authoring; small additive schema field on `ParsedFittedModel` | Phase 3H.1 |
| Adstock + saturation transforms native to `fitted_models:` | Fitted-model evaluation layer; biggest model-layer gap but kernel-adjacent in a different way (touches `cube.rs::resolve_cross_coord_read` PredictModel arm) | Phase 3H.1 |

Both are real and shipped-in-3H.1, NOT permanently deferred. Splitting by concern keeps each phase's review focused on one set of design decisions.

**Out of scope (truly deferred to future phases — no current ADR planned):**

- General string ordering operators (`<`, `>`, `<=`, `>=` on strings) — locale-dependent, no current use case
- `parameters:` block with `body:` (computed parameters) — v1 is constants only; computed comes if demanded
- Custom `Scope` extensions beyond the 4 listed — ad-hoc rule scopes (e.g., "InputScope") need a separate design conversation
- Stochastic measure roles (`Random`, `Sampled`) — out of scope for deterministic kernel
- Multi-dimensional Indicators (`Indicator` over multiple dims simultaneously) — v1 is single dimension

### Decision 2: `ScalarValue::Str` is **transient**, never stored

**Binding choice (the load-bearing kernel decision):** `ScalarValue::Str` exists in expression evaluation but **never reaches storage, consolidation, writeback, or the dirty tracker**. Cells continue to hold only `F64(f64)` or `Null`.

This means:

- A formula CAN produce a `Str` intermediate value (e.g., `current_element(Channel)` returns `Str("Email")`).
- That `Str` value can ONLY be consumed by string equality (`==`, `!=`) which produces `F64(0.0)` or `F64(1.0)`.
- A rule whose body evaluates to `Str` at runtime is **MC2058** (rule body returned non-numeric).
- A `Cube::write` call with `ScalarValue::Str` fails at writeback validation with **MC2059**.
- The consolidation engine, dirty tracker, snapshot, NaN-check, and trace machinery never see `Str` values.

**Why this matters.** Promoting `Str` to a first-class storeable value would require:
- Type-aware consolidation (Sum of strings is undefined → would need new measure roles or per-cell type fields)
- Dirty propagation that doesn't choke on string values
- Writeback NaN-rejection that handles "can't NaN a string"
- Storage format that handles variable-width data (today's `HashMapStore` keys are `CellCoordinate`, values are `CellValue` with fixed `f64` payload)

That's a Phase 4+ scope. Phase 3J keeps strings transient — they live in expression evaluation only.

**Side effect.** A user CANNOT declare a measure that "stores a market name." For that, they'd use the dimension element directly — the cube already knows the market name via the coordinate; a measure storing it as a string is redundant.

### Decision 3: String comparison operators (`==`, `!=` only)

**Binding:**

- `Str == Str` → `F64(1.0)` if equal, `F64(0.0)` if not.
- `Str != Str` → inverse of `==`.
- `Str == F64` or `F64 == Str` → **MC1027** (type mismatch in comparison; parse-time if both literals, eval-time if one is a runtime value).
- `Str == Null` → `Null` (Null propagation, matches existing behavior for `F64 == Null`).
- `Str < Str`, `Str > Str`, `Str <= Str`, `Str >= Str` → **MC1028** (string ordering not supported in v1).

Existing `==` and `!=` between `F64` values stay unchanged (semantically `(a - b).abs() < 1e-9`).

**Why no string ordering.** String ordering depends on locale (case sensitivity, character classes, normalization). v1 ships with equality only; ordering can be added if a real use case surfaces.

### Decision 4: Where strings can appear (allowlist)

Strings are allowed:

1. As literal arguments to `is_element(Dim, "Element")` (Phase 3I — already shipped).
2. As literal arguments to `current_element(Dim)` returns (Phase 3J item 1c — value is the element name in `Dim` at the current coordinate).
3. As literal arguments to `scenario_ref(measure, "ScenarioName")` (Phase 3J item 7 — second arg is a string literal naming the scenario).
4. In string-equality comparisons inside formula expressions (`current_element(Channel) == "Email"`).
5. As string literals (e.g., `"Houston"`) in formula source code, parsed as `Expr::StrLiteral(String)`.

Strings are NOT allowed:

- As stored cell values (writeback rejects with MC2059).
- As rule body return values (validate rejects with MC2058).
- In arithmetic operations (`Str + Str`, `Str * F64`, etc.) → **MC1026**.
- In numeric comparisons (`<`, `>`, etc.) → MC1028.
- In any cross-coord operator's measure argument (e.g., `prev(Str_returning_thing)` is undefined) — validate rejects.
- As `parameters:` block values (Decision 6: parameters are F64 only in v1).

### Decision 5: `Scope` enum extension — 3 new variants

**Binding:** the kernel `Scope` enum (currently single-variant `AllLeaves`) extends to 4 variants:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Scope {
    AllLeaves,                // existing
    FutureLeaves,             // NEW: leaves at coords where is_future() is true (per time_anchor)
    PastLeaves,               // NEW: leaves where is_past() is true
    CurrentLeaves,            // NEW: leaves where is_current() is true
}
```

The compiler maps YAML `scope:` strings to enum variants:

```yaml
rules:
  - name: extend_adspend_to_future
    target: AdSpend
    scope: FutureLeaves         # NEW
    body: "extrapolate_last_value(AdSpend)"
```

Validator accepts the 4 names; unknown name → **MC1029** (invalid scope name; parse-time). Compile-time fallthrough on unknown scope → **MC2068** (defense-in-depth; should never fire if validator is correct).

**Why these 3 specifically.** They mirror the existing `is_past`/`is_current`/`is_future` runtime functions from Phase 3F.1. Using the same vocabulary in `scope:` makes the intent immediately clear.

**Side effect on rule evaluation.** A rule with `scope: FutureLeaves` only writes to coords where `is_future()` is true. The dependency graph's reverse edges still include all `AllLeaves`-scoped consumers, but the rule's compute pass skips coords outside its scope. If a coord is in scope and the rule errors, that's still an error; "out of scope" means "this rule doesn't apply here, leave the cell alone."

### Decision 6: `parameters:` block — constants only in v1

**Binding schema:**

```yaml
parameters:
  - name: q1_anchor_revenue
    value: 1234.56              # f64 literal; required
    description: "Q1 2026 revenue baseline"   # optional
```

**Reference syntax in formulas:** `param(name)`. Bare `name` is forbidden (collides with measure names + dim element names; ambiguous). Validator catches collision between parameter names and measure names (**MC2060**) or dim element names (**MC2061**). Reference to undeclared parameter → **MC2062**.

**Type:** v1 supports only `f64` values. No `int`, no `string`, no `bool`. If demanded, future amendments add typed parameters.

**Computed parameters deferred.** A `body:` field that evaluates a formula at compile time is explicitly NOT in 3J. Use a derived measure for computed values today; computed parameters can come in Phase 3J.1 amendment if real demand surfaces.

**Storage:** parameters are read-only at runtime. They live in `ParsedModel.parameters: Vec<ParsedParameter>` and become a `HashMap<String, f64>` in `CompiledCube`. The eval path for `param(name)` does a single map lookup. No dependency tracking (parameters can't reference cells; they're literals).

### Decision 7: `Indicator` measure role coexists with `is_element` function

**Binding:** add `MeasureRole::Indicator` as a new variant alongside `Input`, `Derived` (and the existing `Both` from the spec, if it ships in 3J). An Indicator measure declares `dimension:` and `element:` and has NO body, NO inputs.

```yaml
measures:
  - name: IsHouston
    role: Indicator
    dimension: Market           # required for Indicator
    element: Houston            # required for Indicator
    description: "1.0 at Houston coords, 0.0 elsewhere"
```

At eval time, an Indicator measure compiles to the equivalent of `is_element(Dim, "Element")` — same `Expr::IsElement` AST under the hood. Reading the cell at any coord returns 1.0 if the coord's element in `dimension` matches `element`, 0.0 otherwise.

**Validator rules:**
- Indicator with `body:` set → **MC2063**.
- Indicator missing `dimension` or `element` → **MC2064**.
- Indicator with `inputs:` declared → MC2063.

**Why coexist with `is_element`.** Different use cases:

- `is_element(Market, "Houston")` inline in a formula: one-off conditional logic.
- `Indicator` measure: declarative, reusable, references-by-name across multiple formulas (`Revenue * IsHouston` is cleaner than `Revenue * is_element(Market, "Houston")` repeated 5 times).

The Indicator role also lets users build "indicator vectors" (one Indicator per element of a dimension) for use as MMM regression features without hand-generating CSV rows.

### Decision 8: `scenario_ref` and `actual_ref` fallback — ship BOTH

**Binding:** ship two complementary primitives.

**`scenario_ref(measure, "ScenarioName")`** — read `measure` from the named scenario at the current coordinate. Returns the cell's value (or Null if the measure has no value at that coord in that scenario).

```yaml
body: "scenario_ref(Spend, 'Plan') * 1.1"     # Plan spend +10%
```

Validator: unknown scenario name → **MC2065**.

**`actual_ref(measure, fallback_expr)`** — gains an optional 2nd arg. If `actual_ref` would return Null (because no actuals exist at this coord), evaluate `fallback_expr` instead.

```yaml
body: "actual_ref(AdSpend, scenario_ref(AdSpend, 'Plan'))"   # use Actual if available, else Plan
```

Backward compat: existing `actual_ref(measure)` (1-arg) form continues to work; missing actuals still return Null. The 2-arg form is the new behavior.

Validator: 2nd arg type mismatch (e.g., returns Str when measure is F64) → **MC2066**.

**Why both.** `actual_ref(m, fallback)` is the narrow common case (matchback workflows). `scenario_ref(m, "X")` is the general primitive (lets users compose any cross-scenario read, not just Actual-with-fallback). Together they close audit M-13 cleanly.

### Decision 9: `extrapolate_last_value(measure)` ties to `Scope::FutureLeaves`

**Binding semantics:** `extrapolate_last_value(measure)` at a coord returns the most recent non-Null value of `measure` at the coord's prior time periods (relative to time_anchor). If no prior non-Null value exists, returns Null.

**Scope-gated by convention:** the function works at any scope, but **the validator emits MC2067 if the rule's scope is anything OTHER than `FutureLeaves`** with an explicit override (e.g., `extrapolate_anywhere: true` field on the rule). This prevents accidental past-gap filling (which is almost always a data bug, not a forecast operation).

```yaml
rules:
  - name: extend_adspend
    target: AdSpend
    scope: FutureLeaves                    # required for safe extrapolate
    body: "extrapolate_last_value(AdSpend)"
```

**Why scope-gate.** Past gaps in actuals data usually mean "the data is missing; surface the gap." Future gaps mean "we're forecasting; project the last known value." Same function, different intent — the scope makes it explicit.

**Why not "always future-gated automatically".** Some use cases legitimately fill past gaps (e.g., a model where data starts at year-2 and the year-1 baseline is "carry zero forward"). The override flag handles those without making the common case error-prone.

### Decision 10: Diagnostic codes — 15 reserved (all swept FREE against current main)

Pre-flight sweep verified 2026-05-06 against `main` HEAD `8d003f7`. All 15 codes are currently unassigned.

| Code | Stage | Meaning |
|---|---|---|
| MC1026 | parse | arithmetic operator on `Str` values |
| MC1027 | parse | type mismatch in comparison (`Str == F64`) |
| MC1028 | parse | string ordering operator (`<`, `>`, `<=`, `>=`) |
| MC1029 | parse | invalid `scope:` name |
| MC2058 | validate | rule body returns `Str` |
| MC2059 | writeback | writeback receives `Str` (engine invariant) |
| MC2060 | validate | parameter name collides with measure name |
| MC2061 | validate | parameter name collides with dim element name |
| MC2062 | validate | reference to undeclared parameter |
| MC2063 | validate | Indicator measure declared with `body:` or `inputs:` |
| MC2064 | validate | Indicator measure missing `dimension:` or `element:` |
| MC2065 | validate | `scenario_ref` references unknown scenario |
| MC2066 | validate | `actual_ref` fallback expression type mismatch |
| MC2067 | validate | `extrapolate_last_value` used at scope other than `FutureLeaves` without override |
| MC2068 | compile | scope name unknown at compile time (defense-in-depth) |

MC1025 was the highest shipped MC1xxx (Phase 3I item 8); MC2057 was the highest shipped MC2xxx (Phase 3I item 4 after Amendment §1 correction).

### Decision 11: Implementation order (binding for the handoff)

The 7 items have dependencies; ship in this order:

1. **Cluster A items 1+2+3 first** — `ScalarValue::Str` promotion, comparison operators, `current_element(Dim)`. This is the kernel foundation; everything downstream depends on it.
2. **Cluster B (Scope extension)** — kernel `Scope` enum + validator. Needed before item 9 (extrapolate).
3. **Cluster C item 5** — `parameters:` block. Schema-only; no kernel touch.
4. **Cluster C item 6** — `Indicator` measure role. Schema + `MeasureRole` enum extension; lightweight.
5. **Cluster D items 7+8** — `scenario_ref` + `actual_ref(measure, fallback)`. Pure formula additions on top of the kernel foundation.
6. **Cluster D item 9** — `extrapolate_last_value` + LOCF. Depends on Cluster B's scope extension.

Implementer's commits go per-cluster (Cluster A as one or several commits, Cluster B as one, etc.) — NOT all-uncommitted-at-end (per process-notes Rule 11 anti-pattern from Phase 3I).

### Decision 12: ADR-first, sequential to handoff

Per process-notes Rule 1 self-test: ADR-first is mandatory (kernel changes + multiple contract surface changes). The handoff is drafted AFTER this ADR is reviewed and accepted by the project owner (vs. the handoff-first parallel flow used for Phase 3D and 3I). Rationale: the kernel-adjacent items (Decisions 2, 5, 7) have substantive design questions that benefit from explicit GPT/Desktop review before the implementer is committed.

If the project owner reviews this ADR and accepts as-is, the handoff drafts immediately. If amendments land, they're recorded in §"Acceptance amendments" before the handoff drafts.

---

## Out of scope

Explicitly NOT in Phase 3J (deferred to ADR-0017 / Phase 3H.1, Phase 3J.1, or future phases):

- `output_bound` on fitted models (Phase 3H.1 / ADR-0017)
- Adstock + saturation transforms native to `fitted_models:` (Phase 3H.1 / ADR-0017)
- Computed parameters (`parameters:` with `body:` field) — v1 is constants only
- String ordering operators (`<`, `>`, `<=`, `>=`)
- Custom Scope variants beyond the 4 (`InputScope`, `RandomLeaves`, etc.)
- Multi-dimensional Indicators (`Indicator` over Market AND Channel simultaneously)
- Stochastic measure roles (`Random`, `Sampled`)
- `parameters:` of types other than `f64`
- Cross-cube `scenario_ref` (referencing a measure in a different cube — out of scope for v1)
- Storage of `Str` values in cells (Phase 4+ — kernel storage layer change required)

---

## Alternatives considered

### Alt 1: Promote `ScalarValue::Str` to first-class storeable

Considered for Decision 2. Would let users declare measures that "store a market name string." **Rejected** because:
- Storage format change cascades through `HashMapStore`, consolidation, dirty tracker, NaN-check, snapshot.
- No real use case in the email-matchback audit. The cube already knows element names via coordinates; storing them as cell values is redundant.
- Phase 4+ scope; not Phase 3 scope.

### Alt 2: One ADR for both 3J and 3H.1

Considered (Option A from the project-owner choice). Would unify all 9 deferred items in one phase. **Rejected** because:
- Different concerns (formula authoring vs fitted-model evaluation layer) should be reviewed separately.
- Phase 3J is already the largest formula-expansion phase to date; bundling 3H.1 makes it 30% bigger.
- 3H.1 can ship in parallel with or after 3J — no shared design surface.

### Alt 3: Scope system extension via `scope:` filter expression

Considered for Decision 5. Would let users write `scope: "is_future()"` as a filter expression instead of named enum variants. **Rejected** because:
- Existing rules use named scope (`AllLeaves`); switching to expressions is a contract change for shipped rules.
- Filter expressions on rule scope re-introduce the cross-coord-in-filter problem from Phase 3I item 8.
- Named variants are simpler to validate, cache, and reason about. Reasonable extension path: if real demand surfaces for ad-hoc scope filters, add `scope_where: "<expr>"` as an alternative field; named variants stay the primary path.

### Alt 4: `Indicator` role replaces `is_element` function

Considered for Decision 7. Would simplify the formula language by having one mechanism instead of two. **Rejected** because:
- `is_element` is already shipped (Phase 3I) and removing it is a breaking change.
- Different use cases: `is_element` for one-off inline use; `Indicator` for declarative reusable measures. Coexistence is correct.

### Alt 5: Only `actual_ref(measure, fallback)`, no `scenario_ref`

Considered for Decision 8. Would close audit M-13 with the narrow case (matchback workflows). **Rejected** because:
- `scenario_ref` is the more general primitive; `actual_ref(m, fallback)` is sugar for `actual_ref(m) ?? scenario_ref(m, "Actual"_or_specific_other)`.
- Both shipped together: narrow case stays ergonomic, general case is available for cross-scenario analysis (variance reports, scenario comparison rules).

### Alt 6: `extrapolate_last_value` always future-gated automatically

Considered for Decision 9. Would make the function safe-by-default. **Rejected** because:
- Some legitimate use cases fill past gaps (year-1 baseline carry).
- An explicit override flag (with MC2067 if missing) makes intent visible without locking out the use case.

### Alt 7: `parameters:` block with computed `body:` in v1

Considered for Decision 6. Would close more email-matchback Python (the Q1-anchor pre-computation). **Rejected** for v1 because:
- Computed parameters interact with the dependency graph in non-trivial ways (when does the body evaluate? what if it references measures with their own dependencies?).
- v1 constants are sufficient for the email-matchback case if the user computes the constant in Python ONCE and writes it to YAML (vs. computing it every formula evaluation).
- Computed parameters can ship as Phase 3J.1 amendment if real demand.

---

## Cross-links

- **Audit reports that surfaced the gaps:** [`../audits/master-gap-report.md`](../audits/master-gap-report.md), [`../audits/codex-phase-6a-followup.md`](../audits/codex-phase-6a-followup.md)
- **Prior formula expansion ADR (the closest precedent):** [`0015-phase-3i-formula-language-completion.md`](0015-phase-3i-formula-language-completion.md)
- **Cross-coord dependency-graph debt that affects Decisions 7/8/9:** [`../research-notes/cross-coord-dep-graph.md`](../research-notes/cross-coord-dep-graph.md)
- **Process rules:** [`../process-notes.md`](../process-notes.md) §1 (handoff-first vs ADR-first), §3 (diagnostic-code retirement + pre-flight sweep), §10 (completion report self-audit), §11 (git workflow + per-item commits)
- **Companion ADR for fitted-model amendments:** ADR-0017 (Phase 3H.1, to be drafted after 3J ships)
- **Handoff (binding implementation contract):** [`../handoffs/phase-3j-formula-deferred-handoff.md`](../handoffs/phase-3j-formula-deferred-handoff.md) (to be drafted after this ADR is accepted)

---

## Notes

**Phase 3 arc summary (post-Phase 3J):**

| Phase | What it added | Tag |
|---|---|---|
| 3A | YAML model definition + validator + 4-stage pipeline | `phase-3a-model-definition-layer` |
| 3B | Lint rules + diagnostic envelope | `phase-3b-lint-and-diagnostics` |
| 3C | `canonical_inputs` + `test_fixtures` schema | `phase-3c-fixtures-and-inputs` |
| 3D | Friendly formula syntax (string parser) | `phase-3d-friendly-formula-syntax` |
| 3E–3G | Conditionals + time-series + reference data | `phase-3e-3f-3g-formula-expansion` |
| 3H | Fitted-model evaluation (predict/calibrate/exp/norm_cdf) | `phase-3h-fitted-model-evaluation` |
| 3I | Formula language completion (math primitives + indicators + multi-key + parser unification) | `phase-3i-formula-language-completion` |
| **3J** | **Formula authoring deferred items (string literals + Scope extension + parameters + Indicator role + scenario_ref + actual_ref fallback + LOCF)** | `phase-3j-formula-deferred-items` (pending) |
| **3H.1** (parallel) | **Fitted-model amendments (output_bound + adstock/saturation)** | `phase-3h-1-fitted-model-amendments` (pending; ADR-0017) |

**The "deferred queue is empty" milestone.** After 3J + 3H.1 both ship, the post-6A audit's deferred-formula-items queue is fully closed. ADR-0015's deferred list goes to zero. Future formula additions are demand-driven (real customer hits a gap → ADR → ship); no speculative formula work without that signal.

**Phase 3 totals (projected post-3J):**
- 9 sub-phases (3A through 3J)
- ~60 new diagnostic codes shipped (MC1003-MC1029, MC2011-MC2068)
- ~250+ regression tests added across the formula engine
- Zero kernel-public-API breakage since Phase 1A
- 6 ADRs (0004, 0005, 0006, 0007, 0011-0013, 0014, 0015, 0016)

**Process improvement applied from Phase 3I.** Per Rule 3 amendment (added 2026-05-06 after the ADR-0015 MC2053 collision), this ADR's Decision 10 includes the pre-flight sweep result. All 15 proposed codes verified FREE against `main` HEAD `8d003f7`.

---

## Acceptance amendments

Per process-notes Rule 2: GPT-sourced amendments numbered §1–N; Claude Desktop–sourced amendments numbered §11+. Both reviews returned 2026-05-06 with **"accept with amendments, skip another review cycle"** consensus. PM applied all 8 amendments before drafting the handoff.

### Amendment §1 (GPT) — Tighten Decision 2/4 Str enforcement

The original Decisions 2/4 said strings cannot appear in arithmetic operations or numeric comparisons but did NOT explicitly cover other consumption sites. Tightening:

**Str values are valid ONLY as intermediate expression values consumed by `==` or `!=`.** A `Str` used in any of the following sites is a parse-time or validate-time error:

- As an `if(cond, then, else)` condition argument → MC1027 extended (type mismatch)
- As an `and(...)` / `or(...)` / `not(...)` operand → MC1027 extended
- As an arithmetic operand (`Str + Str`, `Str * F64`, etc.) → MC1026 (existing)
- As a numeric comparison operand (`<`, `>`, `<=`, `>=`) → MC1028 (existing)
- As a `parameters:` block value → MC2060 extended (parameters are F64-only per Decision 6)
- As a rule body return value (the rule's outermost expression evaluates to `Str`) → MC2058 (existing)
- As a `Cube::write` value (writeback) → MC2059 (existing)

**Required regression tests added to handoff scope:**
- `current_element(Market) == "Houston"` → works (returns 1.0 / 0.0)
- `if(current_element(Market), 1, 0)` → fails parse-time (MC1027)
- `and(current_element(Market), SomeFlag)` → fails parse-time (MC1027)
- `not(current_element(Market))` → fails parse-time (MC1027)
- A rule body that resolves to `Str` at runtime → fails validate-time (MC2058)

The handoff binds these tests as required for Cluster A acceptance.

### Amendment §2 (GPT) — Decision 6 parameters: only PARTIALLY closes M-14

Original Decision 6 implied that `parameters:` v1 closes the M-14 audit gap. **It does not.** M-14's full scope is partial-coordinate parameters (e.g., per-Scenario or per-Market constants that vary across some dimensions but not Time). v1 ships **global f64 constants only**.

The Q1-anchor broadcast workaround in `prepare_v2_inputs.py` is **not** fully closed by 3J — global constants help where the anchor is truly time-invariant across all coords, but a per-Market or per-Scenario anchor still requires Python pre-computation or a derived measure.

**Wording correction in Decision 1's scope table:**

> ~~`parameters:` block closes M-14~~ → `parameters:` block partially closes M-14 (global scalar constants only); scoped parameters and computed parameters remain deferred to Phase 3J.1 unless real demand surfaces during 3J implementation.

**Computed parameters and scoped parameters are explicitly Phase 3J.1 scope.** The 3J completion report should re-survey email-matchback's residual Python after 3J ships to confirm what fraction of M-14 is closed vs deferred.

### Amendment §3 (GPT) — Decision 8 actual_ref fallback nesting clarification

The original Decision 8 ships `actual_ref(measure, fallback_expr)` but didn't address the existing **MC1013 cross-coordinate nesting prohibition** from ADR-0011/0012 (which forbids patterns like `prev(actual_ref(Revenue))` and `actual_ref(prev(Revenue))`).

`actual_ref(AdSpend, scenario_ref(AdSpend, "Plan"))` would technically violate that rule unless explicitly relaxed.

**Binding relaxation (specific to actual_ref's fallback path):**

- The `fallback_expr` argument of `actual_ref(measure, fallback_expr)` is **lazy** — evaluated only if the actual_ref lookup returns Null.
- Cross-coordinate functions inside `fallback_expr` are **allowed** if independently valid: `scenario_ref(...)`, `lag(...)`, `prev(...)`, `lookup(...)` all permitted as fallback expressions.
- All OTHER cross-coordinate nesting patterns (e.g., `prev(actual_ref(...))`, `lag(scenario_ref(...))`) **remain rejected by MC1013** unless separately approved by future ADR.

**Required regression test added to handoff scope:**
- `actual_ref(AdSpend, scenario_ref(AdSpend, "Plan"))` evaluates correctly with lazy fallback (Plan only consulted when Actual is Null).

### Amendment §4 (GPT) — Decision 5 scope variants require time_anchor

`FutureLeaves` / `PastLeaves` / `CurrentLeaves` semantically depend on `is_future()` / `is_past()` / `is_current()`, which require a configured `time_anchor` (Phase 3F.1).

**Binding:** if a model declares any rule with `scope: FutureLeaves | PastLeaves | CurrentLeaves` but has no `time_anchor` configured on the Time dimension, validate fails with **MC2069** (extension to Decision 10's reserved range; sweep verified FREE) — "scope variant requires time_anchor".

`Scope::AllLeaves` continues to work without `time_anchor` (existing behavior, backward compat).

**Required regression tests added to handoff scope:**
- `scope: FutureLeaves` with `time_anchor` configured → works.
- `scope: FutureLeaves` without `time_anchor` → fails validate with MC2069.
- `scope: AllLeaves` works regardless of `time_anchor` presence (no regression).

### Amendment §5 (GPT) — Decision 9 reserve max_periods argument shape

`extrapolate_last_value(measure)` v1 scans backward until the first non-Null value, with no upper bound on staleness. A 2-month forecast gap and a 24-month stale-data gap behave identically. That's risky in planning.

**Reserve the future shape but DON'T block v1 on it:**

- v1 ships: `extrapolate_last_value(measure)` (1 arg, unbounded backward scan).
- Reserved future shape: `extrapolate_last_value(measure, max_periods)` (optional 2nd arg bounding lookback). If the scan exceeds `max_periods` without finding a non-Null, returns Null.
- The 2-arg form is **NOT** implemented in v1 but documented in handoff as the reserved future shape so users don't expect it.

**Required regression test added to handoff scope:**
- `extrapolate_last_value` at a coord with no prior non-Null returns Null (current v1 behavior; future test once max_periods ships will refine this).

### Amendment §6 (GPT) — Indicator role compiles to same Expr AST as is_element

Decision 7 says the Indicator measure role and the `is_element()` function "compile to the equivalent" Expr AST. Make this **binding rather than aspirational**:

**Indicator measures MUST compile to the same `Expr::IsElement(DimensionId, ElementId)` variant that `is_element(Dim, "Element")` produces.** No second evaluation path. The `MeasureRole::Indicator` variant exists at the schema layer (so the YAML is declarative and reusable), but at the kernel level there is exactly ONE evaluation rule for this semantic.

**Required regression test added to handoff scope:**
- A model with `Indicator` measure `IsHouston` and a separate rule using `is_element(Market, "Houston")` produce byte-identical `Expr::IsElement(...)` AST nodes (snapshot test).
- A model swapping between the two forms produces identical query results across all coords (golden test).

### Amendment §11 (Claude Desktop) — Decision 9 rename override flag

The original Decision 9 proposed `extrapolate_anywhere: true` as the override flag for using `extrapolate_last_value` outside `Scope::FutureLeaves`. Generic flag names invite misuse.

**Renamed (binding):** `allow_past_extrapolation: true`. Specific, self-documenting; the user reading the YAML knows immediately what they're unlocking.

**Updated MC2067 wording:** "extrapolate_last_value used at scope other than FutureLeaves without `allow_past_extrapolation: true`".

### Amendment §12 (Claude Desktop) — Decision 8 cross-coord dep-graph performance note

`scenario_ref` and `actual_ref(measure, fallback)` are cross-coordinate reads. They share the existing cross-coordinate dependency-graph debt (every write currently invalidates all derived cells; correctness preserved via revision-bumping; performance is the issue — see `docs/research-notes/cross-coord-dep-graph.md`).

**Binding addition to Decision 8 (documented at the decision level, not just cross-linked):**

> **Performance note (inherited debt):** `scenario_ref` and `actual_ref(measure, fallback)` inherit the existing cross-coordinate dep-graph behavior. Every write to a cube containing a rule that uses these functions invalidates all derived cells (over-invalidation; correctness preserved). This is a known performance issue, not a correctness issue. Phase 3J does NOT fix this — the fix requires the broader cross-coord dep-graph work documented in `docs/research-notes/cross-coord-dep-graph.md` and is scoped for a future phase ADR. Cartridges using these functions extensively at high cube cardinality may experience slow writes; document expectations in cartridge READMEs.

This documents the debt as KNOWN and INHERITED (not silently re-introduced or forgotten). The fix lands when the dep-graph debt phase is scoped, not before.

### Diagnostic-code reservation summary (post-amendments)

The ADR's Decision 10 reserved 15 codes (MC1026-1029, MC2058-2068). Amendment §4 adds **MC2069** (scope variant requires time_anchor), bringing the total to **16 reserved codes**. Pre-flight sweep confirmed FREE against `main` HEAD `8d003f7`.

| Code | Stage | Meaning | Source |
|---|---|---|---|
| MC1026 | parse | arithmetic operator on `Str` values | Decision 4 |
| MC1027 | parse | type mismatch in comparison or condition (`Str == F64`, `if(Str, ...)`, etc.) | Decisions 3, 4; **Amendment §1** |
| MC1028 | parse | string ordering operator | Decision 3 |
| MC1029 | parse | invalid `scope:` name | Decision 5 |
| MC2058 | validate | rule body returns `Str` | Decision 4 |
| MC2059 | writeback | writeback receives `Str` | Decision 4 |
| MC2060 | validate | parameter name collides with measure name (or parameter value is non-F64) | Decision 6; Amendment §1 (Str-as-param-value) |
| MC2061 | validate | parameter name collides with dim element name | Decision 6 |
| MC2062 | validate | reference to undeclared parameter | Decision 6 |
| MC2063 | validate | Indicator measure declared with `body:` or `inputs:` | Decision 7 |
| MC2064 | validate | Indicator measure missing `dimension:` or `element:` | Decision 7 |
| MC2065 | validate | `scenario_ref` references unknown scenario | Decision 8 |
| MC2066 | validate | `actual_ref` fallback expression type mismatch | Decision 8 |
| MC2067 | validate | `extrapolate_last_value` used at scope other than `FutureLeaves` without `allow_past_extrapolation: true` | Decision 9 + **Amendment §11** |
| MC2068 | compile | scope name unknown at compile time (defense-in-depth) | Decision 5 |
| **MC2069** | **validate** | **scope variant requires `time_anchor` configured on Time dim** | **Amendment §4** |
