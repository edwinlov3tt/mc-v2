# Phase 3H.2 Handoff — Fitted-Model Adstock + Saturation Transforms

> **Audience:** the Claude Code instance that implements Phase 3H.2.
> **You inherit `main` at `98746d5` (880 / 0 / 5 tests). You'll work on
> the branch `phase-3h-2/adstock-saturation` — see process-notes §11
> for the git workflow rule (single instance, sequential = branch
> but no worktree).**
>
> **This is the FINAL formula-engine phase.** After Phase 3H.2 ships,
> the formula-engine deferred queue from ADR-0015 is **empty**. Phase
> 3 closes after 11 sub-phases. The formula language is complete
> enough to express marketing-mix, finance, sales, demand, sports
> betting, and stock forecasting cartridges. Future formula or
> fitted-model work becomes demand-driven (real customer hits a gap
> → ADR → ship).
>
> **Hard rule:** Phase 3H.2 modifies only:
> - `crates/mc-model/src/{schema, validate, compile}.rs` — new
>   `transforms:` block on `ParsedFittedModel`
> - `crates/mc-core/src/cube.rs` — adstock backward-scan + saturation
>   pipeline in `resolve_cross_coord_read`'s `PredictModel` arm; new
>   `Transforms`, `AdstockSpec`, `SaturationSpec`, `SaturationType`
>   types as fields on the existing public `FittedModelData`
> - `crates/mc-core/src/lib.rs` — re-export new pub types if needed
>   (per ADR-0017 precedent for `OutputBound`)
> - `crates/mc-model/tests/fitted_model_adstock_saturation.rs` (new)
>
> All other crates locked. NO mc-cli changes. NO new public functions
> in mc-core.
>
> **Process directive (per process-notes Rule 11):** commit AS YOU GO,
> per cluster. Suggested 5 commits per ADR-0018 Decision 11:
> - `feat(3H.2): transforms schema + compile + types`
> - `feat(3H.2): adstock + saturation validators (MC2071-MC2077)`
> - `feat(3H.2): adstock backward-scan eval (cross-coord)`
> - `feat(3H.2): hill + log saturation eval + integrated pipeline`
> - `test(3H.2): regression suite + email-matchback re-survey`
>
> Don't all-uncommitted-at-end (Phase 3I anti-pattern).

---

## The one paragraph you must internalize

The MMM convention is immovable: feature → adstock → saturation → standardization → coefficient → sum + intercept → link → output_bound. **Decision 7 in ADR-0018 binds this order.** Adstock requires backward scans through Time periods (cross-coord access from `predict()`); per Decision 8 + Amendment §11, this inherits the existing cross-coord dep-graph debt — DOCUMENT IT, don't try to fix it. **Decision 3's Null-as-zero exception is a deliberate departure from Mosaic's Null-propagation discipline** (the only such departure in Phase 3) — document it loudly in the eval site's doc comment. After this ships, write a Phase 3 retrospective per Desktop's recommendation; the deferred queue is empty.

---

## Production-quality framing

Same as 3J: this is a no-second-pass phase. ADR-0018 was reviewed and accepted with a single Amendment §11 (cumulative dep-graph debt tracking). The 12 binding decisions are locked. The 6 alternatives considered + rejected are NOT to be re-litigated.

If you hit a wall the per-cluster Decision Matrices below don't cover, file a SPEC QUESTION using CLAUDE.md §11. Don't guess.

The single highest-risk implementation site is the adstock backward-scan in `cube.rs` — it's the new cross-coord access path. Get the Null handling right (Decision 3); get the max_lookback truncation right (Decision 2); get the eval order right (Decision 7).

---

## Items (5 implementation steps spanning 2 conceptual clusters)

### Cluster Schema — Steps 1+2

#### Step 1 — `transforms:` block schema + compile + types

**Files:** `crates/mc-model/src/schema.rs`, `crates/mc-core/src/cube.rs` (or wherever `FittedModelData` lives), `crates/mc-model/src/compile.rs`.

**Schema additions to `ParsedFittedModel`:**

```rust
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct Transforms {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adstock: Vec<AdstockSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub saturation: Vec<SaturationSpec>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct AdstockSpec {
    pub feature: String,
    pub rate: f64,           // [0.0, 1.0] required
    pub max_lookback: u32,   // > 0 required
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SaturationSpec {
    Hill {
        feature: String,
        alpha: f64,    // > 0
        gamma: f64,    // > 0
    },
    Log {
        feature: String,
        scale: f64,    // > 0
    },
}

// On ParsedFittedModel:
#[serde(default, skip_serializing_if = "Option::is_none")]
pub transforms: Option<Transforms>,
```

**Mirror to `FittedModelData` in mc-core** (same shape; the schema/kernel split mirrors how `OutputBound` was handled in 3H.1).

**Compile pass-through:** copy parsed transforms field-by-field into `FittedModelData.transforms`.

**No schema_version bump** — the field is additive (`Option<Transforms>` defaults to None for existing models).

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: Where do `Transforms` / `AdstockSpec` / `SaturationSpec` live — mc-model only, mc-core only, or both? | **Both** (split per the existing `OutputBound` / `Standardization` pattern from 3H.1). mc-model has the schema deserialize types; mc-core has the kernel-internal compiled types. They're identical in shape; compile copies field-by-field. | Consistency with shipped 3H.1 split. |
| W2: `SaturationType` enum or untagged variants? | **Tagged enum** with `#[serde(tag = "type", rename_all = "snake_case")]`. The user writes `type: hill` or `type: log` in YAML. Clean parse errors for unknown types. | Matches Mosaic's existing tagged-enum patterns. |
| W3: Do `feature:` strings need pre-resolution to ElementId? | **Stay as strings until validate.** The validator (Step 2) cross-checks against `coefficients[].feature`. Eval (Step 3) looks up by string-equality (small lists; HashMap lookup is overkill). | Consistency with how `coefficients` already work. |
| W4: Should empty `transforms: {}` (no adstock or saturation) be allowed? | **Allowed.** Identical behavior to no transforms block. Don't over-validate. | Permissive; matches `Default` derive. |

#### Step 2 — Validators (MC2071–MC2077)

**File:** `crates/mc-model/src/validate.rs`.

7 new diagnostic codes per ADR-0018 Decision 10 + Amendment §11 audit-trail:

| Code | Trigger |
|---|---|
| MC2071 | `transforms.adstock` declares a feature not in `coefficients` |
| MC2072 | Hill saturation with `alpha <= 0` or `gamma <= 0` |
| MC2073 | Log saturation with `scale <= 0` |
| MC2074 | `transforms.saturation` declares a feature not in `coefficients` |
| MC2075 | Adstock `rate` outside `[0.0, 1.0]` |
| MC2076 | Adstock `max_lookback == 0` |
| MC2077 | Unknown saturation `type` (handled by serde — fires as a deserialization error / MC1xxx parse code; Decision 5 reservation may not be needed if serde catches first; PM check during implementation) |

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: MC2077 — does serde catch unknown variants before validate even runs? | **Probably yes** (with `#[serde(tag = "type")]` + concrete variants, an unknown type fires a parse-time deserialize error which becomes MC1001 or similar parse code). **Verify during Step 2 implementation.** If serde catches it, MC2077 stays unassigned (don't ship the code at all; reservation remains in ADR-0018 audit trail). If serde doesn't, ship MC2077 as planned. | Empirical decision; sweep before committing. |
| W2: Should validators emit one error per bad spec, or batch into one error per fitted model? | **One error per bad spec.** A model with 5 bad transforms produces 5 diagnostics. Matches existing validator patterns. | LLM-friendly. |
| W3: Same feature in both adstock + saturation specs? | **Allowed** (the common MMM use case is "spend gets both adstock AND saturation"). Don't validate uniqueness. | Domain correctness. |
| W4: Same feature listed twice in adstock specs? | **MC2071 extended OR new code.** The PM read this as: should error (probably MC2078 "duplicate adstock spec for feature"). **PM call during Step 2:** if you encounter this, file SPEC QUESTION; otherwise default to allowing it (last-wins, like coefficient ordering). | Edge case; surface if it bites. |

**Required tests (7+1 = 8):**
1-7. One per MC2071-MC2077 (skip MC2077 if serde catches it).
8. Pre-flight sweep verification regression test (`tests/diagnostic_codes.rs` or wherever code-uniqueness is tested) — confirm no overlaps.

---

### Cluster Eval — Steps 3+4

#### Step 3 — Adstock backward-scan eval (cross-coord)

**File:** `crates/mc-core/src/cube.rs::resolve_cross_coord_read`'s `PredictModel` arm.

**The math (Decision 2 binding):**

```
adstocked[t] = sum_{k=0}^{min(t, max_lookback)} (rate^k * feature[t-k])
```

**Implementation pattern:**

```rust
fn apply_adstock(
    feature_value_at_current_coord: f64,
    spec: &AdstockSpec,
    cube: &Cube,
    coord: &CellCoordinate,
    feature_measure_id: MeasureId,
    time_dim_idx: usize,
    refs: &Refs,
) -> Result<f64, EngineError> {
    let current_time_pos = coord.element_at(time_dim_idx).0 as usize;
    let mut adstocked = 0.0;
    let max_k = current_time_pos.min(spec.max_lookback as usize);

    for k in 0..=max_k {
        let prior_pos = current_time_pos - k;
        let prior_coord = coord.with_element_at(time_dim_idx, ElementId(prior_pos as u32));
        let prior_value = match cube.read(&prior_coord, refs)? {
            ScalarValue::F64(v) => v,
            ScalarValue::Null => 0.0,           // Decision 3: Null treated as 0
            other => return Err(...),           // Defense; shouldn't happen for fitted-model features
        };
        adstocked += spec.rate.powi(k as i32) * prior_value;
    }

    Ok(adstocked)
}
```

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1 (Decision 3 — load-bearing): Null at any prior period — propagate or treat as 0? | **Treat as 0.** Deliberate exception to Mosaic's Null-propagation. **Document loudly in the function doc comment** with reference to ADR-0018 Decision 3 + the MMM convention rationale. | Matches MMM domain expectation: "Null spend = no campaign that period = 0 contribution to adstock." |
| W2: Time dim element ordering — assume index = time order? | **Yes** (per existing Phase 3F `prev`/`lag` semantics). Element `ElementId(0)` is the earliest time period; ascending index = ascending time. | Consistency with shipped time-series functions. |
| W3: At t=0 (first time period), `max_k = 0`; loop runs once with `k=0`; result is just `feature[0]`. Correct? | **Yes.** Edge case handled by the loop bounds; no special case. | Self-consistent. |
| W4: max_lookback exceeds Time dim length? | **Silently truncate.** `current_time_pos.min(max_lookback)` handles it. Document. | No error needed; operationally common. |
| W5: Performance — every predict() call now does N reads through Time? | **Inherit cross-coord dep-graph debt per Decision 8 + Amendment §11.** Document in eval site doc comment + the cartridge READMEs. Performance fix is a future ADR (the dedicated cross-coord dep-graph fix-it phase). | Bounded scope. |
| W6: Reading prior coord's value — does this count as a cell read for dep-graph purposes? | **Yes** (every `cube.read(prior_coord, ...)` registers as a read). The existing dep-graph machinery captures these; the over-invalidation behavior is what's known-broken (per Amendment §11). | Don't bypass dep-graph; just inherit the debt. |

**Required tests (6 minimum):**
1. `test_adstock_geometric_decay_at_steady_state` — feature = 100 at all periods, rate = 0.5, max_lookback = 3 → adstocked = 100 + 50 + 25 + 12.5 = 187.5.
2. `test_adstock_at_first_time_period_returns_current_value`.
3. `test_adstock_with_null_prior_treats_as_zero` (Decision 3 — load-bearing).
4. `test_adstock_max_lookback_truncates_correctly`.
5. `test_adstock_max_lookback_exceeds_time_dim_length_silently_caps`.
6. `test_adstock_rate_zero_means_no_carryover` (rate=0 → adstocked = current value; sanity check the formula).

#### Step 4 — Hill + Log saturation eval + integrated pipeline

**File:** `crates/mc-core/src/cube.rs::resolve_cross_coord_read`'s `PredictModel` arm (continued).

**Saturation formulas (Decision 5 binding):**

```rust
fn hill(x: f64, alpha: f64, gamma: f64) -> f64 {
    if x.is_nan() || x.is_infinite() { return f64::NAN; }
    let x_alpha = x.max(0.0).powf(alpha);     // clamp to non-negative input
    let g_alpha = gamma.powf(alpha);
    x_alpha / (g_alpha + x_alpha)
}

fn log_saturation(x: f64, scale: f64) -> f64 {
    if x.is_nan() || x.is_infinite() { return f64::NAN; }
    (1.0 + x.max(0.0) / scale).ln()
}
```

**Integrated pipeline (Decision 7 binding):**

```rust
// In the PredictModel arm, after collecting feature values from call args:
let mut linear = model.intercept;
for (i, (feature_name, weight)) in model.coefficients.iter().enumerate() {
    let value = feature_values[i];

    // Step 1: adstock (if declared for this feature)
    let value = if let Some(spec) = adstock_for(feature_name, &model.transforms) {
        apply_adstock(value, spec, ..)?
    } else { value };

    // Step 2: saturation (if declared for this feature)
    let value = if let Some(spec) = saturation_for(feature_name, &model.transforms) {
        match spec {
            SaturationSpec::Hill { alpha, gamma, .. } => hill(value, *alpha, *gamma),
            SaturationSpec::Log { scale, .. } => log_saturation(value, *scale),
        }
    } else { value };

    // Step 3: standardization (Phase 6A.1; if declared for this feature)
    let value = if let Some((mean, std)) = standardization_for(feature_name, &model.standardization) {
        if *std > 0.0 { (value - mean) / std } else { value }
    } else { value };

    // Step 4: multiply by coefficient
    linear += weight * value;
}

// Step 5: link function (Phase 3H)
let prediction = match model.method.as_str() {
    "logistic" => 1.0 / (1.0 + (-linear).exp()),
    _ => linear,
};

// Step 6: output_bound (Phase 3H.1)
let prediction = match &model.output_bound {
    Some(bound) => bound.apply(prediction),
    None => prediction,
};

ScalarValue::F64(prediction)
```

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: Hill with `x = 0`? | **Returns 0.** `0^alpha = 0`, so numerator = 0; well-defined. | Mathematically clean. |
| W2: Negative `x` (after adstock)? | **Clamp to 0 before applying saturation** (`x.max(0.0)`). Negative spend is nonsensical for MMM; saturation curves are defined on x ≥ 0. | Defensive; documented in function comments. |
| W3: NaN or Infinite at any pipeline step? | **Pass through unchanged.** NaN never enters cube storage (Phase 6A.1 invariant); if it appears mid-pipeline, the Cube::write rejection catches it later. | Defense in depth. |
| W4: Standardization order — before or after saturation? | **AFTER saturation, BEFORE coefficient** per Decision 7 binding. Don't try to be clever. | MMM convention + ADR binding. |
| W5: Should the integrated pipeline be a single function or per-step helpers? | **Per-step helpers** (`apply_adstock`, `hill`, `log_saturation`) with the integrated pipeline as straight-line code in the `PredictModel` arm. Easier to read + test individually. | Maintainability. |

**Required tests (8 minimum):**
1. `test_hill_saturation_basic` — known input → known output (cite the formula in the test).
2. `test_log_saturation_basic`.
3. `test_hill_saturation_clamps_negative_to_zero`.
4. `test_log_saturation_clamps_negative_to_zero`.
5. `test_full_pipeline_adstock_then_saturation` — integration test with both transforms applied.
6. `test_full_pipeline_with_standardization` — confirm Phase 6A.1's standardization slots in correctly.
7. `test_full_pipeline_logistic_with_output_bound` — confirm Phase 3H.1's output_bound applies as the final step.
8. `test_predict_without_transforms_unchanged` — backward compat: a fitted model without `transforms:` evaluates identically to pre-3H.2.

---

### Step 5 — Email-matchback re-survey + integration tests

**Files:** new `crates/mc-model/tests/fitted_model_adstock_saturation.rs`; spot-check against `~/Projects/email-matchback/scripts/mosaic/prepare_mmm_inputs.py`.

After Steps 1-4 land, re-survey the email-matchback Tide MMM. Specifically:
- Read `prepare_mmm_inputs.py` (the ~80-line adstock+saturation pre-processor).
- Author a YAML fragment that declares the same transforms natively in `fitted_models:`.
- Verify the predicted values match (within float tolerance) what the Python pre-processor + raw `predict()` produces.

**The re-survey is a deliverable for the completion report** (per ADR-0018 Decision 12) — confirms the closure of M-14's MMM-related Python residual and validates that the design pipeline (Decision 7) matches real-world MMM convention.

**Required: at least 1 end-to-end integration test** (`test_tide_mmm_adstock_saturation_pipeline` or similar) that exercises a realistic adstock+saturation+standardization+output_bound combination on a Tide-MMM-shaped fixture. ~50-100 lines of test code.

---

## Out of Scope (deferred — DO NOT implement)

Per ADR-0018 Decision 1:

| Item | Why deferred | Future phase |
|---|---|---|
| Weibull adstock | v1 is geometric only | Phase 3H.3 amendment if demanded |
| Root / Exp / S-curve saturation | v1 is Hill + Log only | Phase 3H.3 amendment if demanded |
| Adstock+saturation as standalone formula functions (`adstock(measure, rate)`) | v1 is fitted-model-internal only | Future phase if demanded |
| Per-coefficient transform overrides | Transforms attach at model level | Future phase if demanded |
| Time-varying transform parameters | Constants only in v1 | Future phase if demanded |
| Cross-coord dep-graph performance fix | Inherited debt per Decision 8 + Amendment §11; tracked for a dedicated fix-it phase within the next 2 phase cycles | Separate phase ADR |
| Adstock for non-Time dimensions (spatial / channel-axis) | v1 is Time-axis only | Future phase if demanded |
| Computed transform parameters (formula-evaluated rate / alpha / etc.) | Literal f64 values only in v1 | Future phase if demanded |
| Calibration curves on adstock outputs (chained with `calibrate()`) | Orthogonal; out of scope | Future phase |

If you encounter any of these and feel the urge to "while I'm here, just add..." — **resist**. Each is its own scoping exercise.

---

## Hard Rules (binding)

1. **Locked surfaces (zero-line diff against `98746d5`):**
   - `crates/mc-fixtures/`
   - `crates/mc-recipe/`
   - `crates/mc-drivers/`
   - `crates/mc-tessera/`
   - `crates/mc-cli/` (the entirety, including tests)
   - `mosaic-plugin/`

2. **Allowed touch (binding scope):**
   - `crates/mc-model/src/{schema, validate, compile}.rs`
   - `crates/mc-model/tests/fitted_model_adstock_saturation.rs` (new file)
   - `crates/mc-core/src/cube.rs` — new types as fields on `FittedModelData`; new eval logic in `resolve_cross_coord_read`'s `PredictModel` arm; helper functions for adstock + saturation
   - `crates/mc-core/src/lib.rs` — re-export new pub types if needed (per `OutputBound` precedent from 3H.1)

3. **No new dependencies.**
4. **No `Cargo.lock` pin churn.**
5. **Toolchain stays Rust 1.78.**
6. **Backward compat:** every existing test passes (Acme + NBA + email-matchback fitted models without `transforms:` continue to evaluate identically).
7. **mc-core API surface adds ONLY new pub types** (`Transforms`, `AdstockSpec`, `SaturationSpec`, `SaturationType`) as field types on the existing public `FittedModelData`. **Zero new public functions.** Helper functions (`apply_adstock`, `hill`, `log_saturation`) are `pub(crate)` or private.
8. **Decision 3 — Null-as-zero in adstock — is a deliberate exception to Mosaic's Null-propagation discipline.** Document this LOUDLY in the eval site's doc comment with reference to ADR-0018 Decision 3 and the MMM convention rationale. Future readers will want to know this isn't a bug.
9. **Per-cluster commit discipline (Rule 11):** 5 commits expected per Decision 11. Don't all-uncommitted-at-end.

---

## Acceptance Gates (lean)

- [ ] `cargo fmt --check --all` exits 0.
- [ ] `cargo clippy --all-targets --workspace -- -D warnings` exits 0.
- [ ] `cargo build --release --workspace` zero warnings.
- [ ] `cargo test --workspace` passes (880 → expect ~+30 to ~+40 = ~910-920).
- [ ] Locked-surfaces grep returns 0 lines.
- [ ] All required regression tests added (6 adstock + 8 saturation/pipeline + 1+ integration = ~15-20 tests).
- [ ] No SPEC QUESTION drift (or every SPEC QUESTION resolved before merge).
- [ ] **Per-cluster commits visible** in `git log 98746d5..HEAD`.
- [ ] All 7 reserved diagnostic codes (MC2071-MC2077) shipped + collision-free (re-sweep against current main per Rule 3).
- [ ] No new public functions in mc-core (`git diff 98746d5 -- crates/mc-core/src/lib.rs | grep "^+pub fn"` empty).

Per-step smoke checks (paste each in completion report):
- [ ] **Step 1:** YAML with `transforms: { adstock: [...], saturation: [...] }` validates clean.
- [ ] **Step 2:** YAML with adstock for a non-existent feature fails MC2071 at validate.
- [ ] **Step 3:** Adstock with `rate: 0.5, max_lookback: 3` on a known feature value at multiple Time periods produces values matching the formula `sum_{k=0}^3 (0.5^k * feature[t-k])`.
- [ ] **Step 4:** Hill with `alpha=2, gamma=5000` on x=5000 returns 0.5 (the half-saturation point — sanity check).
- [ ] **Step 5:** End-to-end smoke (Tide-MMM-shaped fixture) — predict() with adstock+saturation+standardization+output_bound returns the expected aggregate prediction.

---

## Order of Operations

1. Read this handoff in full.
2. Read [`docs/decisions/0018-phase-3h-2-fitted-model-adstock-saturation.md`](../decisions/0018-phase-3h-2-fitted-model-adstock-saturation.md) — the binding ADR. Pay attention to:
   - Decision 7 (eval pipeline order — load-bearing)
   - Decision 3 (Null-as-zero — deliberate exception)
   - Decision 8 + Amendment §11 (cross-coord debt inheritance + cumulative tracking)
3. Skim [`docs/process-notes.md`](../process-notes.md) Rules 1, 3, 7, 10, 11.
4. Skim [`docs/research-notes/cross-coord-dep-graph.md`](../research-notes/cross-coord-dep-graph.md) for the inherited debt context (Amendment §11).
5. **Implementation order (binding per Decision 11):**
   - Step 1: schema additions + compile pass-through (1 commit).
   - Step 2: validators (1 commit covering MC2071-MC2077; verify MC2077 sweep).
   - Step 3: adstock backward-scan eval (1 commit; cross-coord access).
   - Step 4: hill + log saturation + integrated pipeline (1 commit).
   - Step 5: regression tests + email-matchback re-survey (1 commit).
6. **Commit per cluster.** 5 commits minimum on the branch by the time you're done.
7. Run gates after each commit.
8. Write the completion report at `docs/reports/phase-3h-2-completion-report.md`.
9. **Stop.** Do not push the branch. PM merges + tags + pushes after audit review.

---

## Completion Report Expectations

Per process-notes Rule 10:
- **Shipped** — what landed for each step with file:line citations.
- **Per-step smoke check outputs** — paste each command + actual output.
- **Email-matchback re-survey** (Step 5 deliverable) — paste the test that validates Mosaic's transforms produce the same predictions as the Python pre-processor.
- **All 7 reserved diagnostic codes shipped + collision-free** (re-sweep against current main per Rule 3 pattern).
- **List of new public mc-core types** (must be exactly the 4 transform-related types as fields on `FittedModelData`; ZERO new public functions).
- **Per-cluster commit log** — paste `git log 98746d5..HEAD` showing the 5+ commits.
- **Cross-coord dep-graph debt note** (Amendment §11) — confirm the eval site's doc comment references ADR-0018 Decision 8 + Amendment §11 and points at the cumulative tracking obligation.
- **KNOWN DEBT section** — anything noticed but not fixed (file follow-ups).
- **Locked surfaces grep** — paste output.

---

## SPEC QUESTION Format

Same as before (CLAUDE.md §11). Most likely candidates in 3H.2:

- Step 2 W1 (MC2077): does serde catch unknown saturation types as a parse error before validate runs? If yes, MC2077 stays unassigned. Verify and SPEC QUESTION if surprising.
- Step 2 W4: duplicate adstock spec for the same feature — error or last-wins? PM default is "allow last-wins" (matches coefficient ordering); SPEC QUESTION if you think otherwise.
- Step 3 W2: Time dim element ordering assumption — confirm Phase 3F `prev`/`lag` uses ElementId-as-time-index. If different (e.g., uses element ordering metadata), Step 3's adstock backward-scan needs the same pattern.
- Step 4 W4: standardization order — before saturation or after? ADR-0018 Decision 7 binds AFTER saturation, BEFORE coefficient. SPEC QUESTION if the existing standardization eval can't be cleanly slotted in there.

---

*End of handoff. Phase 3H.2 closes the formula-engine deferred queue. After this ships + tag + Phase 3 retrospective document writes (Desktop's recommendation), the Phase 3 area transitions to "demand-driven only." Next phase decisions (4C / 5D / 6B / 6C) become the project's focus.*
