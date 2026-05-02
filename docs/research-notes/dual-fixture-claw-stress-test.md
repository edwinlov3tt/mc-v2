---
name: Dual-fixture stress test + planning workflow using claw-core data
description: Proposal — use the claw-core (NBA totals pricing engine) production dataset as MC's second fixture, parallel to Acme; surfaces real-world cardinality + sparsity for stress testing the kernel and tests whether MC's planning-engine superpowers (scenarios, writeback, snapshot/rollback, derived rules) translate to a sports-betting backtest workflow
type: research-note
---

# Dual-fixture stress test + planning workflow using claw-core data

**Status:** proposal — seeking validation (not adopted; not on the main plan)
**Created:** 2026-05-01
**Last touched:** 2026-05-01
**Spans phases:** would land between Phase 2B (consolidation fast path) and Phase 3 (data ingest), or as a Phase 2B-sibling experiment
**Author:** drafted by Claude during a Phase 2A wrap-up conversation; written for review by another instance before any commitment

---

## What I'm asking another instance to validate

> **Is using `claw-core`'s production dataset (NBA totals pricing engine — 5 seasons, ~12K games, 13 books, ~1,300 OOD bets) as MC's second fixture a good use of ~1 week of work, given Phase 2B is queued and `mc-core/src/` is otherwise idle?**

Three sub-questions a reviewer should pressure-test:

1. **Is the kernel-fit honest?** I claim the data is genuinely multidimensional (Scenario × Season × Sportsbook × Market × Edge_Bucket × Measure) and that MC's superpowers (writeback, scenarios, snapshot/rollback, weighted-avg consolidation) map onto real claw planning questions. *Pressure test:* could a reviewer name a planning question I list that doesn't actually need MC?
2. **Is the stress-test value real?** I claim ~7,800 leaf coords + larger fan-out exposes load patterns Acme's 2,520 cells cannot — specifically sparse leaves (many strategy/season/book/edge combos have zero bets), the Sum + WeightedAverage aggregation mix, and consolidation hot-paths at deeper fan-out. *Pressure test:* would a reviewer expect this to surface a different bottleneck than Phase 2A's PERF.md §6.7 already shows?
3. **Is the opportunity cost worth it?** Phase 2B's `Arc<Hierarchy>` consolidation fix is ~½ day, data-justified, and improves the existing PERF.md cold-row floor. The claw fixture is ~3-4 days for stress-test-only or ~1 week with planning-test integration tests. *Pressure test:* a reviewer who reads only PERF.md §6.7–§6.10 — would they recommend this work next, or would they push for Phase 2B + a different Phase 2C?

I want a second opinion before this becomes a phase. The user explicitly said "we can come back to this, not add it to the main plan yet."

---

## Background — what a reviewer needs to know cold

### MC, in one paragraph

MC (this repo, `~/Projects/mc-v2`) is a Phase 1 Rust kernel for a multidimensional planning engine. Acme demo cube: 6 dims `[Scenario, Version, Time, Channel, Market, Measure]`, 11 measures (6 inputs, 5 derived via `Spend → Clicks → Leads → Customers → Revenue → Gross_Profit` rules), 3 hierarchies (Time/Channel/Market), 2,520 input cells. As of Phase 2A (commit `48d52e9`, tag `phase-2a-cold-path-baseline`, 2026-05-01) the kernel passes every brief §11 1A ceiling on real cold reads, with snapshot clone + hierarchy mark microbench data in PERF.md §6.7–§6.10. **Acme is the only fixture exercising the kernel today.** That is a real risk: any pattern Acme doesn't show, the kernel hasn't been tested against.

Important kernel constraints, restated for a reviewer who hasn't read the brief:

- One default hierarchy per dim (Phase 1 narrowing of the semantics doc).
- No CSV / SQL / network ingest — `mc-fixtures` is a Rust crate that builds the cube programmatically.
- No async, no threads, no `unsafe`, no `Box<dyn Trait>` for storage.
- Concrete `HashMapStore` (no `CellStore` trait until Phase 2+).
- Snapshot is a deep clone of `HashMapStore` (Phase 2A measured this at 55 µs for ~25K cells).
- Writeback rules: input cells only; derived cells rejected; consolidated cells rejected; NaN rejected; type-checked; revision-bumped.
- Per-write hierarchy ancestor mark walk dominates write cost (~712 ns/mark on Acme; ~98 ns/mark on the synthetic minimal-hierarchy fixture).

### claw-core, in one paragraph

`claw-core` (separate repo, `~/Projects/claw-core`) is a NBA totals pricing engine — Cloudflare Worker + Hono + D1 + KV + R2. The active model is V1.6 Lasso L1 with player features (50 raw-space coefs, 9 non-zero) — sparse by design. Holdout MAE 13.78 points; production-honest forward-looking edge per [`docs/CURRENT_STATE.md`](../../../claw-core/docs/CURRENT_STATE.md) is ~1.7pp above breakeven (54.10% WR / +4.19% ROI on 1,331 OOD bets, EXP-015). Production runs Lasso 90% / XGBoost 10% per `src/scheduled/pre-game-analysis.ts:97`; EXP-015 swept all weights and found XGBoost adds zero measurable OOD value (Lasso/XGB prediction correlation 0.96 — they're essentially redundant). Calibration map live in KV at `calibration:map:current` since 2026-04-30 (EXP-016, Brier 0.289 → 0.250). The system describes itself as a **pricing engine, not a picks engine** — it outputs prices/edges/Kelly fractions; the bet-or-skip decision happens downstream in the KellyBets iMessage agent (cron schedule: `kb-daily-predictions` at 12:20pm ET, `kb-confirmation-gate` at 6:35pm ET, `kb-settlement-report` at 9am ET).

Data accessibility (the path that makes this proposal tractable):

- **D1 database `claw-edge-db`** is queryable via `wrangler d1 execute claw-edge-db --remote --command "SELECT …"`.
- **Schema** has 19 migrations (`schema/migration-001-…sql` … `migration-019-…sql`); core tables include `games`, `bookmakers`, `predictions`, `wagers`, `bankroll_accounts`, `bankroll_transactions`, `team_stats`, `features`, `confirmation_status` (migration 018).
- **OOD vs IS season split:** OOD = 2020-21 + 2021-22 (model never saw); IS = 2022-23 → 2024-25 (training). The 2.5pp inflation between IS and OOD is documented in `claw-core/docs/CURRENT_STATE.md` "Headline".
- **Multi-season backtest artifacts** like `training/artifacts/exp013_v16_production_replay.parquet` exist locally (annotated bets per the EXP-013 simulator).

### Why a "second fixture" is interesting at all

- **Acme is internally consistent (every leaf has every measure) and dense.** Real planning data is sparse, asymmetric, and has measure mixtures Acme doesn't produce.
- **Acme's rule chain is a multiplication tree (every rule is `Mul` or `Div`).** Real planning rules include subtraction (`Net_PnL = Returns - Stake`), ratios at consolidated level (ROI), and weighted averages with non-stake weights (Hit_Rate weighted by Bets_Placed, not by Stake).
- **Acme has no NULL / missing leaves at scale.** Sports betting datasets are full of (Strategy × Season × Book × Market × Edge_Bucket) combos with zero bets — those leaves should consolidate as Null or 0, and MC's behavior on sparse cubes has not been benched.
- **Acme has no validation against an external ground truth.** Claw's EXP-013 simulator computes ROI/PnL/CLV in pandas; an MC fixture that recomputes the same numbers via consolidation gives you a cross-validation test that an Acme-only fixture cannot.

---

## The proposal — `Claw_Backtest` cube, concrete shape

### Dimensions (canonical order; 7 dims; 1 more than Acme)

```
[Scenario, Version, Season, Sportsbook, Market, Edge_Bucket, Measure]
```

| Dim | Leaves | Hierarchy | Depth |
|---|---|---|---|
| `Scenario` | `Baseline_Lasso100`, `Lasso90_XGB10` (current production), `XGB100`, `Calibrated_Only`, `EdgeGate10`, `EVGate2pct`, `Sharp_Books_Only`, `Soft_Books_Excluded`, `Quarter_Kelly`, `Half_Kelly`, `No_Playoff_Offset`, … | flat | — |
| `Version` | `Working`, `Submitted`, `Approved` | flat | — |
| `Season` | `2020-21` … `2024-25` | `FY_All → {OOD, IS} → seasons`. OOD = 2020-21, 2021-22; IS = 2022-23 / 23-24 / 24-25 | 2 |
| `Sportsbook` | 13 books (pinnacle, circa, bookmaker, fanduel, betrivers, draftkings, betmgm, …) | `All_Books → {Sharp, Mid, Soft} → individual books` (matches `bookmakers.tier`) | 2 |
| `Market` | `Totals` (Phase 1 of claw); placeholder for `Spreads`, `Moneyline`, `Player_Props` | `All_Markets → {Totals, Spreads, ML, Props}` | 1 |
| `Edge_Bucket` | `<10pct`, `10-15pct`, `15-20pct`, `>20pct` | `All_Edges → {Below_Threshold, Above_Threshold} → 4 buckets` | 2 |
| `Measure` | 13 measures (below) | flat | — |

Hierarchies max at depth 2 (Acme's max is Market at depth 3: City → State → Region → USA). Cardinality at the smallest sensible aggregation: `~10 × 3 × 5 × 13 × 1 × 4 = ~7,800` non-Measure tuples × 8 input measures = **~63,000 input cells before rollups**, ~25× Acme's 2,520. Plus consolidated coords. Right shape for cardinality stress without exploding to per-game (~12K games × 13 books × 4 buckets ≈ 600K cells, which is too sparse to be useful as a planning fixture).

### Measures (8 input + 5 derived = 13 — Acme has 11)

**Inputs:**

| # | Name | Type | Aggregation | Source |
|---|---|---|---|---|
| 1 | `Bets_Placed` | count (i64) | `Sum` | `COUNT(*) FROM wagers WHERE …` |
| 2 | `Bets_Won` | count (i64) | `Sum` | `COUNT(*) FROM wagers WHERE result='WON'` |
| 3 | `Stake_Total` | F64 ($) | `Sum` | `SUM(stake) FROM wagers` |
| 4 | `Returns_Total` | F64 ($, gross including stake) | `Sum` | `SUM(payout) FROM wagers` |
| 5 | `Edge_Avg` | F64 (decimal) | `WeightedAverage(weight=Stake_Total)` | stake-weighted edge from `predictions.edge_probability` |
| 6 | `CLV_Avg` | F64 (points) | `WeightedAverage(weight=Stake_Total)` | stake-weighted CLV from `bet_line - closing_line` |
| 7 | `Calibrated_Prob_Avg` | F64 | `WeightedAverage(weight=Stake_Total)` | stake-weighted calibrated probability (post EXP-016 map) |
| 8 | `Implied_Prob_Avg` | F64 | `WeightedAverage(weight=Stake_Total)` | stake-weighted market-implied probability |

**Derived (rule-evaluated per leaf, then aggregated):**

| # | Name | Rule body | Aggregation | Why this aggregation |
|---|---|---|---|---|
| 9 | `Net_PnL` | `Returns_Total - Stake_Total` | `Sum` | Σ Net_PnL = Σ Returns − Σ Stake; Sum is correct. |
| 10 | `ROI` | `Net_PnL / Stake_Total` | `WeightedAverage(weight=Stake_Total)` | Consolidated ROI = (Σ Net_PnL × Stake) / Σ Stake² is wrong; we want Σ Net_PnL / Σ Stake, which equals weighted-avg of per-leaf ROI weighted by Stake. |
| 11 | `Hit_Rate` | `Bets_Won / Bets_Placed` | `WeightedAverage(weight=Bets_Placed)` | Same logic — consolidated Hit_Rate = Σ Won / Σ Placed = weighted-avg by Bets_Placed. |
| 12 | `Avg_Stake` | `Stake_Total / Bets_Placed` | `WeightedAverage(weight=Bets_Placed)` | Per-bet avg stake; consolidates as Σ Stake / Σ Bets. |
| 13 | `Edge_vs_Implied` | `Calibrated_Prob_Avg - Implied_Prob_Avg` | `WeightedAverage(weight=Stake_Total)` | Sanity: should equal `Edge_Avg` to first order; cross-check that the calibration map is doing what EXP-016 claims. |

**Rule chain depth = 2** (`ROI` depends on `Net_PnL`). Acme has depth 5 (`Spend → Clicks → Leads → Customers → Revenue → Gross_Profit`). If rule-chain-depth parity is wanted for stress-testing, add `Net_PnL_per_Bet = Net_PnL / Bets_Placed` (depth 2) and `Edge_Realized = Hit_Rate - Implied_Prob_Avg` (depth 2 via `Hit_Rate`); these aren't strictly needed for the planning workflow.

The `Sum` + `WeightedAverage` mixture is more diverse than Acme (which is ~9 Sum / 5 WeightedAverage on the input side and all-Sum on derived). Stress tests `consolidation.rs::Consolidator::read`'s strategy dispatch.

### Rules (5 — matches Acme's 5)

```
1. Hit_Rate         = Bets_Won      / Bets_Placed
2. Net_PnL          = Returns_Total - Stake_Total
3. ROI              = Net_PnL       / Stake_Total
4. Avg_Stake        = Stake_Total   / Bets_Placed
5. Edge_vs_Implied  = Calibrated_Prob_Avg - Implied_Prob_Avg
```

All bodies are `Expr::Sub` or `Expr::Div` over `Expr::SelfRef` (matches the Phase 1 expression grammar exactly). Division-by-zero at `Bets_Placed = 0` returns `ScalarValue::Null` per spec §7 — exactly the case Acme doesn't naturally produce, so this is also an unintended-but-welcome NaN/Null path stress test.

### Ingest path (where the work concentrates)

There is no SQL/CSV connector in `mc-core` and there shouldn't be (Phase 1 is single-purpose; data ingest is Phase 3). The minimum-viable approach is a **one-off export to a checked-in CSV**, then a Rust-side parser at fixture-build time:

```
crates/
└── mc-fixtures-claw/                       NEW crate; depends on mc-core + csv (workspace-allowable dep)
    ├── Cargo.toml
    ├── src/
    │   ├── lib.rs                          build_claw_cube() -> (Cube, ClawRefs); same shape as build_acme_cube
    │   └── ingest.rs                       parse CSV -> Vec<WritebackRequest> -> cube.write loop
    └── data/
        └── claw_backtest_2026-05-01.csv    one-shot export from claw-D1, ~few MB, in repo
```

Export script (run once on the user's machine, not part of the build):

```bash
# claw-core repo
npx wrangler d1 execute claw-edge-db --remote --command "
  SELECT
    p.model_version,
    g.season,
    p.recommended_book,
    'totals' AS market,
    CASE
      WHEN p.edge_probability < 0.10 THEN '<10pct'
      WHEN p.edge_probability < 0.15 THEN '10-15pct'
      WHEN p.edge_probability < 0.20 THEN '15-20pct'
      ELSE '>20pct'
    END AS edge_bucket,
    -- per-leaf aggregations
    COUNT(*) AS bets_placed,
    SUM(CASE WHEN p.result = 'WON' THEN 1 ELSE 0 END) AS bets_won,
    SUM(p.kelly_fraction * 1000) AS stake_total,  -- stake = $1000 unit × kelly_fraction
    -- … etc per the input-measure list above
  FROM predictions p
  JOIN games g ON g.id = p.game_id
  WHERE g.completed = TRUE
  GROUP BY p.model_version, g.season, p.recommended_book, edge_bucket
" --json > /Users/edwinlovettiii/Projects/mc-v2/crates/mc-fixtures-claw/data/claw_backtest_2026-05-01.csv
```

The CSV is checked into the repo. Fixture build is deterministic, network-free, and Cargo-buildable. Production claw-D1 stays untouched.

The `csv` crate is currently NOT in `mc-fixtures` deps; adding it to `mc-fixtures-claw` only is fine (CLAUDE.md §1's banned-deps list permits crates that aren't `serde`/`tokio`/`async-std`/`rayon`/`anyhow`).

---

## Why this might work — the planning use cases

These are NOT analytics queries (claw already does analytics). These are *forward-looking what-if* questions where MC's writeback + scenarios + snapshot/rollback are load-bearing:

### 1. Bankroll-strategy what-ifs

> *"If I had used Half-Kelly instead of Quarter-Kelly across all OOD-season Sharp-book bets, what would my Net_PnL and ROI have been?"*

- Write a `Stake_Multiplier` input on the `Half_Kelly` Scenario.
- Read consolidated `(Half_Kelly, Working, FY_All_OOD, Sharp, Totals, All_Edges, ROI)` and `…Net_PnL`.
- Compare to `Baseline_Lasso100` at the same coord.
- Snapshot/rollback isolates the experiment so the original numbers come back.

Today this is a Python re-run of the EXP-013 simulator with new args. With MC it's three writes + a consolidated read.

### 2. Edge-threshold sweeps

> *"What ROI would I have realized at 5% / 7.5% / 10% / 12.5% edge gates?"*

EXP-015 already swept some of this; what MC adds is **interactive composition**. The `EdgeGate10`, `EVGate2pct` Scenarios live as cube cells; you can read consolidated metrics by Scenario × Season × Edge_Bucket without re-running the simulator. Adding a new gate means writing one Scenario's input cells.

### 3. Sportsbook-tier filter scenarios

> *"What's the CLV impact of avoiding Soft books entirely (DraftKings, BetMGM, Caesars)?"*

The `Sharp_Books_Only` Scenario writes only Sharp/Mid bets; its consolidated `CLV_Avg` and `ROI` are reads against `(Sharp_Books_Only, Working, FY_All_OOD, All_Books, Totals, All_Edges, *)`. Compare to `Baseline_Lasso100`'s same coord. The hierarchical cube structure means this is a **single coordinate read**, not a re-aggregation.

### 4. Calibration-on/off counterfactual

> *"What would my Hit_Rate and PnL have been WITHOUT EXP-016's calibration map?"*

- `Calibrated_Only` Scenario uses the post-2026-04-30 calibrated probs.
- `Baseline_Lasso100` uses the pre-calibration probs.
- Read both at `(*, Working, FY_All_OOD, *, *, *, Hit_Rate)` and diff.

This is the test that turns claw's "Brier 0.289 → 0.250" headline into a $-denominated impact.

### 5. (Bonus) Production strategy validation

> *"Is the current 90/10 Lasso/XGB ensemble still better than Lasso 100/0 on the latest OOD season?"*

EXP-015 says XGBoost adds zero measurable OOD value. As new OOD data accrues (live 2026-04-29 onwards), this question keeps coming up. With MC, it's a Scenario read every time `kb-settlement-report` lands new data; today it's a Python re-run.

### A real planning workflow MC could plug into

`claw-core` already has a daily flow:
- `kb-daily-predictions` (12:20pm ET) — generate the day's slate
- `kb-confirmation-gate` (6:35pm ET) — apply ESPN injury + lineup confirmation
- `kb-settlement-report` (9am ET) — compute realized PnL

The settlement report is the natural MC integration point: a daily cube `cube.write` that updates `Bets_Placed/Won/Stake_Total/Returns_Total` for `(Baseline_Lasso100, Working, current_season, …)`, then a `cube.read` against the question-of-the-day coord. The KellyBets iMessage agent could send the answer.

That last paragraph is **speculative** and beyond the proposal's scope. Including it because it's where "MC becomes useful" lands if the fixture proves out.

---

## Why this might NOT work — honest red flags

### 1. Planning-vs-analytics framing is doing a lot of load-bearing work

If a reviewer pushes on "name a planning question that doesn't reduce to a SQL query against `claw-edge-db`," I have specific answers (the Sharp_Books_Only counterfactual, the calibration-on/off impact, the Half_Kelly sweep) — but they're all *one-shot questions*, not recurring workflows. A reasonable reviewer might say: "Just write the SQL." If the planning superpowers (writeback + scenarios + snapshot) only matter for one-off analyses, MC may be the wrong tool.

The real test: would the user reach for MC daily? Or would MC be a "ran it once, validated, never opened again" tool? The KellyBets-integration angle in the previous section is the one path I see to "daily." It's also the most speculative.

### 2. Per-cell granularity may be wrong

The proposed cube aggregates up-front (per-Scenario, per-Season, per-Sportsbook, per-Edge_Bucket) before writing to MC. That means MC never sees individual games. Pros: stays sparse-but-bounded; planning questions stay tractable; matches MC's multidim-planning pattern. Cons: any question requiring per-game detail (e.g., "what about the Lakers @ Celtics game on 2024-12-25?") falls back to claw's D1.

A reviewer might argue the right granularity is **per-game**, which would push MC toward ~600K cells and turn the fixture into a single-purpose stress-test rather than a planning tool. I think aggregated-up-front is correct, but I'm not certain.

### 3. Phase 2A's PERF.md §8.1 finding may already be the bottleneck

PERF.md §8.1 + §6.10 measured the per-mark cost on Acme at ~712 ns/mark vs ~98 ns/mark on the synthetic minimal-hierarchy fixture. The 7× gap is dominated by 6-element `CellCoordinate` allocation + AHashSet insert.

A 7-dim cube (claw) widens the `CellCoordinate` to 7 ElementIds, raising per-mark cost. **Proposed cube would have higher per-write latency than Acme even without bigger fan-out.** That makes claw a stricter stress test, but it's worth verifying the cube is actually buildable in reasonable time before committing to the planning workflow.

### 4. The "validate the kernel against external ground truth" angle is weaker than it sounds

Claw's EXP-013 simulator computes ROI/PnL/CLV in pandas. A reviewer could fairly say: *if MC reproduces those numbers, the only validation is "addition and division work" — which we knew.* Real cross-validation would require MC to compute something pandas DOESN'T, e.g., a multidimensional rollup that pandas would express as a tower of `GROUP BY ... ROLLUP`. The cube structure does that natively, so this is a real (if narrow) validation. But it's not the headline I'd lead with.

### 5. Adding `csv` as a workspace dep is a CLAUDE.md §1 "are you sure" moment

The brief's banned-deps list permits `csv` (it's not in §1's exclusion list and it's not async / serde / etc.), but the spirit of CLAUDE.md §7.1 is "default to no new deps." I'd want a reviewer to validate that adding `csv` to a NEW crate (`mc-fixtures-claw`) is acceptable, vs. hand-rolling a parser, vs. exporting to a Rust source file (e.g., `data/claw_backtest.rs` that's literally `pub const ROWS: &[(...)] = &[…];`). Hand-rolled or const-data is uglier but keeps deps zero.

### 6. claw-core may not stay shape-stable

claw is an active production system. Schema migration 019 was the last; migration 020 could land tomorrow with new columns. An MC fixture pinned to a 2026-05-01 export becomes stale unless someone runs the export periodically. Acme is internal-spec; claw is external-real. That's a maintenance burden that didn't exist before.

A reviewer might reasonably say: pin the CSV, document the export procedure, and accept the staleness. Or use `claw-core/training/artifacts/exp013_v16_production_replay.parquet` (per `training/exp013_v16_production_replay.py`) which is a pinned historical artifact.

---

## Things a reviewer should specifically validate

1. **Aggregation choices.** Is `WeightedAverage(weight=Stake_Total)` for `ROI` actually what MC's consolidator produces, given the rule body is `Net_PnL / Stake_Total` and the per-leaf result is consolidated? Spec §11 walks through this for CPC; I think ROI is structurally identical but I'd want a second pass.
2. **Dimensional design.** Is 7 dims overkill? Could `Edge_Bucket` be folded into `Scenario` (each strategy has its own edge gate), dropping a dim and matching Acme's 6? My instinct says no (Edge_Bucket is orthogonal to Strategy), but I'm willing to be wrong.
3. **Rule-chain depth.** Does adding `Net_PnL_per_Bet` and `Edge_Realized` (to reach depth-3 rule chains) buy enough stress-test value to justify the extra measures, or is depth-2 fine?
4. **Sparse-leaf handling.** Many `(Scenario, Season, Sportsbook, Market, Edge_Bucket)` combinations will have zero bets. MC's `read_input_leaf` returns `ScalarValue::Null` with `Provenance::Default { reason: "no input written" }` when the cell is absent (per `cube.rs:328-358`). The consolidator handles Null per spec §7. **Is there a known bug or edge case in this path that an Acme-shaped fixture wouldn't surface?** I haven't checked; a reviewer with kernel-internals familiarity should.
5. **Planning workflow legitimacy.** Are the 4 use cases (Half_Kelly, Edge sweep, Sharp_Books_Only, Calibration-on/off) genuinely planning questions, or are they better expressed as Python re-runs against claw's existing simulator? Pressure-test specifically use case #4 (calibration-on/off) — this is the one I'm least sure adds MC value over a 50-line pandas script.
6. **Effort estimate honesty.** I claimed ~1 week. A reviewer who has built a similar fixture (or has built `mc-fixtures` itself) — does that match their intuition? I'm worried about: (a) golden-value generation taking longer than expected because pandas-vs-MC float arithmetic disagrees at the 1e-9 level; (b) sparse-leaf assertion patterns being more involved than Acme's dense-grid `t_acme_*` tests.

---

## Comparison to alternatives

### Alternative A: Phase 2B only (~½ day)

Land `Arc<Hierarchy>` consolidation fast-path, re-run `cargo bench --workspace`, append PERF.md §6.7 update. Closes the 3-leaf 1B target miss (14.3 µs → expected sub-µs). **Pros:** data-justified by current measurements; small surface area; pure perf win. **Cons:** doesn't broaden the kernel's tested envelope; doesn't add a planning workflow.

### Alternative B: Phase 2B then claw fixture (~1 week + ½ day)

Sequence: 2B first (½ day), claw fixture next (~3-4 days for stress-test-only or ~1 week with planning integration tests). **Pros:** the 2B win shows up in claw's bench numbers, validating the optimization across two fixtures; claw-fixture stress test is more credible after 2B because the kernel is fast enough. **Cons:** ~1.5× the time of A; the planning-workflow hypothesis is still unvalidated until a 2nd-instance review.

### Alternative C: Skip claw fixture; do Phase 2B + a different Phase 2C

E.g., Phase 2C could be the §9.3 hierarchy-mark closure cost reduction (bitset-backed dirty tracker), which PERF.md identifies as the next data-justified candidate. **Pros:** every Phase 2 sub-phase is data-driven from PERF.md; no new fixture maintenance burden. **Cons:** doesn't address the "Acme-only fixture" risk; doesn't dogfood MC on real planning data.

### Alternative D: Do nothing (don't add a second fixture; declare Phase 1 done; close the project until a real planning consumer materializes)

**Pros:** smallest scope; matches CLAUDE.md §7's "Probably don't add it" default. **Cons:** the "Acme is the only fixture" risk persists; MC is feature-complete but unproven on anything other than the brief's demo.

### My (uncertain) recommendation

**B**, contingent on a second-instance validation that the planning workflow is real. If the reviewer says "the use cases reduce to SQL," fall back to **A** + revisit. If the reviewer says "use cases are real and worth the maintenance burden," do **B**.

---

## Open questions

1. **Should `mc-fixtures-claw` ship with the cube data or fetch it?** If shipped: deterministic, network-free, Cargo-buildable; staleness risk. If fetched: always-fresh; needs internet + auth + introduces a `tokio` dep (forbidden).
   *My answer:* ship the CSV; document the export procedure; accept staleness.
2. **Is `csv` as a dep acceptable, or hand-roll a parser?** Csv crate adds ~3 transitive deps (`memchr`, `serde-adjacent`, etc.) which we'd want to audit. A 100-line hand-rolled parser is achievable.
   *My answer:* hand-rolled parser. Keeps deps zero; CSV format is fixed and small.
3. **Where does `Claw_Backtest` live in the repo?** New crate `mc-fixtures-claw/` (parallel to `mc-fixtures/`) vs. an additional builder inside `mc-fixtures/lib.rs` (next to `build_acme_cube` and `build_minimal_cube`).
   *My answer:* new crate. Keeps `mc-fixtures` Acme-pure; isolates the `csv`-or-hand-rolled-parser dep; allows `mc-fixtures-claw` to be optional in the build graph.
4. **Should the planning workflow integration tests live under `mc-core/tests/`?** That violates the Phase 2A handoff's "no `crates/mc-core/tests/` modifications" rule but Phase 2A is over and that rule was Phase 2A-scoped. Phase 2C / Phase 3 can add tests freely.
   *My answer:* yes, under `mc-core/tests/claw_planning.rs` once the fixture exists. The discipline that protected Phase 2A doesn't protect against legitimate new test files.
5. **Does the proposal need a second PRD-style document in `docs/product/`?** That folder is "historical" per its README. The proposal isn't product framing for a new spec; it's a research/exploration question.
   *My answer:* no. This research-note is the right shape; if it gets adopted, the *result* lands as a Phase 2C handoff in `docs/handoffs/` and a completion report in `docs/reports/`.
6. **Does `Edge_Bucket = >20pct` actually exist in claw OOD data?** EXP-015's edge-tier doctrine table shows `>20%: ~330 OOD bets` — yes. Will check before fixture lands.

---

## Where it shows up in the engine (today: nowhere)

- **Source:** N/A — proposal not adopted.
- **Tests:** N/A.
- **Spec:** would extend [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) §4 (fixtures) if adopted; no spec change today.
- **ADR:** would warrant `decisions/0002-second-fixture.md` if adopted, capturing the maintenance-burden tradeoff.

## Related notes

- [`./snapshot-as-deep-clone.md`](./snapshot-as-deep-clone.md) — the snapshot/rollback mechanic this proposal leans on for use case #1.
- [`./weighted-average-consolidation.md`](./weighted-average-consolidation.md) — the consolidation pattern the proposed `ROI`/`Hit_Rate` measures rely on; structurally identical to Acme's CPC.
- [`./two-caching-layers-in-read.md`](./two-caching-layers-in-read.md) — the cold/warm distinction the proposed claw cold-consolidation bench would extend to a 7-dim cube.
- [`../PERF.md`](../PERF.md) §6.7–§6.10 — the Phase 2A baseline this proposal would build on.
- [`../../../claw-core/docs/CURRENT_STATE.md`](../../../claw-core/docs/CURRENT_STATE.md) — claw's authoritative state (read first if validating this proposal).
- [`../../../claw-core/docs/HANDOFF.md`](../../../claw-core/docs/HANDOFF.md) — claw's handoff doc; companion to its CURRENT_STATE.

## History

- 2026-05-01 — created during a Phase 2A wrap-up conversation. Not adopted. Awaiting second-instance review.
