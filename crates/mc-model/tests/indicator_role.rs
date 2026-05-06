//! Phase 3J Item 4: `Indicator` measure role.
//!
//! Indicator measures are the declarative form of the `is_element(Dim,
//! "Element")` formula function. They declare `dimension:` and
//! `element:` fields and require no `body:`. ADR-0016 Decision 7 +
//! Amendment §6 binds Indicator measures to compile to the same
//! `Expr::IsElement(DimensionId, ElementId)` AST that `is_element(Dim,
//! "Element")` produces.

use std::collections::BTreeMap;

use mc_core::{CellCoordinate, Expr, ScalarValue};
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
  name: "P3JI4"
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
    elements:
      - { name: "Houston" }
      - { name: "Austin" }
  - name: "Measure"
    kind: "Measure"
    elements: []
"#;

/// Test 1: `Indicator` measure loads from YAML cleanly.
#[test]
fn test_indicator_measure_loads_from_yaml() {
    let yaml = format!(
        r#"{BASE}
measures:
  - name: "IsHouston"
    role: "Indicator"
    dimension: "Market"
    element: "Houston"
    description: "1.0 at Houston coords"
rules: []
"#
    );
    let compiled = build(&yaml);
    // The cube should have an `IsHouston` measure registered.
    assert!(
        compiled.refs.element("Measure", "IsHouston").is_some(),
        "IsHouston measure must be registered"
    );
}

/// Test 2: Reading IsHouston at a Houston coord returns 1.0.
#[test]
fn test_indicator_returns_one_at_matching_coord() {
    let yaml = format!(
        r#"{BASE}
measures:
  - name: "IsHouston"
    role: "Indicator"
    dimension: "Market"
    element: "Houston"
rules: []
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
            ("Market", "Houston"),
            ("Measure", "IsHouston"),
        ],
    );
    assert!(
        (v - 1.0).abs() < 1e-9,
        "IsHouston at Houston coord must equal 1.0, got {v}"
    );
}

/// Test 3: Reading IsHouston at a non-matching coord returns 0.0.
#[test]
fn test_indicator_returns_zero_at_non_matching_coord() {
    let yaml = format!(
        r#"{BASE}
measures:
  - name: "IsHouston"
    role: "Indicator"
    dimension: "Market"
    element: "Houston"
rules: []
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
            ("Market", "Austin"),
            ("Measure", "IsHouston"),
        ],
    );
    assert!(
        v.abs() < 1e-9,
        "IsHouston at Austin coord must equal 0.0, got {v}"
    );
}

/// Test 4: Indicator with `body:` (i.e., a user-supplied rule targeting
/// the Indicator measure) → MC2063.
#[test]
fn test_indicator_with_body_fails_mc2063() {
    let yaml = format!(
        r#"{BASE}
measures:
  - name: "IsHouston"
    role: "Indicator"
    dimension: "Market"
    element: "Houston"
rules:
  - name: "rule_overrides_indicator"
    target_measure: "IsHouston"
    scope: "AllLeaves"
    body: 'is_element(Market, "Austin")'
    declared_dependencies: []
"#
    );
    let result = mc_model::load_str(&yaml, Some("test".into()));
    assert!(result.is_err(), "user rule on Indicator must fail");
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains("MC2063"));
    assert!(any, "expected MC2063 in errors: {errs:?}");
}

/// Test 5 (Amendment §6 binding): Indicator measure compiles to the
/// same `Expr::IsElement(...)` AST as a comparable `is_element` rule.
#[test]
fn test_indicator_compiles_to_same_ast_as_is_element() {
    let indicator_yaml = format!(
        r#"{BASE}
measures:
  - name: "IsHouston"
    role: "Indicator"
    dimension: "Market"
    element: "Houston"
"#
    );
    let is_element_yaml = format!(
        r#"{BASE}
measures:
  - {{ name: "IsHouston", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_is_houston"
    target_measure: "IsHouston"
    scope: "AllLeaves"
    body: 'is_element(Market, "Houston")'
    declared_dependencies: []
"#
    );

    let ind_compiled = build(&indicator_yaml);
    let ie_compiled = build(&is_element_yaml);

    // Find the IsHouston rule body in each cube. The rule registry's
    // public surface returns Vec<usize> indices via rules_for_measure;
    // we inspect via the public iter() on RuleSet.
    let ind_target = ind_compiled
        .refs
        .element("Measure", "IsHouston")
        .expect("IsHouston measure id");
    let ie_target = ie_compiled
        .refs
        .element("Measure", "IsHouston")
        .expect("IsHouston measure id");

    let ind_body = find_rule_body(&ind_compiled.cube, ind_target);
    let ie_body = find_rule_body(&ie_compiled.cube, ie_target);

    // Both bodies should be Expr::IsElement(_, _). The dim/element IDs
    // are different across cubes (separate IdGenerators), but the
    // structural shape and element NAMES must match.
    match (&ind_body, &ie_body) {
        (Expr::IsElement(ind_dim, ind_elem), Expr::IsElement(ie_dim, ie_elem)) => {
            // Verify dim and element names match across both cubes.
            let ind_dim_name = ind_compiled
                .cube
                .dimensions()
                .iter()
                .find(|d| d.id == *ind_dim)
                .map(|d| d.name.as_str())
                .expect("indicator dim name");
            let ie_dim_name = ie_compiled
                .cube
                .dimensions()
                .iter()
                .find(|d| d.id == *ie_dim)
                .map(|d| d.name.as_str())
                .expect("is_element dim name");
            assert_eq!(
                ind_dim_name, ie_dim_name,
                "Indicator dim must match is_element dim"
            );
            let ind_elem_name = lookup_element_name(&ind_compiled.cube, *ind_dim, *ind_elem);
            let ie_elem_name = lookup_element_name(&ie_compiled.cube, *ie_dim, *ie_elem);
            assert_eq!(
                ind_elem_name, ie_elem_name,
                "Indicator element must match is_element element"
            );
        }
        (other_a, other_b) => panic!(
            "expected both bodies to be Expr::IsElement, got Indicator={other_a:?}, is_element={other_b:?}"
        ),
    }
}

fn find_rule_body(cube: &mc_core::Cube, target: mc_core::ElementId) -> Expr {
    for r in cube.rules().iter() {
        if r.target_measure == target {
            return r.body.clone();
        }
    }
    panic!("no rule found for target {target:?}");
}

fn lookup_element_name(
    cube: &mc_core::Cube,
    dim: mc_core::DimensionId,
    elem: mc_core::ElementId,
) -> String {
    cube.dimensions()
        .iter()
        .find(|d| d.id == dim)
        .and_then(|d| d.element(elem))
        .map(|e| e.name.clone())
        .expect("element lookup")
}
