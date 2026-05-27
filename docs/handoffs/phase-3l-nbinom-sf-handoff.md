# Phase 3L Handoff — `nbinom_sf()` Negative Binomial Survival Function

**Status:** Accepted, ready to start
**Date:** 2026-05-27
**ADR:** [ADR-0031](../decisions/0031-nbinom-sf-formula-function.md) (Accepted with 7 acceptance amendments — read amendments BEFORE the body; amendments win on conflicts)
**Estimated effort:** 1–2 sessions (~140 LOC + ~80 lines of tests)
**Crates:** `mc-model` (parser) + `mc-core` (evaluator); no kernel-interface changes
**Branch:** `phase-3l/nbinom-sf`

---

## What this phase ships

A native Negative Binomial survival function `nbinom_sf(k, mu, alpha)` and its complement `nbinom_cdf(k, mu, alpha)` in the formula evaluator. Mirrors the Phase 3H `norm_cdf` pattern verbatim — hand-rolled direct PMF summation, zero new dependencies. Half-integer line semantics floor `k` before computation.

Once shipped, claw-core's MLB cartridge can compute `P_Over_NB` live inside Mosaic instead of baking it in as a precomputed Input. The slider workflow ADR-0001 was built around finally works end-to-end for MLB (today it works for NBA via `norm_cdf`).

**Independent of Phase 8.2.** Both phases can ship in parallel; this one is the formula primitive, 8.2 is the HTTP surface.

---

## Required reading (in this order)

1. **ADR-0031 Amendments (CRITICAL — read first).** All 7 amendments are binding for implementation. They override the body of the ADR where they conflict. In particular:
   - Amendment 1 corrects the fixture values (the body's `0.4513` test value is wrong; scipy says `0.4492`)
   - Amendment 2 changes the invalid-domain behavior from "runtime error" to "return Null"
   - Amendment 5 specifies the shared `nbinom_cdf_compute` helper architecture
2. **ADR-0031 body** — math, parametrization, rationale (interpret through the amendments)
3. **Research note:** [`../research-notes/nbinom-sf-formula-function.md`](../research-notes/nbinom-sf-formula-function.md) — context on why this primitive exists. ⚠ NOTE: the research note's fixture values are wrong (corrected in Amendment 1); use Amendment 1's table.
4. **Phase 3H precedent files (the pattern to mirror):**
   - `crates/mc-model/src/formula.rs:933` — `norm_cdf` parse site
   - `crates/mc-core/src/rule.rs:1213` — `norm_cdf_compute` numeric body
   - `crates/mc-core/src/rule.rs:133-134` and `946-955` — `ParsedRuleBody` enum + `ParsedNormCdfBody` struct shape
5. **CLAUDE.md** (project root) — §2.5 (Null semantics), §3.1 (forbidden patterns), §6 (self-check gates)

---

## Phase 3L scope

| # | Item |
|---|---|
| 1 | `nbinom_cdf_compute(k, mu, alpha) -> Option<f64>` — shared PMF accumulation helper (Amendment 5) |
| 2 | `nbinom_sf_compute(k, mu, alpha) -> Option<f64>` — derived from cdf (Amendment 5) |
| 3 | Parser entries in `formula.rs` for both functions (mirrors `norm_cdf`) |
| 4 | AST enum variants `ParsedRuleBody::NbinomSf` and `NbinomCdf` |
| 5 | Eval dispatch — `Option<f64>` mapping to `ScalarValue::Number(x)` / `ScalarValue::Null` |
| 6 | Diagnostic codes registered post-preflight per Amendment 3 + 7 |
| 7 | Tests: Amendment 1 fixture grid + Amendment 6 named invariant tests |
| 8 | Python regeneration script committed at `crates/mc-core/tests/nbinom_sf_fixtures.py` |
| 9 | JSON schema regenerated per Phase 3K (new `ParsedRuleBody` variants need `#[derive(JsonSchema)]`) |
| 10 | Doc comments on parser handlers + compute fns with validity-range note (Amendment 4 language) |

**Out of scope (do NOT add):**
- `nbinom_pmf` (rejected in Alt 3 — derivable via `cdf(k) - cdf(k-1)` if a consumer needs it)
- `statrs` dep (rejected in Alt 1 — hand-roll covers MLB range exactly)
- Log-space PMF for `k > 50` (Decision 7 future work)
- Incomplete-beta path (Decision 7 future work)
- Runtime MC2058 diagnostic (Amendment 2 retired it)
- Cartridge migration of `P_Over_NB` from Input → Derived in `examples/sports-betting/mlb-totals.yaml` (claw-core-side soft acceptance; not blocking this phase)

---

## Pre-flight checklist (before writing any code)

Run these and resolve before starting:

```bash
# 1. Diagnostic code preflight (Amendment 3 + 7)
grep -RE "MC1018|MC1019|MC3013" docs/ crates/ 2>/dev/null
# Expected output: no matches. If any code is allocated, shift to next free MC10xx / MC30xx.

# 2. Verify the norm_cdf precedent files exist at the cited line numbers
grep -n "norm_cdf" crates/mc-model/src/formula.rs | head -5
grep -n "norm_cdf_compute\|NormCdf" crates/mc-core/src/rule.rs | head -10

# 3. Confirm scipy version for fixture regeneration
python3 -c "import scipy; print('scipy', scipy.__version__)"
# Expected: scipy >= 1.11. If older, install scipy>=1.11 before generating fixtures.

# 4. Confirm Phase 3K JsonSchema infrastructure is in place
grep -RE "JsonSchema" crates/mc-model/src/ | head -5
# Expected: ParsedRuleBody and surrounding types already derive JsonSchema.

# 5. Verify clean working tree
git status
# Expected: clean. If not, commit or stash before starting.
```

Record the code allocations + scipy version + git SHA in chat before Step 1.

---

## Implementation path

### Step 1: Regenerate fixtures (do this BEFORE writing code)

**Create:** `crates/mc-core/tests/nbinom_sf_fixtures.py`

```python
"""
Generator for nbinom_sf / nbinom_cdf reference values.
Pinned to the scipy version recorded in the comment below.
Run: python3 nbinom_sf_fixtures.py > fixtures.txt
Then paste the Markdown table from the output into the test file's doc comment.

DO NOT EDIT INDIVIDUAL VALUES — regenerate the whole table.
"""
from scipy.stats import nbinom
import math
import scipy

print(f"# Generated by nbinom_sf_fixtures.py against scipy {scipy.__version__}")
print(f"# Convention: nbinom_sf(k, mu, alpha) = nbinom.sf(floor(k), n=1/alpha, p=1/(1+alpha*mu))")
print()

def sf(k, mu, alpha):
    n = 1.0 / alpha
    p = 1.0 / (1.0 + alpha * mu)
    return float(nbinom.sf(math.floor(k), n, p))

def cdf(k, mu, alpha):
    n = 1.0 / alpha
    p = 1.0 / (1.0 + alpha * mu)
    return float(nbinom.cdf(math.floor(k), n, p))

fixtures = [
    ( 8.0,  8.5, 0.13, "MLB typical line, integer"),
    ( 8.5,  8.5, 0.13, "Half-integer — equals integer k=8"),
    ( 9.0,  8.5, 0.13, "MLB push-line behavior"),
    (10.0,  8.5, 0.13, "High line"),
    (20.0,  8.5, 0.13, "Extreme over (lottery)"),
    ( 0.0,  4.0, 0.15, "Low-mu, k=0 (almost always over)"),
    ( 4.0,  4.0, 0.15, "At-the-line low-scoring matchup"),
    ( 5.5,  4.0, 0.15, "Half-integer over, low-scoring"),
    ( 7.5, 12.0, 0.10, "Half-integer under, high-scoring"),
    (12.5, 12.0, 0.10, "Half-integer over, high-scoring"),
    (-1.0,  8.5, 0.13, "Negative k → always over (X >= 0 invariant)"),
    ( 8.0,  8.5, 0.05, "Near-Poisson (small α)"),
    ( 8.0,  8.5, 0.30, "High dispersion"),
]

print("| k     | mu   | alpha | nbinom_sf       | nbinom_cdf      | note |")
print("|-------|------|-------|-----------------|-----------------|------|")
for k, mu, alpha, note in fixtures:
    print(f"| {k:>5} | {mu:>4} | {alpha:>5} | {sf(k, mu, alpha):.9f} | {cdf(k, mu, alpha):.9f} | {note} |")
```

Run it, paste the output table into the doc comment header of `crates/mc-core/tests/nbinom_sf.rs` (Step 6 below). Commit the `.py` file alongside the test.

### Step 2: AST + parser

**File:** `crates/mc-core/src/rule.rs` — add to `ParsedRuleBody`:

```rust
NbinomSf(ParsedNbinomBody),
NbinomCdf(ParsedNbinomBody),
```

And alongside `ParsedNormCdfBody`, add:

```rust
#[derive(Clone, Debug)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ParsedNbinomBody {
    pub k: Box<ParsedExpr>,
    pub mu: Box<ParsedExpr>,
    pub alpha: Box<ParsedExpr>,
}
```

**File:** `crates/mc-model/src/formula.rs:933` (around the `norm_cdf` parse site) — add two parallel branches:

```rust
"nbinom_sf" => {
    let args = self.parse_arg_list()?;
    self.expect_close_paren("nbinom_sf")?;
    if args.len() != 3 {
        return Err(FormulaError::diagnostic(
            DiagnosticCode::NBINOM_SF_WRONG_ARG_COUNT,  // assign code at preflight
            call_start,
            format!("nbinom_sf expects exactly 3 arguments (k, mu, alpha), got {}", args.len()),
        ));
    }
    let [k, mu, alpha] = take3(args);
    Ok(ParsedRuleBody::NbinomSf(ParsedNbinomBody {
        k: Box::new(k), mu: Box::new(mu), alpha: Box::new(alpha),
    }))
}
"nbinom_cdf" => {
    // identical shape, returns ParsedRuleBody::NbinomCdf with NBINOM_CDF_WRONG_ARG_COUNT
}
```

Error message format MUST match `norm_cdf`'s wrong-arg-count message verbatim except for the function name.

### Step 3: Compute helpers (Amendment 5 architecture)

**File:** `crates/mc-core/src/rule.rs` (near `norm_cdf_compute` at line 1213):

```rust
/// Negative Binomial CDF in mean-dispersion parametrization.
///
/// Per ADR-0031 §6 (floor convention) and Amendment 2 (Null semantics):
/// - Returns Some(cdf) ∈ [0.0, 1.0] for valid inputs
/// - Returns None when any input is NaN, mu <= 0, or alpha <= 0
/// - Floors non-integer k before summing the PMF
///
/// Validated against scipy.stats.nbinom within 1e-6 absolute tolerance
/// across the declared MLB operating range (k ∈ [0, 50], α ∈ [0.05, 1.0],
/// μ ∈ [0.5, 50]). Outside this range precision may degrade.
fn nbinom_cdf_compute(k: f64, mu: f64, alpha: f64) -> Option<f64> {
    if k.is_nan() || mu.is_nan() || alpha.is_nan() { return None; }
    if mu <= 0.0 || alpha <= 0.0 { return None; }
    let k_int = k.floor() as i64;
    if k_int < 0 { return Some(0.0); }  // P(X <= negative) = 0

    let r = 1.0 / alpha;
    let p = 1.0 / (1.0 + alpha * mu);

    let mut cdf = p.powf(r);            // pmf(0) = p^r
    let mut pmf_i = cdf;
    for i in 1..=k_int {
        // Ratio recursion for stability:
        // pmf(i) / pmf(i-1) = (r + i - 1) / i * (1 - p)
        pmf_i *= (r + (i as f64) - 1.0) / (i as f64) * (1.0 - p);
        cdf += pmf_i;
    }
    Some(cdf.clamp(0.0, 1.0))
}

/// Survival function: 1 - cdf. Per ADR-0031 Amendment 5, derived through
/// the shared helper to prevent drift.
fn nbinom_sf_compute(k: f64, mu: f64, alpha: f64) -> Option<f64> {
    nbinom_cdf_compute(k, mu, alpha).map(|cdf| (1.0 - cdf).clamp(0.0, 1.0))
}
```

### Step 4: Eval dispatch

In whichever function dispatches `ParsedRuleBody` variants (locate by grep `ParsedRuleBody::NormCdf`), add two new arms. Mirror the `NormCdf` arm exactly. For `Option<f64>`:
- `Some(value)` → `ScalarValue::Number(value)`
- `None` → `ScalarValue::Null`

No error path for invalid inputs (Amendment 2). The arg evaluation itself can still produce errors (e.g., unresolved cell reference), but the `nbinom_*` math never produces a Result::Err.

### Step 5: Doc comments + spec references

Every public-facing surface (parser handler, compute fn, AST variant) needs:

```rust
// Per docs/decisions/0031-nbinom-sf-formula-function.md §<section>:
//   <one-line invariant>
```

At minimum:
- Parser handler: "Per ADR-0031 §3 (Decision 1): nbinom_sf takes 3 args (k, mu, alpha)"
- `nbinom_cdf_compute`: "Per ADR-0031 §6 (floor convention) and Amendment 2 (Null semantics)"
- AST variant: doc comment pointing to ADR-0031

### Step 6: Tests (Amendment 1 fixtures + Amendment 6 invariants)

**Create:** `crates/mc-core/tests/nbinom_sf.rs`

Paste the Amendment 1 fixture grid into the file's doc comment (from Step 1 script output). Then write:

```rust
//! Tests for nbinom_sf / nbinom_cdf.
//!
//! Reference values generated by tests/nbinom_sf_fixtures.py against scipy 1.13.1.
//! Tolerance: 1e-6 absolute (PMF accumulation regime — wider than 1e-9 kernel epsilon).
//!
//! [paste the fixture table here verbatim]

use mc_core::rule::{nbinom_sf_compute, nbinom_cdf_compute};

const TOL: f64 = 1e-6;

#[test]
fn t_nbinom_sf_fixtures_against_scipy() {
    // Each row: (k, mu, alpha, expected_sf, expected_cdf)
    let cases = [
        ( 8.0_f64,  8.5, 0.13, 0.449201940, 0.550798060),
        ( 8.5,      8.5, 0.13, 0.449201940, 0.550798060),
        ( 9.0,      8.5, 0.13, 0.360882026, 0.639117974),
        (10.0,      8.5, 0.13, 0.283491963, 0.716508037),
        (20.0,      8.5, 0.13, 0.010324246, 0.989675754),
        ( 0.0,      4.0, 0.15, 0.956428740, 0.043571260),
        ( 4.0,      4.0, 0.15, 0.367553663, 0.632446337),
        ( 5.5,      4.0, 0.15, 0.244569812, 0.755430188),
        ( 7.5,     12.0, 0.10, 0.806240752, 0.193759248),
        (12.5,     12.0, 0.10, 0.418033119, 0.581966881),
        (-1.0,      8.5, 0.13, 1.000000000, 0.000000000),
        ( 8.0,      8.5, 0.05, 0.464321488, 0.535678512),
        ( 8.0,      8.5, 0.30, 0.425855506, 0.574144494),
    ];
    for (k, mu, alpha, expected_sf, expected_cdf) in cases {
        let sf = nbinom_sf_compute(k, mu, alpha).expect("valid inputs");
        let cdf = nbinom_cdf_compute(k, mu, alpha).expect("valid inputs");
        assert!((sf - expected_sf).abs() < TOL,
            "sf({k}, mu={mu}, alpha={alpha}) = {sf}, expected {expected_sf}");
        assert!((cdf - expected_cdf).abs() < TOL,
            "cdf({k}, mu={mu}, alpha={alpha}) = {cdf}, expected {expected_cdf}");
    }
}

// Then add the 6 Amendment 6 named tests verbatim:
//   t_nbinom_sf_half_integer_floor
//   t_nbinom_sf_integer_push_line
//   t_nbinom_sf_monotone_decreasing_in_k
//   t_nbinom_sf_monotone_increasing_in_mu
//   t_nbinom_sf_cdf_complement
//   t_nbinom_sf_invalid_returns_null
```

Add 2-3 parser-level integration tests in `crates/mc-model/tests/` (or wherever `norm_cdf` parse tests live) verifying:
- `nbinom_sf(8.5, 8.5, 0.13)` parses correctly
- `nbinom_sf(x, y)` (wrong arg count) produces the assigned MC code
- `nbinom_cdf(...)` parses to the right variant

### Step 7: JSON schema regeneration

Per Phase 3K:

```bash
cargo run --bin mc-model-schema > docs/specs/mosaic-model-schema.json
git diff docs/specs/mosaic-model-schema.json  # verify the new variants appear
```

The drift-check CI test runs this internally; verify it passes locally before pushing.

### Step 8: Build gates (CLAUDE.md §6)

```bash
cargo fmt --check --all
cargo clippy --all-targets --workspace -- -D warnings
cargo build --release --workspace
cargo test --workspace
grep -rn "\.unwrap()\|\.expect(\|panic!(\|unimplemented!(\|todo!(" crates/mc-core/src/
```

Last line: only matches in test/bench files are acceptable. Any matches in `crates/mc-core/src/` proper are violations.

---

## Acceptance gate (binding — Amendment 7 of ADR-0031)

Implementer reports each of these explicitly when claiming done:

- [ ] AC #1: `nbinom_sf(k, mu, alpha)` parses; correct diagnostic on wrong arg count
- [ ] AC #2: `nbinom_cdf(k, mu, alpha)` parses; correct diagnostic on wrong arg count
- [ ] AC #3: 13 fixture cases pass within 1e-6 tolerance
- [ ] AC #4: `t_nbinom_sf_half_integer_floor` passes
- [ ] AC #5: `t_nbinom_sf_integer_push_line` passes
- [ ] AC #6: `t_nbinom_sf_monotone_decreasing_in_k` passes
- [ ] AC #7: `t_nbinom_sf_monotone_increasing_in_mu` passes
- [ ] AC #8: `t_nbinom_sf_cdf_complement` passes
- [ ] AC #9: `t_nbinom_sf_invalid_returns_null` passes (mu≤0, alpha≤0, NaN → Null)
- [ ] AC #10: `k < 0` returns 1.0 via shared-helper path
- [ ] AC #11: Validity-range doc comment uses Amendment 4 language ("validated against scipy within tolerance" — not "numerically exact")
- [ ] AC #12: `nbinom_sf_compute` calls `nbinom_cdf_compute` (no duplicated PMF loop)
- [ ] AC #13: Diagnostic codes assigned post-preflight (record final codes in completion report)
- [ ] AC #14: No new external dependencies in any Cargo.toml
- [ ] AC #15: All existing tests pass unchanged
- [ ] AC #16: `cargo test --workspace` passes
- [ ] AC #17: `cargo clippy --all-targets --workspace -- -D warnings` clean
- [ ] AC #18: `cargo fmt --check --all` clean
- [ ] AC #19: JSON schema regenerated; CI drift check passes
- [ ] AC #20: `nbinom_sf_fixtures.py` committed with scipy version pinned in header

Soft acceptance (separate commit, not blocking — claw-core side or follow-up):
- [ ] AC #SOFT-1: MLB cartridge `examples/sports-betting/mlb-totals.yaml` migrated from baked Input `P_Over_NB` to Derived rule `nbinom_sf(Market_Line, Predicted_Mean, Dispersion_Alpha)`
- [ ] AC #SOFT-2: `mc model whatif --coord '...Game=X,Measure=sharp_close_line' --value 9.5` returns updated `P_Over_NB`

---

## Effort and shape

- ~140 LOC of compute + parser code (mirrors `norm_cdf` structurally)
- ~150 LOC of tests (13 fixtures + 6 named invariant tests + 2-3 parser tests)
- ~30 LOC Python regen script
- ~1-2 sessions including the build-gate self-check

The novelty is in the math (PMF ratio recursion) and the Null-semantics adoption (Amendment 2). The plumbing is mechanical.

---

## Common pitfalls (forewarned, forearmed)

1. **Reading the research note's fixture values as truth.** They're wrong. Use Amendment 1.
2. **Implementing `nbinom_sf` directly instead of through `nbinom_cdf`.** Amendment 5 specifies the shared-helper architecture for drift prevention.
3. **Returning an error for `mu ≤ 0`.** Amendment 2 changed this to Null. The body of the ADR still mentions MC2058 in places — ignore those mentions; Amendment 2 retired the code.
4. **Hard-coding MC1018/MC1019.** Preflight before allocation. Use semantic names internally; the numeric codes can shift.
5. **Forgetting the `#[derive(JsonSchema)]` on `ParsedNbinomBody`.** Phase 3K's schema generation will fail silently otherwise — the new variant won't appear in the schema and editor autocomplete won't catch typos in the cube YAML.
6. **Comparing fixture values with `==`.** Float comparison hazard. Use `(actual - expected).abs() < 1e-6`.
7. **Adding `nbinom_pmf` "while you're there."** Don't. Rejected in Alt 3.
8. **Pulling in `statrs` "because it's one line."** Rejected in Alt 1. The hand-roll is the contract.

---

## Cross-links

- ADR-0031: [`../decisions/0031-nbinom-sf-formula-function.md`](../decisions/0031-nbinom-sf-formula-function.md)
- Research note: [`../research-notes/nbinom-sf-formula-function.md`](../research-notes/nbinom-sf-formula-function.md) (⚠ fixture values wrong; use Amendment 1)
- Phase 3H precedent ADR: [`../decisions/0018-phase-3h-2-fitted-model-adstock-saturation.md`](../decisions/0018-phase-3h-2-fitted-model-adstock-saturation.md)
- Phase 3K (JsonSchema infra): [`../decisions/0030-model-authoring-ergonomics.md`](../decisions/0030-model-authoring-ergonomics.md)
- claw-core EXP-028 (the demand driver): https://github.com/edwinlov3tt/claw-core/blob/main/docs/reports/exp-028-walk-forward-v10-nb.md
- Sibling phase: [`./phase-8-2-consumer-api-handoff.md`](./phase-8-2-consumer-api-handoff.md) — independent, can ship in parallel

---

## Completion report template

When done, write `docs/reports/phase-3l-completion-report.md` covering:

1. Final MC diagnostic code assignments (parse-time + lint if Amendment 2a was implemented)
2. Test count + pass status (workspace-wide)
3. Build gate results (fmt, clippy, build, test, grep)
4. Validity-range deviations encountered (if any) — fixtures that needed widened tolerance
5. Cartridge migration follow-up status (soft acceptance — link to claw-core PR if filed)
6. Effort actual vs estimate (1-2 sessions)
7. Anything surprising or worth amending in the ADR
