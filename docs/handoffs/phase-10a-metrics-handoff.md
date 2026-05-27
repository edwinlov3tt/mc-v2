# Phase 10A Handoff — Evaluation Metrics Library

**Status:** Accepted, ready to start
**Date:** 2026-05-27
**ADR:** [ADR-0033](../decisions/0033-phase-10a-evaluation-metrics-library.md) (Accepted with 7 acceptance amendments — read amendments BEFORE the body; amendments win on conflicts)
**Estimated effort:** 1–2 sessions (~150 LOC + ~120 LOC tests + cookbook)
**Crates:** `mc-model` (parser) + `mc-core` (evaluator); no daemon, no kernel-interface changes
**Branch:** `phase-10a/metrics-library`

---

## What this phase ships

Five new formula primitives that unblock the evaluation-primitives track (Phase 10B `grade`, 10C `backtest`, 10D batch `sweep`, 10E `walk-forward`, 10F `simulate`):

- `std_over(dim, measure)` — sample standard deviation (ddof=1)
- `var_over(dim, measure)` — sample variance (ddof=1)
- `count_over(dim, measure)` — count of non-null evaluated values across dim
- `wilson_ci_lower(p, n)` — Wilson 95% CI lower bound
- `wilson_ci_upper(p, n)` — Wilson 95% CI upper bound

Plus `docs/specs/metrics-cookbook.md` — the user-facing surface that documents how to compose direction_accuracy, ROI, Brier, Sharpe, and mean_residual from the new primitives + existing operations.

Hand-rolled, no new dependencies. Mirrors Phase 3L (`nbinom_sf`) precedent verbatim.

---

## Required reading (in this order)

1. **ADR-0033 Amendments (CRITICAL — read first).** All 7 amendments are binding and override the body. The biggest ones:
   - **Amendment 1**: `_over` accepts BARE MEASURES ONLY, not arbitrary expressions. The body's cookbook examples (`avg_over(holdout_games, if(...))`) WILL NOT WORK. Cookbook must use intermediate derived measures.
   - **Amendment 3**: Sample variance (ddof=1) is the default — NOT population variance. Matches numpy/statsmodels/pandas. The body's defense of population variance is wrong for the actual use case (walk-forward backtesting is inferential).
   - **Amendment 2**: `count_over` evaluates the measure at every leaf via the same dispatch as `sum_over`. Mirror that path, do NOT use the `&[ScalarValue]` sketch from the body (it's misleading).
2. **ADR-0033 body** — context, rationale, alternatives (interpret through amendments)
3. **ADR-0031 (Phase 3L) precedent** — the pattern to mirror. Look at the implementation of `nbinom_sf` (commit `b8a858d`):
   - `crates/mc-model/src/formula.rs:933` — parser site (mirror this for Wilson family)
   - `crates/mc-core/src/rule.rs` — `nbinom_sf_compute` (mirror this style for Wilson + Welford)
   - `crates/mc-model/src/schema.rs` — `ParsedRuleBody` + `ParsedNbinomBody` (mirror for `ParsedWilsonBody`)
4. **Existing `_over` family** — what to extend:
   - `crates/mc-model/src/formula.rs` `sum_over` handler — uses `parse_bare_identifier` × 2 (confirms Amendment 1)
   - `OverKind` enum (`mc-core/src/rule.rs` and `mc-model/src/schema.rs`) — add `Std`, `Var`, `Count` variants
   - The dispatch path for `OverKind::Sum` — find it, mirror the new variants alongside
5. **CLAUDE.md** (project root) — §2.5 (Null semantics), §3.1 (forbidden patterns), §6 (self-check gates)

---

## Phase 10A scope

| # | Item |
|---|---|
| 1 | `OverKind::Std`, `OverKind::Var`, `OverKind::Count` variants in mc-core + mc-model |
| 2 | `std_over` / `var_over` / `count_over` parse handlers (mirror `sum_over` shape) |
| 3 | `WilsonCiLower` / `WilsonCiUpper` `ParsedRuleBody` variants + `ParsedWilsonBody` struct |
| 4 | `wilson_ci_lower` / `wilson_ci_upper` parse handlers (mirror `norm_cdf` 2-arg shape) |
| 5 | `std_compute` / `var_compute` (Welford, ddof=1) — shared internal helper |
| 6 | `count_compute` — counts non-null evaluated values |
| 7 | `wilson_ci_lower_compute` / `wilson_ci_upper_compute` — closed-form, hand-coded z constant |
| 8 | Eval dispatch — extend OverKind match + add Wilson arms (mirror norm_cdf) |
| 9 | `crates/mc-core/tests/metrics.rs` — fixture tests + invariant tests |
| 10 | `crates/mc-core/tests/metrics_fixtures.py` — Python regen script with statsmodels/numpy pinned |
| 11 | `docs/specs/metrics-cookbook.md` — user-facing cookbook with safer patterns |
| 12 | JSON schema regenerated per Phase 3K |
| 13 | All build gates pass |

**Out of scope (do NOT implement):**
- `max_drawdown`, `recovery_bets` (deferred to Phase 10F)
- `sharpe_ratio` as native primitive (compositional via cookbook)
- `std_pop_over` / `var_pop_over` (deferred to demand)
- Configurable Wilson confidence level (`level` as third arg — deferred)
- Bootstrap CI / Clopper-Pearson / Agresti-Coull (deferred)
- The five `mc model` commands themselves (10B–10F, separate ADRs)
- Extending `_over` to accept arbitrary expressions (separate phase if/when needed)

---

## Pre-flight checklist (before writing any code)

Run these and report results in chat before Step 1:

```bash
# 1. Verify ADR-0033 amendments understood (re-read the amendments section)

# 2. Diagnostic code reuse confirmed (Amendment 6)
grep -n "wrong_arg_count" crates/mc-model/src/formula.rs | head -5
# Confirm: existing helper emits MC1008 with function name in message text.

# 3. Verify _over parse uses bare identifiers (Amendment 1 grounds)
grep -A 12 '"sum_over" => {' crates/mc-model/src/formula.rs | head -15
# Confirm: parse_bare_identifier(...) for both args, NOT parse_or_expression.

# 4. Verify Null arithmetic (Amendment 4 grounds)
grep -n "Null poisons" crates/mc-core/src/rule.rs
# Expected: line ~1943 documents Null * X = Null for any X.

# 5. Find OverKind enum location
grep -RE "enum OverKind" crates/mc-core/src/ crates/mc-model/src/
# Expected: definition in mc-core/src/rule.rs (eval AST) AND mc-model/src/schema.rs (parse AST)
# — Phase 3L surfaced this two-layer architecture.

# 6. Verify JsonSchema derive on existing OverKind (Phase 3K requirement)
grep -B 2 "enum OverKind" crates/mc-model/src/schema.rs
# Expected: #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]

# 7. scipy/statsmodels for fixture generation
python3 -c "import statsmodels; print('statsmodels', statsmodels.__version__)"
python3 -c "import numpy; print('numpy', numpy.__version__)"
# Need statsmodels >= 0.14 for stable Wilson CI; numpy any recent.

# 8. Clean working tree
git worktree add ../mc-v2-phase-10a -b phase-10a/metrics-library main
cd ../mc-v2-phase-10a
git status
```

Record: working tree state, JsonSchema derive status, statsmodels version. Surface in chat before Step 1.

---

## Implementation path

### Step 1: Regenerate fixtures with Python

**Create:** `crates/mc-core/tests/metrics_fixtures.py`

```python
"""
Reference values for ADR-0033 metrics primitives.
Pinned to statsmodels/numpy versions recorded below.
Run: python3 metrics_fixtures.py
Paste the output into crates/mc-core/tests/metrics.rs doc comment.

DO NOT EDIT INDIVIDUAL VALUES — regenerate the whole table.
"""
import statsmodels
from statsmodels.stats.proportion import proportion_confint
import numpy as np

print(f"# Generated by metrics_fixtures.py")
print(f"# statsmodels {statsmodels.__version__}; numpy {np.__version__}")
print(f"# Wilson 95% via proportion_confint(method='wilson')")
print(f"# std/var via numpy with ddof=1 (sample variance per ADR-0033 Amendment 3)")
print()

# Wilson CI fixtures
print("## Wilson CI fixtures")
print("| p      | n     | lower            | upper            | note |")
print("|--------|-------|------------------|------------------|------|")
wilson_cases = [
    (0.5,    100,   "balanced n=100"),
    (0.6,    100,   "moderate edge n=100"),
    (0.5968, 1508,  "MLB walk-forward — V1.0+NB headline (tol 1e-3 vs 0.5718)"),
    (0.0,    100,   "degenerate p=0"),
    (1.0,    100,   "degenerate p=1"),
    (0.5,    1,     "tiny n=1"),
    (0.4,    100,   "for complementary invariant test"),
]
for p, n, note in wilson_cases:
    k = int(round(p * n))
    lo, hi = proportion_confint(k, n, alpha=0.05, method='wilson')
    print(f"| {p:.4f} | {n:>5} | {lo:.9f}      | {hi:.9f}      | {note} |")

# Sample std/var fixtures (ddof=1)
print("\n## std/var fixtures (sample, ddof=1)")
print("| values                              | mean        | std (ddof=1) | var (ddof=1) |")
print("|-------------------------------------|-------------|--------------|--------------|")
std_cases = [
    [1.0, 2.0, 3.0, 4.0, 5.0],
    [0.55, 0.62, 0.48, 0.71, 0.53, 0.58],  # MLB-shaped P_Over values
    [0.0, 0.0, 0.0, 0.0, 1.0],             # mostly zeros (sparse)
    [1.0, 1.0, 1.0, 1.0, 1.0],             # all same value
]
for vals in std_cases:
    arr = np.array(vals)
    print(f"| {vals} | {arr.mean():.9f} | {arr.std(ddof=1):.9f} | {arr.var(ddof=1):.9f} |")
```

Run it, paste the output table into the doc comment of `crates/mc-core/tests/metrics.rs`. Commit the `.py` file alongside the test.

### Step 2: AST extensions

**File:** `crates/mc-model/src/schema.rs` (the parse AST)

Add to `OverKind` enum:
```rust
Std,
Var,
Count,
```

Add `ParsedWilsonBody`:
```rust
#[derive(Clone, Debug)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ParsedWilsonBody {
    pub p: Box<ParsedExpr>,
    pub n: Box<ParsedExpr>,
}
```

Add to `ParsedRuleBody`:
```rust
WilsonCiLower(ParsedWilsonBody),
WilsonCiUpper(ParsedWilsonBody),
```

**File:** `crates/mc-core/src/rule.rs` (the eval AST) — mirror the additions if `OverKind`/`ParsedRuleBody` is duplicated there (Phase 3L noted this two-layer architecture; check both files).

### Step 3: Parser handlers

**File:** `crates/mc-model/src/formula.rs` — add to the `_over` family parse switch (around the existing `sum_over` site):

```rust
"std_over" => parse_simple_over(self, "std_over", call_start, OverKind::Std),
"var_over" => parse_simple_over(self, "var_over", call_start, OverKind::Var),
"count_over" => parse_simple_over(self, "count_over", call_start, OverKind::Count),
```

Verify `parse_simple_over` already exists (the existing avg_over/min_over/max_over use it). If `sum_over` uses inline parsing instead, mirror that pattern for the new variants — whichever the existing code uses for consistency.

Around the `norm_cdf` parse site (line ~933), add the Wilson family — mirror `norm_cdf`'s 2-arg parse with `parse_or_expression` for both args (Wilson p and n are arbitrary numeric expressions, not bare identifiers):

```rust
"wilson_ci_lower" => {
    let args = self.parse_arg_list()?;
    self.expect_close_paren("wilson_ci_lower")?;
    if args.len() != 2 {
        return Err(FormulaError::wrong_arg_count(
            call_start,
            format!("wilson_ci_lower expects 2 arguments (p, n), got {}", args.len()),
        ));
    }
    let [p, n] = take2(args);
    Ok(ParsedRuleBody::WilsonCiLower(ParsedWilsonBody {
        p: Box::new(p), n: Box::new(n),
    }))
}
"wilson_ci_upper" => { /* same shape */ }
```

Match `nbinom_sf` parse error message format verbatim (the wrong_arg_count helper emits MC1008 with the message text — Amendment 6).

### Step 4: Compute helpers

**File:** `crates/mc-core/src/rule.rs` (near `nbinom_sf_compute`):

```rust
/// Welford's single-pass sample variance (ddof=1) per ADR-0033 Amendment 3.
/// Returns None for fewer than 2 non-NaN values.
fn var_compute(values: &[f64]) -> Option<f64> {
    let mut k = 0.0_f64;
    let mut mean = 0.0_f64;
    let mut m2 = 0.0_f64;
    for &v in values.iter().filter(|v| !v.is_nan()) {
        k += 1.0;
        let delta = v - mean;
        mean += delta / k;
        let delta2 = v - mean;
        m2 += delta * delta2;
    }
    if k < 2.0 { return None; }
    Some(m2 / (k - 1.0))  // ddof=1, sample variance
}

fn std_compute(values: &[f64]) -> Option<f64> {
    var_compute(values).map(f64::sqrt)
}

/// Wilson 95% CI lower bound. Returns None for invalid inputs.
/// Per ADR-0033 Decision 2 and Amendment 2 Null semantics.
fn wilson_ci_lower_compute(p: f64, n: f64) -> Option<f64> {
    if p.is_nan() || n.is_nan() { return None; }
    if n <= 0.0 || !(0.0..=1.0).contains(&p) { return None; }
    const Z: f64 = 1.959963984540054;
    let z2 = Z * Z;
    let denom = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / denom;
    let margin = Z * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt() / denom;
    Some((center - margin).clamp(0.0, 1.0))
}

fn wilson_ci_upper_compute(p: f64, n: f64) -> Option<f64> {
    // Same as lower but `+ margin` at the end
    // Refactor to a shared helper that returns (lower, upper) tuple if it
    // reduces duplication — both reviewers would appreciate that pattern.
    ...
}
```

### Step 5: Eval dispatch — `count_over` semantics (Amendment 2)

**File:** `crates/mc-core/src/rule.rs` — extend the `OverKind` dispatch (find the existing match arm for `OverKind::Sum` and add the three new variants).

CRITICAL (Amendment 2): `count_over` MUST evaluate the measure at every leaf, NOT count cells from the store. Mirror `sum_over`'s dispatch path. The accumulator changes:

```rust
// Sum: accumulator is a running f64 sum
// Avg: accumulator is (running_sum, count)
// Count: accumulator is just count of non-null evaluations
// Std/Var: collect non-null values into a Vec<f64>, pass to std_compute/var_compute

OverKind::Count => {
    // Same per-leaf eval as Sum/Avg; just count non-null results
    let mut count = 0_u64;
    for leaf in dim_leaves(...) {
        let value = eval_measure_at(leaf, ...)?;  // mirror sum_over's eval call
        if !matches!(value, ScalarValue::Null) {
            count += 1;
        }
    }
    Ok(ScalarValue::F64(count as f64))  // returns Some(0.0) for empty scope
}

OverKind::Std => {
    let mut values = Vec::new();
    for leaf in dim_leaves(...) {
        if let ScalarValue::F64(v) = eval_measure_at(leaf, ...)? {
            values.push(v);
        }
    }
    match std_compute(&values) {
        Some(s) => Ok(ScalarValue::F64(s)),
        None => Ok(ScalarValue::Null),  // <2 values → Null per Decision 3
    }
}
```

Wilson dispatch is simpler — both args evaluate to scalars, pass to compute helper, map `Option<f64>` → `ScalarValue`:

```rust
ParsedRuleBody::WilsonCiLower(body) => {
    let p = eval_expr(&body.p, ctx)?;
    let n = eval_expr(&body.n, ctx)?;
    let (ScalarValue::F64(p_val), ScalarValue::F64(n_val)) = (p, n) else {
        return Ok(ScalarValue::Null);  // non-numeric args → Null
    };
    match wilson_ci_lower_compute(p_val, n_val) {
        Some(v) => Ok(ScalarValue::F64(v)),
        None => Ok(ScalarValue::Null),
    }
}
```

### Step 6: Tests

**File:** `crates/mc-core/tests/metrics.rs` (new)

Paste the Amendment 1 fixture table from Step 1's Python output into the doc comment. Then:

```rust
//! Tests for evaluation metrics primitives — ADR-0033.
//! Reference values generated by tests/metrics_fixtures.py against
//! statsmodels <VERSION> and numpy <VERSION>.
//!
//! Tolerance: 1e-6 for scipy/statsmodels fixtures
//! EXCEPTION: MLB walk-forward headline parity uses 1e-3 (4-sig-fig precision
//! of the published "Wilson LB 57.18%" claim) — see ADR-0033 Amendment 7.
//!
//! [paste fixture table here]

use mc_core::rule::*;

const TOL_FIXTURE: f64 = 1e-6;
const TOL_HEADLINE: f64 = 1e-3;

#[test] fn t_wilson_ci_balanced_n100() {
    let lo = wilson_ci_lower_compute(0.5, 100.0).unwrap();
    let hi = wilson_ci_upper_compute(0.5, 100.0).unwrap();
    // From statsmodels: lo=0.402642036, hi=0.597357964
    assert!((lo - 0.402642036).abs() < TOL_FIXTURE);
    assert!((hi - 0.597357964).abs() < TOL_FIXTURE);
}

#[test] fn t_wilson_ci_mlb_walk_forward_headline() {
    // Per Amendment 7: 1e-3 tolerance for headline parity claim
    let lo = wilson_ci_lower_compute(0.5968, 1508.0).unwrap();
    assert!((lo - 0.5718).abs() < TOL_HEADLINE,
        "Wilson LB 57.18% headline parity failed: got {lo}");
}

#[test] fn t_wilson_ci_complementary() {
    // wilson_lower(p, n) + wilson_upper(1-p, n) == 1.0
    let lo = wilson_ci_lower_compute(0.4, 100.0).unwrap();
    let hi = wilson_ci_upper_compute(0.6, 100.0).unwrap();
    assert!((lo + hi - 1.0).abs() < 1e-9);
}

#[test] fn t_wilson_ci_degenerate_p0() {
    let lo = wilson_ci_lower_compute(0.0, 100.0).unwrap();
    let hi = wilson_ci_upper_compute(0.0, 100.0).unwrap();
    assert!((lo - 0.0).abs() < TOL_FIXTURE);
    assert!(hi > 0.0 && hi < 0.05);  // small upper bound, not zero
}

#[test] fn t_wilson_ci_degenerate_p1() { /* mirror p0 */ }

#[test] fn t_wilson_ci_invalid_returns_null() {
    // All invalid inputs return None (mapped to ScalarValue::Null at dispatch)
    assert!(wilson_ci_lower_compute(0.5, 0.0).is_none());      // n=0
    assert!(wilson_ci_lower_compute(0.5, -1.0).is_none());     // n<0
    assert!(wilson_ci_lower_compute(-0.1, 100.0).is_none());   // p<0
    assert!(wilson_ci_lower_compute(1.1, 100.0).is_none());    // p>1
    assert!(wilson_ci_lower_compute(f64::NAN, 100.0).is_none());
    assert!(wilson_ci_lower_compute(0.5, f64::NAN).is_none());
}

#[test] fn t_std_var_sample_default() {
    // Per Amendment 3: ddof=1 sample variance
    let vals = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    // numpy var(ddof=1) = 2.5; std(ddof=1) = ~1.5811388300841898
    let v = var_compute(&vals).unwrap();
    let s = std_compute(&vals).unwrap();
    assert!((v - 2.5).abs() < TOL_FIXTURE);
    assert!((s - 1.5811388300841898).abs() < TOL_FIXTURE);
}

#[test] fn t_var_n_less_than_2_returns_null() {
    assert!(var_compute(&[]).is_none());
    assert!(var_compute(&[1.0]).is_none());  // n=1 → undefined for sample variance
}

#[test] fn t_var_skips_nan() {
    let vals = vec![1.0, 2.0, f64::NAN, 3.0, 4.0, 5.0];
    // Should compute over the 5 non-NaN values, same as t_std_var_sample_default
    let v = var_compute(&vals).unwrap();
    assert!((v - 2.5).abs() < TOL_FIXTURE);
}

// count_over tests require integration with the cube — see Step 7 (integration test)
```

**Integration test for `count_over` (Amendment 2 requires this):**

```rust
// crates/mc-core/tests/count_over_evaluation.rs (new)
//
// Verifies that count_over evaluates the measure at every leaf, not just
// counts materialized store entries. Per ADR-0033 Amendment 2.

#[test] fn t_count_over_evaluates_derived_measures() {
    // Build a cube with:
    //  - dim "games" with N leaves
    //  - input measure "x" (some games null, some have values)
    //  - derived measure "y" = if(x > 0, 1.0, Null)  (computed, never stored)
    //  - rule that uses count_over(games, y)
    //
    // Assert: count_over returns the correct count of evaluated non-null
    // y values, NOT the count of materialized x values.

    // [full test setup]
}
```

Plus parser-level tests (wrong_arg_count → MC1008, correct parse-to-variant) for all 5 new functions.

### Step 7: Metrics cookbook (Amendment 1 + 5 binding)

**Create:** `docs/specs/metrics-cookbook.md`

CRITICAL: use intermediate derived measures, NOT inline expressions in `_over`. The Amendment 1 verification proved `_over` accepts bare measures only.

Outline:

```markdown
# Metrics Cookbook — Evaluation Patterns for Mosaic Cubes

How to express the 10 standard evaluation metrics from claw-core's
experiment scripts (and similar workflows) using Mosaic's formula
language. Five of the 10 use new Phase 10A primitives; five are
compositional via existing primitives.

## Important conventions

**1. `_over` aggregations accept bare measure names only, not expressions.**
For metrics that need an expression (e.g., "1 if correct, else 0"),
declare an intermediate derived measure first, then aggregate over it.

**2. `wilson_ci_*` is ONLY for binomial proportions.** Do not use for
ROI, Sharpe, or arbitrary value bounds — see the Wilson section below.

**3. `count_over` evaluates the measure at every leaf.** Performance
is O(N_leaves × eval_cost_of_measure), same as `sum_over`/`avg_over`.

---

## 1. direction_accuracy

How often the model's predicted direction (over/under) matched the
actual outcome.

```yaml
- name: direction_correct
  body: 'if(predicted_direction == actual_direction, 1.0, 0.0)'
  scope: Leaf
  # IMPORTANT: returns 1.0 or 0.0 (never Null) — this is load-bearing
  # for count_over to count trials (n), not successes (k).

- name: direction_accuracy
  body: 'avg_over(holdout_games, direction_correct)'
```

## 2. direction_accuracy with 95% Wilson CI

```yaml
# direction_correct as above (1.0 or 0.0, never Null)

- name: direction_accuracy_lower_95
  body: 'wilson_ci_lower(direction_accuracy, count_over(holdout_games, direction_correct))'
  # count_over counts all non-null values of direction_correct = all games.
  # This is the trial count (n), which is what Wilson CI needs.

- name: direction_accuracy_upper_95
  body: 'wilson_ci_upper(direction_accuracy, count_over(holdout_games, direction_correct))'
```

WRONG PATTERN (footgun — produces incorrect n):
```yaml
- name: direction_correct_with_null  # WRONG SHAPE
  body: 'if(predicted_direction == actual_direction, 1.0, Null)'
  # Returns Null for incorrect predictions.

- name: direction_accuracy_lower_95_BROKEN
  body: 'wilson_ci_lower(direction_accuracy, count_over(holdout_games, direction_correct_with_null))'
  # count_over counts only the successes (non-null = 1.0) = k, not n.
  # Wilson gets the wrong n → wrong CI → silently incorrect output.
```

## 3. ROI (return on investment)

```yaml
- name: bet_pnl
  body: 'if(direction_correct == 1.0, stake * (decimal_odds - 1.0), -stake)'

- name: roi
  body: 'sum_over(holdout_games, bet_pnl) / sum_over(holdout_games, stake)'
```

(ROI ranges outside [0, 1]. DO NOT pass to Wilson CI.)

## 4. Brier score

```yaml
- name: brier_error
  body: 'pow(predicted_prob - outcome, 2.0)'

- name: brier_score
  body: 'avg_over(holdout_games, brier_error)'
```

## 5. Sharpe ratio (two forms)

```yaml
# Concise form (relies on Null propagation: any Null in arithmetic → Null)
- name: sharpe_ratio
  body: 'avg_over(holdout_games, returns) / std_over(holdout_games, returns) * sqrt(count_over(holdout_games, returns))'

# Explicit form (intent visible; same behavior)
- name: sharpe_ratio_guarded
  body: 'if(count_over(holdout_games, returns) >= 2, avg_over(holdout_games, returns) / std_over(holdout_games, returns) * sqrt(count_over(holdout_games, returns)), Null)'
```

Both produce identical output (`Null * 0 = Null` per `rule.rs:1943`).
Choose by style preference.

## 6. mean_residual

```yaml
- name: residual
  body: 'actual_total - predicted_total'

- name: mean_residual
  body: 'avg_over(holdout_games, residual)'
```

## 7. n_bets (count after filter)

```yaml
- name: is_eligible_bet
  body: 'if(abs(edge_pct) >= 0.10, 1.0, 0.0)'

- name: n_bets
  body: 'count_over(holdout_games, is_eligible_bet)'
  # Note: count_over counts non-null, so this counts ALL games (since
  # is_eligible_bet is always 1.0 or 0.0). To count ONLY eligible:
  #   n_eligible: sum_over(holdout_games, is_eligible_bet)
```

## 8. (NEW Phase 10A) std_over and var_over

```yaml
- name: prediction_std
  body: 'std_over(holdout_games, predicted_total)'
  # Sample standard deviation (ddof=1) per ADR-0033 Amendment 3.

- name: prediction_var
  body: 'var_over(holdout_games, predicted_total)'
```

If you need population variance (rare — typically only for non-inferential
descriptive statistics), file a feature request — `std_pop_over` /
`var_pop_over` are deferred to demand-driven follow-up.

---

## Wilson CI: ONLY for binomial proportions

`wilson_ci_lower(p, n)` and `wilson_ci_upper(p, n)` apply specifically
to observed binomial proportions: k successes out of n independent
trials with `p = k/n`.

**USE for:**
- Win rate (direction accuracy)
- Conversion rate
- Hit rate
- Any rate that's k/n where k ∈ {0, ..., n}

**DO NOT USE for:**
- ROI (can be negative or > 1)
- Sharpe ratio
- Mean residual
- Expected value
- PnL or other dollar quantities

If `p` falls outside [0, 1], Wilson returns Null (silent — no error
diagnostic). For arbitrary-bound CIs, bootstrap CI primitives are
deferred to a future phase.

---

## Demo proof: claw-core MLB cartridge in Mosaic-native form

Per the integration test report, claw-core's V1.0+NB MLB cartridge
clears the betting gate at 59.68% win rate (n=1508 bets, walk-forward
2023-2025). The Wilson lower bound at 95% is 57.18% — the published
defensible claim.

In Mosaic-native form:

```yaml
# Per-bet outcome (1.0 for correct, 0.0 for incorrect)
- name: direction_correct
  body: 'if(predicted_direction == actual_direction, 1.0, 0.0)'

# Aggregate metrics
- name: direction_accuracy
  body: 'avg_over(walk_forward_bets, direction_correct)'

- name: n_bets
  body: 'count_over(walk_forward_bets, direction_correct)'

# 95% CI on the win rate — the load-bearing inferential claim
- name: direction_accuracy_lower_95
  body: 'wilson_ci_lower(direction_accuracy, n_bets)'

- name: direction_accuracy_upper_95
  body: 'wilson_ci_upper(direction_accuracy, n_bets)'
```

That's the entire EXP-028 walk-forward statistic emission, expressed
as 5 cube rules. The original Python script was ~300 lines.
```

### Step 8: JSON schema regeneration (Phase 3K)

```bash
cargo run --bin mc-model-schema > docs/specs/mosaic-model-schema.json
git diff docs/specs/mosaic-model-schema.json
# Verify: new ParsedRuleBody variants appear; new OverKind variants appear.
# CI drift check should pass after this.
```

### Step 9: Build gates (CLAUDE.md §6)

```bash
cargo fmt --check --all
cargo clippy --all-targets --workspace -- -D warnings
cargo build --release --workspace
cargo test --workspace
grep -rn "\.unwrap()\|\.expect(\|panic!(\|unimplemented!(\|todo!(" crates/mc-core/src/
# Last grep: only test/bench matches acceptable
```

---

## Acceptance gate (binding — body + 7 amendments)

Implementer reports each of these explicitly when claiming done:

**Body-level (from ADR-0033 §"Acceptance criteria"):**
- [ ] AC #1-3: 3 _over primitives parse correctly; MC1008 on wrong arg count
- [ ] AC #4: Wilson lower matches statsmodels within 1e-6 on 6 fixtures
- [ ] AC #5: Wilson upper matches statsmodels within 1e-6
- [ ] AC #6: Complementary invariant `wilson_lower(p) + wilson_upper(1-p) = 1.0` (within 1e-9)
- [ ] AC #8: Invalid inputs return Null (n≤0, p<0, p>1, NaN any arg)
- [ ] AC #9: `std_over` returns Null for n<2 valid values
- [ ] AC #10: `count_over` returns 0 for empty scope (not Null)
- [ ] AC #12: JSON schema regenerated; CI drift check passes
- [ ] AC #13: `metrics_fixtures.py` committed with statsmodels + numpy versions pinned in header
- [ ] AC #14: Diagnostic code allocation recorded (MC1008 reused — Amendment 6)
- [ ] AC #15: No new external dependencies
- [ ] AC #16-19: All existing tests pass; cargo test/clippy/fmt clean
- [ ] AC #20: New ParsedRuleBody variants derive JsonSchema

**Amendment-driven (from ADR-0033 §"Acceptance amendments"):**
- [ ] AC #7 (Amendment 7): Wilson MLB headline parity `wilson_ci_lower(0.5968, 1508) ≈ 0.5718 ± 0.001`
- [ ] AC #11 (Amendments 1 + 5): Cookbook uses intermediate derived measures; safer Wilson pattern documented; proportion-only guidance prominent
- [ ] AC #21 (Amendments 1 + 5): Cookbook includes claw-core MLB demo proof showing the safer pattern
- [ ] AC #22 (Amendment 3): `std_over`/`var_over` use sample variance ddof=1, NOT population
- [ ] AC #23 (Amendment 2): `count_over` evaluates measure at every leaf — verified by integration test
- [ ] AC #24 (Amendment 1): All cookbook examples use intermediate measures (no inline expressions in `_over`)

---

## Effort and shape

- ~150 LOC compute + parser + AST extensions
- ~120 LOC tests (8 unit tests + 2-3 parser integration tests + 1 count_over integration test)
- ~30 LOC Python regen script
- ~300 LOC metrics cookbook
- ~1-2 sessions including build-gate self-check

Same shape as Phase 3L (`nbinom_sf`) but slightly broader scope (5 primitives vs 2).

---

## Common pitfalls (forewarned, forearmed)

1. **Implementing the cookbook from the body of the ADR.** The body's examples use inline expressions in `_over` (e.g., `avg_over(games, if(...))`). Those WILL NOT PARSE. Use Amendment 1's rewrite with intermediate derived measures.
2. **Shipping `std_over` as population variance.** The body's defense is wrong for the actual use case. Amendment 3 flips it to sample variance (ddof=1). Read Amendment 3 BEFORE writing `var_compute`.
3. **Implementing `count_over` with `&[ScalarValue]` from the body's sketch.** That sketch suggests counting materialized cells; Amendment 2 requires evaluating the measure at every leaf via the same dispatch as `sum_over`. Mirror sum_over's eval call exactly.
4. **Allocating new MC codes for wrong_arg_count.** Reuse MC1008 via the existing helper (Amendment 6). The function name is in the message text — disambiguation works.
5. **Writing inline Wilson tests with 1e-6 tolerance for the MLB headline test.** Amendment 7 sets that test to 1e-3 (the "57.18%" is 4-sig-fig precision of a rounded value).
6. **Forgetting `#[derive(JsonSchema)]` on `ParsedWilsonBody`.** Phase 3K's schema drift check will fail silently otherwise — new variants won't appear in the published schema.
7. **Cookbook Wilson example using `count_over` on a Null-on-failure measure.** Subtle bug; counts only successes (k), not trials (n). Amendment 5 shows the safer pattern; cookbook must use it.
8. **Adding `sharpe_ratio` as a native primitive.** Out of scope. Compositional via cookbook (which already shows two forms).

---

## Cross-links

- ADR-0033: [`../decisions/0033-phase-10a-evaluation-metrics-library.md`](../decisions/0033-phase-10a-evaluation-metrics-library.md)
- Research notes:
  - [`../research-notes/built-in-evaluation-primitives.md`](../research-notes/built-in-evaluation-primitives.md) — the 5 commands this unblocks
  - [`../research-notes/pymc-marketing-pattern-extraction.md`](../research-notes/pymc-marketing-pattern-extraction.md) — companion Bayesian track (separate)
- Phase 3L precedent (the pattern to mirror):
  - ADR: [`../decisions/0031-nbinom-sf-formula-function.md`](../decisions/0031-nbinom-sf-formula-function.md)
  - Handoff: [`./phase-3l-nbinom-sf-handoff.md`](./phase-3l-nbinom-sf-handoff.md)
  - Implementation: commit `b8a858d` merged at `1ec2c06`
- claw-core integration test (the consumer):
  - `claw-core/docs/reports/mosaic-integration-test.md` — Wilson LB 57.18% headline
  - V1.0+NB walk-forward (EXP-028) — 1508 bets, 59.68% WR

---

## Completion report template

When done, write `docs/reports/phase-10a-completion-report.md` covering:

1. Final MC diagnostic code: confirmed MC1008 reuse via shared helper
2. Test count + pass status (workspace-wide)
3. Build gate results (fmt, clippy, build, test, grep)
4. Tolerance deviations (if any) — fixtures that needed widened tolerance
5. statsmodels + numpy versions pinned in metrics_fixtures.py header
6. Cookbook accepted as written or amended during implementation
7. Cookbook safer-pattern coverage (verify Amendment 5 + 1 examples are present)
8. JsonSchema drift-check status
9. Effort actual vs estimate (1-2 sessions)
10. Recommended next phase based on consumer demand signal (10B grade, 10C backtest, or 10D batch sweep)
