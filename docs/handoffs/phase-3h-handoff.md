# Phase 3H Handoff — Fitted-Model Evaluation

> **Audience:** the Claude Code instance running in
> `/Users/edwinlovettiii/Projects/mc-v2/` that picks up Phase 3H.
> **You inherit a green Phase 5C** (578+ tests, all formula expansion
> through Phase 3G shipped, time_anchor from 3F.1 shipped).
>
> **This phase adds the architectural keystone for ML/forecasting
> domains:** the ability to evaluate pre-fitted statistical models
> (Lasso, Ridge, logistic regression, etc.) INSIDE Mosaic formulas.
> The user declares a fitted model in YAML (coefficients + metadata);
> the formula engine evaluates it via `predict()`. Fitting happens
> OUTSIDE Mosaic (Python/sklearn/R); evaluation happens INSIDE.
>
> **Why this matters:** Phase 3E-3G gave Mosaic conditionals, time-series,
> and reference data. Phase 3H gives it **prediction** — the ability to
> say "given these features, what does the model predict?" as a formula.
> This is the feature that makes Mosaic useful for sports betting
> (predict game totals from pace/rating features), demand forecasting
> (predict next-quarter revenue from leading indicators), and prospect
> scoring (predict conversion probability from engagement features).
>
> **Hard rule:** Phase 3H extends `mc-model` (parser, schema, validator,
> evaluator) to add new YAML blocks and formula functions. It does NOT
> add fitting/training capability (that stays in Python). It does NOT
> modify `mc-core`'s `ScalarValue` type (that's Phase 3J). It adds
> EVALUATION only.

---

## The one paragraph you must internalize before writing code

**One primitive, many models.** Lasso, Ridge, ElasticNet, OLS, and
logistic regression are ALL the same operation at evaluation time:
multiply features by coefficients, sum, optionally apply a link
function (sigmoid for logistic). If you build `predict()` as one
generic evaluator that reads coefficients from YAML, you get all
five model types for free. Do NOT build separate `predict_lasso()`,
`predict_ridge()`, `predict_logistic()` functions — build ONE
`predict(model_id, ...features)` that dispatches on `method:` in
the YAML declaration.

The fitted-artifact pattern (YAML block declares parameters; formula
function evaluates them) is the SAME pattern already established by
Phase 3G's `benchmarks:` / `lookup_tables:` / `status_thresholds:`.
Phase 3H adds `fitted_models:` and `calibration_maps:` as two more
instances of that pattern. The plumbing is identical — new top-level
YAML block, new schema types, new formula functions that read from it.

---

## ADR context

Phase 3H was documented in the research doc at
`docs/research-notes/formula-language-expansion.md` (the Phase 3H
section). There is no standalone ADR-0015 yet — this handoff serves
as the binding contract. If the scope surfaces decisions that need
an ADR, file a SPEC QUESTION.

Key architectural decisions (from the research doc + owner direction):

1. **Mosaic evaluates, Python fits.** The `fitted_models:` YAML block
   holds coefficients produced by external tools (sklearn, R,
   statsmodels, custom code). Mosaic never runs gradient descent or
   cross-validation — it loads pre-computed weights and does a dot
   product.

2. **One evaluator, multiple model types.** The `method:` field in the
   YAML dispatches: `linear` (dot product), `logistic` (dot product +
   sigmoid), `gam_eval` (future — sum of spline evaluations per
   feature). Phase 3H ships `linear` + `logistic` only.

3. **Forward-compatible with Phase 3J distributional cells.** When
   Phase 3J adds `ScalarValue::Distribution(mean, std)`, the fitted
   model's `residual_std` field enables `predict_dist()` to return a
   distribution instead of a point estimate. Phase 3H ships
   `predict()` (point estimate) only; `predict_dist()` is Phase 3J.

4. **Calibration maps** (PAVA/Platt) are a second fitted-artifact type.
   `calibrate(raw_prob, map_id)` applies a monotonic mapping from raw
   model output → calibrated probability. Same pattern as predict() —
   YAML declares the map; formula evaluates it.

---

## Phase 3H prompt (verbatim — this is your contract)

> **Scope items:**
>
> 1. **New YAML top-level block: `fitted_models:`**
>
>    ```yaml
>    fitted_models:
>      - name: "nba_total_v1_lasso"
>        method: "linear"              # "linear" | "logistic"
>        intercept: 211.34
>        coefficients:
>          - { feature: "avg_pace", weight: 3.016 }
>          - { feature: "combined_off_rating", weight: 0.548 }
>          - { feature: "avg_recent_total_10", weight: 0.331 }
>          - { feature: "combined_def_rating", weight: 0.602 }
>          - { feature: "home_missing_top_scorers", weight: -1.203 }
>        standardization:              # optional; if model was fit on z-scored inputs
>          method: "zscore"
>          params:
>            - { feature: "avg_pace", mean: 99.2, std: 4.7 }
>            - { feature: "combined_off_rating", mean: 113.4, std: 8.2 }
>            - { feature: "combined_def_rating", mean: 110.1, std: 7.8 }
>            - { feature: "avg_recent_total_10", mean: 222.5, std: 15.3 }
>            - { feature: "home_missing_top_scorers", mean: 0.12, std: 0.33 }
>        residual_std: 17.251          # for future predict_dist() (Phase 3J)
>        metadata:
>          fitted_at: "2026-04-08T12:00:00Z"
>          algorithm: "lasso"
>          alpha: 0.7
>          n_train: 3685
>          holdout_mae: 13.783
>    ```
>
> 2. **New YAML top-level block: `calibration_maps:`**
>
>    ```yaml
>    calibration_maps:
>      - name: "nba_totals_calibration_v1"
>        method: "pava"                # "pava" | "platt"
>        points:                        # for pava: monotonic mapping points
>          - { raw: 0.50, calibrated: 0.42 }
>          - { raw: 0.55, calibrated: 0.46 }
>          - { raw: 0.60, calibrated: 0.50 }
>          - { raw: 0.65, calibrated: 0.55 }
>          - { raw: 0.70, calibrated: 0.61 }
>          - { raw: 0.75, calibrated: 0.68 }
>          - { raw: 0.80, calibrated: 0.76 }
>        platt_params:                  # for platt: sigmoid parameters
>          a: -1.2
>          b: 0.3
>        metadata:
>          fitted_at: "2026-04-30T01:29:14Z"
>          sample_size: 1312
>          raw_brier: 0.2887
>          calibrated_brier: 0.2499
>    ```
>
> 3. **New formula function: `predict(model_id, feature1, feature2, ...)`**
>
>    Evaluates a fitted model by name. Arguments after model_id are
>    feature values (measures or expressions) passed IN THE ORDER
>    declared in the model's `coefficients:` array.
>
>    Evaluation logic for `method: "linear"`:
>    ```
>    1. For each feature in coefficients order:
>       a. Read the feature value (the Nth argument to predict())
>       b. If standardization exists for this feature:
>          z = (value - mean) / std
>       c. Else: z = value
>       d. weighted = z * weight
>    2. result = intercept + sum(weighted values)
>    3. Return result as f64
>    ```
>
>    Evaluation logic for `method: "logistic"`:
>    ```
>    Same as linear, then:
>    4. result = 1.0 / (1.0 + exp(-linear_result))   # sigmoid
>    5. Return result (probability between 0 and 1)
>    ```
>
>    **Null handling:** if ANY feature value is Null, `predict()` returns
>    Null (Null-poisoning, same as arithmetic operators).
>
>    **Feature imputation (optional, future):** a `feature_imputation:`
>    block could declare default values for Null features. Defer to
>    Phase 3H.1 — Phase 3H ships Null-poisoning only.
>
> 4. **New formula function: `calibrate(raw_value, map_id)`**
>
>    Applies a calibration map to transform a raw probability into a
>    calibrated probability.
>
>    For `method: "pava"` (isotonic regression / Pool Adjacent Violators):
>    ```
>    1. Find the two adjacent points in the map where
>       points[i].raw <= raw_value < points[i+1].raw
>    2. Linear interpolate between calibrated[i] and calibrated[i+1]
>    3. If raw_value < first point: return first calibrated value (clamp)
>    4. If raw_value > last point: return last calibrated value (clamp)
>    ```
>
>    For `method: "platt"` (Platt scaling / sigmoid):
>    ```
>    result = 1.0 / (1.0 + exp(a * raw_value + b))
>    ```
>
>    **Null handling:** `calibrate(Null, ...)` returns Null.
>
> 4b. **New formula function: `exp(x)` — standalone exponential**
>
>    Returns `e^x` (Euler's number raised to the power of x).
>    Needed internally by logistic predict + Platt calibrate, but
>    also exposed standalone for:
>    - Exponential growth: `Starting_MRR * exp(Growth_Rate * period_index())`
>    - Exponential decay: `Base_Value * exp(-Decay_Rate * periods_since_anchor())`
>    - Black-Scholes components, Kelly criterion variants, etc.
>
>    Implementation: one line — `x.exp()` (Rust's built-in f64::exp).
>    Null handling: `exp(Null)` returns Null.
>
> 4c. **New formula function: `norm_cdf(x, mu, sigma)` — normal distribution CDF**
>
>    Returns P(X ≤ x) where X ~ Normal(mu, sigma). The probability
>    that a normally-distributed variable is less than or equal to x.
>
>    **This is the bridge from point estimates to probability-aware forecasting.**
>    With predict() giving a point estimate and residual_std giving
>    the model's uncertainty, norm_cdf lets users ask:
>    - "What's the probability this stock returns > 5%?"
>      `body: "1 - norm_cdf(0.05, Expected_Return, Model_Std)"`
>    - "Probability game total goes over the spread?"
>      `body: "1 - norm_cdf(Market_Line, Predicted_Total, 17.25)"`
>    - "Probability of meeting quarterly revenue target?"
>      `body: "1 - norm_cdf(Target, Forecast, Forecast_Std)"`
>
>    Implementation: Abramowitz & Stegun 26.2.17 polynomial approximation
>    (~15 lines, ~7.5e-8 accuracy, zero deps):
>
>    ```rust
>    fn norm_cdf(x: f64, mu: f64, sigma: f64) -> f64 {
>        let z = (x - mu) / sigma;
>        let t = 1.0 / (1.0 + 0.2316419 * z.abs());
>        let d = 0.3989422804014327 * (-z * z / 2.0).exp();
>        let p = d * t * (0.3193815 + t * (-0.3565638
>            + t * (1.781478 + t * (-1.8212560 + t * 1.330274))));
>        if z > 0.0 { 1.0 - p } else { p }
>    }
>    ```
>
>    Null handling: if ANY of x, mu, sigma is Null → returns Null.
>    Edge case: if sigma ≤ 0 → returns Null (invalid distribution).
>
>    **Why pull forward from Phase 3I:** norm_cdf is the single primitive
>    that transforms "I have a prediction" into "I have a probability."
>    Without it, predict() gives you a number but you can't ask "how
>    likely is the outcome above/below a threshold?" — which is THE
>    question sports bettors, stock investors, and demand planners ask.
>    Implementation cost: 15 lines, zero deps. Value: transforms 3H
>    from "evaluate models" to "evaluate models AND reason about
>    probabilities."
>
> 5. **Validation rules (new diagnostic codes):**
>
>    | Code | Fires when |
>    |---|---|
>    | MC2050 | `predict()` references a model_id not in `fitted_models:` |
>    | MC2051 | `predict()` argument count doesn't match model's coefficient count |
>    | MC2052 | `calibrate()` references a map_id not in `calibration_maps:` |
>    | MC2053 | Duplicate name in `fitted_models:` or `calibration_maps:` |
>    | MC2054 | Calibration map points not in ascending `raw` order |
>    | MC2055 | Calibration map has < 2 points (can't interpolate) |
>    | MC2056 | Standardization declares a feature not in coefficients list |
>    | MC3017 | Lint: fitted_model metadata.fitted_at > 6 months old (staleness) |
>    | MC3018 | Lint: calibration_map metadata.fitted_at > 6 months old |
>
> 6. **Schema types (new Rust types in mc-model):**
>
>    ```rust
>    #[derive(Debug, Clone, Serialize, Deserialize)]
>    pub struct ParsedFittedModel {
>        pub name: String,
>        pub method: FittedModelMethod,      // Linear | Logistic
>        pub intercept: f64,
>        pub coefficients: Vec<FittedCoefficient>,
>        pub standardization: Option<StandardizationConfig>,
>        pub residual_std: Option<f64>,
>        pub metadata: Option<FittedModelMetadata>,
>    }
>
>    #[derive(Debug, Clone, Serialize, Deserialize)]
>    pub enum FittedModelMethod { Linear, Logistic }
>
>    #[derive(Debug, Clone, Serialize, Deserialize)]
>    pub struct FittedCoefficient {
>        pub feature: String,
>        pub weight: f64,
>    }
>
>    #[derive(Debug, Clone, Serialize, Deserialize)]
>    pub struct StandardizationConfig {
>        pub method: String,         // "zscore" (only supported method for now)
>        pub params: Vec<StandardizationParam>,
>    }
>
>    #[derive(Debug, Clone, Serialize, Deserialize)]
>    pub struct StandardizationParam {
>        pub feature: String,
>        pub mean: f64,
>        pub std: f64,
>    }
>
>    #[derive(Debug, Clone, Serialize, Deserialize)]
>    pub struct FittedModelMetadata {
>        pub fitted_at: Option<String>,
>        pub algorithm: Option<String>,
>        pub alpha: Option<f64>,
>        pub n_train: Option<usize>,
>        pub holdout_mae: Option<f64>,
>    }
>
>    #[derive(Debug, Clone, Serialize, Deserialize)]
>    pub struct ParsedCalibrationMap {
>        pub name: String,
>        pub method: CalibrationMethod,      // Pava | Platt
>        pub points: Option<Vec<CalibrationPoint>>,
>        pub platt_params: Option<PlattParams>,
>        pub metadata: Option<CalibrationMetadata>,
>    }
>
>    #[derive(Debug, Clone, Serialize, Deserialize)]
>    pub enum CalibrationMethod { Pava, Platt }
>
>    #[derive(Debug, Clone, Serialize, Deserialize)]
>    pub struct CalibrationPoint {
>        pub raw: f64,
>        pub calibrated: f64,
>    }
>
>    #[derive(Debug, Clone, Serialize, Deserialize)]
>    pub struct PlattParams {
>        pub a: f64,
>        pub b: f64,
>    }
>    ```
>
> 7. **New AST nodes (extends ParsedRuleBody):**
>    - `Predict { model_id: String, features: Vec<Box<ParsedRuleBody>> }`
>    - `Calibrate { map_id: String, value: Box<ParsedRuleBody> }`
>    - `Exp(Box<ParsedRuleBody>)` — standalone exponential
>    - `NormCdf { x: Box<ParsedRuleBody>, mu: Box<ParsedRuleBody>, sigma: Box<ParsedRuleBody> }` — normal CDF
>
> 8. **Parser extension:**
>    - `predict("model_name", expr1, expr2, ...)` — variable-arity function; first arg is string literal (model ID), rest are expressions
>    - `calibrate(expr, "map_name")` — two-arg function; first is the value to calibrate, second is string literal (map ID)
>    - `exp(expr)` — single-arg function
>    - `norm_cdf(x_expr, mu_expr, sigma_expr)` — three-arg function
>
> 9. **Serializer extension (round-trip):**
>    - `Predict` → `predict("model_name", serialize(f1), serialize(f2), ...)`
>    - `Calibrate` → `calibrate(serialize(value), "map_name")`
>    - `Exp` → `exp(serialize(x))`
>    - `NormCdf` → `norm_cdf(serialize(x), serialize(mu), serialize(sigma))`
>
> 10. **Eval extension (in mc-core or mc-model's eval path):**
>     - `Predict`: look up model by name in a models registry passed via eval context; evaluate per the method-specific logic above
>     - `Calibrate`: look up map by name; interpolate or apply Platt sigmoid
>     - Both need access to the parsed fitted_models/calibration_maps data at eval time (passed through EvalContext or similar)
>
> 11. **Example model (sports betting — NBA totals prediction):**
>
>     Create `examples/sports-betting/nba-totals.yaml` demonstrating:
>     - Input measures: avg_pace, combined_off_rating, combined_def_rating, avg_recent_total_10, home_missing_top_scorers
>     - Fitted model: the Lasso from scope item 1's example
>     - Derived measure: `Predicted_Total` with body `predict("nba_total_v1_lasso", avg_pace, combined_off_rating, avg_recent_total_10, combined_def_rating, home_missing_top_scorers)`
>     - Calibration map: the PAVA from scope item 2's example
>     - Derived measure: `Win_Probability` with body using `calibrate(...)`
>     - Golden tests pinning specific predictions against known coefficient outputs
>
> 12. **Plugin skill:** `mosaic-plugin/skills/fitted-models/SKILL.md`
>     - How to declare fitted_models in YAML
>     - How to use predict() in formulas
>     - How calibration maps work
>     - How to export from sklearn/R to Mosaic YAML format
>     - The "Mosaic evaluates, Python fits" principle
>
> **Hard rules:**
>
> - `mc-core`: gains eval arms for `Predict` + `Calibrate` ONLY. No new deps. No unsafe. No async.
> - `mc-model`: primary target (schema, parser, validator, formula, inspect, lint, compile).
> - `mc-fixtures`, `mc-cli`, `mc-drivers`, `mc-recipe`, `mc-tessera`: LOCKED (0-line diff).
> - All existing tests pass unchanged. New YAML blocks are `#[serde(default)]`.
> - `schema_version` stays `"1.0"` (adding optional blocks is backwards-compatible).
> - No `mc model fit` verb in Phase 3H (fitting is external). That's Phase 3H.1+.
> - No `predict_dist()` (distributional output). That's Phase 3J.
> - No feature imputation (Null-poisoning for now). That's Phase 3H.1.
> - Toolchain stays Rust 1.78. No new dependencies.
>
> **Acceptance gates:**
>
> 1. All existing tests pass unchanged (578+).
> 2. `predict("model", f1, f2, ...)` correctly evaluates a linear model (dot product + intercept).
> 3. `predict("model", ...)` with `method: "logistic"` applies sigmoid correctly.
> 4. Standardization (z-scoring) is applied when declared; features without standardization params pass through raw.
> 5. `calibrate(raw, "map")` correctly interpolates PAVA points.
> 6. `calibrate(raw, "map")` with Platt method applies sigmoid correctly.
> 7. `exp(x)` evaluates correctly (test: `exp(0) == 1.0`, `exp(1) ≈ 2.71828`, `exp(-1) ≈ 0.36788`).
> 8. `norm_cdf(x, mu, sigma)` evaluates correctly (test: `norm_cdf(0, 0, 1) == 0.5`, `norm_cdf(1.96, 0, 1) ≈ 0.975`, `norm_cdf(100, 211.34, 17.25) ≈ small probability`).
> 9. `norm_cdf` with sigma ≤ 0 returns Null.
> 10. All 9 diagnostic codes (MC2050-MC2056, MC3017-MC3018) fire on appropriate fixtures.
> 8. Round-trip: `parse(serialize(parse(model_with_fitted))) == parse(model_with_fitted)`.
> 9. The NBA totals example model validates + lints clean + golden tests pass (predictions match hand-computed expected values from the coefficients).
> 10. Locked surfaces: 0-line diff on mc-fixtures, mc-cli, mc-drivers, mc-recipe, mc-tessera.
>
> **SPEC QUESTION triggers:**
>
> 1. The evaluator needs fitted_models data at eval time but can't access ParsedModel fields from within mc-core's eval function. How to thread the data through? (Likely: pass a reference to a `ModelContext` struct alongside the existing eval args.)
> 2. `predict()` with wrong argument count vs coefficient count — is this a parse-time or compile-time error? (Recommend: compile-time, because the model_id might be dynamic in future. For now model_id is always a string literal, so it COULD be parse-time.)
> 3. Feature ORDER matters (args are positional against the coefficients array). Should there be a named-argument mode for safety? (Recommend: defer. Positional in 3H matches how sklearn exports coefficients. Named-argument mode is Phase 3H.1 if demand surfaces.)
> 4. ~~`exp()` function~~ — **RESOLVED: yes, included in scope.** `exp(x)` and `norm_cdf(x, mu, sigma)` are both in Phase 3H scope (not deferred). See scope items 4b and 4c.
>
> **Completion report format:**
>
> ```
> DONE: Phase 3H — Fitted-Model Evaluation
>
> Build/Format/Lint/Tests: ✓ / ✓ / ✓ / [N]/0
> New YAML blocks: fitted_models:, calibration_maps:
> New AST nodes: Predict, Calibrate, Exp, NormCdf
> New diagnostic codes: MC2050-MC2056, MC3017-MC3018
> New tests: [count]
> Round-trip: ✓
> Locked surfaces: 0-line diff ✓
> Sports betting example: validates + lints + goldens pass ✓
> Plugin skill: mosaic-plugin/skills/fitted-models/SKILL.md ✓
> ```
>
> Do NOT commit or tag. User reviews first.

---

## Context the prompt does NOT spell out

### A. The Phase 3G pattern to follow

Phase 3G already shipped `benchmarks:`, `lookup_tables:`, `status_thresholds:` as top-level YAML blocks with formula functions that read from them. Phase 3H follows the EXACT same pattern:

1. New struct types in `schema.rs` (like `ParsedBenchmark`, `ParsedLookupTable`)
2. New `Option<Vec<...>>` field on `ParsedModel` with `#[serde(default)]`
3. Validation in `validate.rs` (name uniqueness, structural checks)
4. Formula function in the parser (new dispatch arm in `parse_identifier_or_call`)
5. Eval arm that looks up the model/map by name from a context object and evaluates

The infrastructure exists. Phase 3H is filling in a new instance of the pattern.

### B. How to thread fitted_models data to the evaluator

The Phase 3G evaluator already has access to reference data (benchmarks, lookups, thresholds) via whatever context mechanism was established. Use the SAME mechanism for fitted_models + calibration_maps. If the evaluator uses an `EvalContext` that holds references to the model's parsed data, add `fitted_models: &[ParsedFittedModel]` and `calibration_maps: &[ParsedCalibrationMap]` to it.

### C. The math (for hand-verification of golden tests)

**Linear prediction:**
```
Given: intercept=211.34, coefficients=[3.016, 0.548, 0.331, 0.602, -1.203]
       features=[99.2, 113.4, 222.5, 110.1, 0.0]  (no standardization)

result = 211.34 + (3.016*99.2) + (0.548*113.4) + (0.331*222.5) + (0.602*110.1) + (-1.203*0.0)
       = 211.34 + 299.19 + 62.14 + 73.65 + 66.28 + 0
       = 712.60
```

**With standardization:**
```
z_pace = (99.2 - 99.2) / 4.7 = 0.0
z_off  = (113.4 - 113.4) / 8.2 = 0.0
...
(all features at their mean → all z-scores = 0 → result = intercept = 211.34)
```

**Logistic (sigmoid):**
```
linear_result = (as above)
probability = 1 / (1 + exp(-linear_result))
```

**PAVA interpolation:**
```
Given: points = [(0.50, 0.42), (0.60, 0.50), (0.70, 0.61)]
       raw = 0.55

Between (0.50, 0.42) and (0.60, 0.50):
fraction = (0.55 - 0.50) / (0.60 - 0.50) = 0.5
calibrated = 0.42 + 0.5 * (0.50 - 0.42) = 0.42 + 0.04 = 0.46
```

### D. Why "one evaluator, many model types" matters

The sklearn export workflow:

```python
# User trains in Python:
from sklearn.linear_model import Lasso
model = Lasso(alpha=0.7).fit(X_train, y_train)

# User exports to Mosaic YAML:
print(yaml.dump({
    "fitted_models": [{
        "name": "my_model_v1",
        "method": "linear",
        "intercept": float(model.intercept_),
        "coefficients": [
            {"feature": name, "weight": float(coef)}
            for name, coef in zip(feature_names, model.coef_)
            if abs(coef) > 1e-10  # skip zero coefficients (Lasso sparsity)
        ],
    }]
}))
```

The same export pattern works for Ridge (identical), ElasticNet (identical), OLS (identical), and logistic regression (change method to "logistic"). **One Mosaic primitive, five sklearn model types.**

---

## Pointers to existing files

| File | Role | Phase 3H action |
|---|---|---|
| `crates/mc-model/src/schema.rs` | AST + model types | Add `ParsedFittedModel`, `ParsedCalibrationMap`, field on `ParsedModel` |
| `crates/mc-model/src/formula.rs` | Parser | Add `predict(...)` and `calibrate(...)` dispatch |
| `crates/mc-model/src/validate.rs` | Validation | Add checks for fitted_models + calibration_maps |
| `crates/mc-model/src/inspect.rs` | Inspect rendering | Show fitted_models + calibration_maps in summary |
| `crates/mc-model/src/lint.rs` | Lint rules | MC3017 + MC3018 staleness |
| `crates/mc-model/src/compile.rs` | Compile stage | Thread fitted data into eval context |
| `crates/mc-core/src/rule.rs` | Evaluator | Add Predict + Calibrate eval arms |
| `mosaic-plugin/skills/fitted-models/SKILL.md` | Plugin skill | NEW — teaches LLMs how to use fitted models |
| `examples/sports-betting/nba-totals.yaml` | Example | NEW — proves the feature works on a real domain |

---

## Reproducible commands

```bash
cd /Users/edwinlovettiii/Projects/mc-v2

# Pre-3H gate (must remain green throughout)
cargo build --release --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                                # 578+ / 0

# Iteration loop
cargo test -p mc-model
cargo test -p mc-model -- predict
cargo test -p mc-model -- calibrate
cargo test -p mc-model -- fitted

# Verify locked surfaces
git diff HEAD -- crates/mc-fixtures/ crates/mc-cli/ crates/mc-drivers/ crates/mc-recipe/ crates/mc-tessera/
# expected: 0 lines

# Verify example model works
mc model validate examples/sports-betting/nba-totals.yaml
mc model lint examples/sports-betting/nba-totals.yaml
mc model test examples/sports-betting/nba-totals.yaml
```

---

## Final checklist

- [ ] `fitted_models:` YAML block parses correctly (serde(default); existing models unchanged)
- [ ] `calibration_maps:` YAML block parses correctly (same)
- [ ] `ParsedFittedModel` + all sub-types defined in schema.rs
- [ ] `ParsedCalibrationMap` + all sub-types defined in schema.rs
- [ ] `predict(model_id, features...)` parses; model_id is a string literal
- [ ] `calibrate(value, map_id)` parses; map_id is a string literal
- [ ] Linear prediction evaluates correctly (dot product + intercept)
- [ ] Logistic prediction evaluates correctly (linear + sigmoid)
- [ ] Standardization applied correctly when declared
- [ ] PAVA interpolation evaluates correctly (linear interp between adjacent points; clamp at edges)
- [ ] Platt scaling evaluates correctly (sigmoid with a, b params)
- [ ] Null-poisoning: predict with any Null feature returns Null
- [ ] Null-poisoning: calibrate(Null) returns Null
- [ ] MC2050-MC2056 all fire on appropriate invalid fixtures
- [ ] MC3017 + MC3018 lint for stale fitted artifacts
- [ ] Round-trip: parse(serialize(parse(model))) == parse(model) for all new constructs
- [ ] NBA totals example: validates + lints clean + goldens pass
- [ ] Plugin skill written: mosaic-plugin/skills/fitted-models/SKILL.md
- [ ] All 578+ existing tests pass unchanged
- [ ] Locked surfaces: 0-line diff on mc-fixtures, mc-cli, mc-drivers, mc-recipe, mc-tessera
- [ ] No new dependencies
- [ ] No unsafe
- [ ] `exp(x)` standalone function added and tested (exp(0)==1, exp(1)≈2.718, Null→Null)
- [ ] `norm_cdf(x, mu, sigma)` added and tested (norm_cdf(0,0,1)==0.5, norm_cdf(1.96,0,1)≈0.975, sigma≤0→Null, Null→Null)
- [ ] Abramowitz & Stegun 26.2.17 polynomial used for norm_cdf (~15 lines, ~7.5e-8 accuracy, zero deps)
- [ ] `mc model inspect` shows fitted_models + calibration_maps sections
- [ ] Do NOT commit. User reviews first.

---

## Operating principles

**Follow the Phase 3G pattern.** The infrastructure for reference-data blocks exists. You're adding two more instances of the same pattern. If 3G's `benchmarks:` works, `fitted_models:` should work the same way.

**The golden tests ARE the correctness proof.** Hand-compute expected predictions from the coefficients (see §C above). Pin them as golden assertions with 1e-9 epsilon. If the golden passes, the evaluator is correct.

**Positional features, not named.** `predict("model", f1, f2, f3)` passes features in the ORDER declared in `coefficients:[]`. This matches how sklearn exports (`model.coef_` is positional). Named features are a Phase 3H.1 convenience.

**The sports betting example proves the domain.** An NBA game-totals model with real-ish coefficients, standardization, and calibration demonstrates that Mosaic can serve as a prediction evaluation engine. This is the "aha moment" for the sports/ML audience.

---

*Phase 3H handoff drafted 2026-05-05. Waits for Phase 5C to complete (so the 578+ test baseline is confirmed stable on main before 3H implementation starts). Implementation touches mc-model (primary) + mc-core (eval arms only). Estimated effort: ~1-2 weeks.*
