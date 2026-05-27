# pymc-marketing Pattern Extraction — Concrete Primitives for Mosaic

**Status:** Research note (synthesis of 4 parallel agent explorations of `pymc-labs/pymc-marketing`)
**Date:** 2026-05-27
**Author:** Mosaic PM (Claude Opus 4.6, 1M context) + 4 exploration agents
**Repo explored:** `/tmp/pymc-marketing-research` (cloned 2026-05-27)
**Companion notes:**
- [`distribution-valued-cells.md`](./distribution-valued-cells.md) — the foundational primitive
- [`built-in-evaluation-primitives.md`](./built-in-evaluation-primitives.md) — the five evaluation commands

This note extracts **concrete, citable patterns** from pymc-marketing that
generalize across domains (marketing, sports betting, finance). Each
finding includes the source file/line, the user-facing API, and a
proposed Mosaic cartridge representation. The goal is to feed downstream
ADRs with named primitives, not to commit to implementation.

---

## TL;DR — the seven extracted patterns

| # | Pattern | pymc-marketing origin | Mosaic adoption priority | Sports-betting analog |
|---|---|---|---|---|
| 1 | Hierarchical `Prior` YAML | `prior.py:118` | **Tier 1** — adopt as serialization convention | Per-team-pair NB dispersion drawn from global |
| 2 | Time-varying coefficient artifacts | `mmm/tvp.py:329` | **Tier 1** — consume as coord-indexed cube | xERA's weight as function of season-month |
| 3 | Constraint + utility registry | `mmm/constraints.py:26`, `utility.py:91+` | **Tier 1** — new `/optimize` endpoint | Portfolio Kelly across slate, CVaR, Sharpe |
| 4 | Counterfactual via `pm.do` | `customer_choice/mv_its.py:341` | **Tier 1** — new `mc model counterfactual` | "What if PNC's PF=1.05 from day 1?" |
| 5 | Evidence injection / lift tests | `mmm/lift_test.py:219` | **Tier 2** — cartridge primitive | Closing-line-value calibration |
| 6 | Posterior-weighted what-if | `lift_test.py` + `whatif` composition | **Tier 2** — extension of `/whatif` | Cheap Bayesian update without refit |
| 7 | Causal DAG / DoWhy backdoor | `mmm/causal.py:1301` | **Tier 3** — defer until MMM cartridge | Limited applicability |

Each row is independently shippable. Their composition — Bayesian
fitted models with hierarchical priors, time-varying coefficients,
counterfactual evaluation, and constrained optimization over posterior
samples — is the full Bayesian LNM vision ADR-0009 set up.

---

## Pattern 1: Hierarchical `Prior` YAML — adopt as serialization convention

**Origin.** `pymc_marketing/prior.py:118-163`. Nested `Prior` objects
serialize to recursive YAML/dict with `distribution`, `dims`,
`centered`, and named hyperparameters.

**User API:**
```python
hierarchical_beta = Prior("Normal",
    mu=Prior("Normal"),           # global mean hyperprior
    sigma=Prior("HalfNormal"),    # global scale hyperprior
    dims="channel",
    centered=False)               # non-centered for sampling stability
```

**YAML form (already in pymc-marketing):**
```yaml
distribution: Normal
mu: { distribution: Normal, mu: 0, sigma: 1 }
sigma: { distribution: HalfNormal, sigma: 1 }
dims: [geo, channel]
centered: false
```

**Why this is borrowable.** The `Prior` YAML shape is exactly the
cartridge-friendly serialization Mosaic would want for declaring a
parameter that varies over a dim with hyperparameters. Mosaic doesn't
sample, but it can adopt this as the canonical schema for
"distribution-valued parameter that varies over cube dimensions" — the
Python trainer attaches a posterior tensor `(samples, *dims)`, and
Mosaic evaluates per-coordinate with the dim-indexed slice.

**Proposed Mosaic cartridge field:**
```yaml
fitted_models:
  - name: mlb_v11_hierarchical
    kind: bayesian_hierarchical
    parameters:
      coefficient_home_xera:
        distribution: Normal
        dims: [team_pair]
        centered: false
        # Posterior tensor (samples × team_pair) lives in this artifact:
        samples_artifact: mlb_v11_team_pair_coefs.parquet
        summary:
          mean_artifact: mlb_v11_team_pair_coef_means.parquet
          std_artifact: mlb_v11_team_pair_coef_stds.parquet
```

**Sports-betting analog.** Per-team-pair NB dispersion alpha
partial-pooled to global. Rare matchups (Marlins @ Athletics: ~5
games/year) lean toward global α=0.1245; common matchups (NL East
divisional: ~76 games/year over the training window) get their own α.

**Cross-link.** This composes with distribution-valued cells. The cube
authors a `team_pair` dimension; the cartridge declares an alpha that
varies over `team_pair`; the kernel evaluates by indexing into the
posterior tensor at each (game's team_pair) coordinate.

---

## Pattern 2: Time-varying coefficient artifacts — consume, don't generate

**Origin.** `pymc_marketing/mmm/tvp.py:275-329`. pymc-marketing
implements time-varying coefficients via **Hilbert Space Gaussian
Process (HSGP)** — reduced-rank GP with Matern52 covariance. The user
sets `time_varying_intercept=True` / `time_varying_media=True` and PyMC
handles the sampling.

**The artifact (post-fit).** A posterior tensor with dims `(chain,
draw, date, [custom_dims], [channel])`. The HSGP basis itself
(frequencies, X_mid) is stored as "frozen deterministics" for
out-of-sample evaluation (`mmm.py:1417-1437`).

**Why this is borrowable (but only the artifact shape).** Mosaic
doesn't need the HSGP machinery — it's heavy and Python-trained.
Mosaic just needs to consume **the evaluated multiplier values per
(date, channel) coordinate**. The Python trainer handles the
HSGP+sampling; the artifact is a coord-indexed coefficient cube.

**Proposed Mosaic cartridge field:**
```yaml
fitted_models:
  - name: mmm_v2_tvp
    kind: bayesian_linear_tvp
    coefficients:
      tv_spend:
        time_varying: true
        coord_dims: [Time, geo]
        values_artifact: tv_coef_tvp.parquet
        # Schema: rows are coord tuples, columns are samples
        # OR: schema_mean + schema_std + schema_quantiles for parametric
```

The cartridge author treats this like a static coefficient — they don't
care it came from a GP. The kernel looks up `coef[Time=2025-04, geo=NE]`
and either returns the mean (cheap path) or the sample vector
(distribution-aware path).

**Sports-betting analog.** xERA's predictive weight as a function of
`Time` (season-month). Early-season xERA from the previous year is a
strong signal; mid-season fresh xERA dominates. Currently Mosaic treats
the coefficient as static; making it time-varying via this pattern
would let the cartridge automatically apply era-appropriate weights.

---

## Pattern 3: Constraint + utility registry — new `/optimize` endpoint

**Origin.** Three files compose this:
- `mmm/constraints.py:26` — `Constraint(key, "eq"|"ineq", constraint_fun)`, symbolic
- `mmm/constraints.py:66` — `compile_constraints_for_scipy` (auto-differentiates, emits `{type, fun, jac}` for SciPy)
- `mmm/utility.py:91+` — pluggable utilities: `average_response`, `value_at_risk`, `conditional_value_at_risk`, `sharpe_ratio`, `raroc`, `portfolio_entropy`
- `mmm/budget_optimizer.py:1322` — SLSQP `minimize(...)` with `jac=True`

**User API:**
```python
optimizer = BudgetOptimizer(
    model=mmm,
    num_periods=8,
    utility_function=conditional_value_at_risk(0.95),
    custom_constraints=[my_constraint])
allocation, result = optimizer.allocate_budget(
    total_budget=1_000_000,
    budget_bounds={"FB": (50_000, 400_000), "TV": (50_000, 600_000)})
```

**Why this is borrowable.** Three primitives generalize cleanly:

1. **`Constraint` as a kernel primitive** — `(key, kind, symbolic_fn)`.
   Decouples *what's optimized* from *the solver*.
2. **Utility-function registry over posterior samples** — `samples × decision_vars → scalar`.
   Drop-in: `expected_log_growth` (Kelly), CVaR, Sharpe.
3. **`/optimize` HTTP endpoint** mirroring `/sweep` shape:
   `decision_vars`, `constraints`, `utility`, returns `{allocation, expected_utility, gradient, per_iter_trace}`.

**Proposed Mosaic `/optimize` request:**
```json
POST /api/v1/optimize
{
  "cube": "mlb-totals",
  "decision_vars": [
    {"name": "stake_game_1", "bounds": [0.0, 0.025], "init": 0.005},
    {"name": "stake_game_2", "bounds": [0.0, 0.025], "init": 0.005}
  ],
  "constraints": [
    {"kind": "ineq", "expr": "1.0 - sum(stake_*)"},
    {"kind": "ineq", "expr": "0.10 - sum(stake_*)"}
  ],
  "utility": "expected_log_growth",
  "where": {"Time": "2026-05-28"},
  "method": "SLSQP"
}
```

**Sports-betting analog.** Tonight's slate has 15 games.
`/whatif` evaluates each game's edge independently. `/optimize` solves
the portfolio problem: optimal stake vector subject to total exposure ≤
10%, no single bet > 2.5%, expected_log_growth maximized over the joint
posterior. This is exactly what `predict_today.py` currently does
per-bet via Kelly + then layers cap rules on top — but as separate
heuristics, not a joint solve. The full joint solve is materially better
for correlated games (weather, umpire bias).

**Risk-aware utility variants:**
- `expected_log_growth` — Kelly criterion
- `cvar_0.05` — minimize CVaR at 5% (conservative bankroll preservation)
- `sharpe_ratio` — risk-adjusted return
- `mean_minus_variance(lambda)` — Markowitz-style

---

## Pattern 4: Counterfactual via `pm.do` — new `mc model counterfactual`

**Origin.** `pymc_marketing/customer_choice/mv_its.py:341` —
`calculate_counterfactual()` uses `pm.do(self.model, {"treatment_sales": zero_sales})`
to lock an input to a counterfactual trajectory, then samples the
posterior predictive over the modified graph.

**Why this is borrowable.** Mosaic's `/whatif` already overrides feature
values at a point. Counterfactual is the same mechanism extended over a
**time-range** with a **paired comparison** output: observed vs.
counterfactual + credible interval on the delta.

**Proposed Mosaic command:**
```bash
mc model counterfactual mlb-totals.yaml \
  --set "PNC=PNC_with_PF_1.05" \
  --override "park_factor.PNC=1.05" \
  --range "2024-04-01:2024-09-30" \
  --show Predicted_Total,P_Over_NB,Should_Bet,PnL \
  --emit observed,counterfactual,delta,ci_95 \
  --output exp038c_pnc_counterfactual.json
```

Returns:
```json
{
  "observed": {
    "Predicted_Total_mean": 8.72,
    "PnL_total": -340.50
  },
  "counterfactual": {
    "Predicted_Total_mean": 9.18,
    "PnL_total": +1240.75
  },
  "delta": {
    "PnL_total": +1581.25,
    "PnL_total_ci_95": [+820, +2150],
    "Should_Bet_flips": 47
  }
}
```

**Why this beats `/sweep`.** `/sweep` varies one parameter and reports a
curve. `/counterfactual` locks an entire trajectory (potentially many
parameters) and reports a paired comparison. The "what if the cartridge
had PF=1.05 from day 1" question is a counterfactual, not a sweep.

**Caveat (Agent 3 surfaced this).** Counterfactual is only as causal as
the underlying rule graph. If `pnl = f(park_factor, ...)` is
correlational (which it is — Mosaic's cube doesn't have a causal model
of HOW park_factor affects PnL beyond the Lasso fit), the result is a
**sensitivity analysis**, not a causal effect. The command's docstring
must say this honestly. For genuine causal analysis you need a causal
DAG (Pattern 7, deferred).

---

## Pattern 5: Evidence injection / lift tests — cartridge primitive

**Origin.** `pymc_marketing/mmm/lift_test.py:219` —
`add_saturation_observations(df, variable_mapping, saturation_function, dist=Gamma)`.
Lift tests are added as **extra likelihood terms**, not coefficient
priors. The model computes its own implied lift, then a Gamma
observation reconciles with the measured lift.

**User API:**
```python
df = pd.DataFrame({
    "channel": ["FB", "FB"],
    "x": [100_000, 100_000],
    "delta_x": [20_000, 20_000],
    "delta_y": [28_000, 32_000],
    "sigma": [3_000, 3_000]
})
mmm.add_lift_test_measurements(df, dist=pmd.Gamma)
mmm.fit(X, y)
```

**Why this is borrowable.** The generic shape is: "at coordinate C,
when input moved by Δx, output moved by Δy ± σ." That's domain-agnostic.

**Sports-betting analog: closing-line value (CLV) calibration.**
Structurally identical:
- `x = open_price`
- `delta_x = close - open`
- `delta_y = realized_edge`
- `sigma = N^{-1/2}` (sample size scaling)

A `CLVObservation` cartridge type lets bettors feed historical
bet-vs-closing-line data and ask: "is my probability model miscalibrated
for MLB totals on weekday games?"

**Proposed cartridge primitive:**
```yaml
evidence_observations:
  - name: weekday_clv_2025
    kind: evidence_injection
    coords: { Day_of_Week: [Mon, Tue, Wed, Thu] }
    x_measure: open_price
    delta_x_measure: close_minus_open
    delta_y_measure: realized_edge
    sigma_formula: "1.0 / sqrt(n_bets)"
    likelihood: Gamma
    artifact: weekday_clv_2025.parquet
```

Mosaic doesn't refit. But the same struct drives:
1. **Calibration scoring** — compute model's implied delta at observed
   coords, compute residual = `(observed - model) / sigma`, return a
   z-score per evidence row. High residuals flag model miscalibration.
2. **Posterior-weighted what-if** (Pattern 6) — cheap importance-sample
   reweighting using the evidence as likelihood weights.

---

## Pattern 6: Posterior-weighted what-if — `/whatif` extension

**Origin.** Composition of Patterns 5 and the existing `/whatif`. Not a
distinct pymc-marketing pattern, but emerges naturally from importance
sampling theory.

**The idea.** Given N posterior samples and M evidence observations,
weight each sample by its likelihood under the evidence:
`w_i = ∏_m p(evidence_m | sample_i)`. Normalize. Then a weighted
posterior-predictive computation gives an updated prediction WITHOUT
refitting the model.

**Why this is borrowable.** This is a cheap Bayesian update primitive.
For a cartridge whose Python trainer already produced N=1000 posterior
samples, applying recent calibration evidence is O(N×M) on the daemon,
not "retrain in Python and re-export."

**Proposed Mosaic `/whatif` extension:**
```json
POST /api/v1/whatif
{
  "cube": "mlb-totals",
  "overrides": [...],
  "evidence": [
    {"coord": {"Day_of_Week": "Tue"}, "x": 8.5, "observed_edge": 0.07, "sigma": 0.02}
  ],
  "where": {...},
  "show": ["P_Over_NB", "Edge_NB"]
}
```

The response's measures reflect both the override AND the importance-
weighted posterior update. Without the `evidence` field, behavior is
identical to today's `/whatif`.

**Caveats.** Importance sampling degenerates when the proposal (the
prior posterior) is too far from the target (after evidence). Effective
sample size monitoring is needed; degraded ESS triggers a "evidence too
strong for cheap update; retrain required" warning.

---

## Pattern 7: Causal DAGs via DoWhy — defer

**Origin.** `pymc_marketing/mmm/causal.py:1274-1394`. Wraps DoWhy's
`CausalModel`, accepts a DOT-string DAG, computes backdoor adjustment
sets via Pearl-style do-calculus.

**User API:**
```python
graph = 'digraph { promo -> tv_spend; promo -> sales; tv_spend -> sales; }'
cgm = CausalGraphModel.build_graphical_model(
    graph=graph, treatment=["tv_spend"], outcome="sales")
controls = cgm.compute_adjustment_sets(
    channel_columns=["tv_spend"],
    control_columns=["promo", "weather"])
```

**Why DEFER.** Domain fit is narrow:
- Marketing MMM has rich confounding structure (promo → spend → sales).
- Sports betting has limited confounding — `park_factor` is exogenous;
  `home_starter_xera` is observable.
- Adopting DAGs requires authoring the graph in YAML, which is heavy
  authoring overhead for cube authors.

**When to revisit.** When/if a marketing MMM cartridge ships and the
confounding question becomes load-bearing. The DOT-string + DoWhy
adjustment-set computation would be the right primitive to import; the
PyMC-side `BuildModelFromDAG` (which compiles DAG → Bayesian model) is
out of scope.

---

## Cross-cutting finding: the export contract for Bayesian models

**The serialization gap (Agent 4).** pymc-marketing saves models as
NetCDF (`.nc`) via ArviZ InferenceData. The model graph is embedded as
JSON in NetCDF attrs and re-instantiated as PyMC ops on load. **None of
this is consumable from Rust.**

**Critical finding.** For nonlinear models (adstock, saturation,
hierarchical), `E[f(θ)] ≠ f(E[θ])`. You **cannot** reduce a Bayesian
MMM to `{coef_mean, coef_std}` — the joint posterior is load-bearing
for correct predictions.

**For Mosaic to consume a fitted Bayesian model:**
1. Python trainer exports posterior samples to **parquet** (not NetCDF),
   flat table form, one column per parameter.
2. Plus transformation metadata (adstock type, l_max, saturation form)
   in JSON.
3. Mosaic's `predict()` reimplements the forward pass in Rust and loops
   over samples.

**Proposed Mosaic cartridge field for Bayesian fitted models:**
```yaml
fitted_models:
  - name: mmm_v2_bayes
    kind: bayesian_glm
    transforms:
      - { type: adstock, channel: tv, max_lookback: 8 }
      - { type: saturation, channel: tv, form: hill }
    samples:
      coefficients_artifact: mmm_v2_coef_samples.parquet     # rows: sample_idx, cols: channel
      intercept_artifact: mmm_v2_intercept_samples.parquet
      sigma_artifact: mmm_v2_sigma_samples.parquet           # residual scale per sample
    n_samples: 4000
    summary:                                                  # convenience: cheap mean path
      coefficient_mean: { tv: 0.42, digital: 0.18, ... }
      coefficient_std:  { tv: 0.05, digital: 0.03, ... }
    link: identity
```

Then `predict("mmm_v2_bayes", ..., return="mean")` does the cheap
`X @ coef_mean + intercept_mean` dot-product (approximate for linear,
exact for none). `predict("mmm_v2_bayes", ..., return="samples")` does
the full sample-loop forward pass. `predict(..., return="quantile:0.95")`
runs samples and returns the requested quantile.

This composes with **Pattern 1's hierarchical Prior YAML** (declare the
parameter structure) and **distribution-valued cells** (the kernel
returns a distribution, not a scalar, when called with `return="samples"`).

---

## Recommended phasing

**Tier 1 (Phase 11 territory — builds on distribution-valued cells):**

| Phase | Scope | Builds on |
|---|---|---|
| 11A | Bayesian `fitted_models` import (posterior samples parquet, summary fields, transforms metadata) | Distribution-valued cells Phase A-D |
| 11B | `mc model counterfactual` command + `/api/v1/counterfactual` endpoint | `/whatif` infrastructure |
| 11C | `mc model optimize` command + `/api/v1/optimize` endpoint (constraints + utility registry, SLSQP via Rust solver crate) | Distribution-valued cells (for risk-aware utilities) |

**Tier 2 (Phase 12 territory):**

| Phase | Scope | Builds on |
|---|---|---|
| 12A | `evidence_observations:` cartridge primitive + calibration scoring | Patterns 5 + Tier 1 |
| 12B | Posterior-weighted `/whatif` (importance-sample reweight) | Patterns 5 + 6 + Tier 1 |
| 12C | Hierarchical Prior YAML adoption (serialization-only; evaluation via coord-indexed posterior lookup) | Pattern 1 |

**Tier 3 (deferred):**

- Time-varying coefficient artifacts — adopt the artifact-consumer
  pattern, defer the HSGP-trainer pattern entirely
- Causal DAGs via DoWhy — defer until MMM-style cartridge demand

**Out of scope forever:**
- MCMC / NUTS samplers in `mc-core`
- PyMC graph compilation
- Variational inference

---

## Open questions for the ADR phase

1. **Rust SLSQP availability.** scipy.optimize.minimize uses SLSQP via
   Fortran. Are there well-maintained Rust crates? (`argmin` crate
   supports L-BFGS, Nelder-Mead, others; SLSQP support is less mature.)
   If not, ship `mc model optimize` as a CLI that shells to a Python
   optimizer initially (similar to how Tessera shells to drivers).

2. **Parquet sample storage scale.** A 4000-sample × 13-coefficient
   matrix is ~400 KB. A 4000-sample × (13 × 30 team-pairs) hierarchical
   matrix is ~12 MB. Per-cube storage budget needs review.

3. **Importance sampling effective sample size.** Pattern 6's
   posterior-weighted what-if needs ESS monitoring. Threshold below
   which we warn? Below which we refuse and require refit? Likely a
   per-cube tunable.

4. **YAML schema versioning for `fitted_models` extensions.** The
   current Lasso fitted_models field has a stable schema. Adding
   Bayesian variants (`kind: bayesian_glm`, `kind: bayesian_hierarchical`,
   `kind: bayesian_linear_tvp`) is a major schema expansion. Versioning
   strategy needs an ADR.

5. **Causal vs sensitivity language.** Pattern 4's counterfactual
   command returns "sensitivity," not "causal effect," unless the cube
   author has declared a DAG (Pattern 7). The documentation must be
   honest about this. Naming the command `counterfactual` may oversell;
   `sensitivity-trajectory` is more honest but less catchy.

---

## Cross-links

- **Companion notes:**
  - [`distribution-valued-cells.md`](./distribution-valued-cells.md) — the foundational primitive
  - [`built-in-evaluation-primitives.md`](./built-in-evaluation-primitives.md) — the five evaluation commands
- **pymc-marketing repo:** https://github.com/pymc-labs/pymc-marketing (cloned at /tmp/pymc-marketing-research)
- **ADR-0009:** LNM substrate vision — these Bayesian primitives are
  what "AI-native planning kernel for quantitative domains" looks like
  at the upper bound of capability
- **ADR-0018 (Phase 3H.2):** Adstock + saturation — pymc-marketing has
  the same primitives, validating Mosaic's existing implementation
  against the field-standard library
- **ADR-0031:** nbinom_sf — same family as pymc-marketing's
  distributional primitives; the design pattern transfers
- **ADR-0032:** Phase 8.2 consumer API — `/whatif` and `/sweep` are the
  single-game versions; `/counterfactual` and `/optimize` are the
  Bayesian extensions

---

## Notes

- **Independent agent verification.** Each pattern was sourced by a
  separate exploration agent reading the cloned repo. File paths and
  line numbers cite the agent's evidence, not the synthesizer's
  recollection. Spot-check by reading the cited lines.
- **Sport-betting analogs are existence proofs, not commitments.**
  Each pattern shows the analog to demonstrate cross-domain
  applicability; whether claw-core actually wants the analog is a
  separate decision per cartridge.
- **The trilogy is now complete.** Distribution-valued cells (the
  foundation), evaluation primitives (the commands), and this pattern
  extraction (the Bayesian techniques) together describe the design
  space for the next ~2-3 phases of Mosaic. None of the three notes
  commit to implementation; all three feed into future ADRs as
  source material.
- **Repo cleanup.** `/tmp/pymc-marketing-research` can be removed
  after the ADR phase consumes these findings. The note + the cited
  file paths preserve the evidence trail.
