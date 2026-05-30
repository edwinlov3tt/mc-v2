# ADR-0034: Phase 10B — `mc model grade` (Segmented Holdout Evaluation)

**Status:** Proposed
**Date:** 2026-05-27
**Deciders:** project owner
**Phase:** 10B (second phase of the evaluation-primitives track; first command built on the 10A metrics library)
**Crate(s) touched:** `mc-cli` (new `grade` subcommand) + possibly `mc-core` (grouped-aggregation helper); no daemon changes, no kernel-interface breaking changes
**Prerequisite reading:**
- [ADR-0033](./0033-phase-10a-evaluation-metrics-library.md) — the metrics library this command consumes (Accepted, merged `2a92c6d`)
- [Metrics cookbook](../specs/metrics-cookbook.md) — the per-unit measure patterns
- [Research note: built-in evaluation primitives](../research-notes/built-in-evaluation-primitives.md) — the 5-command design; grade replaces 4 scripts
- [ADR-0033 §"Acceptance amendments"](./0033-phase-10a-evaluation-metrics-library.md) — Wilson trial-count footgun (load-bearing for grade)

---

## Context

claw-core's MLB cartridge produced ~4 experiment scripts (EXP-022 edge
buckets × noise bands, EXP-023 line-source audit, EXP-031c April bias
decomposition, noise_band_analysis) that all share one shape:

> Partition a holdout set by one or more dimensions. For each segment,
> compute {n, win-rate, Wilson CI, ROI, mean-residual}. Flag segments
> that cross a threshold (e.g., Wilson lower bound below breakeven).

EXP-048's headline finding is the canonical example. Grouping 456
line=9.0 bets by bet side surfaced the smoking gun:

| Bet side | n | WR | 95% CI |
|---|---|---|---|
| OVER | 7 | 42.86% | [15.82, 74.95] |
| UNDER | 449 | 65.70% | [61.19, 69.94] |

That table — and the dome-status and xERA-tier breakdowns beneath it —
is exactly what `mc model grade` produces, in one command instead of a
120-line Python script.

Phase 10A shipped the *reducers* (`count_over`, `wilson_ci_lower`, etc.).
Phase 10B ships the *group-by engine* that applies them per segment.

---

## The core model: grouped map-reduce over a unit dimension

A grade run is a map-reduce:

1. **Unit dimension** — the dimension whose leaves are the units of
   analysis (one leaf = one game, one customer, one campaign-week).
2. **Map** — for each unit leaf, evaluate the *group keys* (which segment
   it belongs to) and the *metric ingredients* (per-unit measures like
   `direction_correct`, `pnl`, `stake`).
3. **Reduce** — within each segment, aggregate the ingredients into
   metrics using the Phase 10A primitives (mean, ratio, Wilson CI…).
4. **Report** — emit a table (one row per segment) + optional flags.

The Phase 10A primitives are the reducers. Phase 10B is the grouping
machinery: it discovers segments, scopes the aggregation to each, and
formats the result.

---

## Decisions

### Decision 1: Command shape

```
mc model grade <cartridge.yaml> \
  --unit <dimension> \
  --holdout "<coord-filter>" \
  --group-by <key> [--group-by <key> ...] \
  --metric "<name>=<reduction>(<ingredient>[,<ingredient>])" [--metric ...] \
  [--bucket <measure> <edge0>:<edge1>:...:<edgeN>] \
  [--flag-if "<metric> <op> <value>"] \
  [--min-n <int>] \
  [--format text|json]
```

Example (the EXP-048 reproduction):

```
mc model grade mlb-totals.yaml \
  --unit games \
  --holdout "Time=2025,line=9.0" \
  --group-by bet_side \
  --metric "n=count(direction_correct)" \
  --metric "win_rate=mean(direction_correct)" \
  --metric "wr_lower_95=wilson_lower(direction_correct)" \
  --metric "wr_upper_95=wilson_upper(direction_correct)" \
  --flag-if "wr_lower_95 < 0.50" \
  --format json
```

### Decision 2: Group keys — dimension OR measure-value, with bucketing

A `--group-by <key>` is one of:

| Key kind | Behavior |
|---|---|
| **Dimension name** | One segment per element of that dimension. Natural for `Time` (season), `Venue`, or any modeled Standard dimension. |
| **Measure name** | grade evaluates the measure at each unit leaf; one segment per distinct value. Natural for `bet_side` (over/under), `dome_status` (0/1). |
| **Bucketed measure** | A measure named in a `--bucket` flag. grade evaluates the measure per leaf, assigns it to a band, one segment per band. Natural for continuous values: `Edge_NB`, `xERA`. |

Multi-level grouping (`--group-by bet_side --group-by dome_status`)
produces the cartesian product of segments — one row per
(bet_side × dome_status) combination, matching EXP-022's edge × noise
cross-tab.

**Bucket syntax:** `--bucket Edge_NB 0:0.03:0.10:0.15:0.20:1.0` defines
left-closed, right-open bands `[0,0.03)`, `[0.03,0.10)`, … `[0.20,1.0]`
(last band right-closed). A unit whose measure value falls outside all
bands is assigned to a `(out-of-range)` segment, surfaced explicitly
(never silently dropped).

**Why measure-value grouping is required (not just dimension grouping).**
The dominant use case groups by attributes that aren't orthogonal
dimensions — `bet_side` and `dome_status` are properties of each game,
not independent axes. Modeling them as dimensions would create a sparse
cube (each game lives at exactly one bet_side). Grouping by measure value
is the natural fit and is what every claw-core segmentation script does.

### Decision 3: Metric reduction vocabulary

A `--metric "name=reduction(ingredients)"` uses a small, closed
vocabulary of reductions that map directly to Phase 10A primitives,
applied *group-scoped*:

| Reduction | Ingredients | Maps to | Notes |
|---|---|---|---|
| `count(m)` | 1 measure | `count_over` scoped to segment | counts non-Null evaluated values = trial count `n` |
| `mean(m)` | 1 measure | `avg_over` scoped | the proportion / average |
| `sum(m)` | 1 measure | `sum_over` scoped | totals |
| `ratio(num, den)` | 2 measures | `sum_over(num)/sum_over(den)` | ROI = ratio(pnl, stake) |
| `std(m)` | 1 measure | `std_over` scoped (ddof=1) | dispersion |
| `wilson_lower(m)` | 1 measure | `wilson_ci_lower(mean(m), count(m))` | binomial CI lower |
| `wilson_upper(m)` | 1 measure | `wilson_ci_upper(mean(m), count(m))` | binomial CI upper |

**Wilson trial-count safety (ADR-0033 Amendment 5, load-bearing here).**
`wilson_lower(m)` internally computes `count(m)` as the trial count `n`.
This is correct *only if `m` is a 1.0/0.0 indicator that is never Null
for an evaluated unit*. grade enforces this: if `m` contains any Null
values within a segment, grade emits a warning ("`wilson_lower(direction_correct)`:
3 of 449 units have Null direction_correct; Wilson n excludes them — is
this intended?"). The cookbook's "indicator must be 0.0 on failure, never
Null" convention is the contract; grade surfaces violations rather than
silently producing a too-narrow CI.

**Why a metric mini-DSL and not free-form formulas.** grade's `--metric`
expressions are deliberately NOT full formula expressions. A closed
vocabulary of 7 reductions covers every claw-core segmentation script
and keeps grade's parser tiny. Free-form per-segment formulas are a
larger surface (and would overlap with what cartridge-defined derived
measures already do). If a consumer needs a metric outside the
vocabulary, they define it as a per-unit derived measure in the
cartridge and pass it as an ingredient.

### Decision 4: CLI-only — no daemon endpoint in this phase

grade runs in-process via the existing `load_model_with_policy` path
(same as `mc model query`/`sweep`). No `/api/v1/grade` daemon endpoint.

**Rationale (per the research note's Open Q 5).** A grade run evaluates
the whole holdout set — potentially thousands of unit leaves. The daemon
is optimized for warm-cache single-coordinate reads, not full-cube
sweeps. Running grade in-process (one cold-load, one pass) beats N HTTP
round-trips. The daemon's `/sweep` is for interactive single-game
sliders; grade is a batch analytic. Different tools, different
deployment. A daemon `/grade` can be added later if an interactive
consumer surfaces; no demand today.

### Decision 5: Output format — text table + JSON

**Text** (default): a segment table mirroring the EXP-048 shape, one row
per segment, columns per metric, flagged rows marked:

```
SEGMENT GRADE: mlb-totals.yaml  (holdout: Time=2025,line=9.0; unit: games)

bet_side  | n    | win_rate | wr_lower_95 | wr_upper_95 | flag
----------+------+----------+-------------+-------------+------
OVER      |   7  |  0.4286  |   0.1582    |   0.7495    | ⚠ wr_lower_95 < 0.50
UNDER     | 449  |  0.6570  |   0.6119    |   0.6994    |
----------+------+----------+-------------+-------------+------
TOTAL     | 456  |  0.6535  |   0.6088    |   0.6961    |

1 segment flagged (wr_lower_95 < 0.50).
```

**JSON** (`--format json`): structured for downstream consumption —
the shape claw-core would parse instead of regex-scraping Python stdout.

```json
{
  "schema_version": "1.0",
  "cartridge": "mlb-totals.yaml",
  "holdout": "Time=2025,line=9.0",
  "unit": "games",
  "group_by": ["bet_side"],
  "segments": [
    { "keys": {"bet_side": "OVER"},  "metrics": {"n": 7,   "win_rate": 0.4286, "wr_lower_95": 0.1582, "wr_upper_95": 0.7495}, "flagged": ["wr_lower_95 < 0.50"] },
    { "keys": {"bet_side": "UNDER"}, "metrics": {"n": 449, "win_rate": 0.6570, "wr_lower_95": 0.6119, "wr_upper_95": 0.6994}, "flagged": [] }
  ],
  "total": { "n": 456, "win_rate": 0.6535, "wr_lower_95": 0.6088, "wr_upper_95": 0.6961 },
  "flagged_count": 1
}
```

A `TOTAL` row (ungrouped aggregate over the whole holdout) is always
included — it's the baseline every segment is implicitly compared against.

### Decision 6: `--min-n` and small-segment handling

Segments below `--min-n` (default: 0, i.e., show all) are computed but
marked `(below min-n)` and excluded from flag evaluation. Rationale:
EXP-048's OVER segment (n=7) has a Wilson CI so wide it's
uninformative; `--min-n 25` would mark it low-confidence rather than
letting a 7-sample segment trigger a flag. Never silently drop
small segments — surface them, just don't act on them.

### Decision 7: Holdout filter semantics

`--holdout "Time=2025,line=9.0"` is a coordinate filter using the same
syntax as `mc model query --coord`. It restricts the unit leaves grade
iterates. Comma-separated `dim=elem` pairs are ANDed. The filter pins
dimensions to specific elements; the unit dimension is iterated, not
pinned.

**Filtering by measure value in the holdout** (e.g., `line=9.0` where
`line` is a measure, not a dimension) requires evaluating the measure
per unit and including only matching units. This reuses the same per-leaf
evaluation as group-by measure keys. If `line` is a dimension, it's a
standard coordinate pin. grade handles both — measure-valued holdout
filters are evaluated, dimension pins are direct.

---

## Implementation plan

Estimate: ~3-4 sessions, ~400-500 LOC + tests. Larger than 10A because
it introduces the grouped-aggregation engine (genuinely new machinery,
not just primitives).

### Step 0: Preflight
- Confirm `load_model_with_policy` is the right load path (mirror `sweep.rs`)
- Confirm the per-leaf eval API in `mc-core` (how `count_over` iterates leaves — reuse that traversal)
- Diagnostic code preflight if grade introduces parse errors (MC4xxx range — verify free)

### Step 1: CLI parse (`crates/mc-cli/src/grade.rs`, new)
Mirror `sweep.rs` structure: `GradeCommand` struct, `parse(&[String])`,
`run(GradeCommand)`. Parse `--unit`, `--holdout`, repeated `--group-by`,
repeated `--metric`, repeated `--bucket`, `--flag-if`, `--min-n`,
`--format`. Wire into `main.rs` model dispatch (`"grade" => ...`).

### Step 2: Metric expression parser
Tiny parser for `name=reduction(ingredients)`. Closed vocabulary of 7
reductions (Decision 3). Validate ingredient measures exist in the
cartridge; validate ratio gets exactly 2 args, others exactly 1.

### Step 3: Group-by + bucket resolution
For each unit leaf, evaluate group-key measures/dimensions and bucket
assignments. Build the segment map: `segment_keys -> Vec<unit_leaf>`.

### Step 4: Grouped reduction (the core engine)
Likely a `mc-core` helper: given a set of unit leaves, a per-unit
measure, and a reduction, compute the scoped aggregate. Reuse the
`count_over`/`avg_over` per-leaf eval path — grade restricts the leaf
set to a segment before reducing. This is the load-bearing new code.

### Step 5: Flag evaluation + TOTAL row
Parse `--flag-if`, evaluate per segment. Compute the ungrouped TOTAL.
Apply `--min-n` exclusion from flagging.

### Step 6: Output (text table + JSON)
Text formatter (aligned columns, flag markers) + JSON serializer
(schema_version envelope matching the daemon's convention).

### Step 7: Tests
- Unit: metric expression parser (valid + invalid forms)
- Unit: bucket assignment (edges, out-of-range)
- Integration: build a small cube, grade by a measure, assert segment
  table matches hand-computed values
- Integration: the EXP-048 reproduction shape (group by 2-value measure,
  Wilson CIs per segment) against a fixture cube — assert the Wilson
  bounds match `metrics.rs` fixtures
- Integration: `--min-n` excludes a small segment from flagging
- Integration: Wilson trial-count warning fires when indicator has Nulls
- Determinism: same input → identical output across runs (segment
  ordering must be deterministic — sort by group keys)

### Step 8: Cookbook + docs
Add a `mc model grade` section to the metrics cookbook with the EXP-048
worked example. Update `docs/specs/` if a grade-specific spec is
warranted.

### Step 9: Build gates (CLAUDE.md §6, including §6.7 — quote the real test run)

---

## Acceptance criteria

1. `mc model grade` parses all flags per Decision 1
2. Group-by dimension produces one segment per element
3. Group-by measure produces one segment per distinct value
4. `--bucket` discretizes a continuous measure; out-of-range surfaced
5. Multi-level group-by produces the cartesian product of segments
6. All 7 reductions compute correct group-scoped values
7. `ratio(num, den)` = `sum(num)/sum(den)` per segment
8. Wilson reductions use the segment's trial count as `n`
9. Wilson trial-count warning fires when the indicator has Nulls in a segment
10. `--flag-if` flags segments crossing the threshold
11. `--min-n` marks small segments and excludes them from flagging
12. TOTAL row reflects the ungrouped holdout aggregate
13. Text output matches the EXP-048 table shape
14. JSON output validates against the Decision 5 schema
15. Segment ordering is deterministic (sorted by group keys)
16. EXP-048 reproduction: grouping line=9.0 bets by bet_side yields the documented WR + Wilson CIs (within metrics.rs tolerance)
17. CLI-only — no daemon changes
18. No mc-core breaking changes (grouped-aggregation helper is additive)
19. `cargo test --workspace` passes — **quote the real `test result` line per CLAUDE.md §6.7**
20. `cargo clippy --all-targets --workspace -- -D warnings` clean
21. `cargo fmt --check --all` clean
22. Metrics cookbook gains a `mc model grade` section with the EXP-048 worked example
23. Determinism: 10 consecutive runs produce identical output

---

## Alternatives considered

### Alt 1: Group only by dimensions (no measure-value grouping)

Considered. Simpler engine — grouping = iterate dimension elements.

**Rejected because** the dominant use case groups by attributes
(bet_side, dome_status, xERA tier) that aren't orthogonal dimensions.
Forcing them into dimensions creates sparse cubes and awkward authoring.
Every claw-core segmentation script groups by measure value. A
dimension-only grade wouldn't cover the actual demand.

### Alt 2: Free-form per-segment formula expressions for metrics

Considered. `--metric "win_rate=avg_over(direction_correct, games)"`
with full formula syntax.

**Rejected because** the `_over` family aggregates over a whole
dimension, not a segment subset — the formula would need a segment-scope
concept that doesn't exist in the formula language. Adding it is a
formula-evaluator semantics change (out of scope; same reasoning as
ADR-0033 Amendment 1's rejection of expression-args for `_over`). The
closed 7-reduction vocabulary covers every observed use case. Consumers
needing more define a per-unit derived measure and pass it as an
ingredient.

### Alt 3: Daemon `/api/v1/grade` endpoint

Considered. Consistency with the Phase 8.2 consumer-API surface.

**Rejected for this phase** because grade is a batch analytic over the
whole holdout, not an interactive single-coordinate operation. In-process
(one cold-load, one pass) beats N HTTP round-trips. No interactive
consumer is asking for grade-over-HTTP. Additive later if one surfaces.

### Alt 4: Reuse `mc model query` with a `--group-by` flag

Considered. Avoid a new subcommand.

**Rejected because** query returns cell values at coordinates; grade
returns grouped aggregates with CIs and flags. Bolting grouped
map-reduce onto query would overload a simple read command. Separate
verb, separate mental model — matches how `sweep` is its own verb rather
than a query flag.

### Alt 5: Ship `max_drawdown` / time-ordered metrics in grade

Considered. They're in the broader metrics vocabulary.

**Rejected** (consistent with ADR-0033 Alt 1) — drawdown needs a
time-ordered scan over chronological records, which is `mc model
simulate`'s natural shape (Phase 10F), not a grouped cube aggregation.
grade's reductions are all order-independent.

---

## Out of scope

- Daemon `/api/v1/grade` endpoint (Alt 3 — additive later)
- Free-form formula metrics (Alt 2 — closed vocabulary only)
- Time-ordered metrics: `max_drawdown`, `recovery_bets` (Phase 10F)
- `mc model backtest` (parameter sweep × holdout — Phase 10C, separate ADR)
- `mc model walk-forward` / `simulate` (Phase 10E/F)
- Statistical significance tests between segments (chi-square, etc.) — grade reports per-segment CIs; cross-segment hypothesis testing is a future addition if demanded
- Nested/hierarchical grouping beyond cartesian product
- Custom CI confidence levels (inherits ADR-0033's fixed-95% Wilson)

---

## Cross-links

- ADR-0033 (Phase 10A): the metrics library grade consumes; Amendment 5 (Wilson trial-count) is load-bearing here
- [Metrics cookbook](../specs/metrics-cookbook.md): per-unit measure patterns + the grade section this phase adds
- [Research note: built-in evaluation primitives](../research-notes/built-in-evaluation-primitives.md): grade replaces EXP-022, EXP-023, EXP-031c, noise_band_analysis
- `crates/mc-cli/src/sweep.rs`: the CLI command structure to mirror
- `crates/mc-cli/src/query.rs`: the `--coord`/`--format` parsing to reuse
- claw-core EXP-048 (`docs/reports/exp-048-line9-deep-dive.md`): the canonical reproduction target — bet-side/dome/xERA segment tables
- CLAUDE.md §6.7: the captured-test-log discipline (Phase 10A lesson)

---

## Notes

**Why grade is the right 10B.** Per the demand-driven sequencing
(Option 3), grade is the simplest of the five commands AND it stresses
the Phase 10A Wilson safer-pattern end-to-end against claw-core's actual
EXP-048 use case. It validates that the metrics library composes into a
real workflow before the heavier commands (backtest, simulate) build on
the same foundation.

**The new machinery is the grouped-aggregation engine.** Phase 10A's
primitives aggregate over a whole dimension. grade restricts the leaf
set to a segment before reducing. That segment-scoping is the load-
bearing new code; everything else (parsing, formatting, flagging) is
mechanical.

**This is bigger than 10A.** ~3-4 sessions vs 10A's 1-2. The map-reduce
engine, the metric mini-DSL parser, the bucketing logic, and the dual
output formats add up. Still a focused single-command phase.

**Sequencing after 10B.** Once grade ships and claw-core validates it
against EXP-048, the next phase follows demand: parameter sweeps →
Phase 10C (backtest); batch sensitivity → Phase 10D (`sweep --games`);
chronological bankroll → Phase 10E/F (walk-forward + simulate).
