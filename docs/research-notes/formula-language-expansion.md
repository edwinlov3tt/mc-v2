# Formula Language Expansion — Phases 3E through 3J

> **Status:** Research note. Sequencing proposed; individual sub-phase ADRs required before implementation.
> **Date:** 2026-05-04
> **Prerequisite reading:** ADR-0007 (Phase 3D formula syntax), `cross-coordinate-formulas.md` (Phase 3E candidate)

---

## 1. Current state

Phase 3D shipped a formula parser over the existing 7-variant `ParsedRuleBody` AST:

| AST Node | Formula syntax | Example |
|---|---|---|
| `Const` | number literal | `1.0`, `1000`, `1.5e-3` |
| `Ref` | identifier | `Spend`, `CPC` |
| `Add` | `a + b` | `Spend + Revenue` |
| `Sub` | `a - b` | `Revenue - Spend` |
| `Mul` | `a * b` | `Customers * AOV` |
| `Div` | `a / b` | `Spend / CPC` |
| `IfNull` | `if_null(a, b)` | `if_null(Actual, Forecast)` |

This covers simple arithmetic ratios and null-fallback. It does NOT cover:
- Conditional logic (if/then/else)
- Comparisons (>, <, ==)
- Time-series references (previous period, lag)
- Cross-coordinate reads (other scenario, other dim position)
- Mathematical functions (min, max, abs, sqrt, ln, exp)
- Reference data lookups (benchmarks, tables, thresholds)
- Fitted model evaluation (linear regression, logistic, calibration)
- Distribution-valued cells (mean + uncertainty)

This document sequences the expansion into 6 sub-phases (3E through 3J), each shipping independently with explicit AST additions, new YAML blocks (where needed), and diagnostic codes.

---

## 2. Sub-phase overview

| Phase | Name | Key unlock | AST nodes added | New YAML blocks | Effort |
|---|---|---|---|---|---|
| **3E** | Conditionals and Basic Operations | 80% of real-world formulas | ~12 | None | 2-3 weeks |
| **3F** | Time-Series and Period Operations | Reporting, variance, trends | ~6 | None | 2-3 weeks |
| **3G** | Reference-Data Blocks | Industry standards, lookup tables | ~4 | `benchmarks:`, `lookup_tables:`, `status_thresholds:` | 3-4 weeks |
| **3H** | Fitted-Model Evaluation | ML/forecasting in the cube | ~3 | `fitted_models:`, `calibration_maps:` | 4-5 weeks |
| **3I** | Mathematical and Statistical Primitives | Growth models, NPV, probability | ~12 | None | 2-3 weeks |
| **3J** | Distributional Cells | Uncertainty quantification | ~5 | None (kernel change) | 6-8 weeks |

**Total new AST nodes:** ~42 (from 7 to ~49)
**Total new YAML blocks:** 5 (in 3G + 3H)
**Kernel-level changes:** Phase 3J only (ScalarValue extension)

---

## 3. Phase 3E — Conditionals and Basic Operations

### 3E.1 Rationale

The 80% unlock. Real-world models constantly need: "if spend > budget, cap it"; "show the better of plan vs actual"; "divide safely when denominator might be zero." Every domain — marketing, finance, sports betting, SaaS — hits this wall within the first model.

### 3E.2 Primitives

| Name | Signature | Example formula | AST node |
|---|---|---|---|
| `if` | `if(cond, then, else)` | `if(Spend > Budget, Budget, Spend)` | `If { condition, then_branch, else_branch }` |
| `>` | `a > b` | `Spend > 10000` | `Gt { left, right }` |
| `<` | `a < b` | `CPC < 2.0` | `Lt { left, right }` |
| `>=` | `a >= b` | `CVR >= 0.05` | `Gte { left, right }` |
| `<=` | `a <= b` | `Spend <= Budget` | `Lte { left, right }` |
| `==` | `a == b` | `Version == 1` | `Eq { left, right }` |
| `!=` | `a != b` | `Scenario_Flag != 0` | `Neq { left, right }` |
| `and` | `a and b` | `Spend > 0 and CPC < 5` | `And { left, right }` |
| `or` | `a or b` | `Budget_Flag == 1 or Override == 1` | `Or { left, right }` |
| `not` | `not a` | `not(Is_Frozen)` | `Not { operand }` |
| `min` | `min(a, b)` | `min(Spend, Budget_Cap)` | `Min { args: Vec }` |
| `max` | `max(a, b)` | `max(0, Revenue - Spend)` | `Max { args: Vec }` |
| `abs` | `abs(x)` | `abs(Plan - Actual)` | `Abs { operand }` |
| `safe_div` | `safe_div(a, b, default)` | `safe_div(Spend, Clicks, 0)` | `SafeDiv { numerator, denominator, default }` |
| `clamp` | `clamp(x, lo, hi)` | `clamp(CPC, 0.5, 25.0)` | `Clamp { value, lo, hi }` |
| `coalesce` | `coalesce(a, b, ...)` | `coalesce(Override, Plan, 0)` | `Coalesce { args: Vec }` |
| `actual_ref` | `actual_ref(measure)` | `actual_ref(Spend) * Seasonality` | `ActualRef { measure }` |

### 3E.3 Concrete user examples

**Marketing — budget capping:**
```yaml
- name: "rule_capped_spend"
  target_measure: "Capped_Spend"
  body: "min(Spend, Budget_Cap)"
  declared_dependencies: ["Spend", "Budget_Cap"]
```

**Finance — variance analysis:**
```yaml
- name: "rule_variance"
  target_measure: "Budget_Variance"
  body: "if(abs(Actual - Budget) > Threshold, Actual - Budget, 0)"
  declared_dependencies: ["Actual", "Budget", "Threshold"]
```

**Sports betting — safe division for win rate:**
```yaml
- name: "rule_win_rate"
  target_measure: "Win_Rate"
  body: "safe_div(Wins, Total_Bets, 0)"
  declared_dependencies: ["Wins", "Total_Bets"]
```

**SaaS — churn flagging:**
```yaml
- name: "rule_at_risk"
  target_measure: "At_Risk_Flag"
  body: "if(MRR_Change < 0 and Usage_Score < 30, 1, 0)"
  declared_dependencies: ["MRR_Change", "Usage_Score"]
```

**Cross-scenario forecast (from research note):**
```yaml
- name: "rule_forecast_spend"
  target_measure: "Forecast_Spend"
  scope: { scenario: "Forecast" }
  body: "actual_ref(Spend) * Plan_Seasonality"
  declared_dependencies: ["Spend", "Plan_Seasonality"]
```

### 3E.4 Precedence order (extended)

From lowest to highest:
1. `or`
2. `and`
3. Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=`
4. Addition: `+`, `-`
5. Multiplication: `*`, `/`
6. Unary: `not`, `+`, `-`
7. Function call: `if(...)`, `min(...)`, `max(...)`, `abs(...)`, `safe_div(...)`, `clamp(...)`, `coalesce(...)`, `actual_ref(...)`
8. Parentheses: `(...)`

### 3E.5 Type semantics

**Decision needed: boolean representation.** Two options:

- **Option A (f64-encoded):** Comparisons return 1.0 (true) / 0.0 (false). `if` treats non-zero as truthy. No ScalarValue change. Pro: no kernel change. Con: "booleans are numbers" is confusing; `Spend > 0` stores as 1.0.
- **Option B (ScalarValue::Bool):** Add a Bool variant to ScalarValue. Pro: type-safe. Con: kernel change (violates "only 3J touches ScalarValue").

**Recommendation:** Option A (f64-encoded) for Phase 3E. Booleans-as-numbers is the pragmatic choice used by Excel, TM1, and most planning engines. Phase 3J's distributional extension is the appropriate time to revisit ScalarValue shape.

### 3E.6 Diagnostic codes

| Code | Fires when |
|---|---|
| MC1007 | Unknown function call (splits from MC1004's catch-all as the function table grows) |
| MC1008 | Wrong argument count for function (e.g., `min(a)` or `if(a, b)`) |
| MC1009 | Comparison operator in non-boolean context (lint-level warning) |

### 3E.7 Dependencies on prior phases

- Phase 3D (formula parser infrastructure) — extends the same recursive-descent parser
- No kernel changes
- No new YAML blocks

### 3E.8 What becomes possible

- Budget capping and floors
- Variance analysis (plan vs actual with thresholds)
- Safe division without null-poison
- Multi-fallback coalesce chains
- Cross-scenario reads (actual_ref pattern from research note)
- Boolean flag measures for downstream filtering

---

## 4. Phase 3F — Time-Series and Period Operations

### 4F.1 Rationale

Every reporting model needs period-over-period comparisons, running totals, and moving averages. Without these, users pre-compute trends externally and import them as inputs — defeating the formula engine's "change input, see cascade" value proposition.

### 4F.2 Primitives

| Name | Signature | Example formula | AST node |
|---|---|---|---|
| `prev` | `prev(measure)` | `prev(Revenue)` | `Prev { measure }` |
| `lag` | `lag(measure, n)` | `lag(Spend, 3)` | `Lag { measure, periods }` |
| `cumulative` | `cumulative(measure)` | `cumulative(Revenue)` | `Cumulative { measure }` |
| `rolling_avg` | `rolling_avg(measure, n)` | `rolling_avg(CPC, 3)` | `RollingAvg { measure, window }` |
| `days_between` | `days_between(d1, d2)` | `days_between(Start_Date, End_Date)` | `DaysBetween { start, end }` |
| `period_index` | `period_index()` | `period_index()` | `PeriodIndex` |

### 4F.3 Concrete user examples

**Marketing — month-over-month growth:**
```yaml
- name: "rule_revenue_growth"
  target_measure: "Revenue_MoM_Growth"
  body: "safe_div(Revenue - prev(Revenue), prev(Revenue), 0)"
  declared_dependencies: ["Revenue"]
```

**SaaS — net revenue retention (trailing 12 months):**
```yaml
- name: "rule_nrr"
  target_measure: "NRR_12M"
  body: "safe_div(cumulative(Expansion_MRR) + cumulative(Base_MRR) - cumulative(Churn_MRR), lag(ARR, 12), 0)"
  declared_dependencies: ["Expansion_MRR", "Base_MRR", "Churn_MRR", "ARR"]
```

**Finance — 3-month rolling average for smoothing:**
```yaml
- name: "rule_smoothed_cogs"
  target_measure: "COGS_3M_Avg"
  body: "rolling_avg(COGS, 3)"
  declared_dependencies: ["COGS"]
```

**Demand planning — cumulative shipments:**
```yaml
- name: "rule_cumulative_shipped"
  target_measure: "YTD_Shipped"
  body: "cumulative(Units_Shipped)"
  declared_dependencies: ["Units_Shipped"]
```

### 4F.4 Key design questions

1. **How does `prev()` know which dimension is Time?**
   - Option A: require `kind: "Time"` (new dim kind). Cleanest; explicit.
   - Option B: use the dim named "Time" by convention. Fragile.
   - Option C: require a `temporal_dimension:` declaration at model level. Explicit but verbose.
   - **Recommendation:** Option A — introduce `kind: "Time"` as a new dimension kind. The validator enforces exactly one Time-kind dim exists. This is a schema-level addition (model_format_version stays at 1; additive field).

2. **Boundary behavior (first period, no prev):**
   - Returns `Null`. Consistent with the existing null-semantics (§7 of the kernel brief). Users who want a default use `if_null(prev(Revenue), 0)` or the 3E `coalesce`.

3. **Does `lag(measure, n)` support negative n (lead)?**
   - **Yes.** `lag(Revenue, -1)` = next period = `lead(Revenue, 1)`. Avoids adding a separate `lead` function. Negative lag is common in forecasting (you're looking forward).

4. **Partial windows in `rolling_avg`:**
   - The first N-1 periods compute the average of available data (partial window). Example: `rolling_avg(X, 3)` at period 2 averages periods 1-2 only. This matches Excel's AVERAGE behavior over a range that includes empty cells at the start.

5. **Element ordering for "previous":**
   - Time elements are ordered by their declaration order in the YAML (index position). `prev(X)` reads X at index `current_index - 1`. This means the YAML must declare time elements in chronological order — enforced by a lint (MC3010: "Time dimension elements appear non-chronological").

6. **Date functions (`days_between`, `period_index`):**
   - Operate on element metadata. Requires elements to carry optional `date:` metadata (e.g., `{ name: "Jan_2026", date: "2026-01-01" }`). If no date metadata, date functions return Null.
   - `period_index()` returns the 0-based position of the current Time element. Useful for linear trends: `Base + Slope * period_index()`.

### 4F.5 Performance implications

`prev()` and `lag()` are **cross-coordinate reads** — same as `actual_ref` in 3E. Each evaluation reads a cell at a different Time position. The dependency graph must capture these cross-Time dependencies for correct dirty propagation.

**Dirty propagation rule:** writing to `Revenue` at `Jan_2026` must dirty `Revenue_MoM_Growth` at `Feb_2026` (because Feb's `prev(Revenue)` reads Jan's value). This is a "forward dependency" — writing period N dirties period N+1's derived measures that use `prev`.

This is architecturally identical to `actual_ref`'s cross-Scenario dependency. The same dep-graph machinery handles both.

### 4F.6 Diagnostic codes

| Code | Fires when |
|---|---|
| MC1010 | `lag` called with non-integer second argument |
| MC1011 | `rolling_avg` called with non-positive window size |
| MC1012 | Time-series function used but no Time-kind dimension declared |

### 4F.7 Dependencies

- Phase 3E (for `safe_div`, `if_null` in common patterns around time-series)
- Optionally: `kind: "Time"` dimension kind addition (schema-level, no kernel change)

### 4F.8 What becomes possible

- Month-over-month / quarter-over-quarter growth rates
- Year-to-date running totals
- Moving averages for smoothing noisy metrics
- Trend lines via `period_index()` + linear coefficients
- Seasonal comparison (lag 12 for year-ago same month)

---

## 5. Phase 3G — Reference-Data Blocks

### 5G.1 Rationale

Real models need external reference data: industry benchmarks ("average SaaS CAC is $200"), lookup tables (tax brackets, seasonal factors, territory mappings), and threshold bands (red/yellow/green for KPI dashboards). Currently, users encode these as input measures with hardcoded values — losing provenance, source attribution, and updateability.

Phase 3G introduces the **"reference data in YAML"** pattern: new top-level blocks that declare structured reference data, plus formula functions that read from them.

### 5G.2 New YAML blocks

**`benchmarks:` block:**
```yaml
benchmarks:
  - name: "industry_cpc"
    description: "Average B2B SaaS CPC by channel"
    source: "WordStream 2025 Industry Benchmark Report"
    last_updated: "2025-03-15"
    values:
      Paid_Search: 5.50
      Paid_Social: 3.20
      Display: 1.80
      Email: 0.0

  - name: "industry_cvr"
    description: "Average B2B SaaS conversion rate"
    source: "Unbounce 2025 Conversion Benchmark Report"
    last_updated: "2025-06-01"
    values:
      Paid_Search: 0.034
      Paid_Social: 0.021
      Display: 0.008
```

**`lookup_tables:` block:**
```yaml
lookup_tables:
  - name: "tax_rate"
    description: "Corporate tax rate by market"
    keys: ["Market"]
    values:
      Florida: 0.055
      Georgia: 0.0575
      North_Carolina: 0.025
      New_York: 0.085

  - name: "seasonal_factor"
    description: "Monthly seasonality index (1.0 = average)"
    keys: ["Time"]
    values:
      Jan_2026: 0.75
      Feb_2026: 0.80
      Mar_2026: 0.90
      Apr_2026: 1.00
      May_2026: 1.10
      Jun_2026: 1.20
      Jul_2026: 1.15
      Aug_2026: 1.10
      Sep_2026: 1.05
      Oct_2026: 1.00
      Nov_2026: 1.20
      Dec_2026: 1.40
```

**`status_thresholds:` block:**
```yaml
status_thresholds:
  - name: "cpc_health"
    description: "CPC health bands by channel"
    bands:
      - label: "Good"
        max: 3.0
      - label: "Warning"
        max: 7.0
      - label: "Critical"
        max: null   # unbounded above
```

### 5G.3 Formula functions

| Name | Signature | Example formula | AST node |
|---|---|---|---|
| `benchmark` | `benchmark(name, key)` | `benchmark("industry_cpc", Channel)` | `Benchmark { name, key_expr }` |
| `lookup` | `lookup(table, key)` | `lookup("tax_rate", Market)` | `Lookup { table, key_expr }` |
| `bucket` | `bucket(value, threshold)` | `bucket(CPC, "cpc_health")` | `Bucket { value, threshold_name }` |
| `sum_over` | `sum_over(dim, measure)` | `sum_over(Channel, Spend)` | `SumOver { dimension, measure }` |

### 5G.4 Concrete user examples

**Marketing — CPC vs industry benchmark:**
```yaml
- name: "rule_cpc_vs_benchmark"
  target_measure: "CPC_vs_Industry"
  body: "safe_div(CPC, benchmark(\"industry_cpc\", Channel), 0) - 1"
  declared_dependencies: ["CPC"]
```

**Finance — after-tax profit:**
```yaml
- name: "rule_after_tax_profit"
  target_measure: "After_Tax_Profit"
  body: "Gross_Profit * (1 - lookup(\"tax_rate\", Market))"
  declared_dependencies: ["Gross_Profit"]
```

**Marketing — share of total spend:**
```yaml
- name: "rule_spend_share"
  target_measure: "Spend_Share"
  body: "safe_div(Spend, sum_over(Channel, Spend), 0)"
  declared_dependencies: ["Spend"]
```

**SaaS — health status:**
```yaml
- name: "rule_cac_status"
  target_measure: "CAC_Status_Rank"
  body: "bucket(CAC, \"cac_health\")"
  declared_dependencies: ["CAC"]
```

### 5G.5 Architectural note: the "reference-data-in-YAML" pattern

`benchmarks:`, `lookup_tables:`, and `status_thresholds:` are all instances of ONE pattern:

> **YAML declares named, structured, attributed reference data. Formula functions read from it by name at eval time. The data is static per model version (not per-cell dynamic).**

This is the same architectural pattern that Phase 3H extends to `fitted_models:` and `calibration_maps:`. Recognizing the pattern means the validator, the schema, and the eval machinery can be designed ONCE and reused across 3G + 3H.

### 5G.6 `sum_over` — the in-formula aggregation question

**Decision needed (owner input):** Should `sum_over(dim, measure)` ship in 3G or defer?

**Arguments FOR 3G:**
- Share-of-total is one of the most-requested patterns ("what % of spend goes to Paid Search?")
- The formula is simple: read the target measure across all elements of a dimension, sum them
- Users currently cannot compute this without external scripts

**Arguments FOR deferral:**
- `sum_over` triggers a multi-cell read PER EVAL. If the dimension has 50 elements, each cell that uses `sum_over` reads 50 cells. For a 2,500-cell cube this is manageable; for a 1M-cell cube, naive implementation is O(N*M).
- Performance risk: dirty propagation becomes N-to-1 (writing any Channel's Spend dirties EVERY Channel's Spend_Share)
- The dep-graph implications are similar to cross-coordinate reads but amplified

**Recommendation:** Ship in 3G with a documented performance caveat and a lint warning (MC3011: "sum_over on high-cardinality dimension may impact performance"). The common case (5-10 channels) is fine; the pathological case (10,000 SKUs) is the user's problem to avoid.

### 5G.7 `bucket` return type

`bucket()` returns a **numeric rank** (0, 1, 2, ...) corresponding to the band index, NOT the string label. String labels are display-layer concerns (Phase 6 UI reads the threshold definition to render "Good"/"Warning"/"Critical"). This avoids introducing string-valued cells into ScalarValue before Phase 3J.

### 5G.8 Diagnostic codes

| Code | Fires when |
|---|---|
| MC1013 | Unknown benchmark name referenced in formula |
| MC1014 | Unknown lookup table referenced in formula |
| MC1015 | Unknown threshold name referenced in formula |
| MC1016 | Lookup key dimension doesn't match table's declared key dimension |
| MC2030 | Benchmark `last_updated` is > 12 months old (lint warning: stale data) |
| MC2031 | Lookup table value referenced by no formula (lint: dead reference data) |

### 5G.9 Schema versioning

Adding new top-level blocks does NOT require `model_format_version: 2`. The blocks are optional (serde `#[serde(default)]`); existing models without them parse unchanged. This follows the Phase 3C precedent (`canonical_inputs:` was additive without a version bump).

### 5G.10 Dependencies

- Phase 3E (for `safe_div` used in benchmark-comparison patterns)
- Phase 3F (for time-keyed lookups like seasonal factors)
- No kernel changes

### 5G.11 What becomes possible

- Industry-standard comparisons with source attribution
- Tax/regulatory rate tables that update once per model version
- Seasonal adjustment factors declared in-model (not hardcoded as input measures)
- Share-of-total computations (spend share, revenue contribution)
- KPI health-status scoring for dashboard traffic lights

---

## 6. Phase 3H — Fitted-Model Evaluation

### 6H.1 Rationale

The keystone phase. Many business domains (demand planning, lead scoring, churn prediction, bid optimization) rely on fitted statistical models — linear regression, logistic regression, Ridge/Lasso/ElasticNet. Currently, these are evaluated externally (Python, R) and their outputs imported as input measures. Phase 3H brings the EVALUATION (not fitting) of these models into the cube formula engine.

**Critical distinction:** Mosaic does NOT fit models. Fitting is computationally expensive, iterative, and requires training data. Mosaic EVALUATES pre-fitted coefficients at cube read time. The fitting happens offline via `mc model fit` (a proposed CLI verb) or externally. The cube stores coefficients; the formula applies them.

### 6H.2 New YAML blocks

**`fitted_models:` block:**
```yaml
fitted_models:
  - name: "lead_score_v3"
    type: "logistic"             # linear | logistic | ridge | lasso | elastic_net
    target: "Conversion_Prob"
    fitted_at: "2026-04-15"
    training_rows: 45000
    metrics:
      auc: 0.82
      log_loss: 0.41
    intercept: -2.34
    coefficients:
      Spend: 0.00012
      Impressions: 0.000003
      Prior_Conversions: 0.45
      Channel_Score: 0.28

  - name: "demand_forecast_q2"
    type: "linear"
    target: "Forecast_Units"
    fitted_at: "2026-03-01"
    training_rows: 12000
    metrics:
      r_squared: 0.91
      rmse: 142.5
    intercept: 500.0
    coefficients:
      Historical_Units: 0.85
      Seasonal_Factor: 200.0
      Promo_Flag: 150.0
```

**`calibration_maps:` block:**
```yaml
calibration_maps:
  - name: "lead_score_calibration"
    description: "PAVA isotonic calibration for lead_score_v3"
    fitted_at: "2026-04-15"
    # Piecewise-linear calibration: raw_prob → calibrated_prob
    points:
      - { raw: 0.0, calibrated: 0.02 }
      - { raw: 0.1, calibrated: 0.08 }
      - { raw: 0.2, calibrated: 0.15 }
      - { raw: 0.3, calibrated: 0.22 }
      - { raw: 0.5, calibrated: 0.48 }
      - { raw: 0.7, calibrated: 0.71 }
      - { raw: 0.9, calibrated: 0.92 }
      - { raw: 1.0, calibrated: 0.98 }
```

### 6H.3 Formula functions

| Name | Signature | Example formula | AST node |
|---|---|---|---|
| `predict` | `predict(model_id, feat1, feat2, ...)` | `predict("lead_score_v3", Spend, Impressions, Prior_Conversions, Channel_Score)` | `Predict { model_id, features: Vec }` |
| `calibrate` | `calibrate(raw, map_id)` | `calibrate(Raw_Score, "lead_score_calibration")` | `Calibrate { value, map_id }` |

### 6H.4 Evaluation semantics

All model types share the SAME evaluator core:

```
linear:      intercept + sum(coeff_i * feature_i)
logistic:    sigmoid(intercept + sum(coeff_i * feature_i))
ridge/lasso/elastic_net: same as linear (regularization affects fitting, not eval)
```

Where `sigmoid(x) = 1 / (1 + exp(-x))`.

This is intentionally simple. The cube formula engine evaluates a dot product (+ optional sigmoid). Fitting (gradient descent, regularization tuning, cross-validation) happens OUTSIDE the cube — either via `mc model fit` or external Python/R scripts that output the coefficient YAML.

### 6H.5 Concrete user examples

**Marketing — lead scoring:**
```yaml
- name: "rule_lead_score"
  target_measure: "Lead_Score"
  body: "calibrate(predict(\"lead_score_v3\", Spend, Impressions, Prior_Conversions, Channel_Score), \"lead_score_calibration\")"
  declared_dependencies: ["Spend", "Impressions", "Prior_Conversions", "Channel_Score"]
```

**Demand planning — unit forecast:**
```yaml
- name: "rule_forecast_units"
  target_measure: "Forecast_Units"
  body: "predict(\"demand_forecast_q2\", Historical_Units, Seasonal_Factor, Promo_Flag)"
  declared_dependencies: ["Historical_Units", "Seasonal_Factor", "Promo_Flag"]
```

**Sports betting — implied probability from a fitted model:**
```yaml
- name: "rule_model_prob"
  target_measure: "Model_Win_Prob"
  body: "predict(\"win_model_v2\", Elo_Diff, Home_Advantage, Recent_Form, Injury_Score)"
  declared_dependencies: ["Elo_Diff", "Home_Advantage", "Recent_Form", "Injury_Score"]
```

### 6H.6 The `mc model fit` concept

**Decision needed (owner input):** Should `mc model fit` be a Mosaic-native verb or delegated to Python?

**Option A (native verb):**
```bash
mc model fit lead_score_v3 \
  --training-data ./data/conversions.csv \
  --type logistic \
  --features Spend,Impressions,Prior_Conversions,Channel_Score \
  --target Converted
```

Outputs: updates the `fitted_models:` block in the model YAML with new coefficients + metrics.

**Option B (Python delegation):**
```bash
python scripts/fit_model.py --output acme-model.yaml --block fitted_models.lead_score_v3
```

A Python script (using scikit-learn) fits and writes the YAML block.

**Recommendation:** Start with Option B (Python). Fitting is inherently a data-science workflow that benefits from the Python ecosystem (pandas, sklearn, cross-validation, hyperparameter tuning). Phase 3H's value is in-cube EVALUATION, not fitting. A native `mc model fit` can come later as a convenience wrapper that calls sklearn under the hood.

### 6H.7 Diagnostic codes

| Code | Fires when |
|---|---|
| MC1017 | Unknown model_id in `predict()` call |
| MC1018 | Feature count mismatch (formula provides N features; model declares M coefficients) |
| MC1019 | Unknown calibration map in `calibrate()` call |
| MC2032 | Model `fitted_at` is > 90 days old (lint: stale model warning) |
| MC2033 | Model coefficients reference measures not in the model (lint: orphan model) |

### 6H.8 Architectural keystone

Phase 3H's `fitted_models:` and `calibration_maps:` are the SAME pattern as 3G's `benchmarks:` and `lookup_tables:`. All four are instances of:

> **Named reference data declared in YAML, read at formula eval time, validated at load time, attributed with provenance metadata.**

The validator, schema types, and eval-time lookup machinery should be designed in 3G with 3H in mind. The only difference is the eval semantics: 3G lookups are direct key-value; 3H lookups are dot-product computations.

### 6H.9 Dependencies

- Phase 3E (for `if`, comparisons used in model-output branching)
- Phase 3G (shared reference-data-in-YAML architecture)
- Phase 3I (`exp()` function needed for sigmoid in logistic eval — but can be inlined in the predict evaluator without exposing `exp` as a user-facing function)

### 6H.10 What becomes possible

- Lead scoring with calibrated probabilities — directly in the cube
- Demand forecasting from fitted coefficients — instant scenario planning
- Risk scoring for credit/insurance models
- Sports model probability evaluation
- Any linear/logistic model evaluation without external Python at read time

---

## 7. Phase 3I — Mathematical and Statistical Primitives

### 7I.1 Rationale

Growth models (compound interest, NPV), gambling mathematics (Kelly criterion), statistical inference (normal distribution), and precision display (rounding) all require mathematical functions beyond basic arithmetic. These are pure math — no new YAML blocks, no cross-coordinate reads, no architectural changes.

### 7I.2 Primitives

| Name | Signature | Example formula | AST node |
|---|---|---|---|
| `pow` | `pow(base, exp)` | `pow(1 + Growth_Rate, Periods)` | `Pow { base, exponent }` |
| `sqrt` | `sqrt(x)` | `sqrt(Variance)` | `Sqrt { operand }` |
| `ln` | `ln(x)` | `ln(Price / Strike)` | `Ln { operand }` |
| `log10` | `log10(x)` | `log10(Users)` | `Log10 { operand }` |
| `exp` | `exp(x)` | `exp(-Lambda * Time)` | `Exp { operand }` |
| `round` | `round(x, n)` | `round(CPC, 2)` | `Round { value, decimals }` |
| `floor` | `floor(x)` | `floor(Units / Batch_Size)` | `Floor { operand }` |
| `ceil` | `ceil(x)` | `ceil(Demand / Container_Size)` | `Ceil { operand }` |
| `mod` | `mod(a, b)` | `mod(Period_Index, 12)` | `Mod { dividend, divisor }` |
| `norm_cdf` | `norm_cdf(x, mu, sigma)` | `norm_cdf(Score, 0, 1)` | `NormCdf { x, mean, std }` |
| `norm_inv` | `norm_inv(p, mu, sigma)` | `norm_inv(0.95, Expected, Std_Dev)` | `NormInv { p, mean, std }` |

### 7I.3 Concrete user examples

**Finance — NPV calculation:**
```yaml
- name: "rule_npv_factor"
  target_measure: "NPV_Discount_Factor"
  body: "pow(1 + Discount_Rate, -1 * period_index())"
  declared_dependencies: ["Discount_Rate"]
```

**Sports betting — Kelly criterion:**
```yaml
- name: "rule_kelly_fraction"
  target_measure: "Kelly_Bet_Size"
  body: "max(0, (Win_Prob * Odds - 1) / (Odds - 1))"
  declared_dependencies: ["Win_Prob", "Odds"]
```

**Demand planning — safety stock (normal distribution):**
```yaml
- name: "rule_safety_stock"
  target_measure: "Safety_Stock"
  body: "norm_inv(0.95, 0, 1) * sqrt(Lead_Time) * Demand_Std_Dev"
  declared_dependencies: ["Lead_Time", "Demand_Std_Dev"]
```

**SaaS — exponential decay for cohort retention:**
```yaml
- name: "rule_retention_curve"
  target_measure: "Expected_Retention"
  body: "exp(-Churn_Rate * period_index())"
  declared_dependencies: ["Churn_Rate"]
```

**Marketing — rounded display value:**
```yaml
- name: "rule_display_cpc"
  target_measure: "CPC_Display"
  body: "round(CPC, 2)"
  declared_dependencies: ["CPC"]
```

### 7I.4 Edge cases and null semantics

- `ln(0)` or `ln(negative)` → `Null` (not NaN, not error — consistent with div-by-zero returning Null per kernel §7)
- `sqrt(negative)` → `Null`
- `pow(0, negative)` → `Null` (division by zero equivalent)
- `norm_inv(0, ...)` or `norm_inv(1, ...)` → `Null` (undefined at boundaries)
- `mod(a, 0)` → `Null`

### 7I.5 Diagnostic codes

| Code | Fires when |
|---|---|
| MC1020 | `round` called with non-integer decimal places |
| MC1021 | `norm_cdf`/`norm_inv` called with sigma <= 0 (lint: always-invalid) |

### 7I.6 Dependencies

- Phase 3E (for `max(0, ...)` patterns used with Kelly, NPV floors)
- Phase 3F (for `period_index()` used in time-decay and NPV)
- No kernel changes, no new YAML blocks

### 7I.7 What becomes possible

- Net present value and IRR calculations
- Kelly criterion bet sizing
- Safety stock calculations (normal distribution quantiles)
- Exponential decay / growth curves
- Compound interest projections
- Rounded display values for reporting
- Modular arithmetic for cyclic patterns (seasons, weeks)

---

## 8. Phase 3J — Distributional Cells

### 8J.1 Rationale

The differentiation bet. Every competing planning tool stores point estimates. Mosaic Phase 3J stores **distributions** — a cell contains not just "expected revenue = $50K" but "revenue ~ N(50000, 5000)" (mean $50K, std dev $5K). This enables:

- Probability queries: "what's the probability revenue exceeds $45K?" → `prob_above(Revenue, 45000)` → 0.84
- Confidence intervals: "give me the 95th percentile" → `quantile(Revenue, 0.95)` → $58,200
- Proper uncertainty propagation through rule chains
- Probabilistic scoring (Brier score, log-loss) as aggregation methods

### 8J.2 Kernel-level change: ScalarValue extension

**This is the ONLY sub-phase that touches the kernel's ScalarValue.**

```rust
// Current (Phase 1):
pub enum ScalarValue {
    Null,
    Scalar(f64),
}

// Phase 3J addition:
pub enum ScalarValue {
    Null,
    Scalar(f64),
    Distribution { mean: f64, std: f64 },  // Normal distribution (sufficient for Phase 3J)
}
```

**Why Normal only (not arbitrary distributions):** Normal distributions are closed under addition and scalar multiplication — meaning rule chains that add/multiply distributions produce valid distributions without Monte Carlo sampling. This makes the kernel's lazy-eval model viable. Non-normal distributions (beta, gamma, etc.) require sampling-based propagation, which is Phase 4+ (if ever).

**Arithmetic on distributions (closed-form):**
- `Dist(m1, s1) + Dist(m2, s2)` = `Dist(m1+m2, sqrt(s1^2 + s2^2))` (assumes independence)
- `Dist(m, s) * k` = `Dist(m*k, s*|k|)` (scalar multiply)
- `Dist(m1, s1) * Dist(m2, s2)` = approximation via delta method (more complex)
- `Dist(m, s) + k` = `Dist(m+k, s)` (scalar add)

### 8J.3 Formula functions

| Name | Signature | Example formula | AST node |
|---|---|---|---|
| `predict_dist` | `predict_dist(model, ...features)` | `predict_dist("demand_model", ...)` | `PredictDist { model_id, features }` |
| `prob_above` | `prob_above(dist, threshold)` | `prob_above(Revenue, 45000)` | `ProbAbove { distribution, threshold }` |
| `prob_between` | `prob_between(dist, lo, hi)` | `prob_between(CPC, 2.0, 5.0)` | `ProbBetween { distribution, lo, hi }` |
| `quantile` | `quantile(dist, p)` | `quantile(Revenue, 0.95)` | `Quantile { distribution, percentile }` |
| `bootstrap_ci` | `bootstrap_ci(measure, n, alpha)` | `bootstrap_ci(ROAS, 1000, 0.05)` | `BootstrapCI { measure, samples, alpha }` |

### 8J.4 Concrete user examples

**Demand planning — probabilistic forecast:**
```yaml
fitted_models:
  - name: "demand_model_dist"
    type: "linear"
    returns: "distribution"      # NEW: model returns mean + std
    intercept: 500.0
    intercept_std: 45.0
    coefficients:
      Historical_Units: { mean: 0.85, std: 0.04 }
      Seasonal_Factor: { mean: 200.0, std: 15.0 }

rules:
  - name: "rule_demand_forecast"
    target_measure: "Forecast_Units"
    body: "predict_dist(\"demand_model_dist\", Historical_Units, Seasonal_Factor)"
    declared_dependencies: ["Historical_Units", "Seasonal_Factor"]

  - name: "rule_stockout_risk"
    target_measure: "Stockout_Probability"
    body: "1 - prob_above(Forecast_Units, Safety_Stock_Level)"
    declared_dependencies: ["Forecast_Units", "Safety_Stock_Level"]
```

**Sports betting — model confidence:**
```yaml
- name: "rule_confidence_band"
  target_measure: "Confidence_Width"
  body: "quantile(Win_Prob_Dist, 0.975) - quantile(Win_Prob_Dist, 0.025)"
  declared_dependencies: ["Win_Prob_Dist"]
```

**Finance — VaR (Value at Risk):**
```yaml
- name: "rule_var_95"
  target_measure: "VaR_95"
  body: "quantile(Portfolio_Return, 0.05)"
  declared_dependencies: ["Portfolio_Return"]
```

### 8J.5 New aggregation methods

| Method | Semantics |
|---|---|
| `Brier` | Brier score: mean squared error between predicted probability and binary outcome |
| `LogLoss` | Logarithmic loss: `-mean(y*ln(p) + (1-y)*ln(1-p))` |

These are proper scoring rules — they measure calibration quality and are meaningful aggregated across cells.

### 8J.6 Backward compatibility

- `ScalarValue::Scalar(f64)` remains the default. Existing models produce only Scalar values.
- When a Distribution enters an arithmetic node expecting a Scalar (e.g., `Distribution + Scalar`), the Scalar promotes to `Distribution { mean: scalar, std: 0.0 }` (zero uncertainty). Arithmetic proceeds.
- When a probability query (`prob_above`, `quantile`) is applied to a Scalar, it returns 0.0 or 1.0 (deterministic — no uncertainty).
- `predict()` (3H, returns Scalar) and `predict_dist()` (3J, returns Distribution) coexist. Users choose.

### 8J.7 Diagnostic codes

| Code | Fires when |
|---|---|
| MC1022 | `prob_above`/`prob_between`/`quantile` applied to non-distributional measure |
| MC1023 | `predict_dist` references model that doesn't declare `returns: "distribution"` |
| MC1024 | Distribution std is negative (validation error at model load) |

### 8J.8 Dependencies

- Phase 3H (fitted_models infrastructure)
- Phase 3I (`exp`, `sqrt`, `ln` used internally for normal CDF/inverse)
- Phase 3E (conditionals for distribution-aware branching)
- **Kernel ADR required** (same rigor as ADR-0010's WriteBatch unlock — this modifies ScalarValue)

### 8J.9 What becomes possible

- Probabilistic demand planning with stockout risk
- Portfolio risk analysis (VaR, CVaR)
- Model calibration assessment via proper scoring rules
- Confidence intervals on any forecast
- Decision-making under uncertainty (not just point estimates)

---

## 9. Domain coverage matrix

Which phase makes each domain "complete enough" for production use?

| Domain | 3E | 3F | 3G | 3H | 3I | 3J |
|---|---|---|---|---|---|---|
| **Marketing reporting** | 85% | **99%** | 100% | — | — | — |
| **Finance / FP&A** | 70% | 85% | 90% | 92% | **99%** | 100% |
| **Sports betting** | 75% | 80% | 85% | **95%** | 99% | 100% |
| **SaaS metrics** | 80% | **95%** | 98% | 99% | 100% | — |
| **Demand planning** | 60% | 80% | 85% | **95%** | 98% | 100% |
| **Retail / CPG** | 70% | 85% | **95%** | 97% | 99% | — |

**Reading:** percentage = "share of real-world formulas in this domain that Mosaic can express." Bold = the phase where the domain crosses the usable threshold.

**Key insight:** Phase 3E + 3F covers marketing and SaaS. Finance and demand planning need the full stack through 3H/3I. Sports betting and risk need 3J for proper differentiation. The sequencing is correct: ship value to the broadest audience first.

---

## 10. Architectural inflection points

### 10.1 Phase 3G: first schema extension since Phase 3C

Introducing `benchmarks:`, `lookup_tables:`, `status_thresholds:` as top-level YAML blocks is the first time the model format grows beyond its Phase 3A shape (dimensions + hierarchies + measures + rules + golden_tests + canonical_inputs + test_fixtures). The validator, the schema types in `mc-model/src/schema.rs`, and the compile pipeline all need extension points.

**Design implication:** build the reference-data architecture generically in 3G so 3H's `fitted_models:` and `calibration_maps:` slot in without architectural surgery.

### 10.2 Phase 3H: the "fitted artifact" pattern

The conceptual unification: benchmarks, lookups, fitted models, and calibration maps are all **named artifacts with metadata, declared in YAML, read at eval time.** The differences are only in eval semantics:

| Artifact type | Eval semantics |
|---|---|
| Benchmark | Key-based lookup (dim element → value) |
| Lookup table | Key-based lookup (dim element → value) |
| Status threshold | Range-based bucketing (value → band index) |
| Fitted model | Dot product + optional activation (features → prediction) |
| Calibration map | Piecewise-linear interpolation (raw → calibrated) |

If the eval dispatcher is designed as a trait or match arm per artifact type, adding new artifact types in future phases is trivial.

### 10.3 Phase 3J: kernel-level ScalarValue change

Phase 3J is a different category of work. It touches:
- `mc-core`'s `ScalarValue` enum (all 416+ existing tests must still pass)
- The arithmetic evaluator (must handle Distribution + Distribution, Distribution + Scalar)
- The consolidation engine (weighted average of distributions = ?)
- The dirty-propagation system (unchanged — operates on coordinates, not values)
- The serialization/persistence layer (if/when persistence ships)

This requires its own ADR with the same rigor as ADR-0010 (the WriteBatch kernel unlock). It should NOT be bundled with the formula-language expansion ADR.

---

## 11. Decisions needing owner input

### 11.1 Should `sum_over` ship in 3G or defer?

See Section 5G.6 above. The performance risk is bounded for typical cube sizes (< 100K cells) but real for large cubes. Recommendation: ship in 3G with lint warning.

### 11.2 Should `actual_ref` ship in 3E or get its own 3E.1?

`actual_ref` is architecturally different from the other 3E primitives — it's a cross-coordinate read requiring dep-graph changes, while all other 3E additions are pure local computation. However:
- The existing research note recommends shipping it with conditionals
- The user need (cross-scenario planning) is acute
- The dep-graph change is the same machinery needed for 3F's `prev()`/`lag()`

**Recommendation:** ship in 3E. The cross-coordinate read machinery is simpler than it appears (it's a targeted read at a known coordinate, not an arbitrary scan), and delaying to 3E.1 adds a release boundary for no architectural benefit.

### 11.3 Should string operations be included?

`concat`, `lower`, `upper`, `contains`, `starts_with`, `left`, `right`, `mid` — should any string operations exist in the formula language?

**Recommendation: defer indefinitely.** String operations belong in the recipe/transform layer (Phase 5 Tessera), not the formula engine. The cube stores numeric values (f64). String manipulation on dimension element names is a data-preparation concern, not a computation concern. If a user needs "first 3 chars of Market name," that's a lookup table or a recipe transform — not a cell formula.

### 11.4 Should `mc model fit` be Mosaic-native?

See Section 6H.6 above. Recommendation: start with Python delegation; native verb is future convenience.

---

## 12. Effort and sequencing summary

```
Phase 3D (DONE) ─── Phase 3E ─── Phase 3F ─── Phase 3G ─── Phase 3H
                                                                │
                                                          Phase 3I
                                                                │
                                                          Phase 3J
```

| Phase | Weeks | Cumulative | Ships independently? |
|---|---|---|---|
| 3E | 2-3 | 2-3 | Yes |
| 3F | 2-3 | 4-6 | Yes (depends on 3E for safe_div patterns) |
| 3G | 3-4 | 7-10 | Yes (depends on 3E) |
| 3H | 4-5 | 11-15 | Yes (depends on 3G architecture) |
| 3I | 2-3 | 13-18 | Yes (depends on 3E) |
| 3J | 6-8 | 19-26 | Yes (depends on 3H + 3I + kernel ADR) |

**3I can run in parallel with 3G/3H** — it has no YAML-block dependencies, only formula-level deps on 3E. The critical path is 3E → 3G → 3H → 3J.

---

## 13. Cross-links

- [ADR-0007](../decisions/0007-phase-3d-friendly-formula-syntax.md) — Phase 3D (the formula parser this extends)
- [cross-coordinate-formulas.md](cross-coordinate-formulas.md) — the original `actual_ref` research note
- [ADR-0010](../decisions/0010-phase-5-tessera-architecture.md) — Phase 5 Tessera (consumer of formula results)
- [SKILL.md (formulas)](../../mosaic-plugin/skills/formulas/SKILL.md) — current formula documentation
- [SKILL.md (schema-design)](../../mosaic-plugin/skills/schema-design/SKILL.md) — aggregation patterns
- [`mc-model/src/schema.rs`](../../crates/mc-model/src/schema.rs) — current ParsedRuleBody AST

---

*End of research note. Each sub-phase requires its own ADR before implementation begins. Phases 3E, 3F, and 3G ADRs are drafted concurrently with this note.*
