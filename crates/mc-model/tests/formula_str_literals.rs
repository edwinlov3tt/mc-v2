//! Phase 3J Item 1 + Amendment §1: ScalarValue::Str first-class in eval.
//!
//! Tests the binding boundary for `ScalarValue::Str`: strings flow through
//! expression evaluation but never reach storage / consolidation / dirty
//! tracker / writeback. Strings are produced by `Expr::StrLiteral` and
//! `Expr::CurrentElementName`, and consumed by `Expr::StrEq` / `Expr::StrNeq`.
//! Per ADR-0016 Decision 2-4 and Hand-off Item 1.

use std::collections::BTreeMap;

use mc_core::{CellCoordinate, ScalarValue};
use mc_model::{load_str, CompiledCube, ModelRefs};

// ----------------------------------------------------------------------------
// Helpers — minimal cube with Channel (Email/Web) + an indicator-style result.
// ----------------------------------------------------------------------------

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
        other => panic!("expected F64 at {slots:?}, got {other:?}"),
    }
}

fn build_cube(yaml: &str) -> CompiledCube {
    load_str(yaml, Some("test".into())).unwrap_or_else(|errs| {
        for e in &errs {
            eprintln!("  error: {e}");
        }
        panic!("model failed to load");
    })
}

// ----------------------------------------------------------------------------
// Required regression tests (10 minimum per handoff Item 1).
// ----------------------------------------------------------------------------

/// Test 1: A string literal in formula source parses to `Expr::StrLiteral`
/// and survives the parse → validate → compile pipeline when consumed by
/// a string-equality operator.
#[test]
fn test_str_literal_in_formula_parses_to_expr_strliteral() {
    use mc_model::ParsedRuleBody;
    // Parse a formula containing a bare string literal as the second arg
    // to a string equality. The parser produces StrLiteral.
    let parsed = mc_model::parse_expression(r#"current_element(Channel) == "Email""#)
        .expect("parse should succeed");
    // The outer expression is Eq(CurrentElement, StrLiteral).
    let ParsedRuleBody::Eq(b) = &parsed else {
        panic!("expected Eq, got {parsed:?}");
    };
    assert!(
        matches!(*b.right, ParsedRuleBody::StrLiteral(_)),
        "rhs should be StrLiteral, got {:?}",
        b.right
    );
}

/// Test 2: `Str == Str` returns F64(1.0) when equal.
#[test]
fn test_str_equality_returns_f64_one_when_equal() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "Phase3JItem1"
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
      - { name: "Email" }
      - { name: "Web" }
  - name: "Market"
    kind: "Standard"
    elements:
      - { name: "US" }
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: 'current_element(Channel) == "Email"'
    declared_dependencies: []
"#;
    let compiled = build_cube(yaml);
    let mut cube = compiled.cube;
    let val = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Email"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    assert!(
        (val - 1.0).abs() < 1e-9,
        "Email coord should match Email literal → 1.0, got {val}"
    );
}

/// Test 3: `Str == Str` returns F64(0.0) when unequal.
#[test]
fn test_str_equality_returns_f64_zero_when_unequal() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "P3JI1T3"
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
    elements:
      - { name: "Email" }
      - { name: "Web" }
  - name: "Market"
    kind: "Standard"
    elements: [{ name: "US" }]
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: 'current_element(Channel) == "Email"'
    declared_dependencies: []
"#;
    let compiled = build_cube(yaml);
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
    assert!(
        val.abs() < 1e-9,
        "Web coord should not match Email literal → 0.0, got {val}"
    );
}

/// Test 4: `Str != Str` is the inverse of `==`.
#[test]
fn test_str_inequality_inverse_of_equality() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "P3JI1T4"
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
    elements:
      - { name: "Email" }
      - { name: "Web" }
  - name: "Market"
    kind: "Standard"
    elements: [{ name: "US" }]
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: 'current_element(Channel) != "Email"'
    declared_dependencies: []
"#;
    let compiled = build_cube(yaml);
    let mut cube = compiled.cube;
    let email_val = read_f64(
        &mut cube,
        &compiled.refs,
        compiled.root_principal,
        &[
            ("Scenario", "Base"),
            ("Version", "Working"),
            ("Time", "P1"),
            ("Channel", "Email"),
            ("Market", "US"),
            ("Measure", "Result"),
        ],
    );
    let web_val = read_f64(
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
        email_val.abs() < 1e-9,
        "!= at Email coord must be 0.0, got {email_val}"
    );
    assert!(
        (web_val - 1.0).abs() < 1e-9,
        "!= at Web coord must be 1.0, got {web_val}"
    );
}

/// Test 5: `Str == Null` returns Null (Null propagation). Exercised at the
/// kernel `eval_expr` level since Null doesn't typically appear as a Str
/// arg in YAML — but `current_element` at a consolidated coord returns
/// Null, which when compared to a string literal yields Null per the
/// `eval_str_eq` semantics.
#[test]
fn test_str_eq_null_returns_null() {
    use mc_core::{Expr, ScalarValue};

    // Build an Expr::StrEq with a Null left side and a StrLiteral right
    // side. Use the kernel's no-cross/no-self lookup to evaluate.
    let body = Expr::StrEq(
        Box::new(Expr::Const(ScalarValue::Null)),
        Box::new(Expr::StrLiteral("Houston".into())),
    );
    let mut self_lookup = |_id: mc_core::ElementId| -> Result<ScalarValue, mc_core::EngineError> {
        Ok(ScalarValue::Null)
    };
    let mut cross_lookup =
        |_r: &mc_core::CrossCoordRead| -> Result<ScalarValue, mc_core::EngineError> {
            Ok(ScalarValue::Null)
        };
    let v = mc_core::eval_expr(&body, &mut self_lookup, &mut cross_lookup)
        .expect("eval should succeed");
    assert!(
        matches!(v, ScalarValue::Null),
        "Null == Str must return Null, got {v:?}"
    );
}

/// Test 6: `Str == F64` (statically detectable) fails validation with MC1027.
#[test]
fn test_str_eq_f64_fails_with_mc1027() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "P3JI1T6"
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
    elements: [{ name: "Email" }]
  - name: "Market"
    kind: "Standard"
    elements: [{ name: "US" }]
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: '"Email" == 1'
    declared_dependencies: []
"#;
    let result = mc_model::load_str(yaml, Some("test".into()));
    assert!(result.is_err(), "Str == F64 must fail validation");
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains("MC1027"));
    assert!(any, "expected MC1027 in errors: {errs:?}");
}

/// Test 7 (Amendment §1): Str in `if` condition fails MC1027.
#[test]
fn test_str_in_if_condition_fails_with_mc1027() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "P3JI1T7"
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
    elements: [{ name: "Email" }]
  - name: "Market"
    kind: "Standard"
    elements: [{ name: "US" }]
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: 'if(current_element(Channel), 1, 0)'
    declared_dependencies: []
"#;
    let result = mc_model::load_str(yaml, Some("test".into()));
    assert!(result.is_err(), "Str in if() condition must fail");
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains("MC1027"));
    assert!(any, "expected MC1027 in errors: {errs:?}");
}

/// Test 8 (Amendment §1): Str as `and`/`or`/`not` operand fails MC1027.
#[test]
fn test_str_in_and_operand_fails_with_mc1027() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "P3JI1T8"
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
    elements: [{ name: "Email" }]
  - name: "Market"
    kind: "Standard"
    elements: [{ name: "US" }]
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: 'current_element(Channel) and 1 > 0'
    declared_dependencies: []
"#;
    let result = mc_model::load_str(yaml, Some("test".into()));
    assert!(result.is_err(), "Str as `and` operand must fail");
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains("MC1027"));
    assert!(any, "expected MC1027 in errors: {errs:?}");
}

/// Test 9: Str in arithmetic fails MC1026.
#[test]
fn test_str_in_arithmetic_fails_with_mc1026() {
    let yaml = r#"
model_format_version: 1
metadata:
  name: "P3JI1T9"
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
    elements: [{ name: "Email" }]
  - name: "Market"
    kind: "Standard"
    elements: [{ name: "US" }]
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: 'current_element(Channel) + 1'
    declared_dependencies: []
"#;
    let result = mc_model::load_str(yaml, Some("test".into()));
    assert!(result.is_err(), "Str + F64 must fail");
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains("MC1026"));
    assert!(any, "expected MC1026 in errors: {errs:?}");
}

/// Test 10b (audit-pinned regression — Section L finding): a rule body
/// whose static type is Indeterminate (an `if` whose branches both
/// return Str) must be rejected at runtime when the rule fires. The
/// pre-fix bug: such a body validated cleanly (because
/// `expr_static_type(If)` returns Indeterminate so MC2058 doesn't fire
/// statically), and at eval time the Str value was cached into
/// `HashMapStore` and emitted in trace JSON output as `"value": "Email"`.
/// The runtime guard in `Cube::read_derived_leaf` now rejects Str
/// values before they hit `self.store.write`, surfaced as
/// `EngineError::RuleBodyTypeMismatch`.
#[test]
fn test_runtime_str_in_rule_body_rejected_at_eval_time() {
    let yaml = r#"
model_format_version: 1
metadata: { name: "x", description: "x", author: "x", created: "2026-01-01" }
dimensions:
  - { name: "Scenario", kind: "Scenario", elements: [{ name: "B", scenario_meta: "Default" }] }
  - { name: "Version", kind: "Version", elements: [{ name: "W", version_state: "Draft" }] }
  - { name: "Time", kind: "Time", elements: [{ name: "P1" }] }
  - { name: "Channel", kind: "Standard", elements: [{ name: "Email" }] }
  - { name: "Market", kind: "Standard", elements: [{ name: "U" }] }
  - { name: "Measure", kind: "Measure", elements: [] }
measures:
  - { name: "R", role: "Derived", data_type: "F64", aggregation: "Sum" }
rules:
  - name: "rr"
    target_measure: "R"
    scope: "AllLeaves"
    body: 'if(1 > 0, current_element(Channel), current_element(Market))'
    declared_dependencies: []
"#;
    let compiled = mc_model::load_str(yaml, Some("test".into())).expect(
        "validate must accept dynamic Str (Indeterminate static type) — runtime catches it",
    );
    let mut cube = compiled.cube;
    let c = mc_core::CellCoordinate::from_parts(
        compiled.refs.cube_id,
        compiled
            .refs
            .dimension_order
            .iter()
            .map(|dim| match dim.as_str() {
                "Scenario" => compiled.refs.element("Scenario", "B").unwrap(),
                "Version" => compiled.refs.element("Version", "W").unwrap(),
                "Time" => compiled.refs.element("Time", "P1").unwrap(),
                "Channel" => compiled.refs.element("Channel", "Email").unwrap(),
                "Market" => compiled.refs.element("Market", "U").unwrap(),
                "Measure" => compiled.refs.element("Measure", "R").unwrap(),
                _ => panic!("unknown dim"),
            }),
    );
    let result = cube.read(&c, compiled.root_principal);
    assert!(
        result.is_err(),
        "rule body returning Str at runtime must error, not cache the Str value"
    );
    let err = result.unwrap_err();
    let msg = format!("{err:?}");
    assert!(
        msg.contains("MC2058 runtime") || msg.contains("Str value"),
        "expected runtime Str rejection error, got: {msg}"
    );
}

/// Test 10: Writeback receives `ScalarValue::Str` → kernel rejects with
/// EngineError::TypeMismatch (the MC2059 contract). The mc-model layer
/// surfaces this as TypeMismatch from the kernel — there is no parser
/// path to write a Str cell value; the test exercises the
/// defense-in-depth `Cube::write` rejection directly.
#[test]
fn test_str_writeback_rejected_with_mc2059() {
    use mc_core::{
        AggregationRule, CellDataType, Cube, Dimension, DimensionKind, Element, IdGenerator,
        MeasureRole, ScalarValue, WriteIntent, WritebackRequest,
    };

    let g = IdGenerator::new();
    let cube_id = g.cube();
    let market_id = g.dimension();
    let measure_id = g.dimension();
    let us = g.element();
    let m_str = g.element();
    let principal = g.principal();

    let market = Dimension::builder(market_id, "Market", DimensionKind::Standard)
        .add_element(Element::leaf(us, "US", market_id))
        .expect("market")
        .build()
        .expect("market build");

    let measure = Dimension::builder(measure_id, "Measure", DimensionKind::Measure)
        .add_element(Element::measure(
            m_str,
            "M",
            measure_id,
            CellDataType::F64,
            MeasureRole::Input,
            AggregationRule::Sum,
        ))
        .expect("measure")
        .build()
        .expect("measure build");

    let mut cube: Cube = Cube::builder(cube_id, "T".to_string())
        .add_dimension(market)
        .add_dimension(measure)
        .measure_dimension("Measure".to_string())
        .root_principal(principal)
        .build()
        .expect("cube");

    let coord = mc_core::CellCoordinate::from_parts(cube_id, [us, m_str]);
    let result = cube.write(WritebackRequest {
        coord,
        new_value: ScalarValue::Str("rogue".into()),
        principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    });
    assert!(
        result.is_err(),
        "Cube::write of ScalarValue::Str must be rejected"
    );
    let err = result.unwrap_err();
    assert!(
        matches!(err, mc_core::EngineError::TypeMismatch { .. }),
        "expected TypeMismatch (MC2059 contract), got {err:?}"
    );
}
