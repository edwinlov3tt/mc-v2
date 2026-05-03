//! Per-validator negative tests (one per ADR-0004 Decision 6 row).
//!
//! Each test starts from a known-good Acme YAML, mutates exactly one
//! field to introduce one specific malformation, then asserts the
//! corresponding `ValidationError` variant fires.
//!
//! The tests deliberately use textual mutation (`replace` on the YAML
//! string) rather than constructing `ParsedModel` programmatically, so
//! the failure mode each test catches is the same one a human author
//! or LLM would actually hit.

use mc_model::{parse, validate, ValidationError};

const ACME: &str = include_str!("../examples/acme.yaml");

fn must_validate_with_error(yaml: &str) -> Vec<ValidationError> {
    let parsed = parse(yaml, None).unwrap_or_else(|e| panic!("parse must succeed: {e}"));
    validate(parsed)
        .err()
        .unwrap_or_else(|| panic!("validation must fail; expected at least one error but got Ok"))
}

fn assert_any<F: Fn(&ValidationError) -> bool>(errs: &[ValidationError], pred: F, label: &str) {
    assert!(
        errs.iter().any(pred),
        "expected a {label} error in: {errs:#?}"
    );
}

// ---------------------------------------------------------------------------
// 1. duplicate_names — duplicate dimension name
// ---------------------------------------------------------------------------

#[test]
fn duplicate_dimension_name_fires() {
    // Rename "Time" → "Channel" so two dims share the name "Channel".
    let mutated = ACME.replacen("- name: \"Time\"\n", "- name: \"Channel\"\n", 1);
    let errs = must_validate_with_error(&mutated);
    assert_any(
        &errs,
        |e| matches!(e, ValidationError::DuplicateName { kind, name } if kind == "dimension" && name == "Channel"),
        "DuplicateName(dimension, Channel)",
    );
}

#[test]
fn duplicate_element_name_fires() {
    // Add a duplicate "Tampa" element to the Market dim.
    let mutated = ACME.replace(
        "      - { name: \"Tampa\" }\n      - { name: \"Orlando\" }",
        "      - { name: \"Tampa\" }\n      - { name: \"Tampa\" }\n      - { name: \"Orlando\" }",
    );
    let errs = must_validate_with_error(&mutated);
    assert_any(
        &errs,
        |e| matches!(e, ValidationError::DuplicateName { kind, name } if kind.contains("Market") && name == "Tampa"),
        "DuplicateName(element in Market, Tampa)",
    );
}

#[test]
fn duplicate_measure_name_fires() {
    let mutated = ACME.replace(
        "  - { name: \"Spend\", role: \"Input\", data_type: \"F64\", aggregation: \"Sum\" }\n",
        "  - { name: \"Spend\", role: \"Input\", data_type: \"F64\", aggregation: \"Sum\" }\n  - { name: \"Spend\", role: \"Input\", data_type: \"F64\", aggregation: \"Sum\" }\n",
    );
    let errs = must_validate_with_error(&mutated);
    assert_any(
        &errs,
        |e| matches!(e, ValidationError::DuplicateName { kind, name } if kind == "measure" && name == "Spend"),
        "DuplicateName(measure, Spend)",
    );
}

#[test]
fn duplicate_rule_name_fires() {
    // Replace the rule_leads name with rule_clicks so two rules share a name.
    let mutated = ACME.replacen("name: \"rule_leads\"", "name: \"rule_clicks\"", 1);
    let errs = must_validate_with_error(&mutated);
    assert_any(
        &errs,
        |e| matches!(e, ValidationError::DuplicateName { kind, name } if kind == "rule" && name == "rule_clicks"),
        "DuplicateName(rule, rule_clicks)",
    );
}

// ---------------------------------------------------------------------------
// 2. missing_dimensions
// ---------------------------------------------------------------------------

#[test]
fn missing_dimension_referenced_by_hierarchy_fires() {
    // Rename the Time hierarchy's `dimension:` reference to a dim that
    // doesn't exist.
    let mutated = ACME.replacen(
        "  - dimension: \"Time\"\n    name: \"Calendar\"",
        "  - dimension: \"Centuries\"\n    name: \"Calendar\"",
        1,
    );
    let errs = must_validate_with_error(&mutated);
    assert_any(
        &errs,
        |e| matches!(e, ValidationError::MissingDimension { name, .. } if name == "Centuries"),
        "MissingDimension(Centuries)",
    );
}

// ---------------------------------------------------------------------------
// 3. invalid_hierarchy_edges
// ---------------------------------------------------------------------------

#[test]
fn invalid_hierarchy_edge_fires() {
    // Reference a non-existent Time element ("Mars_2026") in the Calendar
    // hierarchy edge list.
    let mutated = ACME.replace(
        "      - { parent: \"FY_2026\", child: \"Q1_2026\", weight: 1.0 }",
        "      - { parent: \"FY_2026\", child: \"Mars_2026\", weight: 1.0 }",
    );
    let errs = must_validate_with_error(&mutated);
    assert_any(
        &errs,
        |e| matches!(e, ValidationError::InvalidHierarchyEdge { dim, element } if dim == "Time" && element == "Mars_2026"),
        "InvalidHierarchyEdge(Time, Mars_2026)",
    );
}

// ---------------------------------------------------------------------------
// 4. hierarchy_cycles
// ---------------------------------------------------------------------------

#[test]
fn hierarchy_cycle_fires() {
    // Insert a Q1_2026 → FY_2026 edge so we have FY → Q1 → FY (a 2-cycle).
    let mutated = ACME.replace(
        "      - { parent: \"FY_2026\", child: \"Q1_2026\", weight: 1.0 }\n",
        "      - { parent: \"FY_2026\", child: \"Q1_2026\", weight: 1.0 }\n      - { parent: \"Q1_2026\", child: \"FY_2026\", weight: 1.0 }\n",
    );
    let errs = must_validate_with_error(&mutated);
    assert_any(
        &errs,
        |e| matches!(e, ValidationError::HierarchyCycle { dim, .. } if dim == "Time"),
        "HierarchyCycle(Time)",
    );
}

// ---------------------------------------------------------------------------
// 5. rules_referencing_unknown_measures
// ---------------------------------------------------------------------------

#[test]
fn rule_referencing_unknown_measure_fires() {
    // Mutate `rule_clicks` to reference a measure that doesn't exist.
    let mutated = ACME.replacen(
        "        - { ref: \"Spend\" }\n        - { ref: \"CPC\" }",
        "        - { ref: \"NotARealMeasure\" }\n        - { ref: \"CPC\" }",
        1,
    );
    let errs = must_validate_with_error(&mutated);
    assert_any(
        &errs,
        |e| matches!(e, ValidationError::RuleReferencesUnknownMeasure { measure_name, .. } if measure_name == "NotARealMeasure"),
        "RuleReferencesUnknownMeasure(NotARealMeasure)",
    );
}

// ---------------------------------------------------------------------------
// 6. derived_measures_without_rules
// ---------------------------------------------------------------------------

#[test]
fn derived_measure_without_rule_fires() {
    // Strip the `rule_gross_profit` rule. Gross_Profit (role: Derived)
    // becomes uncovered.
    let stripped = strip_rule(ACME, "rule_gross_profit");
    let errs = must_validate_with_error(&stripped);
    assert_any(
        &errs,
        |e| matches!(e, ValidationError::DerivedMeasureWithoutRule { measure_name } if measure_name == "Gross_Profit"),
        "DerivedMeasureWithoutRule(Gross_Profit)",
    );
}

// ---------------------------------------------------------------------------
// 7. input_measures_with_rules
// ---------------------------------------------------------------------------

#[test]
fn input_measure_with_rule_fires() {
    // Change `rule_clicks` to target Spend (an Input measure) instead of Clicks.
    let mutated = ACME.replacen(
        "  - name: \"rule_clicks\"\n    target_measure: \"Clicks\"",
        "  - name: \"rule_clicks\"\n    target_measure: \"Spend\"",
        1,
    );
    let errs = must_validate_with_error(&mutated);
    assert_any(
        &errs,
        |e| matches!(e, ValidationError::InputMeasureHasRule { measure_name, .. } if measure_name == "Spend"),
        "InputMeasureHasRule(Spend)",
    );
}

// ---------------------------------------------------------------------------
// 8. rule_cycles
// ---------------------------------------------------------------------------

#[test]
fn rule_cycle_fires() {
    // Make rule_clicks depend on Revenue. The chain becomes:
    //   Clicks → Revenue → Customers → Leads → Clicks
    let mutated = ACME.replacen(
        "    declared_dependencies: [\"Spend\", \"CPC\"]",
        "    declared_dependencies: [\"Spend\", \"CPC\", \"Revenue\"]",
        1,
    );
    let errs = must_validate_with_error(&mutated);
    assert_any(
        &errs,
        |e| matches!(e, ValidationError::RuleCycle { .. }),
        "RuleCycle",
    );
}

// ---------------------------------------------------------------------------
// 9. unsupported_aggregation_methods
// ---------------------------------------------------------------------------

#[test]
fn unsupported_aggregation_fires() {
    // Set Spend's aggregation to "Median", which mc_core doesn't implement.
    let mutated = ACME.replacen(
        "  - { name: \"Spend\", role: \"Input\", data_type: \"F64\", aggregation: \"Sum\" }",
        "  - { name: \"Spend\", role: \"Input\", data_type: \"F64\", aggregation: \"Median\" }",
        1,
    );
    let errs = must_validate_with_error(&mutated);
    assert_any(
        &errs,
        |e| matches!(e, ValidationError::UnsupportedAggregation { measure_name, method } if measure_name == "Spend" && method == "Median"),
        "UnsupportedAggregation(Spend, Median)",
    );
}

// ---------------------------------------------------------------------------
// 10. golden_test_mismatches — surfaced at the test layer (golden_acme.rs).
//     The validator catches *structural* problems with golden_tests
//     (must set exactly one of expect / expect_within_epsilon, refs a
//     declared dim, etc.). Below covers the structural side.
// ---------------------------------------------------------------------------

#[test]
fn golden_test_with_neither_expect_nor_epsilon_fires() {
    let mutated = ACME.replacen("    expect: 11500.0\n", "", 1);
    let errs = must_validate_with_error(&mutated);
    assert_any(
        &errs,
        |e| matches!(e, ValidationError::Schema { message } if message.contains("must set either")),
        "Schema(missing expect)",
    );
}

#[test]
fn golden_test_with_unknown_dim_fires() {
    let mutated = ACME.replacen(
        "      Scenario: \"Baseline\"\n      Version: \"Working\"\n      Time: \"Mar_2026\"\n      Channel: \"Paid_Search\"\n      Market: \"Tampa\"\n      Measure: \"Spend\"\n    expect: 11500.0",
        "      Scenario: \"Baseline\"\n      Version: \"Working\"\n      Time: \"Mar_2026\"\n      Channel: \"Paid_Search\"\n      Market: \"Tampa\"\n      Measure: \"Spend\"\n      Quadrant: \"NW\"\n    expect: 11500.0",
        1,
    );
    let errs = must_validate_with_error(&mutated);
    assert_any(
        &errs,
        |e| matches!(e, ValidationError::MissingDimension { name, .. } if name == "Quadrant"),
        "MissingDimension(Quadrant)",
    );
}

// Helper: strip a named rule from the YAML. The rule blocks span from
// `  - name: "<rule_name>"` to the end of the rule's
// `    declared_dependencies: [...]` line.
fn strip_rule(yaml: &str, rule_name: &str) -> String {
    let needle = format!("  - name: \"{rule_name}\"");
    let start = yaml.find(&needle).unwrap_or_else(|| {
        panic!("rule {rule_name} not found in YAML; mutator out of date");
    });
    // Walk forward to the next `  - name:` line OR end-of-rules-section.
    let after_start = &yaml[start + needle.len()..];
    let next_rule = after_start.find("\n  - name:").unwrap_or(after_start.len());
    let next_section = after_start
        .find("\ngolden_tests:")
        .unwrap_or(after_start.len());
    let stop = next_rule.min(next_section);
    let end = start + needle.len() + stop + 1;
    let mut out = String::with_capacity(yaml.len() - (end - start));
    out.push_str(&yaml[..start]);
    out.push_str(&yaml[end..]);
    out
}
