# Distribution-Valued Cells — From Point Estimates to Credible Intervals

**Status:** Research note (pre-ADR; explores the design space)
**Date:** 2026-05-27
**Author:** Mosaic PM (Claude Opus 4.6, 1M context)
**Companion to:** [`built-in-evaluation-primitives.md`](./built-in-evaluation-primitives.md) — those five commands plus this primitive together unlock Bayesian analytics in Mosaic without an MCMC sampler in the kernel.

---

## The observation

Today every cell in a Mosaic cube holds a `ScalarValue::Number(f64)` (or
`Null`, `Boolean`, etc.). Predictions are point estimates. `Predicted_Total
= 8.832665` — a number. There is no way to express "the model believes
this is roughly 8.83 ± 0.45, with 95% credible interval [7.95, 9.71]."

This is a foundational limitation. Five distinct production needs surface
from it:

1. **Risk-aware bet sizing.** Should_Bet should fire when the 95% CI lower
   bound of edge exceeds 10pp, not when the point estimate does. A bet
   with point edge 0.12 but CI [0.02, 0.22] is structurally different
   from one with point edge 0.12 and CI [0.10, 0.14] — the first is a
   coin flip dressed as a coin flip, the second is a real edge.
2. **Calibration diagnostics.** "How often does the actual value fall
   inside the predicted 95% interval?" requires the interval to exist.
3. **Counterfactual confidence.** When `/sweep` says "Should_Bet flips at
   park_factor 1.05," we currently don't know if that flip is robust or
   on the edge of the model's uncertainty. With distributions, "Should_Bet
   flips with 80% posterior probability" is a different conclusion from
   "Should_Bet flips on a knife edge."
4. **Hierarchical / partial pooling.** Each Team or Region having its
   own coefficient drawn from a global prior requires the global prior
   to be a distribution, not a scalar.
5. **Posterior-aware portfolio optimization.** Multi-bet Kelly that
   accounts for correlated uncertainty across tonight's slate needs
   joint posterior samples, not marginal point estimates.

Every one of those problems collapses to: **cells need to carry their
distribution, not just their expected value.**

---

## Why this is the foundational primitive

The earlier `built-in-evaluation-primitives.md` research note proposed
five commands (`backtest`, `grade`, `simulate`, `sweep`, `walk-forward`)
plus a metrics library. The companion `pymc-marketing` exploration
surfaced five Bayesian primitives worth importing (time-varying
coefficients, posterior intervals, counterfactuals, multi-objective
optimization, hierarchical priors).

All ten of those primitives become *more powerful* when cells are
distributions — and four of them are *only meaningful* when cells are
distributions:

| Primitive | Works on point estimates? | Works on distributions? |
|---|---|---|
| `mc model backtest` (parameter sweep × holdout) | yes | richer — metrics gain confidence bands |
| `mc model grade` (segmented holdout) | yes | richer — coverage metrics become possible |
| `mc model simulate` (Monte Carlo on bet records) | yes | richer — posterior-aware sizing |
| `mc model sweep` (single-game sensitivity) | yes | richer — flip robustness |
| `mc model walk-forward` | yes | richer — calibration tracked per fold |
| Adstock + saturation | yes (already shipped) | yes |
| **Time-varying coefficients** | partial — points only | **natural fit** |
| **Posterior credible intervals** | **NO — definitionally impossible** | **YES** |
| **Counterfactual / MVITS** | partial — bias decomposition only | **proper causal inference** |
| **Multi-objective optimization** | yes — but no risk-aware variants | **richer — CVaR, mean-variance** |
| **Hierarchical / partial pooling** | **NO — partial pooling IS a distribution operation** | **YES** |

The four bolded entries are blocked without distribution-valued cells.
The other six get materially better.

---

## Representation: three candidate approaches

The core question is *how* a distribution is stored. There are three
defensible approaches, each with different trade-offs.

### Approach A: Parametric only

A cell stores `ScalarValue::Distribution { family, params }`. Families
are a closed enum:

```rust
enum DistributionFamily {
    Normal { mean: f64, std: f64 },
    LogNormal { mu: f64, sigma: f64 },
    NegativeBinomial { mu: f64, alpha: f64 },
    Beta { alpha: f64, beta: f64 },
    Gamma { shape: f64, scale: f64 },
    Uniform { low: f64, high: f64 },
    PointMass { value: f64 },   // backward compat — scalar IS a degenerate distribution
}
```

**Pros:**
- Compact storage — 2-3 f64 per cell vs N samples
- Analytical operations where they exist (Normal + Normal → Normal,
  LogNormal × LogNormal → LogNormal in log-space, etc.)
- Fast: arithmetic is O(1)
- Naturally maps to scipy.stats families that PyMC and statsmodels emit

**Cons:**
- Family-closed: can't represent arbitrary posteriors (e.g., the output
  of `predict() * adstock()` on a Normal prior is rarely Normal)
- Approximations everywhere: `Normal + LogNormal` falls back to moment
  matching, which throws away information
- Mixture distributions and multimodal posteriors don't have a family

### Approach B: Posterior samples

A cell stores `ScalarValue::Samples(Vec<f64>)` — a 1000-element vector
of posterior draws.

**Pros:**
- Universally expressive — any distribution can be represented as samples
- Arithmetic is element-wise: `samples_a + samples_b` is trivially correct
  if samples are aligned (joint posterior) or correctly approximates
  marginals (independent assumption)
- Matches PyMC's natural output (`trace.posterior` is samples)
- No analytical approximations needed for any operation

**Cons:**
- Storage: 8 KB per cell at N=1000 samples. A 2,500-game MLB cube with
  30 measures becomes 600 MB instead of 6 MB.
- Compute: every operation is O(N). `predict() * adstock() * saturation()`
  costs ~3000 multiplications per cell.
- Sampling correlation: independent samples assume independence; joint
  posteriors require coordinated sample indexing across cells. This
  introduces "sample alignment" as a new invariant.
- Aggregations (mean, sum) over many cells produce huge intermediate
  arrays.

### Approach C: Hybrid (Parametric default, Samples on demand)

A cell stores whichever representation is most natural for how it was
produced:

```rust
enum DistributionRepr {
    Parametric(DistributionFamily),
    Samples(Vec<f64>),
    Lazy(Box<dyn FnOnce() -> DistributionRepr>),  // computed on demand
}
```

Cells produced by closed-form operations stay parametric. Cells produced
by operations that lose family closure get materialized as samples. The
formula evaluator decides based on op signatures:

| Op | Inputs | Output |
|---|---|---|
| `Normal + Normal` | parametric, parametric | parametric (Normal) |
| `Normal + LogNormal` | parametric, parametric | samples (no closed form) |
| `Normal * scalar` | parametric, scalar | parametric (Normal, scaled) |
| `nbinom_sf(line, Normal_mu, alpha)` | scalar, parametric, scalar | samples (push through nonlinear) |
| `mean(distribution)` | parametric | scalar (closed form) |
| `mean(samples)` | samples | scalar (numerical) |

**Pros:**
- Best of both worlds where the cases are clean
- Backward compatible: existing point-estimate cubes stay as PointMass
  (essentially Parametric) with no memory penalty
- Lazy materialization avoids paying for samples until a sample-requiring
  op runs

**Cons:**
- Implementation complexity — every formula primitive needs cases for
  parametric and samples inputs
- The "when does parametric flip to samples" rule is non-obvious to
  cube authors; debugging "why is this cell suddenly 8 KB" requires
  tracing back through the formula graph
- Lazy evaluation interacts oddly with caching (Phase 8.0's per-coord
  cache) — what does it mean to cache a Lazy?

### Recommendation (provisional, for the ADR phase)

**Approach C (Hybrid) with constraints to limit complexity:**

1. Default to **Parametric** when both inputs are parametric AND the op
   has a closed form
2. Auto-materialize to **Samples(N=1000)** when any operation loses
   family closure
3. **No Lazy** in v1 — every distribution is concrete (Parametric or
   Samples). Lazy is a v2 optimization if/when materialization cost
   shows up in profiles.
4. **Sample alignment via deterministic seeds:** every distribution-
   producing operation seeds its sampler from a hash of `(coord,
   formula_path)`. This makes samples reproducible AND makes "joint vs
   marginal" explicit — joint samples come from the same seed family,
   marginal samples come from different seeds. Cube authors opt into
   joint sampling with a `joint!` annotation.

This is the v1 commitment. Approach A or B can win if v1 hits
performance or expressiveness walls.

---

## Propagation semantics

The kernel currently propagates `ScalarValue` through formulas:

```
Predicted_Total (Number) → P_Over_NB = nbinom_sf(line, Predicted_Total, alpha) → Number
```

With distribution-valued cells, the propagation becomes:

```
Predicted_Total (Normal(μ̂, σ̂)) → P_Over_NB = nbinom_sf(line, Predicted_Total, alpha)
                                            → Samples (push the distribution through nbinom_sf)
                                            → Materialized as 1000 samples
                                            → mean(samples) ≈ 0.577
                                            → quantile(samples, 0.025) ≈ 0.510
                                            → quantile(samples, 0.975) ≈ 0.642
```

The kernel's job is propagation; the consumer's job is summarization.
Three reductions are first-class:

| Reduction | Returns | Use case |
|---|---|---|
| `mean(cell)` | scalar | backward-compat point estimate |
| `quantile(cell, q)` | scalar | credible interval bounds |
| `prob_above(cell, threshold)` | scalar | "P(edge > 10pp)" for risk-aware filtering |

These reductions are how the existing formula language continues to
work without a wholesale rewrite. Authors who don't care about
distributions write `mean(Predicted_Total)` and get back exactly what
they got before. Authors who want uncertainty write
`quantile(Predicted_Total, 0.025)` and get the lower bound.

**Critical default:** when a derived measure references a distribution
cell without an explicit reduction, the engine implicitly takes `mean()`.
This preserves backward compatibility — existing cartridges keep working
because every measure that uses a fitted-model output just gets its
point estimate as before. Distribution awareness is opt-in via explicit
reductions.

---

## Formula primitive extensions

Every existing math primitive must define its distribution semantics:

| Primitive | Distribution semantics |
|---|---|
| `+`, `-`, `*`, `/` | Element-wise on samples; closed form for compatible parametric pairs |
| `pow`, `sqrt`, `ln`, `exp` | Push through samples; analytical for LogNormal where applicable |
| `min`, `max` | Element-wise on samples |
| `if(cond, a, b)` | If `cond` is a Boolean scalar, normal semantics; if `cond` is itself a distribution (i.e., `prob_above(...) > 0.5`), the result is a mixture |
| `norm_cdf`, `nbinom_sf`, `nbinom_cdf` | Already produce probabilities; with distribution-valued μ inputs, push samples through and produce distribution outputs |
| `predict(model, ...)` | When the fitted model is Bayesian (has posterior samples), returns Samples; when frequentist (Lasso), returns Parametric Normal using residual_std |
| `calibrate(p, "map")` | PAVA is monotonic; element-wise on samples preserves calibration |
| `adstock(...)`, `saturation(...)` | Already-shipped Phase 3H.2 primitives; extend element-wise |

**New reductions:**
- `mean(cell)` — expected value
- `std(cell)` — standard deviation  
- `quantile(cell, q)` — q ∈ [0, 1]
- `credible_interval(cell, level)` — returns a 2-tuple [low, high]
- `prob_above(cell, threshold)`, `prob_below(cell, threshold)` — P(X > t), P(X < t)
- `sample(cell, k)` — draw k samples (useful for downstream Monte Carlo)
- `point(cell)` — explicit "I want the point estimate" (same as `mean` for symmetric distributions; useful when authors want explicit clarity)

---

## Storage and performance

### Memory cost

| Cube shape | Point estimate | Hybrid (90% parametric, 10% samples) | All samples (worst case) |
|---|---|---|---|
| Acme (2,520 cells) | 20 KB | ~250 KB | 20 MB |
| NBA (~10K cells) | 80 KB | ~1 MB | 80 MB |
| MLB (~290K cells, 9.7K games × 30 measures) | 2.3 MB | ~30 MB | 2.3 GB |

The all-samples worst case is prohibitive for MLB-scale cubes. The hybrid
approach is acceptable. If most cells stay point estimates (which they
will — Input measures are always points; only model-output derived
measures become distributions), the cost is manageable.

### Compute cost

Phase 1B benchmarks showed warm-cache reads at ~67 ns. Distribution
reads need:
- Parametric: ~67 ns (still one struct)
- Samples (N=1000): ~8 µs (read + minor work) — 120× slower

A cube that's 90% parametric and 10% samples sees ~10× p99 read latency.
Acceptable if the 10% is the right 10% (model outputs, which the
consumer DOES want with uncertainty).

### Caching

Phase 8.0's per-coord cache stores `ScalarValue` per `(coord, revision)`.
With distribution-valued cells, the cache stores larger values. Cache
budget enforcement (currently 512 MB default) needs to account for
sample-valued entries being ~1000× the size of point-valued entries.

**Mitigation:** the cache budget enforcement is already in place from
Phase 8.0. The distribution work just changes the byte-counting math.

---

## Backward compatibility

This is the make-or-break design constraint. Existing cubes (Acme,
NBA, MLB) must continue to work unchanged.

**The default-mean rule** preserves this. Every existing rule that
references a measure gets the `mean()` automatically. The
`Predicted_Total` cell can become `Normal(8.83, 0.45)` and:

- `P_Over_NB = nbinom_sf(line, Predicted_Total, alpha)` — implicitly
  becomes `nbinom_sf(line, mean(Predicted_Total), alpha)` = scalar
- Old cubes work identically: zero diff in output
- New cubes can opt into uncertainty by replacing `Predicted_Total`
  with `quantile(Predicted_Total, 0.025)` in the rule body, OR by
  evaluating `nbinom_sf(line, Predicted_Total, alpha)` with the
  distribution-aware path (which returns Samples)

**Migration path for cartridges:**
1. v0 cartridge: all rules use scalars (current state)
2. v1 cartridge: fitted models declare posterior samples; downstream
   rules opt into uncertainty by using `quantile()` / `prob_above()`
   in `Should_Bet` formulas
3. v2 cartridge: full Bayesian propagation; every model output is a
   distribution, every derived measure carries its credible interval

Authors choose their migration speed. Mosaic doesn't force the shift.

---

## What this unlocks (the concrete payoff)

Once distribution-valued cells ship, the following become possible:

### Risk-aware Should_Bet
```yaml
- name: should_bet_risk_aware
  target_measure: Should_Bet
  body: 'if(prob_above(Edge_NB, 0.10) >= 0.8, 1, 0)'
```
Fires only when there's ≥ 80% posterior probability that edge exceeds
10pp. Replaces today's point-estimate "edge > 0.10" with a uncertainty-
aware filter.

### Credible-interval reporting
```yaml
- name: P_Over_CI_lower
  body: 'quantile(P_Over_NB, 0.025)'
- name: P_Over_CI_upper
  body: 'quantile(P_Over_NB, 0.975)'
```
Cards display "P(over) = 0.577 [0.510, 0.642]" instead of "P(over) = 0.577".

### Calibration tracking
```yaml
- name: coverage_95
  body: 'if(actual_total > quantile(Predicted_Total, 0.025) and actual_total < quantile(Predicted_Total, 0.975), 1, 0)'
- name: coverage_rate
  body: 'mean(coverage_95) over Time=2025'
```
If `coverage_rate < 0.90`, the model is overconfident and credible
intervals are too narrow.

### Posterior-aware portfolio Kelly (more advanced)
With joint posterior samples across tonight's slate, the Monte Carlo
simulation can compute portfolio-level optimal stakes that account for
correlated outcomes (e.g., two bets on weather-dependent games are not
independent).

### Hierarchical fitted models
A `fitted_models` entry can declare a hierarchical structure:
```yaml
fitted_models:
  - name: mlb_v11_hierarchical
    type: hierarchical_lasso
    levels:
      - global
      - team_pair
    global_prior:
      coefficients: { ... }
      coefficient_std: { ... }   # how much team-pair can deviate
    team_pair_posteriors: { ... }  # per-pair Lasso draws
```
Then `predict("mlb_v11_hierarchical", team_pair=X, ...)` returns the
team-pair-specific distribution, partial-pooled toward global for rare
pairs.

---

## The integration pattern (Python trains, Mosaic evaluates)

We've already established this pattern for Lasso. It extends naturally
to Bayesian models:

```
Python                                      Mosaic
─────                                        ──────
PyMC fits the Bayesian MMM     ─────→       Cartridge consumes the posterior
(NUTS sampling, MCMC)                       samples as a fitted_model with
                                            posterior_samples field

export_to_mosaic.py:                        fitted_models:
  - posterior_samples: 1000 draws            - name: mmm_v2
    of (intercept, coefs, sigma)              type: bayesian_linear
  - convergence: r_hat, ESS                   posterior_samples_path: ...

                                            predict("mmm_v2", channels)
                                            → returns Normal(mu, std)
                                            (mu = mean of posterior predictive,
                                             std = std of posterior predictive)
```

No MCMC sampler in `mc-core`. The Bayesian inference happens in Python
(PyMC, sklearn-bayes, statsmodels). Mosaic consumes the posterior as
an artifact, exactly like it consumes Lasso coefficients today.

This keeps `mc-core` lean (no heavy stats deps), preserves the
deployment-agnostic kernel discipline (CLAUDE.md §1), and lets the
Bayesian world bring its own samplers.

---

## Scope and phasing (rough)

| Phase | Scope | Estimate |
|---|---|---|
| Phase A | `ScalarValue::Distribution(...)` enum addition; parametric-only; basic arithmetic; backward-compat default-mean | 1 ADR, 4-6 sessions |
| Phase B | Reductions (`mean`, `std`, `quantile`, `prob_above`, etc.) as formula primitives | 1 ADR, 2-3 sessions |
| Phase C | Samples representation; hybrid promotion rules; deterministic seeding for sample alignment | 1 ADR, 4-6 sessions |
| Phase D | Distribution-aware formula primitives (every existing op extended) | 1 ADR, 3-4 sessions |
| Phase E | Bayesian fitted_models import (PyMC posterior samples → Mosaic `predict()`) | 1 ADR, 2-3 sessions |
| Phase F | Cache/storage budget accounting updates | 1 ADR, 1-2 sessions |

Total: ~6 ADRs, ~16-24 sessions. This is a major undertaking — likely
Phase 11 or 12 territory. The five evaluation-primitives commands
(Phase 10) probably ship first since they're independently valuable
and don't require this foundation.

Phase A alone is shippable as "scalar cubes can now hold parametric
distributions." That's already useful — it gives cube authors a way to
record `Normal(8.83, 0.45)` and recover the mean/std/quantiles via
explicit reductions. Samples (Phase C) come after the parametric
foundation is solid.

---

## Open design questions (for the ADR phase)

1. **Sample count default.** N=1000 is PyMC's typical posterior. Lower
   N (200-500) saves memory but loses tail precision. Higher (5000-10000)
   is rarely needed. Make it cube-level configurable with N=1000 default?

2. **Joint vs marginal samples.** When two cells are both Samples, are
   they jointly distributed (correlated draws) or marginally (independent
   draws)? Sport-betting context: tonight's 15 games are mostly
   independent EXCEPT through shared weather/umpire effects. Marketing
   context: channels are correlated through shared market conditions.
   Default to marginal; add `joint!` annotation for explicit joint
   sampling?

3. **Reduction return types.** `quantile(cell, 0.025)` returns a scalar.
   What about `credible_interval(cell, 0.95)`? Tuple? Two separate
   measures? The cube model doesn't have first-class tuples today;
   adding them is a non-trivial type-system change.

4. **Serialization for export/import.** Tessera ingests CSVs and writes
   them to canonical_inputs. Currently those are scalars. How does
   Tessera ingest a 1000-sample posterior per row? Probably: each
   row gets ONE sample (point estimate), and the "this is a distribution"
   metadata lives on the measure declaration, not the cell. Distribution
   reconstruction happens via the fitted_model declaration, not row data.

5. **Caching semantics.** Phase 8.0's per-coord cache stores
   `ScalarValue`. A distribution-valued cell is much larger. Should the
   cache budget be denominated in bytes (not entries)? Probably yes —
   already implied by the 512 MB default.

6. **MCP / API representation.** The daemon returns JSON. A distribution
   cell as JSON is either `{"family": "Normal", "mean": 8.83, "std": 0.45}`
   (parametric) or `{"samples": [8.4, 9.1, 8.5, ...]}` (samples). The
   Worker side (claw-core) needs a typed client; OpenAPI schemas have
   to accommodate both. Discriminated union via `repr` field?

7. **Test parity strategy.** The Acme cube's golden tests use scalar
   point estimates. Distribution support must not break these. The
   default-mean rule covers it, but the test suite needs explicit
   coverage for the "old cube still works" invariant.

---

## What's in scope; what's not

**In scope (the Bayesian primitives this unlocks):**
- Posterior credible intervals on any model output
- Risk-aware filters (`prob_above`, `prob_below`)
- Posterior-aware sweeps and counterfactuals
- Hierarchical fitted models (via the import pattern, not native MCMC)
- Calibration coverage diagnostics

**Out of scope (Phase 12+ or never):**
- An MCMC sampler in `mc-core` — too heavy; Python owns this
- Variational inference in the kernel — same reason
- Causal DAG construction — domain-specific; cube author's job
- Symbolic differentiation through the formula graph — would enable
  HMC/NUTS natively but is a massive scope explosion
- Probabilistic programming language constructs (`pm.Normal`,
  `pm.sample`) — that's PyMC; Mosaic consumes its output

---

## Cross-links

- **Companion research note:** [`built-in-evaluation-primitives.md`](./built-in-evaluation-primitives.md)
- **PyMC-Marketing:** https://github.com/pymc-labs/pymc-marketing (the cross-domain inspiration)
- **ADR-0009:** LNM substrate vision — Bayesian primitives are what
  "AI-native planning kernel" looks like at the upper bound of capability
- **ADR-0025:** Kernel discipline — explicitly preserves the no-heavy-deps
  rule; the integration pattern (Python trains, Mosaic evaluates)
  respects this
- **ADR-0031:** nbinom_sf — current pattern of distributional primitives
  in the formula language. Distribution-valued cells generalize this:
  rather than functions that take scalars and return scalars, the cells
  themselves become distributions.
- **Phase 3H.2 (ADR-0018):** Adstock + saturation — already-shipped
  formula primitives that extend naturally to element-wise on samples
- **claw-core integration test:** `docs/reports/mosaic-integration-test.md`
  — demonstrates the Python-trains-Mosaic-evaluates pattern in
  production today; this note extends that pattern to Bayesian models

---

## Notes

- This is the foundational primitive. The other four cross-domain
  Bayesian primitives (time-varying coefficients, counterfactuals,
  multi-objective optimization, hierarchical pooling) all stack on top
  of it. If Mosaic only adopts one PyMC-Marketing idea, it should be
  this one.
- The Phase A scope (parametric distributions only, backward-compat
  default-mean) is shippable independently and immediately useful.
  Authors who want uncertainty can opt in; existing cartridges keep
  working unchanged. That's the lowest-risk path to validating the
  primitive before committing to the full vision.
- The MCP / API representation question (Open Q 6) is load-bearing for
  claw-core's Worker integration. If distribution cells show up in
  `/whatif` responses, the Worker codegen needs to handle them. That
  argues for the parametric-default approach in Phase A — claw-core
  gets `{"family": "Normal", "mean": 8.83, "std": 0.45}` and decides
  whether to use the mean or the interval.
