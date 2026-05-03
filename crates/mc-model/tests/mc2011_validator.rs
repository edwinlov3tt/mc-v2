//! MC2011 validator promotion test.
//!
//! Per ADR-0005 amendment #4, "WeightedAverage measure missing
//! weight_measure" was promoted from a lint warning to a hard validation
//! error in Phase 3B. The error blocks `mc_model::load()`; consumers that
//! get this error know the model is structurally incorrect (the kernel
//! cannot consolidate a WeightedAverage measure without a weight).

use mc_model::{load_str, Error, ValidationError};

const FIXTURE: &str = include_str!("lint_fixtures/MC2011_weighted_average_missing_weight.yaml");

#[test]
fn load_returns_err_with_mc2011_when_weight_missing() {
    let result = load_str(FIXTURE, Some("MC2011_fixture.yaml".into()));
    let errs = result.expect_err("load must fail");
    let mut saw_mc2011 = false;
    for e in &errs {
        if let Error::Validation(v) = e {
            if matches!(v, ValidationError::WeightedAverageMissingWeight { .. }) {
                assert_eq!(v.code(), "MC2011");
                saw_mc2011 = true;
            }
        }
    }
    assert!(
        saw_mc2011,
        "expected at least one ValidationError::WeightedAverageMissingWeight; got {errs:#?}"
    );
}

#[test]
fn validation_error_codes_are_mc2001_to_mc2011() {
    use mc_model::{parse, validate};

    // Build a synthetic ParsedModel and run it through the validator
    // path that produces each of the 11 codes. We don't need to trigger
    // every variant — exhaustive coverage of the discriminants lives in
    // tests/validators.rs. Here we just spot-check the code() wiring on
    // one error per code namespace.
    let bad = "model_format_version: 99\nmetadata:\n  name: \"X\"\ndimensions:\n  - { name: \"M\", kind: \"Measure\", elements: [] }\nmeasures: []\nrules: []\n";
    let parsed = parse(bad, None).expect("parses");
    let errs = validate(parsed).expect_err("must fail");
    for e in errs {
        let code = e.code();
        assert!(
            code.starts_with("MC2") && code.len() == 6,
            "every ValidationError code must be MC2xxx; got {code:?}"
        );
        assert!(
            code != "MC3008",
            "validator must never emit MC3008 (retired)"
        );
    }
}
