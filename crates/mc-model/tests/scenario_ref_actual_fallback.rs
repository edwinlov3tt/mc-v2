//! Phase 3J Item 6: `scenario_ref` + `actual_ref(measure, fallback)`.
//!
//! ADR-0016 Decision 8 + Amendment §3 (lazy fallback can contain
//! cross-coord functions, relaxing MC1013) + Amendment §12 (inherits
//! cross-coord dep-graph performance debt).

use std::collections::BTreeMap;

use mc_core::{CellCoordinate, ScalarValue, WriteIntent, WritebackRequest};
use mc_model::{load_str, CompiledCube, ModelRefs};

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

fn read_f64(
    cube: &mut mc_core::Cube,
    refs: &ModelRefs,
    principal: mc_core::PrincipalId,
    slots: &[(&str, &str)],
) -> f64 {
    match read_value(cube, refs, principal, slots) {
        ScalarValue::F64(v) => v,
        other => panic!("expected F64, got {other:?}"),
    }
}

fn build(yaml: &str) -> CompiledCube {
    load_str(yaml, Some("test".into())).unwrap_or_else(|errs| {
        for e in &errs {
            eprintln!("  error: {e}");
        }
        panic!("model failed to load");
    })
}

/// Two-scenario, single-time, single-measure model. `Plan` is the
/// non-default scenario; `Actual` is the actuals_element. Used to
/// exercise scenario_ref + actual_ref(measure, fallback).
const TWO_SCENARIO: &str = r#"
model_format_version: 1
metadata:
  name: "P3JI6"
  description: "t"
  author: "t"
  created: "2026-01-01"
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    actuals_element: "Actual"
    elements:
      - { name: "Actual", scenario_meta: "Default" }
      - { name: "Plan", scenario_meta: "NonDefault" }
  - name: "Version"
    kind: "Version"
    elements: [{ name: "Working", version_state: "Draft" }]
  - name: "Time"
    kind: "Time"
    elements: [{ name: "P1" }]
  - name: "Channel"
    kind: "Standard"
    elements: [{ name: "Web" }]
  - name: "Market"
    kind: "Standard"
    elements: [{ name: "US" }]
  - name: "Measure"
    kind: "Measure"
    elements: []
"#;

/// Test 1: scenario_ref(Spend, "Plan") at any coord reads Plan-scenario Spend.
#[test]
fn test_scenario_ref_reads_from_named_scenario() {
    let yaml = format!(
        r#"{TWO_SCENARIO}
measures:
  - {{ name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }}
  - {{ name: "PlanSpend", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_plan_spend"
    target_measure: "PlanSpend"
    scope: "AllLeaves"
    body: 'scenario_ref(Spend, "Plan")'
    declared_dependencies: ["Spend"]
"#
    );
    let compiled = build(&yaml);
    let mut cube = compiled.cube;
    write_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Plan"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        100.0,
    );
    // Read PlanSpend at the Actual scenario — it should pull Plan's Spend.
    let v = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Actual"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "PlanSpend"),
        ],
    );
    assert!(
        (v - 100.0).abs() < 1e-9,
        "PlanSpend at Actual coord should read Plan's Spend (100), got {v}"
    );
}

/// Test 2: scenario_ref against an unknown scenario element → MC2065.
#[test]
fn test_scenario_ref_unknown_scenario_fails_mc2065() {
    let yaml = format!(
        r#"{TWO_SCENARIO}
measures:
  - {{ name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }}
  - {{ name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "r"
    target_measure: "Result"
    scope: "AllLeaves"
    body: 'scenario_ref(Spend, "Bogus")'
    declared_dependencies: ["Spend"]
"#
    );
    let result = mc_model::load_str(&yaml, Some("test".into()));
    assert!(result.is_err(), "scenario_ref unknown name must fail");
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains("MC2065"));
    assert!(any, "expected MC2065 in errors: {errs:?}");
}

/// Test 3 (Amendment §3 lazy eval): actual_ref with fallback uses the
/// fallback when actual is Null.
#[test]
fn test_actual_ref_with_fallback_uses_fallback_when_actual_null() {
    let yaml = format!(
        r#"{TWO_SCENARIO}
measures:
  - {{ name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }}
  - {{ name: "ActualOrFallback", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_a_or_f"
    target_measure: "ActualOrFallback"
    scope: "AllLeaves"
    body: 'actual_ref(Spend, scenario_ref(Spend, "Plan"))'
    declared_dependencies: ["Spend"]
"#
    );
    let compiled = build(&yaml);
    let mut cube = compiled.cube;
    // Actual is unwritten (Null); Plan has 50.0. Fallback should fire.
    write_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Plan"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        50.0,
    );
    let v = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Plan"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "ActualOrFallback"),
        ],
    );
    assert!(
        (v - 50.0).abs() < 1e-9,
        "fallback (Plan Spend = 50) must fire when Actual is Null, got {v}"
    );
}

/// Test 4: actual_ref(measure, fallback) returns Actual when present
/// (fallback NOT evaluated).
#[test]
fn test_actual_ref_with_fallback_uses_actual_when_present() {
    let yaml = format!(
        r#"{TWO_SCENARIO}
measures:
  - {{ name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }}
  - {{ name: "ActualOrFallback", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_a_or_f"
    target_measure: "ActualOrFallback"
    scope: "AllLeaves"
    body: 'actual_ref(Spend, scenario_ref(Spend, "Plan"))'
    declared_dependencies: ["Spend"]
"#
    );
    let compiled = build(&yaml);
    let mut cube = compiled.cube;
    // Both Actual and Plan written; Actual should win.
    write_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Actual"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        99.0,
    );
    write_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Plan"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        50.0,
    );
    let v = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Plan"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "ActualOrFallback"),
        ],
    );
    assert!(
        (v - 99.0).abs() < 1e-9,
        "Actual (99) wins over Plan fallback when present, got {v}"
    );
}

/// Test 5 (Amendment §3 canonical pattern): `actual_ref(m, scenario_ref(m, "X"))`
/// validates and evaluates correctly. This is the load-bearing test for
/// the MC1013 nesting relaxation.
#[test]
fn test_actual_ref_fallback_with_scenario_ref_works() {
    // Same as Test 3 — but here we explicitly assert that NO MC1013
    // error fires on the canonical pattern.
    let yaml = format!(
        r#"{TWO_SCENARIO}
measures:
  - {{ name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }}
  - {{ name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "r"
    target_measure: "Result"
    scope: "AllLeaves"
    body: 'actual_ref(Spend, scenario_ref(Spend, "Plan"))'
    declared_dependencies: ["Spend"]
"#
    );
    let result = mc_model::load_str(&yaml, Some("test".into()));
    assert!(
        result.is_ok(),
        "canonical pattern must validate cleanly: {:?}",
        result.err()
    );
}

/// Test 6: actual_ref fallback type mismatch (Str fallback for F64
/// measure) → MC2066.
#[test]
fn test_actual_ref_fallback_type_mismatch_fails_mc2066() {
    let yaml = format!(
        r#"{TWO_SCENARIO}
measures:
  - {{ name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }}
  - {{ name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "r"
    target_measure: "Result"
    scope: "AllLeaves"
    body: 'actual_ref(Spend, "rogue_string")'
    declared_dependencies: ["Spend"]
"#
    );
    let result = mc_model::load_str(&yaml, Some("test".into()));
    assert!(result.is_err(), "Str fallback for F64 measure must fail");
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains("MC2066"));
    assert!(any, "expected MC2066 in errors: {errs:?}");
}

/// Test 7: Backward compat — actual_ref(measure) 1-arg form unchanged.
#[test]
fn test_actual_ref_one_arg_form_unchanged() {
    let yaml = format!(
        r#"{TWO_SCENARIO}
measures:
  - {{ name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }}
  - {{ name: "ActualSpend", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_actual"
    target_measure: "ActualSpend"
    scope: "AllLeaves"
    body: 'actual_ref(Spend)'
    declared_dependencies: ["Spend"]
"#
    );
    let compiled = build(&yaml);
    let mut cube = compiled.cube;
    write_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Actual"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "Spend"),
        ],
        77.0,
    );
    // Read at Plan scenario; actual_ref pulls from Actual.
    let v = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Plan"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Web"),
            ("Market", "US"),
            ("Measure", "ActualSpend"),
        ],
    );
    assert!(
        (v - 77.0).abs() < 1e-9,
        "actual_ref(Spend) (1-arg form) must still pull Actual (77.0), got {v}"
    );
}
