# ADR-0035: Phase 10F — `mc model simulate` (Chronological Bankroll Simulation)

**Status:** Accepted; Phase 10F shipped (`6e2d3e9` merged `7462c22`); Phase 10F.1 patch pending (Amdts 18-19). 19 acceptance amendments total — see bottom.
**Date:** 2026-05-27
**Accepted:** 2026-05-27 (project owner approved after dual external review pass)
**Last amended:** 2026-05-27 — Amdts 18-19 (Phase 10F.1) added after claw-core's first production use surfaced a **38% overstatement** from push mis-scoring (their `won`-0/1 column scored integer-line pushes as wins). 18 = auto-derive-pushes default + win_rate excludes pushes + harder legacy-binary warning; 19 = `--max-stake` + count-based `--window first:n` (EXP-029d/e gaps). Amdt 17 (`--replay`) added during 10F pre-flight. Amdts 1-16 from dual review. Same-timestamp prevalence: 3,273/7,272 (45%).
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
  [--replay batch|sequential] \
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

---

## Acceptance amendments

Filed 2026-05-27 after dual external review (Claude Desktop + GPT-5.1,
high-effort thinking). Both returned **accept-with-amendments**. GPT
proposed 10; Desktop endorsed all 10 and added 6. All 16 are **binding**
for implementation and override the body where they conflict. None change
the architecture (records-in, CLI-only, zero-mc-core, `mc model simulate`
namespace, closed sizing vocabulary all stand) — they tighten safety
semantics, edge cases, and ambiguities. The amendments concentrate on
"what happens at the edges" (ties, bankruptcy, empty data, RNG, output
invariants) — exactly where a betting tool produces wrong-but-plausible
headline numbers.

**Codebase grounding (verified before adoption):**
- **Same-timestamp prevalence (Amendment 1 — the load-bearing one):**
  in claw-core's real `exp028_bets.parquet`, **3,273 of 7,272 bets (45%)
  share a `commence_time` with another bet; max 9 bets at one timestamp.**
  This is not an edge case — arbitrary `bet_id` tiebreak ordering would
  make the path-dependent bankroll non-deterministic on nearly half the
  input. Amendment 1 is mandatory.
- **DuckDB availability (Amendment 4):** `mc-cli` depends on `mc-drivers`
  (Cargo.toml:20), which pins `duckdb = "=1.1.1"` (bundled, Rust-1.78-
  compatible — Cargo.toml:48). Parquet input reuses this path; no new
  Arrow/parquet dependency.

### Amendment 1: Same-timestamp bets = simultaneous batch (CRITICAL)

**Problem.** The body sorts by `(timestamp, bet_id)`, but bankroll sizing
is path-dependent — if N bets share a timestamp, arbitrary `bet_id` order
changes every stake. Verified: 45% of claw-core's bets share a timestamp.

**Amendment.** Same-timestamp bets are a **simultaneous batch** by default:
all stakes in the batch are computed from the bankroll **as of the batch's
start** (before any of the batch's outcomes apply), then all outcomes are
applied atomically to produce the bankroll for the next batch. This models
"these bets were all placed at once from the same bankroll" — the honest
reading of same-commence-time games. An optional `sequence` column (when
present in the records) enables true sequential replay (each bet sees the
bankroll after the prior bet), for consumers who genuinely placed bets in
a known order within a timestamp. Update Decision 2 and Decision 5 with
the batch-vs-sequential rule. **Batch sizing is the default; `sequence`
opts into sequential.**

### Amendment 2: 4-state outcome REQUIRED; binary behind explicit flag; --derive-pushes repair

**Problem.** The body accepted `won`-0/1 with a warning. 295 pushes are
folded into `won` in the real file. Warning-only repeats the ADR-0034
Wilson-Null mistake — a wrong bankroll headline is worse than a hard error.

**Amendment.** Canonical input **requires** a 4-state `outcome` column.
Binary 0/1 input **hard-errors** unless `--outcome-mode legacy-binary` is
explicitly passed (which scores 1→win, 0→loss and stamps `outcome_mode:
"legacy-binary"` in the output so the approximation is visible). When both
`actual_total` and `line` columns are present, support an explicit
`--derive-pushes actual_total=line` repair path that reconstructs the
4-state outcome (actual==line → push, else win/loss by side). Update
Decision 3 and acceptance criterion 3.

### Amendment 3: Bankruptcy / no-borrowing semantics

**Problem.** The body never specified what happens when bankroll falls
below the computed stake.

**Amendment.** No margin, no borrowing:
- Stake is **capped at current bankroll** (you can't bet more than you have).
- Bankroll **cannot go negative**.
- When bankroll reaches **≤ 0**, set `ruin: true` and `ruin_index: <bet
  index>`; **skip all remaining bets** (they're not placed — the bankroll
  is gone). The curve ends at the ruin row.
- For batch semantics (Amendment 1): if the batch's total stake would
  exceed bankroll, stakes are scaled down proportionally OR the batch is
  capped — **default: scale proportionally** so the batch never stakes
  more than the bankroll-at-batch-start.

Add to Decision 5 and a new acceptance criterion.

### Amendment 4: Parquet input via existing DuckDB; jsonl curve output in v1

**Problem.** The body listed parquet-reader options as open. Verified:
DuckDB is already in the tree via `mc-drivers`.

**Amendment.** Parquet **input** uses the existing DuckDB path through
`mc-drivers` — **no new Arrow/parquet dependency**. Curve **output**
(`--emit-curve`) is **jsonl in v1** (dependency-free write). A
DuckDB-backed parquet curve writer is deferred to v1.1 unless explicitly
needed. Update Decision 4 and Decision 9. (The output curve still matches
`exp029_bankroll_curve.parquet`'s *columns*; only the serialization is
jsonl in v1.)

### Amendment 5: Pinned RNG algorithm

**Problem.** "Seeded determinism" without a named algorithm produces
cross-platform inconsistency (different `rand` versions, different
backends → different draws from the same seed).

**Amendment.** Pin a stable, self-contained PRNG implemented in `mc-cli`
(suggest **splitmix64** or **xoshiro256\*\***) — NOT the `rand` crate
(avoids a dependency + version-drift). Seed is **required** when
`--monte-carlo` is set. Same seed → byte-identical output on every
platform. Document the algorithm choice in Decision 6. (If the implementer
strongly prefers `rand`, that needs an explicit dependency justification
surfaced in chat — default is the hand-rolled PRNG, consistent with the
no-new-deps discipline.)

### Amendment 6: Bootstrap resampling details

**Problem.** Block-resampling semantics were thin (block definition,
partial blocks, overlap, default length, percentile method).

**Amendment.** Specify:
- Resample **sample length = the filtered/windowed path length** (each
  bootstrap replay has the same number of bets as the real path).
- `iid`: draw bets with replacement from the filtered pool.
- `block:<len>`: **non-overlapping fixed blocks** of length `len` drawn
  from contiguous time-ordered bets; concatenate blocks then **truncate**
  to the sample length.
- Default block length `L = max(1, round(sqrt(N)))` where N = filtered
  pool size.
- Default `--resample iid`.
- Percentile method: **nearest-rank** (fixed; document it so CIs are
  reproducible).
- References to moving-block-bootstrap variants are deferred.

Update Decision 6.

### Amendment 7: Metric edge cases

**Problem.** `recovery_bets = ∞` is not JSON-safe; Sharpe/ROI undefined
cases unspecified.

**Amendment.** Define:
- `recovery_bets`: integer when recovered; **null** when never recovered,
  paired with `recovery_status: "recovered" | "unrecovered" |
  "never_underwater"`. Never emit `∞`.
- `sharpe`: per-bet return basis = `pnl / stake` per placed bet; uses
  sample std (ddof=1, consistent with ADR-0033 Amendment 3); **null** when
  `n_bets < 2` or stddev == 0 (`abs() < 1e-300`).
- `roi` cumulative (see Amendment 13); `total_staked == 0` → ROI metrics
  are **null**.
- Pushes count as placed bets with zero return (contribute to `n_bets`,
  zero to pnl); voids excluded from `n_bets` and pnl; ruin-skipped bets
  excluded.

Add to Decision 7.

### Amendment 8: `stake_hint` is an explicit sizing rule, not auto-detect

**Problem.** The body listed `stake_hint` as an optional column that
bypasses `--sizing` — implicit behavior triggered by a column happening
to exist. That's the footgun class the whole amendment set guards against.

**Amendment.** Remove `stake_hint` from the auto-detected optional-column
list. Pre-computed stakes are used **only** via an explicit
`--sizing from_column:stake_hint` rule. A `stake_hint` column present in
the records is otherwise ignored. Update Decision 4; remove from
Decision 2's optional-column auto-detect.

### Amendment 9: `--odds` grammar explicit; applies to BOTH sizing and settlement

**Problem.** The body was ambiguous about whether `--odds` overrides odds
for Kelly sizing only, or also for payout settlement.

**Amendment.** `--odds` grammar: `--odds fixed:<decimal>` or
`--odds column:<name>`. The resolved odds apply to **both** the Kelly
stake computation AND the win-payout settlement (`win → stake × (odds −
1)`). A mismatch between sizing-odds and settlement-odds would be a
correctness bug; they're the same value. Update Decision 4 and Decision 5.

### Amendment 10: `mc model simulate` namespace retained; documented as model-adjacent

**Amendment.** Keep `mc model simulate` (the records are
model-evaluation artifacts; the namespace is correct). Document in
Decision 1 that this verb consumes a **model-adjacent artifact** (a
bet-record file) rather than reading the cube directly — the cartridge,
when supplied, is a validator/provenance source, not the primary input.
A top-level `mc simulate` alias is deferred (additive, no demand yet).

### Amendment 11: Cartridge validation = column-name provenance only

**Problem.** Decision 1's "provenance check" language was ambiguous
between loose (column names match measures), medium (names + types), and
strict (cryptographic — records signed by the cartridge that produced
them).

**Amendment.** When a cartridge is supplied, validation is **column-name
provenance only**, and it is **warn-only, never a hard error**. The
validator notes any referenced column that does NOT correspond to a
declared cartridge measure, but does not block the run. **Cryptographic
provenance** (record file signed by the cartridge version that produced
it) is deferred to future Grout integration. Update Decision 1.

**Reconciliation (added 2026-05-27 post-implementation — A11 self-correction).**
This amendment originally said "unknown reference → error" in one
sentence while saying "warn, don't hard-block" in the next — an internal
contradiction. The 10F implementer correctly flagged it: Decision 1's own
worked example passes a cartridge while `--filter`-ing on `abs_edge_pp`
and stratifying on `season` — both bet-record columns that are NOT cube
measures. Hard-erroring on unknown references would break the documented
invocation. **The resolution is warn-only, full stop:** bet-record columns
legitimately exceed the cartridge's measure set (the records carry
outcomes, odds, edges the cube never had), so an "unknown column" is the
normal case, not an error. The cartridge-when-present is a provenance
*hint* surfaced in `schema_mapping` + warnings, not a gate. As-shipped
behavior (warn-only) is the correct behavior; this text is corrected to
match. AC #33 reflects warn-only.

### Amendment 12: Filter/window order of operations locked

**Problem.** `--filter` + `--window first:30` could mean "first 30 of
filtered" or "filter the first 30 chronologically" — different bet sets.

**Amendment.** **`--filter` applies first; `--window` selects from the
filtered pool.** So `--filter "abs_edge_pp >= 0.10" --window first:30`
yields the first 30 bets (chronologically) from the edge-filtered universe
— the EXP-029e "first 30 bets" intent. Document in Decision 5 and add to
acceptance criteria.

### Amendment 13: `roi` is cumulative; `roi_per_bet` separate

**Problem.** Decision 7 listed `roi` without defining it for the bankroll
context. claw-core's headline "+196%" is cumulative ROI; grade's `roi`
(via `ratio`) is per-bet (sum(pnl)/sum(stake)). Two different numbers.

**Amendment.** In simulate, **`roi` = cumulative**: `(final_bank −
start_bank) / start_bank` — the path-dependent, compounding headline
number. Add `roi_per_bet = total_pnl / total_staked` as a **separate**
metric for grade-compatibility. Document the distinction loudly in
Decision 7 and the cookbook (the same metric name means different things
in grade vs simulate — this MUST be unambiguous or the headline number
disappears).

### Amendment 14: Curve invariants

**Problem.** `--emit-curve`'s output invariants were unstated (empty pool?
pushes included? ruin?).

**Amendment.** Curve invariants:
- **One row per placed bet** — pushes INCLUDED (bankroll unchanged row);
  voids and ruin-skipped bets EXCLUDED.
- `bankroll_after` reflects state after this bet (unchanged for pushes).
- For batch timestamps (Amendment 1): each bet in the batch gets a row;
  `bankroll_after` on intra-batch rows reflects the batch-end bankroll
  (since the batch applies atomically) — OR carries a `batch_id` so
  consumers can see the grouping. **Default: stamp `batch_id`; intra-batch
  `bankroll_after` = batch-end bankroll.**
- Ruined simulations: curve ends at the ruin row.
- Empty filtered/windowed pool → empty curve (header/schema only, zero
  data rows) + a run-level warning.

Update Decision 9.

### Amendment 15: EXP-049 reproduction tolerance

**Problem.** Acceptance criterion 12 said "within tolerance" without a
number. Floating-point accumulation across 1,508 bets drifts.

**Amendment.** EXP-049 reproduction: **final bankroll within 0.1%** of
claw-core's reported value (floating-point accumulation tolerance); curve
compared at **start, end, and 5 interior checkpoints** to within 0.01%
each. Matches the precision regime of the reported headline (like
ADR-0033's Wilson 1e-3 headline tolerance). Update acceptance criterion 12.

**Caveat (load-bearing for the repro):** claw-core's `exp028_bets.parquet`
is `won`-0/1 with pushes folded in. To reproduce EXP-049 exactly, the
repro test must run in `--outcome-mode legacy-binary` (matching how the
original number was computed) OR use `--derive-pushes` and accept that the
push-accurate number will differ slightly from the legacy headline. The
test should pin which mode it uses and reproduce THAT number. Document
this in the test.

### Amendment 16: Output JSON expansion

**Amendment.** The JSON output (Decision 9) includes: `warnings` (array),
`outcome_counts` ({win, loss, push, void} tallies), `skip_counts`
({below_min_odds, ruin_skipped, ...}), `ruin` (bool) + `ruin_index`,
`recovery_status`, `curve_path` (when `--emit-curve`), `input_format`
(parquet/jsonl), `schema_mapping` (the resolved column aliases),
`outcome_mode` (canonical/legacy-binary), and the full run config
(sizing, filter, window, seed, odds). This is the codegen contract for
claw-core's Worker — every invalid-evaluation state and every config
input must be machine-readable. Update Decision 9.

### Amendment 17: `--replay batch|sequential` flag (surfaced during 10F implementation)

**Problem (surfaced by the implementer pre-flight).** A1 makes
simultaneous-batch the default. But claw-core's $2,962.16 EXP-049
headline was computed **sequentially** — their Python iterated the
dataframe in row order, compounding each bet even within a same-timestamp
slate. Batch passes the 0.1% final-bank tolerance (0.067% on 2025) but
will NOT match the 0.01% interior checkpoints or claw-core's
peak/max_drawdown, because the curves diverge whenever a timestamp holds
multiple bets (45% of bets). The EXP-049 repro test (AC #12) cannot
reproduce the famous number under the batch default.

**The finding underneath.** Batch is the *more financially honest* model,
not merely the more deterministic one (A1's rationale). Same-commence-time
games are simultaneous — you size all bets on a 7:05pm slate from your
current bankroll because none have resolved when you place them.
Sequential intra-slate compounding pretends you knew bet 1's outcome
before sizing bet 2. claw-core's headline is therefore slightly inflated;
the batch number is the achievable one.

**Amendment.** Add an explicit `--replay batch|sequential` flag,
**default `batch`** (A1 preserved untouched):
- `batch`: A1 simultaneous-batch semantics — the realistic default.
- `sequential`: stable-sort by timestamp, compound each bet in order.
  Intra-timestamp order = the `sequence` column if present (A1's
  mechanism), else **stable file row order** (NOT re-sorted by `bet_id`
  — preserves input order; deterministic for a given file).

The `--replay` flag and the `sequence` column are complementary, not
redundant: `sequence` answers "within a batch, what order?" (fine-grained);
`--replay sequential` answers "compound within timestamps at all?"
(global toggle — claw-core's actual need). `sequential` mode composes the
`sequence` column underneath it when present.

**EXP-049 repro (AC #12) runs `--replay sequential --outcome-mode
legacy-binary`** against claw-core's *real, unmodified* file →
reproduces $2,962.16 + interior checkpoints + peak/max_drawdown exactly.
No doctored fixture, no forcing claw-core to add a column to reproduce
their own number.

**Cookbook documents both numbers**: the batch number (realistic, what's
achievable — the recommended default for new analysis) and the sequential
number (reproduces legacy headlines). Note that V1.1 gating may want to
re-baseline against the batch number since that's the achievable one.

Small additive scope (~15 lines + the flag). Update Decision 1 (command
shape), Decision 5 (replay), and the cookbook.

### Amendment 18: Auto-derive pushes when score columns present; legacy-binary stops being the silent default (surfaced by claw-core production use — Phase 10F.1)

**The catch that motivated this.** claw-core ran the shipped simulate on
their real `exp028_bets.parquet` (a `won`-0/1 file → `legacy-binary`
mode) and `--derive-pushes` surfaced a **38% overstatement** in their own
published numbers. Integer-line games landing exactly on the line
(`actual_total == line`) are pushes (stake returned, neutral) — but their
`won` column scored them as WINS for UNDER bets (24 of 26 in 2025). The
model is UNDER-heavy, so phantom-push-wins compounded all season:
2025 V1.0 went from a published $2,962 (legacy-binary, pushes-as-wins) to
**$1,829 push-accurate (−38%)**. The error propagated through 8
experiments. (For contrast: the batch-vs-sequential correction from A1/A17
was ~$2 / 0.07% on the same data — the push correction is ~500× larger.)

**The defect this exposes.** A2 correctly made 4-state canonical and
`legacy-binary` an explicit opt-in. But once a user IS in `legacy-binary`
(which any `won`-0/1 file forces), pushes-as-wins is silent AND
`--derive-pushes` is opt-in — so when the data to detect pushes is *right
there* (`actual_total` + `line` columns both present, as in claw-core's
file), simulate makes the user ASK for the correct number instead of
giving it by default. That's the "silently-wrong default" footgun the
whole amendment set guards against (cf. A2, A3, and ADR-0034's
Wilson-Null hard-error).

**Amendment.** Push-accuracy becomes the default whenever it's derivable:

1. **Auto-derive pushes when both score columns are present.** If the
   records carry columns that can express a push (default detection:
   `actual_total` + `line`, or whatever the records' canonical score/line
   columns are), simulate **auto-derives pushes** (`actual == line` →
   push) regardless of `--outcome-mode`. Opt out with an explicit
   `--no-derive-pushes`. The existing `--derive-pushes <actual>=<line>`
   stays as the way to name non-default column pairs.

2. **`legacy-binary` without derivable pushes warns harder.** When a
   binary `won` column is scored as win/loss AND no push-derivation is
   possible (score columns absent), the warning escalates: it states that
   pushes are being counted as wins/losses and that the bankroll is
   therefore **inaccurate, not just approximate** — with the magnitude
   framing ("any integer-line push is mis-scored; for UNDER-heavy models
   this compounds"). Legacy-binary remains available for *reproducing a
   known-published number* (it's how AC #12 reproduces the $2,962
   headline), but it is no longer the path of least resistance for a file
   that *could* be push-accurate.

3. **`win_rate` excludes pushes.** In any mode, `win_rate = wins /
   (wins + losses)` — pushes are NOT counted in the denominator OR as
   wins (they're neutral). Under legacy-binary-without-derivation (pushes
   invisible), `win_rate` carries a caveat in the JSON that it may be
   inflated by undetected pushes. This also flags the knock-on claw-core
   reported: their "59.68% WR" headline (EXP-026/028) used the same `won`
   column and is a few points high — a push-accurate WR re-baseline is
   the fix on their side.

**Reproducibility caveat (load-bearing for AC #12).** The EXP-049
reproduction test (AC #12) reproduces claw-core's *published* $2,962
headline, which was computed pushes-as-wins. So AC #12 MUST run with
`--no-derive-pushes --outcome-mode legacy-binary` to match the
known-wrong-but-published number, and the test comment must say so
explicitly: "reproduces the legacy published number, which is now known
to overstate by ~38% due to push mis-scoring; push-accurate is $1,829.
This test pins reproduction of the historical figure, not the correct
one." The push-accurate number ($1,829) gets its own assertion as the
*correct* result.

**Scope:** Phase 10F.1 patch (mc-cli only, ~40 lines in
`simulate_reader.rs`'s outcome-mode decision + the warning text + the
win_rate denominator). Same instance as 10F (the reader/outcome code is
in its context). Bundled with the two EXP-029-family gaps below.

### Amendment 19: `--max-stake` (EXP-029d) and `--window first:<n>` (EXP-029e) — close the EXP-029-family gaps

**Problem.** claw-core's adoption review found simulate replaces ~80% of
the EXP-029 family but two gaps remain: 029d (bet caps) needs a **fixed-
dollar** stake cap (the existing `cap=` modifier is a *fraction* of
bankroll), and 029e (first-30-bet risk) needs a **count-based** window
(the existing `--window first:<n>` was specified but claw-core flagged it
as needed — confirm it's count-based, not date-based).

**Amendment.** Two small additive flags:
1. **`--max-stake <amount>`** — an absolute-dollar cap applied AFTER the
   sizing rule and the fractional `cap=` modifier (stake = min(sized,
   fractional_cap × bankroll, max_stake)). Reproduces EXP-029d's
   account-limit stress (e.g. "what if no bet can exceed $500 regardless
   of bankroll"). Composes with `--monte-carlo` for the cap × bankroll
   matrix (scripted, per the original cap-matrix deferral).
2. **`--window first:<n>`** — confirm/ensure it's a **count-based** window
   (first N placed bets chronologically, after `--filter`), distinct from
   `range:<a>:<b>` (date-based). EXP-029e (first-30 risk) +
   `--monte-carlo` reproduces the early-window drawdown distribution.

**Scope:** part of the 10F.1 patch. ~30 lines (flag parse + the min() in
sizing + window selection). Bundled with Amendment 18.

---

## Consolidated acceptance-criteria revisions

Body's 24 ACs stand, with these amendment-driven changes + additions:

- **AC #3** (outcome): 4-state required; binary hard-errors unless `--outcome-mode legacy-binary`; `--derive-pushes` repair (Amdt 2)
- **AC #6** (single-path): same-timestamp = simultaneous batch by default; `sequence` column → sequential (Amdt 1)
- **AC #10** (sharpe): sample-std, null on n<2 / zero-stddev (Amdt 7)
- **AC #12** (EXP-049 repro): runs `--replay sequential --outcome-mode legacy-binary --no-derive-pushes` against claw-core's real file → reproduces the *published* $2,962.16 within 0.1% final / 0.01% checkpoints + peak/max_drawdown. Test comment notes this is the known-wrong published figure (overstates ~38% from push mis-scoring); a paired assertion checks the push-accurate $1,829 as the *correct* number (Amdt 15 + Amdt 17 + Amdt 18)
- **AC #14** (curve): invariants per Amdt 14
- **AC #16** (zero mc-core): unchanged; PRNG is hand-rolled in mc-cli (Amdt 5)

New ACs:
- **AC #25:** bankruptcy/ruin — stake capped at bankroll; ruin sets `ruin:true`, skips remaining; batch over-stake scales proportionally (Amdt 3)
- **AC #26:** pinned PRNG (splitmix64/xoshiro256\*\*); same seed → byte-identical across platforms (Amdt 5)
- **AC #27:** bootstrap details — sample length = path length, non-overlapping blocks truncated, default iid, nearest-rank percentiles (Amdt 6)
- **AC #28:** `--odds fixed:|column:` applies to both sizing and settlement (Amdt 9)
- **AC #29:** `--filter` first, `--window` second (Amdt 12)
- **AC #30:** `roi` cumulative; `roi_per_bet` separate metric (Amdt 13)
- **AC #31:** `--sizing from_column:stake_hint` explicit; bare stake_hint column ignored (Amdt 8)
- **AC #32:** JSON exposes warnings/outcome_counts/skip_counts/ruin/recovery_status/schema_mapping/outcome_mode/run-config (Amdt 16)
- **AC #33:** cartridge validation = column-name provenance only, **warn-only never hard-error** (bet-record columns legitimately exceed cube measures); crypto provenance deferred (Amdt 11, reconciled post-implementation)
- **AC #34:** `--replay batch|sequential` (default batch); sequential = stable-sort timestamp, compound in `sequence`-col-or-file order; batch is A1 default. EXP-049 repro uses sequential (Amdt 17)

Phase 10F.1 ACs (push-correctness patch — Amdts 18-19):
- **AC #35:** when score columns are present, pushes are auto-derived by default (`actual == line` → push) regardless of `--outcome-mode`; opt out with `--no-derive-pushes`. A `won`-style binary file WITH score columns produces a push-accurate bankroll without the user asking (Amdt 18)
- **AC #36:** `win_rate = wins / (wins + losses)` — pushes excluded from numerator AND denominator; legacy-binary-without-derivable-pushes carries a JSON caveat that win_rate may be push-inflated (Amdt 18)
- **AC #37:** legacy-binary-without-derivable-pushes warning escalates to state the bankroll is inaccurate (not merely approximate) and names the compounding risk for direction-skewed models (Amdt 18)
- **AC #38:** EXP-049 push-accurate paired assertion — `--derive-pushes` (or default auto-derive) on claw-core's 2025 window yields ~$1,829, asserted as the correct figure alongside the legacy $2,962 (Amdt 18)
- **AC #39:** `--max-stake <amount>` absolute-dollar cap applied after sizing + fractional cap (stake = min(sized, cap×bankroll, max_stake)); reproduces EXP-029d (Amdt 19)
- **AC #40:** `--window first:<n>` is count-based (first N placed bets after filter, chronological); distinct from date-based `range:`; + `--monte-carlo` reproduces EXP-029e (Amdt 19)

---

*End of amendments. Body of ADR above is preserved for audit-trail
purposes; amendments win on conflicts.*
