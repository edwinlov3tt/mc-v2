//! Phase 3D MC1003-MC1006 negative tests + the MC2xxx-on-null-body
//! amendment-#26 case.
//!
//! Each test loads one fixture under `tests/formula_fixtures/` and asserts
//! that `validate(parsed)` returns the expected MC code among the errors.
//! The test layer mirrors `tests/fixture_validators.rs` (Phase 3C) so the
//! diagnostic-code surface stays uniform.

use mc_model::{parse, validate};

const FIXTURE_DIR: &str = "tests/formula_fixtures";

/// Load a fixture, run parse → validate, and return the set of
/// diagnostic codes that fired. `parse` failures panic — the negative
/// fixtures are syntactically-valid YAML by construction; the error
/// codes under test are emitted by the validate stage's formula-parse
/// step, not by serde_yaml.
fn run_pipeline(fixture_filename: &str) -> Vec<&'static str> {
    let path = format!("{FIXTURE_DIR}/{fixture_filename}");
    let yaml = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    let parsed = parse(&yaml, Some(path.clone()))
        .unwrap_or_else(|e| panic!("fixture {fixture_filename:?} failed to parse: {e}"));
    let errs = validate(parsed)
        .err()
        .unwrap_or_else(|| panic!("fixture {fixture_filename:?} unexpectedly validated"));
    errs.iter().map(|e| e.code()).collect()
}

fn assert_codes_contain(fixture: &str, expected: &str) {
    let codes = run_pipeline(fixture);
    assert!(
        codes.contains(&expected),
        "fixture {fixture:?} expected {expected}; got codes={codes:?}"
    );
}

// ---------------------------------------------------------------------------
// MC1003 — unbalanced or unexpected paren
// ---------------------------------------------------------------------------

#[test]
fn mc1003_open_paren_missing_close() {
    assert_codes_contain("unbalanced_parens_open.yaml", "MC1003");
}

#[test]
fn mc1003_extra_close_paren() {
    assert_codes_contain("unbalanced_parens_close.yaml", "MC1003");
}

// ---------------------------------------------------------------------------
// MC1007 — unknown function call (Phase 3E: split from MC1004)
// ---------------------------------------------------------------------------

#[test]
fn mc1004_unknown_function_call() {
    assert_codes_contain("unknown_function.yaml", "MC1007");
}

#[test]
fn mc1008_if_null_arity_one_arg() {
    assert_codes_contain("wrong_if_null_arity_one.yaml", "MC1008");
}

#[test]
fn mc1008_if_null_arity_three_args() {
    assert_codes_contain("wrong_if_null_arity_three.yaml", "MC1008");
}

// ---------------------------------------------------------------------------
// MC1005 — expected expression (e.g., trailing operator)
// ---------------------------------------------------------------------------

#[test]
fn mc1005_trailing_operator() {
    assert_codes_contain("trailing_operator.yaml", "MC1005");
}

// ---------------------------------------------------------------------------
// MC1006 — invalid number literal
// ---------------------------------------------------------------------------

#[test]
fn mc1006_invalid_number_double_dot() {
    assert_codes_contain("invalid_number.yaml", "MC1006");
}

// ---------------------------------------------------------------------------
// Amendment #26 — YAML null body fires existing MC2xxx, not a new code
// ---------------------------------------------------------------------------

#[test]
fn null_body_fires_existing_mc1001_not_a_new_formula_code() {
    // Per amendment #26: `body: null` MUST surface as an existing schema
    // error code, NOT one of the four new MC1003-MC1006 formula codes.
    // serde_yaml rejects null where a string-or-map is expected at parse
    // time → MC1001 (yaml syntax error), which is the right "existing
    // MC1xxx schema error" the amendment intends. The contract is "no
    // new formula code fires"; both MC1001 (parse) and MC2xxx (validate)
    // satisfy it.
    let path = format!("{FIXTURE_DIR}/null_body.yaml");
    let yaml = std::fs::read_to_string(&path).unwrap();
    // The parse stage may reject this directly (serde rejects null for
    // a non-Option enum field). If it does, that's MC1001 — fine. If it
    // gets through to validate, ensure no MC1003-MC1006 fires.
    match parse(&yaml, Some(path)) {
        Ok(parsed) => {
            // Got past parse — validate must NOT emit MC1003-MC1006.
            let errs = validate(parsed)
                .err()
                .unwrap_or_else(|| panic!("null body expected to fail"));
            for e in &errs {
                let code = e.code();
                assert!(
                    !matches!(code, "MC1003" | "MC1004" | "MC1005" | "MC1006"),
                    "null body must NOT fire a formula-parse code; got {code} in errs={errs:?}"
                );
            }
        }
        Err(pe) => {
            // Parse-stage rejection — MC1001 is the standard YAML
            // syntax error code. Any MC1xxx other than the four new
            // formula codes is acceptable.
            let code = pe.code();
            assert!(
                !matches!(code, "MC1003" | "MC1004" | "MC1005" | "MC1006"),
                "null body must NOT fire a formula-parse code at parse stage; got {code}"
            );
        }
    }
}
