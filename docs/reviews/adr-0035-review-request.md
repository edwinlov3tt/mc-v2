# Review Request — ADR-0035: Phase 10F `mc model simulate`

**For:** dual external review (Claude Desktop + GPT-5.1, high-effort thinking)
**ADR under review:** [`docs/decisions/0035-phase-10f-model-simulate.md`](../decisions/0035-phase-10f-model-simulate.md)
**Status:** Proposed (not yet accepted; this review gates implementation)
**Date:** 2026-05-27

---

## How to review

Read the full ADR (~430 lines). Respond with **accept /
accept-with-amendments / reject** + specifics. The prior four ADRs in
this track (0031/0032/0033/0034) each shipped with 7-12 amendments after
this dual-review pass; real bugs were caught every time. Be adversarial.
The bet-record format (Decision 2) is the load-bearing contract — push
hardest there, because changing it post-ship breaks every consumer.

Amendments as a numbered, copy-pasteable block, please.

---

## Context

**Mosaic** is a Rust deployment-agnostic kernel for *evaluating* fitted
models over cubes. It does NOT train — Python trains, exports artifacts;
Mosaic consumes and evaluates. The evaluation track is adding `mc model`
subcommands that replace claw-core's one-shot Python experiment scripts.

**Shipped so far:**
- Phase 10A (ADR-0033): metrics library — `std_over`/`var_over`/
  `count_over`/`wilson_ci_lower`/`wilson_ci_upper`. Sample variance
  (ddof=1). `max_drawdown`/`recovery_bets` were DEFERRED here "to the
  phase where bet-record time-series exists."
- Phase 10B (ADR-0034): `mc model grade` — segmented holdout evaluation,
  grouped map-reduce. Order-INDEPENDENT. Adopted by claw-core; reproduced
  their EXP-048 finding live.

**What this ADR proposes.** `mc model simulate` — chronological bankroll
simulation. claw-core named it their #1 next request after adopting
grade: the headline numbers that gate their V1.1 model ("$1k → $2,962,
+196%") are path-dependent bankroll numbers that grade structurally
cannot produce (grade is order-independent; bankroll is the opposite).
simulate also finally homes `max_drawdown`/`recovery_bets`.

**The architectural pivot:** simulate is the first `mc model` verb that
consumes a *bet-record file* (one row per placed bet, prediction +
outcome + odds pre-resolved) rather than reading the cube. The records
come from walk-forward validation (a Python retraining job); Mosaic
replays them. This is the Python-trains-Mosaic-evaluates split applied to
bet records instead of coefficients.

---

## Grounding: the format is NOT speculative

The bet-record format (Decision 2) and the output-curve format
(Decision 9) are reverse-engineered from claw-core's REAL files, verified
this session:

**`exp028_bets.parquet`** — 7,272 rows × 16 cols, already time-sorted by
`commence_time`:
```
game_pk(int) commence_time(str) season(int) predicted_mu(f64) sigma(f64)
nb_alpha(f64) line(f64) market_p_over(f64) p_over_nb(f64) edge_pp(f64)
abs_edge_pp(f64) side(str OVER/UNDER) p_bet_side(f64) actual_total(f64)
won(int 0/1) decimal_odds(f64, constant 1.909 here)
```
Verified facts: `won` is 0/1 only (NO push state) despite **295 rows
where actual_total == line** (pushes folded into `won` upstream).
`abs_edge_pp >= 0.10` selects **1,508 of 7,272** rows. decimal_odds is
constant -110 here but the column exists for per-bet variation.

**`exp029_bankroll_curve.parquet`** — the output simulate produces:
```
commence_time season game_pk side p_bet_side abs_edge_pp stake won bankroll_after
```

---

## The 9 decisions (read the ADR for rationale)

1. **Command shape; cartridge OPTIONAL.** simulate needs only the records.
   Cartridge arg, when present, validates record columns against the
   measure schema (provenance) but the sim math never reads the cube.
2. **The bet-record format (load-bearing).** Required: `bet_id`,
   `timestamp`, `p_bet_side`, `decimal_odds`, `outcome`. Optional:
   edge/side/season/stake_hint. Column aliasing via `--columns` or sidecar
   schema (claw-core's `game_pk`→`bet_id` etc). Sorted by timestamp before
   replay.
3. **Outcome: explicit 4-state enum** (win/loss/push/void) with a `won`-0/1
   compat path that emits a precision-loss warning (the 295 pushes get
   approximated as win/loss).
4. **Closed sizing vocabulary:** flat / flat_current / kelly /
   quarter_kelly / half_kelly, with cap/shrink/min_odds/floor modifiers.
   Pinned Kelly formula. Reproduces claw-core's `quarter_kelly:cap=0.025,
   shrink=0.02`.
5. **Single-path replay** — sort, filter, window, replay loop, curve.
6. **Monte Carlo wrapper** — `--monte-carlo N --resample iid|block:len`,
   seeded determinism, percentile bands.
7. **Metrics incl. the deferred drawdown family** — final_bank/roi/
   max_drawdown/recovery_bets/sharpe/p_underwater/percentile-banks.
8. **CLI-only, mc-cli impl, zero mc-core** (same as grade).
9. **Output: text + JSON + optional `--emit-curve`** matching the real
   bankroll-curve file shape.

---

## Pressure-test questions

### Q1 — Is the bet-record column set right? (THE question)
Required = {bet_id, timestamp, p_bet_side, decimal_odds, outcome}.
Optional = {edge, side, season, stake_hint}. Is anything the EXP-029
family needs missing? Over-specified? Specifically: does Monte Carlo
block-resampling need anything beyond timestamp ordering? Does
per-stratum simulation (per-season) need season as required, not
optional? Is `stake_hint` (pre-computed stake bypassing --sizing) a
footgun or a useful escape hatch?

### Q2 — 4-state outcome enum vs hard-error on 0/1
The proposal accepts claw-core's 0/1 `won` with a warning (pushes
approximated). Alternative: hard-error on 0/1, forcing claw-core to
re-export with an explicit 4-state outcome (push-accurate). For a betting
tool where the bankroll number is the headline claim, is "run with a
warning, 295 pushes approximated as win/loss" acceptable, or is a wrong
bankroll worse than an inconvenient hard error? (This is the 0034-Amendment-3
"warning vs hard-error" question again, in a new context.)

### Q3 — `--filter`: reuse grade's grammar or simpler predicate parser?
grade's `Filter` operates on cube measures. simulate's records are flat
columns. Reuse `Filter` verbatim (with a record-column adapter), or a
simpler standalone predicate parser? The float-equality guard (0034-Amdt-1)
applies either way — `line == 9.0` on a float column is the same hazard.

### Q4 — Parquet dependency
simulate must read parquet (claw-core's records are parquet). Options:
(a) reuse `mc-drivers`/duckdb (already in the tree for Tessera, but heavy
for a CLI verb); (b) a lighter standalone parquet crate; (c) jsonl-only
for v1, parquet via a Tessera pre-convert step. Which respects kernel
discipline + ships fastest? Note mc-cli already depends on the workspace;
the question is whether to pull a parquet path into the CLI specifically.

### Q5 — Cartridge-optional under `mc model`, or top-level `mc simulate`?
simulate doesn't read the cube. Is keeping it `mc model simulate
[cartridge]` (cartridge optional, for provenance/validation) the right
ergonomic, or should it be `mc simulate --bets ...` (no model namespace)
since it operates on records not models? The `mc model` namespace implies
"reads a model"; simulate breaks that implication.

### Q6 — Monte Carlo determinism + scope
Seed required when `--monte-carlo` set (current). Block-resample length
default? Is iid + block enough, or does the EXP-029 family need a third
mode? Are percentile bands (P5/25/50/75/95) the right summary, or also
expose the full distribution / histogram?

### Q7 — Anything missing for the EXP-029 family specifically?
The five scripts this replaces: EXP-029b (bootstrap), 029c (slippage),
029d (bet caps), 029e (first-30), 029f (recovery). Walk each: can the
proposed command express it? 029c (slippage) is partially deferred (odds
override, not stochastic-odds mode) and 029d (cap matrix) is deferred
(script multiple calls). Are those deferrals right, or do they gut the
command's value for the family it's meant to replace?

---

## What NOT to relitigate
Settled by prior accepted ADRs:
- The 10A primitive set + sample-variance ddof=1 (ADR-0033)
- grade's design (ADR-0034)
- Python-trains-Mosaic-evaluates (ADR-0025)
- CLI-only / no-daemon for batch analytics (ADR-0034 precedent)
- Demand-driven sequencing (this session)

---

## Output format requested
```
VERDICT: accept | accept-with-amendments | reject
[If amendments:] numbered, copy-pasteable.
Per-question: Q1...Q7
What's well done: ...
Biggest risk if shipped as-is: ...
```
