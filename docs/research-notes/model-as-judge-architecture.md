# Model-as-Judge Architecture — Mosaic as Both Calculator AND Judge

> **Status:** Research note + Phase 3H.1 scope proposal.
> **Filed:** 2026-05-05, after analyzing ~10,000 lines of production sports-betting code (claw-core) and identifying the gap between "compute predictions" and "prove predictions are correct."
> **Motivation:** the project owner's requirement: "Mosaic should be capable of proving or disproving the accuracy of a model — modeling the model itself."

---

## The insight

Most planning/forecasting tools are **calculators** — you give them weights, they give you numbers. The user trusts the numbers or doesn't; the tool has no opinion.

Mosaic should be a **calculator AND a judge** — it computes predictions AND grades them against reality AND diagnoses where they fail AND lets you change weights to see if accuracy improves. All in one model file, all recalculating in <50ms.

This is what makes Mosaic a **Large Numbers Model** rather than just a spreadsheet. A spreadsheet can multiply coefficients by features. Only a judge can tell you "your 65% confidence predictions only win 52% of the time, and the misses cluster in high-pace playoff games on back-to-backs, and if you reduce the pace coefficient from 3.0 to 2.0 the MAE drops by 0.4 points."

---

## The three-layer architecture

Every Mosaic model that evaluates a fitted model (Phase 3H's `predict()` / `calibrate()`) should have three layers of derived measures:

```
┌─────────────────────────────────────────────────────────────────┐
│ LAYER 3: DIAGNOSTIC RULES (the investigator)                     │
│                                                                   │
│ WHERE does the model fail? Under what conditions?                │
│ High-pace games? Playoffs? Back-to-backs? Certain markets?       │
│ Features correlated with large errors?                           │
│                                                                   │
│ Example rules:                                                    │
│   Overconfidence_Flag = if(Confidence > 0.6 and Wrong, 1, 0)    │
│   High_Pace_Miss = if(Pace > 102 and Abs_Error > 15, 1, 0)     │
│   Regime_Error = if(Is_Playoff and Abs_Error > 15, 1, 0)        │
│   Error_Cluster = bucket(Abs_Error, "error_severity")            │
├─────────────────────────────────────────────────────────────────┤
│ LAYER 2: GRADING RULES (the judge)                               │
│                                                                   │
│ HOW GOOD is the model overall? Calibrated? Profitable?           │
│ Does stated confidence match empirical accuracy?                 │
│                                                                   │
│ Example rules:                                                    │
│   Prediction_Error = Predicted - Actual                          │
│   Abs_Error = abs(Predicted - Actual)                            │
│   Direction_Correct = if(same side as actual, 1, 0)              │
│   Brier_Component = (Stated_Prob - Outcome)^2                    │
│   Confidence_Bucket = bucket(Calibrated_P, "confidence_bins")    │
│   Was_Profitable = if(edge > 0 and correct, profit, -stake)     │
├─────────────────────────────────────────────────────────────────┤
│ LAYER 1: PREDICTION RULES (the calculator)                       │
│                                                                   │
│ WHAT does the model predict?                                     │
│ Raw prediction, calibrated probability, expected value, sizing   │
│                                                                   │
│ Example rules:                                                    │
│   Predicted_Total = predict("model_v1", features...)             │
│   P_Over = 1 - norm_cdf(Line, Predicted_Total, Residual_Std)    │
│   Calibrated_P = calibrate(P_Over, "calibration_v1")            │
│   EV = Calibrated_P * (Decimal_Odds - 1) - (1 - Calibrated_P)  │
│   Kelly = if(EV > 0, safe_div(edge, odds - 1, 0) * 0.25, 0)    │
└─────────────────────────────────────────────────────────────────┘

    ▲ All three layers recalculate when ANY input changes:
    │ - Change a weight in fitted_models: → prediction changes → grading changes → diagnostics change
    │ - Load new actuals data → grading changes → diagnostics change
    │ - Change the line → probability changes → EV changes → sizing changes
    │ All in <50ms. No external scripts. No re-training. Just re-evaluation.
```

---

## Architectural invariants (from GPT + Desktop review)

### Cross-layer dependency graph

The three layers are conceptually distinct but architecturally they're all derived measures in the same cube. Layer 2 references Layer 1 outputs; Layer 3 references Layer 2 outputs. This means:

- When a fitted-model weight changes, the dirty-propagation cascade MUST flow through all three layers deterministically and completely
- Layer 1 must finish before Layer 2 evaluates; Layer 2 before Layer 3
- The kernel's existing topological-sort guarantees handle this (derived measures evaluate in dependency order)
- If Phase 7+ ever introduces parallel evaluation (rayon), layer ordering must be preserved

**The "all three layers recalculate in <50ms" claim is an architectural commitment**, not just a feature. It relies on the dirty-propagation work from Phases 2D + 3E/3F (cross-coordinate reads). Worth testing explicitly in the integration suite.

### Null-actuals handling (Layer 2/3 graceful degradation)

The grading layer assumes actuals exist. But for forward-looking predictions (next month's revenue, next week's NBA total), actuals don't exist yet. Layer 2/3 MUST be null-safe:

```yaml
# Layer 2: only grade if actuals are present
- name: abs_error
  body: "if(Actual_Total == Null, Null, abs(Predicted_Total - Actual_Total))"
  # OR equivalently (Null arithmetic propagates):
  body: "abs(Predicted_Total - Actual_Total)"
  # (returns Null when Actual_Total is Null — correct by default)

# Layer 2 aggregate: count of games that CAN be graded
- name: is_graded
  body: "if(Actual_Total == Null, 0, 1)"
```

Diagnostic rules (Layer 3) similarly only fire on graded coordinates. Cumulative metrics (`cumulative(Profit_Units)`) automatically skip Nulls (cumulative treats Null as 0). This is mechanical but important — the three-layer pattern works for both historical grading AND forward-looking prediction without modification.

### Calibration aggregation depends on `sum_over`

Per-row grading works today (Layer 2: abs_error, brier_component per game). But **full calibration-bucket reporting** — "of all games in the 60-65% bucket, the empirical win rate is X" — requires aggregation across a set of predictions:

1. Bucket predictions by stated confidence
2. For each bucket, count wins / count total → empirical win rate
3. Compare empirical vs stated

This requires `sum_over` (Phase 3G, shipped) or a similar cross-coordinate aggregation. The note should be explicit: **basic per-row grading works now; full calibration-by-bucket reporting depends on the Phase 3G aggregation primitives already shipped.** If `sum_over` performance limits (MC3011 at >50 elements) become a concern for large game datasets, the calibration step may need a dedicated aggregation path.

---

## Why this is domain-agnostic

The three layers map to ANY domain that has predictions + outcomes:

| Domain | Layer 1 (predict) | Layer 2 (grade) | Layer 3 (diagnose) |
|---|---|---|---|
| **Sports betting** | predict total; P(Over); EV; Kelly | abs_error; direction_correct; brier; P&L | high_pace_miss; playoff_error; overconfidence |
| **Stock forecasting** | predict return; P(return > 5%); position size | abs_error; direction_correct; sharpe; P&L | sector_miss; volatility_regime_error; earnings_surprise |
| **Marketing ROI** | predict ROAS; P(ROI > 1.5×); budget allocation | forecast_vs_actual; MAPE; budget_variance | seasonal_miss; channel_underperformance; new_market_error |
| **Sales pipeline** | predict close_rate; P(deal > $50K); weighted pipeline | won_vs_predicted; forecast_accuracy; quota_attainment | stage_specific_miss; rep_calibration; deal_size_error |
| **Demand planning** | predict demand; P(stockout); safety_stock | forecast_vs_actual; bias; fill_rate | seasonal_miss; promotion_effect_error; new_product_error |

**The pattern is always the same:**
1. A fitted model makes a prediction (Layer 1)
2. Reality is observed and compared to the prediction (Layer 2)
3. The comparison is sliced by conditions to find systematic failures (Layer 3)

The formulas differ; the architecture doesn't.

---

## What Mosaic can do TODAY for judging

### Accuracy measurement

```yaml
# Mean Absolute Error (per game):
- name: abs_error
  body: "abs(Predicted_Total - Actual_Total)"

# Directional accuracy (did we pick the right side?):
- name: direction_correct
  body: |
    if((Predicted_Total > Market_Line and Actual_Total > Market_Line)
       or (Predicted_Total < Market_Line and Actual_Total < Market_Line),
       1, 0)

# Brier score component (per game, lower = better calibrated):
- name: brier_component
  body: "(Calibrated_P - Direction_Correct) * (Calibrated_P - Direction_Correct)"
```

### Calibration verification

```yaml
# Which confidence bucket does this prediction fall in?
status_thresholds:
  - name: "confidence_bins"
    bands:
      - { label: "50-55%", max: 0.55 }
      - { label: "55-60%", max: 0.60 }
      - { label: "60-65%", max: 0.65 }
      - { label: "65-70%", max: 0.70 }
      - { label: "70%+"}

- name: confidence_bucket
  body: "bucket(Calibrated_P, 'confidence_bins')"

# Then via golden tests:
# "Of all games in the 60-65% bucket, the empirical win rate should be 0.60-0.65"
# If it's 0.52 instead, the model is overconfident in that range
```

### Profitability tracking

```yaml
# Per-game profit/loss (in units):
- name: profit_units
  body: |
    if(EV_Per_Dollar > 0,
       if(Direction_Correct == 1, Decimal_Odds - 1, -1),
       0)

# Cumulative P&L over the season:
- name: cumulative_pnl
  body: "cumulative(Profit_Units)"
```

### Failure-mode detection

```yaml
# Flag games where model was confidently wrong:
- name: overconfidence_flag
  body: "if(Calibrated_P > 0.60 and Direction_Correct == 0, 1, 0)"

# Flag high-pace games with large errors:
- name: high_pace_miss
  body: "if(Avg_Pace > 102 and Abs_Error > 15, 1, 0)"

# Flag playoff-specific errors:
- name: playoff_error_flag
  body: "if(Is_Playoff == 1 and Abs_Error > Avg_Abs_Error * 1.5, 1, 0)"

# Severity classification:
status_thresholds:
  - name: "error_severity"
    bands:
      - { label: "Good", max: 5.0 }
      - { label: "Acceptable", max: 10.0 }
      - { label: "Concerning", max: 15.0 }
      - { label: "Investigate" }

- name: error_grade
  body: "bucket(Abs_Error, 'error_severity')"
```

---

## What Mosaic CANNOT do yet (Phase 3H.1 scope)

### The "weight sensitivity" gap

Today, to test "what if avg_pace coefficient was 2.0 instead of 3.016?", the user must:

1. Edit the YAML file (change the number)
2. Re-run `mc model test`
3. Read the results
4. Manually compare to the previous run

This is functional but manual. **Phase 3H.1 should automate step 1-4** with a `mc model sweep` verb (or equivalent):

```bash
mc model sweep nba-totals.yaml \
  --parameter "fitted_models.nba_v16_lasso.coefficients[0].weight" \
  --range 0.0:5.0:0.5 \
  --metric "avg(Abs_Error)" \
  --report sweep-results.csv
```

This would:
1. For each value in the range (0.0, 0.5, 1.0, ..., 5.0)
2. Temporarily override the coefficient
3. Re-evaluate the full model (predictions + grading)
4. Record the metric (average absolute error)
5. Report which value minimizes the metric

**This is NOT training** (no gradient descent, no regularization, no cross-validation). It's **exhaustive evaluation** — try N values, measure the result for each, report the best. The cube does the computation; the sweep just orchestrates multiple runs.

**Implementation scope (estimated ~1-2 days):**
- A `mc model sweep` CLI verb in mc-cli
- Takes: parameter path (JSONPath-like into the YAML), range (start:end:step), metric expression (a derived measure to aggregate)
- Runs: N model evaluations (one per parameter value), collecting the metric after each
- Reports: a table of (parameter_value, metric_value) + the minimum
- No new formula primitives needed — uses the existing predict/eval pipeline
- No kernel changes — the sweep is orchestration in mc-cli

**Why this matters:** it turns Mosaic from "calculator that the user manually pokes" into "a tool that systematically explores the parameter space and tells you which configuration performs best on your data." The difference between "I think pace matters this much" and "I KNOW pace matters this much because I tested 11 values and this one minimizes error."

### The "walk-forward grading" pattern

Walk-forward training (fit the model on prior data, test on future data) stays in Python. But walk-forward **grading** (load 7 pre-fitted models, evaluate each on its test season, compare accuracy across seasons) is pure Mosaic:

```yaml
# 7 fitted models, one per walk-forward fold:
fitted_models:
  - name: "v16_fold_2019"    # trained on 2017-2018, tests on 2019
    method: "linear"
    intercept: 225.1
    coefficients: [...]
  - name: "v16_fold_2020"    # trained on 2017-2019, tests on 2020
    method: "linear"
    intercept: 224.8
    coefficients: [...]
  # ... 7 total

# Rule selects the right model per season:
- name: predicted_total
  body: |
    if(Season == 2019, predict("v16_fold_2019", features...),
    if(Season == 2020, predict("v16_fold_2020", features...),
    ...))
```

This lets you see: "the model trained ONLY on 2017-2018 predicts 2019 with MAE 13.5; the model trained on 2017-2020 predicts 2021 with MAE 14.2." If MAE increases over time, the model is degrading (regime change). If MAE is stable, the signal is durable.

**No new primitives needed** — just multiple `fitted_models:` entries and conditional dispatch via `if()`. The diagnostic layer (Layer 3) then slices by season to identify where each fold performs best/worst.

### The "calibration drift detection" pattern

With `time_anchor` + `cumulative`, Mosaic can track calibration over time:

```yaml
# Running calibration: does the stated win rate match the running empirical win rate?
- name: running_win_rate
  body: "safe_div(cumulative(Direction_Correct), period_index() + 1, 0)"

- name: calibration_drift
  body: "abs(running_win_rate - avg_stated_confidence)"

# Flag: if calibration drift exceeds 5%, the model may need re-fitting
- name: needs_refit_flag
  body: "if(calibration_drift > 0.05, 1, 0)"
```

When `needs_refit_flag` fires persistently, it's time to re-train in Python and export new weights.

---

## Phase 3H.1: Weight Sweep (the specific proposal)

### What it is

A CLI verb that systematically varies one parameter in a model, re-evaluates, and reports which value optimizes a chosen metric.

### CLI interface

```bash
# Sweep one coefficient:
mc model sweep model.yaml \
  --parameter "fitted_models[0].coefficients[2].weight" \
  --range "0.0:5.0:0.5" \
  --metric "mean(Abs_Error)" \
  --output sweep-results.json

# Sweep the intercept:
mc model sweep model.yaml \
  --parameter "fitted_models[0].intercept" \
  --range "200:240:2" \
  --metric "mean(Abs_Error)"

# Sweep a lookup table value (e.g., playoff offset):
mc model sweep model.yaml \
  --parameter "lookup_tables[0].values.Playoff" \
  --range "-30:0:3" \
  --metric "mean(Playoff_Abs_Error)"

# Sweep with a time-anchor (only grade future periods):
mc model sweep model.yaml \
  --parameter "fitted_models[0].coefficients[0].weight" \
  --range "0:5:0.5" \
  --metric "mean(Abs_Error)" \
  --time-anchor "2024_01"
```

### Output

```json
{
  "parameter": "fitted_models[0].coefficients[2].weight",
  "feature_name": "avg_recent_total_10",
  "original_value": 0.331,
  "sweep": [
    { "value": 0.0, "metric": 14.21 },
    { "value": 0.5, "metric": 13.98 },
    { "value": 1.0, "metric": 13.85 },
    { "value": 1.5, "metric": 13.92 },
    { "value": 2.0, "metric": 14.15 }
  ],
  "optimal": { "value": 1.0, "metric": 13.85 },
  "improvement_vs_original": -0.16,
  "recommendation": "Reducing avg_recent_total_10 weight from 0.331 to 1.0 reduces MAE by 0.16 on this dataset"
}
```

### What it is NOT

- **NOT gradient descent.** It's exhaustive grid search. No learning rate, no convergence, no regularization.
- **NOT cross-validation.** It evaluates on whatever data is loaded. The user chooses the evaluation set (train, test, or both) by what they put in the CSV.
- **NOT multi-parameter.** One parameter at a time. Multi-parameter sweeps (grid search across 2+ params) are Phase 3H.2 if demand surfaces.
- **NOT a replacement for sklearn.** For serious model fitting (50+ features, regularization selection, hyperparameter tuning), use Python. The sweep is for "I have a fitted model and want to understand how sensitive the output is to each coefficient."

### Why single-parameter sweeps are enough

For a Lasso model with 54 coefficients, you DON'T need to sweep all 54 simultaneously. You sweep each independently:

1. Hold all other coefficients at their fitted values
2. Sweep coefficient #1 across a range
3. Record the metric curve
4. The curve tells you: is this coefficient at its optimum? Is the metric sensitive to it? Is there a better value?

If the metric is flat across the range, the coefficient doesn't matter much. If it has a sharp minimum, the coefficient is important and you can see exactly where it should be. If the minimum is far from the fitted value, the model may have overfit to training data.

This is **sensitivity analysis**, not optimization. It tells you where to look, not what to do. The actual re-fitting (with proper regularization and cross-validation) stays in Python.

### CLI interface (revised per GPT + Desktop review)

Parameters are addressed by **name**, not array index — indices are brittle and break silently when YAML order changes:

```bash
# Sweep one coefficient by name:
mc model sweep model.yaml \
  --model nba_v16_lasso \
  --coefficient avg_recent_total_10 \
  --range "0:5:0.5" \
  --metric "mean(Abs_Error)" \
  --goal minimize

# Sweep the intercept:
mc model sweep model.yaml \
  --model nba_v16_lasso \
  --intercept \
  --range "200:240:2" \
  --metric "mean(Abs_Error)" \
  --goal minimize

# Sweep a lookup table value:
mc model sweep model.yaml \
  --lookup seasonal_factor \
  --key "Playoff" \
  --range "-30:0:3" \
  --metric "mean(Playoff_Abs_Error)" \
  --goal minimize

# With time-anchor (only grade future periods):
mc model sweep model.yaml \
  --model nba_v16_lasso \
  --coefficient avg_pace \
  --range "0:5:0.5" \
  --metric "mean(Abs_Error)" \
  --goal minimize \
  --time-anchor "2024_01"
```

**Flags:**
- `--goal minimize|maximize` — required; don't guess metric direction
- `--no-baseline` — skip comparison to original value (default: compare)
- `--format json|csv|text` — JSON is canonical artifact; CSV is flattened; text for terminal
- `--output <path>` — write report to file (default: stdout)

### Implementation approach (revised: in-memory override, NO YAML patching)

```rust
// In mc-cli/src/sweep.rs (new module)
pub fn run_sweep(args: SweepArgs) -> SweepResult {
    // 1. Parse + validate the model ONCE
    let base_yaml = std::fs::read_to_string(&args.model_path)?;
    let parsed = mc_model::parse(&base_yaml)?;
    let validated = mc_model::validate(parsed)?;
    
    // 2. Resolve the named parameter selector
    let selector = resolve_selector(&validated, &args)?;
    // e.g., ModelSelector::Coefficient { model: "nba_v16_lasso", feature: "avg_pace" }
    
    // 3. Evaluate the ORIGINAL value as baseline (default behavior)
    let baseline_metric = evaluate_with_override(&validated, &selector, selector.original_value(), &args)?;
    
    let mut results = Vec::new();
    for value in args.range.iter() {
        // 4. Clone the validated model, override the parameter IN MEMORY
        //    (no YAML write, no disk I/O, no comment loss)
        let metric = evaluate_with_override(&validated, &selector, value, &args)?;
        results.push(SweepPoint { value, metric });
    }
    
    // 5. Find optimal per --goal direction
    let optimal = match args.goal {
        Goal::Minimize => results.iter().min_by(|a, b| a.metric.partial_cmp(&b.metric).unwrap()),
        Goal::Maximize => results.iter().max_by(|a, b| a.metric.partial_cmp(&b.metric).unwrap()),
    };
    
    SweepResult {
        parameter: selector.display_name(),
        original_value: selector.original_value(),
        baseline_metric,
        sweep: results,
        optimal,
        improvement_vs_original: optimal.metric - baseline_metric,
    }
}

fn evaluate_with_override(
    validated: &ValidatedModel,
    selector: &ModelSelector,
    override_value: f64,
    args: &SweepArgs,
) -> Result<f64, Error> {
    // Clone validated, apply override to the parsed fitted_models/lookup_tables/etc
    let mut model = validated.clone();
    selector.apply_override(&mut model, override_value);
    
    // Compile + load inputs + set time_anchor + evaluate
    let compiled = mc_model::compile(model)?;
    let mut cube = compiled.cube;
    // ... apply canonical_inputs, set time_anchor from args ...
    
    // Read the metric measure across all graded coordinates (non-Null actuals)
    let metric_values = read_graded_values(&cube, &args.metric_measure);
    aggregate(metric_values, args.aggregation)
}
```

**Critical: NO YAML patching.** The sweep operates entirely in-memory via struct-level override on the `ValidatedModel`. The source YAML file is never modified. This:
- Preserves comments and formatting
- Avoids `serde_yaml` round-trip lossyness
- Is faster (no disk I/O per sweep point)
- Reuses the existing parse/compile pipeline
- Makes `sweep` conceptually a "loop of whatif evaluations" (per Desktop's suggestion)

**Effort estimate:** ~2-3 days.
- Day 1: Named selector resolution (model name → coefficient name → field path in the parsed struct)
- Day 2: sweep loop with in-memory override + metric aggregation + output formatting
- Day 3: CLI wiring + JSON/CSV output + tests + edge cases

**Dependencies:** mc-model (parse/validate/compile), mc-core (cube read). No new kernel primitives. No new formula functions. Pure orchestration.

---

## The full "model judge" workflow

With Layers 1-3 + Phase 3H.1 sweep, the complete workflow is:

```
Step 1: BUILD the model
  Python trains → exports weights to fitted_models: YAML
  OR: user authors rules directly (non-ML use case)

Step 2: LOAD actuals
  mc tessera apply recipe.yaml
  OR: canonical_inputs in the model YAML

Step 3: EVALUATE (Layer 1)
  mc model test model.yaml --time-anchor 2024_01
  → predictions computed; Layer 1 rules fire

Step 4: GRADE (Layer 2)
  Same mc model test run; Layer 2 rules fire automatically
  → abs_error, direction_correct, brier, profit computed per game
  → goldens pin expected accuracy: "MAE should be ≤ 14.0"

Step 5: DIAGNOSE (Layer 3)
  Same run; Layer 3 rules fire
  → overconfidence flags, pace-miss flags, playoff-error flags
  → "12% of high-pace games have errors > 15 points"

Step 6: INVESTIGATE (Phase 3H.1 sweep)
  mc model sweep model.yaml \
    --parameter "fitted_models[0].coefficients[0].weight" \
    --range "0:5:0.5" \
    --metric "mean(Abs_Error)"
  → "reducing avg_pace weight from 3.016 to 2.0 drops MAE by 0.3"

Step 7: DECIDE
  User reads the sweep results, decides whether to update the weight
  OR: sends findings back to Python for proper re-training with regularization

Step 8: ITERATE
  Update weights in YAML → re-run → all three layers recalculate
  → did accuracy improve? Did the failure modes shift?
  → repeat until the model is "good enough" or needs full retraining
```

**Total time for steps 3-6 on a 3,685-game dataset:** ~2-5 seconds (3685 × ~50ms per full evaluation, but most of that is I/O; the sweep amortizes parse/compile across runs).

---

## Domain examples (proving it's not just sports)

### Marketing: "Is our ROAS model accurate?"

```yaml
# Layer 1: predict ROAS from features
- body: "predict('roas_model', Spend, Prev_Month_Revenue, Seasonal_Factor)"

# Layer 2: grade against actual ROAS
- body: "abs(Predicted_ROAS - Actual_ROAS)"

# Layer 3: diagnose
- body: "if(Is_December and Abs_ROAS_Error > 0.5, 1, 0)"  # holiday miss?
- body: "if(Is_New_Market and Abs_ROAS_Error > 0.5, 1, 0)" # new market miss?

# Sweep: is the spend coefficient right?
mc model sweep marketing.yaml --parameter "fitted_models[0].coefficients[0].weight" --range "0:0.01:0.001" --metric "mean(ROAS_Abs_Error)"
```

### Sales: "Is our close-rate forecast accurate?"

```yaml
# Layer 1: predict close probability
- body: "predict('close_model', Deal_Size, Days_In_Stage, Rep_Win_Rate)"

# Layer 2: grade against actual outcomes
- body: "if(Predicted_Close > 0.5 and Actually_Closed == 1, 1, 0)"  # true positive
- body: "(Predicted_Close - Actually_Closed) * (Predicted_Close - Actually_Closed)"  # brier

# Layer 3: diagnose
- body: "if(Deal_Size > 100000 and Prediction_Wrong, 1, 0)"  # big deal misses
- body: "bucket(Days_In_Stage, 'stage_duration_bins')"  # error by deal velocity

# Sweep: does rep_win_rate matter as much as the model thinks?
mc model sweep sales.yaml --parameter "fitted_models[0].coefficients[2].weight" --range "0:2:0.2" --metric "mean(Brier_Component)"
```

### Demand: "Is our demand forecast accurate?"

```yaml
# Layer 1: predict demand
- body: "predict('demand_model', Prev_Quarter_Demand, Price, Promotion_Active)"

# Layer 2: grade
- body: "abs(Predicted_Demand - Actual_Demand)"
- body: "if(Predicted_Demand < Actual_Demand, 1, 0)"  # underforecast (stockout risk)

# Layer 3: diagnose
- body: "if(Promotion_Active and Abs_Demand_Error > 100, 1, 0)"  # promotion effect wrong?
- body: "if(Is_New_Product and Abs_Demand_Error > 50, 1, 0)"  # new product miss?

# Sweep: is the promotion coefficient right?
mc model sweep demand.yaml --parameter "fitted_models[0].coefficients[2].weight" --range "-1:1:0.1" --metric "mean(Demand_Abs_Error)"
```

---

## What this means for the claw-core cartridge

The NBA totals cartridge should be built with all three layers from the start:

```
examples/sports-betting/
├── nba-totals.yaml              # Model with all 3 layers
│   ├── fitted_models:           # V1.6 Lasso (54 coefficients)
│   ├── calibration_maps:        # 8-point PAVA
│   ├── lookup_tables:           # book tiers, playoff offset, feature defaults
│   ├── status_thresholds:       # confidence bins, error severity
│   ├── # Layer 1 rules: predict → calibrate → P(Over) → EV → Kelly
│   ├── # Layer 2 rules: abs_error, direction_correct, brier, P&L
│   └── # Layer 3 rules: overconfidence, pace_miss, playoff_error, regime_drift
├── nba-totals.inputs.csv        # Historical games + actuals
├── nba-totals.weights.json      # Exported from claw-core (for audit trail)
└── README.md                    # How to evaluate, grade, sweep, and improve
```

The cartridge README should teach:
1. How to load games and evaluate predictions
2. How to read the grading output (is the model good?)
3. How to read the diagnostic output (where does it fail?)
4. How to use `mc model sweep` to test weight sensitivity
5. How to export findings back to Python for re-training
6. **The honest disclaimer:** "Mosaic evaluates and judges models; it does not guarantee profitability. The model may have edge or it may not. Mosaic tells you which — honestly, with measured evidence — so you can decide."

---

## Sweep-vs-goldens workflow (per GPT + Desktop review)

**Sweep does NOT mutate the model YAML and does NOT update goldens.**

If a user runs `mc model sweep` and finds that reducing `avg_pace` from 3.016 to 2.0 improves MAE by 0.4, the workflow is:

```
1. RUN SWEEP → "value 2.0 minimizes MAE on this evaluation set"
2. DECIDE → user reviews the sweep curve; decides whether to accept
3. UPDATE (manual) → user edits fitted_models YAML: avg_pace weight = 2.0
4. RE-RUN TESTS → mc model test; existing goldens FAIL (predictions changed)
5. REGENERATE GOLDENS → user verifies new predictions, updates golden expected values
6. VERIFY → new goldens pass; model is accepted at new weights
```

**Why not auto-update:** sweep results are evaluation-set-specific and CAN overfit. A weight that minimizes MAE on 2024 data may not generalize to 2025. The decision to accept a new weight requires human judgment (or a proper walk-forward validation in Python). Sweep provides the evidence; the human makes the call.

**Holdout warning:** the sweep report should include a note:
> "These results are specific to the data currently loaded. For production model updates, evaluate sweep candidates against a holdout period (use `--time-anchor` to restrict grading to out-of-sample data)."

---

## Abstention as a diagnostic pattern (not Phase 3H.1 scope)

**The strongest version of the "judge" framing (per Desktop review):**

> "Most tools tell you what to predict. Mosaic tells you whether you should be predicting at all."

The diagnostic layer's job isn't just to find where the model fails — it's to identify regimes where the model has **no signal** and shouldn't be used. Knowing when to abstain is more valuable than knowing when to act.

**Abstention is expressed through model rules, not through the sweep:**

```yaml
# Abstention rule: only bet when edge AND confidence are both sufficient
- name: should_bet
  body: "if(EV_Per_Dollar > 0.03 and Calibrated_P > 0.56, 1, 0)"

# Abstention diagnostic: why NOT betting?
- name: abstain_reason
  body: |
    if(EV_Per_Dollar <= 0, 0,
    if(Calibrated_P <= 0.56, 1,
    2))
  # 0 = negative EV, 1 = insufficient confidence, 2 = should bet

# Selectivity metric: what fraction of opportunities do we actually bet?
- name: bet_rate
  body: "safe_div(sum_over('Game', Should_Bet), sum_over('Game', Is_Graded), 0)"
```

**The "selectivity > volume" lesson from claw-core generalizes.** The best model might be one that says "no bet" 90% of the time. The diagnostic layer surfaces this as data: "model recommends action on 10% of opportunities; those 10% have 62% win rate; the other 90% are noise." That's the output that sophisticated users actually want.

**Phase 3H.1 scope boundary:** abstention thresholds CAN be swept later ("what if the edge threshold was 0.05 instead of 0.03?"), but abstention optimization is NOT a Phase 3H.1 requirement. The sweep is general; abstention is one application of it.

---

## Implementation phasing

| Phase | What ships | Effort |
|---|---|---|
| **Now** (cartridge) | NBA totals model with all 3 layers, using actual claw-core weights + calibration | ~1 day |
| **Phase 3H.1** | `mc model sweep` CLI verb (single-parameter sensitivity analysis; named selectors; in-memory override; JSON canonical output) | ~2-3 days |
| **Phase 3H.2** (future) | Multi-parameter sweep (2D grid search), Pareto-front reporting | ~1 week |
| **Phase 3I** (future) | `pow`, `ln`, `sqrt` (enables more complex grading metrics like Sharpe ratio) | Already planned |
| **Phase 3J** (future) | Distributional cells (enables prediction intervals: "221.5 ± 17.3" as a single cell value) | Already planned |

---

## Cross-links

- [Phase 3H handoff](../handoffs/phase-3h-handoff.md) — the fitted-model evaluation primitives this architecture builds on
- [Formula language expansion research](./formula-language-expansion.md) — the complete formula primitive inventory
- [ADR-0014 Time Representation](../decisions/0014-time-representation.md) — the time_anchor infrastructure for walk-forward grading
- [PERF.md §6.18](../PERF.md) — the 25ms recompute benchmark that makes sweep feasible (3685 games × ~1ms each ≈ 4 seconds per sweep point)
- [claw-core training/](../../) — the production sports-betting code this analysis is based on
- Process note: the three-layer pattern should become a plugin skill (`skills/model-evaluation/SKILL.md`) so LLMs authoring models include grading + diagnostic rules automatically

---

## The one-liner (revised per Desktop review)

> **Most tools tell you what to predict. Mosaic tells you whether you should be predicting at all — and when you do predict, it tells you if you should believe it, where it fails, what to change, and recalculates in 25ms so you can try the change and see if it worked.**

That's the "calculator AND judge" capability. The architecture is domain-agnostic; the three-layer pattern applies to any domain where predictions meet reality. Phase 3H.1's sweep verb is the automation that turns manual weight-poking into systematic sensitivity analysis. Abstention as a first-class diagnostic output is what makes Mosaic genuinely useful for sophisticated users who've seen too many tools that push toward action when the honest answer is "no signal here."

That's the "calculator AND judge" capability the project owner asked for. The architecture is domain-agnostic; the three-layer pattern applies to any domain where predictions meet reality. Phase 3H.1's sweep verb is the automation that turns manual weight-poking into systematic sensitivity analysis.
