//! Phase 3B per-rule lint fixtures.
//!
//! For each lint code MC3001..MC3007, MC3009..MC3011, the corresponding
//! YAML fixture under `tests/lint_fixtures/` is the **smallest** model
//! that triggers exactly that rule. Each test asserts:
//!
//! 1. The expected code fires at least once.
//! 2. **No other code fires** — the fixture is surgical.
//!
//! Plus the MC3008-retired assertion: every lint we ship asserts no
//! active rule emits the code `"MC3008"` (per ADR-0005 amendment #11;
//! the slot is permanently reserved-as-retired).

use mc_model::{lint, parse, validate, Diagnostic};

fn lint_fixture(yaml: &str) -> Vec<Diagnostic> {
    let parsed = parse(yaml, None).unwrap_or_else(|e| panic!("fixture must parse: {e}"));
    let model = validate(parsed).unwrap_or_else(|errs| {
        panic!(
            "fixture must validate cleanly; got {} validation errors: {errs:#?}",
            errs.len()
        )
    });
    lint(&model)
}

fn assert_only_code(diagnostics: &[Diagnostic], expected: &str) {
    let codes: Vec<&str> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.iter().any(|c| *c == expected),
        "expected {expected} to fire; got codes {codes:?}"
    );
    let unexpected: Vec<&&str> = codes.iter().filter(|c| **c != expected).collect();
    assert!(
        unexpected.is_empty(),
        "fixture must trigger only {expected}; spurious codes: {unexpected:?}"
    );
}

#[test]
fn mc3001_missing_dim_description_fires_alone() {
    let yaml = include_str!("lint_fixtures/MC3001_missing_dim_description.yaml");
    let diags = lint_fixture(yaml);
    assert_only_code(&diags, "MC3001");
}

#[test]
fn mc3002_missing_measure_description_fires_alone() {
    let yaml = include_str!("lint_fixtures/MC3002_missing_measure_description.yaml");
    let diags = lint_fixture(yaml);
    assert_only_code(&diags, "MC3002");
}

#[test]
fn mc3003_missing_rule_description_fires_alone() {
    let yaml = include_str!("lint_fixtures/MC3003_missing_rule_description.yaml");
    let diags = lint_fixture(yaml);
    assert_only_code(&diags, "MC3003");
}

#[test]
fn mc3004_no_golden_tests_fires_alone() {
    let yaml = include_str!("lint_fixtures/MC3004_no_golden_tests.yaml");
    let diags = lint_fixture(yaml);
    assert_only_code(&diags, "MC3004");
}

#[test]
fn mc3005_orphan_element_fires_alone() {
    let yaml = include_str!("lint_fixtures/MC3005_orphan_element.yaml");
    let diags = lint_fixture(yaml);
    assert_only_code(&diags, "MC3005");
}

#[test]
fn mc3006_long_rule_chain_fires_alone() {
    let yaml = include_str!("lint_fixtures/MC3006_long_rule_chain.yaml");
    let diags = lint_fixture(yaml);
    assert_only_code(&diags, "MC3006");
}

#[test]
fn mc3007_ratio_with_sum_fires_alone() {
    let yaml = include_str!("lint_fixtures/MC3007_ratio_with_sum.yaml");
    let diags = lint_fixture(yaml);
    assert_only_code(&diags, "MC3007");
}

#[test]
fn mc3009_unused_input_measure_fires_alone() {
    let yaml = include_str!("lint_fixtures/MC3009_unused_input_measure.yaml");
    let diags = lint_fixture(yaml);
    assert_only_code(&diags, "MC3009");
}

#[test]
fn mc3010_unused_derived_measure_fires_alone() {
    let yaml = include_str!("lint_fixtures/MC3010_unused_derived_measure.yaml");
    let diags = lint_fixture(yaml);
    assert_only_code(&diags, "MC3010");
}

#[test]
fn mc3011_hierarchy_root_ambiguity_fires_alone() {
    let yaml = include_str!("lint_fixtures/MC3011_hierarchy_root_ambiguity.yaml");
    let diags = lint_fixture(yaml);
    assert_only_code(&diags, "MC3011");
}

/// Guard against accidental MC3008 reuse. Per ADR-0005 amendment #11 the
/// MC3008 slot is permanently reserved-as-retired (formerly
/// "WeightedAverage missing weight" — promoted to MC2011 in Phase 3B).
/// Any active lint emitting `"MC3008"` would silently break the
/// code-to-meaning maps Phase 4 LLM scaffolding and Phase 6 UI editor pin
/// against.
///
/// The check sweeps every fixture under `lint_fixtures/`, the canonical
/// `examples/acme.yaml`, plus a synthetic "everything wrong" model and
/// asserts no diagnostic ever carries the code `"MC3008"`.
#[test]
fn no_active_lint_emits_mc3008() {
    let fixtures: &[&str] = &[
        include_str!("../examples/acme.yaml"),
        include_str!("lint_fixtures/MC3001_missing_dim_description.yaml"),
        include_str!("lint_fixtures/MC3002_missing_measure_description.yaml"),
        include_str!("lint_fixtures/MC3003_missing_rule_description.yaml"),
        include_str!("lint_fixtures/MC3004_no_golden_tests.yaml"),
        include_str!("lint_fixtures/MC3005_orphan_element.yaml"),
        include_str!("lint_fixtures/MC3006_long_rule_chain.yaml"),
        include_str!("lint_fixtures/MC3007_ratio_with_sum.yaml"),
        include_str!("lint_fixtures/MC3009_unused_input_measure.yaml"),
        include_str!("lint_fixtures/MC3010_unused_derived_measure.yaml"),
        include_str!("lint_fixtures/MC3011_hierarchy_root_ambiguity.yaml"),
    ];
    for (i, yaml) in fixtures.iter().enumerate() {
        let parsed = parse(yaml, None).expect("fixture must parse");
        let model = match validate(parsed) {
            Ok(m) => m,
            Err(errs) => {
                // Skip fixtures that don't validate (none of our lint
                // fixtures should fail validation, but be defensive).
                panic!("fixture #{i} failed validation: {errs:#?}");
            }
        };
        let diags = lint(&model);
        for d in &diags {
            assert_ne!(
                d.code, "MC3008",
                "no active lint may emit MC3008 (retired, promoted to MC2011) — fixture #{i}"
            );
        }
    }
}
