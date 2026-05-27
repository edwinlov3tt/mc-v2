# ADR-0031: `nbinom_sf()` Negative Binomial Survival Function

**Status:** Proposed
**Date:** 2026-05-27
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

*(None as of authoring. Project owner review pending.)*
