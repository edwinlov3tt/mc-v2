# Calculation Audit — Phase 6A.1 Gap Analysis

## Reviewer: Claude Sonnet 4.6
## Date: 2026-05-06
## Scope

Read: `docs/audits/AUDIT-PROTOCOL.md`, `~/Projects/email-matchback/models/tide-matchback.yaml`,
`~/Projects/email-matchback/models/tide-ltv-cohort.yaml`,
`~/Projects/email-matchback/models/tide-mmm.yaml`,
`~/Projects/email-matchback/scripts/mosaic/prepare_v2_inputs.py`,
`~/Projects/email-matchback/scripts/mosaic/prepare_mmm_inputs.py`,
`crates/mc-core/src/rule.rs` (Expr enum + eval_expr),
`crates/mc-model/src/formula.rs` (parser; all recognized functions),
`crates/mc-model/src/validate.rs` (all validators),
`crates/mc-model/src/schema.rs` (full YAML schema),
`docs/research-notes/formula-language-expansion.md`,
`docs/research-notes/cross-coord-dep-graph.md`.

---

## Closed by 6A/6A.1 (verification)

### G-CLOSED-1: Per-leaf seasonality indices as hardcoded Input measures

**Was:** Earliest version of the matchback model stored 2025-month seasonality indices as per-leaf Input measures in the CSV (referenced in `prepare_v2_inputs.py:62–69` via `SEASONALITY_NAME` dict). Every Time element needed a separate row per market per measure — hundreds of rows of reference data masquerading as business inputs.

**Now:** `tide-matchback.yaml:168–238` declares five `lookup_tables:` entries (`houston_revenue_seasonality`, etc.), keyed by Time element. Rules at lines 297–322 use `lookup("table_name", Time)`. The CSV no longer carries seasonality rows: `prepare_v2_inputs.py:200–203` explicitly prints "per-leaf seasonality rows: 0 (now in YAML lookup_tables)".

**Evidence:** Phase 3G (`lookup`, `benchmark`, `status_thresholds` blocks). Validate logic at `crates/mc-model/src/validate.rs:1247–1400`.

---

### G-CLOSED-2: IsPast flag as Input measure

**Was:** Original v1 model injected `IsPast = 1.0/0.0` per (Time, Market) row to drive `if(IsPast, actual_ref(M), forecast)` branching. Generating these rows required Python.

**Now:** `tide-matchback.yaml:297` uses `if(is_past() or is_current(), actual_ref(MatchedRevenue), ...)`. Phase 3F.1 anchor functions (`is_past()`, `is_current()`, `is_future()`, `periods_since_anchor()`, `periods_to_end()`, `anchor_index()`) ship natively. `prepare_v2_inputs.py:203` confirms "IsPast rows: 0 (now via is_past()/is_current() with --time-anchor)".

**Evidence:** `crates/mc-model/src/formula.rs:668–695` (parser dispatch for `is_past`, `is_current`, `is_future`); `crates/mc-core/src/rule.rs:92–98` (Expr variants).

---

### G-CLOSED-3: Conditional / logical formulas

**Was:** No conditional logic in formula engine. Every branching rule was pre-computed in Python.

**Now:** Phase 3E adds `if(cond, then, else)`, `and`, `or`, `not`, comparisons (`>`, `<`, `>=`, `<=`, `==`, `!=`), `min`, `max`, `abs`, `safe_div`, `clamp`, `coalesce`. Confirmed via `crates/mc-model/src/formula.rs:481–596`.

**Evidence:** `tide-matchback.yaml:297–322` — every unified rule uses `if(is_past() or is_current(), ..., ...)` with no Python pre-computation.

---

### G-CLOSED-4: Time-series operators (prev / lag / cumulative / rolling_avg)

**Was:** Lag features, running sums, and rolling averages required Python ETL before ingestion.

**Now:** Phase 3F + 3F.1. `tide-mmm.yaml:177–186` uses `lag(AdSpend, 1)` and `rolling_avg(AdSpend, 3)` natively. `tide-ltv-cohort.yaml:183–186` uses `cumulative(RevenuePerActive)`. Parser at `crates/mc-model/src/formula.rs:601–652`. The MMM lag/roll goldens at `tide-mmm.yaml:223–232` lock the values.

---

### G-CLOSED-5: Fitted-model evaluation (predict / calibrate / norm_cdf)

**Was:** Model evaluation ran entirely in `fit_mmm.py` (240 lines) then imported predictions as Input measures.

**Now:** `tide-mmm.yaml:189–213` evaluates the pooled MMM via `predict("mmm_v1", ...)`, computes probabilistic exceedance via `norm_cdf(...)`, and classifies ROAS via `bucket(...)`. Zero Python evaluation at read time. Parser: `crates/mc-model/src/formula.rs:781–846`. Schema: `crates/mc-model/src/schema.rs:630–732`.

---

### G-CLOSED-6: MC2005 false positive on dimension-name key expressions in lookup()

**Was:** `prepare_v2_inputs.py:17–24` documents that `lookup("table", Time)` would fire MC2005 ("rule references unknown measure") because `Time` was treated as a measure reference by the dep-graph validator.

**Now:** `crates/mc-model/src/validate.rs:508–512` explicitly collects `known_dimensions` and at lines 539–567 excludes dimension names from both the unknown-measure check and the declared-dependencies check. `tide-matchback.yaml:297` uses `lookup("houston_revenue_seasonality", Time)` without listing `Time` in `declared_dependencies`.

---

## Open gaps — clear path

### G-OPEN-1: No formula syntax for coordinate-identity tests — forces 464-row indicator CSV

**Use case:** The MMM needs 1.0/0.0 one-hot features per market (IsHouston, IsAustin, IsDenver, IsAmarillo) to pass to `predict()`. The engine already knows the Market coordinate at eval time.

**Evidence (Python):** `prepare_mmm_inputs.py:70–85` generates `4 markets × 29 months × 4 indicators = 464 rows`. The indicator generation loop is the entire second half of the script. `tide-mmm.yaml:151–154` stores four Input measures carrying nothing but coordinate identity.

**Evidence (Mosaic absence):** `Expr::DimElement` exists at `crates/mc-core/src/rule.rs:107` and is used internally when compile resolves `lookup("t", Market)` — the dim reference is compiled to `DimElement(dim_id)` as a key expression. But there is no user-facing formula function that exposes this. There is no `is_element(DimName, "ElementName")` function in `crates/mc-model/src/formula.rs`. There are no string literals in the formula language (planned Phase 3I, per `docs/research-notes/formula-language-expansion.md:719–733`), so `Market == "Houston"` cannot be written.

**Impact:**
- Lines of Python eliminated: ~20 (the indicator generation loop)
- CSV rows eliminated: 464
- Other affected use cases: any pooled model requiring market/channel dummy features; geography-conditional rules; tiered pricing by segment

**Proposed fix shape:** Two independent paths converge on this. Path A (shortest): add `is_element(DimName, "ElementValue")` as a zero-arity formula function that evaluates to 1.0/0.0 at the current coordinate — maps cleanly to `Expr::DimElement` already in the kernel. Path B (broader): Phase 3I string literal support enables `if(current_element(Market) == "Houston", 1.0, 0.0)` via a new `current_element(DimName)` function returning a string value for comparison. Path B also unlocks `switch`-style dimension-based branching.

**Phase mapping:** Path A fits Phase 3I (string literals + formula unification). Path B is Phase 3I.

---

### G-OPEN-2: No `parameters:` block — forces anchor constants broadcast to every Time leaf

**Use case:** The Q1-2026 per-dollar revenue anchor (`Q1_2026_RevPerDollar_Anchor`) is a scalar constant per Market that doesn't vary by Time — but the formula reads it as a measure at the current (Time, Market) coordinate, so it must be stored at every leaf.

**Evidence (Python):** `prepare_v2_inputs.py:180–194` broadcasts the computed anchor to every Time element: `for t in time_leaves: new_rows.append({..., "Time": t, "Measure": anchor_measure, "Value": ...})`. Five anchor measures × ~29 months × 4 markets = ~580 CSV rows storing identical constants.

**Evidence (Mosaic absence):** `crates/mc-model/src/schema.rs:25–61` `ParsedModel` has no `parameters:` block. There is no way to declare a named scalar that is keyed only by (Scenario, Version, Market, Channel) without also varying by Time. The closest workaround — a lookup table keyed by Market — only works for market-level constants, not for constants that also vary by measure type.

**Impact:**
- Lines of Python eliminated: ~30 (the broadcast loop in prepare_v2_inputs.py)
- CSV rows eliminated: ~580
- Other affected use cases: any model with calibration constants, conversion rates, or budget targets that are constant across the Time dimension; FP&A annual budget targets broadcast monthly

**Proposed fix shape:** A `parameters:` top-level YAML block where each entry declares `name:`, `scope: { dim: value, ... }` for partial-coord fixing, and `body:` as a formula evaluated once per scope combination. The result is addressable by other rules as a named measure with the missing dimensions treated as "any."

**Phase mapping:** Needs new phase or 3I amendment.

---

### G-OPEN-3: No `extrapolate_last_value` or carry-forward rule — forces Python extension

**Use case:** The email-matchback spreadsheet stops AdSpend at October 2026 for some markets. The forecast rules need AdSpend at every future Time leaf (Nov, Dec 2026). Python extends it by repeating the last known value.

**Evidence (Python):** `prepare_v2_inputs.py:154–176` and `prepare_mmm_inputs.py:47–65` — both scripts contain identical loops that find the last Actual AdSpend value per market and inject rows for `M_2026_11` and `M_2026_12` at that value.

**Evidence (Mosaic absence):** There is no `extrapolate_last_value(measure)` or `carry_forward(measure)` function in `crates/mc-model/src/formula.rs`. The closest approximation would be `if_null(AdSpend, lag(AdSpend, 1))` applied recursively, but `lag()` with a Null source returns Null — it doesn't "fill forward." The formula `if_null(AdSpend, if_null(lag(AdSpend, 1), lag(AdSpend, 2)))` requires the user to hard-code the maximum gap length.

**Impact:**
- Lines of Python eliminated: ~25 (both scripts combined)
- Other affected use cases: any model where actuals lag data availability by 1-2 months; budget freeze patterns; "last-known-value" baseline projections; FP&A models where year-end data arrives late

**Proposed fix shape:** Add `extrapolate_last_value(measure)` — a time-series function that scans backward from the current Time element and returns the first non-Null value found. Also useful: `fill_forward(measure, max_periods)` with a gap-length guard. Both are pure time-series reads; no new YAML block required.

**Phase mapping:** Phase 3F amendment or 3I.

---

### G-OPEN-4: `actual_ref()` has no fallback — forces Plan→Actual AdSpend mirror

**Use case:** Future-month formulas read `actual_ref(AdSpend)` to get "what was actually spent." For months that haven't happened yet, the Actual scenario has no AdSpend recorded, so `actual_ref(AdSpend)` returns Null. Python mirrors the Plan AdSpend value into the Actual scenario so the formula has data to read.

**Evidence (Python):** `prepare_v2_inputs.py:134–152` and `prepare_mmm_inputs.py:34–46` — both scripts contain identical "mirror Plan→Actual" loops that inject ~20-30 rows per run.

**Evidence (Mosaic absence):** `crates/mc-model/src/schema.rs:430–434` `ParsedActualRefBody` holds only `measure: String`. There is no second argument for a fallback. If `actual_ref(AdSpend)` returns Null, the only current option is `if_null(actual_ref(AdSpend), Plan_AdSpend)` — which requires AdSpend to be declared twice (once as Actual, once as Plan) and forces the formula author to construct the Plan cross-reference manually. There is no `scenario_ref(Measure, "ScenarioName")` function that can read from an arbitrary scenario.

**Impact:**
- Lines of Python eliminated: ~25 (mirror loops in both scripts)
- Other affected use cases: any rolling-forecast model where future-month actuals haven't yet landed; Forecast scenario that falls back to Plan values; version-controlled models that read prior-version actuals

**Proposed fix shape:** Either (a) extend `actual_ref(measure, fallback_expr)` to accept an optional second argument evaluated when the Actuals cell is Null; or (b) add `scenario_ref(measure, "ScenarioName")` as a generalization of `actual_ref` that lets formulas read from any named scenario. Option (b) subsumes option (a) and also enables `coalesce(scenario_ref(M, "Actual"), scenario_ref(M, "Plan"), 0)` patterns.

**Phase mapping:** Phase 3E amendment or dedicated 3E.2.

---

### G-OPEN-5: `lookup_tables` supports only a single key dimension

**Use case:** The MMM and matchback models need lookup tables keyed by (Time × Measure) or (Market × Time) — e.g., per-market per-month seasonality factors. Today, the only option is one lookup table per measure, producing five tables in tide-matchback.yaml with near-identical structure.

**Evidence (Python):** Not Python per se, but `tide-matchback.yaml:169–238` declares five lookup tables with identical structure (`key_dimension: "Time"`, value map) that differ only in the measure they represent. This is a consequence of the single-key constraint.

**Evidence (Mosaic absence):** `crates/mc-model/src/schema.rs:596–604` `ParsedLookupTable` has `key_dimension: String` (singular). The validator at `crates/mc-model/src/validate.rs:1298–1329` validates one key dimension. There is no `key_dimensions: Vec<String>` field.

**Impact:**
- Model verbosity reduced: 5 lookup tables collapse to 1
- Other affected use cases: territory-by-time rate tables; product-by-channel margin tables; any cross-dimensional reference data

**Proposed fix shape:** Add optional `key_dimensions: Vec<String>` to `ParsedLookupTable` alongside the existing `key_dimension` (backward-compatible). The `values:` map keys become composite strings (e.g., `"Houston::M_2026_04": 2.358449`) or the schema gains a nested map form. The `lookup()` formula function signature becomes `lookup("table", DimRef1, DimRef2, ...)` for multi-key tables.

**Phase mapping:** Phase 3G amendment.

---

### G-OPEN-6: No `output_bound` on fitted models — negative-revenue Amarillo case

**Use case:** The pooled Ridge MMM (`tide-mmm.yaml:121–143`) predicts negative matched revenue for the Amarillo market in some months. The real value cannot be negative; the model should clamp predictions to ≥ 0.

**Evidence (Python):** Not mitigated in Python currently — `rule_predicted_revenue` in `tide-mmm.yaml:188–193` uses bare `predict(...)` with no floor. The golden at `tide-mmm.yaml:253` acknowledges the Amarillo -$5,706 case in comments.

**Evidence (Mosaic absence):** `crates/mc-model/src/schema.rs:633–646` `ParsedFittedModel` has no `output_bound` field. Users must wrap every predict call: `max(0, predict("mmm_v1", ...))`, which is verbose and easy to forget.

**Impact:**
- Correctness risk: any model where the fitted coefficients can extrapolate beyond the training range produces unbounded outputs
- Other affected use cases: logistic regression output should be bounded [0, 1] by construction (sigmoid), but linear and Ridge models have no inherent bound

**Proposed fix shape:** Add optional `output_bound: { min: float, max: float }` to `ParsedFittedModel`. The eval path applies `clamp(result, min, max)` after the dot product. Validation fires MC2056 if `min > max`. Logistic models could have `output_bound: { min: 0, max: 1 }` auto-enforced.

**Phase mapping:** Phase 3H amendment.

---

### G-OPEN-7: Phase 3I math primitives absent — blocks financial and statistical formulas

**Use case:** FP&A models (NPV, compound growth), demand planning (safety stock), and gambling (Kelly criterion) require `pow`, `sqrt`, `ln`, `exp` (user-facing), `round`, `floor`, `ceil`, `mod`, `norm_inv`.

**Evidence (Python):** Not directly in email-matchback (the matchback model didn't need these). Theoretical — per `docs/research-notes/formula-language-expansion.md:629–715`, the domain-coverage matrix shows Finance FP&A reaching only 85% without Phase 3I.

**Evidence (Mosaic absence):** `crates/mc-model/src/formula.rs:817–843` has `exp()` and `norm_cdf()` (added for Phase 3H sigmoid and probability needs), but `pow`, `sqrt`, `ln`, `log10`, `round`, `floor`, `ceil`, `mod`, `norm_inv` are all absent. The parser at line 847 returns `MC1007` for any unknown function name.

**Impact:**
- Lines of Python eliminated: entire NPV / Kelly / safety-stock computation classes
- Other affected use cases: SaaS cohort retention curves (`exp(-churn * period)`), compounding interest, safety stock (`norm_inv(0.95) * sqrt(lead_time) * sigma`), fiscal period modular arithmetic (`mod(period_index(), 12)`)

**Proposed fix shape:** Add 9 new parser cases and Expr variants per `docs/research-notes/formula-language-expansion.md:629–715`. No new YAML blocks. No kernel changes. Edge-case semantics: `ln(0)`, `sqrt(-1)`, `mod(a, 0)` → `Null` (consistent with div-by-zero → Null per engine §7).

**Phase mapping:** Phase 3I (committed).

---

### G-OPEN-8: `predict()` feature-count mismatch not validated

**Use case:** A user calls `predict("mmm_v1", AdSpend)` (1 feature) for a model with 6 coefficients. The formula parser accepts this silently.

**Evidence (Python):** n/a — theoretical.

**Evidence (Mosaic absence):** `crates/mc-model/src/validate.rs:1706–1768` `check_fitted_model_blocks` validates method, non-empty coefficients, and standardization params — but does NOT validate that the number of features in any `predict()` call matches the coefficient count. The validator at lines 1247–1400 checks reference-data blocks but not the cross-reference between formula arity and model declaration. The formula parser at `crates/mc-model/src/formula.rs:781–796` accepts any number of features after the model name string.

**Impact:**
- Silent wrong results: if fewer features than coefficients are passed, the missing features contribute zero to the dot product, silently producing incorrect predictions
- If more features than coefficients are passed, overflow behavior depends on the eval implementation

**Proposed fix shape:** In the validator's `check_rules_reference_known_measures` or a new `check_predict_arities` pass, collect all `ParsedRuleBody::Predict` nodes in rule bodies, resolve the model name against `parsed.fitted_models`, and verify `features.len() == model.coefficients.len()`. Emit MC2057 on mismatch.

**Phase mapping:** Phase 3H amendment (validation fix, no new functionality).

---

### G-OPEN-9: `norm_cdf` sigma ≤ 0 not validated at model-load time

**Use case:** `norm_cdf(50000.0, PredictedRevenue, 63702.5556)` is correct. But `norm_cdf(x, mu, 0)` or a sigma-valued measure that becomes zero at runtime produces undefined results.

**Evidence (Python):** n/a — theoretical.

**Evidence (Mosaic absence):** `crates/mc-model/src/validate.rs:1706–1846` does not include a static check for sigma-valued expressions in `norm_cdf`. The research note at `docs/research-notes/formula-language-expansion.md:699` identifies diagnostic code MC1021 ("norm_cdf/norm_inv called with sigma <= 0") but this is not implemented in validate.rs. Eval-time behavior with sigma=0 or sigma=Null would produce NaN or Infinity in the normal CDF approximation, violating the engine's NaN-exclusion invariant (engine-semantics §7).

**Impact:**
- If sigma resolves to 0.0 at eval time, the CDF is undefined; the engine likely produces +Inf or NaN rather than Null, violating the NaN-never-enters-storage invariant

**Proposed fix shape:** (a) Static lint: if sigma argument is a literal constant ≤ 0, fire MC1021 at validate time. (b) Runtime guard: in the eval path for `NormCdf`, if sigma ≤ 0 after evaluation, return `ScalarValue::Null` rather than propagating NaN.

**Phase mapping:** Phase 3H or 3I bugfix (no new functionality).

---

### G-OPEN-10: `sum_over` supports only Sum — no `avg_over`, `min_over`, `max_over`

**Use case:** `Revenue_Share = safe_div(Revenue, sum_over(Market, Revenue), 0)` works. But computing a market-average ROAS requires the weighted average of ROAS across markets — `avg_over(Market, ROAS, AdSpend)` (weighted). Currently impossible without pre-computing AdSpend total separately.

**Evidence (Python):** Not directly observed — theoretical gap for the FP&A and analytics use cases.

**Evidence (Mosaic absence):** `crates/mc-model/src/formula.rs:760–778` — `sum_over` is the only cross-dimension aggregation function. `crates/mc-core/src/rule.rs:104` has `SumOver(DimensionId, ElementId)` — hardcoded to sum semantics.

**Impact:**
- Cannot compute "max across all channels" or "average across all markets" without multiple rules
- Other affected use cases: performance relative to median channel spend; top-N ranking patterns

**Proposed fix shape:** Extend `sum_over` to a family: `sum_over(dim, measure)`, `avg_over(dim, measure)`, `min_over(dim, measure)`, `max_over(dim, measure)`, `wavg_over(dim, measure, weight)`. Each adds one parser case and one kernel Expr variant.

**Phase mapping:** Phase 3G amendment or 3I.

---

### G-OPEN-11: Aggregation rules — only Sum / WeightedAverage / Min / Max

**Use case:** FP&A models need median NPS score, variance of regional margins, first/last value in a time period.

**Evidence (Python):** Not observed in email-matchback — theoretical for analytics use cases.

**Evidence (Mosaic absence):** `crates/mc-model/src/validate.rs:854–922` `check_aggregation_methods_supported` accepts only `"Sum"`, `"WeightedAverage"`, `"Min"`, `"Max"` — rejects anything else with `UnsupportedAggregation`. No `"Median"`, `"Variance"`, `"StdDev"`, `"First"`, `"Last"`, `"DistinctCount"` variants.

**Impact:**
- Survey / satisfaction models (median NPS per region) cannot consolidate correctly
- Risk models (variance of returns) must compute variance as derived measure manually
- Time-ordered first/last value semantics (beginning-of-period vs. end-of-period balance)

**Proposed fix shape:** Add `"Median"`, `"First"`, `"Last"`, `"Variance"`, `"DistinctCount"` to the supported aggregation methods. `First`/`Last` require knowing the Time dimension order (already enforced). `Median` requires sorting the child set at consolidation time (O(N log N) vs. O(N) for Sum). Each is a `mc-core` change to the consolidation engine.

**Phase mapping:** Needs new phase (touches kernel consolidation logic).

---

### G-OPEN-12: `if()` does not short-circuit `and`/`or` sub-expressions

**Use case:** The unified-revenue rule `if(is_past() or is_current(), actual_ref(MatchedRevenue), ...)` — the `or` expression evaluates both `is_past()` and `is_current()` eagerly. For pure functions like anchor predicates, this is harmless. But for a formula like `if(SpendFlag or lag(AdSpend, 1) > 0, ...)`, the `lag()` cross-coord read fires even when `SpendFlag` is already truthy.

**Evidence (Python):** n/a — the email-matchback formulas use `or` only between anchor predicates (pure, cheap). Theoretical performance impact.

**Evidence (Mosaic absence):** `crates/mc-core/src/rule.rs:546–562` — `And` and `Or` evaluate both sides unconditionally before applying the logical. By contrast, `if()` at lines 575–583 IS short-circuiting (evaluates only the matching branch). The asymmetry means `and`/`or` can trigger unnecessary cross-coord reads in complex formulas.

**Impact:**
- Performance: in models with expensive cross-coord reads in `or` predicates, every cell evaluation pays full cost even when the first predicate short-circuits
- Correctness: not a correctness issue, only performance

**Proposed fix shape:** Change the `And` eval to check left side first; if falsy, return 0.0 without evaluating right. Change `Or` to check left; if truthy, return 1.0 without evaluating right. This is a standard short-circuit evaluation change consistent with how `if()` already behaves.

**Phase mapping:** Phase 3I or a targeted 6A.2 fix.

---

### G-OPEN-13: No `ifs()` / `switch()` — nested `if` chains required

**Use case:** The `bucket()` function exists for threshold classification, but arbitrary multi-way branching requires deeply nested `if(cond1, val1, if(cond2, val2, if(cond3, val3, default)))` chains, which are hard to read and error-prone.

**Evidence (Python):** Not directly observed — theoretical for dashboard segmentation, scenario-type branching.

**Evidence (Mosaic absence):** `crates/mc-model/src/formula.rs:481–497` — only `if(cond, then, else)` with exactly 3 arguments. No `ifs(c1, v1, c2, v2, default)` variadic form. No `switch(key, case1, val1, case2, val2, default)`.

**Impact:**
- Ergonomics: models with 4-6 branches become unreadable
- Error risk: a missed `else` in a nested chain defaults to Null silently

**Proposed fix shape:** Add `ifs(c1, v1, c2, v2, ..., default)` as a variadic function that evaluates condition-value pairs left to right, returning the value for the first truthy condition. Maps to a chain of `If` Expr nodes at compile time; no kernel change needed.

**Phase mapping:** Phase 3I.

---

## Open gaps — needs design

### G-DESIGN-1: Indicator / dummy-variable generation — three viable shapes

**Use case:** The MMM one-hot market indicators (`IsHouston`, `IsAustin`, etc.) must be generated externally today. More broadly, any model that needs to branch on "which element am I at" in a dimension requires either pre-loaded Input rows or formula support.

**Evidence:** `prepare_mmm_inputs.py:70–85` generates 464 rows. `tide-mmm.yaml:151–154` declares four Input measures. The engine's `Expr::DimElement` at `crates/mc-core/src/rule.rs:107` is used internally but unexposed.

**Why design is non-obvious:** Three shapes each have different tradeoffs:

1. **String literal + `==` operator (Phase 3I):** `if(current_element(Market) == "Houston", 1.0, 0.0)` — requires string literals in formula language, a `current_element(DimName)` function, and string comparison semantics in `Eq`. Most general; unlocks arbitrary conditional-on-dimension-value. But string equality for measures (all currently f64) may create type confusion.

2. **`is_element(DimName, "ElementName")` predicate:** A purpose-built boolean function that evaluates to 1.0/0.0 at the current coordinate. Narrower than option 1 (can't compose with other string ops), but simpler to implement and reason about. Maps directly to `Expr::DimElement` already in the kernel.

3. **`indicators:` declarative block:** A new YAML top-level block where you declare `indicators: [{name: "IsHouston", dimension: "Market", element: "Houston"}]`. The engine auto-generates the 1.0/0.0 measure without any formula. Cleanest for the MMM use case but adds a new schema block and doesn't generalize to conditional-on-coordinate logic in formulas.

**Alternatives:**
- Option 1 (string literals + current_element): Most general; required anyway for Phase 3I filter-parser unification. Highest effort.
- Option 2 (is_element predicate): Fast path; solves the indicator problem in isolation. Doesn't solve the broader "string in formula" need.
- Option 3 (indicators: block): Zero formula complexity; solves exactly the dummy-variable generation use case. Doesn't solve conditional-on-coordinate formulas.

**Phase mapping:** Needs ADR before phase scoping. String literals route is Phase 3I. Predicate function could be earlier.

---

### G-DESIGN-2: Scenario-fallback chain — no design consensus

**Use case:** `actual_ref(AdSpend)` returns Null for future months. Python mirrors Plan→Actual to provide data. The right fix is "try Actual, fall back to Plan" — but the current engine has no primitive for this.

**Evidence:** `prepare_v2_inputs.py:134–152`, `prepare_mmm_inputs.py:34–46` — Plan→Actual mirror loops.

**Why design is non-obvious:** Three viable shapes:

1. **`actual_ref(measure, fallback_expr)`:** Extend the existing `actual_ref` to accept an optional second argument. Simple, targeted; but `actual_ref` currently hardcodes the `actuals_element` scenario — the fallback would evaluate in the calling scenario's context. The semantics of "evaluate fallback in Forecast when Actual is Null" need specification.

2. **`scenario_ref(measure, "ScenarioName")`:** A new function that reads `measure` from an explicitly-named scenario, regardless of the caller's scenario. `coalesce(scenario_ref(M, "Actual"), scenario_ref(M, "Plan"), 0)` then expresses priority chains explicitly. More general than (1). Requires cross-scenario reads to be graph-tracked (MAJ-3 territory).

3. **Model-level scenario inheritance:** A YAML declaration like `scenario_fallback: Actual -> Plan` that the engine applies at read time whenever a cell is Null in the Actual scenario. No formula change; entirely declarative. Less flexible but simpler for users who always want the same fallback chain.

**Alternatives:** (1) is backwards-compatible. (2) is most powerful. (3) is most declarative but less composable. All require addressing MAJ-3 cross-coord dep-graph tracking for correctness at scale.

**Phase mapping:** Needs ADR. Coupled to MAJ-3 resolution.

---

### G-DESIGN-3: Cross-coordinate dependency graph — MAJ-3 still open, scope still correct

**Use case:** Writing to any Input cell currently triggers bulk revision-bump invalidation of all cached derived cells. For cubes with cross-coord rules (`prev`, `lag`, `cumulative`, `rolling_avg`, `actual_ref`, `sum_over`), this means a write to `AdSpend[Jan]` invalidates every derived cell in the cube rather than just the cells that depend on `Jan`'s AdSpend.

**Evidence:** `docs/research-notes/cross-coord-dep-graph.md:16–36` — the cross-coord reads bypass `actual_reads` at `cube.rs:457`; correctness preserved via revision-bump. The three email-matchback cubes all make heavy use of cross-coord operators: 5 `actual_ref` calls in tide-matchback, 1 `lag` + 1 `rolling_avg` in tide-mmm, 1 `cumulative` in tide-ltv-cohort.

**Why design is non-obvious:** `docs/research-notes/cross-coord-dep-graph.md:38–59` identifies five unresolved architectural questions:
- Concrete-only vs. parametric edges (storage vs. dirty-walk complexity)
- Window fan-in for rolling_avg (O(N·w) concrete edges)
- Cumulative fan-in O(T) per target cell
- time-anchor as a "writeable" dependency
- Whether to keep bulk invalidation as a fallback for untracked operators

**Alternatives:** See cross-coord-dep-graph.md §"Possible directions." The five cross-coord operators span the design space; no single approach fits all cleanly.

**Phase mapping:** Needs ADR. The audit did not surface new information that would sharpen or narrow the MAJ-3 scope beyond what the research note already captures. The scope remains correct.

---

### G-DESIGN-4: Carry-forward extrapolation interacts with is_future() and time_anchor

**Use case:** Extending AdSpend to Nov/Dec via carry-forward needs to be aware that the anchor is March 2026 — so "future months with no data" should carry-forward, but "past months with no data" (genuinely missing actuals) should return Null. The distinction requires knowing `is_future()`.

**Evidence:** `prepare_v2_inputs.py:155–176` does not distinguish past-gaps from future-gaps — it applies carry-forward only to months not yet in the data. But the formula approach (`extrapolate_last_value`) would need the time-anchor to correctly scope which months get the extrapolation treatment.

**Why design is non-obvious:** Three interaction modes between extrapolation and anchor:

1. **Unconditional carry-forward:** `extrapolate_last_value(AdSpend)` fills any Null by walking backward. Fills both past-gaps and future-gaps equally. Simple but wrong for past-gaps (a genuinely missing actual should stay Null for data integrity).

2. **Anchor-conditional carry-forward:** `if(is_future(), extrapolate_last_value(AdSpend), AdSpend)` — only fill-forward for future periods. Requires extrapolation to compose correctly with is_future as a guard. This is the right semantic but requires is_future() to short-circuit the extrapolation lookup.

3. **Extrapolation rule kind:** A new rule scope / kind `scope: "FutureLeaves"` that only fires for periods after the anchor. Would apply the body formula exclusively to future cells, leaving past cells as Input. Requires the scope system to expand beyond "AllLeaves."

**Alternatives:** (1) simple but semantically wrong. (2) correct but compositionally fragile. (3) cleanest but requires scope system extension.

**Phase mapping:** Needs ADR. Coupled to scope system design.

---

### G-DESIGN-5: `--where` filter parser vs. formula parser duality — capability unlocked but design committed

**Use case:** `mc model query --where "CAC_Matched < 30"` currently uses a custom Phase-6A filter parser in mc-cli. Computed measures (e.g., `CAC_Matched` is derived) can be referenced in filters but the filter parser is not the formula parser.

**Evidence (capability):** `docs/research-notes/formula-language-expansion.md:719–733` commits to unifying the filter parser with the formula parser in Phase 3I. The two-parser state is "explicitly temporary."

**Why design is non-obvious:** Unifying the parsers unlocks three new capabilities that aren't currently planned:

1. **Filter on derived measures computed during query:** `--where "predict('mmm_v1', ...) > 50000"` — the filter would need to trigger rule evaluation during filtering, not just dimension-element matching. This couples query planning to the rule engine in a new way.

2. **Filter expressions as formula bodies:** If the formula parser handles `--where`, then the full formula language (including cross-coord functions) could appear in filters. A filter like `--where "lag(Revenue, 1) > Revenue"` would require cross-coord reads during filter evaluation — the dependency tracking implications are undefined.

3. **Named filter aliases:** A model could declare `named_filters: [{name: "high_cac", expr: "CAC_Matched > 50"}]` and queries reference them by name. This is only possible if filters and formulas share a parser.

**Alternatives:**
- (a) Unify parsers but limit filter expressions to locally-computed formulas (no cross-coord). MC1013-style validation at filter parse time.
- (b) Unify parsers with full formula support, accepting the complexity.
- (c) Share the parser module but maintain separate validation rules for filter vs. formula contexts.

**Phase mapping:** Phase 3I (committed). Design decision on filter-formula interaction needed before implementation.

---

### G-DESIGN-6: Monte Carlo / goal-seek / optimization — no clear path in current phase plan

**Use case:** "Find the AdSpend allocation across 4 markets that maximizes total PredictedRevenue subject to total_budget ≤ $100K." "What ROAS does Amarillo need to achieve for AllMarkets ROAS to exceed 2.0?" These are first-class business questions in FP&A and marketing.

**Evidence (Mosaic absence):** Not in any current phase plan. `docs/research-notes/formula-language-expansion.md` covers 3E through 3J; 3J adds distributional cells and probability queries but not optimization or goal-seek.

**Why design is non-obvious:** These are fundamentally different computational modes from rule evaluation:

1. **Goal-seek** (one-dimensional): find Input X such that formula F(X) = target. Well-defined for monotone functions; requires the engine to expose a "solve for X" interface that iterates writes and reads. No existing hook in the `Cube` API.

2. **Optimization** (multi-dimensional): find Input vector X that maximizes/minimizes objective. Requires: (a) defining an objective formula; (b) gradient computation or numerical differentiation; (c) constraint handling. Far beyond the current formula engine.

3. **Monte Carlo**: repeated sampling of Input distributions to estimate output distributions. Requires either `ScalarValue::Distribution` (Phase 3J) or an external sampling loop — both require architectural extension.

**Alternatives:**
- Recipe-layer (Phase 5 Tessera): optimization lives in a Tessera recipe that uses `mc model query` in a loop, never in the formula engine itself. Correct architectural choice for optimization.
- Native goal-seek primitive: feasible for one-dimensional monotone functions. Could be `goal_seek(formula, target, input_measure, lo, hi)` returning the root by bisection.

**Phase mapping:** Needs ADR. Goal-seek might fit Phase 3I or 5D. Multi-dimensional optimization belongs in Tessera.

---

### G-DESIGN-7: Multi-frequency Time dimensions — daily with monthly rollups

**Use case:** An e-commerce model needs daily Revenue for operational tracking but monthly aggregation for forecasting. A daily Time dim with ~730 elements (2 years) and a monthly hierarchy would enable both, but `cumulative()`, `lag()`, and `rolling_avg()` would operate on daily granularity — and `period_index()` returns 0–729, making monthly period comparisons awkward.

**Evidence (Mosaic absence):** `crates/mc-model/src/validate.rs:1086–1107` enforces `time_dim_count <= 1` (MC2036). There is no mechanism for declaring multiple granularities within one Time dimension, or for declaring a "coarse" hierarchy alongside a "fine" one with different temporal semantics.

**Why design is non-obvious:**

1. **Single Time dim with mixed-granularity elements:** Declare both daily and monthly elements in one Time dimension, using hierarchy to group daily elements under months. `cumulative()` and `lag()` would need to respect granularity for their offset semantics ("lag 1 month" vs. "lag 1 day" are both "lag 1 period" today).

2. **Multiple Time dims (relax MC2036):** Allow two Time-kind dimensions with different granularities. `lag()` and `cumulative()` would need a `dimension:` argument to specify which Time axis to operate on. Significant parser and kernel change.

3. **Separate cubes + cross-cube reference:** Daily and monthly cubes are separate; monthly values are reads from the daily cube's consolidation level. No cross-cube reads today.

**Alternatives:** (1) is simpler but requires granularity-aware time-series functions. (2) is cleanest semantically but has the highest implementation cost. (3) works today but requires cross-cube infrastructure.

**Phase mapping:** Needs ADR. Multi-frequency is a common FP&A need; the current one-Time-dim constraint will be a hard blocker for daily-to-monthly models.

---

## Edge cases / latent bugs found during audit

### E-1: `rolling_avg` partial-window boundary behavior unspecified in schema

**Where:** `crates/mc-model/src/formula.rs:634–652` (parser, no semantics) and `crates/mc-core/src/rule.rs:88` (Expr variant, no inline docs).

**What I expected:** `rolling_avg(AdSpend, 3)` at the first period (index 0) should return `AdSpend[0]` (window of size 1), not Null. The research note at `docs/research-notes/formula-language-expansion.md:237–238` says "partial windows compute the average of available data."

**What I observed:** The schema and parser accept `rolling_avg(measure, window)` but the YAML schema for `ParsedRollingAvgBody` carries no metadata about partial-window policy. The eval behavior is in `cube.rs` (not read in this audit). The golden at `tide-mmm.yaml:229–232` only checks `rolling_avg` at a non-boundary period (March 2025 = index 7). The boundary golden at `tide-mmm.yaml:278–281` tests `AdSpend_Lag1` at Aug 2024 (index 0) returning 0.0 via `if_null(lag(...), 0)` — but there is no golden for `rolling_avg` at index 0 or 1.

**Impact:** If `rolling_avg` returns Null for partial windows, `AdSpend_Roll3` at Aug-Sep 2024 would be Null, and `predict(...)` would evaluate to Null for the first two months — silently dropping them from any analysis. No golden locks this behavior.

---

### E-2: `if()` truthy test uses 1e-9 epsilon — integer band comparisons may surprise users

**Where:** `crates/mc-core/src/rule.rs:580–582`.

**What I expected:** `if(CAC_HealthBand == 0, ...)` would be truthy when `bucket()` returns exactly 0.0 (the Excellent band).

**What I observed:** `if(cond, then, else)` evaluates the condition, then checks `x.abs() < 1e-9` for falsiness. This means anything with absolute value < 1e-9 is treated as 0.0 (false). `CAC_HealthBand == 0` produces 1.0 (true) or 0.0 (false) via the `Eq` comparison — so the if condition is 0.0 or 1.0, and the 1e-9 epsilon correctly handles 0.0. However, `if(CAC_HealthBand, ...)` (using the band index itself as the condition) would treat band 0 as "false" — which is unintuitive if the user means "if a band is assigned."

**Impact:** Low — users who write `if(bucket(CAC, "bands"), ...)` and expect "any valid band is truthy" would be surprised that band 0 is falsy. Not a bug, but needs documentation.

---

### E-3: `calibrate()` out-of-range behavior unspecified

**Where:** `crates/mc-model/src/schema.rs:690–732` (schema), `crates/mc-model/src/validate.rs:1771–1845` (validation).

**What I expected:** If `calibrate(raw, "map")` is called with a raw value outside [min_raw, max_raw] of the calibration map's points, the behavior should be documented.

**What I observed:** The validator checks that PAVA points are in ascending raw order and there are ≥ 2 points, but does not specify boundary behavior. The eval (not read in this audit) may clamp, extrapolate linearly, or return Null for out-of-range inputs. No golden in tide-mmm.yaml tests `calibrate()` at boundary conditions. The `PBeatBenchmark` rule at `tide-mmm.yaml:208–212` uses `norm_cdf` directly rather than a calibration map, so no actual calibration boundary test exists in the shipped models.

**Impact:** If the eval extrapolates beyond the calibration range rather than clamping, calibrated probabilities could exceed [0, 1] for extreme inputs — an invariant violation for probability measures.

---

### E-4: `Eq` comparison precision — `band_index == 2` may produce unexpected results

**Where:** `crates/mc-core/src/rule.rs:540–542`.

**What I expected:** `if(ROASHealthBand == 2, ...)` where `bucket()` returns exactly 2.0 (a count, not a float computation) should evaluate to true.

**What I observed:** `Eq` uses `(l - r).abs() < 1e-9`. `bucket()` returns integer-indexed values (0.0, 1.0, 2.0, 3.0) that should compare exactly to integer literals. This works correctly in practice because `2.0 - 2.0 = 0.0 < 1e-9`. However, if a derived measure accumulates floating-point error through arithmetic before being compared to an integer — e.g., `round(some_ratio) == 2` — the 1e-9 tolerance may silently succeed or fail. This is not a bug in the epsilon choice but a documentation gap: users should know that `==` in Mosaic is approximate equality.

**Impact:** Minimal for current models but could cause confusion in heavily-derived band-comparison formulas.

---

### E-5: `lag(measure, k)` with negative k (lead) — support unconfirmed

**Where:** `crates/mc-core/src/rule.rs:87` (`Lag(ElementId, Box<Expr>)`) and the eval dispatch in `cube.rs` (not fully read in this audit).

**What I expected:** `lag(AdSpend, -1)` would return next-period AdSpend. The research note at `docs/research-notes/formula-language-expansion.md:233–234` says "Yes" — negative lag is a lead — and cites this as deliberate design to avoid a separate `lead()` function.

**What I observed:** The parser at `crates/mc-model/src/formula.rs:615–633` accepts any expression for the periods argument, including negative literals. The validator at `crates/mc-model/src/validate.rs` has no MC1010 check ("lag called with non-integer second argument") — this code was planned in the research note but not implemented. The kernel `Lag` variant stores a `Box<Expr>` for periods that is evaluated at runtime. Whether the `cube.rs` eval correctly handles negative period offsets (walking forward in the Time element list rather than backward) is not confirmed without reading the full `resolve_cross_coord_read` implementation.

**Impact:** If negative lags are silently mishandled (treated as 0 or ignored), forecasting rules that look at next-period values would silently return the current period's value, producing wrong-but-non-crashing results. No golden covers negative-lag behavior.

---

## Confirmed working (sanity checks only)

- `is_past()`, `is_current()`, `is_future()` with `time_anchor` declaration — used in tide-matchback.yaml rules; goldens lock correct branching between actual-ref and lookup paths.
- `lookup("table", DimRef)` with dimension name as key expression — tide-matchback.yaml:297; MC2005 is correctly suppressed for dimension-name references (validate.rs:508–512).
- `predict("model", f1, ..., f6)` with z-score standardization — tide-mmm.yaml:251–252 golden pins `PredictedRevenue` to 116201 ± 500.
- `norm_cdf(x, mu, sigma)` — tide-mmm.yaml:265–269 golden pins `PBeatBenchmark` to 0.85 ± 0.02.
- `cumulative(measure)` over the TenureMonth axis — tide-ltv-cohort.yaml:241–248 goldens pin `CumulativeLTV` at T_03 and T_12.
- `bucket(value, "threshold")` — tide-matchback.yaml:394–401 and tide-ltv-cohort.yaml:261–268 goldens confirm 0-indexed band classification.
- `sum_over(dimension, measure)` — tide-matchback.yaml:342–344 uses `sum_over(Market, MatchedRevenue_Unified)`; revenue-share golden at line 409 confirms cross-market sum is computed.
- `rolling_avg(measure, 3)` — tide-mmm.yaml:229–232 golden confirms `AdSpend_Roll3` = 20343.33 at a non-boundary period.
- `WeightedAverage` with `weight_measure` consolidation — tide-matchback.yaml:283–284 `CAC_Matched` and `ROAS` use this correctly; schema enforces weight_measure presence (validate.rs:873–889).

---

## What I couldn't verify

1. **`rolling_avg` partial-window behavior at index 0/1** — eval code in `cube.rs:resolve_cross_coord_read` was not fully read; behavior at boundaries not locked by any golden.

2. **`lag` with negative periods** — `cube.rs` eval of negative period offsets not confirmed; no golden tests negative-lag behavior.

3. **`calibrate()` out-of-range behavior** — eval path for values outside calibration map bounds not confirmed.

4. **Exact dirty-set behavior for cross-coord writes** — MAJ-3 analysis confirmed from the research note but the specific cell counts affected by a write to a cell with `lag` or `actual_ref` dependents was not re-verified against current code.

5. **`predict()` feature-count mismatch at eval time** — what the kernel does when fewer features than coefficients are passed was not confirmed by reading the eval path.

6. **Whether `output_bound` missing on the logistic predict path produces out-of-[0,1] values** — the `tide-mmm.yaml` model uses `method: "linear"`, not `"logistic"`, so the sigmoid path was not exercised by the goldens.
