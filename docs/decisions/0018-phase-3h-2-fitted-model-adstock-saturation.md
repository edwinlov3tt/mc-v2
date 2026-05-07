# ADR-0018: Phase 3H.2 — Fitted-Model Adstock + Saturation Transforms

**Status:** Accepted (with Amendment §11 from Claude Desktop review; PM-accepted same day per Rule 1 alternative — design space is well-established MMM convention with no novel architectural decisions)
**Date:** 2026-05-06
**Deciders:** project owner
**Phase:** 3H.2 (per [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md))

> Phase 3H.2 ships native adstock + saturation transforms in `fitted_models:` — the second half of the ADR-0015 cluster E split that started with Phase 3H.1's `output_bound`. After 3H.2 ships, the formula-engine deferred queue from ADR-0015 is **empty**. This is the final phase to close the post-6A audit's deferred items; Phase 3 transitions to "demand-driven only" post-merge.

---

## Context

Marketing-mix models (MMMs) almost universally apply two transforms to media spend before the linear regression: **adstock** (carryover effect — today's TV spend influences next month's revenue, decayed) and **saturation** (diminishing returns — doubling spend doesn't double conversions). Today, Mosaic's `predict()` accepts pre-transformed feature values; users compute adstock + saturation in Python before calling predict(). The Tide MMM (email-matchback) is the canonical example: ~80 lines of Python adstock/saturation pre-processing per `prepare_mmm_inputs.py`.

Native support means:
- Cube authors declare adstock + saturation alongside the fitted model coefficients.
- `predict()` applies adstock (cross-coord backward scan), then saturation, then coefficients — automatically.
- The Python pre-processing collapses to a YAML declaration.

This is the largest single closure of email-matchback Python in any single phase: ~80 lines of pre-processing code go away; the model itself becomes more inspectable (the transform parameters live in YAML, not buried in pandas chains).

**Architectural importance.** Moderate. Adstock requires `predict()` to make cross-coord reads (back through Time periods), which inherits the existing cross-coord dependency-graph debt documented in [`../research-notes/cross-coord-dep-graph.md`](../research-notes/cross-coord-dep-graph.md). Per ADR-0016 Amendment §12 precedent, we accept this debt — performance fix is a future ADR, not 3H.2 scope.

This is the **final** formula-engine phase before the deferred queue from ADR-0015 is empty. Phase 3H.2 closes that queue; Phase 3 then transitions to "demand-driven only" — future formula or fitted-model additions require a real customer use case.

---

## Decisions

### Decision 1: Scope — adstock + saturation in 3H.2; deferred queue closes after this ships

**In scope (binding):**

- Adstock transforms on `fitted_models:` — geometric only in v1
- Saturation transforms on `fitted_models:` — Hill + Log in v1
- Per-feature transform specifications (each feature can have adstock, saturation, both, or neither)
- Combined transform pipeline: feature value → adstock → saturation → multiply by coefficient → sum + intercept → link function → output_bound (Phase 3H.1)
- 6 new diagnostic codes (MC2071-MC2076) for adstock + saturation validation
- Cross-coord access from `predict()` (inherits existing dep-graph debt per Amendment §12 precedent)

**Out of scope (deferred to future amendments if demanded):**

- Weibull adstock (only geometric in v1)
- Root / exp / S-curve saturation forms (only Hill + Log in v1)
- Adstock+saturation as standalone formula functions (only inside `fitted_models:` declaration; not as `adstock(measure, rate)` callable)
- Per-coefficient transform overrides (transforms attach to features at the model level, not per-call)
- Time-varying transform parameters (e.g., adstock rate that itself decays — out of scope; transforms are constants per fitted model)
- Cross-coord dep-graph performance fix (separate phase per Amendment §12)
- Adstock for non-Time dimensions (only Time-axis carryover; spatial / channel-axis carryover is future work)

**After this ships:** the ADR-0015 deferred queue is empty. Future formula additions are demand-driven.

### Decision 2: Adstock model — geometric, per-feature, required `max_lookback`

**Binding adstock formula (geometric):**

```
adstocked[t] = feature[t] + rate * adstocked[t-1]
```

with the recursion bounded by `max_lookback`:

```
adstocked[t] = sum_{k=0}^{min(t, max_lookback)} (rate^k * feature[t-k])
```

**Per-feature configuration:** each feature in the fitted model can independently declare its adstock rate. Some channels (TV) decay slowly (rate ~0.7); others (paid search) decay quickly (rate ~0.2).

**Required parameters:**
- `rate: <f64>` in `[0.0, 1.0]` (geometric decay rate; 0 = no carryover, 1 = full carryover forever)
- `max_lookback: <u32>` ≥ 1 (number of prior periods to include; prevents unbounded scans)

**Why required, not optional:**
- `rate` has no sensible default (channel-specific).
- `max_lookback` has no sensible default (model-specific). Forces the user to think about staleness explicitly. Without this, a future model could scan 10 years of history per predict() call.

**Validation:**
- `rate` outside [0, 1] → MC2075
- `max_lookback` <= 0 → MC2076
- Feature name in adstock declaration not present in `coefficients` → MC2071

### Decision 3: Adstock eval — Null handling and edge cases

**Binding behavior:**

- **At t=0** (first time period in the Time dim): no prior periods; adstocked value = current feature value.
- **At t < max_lookback:** scan back as far as exists in the Time dim; truncate at the start.
- **At t >= max_lookback:** scan back exactly `max_lookback` periods.
- **If feature[t-k] is Null:** treat as 0.0 for the recursive sum. Rationale: Null typically means "no spend that period" semantically; treating as 0 makes adstocked values consistent. **NOT NULL-propagating** (this is a deliberate exception to Mosaic's normal Null-propagation rule, per the MMM convention that "missing spend = zero spend").
- **If the entire backward scan is Null:** the result is the current feature value (not Null) — same as t=0 case.
- **`max_lookback` exceeds Time dim length:** silently truncate (no error). Documented behavior.

### Decision 4: Adstock schema shape

**Binding:**

```yaml
fitted_models:
  - name: tide_mmm_v1
    method: linear
    intercept: 1234.56
    coefficients:
      - { feature: tv_spend, weight: 0.5 }
      - { feature: search_spend, weight: 1.2 }
      - { feature: ooh_spend, weight: 0.3 }

    # NEW: transforms block (optional; both adstock + saturation optional within)
    transforms:
      adstock:
        - { feature: tv_spend,     rate: 0.7, max_lookback: 12 }
        - { feature: search_spend, rate: 0.2, max_lookback: 4 }
        # ooh_spend has no adstock declared → no carryover
      # saturation block — see Decision 6
```

The `transforms:` block is optional. Within it, `adstock:` is optional (a list of per-feature specs). A feature without an adstock entry just uses its current-period value (existing pre-3H.2 behavior).

**Rationale for grouping under `transforms:` (vs. nesting in coefficients):**
- Schema clarity: all transforms in one place; easier to audit "what's being transformed."
- Future extension: if computed transforms or other forms ship, they live alongside.
- Compile-time efficiency: the engine collects all transforms in one pass.

### Decision 5: Saturation forms — Hill + Log in v1

**Binding (only these two forms in v1):**

**Hill saturation** (S-curve; industry standard for MMM):

```
saturation(x) = x^alpha / (gamma^alpha + x^alpha)
```

Parameters:
- `alpha: <f64>` > 0 (shape parameter; controls steepness)
- `gamma: <f64>` > 0 (half-saturation point; the value at which saturation = 0.5)

Output: in `[0, 1]` for x ≥ 0.

**Log saturation** (concave; simpler):

```
saturation(x) = log(1 + x / scale)
```

Parameters:
- `scale: <f64>` > 0 (controls the saturation rate)

Output: ≥ 0 for x ≥ 0.

**Forms deferred to future amendments:**
- Root: `x^(1/k)`
- Exp / S-curve variant: `1 - exp(-k*x)`
- Negative-binomial: rare; only ship if a real customer asks
- Custom (user-defined): out of scope for v1 (no formula-language integration)

**Validation:**
- Hill `alpha` <= 0 or `gamma` <= 0 → MC2072
- Log `scale` <= 0 → MC2073
- Feature in saturation declaration not in `coefficients` → MC2074
- Unknown saturation `type` (e.g., user types `"hil"` instead of `"hill"`) → MC2077

(MC2077 is a minor addition; will need pre-flight verification before shipping.)

### Decision 6: Saturation schema shape

**Binding:**

```yaml
fitted_models:
  - name: tide_mmm_v1
    method: linear
    coefficients: [...]
    transforms:
      adstock: [...]
      saturation:
        - { feature: tv_spend,     type: hill, alpha: 2.0, gamma: 5000.0 }
        - { feature: search_spend, type: log,  scale: 1000.0 }
        # ooh_spend has no saturation → linear response
```

The `saturation:` block is optional within `transforms:`. A feature without a saturation entry just uses its (post-adstock) value linearly.

### Decision 7: Combined transform pipeline (eval order)

**Binding eval order for each feature in `predict()`:**

1. Read the feature's value from the call args (typically a same-coord measure ref).
2. If adstock is declared for this feature: apply adstock backward scan (Decision 2 + 3) → produces `adstocked_value`.
3. If saturation is declared for this feature: apply saturation curve (Decision 5) to `adstocked_value` → produces `transformed_value`.
4. Multiply by the feature's coefficient.
5. Sum across all features + intercept = `linear_prediction`.
6. Apply link function (logistic if `method: logistic`).
7. Apply `output_bound` clamp (Phase 3H.1).
8. Return.

**Rationale:** This matches MMM convention (adstock first because it's a temporal transformation; saturation second because it operates on the time-aggregated effect).

**Standardization (Phase 6A.1):** if standardization is also declared for a feature, it applies AFTER all transforms but BEFORE the coefficient multiplication. So the full pipeline is:

```
feature → adstock → saturation → standardization → multiply by coefficient → sum + intercept → link → output_bound
```

This is the binding order. Document explicitly.

### Decision 8: Cross-coord dep-graph debt — inherit per Amendment §12 precedent

**Binding (per ADR-0016 Amendment §12 precedent):**

Adstock requires `predict()` to read prior Time-period values, making predict-with-adstock a **cross-coordinate operator**. This inherits the existing cross-coord dependency-graph debt:

> **Performance note (inherited debt):** `predict()` calls in fitted models with `adstock:` declared inherit the existing cross-coordinate dep-graph behavior. Every write to a cube containing such a model invalidates all derived cells using that fitted model (over-invalidation; correctness preserved via revision-bumping). Performance fix is deferred to a future phase ADR (the dep-graph rework). Cartridges using adstock at high cube cardinality may experience slow writes; document expectations in cartridge READMEs.

This is documented at the decision level (not just cross-linked) per the Amendment §12 pattern.

**Scope clarification:** `predict()` WITHOUT adstock stays a same-coord operator (no dep-graph debt). The cross-coord behavior triggers only when `transforms.adstock:` is declared.

### Decision 9: Backward compat — no schema_version bump

**Binding:** the `transforms:` block is additive (`#[serde(default, skip_serializing_if = "Option::is_none")]`). Existing fitted models without `transforms:` work identically to today. No schema_version bump on the model envelope.

The `predict()` formula function signature is unchanged. Cube authors don't modify their formulas — declaring `transforms:` on the fitted model is sufficient.

### Decision 10: Diagnostic codes — 6 reserved (sweep pending)

**Pre-flight sweep (already done before draft):** MC2071, MC2072, MC2073, MC2074, MC2075, MC2076 all unassigned against `main` HEAD `98746d5`. MC2077 (added by Decision 5) needs sweep before final acceptance.

| Code | Stage | Meaning |
|---|---|---|
| MC2071 | validate | `transforms.adstock` declares feature not in coefficients |
| MC2072 | validate | Hill saturation parameters invalid (`alpha` <= 0 or `gamma` <= 0) |
| MC2073 | validate | Log saturation parameters invalid (`scale` <= 0) |
| MC2074 | validate | `transforms.saturation` declares feature not in coefficients |
| MC2075 | validate | Adstock `rate` outside [0, 1] |
| MC2076 | validate | Adstock `max_lookback` <= 0 |
| MC2077 | validate | Unknown saturation `type` (e.g., `"hil"` instead of `"hill"`) — pending sweep verification |

### Decision 11: Implementation order — single feature build

Suggested order (5 commits expected):

1. **Schema additions:** `Transforms`, `AdstockSpec`, `SaturationSpec`, `SaturationType` enum on `ParsedFittedModel`. Compile pass-through.
2. **Validator (MC2071-MC2076 + MC2077):** all 7 codes with regression tests.
3. **Adstock eval (kernel cross-coord):** add backward-scan logic in `cube.rs::resolve_cross_coord_read`'s `PredictModel` arm.
4. **Saturation eval (kernel):** Hill + Log functions; integrate into the pipeline.
5. **Integration tests + email-matchback re-survey:** end-to-end fitted-model evaluation with both transforms; confirm Tide MMM workflow.

### Decision 12: Process flow — ADR-first with PM disposition

Per process-notes Rule 1 self-test:

1. Kernel change? **Yes** (new cross-coord read path in `cube.rs`).
2. Runtime dep added? No.
3. Contract surface change? Yes (new `Transforms` schema block + new public types in mc-core for transform specs).
4. Scope < ~1500 lines? Estimated ~960 source + ~600 test = ~1560. Borderline.
5. Strategic decisions derivable from prior ADRs? **Mostly** — the MMM conventions (geometric adstock, Hill saturation) are well-established; the cross-coord dep-graph implication has explicit Amendment §12 precedent. Few genuinely novel design choices.

**PM disposition:** the ADR is technically ADR-first per Rule 1 (kernel change + contract surface change), but the design space is well-established MMM convention, NOT novel. Recommendation: **PM-accept directly without GPT/Desktop review cycle**, similar to ADR-0017. If the project owner prefers review (matching the ADR-0016 pattern given 3H.2 closes the formula-engine deferred queue and "we want this right"), that's also fine — adds one round-trip cycle but no scope change expected from review.

---

## Out of scope

Beyond Decision 1's deferred items:

- Multi-feature interactions (e.g., adstock with cross-feature decay)
- Temporal saturation (saturation parameters that vary by time period)
- Bayesian credible intervals on transform parameters (would require a stochastic kernel — out of scope)
- Adstock visualization in `mc model inspect` (could be a Phase 6B/UI concern)
- Calibration curves on adstock outputs (orthogonal to existing `calibrate()` from Phase 3H)
- Auto-fitting of transform parameters (Mosaic doesn't fit; Python fits per ADR-0015 architecture)

---

## Alternatives considered

### Alt 1: Standalone formula functions `adstock(measure, rate)` and `hill_saturation(value, alpha, gamma)`

Considered. Cube authors could write `predict('mmm', hill_saturation(adstock(spend, 0.7), 2.0, 5000.0), ...)` instead of declaring transforms on the model. **Rejected** because:

- The transform IS a property of the fitted model (the model was FITTED with adstock+saturation; using it WITHOUT them produces wrong predictions).
- Coupling the transforms to the model declaration prevents the "correct prediction" path from being optional.
- Authoring friction: one fitted model used in 8 formulas would require all 8 to remember the same transform parameters.

### Alt 2: Per-coefficient transforms (transforms nested under each coefficient entry)

Considered:
```yaml
coefficients:
  - { feature: tv_spend, weight: 0.5, adstock: { rate: 0.7, max_lookback: 12 }, saturation: { type: hill, ... } }
```
**Rejected** for Decision 4 — the standalone `transforms:` block is cleaner for cross-feature audit ("what's being adstocked?" reads at a glance) and matches the existing `standardization:` sibling pattern.

### Alt 3: Weibull adstock instead of (or alongside) geometric

Considered. Weibull (shape + scale parameters) is more flexible; can model "delayed peak" carryover. **Rejected for v1** because:
- Geometric covers ~90% of MMM use in practice.
- Weibull adds 2 more parameters to validate + 2 more failure modes to document.
- Real-world MMM packages (Robyn, Meta MMM, LightweightMMM) ship geometric as default; Weibull is opt-in.
- Weibull can be added in a future amendment if a real customer asks.

### Alt 4: Defer cross-coord dep-graph debt to before-3H.2

Considered. Fix the dep-graph first (separate phase), then ship 3H.2 on top of the cleaner foundation. **Rejected** because:
- The dep-graph fix is its own substantial scope (per cross-coord-dep-graph.md, multiple architectural questions to answer).
- 3H.2's correctness doesn't depend on the fix — performance does.
- Per Amendment §12 precedent (ADR-0016 cluster D items), inheriting the debt is documented and acceptable.

### Alt 5: Single saturation form (Hill only) in v1

Considered. Ships even more conservatively. **Rejected** because:
- Hill is the MMM standard but has 2 parameters that can be tricky to fit (alpha, gamma)
- Log is a simpler alternative for users new to MMMs (1 parameter, more interpretable)
- Both are mathematically distinct and serve different use cases
- Shipping both upfront avoids "users wait for v2 to use Log"

### Alt 6: Bundle 3H.1 and 3H.2 (un-split)

Considered. The original ADR-0016 framing put both in Phase 3H.1. **Rejected** post-3J review: 3H.1 is small and was shippable in a single session; 3H.2 is bigger and benefits from its own ADR. Splitting was the right call (validated by 3H.1 shipping cleanly with no SPEC QUESTIONs).

---

## Cross-links

- **The half that already shipped:** [`0017-phase-3h-1-fitted-model-output-bound.md`](0017-phase-3h-1-fitted-model-output-bound.md)
- **Original ADR that bundled cluster E:** [`0015-phase-3i-formula-language-completion.md`](0015-phase-3i-formula-language-completion.md) Decision 1; [`0016-phase-3j-formula-deferred-items.md`](0016-phase-3j-formula-deferred-items.md) Decision 1
- **Cross-coord dep-graph debt (inherited per Amendment §12):** [`../research-notes/cross-coord-dep-graph.md`](../research-notes/cross-coord-dep-graph.md)
- **Phase 3H base:** the existing `predict()` evaluation in `cube.rs` is the integration point for transforms
- **Email-matchback Tide MMM:** the canonical use case driving this phase (~80 lines of Python pre-processing closes)
- **Process rules:** [`../process-notes.md`](../process-notes.md) §1 (handoff-first vs ADR-first), §3 (diagnostic-code retirement + pre-flight sweep), §11 (git workflow)

---

## PM disposition (binding for next steps)

The ADR is ready for project-owner decision:

- **If accept-as-is:** I draft the handoff next; implementer gets one focused phase to close out the deferred queue. Estimated 1-2 sessions of implementer time.
- **If accept-with-amendments:** name the amendments; I update the "Acceptance amendments" section, then draft the handoff.
- **If send for GPT/Desktop review:** standard pattern (matches ADR-0016); review feedback lands as amendments before the handoff drafts. Adds ~1 round-trip cycle.

**My recommendation:** accept-as-is. The design space is well-established MMM convention; the cross-coord dep-graph implication has explicit Amendment §12 precedent; the scope is well-bounded by Decision 1's "out of scope" lists. Skip the review cycle and ship.

If you'd rather review, the most likely amendment is on **Decision 5** (which saturation forms in v1) — Root and Exp could legitimately ship in v1 if you want to over-deliver. The PM's call was Hill+Log only; reasonable people could disagree.

---

## Acceptance amendments

Per process-notes Rule 2: Claude Desktop–sourced amendments numbered §11+. Project owner reviewed Desktop's recommendation and accepted the ADR with the single amendment below (skipped GPT/Desktop review cycle since design space is well-established MMM convention with no novel architectural decisions).

### Amendment §11 (Claude Desktop) — Cross-coord dep-graph debt cumulative tracking

**Source:** Claude Desktop review (2026-05-06). The reviewer accepted the ADR's Decision 8 (inherit cross-coord dep-graph debt per ADR-0016 Amendment §12 precedent) but flagged the cumulative position: by the time 3H.2 ships, the same debt is inherited by 4+ ADRs across the formula engine. Each individual inheritance is bounded; the cumulative position warrants explicit tracking.

**Binding addition to Decision 8:**

> **Cumulative tracking:** After 3H.2 ships, cross-coord dep-graph debt is inherited by four or more ADRs (Phase 3E `prev` / `lag` / `actual_ref`, Phase 3J `scenario_ref` + 2-arg `actual_ref`, Phase 3H.2 adstock). The dedicated fix-it phase should be scoped within the next two phase cycles to prevent further accumulation. Each individual inheritance is bounded; the cumulative position warrants tracking. The current behavior (revision-bumping invalidates all derived cells on every write) remains correct; the cost is purely performance. The fix-it phase requires its own ADR (the architectural questions — granular per-coord cross-cube edges vs scoped invalidation sets vs other shapes — are documented in `docs/research-notes/cross-coord-dep-graph.md`).

**Process implication:** the next phase cycle (post-3H.2 / post-Phase 3) MUST include explicit consideration of when to scope the cross-coord dep-graph fix-it phase. The PM tracks this in MASTER_PHASE_PLAN.md as "queued — cross-coord dep-graph fix-it phase (target: within next 2 phase cycles after 3H.2)."

### Amendment §12 (PM, post-implementation audit) — Hard Rule 7 violation caught + remediation; M-14 honest assessment; coverage additions

**Source:** Phase 3H.2 implementer self-audit (Sections C, K, M).

**Findings during audit:**

1. **Hard Rule 7 violation (Section C — caught + fixed mid-audit, commit `6fb50aa`).** `SaturationSpec::feature_name` shipped as `pub fn` returning `&str`, intended as a convenience accessor. The handoff Hard Rule 7 binds "Zero new public functions in mc-core." The implementer demoted to `pub(crate) fn` mid-audit; variant fields stay `pub` so external callers can pattern-match the enum directly without needing the accessor. Same data accessibility, no semantic change, Hard Rule 7 honored.

2. **Coverage gaps surfaced by Section K embarrassment test (caught + filled, commit `4660136`).** The implementer noticed two tests missing:
   - No test exercised all 5 transforms simultaneously (adstock + saturation + standardization + logistic link + output_bound). Added `test_full_pipeline_all_five_transforms_active`.
   - `rate=1.0` (full carryover; symmetric counterpart to existing `rate=0.0` test) was untested. Added `test_adstock_rate_one_full_carryover`.

3. **M-14 closure is ASPIRATIONAL, not COMMITTED (Section M honest disclosure — preserved in completion report and MASTER_PHASE_PLAN).** The Tide MMM cartridge in email-matchback uses an earlier `lag() + rolling_avg()` architecture, not 3H.2's native `transforms:` block. Phase 3H.2 ships the CAPABILITY for general MMM authors; **the existing Tide MMM cartridge has NOT been migrated.** Whether the Tide MMM team chooses to migrate is their call. The Step 5 integration test (`test_tide_mmm_adstock_saturation_pipeline`) is fixture-based — proves what's POSSIBLE with 3H.2, not what's already DEPLOYED. This is acceptable disposition: Phase 3H.2 enables; cartridge migration is separate work scoped at the cartridge maintainer's discretion.

4. **Documented deviations (Section I, all minor):**
   - Schema types shipped as 3 (not 4 as the handoff suggested). `SaturationType` enum collapsed into the tagged-enum `SaturationSpec` itself via `#[serde(tag = "type")]`. Cleaner; same expressive power.
   - `apply_adstock` is `fn apply_adstock(&mut self, ...)` on `Cube` (vs the handoff's free `pub(crate) fn` pseudocode). Stylistic improvement matching 3H.1's `OutputBound::apply` precedent. Same visibility.
   - The integration test (`test_tide_mmm_adstock_saturation_pipeline`) uses a fixture-shaped cube, not the live `~/Projects/email-matchback/models/tide-mmm.yaml` (because that file uses the older architecture; modifying it would require email-matchback team buy-in).

**Process implication.** This is the third instance of the audit pattern catching real issues mid-flight (after Phase 3J's Section L Str-leakage bug and Phase 3I's MC2053 collision). The pattern is mature and load-bearing for Phase 3 quality. Recommend retaining Sections C, D, K, L for any future kernel-touching phase; the Hard Rule 7 check (Section C) and the embarrassment test (Section K) are the two that surface non-obvious issues.

**M-14 status update for the deferred queue.** The "deferred queue is empty" framing in this ADR's Notes section remains correct in the CAPABILITY sense — every formula-engine deferred item from ADR-0015 has shipped. **Cartridge migration to use the new capabilities is a separate concern**, tracked per-cartridge by the cartridge maintainer (in this case, the email-matchback team). The Phase 3 retrospective document should make this distinction explicit so future readers don't conflate "shipped capability" with "shipped migration."

---

## PM-accepted disposition

Per Claude Desktop's "skip GPT/Desktop review, accept-as-is with the one small amendment" recommendation. PM concurs:

- The split from 3H.1 was the right call (validated by 3H.1 shipping cleanly with no SPEC QUESTIONs).
- Decision 7 (eval pipeline order) follows MMM convention exactly; no novel choices.
- Decision 8 inherits known debt per established Amendment §12 precedent; Amendment §11 above tracks the cumulative position.
- Decision 5 (Hill + Log only in v1) is the right conservative cut; Root/Exp deferred to demand-driven amendments.
- Decision 3 (Null treated as 0 in adstock) is a deliberate exception to Mosaic's Null-propagation; the implementer must document this loudly in the implementation per Desktop's flagging.

Next: handoff drafts immediately at `docs/handoffs/phase-3h-2-adstock-saturation-handoff.md`. After 3H.2 ships, write a Phase 3 retrospective document at `docs/reports/phase-3-retrospective.md` (Desktop's recommendation; closes the 11-sub-phase arc cleanly with a higher-level "what the formula engine became and why" piece).

---

## Notes

**Phase 3 arc summary (post-3H.2):**

After Phase 3H.2 ships, the formula-engine deferred queue from ADR-0015 is **EMPTY**. Phase 3 transitions to "demand-driven only" — future formula or fitted-model additions require a real customer use case, not speculative work. The arc that started at Phase 3A (model definition layer) closes at Phase 3H.2 across 12 sub-phases:

3A → 3B → 3C → 3D → 3E-3G (bundled) → 3F.1 (bundled) → 3H → 3I → 3J → 3H.1 → **3H.2**

Test count projection: ~880 (post-3H.1) → ~920+ post-3H.2 (depending on integration test density).

The next phase pivots away from formula-engine work entirely. Candidates documented in MASTER_PHASE_PLAN.md:
- **Phase 4C** — multi-domain workspace primitive (per the proposal in `docs/research-notes/multi-domain-workspaces-proposal.md`)
- **Phase 5D** — Tessera xlsx + group_by + multi-file ingest
- **Phase 6B** — web UI (the prototype at `docs/prototypes/mosaic-grid-prototype.html` is a starting point)
- **Phase 6C** — distribution + install pipeline (cargo-dist + Homebrew + curl installer)

Strategic implication: with Phase 3 closed, the next decision is "which surface area gets focus next" — modeling depth (3J.1+ if demanded), platform breadth (4C workspaces), data ingestion (5D), or user-facing surface (6B/6C).
