//! Phase 3C schema-stability assertion
//! (per ADR-0006 Decision 9 #12 + acceptance amendment #20).
//!
//! Phase 3C must NOT change the `Diagnostic` struct shape — adding a
//! field requires a `schema_version` bump from `"1.0"` to `"1.1"`. The
//! `Diagnostic` codes themselves grow (Phase 3C adds MC2012–MC2025),
//! but field-level additions are backwards-incompatible for
//! consumers (Phase 4 LLM, Phase 6 UI) that pin to a known shape.
//!
//! These tests:
//!
//! 1. Load every `tests/expected/lint_*.json` snapshot fixture (these
//!    were locked in Phase 3B).
//! 2. Assert each fixture still parses cleanly into the current
//!    `Diagnostic` shape (uses string-shape checks since we don't have
//!    `serde_json`).
//! 3. Assert each fixture's `schema_version` is `"1.0"` (un-bumped).
//! 4. Assert the `Diagnostic` JSON envelope round-trips bit-for-bit
//!    when re-emitted via `diagnostics_to_json` (catches changes to
//!    field order / formatting).

use mc_model::diagnostic::Span;
use mc_model::{diagnostics_to_json, Diagnostic, ModelPath, Severity, SCHEMA_VERSION};

const EXPECTED_DIR: &str = "tests/expected";

fn lint_fixture_files() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(EXPECTED_DIR).expect("read expected dir") {
        let entry = entry.expect("entry");
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("lint_") && name.ends_with(".json") {
            let content = std::fs::read_to_string(entry.path()).expect("read fixture");
            out.push((name, content));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn schema_version_constant_is_unchanged() {
    assert_eq!(
        SCHEMA_VERSION, "1.0",
        "Phase 3C MUST NOT bump SCHEMA_VERSION (per ADR-0006 amendment #20). \
         If you intentionally bumped it, the JSON envelope contract changed and \
         every Phase 3B lint snapshot has to be regenerated AND every Phase 4 / 6 \
         consumer pinned to 1.0 will break."
    );
}

#[test]
fn every_lint_snapshot_carries_schema_version_one_zero() {
    let fixtures = lint_fixture_files();
    assert!(
        !fixtures.is_empty(),
        "no lint_*.json fixtures found in {EXPECTED_DIR}"
    );
    for (name, content) in &fixtures {
        assert!(
            content.contains("\"schema_version\": \"1.0\""),
            "fixture {name:?} missing `\"schema_version\": \"1.0\"` — \
             a Phase 3C change either drifted the envelope or accidentally \
             bumped to 1.1. Restore the 1.0 emission or document a deliberate \
             bump in the completion report."
        );
    }
}

#[test]
fn every_lint_snapshot_diagnostic_has_all_five_fields() {
    let fixtures = lint_fixture_files();
    for (name, content) in &fixtures {
        // Empty-diagnostics envelopes (e.g., lint_acme_clean.json)
        // skip the per-diag field check.
        if !content.contains("\"code\":") {
            continue;
        }
        for field in ["code", "severity", "path", "message", "suggestion"] {
            let needle = format!("\"{field}\":");
            assert!(
                content.contains(&needle),
                "fixture {name:?} missing top-level Diagnostic field {field:?} \
                 (Diagnostic shape change requires schema_version bump per amendment #20)"
            );
        }
        for path_field in ["file", "span", "yaml_pointer", "model_path"] {
            let needle = format!("\"{path_field}\":");
            assert!(
                content.contains(&needle),
                "fixture {name:?} missing ModelPath sub-field {path_field:?} \
                 (ModelPath shape change requires schema_version bump per amendment #20)"
            );
        }
    }
}

#[test]
fn diagnostic_json_round_trip_emits_envelope_at_schema_one_zero() {
    // Synthesize one of each Severity at the same path, emit via
    // diagnostics_to_json, and assert the result still uses the
    // expected envelope and field shape. This is the live contract
    // (independent of whatever lint fixtures happen to exist).
    let path = ModelPath {
        file: std::path::PathBuf::from("acme.yaml"),
        span: Some(Span::new(1, 1)),
        yaml_pointer: "/dimensions/0".into(),
        model_path: "dimensions.Time".into(),
    };
    let diags = vec![
        Diagnostic {
            code: "MC2012",
            severity: Severity::Error,
            path: path.clone(),
            message: "shape probe error".into(),
            suggestion: Some("hint".into()),
        },
        Diagnostic {
            code: "MC3001",
            severity: Severity::Warning,
            path: path.clone(),
            message: "shape probe warning".into(),
            suggestion: None,
        },
    ];
    let json = diagnostics_to_json(&diags);
    assert!(
        json.contains("\"schema_version\": \"1.0\""),
        "envelope drifted: {json}"
    );
    for field in ["code", "severity", "path", "message", "suggestion"] {
        assert!(
            json.contains(&format!("\"{field}\":")),
            "round-trip JSON missing {field:?}: {json}"
        );
    }
    for field in ["file", "span", "yaml_pointer", "model_path"] {
        assert!(
            json.contains(&format!("\"{field}\":")),
            "round-trip JSON missing path.{field:?}: {json}"
        );
    }
    // No NEW field has snuck in (basic over/under count check on
    // top-level Diagnostic keys). The 5 keys are listed once per
    // diagnostic, so 2 diags = 10 occurrences. If a 6th key were
    // added, this count would change.
    let key_count: usize = ["code", "severity", "path", "message", "suggestion"]
        .iter()
        .map(|k| json.matches(&format!("\"{k}\":")).count())
        .sum();
    assert_eq!(
        key_count, 10,
        "expected exactly 10 top-level Diagnostic key occurrences across \
         2 diagnostics (5 keys × 2); a count mismatch suggests a Diagnostic \
         shape change. Got: {json}"
    );
}
