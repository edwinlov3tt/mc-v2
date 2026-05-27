# ADR-0033: Phase 10A — Evaluation Metrics Library

**Status:** Proposed
**Date:** 2026-05-27
**Deciders:** project owner
**Phase:** 10A (first phase of the evaluation-primitives track; foundational for 10B-F)
**Crate(s) touched:** `mc-model` (parser) + `mc-core` (evaluator); no daemon, no kernel-interface changes
**Prerequisite reading:**
- [Research note: built-in evaluation primitives](../research-notes/built-in-evaluation-primitives.md) — the 5 commands this track will eventually ship
- [Research note: pymc-marketing pattern extraction](../research-notes/pymc-marketing-pattern-extraction.md) — companion Bayesian primitives (separate track)
- [ADR-0031](./0031-nbinom-sf-formula-function.md) — the precedent pattern (hand-rolled, no deps, mirrors norm_cdf shape)
- [ADR-0015 (Phase 3I)](./0015-phase-3i-formula-language-completion.md) — the existing aggregation family (`sum_over`, `avg_over`, `min_over`, `max_over`, `wavg_over`)

---

## Context

claw-core's MLB cartridge produced 29 experiment scripts in one quarter
(EXP-021 through EXP-045). Pattern analysis surfaced 5 repeating shapes
that account for 26 of them, with one cross-cutting need: **the same
~10 metrics — direction_accuracy, ROI, Sharpe, Wilson CI, Brier score,
max_drawdown, etc. — are reimplemented in every script.**

The companion research note proposed 5 new `mc model` subcommands
(`backtest`, `walk-forward`, `simulate`, `grade`, batch `sweep`) that
would replace 26 of those scripts. All 5 commands depend on the same
metrics library. The project owner chose Option 3 (demand-driven) for
the sequencing — ship the foundational metrics library + `mc model
grade` (the simplest command) + batch `sweep` first, then let consumer
demand sequence the rest.

**This ADR is the first commit in that track.** It ships the metrics
library as new formula primitives in `mc-core`/`mc-model`, so future
`mc model` commands have a uniform vocabulary to express their
aggregations. Implementation pattern mirrors Phase 3L (`nbinom_sf`)
verbatim — hand-rolled, no new dependencies, surgical scope.

---

## The two categories of metrics

Inspecting the 10 metrics from the research note, they split cleanly
into two groups:

### Compositional (no new primitives needed)

| Metric | Composition from existing primitives |
|---|---|
| `direction_accuracy` | `avg_over(...if(predicted == actual, 1, 0)...)` |
| `roi` | `sum_over(pnl) / sum_over(stake)` |
| `brier` | `avg_over(pow(predicted_prob - outcome, 2))` |
| `mean_residual` | `avg_over(actual - predicted)` |
| `n_bets` | currently requires a new `count_over` (see below); composable thereafter |

Four of the five are expressible as one-line cartridge rules using the
existing `_over` family + arithmetic + `if`. Authors write these in
YAML; no Rust changes needed.

### Native (new primitives this ADR ships)

| Metric | Why native | Notes |
|---|---|---|
| `std_over(dim, measure)` | No standard-deviation aggregation exists today | Needed for Sharpe ratio (mean/std), for risk-adjusted variants of every metric, and for credible-interval-adjacent work in Phase 11 |
| `var_over(dim, measure)` | Same family as std; cheap to add alongside | `var_over = std_over²`; sometimes preferred for downstream variance decomposition |
| `count_over(dim, measure)` | Currently only `sum_over` + `if` workarounds exist for "how many cells match condition" | Needed for `n_bets`, Brier denominators, segmented counts |
| `wilson_ci_lower(p, n)` | Statistical CI lower bound; closed-form but specific | Needed for direction-accuracy confidence intervals — the load-bearing "is this above breakeven" check |
| `wilson_ci_upper(p, n)` | Same as lower; ship together for symmetry | Needed for two-sided segment evaluation |

**Deferred to later phases (Phase 10E/F or beyond):**
- `max_drawdown(series, time_dim)` — needs bet-record time-series semantics that walk-forward + simulate naturally provide
- `recovery_bets(series, time_dim)` — same
- `sharpe_ratio` — compositional once `std_over` ships: `avg_over(returns) / std_over(returns) * sqrt(count_over(returns))`

The deferrals are real: max_drawdown and recovery_bets need a time-
ordered scan over chronological bet records, which is closer to walk-
forward's natural output shape than a cube aggregation. Ship them in
the phase where they're consumed.

---

## Decisions

### Decision 1: Function signatures

```
std_over(dim, measure)              -> f64
var_over(dim, measure)              -> f64
count_over(dim, measure)            -> f64    (counts non-null cells in measure across dim)
wilson_ci_lower(p, n)               -> f64    (95% CI by default; level optional in v2)
wilson_ci_upper(p, n)               -> f64    (95% CI by default)
```

**Naming alignment.** `std_over` / `var_over` / `count_over` match the
existing `sum_over` / `avg_over` / `min_over` / `max_over` family
verbatim (ADR-0015 + Phase 3I). `wilson_ci_*` is a new family — the
`_ci_` infix leaves room for `bootstrap_ci_*` or other interval types
without naming collision.

**Argument order matches existing precedent.** `sum_over(dim, measure)`
puts the aggregation dimension first; `std_over` and `var_over` mirror
this. `count_over(dim, measure)` follows the same shape — counts how
many leaves under `dim` have non-null values in `measure`.

**Why fixed 95% for Wilson CI in v1.** Most consumers want 95% out of
the box. Adding a `level` parameter requires three-arg parsing + a
default — small but unnecessary scope for v1. If a consumer needs 90%
or 99%, add `wilson_ci_lower(p, n, level)` in a follow-up phase
(additive change, no breaking surface).

### Decision 2: Implementation — hand-rolled, no new dependencies

Mirror Phase 3L (`nbinom_sf`) precedent verbatim. Wilson CI has a
closed-form formula (Wilson 1927); std/var/count are textbook
aggregations. Zero new dependencies in `mc-core` or `mc-model`.

**Wilson score interval (binding for the math).** For `p̂ = k/n` with
confidence level `1-α`:

```
center  = (p̂ + z² / (2n)) / (1 + z² / n)
margin  = z · sqrt(p̂(1-p̂)/n + z²/(4n²)) / (1 + z² / n)
lower   = center - margin
upper   = center + margin
```

where `z = 1.959963984540054` (the inverse normal CDF at 0.975 for
two-sided 95%). Hand-coded constant; no scipy dependency.

**Edge cases (binding):**
- `n = 0` → return `Null` (no information; matches ADR-0031 Amendment 2 discipline)
- `n < 0` → return `Null` (invalid)
- `p < 0` or `p > 1` → return `Null` (invalid probability)
- `p` or `n` is NaN → return `Null` (never let NaN flow through)
- `p = 0` and `n > 0` → lower = 0.0, upper = `z²/(n+z²)` (degenerate but defined)
- `p = 1` and `n > 0` → lower = `n/(n+z²)`, upper = 1.0 (degenerate but defined)

**std/var implementation.** Standard population variance via Welford's
algorithm (single-pass, numerically stable):

```rust
fn var_compute(values: &[f64]) -> Option<f64> {
    let n = values.iter().filter(|v| !v.is_nan()).count();
    if n < 2 { return None; }    // need ≥ 2 samples for variance
    let mut mean = 0.0;
    let mut m2 = 0.0;
    let mut k = 0.0;
    for &v in values.iter().filter(|v| !v.is_nan()) {
        k += 1.0;
        let delta = v - mean;
        mean += delta / k;
        let delta2 = v - mean;
        m2 += delta * delta2;
    }
    Some(m2 / k)    // population variance (divide by n, not n-1)
}

fn std_compute(values: &[f64]) -> Option<f64> {
    var_compute(values).map(f64::sqrt)
}
```

**Population vs sample variance.** Population variance (divide by n).
Rationale: in cube evaluation, "the sample" IS the population — we're
aggregating over enumerated leaves, not a sample drawn from a
larger universe. If a consumer needs sample variance (divide by n-1)
for Bessel-corrected inference, surface in chat — easy to add a
`std_sample_over` variant later.

**count_over implementation.** Trivially counts non-null cells under
the scope:

```rust
fn count_compute(values: &[ScalarValue]) -> Option<f64> {
    let n = values.iter().filter(|v| !matches!(v, ScalarValue::Null)).count();
    Some(n as f64)
}
```

Returns `Some(0.0)` for empty scope (different from std/var's `None`).
Zero is a valid count; "no information" is not — knowing the count is
zero IS information.

### Decision 3: Null semantics — match ADR-0031 Amendment 2

| Input | Behavior |
|---|---|
| Empty scope (no leaves) | `count_over` returns 0; `std/var_over` return `Null`; `wilson_ci_*` returns `Null` |
| All-Null scope | `count_over` returns 0; `std/var_over` return `Null` |
| Mixed Null + valid | Null values skipped; aggregate over valid values; n reflects valid count |
| NaN input (wilson) | Return `Null` |
| Invalid p/n (wilson) | Return `Null` |

**Rationale.** Consistent with ADR-0031's Null-propagation discipline.
Never returns NaN; never errors at runtime. Authors compose with
existing primitives the same way they would for `nbinom_sf` — implicit
`mean()` reduction, Null propagation through derived measures.

### Decision 4: Diagnostic codes — preflight required

Follow the established pattern (ADR-0031 Amendment 3 + ADR-0032
Amendment 7). Semantic names lock; numeric codes assigned post-preflight.

**Semantic names:**
- `STD_OVER_WRONG_ARG_COUNT`
- `VAR_OVER_WRONG_ARG_COUNT`
- `COUNT_OVER_WRONG_ARG_COUNT`
- `WILSON_CI_LOWER_WRONG_ARG_COUNT`
- `WILSON_CI_UPPER_WRONG_ARG_COUNT`

**Reuse expectation.** Per ADR-0031's discovery: existing
`FormulaError::wrong_arg_count` helper emits **MC1008** for all formula
function arity errors. Default to **reusing MC1008** for these new
primitives — the message text disambiguates the function. Allocate
new codes only if the preflight surfaces a structural reason to
diverge from the shared-helper pattern.

**Preflight command before implementation:**
```bash
grep -RE "MC10[0-9]{2}" docs/ crates/ 2>/dev/null | head -20
```

### Decision 5: Metrics cookbook — separate document, ships with this ADR

Most metrics are compositional (Decision context above). Cube authors
need to know HOW to compose them. Ship `docs/specs/metrics-cookbook.md`
alongside the implementation:

```markdown
# Metrics Cookbook

## direction_accuracy
```yaml
- name: direction_accuracy
  body: 'avg_over(holdout_games, if(predicted_direction == actual_direction, 1.0, 0.0))'
  scope: AllLeaves
```

## roi
```yaml
- name: roi
  body: 'sum_over(holdout_games, pnl) / sum_over(holdout_games, stake)'
```

## brier
```yaml
- name: brier
  body: 'avg_over(holdout_games, pow(predicted_prob - outcome, 2))'
```

## sharpe_ratio  (uses new std_over)
```yaml
- name: sharpe_ratio
  body: 'avg_over(holdout_games, return) / std_over(holdout_games, return) * sqrt(count_over(holdout_games, return))'
```

## direction_accuracy with 95% Wilson confidence
```yaml
- name: direction_accuracy_lower_95
  body: 'wilson_ci_lower(direction_accuracy, count_over(holdout_games, direction_correct))'
```
...
```

This is the user-facing surface. The metrics library is small (5
primitives); the cookbook is what consumers actually read. Treat the
cookbook as a first-class deliverable — it's the demo proof that
authors don't need new code to express the experiment metrics they're
currently writing 300-line scripts for.

### Decision 6: Backward compatibility

Additive only. Zero changes to existing aggregations. Existing cubes
unchanged. No JSON schema breaking changes (just additions per
Phase 3K pattern).

---

## Implementation plan

Mirrors Phase 3L (ADR-0031) structure. Total estimate: ~150 LOC + ~120
LOC of tests + cookbook doc. 1-2 sessions.

### Step 0: Diagnostic code preflight

```bash
grep -RE "MC10[0-9]{2}" docs/ crates/ 2>/dev/null | head -20
```

Confirm MC1008 is the right shared code or surface alternatives.

### Step 1: AST + parser

**File:** `crates/mc-model/src/formula.rs` — add three parse cases to
the `_over` family (around the existing `sum_over` / `avg_over` handler
near line ~950, exact location depends on current layout):

```rust
"std_over" => parse_simple_over(self, "std_over", call_start, OverKind::Std),
"var_over" => parse_simple_over(self, "var_over", call_start, OverKind::Var),
"count_over" => parse_simple_over(self, "count_over", call_start, OverKind::Count),
```

And two for the Wilson family (around the `norm_cdf` parse site near
line ~933):

```rust
"wilson_ci_lower" => { /* mirror norm_cdf 2-arg shape */ }
"wilson_ci_upper" => { /* same */ }
```

**File:** `crates/mc-core/src/rule.rs` — extend `OverKind` enum:

```rust
pub enum OverKind {
    Sum, Avg, Min, Max,
    Std,      // new
    Var,      // new
    Count,    // new
}
```

And add to `ParsedRuleBody`:

```rust
WilsonCiLower(ParsedWilsonBody),
WilsonCiUpper(ParsedWilsonBody),
```

With the body struct:

```rust
#[derive(Clone, Debug)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ParsedWilsonBody {
    pub p: Box<ParsedExpr>,
    pub n: Box<ParsedExpr>,
}
```

### Step 2: Compute helpers

**File:** `crates/mc-core/src/rule.rs` — near `nbinom_sf_compute`:

```rust
/// Wilson 95% CI lower bound. Returns None for invalid inputs (n≤0,
/// p outside [0,1], NaN).
/// Per ADR-0033 Decision 2.
fn wilson_ci_lower_compute(p: f64, n: f64) -> Option<f64> {
    if p.is_nan() || n.is_nan() { return None; }
    if n <= 0.0 || !(0.0..=1.0).contains(&p) { return None; }
    const Z: f64 = 1.959963984540054;  // inverse normal CDF at 0.975
    let z2 = Z * Z;
    let denom = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / denom;
    let margin = Z * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt() / denom;
    Some((center - margin).clamp(0.0, 1.0))
}

fn wilson_ci_upper_compute(p: f64, n: f64) -> Option<f64> {
    if p.is_nan() || n.is_nan() { return None; }
    if n <= 0.0 || !(0.0..=1.0).contains(&p) { return None; }
    const Z: f64 = 1.959963984540054;
    let z2 = Z * Z;
    let denom = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / denom;
    let margin = Z * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt() / denom;
    Some((center + margin).clamp(0.0, 1.0))
}
```

Welford std/var/count helpers per Decision 2.

### Step 3: Eval dispatch

Locate the existing `OverKind` match in the evaluator; add `Std`, `Var`,
`Count` arms. Locate the `ParsedRuleBody` dispatch (where `NormCdf` is
handled — added in Phase 3H); add `WilsonCiLower` and `WilsonCiUpper`
arms. Mirror Phase 3L plumbing.

### Step 4: Tests

**File:** `crates/mc-core/tests/metrics.rs` (new)

Generate reference values from Python:

```python
# crates/mc-core/tests/metrics_fixtures.py
# Run with: python3 metrics_fixtures.py
# Pin: statsmodels >= 0.14 for Wilson CI; numpy for std/var
from statsmodels.stats.proportion import proportion_confint
import numpy as np

# Wilson CI fixtures
cases_wilson = [
    (0.5,   100, "balanced n=100"),
    (0.6,   100, "moderate edge n=100"),
    (0.55,  1508, "MLB walk-forward n=1508 — V1.0+NB headline"),
    (0.0,   100, "degenerate p=0"),
    (1.0,   100, "degenerate p=1"),
    (0.5,   1,   "tiny n=1"),
]
print("# Wilson 95% CI fixtures (statsmodels proportion_confint)")
for p, n, note in cases_wilson:
    lo, hi = proportion_confint(int(p * n), n, alpha=0.05, method='wilson')
    print(f"  p={p:.3f} n={n:>5}: lower={lo:.9f} upper={hi:.9f}  # {note}")

# std/var fixtures
print("\n# Population std/var (numpy with ddof=0)")
cases_std = [
    [1.0, 2.0, 3.0, 4.0, 5.0],
    [0.55, 0.62, 0.48, 0.71, 0.53, 0.58],  # MLB-shaped P_Over values
]
for vals in cases_std:
    arr = np.array(vals)
    print(f"  vals={vals}: mean={arr.mean():.9f} std={arr.std(ddof=0):.9f} var={arr.var(ddof=0):.9f}")
```

Test functions:

```rust
#[test] fn t_wilson_ci_balanced_n100() {
    let lo = wilson_ci_lower_compute(0.5, 100.0).unwrap();
    let hi = wilson_ci_upper_compute(0.5, 100.0).unwrap();
    // statsmodels: lo=0.402642036, hi=0.597357964
    assert!((lo - 0.402642036).abs() < 1e-6);
    assert!((hi - 0.597357964).abs() < 1e-6);
}

#[test] fn t_wilson_ci_mlb_walk_forward() {
    // claw-core V1.0+NB headline: 1508 bets at 59.68% direction accuracy
    let lo = wilson_ci_lower_compute(0.5968, 1508.0).unwrap();
    // statsmodels lower bound ~0.5718 → confirms the "Wilson LB 57.18%" claim
    // in the integration test report
    assert!((lo - 0.5718).abs() < 0.001);  // ~0.001 tolerance for headline parity
}

#[test] fn t_wilson_ci_degenerate_p0() { ... }
#[test] fn t_wilson_ci_degenerate_p1() { ... }
#[test] fn t_wilson_ci_invalid_returns_null() { ... }  // n=0, n<0, p<0, p>1, NaN
#[test] fn t_wilson_ci_complementary() {
    // wilson(p) and wilson(1-p) should mirror around 0.5
    let lo_p = wilson_ci_lower_compute(0.4, 100.0).unwrap();
    let hi_1mp = wilson_ci_upper_compute(0.6, 100.0).unwrap();
    assert!((lo_p + hi_1mp - 1.0).abs() < 1e-9);
}

#[test] fn t_std_over_basic() { ... }
#[test] fn t_var_over_equals_std_squared() { ... }
#[test] fn t_count_over_skips_nulls() { ... }
#[test] fn t_std_over_n_less_than_2_returns_null() { ... }
#[test] fn t_count_over_empty_scope_returns_zero() { ... }
```

Plus 2-3 parser integration tests (wrong arg count → MC1008, correct
parse-to-variant).

### Step 5: JSON schema regeneration

Per Phase 3K:

```bash
cargo run --bin mc-model-schema > docs/specs/mosaic-model-schema.json
git diff docs/specs/mosaic-model-schema.json  # verify new variants appear
```

### Step 6: Metrics cookbook

**File:** `docs/specs/metrics-cookbook.md` (new) per Decision 5.

Document ALL 10 metrics from the research note, marking each as
compositional (one-line YAML) or native (uses new primitives). Include
the MLB cartridge as a worked example — show that ROI, direction
accuracy, Brier, mean residual, and Wilson-bounded direction accuracy
can all be expressed without writing new Rust.

### Step 7: Build gates (CLAUDE.md §6)

Standard: fmt, clippy -D warnings, build, test, forbidden-pattern grep.

---

## Acceptance criteria

1. `std_over(dim, measure)` parses; correct diagnostic on wrong arg count (likely MC1008)
2. `var_over(dim, measure)` parses; `var_over² == std_over` test passes
3. `count_over(dim, measure)` parses; skips Null cells correctly
4. `wilson_ci_lower(p, n)` parses; matches statsmodels `proportion_confint(..., method='wilson')` lower bound within 1e-6 on the 6 fixtures
5. `wilson_ci_upper(p, n)` parses; matches statsmodels upper bound within 1e-6
6. `wilson_ci_complementary` invariant: `wilson_ci_lower(p, n) + wilson_ci_upper(1-p, n) == 1.0`
7. MLB walk-forward headline parity: `wilson_ci_lower(0.5968, 1508)` returns ~0.5718 (the "Wilson LB 57.18%" claim from the integration test report)
8. Invalid inputs return Null per Decision 3 (n≤0, p<0, p>1, NaN any arg)
9. `std_over` returns Null for scope with fewer than 2 non-null values
10. `count_over` returns 0 for empty scope (zero is information; not Null)
11. All five primitives covered in `docs/specs/metrics-cookbook.md` with worked YAML examples
12. JSON schema regenerated; CI drift check passes
13. `metrics_fixtures.py` committed at `crates/mc-core/tests/metrics_fixtures.py` with scipy/statsmodels version pin in header comment
14. Diagnostic codes recorded in completion report (MC1008 reused or new codes per preflight)
15. No new external dependencies in any Cargo.toml
16. All existing tests pass unchanged (Acme, NBA, MLB, Phase 3L)
17. `cargo test --workspace` passes
18. `cargo clippy --all-targets --workspace -- -D warnings` clean
19. `cargo fmt --check --all` clean
20. New `ParsedRuleBody` variants have `#[derive(JsonSchema)]` per Phase 3K
21. The metrics cookbook includes the demo proof: at least one example showing how a claw-core experiment (e.g., exp022 edge buckets, exp023 line source audit) would be expressed as a one-line YAML rule

---

## Alternatives considered

### Alt 1: Ship max_drawdown and recovery_bets in this phase too

Considered. Would let `mc model simulate` ship immediately.

**Rejected because:**
- Both metrics need time-ordered scan over chronological bet records
- Bet records have a natural shape (parquet output from walk-forward) that's not yet defined in any phase
- Adding cube-aggregation versions of these now would compete with the eventual bet-record-based versions later
- Ship them with the phase (10F simulate) that defines the bet-record format

### Alt 2: Ship `sharpe_ratio` as a native primitive

Considered. Sharpe is well-known and frequently asked.

**Rejected because:**
- Compositional once `std_over` ships: `avg_over(returns) / std_over(returns) * sqrt(count_over(returns))`
- Native version would have to decide: annualized? Trade-sequence? Bet-volume-weighted? Different consumers want different definitions
- Cookbook entry shows the composition; consumers pick their flavor

### Alt 3: Wilson CI with configurable confidence level

Considered. `wilson_ci_lower(p, n, level=0.95)` is a small API addition.

**Rejected for v1 because:**
- Three-arg parsing requires default-value handling that's a small but distinct scope
- Most consumers want 95%; the 5% edge case can be a v2 additive function
- If demand surfaces, ship `wilson_ci_lower(p, n, level)` later as a backward-compatible overload

### Alt 4: Sample variance (Bessel-corrected, n-1) instead of population variance

Considered. Sample variance is the default in statsmodels/numpy.

**Rejected for v1 because:**
- In cube aggregation, the "sample" IS the enumerated population — leaves under a scope are not drawn from a larger distribution
- Population variance is the right statistic for "what's the spread of these values"
- Sample variance can be added as `std_sample_over` later if a consumer needs Bessel correction for inferential work

### Alt 5: Build a separate `mc-metrics` crate

Considered. Metrics library could live in its own crate.

**Rejected because:**
- The metrics ARE formula primitives; the formula evaluator lives in mc-core
- Splitting would create a circular dep (mc-metrics → mc-core for ScalarValue; mc-core → mc-metrics for primitives)
- Phase 3L (nbinom_sf) precedent: distributional primitives live directly in mc-core/mc-model
- Single-crate ownership = single test surface = single audit trail

### Alt 6: Pull in a stats crate (statrs)

Considered. statrs has Wilson CI and Welford variance.

**Rejected because:**
- ADR-0025 + Phase 3L precedent: no new dependencies for primitives with closed-form math
- Wilson CI is ~10 lines; Welford is ~10 lines; both are textbook
- statrs pulls in nalgebra and other heavy transitives
- Hand-roll is correct for the validity range (0 ≤ p ≤ 1, n > 0)

---

## Out of scope

- `max_drawdown` and `recovery_bets` (deferred to Phase 10F simulate)
- `sharpe_ratio` as a native primitive (compositional via cookbook)
- Configurable confidence level for Wilson CI (deferred to additive overload)
- Sample variance / Bessel correction (deferred until demand)
- Custom-confidence-level Wilson CI (`level` as third arg)
- Bootstrap CI / jackknife CI (separate primitive family; deferred)
- Other discrete-distribution CIs (Clopper-Pearson, Jeffreys, Agresti-Coull) — same family; deferred until demand
- Brier score as native (compositional)
- Direction accuracy as native (compositional)
- ROI as native (compositional)
- The five `mc model` commands themselves (10B grade, 10C backtest, 10D batch sweep, 10E walk-forward, 10F simulate) — separate ADRs

---

## Cross-links

- **Research notes:**
  - [`built-in-evaluation-primitives.md`](../research-notes/built-in-evaluation-primitives.md) — the parent design for the 5 commands
  - [`pymc-marketing-pattern-extraction.md`](../research-notes/pymc-marketing-pattern-extraction.md) — companion Bayesian track (separate)
- **ADR-0031 (Phase 3L):** `nbinom_sf` — the precedent pattern (hand-rolled, Null semantics, MC1008 reuse)
- **ADR-0015 (Phase 3I):** Formula language completion — defines the existing `_over` aggregation family this phase extends
- **ADR-0030 (Phase 3K):** Model authoring ergonomics — JSON schema generation; new ParsedRuleBody variants must derive JsonSchema
- **CLAUDE.md §6:** Self-check protocol (build gates)
- **claw-core EXP-022 through EXP-045:** the 26 experiment scripts these primitives + downstream commands will eventually replace
- **claw-core integration test:** `docs/reports/mosaic-integration-test.md` — the Wilson LB 57.18% headline that test #7 validates

---

## Notes

**Why now.** Demand-driven sequencing (Option 3) starts with the
foundational layer. The metrics library unblocks Phase 10B (grade),
10C (backtest), and 10D (batch sweep). All three are independently
shippable but all need the same vocabulary. Ship the vocabulary first.

**Why so small.** Phase 10A is deliberately minimal — 5 new
primitives + a cookbook doc. The temptation is to ship everything at
once (max_drawdown, recovery_bets, sharpe, brier as natives). Resist.
Smaller phases are cheaper to review, faster to ship, and harder to
get wrong. The five primitives here are the load-bearing ones; the
rest are compositional or deferrable.

**The cookbook is the demo proof.** The metrics library is small but
the cookbook is what consumers READ. It demonstrates that ~7 of the 10
metrics from the research note are already expressible TODAY with one
new ADR's worth of primitives. That's the "5 commands replace 26
scripts" pitch made concrete.

**Effort:** ~150 LOC implementation + ~120 LOC tests + cookbook doc.
1-2 sessions. Same shape as Phase 3L (nbinom_sf).

**Sequencing of follow-up phases (recommendation, not commitment):**
After this ADR ships, run preflight on consumer demand:
- If claw-core asks for segmented evaluation → Phase 10B (`mc model grade`)
- If batch sensitivity audits → Phase 10D (`mc model sweep --games "..."`)
- If parameter sweeps across the holdout → Phase 10C (`mc model backtest`)

The metrics library makes any of those a focused 2-3 session phase.
