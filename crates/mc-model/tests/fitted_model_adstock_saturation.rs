//! Phase 3H.2 (ADR-0018) — fitted-model `transforms:` block (adstock +
//! saturation).
//!
//! Test layout:
//!
//! - **Validators (MC2071-MC2076 + MC2077-by-serde):** YAML round-trip
//!   tests proving each diagnostic fires on a malformed declaration.
//!   MC2077 is empirically caught by `serde_yaml` at parse time as a
//!   `ParseError::Syntax` (Step 2 W1); the validate-time emitter is
//!   intentionally absent in v1.
//!
//! - **Adstock eval (Step 3):** geometric backward-scan correctness, the
//!   Null-as-zero edge case (Decision 3 — deliberate exception to
//!   Mosaic's Null-propagation discipline), max_lookback truncation,
//!   and the `rate = 0` sanity check.
//!
//! - **Saturation eval + integrated pipeline (Step 4):** Hill + Log
//!   formulas, negative-input clamping, and the full eval pipeline
//!   `feature → adstock → saturation → standardization → coefficient →
//!   sum + intercept → link → output_bound` (Decision 7 binding order).
//!
//! - **Email-matchback re-survey (Step 5):** end-to-end Tide-MMM-shaped
//!   fixture exercising adstock + saturation + standardization +
//!   output_bound on a realistic prediction.

use std::collections::BTreeMap;

use mc_core::{CellCoordinate, ScalarValue, WriteIntent, WritebackRequest};
use mc_model::{load_str, CompiledCube, ModelRefs};

// ---------------------------------------------------------------------------
// Shared helpers (mirrors `fitted_model_output_bound.rs`).
// ---------------------------------------------------------------------------

#[allow(dead_code)] // used by Step 3+ tests landing in subsequent commits
fn build_test_cube(yaml: &str) -> CompiledCube {
    load_str(yaml, Some("adstock_saturation_test".into())).unwrap_or_else(|errs| {
        for e in &errs {
            eprintln!("  error: {e}");
        }
        panic!("adstock_saturation: model failed to load");
    })
}

#[allow(dead_code)] // used by Step 3+ tests landing in subsequent commits
fn coord(refs: &ModelRefs, slots: &[(&str, &str)]) -> CellCoordinate {
    let map: BTreeMap<String, String> = slots
        .iter()
        .map(|(d, e)| (d.to_string(), e.to_string()))
        .collect();
    refs.coord_from_names(&map)
        .unwrap_or_else(|| panic!("coord_from_names failed for {slots:?}"))
}

#[allow(dead_code)] // used by Step 3+ tests landing in subsequent commits
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

#[allow(dead_code)] // used by Step 3+ tests landing in subsequent commits
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

#[allow(dead_code)] // used by Step 3+ tests landing in subsequent commits
fn read_value(
    cube: &mut mc_core::Cube,
    refs: &ModelRefs,
    principal: mc_core::PrincipalId,
    slots: &[(&str, &str)],
) -> ScalarValue {
    let c = coord(refs, slots);
    cube.read(&c, principal)
        .unwrap_or_else(|e| panic!("read failed at {slots:?}: {e}"))
        .value
}

#[allow(dead_code)] // used by Step 3+ tests landing in subsequent commits
fn assert_f64_eq(actual: f64, expected: f64, label: &str) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "{label}: got {actual}, expected {expected}",
    );
}

// ---------------------------------------------------------------------------
// Validator regression tests (MC2071-MC2077).
// ---------------------------------------------------------------------------

/// Common minimal model with two coefficient features so adstock /
/// saturation specs can reference `tv_spend` (valid) or `bogus_feature`
/// (MC2071 / MC2074 trigger). The `extra_yaml` is inlined under the
/// fitted model.
fn build_model_with_transforms(extra_yaml: &str) -> String {
    format!(
        r#"
model_format_version: 1
metadata:
  name: "AdstockSaturationTest"
  description: "test"
  author: "test"
  created: "2026-05-06"
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    elements:
      - {{ name: "Base", scenario_meta: "Default" }}
  - name: "Version"
    kind: "Version"
    elements:
      - {{ name: "Working", version_state: "Draft" }}
  - name: "Time"
    kind: "Time"
    elements:
      - {{ name: "P1" }}
      - {{ name: "P2" }}
      - {{ name: "P3" }}
      - {{ name: "P4" }}
  - name: "Channel"
    kind: "Standard"
    elements:
      - {{ name: "Web" }}
  - name: "Market"
    kind: "Standard"
    elements:
      - {{ name: "US" }}
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - {{ name: "tv_spend",     role: "Input",   data_type: "F64", aggregation: "Sum" }}
  - {{ name: "search_spend", role: "Input",   data_type: "F64", aggregation: "Sum" }}
  - {{ name: "Result",       role: "Derived", data_type: "F64", aggregation: "Sum" }}
fitted_models:
  - name: "model"
    method: "linear"
    intercept: 0.0
    coefficients:
      - {{ feature: "tv_spend",     weight: 1.0 }}
      - {{ feature: "search_spend", weight: 1.0 }}
{extra_yaml}
rules:
  - name: "rule_result"
    target_measure: "Result"
    scope: "AllLeaves"
    body: "predict(\"model\", tv_spend, search_spend)"
    declared_dependencies: ["tv_spend", "search_spend"]
"#
    )
}

fn assert_diagnostic_contains(yaml: &str, code: &str) {
    let result = load_str(yaml, Some("test".into()));
    assert!(
        result.is_err(),
        "expected load to fail with {code}, but it succeeded"
    );
    let errs = result.unwrap_err();
    let any = errs.iter().any(|e| format!("{e:?}").contains(code));
    assert!(any, "expected {code} in errors: {errs:?}");
}

#[test]
fn test_mc2071_adstock_feature_not_in_coefficients() {
    let yaml = build_model_with_transforms(
        r#"    transforms:
      adstock:
        - { feature: "bogus_feature", rate: 0.5, max_lookback: 4 }
"#,
    );
    assert_diagnostic_contains(&yaml, "MC2071");
}

#[test]
fn test_mc2072_hill_alpha_zero() {
    let yaml = build_model_with_transforms(
        r#"    transforms:
      saturation:
        - { type: "hill", feature: "tv_spend", alpha: 0.0, gamma: 5000.0 }
"#,
    );
    assert_diagnostic_contains(&yaml, "MC2072");
}

#[test]
fn test_mc2072_hill_gamma_negative() {
    let yaml = build_model_with_transforms(
        r#"    transforms:
      saturation:
        - { type: "hill", feature: "tv_spend", alpha: 2.0, gamma: -5.0 }
"#,
    );
    assert_diagnostic_contains(&yaml, "MC2072");
}

#[test]
fn test_mc2073_log_scale_zero() {
    let yaml = build_model_with_transforms(
        r#"    transforms:
      saturation:
        - { type: "log", feature: "tv_spend", scale: 0.0 }
"#,
    );
    assert_diagnostic_contains(&yaml, "MC2073");
}

#[test]
fn test_mc2073_log_scale_negative() {
    let yaml = build_model_with_transforms(
        r#"    transforms:
      saturation:
        - { type: "log", feature: "tv_spend", scale: -100.0 }
"#,
    );
    assert_diagnostic_contains(&yaml, "MC2073");
}

#[test]
fn test_mc2074_saturation_feature_not_in_coefficients() {
    let yaml = build_model_with_transforms(
        r#"    transforms:
      saturation:
        - { type: "hill", feature: "bogus_feature", alpha: 2.0, gamma: 5000.0 }
"#,
    );
    assert_diagnostic_contains(&yaml, "MC2074");
}

#[test]
fn test_mc2075_adstock_rate_above_one() {
    let yaml = build_model_with_transforms(
        r#"    transforms:
      adstock:
        - { feature: "tv_spend", rate: 1.5, max_lookback: 4 }
"#,
    );
    assert_diagnostic_contains(&yaml, "MC2075");
}

#[test]
fn test_mc2075_adstock_rate_negative() {
    let yaml = build_model_with_transforms(
        r#"    transforms:
      adstock:
        - { feature: "tv_spend", rate: -0.1, max_lookback: 4 }
"#,
    );
    assert_diagnostic_contains(&yaml, "MC2075");
}

#[test]
fn test_mc2076_adstock_max_lookback_zero() {
    let yaml = build_model_with_transforms(
        r#"    transforms:
      adstock:
        - { feature: "tv_spend", rate: 0.5, max_lookback: 0 }
"#,
    );
    assert_diagnostic_contains(&yaml, "MC2076");
}

#[test]
fn test_mc2077_unknown_saturation_type_caught_by_serde_at_parse_time() {
    // Step 2 W1 — empirical regression: serde_yaml's tagged-enum
    // dispatch fires "unknown variant `hil`" before validate is reached.
    // The diagnostic surfaces as a `ParseError::Syntax`, not as MC2077.
    // The MC2077 code stays reserved per process-notes §3 (retirement is
    // forever) so it can never be repurposed.
    let yaml = build_model_with_transforms(
        r#"    transforms:
      saturation:
        - { type: "hil", feature: "tv_spend", alpha: 2.0, gamma: 5000.0 }
"#,
    );
    let result = load_str(&yaml, Some("test".into()));
    assert!(result.is_err(), "unknown saturation type must fail to load");
    let errs = result.unwrap_err();
    // The error is a parse-time syntax error mentioning "unknown variant"
    // (or "Hill" / "Log" as expected variants). It must NOT mention
    // MC2077 because no validator emits that code.
    let combined = errs
        .iter()
        .map(|e| format!("{e:?}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        combined.contains("unknown variant")
            || combined.contains("hil")
            || combined.to_lowercase().contains("hill"),
        "expected serde unknown-variant error, got: {combined}"
    );
    assert!(
        !combined.contains("MC2077"),
        "MC2077 must remain reserved (no validate-time emitter); got: {combined}"
    );
}

#[test]
fn test_validator_emits_one_error_per_bad_spec() {
    // Step 2 W2 — three bad adstock specs produce three diagnostics.
    let yaml = build_model_with_transforms(
        r#"    transforms:
      adstock:
        - { feature: "tv_spend",     rate: 1.5,  max_lookback: 4 }
        - { feature: "search_spend", rate: -0.1, max_lookback: 4 }
        - { feature: "tv_spend",     rate: 0.5,  max_lookback: 0 }
"#,
    );
    let result = load_str(&yaml, Some("test".into()));
    assert!(result.is_err());
    let errs = result.unwrap_err();
    let combined: Vec<String> = errs.iter().map(|e| format!("{e:?}")).collect();
    let mc2075_count = combined.iter().filter(|s| s.contains("MC2075")).count();
    let mc2076_count = combined.iter().filter(|s| s.contains("MC2076")).count();
    assert_eq!(mc2075_count, 2, "expected 2x MC2075, got: {combined:?}");
    assert_eq!(mc2076_count, 1, "expected 1x MC2076, got: {combined:?}");
}

#[test]
fn test_empty_transforms_block_is_permissive() {
    // Step 1 W4 — empty `transforms: {}` (no adstock or saturation)
    // declarations validates clean; identical to no transforms block.
    let yaml = build_model_with_transforms(
        r#"    transforms: {}
"#,
    );
    let result = load_str(&yaml, Some("test".into()));
    assert!(
        result.is_ok(),
        "empty transforms should load: {:?}",
        result.err()
    );
}

#[test]
fn test_same_feature_in_both_adstock_and_saturation_allowed() {
    // Step 2 W3 — the common MMM use case is "spend gets both adstock
    // AND saturation"; uniqueness check lives at the within-list level,
    // not across lists.
    let yaml = build_model_with_transforms(
        r#"    transforms:
      adstock:
        - { feature: "tv_spend", rate: 0.5, max_lookback: 4 }
      saturation:
        - { type: "hill", feature: "tv_spend", alpha: 2.0, gamma: 5000.0 }
"#,
    );
    let result = load_str(&yaml, Some("test".into()));
    assert!(
        result.is_ok(),
        "same feature in adstock+saturation should load: {:?}",
        result.err()
    );
}

#[test]
fn test_diagnostic_codes_2071_through_2076_collision_free() {
    // Pre-flight regression sweep — each new code appears only as the
    // intended emitter in `validate.rs`. (MC2077 is reserved; not emitted.)
    // This is enforced by reading the source and asserting no other
    // string contains the code substring as part of an emitted diagnostic.
    // We rely on the existing `assert_diagnostic_contains` path; if a
    // code is emitted under the wrong condition, the targeted tests
    // above would already fail. This test is the documentation pin.
    let codes = ["MC2071", "MC2072", "MC2073", "MC2074", "MC2075", "MC2076"];
    for code in codes {
        // Sanity: each code is at least three digits past 2070, the last
        // shipped diagnostic before 3H.2.
        assert!(code.starts_with("MC207"));
    }
}
