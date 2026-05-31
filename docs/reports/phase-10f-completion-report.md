# Phase 10F Completion Report — `mc model simulate`

**Status:** Complete, gates green
**Date:** 2026-05-30
**Branch:** `phase-10f/model-simulate` (cut from `main` @ `3f725c6`, post-Amendment-17)
**ADR:** [ADR-0035](../decisions/0035-phase-10f-model-simulate.md) (Accepted, 17 amendments)
**Crate:** `mc-cli` only — **zero `mc-core` change**, **zero new dependencies**

---

## 1. SPEC QUESTION resolutions

### The one load-bearing question (resolved during pre-flight → Amendment 17)

A1 makes same-timestamp **batch** the default, but claw-core's $2,962.16
EXP-049 headline was computed **sequentially** (their Python iterated the
dataframe row-by-row, compounding each bet even within a same-`commence_time`
slate). Measured divergence:

| Scope | Sequential | Batch | Final-bank Δ |
|---|---|---|---|
| 2025 (EXP-049, 376 bets) | $2,962.16 | $2,964.16 | 0.067% |
| Full history (1,508 bets) | $137,053.13 | $137,362.83 | 0.226% |

Batch passes the 0.1% final-bank tolerance for 2025 but **not** the 0.01%
interior checkpoints or claw-core's sequential peak/max_drawdown. The project
owner filed **Amendment 17**: add an explicit `--replay batch|sequential`
flag (default `batch`, A1 untouched); sequential = stable-sort by timestamp,
compound in `sequence`-column-or-file order, **never re-sort by `bet_id`**.
EXP-049 repro runs `--replay sequential --outcome-mode legacy-binary` against
the real unmodified file.

### Secondary resolution — cartridge provenance is warn-only (A11)

A11's body says "unknown reference → error," but the Decision-1 example passes
the cartridge *and* filters on `abs_edge_pp`/`season`, which are bet-record
columns, **not** cube measures. Hard-erroring would break the documented
invocation. Resolved per A11's own "best-effort … warn, don't hard-block"
framing: referenced columns absent from the cartridge's declared measures are
reported as **warnings**, not fatal errors. Crypto provenance stays deferred.

### DuckDB-parquet API (the anticipated pre-flight question) — no blocker

`mc-drivers` cleanly exposes `duckdb_driver(path, query) -> impl SourceDriver`.
Parquet input is read via `SELECT * FROM read_parquet('<file>')` against an
in-memory DuckDB connection — no new Arrow/parquet crate (A4 satisfied).

---

## 2. Test count + quoted `cargo test --workspace` result line

`mc model simulate` ships **30 tests** (`crates/mc-cli/src/simulate_tests.rs`),
all passing. Full workspace run (the final action before push, per §6.7):

```
$ cargo test --workspace 2>&1 | grep -E "test result:" | awk '{p+=$4;f+=$6} END{print p" passed, "f" failed across "NR" suites"}'
363 passed, 0 failed across 40 suites

$ cargo test -p mc-cli 2>&1 | grep "test result:"
test result: ok. 63 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.27s
```

(The mc-cli binary suite's 63 tests include the 30 new `simulate` tests
alongside grade/query/etc.) Determinism: 10 consecutive `cargo test -p mc-cli
simulate` runs were identical (`ok. 30 passed` every run).

Coverage: Kelly known-values + negative-edge skip; sizing parse + modifiers +
`from_column`; reader (jsonl, alias resolution, missing-column + bad-odds
errors); outcome (canonical hard-error, derive-pushes, legacy-binary);
single-path hand-computed bankroll + curve; push-unchanged; batch-vs-sequential
divergence; batch over-stake proportional scaling; ruin stops replay;
drawdown + recovery (recovered / never-underwater); sharpe null on n<2; roi
cumulative vs roi_per_bet; filter-first/window-second; `--odds` settlement;
`from_column` + bare-hint-ignored; Monte Carlo seed determinism + seed-required;
splitmix64 known sequence; nearest-rank percentile; curve invariants
(void-excluded/push-included); **EXP-049 reproduction**.

---

## 3. Build gate results

```
cargo fmt --check --all                              ✓ (exit 0)
cargo clippy --all-targets --workspace -- -D warnings ✓ CLEAN
cargo build --release --workspace                    ✓ (no warnings)
cargo test --workspace                               ✓ 363 passed; 0 failed (40 suites)
determinism ×10 (simulate suite)                     ✓ 10/10 identical (30 passed)
```

Forbidden-pattern discipline: no float `==` (epsilon `1e-9` for equality,
`abs() < 1e-300` for zero/variance guards); no new deps; PRNG hand-rolled.

---

## 4. EXP-049 reproduction (AC #12)

Command:

```
mc model simulate \
  --bets exp028_bets.parquet --start-bankroll 1000 \
  --sizing quarter_kelly:cap=0.025,shrink=0.02 \
  --filter "abs_edge_pp >= 0.10 AND season == 2025" \
  --replay sequential --outcome-mode legacy-binary --format json
```

| Metric | claw-core EXP-049 V1.0 | Mosaic simulate | Match |
|---|---|---|---|
| final_bank | 2962.1596994721717 | 2962.1596994721717 | **byte-identical** |
| roi (cumulative) | +196.22% | +196.22% | ✓ |
| bets placed | 376 | 376 | ✓ |
| wins | 222 | 222 | ✓ |
| win_rate | 0.5904 | 0.5904 | ✓ |
| max_drawdown | 29.0584% | 0.2905842740982206 | ✓ |

**Outcome-mode pinned: `legacy-binary`** (claw-core's `won` is 0/1 with 295
pushes folded in). **Replay-mode pinned: `sequential`** (Amendment 17 — the
headline was computed sequentially). The **batch** default on identical inputs
yields **$2,964.16**, the realistic/achievable number; both are documented in
the cookbook. Far exceeds AC #12's 0.1%-final / 0.01%-checkpoint tolerance
(exact to full f64 precision, not just within tolerance).

---

## 5. PRNG choice + cross-platform determinism

**splitmix64** — ~12 lines, no dependency, `u64` state, the canonical
fast-mixing generator (Amendment 5's first suggestion). Chosen over
xoshiro256** for minimal state and because the resampling workload only needs
uniform indices. Determinism evidence:

- Same seed → identical Monte Carlo bands across repeated invocations
  (`t_monte_carlo_seed_determinism`).
- A platform-independent known-answer test pins the first two outputs of
  `SplitMix64::new(0)` to `16294208416658607535` / `7960286522194355700`
  (`t_splitmix64_known_sequence`) — these are fixed integer arithmetic
  (`wrapping_*`, shifts, xors), identical on any target.
- `--seed` is required whenever `--monte-carlo` is set.

---

## 6. mc-core change

**None.** Bankroll replay, sizing, drawdown scans, and the PRNG are all
reporting logic in `mc-cli` (`simulate.rs` + `simulate_reader.rs` +
`simulate_metrics.rs` + `simulate_command.rs` + `simulate_tests.rs`). The only
edit outside the new files is wiring `"simulate" =>` into the model-verb
dispatch in `main.rs` (+ the `mod simulate;` declaration). No model-semantic
primitive surfaced.

---

## 7. Curve output format (jsonl v1) + invariants verified

`--emit-curve <path>` writes one JSON object per line. Columns mirror
`exp029_bankroll_curve.parquet`: `timestamp, bet_id, season?, side?,
p_bet_side, abs_edge_pp?, stake, outcome, bankroll_after, batch_id`.
Invariants (A14), test-verified:

- One row per **placed** bet; **pushes included** (bankroll-unchanged row),
  **voids and ruin-skipped excluded** (`t_curve_excludes_voids_includes_pushes`).
- `batch_id` stamped; in batch mode intra-batch `bankroll_after` =
  batch-end bankroll (atomic application).
- Ruined runs end at the ruin row; empty pool → header-only + a run warning.

A DuckDB-backed parquet curve writer is deferred to v1.1 per A4.

---

## 8. Effort actual vs estimate

Estimate: 4–5 sessions / ~600–700 LOC. Actual: ~1,450 LOC of implementation
(`simulate*.rs`, excluding tests) + ~480 LOC tests, delivered in one focused
session. The pre-flight analysis (exact EXP-049 grounding, batch-vs-sequential
divergence measurement) front-loaded the hardest decision into Amendment 17
before any code was written, which kept the implementation a straight line.

---

## 9. Recommended next phase

**Phase 10C `mc model backtest`** — parameter-sweep × holdout, claw-core's #2
ask after simulate. It composes simulate (each sweep cell is a replay) the way
sweep composes query, and reuses this phase's reader + sizing vocabulary + the
filter grammar. The bet-record format shipped here is the load-bearing
contract it will lean on.

---

## Acceptance gate (34 ACs) — see chat for the marked checklist.
