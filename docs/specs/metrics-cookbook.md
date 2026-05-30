# Metrics Cookbook — Evaluation Patterns for Mosaic Cubes

How to express the standard evaluation metrics from claw-core's
experiment scripts (and similar inferential workflows) using Mosaic's
formula language. Five of the ten patterns below use Phase 10A
primitives (`std_over`, `var_over`, `count_over`, `wilson_ci_lower`,
`wilson_ci_upper`); the rest compose from existing operations
(`avg_over`, `sum_over`, arithmetic).

This cookbook is the **user-facing surface for ADR-0033**.

---

## Important conventions

These three rules are load-bearing — every example below depends on them.

### 1. `_over` aggregations accept BARE measure names only — not expressions

Per ADR-0033 Amendment 1, all `_over` functions (`sum_over`, `avg_over`,
`min_over`, `max_over`, `std_over`, `var_over`, `count_over`) take exactly
two bare identifiers: a measure name and a dimension name. They **do
not** accept inline expressions in the measure slot.

For metrics that need an expression (e.g., "1 if correct, else 0"),
declare an intermediate **derived measure** first, then aggregate over
it. Every example in this cookbook follows this pattern.

```yaml
# WRONG — does not parse:
- name: direction_accuracy
  body: 'avg_over(if(predicted == actual, 1, 0), holdout_games)'

# RIGHT — intermediate derived measure:
- name: direction_correct
  body: 'if(predicted_direction == actual_direction, 1.0, 0.0)'

- name: direction_accuracy
  body: 'avg_over(direction_correct, holdout_games)'
```

If you find yourself wanting to inline an expression, declare it as a
derived measure. The dependency graph handles the indirection cleanly.

### 2. Argument order: `<func>(measure, dim)` for the avg-family

Per the existing parser convention (`crates/mc-model/src/formula.rs`
`parse_simple_over`), the order for the `_over` family is **measure
first, dimension second**:

```yaml
avg_over(direction_correct, holdout_games)   # measure=direction_correct, dim=holdout_games
std_over(predicted_total, holdout_games)
count_over(direction_correct, holdout_games)
```

`sum_over` is the historical exception — it accepts `(dimension, measure)`
in the opposite order. This is a pre-existing inconsistency that
Phase 10A does NOT change; the new primitives match the avg-family
convention because they all share the `parse_simple_over` helper.

### 3. `wilson_ci_*` is ONLY for binomial proportions

`wilson_ci_lower(p, n)` and `wilson_ci_upper(p, n)` apply specifically
to observed binomial proportions: `k` successes out of `n` independent
trials with `p = k/n`. Passing arbitrary numerics (ROI, Sharpe, residuals)
silently returns Null when `p` falls outside `[0, 1]`. See the dedicated
section below.

### 4. `count_over` evaluates the measure at every leaf

Per ADR-0033 Amendment 2, `count_over` invokes the measure's rule (for
Derived measures) or reads the cell (for Input measures) at every leaf
under the named dimension and counts non-Null results. Performance is
O(N_leaves × eval_cost_of_measure) — same as `sum_over` and `avg_over`.

This means `count_over` of a Derived measure that returns 1.0/0.0
(never Null) counts ALL leaves, not just the ones that evaluate to 1.0.
To count only the 1.0s, use `sum_over` instead. The Wilson-CI section
shows why this distinction matters.

---

## 1. direction_accuracy

How often the model's predicted direction matched the actual outcome.

```yaml
- name: direction_correct
  body: 'if(predicted_direction == actual_direction, 1.0, 0.0)'
  scope: Leaf
  # 1.0 or 0.0 — NEVER Null for a defined prediction. This is
  # load-bearing for the Wilson-CI pattern below (count_over of
  # direction_correct = n, the trial count).

- name: direction_accuracy
  body: 'avg_over(direction_correct, holdout_games)'
```

## 2. direction_accuracy with 95% Wilson CI

The Wilson interval is the right CI choice for binomial proportions:
it has the correct coverage even near `p=0` / `p=1` and at small `n`,
where the normal approximation breaks down.

```yaml
# direction_correct as above (1.0 or 0.0, never Null)

# direction_accuracy as above

- name: direction_accuracy_lower_95
  body: 'wilson_ci_lower(direction_accuracy, count_over(direction_correct, holdout_games))'
  # count_over counts non-Null direction_correct values = ALL games
  # (because direction_correct is 1.0 or 0.0, never Null). That's the
  # trial count n, which is what Wilson needs.

- name: direction_accuracy_upper_95
  body: 'wilson_ci_upper(direction_accuracy, count_over(direction_correct, holdout_games))'
```

### Common footgun: `count_over` on a Null-on-failure indicator

```yaml
# WRONG — direction_correct_with_null is Null for incorrect predictions
- name: direction_correct_with_null
  body: 'if(predicted_direction == actual_direction, 1.0, Null)'

- name: direction_accuracy_lower_95_BROKEN
  body: 'wilson_ci_lower(direction_accuracy, count_over(direction_correct_with_null, holdout_games))'
  # count_over only counts non-Null values = only successes = k.
  # Wilson gets k where it needs n. Silently wrong narrower CI.
```

**Convention to follow:** indicator measures used as Wilson-CI inputs
must return `1.0` for success and `0.0` for failure — never Null. That
way `count_over` returns `n` (trial count), and Wilson's denominator is
correct.

## 3. ROI (return on investment)

```yaml
- name: bet_pnl
  body: 'if(direction_correct == 1.0, stake * (decimal_odds - 1.0), -stake)'

- name: roi
  body: 'sum_over(holdout_games, bet_pnl) / sum_over(holdout_games, stake)'
  # sum_over uses (dim, measure) — historical exception, see §2 above.
```

ROI can be negative or > 1; **do NOT** pass it to Wilson CI. See the
Wilson-CI guardrails section.

## 4. Brier score

Mean squared error between predicted probability and outcome.

```yaml
- name: brier_error
  body: 'pow(predicted_prob - outcome, 2.0)'

- name: brier_score
  body: 'avg_over(brier_error, holdout_games)'
```

## 5. Sharpe ratio

Two equivalent formulations — choose by style.

```yaml
# Concise — relies on Null propagation
- name: sharpe_ratio
  body: 'avg_over(returns, holdout_games) / std_over(returns, holdout_games) * sqrt(count_over(returns, holdout_games))'

# Explicit — same behavior, intent visible
- name: sharpe_ratio_guarded
  body: 'if(count_over(returns, holdout_games) >= 2, avg_over(returns, holdout_games) / std_over(returns, holdout_games) * sqrt(count_over(returns, holdout_games)), Null)'
```

Both produce identical output. When `count_over < 2`, `std_over` returns
Null (sample variance undefined), and Null * X = Null per
`crates/mc-core/src/rule.rs:1943`, so the concise form's outer
multiplication poisons to Null cleanly. The explicit form is preferred
when the intent matters for readers.

## 6. mean_residual

```yaml
- name: residual
  body: 'actual_total - predicted_total'

- name: mean_residual
  body: 'avg_over(residual, holdout_games)'
```

## 7. n_bets (trial count after eligibility filter)

```yaml
- name: is_eligible_bet
  body: 'if(abs(edge_pct) >= 0.10, 1.0, 0.0)'

# Number of games scanned (all leaves get a 1.0/0.0 value)
- name: n_games_evaluated
  body: 'count_over(is_eligible_bet, holdout_games)'

# Number of actually eligible bets (sum the indicator)
- name: n_eligible
  body: 'sum_over(holdout_games, is_eligible_bet)'
```

`count_over` and `sum_over` of a 1.0/0.0 indicator answer **different
questions**: count gives the denominator (games considered), sum gives
the numerator (games passing the filter).

## 8. (NEW Phase 10A) std_over and var_over

```yaml
- name: prediction_std
  body: 'std_over(predicted_total, holdout_games)'
  # Sample standard deviation (ddof=1) per ADR-0033 Amendment 3.

- name: prediction_var
  body: 'var_over(predicted_total, holdout_games)'
```

Per ADR-0033 Amendment 3, both functions use **sample variance
(ddof=1)** — the divisor is `n-1`, not `n`. This matches numpy,
statsmodels, pandas, and scipy defaults and is the correct estimator
for the inferential workflows that motivated Phase 10A.

If you need population variance for non-inferential descriptive
statistics, file a request — `std_pop_over` / `var_pop_over` are
deferred to demand and would ship as additive variants.

`std_over` / `var_over` return **Null** when fewer than 2 non-Null
values are observed (sample variance is undefined for n<2).
`count_over` returns **0.0** for an empty scope (zero is information —
distinct from "undefined").

---

## Wilson CI: ONLY for binomial proportions

`wilson_ci_lower(p, n)` and `wilson_ci_upper(p, n)` apply specifically
to observed binomial proportions — `k` successes out of `n` independent
trials with `p = k/n`.

**USE for:**
- Win rate (direction accuracy)
- Conversion rate
- Hit rate
- Any rate that's `k/n` where `k ∈ {0, ..., n}`

**DO NOT USE for:**
- ROI (can be negative or > 1)
- Sharpe ratio
- Mean residual
- Expected value
- PnL or other dollar quantities

If `p` falls outside `[0, 1]` or `n ≤ 0`, Wilson silently returns
`Null` (no error diagnostic — per ADR-0031 Amendment 2's invalid-input
contract). The cube reader sees the Null and the report renders it as
empty; no exception fires. For arbitrary-bound CIs, bootstrap CI
primitives are deferred to a future phase.

### Wilson at degenerate boundaries

`wilson_ci_lower(0.0, n)` returns `0.0` (clamped).
`wilson_ci_upper(0.0, n)` returns a small positive bound — the "rule of
three" style interval that quantifies the maximum plausible rate
consistent with zero observed successes. Symmetrically for `p = 1.0`.

```yaml
- name: conversion_rate
  body: 'avg_over(converted_indicator, sessions)'

# Even when conversion_rate = 0 across the holdout, Wilson reports
# a non-trivial upper bound so the report has a defensible "no
# more than X%" claim instead of a misleading 0% point estimate.
- name: conversion_rate_upper_95
  body: 'wilson_ci_upper(conversion_rate, count_over(converted_indicator, sessions))'
```

---

## Demo proof: claw-core MLB cartridge in Mosaic-native form

Per the integration test report, claw-core's V1.0+NB MLB cartridge
clears the betting gate at 59.68% win rate (n=1508 bets, walk-forward
2023–2025). The Wilson 95% lower bound is 57.18% — the published
defensible claim.

In Mosaic-native form, the entire EXP-028 walk-forward statistic
emission is five cube rules:

```yaml
measures:
  - { name: predicted_direction, role: Input, data_type: F64, aggregation: Sum }
  - { name: actual_direction,    role: Input, data_type: F64, aggregation: Sum }
  - { name: direction_correct,   role: Derived, data_type: F64, aggregation: Sum }
  - { name: direction_accuracy,  role: Derived, data_type: F64, aggregation: Avg }
  - { name: n_bets,              role: Derived, data_type: F64, aggregation: Sum }
  - { name: direction_accuracy_lower_95, role: Derived, data_type: F64, aggregation: Avg }
  - { name: direction_accuracy_upper_95, role: Derived, data_type: F64, aggregation: Avg }

rules:
  # Per-bet outcome (1.0 for correct, 0.0 for incorrect — NEVER Null)
  - name: rule_direction_correct
    target_measure: direction_correct
    body: 'if(predicted_direction == actual_direction, 1.0, 0.0)'
    declared_dependencies: [predicted_direction, actual_direction]

  # Aggregate metrics
  - name: rule_direction_accuracy
    target_measure: direction_accuracy
    body: 'avg_over(direction_correct, walk_forward_bets)'
    declared_dependencies: [direction_correct]

  - name: rule_n_bets
    target_measure: n_bets
    body: 'count_over(direction_correct, walk_forward_bets)'
    declared_dependencies: [direction_correct]

  # 95% CI on the win rate — the load-bearing inferential claim
  - name: rule_direction_accuracy_lower_95
    target_measure: direction_accuracy_lower_95
    body: 'wilson_ci_lower(direction_accuracy, n_bets)'
    declared_dependencies: [direction_accuracy, n_bets]

  - name: rule_direction_accuracy_upper_95
    target_measure: direction_accuracy_upper_95
    body: 'wilson_ci_upper(direction_accuracy, n_bets)'
    declared_dependencies: [direction_accuracy, n_bets]
```

The original Python emission script was ~300 lines. The cube
formulation is five rules. The kernel re-evaluates the entire metric
chain on every write to the upstream inputs — no manual recompute, no
intermediate state to invalidate.

Per ADR-0033 Amendment 7, the Wilson lower bound matches the published
57.18% headline to 1e-3 tolerance (the "57.18%" is reported to four
significant figures; tighter tolerance would flake on the rounding).

---

## `mc model grade` — segmented holdout evaluation (Phase 10B, ADR-0034)

`grade` groups a holdout set by an attribute and computes the per-segment
metrics above in one command, instead of a ~120-line Python script. It is
the command form of the EXP-048 segment table: it *composes* the Phase 10A
primitives by restricting their per-leaf evaluation to each segment.

```
mc model grade <cartridge.yaml> \
  --unit <dimension> \
  --holdout "<filter>" \
  --group-by <key> [--group-by <key> ...] \
  --metric "<name>=<reduction>(<ingredient>[,<ingredient>])" [--metric ...] \
  [--bucket <measure> <e0>:<e1>:...:<eN>] \
  [--flag-if "<metric> <op> <value>"] \
  [--min-n <int>] [--max-segments <int>] \
  [--wilson-null error|drop] [--include-writes] \
  [--format text|json]
```

### Reductions (9, closed vocabulary)

`count`, `mean`, `sum`, `ratio(num,den)`, `std` (ddof=1), `min`, `max`,
`wilson_lower`, `wilson_upper`. Every reduction except `ratio` takes
exactly one ingredient; `ratio` takes two. Ingredients must be measures
defined in the cartridge. For anything outside this vocabulary, declare a
per-unit derived measure and pass it as an ingredient (same rule as §1).

### Group keys

A `--group-by <key>` is a **dimension** (one segment per element), or a
**measure**. Because Mosaic measures carry no discrete/low-cardinality
metadata, a measure group key follows this rule:

- A **string/categorical** measure value groups by distinct value directly.
- A **continuous F64** measure REQUIRES `--bucket` — grouping a float by
  distinct value is a float-equality hazard and would explode into
  thousands of singletons. Omitting `--bucket` for an F64 key is a hard
  error.

Buckets are left-closed / right-open, last band right-closed:
`--bucket Edge_NB 0:0.03:0.10:1.0` → `[0,0.03)`, `[0.03,0.10)`,
`[0.10,1.0]`. Values outside every band land in a surfaced
`(out-of-range)` segment (counted in TOTAL, never silently dropped).
`--max-segments` (default 50) caps the resolved segment count.

### Holdout filter (reuses the `--where` grammar)

`--holdout` uses the same `Filter` grammar as `mc model query --where`
(`And`/`Or`/`Not`, `==`/`!=`/`>`/`>=`/`<`/`<=`, dimension and measure
atoms) — **not** the `--coord` dimension-pin syntax. Worked examples:

```bash
# Dimension pin (string equality is safe):
--holdout 'Scenario == "balanced"'

# Measure range (the correct way to pin a continuous F64 measure):
--holdout 'line >= 8.99 and line <= 9.01'
```

A **bare `==` / `!=` against a numeric literal on a measure**
(`line == 9.0`) is a hard error: float equality is hazardous and no
discrete-marking exists. Use a range or an explicit tolerance instead.

### Wilson Null-indicator safety (hard error by default)

`wilson_lower(m)` / `wilson_upper(m)` compute `n` as the segment's
non-Null trial count and `p` as the mean. Per the §2/§3 convention the
indicator must be `1.0`/`0.0`, **never Null**. If `m` has any Null in a
segment, grade **hard-errors by default** (a too-narrow CI is the wrong
failure in a betting context). `--wilson-null drop` excludes the Null
units (changing `n`) and emits a warning instead.

### `ratio`, TOTAL, and `--min-n`

`ratio(num, den)` returns Null (never `inf`/`NaN`/`0`) when the
denominator sum is zero/empty, with a diagnostic in `warnings` and the
segment listed in `denominator_zero_segments` (zero-check via
`abs() < 1e-300`, not `== 0.0`). The always-present **TOTAL** row
aggregates *every* holdout unit — inclusive of `--min-n`-excluded and
`(out-of-range)` segments (`--min-n` affects flag eligibility only, not
measurement). Only units failing the `--holdout` filter are absent from
TOTAL.

### EXP-048 worked example

Grouping the 2025 `line=9.0` holdout by bet side reproduces the EXP-048
smoking gun — 98.5% of bets are UNDERs hitting 65.70% with a Wilson 95% CI
of [61.19, 69.94]:

```bash
mc model grade mlb-totals.yaml \
  --unit Game \
  --holdout 'Scenario == "balanced" and line >= 8.99 and line <= 9.01' \
  --group-by bet_side --bucket bet_side 0:0.5:1.0 \
  --metric "n=count(direction_correct)" \
  --metric "win_rate=mean(direction_correct)" \
  --metric "wr_lower_95=wilson_lower(direction_correct)" \
  --metric "wr_upper_95=wilson_upper(direction_correct)" \
  --flag-if "wr_lower_95 < 0.50" \
  --format json
```

The Wilson bounds match the `metrics.rs` continuous-`p` reference to the
1e-3 headline tolerance (the published rates are reported to four
significant figures).

### Reproducibility note (Amendment 10)

grade defaults to `LoadPolicy::Reproducible`: it starts from the
version-controlled model state, **excluding** operational
`.tessera/writes.jsonl` post-hoc writes. `--include-writes` folds them in.
grade reads cube state as of load time and performs no live re-evaluation
against changing data files during a run. For exact reproducibility
against a historical snapshot, pin to a known cube revision before running
grade; an explicit `--at-revision` flag is deferred to demand.

---

## Cross-links

- [ADR-0033](../decisions/0033-phase-10a-evaluation-metrics-library.md) — Phase 10A acceptance + amendments
- [ADR-0031](../decisions/0031-nbinom-sf-formula-function.md) — Null-on-invalid-input precedent (Amendment 2)
- [research note: built-in evaluation primitives](../research-notes/built-in-evaluation-primitives.md) — the 5 commands this cookbook unblocks
- Phase 1 engine semantics — `docs/specs/engine-semantics.md`
- Phase 1 build brief — `docs/specs/phase-1-rust-kernel-build-brief.md`
