---
name: mosaic-formulas
description: Complete formula syntax reference for Mosaic rule bodies — all phases through 3H. Covers operators (+ - * /), if_null, Phase 3E conditionals (if, comparisons, and/or/not, min, max, abs, safe_div, clamp, coalesce, actual_ref), Phase 3F time-series (prev, lag, cumulative, rolling_avg, period_index), Phase 3F.1 time anchors (anchor_index, is_past, is_current, is_future, periods_since_anchor, periods_to_end), Phase 3G reference data (benchmark, lookup, bucket, sum_over), and Phase 3H fitted models (predict, calibrate, exp, norm_cdf). Use whenever the user is writing a rule body, asks why a formula doesn't parse, or wants to know what functions are available.
---

# Mosaic Formula Syntax — Complete Reference (through Phase 3H)

Mosaic formulas are strings in the `body:` field of a rule. They compile to the same internal AST as the structured form; either can be used interchangeably.

```yaml
# Formula form (recommended for human authors):
- target_measure: "Revenue"
  body: "Customers * AOV"
  declared_dependencies: ["Customers", "AOV"]

# Structured form (still supported):
- target_measure: "Revenue"
  body:
    mul:
      - { ref: "Customers" }
      - { ref: "AOV" }
  declared_dependencies: ["Customers", "AOV"]
```

---

## Grammar

```
expr        ::= or_expr
or_expr     ::= and_expr ('or' and_expr)*
and_expr    ::= not_expr ('and' not_expr)*
not_expr    ::= 'not' not_expr | comparison
comparison  ::= addition (('==' | '!=' | '<' | '>' | '<=' | '>=') addition)?
                  -- non-associative: a > b > c is MC1008
addition    ::= term (('+' | '-') term)*
term        ::= factor (('*' | '/') factor)*
factor      ::= unary | primary
unary       ::= ('+' | '-') factor
primary     ::= number | ref | call | '(' expr ')'
call        ::= identifier '(' [expr (',' expr)*] ')'
ref         ::= identifier
identifier  ::= [A-Za-z_][A-Za-z0-9_]*
number      ::= F64 literal (decimal, scientific, no underscores, no hex)
```

---

## Operators (in precedence order, highest first)

1. **Parentheses** `( ... )` — explicit grouping.
2. **Unary `+`** / **unary `-`** — at start of expression or after `(`.
3. **Multiplication `*`** / **Division `/`** — left-associative.
4. **Addition `+`** / **Subtraction `-`** — left-associative.
5. **Comparison** `==`, `!=`, `<`, `>`, `<=`, `>=` — non-associative (chained comparisons fire MC1008; use `and`).
6. **Logical `not`** — unary.
7. **Logical `and`** — left-associative.
8. **Logical `or`** — lowest precedence, left-associative.

Examples:

```
Spend / CPC                            # division
Spend * (1 - COGS_Rate)                # multiplication, parens, subtraction
-Spend                                  # unary minus → AST: Sub(Const(0), Ref(Spend))
Spend + Clicks * CPC                    # * binds tighter than +
Revenue > 0 and Spend > 0              # comparison then and
not (CPC > 5)                          # logical not
```

---

## Identifiers (measure references)

- Identifiers are **case-sensitive measure names**. `Spend` ≠ `spend` ≠ `SPEND`.
- They must match a measure declared in `measures:` (or fire MC2005 at validation).
- Keywords: `and`, `or`, `not` are reserved as logical operators. All function names (`if`, `min`, `max`, etc.) are keywords only when followed by `(`.

## Number literals

- F64 (double-precision float). Integer literals like `1000` auto-promote.
- **No underscores:** `1_000` fires MC1006. Write `1000`.
- **No hex:** `0x1A` fires MC1006.
- **Scientific notation OK:** `1.5e-3`, `2E10`.
- **No leading dot:** `.5` may not parse — write `0.5`.
- Negative numbers: write as unary minus (`-3.0` → `Sub([Const(0.0), Const(3.0)])`).

## Unary minus desugaring

`-x` desugars to `Sub([Const(F64(0.0)), x])` at parse time. The AST contains a `Sub` node, not a `Neg`. `inspect` renders `-Spend` as `(0 - Spend)` — expected, not a bug.

---

## Phase 3D Functions

### `if_null(primary, fallback)`

Returns `primary` if it is non-Null; otherwise returns `fallback`.

```yaml
body: "if_null(Actual, Forecast)"
declared_dependencies: ["Actual", "Forecast"]
```

- **Null handling:** if `primary` is Null, returns `fallback` (which may itself be Null).
- Use for multi-step fallback by nesting: `if_null(A, if_null(B, C))`.

---

## Phase 3E — Conditionals and Logic

### `if(condition, then_value, else_value)`

**Signature:** `if(condition_expr, then_expr, else_expr)`

Conditional branch. Evaluates `condition_expr`; if truthy (non-zero), returns `then_expr`; if falsy (zero or Null), returns `else_expr`.

```yaml
body: "if(Spend > Budget_Cap, Budget_Cap, Spend)"
declared_dependencies: ["Spend", "Budget_Cap"]
```

- **Null condition:** `if(Null, then, else)` returns the **else branch**. Null means "unknown"; the safe path is the else branch.
- **Boolean encoding:** conditions are f64; 1.0 = true, 0.0 = false, Null = unknown.

---

### Comparison operators: `>`, `<`, `>=`, `<=`, `==`, `!=`

Return `1.0` (true) or `0.0` (false). Non-associative: `a > b > c` fires MC1008.

| Operator | Meaning |
|---|---|
| `a > b` | 1.0 if a greater than b |
| `a < b` | 1.0 if a less than b |
| `a >= b` | 1.0 if a greater than or equal to b |
| `a <= b` | 1.0 if a less than or equal to b |
| `a == b` | 1.0 if a equals b |
| `a != b` | 1.0 if a not equal to b |

```yaml
body: "if(CPC > 5.0, 1.0, 0.0)"
body: "if(Revenue == 0, 0, Margin / Revenue)"
```

- **Null handling:** any comparison involving Null returns **Null** (not 0.0). This preserves SQL three-valued logic. `Null > 5` = Null; `Null == Null` = Null.

---

### Logical operators: `and(a, b)`, `or(a, b)`, `not(a)`

Written as infix keywords (`a and b`, `a or b`) or prefix (`not a`). Combine boolean conditions.

```yaml
body: "if(Spend > 0 and Clicks > 0, Spend / Clicks, 0)"
body: "if(not (Status == 0), Active_Rate, 0)"
body: "if(Revenue < 0 or Margin < 0, 1.0, 0.0)"
```

- **Null handling:** Null propagates — `Null and x` = Null, `Null or x` = Null, `not(Null)` = Null.

---

### `min(a, b)` / `max(a, b)`

**Signature:** `min(expr, expr, ...)` / `max(expr, expr, ...)` — variadic (2+ arguments)

Returns the minimum (or maximum) of the arguments.

```yaml
body: "min(Spend, Budget_Cap)"
declared_dependencies: ["Spend", "Budget_Cap"]

body: "max(Margin, 0)"
declared_dependencies: ["Margin"]
```

- **Null handling:** Null propagates — `min(Null, 5)` = Null.

---

### `abs(x)`

**Signature:** `abs(expr)`

Returns the absolute value of `x`.

```yaml
body: "abs(Actual - Budget)"
declared_dependencies: ["Actual", "Budget"]
```

- **Null handling:** `abs(Null)` = Null.

---

### `safe_div(a, b, default)`

**Signature:** `safe_div(numerator, denominator, default_expr)`

Divides `a` by `b`. If `b` is zero or Null, returns `default` instead of dividing.

```yaml
body: "safe_div(Revenue - Cost, Revenue, 0)"
declared_dependencies: ["Revenue", "Cost"]

body: "safe_div(Clicks, Impressions, 0)"
declared_dependencies: ["Clicks", "Impressions"]
```

- **Null handling:** if `a` is Null, returns Null. If `b` is zero or Null, returns `default`. The `default` expression is evaluated lazily only when needed.
- Use `safe_div` instead of `a / b` whenever the denominator might be zero (e.g., rates, shares, ratios).

---

### `clamp(x, lo, hi)`

**Signature:** `clamp(value_expr, lo_expr, hi_expr)`

Returns `x` bounded between `lo` and `hi`. Equivalent to `max(lo, min(x, hi))`.

```yaml
body: "clamp(CPC, 0.5, 20.0)"
declared_dependencies: ["CPC"]

body: "clamp(Growth_Rate, Min_Growth, Max_Growth)"
declared_dependencies: ["Growth_Rate", "Min_Growth", "Max_Growth"]
```

- **Null handling:** if any argument is Null, returns Null.

---

### `coalesce(a, b, c, ...)`

**Signature:** `coalesce(expr, expr, ...)` — variadic (2+ arguments)

Returns the first non-Null value among the arguments.

```yaml
body: "coalesce(Actual, Forecast, Default_Value)"
declared_dependencies: ["Actual", "Forecast", "Default_Value"]
```

- **Null handling:** scans left to right; returns the first non-Null. If all arguments are Null, returns Null.
- Prefer `coalesce(a, b, c, d)` over nested `if_null(a, if_null(b, if_null(c, d)))` for readability.

---

### `actual_ref(measure)`

**Signature:** `actual_ref(MeasureName)` — argument must be a bare measure identifier

Reads the named measure at the SAME coordinate except the Scenario-kind dimension shifts to the declared `actuals_element`.

```yaml
body: "safe_div(Forecast - actual_ref(Revenue), actual_ref(Revenue), 0)"
declared_dependencies: ["Forecast"]
```

**Requirements:**
- The model must have exactly one dimension with `kind: "Scenario"`.
- That dimension must declare `actuals_element: "Actual"` (or whichever element name holds actuals).
- If the `actuals_element` field is missing, MC2037 fires.

```yaml
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    actuals_element: "Actual"
    elements:
      - { name: "Actual", scenario_meta: "Default" }
      - { name: "Forecast", scenario_meta: "NonDefault" }
```

- **Null handling:** if no actuals value exists at the requested coordinate (e.g., future period), returns Null.
- **Nesting prohibition:** `actual_ref` cannot be nested inside `prev`/`lag`/`sum_over` or vice versa (MC1013).

---

## Phase 3E Diagnostic Codes

| Code | Fires when |
|---|---|
| **MC1007** | Unknown function call (identifier followed by `(` not in the function table) |
| **MC1008** | Wrong argument count for a function, OR chained non-associative comparisons (`a > b > c`) |
| **MC1009** | `actual_ref` called with a non-identifier argument (must be a bare measure name) |
| **MC1013** | Cross-coordinate functions nested (`prev(actual_ref(X))`, `actual_ref(prev(X))`) |

---

## Phase 3F — Time-Series

These functions require a dimension with `kind: "Time"` declared in the model. If no such dimension exists and a time-series function is used, MC1012 fires.

```yaml
dimensions:
  - name: "Time"
    kind: "Time"
    elements:
      - { name: "Jan_2026" }
      - { name: "Feb_2026" }
      - { name: "Mar_2026" }
```

"Previous" and "next" are defined by **element declaration order** — not by calendar dates.

---

### `prev(measure)`

**Signature:** `prev(MeasureName)` — argument must be a bare measure identifier

Returns the value of `measure` at the previous Time element (position - 1).

```yaml
body: "safe_div(Revenue - prev(Revenue), prev(Revenue), 0)"
declared_dependencies: ["Revenue"]
```

- **Null handling:** at the first Time element, `prev(X)` returns Null (there is no prior period). Use `if_null(prev(X), 0)` to treat the first period as zero.

---

### `lag(measure, n)`

**Signature:** `lag(MeasureName, n_expr)` — measure is a bare identifier; `n` is an expression

Returns the value of `measure` at `n` positions ago. Negative `n` looks forward (lead).

| `n` value | Meaning |
|---|---|
| 1 | previous period (same as `prev(measure)`) |
| 3 | 3 periods ago |
| -1 | next period (lead) |
| 0 | current period |

```yaml
body: "safe_div(Revenue - lag(Revenue, 12), lag(Revenue, 12), 0)"
declared_dependencies: ["Revenue"]
```

- **Null handling:** if the target period is out of range (before first or after last element), returns Null.
- MC1010 fires if the `n` argument is non-numeric at eval time.

---

### `cumulative(measure)`

**Signature:** `cumulative(MeasureName)` — argument must be a bare measure identifier

Returns the running sum of `measure` from the first Time element through the current one.

```yaml
body: "cumulative(Spend)"
declared_dependencies: ["Spend"]
```

- **Null handling:** Null periods are treated as 0 for the running sum (they contribute nothing). `cumulative(X)` at period 1 returns `X` at period 1.
- **Performance note:** writing to period P dirties all subsequent periods. MC3012 warns if the model is large enough that this could be expensive (time_elements × other_dimension_product × cumulative_measure_count > 50,000).

---

### `rolling_avg(measure, n)`

**Signature:** `rolling_avg(MeasureName, n_expr)` — measure is a bare identifier; `n` is the window size

Returns the moving average of `measure` over the last `n` periods (inclusive of the current period).

```yaml
body: "rolling_avg(CPC, 3)"
declared_dependencies: ["CPC"]
```

- **Null handling:** partial windows (fewer than `n` periods available) compute the average over available periods. `rolling_avg(CPC, 3)` at period 2 = average of periods 1 and 2. If you want Null until the window is full: `if(period_index() >= 2, rolling_avg(CPC, 3), Null)`.
- MC1011 fires if `n` resolves to a non-positive integer at eval time.

---

### `period_index()`

**Signature:** `period_index()` — no arguments

Returns the 0-based position of the current Time element in the dimension's declaration order.

```yaml
body: "if(period_index() == 0, Opening_Balance, prev(Closing_Balance))"
declared_dependencies: ["Opening_Balance", "Closing_Balance"]
```

- **Null handling:** never Null; always returns an integer ≥ 0.
- Does NOT parse element names as dates. "Position 0" is the first element declared in the YAML, regardless of its name.

---

## Phase 3F Diagnostic Codes

| Code | Fires when |
|---|---|
| **MC1010** | `lag` period argument is non-numeric |
| **MC1011** | `rolling_avg` window resolves to non-positive integer |
| **MC1012** | Time-series function used but no `kind: "Time"` dimension declared |
| **MC2035** | No `kind: "Time"` dimension but time-series formulas exist |
| **MC2036** | Multiple dimensions with `kind: "Time"` |
| **MC3010** | Time elements with `date:` metadata in non-chronological order (lint) |
| **MC3012** | `cumulative` used on a large time dimension (performance lint) |

---

## Phase 3F.1 — Time Anchor

These functions require a `time_anchor` to be configured — either at the dimension level in YAML or via the `--time-anchor` CLI flag. If an anchor function is used without a configured anchor, MC1017 fires.

```yaml
dimensions:
  - name: "Time"
    kind: "Time"
    time_anchor: "Oct_2025"   # dimension-level default
    elements:
      - { name: "Jan_2025" }
      # ...
      - { name: "Oct_2025" }
      - { name: "Nov_2025" }
      - { name: "Dec_2025" }
```

```bash
# Override at runtime:
mc model test mymodel.yaml --time-anchor Nov_2025
```

The anchor resolves to a Time element by **name-equality** (not date parsing).

---

### `anchor_index()`

**Signature:** `anchor_index()` — no arguments

Returns the `period_index()` of the configured `time_anchor` element.

```yaml
body: "anchor_index()"
```

- **Null handling:** never Null when a valid anchor is configured; MC1017 fires if no anchor is set.

---

### `is_past()`

**Signature:** `is_past()` — no arguments

Returns `1.0` if the current period is before the anchor (past); `0.0` otherwise.

```yaml
body: "if(is_past(), actual_ref(Revenue), Forecast)"
declared_dependencies: ["Forecast"]
```

- Equivalent to `if(period_index() < anchor_index(), 1.0, 0.0)`.
- **Null handling:** never Null when a valid anchor is configured.

---

### `is_current()`

**Signature:** `is_current()` — no arguments

Returns `1.0` if the current period equals the anchor; `0.0` otherwise.

```yaml
body: "if(is_current(), Highlight_Rate, Normal_Rate)"
declared_dependencies: ["Highlight_Rate", "Normal_Rate"]
```

- Equivalent to `if(period_index() == anchor_index(), 1.0, 0.0)`.

---

### `is_future()`

**Signature:** `is_future()` — no arguments

Returns `1.0` if the current period is after the anchor (future); `0.0` otherwise.

```yaml
body: "if(is_future(), Projected_Spend, actual_ref(Spend))"
declared_dependencies: ["Projected_Spend"]
```

- Equivalent to `if(period_index() > anchor_index(), 1.0, 0.0)`.

---

### `periods_since_anchor()`

**Signature:** `periods_since_anchor()` — no arguments

Returns `period_index() - anchor_index()`. Negative = past; zero = current; positive = future.

```yaml
body: "Base_Value * exp(-Decay_Rate * periods_since_anchor())"
declared_dependencies: ["Base_Value", "Decay_Rate"]
```

- **Null handling:** never Null when a valid anchor is configured.

---

### `periods_to_end()`

**Signature:** `periods_to_end()` — no arguments

Returns `max_period_index - period_index()`. Zero at the last period.

```yaml
body: "safe_div(cumulative(Spend), period_index() + 1 + periods_to_end(), 0)"
declared_dependencies: ["Spend"]
```

- **Null handling:** never Null.

---

## Phase 3F.1 Diagnostic Codes

| Code | Fires when |
|---|---|
| **MC1017** | Anchor function used but no `time_anchor` configured (YAML or CLI) |
| **MC2043** | Element date metadata is not ISO 8601 `YYYY-MM-DD` format |
| **MC2044** | Timestamp metadata is not UTC (missing `Z` suffix) |
| **MC2045** | Time element intervals don't match the dimension's declared `granularity` |
| **MC2046** | Gap between consecutive Time elements (non-contiguous periods) |
| **MC2047** | Overlapping Time elements |
| **MC2048** | `time_anchor` names an element not declared in the Time dimension |
| **MC3016** | Time elements not in chronological order (lint warning) |

---

## Phase 3G — Reference Data

Phase 3G introduces three top-level YAML blocks for structured reference data, plus four formula functions to read from them.

### YAML blocks required

```yaml
benchmarks:
  - name: "industry_cpc"
    source: "WordStream 2025"
    last_updated: "2025-03-15"
    key_dimension: "Channel"
    values:
      Paid_Search: 5.50
      Paid_Social: 3.20
      Display: 1.80

lookup_tables:
  - name: "seasonal_factor"
    key_dimension: "Time"
    values:
      Jan_2026: 0.75
      Feb_2026: 0.80
      Mar_2026: 0.90

status_thresholds:
  - name: "cpc_health"
    bands:
      - { label: "Good",     max: 3.0 }
      - { label: "Warning",  max: 7.0 }
      - { label: "Critical" }         # no max = unbounded above
```

---

### `benchmark("name", DimName)`

**Signature:** `benchmark("benchmark_name", DimensionRef)`

Reads the value for the current element of `DimensionRef` from the named benchmark block.

```yaml
body: "safe_div(CPC, benchmark(\"industry_cpc\", Channel), 1)"
declared_dependencies: ["CPC"]
```

- `DimensionRef` is a dimension name (not a measure) — it resolves to the current element name in that dimension, then looks it up in the benchmark's `values` map.
- **Null handling:** if the current element has no entry in the benchmark, returns Null.
- MC1013 fires if the benchmark name is unknown. MC2038 fires if the key_dimension is undeclared.

---

### `lookup("table_name", DimName)`

**Signature:** `lookup("table_name", DimensionRef)`

Reads the value for the current element of `DimensionRef` from the named lookup table. Exact-match only.

```yaml
body: "Revenue * lookup(\"tax_rate\", Market)"
declared_dependencies: ["Revenue"]

body: "Base_Spend * lookup(\"seasonal_factor\", Time)"
declared_dependencies: ["Base_Spend"]
```

- **Null handling:** if the current element has no entry in the lookup table, returns Null.
- MC1014 fires if the table name is unknown.

---

### `bucket(value, "threshold_name")`

**Signature:** `bucket(value_expr, "threshold_name")`

Returns the zero-based band index (0.0, 1.0, 2.0, ...) for where `value` falls in the named threshold definition.

```yaml
body: "bucket(CPC, \"cpc_health\")"
declared_dependencies: ["CPC"]
# Returns 0.0 for "Good", 1.0 for "Warning", 2.0 for "Critical"

body: "if(bucket(CPC, \"cpc_health\") >= 2, Alert_Rate, Normal_Rate)"
declared_dependencies: ["CPC", "Alert_Rate", "Normal_Rate"]
```

- Band 0 is the first declared band; each subsequent band is +1.0.
- The string label ("Good", "Warning", "Critical") is a display concern — formulas operate on the numeric index.
- **Null handling:** `bucket(Null, ...)` returns Null.
- MC1015 fires if the threshold name is unknown.

---

### `sum_over("DimName", measure)`

**Signature:** `sum_over("DimensionName", MeasureName)` — both arguments are string/identifier literals

Sums `measure` across ALL leaf elements of the named dimension, holding all other dimensions at the current coordinate.

```yaml
body: "safe_div(Spend, sum_over(\"Channel\", Spend), 0)"
declared_dependencies: ["Spend"]
# Computes Spend's share of total Channel spend
```

- Sums leaf elements only — does not include consolidated/parent elements (no double-counting).
- **Null handling:** Null leaf values are treated as 0 in the sum.
- **Performance note:** each eval of a cell containing `sum_over` triggers N reads (N = leaf count of the named dimension). MC3011 warns at > 50 leaf elements.
- MC1016 fires if the dimension name is not declared in the model.

---

## Phase 3G Diagnostic Codes

| Code | Fires when |
|---|---|
| **MC1013** | Formula references unknown benchmark name |
| **MC1014** | Formula references unknown lookup table name |
| **MC1015** | Formula references unknown threshold name |
| **MC1016** | `sum_over` first argument is not a declared dimension name |
| **MC2030** | Benchmark `last_updated` > 12 months old (lint warning) |
| **MC2031** | Reference data block unreferenced by any formula (lint) |
| **MC2037** | Duplicate name across reference-data blocks |
| **MC2038** | `key_dimension` references an undeclared dimension |
| **MC2039** | Value key is not a valid element in the key dimension |
| **MC2040** | Status threshold has fewer than 2 bands |
| **MC2041** | Threshold bands have non-ascending `max` values |
| **MC2042** | Last threshold band has a `max` (should be unbounded — omit `max`) |
| **MC3011** | `sum_over` on a dimension with > 50 leaf elements (performance lint) |
| **MC3013** | Benchmark `source` field is empty (provenance lint) |
| **MC3014** | `benchmark`/`lookup` key argument is a complex expression, not a dimension name (lint) |
| **MC5025** | Status threshold has a gap between bands |
| **MC5026** | Status threshold bands overlap |

---

## Phase 3H — Fitted Models and Probability

Phase 3H adds two new top-level YAML blocks (`fitted_models:` and `calibration_maps:`) and four formula functions for evaluating pre-fitted statistical models. Fitting happens in Python/sklearn/R; Mosaic evaluates.

### YAML blocks required

```yaml
fitted_models:
  - name: "nba_total_v1_lasso"
    method: "linear"          # "linear" | "logistic"
    intercept: 211.34
    coefficients:
      - { feature: "avg_pace",             weight: 3.016  }
      - { feature: "combined_off_rating",  weight: 0.548  }
      - { feature: "avg_recent_total_10",  weight: 0.331  }
      - { feature: "combined_def_rating",  weight: 0.602  }
      - { feature: "home_missing_scorers", weight: -1.203 }
    standardization:          # optional: applied when model was fit on z-scored inputs
      method: "zscore"
      params:
        - { feature: "avg_pace",            mean: 99.2,  std: 4.7  }
        - { feature: "combined_off_rating", mean: 113.4, std: 8.2  }
    residual_std: 17.251      # for future predict_dist() (Phase 3J)
    metadata:
      fitted_at: "2026-04-08T12:00:00Z"
      algorithm: "lasso"
      n_train: 3685
      holdout_mae: 13.783

calibration_maps:
  - name: "nba_totals_cal_v1"
    method: "pava"            # "pava" | "platt"
    points:                   # for pava: monotonic mapping points
      - { raw: 0.50, calibrated: 0.42 }
      - { raw: 0.60, calibrated: 0.50 }
      - { raw: 0.70, calibrated: 0.61 }
      - { raw: 0.80, calibrated: 0.76 }
    platt_params:             # for platt: sigmoid parameters (used instead of points)
      a: -1.2
      b: 0.3
    metadata:
      fitted_at: "2026-04-30T01:29:14Z"
      sample_size: 1312
      calibrated_brier: 0.2499
```

---

### `predict("model_id", feature1, feature2, ...)`

**Signature:** `predict("model_name", expr, expr, ...)` — first arg is a string literal model name; remaining args are feature expressions in the ORDER declared in the model's `coefficients:` array

Evaluates a pre-fitted model by name.

```yaml
body: >
  predict("nba_total_v1_lasso",
    avg_pace, combined_off_rating, avg_recent_total_10,
    combined_def_rating, home_missing_scorers)
declared_dependencies:
  ["avg_pace", "combined_off_rating", "avg_recent_total_10",
   "combined_def_rating", "home_missing_scorers"]
```

**Evaluation for `method: "linear"`:**
1. For each feature in coefficient order: read the argument value.
2. If standardization declared for this feature: `z = (value - mean) / std`. Else `z = value`.
3. `weighted = z × weight`.
4. `result = intercept + sum(all weighted values)`.

**Evaluation for `method: "logistic"`:**
- Same as linear, then apply sigmoid: `result = 1 / (1 + exp(-linear_result))`. Returns probability between 0.0 and 1.0.

- **Feature order matters:** args are positional against `coefficients:` array. `predict("m", f1, f2, f3)` maps f1→coefficients[0], f2→coefficients[1], f3→coefficients[2].
- **Null handling:** if ANY feature value is Null, `predict()` returns Null (Null-poisoning).
- MC2050 fires if the model name is unknown; MC2051 fires if argument count doesn't match coefficient count.

**sklearn export pattern:**
```python
print(yaml.dump({"fitted_models": [{
    "name": "my_model_v1",
    "method": "linear",
    "intercept": float(model.intercept_),
    "coefficients": [
        {"feature": name, "weight": float(coef)}
        for name, coef in zip(feature_names, model.coef_)
        if abs(coef) > 1e-10  # skip Lasso zero coefficients
    ],
}]}))
```
Works identically for Lasso, Ridge, ElasticNet, OLS, and logistic regression.

---

### `calibrate(raw_value, "map_id")`

**Signature:** `calibrate(value_expr, "map_name")` — second arg is a string literal map name

Applies a calibration map to transform a raw model output into a calibrated probability.

```yaml
body: "calibrate(predict(\"nba_total_v1_lasso\", ...), \"nba_totals_cal_v1\")"
# Or, using an intermediate derived measure:
body: "calibrate(Raw_Win_Prob, \"nba_totals_cal_v1\")"
declared_dependencies: ["Raw_Win_Prob"]
```

**Evaluation for `method: "pava"`** (isotonic regression interpolation):
1. Find the two adjacent points where `points[i].raw <= raw_value < points[i+1].raw`.
2. Linear interpolate: `calibrated[i] + fraction × (calibrated[i+1] - calibrated[i])`.
3. If `raw_value` is below the first point: clamp to `calibrated[0]`.
4. If `raw_value` is above the last point: clamp to last `calibrated` value.

**Evaluation for `method: "platt"`** (Platt scaling):
- `result = 1 / (1 + exp(a × raw_value + b))`.

- **Null handling:** `calibrate(Null, ...)` returns Null.
- MC2052 fires if the map name is unknown; MC2054/MC2055 fire if the calibration map is structurally invalid.

---

### `exp(x)`

**Signature:** `exp(expr)`

Returns e^x (Euler's number raised to the power of x). Useful for exponential growth, decay, and sigmoid-based models.

```yaml
body: "Starting_MRR * exp(Growth_Rate * period_index())"
declared_dependencies: ["Starting_MRR", "Growth_Rate"]

body: "Base_Value * exp(-Decay_Rate * periods_since_anchor())"
declared_dependencies: ["Base_Value", "Decay_Rate"]
```

- `exp(0)` = 1.0; `exp(1)` ≈ 2.71828; `exp(-1)` ≈ 0.36788.
- **Null handling:** `exp(Null)` returns Null.

---

### `norm_cdf(x, mu, sigma)`

**Signature:** `norm_cdf(x_expr, mu_expr, sigma_expr)`

Returns P(X ≤ x) where X ~ Normal(mu, sigma). The probability that a normally-distributed variable is at or below `x`.

```yaml
# Probability that a game total goes OVER a market line:
body: "1 - norm_cdf(Market_Line, Predicted_Total, 17.25)"
declared_dependencies: ["Market_Line", "Predicted_Total"]

# Probability of meeting a revenue target:
body: "1 - norm_cdf(Revenue_Target, Forecast, Forecast_Std)"
declared_dependencies: ["Revenue_Target", "Forecast", "Forecast_Std"]

# Probability a stock return exceeds 5%:
body: "1 - norm_cdf(0.05, Expected_Return, Model_Std)"
declared_dependencies: ["Expected_Return", "Model_Std"]
```

- `norm_cdf(0, 0, 1)` = 0.5 (median of the standard normal).
- `norm_cdf(1.96, 0, 1)` ≈ 0.975 (standard 95% confidence bound).
- This is the bridge from point estimates (from `predict()`) to probabilities. Combine with `residual_std` from the fitted model to answer "how likely is outcome > threshold?"
- **Null handling:** if ANY of x, mu, sigma is Null → returns Null.
- **Edge case:** if `sigma ≤ 0` → returns Null (invalid distribution parameter).

---

## Phase 3H Diagnostic Codes

| Code | Fires when |
|---|---|
| **MC2050** | `predict()` references a model_id not in `fitted_models:` |
| **MC2051** | `predict()` argument count doesn't match the model's coefficient count |
| **MC2052** | `calibrate()` references a map_id not in `calibration_maps:` |
| **MC2053** | Duplicate name in `fitted_models:` or `calibration_maps:` |
| **MC2054** | Calibration map `points` not in ascending `raw` order |
| **MC2055** | Calibration map has fewer than 2 points (cannot interpolate) |
| **MC2056** | `standardization:` declares a feature not in the `coefficients:` list |
| **MC3017** | `fitted_model` metadata.fitted_at > 6 months old (staleness lint) |
| **MC3018** | `calibration_map` metadata.fitted_at > 6 months old (staleness lint) |

---

## Phase 3D Diagnostic Codes (parse layer)

| Code | Fires when |
|---|---|
| **MC1003** | Unbalanced parens, or paren in unexpected position |
| **MC1004** | Unexpected token (stray `.`, `,`, `=`, etc.) |
| **MC1005** | Expected an expression but didn't find one (trailing operator, two operators in a row) |
| **MC1006** | Number literal can't be parsed as F64 (e.g., underscores, hex) |

For the full diagnostic-code registry and fix patterns, see `skills/debugging/SKILL.md`.

---

## Parser round-trip

`mc model inspect` renders rules in formula form regardless of how they were authored. The serializer:
- Adds parens only when precedence requires them.
- Renders unary minus as `(0 - x)` (its AST shape after desugaring).
- Round-trips ALL functions: `predict(...)`, `calibrate(...)`, `norm_cdf(...)`, etc.

Parse → serialize → parse produces an identical AST.

---

## Acme rule examples (Phase 3D baseline)

```yaml
- name: "rule_clicks"
  body: "Spend / CPC"
  declared_dependencies: ["Spend", "CPC"]

- name: "rule_leads"
  body: "Clicks * CVR"
  declared_dependencies: ["Clicks", "CVR"]

- name: "rule_customers"
  body: "Leads * Close_Rate"
  declared_dependencies: ["Leads", "Close_Rate"]

- name: "rule_revenue"
  body: "Customers * AOV"
  declared_dependencies: ["Customers", "AOV"]

- name: "rule_gross_profit"
  body: "Revenue * (1 - COGS_Rate)"
  declared_dependencies: ["Revenue", "COGS_Rate"]
```

---

## Extended examples (Phases 3E–3H)

```yaml
# Budget cap with safe division (Phase 3E)
- name: "rule_capped_cpc"
  body: "safe_div(Spend, min(Clicks, max(Clicks, 1)), 0)"
  declared_dependencies: ["Spend", "Clicks"]

# Variance analysis with comparisons (Phase 3E)
- name: "rule_over_budget_flag"
  body: "if(Spend > Budget, 1.0, 0.0)"
  declared_dependencies: ["Spend", "Budget"]

# Actual vs. forecast comparison (Phase 3E)
- name: "rule_forecast_accuracy"
  body: "safe_div(abs(actual_ref(Revenue) - Revenue), actual_ref(Revenue), 0)"
  declared_dependencies: ["Revenue"]

# Month-over-month growth (Phase 3F)
- name: "rule_mom_growth"
  body: "safe_div(Revenue - prev(Revenue), prev(Revenue), 0)"
  declared_dependencies: ["Revenue"]

# Year-to-date spend (Phase 3F)
- name: "rule_ytd_spend"
  body: "cumulative(Spend)"
  declared_dependencies: ["Spend"]

# Rolling 3-month CPC average (Phase 3F)
- name: "rule_rolling_cpc"
  body: "rolling_avg(CPC, 3)"
  declared_dependencies: ["CPC"]

# Past vs. future blended plan (Phase 3F.1)
- name: "rule_blended_revenue"
  body: "if(is_past(), actual_ref(Revenue), Forecast)"
  declared_dependencies: ["Forecast"]

# Benchmark comparison (Phase 3G)
- name: "rule_cpc_vs_benchmark"
  body: "safe_div(CPC, benchmark(\"industry_cpc\", Channel), 1)"
  declared_dependencies: ["CPC"]

# Share of total spend (Phase 3G)
- name: "rule_spend_share"
  body: "safe_div(Spend, sum_over(\"Channel\", Spend), 0)"
  declared_dependencies: ["Spend"]

# CPC health status (Phase 3G)
- name: "rule_cpc_status"
  body: "bucket(CPC, \"cpc_health\")"
  declared_dependencies: ["CPC"]

# ML prediction (Phase 3H)
- name: "rule_predicted_total"
  body: >
    predict("nba_total_v1_lasso",
      avg_pace, combined_off_rating,
      avg_recent_total_10, combined_def_rating,
      home_missing_scorers)
  declared_dependencies:
    ["avg_pace", "combined_off_rating", "avg_recent_total_10",
     "combined_def_rating", "home_missing_scorers"]

# Calibrated win probability (Phase 3H)
- name: "rule_win_probability"
  body: "calibrate(Raw_Win_Prob, \"nba_totals_cal_v1\")"
  declared_dependencies: ["Raw_Win_Prob"]

# Over probability from normal distribution (Phase 3H)
- name: "rule_over_probability"
  body: "1 - norm_cdf(Market_Line, Predicted_Total, 17.25)"
  declared_dependencies: ["Market_Line", "Predicted_Total"]

# Exponential decay model (Phase 3H)
- name: "rule_decayed_value"
  body: "Base_Value * exp(-Decay_Rate * periods_since_anchor())"
  declared_dependencies: ["Base_Value", "Decay_Rate"]
```

---

## Anti-patterns (DON'T)

- **Don't use number-literal underscores.** `1_000` fires MC1006. Write `1000`.
- **Don't case-vary measure names.** `spend` ≠ `Spend`. Use the exact name from `measures:`.
- **Don't omit `declared_dependencies`.** The kernel rejects undeclared reads. List every measure you reference (but NOT dimension names used as `benchmark`/`lookup`/`sum_over` keys — those are not declared_dependencies).
- **Don't chain comparisons.** `a > b > c` fires MC1008. Write `a > b and b > c`.
- **Don't nest cross-coordinate functions.** `prev(actual_ref(X))` fires MC1013. Use an intermediate derived measure.
- **Don't use `predict()` with wrong argument count.** MC2051 fires. Count your coefficients and match the argument list.
- **Don't use anchor functions without a configured anchor.** MC1017 fires. Set `time_anchor:` in the dimension YAML or pass `--time-anchor` at runtime.
- **Don't use time-series functions without `kind: "Time"` on the dimension.** MC1012 fires.
- **Don't use `actual_ref` without `actuals_element:` on the Scenario dimension.** MC2037 fires.
