//! Tokio-path dependency gate.
//!
//! ADR-0010 Decision 5 (Path 2): Mosaic source code is fully synchronous,
//! and `tokio` may appear in the dependency tree ONLY as a transitive of
//! `postgres` → `tokio-postgres`. This test enforces that mechanically by
//! shelling out to `cargo tree --invert tokio` and inspecting the output.
//!
//! Gated behind feature `dependency-gate` so it does not slow every
//! `cargo test` invocation. Run with:
//!
//! ```sh
//! cargo test -p mc-drivers --features dependency-gate -- dependency_gate
//! ```

#![cfg(feature = "dependency-gate")]

use std::process::Command;

#[test]
fn dependency_gate_tokio_only_via_postgres() {
    let output = Command::new(env!("CARGO"))
        .args([
            "tree",
            "-p",
            "mc-drivers",
            "--invert",
            "--package",
            "tokio",
            "--prefix",
            "depth",
            "--no-default-features",
            "--all-features",
        ])
        .output()
        .expect("cargo tree --invert tokio");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "cargo tree failed: stdout={} stderr={}",
        stdout,
        stderr
    );
    assert!(!stdout.is_empty(), "cargo tree produced no output");

    // Each non-tokio line in the inverted tree is a crate that pulls
    // tokio. The first line is "0tokio …" (the queried package itself).
    // For Path 2 compliance, every line at depth 1 must be tokio-postgres
    // or postgres — no other Mosaic-side crate may pull tokio.
    let mut depth1_consumers: Vec<String> = Vec::new();
    for line in stdout.lines() {
        // cargo tree --prefix depth produces "<n><name> v<ver> ..." with
        // n a leading digit (or several digits for very deep trees).
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let depth_end = trimmed
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(trimmed.len());
        if depth_end == 0 {
            continue;
        }
        let depth: usize = trimmed[..depth_end].parse().unwrap_or(0);
        if depth != 1 {
            continue;
        }
        let after = &trimmed[depth_end..];
        let name = after.split_whitespace().next().unwrap_or("").to_string();
        depth1_consumers.push(name);
    }

    assert!(
        !depth1_consumers.is_empty(),
        "expected at least one depth-1 tokio consumer, full output:\n{}",
        stdout
    );

    // Path 2 (ADR-0010 Decision 5): tokio may appear in Cargo.lock ONLY
    // as a transitive of `postgres → tokio-postgres`. Direct consumers we
    // permit:
    //   - `postgres`, `tokio-postgres` — the canonical pair from Decision 5
    //   - `tokio-util` — a tokio-postgres internal helper that itself
    //     pulls tokio; not Mosaic-side
    let allowed = ["tokio-postgres", "postgres", "tokio-util"];
    let mut violations: Vec<&String> = Vec::new();
    for c in &depth1_consumers {
        if !allowed.contains(&c.as_str()) {
            violations.push(c);
        }
    }

    assert!(
        violations.is_empty(),
        "ADR-0010 Path 2 violation — these crates pull tokio but should not:\n  {:?}\n\nFull cargo tree --invert tokio output:\n{}",
        violations,
        stdout
    );

    // Belt-and-braces: no Mosaic-side crate may consume tokio at any
    // depth in the inverted tree.
    let mosaic_crates = ["mc-core", "mc-fixtures", "mc-model", "mc-cli"];
    for line in stdout.lines() {
        for mc in &mosaic_crates {
            assert!(
                !line.contains(mc),
                "Path 2 violation: Mosaic-side crate `{}` appears in tokio's reverse-dep tree:\n  {}\n\nFull tree:\n{}",
                mc,
                line,
                stdout
            );
        }
    }

    // Sanity check: postgres or tokio-postgres IS present (the legitimate
    // Path 2 consumer).
    assert!(
        depth1_consumers
            .iter()
            .any(|c| matches!(c.as_str(), "tokio-postgres" | "postgres")),
        "expected tokio-postgres or postgres to consume tokio, got: {:?}",
        depth1_consumers
    );
}

#[test]
fn dependency_gate_block_buffer_is_pre_edition2024() {
    // Verifies the Decision 4 RustCrypto-chain pin. block-buffer 0.10.x
    // is the last pre-edition2024 release; 0.12.0+ requires Cargo
    // edition2024 stabilisation (Rust 1.85+).
    let output = Command::new(env!("CARGO"))
        .args([
            "tree",
            "-p",
            "mc-drivers",
            "--invert",
            "--package",
            "block-buffer",
            "--prefix",
            "none",
        ])
        .output()
        .expect("cargo tree --invert block-buffer");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let first = stdout.lines().next().unwrap_or("");
    assert!(
        first.contains("v0.10."),
        "block-buffer must be 0.10.x (Decision 4 pin); got: {}",
        first
    );
}
