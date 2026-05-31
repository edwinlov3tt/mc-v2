# Phase 10F Handoff — `mc model simulate` (Chronological Bankroll Simulation)

**Status:** Accepted, ready to start
**Date:** 2026-05-27
**ADR:** [ADR-0035](../decisions/0035-phase-10f-model-simulate.md) (Accepted with 16 acceptance amendments — read amendments BEFORE the body; amendments win on conflicts)
**Estimated effort:** 4–5 sessions (~600-700 LOC + tests) — the biggest evaluation-track command
**Crate:** `mc-cli` ONLY (no `mc-core` change unless a model-semantic primitive surfaces — default none); no daemon
**Branch:** `phase-10f/model-simulate`

---

## What this phase ships

`mc model simulate` — chronological bankroll replay. Consumes a
bet-record file (NOT the cube), sizes each bet via a Kelly vocabulary,
walks the bankroll forward in time order, computes
final_bank/roi/max_drawdown/recovery/sharpe, and optionally Monte Carlo
percentile bands. claw-core's #1 ask after adopting grade — the V1.1-
gating headline numbers ("$1k → $2,962, +196%") are path-dependent
bankroll numbers grade structurally cannot produce. Also finally homes
`max_drawdown`/`recovery_bets` (deferred from 10A).

---

## Required reading (in this order)

1. **ADR-0035 Amendments (CRITICAL — read first).** 16 binding amendments
   from dual review. They override the body. The ones that change
   implementation most:
   - **A1 (load-bearing)**: same-timestamp bets = SIMULTANEOUS BATCH by
     default (all stakes from bankroll-at-batch-start, outcomes atomic).
     **45% of claw-core's real bets share a timestamp** — this is not an
     edge case. Optional `sequence` column → sequential replay.
   - **A2**: 4-state outcome REQUIRED; binary 0/1 hard-errors unless
     `--outcome-mode legacy-binary`; `--derive-pushes` repair path.
   - **A3**: bankruptcy/ruin — cap stake at bankroll, never negative,
     ruin → skip remaining.
   - **A4**: parquet INPUT via existing DuckDB (mc-drivers, no new dep);
     jsonl curve OUTPUT in v1.
   - **A5**: pinned hand-rolled PRNG (splitmix64/xoshiro256\*\*), NOT the
     rand crate.
   - **A13**: `roi` = cumulative (final/start − 1); `roi_per_bet` separate.
   - **A17**: `--replay batch|sequential` flag (default batch = A1). The
     EXP-049 repro runs `--replay sequential --outcome-mode legacy-binary`
     to reproduce claw-core's $2,962.16 from their REAL file. Batch is the
     more-financially-honest default (no intra-slate compounding);
     sequential reproduces legacy headlines.
2. **ADR-0035 body** — the records-in pivot, 9 decisions (interpret
   through amendments).
3. **ADR-0034 (grade)** — the order-independent sibling. Same CLI
   structure, same filter-grammar question (A12 here), same float-`==`
   guard, same CLI-only/zero-mc-core discipline.
4. **ADR-0033 (10A)** — `max_drawdown`/`recovery_bets` deferred there,
   landing here; sharpe reuses sample-std (ddof=1).
5. **The real files (the format contract source):**
   - claw-core `training/mlb/artifacts/exp028_bets.parquet` — INPUT
     format (7,272 rows × 16 cols; `game_pk`/`commence_time`/`won`/
     `decimal_odds`/`p_bet_side`/`abs_edge_pp`/`side`/`season`/
     `actual_total`/`line`)
   - claw-core `training/mlb/artifacts/exp029_bankroll_curve.parquet` —
     OUTPUT curve format
6. **Code to reuse:**
   - `crates/mc-cli/src/grade.rs` — CLI structure, filter grammar
     decision, LoadPolicy::Reproducible
   - `crates/mc-cli/src/sweep.rs` — parse/run skeleton
   - `mc-drivers` — the DuckDB parquet-read path (A4)
7. **CLAUDE.md** — §2.5 (Null), §3.1 (NO float `==`), §4.5 (single-brace
   test YAML), §6 (gates), §6.7 (quote the real test run).

---

## Phase 10F scope

| # | Item |
|---|---|
| 1 | `crates/mc-cli/src/simulate.rs` (new) — command, parse, run |
| 2 | Wire `"simulate" =>` into main.rs model-verb dispatch |
| 3 | Bet-record reader: parquet (via DuckDB/mc-drivers) + jsonl + column aliasing + sidecar schema (A4, Decision 2) |
| 4 | Outcome normalization: 4-state enum + `--outcome-mode legacy-binary` + `--derive-pushes` (A2) |
| 5 | Sizing-rule parser + pinned Kelly formula + modifiers (Decision 4) + `from_column:stake_hint` (A8) |
| 6 | Same-timestamp batch grouping (A1) + `--replay batch\|sequential` flag (A17) — DEFAULT batch; sequential reproduces legacy headlines |
| 7 | Single-path replay + bankruptcy/ruin (A3) + filter-then-window order (A12) |
| 8 | Metrics incl. drawdown scans + edge cases (A7) + cumulative roi (A13) |
| 9 | Monte Carlo: pinned PRNG (A5) + iid/block resample (A6) |
| 10 | `--odds fixed:|column:` for sizing AND settlement (A9) |
| 11 | Output: text + expanded JSON (A16) + jsonl `--emit-curve` w/ invariants (A14) |
| 12 | Cartridge-optional column-name validation (A11) |
| 13 | Tests (reader, outcome, sizing, batch, replay, ruin, drawdown, monte-carlo determinism, EXP-049 repro) |
| 14 | metrics-cookbook.md `mc model simulate` section incl. bet-record format spec |

**Out of scope:** walk-forward / record generation (Phase 10E); daemon
endpoint; free-form sizing; portfolio/correlated Kelly (Phase 12B);
stochastic-odds slippage mode; cap-matrix sweeps; parquet curve output
(v1 is jsonl, A4); the `rand` crate (A5).

---

## Pre-flight checklist (report in chat before Step 1)

```bash
cd /Users/edwinlovettiii/Projects/mc-v2
git worktree add ../mc-v2-phase-10f -b phase-10f/model-simulate main
cd ../mc-v2-phase-10f

# 1. DuckDB parquet-read path (A4) — how does mc-drivers expose it?
grep -rn "duckdb\|read_parquet\|fn.*parquet" crates/mc-drivers/src/ | head -10
# Confirm: can simulate call a parquet→rows path through mc-drivers, or
# does it need a thin new reader? Report the API shape.

# 2. Confirm DuckDB pin (A4 grounding)
grep -n "duckdb" Cargo.toml crates/mc-cli/Cargo.toml

# 3. grade's filter grammar — reuse or adapt for flat record columns? (A12, Q3)
sed -n '413,460p' ../mc-v2/crates/mc-cli/src/query.rs 2>/dev/null || sed -n '413,460p' crates/mc-cli/src/query.rs
# Decision per A12/Desktop-Q3: SAME user-facing syntax, SEPARATE parser
# for flat record columns (don't couple to cube-measure Filter).

# 4. Inspect the real input file so the reader matches (run from claw-core)
python3 -c "import pandas as pd; df=pd.read_parquet('/Users/edwinlovettiii/Projects/claw-core/training/mlb/artifacts/exp028_bets.parquet'); print(df.dtypes); print('rows', len(df)); print('same-ts groups', (df.groupby('commence_time').size()>1).sum())"
# Expect: 7272 rows, ~1314 same-timestamp groups (45% of bets)

# 5. Diagnostic-code preflight (MC4xxx)
grep -RE "MC4[0-9]{3}" docs/ crates/ | tail -10

# 6. Clean tree
git status
```

The Step-1 DuckDB-API question is the one most likely to need a SPEC
QUESTION — if `mc-drivers` doesn't cleanly expose a "parquet file → Vec
of rows" path, surface it (don't add a new parquet crate per A4; the
fallback is jsonl-only for v1 + a note that parquet needs a Tessera
pre-convert).

---

## Implementation path

### Step 1: Bet-record reader (A4, Decision 2)
parquet (DuckDB via mc-drivers) + jsonl → normalized `Vec<BetRecord>`.
Column-name resolution: canonical → alias (`game_pk`→`bet_id`,
`commence_time`→`timestamp`, `won`→`outcome`) → `--columns` override →
sidecar `.schema.json`. Validation: required columns present, odds > 1,
p ∈ [0,1], types correct.

### Step 2: Outcome normalization (A2)
4-state enum parse. `--outcome-mode legacy-binary` accepts 0/1
(1→win/0→loss, stamps `outcome_mode` in output). `--derive-pushes
actual_total=line` reconstructs 4-state when both columns present.
**Canonical default: 4-state required; bare 0/1 hard-errors.**

### Step 3: Sizing + Kelly (Decision 4, A8)
`SizingRule` struct, parse `rule:param=val,...`. Pinned Kelly:
`b=d−1`, `f=(b·p−(1−p))/b` clamped `[0,∞)`, `stake_pct=min(F·f, cap)`.
Modifiers cap/shrink/min_odds/floor. `from_column:stake_hint` is an
EXPLICIT rule (A8) — bare stake_hint column ignored.

### Step 4: Same-timestamp batch grouping + `--replay` (A1 — load-bearing; A17)
Sort by timestamp (stable — preserve file order within a timestamp; do
NOT re-sort by `bet_id`, per A17). `--replay batch` (DEFAULT, A1): group
consecutive same-timestamp bets into batches — all stakes computed from
bankroll-at-batch-start, outcomes applied atomically, bankroll updated
once per batch; batch over-stake → scale stakes proportionally (A3).
`--replay sequential` (A17): compound each bet in order; intra-timestamp
order = `sequence` column if present, else stable file row order. The
`--replay` flag is the global toggle; the `sequence` column refines
intra-batch order under sequential. EXP-049 repro uses `--replay
sequential` (A17 + A15).

### Step 5: Single-path replay + ruin (A3, A12)
Filter FIRST, window SECOND (A12). Then: bankroll=start; for each batch
in time order: compute stakes (capped at bankroll), apply outcomes,
append curve rows (batch_id stamped), update bankroll. Ruin (bankroll≤0)
→ `ruin:true`, `ruin_index`, skip remaining, curve ends at ruin row.

### Step 6: Metrics + drawdown scans (Decision 7, A7, A13)
Accumulator reads (final_bank, total_staked). `roi` = cumulative
(final/start−1); `roi_per_bet` separate (A13). max_drawdown +
recovery_bets = single-pass curve scans (recovery null+status, not ∞,
A7). sharpe = sample-std per-bet returns, null on n<2/zero-stddev (A7).

### Step 7: Monte Carlo (A5, A6)
Hand-rolled PRNG (splitmix64 — ~15 lines, seed required). iid: draw N
with replacement. block:L: non-overlapping blocks, concatenate+truncate
to path length, default L=round(sqrt(N)). Default iid. Nearest-rank
percentiles P5/25/50/75/95. Determinism test: same seed → identical.

### Step 8: --odds (A9)
`--odds fixed:<d>` or `--odds column:<name>`. Resolved odds used for
BOTH Kelly sizing AND win settlement (same value, no mismatch).

### Step 9: Output (A14, A16)
Text summary (headline shape). Expanded JSON (A16): warnings,
outcome_counts, skip_counts, ruin/ruin_index, recovery_status,
curve_path, input_format, schema_mapping, outcome_mode, run-config.
jsonl `--emit-curve` (A4) with invariants (A14): one row per placed bet,
pushes included, voids/ruin-skipped excluded, batch_id stamped, empty
pool → header-only + warning.

### Step 10: Cartridge validation (A11)
When cartridge supplied: column-name provenance only (referenced columns
must be declared measures). Type mismatch → warn, don't block. Crypto
provenance deferred.

### Step 11: Tests + cookbook + gates
See acceptance gate below. Cookbook section + the bet-record format spec.
All gates incl. §6.7 quoted test run. Test YAML/fixtures single-brace (§4.5).

---

## Acceptance gate (binding — body 24 ACs + amendment revisions)

Report each explicitly when claiming done. Consolidated per the ADR's
"Consolidated acceptance-criteria revisions":

- [ ] AC #1-2, #4-5, #7-9, #11, #13, #15, #17, #19-24: per body
- [ ] AC #3: 4-state required; binary hard-errors unless `--outcome-mode legacy-binary`; `--derive-pushes` (A2)
- [ ] AC #6: same-timestamp = simultaneous batch default; `sequence` → sequential (A1)
- [ ] AC #10: sharpe sample-std, null on n<2/zero-stddev (A7)
- [ ] AC #12: EXP-049 repro runs `--replay sequential --outcome-mode legacy-binary` against claw-core's real file → $2,962.16 within 0.1% final / 0.01% checkpoints + peak/max_drawdown (A15 + A17)
- [ ] AC #14: curve invariants per A14
- [ ] AC #16: zero mc-core; PRNG hand-rolled in mc-cli (A5)
- [ ] AC #25: bankruptcy/ruin — cap at bankroll, ruin skips remaining, batch over-stake scales (A3)
- [ ] AC #26: pinned PRNG, same seed → byte-identical cross-platform (A5)
- [ ] AC #27: bootstrap — sample=path length, non-overlap blocks truncated, default iid, nearest-rank (A6)
- [ ] AC #28: `--odds fixed:|column:` for sizing AND settlement (A9)
- [ ] AC #29: filter first, window second (A12)
- [ ] AC #30: `roi` cumulative; `roi_per_bet` separate (A13)
- [ ] AC #31: `--sizing from_column:stake_hint` explicit; bare column ignored (A8)
- [ ] AC #32: JSON exposes warnings/outcome_counts/skip_counts/ruin/recovery_status/schema_mapping/outcome_mode/run-config (A16)
- [ ] AC #33: cartridge validation = column-name provenance only (A11)
- [ ] AC #34: `--replay batch|sequential` (default batch); sequential = stable-sort timestamp + compound in sequence-col-or-file order (A17)
- [ ] Build gates: fmt, clippy -D warnings, build, **`cargo test --workspace` quoted result line (§6.7)**, determinism ×10
- [ ] No float `==` (§3.1); zero-checks via `abs() < 1e-300`; no new deps (A5 — PRNG hand-rolled)

---

## Common pitfalls (forewarned)

1. **Ignoring same-timestamp batching.** 45% of real bets share a
   timestamp. Batch is the default (A1) — all same-timestamp stakes sized
   off one bankroll snapshot. THE bug to avoid.
1b. **Re-sorting by `bet_id` within a timestamp (A17).** Sort must be
   STABLE on timestamp — preserve file row order within a tie, do NOT
   re-sort by `bet_id`. Sequential replay (`--replay sequential`)
   compounds in file order (or `sequence`-column order if present).
   Re-sorting by bet_id would make `--replay sequential` reproduce a
   DIFFERENT number than claw-core's row-order headline — breaking the
   EXP-049 repro for a subtle reason.
2. **Running binary `won` by default.** Hard-error unless
   `--outcome-mode legacy-binary` (A2). Silent push-conflation is the
   ADR-0034-Wilson mistake again.
3. **Adding the `rand` crate.** Hand-roll splitmix64 (A5). No new dep.
4. **Adding a parquet/Arrow crate.** Use DuckDB via mc-drivers for input;
   jsonl for curve output (A4).
5. **`roi` as per-bet.** simulate's `roi` is cumulative (A13). Per-bet is
   `roi_per_bet`. Getting this wrong makes the +196% headline vanish.
6. **`recovery_bets = ∞` in JSON.** null + `recovery_status` (A7).
7. **Negative bankroll / borrowing.** Cap at current bankroll; ruin
   stops the replay (A3).
8. **Window before filter.** Filter first (A12).
9. **Auto-using a stake_hint column.** Only via explicit
   `--sizing from_column:stake_hint` (A8).
10. **Double-brace test YAML / unquoted "all green" claim.** §4.5 + §6.7.

---

## Cross-links

- ADR-0035: [`../decisions/0035-phase-10f-model-simulate.md`](../decisions/0035-phase-10f-model-simulate.md)
- Review request (16 amendments): [`../reviews/adr-0035-review-request.md`](../reviews/adr-0035-review-request.md)
- ADR-0034 (grade sibling): [`../decisions/0034-phase-10b-model-grade.md`](../decisions/0034-phase-10b-model-grade.md)
- ADR-0033 (10A — drawdown deferred here): [`../decisions/0033-phase-10a-evaluation-metrics-library.md`](../decisions/0033-phase-10a-evaluation-metrics-library.md)
- claw-core `exp028_bets.parquet` (input fmt) + `exp029_bankroll_curve.parquet` (output fmt)
- claw-core EXP-029/047/049 reports — sizing/bankroll findings the vocabulary is grounded in
- CLAUDE.md §3.1, §4.5, §6.7

---

## Completion report template

Write `docs/reports/phase-10f-completion-report.md`:
1. SPEC QUESTION resolutions (esp. the DuckDB-parquet-API question)
2. Test count + **quoted `cargo test --workspace` result line**
3. Build gate results
4. EXP-049 reproduction: which outcome-mode, final-bank parity vs claw-core
5. PRNG algorithm chosen + cross-platform determinism evidence
6. Any mc-core change (should be none; surfaced justification if any)
7. Curve output format (jsonl v1) + invariants verified
8. Effort actual vs estimate (4-5 sessions)
9. Recommended next phase from demand (10C backtest = claw-core's #2 ask)
