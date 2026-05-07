# ADR-0017: Phase 3H.1 — Fitted-Model `output_bound`

**Status:** Accepted (handoff-first parallel flow per process-notes Rule 1; PM-accepted same day)
**Date:** 2026-05-06
**Deciders:** project owner
**Phase:** 3H.1 (per [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md))

> Phase 3H.1 ships `output_bound: { min, max }` on fitted models — a small additive schema field that clamps `predict()` output to a configured range. Closes the Amarillo case (linear regression extrapolating into negative revenue) and gives logistic models a way to declare safety bounds (e.g., 0.001 / 0.999 instead of strict 0.0 / 1.0). This is the FIRST half of the ADR-0015 / ADR-0016 cluster E split — the SECOND half (adstock + saturation transforms) is Phase 3H.2 / ADR-0018.

---

## Context

The post-6A audit (M-20 in master-gap-report) flagged a real correctness risk in fitted-model evaluation: a Lasso regression for the Tide MMM produced a -$5,706 prediction for Amarillo at zero spend. Linear models extrapolating into negative territory aren't unusual; the audit's binding decision was to surface a declarative `output_bound: { min: 0 }` on fitted models so cube authors can clamp at the model layer rather than wrap every `predict()` call in `clamp(predict(...), 0, max)`.

ADR-0016 originally bundled `output_bound` with adstock + saturation transforms as a single Phase 3H.1. The post-Phase-3J PM scope review (2026-05-06) split the two:

- `output_bound` is a small additive field with bounded scope (~50 lines, 5 tests, one new MC code).
- Adstock + saturation has multiple binding design decisions (geometric vs Weibull adstock; Hill vs log saturation; per-feature schema; eval order; cross-coord access for time-decayed features) that warrant a dedicated ADR.

Splitting honors the post-3J "demand-driven only" framing: ship `output_bound` because the Amarillo case is real demand, defer adstock/saturation to its own ADR (3H.2 / ADR-0018) for proper design treatment.

**Architectural importance.** None. This is mechanical extension of `ParsedFittedModel` per the Phase 3H pattern. The kernel touch is a single clamp at the eval site. No new public types, no new public functions in mc-core.

---

## Decisions

### Decision 1: Scope — `output_bound` only; adstock/saturation deferred to 3H.2

**In scope (binding):**

- Add `output_bound: { min: <f64>?, max: <f64>? }` field to `ParsedFittedModel`. Both `min` and `max` are optional; either or both can be set.
- At eval time, after the prediction is computed (and after the link function for logistic models), clamp the result to the bounds.
- Validator: if both `min` and `max` are set, `min < max` (strict inequality). MC2070 if violated.
- Backward compat: existing fitted models without `output_bound` work unchanged.

**Out of scope (deferred to ADR-0018 / Phase 3H.2):**

- Adstock transforms (geometric, Weibull) on `fitted_models:`
- Saturation transforms (Hill, log, root) on `fitted_models:`
- Per-feature transforms
- Multi-step transform pipelines (adstock → saturation → coefficient)
- Cross-coord time-decayed feature evaluation

After ADR-0018 / 3H.2 ships, the formula-engine deferred queue from ADR-0015 is empty.

### Decision 2: Schema shape

```yaml
fitted_models:
  - name: tide_mmm_v1
    method: linear
    intercept: 1234.56
    coefficients:
      - { feature: spend, weight: 0.5 }
      - { feature: pace, weight: 1.2 }
    output_bound:               # NEW; both fields optional
      min: 0.0
      max: 1000000.0
```

For logistic models, typical use is safety bounds inside [0, 1]:

```yaml
  - name: win_probability
    method: logistic
    intercept: -0.5
    coefficients: [ ... ]
    output_bound:
      min: 0.001
      max: 0.999
```

For linear models, typical use is non-negativity:

```yaml
    output_bound:
      min: 0
```

### Decision 3: Where in eval the clamp applies

**Binding:** the clamp applies AFTER all other prediction steps (standardization, linear combination, link function). Specifically in `crates/mc-core/src/cube.rs::resolve_cross_coord_read` for the `PredictModel` arm:

```rust
// existing: linear combination
let mut prediction = model.intercept + sum_of(coef * standardized_feature);

// existing: link function
let prediction = match model.method.as_str() {
    "logistic" => 1.0 / (1.0 + (-prediction).exp()),
    _ => prediction,  // linear
};

// NEW: output_bound clamp
let prediction = match &model.output_bound {
    Some(bound) => bound.clamp(prediction),
    None => prediction,
};

ScalarValue::F64(prediction)
```

The clamp logic:
- If only `min` is set: `prediction.max(min)` (floor)
- If only `max` is set: `prediction.min(max)` (ceiling)
- If both: `prediction.clamp(min, max)`
- If `prediction.is_nan()`: pass through unchanged (NaN propagation; should be impossible given Phase 6A.1 NaN-rejection but defense-in-depth)

### Decision 4: Validator rules

| Rule | Code | Severity |
|---|---|---|
| `output_bound.min` and `output_bound.max` both set, but `min >= max` | **MC2070** | Error (validate-time) |
| `output_bound.min` is NaN or infinite | (deserialize error — `serde_yaml` rejects) | Parse |
| `output_bound.max` is NaN or infinite | (deserialize error) | Parse |

**Not validated** (deliberate non-decisions):

- Logistic model with `output_bound` outside [0, 1]: allowed without warning. A user might want safety bounds at 0.05 / 0.95. Linting this would create false positives for legitimate use.
- Linear model with `min: 0` when negative predictions are expected: allowed; the user knows their domain.
- `output_bound` set with only one of min/max: allowed (one-sided clamp is a normal use case).

### Decision 5: Backward compat

**Binding:** no schema_version bump on the model envelope. The `output_bound` field is additive (`#[serde(default)]`); existing parsers see it as an unknown-but-tolerated absent field. Existing fitted models without `output_bound` evaluate identically to today.

The `predict()` formula function signature is unchanged. Cube authors don't need to do anything to their formulas to get clamping — declaring `output_bound` on the fitted model is sufficient.

### Decision 6: Diagnostic codes

**One new code:** MC2070 (validate-time, `output_bound.min >= output_bound.max`).

Pre-flight sweep verified 2026-05-06 against `main` HEAD `123487a`: MC2070 unassigned. Highest shipped MC2xxx is MC2069 (Phase 3J / ADR-0016 Amendment §4).

### Decision 7: Implementation order — single cluster

Trivial sequencing: schema field → validator → eval. Single commit OK (the phase is small enough that per-cluster commit discipline doesn't apply — there's only one cluster).

If the implementer prefers per-step commits (`feat(3H.1): output_bound schema`, `feat(3H.1): output_bound validator`, `feat(3H.1): output_bound eval clamp`), that's also fine. The PM doesn't require per-step here given the small scope.

### Decision 8: Process flow — handoff-first parallel

Per process-notes Rule 1 self-test:

1. Kernel change? Yes (single clamp at eval site). FAILS strict ADR-first → would normally require ADR-first.
2. Runtime dep added? No.
3. Contract surface change? Just adding an optional schema field; no public API addition in mc-core.
4. Scope < ~1500 lines? Yes (~50 lines).
5. Strategic decisions derivable from prior ADRs? Yes (Phase 3H established the fitted-model evaluation pattern; this is a small additive field on top).

The kernel touch is so narrow (one clamp call) and the design questions are so bounded (Decision 4's table is exhaustive) that handoff-first parallel is appropriate. ADR-0017 lands alongside the implementation per Phase 3D / 3I / 3J precedent.

---

## Out of scope

- Adstock + saturation (Phase 3H.2 / ADR-0018; explicit 3H.1/3H.2 split)
- Per-feature output bounds (one bound per fitted model, not per coefficient)
- Bound expressions (the bounds are literal f64 values, not formulas — computed bounds would need parameter integration; defer)
- Asymmetric bound semantics (e.g., "soft clamp" via sigmoid) — explicit hard clamp only
- Output transforms beyond clamping (e.g., absolute value, square, etc.)

---

## Alternatives considered

### Alt 1: Wrap `clamp()` in formula bodies instead of declaring on the model

Considered. Cube authors could write `body: "clamp(predict('mmm', features), 0, 1000000)"`. **Rejected** because:

- The clamp is a property of the model (the model knows its valid output range), not a property of every formula that uses it.
- Bound is declared once on the model definition; every consuming formula gets it for free.
- Authoring friction: 8 formulas using the same fitted model would all need the same wrapping.

### Alt 2: Split min and max into separate top-level fields (`output_min:` and `output_max:`)

Considered. Two siblings on `ParsedFittedModel`. **Rejected** because:

- Grouping under `output_bound:` makes intent clear and matches the existing `standardization:` pattern (a sub-block holding related fields).
- Easier to extend later (e.g., `output_bound: { min: 0, soft_max_via_sigmoid: 1000 }` if soft clamping is ever wanted).

### Alt 3: Bundle with adstock/saturation in one Phase 3H.1 ADR

The original ADR-0016 framing. **Rejected** post-3J review:

- `output_bound` is small (~50 lines, 1 MC code).
- Adstock/saturation has multiple binding design decisions deserving GPT/Desktop review.
- Splitting honors the "demand-driven only" framing (output_bound has real demand from the Amarillo case; adstock/saturation is speculative until a real MMM customer asks).

### Alt 4: Defer entirely until adstock/saturation lands

Considered. Ship 3H.1 + 3H.2 together as one phase. **Rejected** because:

- The Amarillo bug exists today in the NBA + email-matchback cartridges; users can wrap with `clamp()` but a declarative fix is cleaner.
- Coupling small fixes to large design work delays the small fix unnecessarily.
- 3H.1 ships in a single session; 3H.2 needs ADR review + design discussion.

---

## Cross-links

- **Audit gap:** [`../audits/master-gap-report.md`](../audits/master-gap-report.md) M-20 (Amarillo case)
- **Original ADR that bundled this:** [`0015-phase-3i-formula-language-completion.md`](0015-phase-3i-formula-language-completion.md) Decision 1 (deferred items table); [`0016-phase-3j-formula-deferred-items.md`](0016-phase-3j-formula-deferred-items.md) Decision 1 (cluster E split rationale)
- **Companion ADR (next):** ADR-0018 — Phase 3H.2 — adstock + saturation transforms (to be drafted)
- **Phase 3H base:** [`0015-phase-3i-formula-language-completion.md`](0015-phase-3i-formula-language-completion.md) (predict/calibrate/exp/norm_cdf shipped; output_bound is a small additive amendment to that work)
- **Process rules:** [`../process-notes.md`](../process-notes.md) §1 (handoff-first parallel flow), §3 (diagnostic-code retirement + pre-flight sweep), §11 (git workflow)

---

## Notes

**Phase 3 arc (post-Phase 3H.1 + 3H.2):**

After both 3H.1 and 3H.2 ship, the deferred queue from ADR-0015 is fully empty. Phase 3 transitions to "demand-driven only" — future formula or fitted-model additions require a real customer use case, not speculative work. This is the explicit closing condition for the Phase 3 arc that started at Phase 3A (model definition layer) and now spans 11 sub-phases (3A → 3J + 3H.1 + 3H.2).

**The split mechanic.** This is the first time the project has split a single cluster into two phases with separate ADRs after the original ADR shipped. The pattern: when the original ADR's scope review reveals the cluster has substantively different design surfaces, splitting is acceptable as long as both halves are committed-to in the new ADRs (ADR-0017 + ADR-0018). The ADR-0016 cluster E framing remains the historical record; ADR-0017 and ADR-0018 are the binding contracts going forward.

**Acceptance amendment audit trail.** Per process-notes Rule 2. Handoff-first parallel flow means GPT/Desktop review feedback after handoff merge will be appended below as Acceptance Amendments §1+ if any surfaces.

---

## Acceptance amendments

*(None as of authoring. Handoff-first parallel flow per Rule 1; PM accepted same day given small scope and bounded design space.)*
