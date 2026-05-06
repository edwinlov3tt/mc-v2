//! Phase 3J Item 3: `parameters:` block.
//!
//! Named scalar `f64` constants referenced via `param(name)` in formulas.
//! v1 supports only `f64` values (ADR-0016 Decision 6 + Amendment §2);
//! computed parameters (`body:` field) and scoped parameters
//! (per-Scenario / per-Market) are deferred to Phase 3J.1.

use std::collections::BTreeMap;

use mc_core::{CellCoordinate, ScalarValue};
use mc_model::{load_str, CompiledCube, ModelRefs};

fn coord(refs: &ModelRefs, slots: &[(&str, &str)]) -> CellCoordinate {
    let map: BTreeMap<String, String> = slots
        .iter()
        .map(|(d, e)| (d.to_string(), e.to_string()))
        .collect();
    refs.coord_from_names(&map)
        .unwrap_or_else(|| panic!("coord_from_names failed for {slots:?}"))
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

const BASE: &str = r#"
model_format_version: 1
metadata:
  name: "P3JI3"
  description: "t"
  author: "t"
  created: "2026-01-01"
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    elements: [{ name: "Base", scenario_meta: "Default" }]
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

/// Test 1: `parameters:` block loads cleanly from YAML and
/// `param(name)` evaluates to the declared value.
#[test]
fn test_parameters_block_loads_from_yaml() {
    let yaml = format!(
        r#"{BASE}
parameters:
  - name: "anchor"
    value: 1234.56
    description: "Q1 anchor revenue"
measures:
  - {{ name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: 'param(anchor)'
    declared_dependencies: []
"#
    );
    let compiled = build(&yaml);
    assert!(
        compiled
            .cube
            .reference_data
            .parameters
            .contains_key("anchor"),
        "Cube.reference_data.parameters must include the declared param"
    );
}

/// Test 2: `param(name)` returns the declared value at eval time.
#[test]
fn test_param_function_returns_value_in_formula() {
    let yaml = format!(
        r#"{BASE}
parameters:
  - name: "anchor"
    value: 1234.56
measures:
  - {{ name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: 'param(anchor) * 2'
    declared_dependencies: []
"#
    );
    let compiled = build(&yaml);
    let mut cube = compiled.cube;
    let v = read_f64(
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
    assert!(
        (v - 2469.12).abs() < 1e-9,
        "param(anchor) * 2 must equal 2469.12, got {v}"
    );
}

/// Test 3: `param(unknown)` fires MC2062.
#[test]
fn test_param_unknown_fails_mc2062() {
    let yaml = format!(
        r#"{BASE}
parameters:
  - name: "anchor"
    value: 1.0
measures:
  - {{ name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: 'param(missing) * 2'
    declared_dependencies: []
"#
    );
    let result = mc_model::load_str(&yaml, Some("test".into()));
    assert!(result.is_err(), "param(missing) must fail validation");
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains("MC2062"));
    assert!(any, "expected MC2062 in errors: {errs:?}");
}

/// Test 4: parameter name collides with a measure name → MC2060.
#[test]
fn test_parameter_name_collides_with_measure_fails_mc2060() {
    let yaml = format!(
        r#"{BASE}
parameters:
  - name: "Spend"
    value: 1.0
measures:
  - {{ name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }}
  - {{ name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: 'Spend'
    declared_dependencies: ["Spend"]
"#
    );
    let result = mc_model::load_str(&yaml, Some("test".into()));
    assert!(result.is_err(), "param/measure collision must fail");
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains("MC2060"));
    assert!(any, "expected MC2060 in errors: {errs:?}");
}

/// Test 5: parameter name collides with a dim element name → MC2061.
#[test]
fn test_parameter_name_collides_with_element_fails_mc2061() {
    let yaml = format!(
        r#"{BASE}
parameters:
  - name: "Web"
    value: 1.0
measures:
  - {{ name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: 'param(Web)'
    declared_dependencies: []
"#
    );
    let result = mc_model::load_str(&yaml, Some("test".into()));
    assert!(result.is_err(), "param/element collision must fail");
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains("MC2061"));
    assert!(any, "expected MC2061 in errors: {errs:?}");
}
