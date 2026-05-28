//! End-to-end formula integration tests.
//!
//! Each test builds a real cube from inline YAML, writes input cells, reads
//! derived cells, and asserts specific values. Exercises every formula
//! function against the full eval pipeline (parse → validate → compile →
//! write → read). These tests would have caught all 6 bugs the Tide Cleaners
//! real-world test surfaced.

use std::collections::BTreeMap;

use mc_core::{CellCoordinate, ScalarValue, WriteIntent, WritebackRequest};
use mc_model::{load_str, CompiledCube, ModelRefs};

// ============================================================================
// Helpers
// ============================================================================

fn build_test_cube(yaml: &str) -> CompiledCube {
    load_str(yaml, Some("formula_integration_test".into())).unwrap_or_else(|errs| {
        for e in &errs {
            eprintln!("  error: {e}");
        }
        panic!("formula_integration: model failed to load");
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

fn read_value(
    cube: &mut mc_core::Cube,
    refs: &ModelRefs,
    principal: mc_core::PrincipalId,
    slots: &[(&str, &str)],
) -> ScalarValue {
    let c = coord(refs, slots);
    let cv = cube
        .read(&c, principal)
        .unwrap_or_else(|e| panic!("read failed at {slots:?}: {e}"));
    cv.value
}

fn assert_f64_eq(actual: f64, expected: f64, label: &str) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "{label}: got {actual}, expected {expected}"
    );
}

fn assert_null(value: ScalarValue, label: &str) {
    assert!(
        matches!(value, ScalarValue::Null),
        "{label}: expected Null, got {value:?}"
    );
}

// ============================================================================
// Minimal model YAML templates
// ============================================================================

/// Two-element dims, one input + one derived.
fn simple_model(rule_body: &str, deps: &str) -> String {
    format!(
        r#"
model_format_version: 1
metadata:
  name: "FormulaTest"
  description: "test"
  author: "test"
  created: "2026-01-01"
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
      - {{ name: "P2" }}
      - {{ name: "P3" }}
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
  - {{ name: "Revenue", role: "Input", data_type: "F64", aggregation: "Sum" }}
  - {{ name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "{rule_body}"
    declared_dependencies: [{deps}]
"#
    )
}

// ============================================================================
// Category 1: Phase 3E — Conditionals and Basic Operations
// ============================================================================

#[test]
fn test_if_true_branch() {
    let yaml = simple_model("if(1 > 0, 100, 200)", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 100.0, "if(1>0, 100, 200)");
}

#[test]
fn test_if_false_branch() {
    let yaml = simple_model("if(0 > 1, 100, 200)", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 200.0, "if(0>1, 100, 200)");
}

#[test]
fn test_if_null_condition_takes_else() {
    // Spend is not written → Null; if(Null > 5, ...) → Null comparison → else
    let yaml = simple_model("if(Spend > 5, 100, 200)", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 200.0, "if(Null > 5, 100, 200) → else");
}

#[test]
fn test_comparisons_with_null_return_null() {
    // Rule body is just (Spend > 5), which should be Null when Spend is Null
    let yaml = simple_model("Spend > 5", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let val = read_value(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_null(val, "Null > 5 → Null");
}

#[test]
fn test_and_or_not() {
    let yaml = simple_model(
        "if(Spend > 0 and Revenue > 0, 1, 0)",
        r#""Spend", "Revenue""#,
    );
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    let base = &[
        ("Scenario", "Base"),
        ("Version", "Working"),
        ("Time", "P1"),
        ("Channel", "Web"),
        ("Market", "US"),
    ];

    // Write both positive
    let mut spend_slots: Vec<(&str, &str)> = base.to_vec();
    spend_slots.push(("Measure", "Spend"));
    write_f64(&mut cube, &compiled.refs, p, &spend_slots, 100.0);

    let mut rev_slots: Vec<(&str, &str)> = base.to_vec();
    rev_slots.push(("Measure", "Revenue"));
    write_f64(&mut cube, &compiled.refs, p, &rev_slots, 50.0);

    let mut result_slots: Vec<(&str, &str)> = base.to_vec();
    result_slots.push(("Measure", "Result"));
    let val = read_f64(&mut cube, &compiled.refs, p, &result_slots);
    assert_f64_eq(val, 1.0, "Spend>0 and Revenue>0 with both positive");
}

#[test]
fn test_min_max() {
    let yaml = simple_model("min(10, 20)", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 10.0, "min(10, 20)");
}

#[test]
fn test_max_function() {
    let yaml = simple_model("max(10, 20)", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 20.0, "max(10, 20)");
}

#[test]
fn test_abs_negative() {
    let yaml = simple_model("abs(0 - 42)", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 42.0, "abs(-42)");
}

#[test]
fn test_abs_positive() {
    let yaml = simple_model("abs(42)", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 42.0, "abs(42)");
}

#[test]
fn test_safe_div_normal() {
    let yaml = simple_model("safe_div(100, 4, 0)", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 25.0, "safe_div(100, 4, 0)");
}

#[test]
fn test_safe_div_zero_denominator() {
    let yaml = simple_model("safe_div(100, 0, -1)", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, -1.0, "safe_div(100, 0, -1) → default");
}

#[test]
fn test_safe_div_null_denominator() {
    // Spend is not written → Null; safe_div(100, Spend, -1) → -1
    let yaml = simple_model("safe_div(100, Spend, -1)", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, -1.0, "safe_div(100, Null, -1) → default");
}

#[test]
fn test_clamp_above() {
    let yaml = simple_model("clamp(150, 0, 100)", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 100.0, "clamp(150, 0, 100) → 100");
}

#[test]
fn test_clamp_below() {
    let yaml = simple_model("clamp(0 - 5, 0, 100)", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 0.0, "clamp(-5, 0, 100) → 0");
}

#[test]
fn test_clamp_in_range() {
    let yaml = simple_model("clamp(50, 0, 100)", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 50.0, "clamp(50, 0, 100) → 50");
}

#[test]
fn test_coalesce() {
    // coalesce(Spend, Revenue, 42) where both are Null → 42
    let yaml = simple_model("coalesce(Spend, Revenue, 42)", r#""Spend", "Revenue""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 42.0, "coalesce(Null, Null, 42) → 42");
}

#[test]
fn test_actual_ref_reads_from_actuals_scenario() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "ActualRefTest"
  description: "test"
  author: "test"
  created: "2026-01-01"
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    actuals_element: "Actual"
    elements:
      - { name: "Actual", scenario_meta: "Default" }
      - { name: "Forecast", scenario_meta: "NonDefault" }
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
  - { name: "Revenue", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "actual_ref(Revenue)"
    declared_dependencies: ["Revenue"]
"#;
    let compiled = build_test_cube(yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    // Write Revenue=500 in Actual scenario
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Actual"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        500.0,
    );

    // Read Result at Forecast scenario → should read Actual's Revenue = 500
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Forecast"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 500.0, "actual_ref(Revenue) at Forecast → 500");
}

#[test]
fn test_actual_ref_same_scenario_returns_value() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "ActualRefSame"
  description: "test"
  author: "test"
  created: "2026-01-01"
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    actuals_element: "Actual"
    elements:
      - { name: "Actual", scenario_meta: "Default" }
      - { name: "Forecast", scenario_meta: "NonDefault" }
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
  - { name: "Revenue", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "actual_ref(Revenue)"
    declared_dependencies: ["Revenue"]
"#;
    let compiled = build_test_cube(yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Actual"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        500.0,
    );

    // Read Result at Actual scenario → should ALSO get 500 (not Null)
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Actual"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 500.0, "actual_ref(Revenue) at Actual → 500");
}

// ============================================================================
// Category 2: Phase 3F — Time-Series
// ============================================================================

#[test]
fn test_prev_returns_prior_period() {
    let yaml = simple_model("prev(Revenue)", r#""Revenue""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        100.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P2"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        200.0,
    );

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P2"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 100.0, "prev(Revenue) at P2 → P1 value (100)");
}

#[test]
fn test_prev_at_first_period_returns_null() {
    let yaml = simple_model("prev(Revenue)", r#""Revenue""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        100.0,
    );

    let val = read_value(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_null(val, "prev(Revenue) at P1 → Null");
}

#[test]
fn test_lag_positive() {
    let yaml = simple_model("lag(Revenue, 2)", r#""Revenue""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        100.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P2"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        200.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        300.0,
    );

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 100.0, "lag(Revenue, 2) at P3 → P1 value (100)");
}

#[test]
fn test_lag_negative_is_lead() {
    let yaml = simple_model("lag(Revenue, 0 - 1)", r#""Revenue""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        100.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P2"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        200.0,
    );

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 200.0, "lag(Revenue, -1) at P1 → P2 value (200)");
}

#[test]
fn test_cumulative() {
    let yaml = simple_model("cumulative(Revenue)", r#""Revenue""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        10.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P2"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        20.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        30.0,
    );

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 60.0, "cumulative(Revenue) at P3 → 10+20+30=60");
}

#[test]
fn test_cumulative_at_first_period() {
    let yaml = simple_model("cumulative(Revenue)", r#""Revenue""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        10.0,
    );

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 10.0, "cumulative(Revenue) at P1 → 10");
}

#[test]
fn test_rolling_avg() {
    let yaml = simple_model("rolling_avg(Revenue, 2)", r#""Revenue""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        10.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P2"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        20.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        30.0,
    );

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 25.0, "rolling_avg(Revenue, 2) at P3 → (20+30)/2=25");
}

#[test]
fn test_rolling_avg_partial_window() {
    let yaml = simple_model("rolling_avg(Revenue, 3)", r#""Revenue""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        10.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P2"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        20.0,
    );

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P2"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 15.0, "rolling_avg(Revenue, 3) at P2 → (10+20)/2=15");
}

#[test]
fn test_period_index() {
    let yaml = simple_model("period_index()", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    let v1 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    let v2 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P2"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    let v3 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );

    assert_f64_eq(v1, 0.0, "period_index() at P1");
    assert_f64_eq(v2, 1.0, "period_index() at P2");
    assert_f64_eq(v3, 2.0, "period_index() at P3");
}

// ============================================================================
// Category 3: Phase 3F.1 — Time Anchor
// ============================================================================

fn anchor_model(rule_body: &str, deps: &str) -> String {
    format!(
        r#"
model_format_version: 1
metadata:
  name: "AnchorTest"
  description: "test"
  author: "test"
  created: "2026-01-01"
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
    time_anchor: "P2"
    elements:
      - {{ name: "P1" }}
      - {{ name: "P2" }}
      - {{ name: "P3" }}
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
  - {{ name: "Revenue", role: "Input", data_type: "F64", aggregation: "Sum" }}
  - {{ name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "{rule_body}"
    declared_dependencies: [{deps}]
"#
    )
}

#[test]
fn test_anchor_index_from_yaml() {
    let yaml = anchor_model("anchor_index()", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    // anchor is P2 → index 1
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 1.0, "anchor_index() → 1.0 (P2 is index 1)");
}

#[test]
fn test_is_past() {
    let yaml = anchor_model("is_past()", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    let v1 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    let v2 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P2"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    let v3 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );

    assert_f64_eq(v1, 1.0, "is_past() at P1 (before anchor P2)");
    assert_f64_eq(v2, 0.0, "is_past() at P2 (anchor)");
    assert_f64_eq(v3, 0.0, "is_past() at P3 (after anchor)");
}

#[test]
fn test_is_current() {
    let yaml = anchor_model("is_current()", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    let v1 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    let v2 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P2"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    let v3 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );

    assert_f64_eq(v1, 0.0, "is_current() at P1");
    assert_f64_eq(v2, 1.0, "is_current() at P2 (anchor)");
    assert_f64_eq(v3, 0.0, "is_current() at P3");
}

#[test]
fn test_is_future() {
    let yaml = anchor_model("is_future()", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    let v1 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    let v2 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P2"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    let v3 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );

    assert_f64_eq(v1, 0.0, "is_future() at P1");
    assert_f64_eq(v2, 0.0, "is_future() at P2 (anchor)");
    assert_f64_eq(v3, 1.0, "is_future() at P3 (after anchor)");
}

#[test]
fn test_periods_since_anchor() {
    let yaml = anchor_model("periods_since_anchor()", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    let v1 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    let v2 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P2"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    let v3 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );

    assert_f64_eq(v1, -1.0, "periods_since_anchor() at P1 → -1");
    assert_f64_eq(v2, 0.0, "periods_since_anchor() at P2 → 0");
    assert_f64_eq(v3, 1.0, "periods_since_anchor() at P3 → 1");
}

#[test]
fn test_periods_to_end() {
    let yaml = anchor_model("periods_to_end()", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    let v1 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    let v2 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P2"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    let v3 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );

    assert_f64_eq(v1, 2.0, "periods_to_end() at P1 → 2");
    assert_f64_eq(v2, 1.0, "periods_to_end() at P2 → 1");
    assert_f64_eq(v3, 0.0, "periods_to_end() at P3 → 0");
}

// ============================================================================
// Category 4: Phase 3G — Reference Data
// ============================================================================

#[test]
fn test_lookup_single_key() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "LookupTest"
  description: "test"
  author: "test"
  created: "2026-01-01"
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
      - { name: "P2" }
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
  - { name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
lookup_tables:
  - name: "seasonality"
    key_dimension: "Time"
    values:
      P1: 1.2
      P2: 0.8
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "lookup(\"seasonality\", Time)"
    declared_dependencies: ["Spend"]
"#;
    let compiled = build_test_cube(yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    let v1 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    let v2 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P2"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );

    assert_f64_eq(v1, 1.2, "lookup(seasonality, Time) at P1 → 1.2");
    assert_f64_eq(v2, 0.8, "lookup(seasonality, Time) at P2 → 0.8");
}

#[test]
fn test_lookup_returns_null_for_missing_key() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "LookupMissTest"
  description: "test"
  author: "test"
  created: "2026-01-01"
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
      - { name: "P2" }
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
  - { name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
lookup_tables:
  - name: "seasonality"
    key_dimension: "Time"
    values:
      P1: 1.2
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "lookup(\"seasonality\", Time)"
    declared_dependencies: ["Spend"]
"#;
    let compiled = build_test_cube(yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    // P2 is not in the lookup table → Null
    let val = read_value(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P2"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_null(val, "lookup(seasonality, Time) at P2 (not in table) → Null");
}

#[test]
fn test_benchmark() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "BenchmarkTest"
  description: "test"
  author: "test"
  created: "2026-01-01"
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
      - { name: "Email" }
  - name: "Market"
    kind: "Standard"
    elements:
      - { name: "US" }
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - { name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
benchmarks:
  - name: "industry_cpc"
    source: "test"
    last_updated: "2026-01-01"
    key_dimension: "Channel"
    values:
      Web: 2.50
      Email: 0.10
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "benchmark(\"industry_cpc\", Channel)"
    declared_dependencies: ["Spend"]
"#;
    let compiled = build_test_cube(yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    let v_web = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    let v_email = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Email"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );

    assert_f64_eq(
        v_web,
        2.50,
        "benchmark(industry_cpc, Channel) at Web → 2.50",
    );
    assert_f64_eq(
        v_email,
        0.10,
        "benchmark(industry_cpc, Channel) at Email → 0.10",
    );
}

#[test]
fn test_bucket_returns_band_index() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "BucketTest"
  description: "test"
  author: "test"
  created: "2026-01-01"
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
      - { name: "P2" }
      - { name: "P3" }
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
  - { name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
status_thresholds:
  - name: "spend_tier"
    bands:
      - { label: "low", max: 25.0 }
      - { label: "mid", max: 50.0 }
      - { label: "high" }
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "bucket(Spend, \"spend_tier\")"
    declared_dependencies: ["Spend"]
"#;
    let compiled = build_test_cube(yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    // Write different spend values at different periods
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        10.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P2"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        30.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        75.0,
    );

    let v1 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    let v2 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P2"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    let v3 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );

    assert_f64_eq(v1, 0.0, "bucket(10, spend_tier) → band 0 (low)");
    assert_f64_eq(v2, 1.0, "bucket(30, spend_tier) → band 1 (mid)");
    assert_f64_eq(v3, 2.0, "bucket(75, spend_tier) → band 2 (high)");
}

#[test]
fn test_bucket_null_input() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "BucketNullTest"
  description: "test"
  author: "test"
  created: "2026-01-01"
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
  - { name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
status_thresholds:
  - name: "spend_tier"
    bands:
      - { label: "low", max: 25.0 }
      - { label: "high" }
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "bucket(Spend, \"spend_tier\")"
    declared_dependencies: ["Spend"]
"#;
    let compiled = build_test_cube(yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    // Don't write Spend → Null
    let val = read_value(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_null(val, "bucket(Null, spend_tier) → Null");
}

#[test]
fn test_sum_over_leaf_elements() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "SumOverTest"
  description: "test"
  author: "test"
  created: "2026-01-01"
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
      - { name: "UK" }
      - { name: "DE" }
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - { name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "safe_div(Spend, sum_over(Market, Spend), 0)"
    declared_dependencies: ["Spend"]
"#;
    let compiled = build_test_cube(yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    // Write spend for each market
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        100.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "UK"),
            ("Measure", "Spend"),
        ],
        200.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "DE"),
            ("Measure", "Spend"),
        ],
        300.0,
    );

    // Result for US = 100 / (100+200+300) = 100/600 = 0.16666...
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 100.0 / 600.0, "sum_over Market share: US = 100/600");

    let val_uk = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "UK"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val_uk, 200.0 / 600.0, "sum_over Market share: UK = 200/600");
}

// ============================================================================
// Category 5: Phase 3H — Fitted Models
// (Note: predict/calibrate are NOT wired at the cube layer yet — they return
//  Null per the 3H completion report. These tests document the expected
//  behavior and will pass once wiring is complete.)
// ============================================================================

#[test]
fn test_predict_linear_evaluates() {
    // Phase 3H predict: linear_model with intercept=10, weights=[2,3]
    let yaml = r#"
model_format_version: 1
metadata:
  name: "PredictTest"
  description: "test"
  author: "test"
  created: "2026-01-01"
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

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Feature1"),
        ],
        5.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Feature2"),
        ],
        4.0,
    );

    // Expected: 10 + 2*5 + 3*4 = 32.0
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 32.0, "predict(linear_model, 5, 4)");
}

#[test]
fn test_calibrate_pava_interpolates() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "CalibrateTest"
  description: "test"
  author: "test"
  created: "2026-01-01"
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
  - { name: "RawScore", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
calibration_maps:
  - name: "pava_map"
    method: "pava"
    points:
      - { raw: 0.0, calibrated: 0.1 }
      - { raw: 0.5, calibrated: 0.4 }
      - { raw: 1.0, calibrated: 0.9 }
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "calibrate(RawScore, \"pava_map\")"
    declared_dependencies: ["RawScore"]
"#;
    let compiled = build_test_cube(yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "RawScore"),
        ],
        0.55,
    );

    // PAVA interpolation: raw=0.55, segment [0.5,1.0] → [0.4,0.9]
    // frac = (0.55 - 0.5) / (1.0 - 0.5) = 0.1
    // result = 0.4 + 0.1 * (0.9 - 0.4) = 0.45
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 0.45, "calibrate(0.55, pava_map)");
}

#[test]
fn test_exp() {
    let yaml = simple_model("exp(0)", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 1.0, "exp(0) → 1.0");
}

#[test]
fn test_exp_one() {
    let yaml = simple_model("exp(1)", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert!(
        (val - std::f64::consts::E).abs() < 1e-9,
        "exp(1) → e ≈ 2.71828, got {val}"
    );
}

#[test]
fn test_norm_cdf_standard_normal_at_zero() {
    let yaml = simple_model("norm_cdf(0, 0, 1)", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    // The kernel uses an approximation (Abramowitz & Stegun or similar),
    // so we allow a wider epsilon for CDF values.
    assert!(
        (val - 0.5).abs() < 1e-6,
        "norm_cdf(0, 0, 1) ≈ 0.5, got {val}"
    );
}

#[test]
fn test_norm_cdf_standard_normal_at_196() {
    let yaml = simple_model("norm_cdf(1.96, 0, 1)", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert!(
        (val - 0.975).abs() < 0.001,
        "norm_cdf(1.96, 0, 1) ≈ 0.975, got {val}"
    );
}

#[test]
fn test_norm_cdf_negative_sigma_returns_null() {
    let yaml = simple_model("norm_cdf(0, 0, 0 - 1)", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    let val = read_value(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_null(val, "norm_cdf(0, 0, -1) → Null");
}

// ============================================================================
// Category 6: Cascade / Integration
// ============================================================================

#[test]
fn test_full_forecast_cascade() {
    // Mini Tide Cleaners: is_past → actual_ref(Revenue), else Spend * lookup
    let yaml = r#"
model_format_version: 1
metadata:
  name: "CascadeTest"
  description: "test"
  author: "test"
  created: "2026-01-01"
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    actuals_element: "Actual"
    elements:
      - { name: "Actual", scenario_meta: "Default" }
      - { name: "Forecast", scenario_meta: "NonDefault" }
  - name: "Version"
    kind: "Version"
    elements:
      - { name: "Working", version_state: "Draft" }
  - name: "Time"
    kind: "Time"
    time_anchor: "P2"
    elements:
      - { name: "P1" }
      - { name: "P2" }
      - { name: "P3" }
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
  - { name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "Revenue", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "NewCust", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "ForecastRev", role: "Derived", data_type: "F64", aggregation: "Sum" }
  - { name: "CAC", role: "Derived", data_type: "F64", aggregation: "Sum" }
lookup_tables:
  - name: "seasonality"
    key_dimension: "Time"
    values:
      P1: 1.5
      P2: 1.0
      P3: 0.8
rules:
  - name: "rule_forecast_rev"
    target_measure: "ForecastRev"
    scope: "AllLeaves"
    body: "if(is_past(), actual_ref(Revenue), Spend * lookup(\"seasonality\", Time))"
    declared_dependencies: ["Revenue", "Spend"]
  - name: "rule_cac"
    target_measure: "CAC"
    scope: "AllLeaves"
    body: "safe_div(Spend, NewCust, 0)"
    declared_dependencies: ["Spend", "NewCust"]
"#;
    let compiled = build_test_cube(yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    // Write actuals for P1 (past period): Revenue in Actual scenario
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Actual"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        1000.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Actual"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        200.0,
    );

    // Write forecast inputs for P3 (future)
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Forecast"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        500.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Forecast"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "NewCust"),
        ],
        10.0,
    );

    // ForecastRev at Forecast/P1 (is_past=true) → actual_ref(Revenue) → Actual/P1/Revenue = 1000
    let v_past = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Forecast"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "ForecastRev"),
        ],
    );
    assert_f64_eq(
        v_past,
        1000.0,
        "ForecastRev at P1 (past) → actual_ref(Revenue) = 1000",
    );

    // ForecastRev at Forecast/P3 (is_future=true, is_past=false) → Spend * seasonality
    // Spend = 500, seasonality P3 = 0.8 → 500 * 0.8 = 400
    let v_future = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Forecast"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "ForecastRev"),
        ],
    );
    assert_f64_eq(
        v_future,
        400.0,
        "ForecastRev at P3 (future) → Spend * seasonality = 400",
    );

    // CAC at Forecast/P3 → safe_div(500, 10, 0) = 50
    let cac = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Forecast"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "CAC"),
        ],
    );
    assert_f64_eq(cac, 50.0, "CAC at P3 → 500/10 = 50");
}

#[test]
fn test_what_if_spend_change() {
    // Write inputs, read derived, change one input, re-read, assert changed
    let yaml = r#"
model_format_version: 1
metadata:
  name: "WhatIfTest"
  description: "test"
  author: "test"
  created: "2026-01-01"
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
  - { name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "CPC", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "Clicks", role: "Derived", data_type: "F64", aggregation: "Sum" }
rules:
  - name: "rule_clicks"
    target_measure: "Clicks"
    scope: "AllLeaves"
    body: "safe_div(Spend, CPC, 0)"
    declared_dependencies: ["Spend", "CPC"]
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
    ];

    let mut spend_slots: Vec<(&str, &str)> = base.to_vec();
    spend_slots.push(("Measure", "Spend"));
    let mut cpc_slots: Vec<(&str, &str)> = base.to_vec();
    cpc_slots.push(("Measure", "CPC"));
    let mut clicks_slots: Vec<(&str, &str)> = base.to_vec();
    clicks_slots.push(("Measure", "Clicks"));

    write_f64(&mut cube, &compiled.refs, p, &spend_slots, 1000.0);
    write_f64(&mut cube, &compiled.refs, p, &cpc_slots, 2.0);

    // Clicks = 1000 / 2 = 500
    let v1 = read_f64(&mut cube, &compiled.refs, p, &clicks_slots);
    assert_f64_eq(v1, 500.0, "Clicks = 1000/2 = 500");

    // What-if: double the spend
    write_f64(&mut cube, &compiled.refs, p, &spend_slots, 2000.0);

    // Clicks should now be 2000 / 2 = 1000
    let v2 = read_f64(&mut cube, &compiled.refs, p, &clicks_slots);
    assert_f64_eq(v2, 1000.0, "Clicks after spend change = 2000/2 = 1000");
}

// ============================================================================
// Additional edge-case tests
// ============================================================================

#[test]
fn test_if_null_function() {
    let yaml = simple_model("if_null(Spend, 99)", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    // Spend not written → Null; if_null(Null, 99) → 99
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 99.0, "if_null(Null, 99) → 99");
}

#[test]
fn test_if_null_non_null() {
    let yaml = simple_model("if_null(Spend, 99)", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        42.0,
    );

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 42.0, "if_null(42, 99) → 42");
}

#[test]
fn test_arithmetic_operators() {
    let yaml = simple_model("Spend + Revenue", r#""Spend", "Revenue""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        100.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        50.0,
    );

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 150.0, "100 + 50 = 150");
}

#[test]
fn test_division_by_zero_returns_null() {
    let yaml = simple_model("Spend / Revenue", r#""Spend", "Revenue""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        100.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        0.0,
    );

    let val = read_value(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_null(val, "100 / 0 → Null");
}

#[test]
fn test_nested_if_with_time_functions() {
    // if(is_past(), prev(Revenue), Revenue * 2) — combining time anchor + time series
    let yaml = r#"
model_format_version: 1
metadata:
  name: "NestedTimeTest"
  description: "test"
  author: "test"
  created: "2026-01-01"
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
    time_anchor: "P2"
    elements:
      - { name: "P1" }
      - { name: "P2" }
      - { name: "P3" }
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
  - { name: "Revenue", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "if(is_future(), Revenue * 2, Revenue)"
    declared_dependencies: ["Revenue"]
"#;
    let compiled = build_test_cube(yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        100.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        100.0,
    );

    // P1 is past → Revenue = 100
    let v1 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(
        v1,
        100.0,
        "P1 (past): if(is_future(), Rev*2, Rev) → Rev = 100",
    );

    // P3 is future → Revenue * 2 = 200
    let v3 = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P3"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(v3, 200.0, "P3 (future): if(is_future(), Rev*2, Rev) → 200");
}

#[test]
fn test_or_logic() {
    let yaml = simple_model(
        "if(Spend > 100 or Revenue > 100, 1, 0)",
        r#""Spend", "Revenue""#,
    );
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    // Only Revenue > 100 → should be 1
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        50.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Revenue"),
        ],
        200.0,
    );

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 1.0, "Spend<100 or Revenue>100 → true → 1");
}

#[test]
fn test_not_logic() {
    let yaml = simple_model("if(not (Spend > 100), 1, 0)", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        50.0,
    );

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 1.0, "not(50>100) → true → 1");
}

#[test]
fn test_comparison_eq() {
    let yaml = simple_model("if(Spend == 100, 1, 0)", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        100.0,
    );

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 1.0, "Spend==100 → 1");
}

#[test]
fn test_comparison_neq() {
    let yaml = simple_model("if(Spend != 100, 1, 0)", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        99.0,
    );

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 1.0, "Spend!=100 (99) → 1");
}

#[test]
fn test_comparison_lte() {
    let yaml = simple_model("if(Spend <= 100, 1, 0)", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        100.0,
    );

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 1.0, "Spend<=100 (100) → 1");
}

#[test]
fn test_comparison_gte() {
    let yaml = simple_model("if(Spend >= 100, 1, 0)", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        100.0,
    );

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 1.0, "Spend>=100 (100) → 1");
}

#[test]
fn test_null_arithmetic_add_identity() {
    // Per spec §7: Null + 5 = 5 (Null is additive identity)
    let yaml = simple_model("Spend + 5", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(
        val,
        5.0,
        "Null + 5 → 5 (Null is additive identity per spec §7)",
    );
}

#[test]
fn test_null_mul_propagation() {
    // Per spec §7: Null * 5 = Null (Null propagates through mul)
    let yaml = simple_model("Spend * 5", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    let val = read_value(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_null(val, "Null * 5 → Null");
}

#[test]
fn test_mul_by_zero() {
    let yaml = simple_model("Spend * 0", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        100.0,
    );

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 0.0, "100 * 0 → 0");
}

// ============================================================================
// Phase 6A.1 — CRIT-1 regression: predict() standardization is name-keyed
// (https://docs/handoffs/phase-6a-1-fixes-handoff.md §"Block 1.1")
// ============================================================================

#[test]
fn test_predict_with_out_of_order_standardization_params() {
    // Two-feature model where standardization.params is intentionally listed
    // in the OPPOSITE order from coefficients. With the previous positional
    // pairing, Spend's value would be standardized using CPC's (mean, std)
    // and vice versa — silently producing a very wrong prediction. With the
    // name-keyed lookup, mean/std pair correctly with their feature.
    //
    // Coefficients:  Spend (w=1.0), CPC (w=1.0)
    // Standardization (declared CPC FIRST, Spend SECOND):
    //   CPC:   mean=2.0,    std=0.5
    //   Spend: mean=1000.0, std=200.0
    // Inputs:        Spend=1200.0, CPC=3.0
    // intercept=0
    //
    // By-name (correct):
    //   Spend' = (1200 - 1000) / 200 = 1.0
    //   CPC'   = (3   -    2) / 0.5 = 2.0
    //   result = 0 + 1.0 * 1.0 + 1.0 * 2.0 = 3.0
    //
    // By-position (the bug):
    //   Spend' = (1200 -    2) / 0.5 = 2396.0
    //   CPC'   = (3   - 1000) / 200 = -4.985
    //   result = 0 + 1.0 * 2396.0 + 1.0 * (-4.985) = 2391.015
    //
    // 3.0 vs 2391.015 — distinct enough that the assertion would have failed
    // against the old positional code at any sensible epsilon.
    let yaml = r#"
model_format_version: 1
metadata:
  name: "PredictOutOfOrderStdTest"
  description: "regression test for CRIT-1"
  author: "test"
  created: "2026-05-05"
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
  - { name: "Spend",  role: "Input",   data_type: "F64", aggregation: "Sum" }
  - { name: "CPC",    role: "Input",   data_type: "F64", aggregation: "Sum" }
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
fitted_models:
  - name: "ooo_model"
    method: "linear"
    intercept: 0.0
    coefficients:
      - { feature: "Spend", weight: 1.0 }
      - { feature: "CPC",   weight: 1.0 }
    standardization:
      method: "zscore"
      params:
        - { feature: "CPC",   mean: 2.0,    std: 0.5 }
        - { feature: "Spend", mean: 1000.0, std: 200.0 }
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "predict(\"ooo_model\", Spend, CPC)"
    declared_dependencies: ["Spend", "CPC"]
"#;
    let compiled = build_test_cube(yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        1200.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "CPC"),
        ],
        3.0,
    );

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 3.0, "predict() pairs (mean, std) with feature by name");
}

// ============================================================================
// Phase 6A.1 — MIN-6 regression: not()/if() use 1e-9 epsilon, not float ==
// (https://docs/handoffs/phase-6a-1-fixes-handoff.md §"Block 3.2")
// ============================================================================

#[test]
fn test_not_handles_arithmetic_zero() {
    // not(Spend - Spend) where Spend is a positive value.
    // Spend - Spend = exactly 0.0 in IEEE 754 (same bits cancel perfectly),
    // so this exercises the exact-zero path of the epsilon fix. The near-zero
    // path (values like 5e-10 that are conceptually zero but != 0.0) is
    // covered by the unit tests in mc-core::rule (eval_unified_not_near_zero_is_true).
    let yaml = r#"
model_format_version: 1
metadata:
  name: "NotEpsilonTest"
  description: "Phase 6A.1 MIN-6 regression"
  author: "test"
  created: "2026-05-05"
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
  - { name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "not(Spend - Spend)"
    declared_dependencies: ["Spend"]
"#;
    let compiled = build_test_cube(yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;

    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        12345.6789,
    );

    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    // not(0) is true (1.0). The fix ensures that "near-zero from float
    // arithmetic" still triggers the falsy branch of `not`.
    assert_f64_eq(val, 1.0, "not(Spend - Spend) → true (1.0)");
}

// ============================================================================
// Phase 3I — Item 2: Math primitives (pow, sqrt, ln, log10, round, floor,
// ceil, mod, norm_inv)
// ============================================================================

/// Evaluate a single-coord expression by stamping it into a derived rule
/// and reading the result. `inputs` writes Spend / Revenue if needed.
fn eval_math_primitive(rule_body: &str, deps: &str, inputs: &[(&str, f64)]) -> ScalarValue {
    let yaml = simple_model(rule_body, deps);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    for (measure, value) in inputs {
        write_f64(
            &mut cube,
            &compiled.refs,
            p,
            &[
                ("Scenario", "Base"),
                ("Version", "Working"),
                ("Time", "P1"),
                ("Channel", "Web"),
                ("Market", "US"),
                ("Measure", measure),
            ],
            *value,
        );
    }
    read_value(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    )
}

#[test]
fn test_pow_basic() {
    match eval_math_primitive("pow(2, 10)", "", &[]) {
        ScalarValue::F64(v) => assert_f64_eq(v, 1024.0, "pow(2,10)"),
        other => panic!("expected F64, got {other:?}"),
    }
    // Negative base + non-integer exp → Null (handoff item 2 W edge case)
    assert_null(eval_math_primitive("pow(-1, 0.5)", "", &[]), "pow(-1, 0.5)");
}

#[test]
fn test_sqrt_basic_and_negative_null() {
    match eval_math_primitive("sqrt(16)", "", &[]) {
        ScalarValue::F64(v) => assert_f64_eq(v, 4.0, "sqrt(16)"),
        other => panic!("expected F64, got {other:?}"),
    }
    assert_null(eval_math_primitive("sqrt(-1)", "", &[]), "sqrt(-1)");
}

#[test]
fn test_ln_and_negative_null() {
    match eval_math_primitive("ln(2.718281828459045)", "", &[]) {
        ScalarValue::F64(v) => assert_f64_eq(v, 1.0, "ln(e) ~= 1"),
        other => panic!("expected F64, got {other:?}"),
    }
    assert_null(eval_math_primitive("ln(0)", "", &[]), "ln(0)");
    assert_null(eval_math_primitive("ln(-1)", "", &[]), "ln(-1)");
}

#[test]
fn test_log10_basic_and_zero_null() {
    match eval_math_primitive("log10(1000)", "", &[]) {
        ScalarValue::F64(v) => assert_f64_eq(v, 3.0, "log10(1000)"),
        other => panic!("expected F64, got {other:?}"),
    }
    assert_null(eval_math_primitive("log10(0)", "", &[]), "log10(0)");
}

#[test]
fn test_round_floor_ceil() {
    match eval_math_primitive("round(2.7)", "", &[]) {
        ScalarValue::F64(v) => assert_f64_eq(v, 3.0, "round(2.7)"),
        other => panic!("expected F64, got {other:?}"),
    }
    match eval_math_primitive("floor(2.9)", "", &[]) {
        ScalarValue::F64(v) => assert_f64_eq(v, 2.0, "floor(2.9)"),
        other => panic!("expected F64, got {other:?}"),
    }
    match eval_math_primitive("ceil(2.1)", "", &[]) {
        ScalarValue::F64(v) => assert_f64_eq(v, 3.0, "ceil(2.1)"),
        other => panic!("expected F64, got {other:?}"),
    }
}

#[test]
fn test_mod_basic_and_zero_divisor_null() {
    match eval_math_primitive("mod(7, 3)", "", &[]) {
        ScalarValue::F64(v) => assert_f64_eq(v, 1.0, "mod(7,3)"),
        other => panic!("expected F64, got {other:?}"),
    }
    assert_null(eval_math_primitive("mod(7, 0)", "", &[]), "mod(7,0)");
}

#[test]
fn test_norm_inv_basic_and_boundary_null() {
    // Standard normal: norm_inv(0.5, 0, 1) = 0
    match eval_math_primitive("norm_inv(0.5, 0, 1)", "", &[]) {
        ScalarValue::F64(v) => assert!(v.abs() < 1e-6, "norm_inv(0.5, 0, 1) ≈ 0; got {v}"),
        other => panic!("expected F64, got {other:?}"),
    }
    // Boundary: p = 0 or p = 1 → Null
    assert_null(
        eval_math_primitive("norm_inv(0, 0, 1)", "", &[]),
        "norm_inv(0,..)",
    );
    assert_null(
        eval_math_primitive("norm_inv(1, 0, 1)", "", &[]),
        "norm_inv(1,..)",
    );
    // sigma <= 0 → Null
    assert_null(
        eval_math_primitive("norm_inv(0.5, 0, 0)", "", &[]),
        "norm_inv(.., sigma=0)",
    );
}

#[test]
fn test_pow_and_sqrt_equivalence_for_positive() {
    // Phase 3I item 2 W6 sanity: pow(x, 0.5) == sqrt(x) for positive x.
    let pow_val = match eval_math_primitive("pow(9, 0.5)", "", &[]) {
        ScalarValue::F64(v) => v,
        other => panic!("expected F64, got {other:?}"),
    };
    let sqrt_val = match eval_math_primitive("sqrt(9)", "", &[]) {
        ScalarValue::F64(v) => v,
        other => panic!("expected F64, got {other:?}"),
    };
    assert!(
        (pow_val - sqrt_val).abs() < 1e-9,
        "pow(9, 0.5)={pow_val} should equal sqrt(9)={sqrt_val}"
    );
}

#[test]
fn test_norm_inv_inverts_norm_cdf() {
    // norm_cdf(norm_inv(p, 0, 1), 0, 1) ≈ p for several p in (0, 1).
    for &p in &[0.1, 0.25, 0.5, 0.75, 0.9, 0.95] {
        let body = format!("norm_cdf(norm_inv({p}, 0, 1), 0, 1)");
        match eval_math_primitive(&body, "", &[]) {
            // Beasley-Springer-Moro central region accuracy is ~1e-4, tail
            // region ~1e-9. norm_cdf approximation is ~7.5e-8. Combined
            // round-trip tolerance: 1e-3 is comfortable.
            ScalarValue::F64(v) => assert!(
                (v - p).abs() < 1e-3,
                "norm_cdf(norm_inv({p}, 0, 1), 0, 1) = {v}; expected ≈ {p}"
            ),
            other => panic!("expected F64, got {other:?}"),
        }
    }
}

#[test]
fn test_math_primitives_propagate_null() {
    // Spend is not written → Null. Every math primitive should propagate.
    for body in [
        "pow(Spend, 2)",
        "sqrt(Spend)",
        "ln(Spend)",
        "log10(Spend)",
        "round(Spend)",
        "floor(Spend)",
        "ceil(Spend)",
        "mod(Spend, 3)",
        "norm_inv(Spend, 0, 1)",
    ] {
        let val = eval_math_primitive(body, r#""Spend""#, &[]);
        assert_null(val, body);
    }
}

// ============================================================================
// Phase 3I — Item 6: ifs() / switch() (compile to nested If)
// ============================================================================

#[test]
fn test_ifs_three_branches_picks_correct() {
    // ifs(Spend > 1000, 0.05, Spend > 100, 0.10, 0.02) — write Spend=500
    // → first cond false, second true → 0.10.
    let body = "ifs(Spend > 1000, 0.05, Spend > 100, 0.10, 0.02)";
    let yaml = simple_model(body, r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        500.0,
    );
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 0.10, "ifs picks second branch");
}

#[test]
fn test_ifs_default_when_no_match() {
    let body = "ifs(0 > 1, 100, 0 > 2, 200, 999)";
    match eval_math_primitive(body, "", &[]) {
        ScalarValue::F64(v) => assert_f64_eq(v, 999.0, "ifs falls through to default"),
        other => panic!("expected F64, got {other:?}"),
    }
}

#[test]
fn test_ifs_even_arg_count_fails_mc1004() {
    // ifs(c1, v1, c2, v2) — missing default → 4 args (even) → MC1008.
    // Per Phase 3E precedent (handoff item 6 W1), arity goes through MC1008.
    let yaml = simple_model("ifs(1 > 0, 100, 0 > 1, 200)", "");
    let result = mc_model::load_str(&yaml, Some("test".into()));
    assert!(result.is_err(), "ifs with even arg count must fail");
    let errs = result.unwrap_err();
    let any_arity = errs.iter().any(|e| {
        let msg = format!("{e:?}");
        msg.contains("MC1008") || msg.contains("odd argument count")
    });
    assert!(
        any_arity,
        "expected MC1008 (arity) error for even ifs arg count, got: {errs:?}"
    );
}

#[test]
fn test_switch_with_period_index_branches() {
    // switch(period_index(), 0, 0.05, 1, 0.10, 0.02) at P1 → period_index=0 → 0.05
    let body = "switch(period_index(), 0, 0.05, 1, 0.10, 0.02)";
    match eval_math_primitive(body, "", &[]) {
        ScalarValue::F64(v) => assert_f64_eq(v, 0.05, "switch period_index=0 → 0.05"),
        other => panic!("expected F64, got {other:?}"),
    }
}

#[test]
fn test_switch_default_when_no_match() {
    // switch(period_index(), 99, 100, 999) — period_index is 0 (P1) so default 999.
    let body = "switch(period_index(), 99, 100, 999)";
    match eval_math_primitive(body, "", &[]) {
        ScalarValue::F64(v) => assert_f64_eq(v, 999.0, "switch falls to default"),
        other => panic!("expected F64, got {other:?}"),
    }
}

// ============================================================================
// Phase 3I — Item 1: is_element(Dim, "Element") narrow numeric form
// ============================================================================

/// Two-Market model so we can prove is_element discriminates between elements.
fn is_element_model(rule_body: &str, deps: &str) -> String {
    format!(
        r#"
model_format_version: 1
metadata:
  name: "IsElementTest"
  description: "test"
  author: "test"
  created: "2026-01-01"
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
      - {{ name: "Houston" }}
      - {{ name: "Dallas" }}
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - {{ name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }}
  - {{ name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "{rule_body}"
    declared_dependencies: [{deps}]
"#
    )
}

#[test]
fn test_is_element_returns_one_at_matching_coord() {
    let yaml = is_element_model(r#"is_element(Market, \"Houston\")"#, "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "Houston"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 1.0, "is_element matches Houston");
}

#[test]
fn test_is_element_returns_zero_elsewhere() {
    let yaml = is_element_model(r#"is_element(Market, \"Houston\")"#, "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "Dallas"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 0.0, "is_element does not match Dallas");
}

#[test]
fn test_is_element_unknown_element_fails_validation_with_mc1022() {
    let yaml = is_element_model(r#"is_element(Market, \"Houston_Typo\")"#, "");
    let result = mc_model::load_str(&yaml, Some("test".into()));
    assert!(result.is_err(), "unknown element must fail validation");
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains("MC1022"));
    assert!(any, "expected MC1022 in errors: {errs:?}");
}

#[test]
fn test_is_element_with_quoted_string_outside_call_fails_with_mc1027() {
    // Phase 3J Item 1 + Amendment §1: string literals are now first-class
    // in expression evaluation. The previous Phase 3I MC1024 catch-all
    // (parser-side reject) is replaced by Phase 3J's type-context check
    // (validator-side). `Spend == "high"` no longer fails at parse — it
    // parses to `Eq(Ref{Spend}, StrLiteral("high"))` and the validator
    // catches the F64-vs-Str mismatch with MC1027.
    let yaml = simple_model(r#"if(Spend == \"high\", 1, 0)"#, r#""Spend""#);
    let result = mc_model::load_str(&yaml, Some("test".into()));
    assert!(result.is_err(), "Str compared with F64 measure must fail");
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains("MC1027"));
    assert!(any, "expected MC1027 in errors: {errs:?}");
}

// ============================================================================
// Phase 3I — Item 5: avg_over / min_over / max_over / wavg_over
// ============================================================================

/// Two markets so the *_over scans have something to aggregate.
fn over_model(rule_body: &str, deps: &str) -> String {
    format!(
        r#"
model_format_version: 1
metadata:
  name: "OverTest"
  description: "test"
  author: "test"
  created: "2026-01-01"
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
      - {{ name: "Houston" }}
      - {{ name: "Dallas" }}
      - {{ name: "Austin" }}
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - {{ name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }}
  - {{ name: "Weight", role: "Input", data_type: "F64", aggregation: "Sum" }}
  - {{ name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "{rule_body}"
    declared_dependencies: [{deps}]
"#
    )
}

fn write_market_inputs(
    cube: &mut mc_core::Cube,
    refs: &ModelRefs,
    p: mc_core::PrincipalId,
    measure: &str,
    values: &[(&str, f64)],
) {
    for (market, value) in values {
        write_f64(
            cube,
            refs,
            p,
            &[
                ("Scenario", "Base"),
                ("Version", "Working"),
                ("Time", "P1"),
                ("Channel", "Web"),
                ("Market", market),
                ("Measure", measure),
            ],
            *value,
        );
    }
}

fn read_houston_result(
    cube: &mut mc_core::Cube,
    refs: &ModelRefs,
    p: mc_core::PrincipalId,
) -> ScalarValue {
    read_value(
        cube,
        refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "Houston"),
            ("Measure", "Result"),
        ],
    )
}

#[test]
fn test_avg_over_basic() {
    let yaml = over_model("avg_over(Spend, Market)", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    write_market_inputs(
        &mut cube,
        &compiled.refs,
        p,
        "Spend",
        &[("Houston", 100.0), ("Dallas", 200.0), ("Austin", 300.0)],
    );
    match read_houston_result(&mut cube, &compiled.refs, p) {
        ScalarValue::F64(v) => assert_f64_eq(v, 200.0, "avg_over of [100, 200, 300]"),
        other => panic!("expected F64, got {other:?}"),
    }
}

#[test]
fn test_min_over_basic() {
    let yaml = over_model("min_over(Spend, Market)", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    write_market_inputs(
        &mut cube,
        &compiled.refs,
        p,
        "Spend",
        &[("Houston", 100.0), ("Dallas", 200.0), ("Austin", 50.0)],
    );
    match read_houston_result(&mut cube, &compiled.refs, p) {
        ScalarValue::F64(v) => assert_f64_eq(v, 50.0, "min_over of [100, 200, 50]"),
        other => panic!("expected F64, got {other:?}"),
    }
}

#[test]
fn test_max_over_basic() {
    let yaml = over_model("max_over(Spend, Market)", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    write_market_inputs(
        &mut cube,
        &compiled.refs,
        p,
        "Spend",
        &[("Houston", 100.0), ("Dallas", 200.0), ("Austin", 50.0)],
    );
    match read_houston_result(&mut cube, &compiled.refs, p) {
        ScalarValue::F64(v) => assert_f64_eq(v, 200.0, "max_over of [100, 200, 50]"),
        other => panic!("expected F64, got {other:?}"),
    }
}

#[test]
fn test_wavg_over_basic() {
    // wavg_over(Spend, Market, Weight): values [100, 200, 300] weighted by [1, 2, 3]
    // = (100 + 400 + 900) / 6 = 233.333...
    let yaml = over_model("wavg_over(Spend, Market, Weight)", r#""Spend", "Weight""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    write_market_inputs(
        &mut cube,
        &compiled.refs,
        p,
        "Spend",
        &[("Houston", 100.0), ("Dallas", 200.0), ("Austin", 300.0)],
    );
    write_market_inputs(
        &mut cube,
        &compiled.refs,
        p,
        "Weight",
        &[("Houston", 1.0), ("Dallas", 2.0), ("Austin", 3.0)],
    );
    match read_houston_result(&mut cube, &compiled.refs, p) {
        ScalarValue::F64(v) => assert_f64_eq(v, 1400.0 / 6.0, "weighted avg"),
        other => panic!("expected F64, got {other:?}"),
    }
}

#[test]
fn test_avg_over_skips_nulls() {
    // avg_over of [100, Null, 300] — skip the null, divide by 2 → 200.
    let yaml = over_model("avg_over(Spend, Market)", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    // Don't write Dallas — it stays Null.
    write_market_inputs(
        &mut cube,
        &compiled.refs,
        p,
        "Spend",
        &[("Houston", 100.0), ("Austin", 300.0)],
    );
    match read_houston_result(&mut cube, &compiled.refs, p) {
        ScalarValue::F64(v) => assert_f64_eq(v, 200.0, "avg_over skips null"),
        other => panic!("expected F64, got {other:?}"),
    }
}

#[test]
fn test_min_over_with_all_nulls_returns_null() {
    let yaml = over_model("min_over(Spend, Market)", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    // No writes — all markets are Null.
    let val = read_houston_result(&mut cube, &compiled.refs, p);
    assert_null(val, "min_over of all-nulls");
}

#[test]
fn test_wavg_over_zero_weights_returns_null() {
    let yaml = over_model("wavg_over(Spend, Market, Weight)", r#""Spend", "Weight""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    write_market_inputs(
        &mut cube,
        &compiled.refs,
        p,
        "Spend",
        &[("Houston", 100.0), ("Dallas", 200.0), ("Austin", 300.0)],
    );
    write_market_inputs(
        &mut cube,
        &compiled.refs,
        p,
        "Weight",
        &[("Houston", 0.0), ("Dallas", 0.0), ("Austin", 0.0)],
    );
    let val = read_houston_result(&mut cube, &compiled.refs, p);
    assert_null(val, "wavg_over zero weights");
}

#[test]
fn test_avg_over_equals_wavg_over_with_unit_weights() {
    // Sanity: avg_over(Spend, Market) == wavg_over(Spend, Market, Weight)
    // when all weights are 1.0.
    let yaml1 = over_model("avg_over(Spend, Market)", r#""Spend""#);
    let yaml2 = over_model("wavg_over(Spend, Market, Weight)", r#""Spend", "Weight""#);
    let compiled1 = build_test_cube(&yaml1);
    let compiled2 = build_test_cube(&yaml2);
    let mut cube1 = compiled1.cube;
    let mut cube2 = compiled2.cube;
    write_market_inputs(
        &mut cube1,
        &compiled1.refs,
        compiled1.root_principal,
        "Spend",
        &[("Houston", 100.0), ("Dallas", 200.0), ("Austin", 300.0)],
    );
    write_market_inputs(
        &mut cube2,
        &compiled2.refs,
        compiled2.root_principal,
        "Spend",
        &[("Houston", 100.0), ("Dallas", 200.0), ("Austin", 300.0)],
    );
    write_market_inputs(
        &mut cube2,
        &compiled2.refs,
        compiled2.root_principal,
        "Weight",
        &[("Houston", 1.0), ("Dallas", 1.0), ("Austin", 1.0)],
    );
    let v1 = read_houston_result(&mut cube1, &compiled1.refs, compiled1.root_principal);
    let v2 = read_houston_result(&mut cube2, &compiled2.refs, compiled2.root_principal);
    match (v1, v2) {
        (ScalarValue::F64(a), ScalarValue::F64(b)) => {
            assert!(
                (a - b).abs() < 1e-9,
                "avg_over={a} should equal wavg_over with unit weights={b}"
            );
        }
        other => panic!("expected F64s, got {other:?}"),
    }
}

// ============================================================================
// Phase 10A (ADR-0033) — evaluation metrics primitives
//
// std_over / var_over / count_over plus wilson_ci_lower / wilson_ci_upper.
// The _over variants mirror the avg_over / min_over / max_over tests above
// — same (measure, dim) bare-identifier shape (Amendment 1), same skip-
// Null semantics. count_over uses an explicit per-leaf eval per Amendment 2.
// Wilson is tested at the parse + integration layer here; closed-form
// numeric correctness against statsmodels lives in
// `crates/mc-core/tests/metrics.rs`.
// ============================================================================

#[test]
fn test_std_over_basic() {
    // Per ADR-0033 Amendment 3: sample variance (ddof=1).
    // std_over of [100, 200, 300] = sample std with n=3 = 100.0.
    let yaml = over_model("std_over(Spend, Market)", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    write_market_inputs(
        &mut cube,
        &compiled.refs,
        p,
        "Spend",
        &[("Houston", 100.0), ("Dallas", 200.0), ("Austin", 300.0)],
    );
    match read_houston_result(&mut cube, &compiled.refs, p) {
        ScalarValue::F64(v) => assert_f64_eq(v, 100.0, "std_over of [100,200,300], ddof=1"),
        other => panic!("expected F64, got {other:?}"),
    }
}

#[test]
fn test_var_over_basic() {
    // var_over of [100, 200, 300] = 10000.0 (ddof=1).
    let yaml = over_model("var_over(Spend, Market)", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    write_market_inputs(
        &mut cube,
        &compiled.refs,
        p,
        "Spend",
        &[("Houston", 100.0), ("Dallas", 200.0), ("Austin", 300.0)],
    );
    match read_houston_result(&mut cube, &compiled.refs, p) {
        ScalarValue::F64(v) => assert_f64_eq(v, 10000.0, "var_over of [100,200,300], ddof=1"),
        other => panic!("expected F64, got {other:?}"),
    }
}

#[test]
fn test_std_over_n_less_than_2_returns_null() {
    // Per Decision 3 + Amendment 3: sample variance is undefined for
    // n<2; std_over / var_over return Null. With only Houston written
    // (n=1 non-Null leaf), the result is Null.
    let yaml = over_model("std_over(Spend, Market)", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    write_market_inputs(&mut cube, &compiled.refs, p, "Spend", &[("Houston", 42.0)]);
    let val = read_houston_result(&mut cube, &compiled.refs, p);
    assert_null(val, "std_over of single-value sample returns Null");
}

#[test]
fn test_count_over_evaluates_input_leaves() {
    // Per Amendment 2: count_over evaluates the measure at every leaf
    // via the same dispatch as sum_over (it does NOT count cells from
    // the store). For an Input measure, eval == store-read, so a
    // 2-of-3 write gives count=2.
    let yaml = over_model("count_over(Spend, Market)", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    write_market_inputs(
        &mut cube,
        &compiled.refs,
        p,
        "Spend",
        &[("Houston", 100.0), ("Austin", 300.0)],
    );
    match read_houston_result(&mut cube, &compiled.refs, p) {
        ScalarValue::F64(v) => assert_f64_eq(v, 2.0, "count_over of 2-of-3 non-Null leaves"),
        other => panic!("expected F64, got {other:?}"),
    }
}

#[test]
fn test_count_over_empty_scope_returns_zero() {
    // Per Decision 3: count_over returns 0.0 (NOT Null) for an empty
    // scope — zero is information. Distinct from std_over / var_over,
    // which return Null because the statistic is undefined.
    let yaml = over_model("count_over(Spend, Market)", r#""Spend""#);
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    // No writes — every Market leaf reads as Null.
    match read_houston_result(&mut cube, &compiled.refs, p) {
        ScalarValue::F64(v) => assert_f64_eq(v, 0.0, "count_over of all-Null returns 0.0"),
        other => panic!("expected F64(0.0), got {other:?}"),
    }
}

#[test]
fn test_count_over_evaluates_derived_measure() {
    // Per Amendment 2 (the load-bearing integration test for the
    // "evaluates measure at every leaf" semantic): count_over with a
    // derived measure must trigger the per-leaf rule eval and count
    // the non-Null results, NOT the store's pre-materialized cells.
    //
    // Setup: Spend is Input (written sparsely — 2 of 3 Market leaves),
    // IsPresent is Derived `if(Spend > 0, 1.0, 0.0)`, and Result is
    // `count_over(IsPresent, Market)`.
    //
    // IsPresent is NEVER written to the store (it's Derived). If
    // count_over were tallying store entries, the result would be 0.
    // It returns 3 instead: at every Market leaf the IsPresent rule
    // evaluates and returns a non-Null F64 (1.0 where Spend is
    // written, 0.0 where Spend is Null because `if`'s else branch
    // fires for the falsy Null-comparison result). 3 ≠ 0 unambiguously
    // proves count_over invokes per-leaf evaluation rather than
    // reading store materialization.
    let yaml = r#"
model_format_version: 1
metadata:
  name: "CountOverDerived"
  description: "Amendment 2 integration"
  author: "test"
  created: "2026-05-27"
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
      - { name: "Houston" }
      - { name: "Dallas" }
      - { name: "Austin" }
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - { name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "IsPresent", role: "Derived", data_type: "F64", aggregation: "Sum" }
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
rules:
  - name: "rule_is_present"
    target_measure: "IsPresent"
    scope: "AllLeaves"
    body: "if(Spend > 0, 1.0, 0.0)"
    declared_dependencies: ["Spend"]
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "count_over(IsPresent, Market)"
    declared_dependencies: ["IsPresent"]
"#;
    let compiled = build_test_cube(yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    write_market_inputs(
        &mut cube,
        &compiled.refs,
        p,
        "Spend",
        &[("Houston", 100.0), ("Austin", 300.0)],
    );
    match read_houston_result(&mut cube, &compiled.refs, p) {
        ScalarValue::F64(v) => assert_f64_eq(
            v,
            3.0,
            "count_over must evaluate IsPresent per leaf (Amendment 2): \
             expected 3 non-Null evaluations across Market (IsPresent is \
             Derived, never stored — 3 ≠ 0 proves per-leaf eval)",
        ),
        other => panic!("expected F64(3.0), got {other:?}"),
    }
}

/// Read `Result` at the single (Base, Working, P1, Web, US) coord that
/// `simple_model` produces. Mirrors the existing pattern used by the
/// Phase 3E if/comparison tests above.
fn read_simple_result(
    cube: &mut mc_core::Cube,
    refs: &ModelRefs,
    p: mc_core::PrincipalId,
) -> ScalarValue {
    read_value(
        cube,
        refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    )
}

#[test]
fn test_wilson_ci_lower_basic() {
    // Wilson takes arbitrary numeric expressions (not bare measures).
    // wilson_ci_lower(0.5, 100) = 0.403831530 per metrics.rs fixtures.
    let yaml = simple_model("wilson_ci_lower(0.5, 100)", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    match read_simple_result(&mut cube, &compiled.refs, p) {
        ScalarValue::F64(v) => {
            assert!(
                (v - 0.403831530).abs() < 1e-6,
                "wilson_ci_lower(0.5, 100) = 0.403831530, got {v}"
            );
        }
        other => panic!("expected F64, got {other:?}"),
    }
}

#[test]
fn test_wilson_ci_upper_basic() {
    // wilson_ci_upper(0.5, 100) = 0.596168470.
    let yaml = simple_model("wilson_ci_upper(0.5, 100)", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    match read_simple_result(&mut cube, &compiled.refs, p) {
        ScalarValue::F64(v) => {
            assert!(
                (v - 0.596168470).abs() < 1e-6,
                "wilson_ci_upper(0.5, 100) = 0.596168470, got {v}"
            );
        }
        other => panic!("expected F64, got {other:?}"),
    }
}

#[test]
fn test_wilson_ci_invalid_returns_null_via_eval() {
    // Per Decision 3 + ADR-0031 Amendment 2: invalid inputs (n=0, p>1,
    // negative p, NaN) return Null. Tested at the eval layer here;
    // the compute helper is exercised in metrics.rs.
    let yaml = simple_model("wilson_ci_lower(0.5, 0)", "");
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    let val = read_simple_result(&mut cube, &compiled.refs, p);
    assert_null(val, "wilson with n=0 returns Null");
}

// ----------------------------------------------------------------------------
// Parse-time tests — wrong arg count → MC1008 per Amendment 6
// ----------------------------------------------------------------------------

#[test]
fn test_std_over_wrong_arity_mc1008() {
    // std_over takes exactly 2 args; passing 1 → MC1008 via the shared
    // wrong_arg_count helper. Same MC code as the rest of the _over
    // family (Amendment 6). The MC code is attached via the
    // ParseError::code() method, not interpolated into the Debug
    // message — match the variant name + the per-fn-name disambiguator
    // in the message text instead.
    let yaml = over_model("std_over(Spend)", r#""Spend""#);
    let errs = mc_model::load_str(&yaml, Some("test".into())).unwrap_err();
    let any = errs.iter().any(|e| {
        let s = format!("{e:?}");
        s.contains("FormulaWrongArgCount") && s.contains("std_over")
    });
    assert!(
        any,
        "expected MC1008 (FormulaWrongArgCount) mentioning std_over: {errs:?}"
    );
}

#[test]
fn test_wilson_ci_lower_wrong_arity_mc1008() {
    // wilson_ci_lower takes exactly 2 args; 3 → MC1008. Per Amendment 6
    // the function name disambiguates in the message text.
    let yaml = simple_model("wilson_ci_lower(0.5, 100, 0.95)", "");
    let errs = mc_model::load_str(&yaml, Some("test".into())).unwrap_err();
    let any = errs.iter().any(|e| {
        let s = format!("{e:?}");
        s.contains("FormulaWrongArgCount") && s.contains("wilson_ci_lower")
    });
    assert!(
        any,
        "expected MC1008 (FormulaWrongArgCount) mentioning wilson_ci_lower: {errs:?}"
    );
}

#[test]
fn test_count_over_parses_with_bare_identifiers_amendment1() {
    // Per Amendment 1: count_over accepts BARE measure + dim
    // identifiers only — no inline expressions. This test confirms
    // the standard 2-bare-identifier form parses cleanly; the inline-
    // expression case fails at parse (the parser tries to consume the
    // identifier and rejects `if(...)` as not an identifier).
    let yaml = over_model("count_over(Spend, Market)", r#""Spend""#);
    let result = mc_model::load_str(&yaml, Some("test".into()));
    assert!(
        result.is_ok(),
        "bare-identifier count_over should parse: {:?}",
        result.err()
    );
}

// ============================================================================
// Phase 3I — Item 3: Multi-key lookup_tables
// ============================================================================

fn lookup_yaml(lookup_block: &str, rule_body: &str, deps: &str) -> String {
    format!(
        r#"
model_format_version: 1
metadata:
  name: "LookupTest"
  description: "test"
  author: "test"
  created: "2026-01-01"
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
      - {{ name: "Jan_2026" }}
      - {{ name: "Feb_2026" }}
  - name: "Channel"
    kind: "Standard"
    elements:
      - {{ name: "Web" }}
  - name: "Market"
    kind: "Standard"
    elements:
      - {{ name: "Houston" }}
      - {{ name: "Dallas" }}
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - {{ name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }}
  - {{ name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }}
{lookup_block}rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "{rule_body}"
    declared_dependencies: [{deps}]
"#
    )
}

#[test]
fn test_lookup_table_single_key_backward_compat() {
    // Phase 3G shape (key_dimension: <single>) must continue to work.
    let lookup_block = r#"lookup_tables:
  - name: "tax_rate"
    key_dimension: "Market"
    values:
      Houston: 0.08
      Dallas: 0.06
"#;
    let yaml = lookup_yaml(
        lookup_block,
        r#"lookup(\"tax_rate\", Market) * Spend"#,
        r#""Spend""#,
    );
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "Jan_2026"),
            ("Channel", "Web"),
            ("Market", "Houston"),
            ("Measure", "Spend"),
        ],
        1000.0,
    );
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "Jan_2026"),
            ("Channel", "Web"),
            ("Market", "Houston"),
            ("Measure", "Result"),
        ],
    );
    assert_f64_eq(val, 80.0, "single-key lookup * Spend");
}

#[test]
fn test_lookup_table_multi_key_two_dims() {
    // Phase 3I item 3: multi-key. key_dimensions: [Market, Time].
    let lookup_block = r#"lookup_tables:
  - name: "seasonality"
    key_dimensions: ["Market", "Time"]
    values:
      "Houston|Jan_2026": 1.05
      "Houston|Feb_2026": 1.12
      "Dallas|Jan_2026": 0.95
      "Dallas|Feb_2026": 0.97
"#;
    let yaml = lookup_yaml(
        lookup_block,
        r#"lookup(\"seasonality\", Market, Time) * Spend"#,
        r#""Spend""#,
    );
    let compiled = build_test_cube(&yaml);
    let mut cube = compiled.cube;
    let p = compiled.root_principal;
    write_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "Feb_2026"),
            ("Channel", "Web"),
            ("Market", "Houston"),
            ("Measure", "Spend"),
        ],
        1000.0,
    );
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        p,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "Feb_2026"),
            ("Channel", "Web"),
            ("Market", "Houston"),
            ("Measure", "Result"),
        ],
    );
    // Houston|Feb_2026 = 1.12 * 1000 = 1120
    assert_f64_eq(val, 1120.0, "multi-key Houston|Feb_2026 lookup * Spend");
}

#[test]
fn test_lookup_table_both_key_fields_set_fails_mc2050() {
    let lookup_block = r#"lookup_tables:
  - name: "bad_table"
    key_dimension: "Market"
    key_dimensions: ["Market", "Time"]
    values:
      Houston: 1.0
"#;
    let yaml = lookup_yaml(
        lookup_block,
        r#"lookup(\"bad_table\", Market) * Spend"#,
        r#""Spend""#,
    );
    let result = mc_model::load_str(&yaml, Some("test".into()));
    assert!(result.is_err(), "both key fields set must fail");
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains("MC2050"));
    assert!(any, "expected MC2050 in errors: {errs:?}");
}

#[test]
fn test_lookup_table_pipe_in_element_name_fails_mc2051() {
    // Build a model where Market has an element name containing '|'.
    let yaml = r#"
model_format_version: 1
metadata:
  name: "PipeTest"
  description: "test"
  author: "test"
  created: "2026-01-01"
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
      - { name: "Has|Pipe" }
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - { name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
lookup_tables:
  - name: "bad"
    key_dimensions: ["Market", "Time"]
    values:
      "Has|Pipe|P1": 1.0
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "lookup(\"bad\", Market, Time) * Spend"
    declared_dependencies: ["Spend"]
"#;
    let result = mc_model::load_str(yaml, Some("test".into()));
    assert!(result.is_err(), "pipe in element name must fail");
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains("MC2051"));
    assert!(any, "expected MC2051 in errors: {errs:?}");
}

#[test]
fn test_lookup_table_key_arity_mismatch_fails_mc2052() {
    // key_dimensions has 2 entries, but a value key only has 1 part.
    let lookup_block = r#"lookup_tables:
  - name: "seasonality"
    key_dimensions: ["Market", "Time"]
    values:
      "Houston": 1.05
"#;
    let yaml = lookup_yaml(
        lookup_block,
        r#"lookup(\"seasonality\", Market, Time) * Spend"#,
        r#""Spend""#,
    );
    let result = mc_model::load_str(&yaml, Some("test".into()));
    assert!(result.is_err(), "key arity mismatch must fail");
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains("MC2052"));
    assert!(any, "expected MC2052 in errors: {errs:?}");
}

// ============================================================================
// Phase 3I — Item 4: predict() arity validation (MC2057, not MC2053 — see audit §G)
// ============================================================================

fn predict_arity_yaml(predict_call: &str, deps: &str, n_coeffs: usize) -> String {
    use std::fmt::Write;
    // Build a fitted_models block with n_coeffs coefficients (Feature1..N)
    let mut coeffs = String::new();
    for i in 1..=n_coeffs {
        let _ = writeln!(
            coeffs,
            "      - {{ feature: \"Feature{i}\", weight: {i}.0 }}"
        );
    }
    let mut measures = String::new();
    for i in 1..=n_coeffs {
        let _ = writeln!(
            measures,
            "  - {{ name: \"Feature{i}\", role: \"Input\", data_type: \"F64\", aggregation: \"Sum\" }}"
        );
    }
    format!(
        r#"
model_format_version: 1
metadata:
  name: "PredictArityTest"
  description: "test"
  author: "test"
  created: "2026-01-01"
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
{measures}  - {{ name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }}
fitted_models:
  - name: "model_a"
    method: "linear"
    intercept: 0.0
    coefficients:
{coeffs}rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "{predict_call}"
    declared_dependencies: [{deps}]
"#
    )
}

#[test]
fn test_predict_too_few_features_fails_mc2057() {
    // Model has 3 coefficients, predict() has 2 features → MC2057.
    // (Handoff said MC2053; collision with Phase 3H — see audit §G.)
    let yaml = predict_arity_yaml(
        r#"predict(\"model_a\", Feature1, Feature2)"#,
        r#""Feature1", "Feature2", "Feature3""#,
        3,
    );
    let result = mc_model::load_str(&yaml, Some("test".into()));
    assert!(result.is_err(), "predict with too few features must fail");
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains("MC2057"));
    assert!(any, "expected MC2057 in errors: {errs:?}");
}

#[test]
fn test_predict_too_many_features_fails_mc2057() {
    let yaml = predict_arity_yaml(
        r#"predict(\"model_a\", Feature1, Feature2, Feature3)"#,
        r#""Feature1", "Feature2", "Feature3""#,
        2, // model has 2 coeffs but call passes 3 features
    );
    let result = mc_model::load_str(&yaml, Some("test".into()));
    assert!(result.is_err(), "predict with too many features must fail");
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains("MC2057"));
    assert!(any, "expected MC2057 in errors: {errs:?}");
}

#[test]
fn test_predict_correct_arity_validates_clean() {
    let yaml = predict_arity_yaml(
        r#"predict(\"model_a\", Feature1, Feature2)"#,
        r#""Feature1", "Feature2""#,
        2,
    );
    let result = mc_model::load_str(&yaml, Some("test".into()));
    assert!(
        result.is_ok(),
        "predict with matching arity must validate: {:?}",
        result.err()
    );
}

#[test]
fn test_ifs_compiles_to_nested_if() {
    // Snapshot: parsed AST shape matches nested If — verifies item 6 W3.
    use mc_model::parse_expression;
    use mc_model::ParsedRuleBody;
    let body = "ifs(1 > 0, 100, 1 > 1, 200, 999)";
    let parsed = parse_expression(body).expect("parse ifs");
    // Top-level should be If (1 > 0, 100, If(1 > 1, 200, 999))
    let ParsedRuleBody::If(outer) = parsed else {
        panic!("expected top-level If, got {parsed:?}");
    };
    let ParsedRuleBody::If(inner) = *outer.else_branch else {
        panic!("expected nested If in else, got {:?}", outer.else_branch);
    };
    let ParsedRuleBody::Const(default) = *inner.else_branch else {
        panic!("expected Const in inner else, got {:?}", inner.else_branch);
    };
    match default.value {
        mc_model::ParsedScalar::Float(v) => assert!((v - 999.0).abs() < 1e-9),
        other => panic!("expected Float default, got {other:?}"),
    }
}
