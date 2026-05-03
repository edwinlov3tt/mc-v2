//! Phase 3B success-gate item 9 (per ADR-0005 amendment #12).
//!
//! `mc demo --model <path>` does NOT run golden tests. Demo's job is
//! "load + validate + run + print". Goldens are exclusively
//! `mc model test`'s responsibility.
//!
//! This test runs `mc demo --model <fixture-with-impossible-golden>`
//! and asserts exit 0. If demo were running goldens, the impossible
//! `expect: 999999.0` value would fail the demo and the test would
//! exit non-zero.
//!
//! It also runs `mc model test` against the same fixture and asserts
//! a non-zero exit code — proving the bad golden actually trips the
//! test path. Together the two assertions pin the separation of
//! concerns.

use std::path::PathBuf;
use std::process::Command;

const FIXTURE: &str = "crates/mc-model/tests/lint_fixtures/_acme_with_bad_golden.yaml";

fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace dir")
        .to_path_buf()
}

fn mc_binary() -> PathBuf {
    let ws = workspace_root();
    let release = ws.join("target").join("release").join("mc");
    if release.exists() {
        return release;
    }
    ws.join("target").join("debug").join("mc")
}

fn run_mc(args: &[&str]) -> (String, String, i32) {
    let bin = mc_binary();
    if !bin.exists() {
        panic!(
            "mc binary not found at {}; run `cargo build --bin mc` first",
            bin.display()
        );
    }
    let out = Command::new(&bin)
        .args(args)
        .current_dir(workspace_root())
        .output()
        .expect("spawn mc");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let code = out.status.code().unwrap_or(-1);
    (stdout, stderr, code)
}

#[test]
fn mc_demo_with_bad_golden_exits_zero() {
    let (stdout, stderr, code) = run_mc(&["demo", "--model", FIXTURE]);
    assert_eq!(
        code, 0,
        "mc demo --model must exit 0 even with an impossible golden — \
         demo does not run goldens.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    // Smoke-check that demo actually ran (not silently skipped).
    assert!(
        stdout.contains("Building Acme cube"),
        "demo did not appear to run; stdout: {stdout}"
    );
}

#[test]
fn mc_model_test_with_bad_golden_exits_nonzero() {
    let (stdout, _stderr, code) = run_mc(&["model", "test", FIXTURE]);
    assert_ne!(
        code, 0,
        "mc model test MUST fail when a golden is wrong; stdout: {stdout}"
    );
    assert!(
        stdout.contains("FAIL"),
        "expected at least one FAIL line; got: {stdout}"
    );
}
