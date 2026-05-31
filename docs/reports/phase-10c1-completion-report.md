# Phase 10C.1 Completion Report — `mc model backtest`

**Status:** Complete, ready for PM review
**Date:** 2026-05-31
**ADR:** [ADR-0036](../decisions/0036-phase-10c-model-backtest.md) (8 amendments, all honored)
**Spike gate:** [10C.0 GREEN](phase-10c0-spike-report.md) — confirmed zero kernel change
**Branch:** `phase-10c.1/model-backtest`
**Crate:** `mc-cli` only

---

## What shipped

`mc model backtest` — sweep one or more axes across a cartesian grid; at each
grid cell run the full `grade`-style holdout evaluation; report the metric
surface and flag the best cell by an objective. Multi-domain by mandate: the
engine ships no domain metric; every metric is a generic reduction over
author-named measures.

Three new files in `mc-cli`:
- `eval_common.rs` — the shared evaluation engine (lifted from grade, A8).
- `backtest.rs` — the command (parse, axis resolution, grid, objective, output).
- `backtest_tests.rs` — 18 tests (`include!`d into `backtest::tests`).

Plus: `grade.rs` rewired to call `eval_common`; `main.rs` dispatch;
`metrics-cookbook.md` backtest section; one test-import line in
`grade_tests.rs`.

---

## Gate (quoted, §6.7 — run as the last action before push)

```
$ cargo build --release --workspace
    Finished `release` profile [optimized] target(s)        (zero warnings)

$ cargo clippy --all-targets --workspace -- -D warnings
    Finished `dev` profile                                  (zero warnings)

$ cargo fmt --check --all
    (clean — exit 0)

$ cargo test --workspace      # aggregated across all suites
    TOTAL PASSED: 1332; failures across all suites: 0
```

The mc-cli binary suite (which contains grade + backtest unit tests) reports
`test result: ok. 87 passed; 0 failed` (69 grade + 18 backtest). No test is
`#[ignore]`d or skipped.

**Determinism:** `t_determinism_ten_runs` runs the backtest 10× and asserts
byte-identical JSON output; passes. The mc-cli bin suite was additionally run
3× consecutively with identical `87 passed; 0 failed` results.

---

## Acceptance criteria (ADR-0036 consolidated, 10C.1 portion)

| AC | Status | Evidence |
|---|---|---|
| #1 `param:` axis | ✅ | `resolve_axis` param branch; `t_param_recompute_via_command_moves_per_cell` |
| #2 `coef:` axis (absolute + Nx multiplier) | ✅ | `resolve_axis` coef branch; `t_coef_axis_absolute` |
| #3 `input:` axis (transient) | ✅ | `resolve_axis` input branch; `t_multi_axis_cartesian_size_and_order` (input axis) |
| #4 cartesian grid, fixed order (first slowest) | ✅ | `decode_cell`; `t_decode_cell_first_axis_slowest`, `t_multi_axis_cartesian_size_and_order` |
| #5 `--max-grid` hard-errors | ✅ | `grid_total` + check; `t_max_grid_hard_errors` |
| #6 full holdout eval per cell (= grade) | ✅ | `run_grid` calls `eval_common::evaluate`; same engine grade uses |
| #7 10 reductions incl. `rmse`; no hardcoded domain metric | ✅ | `eval_common::Reduction::Rmse`; `t_multidomain_forecasting_rmse_sweep` |
| #8 `--objective`/`--goal` flag best; omitted → no winner | ✅ | objective selection; `t_exp033_reproduction…`, `t_objective_minimize_picks_low` |
| #9 `--goal` defaults maximize; minimize works | ✅ | `Goal::Maximize` default; `t_objective_minimize_picks_low` |
| #10 `--simulate` NOT present (deferred A4) | ✅ | no `--simulate` flag anywhere in `backtest.rs` |
| #11 `--group-by`/`--bucket` compose | ✅ | passed through `EvalSpec`; `t_best_by_segment_per_group` (group+bucket) |
| #12 Reproducible default; overrides transient | ✅ | `LoadPolicy::Reproducible`; rollback_to per cell, cube never persisted |
| #13 text + JSON + `--emit-grid` jsonl | ✅ | `format_text`/`format_json`/`write_emit_grid`; JSON validated via `python -m json.tool` |
| #14 EXP-033 reproduction within tolerance | ✅ | `t_exp033_reproduction_threshold_roi_surface` (hand-computed roi surface + optimal threshold) |
| #15 **multi-domain test; zero betting vocab in engine** | ✅ | `t_multidomain_forecasting_rmse_sweep` (forecasting + rmse) |
| #16 determinism ×10 | ✅ | `t_determinism_ten_runs` |
| #17 **zero mc-core/mc-model change** | ✅ | diff is `mc-cli` + docs only; spike confirmed GREEN |
| #18 `cargo test --workspace` (quoted) | ✅ | 1332 passed; 0 failed (above) |
| #19 clippy `-D warnings` | ✅ | clean (above) |
| #20 `cargo fmt --check` | ✅ | clean (above) |
| #21 no float `==`; zero-checks `abs() < 1e-300` | ✅ | `ZERO_EPS` in eval_common; `is_better` uses `>`/`<`; objective ties via strict-improve |
| #22 cookbook: betting AND non-betting (rmse) | ✅ | metrics-cookbook.md §`mc model backtest` (Example A betting, Example B forecasting rmse) |
| #23 single-brace test YAML | ✅ | all fixtures use single-brace inline maps |
| #24 `values:[...]` axis + `--dry-run` | ✅ | `parse_points` list form; `t_parse_points_value_list`, `t_dry_run_prints_grid_without_eval` |
| #25 per-cell clean state; 2-axis independent | ✅ | rollback_to per cell; `t_param_recompute_via_command_moves_per_cell`, `t_multi_axis_cartesian_size_and_order` |
| #26 `--best-by total\|segment` | ✅ | `BestBy`; `t_best_by_segment_per_group` |
| #27 objective Null excluded / all-Null errors / ties→first | ✅ | `t_objective_all_null_hard_errors`, `t_objective_tie_breaks_to_first` |
| #28 "replaces 5–6 no-refit scripts"; variant: deferred | ✅ | cookbook + help text say 5–6; no `variant:` axis |
| #29 grade + backtest share `eval_common`; grade tests pass | ✅ | grade_cube wraps `eval_common::evaluate`; grade's full suite green post-extraction |

---

## EXP-033 reproduction parity

The betting fixture (`exp033_betting`) is a 5-bet cube with `edge`/`pnl`/`stake`
and a swept `param(edge_threshold)` gating `qualified = if(edge >=
param(edge_threshold), 1, 0)`. The edge-threshold → ROI surface is **hand-
computed ground truth** (claw-core's actual run was not available in-repo, so
parity is against the analytically-derived surface for this controlled
fixture — the EXP-033 *workflow* is faithfully reproduced):

| threshold | qualified n | ROI |
|---|---|---|
| 0.00 | 5 | 0.800000 |
| 0.05 | 4 | 1.250000 |
| 0.10 | 2 | 2.0 |

Optimal threshold = 0.10 (max ROI). `t_exp033_reproduction_threshold_roi_surface`
asserts every cell within `1e-6` and the flagged optimum.

## The multi-domain (non-betting) test — which domain

**Forecasting** (`t_multidomain_forecasting_rmse_sweep`). A `param(blend)`
weights a model forecast against a baseline; `squared_error =
(forecast - actual)^2`; the metric is `rmse=rmse(squared_error)`. Sweeping the
blend and minimizing RMSE recovers blend=1.0 (the model is perfect in the
fixture). RMSE correctness is asserted at three points (`sqrt(250)`,
`sqrt(62.5)`, `0`). The engine path carries **zero** betting vocabulary — the
identical command shape serves both domains. This is the spine of the ADR
(AC #15) and the marquee example of AC #22.

## eval_common extraction note (A8)

grade's metric grammar + reduction engine + Filter guard + bucket/group
resolution + the per-segment reduction core moved verbatim into
`eval_common.rs`. `grade_cube` is now a thin wrapper: it builds an `EvalSpec`,
calls `eval_common::evaluate`, then applies grade's `--flag-if` pass (the only
grade-specific eval step). The `rmse` reduction was added in `eval_common`, so
grade gains it for free. grade's full test suite passes unchanged against the
shared module (the tests reference the moved primitives directly from
`eval_common`; `grade_cube`, kept as the wrapper, satisfies the 14 call sites).
**Zero behavioral change to grade.**

## Confirmation: zero mc-core change held (AC #17)

The 10C.0 spike's GREEN verdict held end-to-end. The `param:` axis recomputes
via the existing `rollback_to` cache-bust — no kernel mutator was added.
`cube.reference_data.parameters` is mutated through its public field (the
spike finding), exactly as `sweep.rs::override_coefficient` mutates
`fitted_models`. The diff touches `crates/mc-cli/**` and `docs/**` only.

---

## Effort vs estimate

Estimate was 3–4 sessions / ~400–500 LOC. Actual: ~1 session;
`eval_common.rs` (~720) + `backtest.rs` (~900) + `backtest_tests.rs` (~470),
of which eval_common is largely lifted (not new) code. The spike having
de-risked the mechanism made the build a straight composition.

## Recommended next

- **10D** (`sweep --games` / batch cartridge sweeps) or **10E** (walk-forward
  refit — the Python-trains-Mosaic-evaluates seam) — both demand-driven.
- **Fast-follow if demand surfaces:** the `variant:` axis (sweep over pre-fit
  model artifacts, ADR-0036 Amendment 5) — unblocks EXP-021/026
  hyperparameter sweeps. Deferred from v1 as honest scope.
- `--simulate` objective (ADR-0036 Amendment 4, deferred) once a
  path-dependent-objective backtest is actually requested.
