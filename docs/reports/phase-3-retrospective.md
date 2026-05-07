# Phase 3 — Retrospective

> **What the formula engine became and why.** This document closes the Phase 3 arc — 11 sub-phases spanning 3A (model definition layer) through 3H.2 (adstock + saturation transforms). It's not a per-phase completion report; those live in [`./`](./) one per phase. This is the higher-level "what was built, in what order, with what trade-offs, and what's now possible" piece. Useful as a strategic record + portfolio artifact.

**Status:** Final.
**Date:** 2026-05-06 (the day Phase 3 closed).
**HEAD at close:** `fa1f634` on `main`. Tag `phase-3h-2-fitted-model-adstock-saturation` at `d240802` is the last formula-engine work shipped.

---

## The thesis

When Phase 3 started in early May 2026, Mosaic was a Rust kernel with a single demo cube authored in Rust. **A planning model author had to write Rust to author a model.** That doesn't scale to UI, doesn't scale to LLM-assisted authoring, doesn't scale to non-engineer planners. Phase 3's job was to build a complete declarative authoring layer that:

1. Reads as a YAML file authored by a human or an LLM.
2. Compiles to the existing Rust kernel without breaking its semantics.
3. Expresses every formula a real planning model needs (marketing-mix, FP&A, sales-forecasting, demand-planning, sports-betting).
4. Validates strictly enough that errors fire at load time, not in production.

Phase 3 shipped that. **By the end of 3H.2, the formula engine covers ~98% of real planning-model formula requirements without dropping to Python.** The remaining 2% is demand-driven (real customer hits a gap → ADR → ship). The phase wasn't framed that way at the start — there was no specification of "98% coverage." It emerged from rigorous demand-driven scoping (audit-driven gap analysis, prior-art research, real-world cartridge stress tests).

The headline measure: **the email-matchback project's "calculator" Python went from ~1,260 lines pre-Phase-6A to ~440 lines after Phase 3 closes** — a 65% reduction in formula-engine-domain Python. The remaining ~440 lines are correctly Python (sklearn fitting, Tessera-driver-gap workarounds, reporting templates).

---

## The arc

Eleven sub-phases. Two narrative chunks:

### Foundation (3A → 3D) — the YAML authoring layer

This was about getting from "no YAML at all" to "Acme cube authored in YAML, byte-identical to the Rust path."

- **3A** — added `mc-model` crate with the four-stage pipeline (parse → validate → resolve_inputs → compile). New top-level YAML schema (`dimensions:`, `measures:`, `rules:`, etc.). Acme cube's first YAML version. **Acceptance:** `diff <(mc demo) <(mc demo --model crates/mc-model/examples/acme.yaml)` produces empty output. *No kernel changes; this was a pure translation layer.*
- **3B** — added `mc model {validate, inspect, lint, test}` + 10 lint rules + structured `Diagnostic { code, severity, path, message, suggestion }` envelope with `schema_version: "1.0"`. **Acceptance:** Acme YAML lints with zero documented warnings.
- **3C** — added `canonical_inputs:` + `test_fixtures:` schema; sibling CSV + tabular inline YAML data forms; 14 new validators (MC2012-MC2025); `mc model test --fixture <name>` filter flag. Removed the embarrassing Acme-name special case from the CLI.
- **3D** — friendly formula syntax. Rule bodies became `body: "Customers * AOV"` strings instead of nested objects. Hand-rolled recursive-descent parser. Acme migrated. The kernel is still receiving the same 7 `ParsedRuleBody` AST variants; only the *authoring shape* changed.

**Phase 3D was the proof-of-concept for handoff-first parallel flow** (per process-notes Rule 1). Small additive layer with no contract changes; ADR landed alongside implementation. This pattern subsequently shipped 5 more times (3I, 3J, 3H.1, 3H.2 + ADR-0018 Amendment §11).

### Expansion (3E → 3H.2) — growing the formula language

This was about taking the AST from 7 nodes (3D) to ~30+ nodes (3H.2) — covering every formula a real planning model needs.

- **3E + 3F + 3F.1 + 3G** (bundled tag) — conditionals (`if`, `ifs`, `switch`), time-series ops (`prev`, `lag`, `lead`, `cumsum`, `period_delta`), runtime time anchor (`is_past`, `is_current`, `is_future`), reference-data blocks (`benchmarks:`, `lookup_tables:`, `status_thresholds:`). Big leap from "arithmetic-only" to "real planning vocabulary."
- **3H** — fitted-model evaluation. `predict()`, `calibrate()`, `exp()`, `norm_cdf()`. Cube cells could now reference fitted statistical models (Lasso regression coefficients, calibration curves) declared in the YAML. The "calculator + judge + investigator" pattern: Python fits, Mosaic evaluates.
- **3I** — formula language completion. `is_element`, 9 math primitives (Beasley-Springer-Moro `norm_inv`), multi-key `lookup_tables`, `predict()` arity validation, `avg/min/max/wavg_over` family, `ifs/switch`, filter-formula parser unification (closed the two-parser state). Audit-driven scope from the Sonnet × 3-lens + Codex independent review.
- **3J** — formula authoring deferred items. `ScalarValue::Str` first-class in eval (transient-only — never stored), `current_element(Dim)`, `parameters:` block (constants only v1), `Indicator` measure role, `Scope` enum extension (`FutureLeaves` / `PastLeaves` / `CurrentLeaves` requiring `time_anchor`), `scenario_ref(measure, "Scenario")` + `actual_ref(measure, fallback)`, `extrapolate_last_value()` + LOCF.
- **3H.1** — fitted-model `output_bound` (small polish; closed the Amarillo -$5,706 case).
- **3H.2** — fitted-model adstock + saturation transforms (geometric adstock + Hill / Log saturation natively in `fitted_models.transforms:`). The closing phase.

**Each expansion phase added a parser case + Expr variant + eval dispatch.** The pattern is so consistent that Phase 3I and 3J both fit the same template at scale. This is what "established pattern" means — once the shape is right, scaling adds capability without adding architectural risk.

---

## What's now possible (the capability checklist)

A planning model author can now express, in YAML alone:

- ✅ **Branching logic** (`if`, `ifs`, `switch`)
- ✅ **Time-series comparisons** (`prev`, `lag`, year-over-year, cumulative sums)
- ✅ **Time-anchor-aware queries** (`is_past`, `is_current`, `is_future`)
- ✅ **Lookup tables** (single-key and multi-key)
- ✅ **Industry benchmarks with source attribution** (`benchmarks:`)
- ✅ **Status thresholds with health bands** (`status_thresholds:`)
- ✅ **Fitted statistical models** (`predict`, `calibrate`)
- ✅ **Math primitives** (`pow`, `sqrt`, `ln`, `norm_inv`, `norm_cdf`, ...)
- ✅ **Cross-coordinate aggregations** (`sum_over`, `avg_over`, `wavg_over`, ...)
- ✅ **Inline indicators** (`is_element`) and **declarative ones** (`Indicator` role)
- ✅ **Named constants** (`parameters:`)
- ✅ **Scope-restricted rules** (`FutureLeaves`, `PastLeaves`, `CurrentLeaves`)
- ✅ **Cross-scenario reads** (`scenario_ref`, `actual_ref(m, fallback)`)
- ✅ **Last-observation-carried-forward** (`extrapolate_last_value`)
- ✅ **Output clamping on fitted models** (`output_bound`)
- ✅ **Native MMM transforms** (geometric adstock + Hill / Log saturation in `fitted_models.transforms:`)

Total: ~30+ formula functions across ~75 diagnostic codes. The complete authoring surface is documented in [`../specs/engine-semantics.md`](../specs/engine-semantics.md) (locked) and the YAML schema in [`../decisions/`](../decisions/) (six ADRs across the arc).

What's deliberately NOT in scope:

- ❌ **Stochastic / random sampling** — out of scope for a deterministic kernel (architectural commitment)
- ❌ **Storing string values in cells** — Phase 4+ kernel storage decision (the Phase 3J Decision 2 boundary)
- ❌ **Computed `parameters:`** — v1 is constants only; demand-driven Phase 3J.1 if asked
- ❌ **Scoped `parameters:`** (per-Scenario, per-Market) — same
- ❌ **`Indicator` over multiple dimensions** — same
- ❌ **Weibull adstock / Root / Exp saturation** — Hill + Log + geometric cover ~95% of MMM use; same demand-driven escape clause
- ❌ **Cross-cube formula refs** — Phase 5+ kernel work per engine-semantics `I-Dep-6`

The ❌ list isn't "we couldn't ship this" — it's "no real customer asked for this yet." When one does, it gets an ADR + amendment + phase. Demand-driven discipline is the closing condition for Phase 3.

---

## The architectural commitments that made this possible

Six commitments, each made early, each held throughout:

### 1. The kernel surface stays locked

`mc-core`'s public API has been locked since Phase 2D. Phase 3 added zero new public functions in mc-core — every formula-language addition went into `Expr` enum variants + eval dispatch + new `pub` types as fields on existing public structs (`FittedModelData`, `ParsedFittedModel`). Hard Rule 7 in every Phase 3 handoff enforced this.

The single cumulative diff against the Phase 1A kernel API: ~30+ `Expr` enum variants, 4 new pub field types (`OutputBound`, `Transforms`, `AdstockSpec`, `SaturationSpec`), 3 new `Scope` variants, 1 new `MeasureRole` variant. Zero new public functions. Zero contract surface changes.

This is what made Phase 3 shippable in 11 sub-phases — every phase was a confined extension, not a rewrite.

### 2. The diagnostic-code namespace is forever (CVE-style)

Per process-notes Rule 3. Once a code ships (validation MC2xxx, lint MC3xxx, parse MC1xxx), its meaning is locked. ~75 codes shipped across Phase 3; zero retired except MC3008 (which was promoted to MC2011 pre-acceptance per ADR-0005 Amendment #11).

Phase 3I caught a near-collision with MC2053 (Phase 3H had already shipped it for "duplicate fitted-artifact name"; the ADR proposed it for `predict()` arity validation). The Phase 3I implementer's self-audit caught the collision and remediated to MC2057 mid-phase. That triggered a process-notes Rule 3 amendment: future ADRs sweep proposed codes against the baseline before publishing. The pre-flight sweep then prevented collisions in 3J (16 codes), 3H.1 (1 code), and 3H.2 (7 codes).

### 3. Hand-rolled wins over deps

Per process-notes Rule 5. Phase 3 added one new runtime dep (`serde_yaml` in `mc-model` for Phase 3A) plus the existing four in `mc-core` (`smallvec`, `ahash`, `thiserror`, `once_cell`). Every other addition — formula parser (3D, ~250 lines), CSV parser (3C), JSON envelope serializer (3B), `norm_inv` Beasley-Springer-Moro (3I, ~30 lines) — was hand-rolled. Each saved 10-100 transitive deps and zero version-bump churn.

### 4. Backward compat is a hard gate, not a nice-to-have

Per process-notes Rule 7. Every Phase 3 handoff included "every existing test passes" as a binding gate; every phase shipped with backward-compat regression tests asserting that prior-phase fixtures still load identically. The Acme YAML from 3A still loads, lints, validates, and tests through 3H.2 unchanged. The NBA cartridge (added during Phase 4A's plugin work) likewise. The email-matchback Tide MMM has been load-bearing across multiple phases as an external compatibility witness.

`schema_version: "1.0"` on every JSON envelope has been bumped exactly once across Phase 3 — the trace envelope to `"1.1"` in Phase 6A.2 (post-Phase 3J). All other phases preserved the "1.0" pin via additive fields with `#[serde(default)]`.

### 5. The audit pattern is load-bearing

Phase 6A.1 introduced the per-phase self-audit pattern after the Sonnet × 3-lens code review surfaced silent-correctness bugs. The pattern matured across 3J / 3H.1 / 3H.2:

- **Section D (revert-and-verify)** caught false-pass tests in 6A.1 (MIN-6 misapplication) and proved the load-bearing fixes were genuinely exercised in subsequent phases.
- **Section G (diagnostic-code namespace check)** caught the MC2053 collision in 3I.
- **Section L (kernel boundary verification)** caught a real `ScalarValue::Str` leakage bug in 3J — would have shipped silently without the dedicated section.
- **Section C (public surface verification)** caught a Hard Rule 7 violation in 3H.2 (`SaturationSpec::feature_name` shipped as `pub fn` instead of `pub(crate)`; remediated mid-audit).

Three audit-pattern catches across Phase 3. Each would have shipped silently without the pattern. The pattern is now mature and worth retaining for any future kernel-touching phase.

### 6. The ADR + handoff pattern carries decisions forward

Each Phase 3 sub-phase (except the bundled 3E/F/F.1/G) shipped under an ADR + handoff pair. The ADR captures binding decisions; the handoff translates them into per-item Decision Matrices that pre-empt likely walls. Six ADRs (0011, 0012, 0013, 0014, 0015, 0016, 0017, 0018) span the arc; each one's Acceptance Amendments section captures GPT/Desktop review feedback as a numbered audit trail (per process-notes Rule 2).

The pattern's value emerged across the arc: Phase 3I had 1 amendment; Phase 3J had 8 amendments; Phase 3H.1 had 0 (small phase, PM-accepted directly); Phase 3H.2 had 2 amendments. The volume tracks scope: bigger phases = more design questions = more amendments = more rigor.

---

## What didn't go well

Honest assessment.

### The all-uncommitted-at-end anti-pattern (Phase 3I)

Phase 3I implemented all 8 items + 45 tests in a single uncommitted branch state. The PM had to commit it as one big merge commit because per-item progression was lost. process-notes Rule 11 was amended post-3I to add this as an explicit anti-pattern; subsequent phases (3J, 3H.1, 3H.2) all honored per-cluster commit discipline.

### The MC2053 collision (Phase 3I)

The PM (this author) proposed MC2053 in ADR-0015 without sweeping the baseline. Phase 3H had already shipped MC2053 for an unrelated rule. The Phase 3I implementer's self-audit caught the collision; remediation was mid-phase but added work that should have been pre-empted. process-notes Rule 3 was amended post-3I to require pre-flight sweeps before publishing ADRs.

### The "M-14 closure is aspirational" gap (Phase 3H.2)

Phase 3H.2 ships native adstock + saturation transforms in the formula engine. The original audit gap (M-14) cited the email-matchback Tide MMM as the demand source — but the Tide MMM uses an earlier `lag() + rolling_avg()` architecture that would need cartridge-team buy-in to migrate. Phase 3H.2 ships the CAPABILITY for general MMM authors but does NOT migrate the Tide MMM. ADR-0018 Amendment §12 documents this honestly. Future readers should not conflate "shipped capability" with "shipped migration."

### Cross-coord dep-graph debt accumulation

Per ADR-0018 Amendment §11. By the time Phase 3 closed, four ADRs inherited the existing cross-coord dep-graph debt (3E `prev`/`lag`/`actual_ref`, 3J `scenario_ref` + 2-arg `actual_ref`, 3H.2 adstock). Each individual inheritance was bounded; the cumulative position warrants tracking. The PM committed to scoping the dedicated cross-coord dep-graph fix-it phase within the next 2 phase cycles. This is technical debt the project knowingly carries forward — surfacing it explicitly is the discipline that prevents accumulation past the bound.

---

## What goes next

Phase 3 closes. The project pivots away from formula-engine work. Per MASTER_PHASE_PLAN.md, the candidate next phases are:

- **Phase 4C — Multi-domain workspace primitive** (per the proposal at [`../research-notes/multi-domain-workspaces-proposal.md`](../research-notes/multi-domain-workspaces-proposal.md)). Tier A scope is bounded; closes 9 of 16 capability gaps from the audit. ADR-0019 to draft.
- **Phase 5D — Tessera xlsx + group_by + multi-file ingest**. Closes ~420 lines of email-matchback Python (the data-ingestion residual). Needs ADR.
- **Phase 6B — Web UI / planning grid**. The prototype at [`../prototypes/mosaic-grid-prototype.html`](../prototypes/mosaic-grid-prototype.html) is a starting point. Big phase; needs design + ADR.
- **Phase 6C — Distribution + install pipeline**. `cargo-dist` + Homebrew + curl installer + `mosaic update`. Crate names already reserved on crates.io. Smallish phase, mostly wiring.
- **Cross-coord dep-graph fix-it phase** — per Amendment §11 cumulative tracking obligation. Within next 2 phase cycles.

The strategic question now isn't "what's missing in the formula engine?" — it's "where does the project's attention go next to deliver more user value?" That's a customer-acquisition / product-direction question, not a feature-completion question.

---

## Stats summary

```
Phase 3 sub-phases:                    11 (3A → 3J + 3H.1 + 3H.2)
ADRs:                                  6 (0011, 0012, 0013, 0014, 0015, 0016, 0017, 0018)
Acceptance Amendments across ADRs:     ~25
Diagnostic codes shipped:              ~75 (MC1003-1029 + MC2011-2069 + MC3001-3011 - retirements)
Diagnostic codes retired:              1 (MC3008; promoted to MC2011 in Phase 3B per ADR-0005 Amendment #11)
Regression tests added:                ~750 (216 → ~970 across the arc; ~570 specifically for Phase 3 formula functions)
mc-core public functions added:        0
mc-core public types added:            ~10 (Expr enum variants, OutputBound, Transforms, AdstockSpec, SaturationSpec, Scope variants, MeasureRole::Indicator)
Cargo.lock pin churn:                  0 since Phase 1B (toolchain stays Rust 1.78)
New runtime dependencies:              1 (serde_yaml in mc-model, Phase 3A)
Tag count for Phase 3:                 9 (3A, 3B, 3C, 3D, 3E-3G bundled, 3H, 3I, 3J, 3H.1, 3H.2)
Final test count at close:             912 / 0 / 5
Email-matchback Python eliminated:     ~750 lines (~60% reduction)
```

---

## Closing thought

Phase 3 took ~6 weeks of focused work across 11 sub-phases. It was the second-largest arc the project has shipped (after Phase 1's kernel), and arguably the highest-leverage. Without the formula engine, every Mosaic user would be writing Rust against the kernel API. With it, they write YAML.

What's worth keeping from how Phase 3 ran:

1. **Discipline in scoping.** Each sub-phase had a tight binding scope; deferred items went to research notes, not "while I'm here" drift.
2. **Discipline in decisions.** Each non-trivial choice got an ADR. Each ADR review got captured as a numbered amendment.
3. **Discipline in verification.** The audit pattern caught three real bugs across the arc; each one would have shipped silently without it.
4. **Discipline in stopping.** The "demand-driven only" framing landed at 3I and held through 3H.2. Adding speculative features wasn't tempting because the demand-driven discipline made it cheap to defer.

The project pivots now. Phase 3 is done. The formula engine became what it needed to be.

---

*Authored 2026-05-06 at the close of Phase 3, by the project's PM (Claude Opus 4.7), in collaboration with the Phase 3 implementing instances (Claude Sonnet 4.6, multiple sessions) and reviewers (GPT-5.5, Claude Desktop). Next deliverable: an ADR / handoff for whichever phase the project owner elects next.*
