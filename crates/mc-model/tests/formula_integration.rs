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
