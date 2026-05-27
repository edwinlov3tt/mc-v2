# ADR-0031: `nbinom_sf()` Negative Binomial Survival Function

**Status:** Accepted (with 7 acceptance amendments — see bottom; binding for implementation)
**Date:** 2026-05-27
**Accepted:** 2026-05-27 (project owner approved after external review pass)
**Last amended:** 2026-05-27 — review feedback folded in; fixture values corrected against scipy 1.13.1
**Deciders:** project owner
**Phase:** 3L (distributional formula primitives — first addition: `nbinom_sf`)
**Crate(s) touched:** `mc-model` (parser) + `mc-core` (evaluator); no other crates
**Prerequisite reading:**
- [Research note](../research-notes/nbinom-sf-formula-function.md) — math, parametrization, reference values
- [`claw-core-first-downstream-consumer.md`](../research-notes/claw-core-first-downstream-consumer.md) — sibling daemon endpoint request

---

## Context

claw-core's MLB cartridge ships in degenerate form today: `P_Over_NB` is pre-computed in Python at training time and baked in as an `Input` measure rather than computed by Mosaic at evaluation time. This works for production betting math but breaks the slider/sweep workflow ADR-0001 was built around — adjusting a feature in `whatif` doesn't update the over-probability because there's no formula to recompute it.

claw-core EXP-028 confirmed the V1.0+NB cartridge clears the betting gate (59.68% WR, 1,508 bets across 2023-2025 walk-forward, Wilson lower bound 57.18%). The model itself is shippable. What's missing is the Mosaic-side function that lets the cartridge express:

```yaml
body: 'nbinom_sf(Market_Line, Predicted_Mean, Dispersion_Alpha)'
```

as a derived rule.

Phase 3H added `norm_cdf` for Gaussian-tailed continuous models (NBA totals fit Gaussian; this is what the NBA cartridge uses). MLB is structurally different — discrete, right-skewed, overdispersed — and needs the Negative Binomial analog. This is the next natural addition in the same family.

The research note (already filed at commit `511ad61` in `docs/research-notes/`) does the math, parametrization conversions, and reference-value derivation. This ADR makes the decisions binding.

---

## Decisions

### Decision 1: API signature — mean-dispersion parametrization

```
nbinom_sf(k, mu, alpha) -> f64
```

| Arg | Type | Meaning |
|---|---|---|
| `k` | f64 | Threshold the count must exceed (the bet line) |
| `mu` | f64 | NB mean (predicted count) |
| `alpha` | f64 | NB dispersion; `Var(X) = mu + alpha * mu²` |

Returns `P(X > k | X ~ NB(mu, alpha))`, a probability in `[0, 1]`.

**Why mean-dispersion form (μ, α) and not the textbook (r, p)?**
- Matches `statsmodels.discrete.discrete_model.NegativeBinomial` (what claw-core uses)
- The mean is the directly interpretable quantity in regression workflows — authors think in "predicted runs," not "number of failures parameter"
- Conversion to (r, p) for the actual computation is one line: `r = 1/α`, `p = 1/(1 + α·μ)`

### Decision 2: Implementation — hand-rolled direct PMF summation (Option A)

Mirror the Phase 3H precedent: `norm_cdf` uses a hand-rolled Abramowitz approximation specifically to avoid pulling in a stats dependency. `nbinom_sf` follows the same convention.

```rust
fn nbinom_sf_compute(k: f64, mu: f64, alpha: f64) -> f64 {
    let k_int = k.floor() as i64;
    if k_int < 0 { return 1.0; }              // X ≥ 0 always, so P(X > negative) = 1
    let r = 1.0 / alpha;
    let p = 1.0 / (1.0 + alpha * mu);
    let mut cdf = p.powf(r);                   // pmf(0) = p^r
    let mut pmf_i = cdf;
    for i in 1..=k_int {
        // pmf(i) / pmf(i-1) = (r + i - 1) / i * (1 - p)  — ratio recursion for stability
        pmf_i *= (r + (i as f64) - 1.0) / (i as f64) * (1.0 - p);
        cdf += pmf_i;
    }
    (1.0 - cdf).clamp(0.0, 1.0)
}
```

~25-line body. Zero dependencies. Numerically exact for the validity range in Decision 7.

**Why not `statrs`?**
- Mirrors Phase 3H discipline (no stats dep in the kernel)
- `statrs` pulls in `nalgebra` / `rand` transitively; dependency audit doubles wall-clock time
- The hand-roll covers MLB's range exactly (`k ≤ ~25`, `α ∈ [0.05, 1.0]`)
- If a future phase needs `poisson_sf` / `beta_cdf` / other distributions and pulls in `statrs` legitimately, retroactively swapping the hand-roll for the dep call is trivial

### Decision 3: Companion function — ship `nbinom_cdf`, skip `nbinom_pmf`

`nbinom_cdf(k, mu, alpha) = 1 - nbinom_sf(k, mu, alpha)`. Trivial complement. Shipped together for symmetry — many authors will want `P(X ≤ line)` for under-probability framing.

`nbinom_pmf(k, mu, alpha)` is **not** included. Only useful for debugging individual probability mass values; authors who need it can call `nbinom_cdf(k, ...) - nbinom_cdf(k-1, ...)`. Skipping keeps the public surface minimal.

### Decision 4: Edge case handling

| Input | Behavior | Diagnostic |
|---|---|---|
| `k < 0` | Return `1.0` (X is non-negative; threshold below zero is always exceeded) | None |
| `mu <= 0` | Runtime evaluation error | **MC2058** |
| `alpha <= 0` | Runtime evaluation error | **MC2058** |
| `k = NaN` or `mu = NaN` or `alpha = NaN` | Return `NaN` | None — propagation rules from Phase 3 apply |
| Non-integer `k` | Floor to integer per Decision 6 | None |
| `k` very large (>10000) | Returns approximately 0; no overflow because PMF goes to 0 | None |

**Why error on μ ≤ 0 / α ≤ 0 rather than NaN?** The mean and dispersion are domain-fundamental — μ=0 means "predict zero count" (degenerate); α=0 collapses to Poisson (separate distribution). Both indicate a programming error upstream, not a propagable missing value. Production never sends these but defensive depth matters.

### Decision 5: Diagnostic codes

| Code | Phase | Fires when |
|---|---|---|
| **MC1018** | Parse | `nbinom_sf` called with wrong arg count (expects exactly 3) |
| **MC1019** | Parse | `nbinom_cdf` called with wrong arg count |
| **MC2058** | Runtime | `nbinom_sf`/`nbinom_cdf` invoked with `mu <= 0` or `alpha <= 0` |

Pre-flight sweep: verify MC1018, MC1019, MC2058 are unallocated against current `main` before implementation. The next free slot in the parse range (MC10xx) should align — MC1015 was used in Phase 3K, MC1016/MC1017 are reserved for the cardinality bands. MC1018/MC1019 should be free. If not, shift to the next unused codes.

### Decision 6: Half-integer line semantics

This is load-bearing for MLB bet lines (8.5, 9.5, etc.) and worth pinning explicitly to avoid scipy-version drift:

```
nbinom_sf(8.5, mu, alpha) ≡ nbinom_sf(8, mu, alpha) = P(X > 8) = P(X ≥ 9)
```

Formal rule: `k` is floored to the nearest integer before computation. `P(X > floor(k))` is what gets returned. For sportsbook lines:
- Line = 8.5 → `floor(8.5) = 8` → `P(X > 8) = P(X ≥ 9)` = "probability the over hits"
- Line = 9.0 (integer push line) → `floor(9.0) = 9` → `P(X > 9) = P(X ≥ 10)` = "probability over, NOT push"

The "push probability" at integer lines is `nbinom_cdf(9) - nbinom_cdf(8) = P(X = 9)` — derivable as `nbinom_cdf(9, ...) - nbinom_cdf(8, ...)` without needing `nbinom_pmf` exposed.

This convention is documented in:
- Doc comment on the `nbinom_sf` parser handler
- A test fixture (`nbinom_sf(8.5, 8.5, 0.13) == nbinom_sf(8, 8.5, 0.13)`)
- A README note in `crates/mc-model/src/formula.rs` near the function dispatch

### Decision 7: Validity range and documentation

The hand-roll is numerically exact for:
- `k ∈ [0, ~50]`
- `α ∈ [0.05, 1.0]`
- `μ ∈ [0.5, 50]`

Outside this range the direct PMF sum can lose precision (PMF underflow at very large `k`; cumulative roundoff for `α` near 0). This range comfortably covers MLB run totals (`μ ≈ 4-13`, `α ≈ 0.10-0.20`, `k ≤ 25`).

If a future consumer needs `k > 50` or `α < 0.05`, the implementation can be extended in two ways:
1. Log-space PMF accumulation (numerically stable to `k ~ 1000`)
2. Incomplete-beta-function path (`P(X > k) = I_{1-p}(k+1, r)`)

Out of scope for this phase. Document the validity range in the doc comment so consumers know when they're outside it.

---

## Implementation plan

Modeled after Phase 3H's `norm_cdf` implementation. Each step references the precedent site so the implementer can mirror exactly.

### Step 1: Parser — add `nbinom_sf` / `nbinom_cdf` tokens

**File:** `crates/mc-model/src/formula.rs` (around line 933, where `norm_cdf` is parsed)

```rust
"nbinom_sf" => {
    let args = self.parse_arg_list()?;
    self.expect_close_paren("nbinom_sf")?;
    if args.len() != 3 {
        return Err(FormulaError::wrong_arg_count(
            call_start,
            format!("nbinom_sf expects exactly 3 arguments (k, mu, alpha), got {}", args.len()),
        ));
    }
    let [k, mu, alpha] = take3(args);
    Ok(ParsedRuleBody::NbinomSf(ParsedNbinomBody {
        k: Box::new(k),
        mu: Box::new(mu),
        alpha: Box::new(alpha),
    }))
}
"nbinom_cdf" => { /* same shape, returns ParsedRuleBody::NbinomCdf */ }
```

The MC1018 / MC1019 diagnostic message text should match the existing wrong-arg-count style verbatim.

### Step 2: AST — add the body variants

**File:** `crates/mc-core/src/rule.rs` (around lines 133-134 and 946-955)

```rust
// ParsedRuleBody enum, alongside NormCdf:
NbinomSf(ParsedNbinomBody),
NbinomCdf(ParsedNbinomBody),

// New struct, mirrors ParsedNormCdfBody:
#[derive(Clone, Debug)]
pub struct ParsedNbinomBody {
    pub k: Box<ParsedExpr>,
    pub mu: Box<ParsedExpr>,
    pub alpha: Box<ParsedExpr>,
}
```

### Step 3: Evaluator — implement `nbinom_sf_compute`

**File:** `crates/mc-core/src/rule.rs` (near `norm_cdf_compute` at line 1213)

Body per Decision 2. Add a `nbinom_cdf_compute` that calls `nbinom_sf_compute` and returns `1.0 - result`.

### Step 4: Wire eval through the rule body dispatch

In whichever function dispatches `ParsedRuleBody` variants to compute functions, add the two new arms. Mirror the `NormCdf` arm exactly. MC2058 fires when `mu ≤ 0` or `alpha ≤ 0` (return `Result::Err` with the appropriate `EngineError::InvalidFitInput` variant or equivalent — match what `norm_cdf` does for its bad-input case).

### Step 5: Tests

**File:** `crates/mc-core/tests/nbinom_sf.rs` (new) — at least the following:

```rust
#[test] fn t_nbinom_sf_basic_mlb_mid_line() {
    // mu=8.5, alpha=0.13, line=8.5 (half-integer floor to 8)
    let r = nbinom_sf_compute(8.5, 8.5, 0.13);
    assert!((r - 0.4513).abs() < 0.001);
}

#[test] fn t_nbinom_sf_low_line() {
    // mu=4.0, alpha=0.15, line=0 → very high over-probability
    let r = nbinom_sf_compute(0.0, 4.0, 0.15);
    assert!((r - 0.7625).abs() < 0.001);
}

#[test] fn t_nbinom_sf_high_line_lottery() {
    // mu=8.5, alpha=0.13, line=20 → extreme over (heavy lottery)
    let r = nbinom_sf_compute(20.0, 8.5, 0.13);
    assert!(r > 0.001 && r < 0.005);
}

#[test] fn t_nbinom_sf_negative_k_returns_one() {
    // P(X > -1 | X ≥ 0) = 1
    assert_eq!(nbinom_sf_compute(-1.0, 8.5, 0.13), 1.0);
}

#[test] fn t_nbinom_sf_half_integer_equals_integer_floor() {
    // line=8.5 should equal line=8
    let a = nbinom_sf_compute(8.5, 8.5, 0.13);
    let b = nbinom_sf_compute(8.0, 8.5, 0.13);
    assert!((a - b).abs() < 1e-9);
}

#[test] fn t_nbinom_cdf_complements_sf() {
    let sf = nbinom_sf_compute(8.0, 8.5, 0.13);
    let cdf = nbinom_cdf_compute(8.0, 8.5, 0.13);
    assert!((sf + cdf - 1.0).abs() < 1e-9);
}

#[test] fn t_nbinom_sf_alpha_zero_errors() {
    // Caller-level check; verify the dispatch returns the appropriate error.
}

#[test] fn t_nbinom_sf_mu_zero_errors() { /* same */ }
```

Plus 4-5 more covering: integer line, low μ extreme, high α (overdispersion), low α (near-Poisson), scipy-verified reference values.

Use `scipy.stats.nbinom(n=1/α, p=1/(1+α·μ)).sf(floor(k))` to generate reference values for every test fixture. Pin to scipy 1.11+ for stable behavior (older versions had `sf(half_integer)` quirks per the research note).

### Step 6: Schema-side validation (mc-model lint)

Optional but recommended: add a lint rule in `crates/mc-model/src/lint.rs` that warns when `nbinom_sf` is called with constant `μ ≤ 0` or `α ≤ 0` at parse time. Mirrors existing parse-time sanity checks. Diagnostic code: **MC3013** (next free in the lint range — verify).

If this isn't trivial to add, skip — runtime MC2058 covers the error case.

### Step 7: Integration test on MLB cartridge

Once the function is implemented, the existing MLB cartridge at `examples/sports-betting/mlb-totals.yaml` can be updated:

1. Remove the pre-computed `P_Over_NB` from `canonical_inputs` (currently 174,549 rows including baked NB probs)
2. Change the `P_Over_NB` measure from `Input` to `Derived`
3. Add a rule: `nbinom_sf(sharp_close_line, Predicted_Total, Dispersion_Alpha)`
4. Add `Dispersion_Alpha` as a constant Input (current value: 0.1245 from claw-core EXP-025)
5. `mc model validate examples/sports-betting/mlb-totals.yaml` must pass
6. `mc model test` goldens must still pass (re-derive expected values via scipy and update if they shift within tolerance)

This cartridge migration is **not** a hard gate for the formula function shipping — claw-core's instance can do it as a follow-up. But it's the proof point that the function actually unblocks the slider workflow.

---

## Acceptance criteria

1. `nbinom_sf(k, mu, alpha)` parses as a formula function
2. `nbinom_cdf(k, mu, alpha)` parses as a formula function
3. Both compute correct values against scipy reference for ≥10 fixture cases spanning the MLB validity range (k ∈ [0, 20], μ ∈ [3, 13], α ∈ [0.05, 0.30])
4. Half-integer `k` floors to integer per Decision 6 — explicit test fixture
5. MC1018 fires on `nbinom_sf` wrong arg count; MC1019 on `nbinom_cdf` wrong arg count
6. MC2058 fires at runtime when `mu ≤ 0` or `alpha ≤ 0`
7. `k < 0` returns 1.0 without error
8. Validity-range documentation in the doc comment of the public function
9. No new external dependencies in `mc-model` or `mc-core` Cargo.toml
10. All existing tests pass unchanged (NBA cartridge, Acme, all phase tests)
11. `cargo test --workspace` passes
12. `cargo clippy --all-targets --workspace -- -D warnings` passes
13. `cargo fmt --check --all` clean
14. JSON schema regenerated (per Phase 3K — the new `ParsedRuleBody` variants need `#[derive(JsonSchema)]` and the committed `docs/specs/mosaic-model-schema.json` updated; CI drift check passes)

**Soft acceptance (cartridge proof point, separate commit, claw-core or Mosaic):**
- MLB cartridge migrated from baked `P_Over_NB` (Input) to formula-derived `P_Over_NB` (Derived via `nbinom_sf`)
- `mc model whatif --coord '...Game=X,Measure=sharp_close_line' --value 9.5` returns updated `P_Over_NB`
- The slider workflow ADR-0001 was built around finally works end-to-end for MLB

---

## Alternatives considered

### Alt 1: Pull in `statrs` for the NB distribution

Considered. `statrs::distribution::NegativeBinomial::new(r, p).unwrap().sf(k)` is a one-liner.

**Rejected because:**
- Phase 3H precedent: `norm_cdf` is hand-rolled to avoid stats deps in the kernel
- `statrs` pulls in `nalgebra` and `rand` transitively in some feature configurations — non-trivial audit
- The hand-roll covers MLB's full operating range exactly
- Retroactively swapping in `statrs` if another phase needs `poisson_sf` / `beta_cdf` is mechanical and doesn't break the formula surface
- The dependency audit doubles wall-clock time for a function that doesn't benefit from the audit

If a future phase pulls in `statrs` for legitimate reasons (e.g., a distributional primitives expansion that needs Beta, Gamma, Chi-squared), the hand-roll here can be replaced in a single commit without behavior change.

### Alt 2: Use `(r, p)` parametrization in the API

Considered. `nbinom_sf(k, r, p)` matches scipy directly.

**Rejected because:**
- The model authoring layer thinks in terms of mean and dispersion — that's what `statsmodels` returns, what Lasso predicts, and what authors reason about
- Forcing authors to compute `r = 1/α` and `p = 1/(1 + α·μ)` inline in YAML would be both error-prone and visually noisy
- The conversion is one line internally; do it once in the eval site, not in every cartridge that uses the function

### Alt 3: Include `nbinom_pmf` in the public surface

Considered. The research note offered it as "optional but useful."

**Rejected because:**
- Only useful for debugging individual probability masses, which is rare in production cartridges
- Derivable as `nbinom_cdf(k) - nbinom_cdf(k-1)` if a consumer ever needs it
- Keeps the public surface minimal — Phase 3 discipline says don't ship API beyond demonstrated demand

If a future cartridge legitimately needs `nbinom_pmf`, add it then. The marginal cost of adding it after the fact is identical to adding it now.

### Alt 4: Defer until the daemon endpoints (sibling note) ship

Considered. The sibling research note (`claw-core-first-downstream-consumer.md`) requests `/api/v1/whatif`, `/api/v1/sweep`, `/api/v1/reload` daemon endpoints. Without them, even a perfectly-implemented `nbinom_sf` doesn't unlock the over-HTTPS slider workflow because the Worker can't reach Mosaic.

**Rejected — but with explicit sequencing.** The daemon endpoints are higher priority and should ship first. But:
- `nbinom_sf` is independently useful: `mc model whatif` on the local CLI already works for cartridge authors and analyst workflows
- The two phases are independent; doing them serially doubles total wall-clock with no shared dependency
- claw-core can use the local-CLI slider for analyst workflows while the daemon HTTP path catches up

**Suggested ordering:**
1. Daemon `/whatif` + `/sweep` HTTP endpoints (Phase 8.2 or similar — separate ADR; higher priority)
2. `nbinom_sf` (this ADR; smaller scope, can land in parallel)

Both ship independently. Doing this ADR first does not block (1); doing (1) first does not block this.

---

## Out of scope

- Other discrete distributions (`poisson_sf`, `binomial_sf`, `hypergeometric_sf`). Add when demand surfaces.
- Other NB parametrizations beyond mean-dispersion. The (r, p) form is computable from (μ, α) inside the function; no need to expose it.
- Log-space PMF accumulation for `k > 50`. Out-of-validity-range work; not needed for MLB. Add when a consumer hits the precision floor.
- Incomplete-beta-function path. Same reasoning as above.
- NB regression *fitting* in Mosaic (training, not evaluation). Fitting stays in Python (statsmodels / sklearn); Mosaic evaluates fitted models per ADR-0025 Decision 1.4.
- `nbinom_pmf` exposure (rejected in Alt 3).
- Daemon HTTP endpoints (`/whatif`, `/sweep`, `/reload`). Separate sibling research note; separate phase.
- Cartridge migration of `P_Over_NB` from Input to Derived. claw-core-side action; not a Mosaic gate.

---

## Cross-links

- **Research note:** [`../research-notes/nbinom-sf-formula-function.md`](../research-notes/nbinom-sf-formula-function.md)
- **Sibling note:** [`../research-notes/claw-core-first-downstream-consumer.md`](../research-notes/claw-core-first-downstream-consumer.md) (daemon endpoints — higher priority, separate ADR)
- **Phase 3H precedent:** [ADR-0018](./0018-phase-3h-2-fitted-model-adstock-saturation.md) and predecessors — `norm_cdf` hand-rolled to avoid stats deps
- **Phase 3K (recent precedent):** [ADR-0030](./0030-model-authoring-ergonomics.md) — model authoring ergonomics including JSON schema generation; the new `ParsedRuleBody` variants need `#[derive(JsonSchema)]` per Phase 3K
- **ADR-0001 (claw-core):** Mosaic substrate vision — the slider workflow this ADR finally unlocks for MLB
- **claw-core EXP-025:** Negative Binomial introduction (the experiment that proved NB was the right distribution for MLB totals)
- **claw-core EXP-028:** Walk-forward NB validation (59.68% WR across 2023-2025; production-ready)
- **Existing `norm_cdf` impl:** `crates/mc-core/src/rule.rs:1213` — the analog to mirror
- **Existing `norm_cdf` parse site:** `crates/mc-model/src/formula.rs:933` — the parse pattern to mirror

---

## Notes

**Why now:** claw-core's MLB cartridge shipped (V1.0+NB clears the betting gate at 59.68% WR walk-forward). The next thing standing between "shipped" and "demoable end-to-end" is the slider workflow, and that needs this function. The daemon endpoints are the other half; once both ship, the ADR-0001 vision is real for MLB.

**Effort and shape:** ~140 lines + tests. Two days of work, one of which is the dual-review pass on the numerical body. Same shape as Phase 3I's `pow` / `sqrt` / `ln` additions — surgical, well-precedented, low-risk.

**Why this is the right next phase even though daemon endpoints are higher priority:** Two engineers can work in parallel — one on daemon HTTP, one on `nbinom_sf`. Neither blocks the other. Sequencing them serially wastes wall-clock for no architectural benefit. The two phases compose cleanly when both land.

**Cartridge migration is a follow-up, not a gate:** Shipping `nbinom_sf` doesn't require updating the MLB cartridge. claw-core can run the cartridge in its current baked-`P_Over_NB` form indefinitely. The cartridge migration happens when claw-core has bandwidth — likely as part of the V1.1 cycle when features get added and the cartridge is being touched anyway.

**Phase numbering:** 3L follows 3K naturally (model layer additions). The research note suggested "Phase 3H.3" as an alternative since this is conceptually a Phase 3H follow-up (fitted-model-adjacent distributional primitive). Going with 3L because Phase 3H.2 explicitly "closed the formula-engine deferred queue" per ADR-0018 — reopening 3H.x would muddy that boundary. 3L is the demand-driven next addition in the same family, parallel to how 3K was demand-driven model authoring ergonomics.

**On the half-integer convention:** scipy's behavior on `nbinom.sf(8.5, n, p)` is version-dependent — some versions floor implicitly, some return the same value as `sf(8.0)`, some raise. Pinning our convention explicitly (Decision 6) avoids the trap. Document it in test fixtures + doc comment + parser site. The convention matches "floor the line, ask P(X > floor)" which is what sportsbook math actually wants.

---

## Acceptance amendments

The amendments below are **binding** for implementation and override the body of this ADR where they conflict. They were filed 2026-05-27 after external review (GPT-5.1 with high-effort thinking) caught a critical fixture-value error and proposed semantic and process improvements. Each amendment was independently verified before adoption — see Amendment 1 for the math check.

### Amendment 1: Corrected reference fixtures (CRITICAL — blocks implementation)

**Problem.** The research note's reference values were wrong. They appear to conflate `P(X ≥ k)` with `P(X > k)` (likely scipy was passed `k-1` or a non-flooring branch). The ADR body's test fixture `nbinom_sf(8.5, 8.5, 0.13) == 0.4513` was also off — within tolerance of the correct value but not derived from scipy.

**Verification.** Independently computed against `scipy.stats.nbinom.sf(floor(k), n=1/α, p=1/(1+α·μ))` using scipy 1.13.1. Both the research note's values and the ADR body's test values disagree with scipy. The corrected values below ARE scipy-authoritative.

**Reference-generation script (pin this verbatim in the test suite — `crates/mc-core/tests/nbinom_sf_fixtures.py`):**

```python
# Run with: python3 nbinom_sf_fixtures.py
# Requires: scipy >= 1.11 (pinned for stable nbinom.sf behavior on half-integers)
from scipy.stats import nbinom
import math

def sf(k, mu, alpha):
    n = 1.0 / alpha
    p = 1.0 / (1.0 + alpha * mu)
    return float(nbinom.sf(math.floor(k), n, p))

def cdf(k, mu, alpha):
    n = 1.0 / alpha
    p = 1.0 / (1.0 + alpha * mu)
    return float(nbinom.cdf(math.floor(k), n, p))
```

**Acceptance fixture grid (scipy 1.13.1, 2026-05-27 — paste into test file verbatim):**

| k     | mu   | alpha | nbinom_sf       | nbinom_cdf      | Note |
|-------|------|-------|-----------------|-----------------|------|
|  8.0  |  8.5 | 0.13  | 0.449201940     | 0.550798060     | MLB typical line, integer |
|  8.5  |  8.5 | 0.13  | 0.449201940     | 0.550798060     | Half-integer — equals integer k=8 (floor convention test) |
|  9.0  |  8.5 | 0.13  | 0.360882026     | 0.639117974     | MLB push-line behavior |
| 10.0  |  8.5 | 0.13  | 0.283491963     | 0.716508037     | High line |
| 20.0  |  8.5 | 0.13  | 0.010324246     | 0.989675754     | Extreme over (lottery) |
|  0.0  |  4.0 | 0.15  | 0.956428740     | 0.043571260     | Low-mu, k=0 (almost always over) |
|  4.0  |  4.0 | 0.15  | 0.367553663     | 0.632446337     | At-the-line low-scoring matchup |
|  5.5  |  4.0 | 0.15  | 0.244569812     | 0.755430188     | Half-integer over, low-scoring |
|  7.5  | 12.0 | 0.10  | 0.806240752     | 0.193759248     | Half-integer under, high-scoring |
| 12.5  | 12.0 | 0.10  | 0.418033119     | 0.581966881     | Half-integer over, high-scoring |
| -1.0  |  8.5 | 0.13  | 1.000000000     | 0.000000000     | Negative k → always over (X ≥ 0 invariant) |
|  8.0  |  8.5 | 0.05  | 0.464321488     | 0.535678512     | Near-Poisson (small α) |
|  8.0  |  8.5 | 0.30  | 0.425855506     | 0.574144494     | High dispersion |

**Tolerance:** `1e-6` absolute, matching the precision regime of the direct-PMF-summation hand-roll. (The kernel-wide `1e-9` epsilon is reserved for exact float comparisons; PMF accumulation has wider numerical bounds.)

**Acceptance gate:** Implementation MUST regenerate this table via the script above against the same scipy version, paste the output into a doc comment in the test file, and assert against the pasted values. If a future scipy version changes any value beyond `1e-6`, the implementer must surface the change in chat before silently adopting the new values — it indicates either a scipy bugfix (adopt it, update the table, note the version bump) or a regression (don't adopt it, pin to older scipy).

### Amendment 2: Invalid-domain inputs return Null, not runtime error or NaN

**Problem.** Decision 4 specified MC2058 runtime errors for `mu ≤ 0` or `alpha ≤ 0`, and NaN propagation for NaN inputs. This diverges from Mosaic formula-engine convention (Phase 3H `norm_cdf` returns Null for invalid sigma rather than erroring) and is dangerous for betting math — a NaN probability silently corrupts edge/Kelly/calibration calculations downstream.

**Amendment.** Replace Decision 4's edge-case table with:

| Input | Behavior | Diagnostic |
|---|---|---|
| `k < 0` | Return `1.0` (X non-negative; threshold below zero always exceeded) | None |
| Any arg is `Null` | Return `Null` (Mosaic Null-propagation default) | None |
| `mu <= 0` | Return `Null` | None |
| `alpha <= 0` | Return `Null` | None |
| Any arg is `NaN` | Return `Null` | None — never let NaN flow through |
| Non-integer `k` (finite, ≥ 0) | Floor to integer per Amendment 6 | None |
| `k` very large (>10000) | Returns approximately 0; no overflow | None |

**Rationale.** Returning Null:
- Matches Mosaic's null-propagation discipline (CLAUDE.md §2.5 — Null is the distinguished "no value" marker)
- Prevents NaN poisoning downstream Kelly/edge math
- Makes invalid inputs visible without crashing the request (the consumer can see Null in the response and react)
- Aligns with Phase 3H `norm_cdf` precedent — sigma ≤ 0 returns Null, not error
- A runtime error on the daemon surfaces as a 500 response; a Null surfaces as a calculable value the consumer can guard against. For a probability function, Null is always preferable.

**MC2058 retired from this ADR.** The diagnostic code stays unallocated. If a future phase needs a runtime diagnostic for nbinom_sf (e.g., a deliberate strict-mode flag), allocate it then.

**Optional companion (Amendment 2a):** A *lint-time* warning (not runtime) when a formula contains literal constants known to be invalid — e.g., `nbinom_sf(k, mu=-1, alpha=0.13)` with literal `-1` in the YAML. This catches authoring mistakes at validate-time without affecting runtime behavior. Diagnostic code reserved as **MC3013** (verify via Amendment 3 preflight before allocating). Optional — skip if it adds friction to implementation.

### Amendment 3: Diagnostic codes are placeholders until preflight sweep

**Problem.** Decision 5 hard-coded MC1018/MC1019/MC2058 (and Amendment 2a above proposes MC3013) before verifying these codes are unallocated against current `main`. Phase 3I had collision issues; treat this as a process discipline gate.

**Amendment.** The diagnostic codes in this ADR are **semantic names + reserved slots**, not final assignments. Before implementation:

```bash
# Preflight: run from repo root
grep -RE "MC1018|MC1019|MC3013" docs/ crates/ 2>/dev/null
```

For each code that returns a match, shift to the next unallocated code in the same range:
- Parse range: MC10xx — next free after the highest existing MC10xx assignment
- Lint range: MC30xx — next free after the highest existing MC30xx assignment

**Update locations after preflight:** Decision 5 table, Implementation Step 1 (parser handler error messages), Implementation Step 5 (tests), and any doc comments referencing the codes. The semantic names (`NBINOM_SF_WRONG_ARG_COUNT`, `NBINOM_CDF_WRONG_ARG_COUNT`, `NBINOM_SF_INVALID_LITERAL_CONSTANT`) stay stable across the rename — only the numeric codes shift.

MC2058 is **removed** entirely per Amendment 2 — no runtime diagnostic.

### Amendment 4: "Numerically exact" → "validated against scipy within tolerance"

**Problem.** Decision 7 claimed the hand-roll is "numerically exact for `k ∈ [0, ~50]`, `α ∈ [0.05, 1.0]`, `μ ∈ [0.5, 50]`." Floating-point recurrence is not exact — it accumulates roundoff. The ADR itself acknowledges precision loss outside the range, which contradicts the "exact" framing.

**Amendment.** Decision 7's language updates to:

> The hand-roll is **validated against `scipy.stats.nbinom.sf` within `1e-6` absolute tolerance** across the declared MLB operating range (`k ∈ [0, 50]`, `α ∈ [0.05, 1.0]`, `μ ∈ [0.5, 50]`).
>
> Outside this range the direct PMF sum can lose precision (PMF underflow at very large `k`; cumulative roundoff for `α` near 0 or `μ` very large). For consumers operating outside this range, see the future-work options in Decision 7 (log-space accumulation, incomplete-beta path). This phase does NOT certify accuracy outside the declared range.

### Amendment 5: Shared helper — sf computed via cdf, not duplicated

**Problem.** Decision 2 shows a `nbinom_sf_compute` body and Decision 3 says `nbinom_cdf` is the trivial complement. The risk: two implementations drift over time as future maintenance touches one path but not the other.

**Amendment.** Implement `nbinom_cdf_compute` as the single source of truth; `nbinom_sf_compute` calls it:

```rust
/// Returns Some(cdf) for valid inputs, None for invalid (per Amendment 2).
fn nbinom_cdf_compute(k: f64, mu: f64, alpha: f64) -> Option<f64> {
    // Invalid-domain guards (Amendment 2)
    if k.is_nan() || mu.is_nan() || alpha.is_nan() { return None; }
    if mu <= 0.0 || alpha <= 0.0 { return None; }
    let k_int = k.floor() as i64;
    if k_int < 0 { return Some(0.0); }   // P(X <= negative) = 0; sf will be 1.0

    let r = 1.0 / alpha;
    let p = 1.0 / (1.0 + alpha * mu);

    let mut cdf = p.powf(r);                 // pmf(0) = p^r
    let mut pmf_i = cdf;
    for i in 1..=k_int {
        pmf_i *= (r + (i as f64) - 1.0) / (i as f64) * (1.0 - p);
        cdf += pmf_i;
    }
    Some(cdf.clamp(0.0, 1.0))
}

fn nbinom_sf_compute(k: f64, mu: f64, alpha: f64) -> Option<f64> {
    nbinom_cdf_compute(k, mu, alpha).map(|cdf| (1.0 - cdf).clamp(0.0, 1.0))
}
```

The dispatch sites convert `None` to `ScalarValue::Null` per Amendment 2; convert `Some(x)` to `ScalarValue::Number(x)`. Both functions share one PMF accumulation loop; cannot drift.

**Note on `k < 0` invariant:** `nbinom_cdf_compute(-1, ...)` returns `Some(0.0)`, so `nbinom_sf_compute(-1, ...)` returns `Some(1.0)`. This matches the ADR body's Decision 4 rule "negative k returns 1.0" — preserved through the shared helper.

### Amendment 6: Half-integer + integer-push-line tests explicit

**Problem.** Decision 6 documented the floor convention but the test list only had one fixture for it. The convention is load-bearing for sportsbook math and deserves more coverage.

**Amendment.** The test suite MUST include these specific named tests (in addition to the Amendment 1 fixture grid):

```rust
#[test]
fn t_nbinom_sf_half_integer_floor() {
    // Per ADR §6: nbinom_sf(8.5, ...) == nbinom_sf(8.0, ...) == nbinom_sf(8.999, ...)
    let a = nbinom_sf_compute(8.5,   8.5, 0.13).unwrap();
    let b = nbinom_sf_compute(8.0,   8.5, 0.13).unwrap();
    let c = nbinom_sf_compute(8.999, 8.5, 0.13).unwrap();
    assert!((a - b).abs() < 1e-9, "8.5 must floor to 8.0");
    assert!((b - c).abs() < 1e-9, "8.999 must floor to 8.0 (NOT round)");
}

#[test]
fn t_nbinom_sf_integer_push_line() {
    // Sportsbook push line: integer line. P(over) = P(X > 9) = sf(9).
    // P(push) = P(X = 9) = cdf(9) - cdf(8) = pmf(9).
    let sf_9   = nbinom_sf_compute(9.0,  8.5, 0.13).unwrap();
    let cdf_9  = nbinom_cdf_compute(9.0, 8.5, 0.13).unwrap();
    let cdf_8  = nbinom_cdf_compute(8.0, 8.5, 0.13).unwrap();
    let push   = cdf_9 - cdf_8;
    let under  = cdf_8;
    let over   = sf_9;
    // Sanity: P(over) + P(push) + P(under) = 1
    assert!((over + push + under - 1.0).abs() < 1e-9);
    // Expected push probability: pmf(9) under the locked convention
    assert!(push > 0.0 && push < 1.0);
}

#[test]
fn t_nbinom_sf_monotone_decreasing_in_k() {
    // sf(k+1) <= sf(k) for all k (sf is non-increasing)
    let mu = 8.5;
    let alpha = 0.13;
    let mut prev = 1.0;
    for k in 0..=20 {
        let s = nbinom_sf_compute(k as f64, mu, alpha).unwrap();
        assert!(s <= prev + 1e-12, "sf must be non-increasing in k: sf({k})={s}, sf({k}-1)={prev}");
        prev = s;
    }
}

#[test]
fn t_nbinom_sf_monotone_increasing_in_mu() {
    // For fixed k, higher mu generally increases sf (more mass at high counts)
    let k = 8.0;
    let alpha = 0.13;
    let sf_low  = nbinom_sf_compute(k, 4.0,  alpha).unwrap();
    let sf_mid  = nbinom_sf_compute(k, 8.0,  alpha).unwrap();
    let sf_high = nbinom_sf_compute(k, 12.0, alpha).unwrap();
    assert!(sf_low < sf_mid,  "sf({k}, mu=4) < sf({k}, mu=8): {sf_low} vs {sf_mid}");
    assert!(sf_mid < sf_high, "sf({k}, mu=8) < sf({k}, mu=12): {sf_mid} vs {sf_high}");
}

#[test]
fn t_nbinom_sf_cdf_complement() {
    // sf + cdf must equal 1.0 (within floating-point tolerance) — shared-helper invariant
    let cases = [(8.0, 8.5, 0.13), (10.0, 8.5, 0.13), (0.0, 4.0, 0.15), (20.0, 8.5, 0.13)];
    for (k, mu, alpha) in cases {
        let sf  = nbinom_sf_compute(k, mu, alpha).unwrap();
        let cdf = nbinom_cdf_compute(k, mu, alpha).unwrap();
        assert!((sf + cdf - 1.0).abs() < 1e-9, "sf+cdf={} at k={k}", sf + cdf);
    }
}

#[test]
fn t_nbinom_sf_invalid_returns_null() {
    // Per Amendment 2: invalid domain returns None (mapped to ScalarValue::Null at dispatch)
    assert!(nbinom_sf_compute(8.0,  0.0, 0.13).is_none(), "mu=0 must return None");
    assert!(nbinom_sf_compute(8.0, -1.0, 0.13).is_none(), "mu<0 must return None");
    assert!(nbinom_sf_compute(8.0,  8.5,  0.0).is_none(), "alpha=0 must return None");
    assert!(nbinom_sf_compute(8.0,  8.5, -0.5).is_none(), "alpha<0 must return None");
    assert!(nbinom_sf_compute(f64::NAN, 8.5, 0.13).is_none(), "NaN k must return None");
    assert!(nbinom_sf_compute(8.0, f64::NAN, 0.13).is_none(), "NaN mu must return None");
    assert!(nbinom_sf_compute(8.0, 8.5, f64::NAN).is_none(), "NaN alpha must return None");
}
```

These tests are in addition to the Amendment 1 fixture-grid tests (which assert specific scipy-reference values). The Amendment 6 tests verify *invariants* (monotonicity, complement, floor, push-decomposition, Null behavior) without depending on specific scipy values.

### Amendment 7: Acceptance criteria updates

Replace the original Decision-7-based acceptance criteria with:

1. `nbinom_sf(k, mu, alpha)` parses as a formula function (semantic name MC code per Amendment 3 preflight)
2. `nbinom_cdf(k, mu, alpha)` parses as a formula function (semantic name MC code per Amendment 3 preflight)
3. Both compute correct values against the Amendment 1 fixture grid within `1e-6` tolerance — 13 fixture cases pasted from the Python regeneration script into the test file
4. **Floor convention** test (Amendment 6 `t_nbinom_sf_half_integer_floor`)
5. **Push-line decomposition** test (Amendment 6 `t_nbinom_sf_integer_push_line`)
6. **Monotonicity in k** test (Amendment 6 `t_nbinom_sf_monotone_decreasing_in_k`)
7. **Monotonicity in μ** test (Amendment 6 `t_nbinom_sf_monotone_increasing_in_mu`)
8. **sf + cdf = 1** complement invariant test (Amendment 6 `t_nbinom_sf_cdf_complement`)
9. **Invalid-domain → Null** test (Amendment 6 `t_nbinom_sf_invalid_returns_null`) — `mu ≤ 0`, `alpha ≤ 0`, NaN any arg all return `None`/`ScalarValue::Null`
10. `k < 0` returns 1.0 without error (shared-helper preserves this through `cdf(-) = 0`)
11. Validity-range documentation reflects Amendment 4 language ("validated against scipy within tolerance" — NOT "numerically exact")
12. Shared-helper architecture per Amendment 5 — `nbinom_sf_compute` calls `nbinom_cdf_compute`; no PMF accumulation duplicated
13. Diagnostic codes assigned post-preflight per Amendment 3
14. No new external dependencies in `mc-model` or `mc-core` Cargo.toml
15. All existing tests pass unchanged (NBA cartridge, Acme, all phase tests)
16. `cargo test --workspace` passes
17. `cargo clippy --all-targets --workspace -- -D warnings` passes
18. `cargo fmt --check --all` clean
19. JSON schema regenerated per Phase 3K — new `ParsedRuleBody` variants `#[derive(JsonSchema)]`; committed `docs/specs/mosaic-model-schema.json` updated; CI drift check passes
20. Python reference-generation script committed at `crates/mc-core/tests/nbinom_sf_fixtures.py` with scipy version pin in a comment header

**Soft acceptance (cartridge proof point — claw-core or Mosaic, separate commit):**
- MLB cartridge migrated from baked `P_Over_NB` (Input) to formula-derived `P_Over_NB` (Derived via `nbinom_sf`)
- `mc model whatif --coord '...Game=X,Measure=sharp_close_line' --value 9.5` returns updated `P_Over_NB`
- Slider workflow ADR-0001 was built around works end-to-end for MLB

The acceptance criteria in the body of this ADR (Decision-7-section §"Acceptance criteria") are SUPERSEDED by this Amendment 7 list. Implementer reads this list, not the original.

---

*End of amendments. Body of ADR above is preserved for audit-trail purposes; amendments win on conflicts.*
