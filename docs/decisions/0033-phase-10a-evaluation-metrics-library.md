# ADR-0033: Phase 10A — Evaluation Metrics Library

**Status:** Accepted (with 7 acceptance amendments — see bottom; binding for implementation)
**Date:** 2026-05-27
**Accepted:** 2026-05-27 (project owner approved after dual external review pass)
**Last amended:** 2026-05-27 — Claude Desktop + GPT-5.1 review feedback folded in; `_over` semantics clarified, variance default flipped to sample (ddof=1)
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

---

## Acceptance amendments

Filed 2026-05-27 after dual external review (Claude Desktop + GPT-5.1, high-effort thinking). Both reviewers independently converged on the same substantive issues. All seven amendments are **binding** for implementation and override the body of this ADR where they conflict. Each amendment was independently verified against the codebase before adoption.

### Amendment 1: `_over` family accepts BARE MEASURES ONLY, not arbitrary expressions (CRITICAL — fixes cookbook)

**Problem.** GPT flagged that the cookbook examples (`avg_over(holdout_games, if(predicted == actual, 1, 0))`, `avg_over(holdout_games, pow(predicted_prob - outcome, 2))`) require the `_over` aggregations to accept arbitrary expressions as their second argument. Existing implementation does NOT.

**Verification (binding).** `crates/mc-model/src/formula.rs` `sum_over` handler uses `parse_bare_identifier`, not `parse_or_expression`:

```rust
"sum_over" => {
    let dimension = self.parse_bare_identifier("sum_over", call_start)?;
    let measure = self.parse_bare_identifier("sum_over", call_start)?;
    ...
}
```

The same pattern applies to `avg_over`, `min_over`, `max_over`, and `wavg_over`. **All `_over` functions accept exactly two bare identifiers: a dimension name and a measure name.**

**Amendment.** Two binding consequences:

(a) **The new primitives (`std_over`, `var_over`, `count_over`) inherit the bare-measure constraint.** Their parse handlers use `parse_bare_identifier` for both arguments, matching `sum_over` verbatim. Do NOT extend any `_over` function to accept expressions in this phase — that's a formula-evaluator semantics change with its own scope and complexity (deferred to a future phase if/when demand surfaces).

(b) **The cookbook is rewritten.** Every compositional metric requires an intermediate derived measure. The "one-line YAML" claim from Decision 5 is replaced with "compositional via 1-3 intermediate measures." Example rewrite:

Before (WRONG — won't parse):
```yaml
- name: direction_accuracy
  body: 'avg_over(holdout_games, if(predicted_direction == actual_direction, 1.0, 0.0))'
```

After (correct):
```yaml
- name: direction_correct
  body: 'if(predicted_direction == actual_direction, 1.0, 0.0)'
  scope: Leaf

- name: direction_accuracy
  body: 'avg_over(holdout_games, direction_correct)'
```

Similar rewrites required for `brier`, `roi`, and `sharpe_ratio` in the cookbook. All examples must use intermediate measures.

**Why not extend `_over`?** GPT's analysis: extending `avg_over(dim, expr)` to accept expressions turns this from a metrics phase into a formula-evaluator semantics phase. Implementation requires scope-correct expression evaluation per leaf, dependency tracking through the expression, and probably new cache invariants. Out of scope for 10A. The intermediate-measure pattern is verbose but correct, and the dependency graph already handles it well.

### Amendment 2: `count_over` semantics — explicit evaluation per leaf

**Problem.** Both reviewers flagged ambiguity in `count_over`'s implementation sketch. The `&[ScalarValue]` shown in the body suggests counting already-materialized cells, which would interact badly with lazy evaluation (CLAUDE.md §2.15).

**Amendment.** Lock the semantics:

```
count_over(dim, measure) evaluates `measure` at every leaf under `dim`,
holding all OTHER coordinates constant. Returns the count of leaves
whose evaluated value is not Null.

This is the same evaluation semantic as sum_over and avg_over.
Performance is O(N_leaves_in_dim × eval_cost_of_measure). Input
measures may use a fast path (skip the eval call) only if the
semantics remain identical.

count_over does NOT:
- Count cells from the store without evaluation
- Count all leaves regardless of measure state
- Trigger side effects beyond the standard lazy graph population
```

**Implementation note.** The compute helper signature changes:
```rust
// WRONG (original sketch):
fn count_compute(values: &[ScalarValue]) -> Option<f64> { ... }

// RIGHT (Amendment 2):
// Receives the dimension and measure refs; performs the per-leaf eval
// the same way sum_over and avg_over do. Mirror their compute path
// exactly, with the count operation replacing the sum/avg accumulation.
```

The implementer mirrors `sum_over`'s eval-dispatch path; only the accumulator differs. Count returns `Some(0.0)` for empty scope (zero is information per Decision 3); `Some(k)` for scope with `k` non-null evaluated values.

### Amendment 3: Sample variance default (ddof=1) — not population variance

**Problem.** Decision 2 + Alt 4 defended population variance (divide by n). Both reviewers pushed back: for the actual primary use case (walk-forward backtesting), past returns ARE a sample drawn from the underlying return distribution. Sample variance (ddof=1) is the correct statistic. Population variance is misleading for inferential workflows like Sharpe ratio computation.

**Amendment.** `std_over` and `var_over` use **sample variance, ddof=1**. This matches numpy, statsmodels, pandas, and scipy defaults — what consumers expect.

**Welford algorithm modification:**

```rust
fn var_compute(values: &[f64]) -> Option<f64> {
    // ... Welford accumulation as before ...
    if k < 2.0 { return None; }    // need ≥ 2 samples for variance (unchanged)
    Some(m2 / (k - 1.0))            // CHANGED: divide by n-1, not n
}
```

The `k < 2.0` guard is unchanged — sample variance is undefined for n=1 (would divide by zero).

**`std_pop_over` and `var_pop_over` are NOT shipped in this phase.** Demand-driven principle: if a consumer needs population variance later, add them as additive variants in a follow-up. The Phase 11+ Bayesian track may need them; that's the right time to allocate.

**Acceptance test update.** Fixture values in `metrics_fixtures.py` change. The Python script becomes:

```python
import numpy as np
print(f"  vals={vals}: mean={arr.mean():.9f} std={arr.std(ddof=1):.9f} var={arr.var(ddof=1):.9f}")
```

(`ddof=1` everywhere.) Re-run the script after Amendment 3 lands; paste new values into the test file.

### Amendment 4: `Null * 0 = Null` confirmed; Sharpe composition is correct as written

**Verification.** `crates/mc-core/src/rule.rs:1943` documents the Null propagation invariant:

> "Null poisons multiplication on either side, including Null * Null."
> `(ScalarValue::F64(x), ScalarValue::F64(y)) => finite_or_null(x * y)`

Lines 2294, 2302, 2361 reinforce: `Mul: Null * 5 = Null`, `Mul: 5 * Null = Null`.

**Amendment.** No code change. The cookbook's Sharpe composition propagates Null correctly when scope is empty (avg_over→Null) or n=1 (std_over→Null). No guard pattern is required for correctness.

**Cookbook clarification.** Still SHOW the explicit guard pattern for clarity:

```yaml
# Concise — relies on Null propagation
- name: sharpe_ratio
  body: 'avg_over(holdout_games, returns) / std_over(holdout_games, returns) * sqrt(count_over(holdout_games, returns))'

# Explicit — same behavior, but the intent is visible
- name: sharpe_ratio_guarded
  body: 'if(count_over(holdout_games, returns) >= 2, avg_over(holdout_games, returns) / std_over(holdout_games, returns) * sqrt(count_over(holdout_games, returns)), Null)'
```

Both produce identical output. Authors choose based on style preference; the cookbook documents both.

### Amendment 5: Wilson CI cookbook guardrails — proportion-only + trial-count `n`

**Problem.** Both reviewers flagged that Wilson CI looks general (it's a "confidence interval") but is specifically for binomial proportions. Authors passing ROI, Sharpe, or any non-proportion get silent Null returns. Additionally, the `n` argument must be total trials, not successes — a subtle modeling mistake that's easy to make.

**Amendment.** Add to the cookbook (binding for the deliverable):

```markdown
### Wilson CI: ONLY for binomial proportions

`wilson_ci_lower(p, n)` and `wilson_ci_upper(p, n)` apply specifically
to observed binomial proportions — k successes out of n independent
trials with `p = k/n`.

USE for:
- Win rate (direction accuracy)
- Conversion rate
- Hit rate
- Any rate that's k/n where k ∈ {0, ..., n}

DO NOT USE for:
- ROI (could be negative or > 1)
- Sharpe ratio
- Mean residual
- Expected value
- PnL or other dollar quantities

If p falls outside [0, 1], Wilson returns Null (silent — no error).
For arbitrary-bound CIs, bootstrap CI primitives are deferred to a
future phase.
```

**The `n` argument trap (safer cookbook pattern):**

```yaml
# WRONG: counts only successes (k), not trials (n)
- name: direction_correct
  body: 'if(predicted_direction == actual_direction, 1.0, Null)'  # Null for incorrect

- name: direction_accuracy_lower_95_WRONG
  body: 'wilson_ci_lower(direction_accuracy, count_over(holdout_games, direction_correct))'
  # count_over counts non-null values → only successes are non-null → wrong n!

# RIGHT: count trials separately from successes
- name: direction_correct
  body: 'if(predicted_direction == actual_direction, 1.0, 0.0)'  # 1 or 0, never Null
  # All games get a value (1 for correct, 0 for incorrect); count_over counts all games

- name: direction_accuracy
  body: 'avg_over(holdout_games, direction_correct)'

- name: direction_accuracy_lower_95
  body: 'wilson_ci_lower(direction_accuracy, count_over(holdout_games, direction_correct))'
  # count_over counts non-null direction_correct values → all games → correct n
```

**Convention to document:** "For Wilson CI, ensure your indicator measure (direction_correct, hit, conversion, etc.) returns 1 for success and 0 for failure — NEVER Null for failure. Otherwise `count_over` gives you k, not n."

### Amendment 6: MC1008 reuse confirmed; message includes function name

**Verification.** `crates/mc-model/src/formula.rs` sum_over handler emits:

```rust
return Err(FormulaError::wrong_arg_count(
    call_start,
    "sum_over expects 2 arguments: sum_over(dimension, measure)".into(),
));
```

The function name is in the message text. Reusing MC1008 for `std_over`, `var_over`, `count_over`, `wilson_ci_lower`, `wilson_ci_upper` will produce messages like "std_over expects 2 arguments..." — disambiguation works without per-function MC codes.

**Amendment.** No code change. Implementer reuses MC1008 via the shared `FormulaError::wrong_arg_count` helper. Record the decision in the completion report under "Diagnostic code allocations: reused MC1008 for all 5 primitives per ADR-0031 Amendment 3 + 7 discovery."

### Amendment 7: Wilson MLB headline test tolerance is 0.001 (not 1e-6)

**Problem.** Acceptance criterion 7 references the "Wilson LB 57.18%" claim from claw-core's integration test report. That claim is reported to 4 significant figures (rounded). Asserting 1e-6 precision against a 0.5718 (4-sig-fig) target will fail spuriously.

**Amendment.** Acceptance criterion 7 explicitly uses 1e-3 tolerance (~0.001):

> AC #7 (revised): `wilson_ci_lower(0.5968, 1508)` returns 0.5718 ± 0.001 — matching the rounded headline claim from claw-core's integration test report. Other Wilson fixtures (AC #4, AC #5) use 1e-6 tolerance against scipy values; only the headline-parity test uses 1e-3 because the reference is a rounded human-readable value.

Test code:
```rust
#[test] fn t_wilson_ci_mlb_walk_forward() {
    let lo = wilson_ci_lower_compute(0.5968, 1508.0).unwrap();
    // 0.5718 is the published Wilson LB; 1e-3 tolerance matches its precision
    assert!((lo - 0.5718).abs() < 0.001,
        "Wilson LB 57.18% headline parity failed: got {lo}");
}
```

---

## Additional binding notes (Desktop's strategic determinism observation)

Wilson CIs are deterministic confidence intervals — same input, same output, every time, verifiable. This contributes to Mosaic's strategic positioning around deterministic interpretation with cryptographic provenance (Grout research note + ADR-0009 LNM substrate vision). When the interpretation ledger (Phase 7A.2) logs evaluation runs, every "we beat the market" claim can be bounded by a Wilson CI from the same trained model + same holdout data, producing a reproducible audit trail.

This isn't binding on the implementation — it's context for why this primitive matters beyond the immediate experiment-replacement story.

---

## Acceptance criteria revisions (consolidated)

The following original ACs are superseded:

- **AC #1, #2, #3** (parsing): unchanged
- **AC #4, #5, #6** (Wilson fixtures + complementary invariant): unchanged, 1e-6 tolerance
- **AC #7 (Wilson MLB headline)**: 1e-3 tolerance per Amendment 7
- **AC #8, #9, #10** (Null semantics): unchanged
- **AC #11** (cookbook covers 5 primitives): EXPANDED — cookbook must include the safer Wilson CI pattern (Amendment 5) and use intermediate derived measures for compositional metrics (Amendment 1)
- **AC #12-20**: unchanged
- **AC #21** (demo proof): EXPANDED — cookbook must include a worked claw-core MLB example showing the safer pattern for `direction_accuracy_lower_95`

**New AC #22 (Amendment 3):** `std_over` and `var_over` use sample variance (ddof=1). Test against numpy/statsmodels with `ddof=1`, NOT `ddof=0`.

**New AC #23 (Amendment 2):** `count_over` evaluates the measure at every leaf via the same dispatch path as `sum_over`/`avg_over`. Verified by a test that constructs a cube with a derived measure (not pre-materialized) and confirms `count_over` returns the correct count after triggering eval.

**New AC #24 (Amendment 1):** All cookbook examples for compositional metrics use intermediate derived measures, NOT inline expressions in `_over` calls. Verified by reading the cookbook against the parse rules.

---

*End of amendments. Body of ADR above is preserved for audit-trail purposes; amendments win on conflicts.*
