# Review Request — ADR-0034: Phase 10B `mc model grade`

**For:** dual external review (Claude Desktop + GPT-5.1, high-effort thinking)
**ADR under review:** [`docs/decisions/0034-phase-10b-model-grade.md`](../decisions/0034-phase-10b-model-grade.md)
**Status:** Proposed (not yet accepted; this review gates implementation)
**Date:** 2026-05-27

---

## How to review

Read the full ADR (link above; ~430 lines). Then respond with **accept /
accept-with-amendments / reject**, plus specifics. The prior three ADRs
in this project (0031, 0032, 0033) each shipped with 7 amendments after
this same dual-review pass — real bugs were caught every time (a wrong
scipy fixture, a `/sweep` contract that missed the primary use case, a
population-vs-sample variance default). Be adversarial. Assume there's
something wrong and find it.

If you propose amendments, format them as a numbered, copy-pasteable
block the PM can fold in verbatim (that's how the prior reviews were
consumed).

---

## Context you need

**What Mosaic is.** A Rust deployment-agnostic kernel (`mc-core`) for
*evaluating* fitted models over multi-dimensional cubes. It does NOT
train models — Python (sklearn / PyMC) trains, exports artifacts;
Mosaic consumes them and evaluates. Cartridges are YAML model
definitions (dimensions, measures, rules). The kernel is sync, no
unsafe, minimal deps (kernel discipline per ADR-0025).

**The consumer driving this.** `claw-core` — an MLB totals sports-betting
model. It produced 29 one-shot Python experiment scripts in one quarter.
Pattern analysis found 5 repeating shapes accounting for 26 of them.
Mosaic is adding 5 `mc model` subcommands to replace those scripts so a
new hypothesis becomes a one-liner instead of a 300-line script. This is
demand-driven: ship the foundational layer, let consumer asks sequence
the rest.

**What already shipped (Phase 10A, ADR-0033, merged).** A metrics
library of 5 formula primitives — `std_over`, `var_over`, `count_over`
(sample variance ddof=1; count evaluates the measure at every leaf),
`wilson_ci_lower`, `wilson_ci_upper` (closed-form Wilson score interval).
Plus a metrics cookbook. Critically, ADR-0033's amendments established:
- `_over` aggregations accept BARE MEASURE NAMES ONLY, not expressions
  (so compositional metrics require intermediate derived measures)
- Wilson CI is for binomial proportions only; the `n` argument must be
  the TRIAL count, not the success count — a footgun where a
  Null-on-failure indicator makes `count_over` return `k` instead of `n`

**What this ADR (10B) proposes.** `mc model grade` — the first command
built on the 10A primitives. It does segmented holdout evaluation:
group a holdout set by some attribute, compute per-segment metrics
(n, win-rate, Wilson CI, ROI), flag segments crossing a threshold.
Reproduces the canonical claw-core finding (EXP-048):

```
bet_side  | n   | win_rate | wr_lower_95 | wr_upper_95 | flag
----------+-----+----------+-------------+-------------+------
OVER      |   7 |  0.4286  |   0.1582    |   0.7495    | ⚠ wr_lower_95 < 0.50
UNDER     | 449 |  0.6570  |   0.6119    |   0.6994    |
```

---

## The ADR's key decisions (summary — read the full doc for rationale)

1. **Core model: grouped map-reduce over a "unit dimension."** Unit =
   the analysis row (one game). Map: per unit, evaluate group keys +
   metric ingredients. Reduce: per segment, aggregate via 10A primitives.
   Report: table + JSON. The 10A primitives are reducers; 10B is the
   group-by engine.

2. **Group keys: dimension OR measure-value OR bucketed-measure.**
   Measure-value grouping is held to be REQUIRED (not just dimension
   grouping) because `bet_side`/`dome_status`/`xERA tier` are game
   attributes, not orthogonal dimensions.

3. **Closed 7-reduction metric vocabulary** — `count`, `mean`, `sum`,
   `ratio(num,den)`, `std`, `wilson_lower`, `wilson_upper`. A
   mini-DSL (`name=reduction(ingredients)`), NOT free-form formulas.
   `wilson_lower(m)` internally computes `count(m)` as `n`, with a
   warning if `m` has Nulls in a segment (carries ADR-0033's footgun).

4. **CLI-only — no daemon `/grade` endpoint.** grade is a batch analytic
   over the whole holdout; in-process beats N HTTP round-trips.

5. **Output: text table + JSON** with a `schema_version` envelope; always
   includes a `TOTAL` baseline row.

6. **`--min-n`** marks small segments low-confidence and excludes them
   from flagging, never silently drops them.

7. **Holdout filter** uses `mc model query --coord` syntax; supports both
   dimension pins and measure-value filters (the latter evaluated per
   unit).

---

## Specific questions to pressure-test

These are the design choices I'm least certain about. Push hardest here.

### Q1 — Is the unit/group-key/ingredient model the right decomposition?

The ADR frames grade as: pick a unit dimension, map each unit to
(group_keys, ingredients), reduce per segment. Is this the natural shape,
or is there a cleaner mental model? Specifically:
- Does "unit dimension" map cleanly onto how Mosaic cubes are actually
  structured, or is it an awkward graft? (In the MLB cube, `games` is a
  dimension and each game's features/outcomes are measures at that
  coordinate.)
- The metric ingredients (`direction_correct`, `pnl`, `stake`) must be
  per-unit measures defined in the cartridge. Is requiring the cartridge
  author to pre-define these (per the metrics cookbook) the right call,
  or should grade compute them inline?

### Q2 — Measure-value grouping: is the engine sound?

Decision 2 says grade can group by a measure's value (evaluate the
measure per unit, one segment per distinct value). This is genuinely new
machinery — the 10A `_over` primitives aggregate over a whole dimension,
not filtered subsets. Concerns:
- **Cardinality explosion.** Grouping by a continuous measure without a
  `--bucket` would create one segment per distinct float value
  (thousands of singleton segments). Should grade *require* a bucket for
  non-discrete measures, or warn, or cap segment count? The ADR is
  silent on this.
- **What counts as a "distinct value" for grouping?** Float equality is
  hazardous (CLAUDE.md forbids `==` on floats). Grouping by a measure
  that happens to be `1.0`/`0.0` is fine; grouping by `Edge_NB` directly
  (without a bucket) is a trap. Is the bucket-or-bust rule for
  continuous measures explicit enough?

### Q3 — The metric mini-DSL: closed vocabulary vs extensibility

Decision 3 ships 7 reductions and explicitly rejects free-form formulas
(consumers needing more define a per-unit derived measure and pass it as
an ingredient). Is the closed vocabulary the right boundary?
- Are there obvious reductions missing that claw-core's scripts need?
  (The ADR's reductions: count, mean, sum, ratio, std, wilson_lower,
  wilson_upper. Notably absent: median, percentile, min, max — though
  min_over/max_over exist as primitives.)
- The `ratio(num, den)` reduction is the only 2-ingredient one. Is
  hardcoding "ratio = sum(num)/sum(den)" too rigid? ROI is
  sum(pnl)/sum(stake), but what about mean-of-ratios vs ratio-of-means?
  They differ and the ADR picks ratio-of-sums silently.

### Q4 — Wilson trial-count safety: is a warning enough?

Decision 3 carries ADR-0033's footgun (n must be trials not successes).
grade *warns* when the indicator has Nulls in a segment. Is a warning
the right severity, or should it be a hard error? A silently-wrong CI in
a betting context produces a too-confident "this edge is real" claim. The
prior reviews flagged this exact class of issue as dangerous. Should
`wilson_lower(m)` on a Null-containing `m` refuse rather than warn?

### Q5 — CLI-only: will this need to be revisited immediately?

Decision 4 ships grade as CLI-only (no daemon endpoint), reasoning that
batch-over-holdout beats N HTTP calls. But claw-core's whole integration
story (ADR-0001) is "the daemon is the brain, the Worker is the shell."
If claw-core wants grade results in its Worker pipeline, CLI-only forces
shelling out to a subprocess. Is the CLI-only decision going to bounce
back as a "we need /api/v1/grade" request within one cycle? Or is grade
genuinely an offline-analysis tool that never belongs in the hot path?

### Q6 — The TOTAL row and multi-level grouping interaction

Decision 5 always includes a TOTAL row (ungrouped aggregate). With
multi-level grouping (`--group-by bet_side --group-by dome_status`),
should there also be subtotal rows (all bet_side=OVER across dome
statuses)? The ADR only specifies the full cartesian product + grand
TOTAL. Is the absence of subtotals a gap for the cross-tab use case
(EXP-022 edge × noise), or correctly out of scope?

### Q7 — Anything missing entirely?

The five-command set is grade / backtest / sweep-batch / walk-forward /
simulate. Does carving grade out first leave any cross-cutting concern
unaddressed that should be decided now rather than retrofitted? (E.g.,
a shared output-schema convention all five commands should share; a
shared `--holdout` filter grammar; a shared bet-record format that grade
and simulate both touch.)

---

## What NOT to relitigate

These are settled by prior accepted ADRs — don't reopen unless you see a
genuine contradiction:
- The 10A primitive set and their semantics (ADR-0033, merged)
- `_over` taking bare measures only (ADR-0033 Amendment 1)
- Sample variance ddof=1 (ADR-0033 Amendment 3)
- Python-trains-Mosaic-evaluates split (ADR-0025)
- Demand-driven sequencing / Option 3 (decided this session)

---

## Output format requested

```
VERDICT: accept | accept-with-amendments | reject

[If amendments:]
Required amendments (copy-pasteable, numbered):
1. ...
2. ...

[Always:]
Per-question responses: Q1...Q7
What's well done: ...
Biggest risk if shipped as-is: ...
```
