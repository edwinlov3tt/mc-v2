# Built-In Evaluation Primitives — Replacing Bespoke Python Scripts with Composable CLI Commands

**Status:** Research note (pre-ADR; explores the design space)
**Date:** 2026-05-27
**Author:** Mosaic PM (Claude Opus 4.6, 1M context)
**Source:** Pattern analysis of 29 one-shot Python experiment scripts in
`claw-core/training/mlb/` (EXP-020 through EXP-045) + 5 repeatable
workflow scripts, accumulated across one sport in one quarter.

---

## The observation

claw-core's MLB cartridge produced 29 experiment scripts between
2026-04 and 2026-05. Each is 150-400 lines of Python. They are
individually valuable — but when you line them up, **five shapes
account for 26 of the 29.** The remaining 3 are genuinely novel
(one-off model-selection choices). The 26 are 80% boilerplate
(load data, loop over games, compute metrics, format JSON) and
20% novel (the specific hypothesis being tested).

The scripts are not composable. `exp039` (threshold × line bucket)
can't reuse `exp033`'s threshold grid. `exp029f` (recovery
bootstrap) can't reuse `exp029b`'s simulation loop. Each new
hypothesis spawns a new 300-line script.

If Mosaic owned the evaluation/simulation/grading primitives as
CLI commands, a new hypothesis would be a **one-liner with different
flags**, not a new script.

---

## The five repeating shapes

### Shape 1: Parameter sweep → metric grid (7 scripts)

**Scripts:** exp021 (Lasso α), exp032 (NB α), exp033 (edge threshold),
exp039 (threshold × line bucket), exp042 (coefficient stress),
exp044 (OOS coef stress), exp045 (per-line cross-season)

**Pattern:** Vary one parameter across a grid. For each grid point,
recompute predictions on an entire holdout set. Evaluate
{direction_accuracy, ROI, Sharpe, Wilson CI, n_bets} per point.
Optionally group by a second dimension (line bucket, season).

**What's identical across all seven:**
- Load holdout data (parquet or Mosaic canonical_inputs)
- Loop: for each grid value, recompute `predict()` → `nbinom_sf()` →
  `calibrate()` → edge → filter → metrics
- Emit JSON with metrics per grid point

**What differs:** The swept parameter (α, threshold, coefficient
multiplier) and the grouping dimension.

**Proposed command:**

```
mc model backtest mlb-totals.yaml \
  --sweep dispersion_alpha 0.05:0.30:0.05 \
  --filter "abs(Edge_NB) >= 0.10" \
  --metrics direction_accuracy,roi,sharpe,wilson_lower,n_bets \
  --holdout "Time=2025" \
  --group-by line_bucket \
  --output exp032_alpha_robustness.json
```

The `--sweep` flag reuses the existing `/sweep` infrastructure but
runs it across every game in the holdout set and aggregates. The
`--filter` applies a post-prediction bet filter. The `--group-by`
produces a nested metric grid (metric × sweep_value × group).

A second variant sweeps a **coefficient** instead of an input:

```
mc model backtest mlb-totals.yaml \
  --sweep-coefficient mlb_v10_lasso.park_factor 0.75:1.25:0.05 \
  --filter "abs(Edge_NB) >= 0.10" \
  --metrics direction_accuracy,roi,n_bets \
  --holdout "Time=2023,Time=2024,Time=2025" \
  --output exp042_coefficient_stress.json
```

### Shape 2: Walk-forward validation (1 script, but load-bearing)

**Script:** exp028

**Pattern:** Given N fold definitions (each specifying a training
window and a holdout season), for each fold: load the fold's
coefficients, evaluate the cartridge on the holdout season, collect
chronological bet records with {game, mu, P_Over, market_p, edge,
outcome, direction_correct}. Emit a unified bet-record table that
downstream simulations consume.

**What stays in Python:** Model training (sklearn Lasso per fold).
The fold outputs `fold_YYYY.json` (coefficients + intercept +
dispersion α + calibration map).

**What moves to Mosaic:** Evaluation + bet-record emission. Given
a cartridge + a fold artifact + a holdout input set, Mosaic
evaluates every game and emits the standardized bet record.

**Proposed command:**

```
mc model walk-forward mlb-totals.yaml \
  --folds artifacts/fold_2023.json,artifacts/fold_2024.json,artifacts/fold_2025.json \
  --holdout-years 2023,2024,2025 \
  --filter "abs(Edge_NB) >= 0.10" \
  --emit-bets walk_forward_bets.parquet \
  --output exp028_walk_forward.json
```

The fold artifact format is a convention: JSON with
`{coefficients, intercept, dispersion_alpha, calibration_map}` —
the same shape `export_to_mosaic.py` already produces. Each fold
temporarily overrides the cartridge's fitted model with the fold's
coefficients and evaluates.

### Shape 3: Monte Carlo / bootstrap on bet records (5 scripts)

**Scripts:** exp029b (bootstrap), exp029c (slippage), exp029d
(bet caps), exp029e (first-30 window), exp029f (recovery)

**Pattern:** Read a bet-record parquet (from walk-forward). Resample
it N times (IID bootstrap or contiguous-block). For each sample,
simulate bankroll evolution under a sizing rule (flat, Kelly variants).
Emit percentile distributions (P5/P25/P50/P75/P95) for {final_bank,
max_drawdown, recovery_bets, Sharpe}.

**What's identical across all five:**
- Load bet records
- Bootstrap loop (N=1000)
- Bankroll simulation (Kelly sizing with configurable fraction/cap)
- Percentile computation
- JSON emission

**What differs:** Sampling method (IID vs block), sizing rule
(flat/quarter-Kelly/half-Kelly), slippage model (fixed vs stochastic
odds), window slice (all, first-30, by-start-quality), stake caps.

**Proposed command:**

```
mc model simulate mlb-totals.yaml \
  --bets walk_forward_bets.parquet \
  --iterations 1000 \
  --sizing quarter_kelly_shrunk \
  --kelly-fraction 0.25 --kelly-cap 0.025 --ci-shrink 0.02 \
  --window first_30 \
  --slippage fixed:-110 \
  --metrics bankroll_p5,bankroll_p25,bankroll_p50,bankroll_p75,bankroll_p95,max_drawdown,recovery_bets \
  --output exp029e_first_30.json
```

A `--stress-matrix` flag could run the full cross-product:

```
mc model simulate mlb-totals.yaml \
  --bets walk_forward_bets.parquet \
  --iterations 1000 \
  --stress-matrix \
    sizing=flat_1pct,quarter_kelly,half_kelly \
    window=all,first_30,first_100 \
    slippage=best_line,fixed:-110,fixed:-115 \
  --output exp029_full_matrix.json
```

That replaces exp029b through exp029f with one invocation.

### Shape 4: Segment/bucket evaluation (4 scripts)

**Scripts:** exp022 (edge × noise), exp023 (line source audit),
exp031c (April bias decomposition), noise_band_analysis

**Pattern:** Partition a holdout set by some dimension(s). Compute
{direction_accuracy, Wilson CI, n_bets, ROI, mean_residual} per
bucket. Flag buckets that exceed or fail breakeven.

**What's identical:** Metric computation, Wilson CI, bucket
aggregation, breakeven flagging.

**What differs:** Grouping dimensions (edge_bucket, noise_band,
line_source, month, venue, team).

**Proposed command:**

```
mc model grade mlb-totals.yaml \
  --holdout "Time=2025" \
  --group-by edge_bucket,noise_band \
  --metrics direction_accuracy,wilson_lower,n_bets,roi,mean_residual \
  --flag-if "wilson_lower < 0.50" \
  --output exp022_edge_buckets.json
```

The `--group-by` accepts any Input measure or derived dimension.
A computed grouping like `edge_bucket` (0-3%, 3-10%, 10-15%, ...)
uses a `--bucket` definition:

```
mc model grade mlb-totals.yaml \
  --holdout "Time=2025" \
  --bucket Edge_NB 0:0.03:0.10:0.15:0.20:1.0 \
  --bucket Noise_Multiplier 0:1.0:1.5:3.0 \
  --group-by Edge_NB_bucket,Noise_Multiplier_bucket \
  --metrics direction_accuracy,wilson_lower,n_bets \
  --output exp022_edge_noise.json
```

### Shape 5: Batch feature sensitivity / counterfactual (5 scripts)

**Scripts:** exp034 (±1σ per feature), exp035 (2D interaction),
exp038 (PNC park_factor audit), exp041 (loss counterfactuals),
exp043 (joint counterfactuals)

**Pattern:** For each game (or a filtered subset), sweep one or
two input measures across a range. Observe which games change their
bet recommendation. Aggregate: {n_flips, flip_threshold, swing_feature}.

exp034 and exp038 already call Mosaic's daemon `/sweep` — they're
thin Python wrappers that loop over games. The missing piece is
**batch sweep with aggregation**.

**Proposed command:**

```
mc model sweep mlb-totals.yaml \
  --games "Time=2025,Scenario=Base" \
  --vary park_factor 0.95:1.10:0.025 \
  --show Predicted_Total,P_Over_NB,Should_Bet \
  --aggregate count_flips(Should_Bet),mean_delta(Predicted_Total) \
  --output exp038_pnc_audit.json
```

For the counterfactual pattern (exp041, exp043), a `--find-flip`
mode finds the minimum perturbation that changes Should_Bet:

```
mc model sweep mlb-totals.yaml \
  --games "Time=2025,Scenario=Base" \
  --filter "Should_Bet == 0" \
  --vary park_factor 0.90:1.20:0.01 \
  --find-flip Should_Bet \
  --output exp041_counterfactuals.json
```

---

## Cross-cutting primitive: metrics library

All five shapes use the same ~10 metrics, reimplemented in every
experiment script:

| Metric | Formula | Used in shapes |
|---|---|---|
| `direction_accuracy` | `mean(predicted_direction == actual_direction)` | 1, 2, 4 |
| `roi` | `sum(pnl) / sum(stakes)` | 1, 2, 3, 4 |
| `sharpe` | `mean(returns) / std(returns) * sqrt(n)` | 1, 3 |
| `wilson_lower` | Wilson 95% CI lower bound on binomial proportion | 1, 4 |
| `wilson_upper` | Wilson 95% CI upper bound | 4 |
| `n_bets` | Count after filter | 1, 2, 3, 4 |
| `brier` | `mean((predicted_prob - outcome)^2)` | 2, 4 |
| `mean_residual` | `mean(actual - predicted)` | 4 |
| `max_drawdown` | Largest peak-to-trough in bankroll history | 3 |
| `recovery_bets` | Bets from trough to previous peak | 3 |

These should live in `mc-core` as first-class aggregation functions,
available to all five commands and to the formula evaluator. Some
(Wilson CI, Sharpe) are statistical; others (max_drawdown, recovery)
are time-series. All are well-specified and domain-independent —
they'd serve an NBA, NFL, or soccer cartridge identically.

---

## What stays in Python permanently

| Category | Scripts | Why |
|---|---|---|
| Model training | `train.py` | sklearn Lasso + StandardScaler + CV. Mosaic evaluates, doesn't train |
| Data collection | `backfill.py`, `historical_odds.py`, `xera_leaderboard.py` | HTTP scraping of MLB Stats API, Odds API, Baseball Savant |
| Feature engineering | `build_features.py` | pandas joins + rolling stats + encodings |
| Model export | `export_to_mosaic.py` | Bridge between Python-trains and Mosaic-evaluates |
| Settlement/ops | `settle_yesterday.py`, `predict_today.py` (data portion) | API calls + file I/O + KB messaging |
| Reference data | `team_names.py`, `venues.py`, `kb_formatter.py` | Static lookups + formatting |

These scripts produce *inputs* to Mosaic (parquet data, model
artifacts, cartridge YAML). Mosaic consumes them and runs the
evaluation/simulation/grading. The boundary is: **Python collects
and trains; Mosaic evaluates and analyzes.**

---

## Generalization beyond sports betting

The five shapes are not sports-specific:

| Shape | Sports betting example | Marketing example | Finance example |
|---|---|---|---|
| Parameter sweep | Edge threshold grid | Budget allocation grid | Discount rate sensitivity |
| Walk-forward | Retrain per season | Retrain per quarter | Rolling-window backtest |
| Monte Carlo | Bankroll simulation | Campaign outcome simulation | Portfolio VaR |
| Segment grading | Edge × noise buckets | Channel × region performance | Sector × duration attribution |
| Batch sensitivity | Feature ±1σ impact | Creative variant impact | Factor exposure decomposition |

If Mosaic ships these primitives, the story changes from "AI-native
planning kernel for marketing" to "AI-native planning kernel for
any quantitative domain that evaluates fitted models against data."
Which is what the LNM positioning (ADR-0009) already claims — these
commands would be the proof.

---

## Scope and phasing (rough)

| Phase | Scope | Estimate |
|---|---|---|
| Phase A | Metrics library in mc-core (10 metrics, well-specified, tested) | 1 ADR, 1-2 sessions |
| Phase B | `mc model grade` (segment evaluation — simplest command, validates the metric library) | 1 ADR, 2-3 sessions |
| Phase C | `mc model backtest` (parameter sweep across holdout — the highest-demand command) | 1 ADR, 3-4 sessions |
| Phase D | Batch `mc model sweep` (multi-game sweep with aggregation — extends existing infrastructure) | 1 ADR, 2-3 sessions |
| Phase E | `mc model simulate` (Monte Carlo on bet records — most self-contained) | 1 ADR, 2-3 sessions |
| Phase F | `mc model walk-forward` (multi-fold evaluation — depends on fold-artifact convention) | 1 ADR, 2-3 sessions |

Total: ~5 ADRs, ~14-18 sessions. This is Phase 10+ territory. The
five commands compose naturally: `walk-forward` produces bet records,
`simulate` consumes them, `grade` evaluates segments, `backtest`
sweeps parameters, batch `sweep` does per-game sensitivity. A full
experiment lifecycle would be:

```bash
# Train in Python (stays there)
python3 train.py --alpha 0.01 --output-folds

# Evaluate in Mosaic (new)
mc model walk-forward mlb-totals.yaml --folds ... --emit-bets bets.parquet
mc model grade mlb-totals.yaml --holdout "Time=2025" --group-by edge_bucket
mc model backtest mlb-totals.yaml --sweep dispersion_alpha 0.05:0.30:0.05
mc model simulate mlb-totals.yaml --bets bets.parquet --iterations 1000
mc model sweep mlb-totals.yaml --games "Time=2025" --vary park_factor 0.95:1.10
```

Five commands replace 26 experiment scripts.

---

## Demand signal

- 29 experiments in one sport in one quarter (MLB, 2026 Q2)
- claw-core's NBA cartridge will produce a parallel set when the
  NBA season opens
- Every new sport cartridge (NFL totals, soccer totals) will face
  the same experiment matrix
- exp034, exp035, exp038 already call Mosaic's daemon — the pattern
  is migrating naturally toward Mosaic even without these primitives

The question is not "should Mosaic do this?" — consumers are already
writing thin Python wrappers around `/sweep` to get it done. The
question is "should the wrappers live in Mosaic or in every consumer?"

---

## Open design questions (for the ADR phase)

1. **Output format.** JSON (like the experiment scripts)? Parquet
   (for downstream consumption)? Both? The experiment scripts emit
   JSON; the simulation scripts consume parquet. A convention that
   defaults to JSON for reports and parquet for bet records would
   cover both.

2. **Holdout specification.** `--holdout "Time=2025"` works for
   Mosaic coordinate filters, but the experiment scripts also filter
   by computed fields (e.g., "games where the model predicted over
   but the line was under 8"). Should `--filter` accept formula
   expressions? That's a significant parser extension.

3. **Fold artifact format.** `walk-forward` needs a convention for
   what a fold JSON looks like. The natural shape mirrors
   `export_to_mosaic.py`'s output, but needs a formal schema.

4. **Metric extensibility.** Should the 10 metrics be hardcoded or
   should there be a `--custom-metric "sum(pnl) / sum(stakes)"` flag
   that accepts formula expressions? Hardcoded is safer; extensible
   is more powerful. The formula evaluator already exists — reusing
   it for metric definitions is natural but adds scope.

5. **Daemon vs CLI.** Should these commands hit the daemon (fast,
   warm cache) or run standalone (simpler, no daemon dependency)?
   For single-game sweep, daemon is clearly better. For 7,000-game
   backtest, the overhead of 7,000 HTTP calls may be worse than a
   single in-process evaluation. Likely answer: CLI for batch
   commands, daemon for interactive single-game commands.

6. **Integration with Tessera.** Tessera already ingests parquet →
   canonical_inputs. Should `walk-forward` emit its bet records as
   a Tessera-importable format so they can be loaded back into a
   "results" cube for further analysis? That closes the loop nicely
   but adds a dependency.

---

## Cross-links

- **ADR-0009:** LNM substrate vision — these commands are the
  proof that Mosaic generalizes beyond marketing
- **ADR-0031:** nbinom_sf — the formula primitive that made MLB
  cartridge evaluation native
- **ADR-0032:** Phase 8.2 consumer API — `/sweep` and `/whatif`
  are the single-game versions of what this note proposes at batch
  scale
- **claw-core EXP-020 through EXP-045:** the 29 experiment scripts
  this note analyzes
- **claw-core integration test:** `docs/reports/mosaic-integration-test.md`
  — proves the single-game primitives work; this note proposes the
  batch generalization
- **POSITIONING.md:** Mosaic as LNM platform — these commands turn
  "AI-native planning kernel" from positioning copy into a
  measurable capability

---

## Notes

- This note deliberately does NOT propose implementation details
  for the metrics library or the five commands. Those belong in
  ADRs once the project owner decides to sequence them.
- The experiment scripts in claw-core should NOT be deleted even
  after these commands ship. They're historical artifacts with
  commit-pinned results. Future commands produce equivalent output,
  but the originals are the audit trail for V1.0 model decisions.
- The "5 commands replace 26 scripts" framing is for the MLB
  cartridge. As more sport/domain cartridges ship, the leverage
  increases — each new cartridge gets the evaluation primitives
  for free instead of writing its own experiment suite.
