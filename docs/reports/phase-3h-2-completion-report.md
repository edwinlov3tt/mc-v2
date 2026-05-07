# Phase 3H.2 — Completion Report (Fitted-Model Adstock + Saturation Transforms)

**Branch:** `phase-3h-2/adstock-saturation`
**Baseline:** `92c2a96` (ADR-0018 + handoff)
**Final HEAD:** `5335182`
**Test count:** 880 / 0 / 5 → **910 / 0 / 5** (+30 tests)
**Test net:** 0 failures across 5 commits
**Date:** 2026-05-06

> Phase 3H.2 closes the formula-engine deferred queue from ADR-0015.
> After this lands + tag, Phase 3 transitions to "demand-driven only"
> per ADR-0018 §"Phase 3 arc summary."

---

## Shipped

### Step 1 — `transforms:` block schema + compile + types
**Commit:** `133305c — feat(3H.2): transforms schema + compile + types`

- `crates/mc-model/src/schema.rs:898` — `transforms: Option<ParsedTransforms>` field on `ParsedFittedModel`
- `crates/mc-model/src/schema.rs:944` — `ParsedTransforms` (adstock + saturation Vecs, default empty)
- `crates/mc-model/src/schema.rs:957` — `ParsedAdstockSpec { feature, rate, max_lookback }`
- `crates/mc-model/src/schema.rs:967` — `ParsedSaturationSpec` enum tagged on `type:` (`hill` | `log`) with `deny_unknown_fields`
- `crates/mc-core/src/cube.rs:2629` — `transforms: Option<Transforms>` field on `FittedModelData`
- `crates/mc-core/src/cube.rs:2674` — kernel-side `Transforms` struct
- `crates/mc-core/src/cube.rs:2685` — kernel-side `AdstockSpec` struct
- `crates/mc-core/src/cube.rs:2697` — kernel-side `SaturationSpec` enum (`Hill { feature, alpha, gamma }` | `Log { feature, scale }`)
- `crates/mc-core/src/lib.rs:70` — re-export `AdstockSpec`, `SaturationSpec`, `Transforms` (3 new public types; zero new public functions)
- `crates/mc-model/src/compile.rs:402` — field-by-field copy from parsed → kernel side, mirroring the `OutputBound` precedent from 3H.1
- **Decisions hit:** Decision 4 (schema shape), Decision 9 (no schema_version bump — additive Option field)

### Step 2 — Validators MC2071-MC2077
**Commit:** `2f70f29 — feat(3H.2): adstock + saturation validators (MC2071-MC2077)`

- `crates/mc-model/src/validate.rs:2148` — adstock + saturation validator block in `check_fitted_model_blocks`
- `crates/mc-model/src/validate.rs:3284` — diagnostic-code catalogue updates for MC2071-MC2077
- **MC2077 disposition:** caught by `serde_yaml`'s tagged-enum dispatch at parse time as a `ParseError::Syntax`; no validate-time emitter ships in v1. Code stays reserved per process-notes §3 (retirement is forever).
- **Tests:** 14 new (MC2071–MC2076 each + MC2077 serde-catch + duplicate-spec batching + empty-block + cross-list + collision-free)
- **Decisions hit:** Decision 10 (diagnostic codes), Step 2 W1 (MC2077 empirical)

### Step 3 — Adstock backward-scan eval (cross-coord)
**Commit:** `bd5614d — feat(3H.2): adstock backward-scan + saturation eval (integrated pipeline)`

- `crates/mc-core/src/cube.rs:1577` — `Cube::apply_adstock` private helper. Doc comment surfaces **Decision 3 (Null-as-zero) LOUDLY** with explicit ADR + MMM-convention rationale, plus Decision 8 + Amendment §11 cross-coord dep-graph debt note.
- `crates/mc-core/src/cube.rs:1304` — adstock arm in PredictModel, applies before saturation per Decision 7
- **Tests:** 7 new adstock eval (steady-state geometric decay, t=0, Null prior treated as zero, max_lookback truncation, max_lookback over-cap, rate=0 sanity, high-rate long-tail)
- **Decisions hit:** Decision 2 (geometric formula), Decision 3 (Null-as-zero), Decision 7 (eval order), Decision 8 + Amendment §11 (inherited cross-coord debt)

### Step 4 — Hill + Log saturation + integrated pipeline
**Commit:** `bd5614d` (eval logic, atomic with Step 3) + `e660a64 — test(3H.2): hill + log saturation + integrated pipeline regressions` (regression tests)

- `crates/mc-core/src/cube.rs:2738` — `apply_hill_saturation(x, alpha, gamma)` `pub(crate)`
- `crates/mc-core/src/cube.rs:2755` — `apply_log_saturation(x, scale)` `pub(crate)`
- `crates/mc-core/src/cube.rs:1364` — saturation arm in PredictModel, applies after adstock and before standardization
- `crates/mc-core/src/cube.rs:1389` — restructured pipeline: feature → adstock → saturation → standardization → coefficient → sum + intercept → link → output_bound (Decision 7 binding order)
- **Tests:** 8 new (Hill basic / Log basic / Hill clamps negative / Log clamps negative / pipeline adstock → saturation / pipeline with standardization / pipeline logistic+output_bound / backward-compat)
- **Decisions hit:** Decision 5 (Hill + Log only in v1), Decision 7 (full pipeline order)

### Step 5 — Email-matchback re-survey + Tide-MMM-shaped integration test
**Commit:** `5335182 — test(3H.2): tide-MMM-shaped pipeline + email-matchback re-survey`

- `crates/mc-model/tests/fitted_model_adstock_saturation.rs:1023` — Tide-MMM-shaped end-to-end integration (`test_tide_mmm_adstock_saturation_pipeline`): 6 time periods, 2 markets, 1 spend with adstock + Hill + standardization + output_bound + 1 one-hot indicator. Verifies every Decision 7 stage in a realistic shape.
- **Email-matchback re-survey** documented in the test file's inline comment block (lines 1006-1022 of the test file). Key finding: the current Tide MMM (`~/Projects/email-matchback/models/tide-mmm.yaml`) does NOT use 3H.2's geometric adstock or Hill/Log saturation today. It uses an earlier architecture (`lag(AdSpend, 1)` + `rolling_avg(AdSpend, 3)` as Mosaic-native derived measures). The Python `prepare_mmm_inputs.py` (~100 lines) does data shaping (Plan→Actual mirror, carry-forward, indicator one-hots) — NOT adstock/saturation pre-processing. So ADR-0018 Decision 1's "M-14 Python residual closure" is **aspirational rather than commitment**: Phase 3H.2 ships the CAPABILITY, available to any future MMM that chooses to use it. The Tide MMM rewrite onto 3H.2 lives with the cartridge.

---

## Per-step smoke-check outputs (paste-targets)

### Step 1 smoke: YAML with `transforms:` validates clean
```
$ cargo test --package mc-model --test fitted_model_adstock_saturation \
    test_empty_transforms_block_is_permissive \
    test_same_feature_in_both_adstock_and_saturation_allowed
running 2 tests
test test_same_feature_in_both_adstock_and_saturation_allowed ... ok
test test_empty_transforms_block_is_permissive ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 27 filtered out
```

### Step 2 smoke: YAML with adstock for non-existent feature fires MC2071
```
$ cargo test --package mc-model --test fitted_model_adstock_saturation \
    test_mc2071_adstock_feature_not_in_coefficients
running 1 test
test test_mc2071_adstock_feature_not_in_coefficients ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 28 filtered out
```

### Step 3 smoke: adstock with `rate: 0.5, max_lookback: 3` matches the formula
```
$ cargo test --package mc-model --test fitted_model_adstock_saturation \
    test_adstock_geometric_decay_at_steady_state
running 1 test
test test_adstock_geometric_decay_at_steady_state ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 28 filtered out

# Verifies adstocked = 100 + 50 + 25 + 12.5 = 187.5 at P4 with feature=100
# at all 4 periods, rate=0.5, max_lookback=3.
```

### Step 4 smoke: Hill at `alpha=2, gamma=5000, x=5000` returns 0.5
```
$ cargo test --package mc-model --test fitted_model_adstock_saturation \
    test_hill_saturation_basic
running 1 test
test test_hill_saturation_basic ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 28 filtered out

# Includes assertions for x=gamma → 0.5, x=100*gamma → ~1, x=0 → 0.
```

### Step 5 smoke: end-to-end Tide-MMM-shaped fixture
```
$ cargo test --package mc-model --test fitted_model_adstock_saturation \
    test_tide_mmm_adstock_saturation_pipeline
running 1 test
test test_tide_mmm_adstock_saturation_pipeline ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 28 filtered out

# Houston P6 = 30100 (intercept + IsHouston coefficient cleanly isolated)
# Austin  P6 = 100   (intercept only; saturation lands at half-sat point)
# Austin  P1 = 0     (output_bound floor clips deeply negative pre-clip)
```

---

## Acceptance Gates (lean, per the handoff)

| Gate | Status |
|---|---|
| `cargo fmt --check --all` exits 0 | ✓ |
| `cargo clippy --all-targets --workspace -- -D warnings` exits 0 | ✓ |
| `cargo build --release --workspace` zero warnings | ✓ |
| `cargo test --workspace` passes | ✓ 910 / 0 / 5 |
| Locked-surfaces `git diff 92c2a96 -- ...` returns 0 lines | ✓ |
| All required regression tests added (~15-20) | ✓ +30 |
| No SPEC QUESTION drift | ✓ (none filed) |
| Per-cluster commits visible in `git log 92c2a96..HEAD` | ✓ 5 commits |
| All 6 active diagnostic codes (MC2071-MC2076) shipped + collision-free | ✓ |
| MC2077 reserved per ADR + process-notes §3 | ✓ |
| No new public functions in mc-core | ✓ |

### Per-cluster commit log
```
$ git log 92c2a96..HEAD --oneline
5335182 test(3H.2): tide-MMM-shaped pipeline + email-matchback re-survey
e660a64 test(3H.2): hill + log saturation + integrated pipeline regressions
bd5614d feat(3H.2): adstock backward-scan + saturation eval (integrated pipeline)
2f70f29 feat(3H.2): adstock + saturation validators (MC2071-MC2077)
133305c feat(3H.2): transforms schema + compile + types
```

5 commits on the branch, per-cluster. The handoff's suggested 5-commit shape was a guideline; cluster Eval (steps 3+4) bundled adstock + saturation eval into the same eval-site commit because they share the PredictModel arm in `cube.rs::resolve_cross_coord_read` and had to ship atomically to avoid a mid-cluster broken state. Step 4's regression tests landed in their own commit so the per-step test density is still attributable.

### Locked-surfaces grep
```
$ git diff 92c2a96 -- crates/mc-fixtures/ crates/mc-recipe/ crates/mc-drivers/ \
                       crates/mc-tessera/ crates/mc-cli/ mosaic-plugin/ | wc -l
0
```

### Diagnostic-code collision sweep
```
$ for code in MC2071 MC2072 MC2073 MC2074 MC2075 MC2076 MC2077; do
    count=$(grep -rh "$code" crates/mc-model/src/ crates/mc-core/src/ \
                              crates/mc-cli/src/ crates/mc-recipe/src/ \
                              crates/mc-tessera/src/ 2>/dev/null | wc -l | tr -d ' ')
    echo "$code: $count source occurrences (validate.rs emit + catalogue + helpers)"
  done
MC2071: 9
MC2072: 8
MC2073: 7
MC2074: 5
MC2075: 6
MC2076: 9
MC2077: 5
```

All occurrences are intentional (emitter strings + catalogue entries + the new `transforms` schema doc-comments referencing them). No prior-phase emitter accidentally produces a 207x code.

### New public mc-core types (the 4 transform-related types as fields on FittedModelData)
```
$ git diff 92c2a96 -- crates/mc-core/src/lib.rs
+    AdstockSpec, CalibrationMapData, Cube, CubeBuilder, FittedModelData, OutputBound,
+    ReferenceData, SaturationSpec, ThresholdBand, Transforms, WriteIntent, WritebackRequest,
+    WritebackResult,
```

3 new public types added: `Transforms` (struct), `AdstockSpec` (struct), `SaturationSpec` (enum with `Hill` and `Log` variants — the variants count as the "fourth" type from the handoff's perspective; `SaturationType` was unified into the enum's tag). **Zero new public functions in mc-core.** Helper functions (`apply_adstock` private method on `Cube`, `apply_hill_saturation` and `apply_log_saturation` free `pub(crate)` fns) are not exposed.

### Cross-coord dep-graph debt note (Amendment §11 confirmation)
The eval site's doc comment at `crates/mc-core/src/cube.rs:1577` (the `apply_adstock` helper) explicitly references **ADR-0018 Decision 8 + Amendment §11**, names the cumulative tracking obligation ("the fourth+ ADR to inherit this debt"), and points at `docs/research-notes/cross-coord-dep-graph.md` for the architectural questions. The fix-it phase is targeted "within the next 2 phase cycles after 3H.2" per the Amendment.

---

## Email-matchback re-survey (paste-target per handoff Step 5)

**Re-survey target:** `~/Projects/email-matchback/models/tide-mmm.yaml` and `~/Projects/email-matchback/scripts/mosaic/prepare_mmm_inputs.py`.

**Finding 1 — current Tide MMM does NOT use 3H.2 transforms.** The fitted model declares 6 features (`AdSpend`, `AdSpend_Lag1`, `AdSpend_Roll3`, `IsHouston`, `IsAustin`, `IsDenver`). `AdSpend_Lag1` and `AdSpend_Roll3` are Mosaic-native derived measures computed via `lag(AdSpend, 1)` and `rolling_avg(AdSpend, 3)` rules in the same YAML. This is an EARLIER architecture predating Phase 3H.2's `transforms:` block: each "transform-like" feature is declared as a separate Derived measure with an explicit Mosaic rule, then passed as a coefficient to `predict()`.

**Finding 2 — Python pre-processor does not do adstock or saturation.** `prepare_mmm_inputs.py` (101 lines, full file at `~/Projects/email-matchback/scripts/mosaic/prepare_mmm_inputs.py`) does:
- Mirror Plan AdSpend → Actual where Actual is missing
- Extend AdSpend to Nov/Dec at carry-forward rate per market
- Add market-indicator (`IsHouston`/`IsAustin`/`IsDenver`/`IsAmarillo`) one-hot rows at every Time leaf

None of these are adstock or saturation. The "carry-forward" is forecast-period extrapolation, not geometric adstock decay.

**Finding 3 — ADR-0018 Decision 1's "M-14 Python residual closure" is aspirational.** The ~80 lines of Python adstock+saturation pre-processing called out as the closure target do not exist in the current email-matchback codebase. Phase 3H.2 ships the CAPABILITY for native geometric adstock + Hill/Log saturation; the Tide MMM's hypothetical rewrite onto 3H.2 is cartridge-side work, not phase-side.

**Finding 4 — the new capability is correctly modelled by the integration test.** The `test_tide_mmm_adstock_saturation_pipeline` regression demonstrates the end-to-end pipeline (adstock → saturation → standardization → coefficient → sum + intercept → link → output_bound) on a Tide-MMM-shaped fixture. A future cartridge author rewriting the Tide MMM on top of 3H.2 would replace the `lag()` + `rolling_avg()` derived measures with a single `transforms:` block, refit the model, and gain a more compact YAML with the adstock + saturation parameters surfaced as data rather than as Mosaic rules.

---

## Test counts (workspace-wide)

| Phase | Passed | Failed | Ignored |
|---|---|---|---|
| Baseline `92c2a96` (post-3H.1) | 880 | 0 | 5 |
| After Step 1 (`133305c`) | 880 | 0 | 5 |
| After Step 2 (`2f70f29`) | 894 | 0 | 5 (+14 validator) |
| After Step 3+4 eval (`bd5614d`) | 901 | 0 | 5 (+7 adstock eval) |
| After Step 4 tests (`e660a64`) | 909 | 0 | 5 (+8 saturation/pipeline) |
| After Step 5 (`5335182` — final) | **910** | **0** | **5** (+1 Tide-MMM end-to-end) |

**Net 3H.2 contribution:** +30 tests across the 5 commits.

---

## Hard-rule confirmations

| Hard rule (from handoff) | Status |
|---|---|
| Locked surfaces unchanged | ✓ (`git diff 92c2a96 -- <locked>` = 0 lines) |
| No new dependencies | ✓ (no `Cargo.toml` runtime-dep changes) |
| No `Cargo.lock` pin churn | ✓ |
| Toolchain stays Rust 1.78 | ✓ (no `rust-toolchain.toml` change) |
| Backward compat: existing fitted models without `transforms:` evaluate identically | ✓ (every pre-3H.2 test passes; backward-compat regression `test_predict_without_transforms_unchanged` ships) |
| mc-core API: only new pub types as field types on `FittedModelData` | ✓ (`Transforms`, `AdstockSpec`, `SaturationSpec`) |
| Zero new public functions in mc-core | ✓ |
| No `schema_version` bump | ✓ (additive `Option<ParsedTransforms>`) |
| Decision 3 documented LOUDLY in eval site | ✓ (`apply_adstock` doc comment surfaces it as the **only** Phase 3 exception to Mosaic's Null-propagation discipline) |
| Per-cluster commit discipline | ✓ (5 commits incremental, no all-uncommitted-at-end) |

---

## KNOWN DEBT (per process-notes Rule 10)

These are surfaced for future maintainers; none are bugs in the shipped 3H.2 work.

### P0 — none.

### P1
- **Cross-coord dep-graph debt (inherited from prior phases).** Adstock makes Phase 3H.2 the fourth+ ADR to inherit the existing over-invalidation behavior on writes. Per ADR-0018 Amendment §11, the dedicated fix-it phase should be scoped within the next 2 phase cycles. Documented in `docs/research-notes/cross-coord-dep-graph.md`. **Correctness is preserved** via revision-bumping; the cost is purely write performance on time-heavy cubes.
- **`FittedModelData` clone in PredictModel arm.** The eval path now clones the entire `FittedModelData` at the top of the PredictModel arm to release the `&self` borrow before adstock's `read_inner` calls. For typical MMM models (5-20 features) this is < 1 KB and negligible; if Phase 4+ introduces models with hundreds of coefficients, the clone may show up in benchmarks. The fix would be to interior-mutate the reference_data store or split adstock state extraction from the model lookup.

### P2
- **Saturation `type:` unknown variant lands as parse-time syntax error, not as MC2077.** Reserved per the diagnostic-code-retirement-is-forever rule (process-notes §3). If a future authoring layer benefits from a stable validate-time code for this case, it would need a new MC code, not MC2077 (reserved code stays retired-style). The `ParseError::Syntax` shape today contains "unknown variant" + the bad string, which is LLM-friendly enough.
- **`build_saturation_cube` test helper is dead-code in Step 5.** Used by Steps 3-4 saturation tests; Step 5's Tide-MMM-shaped test inlines its YAML for the multi-feature shape. Not worth pruning since the helper documents the single-feature shape clearly.

### Trade-offs taken deliberately
- **Adstock is Time-axis only in v1.** ADR-0018 Decision 1 deferred spatial / channel-axis carryover to a future phase. The hard-coded `find_time_dimension_position()` lookup in the eval path encodes this assumption.
- **Adstock features must be measure-named.** When `apply_adstock` resolves `spec.feature` against the Measure dim by name and the lookup misses, the function returns the pre-evaluated current value with no carryover. This is defensive degradation rather than a hard error. If a future authoring layer wants strict validation that `coefficients[i].feature` is a real measure name, that would be a new MC code (orthogonal to 3H.2).
- **Cluster Eval steps 3+4 shared one commit.** Adstock and saturation eval both restructured the same `PredictModel` arm; splitting them into two commits would have left a mid-cluster broken state where saturation was wired but unused, or vice versa. The regression tests for Step 4 landed in a separate commit (`e660a64`) so per-step test density is still attributable in `git log`.

### What I would have done with more time
- Add a benchmark for `predict()` with adstock declared, to measure the cross-coord overhead vs same-coord predict(). The Phase 1B benches at PERF.md don't cover this path. Justified for the cross-coord dep-graph fix-it phase as a baseline.
- Survey other cartridges (NBA, sports betting) for `predict()` usage and document whether any plausible v1 reach could simplify by adopting `transforms:`.
- Write a "for-dummies" doc explaining MMM adstock + saturation in non-statistical terms; this is the kind of capability that benefits from authoring-time guidance.

---

## What ships

5 commits on `phase-3h-2/adstock-saturation`. Branch is ready for PM review + merge + tag. **Do not push** per the handoff's order-of-operations rule.

Phase 3 closes after this lands. The next phase decision (4C / 5D / 6B / 6C) is the project's pivot moment per ADR-0018 §"Phase 3 arc summary."
