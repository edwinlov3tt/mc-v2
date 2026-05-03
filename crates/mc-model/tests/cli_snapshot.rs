//! Hand-rolled CLI snapshot harness (per ADR-0005 amendment #7).
//!
//! Locks the text + JSON output of `mc model inspect` and `mc model lint`
//! against fixtures under `tests/expected/`. The harness avoids `insta`
//! by hand-rolling a `assert_snapshot(actual, expected_path)` helper —
//! Phase 3B's "minimum dep churn" policy makes the trade-off worth it
//! (the helper is ~30 lines).
//!
//! Failures print a clear diff between actual and expected. To regenerate
//! a snapshot, set `MC_SNAPSHOT_UPDATE=1` and rerun the test (the helper
//! writes the actual output to the expected path and panics so the test
//! still fails — the developer reviews the new snapshot before committing).

use std::path::Path;
use std::process::Command;

/// Compare `actual` against the contents of `expected_path`. On mismatch:
///
/// 1. If env var `MC_SNAPSHOT_UPDATE=1`, write `actual` to `expected_path`
///    and panic so the test fails (the developer reviews + commits).
/// 2. Otherwise panic with a brief diff hint.
fn assert_snapshot(actual: &str, expected_path: impl AsRef<Path>) {
    let expected_path = expected_path.as_ref();
    let expected = std::fs::read_to_string(expected_path).unwrap_or_else(|e| {
        panic!(
            "snapshot read failed for {}: {e}; create the file or set MC_SNAPSHOT_UPDATE=1",
            expected_path.display()
        )
    });
    if actual == expected {
        return;
    }
    if std::env::var("MC_SNAPSHOT_UPDATE").is_ok() {
        std::fs::write(expected_path, actual).expect("snapshot write");
        panic!(
            "snapshot updated at {}: re-review and commit (test still fails by design)",
            expected_path.display()
        );
    }
    let actual_preview = preview(actual);
    let expected_preview = preview(&expected);
    panic!(
        "snapshot mismatch at {}\n--- expected (len {}) ---\n{}\n--- actual (len {}) ---\n{}\n--- end ---\n(set MC_SNAPSHOT_UPDATE=1 to regenerate)",
        expected_path.display(),
        expected.len(),
        expected_preview,
        actual.len(),
        actual_preview,
    );
}

fn preview(s: &str) -> String {
    if s.len() <= 4096 {
        return s.to_string();
    }
    let mut head = s.chars().take(2000).collect::<String>();
    head.push_str("\n... <truncated> ...\n");
    head.push_str(
        &s.chars()
            .skip(s.chars().count().saturating_sub(2000))
            .collect::<String>(),
    );
    head
}

/// Locate the `mc` binary for the current build profile. Tests run with
/// `cargo test`; the workspace target dir hosts both `target/debug/mc`
/// and `target/release/mc`. We prefer release if present (matches the
/// gate's `cargo run --release` invocations) and fall back to debug.
fn mc_binary() -> std::path::PathBuf {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace dir");
    let release = workspace.join("target").join("release").join("mc");
    if release.exists() {
        return release;
    }
    workspace.join("target").join("debug").join("mc")
}

/// Run the `mc` binary with the given argv (relative to the workspace
/// root). Returns `(stdout, stderr, exit_code)`. Panics on spawn errors.
fn run_mc(args: &[&str]) -> (String, String, i32) {
    let bin = mc_binary();
    if !bin.exists() {
        panic!(
            "mc binary not found at {}; run `cargo build --bin mc` first",
            bin.display()
        );
    }
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace dir")
        .to_path_buf();
    let out = Command::new(&bin)
        .args(args)
        .current_dir(&workspace)
        .output()
        .unwrap_or_else(|e| panic!("spawn failed for {}: {e}", bin.display()));
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(out.stderr).expect("utf8 stderr");
    let code = out.status.code().unwrap_or(-1);
    (stdout, stderr, code)
}

fn fixture_expected(name: &str) -> std::path::PathBuf {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("tests").join("expected").join(name)
}

// ---------------------------------------------------------------------------
// inspect
// ---------------------------------------------------------------------------

#[test]
fn inspect_acme_text_matches_snapshot() {
    let (stdout, stderr, code) =
        run_mc(&["model", "inspect", "crates/mc-model/examples/acme.yaml"]);
    assert_eq!(code, 0, "expected exit 0; stderr: {stderr}");
    assert_snapshot(&stdout, fixture_expected("inspect_acme.txt"));
}

/// Phase 3D acceptance amendment #24 — inspect renders ALL rules in
/// formula form regardless of authoring form. This test pins the
/// uniformity using `_acme_with_bad_golden.yaml` (structured-authored)
/// as the canary: its inspect output renders rules via
/// `formula::serialize`, identical to the formula-authored Acme.
#[test]
fn inspect_structured_authored_fixture_renders_rules_as_formulas() {
    let (stdout, stderr, code) = run_mc(&[
        "model",
        "inspect",
        "crates/mc-model/tests/lint_fixtures/_acme_with_bad_golden.yaml",
    ]);
    assert_eq!(code, 0, "expected exit 0; stderr: {stderr}");
    assert_snapshot(
        &stdout,
        fixture_expected("inspect_acme_with_bad_golden.txt"),
    );
    // Sanity: the snapshot must NOT contain the old structured rendering
    // shape (parens around binary ops, `Const(Float(...))` debug form).
    assert!(
        !stdout.contains("Const(Float"),
        "structured-authored fixture must render via formula::serialize, not Debug-format Consts: {stdout}"
    );
    assert!(
        stdout.contains("Revenue * (1 - COGS_Rate)"),
        "expected formula-form rendering of Gross_Profit; got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// lint — per-fixture text + Acme JSON
// ---------------------------------------------------------------------------

fn lint_fixture_path(name: &str) -> String {
    format!("crates/mc-model/tests/lint_fixtures/{name}.yaml")
}

fn check_lint_snapshot(fixture: &str, snapshot_name: &str, expected_exit: i32) {
    let (stdout, stderr, code) = run_mc(&["model", "lint", &lint_fixture_path(fixture)]);
    assert_eq!(code, expected_exit, "exit mismatch; stderr: {stderr}");
    assert_snapshot(&stdout, fixture_expected(snapshot_name));
}

#[test]
fn lint_mc3001_matches_snapshot() {
    check_lint_snapshot(
        "MC3001_missing_dim_description",
        "lint_MC3001_missing_dim_description.txt",
        0,
    );
}

#[test]
fn lint_mc3002_matches_snapshot() {
    check_lint_snapshot(
        "MC3002_missing_measure_description",
        "lint_MC3002_missing_measure_description.txt",
        0,
    );
}

#[test]
fn lint_mc3003_matches_snapshot() {
    check_lint_snapshot(
        "MC3003_missing_rule_description",
        "lint_MC3003_missing_rule_description.txt",
        0,
    );
}

#[test]
fn lint_mc3004_matches_snapshot() {
    check_lint_snapshot(
        "MC3004_no_golden_tests",
        "lint_MC3004_no_golden_tests.txt",
        0,
    );
}

#[test]
fn lint_mc3005_matches_snapshot() {
    check_lint_snapshot("MC3005_orphan_element", "lint_MC3005_orphan_element.txt", 0);
}

#[test]
fn lint_mc3006_matches_snapshot() {
    check_lint_snapshot(
        "MC3006_long_rule_chain",
        "lint_MC3006_long_rule_chain.txt",
        0,
    );
}

#[test]
fn lint_mc3007_matches_snapshot() {
    check_lint_snapshot("MC3007_ratio_with_sum", "lint_MC3007_ratio_with_sum.txt", 0);
}

#[test]
fn lint_mc3009_matches_snapshot() {
    check_lint_snapshot(
        "MC3009_unused_input_measure",
        "lint_MC3009_unused_input_measure.txt",
        0,
    );
}

#[test]
fn lint_mc3010_matches_snapshot() {
    check_lint_snapshot(
        "MC3010_unused_derived_measure",
        "lint_MC3010_unused_derived_measure.txt",
        0,
    );
}

#[test]
fn lint_mc3011_matches_snapshot() {
    check_lint_snapshot(
        "MC3011_hierarchy_root_ambiguity",
        "lint_MC3011_hierarchy_root_ambiguity.txt",
        0,
    );
}

#[test]
fn lint_acme_json_envelope_clean() {
    // Phase 3B success-gate item 7 (per ADR-0005 amendment #13): the
    // envelope must include `schema_version: "1.0"` even with zero
    // diagnostics. This test pins the empty-case JSON byte-for-byte.
    let (stdout, stderr, code) = run_mc(&[
        "model",
        "lint",
        "crates/mc-model/examples/acme.yaml",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "expected exit 0; stderr: {stderr}");
    assert!(
        stdout.contains("\"schema_version\": \"1.0\""),
        "envelope missing schema_version: {stdout}"
    );
    assert!(
        stdout.contains("\"diagnostics\": []"),
        "envelope must contain empty diagnostics array; got: {stdout}"
    );
    assert_snapshot(&stdout, fixture_expected("lint_acme_clean.json"));
}

#[test]
fn lint_mc3001_json_envelope_with_diagnostic() {
    let (stdout, stderr, code) = run_mc(&[
        "model",
        "lint",
        &lint_fixture_path("MC3001_missing_dim_description"),
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "expected exit 0; stderr: {stderr}");
    // Phase 3B success-gate item 7: every diagnostic must carry the
    // five contracted fields.
    for field in &[
        "\"schema_version\": \"1.0\"",
        "\"code\": \"MC3001\"",
        "\"severity\": \"Warning\"",
        "\"path\":",
        "\"message\":",
        "\"suggestion\":",
        "\"yaml_pointer\":",
        "\"model_path\":",
    ] {
        assert!(
            stdout.contains(field),
            "envelope missing required field/value {field:?}; got: {stdout}"
        );
    }
    assert_snapshot(&stdout, fixture_expected("lint_MC3001.json"));
}

#[test]
fn lint_deny_warnings_returns_nonzero_on_warnings() {
    let (_stdout, _stderr, code) = run_mc(&[
        "model",
        "lint",
        &lint_fixture_path("MC3001_missing_dim_description"),
        "--deny-warnings",
    ]);
    assert_eq!(code, 1, "--deny-warnings must elevate to non-zero exit");
}

#[test]
fn lint_deny_warnings_zero_on_clean_acme() {
    let (_stdout, _stderr, code) = run_mc(&[
        "model",
        "lint",
        "crates/mc-model/examples/acme.yaml",
        "--deny-warnings",
    ]);
    assert_eq!(code, 0, "--deny-warnings on a clean lint must still exit 0");
}

// ---------------------------------------------------------------------------
// validate / test golden gates
// ---------------------------------------------------------------------------

#[test]
fn validate_acme_exits_zero() {
    let (stdout, stderr, code) =
        run_mc(&["model", "validate", "crates/mc-model/examples/acme.yaml"]);
    assert_eq!(code, 0, "expected exit 0; stderr: {stderr}");
    assert!(
        stdout.is_empty(),
        "validate text format is silent on success"
    );
}

#[test]
fn validate_mc2011_fixture_returns_nonzero_with_code() {
    let (_stdout, stderr, code) = run_mc(&[
        "model",
        "validate",
        &lint_fixture_path("MC2011_weighted_average_missing_weight"),
    ]);
    assert_ne!(code, 0, "MC2011 fixture must fail validation");
    assert!(
        stderr.contains("MC2011"),
        "stderr should mention MC2011 code; got: {stderr}"
    );
}

#[test]
fn test_acme_passes_all_goldens() {
    // Phase 3B success-gate item 15: `mc model test` on Acme exits 0
    // with all 9 inline goldens passing.
    let (stdout, stderr, code) = run_mc(&["model", "test", "crates/mc-model/examples/acme.yaml"]);
    assert_eq!(code, 0, "expected exit 0; stderr: {stderr}");
    assert!(
        stdout.contains("Goldens: 9/9 passed, 0 failed"),
        "expected '9/9 passed' line; got: {stdout}"
    );
}
