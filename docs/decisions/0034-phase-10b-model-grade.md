# ADR-0034: Phase 10B — `mc model grade` (Segmented Holdout Evaluation)

**Status:** Accepted (with 12 acceptance amendments — see bottom; binding for implementation)
**Date:** 2026-05-27
**Accepted:** 2026-05-27 (project owner approved after dual external review pass)
**Last amended:** 2026-05-27 — Claude Desktop + GPT-5.1 review feedback folded in (12 amendments); safety semantics tightened, filter grammar grounded in existing `Filter` type
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

---

## Acceptance amendments

Filed 2026-05-27 after dual external review (Claude Desktop + GPT-5.1,
high-effort thinking). Both reviewers returned **accept-with-amendments**.
GPT proposed 8 amendments; Desktop endorsed all 8 and added 4. All 12 are
**binding** for implementation and override the body where they conflict.
None change the core architecture (map-reduce framing, closed reduction
vocabulary, EXP-048 reproduction, CLI-only scope all stand) — they
tighten safety semantics and specify ambiguous behaviors. Each closes a
specific failure mode that would otherwise produce silently-wrong
analytics or implementation churn.

**Codebase grounding (verified before adoption):** the existing `Filter`
type (`crates/mc-cli/src/query.rs:413`) ALREADY supports measure-value
predicates — `FilterAtom::Measure(String)` + `CmpOp::Eq` etc. `--where
"line == 9.0"` works today. This means grade does NOT invent a filter
grammar; it reuses `Filter`. It also means the float-equality hazard
(`CmpOp::Eq` on an F64 measure) is real and present today, which is
exactly what Amendments 1+2 guard. `sweep.rs:184` confirms the
`LoadPolicy::Reproducible` precedent for Amendment 8.

### Amendment 1: `--holdout` reuses the existing `Filter` grammar; numeric-equality guarded

**Problem.** The body said `--holdout` "uses `mc model query --coord`
syntax" but then used measure-value predicates (`line=9.0`). `--coord`
(`parse_coord_string`) is dimension-pin-only; measure predicates need
the richer grammar.

**Amendment.** `--holdout` reuses the existing `Filter` grammar
(`crates/mc-cli/src/query.rs:413`, the same grammar `--where` uses), NOT
the `--coord` dimension-pin syntax. That grammar already handles both:
- **Dimension pins:** `Time == "2025"` (FilterAtom::Dimension)
- **Measure predicates:** `line == 9.0` (FilterAtom::Measure)

with `And`/`Or`/`Not`/`Compare`. grade evaluates the filter per unit leaf
to decide inclusion. **Numeric-equality guard:** `CmpOp::Eq` / `Neq`
against an F64 measure value is hazardous (float `==`). grade applies the
same rule as Amendment 2 — equality on a continuous F64 measure requires
the measure be marked discrete/low-cardinality in the cartridge, OR the
caller uses a range predicate (`line >= 8.75 and line < 9.25`), OR an
explicit tolerance. A bare `line == 9.0` on an unmarked F64 measure is a
hard error with the suggested alternatives. Document the grammar in
Decision 7 with worked examples for both the dimension-pin and
measure-predicate cases.

### Amendment 2: Measure-value `--group-by` requires `--bucket` for continuous measures; `--max-segments` cap

**Problem.** "One segment per distinct value" is correct for `bet_side`
(2 values) and dangerous for `Edge_NB` (thousands of distinct floats →
thousands of singleton segments; float-equality grouping is a CLAUDE.md
violation).

**Amendment.** For a measure `--group-by` key:
- If the measure is marked **discrete/low-cardinality** in cartridge
  metadata → group by distinct value directly.
- If the measure is **continuous/F64 (unmarked)** → `--bucket` is
  REQUIRED. Grouping a continuous measure without a bucket is a hard
  error: "`Edge_NB` is a continuous measure; provide `--bucket Edge_NB
  <edges>` to group it, or mark it discrete in the cartridge."
- Add `--max-segments <n>` (default **50**). If the resolved segment
  count (including cartesian products from multi-level grouping) exceeds
  the cap, hard-error with the count and the cap. Prevents accidental
  segment explosion.

Update Decision 2 (the rule) and Decision 6 (the cap behavior).

### Amendment 3: Wilson Null indicator — hard error by default, not warning

**Problem.** Decision 3 *warned* when a Wilson indicator had Nulls in a
segment. ADR-0033's cookbook explicitly requires "1.0 or 0.0, never Null"
for Wilson indicators. In a betting context a too-narrow CI silently
produces a too-confident "this edge is real" claim — the wrong failure
mode.

**Amendment.** `wilson_lower(m)` / `wilson_upper(m)` **hard-error by
default** when `m` contains any Null within a segment being reduced:
"`wilson_lower(direction_correct)`: 3 of 449 units in segment {bet_side:
UNDER} have Null direction_correct. Wilson n requires a non-Null 1.0/0.0
indicator for every unit. Fix the indicator (use `if(cond, 1.0, 0.0)` —
never Null), or pass `--wilson-null drop` to exclude Null units (changes
n)." The `--wilson-null drop|error` flag defaults to `error`. Update
Decision 3 and acceptance criterion 9.

### Amendment 4: Grouped-reduction stays in `mc-cli` — no `mc-core` change unless proven necessary

**Problem.** The body said "possibly `mc-core` (grouped-aggregation
helper)" and Step 4 suggested a core helper. Grouping/segmentation is
CLI/reporting semantics, not model semantics; promoting it to the kernel
violates ADR-0025 kernel discipline.

**Amendment.** The grouped-reduction engine lives entirely in `mc-cli`.
Amend Decision 1's crate list and Step 4: "**no `mc-core` change unless
implementation surfaces a missing *model-semantic* primitive that kernel
discipline justifies promoting** — and if so, that promotion is a
separate surfaced decision, not a silent add." grade composes the
existing 10A primitives by restricting the leaf set per segment; the
per-leaf eval traversal already exists in `mc-core` and is *called*, not
extended.

### Amendment 5: Expand the JSON schema for automation

**Problem.** The Decision 5 JSON had only `segments`/`metrics`/`total`/
`flagged_count` — too thin to consume the behaviors the ADR introduces.

**Amendment.** The JSON output adds:
- Per-segment `status`: `ok | below_min_n | out_of_range | excluded_from_flags`
- Per-segment `null_counts`: `{measure_name: count}` (units with Null for each ingredient)
- `warnings`: array of structured warning objects, at both segment and run level
- `bucket` metadata: `{measure_name: [edge0, edge1, ...]}` when buckets are used
- `denominator_zero_segments`: array of segment keys where a `ratio` denominator was zero/Null
- Reserve `subtotals: []` (absent or empty in v1; see Amendment — Q6 deferral) for additive growth

Update Decision 5 with the full schema. The JSON is the contract
downstream consumers (claw-core's Worker) codegen against — it must
expose every invalid-evaluation state, not just the happy path.

### Amendment 6: `ratio(num, den)` denominator semantics

**Problem.** Decision 3 didn't specify `ratio` behavior when the
denominator sum is zero, Null, or all-Null in a segment.

**Amendment.** `ratio(num, den)` per segment:
- `sum(den) == 0`, or `sum(den)` is Null, or all `den` values Null →
  metric value is **Null**, with a structured diagnostic appended to
  `warnings` and the segment listed in `denominator_zero_segments`.
- **Never** produce `inf`, `NaN`, or a silent `0`.
- Float-zero check uses the kernel's `value.abs() < 1e-300` convention
  (per CLAUDE.md §7), not `== 0.0`.

Update Decision 3 and acceptance criterion 7.

### Amendment 7: Add `min(m)` and `max(m)` to the reduction vocabulary

**Problem.** The 7-reduction vocabulary omitted min/max, though
`min_over`/`max_over` exist as 10A-adjacent primitives and are useful
sanity checks.

**Amendment.** Add `min(m)` and `max(m)` (→ `min_over`/`max_over` scoped
to segment). The vocabulary is now 9 reductions: `count`, `mean`, `sum`,
`ratio`, `std`, `min`, `max`, `wilson_lower`, `wilson_upper`.
`median`/`percentile` remain deferred (no closed-form primitive; needs a
sort — defer to demand). Update Decision 3 table and acceptance
criterion 6.

### Amendment 8: Default to `LoadPolicy::Reproducible`

**Problem.** The body didn't specify a load policy. grade is an
experiment (closer to `sweep` than `query`); it should start from
version-controlled model state, not operational reality patched by
`.tessera/writes.jsonl`.

**Amendment.** grade defaults to `LoadPolicy::Reproducible` (matching
`sweep.rs:184`). Post-hoc operational writes are excluded by default. Add
an optional `--include-writes` flag for callers who explicitly want
operational state folded in. Document in Decision 7 alongside the holdout
filter.

### Amendment 9: TOTAL row is inclusive of min-n-excluded segments

**Problem.** Decision 5 (TOTAL = ungrouped aggregate) and Decision 6
(`--min-n` excludes small segments) interact ambiguously: does TOTAL
include below-min-n segment units?

**Amendment.** **TOTAL aggregates ALL holdout units regardless of segment
min-n status.** `--min-n` affects only *flag evaluation*, not
*measurement*. A segment may be `excluded_from_flags` while its units
still contribute to TOTAL. Rationale: TOTAL reads as "everything" and
enables the "this segment has 7 units; total has 456" comparison. Add an
explicit note to Decision 6 and acceptance criterion 12.

(Units excluded by `--bucket` out-of-range, by contrast, ARE surfaced as
their own `(out-of-range)` segment and DO contribute to TOTAL — they're
not dropped, just labeled. Only units failing the `--holdout` filter are
absent from TOTAL.)

### Amendment 10: Reproducibility / snapshot note

**Problem.** The reproducibility claim depends on what cube state is
frozen and when, which wasn't specified. For a betting tool where "we
beat the market by 7%" is defensible only if the data is provably the
same, this matters.

**Amendment.** Add to Decision 7: "grade reads cube state as of load time;
it performs no live re-evaluation against changing data files during the
run. For exact reproducibility against a historical snapshot, callers
should pin to a known cube revision (snapshot/rollback machinery) before
running grade. A future `--at-revision <rev>` flag for explicit snapshot
pinning is deferred." This prevents a "why did my grade results change
overnight" surprise when underlying data files are updated between runs.

### Amendment 11: Formal metric-expression grammar

**Problem.** Step 2's "tiny parser for `name=reduction(ingredients)`"
undersold the design surface (whitespace, quoting, special chars in
measure names, error UX).

**Amendment.** Specify the grammar formally in Decision 3:

```
metric_expr    := IDENT '=' reduction
reduction      := REDUCTION_NAME '(' ingredient (',' ingredient)* ')'
ingredient     := MEASURE_NAME
REDUCTION_NAME := count | mean | sum | ratio | std | min | max
                | wilson_lower | wilson_upper
```

- **Whitespace** is tolerated around `=`, `,`, and parens; not within
  identifiers. `"win_rate = mean(direction_correct)"` and
  `"win_rate=mean(direction_correct)"` both parse.
- **Identifiers** (metric names, measure names) are bare — no quotes in
  the grammar. CLI shell quoting (to escape `=`/`()`) is independent and
  the parser sees the unquoted string.
- **Measure names** follow the cartridge's existing identifier rules
  (whatever `parse_bare_identifier` accepts); grade validates each
  ingredient exists as a measure in the cartridge.
- **Arity:** `ratio` requires exactly 2 ingredients; all others exactly
  1. Wrong arity is a parse error.
- **Error UX:** unknown reduction → `"unknown reduction 'avgg'; expected
  one of: count, mean, sum, ratio, std, min, max, wilson_lower,
  wilson_upper"`.

### Amendment 12: Lock multi-level segment ordering

**Problem.** Acceptance criterion 15 required deterministic ordering but
didn't specify the order across multiple group-by levels.

**Amendment.** Multi-level grouping produces **lexicographic ordering by
group-by flag order — first flag varies slowest, last varies fastest**
(the leftmost grouping is the major partition, matching standard
reporting convention). For `--group-by bet_side --group-by dome_status`:

```
OVER,  dome=0
OVER,  dome=1
UNDER, dome=0
UNDER, dome=1
```

Within a single key, segments sort ascending by key value (string sort
for categorical, numeric sort for bucket bands by lower edge). Add to
Decision 5 and acceptance criterion 15.

---

## Consolidated acceptance-criteria revisions

The body's 23 ACs stand, with these amendment-driven changes:

- **AC #6** (reductions): now 9 reductions including `min`/`max` (Amdt 7)
- **AC #7** (ratio): denominator-zero/Null → Null + diagnostic, never inf/NaN/0 (Amdt 6)
- **AC #9** (Wilson): hard-error on Null indicator by default; `--wilson-null drop` escape hatch (Amdt 3)
- **AC #12** (TOTAL): inclusive of min-n-excluded segments (Amdt 9)
- **AC #14** (JSON): validates against the expanded schema (Amdt 5)
- **AC #15** (ordering): lexicographic by group-by flag order, first slowest (Amdt 12)
- **AC #17** (CLI-only): unchanged — but no `mc-core` change at all unless a model-semantic primitive is surfaced (Amdt 4)

New ACs:
- **AC #24:** `--holdout` reuses the `Filter` grammar; F64-measure equality requires discrete-marking / range / tolerance, else hard error (Amdt 1)
- **AC #25:** continuous-measure `--group-by` without `--bucket` is a hard error; `--max-segments` (default 50) caps segment count (Amdt 2)
- **AC #26:** grade defaults to `LoadPolicy::Reproducible`; `--include-writes` opt-in (Amdt 8)
- **AC #27:** metric-expression grammar parses per Amendment 11, with the specified error UX
- **AC #28:** JSON exposes per-segment status, null_counts, warnings, bucket metadata, denominator_zero_segments (Amdt 5)
- **AC #29:** reproducibility note documented; load-time snapshot semantics explicit (Amdt 10)

---

*End of amendments. Body of ADR above is preserved for audit-trail
purposes; amendments win on conflicts.*
