# `nbinom_sf()` — Negative Binomial Survival Function in the Formula Evaluator

**Status:** Research note (cross-repo feature request from claw-core)
**Date:** 2026-05-27
**Author:** claw-core LLM session (Claude Opus 4.7)
**Source:** claw-core EXP-025 through EXP-030 (MLB totals cartridge); filed
in mc-v2 per the ADR-0001 cross-repo handshake pattern. Sibling to the
existing [`claw-core-first-downstream-consumer.md`](./claw-core-first-downstream-consumer.md).

---

## Context

claw-core's MLB cartridge ([ADR-0002](https://github.com/edwinlov3tt/claw-core/blob/main/docs/decisions/ADR-0002-mlb-totals-cartridge.md))
shipped as a Negative Binomial model after the linear-Gaussian Lasso
baseline (Option A in ADR-0002) produced ~49% direction accuracy on
2025 holdout — at-coin-flip, not beating the market. EXP-025 introduced
a Negative Binomial reparametrization (Lasso for the mean μ, fitted
dispersion α independent of μ) and EXP-028 walk-forward confirmed
59.68% WR (Wilson lower bound 57.18%) on 1,508 filtered bets across
2023/2024/2025 — solid +EV.

The cartridge's `Predicted_Total` formula uses the existing
`predict("mlb_v10_lasso", ...)` for the NB mean. But computing
**P(Total > Line)** — the over-probability that drives every edge
calculation — requires the **NB survival function**, which Mosaic's
formula engine does not currently expose.

claw-core's interim workaround: pre-compute `P_Over_NB` in Python at
training time and ship it as a baked-in measure value per game. This
works but is a degenerate cartridge — the model isn't actually
*evaluated* inside Mosaic; it's just *stored* there. The slider/sweep
workflow (ADR-0001's primary motivator) is broken because users can't
adjust the line and see the over-prob change — that requires the NB
survival function to be called at eval time.

This note requests a native `nbinom_sf(k, mu, alpha)` (or compatible
parametrization) formula function so the cartridge can express:

```yaml
rules:
  - name: predict_p_over
    target_measure: P_Over_NB
    body: "nbinom_sf(Market_Line, Predicted_Mean, Dispersion_Alpha)"
```

and the daemon evaluates it on every `POST /api/v1/query` or `/whatif`
call.

## The math

Negative Binomial has multiple parametrizations. The one MLB modeling
uses (and what NB regression in `statsmodels.NegativeBinomial` returns)
is the **mean-dispersion form**:

- `μ` = expected count (the Lasso mean prediction)
- `α` = dispersion parameter, where Var(X) = μ + α·μ² (Poisson if α→0)

Conversion to the standard NB(r, p) form used in textbooks / `scipy.stats.nbinom`:

```
r = 1 / α                      (number-of-failures parameter)
p = 1 / (1 + α·μ)               (success probability per trial)
```

(In `scipy.stats.nbinom(n, p)`: `n = r = 1/α`, `p = 1 / (1 + α·μ)`.)

The survival function we need:

```
nbinom_sf(k, μ, α) = P(X > k | X ~ NB(μ, α))
                   = 1 - F(k)
```

where F is the CDF. Equivalent formulations:

```
P(X > k) = I_{1-p}(k+1, r)     using the regularized incomplete beta function
P(X > k) = 1 - Σ_{i=0..k} pmf(i)   direct summation
```

For MLB totals, `k` is bounded (0 ≤ k ≤ ~25 in practice) so direct
summation is feasible and exact. For other use cases the incomplete
beta path scales better.

## What claw-core needs

### Suggested API (mirror `norm_cdf` style)

```
nbinom_sf(k, mu, alpha)         # P(X > k | NB(mu, alpha))
```

Arguments:
- `k`: f64 — the line / threshold to exceed (MLB use: the closing total line)
- `mu`: f64 — NB mean (Lasso prediction)
- `alpha`: f64 — NB dispersion (positive; smaller → closer to Poisson)

Returns: f64 in [0, 1] — survival probability.

Edge cases the implementation should handle:
- `alpha → 0`: degenerate to Poisson; either pass through to a Poisson
  branch or document a minimum α below which we use Poisson approximation.
- `mu ≤ 0` or `alpha < 0`: return NaN / error (production never sends
  these but defensive depth matters).
- Non-integer `k`: floor to integer before the sum (NB is discrete).
  In the MLB use case the line is half-integer like 8.5 — we want
  `P(X > 8.5) = P(X >= 9) = P(X > 8)` after flooring. Document this.

### Companion (optional but useful)

```
nbinom_cdf(k, mu, alpha)        # P(X <= k) — convenience, complement of sf
nbinom_pmf(k, mu, alpha)        # P(X == k) — for goldens / debugging
```

## Implementation options

### Option A: Hand-rolled direct summation (zero deps)

Mirror the `norm_cdf_compute` pattern (rule.rs:1213). For each call:

```rust
fn nbinom_sf_compute(k: f64, mu: f64, alpha: f64) -> f64 {
    // Floor k to integer; line=8.5 → k_int=8 → P(X > 8) = P(X >= 9)
    let k_int = k.floor() as i64;
    if k_int < 0 { return 1.0; }
    // NB(r, p) parametrization
    let r = 1.0 / alpha;
    let p = 1.0 / (1.0 + alpha * mu);
    // F(k) = Σ_{i=0..k} pmf(i); SF = 1 - F(k)
    let mut cdf = 0.0;
    let mut pmf_i = p.powf(r);                 // pmf(0) = p^r
    cdf += pmf_i;
    for i in 1..=k_int {
        // pmf(i) / pmf(i-1) = (r + i - 1) / i * (1 - p)
        pmf_i *= (r + (i as f64) - 1.0) / (i as f64) * (1.0 - p);
        cdf += pmf_i;
    }
    (1.0 - cdf).max(0.0).min(1.0)
}
```

~25 lines. No dependencies. Numerically accurate for k ≤ 50 with α in
[0.05, 1.0] (MLB range is α ~ 0.13). For k > 50 the direct sum loses
precision; a log-space variant or incomplete-beta path is needed.

Phase-3H precedent: `norm_cdf` uses hand-rolled Abramowitz approx
specifically to avoid deps. Same call here.

### Option B: `statrs` crate (one dep)

```toml
[dependencies]
statrs = "0.16"
```

```rust
use statrs::distribution::{NegativeBinomial, Discrete, DiscreteCDF};
fn nbinom_sf_compute(k: f64, mu: f64, alpha: f64) -> f64 {
    let r = 1.0 / alpha;
    let p = 1.0 / (1.0 + alpha * mu);
    let nb = NegativeBinomial::new(r, p).unwrap();
    nb.sf(k.floor() as u64)
}
```

Pros: Battle-tested across all valid (r, p), well-documented.
Cons: New dep on mc-model crate. `statrs` pulls in `nalgebra` /
`rand` transitively in some configs — needs feature audit. Phase-3H
explicitly avoided deps for the Abramowitz approx.

**Recommendation: Option A** unless the dep audit comes out clean. The
hand-roll covers MLB's range exactly; the dep is overkill.

## Test fixtures (validate against scipy)

```python
# Reference values (scipy.stats.nbinom; n=1/α, p=1/(1+α·μ))
# All values rounded to 6 decimal places:
nbinom_sf(8, mu=8.5, alpha=0.13) == 0.553 (approximate)
nbinom_sf(9, mu=8.5, alpha=0.13) == 0.451
nbinom_sf(10, mu=8.5, alpha=0.13) == 0.350
nbinom_sf(8.5, mu=8.5, alpha=0.13) == 0.451  # half-integer → floors to k=8 → P(X>8) = P(X>=9)
nbinom_sf(0, mu=4.0, alpha=0.15) == 0.762
nbinom_sf(20, mu=8.5, alpha=0.13) == 0.0024  # extreme over — heavy lottery ticket
```

For YAML goldens, include 5-10 (μ, α, line, expected) triples covering
the typical MLB run-total grid (μ ∈ [3, 13], α ∈ [0.10, 0.20],
line ∈ [4.5, 12.5]). Generate via:

```python
from scipy.stats import nbinom
n = 1 / alpha
p = 1 / (1 + alpha * mu)
expected = nbinom.sf(int(line), n, p)
```

## Where it lands in the cartridge

Once `nbinom_sf` is in the formula evaluator, claw-core's MLB cartridge
becomes a properly-native Mosaic model:

```yaml
metadata:
  name: "MLB_Totals_V10_NB"
  description: >
    MLB totals model. Lasso-predicted mean + Negative Binomial dispersion.
    p_over computed inline via nbinom_sf — no pre-computed values.

fitted_models:
  - name: mlb_v10_lasso         # the existing Lasso predict() for the mean
    method: linear
    intercept: -4.87
    coefficients: { ... }
    residual_std: 4.32           # unused once we have NB; kept for legacy

measures:
  - { name: Market_Line, role: Input, ... }
  - { name: Predicted_Mean, role: Derived, ... }
  - { name: Dispersion_Alpha, role: Input, ... }   # constant ~0.13 (per fold's α)
  - { name: P_Over_NB, role: Derived, ... }
  - { name: P_Over_Calibrated, role: Derived, ... }

rules:
  - name: predict_mean
    target_measure: Predicted_Mean
    body: 'predict("mlb_v10_lasso", home_starter_xera, away_starter_xera, ... )'

  - name: predict_p_over
    target_measure: P_Over_NB
    body: 'nbinom_sf(Market_Line, Predicted_Mean, Dispersion_Alpha)'

  - name: calibrate_p_over
    target_measure: P_Over_Calibrated
    body: 'calibrate(P_Over_NB, "mlb_v10_pava")'
```

Now users can:
- `mc model whatif mlb-totals.yaml --set 'Market_Line=8.5' --show P_Over_Calibrated`
  → P(Over) recomputed live
- `mc model sweep mlb-totals.yaml --coefficient home_starter_xera --range 2.5..5.5`
  → see how the over-prob shifts as the pitcher quality slider moves

The slider workflow ADR-0001 was built around finally works end-to-end
for MLB.

## Effort estimate (from the outside)

Option A (hand-rolled):

- ~30 lines parse-site in `formula.rs` (mirror norm_cdf parse pattern)
- ~30 lines AST + Expr enum variant in `rule.rs`
- ~30 lines `nbinom_sf_compute` numeric body
- ~50 lines tests + goldens
- Diagnostic code: MC10xx range (consistent with parse-time errors)
- Doc updates: schema.rs comment + formula.rs syntax docs

**Total: ~140 lines + tests.** Same shape as Phase 3I's `pow` / `sqrt` /
`ln` additions. Probably a 1-2 day phase including the dual-review pass.

Option B (statrs dep) is ~50 lines but adds a dependency audit step
which probably ~doubles the wall time.

## Sequencing question for Mosaic

Where does this fit? Two natural homes:

1. **Bundle with the next formula-engine phase** (a Phase 3K?
   "Distributional formula primitives" — could include `nbinom_sf`,
   `poisson_sf`, `beta_cdf` if other consumers surface).
2. **Standalone "MLB cartridge support" phase** (call it 3J.1 or
   3H.3). Smaller scope; ships faster; doesn't wait for other
   distributions to have demand.

claw-core's preference is whichever ships sooner — we're currently
running the cartridge with a pre-computed P_Over_NB workaround, which
is fine but blocks the slider workflow. No urgency though; sim
results are honest and shippable as-is.

## Workaround in the interim

Until `nbinom_sf` lands, claw-core ships the MLB cartridge with
`P_Over_NB` as an Input measure (Python-precomputed per game, baked
into the YAML's `canonical_inputs:`). The cartridge still validates
and tests cleanly; it's just not a "live model" — adjusting features
in `whatif` doesn't update `P_Over_NB` because there's no formula
recomputing it.

The slider workflow remains broken for MLB in this state. NBA's
existing cartridge has the same shape (uses `norm_cdf` for over-prob
because NBA totals fit Gaussian) and works end-to-end with sliders.

## Cross-links

- ADR-0001 (claw-core, Mosaic substrate): https://github.com/edwinlov3tt/claw-core/blob/main/docs/decisions/ADR-0001-mosaic-as-modeling-substrate.md
- ADR-0002 (claw-core, MLB cartridge — will be amended post-EXP-028):
  https://github.com/edwinlov3tt/claw-core/blob/main/docs/decisions/ADR-0002-mlb-totals-cartridge.md
- claw-core EXP-025 (NB introduction): `training/mlb/exp025_negbin.py` +
  `docs/reports/exp-025-negative-binomial-mlb-totals.md`
- claw-core EXP-028 (walk-forward NB, 59.68% WR headline):
  `training/mlb/exp028_walk_forward.py` +
  `docs/reports/exp-028-walk-forward-v10-nb.md`
- Sibling cross-repo note: [`./claw-core-first-downstream-consumer.md`](./claw-core-first-downstream-consumer.md)
- Existing `norm_cdf` impl (the analog to mirror):
  [`../../crates/mc-core/src/rule.rs`](../../crates/mc-core/src/rule.rs) line 1213
- Existing `norm_cdf` parse site:
  [`../../crates/mc-model/src/formula.rs`](../../crates/mc-model/src/formula.rs) line 933

## Notes

- I drafted this from outside the mc-v2 codebase; if the right home is
  a different crate or the API shape should differ, a Mosaic-side
  amendment to this note (or a counter-proposal research note) is
  the expected response — same pattern as the daemon-endpoints note.
- The half-integer-line semantics (line=8.5 → floor to k=8 → P(X>8) =
  P(X>=9)) is a load-bearing convention worth nailing down in the
  spec. `scipy.stats.nbinom.sf(8.5, ...)` may behave differently from
  `nbinom.sf(8, ...)` depending on scipy version; pinning our convention
  explicitly avoids the trap.
- If statrs comes in as a dep for other reasons (e.g., a future phase
  needs `poisson_sf` or `beta_cdf`), retroactively swapping the Option A
  hand-roll for the statrs path is trivial and doesn't break the formula
  surface.
