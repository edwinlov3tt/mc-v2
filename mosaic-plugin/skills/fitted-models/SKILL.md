---
name: mosaic-fitted-models
description: How to declare and evaluate pre-fitted statistical models (Lasso, Ridge, logistic regression, etc.) in Mosaic YAML using predict(), calibrate(), exp(), and norm_cdf(). Use when the user wants to evaluate ML models, compute predictions from coefficients, calibrate probabilities, or use exponential/normal-CDF functions in formulas. Covers the fitted_models and calibration_maps YAML blocks, sklearn export workflow, and diagnostic codes MC2050-MC2056/MC3017-MC3018.
---

# Fitted Models in Mosaic

Mosaic evaluates pre-fitted statistical models inside formulas. Fitting happens outside Mosaic (Python/sklearn/R); evaluation happens inside.

## Declaring a Fitted Model

Add a `fitted_models:` block to your YAML:

```yaml
fitted_models:
  - name: "revenue_forecast_v2"
    method: "linear"              # "linear" or "logistic"
    intercept: 1250.7
    coefficients:
      - { feature: "marketing_spend", weight: 0.85 }
      - { feature: "seasonality_index", weight: 312.4 }
      - { feature: "competitor_price", weight: -0.42 }
    standardization:              # optional; for z-scored inputs
      method: "zscore"
      params:
        - { feature: "marketing_spend", mean: 50000, std: 15000 }
        - { feature: "seasonality_index", mean: 1.0, std: 0.3 }
        - { feature: "competitor_price", mean: 29.99, std: 5.0 }
    residual_std: 450.2           # for norm_cdf probability queries
    metadata:
      fitted_at: "2026-04-01T00:00:00Z"
      algorithm: "ridge"
      n_train: 2400
      holdout_mae: 380.5
```

## Using `predict()` in Formulas

```yaml
rules:
  - name: "forecast_revenue"
    target_measure: "Predicted_Revenue"
    body: "predict(\"revenue_forecast_v2\", marketing_spend, seasonality_index, competitor_price)"
    declared_dependencies: [marketing_spend, seasonality_index, competitor_price]
```

Features are **positional** -- they map to the `coefficients:` array in order.

### How prediction works

**Linear (`method: "linear"`):**
1. For each feature: if standardization exists, z-score it: `z = (value - mean) / std`
2. `result = intercept + sum(z_i * weight_i)`

**Logistic (`method: "logistic"`):**
1. Same as linear
2. Apply sigmoid: `result = 1 / (1 + exp(-linear_result))`

### Null handling
If ANY feature is Null, `predict()` returns Null (null-poisoning).

## Calibration Maps

Transform raw probabilities into calibrated probabilities:

```yaml
calibration_maps:
  - name: "conversion_calibration"
    method: "pava"                # isotonic regression
    points:
      - { raw: 0.1, calibrated: 0.08 }
      - { raw: 0.3, calibrated: 0.25 }
      - { raw: 0.5, calibrated: 0.48 }
      - { raw: 0.7, calibrated: 0.72 }
      - { raw: 0.9, calibrated: 0.91 }
```

Use in formulas:
```yaml
body: "calibrate(raw_probability, \"conversion_calibration\")"
```

**PAVA method:** linear interpolation between adjacent points; clamps at edges.
**Platt method:** `1 / (1 + exp(a * raw + b))` using declared `platt_params`.

## Standalone Functions

### `exp(x)` -- exponential
Returns `e^x`. Useful for growth/decay models.
```yaml
body: "Starting_Value * exp(Growth_Rate * period_index())"
```

### `norm_cdf(x, mu, sigma)` -- normal CDF
Returns P(X <= x) where X ~ Normal(mu, sigma).
```yaml
# Probability game total exceeds the spread
body: "1 - norm_cdf(Market_Line, Predicted_Total, 17.25)"
```

Returns Null if sigma <= 0.

## Exporting from sklearn

```python
from sklearn.linear_model import Lasso
model = Lasso(alpha=0.7).fit(X_train, y_train)

# Export to Mosaic YAML
import yaml
print(yaml.dump({
    "fitted_models": [{
        "name": "my_model_v1",
        "method": "linear",
        "intercept": float(model.intercept_),
        "coefficients": [
            {"feature": name, "weight": float(coef)}
            for name, coef in zip(feature_names, model.coef_)
            if abs(coef) > 1e-10
        ],
    }]
}))
```

Works identically for Ridge, ElasticNet, OLS, and LogisticRegression (change method to "logistic").

## Diagnostic Codes

| Code | Fires when |
|------|-----------|
| MC2050 | `predict()` references unknown model_id |
| MC2051 | `predict()` arg count != coefficient count |
| MC2052 | `calibrate()` references unknown map_id |
| MC2053 | Duplicate name in fitted_models/calibration_maps |
| MC2054 | Calibration points not in ascending raw order |
| MC2055 | Calibration map has < 2 points |
| MC2056 | Standardization feature not in coefficients |
| MC3017 | Lint: fitted_model fitted_at > 6 months old |
| MC3018 | Lint: calibration_map fitted_at > 6 months old |
