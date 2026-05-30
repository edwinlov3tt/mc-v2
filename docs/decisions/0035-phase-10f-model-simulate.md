# ADR-0035: Phase 10F — `mc model simulate` (Chronological Bankroll Simulation)

**Status:** Proposed
**Date:** 2026-05-27
**Deciders:** project owner
**Phase:** 10F (third command in the evaluation track; the first that consumes a bet-record file rather than the cube)
**Crate(s) touched:** `mc-cli` (new `simulate` subcommand) + `mc-core` ONLY if a sizing/drawdown helper proves model-semantic (default: none — same discipline as ADR-0034 Amendment 4)
**Prerequisite reading:**
- [ADR-0034](./0034-phase-10b-model-grade.md) — `mc model grade`; simulate is its time-ordered sibling. grade is order-independent map-reduce; simulate is path-dependent replay.
- [ADR-0033](./0033-phase-10a-evaluation-metrics-library.md) — metrics library; `max_drawdown`/`recovery_bets` were explicitly deferred there (Alt 1) "to the phase where bet-record time-series exists." This is that phase.
- [Research note: built-in evaluation primitives](../research-notes/built-in-evaluation-primitives.md) — simulate replaces 5 scripts (EXP-029b/c/d/e/f)
- claw-core `exp028_bets.parquet` (input contract source) + `exp029_bankroll_curve.parquet` (output contract source)

---

## Context

claw-core's headline V1.0 number — "$1k → $2,962, +196%" — does not come
from grade or any order-independent aggregation. It comes from
**chronological bankroll replay**: walk the bets in time order, size each
with path-dependent Kelly state (the stake depends on the current
bankroll, which depends on every prior bet's outcome), track the
bankroll curve, and measure final bank / ROI / max drawdown / recovery.

grade (Phase 10B) structurally cannot express this. Its grouped
map-reduce is order-independent — every reduction (count, mean, Wilson,
ratio) is invariant to bet ordering. Bankroll evolution is the opposite:
order is the whole point, and the result is path-dependent.

claw-core named this as its **#1 next-phase request** after adopting
grade: "EXP-053 V1.1 walk-forward, the whole EXP-029/049/050/051
bankroll family need time-ordered bet replay + path-dependent Kelly
state + max drawdown. The headline numbers that actually gate V1.1 only
come from chronological sim."

This phase ships `mc model simulate`. It also finally gives
`max_drawdown` and `recovery_bets` their home — those were deferred from
Phase 10A (ADR-0033 Alt 1) precisely because they need a time-ordered
scan over chronological records, which is simulate's native shape.

---

## The architectural pivot: simulate consumes records, not the cube

grade, query, sweep, whatif all read **cube state** — they evaluate the
model. simulate is different: it consumes a **bet-record file** (one row
per placed bet, with the prediction, the outcome, and the odds already
resolved) and replays it. It does not evaluate the model.

**Why records-in, not cube-in.** The bet records come from walk-forward
validation — retrain on a growing window, predict the next season,
collect chronological bet records. The retraining is a Python job
(sklearn/PyMC); Mosaic doesn't train. So the records arrive as an
artifact, exactly like Lasso coefficients arrive as an artifact. simulate
is the evaluation half of the Python-trains-Mosaic-evaluates split,
applied to bet records instead of to a cube.

This means **simulate is the first `mc model` verb whose cartridge
argument is optional** — the simulation needs only the records. See
Decision 1 for how the cartridge stays in the signature for provenance
+ validation without being load-bearing.

---

## Decisions

### Decision 1: Command shape; cartridge optional

```
mc model simulate [<cartridge.yaml>] \
  --bets <records.parquet|records.jsonl> \
  --start-bankroll <amount> \
  --sizing <rule>[:param=value,...] \
  [--filter "<predicate>"] \
  [--odds <override>] \
  [--monte-carlo <n>] [--resample iid|block:<len>] \
  [--window all|first:<n>|range:<a>:<b>] \
  [--metric <name> ...] \
  [--seed <int>] \
  [--format text|json] \
  [--emit-curve <path>]
```

The cartridge positional arg is **optional**. When present, simulate
validates the bet-record columns against the cartridge's measure schema
(provenance check — "these records came from this model") and records the
cartridge identity in the output. When absent, simulate runs on the
records alone. The simulation math never reads the cube.

Example (the EXP-049 reproduction — V1.0 bankroll on 2025):

```
mc model simulate examples/sports-betting/mlb-totals.yaml \
  --bets exp028_bets.parquet \
  --start-bankroll 1000 \
  --sizing quarter_kelly:cap=0.025,shrink=0.02 \
  --filter "abs_edge_pp >= 0.10" \
  --window range:2025-01-01:2025-12-31 \
  --metric final_bank --metric roi --metric max_drawdown --metric sharpe \
  --emit-curve bankroll_2025.parquet \
  --format json
```

### Decision 2: The bet-record format (THE load-bearing contract)

This is the central decision. The format is grounded in claw-core's
real `exp028_bets.parquet` (7,272 rows; verified schema). simulate reads
parquet OR jsonl with these columns:

**Required columns:**

| Column | Type | Meaning |
|---|---|---|
| `bet_id` | string/int | Unique bet identifier (claw-core: `game_pk`) |
| `timestamp` | RFC3339 string or epoch | Chronological ordering key (claw-core: `commence_time`) |
| `p_bet_side` | f64 ∈ [0,1] | Model's probability the bet side wins (drives Kelly) |
| `decimal_odds` | f64 > 1.0 | Payout multiple (e.g. 1.909 for -110) |
| `outcome` | enum: `win`/`loss`/`push`/`void` | Bet result (see Decision 3 on push) |

**Optional columns (used by filters / stratification / odds override):**

| Column | Type | Meaning |
|---|---|---|
| `abs_edge_pp` / `edge_pp` | f64 | Edge magnitude (for `--filter`) |
| `side` | string | OVER/UNDER etc (stratification) |
| `season` / any label | any | Grouping for per-stratum sims |
| `stake_hint` | f64 | Pre-computed stake to use verbatim (bypasses `--sizing`) |

**Column aliasing.** claw-core's file uses `game_pk`/`commence_time`/`won`
not `bet_id`/`timestamp`/`outcome`. simulate accepts a `--columns` map
(`--columns bet_id=game_pk,timestamp=commence_time,outcome=won`) OR reads
a sidecar `<records>.schema.json` declaring the mapping. Default: look
for the canonical names, fall back to common aliases
(`game_pk`→`bet_id`, `commence_time`→`timestamp`), error if ambiguous.

**Records MUST be replayable in a deterministic order.** simulate sorts
by `timestamp` ascending before replay (claw-core's file is already
sorted, but simulate does not assume it). Ties broken by `bet_id` for
determinism.

**This format is the contract `walk-forward` (Phase 10E) will emit.**
Designing it now, grounded in the real Python output, means a future
Mosaic-side walk-forward produces the same shape simulate already
consumes — the two phases compose without a format renegotiation.

### Decision 3: Outcome semantics — explicit 4-state, with a `won`-compat path

claw-core's `won` column is 0/1 — **no push state, despite 295 of 7,272
rows having `actual_total == line`** (pushes were folded into `won`
upstream). simulate must not silently inherit that ambiguity.

**The `outcome` column is a 4-state enum:** `win | loss | push | void`.
- `win` → bankroll += stake × (decimal_odds − 1)
- `loss` → bankroll −= stake
- `push` → bankroll unchanged (stake returned); counts as a placed bet
  with zero P&L
- `void` → bet not placed; excluded from count and P&L (e.g. postponed game)

**Compat path for `won`-style 0/1 records:** if the mapped outcome column
is integer 0/1 (not the enum), simulate treats `1`→`win`, `0`→`loss`,
and **emits a warning**: "outcome column is 0/1 binary; pushes/voids
cannot be distinguished and are scored as win/loss. For push-accurate
bankroll, provide a 4-state `outcome` column." This makes the precision
loss visible rather than silent — claw-core's existing file will run, but
the operator learns the 295 pushes are being approximated.

### Decision 4: Sizing-rule vocabulary

A closed vocabulary, grounded in the EXP-029/047 family:

| Rule | Params | Formula |
|---|---|---|
| `flat:pct=X` | pct of start bankroll | stake = X × start_bankroll (constant) |
| `flat_current:pct=X` | pct of current bankroll | stake = X × current_bankroll |
| `kelly:fraction=F` | Kelly fraction | stake = F × kelly(p, odds) × current_bankroll |
| `quarter_kelly` | shorthand for kelly:fraction=0.25 | — |
| `half_kelly` | shorthand for kelly:fraction=0.5 | — |

**Universal modifiers (apply to any rule):**
- `cap=X` — stake capped at X × current_bankroll (claw-core: 0.025)
- `shrink=X` — subtract X from `p` before Kelly (CI haircut; claw-core: 0.02)
- `min_odds=X` — skip bets below X decimal odds
- `floor=X` — minimum stake (below which skip)

Kelly with these modifiers reproduces claw-core's production sizing
(`quarter_kelly:cap=0.025,shrink=0.02`). The full sizing string parses to
a `SizingRule` struct; unknown rule/param → hard error with the valid set.

**Kelly formula (pinned):** for decimal odds `d`, win prob `p` (after
shrink), `b = d − 1`, `kelly_fraction = (b·p − (1−p)) / b`, clamped to
`[0, ∞)` (never bet a negative Kelly — skip instead). Then
`stake_pct = min(F × kelly_fraction, cap)`.

### Decision 5: Single-path simulation (the core)

The deterministic single replay:

1. Sort records by `(timestamp, bet_id)`.
2. Apply `--filter` (a predicate over record columns, reusing grade's
   filter grammar where it fits — abs_edge_pp >= 0.10 etc).
3. Apply `--window` (all / first-N / date-range).
4. `bankroll = start_bankroll`.
5. For each record in order: compute stake via `--sizing`, apply outcome,
   update bankroll, append `(timestamp, bet_id, stake, outcome,
   bankroll_after)` to the curve.
6. Emit the curve (if `--emit-curve`) + the run metrics.

The emitted curve matches claw-core's `exp029_bankroll_curve.parquet`
shape: `timestamp, bet_id, [season/labels], p_bet_side, abs_edge_pp,
stake, outcome, bankroll_after`. Grounded in the real output file.

### Decision 6: Monte Carlo wrapper (resampling)

`--monte-carlo <n>` runs N resampled simulations and reports percentile
distributions instead of a single path. Two resampling modes
(EXP-029b grounding):

- `--resample iid` — bootstrap: draw N bets with replacement from the
  filtered pool, replay, record final metrics. Repeat `n` times.
- `--resample block:<len>` — block bootstrap: draw contiguous blocks of
  `len` bets to preserve local autocorrelation (streaks). Repeat `n`
  times.

Output: per-metric percentile bands (P5/P25/P50/P75/P95) over the `n`
runs, plus the deterministic single-path result as the "actual" baseline.
Determinism via `--seed` (required when `--monte-carlo` is set; the RNG
is seeded so the same seed → identical distributions — mirrors
CLAUDE.md's determinism discipline).

**`--window first:<n>`** composes with Monte Carlo to reproduce EXP-029e
(first-30-bet risk) and EXP-029f (recovery-from-bad-start) — resample
the first-N window and report the drawdown/recovery distribution.

### Decision 7: Metrics — including the deferred drawdown family

The metric vocabulary, with `max_drawdown` and `recovery_bets` finally
landing (deferred from ADR-0033 Alt 1):

| Metric | Definition |
|---|---|
| `final_bank` | bankroll after the last bet |
| `roi` | (final_bank − start_bankroll) / start_bankroll |
| `total_staked` | sum of all stakes |
| `roi_per_dollar` | total P&L / total_staked |
| `n_bets` | count of placed bets (win+loss+push; excludes void) |
| `win_rate` | wins / (wins + losses) |
| `max_drawdown` | largest peak-to-trough decline in the bankroll curve, as a fraction of peak |
| `recovery_bets` | number of bets from the max-drawdown trough back to the prior peak (∞ if never recovered) |
| `sharpe` | mean(per-bet returns) / std(per-bet returns) × √n_bets (uses 10A `std_over` math — sample std) |
| `p_underwater` | (Monte Carlo only) fraction of runs ending below start_bankroll |
| `terminal_p5`/`p50`/`p95` | (Monte Carlo only) percentile final banks |

`max_drawdown` and `recovery_bets` are single-pass scans over the curve —
they're why this metric family had to wait for simulate's time-ordered
structure. Document in the metrics cookbook that these two are
simulate-only (not available in grade's order-independent reductions).

### Decision 8: CLI-only; no daemon; mc-cli implementation

Same disposition as grade (ADR-0034 Amendment 4): simulate is a batch
analytic. CLI-only, no `/api/v1/simulate`. Implementation lives in
`mc-cli`. No `mc-core` change unless a genuinely model-semantic primitive
surfaces (none expected — bankroll replay is reporting logic, not kernel
logic).

### Decision 9: Output — text summary + JSON + optional curve file

**Text** (default): the headline summary claw-core quotes — start/final
bank, ROI, n_bets, win rate, max drawdown, Sharpe; plus the Monte Carlo
band table when `--monte-carlo` is set.

**JSON** (`--format json`): structured, `schema_version` envelope, every
metric + the run config (sizing, filter, window, seed) for
reproducibility. Monte Carlo runs include the per-percentile bands and
the run count.

**Curve** (`--emit-curve <path>`): the per-bet bankroll curve as
parquet/jsonl, matching `exp029_bankroll_curve.parquet`. Enables
downstream plotting and the "show me the equity curve" workflow.

---

## Implementation plan

Estimate: ~4-5 sessions, ~600-700 LOC + tests. The largest command in the
track — single-path replay + Monte Carlo + the drawdown scans + the
record-format reader + column aliasing.

### Step 0: Preflight
- Confirm a parquet reader is available in the workspace (Tessera uses
  `mc-drivers` / duckdb — check whether simulate reuses that or needs a
  lighter parquet path; jsonl is dependency-free fallback)
- Confirm grade's filter grammar is reusable for `--filter` over record
  columns (it operates on cube measures; records are columns — may need a
  thin record-column adapter)
- Diagnostic-code preflight (MC4xxx range)

### Step 1: Bet-record reader + column aliasing (Decision 2)
Parquet + jsonl readers → a normalized `Vec<BetRecord>`. Column-name
resolution (canonical → alias → `--columns` override → sidecar schema).
Validation: required columns present, types correct, odds > 1, p ∈ [0,1].

### Step 2: Outcome normalization (Decision 3)
4-state enum parse; `won`-style 0/1 compat with the warning.

### Step 3: Sizing-rule parser + Kelly (Decision 4)
`SizingRule` struct, parse the rule string + modifiers, the pinned Kelly
formula. Unit-test each rule + modifier against hand-computed stakes.

### Step 4: Single-path replay (Decision 5)
Sort, filter, window, replay loop, curve accumulation. This is the
deterministic core everything else wraps.

### Step 5: Metrics incl. drawdown scans (Decision 7)
final_bank/roi/etc are accumulator reads. max_drawdown + recovery_bets
are single-pass curve scans. sharpe reuses the sample-std math.

### Step 6: Monte Carlo wrapper (Decision 6)
Seeded RNG, iid + block resampling, N replays, percentile aggregation.
Determinism test: same seed → identical bands.

### Step 7: Output (Decision 9)
Text summary, JSON envelope, optional curve emission.

### Step 8: Tests
- Record reader: parquet + jsonl + aliasing + sidecar schema + validation errors
- Outcome: 4-state + 0/1 compat warning
- Sizing: each rule + cap/shrink/min_odds/floor against hand-computed values
- Kelly: known (p, odds) → known fraction; negative-Kelly → skip
- Single-path: a 5-bet fixture → hand-computed final bankroll + curve
- **EXP-049 reproduction**: V1.0 quarter-Kelly + cap on a 2025 fixture →
  matches claw-core's reported final bank within tolerance
- Drawdown: a curve with a known peak-trough → correct max_drawdown + recovery_bets
- Monte Carlo: seeded determinism (same seed → identical), iid vs block differ
- `--window first:30` + monte-carlo → EXP-029e shape
- Push handling: a record set with pushes → bankroll unchanged on push rows

### Step 9: Cookbook + gates
metrics-cookbook.md `mc model simulate` section (sizing rules, the
drawdown-family note, the EXP-049 worked example, the bet-record format
spec). All gates incl. §6.7 quoted test run.

---

## Acceptance criteria

1. Reads parquet + jsonl bet records; column aliasing resolves claw-core's `game_pk`/`commence_time`/`won`
2. Required-column validation; clear errors on missing/malformed
3. 4-state outcome enum; `won`-style 0/1 compat emits the precision-loss warning
4. All sizing rules + modifiers (cap/shrink/min_odds/floor) compute correct stakes
5. Kelly formula matches hand-computed values; negative Kelly → skip
6. Single-path replay: 5-bet fixture → exact hand-computed bankroll + curve
7. `--filter` restricts the bet pool (abs_edge_pp >= 0.10 → 1508 of 7272 on claw-core's file)
8. `--window` all/first-N/date-range all work
9. `max_drawdown` + `recovery_bets` correct on a known curve
10. `sharpe` uses sample-std (ddof=1) consistent with ADR-0033 Amendment 3
11. Monte Carlo: seeded determinism (same seed → identical bands); iid + block both work
12. EXP-049 reproduction: V1.0 sizing on 2025 → final bank within tolerance of claw-core's report
13. Push records leave bankroll unchanged; void records excluded
14. `--emit-curve` produces a file matching `exp029_bankroll_curve.parquet` shape
15. Text + JSON output per Decision 9; JSON has schema_version + run config
16. CLI-only; no daemon; **zero mc-core change** (or surfaced model-semantic justification)
17. Cartridge arg optional; when present, validates record columns against measure schema
18. `cargo test --workspace` passes — **quote the real result line (§6.7)**
19. `cargo clippy --all-targets --workspace -- -D warnings` clean
20. `cargo fmt --check --all` clean
21. No float `==` (CLAUDE.md §3.1); zero-checks via `abs() < 1e-300`
22. Determinism: 10 runs identical (single-path always; Monte Carlo with fixed seed)
23. Metrics cookbook gains a `mc model simulate` section incl. the bet-record format spec + EXP-049 worked example
24. Test YAML/fixtures use single braces (CLAUDE.md §4.5)

---

## Alternatives considered

### Alt 1: simulate regenerates bet records from the cube (no external file)

Considered. simulate evaluates the cartridge over a holdout to produce
predictions + outcomes, then replays them — no external parquet needed.

**Rejected because** the bet records require walk-forward (retrain per
fold), which is a Python job — Mosaic doesn't train. Even single-model
prediction over a holdout wouldn't capture the walk-forward structure
that makes the records honest (no lookahead). The records are a
legitimate artifact from the training pipeline; simulate consumes them,
exactly as `predict()` consumes Lasso coefficients. A future Mosaic-side
walk-forward (Phase 10E) would *produce* records in this format — but
that's a separate phase, and simulate shouldn't block on it.

### Alt 2: Free-form sizing expressions instead of a closed vocabulary

Considered. Let users write `--sizing "min(0.25 * kelly(p, odds), 0.025) * bankroll"`.

**Rejected** for the same reason grade rejected free-form metrics
(ADR-0034): a closed vocabulary covers every EXP-029/047 use case, keeps
the parser tiny, and avoids a sizing-expression mini-language with its own
evaluation semantics. The 5 rules + 4 modifiers span flat, current-flat,
and all Kelly variants claw-core has tested. New rule → add to the
vocabulary with a test, not a DSL.

### Alt 3: Bundle walk-forward (10E) into this phase

Considered. Ship record-generation + simulation together.

**Rejected** — walk-forward needs per-fold retraining (Python) or a
fitted-model-snapshot convention that doesn't exist yet. simulate works
today on claw-core's existing `exp028_bets.parquet`. Shipping simulate
standalone delivers the #1 ask now; walk-forward becomes its own phase
when there's demand for Mosaic to produce records rather than consume
Python's.

### Alt 4: Inherit claw-core's `won` 0/1 as the outcome contract

Considered. Simpler — match the existing file exactly.

**Rejected** — 0/1 silently conflates pushes (295 rows in the real file)
with wins/losses, producing a subtly wrong bankroll. The 4-state enum is
the correct contract; the 0/1 compat path (with warning) lets claw-core's
existing file run while making the precision loss visible. Honest format,
graceful degradation.

### Alt 5: Daemon `/api/v1/simulate`

Considered. Consistency with the Phase 8.2 surface.

**Rejected** for this phase — simulate is offline batch analysis over a
records file, not an interactive operation. Same reasoning as grade. A
Worker that wants sim results shells out. Additive later if an
interactive consumer surfaces.

---

## Out of scope

- `mc model walk-forward` / Mosaic-side record generation (Phase 10E; Alt 3)
- Daemon `/simulate` endpoint (Alt 5)
- Free-form sizing expressions (Alt 2)
- Portfolio / multi-bet-per-day correlated sizing (that's Phase 12B `mc model optimize` — joint Kelly over correlated outcomes; simulate is sequential single-bet)
- Slippage *modeling* beyond the `--odds` column override (EXP-029c's stochastic-odds scenario is a future `--odds stochastic:...` mode if demanded; v1 takes a fixed override or per-row column)
- Bet-cap *matrix* sweeps (EXP-029d's cap × start-bankroll grid — composable by scripting multiple simulate calls; a `--stress-matrix` convenience is deferred)
- Tax / fee modeling
- Live/streaming simulation

---

## Cross-links

- ADR-0034 (grade): the order-independent sibling; simulate is path-dependent
- ADR-0033 (10A metrics): `max_drawdown`/`recovery_bets` deferred there (Alt 1), landing here; sharpe reuses sample-std
- [Research note: built-in evaluation primitives](../research-notes/built-in-evaluation-primitives.md): simulate replaces EXP-029b/c/d/e/f
- claw-core `exp028_bets.parquet`: the input-format source (7,272 rows, verified schema)
- claw-core `exp029_bankroll_curve.parquet`: the output-curve-format source
- claw-core reports EXP-029/047/049: sizing + bankroll + Kelly-fraction findings the sizing vocabulary is grounded in
- CLAUDE.md §3.1 (no float `==`), §4.5 (single-brace test YAML), §6.7 (quoted test run)

---

## Notes

**Why simulate is 10F not 10C.** The research note ordered the commands
grade/backtest/sweep/walk-forward/simulate. claw-core's actual demand
reordered them: grade first (shipped), then simulate (this) — because the
V1.1-gating headline numbers are bankroll numbers, and those are
path-dependent. backtest (parameter sweep × holdout) is the #2 ask and
becomes the next phase after this. Demand-driven sequencing means the
phase numbers reflect dependency + the research-note taxonomy, not ship
order — 10F ships before 10C because the consumer needs it first.

**The bet-record format is the load-bearing artifact.** Get it right and
it serves three masters: simulate consumes it (now), walk-forward emits
it (Phase 10E), and any future cross-tool bet analysis reads it. It's
grounded in claw-core's real `exp028_bets.parquet` so it's not
speculative — but the dual review should pressure-test the column set,
the outcome enum, and the aliasing strategy hardest, because changing it
later breaks the contract.

**This is the biggest evaluation-track command.** grade was ~500 LOC;
simulate is ~600-700 because it carries the record reader, the sizing
vocabulary, the single-path engine, the Monte Carlo wrapper, and the
drawdown scans. Worth keeping the v1 scope disciplined (the Out-of-Scope
list is doing real work — slippage modes, cap matrices, and portfolio
sizing are all deferred to keep this shippable).

**Open questions flagged for dual review:**
1. Is the bet-record column set right? Missing anything EXP-029's family needs? Over-specified?
2. Is the 4-state outcome enum + 0/1-compat-with-warning the right call, or should 0/1 be a hard error (forcing claw-core to re-export with explicit push)?
3. Should `--filter` reuse grade's filter grammar verbatim, or does operating on flat record columns (vs cube measures) warrant a simpler predicate parser?
4. Parquet dependency: reuse `mc-drivers`/duckdb (heavy, already in the tree for Tessera) or a lighter parquet reader for mc-cli? Or jsonl-only for v1 and parquet via a Tessera pre-convert?
5. Is cartridge-optional the right architecture, or should simulate be a top-level `mc simulate` (not under `mc model`) since it doesn't read the model?
6. Monte Carlo seed: required when `--monte-carlo` set (current proposal), or default-seeded with a logged value?
