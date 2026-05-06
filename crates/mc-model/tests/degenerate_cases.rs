//! Phase 3I degenerate-case regression tests.
//!
//! Surfaced during the Phase 3I self-audit (§K of the audit report).
//! These tests cover the shapes the main regression suite missed:
//! 1-arg `ifs(default)`, 2-arg `switch(expr, default)` (no pairs),
//! `lookup_tables` with `key_dimensions: []`, and AST-shape verification
//! of cross-coord ops in the filter context.

use mc_model::{parse_expression, ParsedRuleBody, ParsedScalar};

#[test]
fn ifs_one_arg_form_emits_bare_const() {
    // Per Phase 3I item 6 W5: `ifs(default_value)` (one arg, no pairs)
    // is allowed — the parser emits the bare default expression.
    let parsed = parse_expression("ifs(0.42)").expect("ifs(default_only) must parse");
    match parsed {
        ParsedRuleBody::Const(c) => match c.value {
            ParsedScalar::Float(v) => assert!((v - 0.42).abs() < 1e-9),
            other => panic!("expected Float, got {other:?}"),
        },
        other => panic!("expected Const default, got {other:?}"),
    }
}

#[test]
fn switch_no_pairs_emits_bare_default() {
    // `switch(expr, default)` with no match/value pairs — degenerate
    // but legal. The parser emits the bare default; the scrutinee is
    // dropped (per audit §K finding — acceptable per matrix W2).
    let parsed =
        parse_expression("switch(period_index(), 999)").expect("switch with no pairs must parse");
    match parsed {
        ParsedRuleBody::Const(c) => match c.value {
            ParsedScalar::Float(v) => assert!((v - 999.0).abs() < 1e-9),
            other => panic!("expected Float default, got {other:?}"),
        },
        other => panic!("expected Const, got {other:?}"),
    }
}

#[test]
fn lookup_table_empty_key_dimensions_is_rejected() {
    // `key_dimensions: []` (empty array) is degenerate. Validation
    // catches this via the multi-key arity-mismatch path (MC2052):
    // any value-key has at least one part, but key_dimensions.len()
    // is zero, so the parts-count check fires.
    let yaml = r#"
model_format_version: 1
metadata:
  name: "EmptyKeyDimsTest"
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
      - { name: "Houston" }
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - { name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "Result", role: "Derived", data_type: "F64", aggregation: "Sum" }
lookup_tables:
  - name: "empty_keys"
    key_dimensions: []
    values:
      "anything": 1.0
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "lookup(\"empty_keys\", Market) * Spend"
    declared_dependencies: ["Spend"]
"#;
    let result = mc_model::load_str(yaml, Some("test".into()));
    assert!(
        result.is_err(),
        "empty key_dimensions array must be rejected"
    );
}

#[test]
fn filter_with_sum_over_ast_contains_cross_coord() {
    // Confirms the parsed AST for a filter-shaped expression with
    // sum_over correctly contains the SumOver node — the
    // Filter::parse path then walks it and emits MC1025. Tested
    // end-to-end in the CLI integration suite; this guards the AST
    // shape so the rejector can't drift past it silently.
    let parsed = parse_expression("sum_over(Spend, Market) > 1000").unwrap();
    fn contains_sum_over(b: &ParsedRuleBody) -> bool {
        match b {
            ParsedRuleBody::SumOver(_) => true,
            ParsedRuleBody::Gt(inner)
            | ParsedRuleBody::Lt(inner)
            | ParsedRuleBody::Gte(inner)
            | ParsedRuleBody::Lte(inner)
            | ParsedRuleBody::Eq(inner)
            | ParsedRuleBody::Neq(inner) => {
                contains_sum_over(&inner.left) || contains_sum_over(&inner.right)
            }
            _ => false,
        }
    }
    assert!(
        contains_sum_over(&parsed),
        "AST must contain SumOver for the audit assertion"
    );
}
