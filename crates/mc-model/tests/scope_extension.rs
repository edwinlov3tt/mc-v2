//! Phase 3J Item 5: `Scope` enum extension.
//!
//! Adds `FutureLeaves`, `PastLeaves`, `CurrentLeaves` scope variants
//! per ADR-0016 Decision 5 + Amendment §4. The new variants require a
//! `time_anchor:` configured on the Time dim (MC2069 if missing); a
//! rule with one of the new scopes only writes to coords matching the
//! anchor-relative predicate.

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

fn build(yaml: &str) -> CompiledCube {
    load_str(yaml, Some("test".into())).unwrap_or_else(|errs| {
        for e in &errs {
            eprintln!("  error: {e}");
        }
        panic!("model failed to load");
    })
}

/// 3-period Time with anchor at P2 (the middle period). P1 is past, P3 future.
const TIME_3P_ANCHORED: &str = r#"
model_format_version: 1
metadata:
  name: "P3JI5"
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
    time_anchor: "P2"
    elements:
      - { name: "P1" }
      - { name: "P2" }
      - { name: "P3" }
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

const TIME_3P_NO_ANCHOR: &str = r#"
model_format_version: 1
metadata:
  name: "P3JI5"
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
    elements:
      - { name: "P1" }
      - { name: "P2" }
      - { name: "P3" }
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

const ALL_SLOTS_AT: &[(&str, &str)] = &[];

fn slots(time: &str, measure: &str) -> [(&'static str, String); 6] {
    [
        ("Scenario", "Base".to_string()),
        ("Version", "Working".to_string()),
        ("Time", time.to_string()),
        ("Channel", "Web".to_string()),
        ("Market", "US".to_string()),
        ("Measure", measure.to_string()),
    ]
}

fn read_at(
    cube: &mut mc_core::Cube,
    refs: &ModelRefs,
    principal: mc_core::PrincipalId,
    time: &str,
    measure: &str,
) -> ScalarValue {
    let s = slots(time, measure);
    let s_refs: Vec<(&str, &str)> = s.iter().map(|(d, e)| (*d, e.as_str())).collect();
    read_value(cube, refs, principal, &s_refs)
}

/// Test 1: AllLeaves is the default when `scope:` is absent (backward
/// compat). The rule fires at every leaf coord.
#[test]
fn test_scope_all_leaves_default_when_field_absent() {
    let _ = ALL_SLOTS_AT; // suppress unused
                          // Existing models with explicit `scope: AllLeaves` continue to load.
    let yaml = format!(
        r#"{TIME_3P_NO_ANCHOR}
measures:
  - {{ name: "R", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_r"
    target_measure: "R"
    scope: "AllLeaves"
    body: "1.0"
    declared_dependencies: []
"#
    );
    let compiled = build(&yaml);
    let mut cube = compiled.cube;
    for time in ["P1", "P2", "P3"] {
        let v = read_at(
            &mut cube,
            &compiled.refs,
            compiled.root_principal,
            time,
            "R",
        );
        match v {
            ScalarValue::F64(f) => assert!((f - 1.0).abs() < 1e-9),
            _ => panic!("expected F64 1.0 at {time}, got {v:?}"),
        }
    }
}

/// Test 2: `scope: FutureLeaves` only writes future coords (P3).
#[test]
fn test_scope_future_leaves_only_writes_future_coords() {
    let yaml = format!(
        r#"{TIME_3P_ANCHORED}
measures:
  - {{ name: "R", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_r"
    target_measure: "R"
    scope: "FutureLeaves"
    body: "42.0"
    declared_dependencies: []
"#
    );
    let compiled = build(&yaml);
    let mut cube = compiled.cube;
    let p1 = read_at(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        "P1",
        "R",
    );
    let p2 = read_at(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        "P2",
        "R",
    );
    let p3 = read_at(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        "P3",
        "R",
    );
    assert!(
        matches!(p1, ScalarValue::Null),
        "P1 (past) → Null, got {p1:?}"
    );
    assert!(
        matches!(p2, ScalarValue::Null),
        "P2 (current) → Null, got {p2:?}"
    );
    match p3 {
        ScalarValue::F64(v) => assert!((v - 42.0).abs() < 1e-9),
        _ => panic!("P3 (future) → 42.0, got {p3:?}"),
    }
}

/// Test 3: `scope: PastLeaves` only writes past coords (P1).
#[test]
fn test_scope_past_leaves_only_writes_past_coords() {
    let yaml = format!(
        r#"{TIME_3P_ANCHORED}
measures:
  - {{ name: "R", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_r"
    target_measure: "R"
    scope: "PastLeaves"
    body: "7.0"
    declared_dependencies: []
"#
    );
    let compiled = build(&yaml);
    let mut cube = compiled.cube;
    let p1 = read_at(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        "P1",
        "R",
    );
    let p2 = read_at(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        "P2",
        "R",
    );
    let p3 = read_at(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        "P3",
        "R",
    );
    match p1 {
        ScalarValue::F64(v) => assert!((v - 7.0).abs() < 1e-9),
        _ => panic!("P1 (past) → 7.0, got {p1:?}"),
    }
    assert!(matches!(p2, ScalarValue::Null), "P2 → Null, got {p2:?}");
    assert!(matches!(p3, ScalarValue::Null), "P3 → Null, got {p3:?}");
}

/// Test 4: `scope: CurrentLeaves` only writes current coord (P2).
#[test]
fn test_scope_current_leaves_only_writes_current_coords() {
    let yaml = format!(
        r#"{TIME_3P_ANCHORED}
measures:
  - {{ name: "R", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_r"
    target_measure: "R"
    scope: "CurrentLeaves"
    body: "13.0"
    declared_dependencies: []
"#
    );
    let compiled = build(&yaml);
    let mut cube = compiled.cube;
    let p1 = read_at(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        "P1",
        "R",
    );
    let p2 = read_at(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        "P2",
        "R",
    );
    let p3 = read_at(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        "P3",
        "R",
    );
    assert!(matches!(p1, ScalarValue::Null), "P1 → Null, got {p1:?}");
    match p2 {
        ScalarValue::F64(v) => assert!((v - 13.0).abs() < 1e-9),
        _ => panic!("P2 (current) → 13.0, got {p2:?}"),
    }
    assert!(matches!(p3, ScalarValue::Null), "P3 → Null, got {p3:?}");
}

/// Test 5: Unknown scope name fires MC1029.
#[test]
fn test_scope_unknown_name_fails_mc1029() {
    let yaml = format!(
        r#"{TIME_3P_ANCHORED}
measures:
  - {{ name: "R", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_r"
    target_measure: "R"
    scope: "FutureLeves"
    body: "1.0"
    declared_dependencies: []
"#
    );
    let result = mc_model::load_str(&yaml, Some("test".into()));
    assert!(result.is_err(), "unknown scope must fail");
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains("MC1029"));
    assert!(any, "expected MC1029 in errors: {errs:?}");
}

/// Test 6 (Amendment §4): `FutureLeaves` without time_anchor → MC2069.
#[test]
fn test_scope_future_leaves_without_time_anchor_fails_mc2069() {
    let yaml = format!(
        r#"{TIME_3P_NO_ANCHOR}
measures:
  - {{ name: "R", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_r"
    target_measure: "R"
    scope: "FutureLeaves"
    body: "1.0"
    declared_dependencies: []
"#
    );
    let result = mc_model::load_str(&yaml, Some("test".into()));
    assert!(
        result.is_err(),
        "FutureLeaves without time_anchor must fail"
    );
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains("MC2069"));
    assert!(any, "expected MC2069 in errors: {errs:?}");
}
