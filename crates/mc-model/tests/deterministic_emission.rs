//! Phase 3B success-gate item 8 (per ADR-0005 amendment #14).
//!
//! Diagnostic emission order is `(severity desc, code asc, yaml_pointer
//! asc, message asc)`. This test verifies:
//!
//! 1. A fixture that triggers ≥ 3 diagnostics emits in the contracted
//!    order.
//! 2. 10 consecutive runs of the same input produce byte-exact output
//!    (no `HashMap`-iteration nondeterminism leaks through).

use mc_model::{lint, parse, validate, Severity};

const MULTI: &str = include_str!("lint_fixtures/_multi_diagnostic.yaml");

#[test]
fn ten_consecutive_runs_produce_identical_output() {
    let mut prev: Option<Vec<(String, String, String)>> = None;
    for run in 0..10 {
        let parsed = parse(MULTI, None).expect("multi fixture parses");
        let model = validate(parsed).expect("multi fixture validates");
        let diags = lint(&model);
        let snapshot: Vec<(String, String, String)> = diags
            .iter()
            .map(|d| {
                (
                    d.code.to_string(),
                    d.severity.label().to_string(),
                    d.path.yaml_pointer.clone(),
                )
            })
            .collect();
        if let Some(p) = &prev {
            assert_eq!(
                p, &snapshot,
                "run {run}: emission diverged from run 0 — sort is nondeterministic"
            );
        }
        prev = Some(snapshot);
    }
    let final_snapshot = prev.expect("at least one run");
    assert!(
        final_snapshot.len() >= 3,
        "fixture must produce ≥ 3 diagnostics; got {}",
        final_snapshot.len()
    );
}

#[test]
fn sort_order_is_severity_desc_then_code_asc_then_pointer_asc() {
    let parsed = parse(MULTI, None).expect("multi fixture parses");
    let model = validate(parsed).expect("multi fixture validates");
    let diags = lint(&model);

    // Walk the sorted output and verify each adjacent pair is in order.
    for window in diags.windows(2) {
        let (a, b) = (&window[0], &window[1]);
        let sev_a = a.severity as u8;
        let sev_b = b.severity as u8;
        if sev_a > sev_b {
            // higher severity first — fine
            continue;
        }
        assert_eq!(
            sev_a, sev_b,
            "severity must be non-increasing; got {:?} then {:?}",
            a.severity, b.severity
        );
        // Same severity — code must be non-decreasing.
        if a.code < b.code {
            continue;
        }
        assert_eq!(
            a.code, b.code,
            "within severity, code must be non-decreasing; got {} then {}",
            a.code, b.code
        );
        // Same severity + code — yaml_pointer must be non-decreasing.
        if a.path.yaml_pointer < b.path.yaml_pointer {
            continue;
        }
        assert_eq!(
            a.path.yaml_pointer, b.path.yaml_pointer,
            "within (severity, code), yaml_pointer must be non-decreasing"
        );
        // Same severity + code + pointer — message is the final tiebreak.
        assert!(a.message <= b.message, "message tiebreak failed");
    }

    // Also assert the structural shape we expect: warnings before info.
    let has_warning = diags.iter().any(|d| d.severity == Severity::Warning);
    let has_info = diags.iter().any(|d| d.severity == Severity::Info);
    if has_warning && has_info {
        let last_warning = diags
            .iter()
            .rposition(|d| d.severity == Severity::Warning)
            .expect("has warning");
        let first_info = diags
            .iter()
            .position(|d| d.severity == Severity::Info)
            .expect("has info");
        assert!(
            last_warning < first_info,
            "all warnings must come before any info"
        );
    }
}
