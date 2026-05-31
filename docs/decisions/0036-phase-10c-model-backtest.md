# ADR-0036: Phase 10C — `mc model backtest` (Parameter Sweep × Holdout Evaluation)

**Status:** Proposed
**Date:** 2026-05-27
**Deciders:** project owner
**Phase:** 10C (fourth command in the evaluation track; claw-core's confirmed #2 ask)
**Crate(s) touched:** `mc-cli` (new `backtest` subcommand) + `mc-core`/`mc-model` ONLY if a model-semantic primitive surfaces (default: none — same discipline as ADR-0034 Amendment 4 / ADR-0035 Amendment 4)
**Prerequisite reading:**
- [ADR-0034](./0034-phase-10b-model-grade.md) — `mc model grade`; backtest evaluates *grade at every grid point*. The 9-reduction metric vocabulary + the `Filter` holdout grammar are reused wholesale.
- [ADR-0035](./0035-phase-10f-model-simulate.md) — `mc model simulate`; backtest can use simulate as a per-grid-point metric source (bankroll/ROI as the objective)
- [ADR-0015](./0015-phase-3i-formula-language-completion.md) — `parameters:` block + `param(name)` (the cube-level swept knob)
- `crates/mc-cli/src/sweep.rs` — single-axis coefficient sweep; backtest generalizes it to N axes × full evaluation
- [Research note: built-in evaluation primitives](../research-notes/built-in-evaluation-primitives.md) — backtest replaces 7 scripts

---

## Context

`sweep` (Phase 8.2 / the existing CLI verb) varies ONE coefficient and
records ONE scalar metric per point — a thin curve. `grade` (10B)
evaluates a holdout set into segmented metrics at ONE fixed parameter
configuration. Neither answers the question claw-core asks most:

> **"Sweep a parameter across a grid; at each value, run the FULL holdout
> evaluation; report the metric surface — and tell me the best setting."**

That's `mc model backtest`. It's the composition of the two existing
engines: **a parameter sweep (one or more axes) × a grade-style holdout
evaluation at each grid point.** Seven of claw-core's experiment scripts
are this exact shape:

| Script | Swept axis | Evaluated metric |
|---|---|---|
| EXP-021 | Lasso α (model coef regularization) | holdout MAE / direction accuracy |
| EXP-032 | NB dispersion α | bet-flip count, WR on flipped subset |
| EXP-033 | edge threshold | n_bets, WR, ROI, Wilson, Sharpe |
| EXP-039 | edge threshold × line bucket | per-bucket optimal threshold |
| EXP-042 | each of 13 coefficients × multiplier | ROI delta vs baseline |
| EXP-044 | OOS coefficient multipliers | direction accuracy + ROI on 2026 |
| EXP-045 | per-line threshold × season | cross-validated stability |

---

## The multi-domain mandate (the spine of this ADR)

**This command must not be sports-betting-shaped.** `simulate` legitimately
carries domain vocabulary (Kelly, win/loss/push, bankroll) because
chronological wagering IS its domain. `backtest` is different — it's a
domain-neutral question ("how does the metric surface respond to a swept
parameter?") that applies identically to:

| Domain | Swept axis | Holdout | Metric |
|---|---|---|---|
| Sports betting | edge threshold, NB α, model coefficient | season's games | ROI, direction accuracy, Wilson-bounded WR |
| Marketing MMM | adstock decay, saturation half-point, budget level | quarter's weeks | predicted revenue, MAPE, ROAS |
| Finance | discount rate, factor exposure multiplier | backtest window | portfolio return, Sharpe, max drawdown |
| Forecasting | smoothing α, seasonality strength | holdout periods | RMSE, MASE, coverage |
| Demand planning | safety-stock multiplier, lead-time assumption | history | fill rate, holding cost, stockout count |

The design rule: **backtest knows nothing about bets.** It knows about
*swept parameters*, *holdout coordinate sets*, *metrics built from the
9-reduction vocabulary or from simulate*, and *an objective to optimize*.
Sports-betting is one cartridge that happens to use it. Every metric name
in the command surface must be either a generic reduction (count/mean/
sum/ratio/std/min/max/wilson) or a measure the *cartridge author* named —
never a hardcoded `roi` or `win_rate` in the engine.

---

## Decisions

### Decision 1: Command shape

```
mc model backtest <cartridge.yaml> \
  --sweep <axis-spec> [--sweep <axis-spec> ...] \
  --holdout "<filter>" \
  --metric "<name>=<reduction>(<ingredient>...)" [--metric ...] \
  [--group-by <key> ...] [--bucket <measure> <edges>] \
  [--objective <metric>] [--goal maximize|minimize] \
  [--simulate <sizing-spec>] \
  [--min-n <int>] [--max-grid <int>] \
  [--format text|json] [--emit-grid <path>]
```

`backtest` reuses, verbatim, from `grade`:
- the `--holdout` `Filter` grammar (dimension pins + measure predicates,
  float-`==` guarded)
- the 9-reduction metric vocabulary + the `name=reduction(ingredients)`
  parser
- `--group-by` / `--bucket` (so a backtest can be segmented too —
  EXP-039's threshold × line-bucket)
- `--min-n`

The NEW machinery is `--sweep` (the axis spec) and the grid orchestration
+ objective selection.

### Decision 2: `--sweep` axis kinds — three, all domain-neutral

A `--sweep <axis-spec>` names what varies. Three kinds, covering every
claw-core script and generalizing cleanly:

| Kind | Spec syntax | What varies | Domain examples |
|---|---|---|---|
| **parameter** | `param:<name>=<start>:<stop>:<step>` | a `parameters:` block scalar (`param(name)`) | NB α, edge threshold, adstock decay, discount rate |
| **coefficient** | `coef:<model>.<name>=<start>:<stop>:<step>` OR `coef:<model>.<name>=<mult>x<lo>:<hi>:<step>` | a fitted-model coefficient (absolute or multiplier) | EXP-042 coefficient stress, factor exposure |
| **input** | `input:<measure>@<coord>=<start>:<stop>:<step>` | an Input measure value at a coord (transient override) | a what-if assumption, a price level, a budget |

- **parameter** is the primary kind and the most domain-neutral — it
  sweeps a cartridge-declared `param(name)` knob. Any domain that wants a
  tunable threshold/rate/multiplier declares it as a parameter and sweeps
  it. This is the recommended path; it keeps the swept quantity a
  first-class, named, documented part of the cartridge.
- **coefficient** sweeps a fitted-model weight (absolute value or a
  multiplier `1.0x0.75:1.25:0.05` for stress testing). EXP-042/044.
- **input** sweeps an Input-measure value at a coordinate (transient, like
  `whatif`). For assumption sensitivity.

**Multi-axis:** repeated `--sweep` produces the **cartesian product** grid
(EXP-039's threshold × line-bucket, EXP-045's threshold × season). Each
grid cell is one full holdout evaluation. `--max-grid` (default 1000)
hard-errors on grid explosion — the same DoS guard as grade's
`--max-segments`.

### Decision 3: At each grid point, run the full evaluation

For each grid cell (a tuple of swept-axis values):
1. Apply the swept values (parameter override / coefficient substitution
   / input override) — all transient, like `whatif`; the cube is never
   mutated, and `LoadPolicy::Reproducible` is the default (ADR-0035 A8).
2. Run the holdout evaluation: filter → (optional group-by/bucket) →
   compute every `--metric` via the 9-reduction vocabulary, exactly as
   `grade` does.
3. Record the grid cell: `{swept_values, metrics, [per-segment metrics]}`.

The result is a **metric surface** — one row per grid cell (× segment, if
grouped). This is the domain-neutral core: swept knobs in, metric surface
out.

### Decision 4: Metrics — the 9-reduction vocabulary, OR a simulate objective

Two metric sources, both domain-neutral:

**(a) Reduction metrics (default).** The same `name=reduction(ingredient)`
vocabulary as grade — `count`, `mean`, `sum`, `ratio`, `std`, `min`,
`max`, `wilson_lower`, `wilson_upper`. The ingredients are cartridge
measures. A marketing cartridge writes `mape=mean(abs_pct_error)`; a
betting cartridge writes `roi=ratio(pnl,stake)`. **The engine ships no
domain metric** — every metric is composed from generic reductions over
author-named measures.

**(b) Simulate objective (`--simulate <sizing>`).** When the per-grid-point
objective is a *path-dependent* quantity (bankroll, max drawdown — things
grade's order-independent reductions can't express), `--simulate` runs
`mc model simulate`'s engine at each grid point over the holdout's bet
records and surfaces its metrics (final_bank, roi, sharpe, max_drawdown)
as backtest metrics. This is the betting-specific power path, but it's
*opt-in* and composes the existing simulate engine — backtest's core
stays domain-neutral; simulate is a pluggable metric source.

### Decision 5: Objective + goal — pick the best grid cell

`--objective <metric> --goal maximize|minimize` selects the best grid cell
by one of the computed metrics. The output flags it (`best: {param: 0.13,
roi: 0.166}`). When `--objective` is omitted, backtest reports the full
surface with no winner (the EXP-021 "show me the whole α curve" case).

`--goal` defaults to `maximize`. For error metrics (MAE, RMSE, MAPE) the
author passes `minimize`. The engine doesn't know which metrics are
"good high" vs "good low" — the user declares it. Domain-neutral.

### Decision 6: Output — surface table + JSON + optional grid file

**Text:** a metric-surface table — one row per grid cell, columns per
metric, the best cell flagged. For multi-axis, the grid coordinates are
the leading columns. For grouped backtests, segments nest under each grid
cell.

**JSON** (`--format json`): `schema_version` envelope; `grid` array of
`{sweep_values, metrics, segments?, flagged?}`; `best` (when objective
set); the full run config (axes, holdout, metrics, objective, goal,
seed if simulate); `warnings`. The codegen contract.

**`--emit-grid <path>`:** the surface as jsonl (one row per grid cell ×
segment) for downstream plotting — the "α-vs-ROI curve" or "threshold ×
line heatmap" a consumer charts. jsonl in v1 (parquet deferred, per
ADR-0035 A4 precedent).

### Decision 7: CLI-only; mc-cli implementation; zero kernel change

Same disposition as grade + simulate. backtest is a batch analytic
composing existing engines. CLI-only, no `/api/v1/backtest`. Implementation
in `mc-cli`. No `mc-core`/`mc-model` change unless a model-semantic
primitive surfaces — and the parameter/coefficient/input override
machinery already exists (sweep does coefficient + set-coord overrides;
whatif does input overrides; param(name) is a model-layer scalar). backtest
*orchestrates* those; it doesn't need new kernel primitives.

### Decision 8: Determinism

The grid is enumerated in a fixed order (axis declaration order, first
axis slowest — same convention as grade's segment ordering, ADR-0034
A12). Each grid cell's evaluation is deterministic (reproducible load
policy; transient overrides). When `--simulate` is used with Monte Carlo,
the seed is required (ADR-0035 A5) and threaded per grid cell so the full
backtest is reproducible. 10 runs → identical output.

---

## Implementation plan

Estimate: ~3-4 sessions, ~400-500 LOC + tests. Smaller than it looks
because it *composes* grade (metric vocab, Filter, group-by, reductions)
and sweep (axis override mechanics) — the new code is grid orchestration +
the `--sweep` axis-spec parser + objective selection.

### Step 0: Preflight
- Confirm grade's metric parser + reduction engine are reusable as a
  library (not private to `grade.rs`) — if private, lift the shared bits
  into a small `eval_common` module both call. Surface as a SPEC QUESTION
  if the refactor is non-trivial.
- Confirm the override mechanics: sweep.rs's coefficient/set-coord
  override + whatif's input override + param(name) resolution — backtest
  reuses all three. Verify they're callable.
- Diagnostic-code preflight (MC4xxx).

### Step 1: `--sweep` axis-spec parser
Parse the three axis kinds (`param:`/`coef:`/`input:`) + range
(`start:stop:step`, and the `Nx lo:hi:step` multiplier form for coef).
Validate the referenced parameter/coefficient/measure exists in the
cartridge. Multi-axis → grid; enforce `--max-grid`.

### Step 2: Grid orchestration
Enumerate the cartesian product in fixed order. For each cell: apply
transient overrides, run the holdout evaluation (Step 3), record.

### Step 3: Per-cell evaluation (reuse grade)
Call the shared grade evaluation: filter → group-by/bucket → reductions.
This is the lifted-common code from Step 0. backtest adds nothing to the
metric math — it just runs grade N times with different swept values.

### Step 4: Simulate objective (`--simulate`, opt-in)
When present, at each grid cell run simulate's engine over the holdout
records and surface its metrics. Threads the sizing spec + seed.

### Step 5: Objective selection + output
Pick the best cell by `--objective`/`--goal`. Surface table + JSON +
`--emit-grid` jsonl.

### Step 6: Tests
- Axis-spec parser: all three kinds, range forms, multiplier form, bad specs
- Single-axis param sweep: known cube, sweep a threshold, assert the
  metric surface matches hand-computed per-point values
- Multi-axis cartesian: 2 axes → correct grid size + ordering
- `--max-grid` hard-errors on explosion
- Objective selection: maximize + minimize pick correct cells
- **EXP-033 reproduction:** sweep edge threshold on a fixture, assert the
  optimal threshold + per-threshold metrics match claw-core's report
- **Multi-domain test (mandatory — the spine):** a NON-betting fixture
  cube (a tiny marketing or forecasting cartridge) swept on a parameter,
  metrics via generic reductions (e.g. `mape=mean(abs_pct_error)`),
  proving zero betting assumptions leak into the engine
- `--simulate` objective: grid point runs simulate, surfaces bankroll
- Determinism ×10
- Test fixtures single-brace (§4.5)

### Step 7: Cookbook + gates
metrics-cookbook.md `mc model backtest` section with BOTH a betting
example (threshold sweep → ROI surface) AND a non-betting example
(marketing adstock-decay sweep → MAPE surface, or forecasting smoothing-α
→ RMSE) to make the multi-domain mandate concrete. All gates incl. §6.7
quoted test run.

---

## Acceptance criteria

1. `--sweep param:<name>=a:b:s` sweeps a `parameters:` scalar
2. `--sweep coef:<model>.<name>=...` sweeps a fitted-model coefficient (absolute + `Nx` multiplier forms)
3. `--sweep input:<measure>@<coord>=...` sweeps an Input value (transient)
4. Multi-axis `--sweep` produces the cartesian-product grid in fixed order (first axis slowest)
5. `--max-grid` (default 1000) hard-errors on grid explosion
6. At each grid cell, the full holdout evaluation runs (filter → group-by → reductions) identically to `grade`
7. Metrics use the 9-reduction vocabulary over cartridge measures — **no hardcoded domain metric in the engine**
8. `--objective <metric> --goal maximize|minimize` flags the best grid cell; omitted → full surface, no winner
9. `--goal` defaults to maximize; minimize works for error metrics
10. `--simulate <sizing>` (opt-in) surfaces simulate metrics (bankroll/roi/sharpe/max_drawdown) per grid cell
11. `--group-by`/`--bucket` compose (EXP-039 threshold × line-bucket)
12. Reproducible load policy default (ADR-0035 A8); overrides transient (cube never mutated)
13. Text surface table + JSON (schema_version + run config) + `--emit-grid` jsonl
14. **EXP-033 reproduction:** edge-threshold sweep → optimal threshold + per-point metrics match claw-core within tolerance
15. **Multi-domain test:** a non-betting fixture cube backtests correctly with generic-reduction metrics; zero betting vocabulary in the engine path
16. Determinism ×10 (seed required + threaded when `--simulate` + Monte Carlo)
17. CLI-only; zero mc-core/mc-model change (or surfaced model-semantic justification)
18. `cargo test --workspace` passes — **quote the real result line (§6.7)**
19. `cargo clippy --all-targets --workspace -- -D warnings` clean
20. `cargo fmt --check --all` clean
21. No float `==` (§3.1); zero-checks via `abs() < 1e-300`
22. metrics-cookbook.md backtest section with BOTH a betting AND a non-betting worked example (the multi-domain mandate, made concrete)
23. Test fixtures single-brace (§4.5)

---

## Alternatives considered

### Alt 1: Extend `sweep` instead of a new `backtest` verb

Considered. `sweep` already does single-axis coefficient × scalar-metric.

**Rejected because** sweep records ONE scalar per point (a thin curve);
backtest runs a FULL holdout evaluation (segmented, multi-metric) per
point. Bolting holdout-evaluation onto sweep would overload a simple
curve command and conflate two mental models. Separate verb, matching how
grade and simulate are their own verbs. (sweep stays as the lightweight
single-axis curve; backtest is the heavyweight grid × evaluation.)

### Alt 2: Hardcode betting metrics (roi, win_rate) in the engine

Rejected hard — this is the multi-domain mandate's whole point. Every
metric is composed from generic reductions over author-named measures.
A betting cartridge writes `roi=ratio(pnl,stake)`; a marketing cartridge
writes `roas=ratio(revenue,spend)`; the engine ships neither. Hardcoding
betting vocabulary would make backtest a sports tool, not a Mosaic
command.

### Alt 3: Bake simulate into backtest (always path-dependent)

Considered — always run the bankroll sim per grid point.

**Rejected because** most backtest questions are order-INDEPENDENT
(EXP-021 MAE sweep, EXP-033 threshold WR, marketing MAPE) — they need
grade's reductions, not simulate's path replay. Path-dependence (bankroll,
drawdown) is the betting-specific case. Making simulate the default would
force every domain through wagering machinery. `--simulate` is opt-in;
the default is the domain-neutral reduction path.

### Alt 4: Daemon `/api/v1/backtest`

Rejected for this phase — batch analytic, same as grade/simulate. A grid
× holdout evaluation is the heaviest batch operation in the track; it
belongs in-process, not over HTTP round-trips. Additive later if an
interactive consumer surfaces.

### Alt 5: Walk-forward refit per grid point (true retraining)

Considered — at each parameter value, RETRAIN the model on the training
window, then evaluate. That's EXP-028's walk-forward.

**Rejected (deferred to Phase 10E)** because retraining is a Python job
(sklearn/PyMC) — Mosaic evaluates fitted models, it doesn't train.
backtest sweeps parameters of an ALREADY-fitted model (coefficients,
thresholds, dispersion) and re-evaluates; it does not refit. True
walk-forward (retrain per fold) is Phase 10E, where Python emits per-fold
fitted-model artifacts and Mosaic evaluates each — exactly the
Python-trains-Mosaic-evaluates seam. backtest is the no-refit cousin;
they compose (10E generates folds, backtest could sweep within each).

---

## Out of scope
- Walk-forward refit (Phase 10E; Alt 5)
- Daemon endpoint (Alt 4)
- Hardcoded domain metrics (Alt 2)
- Bayesian-optimization / adaptive grid search (v1 is exhaustive cartesian grid; adaptive search is a future enhancement if grids get large)
- Parallel grid evaluation (v1 is sequential; the grid is embarrassingly parallel but parallelism is a perf optimization deferred until a grid is slow enough to need it)
- parquet `--emit-grid` (jsonl v1, per ADR-0035 A4)
- Gradient/finite-difference sensitivity (the surface IS the sensitivity; analytic gradients are out of scope)

---

## Cross-links
- ADR-0034 (grade): the per-grid-point evaluation engine + metric vocabulary + Filter grammar backtest reuses
- ADR-0035 (simulate): the opt-in path-dependent metric source (`--simulate`)
- ADR-0015 (Phase 3I): `parameters:` / `param(name)` — the primary domain-neutral swept knob
- `crates/mc-cli/src/sweep.rs`: single-axis override mechanics backtest generalizes
- `crates/mc-cli/src/grade.rs`: the metric parser + reduction engine to lift into shared code
- [Research note: built-in evaluation primitives](../research-notes/built-in-evaluation-primitives.md): backtest replaces EXP-021/026/032/033/039/042/044/045
- [Research note: evaluation-oracle-validation-push-bug](../research-notes/evaluation-oracle-validation-push-bug.md): why the corrected push-accurate default matters — backtest builds on it
- CLAUDE.md §3.1, §4.5, §6.7

---

## Notes
**Why this is the multi-domain proof point.** grade and simulate were
demand-driven by claw-core and carry (in simulate's case) betting
vocabulary. backtest is the command where the project owner explicitly
asked for multi-domain. The design enforces it: the swept knobs
(parameter/coefficient/input) and the metrics (generic reductions over
author measures) are domain-neutral by construction. The mandatory
non-betting test (AC #15) and the dual cookbook example (AC #22) are the
guardrails that keep it that way — without them, the betting consumer's
gravity would slowly pull backtest sports-shaped. They're not optional.

**Why it composes rather than reinvents.** backtest = grade (per point) ×
a sweep (the axes). The new code is the grid orchestrator + axis-spec
parser + objective selection — maybe 200 LOC of genuinely new logic; the
rest is calling grade's engine N times. This is the payoff of having
shipped grade first: backtest is mostly composition.

**Why backtest ≠ walk-forward.** backtest sweeps parameters of a fitted
model and re-evaluates (no refit). Walk-forward (10E) retrains per fold
(Python). Keeping them distinct preserves the Python-trains-Mosaic-
evaluates seam. They compose: 10E emits fold artifacts, backtest could
sweep within each fold. But 10C ships standalone — it needs no refit, and
claw-core's #2 ask (α/threshold/Kelly sweeps on the existing fitted model)
is exactly the no-refit case.

**Open questions for dual review:**
1. Is the three-axis-kind taxonomy (parameter/coefficient/input) right, or is there a fourth swept-quantity class a non-betting domain needs?
2. Is lifting grade's reduction engine into shared code the right refactor, or should backtest call grade as a subprocess/library boundary?
3. Should `--simulate` objective be in v1, or deferred (keeping v1 purely reduction-based and domain-neutral, adding the path-dependent source later)?
4. Multi-axis cartesian only, or is a "zip" mode (parallel axes, not product) needed for any script?
5. Is `--max-grid 1000` the right default ceiling, and should grid explosion be a hard error or a confirm-prompt?
6. Does the multi-domain mandate need a SECOND non-betting cartridge in the test suite (e.g. both marketing AND forecasting) to truly prove neutrality, or is one sufficient?
