//! Phase 3H.1 (ADR-0017) — `output_bound` clamp on fitted models.
//!
//! Adds a regression suite for the small additive `output_bound: { min, max }`
//! field on `ParsedFittedModel` / `FittedModelData`. Coverage:
//!
//! 1. `min`-only clamp (floor; floor an under-prediction).
//! 2. `max`-only clamp (ceiling; clip an over-prediction).
//! 3. `min` and `max` together (in-band, below-band, above-band).
//! 4. Validator MC2070 fires when `min >= max`.
//! 5. Logistic safety bounds (clamp away from saturation extremes).
//! 6. Backward compat: a fitted model without `output_bound` evaluates
//!    identically to its pre-3H.1 behavior.
//!
//! Closes the Amarillo case from the post-6A audit (M-20): a Lasso linear
//! regression that extrapolated to -$5,706 at zero spend. With
//! `output_bound: { min: 0 }`, the prediction clamps to 0.

use std::collections::BTreeMap;

use mc_core::{CellCoordinate, ScalarValue, WriteIntent, WritebackRequest};
use mc_model::{load_str, CompiledCube, ModelRefs};

// ---------------------------------------------------------------------------
// Helpers (shared shape with formula_integration.rs).
// ---------------------------------------------------------------------------

fn build_test_cube(yaml: &str) -> CompiledCube {
    load_str(yaml, Some("output_bound_test".into())).unwrap_or_else(|errs| {
        for e in &errs {
            eprintln!("  error: {e}");
        }
        panic!("output_bound: model failed to load");
    })
}

fn coord(refs: &ModelRefs, slots: &[(&str, &str)]) -> CellCoordinate {
    let map: BTreeMap<String, String> = slots
        .iter()
        .map(|(d, e)| (d.to_string(), e.to_string()))
        .collect();
    refs.coord_from_names(&map)
        .unwrap_or_else(|| panic!("coord_from_names failed for {slots:?}"))
}

fn write_f64(
    cube: &mut mc_core::Cube,
    refs: &ModelRefs,
    principal: mc_core::PrincipalId,
    slots: &[(&str, &str)],
    value: f64,
) {
    let c = coord(refs, slots);
    cube.write(WritebackRequest {
        coord: c,
        new_value: ScalarValue::F64(value),
        principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .unwrap_or_else(|e| panic!("write failed at {slots:?}: {e}"));
}

fn read_f64(
    cube: &mut mc_core::Cube,
    refs: &ModelRefs,
    principal: mc_core::PrincipalId,
    slots: &[(&str, &str)],
) -> f64 {
    let c = coord(refs, slots);
    let cv = cube
        .read(&c, principal)
        .unwrap_or_else(|e| panic!("read failed at {slots:?}: {e}"));
    match cv.value {
        ScalarValue::F64(v) => v,
        other => panic!("expected F64 at {slots:?}, got {other:?}"),
    }
}

fn assert_f64_eq(actual: f64, expected: f64, label: &str) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "{label}: got {actual}, expected {expected}",
    );
}

/// Build a single-coord cube with one F64 input feature `Spend`, one Derived
/// `Result`, and a fitted model named `model` that maps `Spend` linearly. The
/// `output_bound_yaml` snippet is inlined under the fitted model. Pass an
/// empty string to omit the field entirely (backward-compat shape).
fn build_linear_model_yaml(intercept: f64, weight: f64, output_bound_yaml: &str) -> String {
    format!(
        r#"
model_format_version: 1
metadata:
  name: "OutputBoundTest"
  description: "test"
  author: "test"
  created: "2026-05-06"
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    elements:
      - {{ name: "Base", scenario_meta: "Default" }}
  - name: "Version"
    kind: "Version"
    elements:
      - {{ name: "Working", version_state: "Draft" }}
  - name: "Time"
    kind: "Time"
    elements:
      - {{ name: "P1" }}
  - name: "Channel"
    kind: "Standard"
    elements:
      - {{ name: "Web" }}
  - name: "Market"
    kind: "Standard"
    elements:
      - {{ name: "US" }}
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - {{ name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }}
  - {{ name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }}
fitted_models:
  - name: "model"
    method: "linear"
    intercept: {intercept}
    coefficients:
      - {{ feature: "Spend", weight: {weight} }}
{output_bound_yaml}
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "predict(\"model\", Spend)"
    declared_dependencies: ["Spend"]
"#
    )
}

const BASE_COORD: &[(&str, &str)] = &[
    ("Scenario", "Base"),
    ("Version", "Working"),
    ("Time", "P1"),
    ("Channel", "Web"),
    ("Market", "US"),
    ("Measure", "Spend"),
];

const RESULT_COORD: &[(&str, &str)] = &[
    ("Scenario", "Base"),
    ("Version", "Working"),
    ("Time", "P1"),
    ("Channel", "Web"),
    ("Market", "US"),
    ("Measure", "Result"),
];

// ---------------------------------------------------------------------------
// Test 1 — `min`-only clamp (the Amarillo case).
// ---------------------------------------------------------------------------

#[test]
fn test_output_bound_min_only_clamps_low_predictions() {
    // Linear: prediction = -1000 + 1.0 * Spend. With Spend=0 the natural
    // prediction is -1000; the `min: 0` clamp must floor it at 0.
    let yaml = build_linear_model_yaml(-1000.0, 1.0, "    output_bound:\n      min: 0.0\n");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(&mut cube, &compiled.refs, p, BASE_COORD, 0.0);
    let val = read_f64(&mut cube, &compiled.refs, p, RESULT_COORD);
    assert_f64_eq(val, 0.0, "min=0 floors -1000 prediction to 0");

    // With Spend=1500 the natural prediction is +500; no clamp applies.
    write_f64(&mut cube, &compiled.refs, p, BASE_COORD, 1500.0);
    let val = read_f64(&mut cube, &compiled.refs, p, RESULT_COORD);
    assert_f64_eq(val, 500.0, "min=0 does not floor a positive prediction");
}

// ---------------------------------------------------------------------------
// Test 2 — `max`-only clamp.
// ---------------------------------------------------------------------------

#[test]
fn test_output_bound_max_only_clamps_high_predictions() {
    // Linear: prediction = 0 + 1.0 * Spend. With Spend=5000 the natural
    // prediction is 5000; the `max: 1000` clamp must ceiling it at 1000.
    let yaml = build_linear_model_yaml(0.0, 1.0, "    output_bound:\n      max: 1000.0\n");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(&mut cube, &compiled.refs, p, BASE_COORD, 5000.0);
    let val = read_f64(&mut cube, &compiled.refs, p, RESULT_COORD);
    assert_f64_eq(val, 1000.0, "max=1000 ceilings 5000 prediction to 1000");

    // With Spend=250 the natural prediction is 250; no clamp applies.
    write_f64(&mut cube, &compiled.refs, p, BASE_COORD, 250.0);
    let val = read_f64(&mut cube, &compiled.refs, p, RESULT_COORD);
    assert_f64_eq(val, 250.0, "max=1000 does not ceiling a smaller prediction");
}

// ---------------------------------------------------------------------------
// Test 3 — both `min` and `max`.
// ---------------------------------------------------------------------------

#[test]
fn test_output_bound_both_clamps_correctly() {
    // Linear: prediction = 0 + 1.0 * Spend. Bound = [10, 100].
    let yaml = build_linear_model_yaml(
        0.0,
        1.0,
        "    output_bound:\n      min: 10.0\n      max: 100.0\n",
    );
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    // Below-band → clamp to min.
    write_f64(&mut cube, &compiled.refs, p, BASE_COORD, -50.0);
    let val = read_f64(&mut cube, &compiled.refs, p, RESULT_COORD);
    assert_f64_eq(val, 10.0, "below-band clamps to min");

    // In-band → unchanged.
    write_f64(&mut cube, &compiled.refs, p, BASE_COORD, 42.0);
    let val = read_f64(&mut cube, &compiled.refs, p, RESULT_COORD);
    assert_f64_eq(val, 42.0, "in-band passes through");

    // Above-band → clamp to max.
    write_f64(&mut cube, &compiled.refs, p, BASE_COORD, 5000.0);
    let val = read_f64(&mut cube, &compiled.refs, p, RESULT_COORD);
    assert_f64_eq(val, 100.0, "above-band clamps to max");
}

// ---------------------------------------------------------------------------
// Test 4 — validator MC2070 fires on min >= max.
// ---------------------------------------------------------------------------

#[test]
fn test_output_bound_min_gte_max_fails_mc2070() {
    let yaml = build_linear_model_yaml(
        0.0,
        1.0,
        "    output_bound:\n      min: 1.0\n      max: 0.5\n",
    );
    let result = load_str(&yaml, Some("test".into()));
    assert!(result.is_err(), "min > max must fail to load");
    let errs = result.unwrap_err();
    let any_mc2070 = errs.iter().any(|e| format!("{e:?}").contains("MC2070"));
    assert!(any_mc2070, "expected MC2070 in errors: {errs:?}");

    // The strict-inequality boundary: min == max must also fail.
    let yaml = build_linear_model_yaml(
        0.0,
        1.0,
        "    output_bound:\n      min: 1.0\n      max: 1.0\n",
    );
    let result = load_str(&yaml, Some("test".into()));
    assert!(result.is_err(), "min == max must fail to load");
    let errs = result.unwrap_err();
    let any_mc2070 = errs.iter().any(|e| format!("{e:?}").contains("MC2070"));
    assert!(any_mc2070, "expected MC2070 (strict <) in errors: {errs:?}");
}

// ---------------------------------------------------------------------------
// Test 5 — logistic with safety bounds.
// ---------------------------------------------------------------------------

#[test]
fn test_output_bound_logistic_with_safety_bounds() {
    // Logistic: sigmoid(intercept + weight*Spend). With a large positive
    // linear sum the sigmoid saturates very close to 1.0; the
    // `max: 0.999` clamp must keep it strictly below the saturation
    // extreme. Bounds inside [0, 1] are the typical logistic use case
    // (ADR-0017 Decision 4: not warned even though "outside the natural
    // sigmoid range" would be tempting to lint — false positives).
    let yaml = r#"
model_format_version: 1
metadata:
  name: "LogisticBoundTest"
  description: "test"
  author: "test"
  created: "2026-05-06"
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    elements:
      - { name: "Base", scenario_meta: "Default" }
  - name: "Version"
    kind: "Version"
    elements:
      - { name: "Working", version_state: "Draft" }
  - name: "Time"
    kind: "Time"
    elements:
      - { name: "P1" }
  - name: "Channel"
    kind: "Standard"
    elements:
      - { name: "Web" }
  - name: "Market"
    kind: "Standard"
    elements:
      - { name: "US" }
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - { name: "Score", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "Probability", role: "Derived", data_type: "F64", aggregation: "Sum" }
fitted_models:
  - name: "logit"
    method: "logistic"
    intercept: 0.0
    coefficients:
      - { feature: "Score", weight: 1.0 }
    output_bound:
      min: 0.001
      max: 0.999
rules:
  - name: "rule_prob"
    target_measure: "Probability"
    scope: "AllLeaves"
    body: "predict(\"logit\", Score)"
    declared_dependencies: ["Score"]
"#;
    let compiled = build_test_cube(yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    let base = &[
        ("Scenario", "Base"),
        ("Version", "Working"),
        ("Time", "P1"),
        ("Channel", "Web"),
        ("Market", "US"),
        ("Measure", "Score"),
    ];
    let result_coord = &[
        ("Scenario", "Base"),
        ("Version", "Working"),
        ("Time", "P1"),
        ("Channel", "Web"),
        ("Market", "US"),
        ("Measure", "Probability"),
    ];

    // Score = 30 → sigmoid(30) ≈ 1 - 1e-13. Without the bound, this would
    // produce a probability indistinguishable from 1.0 in f64; the
    // `max: 0.999` clamp must bring it back to exactly 0.999.
    write_f64(&mut cube, &compiled.refs, p, base, 30.0);
    let val = read_f64(&mut cube, &compiled.refs, p, result_coord);
    assert_f64_eq(
        val,
        0.999,
        "logistic saturates above max → clamped to 0.999",
    );

    // Score = -30 → sigmoid(-30) ≈ 1e-13 (≈ 0); the `min: 0.001` clamp
    // must floor it at 0.001.
    write_f64(&mut cube, &compiled.refs, p, base, -30.0);
    let val = read_f64(&mut cube, &compiled.refs, p, result_coord);
    assert_f64_eq(
        val,
        0.001,
        "logistic saturates below min → clamped to 0.001",
    );

    // Score = 0 → sigmoid(0) = 0.5; in-band, passes through unchanged.
    write_f64(&mut cube, &compiled.refs, p, base, 0.0);
    let val = read_f64(&mut cube, &compiled.refs, p, result_coord);
    assert_f64_eq(val, 0.5, "logistic in-band passes through");
}

// ---------------------------------------------------------------------------
// Test 6 — backward compat: existing fitted models (no output_bound)
//          evaluate identically.
// ---------------------------------------------------------------------------

#[test]
fn test_fitted_model_without_output_bound_unchanged() {
    // Same structural shape as Phase 3H's `test_predict_linear_evaluates`,
    // sans `output_bound`. Verifies the additive field truly defaults to
    // None and that prediction matches the pre-3H.1 closed-form result.
    let yaml = r#"
model_format_version: 1
metadata:
  name: "BackCompatTest"
  description: "test"
  author: "test"
  created: "2026-05-06"
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    elements:
      - { name: "Base", scenario_meta: "Default" }
  - name: "Version"
    kind: "Version"
    elements:
      - { name: "Working", version_state: "Draft" }
  - name: "Time"
    kind: "Time"
    elements:
      - { name: "P1" }
  - name: "Channel"
    kind: "Standard"
    elements:
      - { name: "Web" }
  - name: "Market"
    kind: "Standard"
    elements:
      - { name: "US" }
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - { name: "Feature1", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "Feature2", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
fitted_models:
  - name: "linear_model"
    method: "linear"
    intercept: 10.0
    coefficients:
      - { feature: "Feature1", weight: 2.0 }
      - { feature: "Feature2", weight: 3.0 }
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "predict(\"linear_model\", Feature1, Feature2)"
    declared_dependencies: ["Feature1", "Feature2"]
"#;
    let compiled = build_test_cube(yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    // Drive Feature1, Feature2 to a deeply-negative product. Without
    // `output_bound`, the prediction is 10 + 2*(-100) + 3*(-100) = -490
    // — exactly the unclamped Phase 3H behavior.
    let f1 = &[
        ("Scenario", "Base"),
        ("Version", "Working"),
        ("Time", "P1"),
        ("Channel", "Web"),
        ("Market", "US"),
        ("Measure", "Feature1"),
    ];
    let f2 = &[
        ("Scenario", "Base"),
        ("Version", "Working"),
        ("Time", "P1"),
        ("Channel", "Web"),
        ("Market", "US"),
        ("Measure", "Feature2"),
    ];
    let result_coord = &[
        ("Scenario", "Base"),
        ("Version", "Working"),
        ("Time", "P1"),
        ("Channel", "Web"),
        ("Market", "US"),
        ("Measure", "Result"),
    ];
    write_f64(&mut cube, &compiled.refs, p, f1, -100.0);
    write_f64(&mut cube, &compiled.refs, p, f2, -100.0);

    let val = read_f64(&mut cube, &compiled.refs, p, result_coord);
    assert_f64_eq(
        val,
        -490.0,
        "no output_bound → unclamped prediction (pre-3H.1 behavior)",
    );

    // And the happy-path positive sum: 10 + 2*5 + 3*4 = 32 (matches the
    // existing `test_predict_linear_evaluates`).
    write_f64(&mut cube, &compiled.refs, p, f1, 5.0);
    write_f64(&mut cube, &compiled.refs, p, f2, 4.0);
    let val = read_f64(&mut cube, &compiled.refs, p, result_coord);
    assert_f64_eq(val, 32.0, "no output_bound → pre-3H.1 closed-form result");
}
