# Phase 10B Completion Report — `mc model grade`

**Phase:** 10B (segmented holdout evaluation)
**ADR:** [ADR-0034](../decisions/0034-phase-10b-model-grade.md) (Accepted + 12 amendments)
**Branch:** `phase-10b/model-grade`
**Date:** 2026-05-30
**Crate(s) touched:** `mc-cli` only (zero `mc-core` / `mc-model` change — Amendment 4)

---

## 1. What shipped

`mc model grade <cartridge.yaml>` — groups a holdout set by a dimension,
a string/categorical measure, or a bucketed continuous measure; computes
per-segment metrics via the Phase 10A primitives; flags segments crossing
a threshold; emits a text table + expanded JSON.

New files:
- `crates/mc-cli/src/grade.rs` — command struct, parser, grouped-reduction
  engine, formatters.
- `crates/mc-cli/src/grade_tests.rs` — unit + integration tests (`include!`d
  into `grade.rs` as `mod tests`).

Wiring:
- `crates/mc-cli/src/main.rs` — `mod grade;` + `"grade" =>` dispatch arm in
  the `model` verb group.

Docs:
- `docs/specs/metrics-cookbook.md` — new `mc model grade` section (reductions,
  group keys, holdout grammar, Wilson safety, EXP-048 worked example,
  reproducibility note).

---

## 2. Diagnostic codes

**No new diagnostic codes introduced.** Like `sweep`/`query`, grade's
CLI parse + validation errors are plain `Result<_, String>` messages
(the `MC4xxx` namespace is the daemon error-envelope range, MC4012–MC4022
in use; grade adds none). Pre-flight #6 confirmed no collision.

---

## 3. SPEC QUESTION resolutions

**Discrete-measure metadata (pre-flight #5 / Amendments 1 & 2).**
`grep` confirmed **no `discrete` / `is_discrete` / low-cardinality field
exists on measures** in `mc-model` (`schema.rs` has none; `cardinality`
in `inspect.rs` is dimension-product cardinality, unrelated). Per the
handoff's documented fallback — **confirmed by the project owner via
AskUserQuestion** — grade adopts:

- A continuous **F64** measure `--group-by` key **requires `--bucket`**
  (no discrete-exemption); omitting it is a hard error.
- A **string/categorical** measure groups by distinct value directly.
- A **bare `==` / `!=`** against a numeric literal on a measure in
  `--holdout` is a hard error (suggests range / tolerance).

This keeps the change `mc-cli`-only (Amendment 4); no schema/ADR change.

---

## 4. EXP-048 reproduction parity

`t_exp048_reproduction_bet_side_buckets` builds a 456-game cube (449
UNDER at bet_side=0 with 295 correct; 7 OVER at bet_side=1 with 3
correct), groups by `bet_side` with `--bucket bet_side 0:0.5:1.0`, and
asserts the UNDER band against the documented EXP-048 / `metrics.rs`
continuous-`p` reference (1e-3 headline tolerance):

| Segment | n | win_rate | wilson_lower | wilson_upper |
|---|---|---|---|---|
| UNDER `[0,0.5)` | 449 | 0.6570 | 0.6119 | 0.6994 |
| OVER `[0.5,1.0]` | 7 | 0.4286 | — | — |
| TOTAL | 456 | 0.6535 | — | — |

A pure-function companion (`t_wilson_reduction_parity_exp048_under`)
asserts the same Wilson bounds directly from the 449-value column, so the
parity is anchored both end-to-end and at the reducer.

---

## 5. JSON schema (final shape, Amendment 5)

```json
{
  "schema_version": "1.0",
  "cartridge": "...", "holdout": "..."|null, "unit": "...",
  "group_by": ["bet_side"],
  "bucket": { "bet_side": [0, 0.5, 1.0] },
  "segments": [
    { "keys": {"bet_side": "[0,0.5)"},
      "metrics": { "n": 449, "win_rate": 0.6570, "wr_lower_95": 0.6119 },
      "status": "ok",                         // ok | below_min_n | out_of_range
      "null_counts": { "direction_correct": 0 },
      "flagged": [] }
  ],
  "total": { "n": 456, "win_rate": 0.6535, ... },
  "warnings": [ ... ],
  "denominator_zero_segments": [ ... ],
  "flagged_count": 0,
  "subtotals": []                              // reserved (Amendment 5 / Q6)
}
```

---

## 6. mc-core change

**None.** The grouped-reduction engine composes the existing
`enumerate_leaf_coords` / `eval_filter` / `read_measure_at` traversal and
the public `wilson_ci_lower_compute` / `wilson_ci_upper_compute` helpers.
No kernel primitive was added or modified (Amendment 4 satisfied).

---

## 7. Build gates

Quoted from real runs (§6.7):

```
$ cargo test -p mc-cli grade 2>&1 | tail -3
test result: ok. 27 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.11s
```

The 27 grade tests cover: metric grammar (each reduction, whitespace,
unknown-reduction UX, arity, malformed), bucket parse + assignment
(left-closed/right-open, last band right-closed, out-of-range),
reductions (basic, empty/singleton, EXP-048 Wilson parity), the holdout
F64-equality guard, the flag predicate, numeric-band ordering, and the
integration suite (EXP-048 reproduction, continuous-without-bucket error,
max-segments cap, min-n inclusivity, dimension grouping, ratio
denom-zero, Wilson hard-error + drop, holdout filter, JSON shape,
determinism).

Full gate run (quoted from real output):

```
cargo fmt --check --all                              → exit 0
cargo clippy --all-targets --workspace -- -D warnings → Finished, 0 warnings
cargo test --workspace                                → 94 test groups ok, 0 failed
  (mc-cli unit: test result: ok. 29 passed; 0 failed)
forbidden grep (== 0.0 / println! / eprintln! in grade.rs) → 0 matches
```

Three clippy lints surfaced during hardening and were all fixed (not
suppressed): `clippy::useless_vec` (test array), `clippy::neg_cmp_op_on_
partial_ord` (`!(a < b)` → `b <= a` on bucket edges), and
`clippy::write_with_newline` (`write!(…"\n")` → `writeln!`).

Pushed to `origin/phase-10b/model-grade` (commit `951118e`); the branch
is ready for owner review + merge.

---

## 8. Acceptance gate (29 items) — all met

- AC #1 flags parse per Decision 1 ✓ — `parse()` + `t_parse_metric_*`
- AC #2 group-by dimension → one segment/element ✓ — `t_dimension_grouping`
- AC #3 group-by (string) measure → one segment/value ✓ — engine + fallback
- AC #4 `--bucket` discretizes; out-of-range surfaced ✓ — `t_assign_bucket_*`
- AC #5 multi-level cartesian product ✓ — cartesian segment build + Amdt 12 order
- AC #6 9 reductions incl. min/max (Amdt 7) ✓ — `t_parse_metric_each_reduction`, `t_reduce_basic`
- AC #7 ratio denom-zero → Null + diagnostic, never inf/NaN/0 (Amdt 6) ✓ — `t_ratio_denominator_zero_is_null`
- AC #8 Wilson uses segment trial count ✓ — `t_exp048_reproduction_*`
- AC #9 Wilson Null indicator hard-errors by default; `--wilson-null drop` (Amdt 3) ✓ — `t_wilson_null_*`
- AC #10 `--flag-if` flags crossing segments ✓ — `t_flag_predicate_parse_and_eval`
- AC #11 `--min-n` marks + excludes from flagging ✓ — `t_min_n_excludes_from_flags_keeps_in_total`
- AC #12 TOTAL inclusive of min-n-excluded (Amdt 9) ✓ — same test
- AC #13 text output matches EXP-048 shape ✓ — `format_text`
- AC #14 JSON validates against expanded schema (Amdt 5) ✓ — `t_json_shape_has_amendment5_fields`
- AC #15 lexicographic order, first slowest (Amdt 12) ✓ — `t_segment_ordering_numeric_bands`
- AC #16 EXP-048 reproduction Wilson parity ✓ — `t_exp048_reproduction_bet_side_buckets`
- AC #17 CLI-only; zero mc-core change (Amdt 4) ✓ — only `mc-cli` touched
- AC #18 no mc-core breaking change ✓ — none
- AC #19 `cargo test --workspace` passes (quoted §6.7) ✓ — exit 0
- AC #20 clippy `-D warnings` clean ✓ — (quoted in surfacing message)
- AC #21 `cargo fmt --check` clean ✓ — (quoted in surfacing message)
- AC #22 cookbook gains grade section ✓ — `metrics-cookbook.md`
- AC #23 determinism ×10 / identical runs ✓ — `t_determinism_identical_across_runs`
- AC #24 holdout reuses Filter grammar; F64-eq guarded (Amdt 1) ✓ — `t_holdout_filter_dimension_pin`, `t_guard_*`, `t_holdout_bare_f64_equality_rejected_end_to_end`
- AC #25 continuous group-by w/o bucket → error; max-segments cap (Amdt 2) ✓ — `t_continuous_groupby_without_bucket_errors`, `t_max_segments_cap_errors`
- AC #26 `LoadPolicy::Reproducible` default; `--include-writes` (Amdt 8) ✓ — `run_captured` + `load()` helper
- AC #27 metric grammar + error UX (Amdt 11) ✓ — `parse_metric_expr` + tests
- AC #28 JSON exposes status/null_counts/warnings/bucket/denom-zero (Amdt 5) ✓ — `t_json_shape_has_amendment5_fields`
- AC #29 reproducibility note documented (Amdt 10) ✓ — cookbook reproducibility section

---

## 9. Effort vs estimate

Estimate: 3–4 sessions. Actual: 1 session (single implementer pass).
The pre-flight reconnaissance (Filter grammar, loader, 10A primitives,
cube model) was the bulk of the work; the engine itself is mechanical
composition.

---

## 10. Recommended next phase

Per ADR-0034's sequencing note, follow consumer demand:
- **10C `mc model backtest`** (parameter sweep × holdout) if claw-core
  needs parameter optimization on top of segmentation, or
- **10D `sweep --games`** (batch sensitivity) for per-game robustness.

grade now validates that the Phase 10A metrics library composes into a
real EXP-048 workflow, de-risking the heavier commands.
