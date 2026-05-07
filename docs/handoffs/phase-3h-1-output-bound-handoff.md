# Phase 3H.1 Handoff — Fitted-Model `output_bound`

> **Audience:** the Claude Code instance that implements Phase 3H.1.
> **You inherit `main` at `123487a` (874 / 0 / 5 tests). You'll work on
> the branch `phase-3h-1/fitted-model-output-bound` — see process-notes
> §11 for the git workflow rule (single instance, sequential = branch
> but no worktree).**
>
> **This is a small phase (~50 lines, ~5 tests, 1 new MC code).**
> Adds `output_bound: { min, max }` to `ParsedFittedModel` and clamps
> `predict()` output to the configured range. Closes the Amarillo
> -$5,706 case from the post-6A audit (M-20). The ADR (0017) is
> small and was PM-accepted directly under handoff-first parallel
> flow per Rule 1 (kernel touch is one clamp call; design space
> bounded).
>
> **Hard rule:** Phase 3H.1 modifies only:
> - `crates/mc-model/src/schema.rs` (new `output_bound:` field on `ParsedFittedModel`)
> - `crates/mc-model/src/validate.rs` (new MC2070 check)
> - `crates/mc-model/src/compile.rs` (carry the bound from parsed → compiled)
> - `crates/mc-core/src/cube.rs` (single clamp at the eval site in `resolve_cross_coord_read`'s `PredictModel` arm)
> - `crates/mc-core/src/cube.rs` or wherever `FittedModelData` is defined (add `output_bound: Option<OutputBound>` field)
> - Tests: a new file `crates/mc-model/tests/fitted_model_output_bound.rs`
>
> All other crates are locked. NO mc-cli changes. NO new public mc-core functions.
>
> **Process directive:** commit-as-you-go pattern (same as Phase 3J Rule 11). For 3H.1's small scope, 1-3 commits is appropriate (`feat(3H.1): output_bound schema + validator`, `feat(3H.1): output_bound eval clamp`, `test(3H.1): regression suite` — or fewer if natural).

---

## The one paragraph you must internalize

7 bullet-points of work. Add an optional `output_bound: { min: <f64>?, max: <f64>? }` field to `ParsedFittedModel`. Carry it through compile to `FittedModelData`. Validate that `min < max` if both set (MC2070). At eval, clamp the prediction AFTER the link function. Five regression tests cover the happy path + edge cases. No schema_version bump (additive field). No new public functions in mc-core. No domain decisions to make beyond what ADR-0017 already locked. After this ships, only Phase 3H.2 (adstock + saturation, separate ADR-0018 to be drafted) remains in the formula-engine deferred queue.

---

## Production-quality framing

This is small enough that the production-quality discipline is "follow ADR-0017 verbatim and write good tests." No new design decisions; no SPEC QUESTION candidates expected. If you do hit something ambiguous, file a SPEC QUESTION using CLAUDE.md §11 format.

---

## The work (single cluster — no Decision Matrix needed)

### Schema (binding per ADR-0017 Decision 2)

Add to `ParsedFittedModel`:

```rust
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct OutputBound {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ParsedFittedModel {
    // ... existing fields ...

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_bound: Option<OutputBound>,
}
```

### Compile

Mirror to the kernel-side struct (`FittedModelData` or wherever the compiled fitted model lives):

```rust
pub struct FittedModelData {
    // ... existing fields ...
    pub output_bound: Option<OutputBound>,    // same shape; reuse type if possible
}
```

If `OutputBound` lives in mc-model, you'll need a separate copy in mc-core (the schema types and the kernel types don't share). Same shape, same fields. Compile copies field-by-field.

### Validator (binding per ADR-0017 Decision 4)

In `validate.rs::check_fitted_model_blocks` (or wherever fitted-model validation lives):

```rust
if let Some(bound) = &fm.output_bound {
    if let (Some(min), Some(max)) = (bound.min, bound.max) {
        if min >= max {
            errors.push(ValidationError::Schema {
                message: format!(
                    "fitted_model {:?} output_bound has min ({}) >= max ({}); \
                     min must be strictly less than max (MC2070)",
                    fm.name, min, max
                ),
                ..
            });
        }
    }
    // NaN/infinite values are caught by serde_yaml at parse time;
    // no additional validation needed here.
}
```

The `Display`/error-formatting machinery for MC2070 should match the existing MC2050-2069 patterns. Code should be `MC2070`.

### Eval (binding per ADR-0017 Decision 3)

In `crates/mc-core/src/cube.rs::resolve_cross_coord_read`'s `PredictModel` arm, after the existing link-function step:

```rust
// existing: linear combination
let mut prediction = model.intercept + sum_of_(coef * standardized_feature);

// existing: link function
let prediction = match model.method.as_str() {
    "logistic" => 1.0 / (1.0 + (-prediction).exp()),
    _ => prediction,  // linear default
};

// NEW: output_bound clamp
let prediction = match &model.output_bound {
    Some(bound) => apply_clamp(prediction, bound),
    None => prediction,
};

ScalarValue::F64(prediction)
```

`apply_clamp` is a helper:

```rust
fn apply_clamp(value: f64, bound: &OutputBound) -> f64 {
    if value.is_nan() {
        return value;  // NaN passes through unchanged (defense; should be impossible after Phase 6A.1 NaN-rejection)
    }
    let mut v = value;
    if let Some(min) = bound.min { v = v.max(min); }
    if let Some(max) = bound.max { v = v.min(max); }
    v
}
```

The helper is `pub(crate)` (internal; not added to mc-core's public API per ADR-0017's no-new-public-functions rule).

### Regression tests (5 minimum required)

In a new file `crates/mc-model/tests/fitted_model_output_bound.rs`:

1. `test_output_bound_min_only_clamps_low_predictions` — linear model with `min: 0`; feature input that produces a negative prediction; assert clamped to 0.
2. `test_output_bound_max_only_clamps_high_predictions` — linear model with `max: 1000`; feature input that produces 5000; assert clamped to 1000.
3. `test_output_bound_both_clamps_correctly` — both min + max set; predictions in / above / below the band.
4. `test_output_bound_min_gte_max_fails_mc2070` — model declares `min: 1.0, max: 0.5`; assert MC2070 at validate.
5. `test_output_bound_logistic_with_safety_bounds` — logistic model with `min: 0.001, max: 0.999`; verify a prediction that would naturally be very close to 1.0 gets clamped to 0.999.

Plus 1 backward-compat test:

6. `test_fitted_model_without_output_bound_unchanged` — load an existing fitted model without the field; verify behavior is identical to pre-3H.1 (Acme's fitted models, NBA cartridge's fitted models — pick one).

---

## Out of Scope (deferred to Phase 3H.2 / ADR-0018)

Per ADR-0017 Decision 1, do NOT implement any of these in 3H.1:

| Item | Why deferred | Future phase |
|---|---|---|
| Adstock transforms (geometric, Weibull) | Multiple design decisions; deserves dedicated ADR | Phase 3H.2 / ADR-0018 |
| Saturation transforms (Hill, log, root, etc.) | Same | Phase 3H.2 / ADR-0018 |
| Per-feature transforms | Same | Phase 3H.2 |
| Multi-step transform pipelines | Same | Phase 3H.2 |
| Cross-coord time-decayed feature evaluation | Adstock requires reading prior periods; kernel-adjacent | Phase 3H.2 |
| Computed bounds (formula-evaluated min/max) | Out of scope; bounds are literal f64 only | Future phase if demanded |
| Per-feature output bounds | Out of scope; one bound per fitted model | Future phase if demanded |

If you find yourself wanting to add any of these "while you're in the file" — resist. Each is its own scoping exercise.

---

## Hard Rules (binding)

1. **Locked surfaces (zero-line diff against `123487a`):**
   - `crates/mc-fixtures/`
   - `crates/mc-recipe/`
   - `crates/mc-drivers/`
   - `crates/mc-tessera/`
   - `mosaic-plugin/`
   - `crates/mc-cli/` (the entirety, including tests — Phase 3H.1 doesn't touch CLI)

2. **Allowed touch (binding scope):**
   - `crates/mc-model/src/{schema,validate,compile}.rs`
   - `crates/mc-model/tests/fitted_model_output_bound.rs` (new)
   - `crates/mc-core/src/cube.rs` (or wherever `FittedModelData` is defined and `resolve_cross_coord_read` lives — single clamp + helper)

3. **No new dependencies.**
4. **No `Cargo.lock` pin churn.**
5. **Toolchain stays Rust 1.78.**
6. **Backward compat:** every existing test passes. The Acme + NBA + email-matchback fitted models (none of which currently set `output_bound`) continue to evaluate identically.
7. **No new public functions in mc-core.** The `apply_clamp` helper is `pub(crate)`. The `OutputBound` struct in mc-core is `pub` (it's a field type on the public `FittedModelData`) but no new public functions.
8. **No schema_version bump** on the model envelope. The new field is additive; existing parsers see it as absent.

---

## Acceptance Gates (lean)

- [ ] `cargo fmt --check --all` exits 0.
- [ ] `cargo clippy --all-targets --workspace -- -D warnings` exits 0.
- [ ] `cargo build --release --workspace` zero warnings.
- [ ] `cargo test --workspace` passes (874 → expect ~+6 = ~880).
- [ ] Locked-surfaces grep returns 0 lines.
- [ ] All 6 regression tests added (5 from §"The work" + 1 backward-compat).
- [ ] No new public functions in mc-core (verify by `git diff 123487a -- crates/mc-core/src/lib.rs | grep "^+pub fn"` — should be empty).
- [ ] MC2070 swept FREE before commit (verify against current main — already done in ADR-0017 Decision 6).

Per-item smoke check (paste output in completion report):

- [ ] Author a YAML with a linear fitted model, `output_bound: { min: 0 }`, and a `predict()` rule that would naturally return -100 at some coord. Run `mc model query` at that coord; confirm it returns 0.0, not -100.

---

## Order of Operations

1. Read this handoff in full.
2. Read [`docs/decisions/0017-phase-3h-1-fitted-model-output-bound.md`](../decisions/0017-phase-3h-1-fitted-model-output-bound.md) — the binding ADR.
3. Skim [`docs/process-notes.md`](../process-notes.md) Rules 1, 3, 7, 10, 11.
4. Implement: schema → compile → validate → eval → tests.
5. Run gates. Write completion report at `docs/reports/phase-3h-1-completion-report.md`.
6. Stop. Do not push the branch. PM merges + tags + pushes after audit review.

---

## Completion Report Expectations

Per process-notes Rule 10. Same shape as 3J:
- **Shipped** — what landed for each step (schema, validate, compile, eval, tests).
- **Per-item smoke check output** — paste the MC query showing the clamp.
- **MC2070 documentation** — confirm code is in error.rs (or wherever) + emitted by validate.rs + asserted by at least one regression test.
- **Acceptance gates checklist.**
- **Known debt** — anything noticed but not fixed.
- **Locked surfaces grep** — paste output.
- **Email-matchback / NBA cartridge spot-check** — confirm those still work unchanged (load + validate + test).

---

## SPEC QUESTION Format

Same as before. Most likely SPEC QUESTION candidates in 3H.1:

- Where exactly the `OutputBound` struct should live (mc-model vs mc-core vs both, given the schema/kernel split). Pick whichever follows the existing `Standardization` / `FittedModelData` pattern.
- Whether the existing `predict()` arity validation (Phase 3I MC2057) should also check that `output_bound`'s presence makes sense for the method. **Default answer: no.** `output_bound` is independent of arity; both validators run; no interaction.

---

*End of handoff. Phase 3H.1 + Phase 3H.2 close the formula-engine deferred queue. After 3H.2 ships, Phase 3 is genuinely complete and the project pivots to Phase 4C / 5D / 6B / 6C.*
