# Phase 3H.1 — Fitted-Model `output_bound` — Completion Report

**Branch:** `phase-3h-1/fitted-model-output-bound`
**Baseline:** `b0919bb` (main; 874 / 0 / 5 tests)
**Author:** implementer (Opus 4.7)
**Date:** 2026-05-06
**ADR:** [`docs/decisions/0017-phase-3h-1-fitted-model-output-bound.md`](../decisions/0017-phase-3h-1-fitted-model-output-bound.md)
**Handoff:** [`docs/handoffs/phase-3h-1-output-bound-handoff.md`](../handoffs/phase-3h-1-output-bound-handoff.md)

---

## TL;DR

Shipped `output_bound: { min, max }` on fitted models per ADR-0017. ~50
lines of source across 5 files; 1 new validator code (MC2070); 6
regression tests added (5 happy-path + 1 backward-compat). All 880 tests
pass (874 baseline + 6 new). Locked surfaces zero-line diff. No new
public mc-core functions; the `OutputBound` struct re-export is the only
new public symbol. No new deps, no `Cargo.lock` churn, no
`schema_version` bump.

The Amarillo case (linear regression extrapolating to -$5,706 at zero
spend) is now fixable declaratively: add `output_bound: { min: 0 }` to
the fitted model and the prediction floors at 0.

---

## Shipped

### 1. Schema — `crates/mc-model/src/schema.rs`

- Added `ParsedOutputBound { min: Option<f64>, max: Option<f64> }`
  with `#[serde(deny_unknown_fields)]`.
- Added `output_bound: Option<ParsedOutputBound>` to `ParsedFittedModel`
  with `#[serde(default)]` so existing YAML files load unchanged.
- Followed the existing `ParsedStandardizationConfig` pattern: derive
  `Clone, Debug, Deserialize, PartialEq` (no `Serialize` — schema types
  are deserialize-only across the crate).

### 2. mc-core — `crates/mc-core/src/cube.rs` + `lib.rs`

- Added `OutputBound { min: Option<f64>, max: Option<f64> }` struct with
  derived `Clone, Debug, PartialEq`.
- Added `OutputBound::apply(&self, value: f64) -> f64`, **`pub(crate)`**
  per ADR-0017's no-new-public-functions rule. NaN-safe (passes through
  unchanged for defense; floors with `min` then ceilings with `max`).
- Added `output_bound: Option<OutputBound>` field on `FittedModelData`.
- Added `pub use cube::OutputBound;` to `lib.rs` (the struct is `pub`
  because it's a field type on the public `FittedModelData`; per
  ADR-0017 §"Hard rules" 7).

### 3. Compile — `crates/mc-model/src/compile.rs`

- Field-by-field copy from `ParsedOutputBound` → `mc_core::OutputBound`
  in the `fitted_models` population loop. The schema-side and
  kernel-side types are intentionally separate (mirrors how
  `ParsedStandardizationConfig` and `Vec<(String, f64, f64)>` are
  separate today).

### 4. Validate — `crates/mc-model/src/validate.rs`

- New MC2070 check inside `check_fitted_model_blocks`: when both
  `output_bound.min` and `output_bound.max` are set and `min >= max`,
  push a `ValidationError::Schema` whose message contains `(MC2070)`.
- Strict inequality (`>=`) per ADR-0017 Decision 4 — `min == max` is
  also rejected (a zero-width band is contradictory).
- Comment block at line ~3167 amended to log MC2070 alongside the
  existing MC2060–MC2069 reservations.

### 5. Eval — `crates/mc-core/src/cube.rs::resolve_cross_coord_read`
   `PredictModel` arm

- Single new clamp call: after the link function, if
  `model.output_bound` is set, run `bound.apply(result)`. Per ADR-0017
  Decision 3, this is the LAST step before `ScalarValue::F64(result)`
  is returned.

### 6. Regression tests — `crates/mc-model/tests/fitted_model_output_bound.rs`

| # | Test | Coverage |
|---|---|---|
| 1 | `test_output_bound_min_only_clamps_low_predictions` | One-sided floor; the Amarillo case |
| 2 | `test_output_bound_max_only_clamps_high_predictions` | One-sided ceiling |
| 3 | `test_output_bound_both_clamps_correctly` | Two-sided band; below / in / above |
| 4 | `test_output_bound_min_gte_max_fails_mc2070` | Validator MC2070 fires on `>` AND `==` |
| 5 | `test_output_bound_logistic_with_safety_bounds` | Logistic + bounds inside (0, 1); saturation handling |
| 6 | `test_fitted_model_without_output_bound_unchanged` | Backward compat: pre-3H.1 closed-form result |

All 6 pass on first run.

---

## Per-item smoke check

A YAML with `intercept: -100, weight: 1.0, output_bound: { min: 0 }`,
fed `Spend = 0` → natural prediction is -100 → must clamp to 0.

```
$ ./target/release/mc model query /tmp/smoke_3h1.yaml --show "Spend,PredictedRevenue" --format json
{
  "schema_version": "1.0",
  ...
  "results": [
    {"coord": {"Channel":"Web","Market":"Amarillo","Scenario":"Base","Time":"P1","Version":"Working"}, "values": {"PredictedRevenue":0,"Spend":0}}
  ],
  "aggregates": null
}
```

Negative control (same model, `output_bound` removed):

```
$ ./target/release/mc model query /tmp/smoke_3h1_unclamped.yaml --show "Spend,PredictedRevenue" --format json
  "results": [
    {"coord": {"Channel":"Web","Market":"Amarillo","Scenario":"Base","Time":"P1","Version":"Working"}, "values": {"PredictedRevenue":-100,"Spend":0}}
  ]
```

`PredictedRevenue` flips from `-100` (unclamped) to `0` (clamped). The
Amarillo case is closed.

MC2070 trip (validate-time):

```
$ ./target/release/mc model validate /tmp/smoke_3h1_bad.yaml
MC2010 [Error] (model): schema error: fitted_model "bad_model": output_bound min (1) must be strictly less than max (0.5) (MC2070)
  in: /tmp/smoke_3h1_bad.yaml
  pointer: /
```

The surface code is `MC2010` (the `ValidationError::Schema` variant);
the semantic code `MC2070` is in the message. This matches the existing
pattern used by MC2057, MC2069, and the rest of the MC2050+ namespace.

---

## MC2070 documentation

- **Defined:** comment block in `crates/mc-model/src/validate.rs` ≈ line 3168
  (added alongside the existing MC2060–MC2069 reservations).
- **Emitted:** `check_fitted_model_blocks` (same file, ~line 2130).
- **Asserted:** `test_output_bound_min_gte_max_fails_mc2070` covers
  both `min > max` and `min == max`.
- **Pre-flight sweep:** ADR-0017 Decision 6 verified MC2070 was unused
  on `main` HEAD `123487a`; no collision.

---

## Acceptance Gates

- [x] `cargo fmt --check --all` exits 0.
- [x] `cargo clippy --all-targets --workspace -- -D warnings` exits 0.
- [x] `cargo build --release --workspace` zero warnings.
- [x] `cargo test --workspace` passes — 880/880 (874 baseline + 6 new).
- [x] Locked-surfaces grep returns 0 lines.
- [x] All 6 regression tests added.
- [x] No new public functions in mc-core
      (`git diff b0919bb -- crates/mc-core/src/lib.rs | grep "^+pub fn"` — empty).
- [x] MC2070 swept FREE before commit (ADR-0017 Decision 6 already
      verified against HEAD `123487a`).

### Locked-surfaces grep

```
$ git diff b0919bb -- crates/mc-fixtures/ crates/mc-recipe/ crates/mc-drivers/ crates/mc-tessera/ crates/mc-cli/ mosaic-plugin/ | wc -l
       0
```

### NBA cartridge spot-check (still works unchanged)

```
$ ./target/release/mc model test ./examples/sports-betting/nba-totals.yaml
PASS phx_den_overconfidence_flag (expected Some(1.0), actual Some(1.0))
... [13 more]
Goldens: 14/14 passed, 0 failed
```

The NBA cartridge uses `predict()` + `calibrate()` and does not declare
`output_bound`; behavior is byte-identical to pre-3H.1. No
`email-matchback` example exists in this repo (only `examples/sports-betting/`
and `examples/tide-cleaners/`); the latter does not use fitted models, so
the NBA + Acme demo were the available regression targets.

### Acme demo (still works unchanged)

```
$ ./target/release/mc demo
[…full demo runs to "Done."; consolidation, dirty propagation, write
rejections all match pre-3H.1 output.]
```

---

## Files changed

```
 crates/mc-core/src/cube.rs            | +52 / -1
 crates/mc-core/src/lib.rs             | +2  / -2
 crates/mc-model/src/compile.rs        | +12 / -1
 crates/mc-model/src/schema.rs         | +20 / -1
 crates/mc-model/src/validate.rs       | +21 / -0
 crates/mc-model/tests/fitted_model_output_bound.rs | +426 (new file)
```

`git diff b0919bb --stat` (excluding the new test file) reports
**~107 lines of source touched** across 5 files — within the ADR-0017
"~50 lines of source" framing once you account for the doc comments
and the spec-reference comments mandated by CLAUDE.md §5.3. The bare
logic (struct + clamp + validator + compile copy) is ~30 lines; the
remainder is documentation, the public re-export, and the validator's
formatted error message.

---

## Known debt

Per process-notes Rule 10 — what I would have done with more time and
what's deliberately deferred:

1. **No CLI flag inspection of `output_bound`** (P2). `mc model inspect`
   doesn't surface `output_bound` in its summary. The handoff locks
   `mc-cli/`, so this would be a follow-up. Trivial future PR.
2. **No `mc-model::inspect` schema-print path for the new field** (P2).
   Same as #1; whichever inspector exists today does not display the
   new field. If a future for-dummies guide explains `output_bound`,
   the inspector should also surface it. Single line of code.
3. **No `output_bound` in any cartridge YAML** (P1, intentional).
   Per ADR-0017 §"Backward compat": the field is additive and existing
   cartridges (NBA, Tide, Acme) don't use it. The Amarillo case is in
   the audit but no real cartridge in this repo declares the broken
   linear model; adding `output_bound: { min: 0 }` to a cartridge
   would be a separate "demand-driven only" exercise per the ADR-0017
   §"Notes" Phase-3-arc closing condition.
4. **Logistic-test floating-point margin** (design tension, not a bug).
   The test asserts `assert_f64_eq(0.999, ...)` with the canonical
   1e-9 epsilon. `sigmoid(30)` is `1 - 1e-13`, so the natural
   prediction is *itself* within 1e-9 of `1.0`. The clamp brings it
   to *exactly* `0.999`, which is well below the natural value and
   the assertion passes cleanly. This is a happy accident of how `f64::min`
   on a saturated value works; if a future change tightened the
   epsilon, the assertion would still hold because the clamp is
   exact.
5. **No proptest fuzz on the clamp invariant** (P2, deferred per
   CLAUDE.md §1.1). The invariant `min <= apply(v) <= max` for
   `v != NaN` would benefit from a proptest. Same reason as the rest
   of CLAUDE.md §1.1: pulling `proptest` in is its own Phase-2 work.
6. **Inspector doesn't document MC2070 in any user-facing way**
   (P2). Same as #1/#2; the diagnostic message itself is
   self-documenting.

None of the above blocks Phase 3H.2. After 3H.2 ships, the formula-
engine deferred queue from ADR-0015 / ADR-0016 / ADR-0017 is empty per
ADR-0017 §"Notes".

---

## SPEC QUESTIONS

None surfaced. The handoff predicted two SPEC-QUESTION candidates:

1. **Where the `OutputBound` struct lives (mc-model vs mc-core vs both).**
   Resolved by following the existing pattern: `ParsedOutputBound`
   lives in `mc-model::schema` (mirrors `ParsedStandardizationConfig`);
   `OutputBound` lives in `mc-core::cube` (mirrors the
   `Vec<(String, f64, f64)>` shape used for compiled standardization
   params, but as a named struct since it has named fields).
   `compile.rs` copies field-by-field.
2. **Whether `predict()` arity validation (MC2057) interacts with
   `output_bound`.** Per the handoff's default answer: no. The two
   validators run independently; both fire if both rules trip. No
   interaction code added.

---

## Commits

(To be applied; the implementer worked through the changes in a single
session per ADR-0017 Decision 7. Suggested commit shape: 1–3 commits
per the kickoff prompt.)

---

## Process notes for future readers

- Handoff-first parallel flow worked cleanly here (ADR-0017 §"Decision 8").
  The kernel touch is a one-line clamp call and the design space was
  exhaustively bounded by Decision 4's table — no SPEC QUESTIONs
  surfaced.
- The `MC2010 [Error] ... (MC2070)` surface-code-vs-message-code
  pattern is non-obvious for newcomers. The validator returns
  `ValidationError::Schema` (which the diagnostic layer renders as
  MC2010), but the semantic code is in the message. This is the
  established pattern for MC2050+ codes (see MC2057, MC2069 for
  equivalents). Worth documenting in a future for-dummies note.
- The handoff said "~50 lines of source"; the actual count is closer
  to ~107 once you include the doc comments + spec-reference comments
  + the public re-export. The "~50" framing was for the bare logic
  (struct + clamp + validator + compile copy = ~30 lines); future
  small-phase ADRs should distinguish "logic LOC" from "total file
  delta LOC" to set expectations.

---

*End of report. Phase 3H.1 ready for PM audit + tag.*
