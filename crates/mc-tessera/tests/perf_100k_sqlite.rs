//! HEADLINE PERFORMANCE TEST — Phase 5A Stream D acceptance gate #5.
//!
//! Per ADR-0010 Decision 1 item 5 + Stream D handoff §12: a 100K-row
//! SQLite recipe import must complete end-to-end in **≤ 3 seconds** on
//! `--release` builds.
//!
//! This test generates a deterministic SQLite fixture at test setup
//! (100K rows × 9 columns), writes a recipe, runs Tessera::apply, and
//! asserts the wall-clock total is under 3000 ms.
//!
//! On debug builds the wall-clock is dominated by the unoptimized kernel
//! and SQLite path; the test skips its assertion on debug and only
//! checks correctness (rows_written = 100_000). For the actual perf
//! gate, run `cargo test -p mc-tessera --release -- perf_100k_sqlite`.

use std::fs;
use std::path::Path;
use std::time::Duration;

use mc_tessera::Tessera;
use rusqlite::Connection;

/// Performance ceiling on `--release` builds.
const PERF_CEILING_RELEASE: Duration = Duration::from_millis(3_000);

#[test]
fn perf_100k_sqlite() {
    let tempdir = make_tempdir("perf_100k_sqlite");
    let db_path = tempdir.join("perf.sqlite");

    // Use a small fixture cube model (1 input measure + a 100k-row
    // dimension). Generating a synthetic 100k-element model is the only
    // way to exercise WriteBatch at 100k cells without depending on a
    // pre-existing fixture model.
    let model_path = tempdir.join("perf.yaml");
    fs::write(model_path, build_perf_model_yaml(100_000)).expect("write perf model");

    // Generate the SQLite fixture: 100K rows × (Slot, value).
    populate_sqlite(&db_path, 100_000);

    let recipe_path = tempdir.join("perf.recipe.yaml");
    fs::write(&recipe_path, PERF_RECIPE_YAML).expect("write recipe");

    let prepared = Tessera::prepare(&recipe_path).expect("prepare");
    let report = Tessera::apply(prepared).expect("apply");

    assert_eq!(report.rows_written, 100_000);
    assert_eq!(report.rows_failed, 0);

    // Only enforce the perf ceiling on release builds. Debug builds
    // ship with the unoptimized kernel and would push the test over the
    // 3-second mark for reasons unrelated to the WriteBatch path.
    if cfg!(not(debug_assertions)) {
        let total = Duration::from_millis(report.timing.total_ms);
        assert!(
            total <= PERF_CEILING_RELEASE,
            "perf_100k_sqlite: {} ms exceeds ceiling {} ms (fetch={}, transform={}, commit={})",
            total.as_millis(),
            PERF_CEILING_RELEASE.as_millis(),
            report.timing.fetch_ms,
            report.timing.transform_ms,
            report.timing.commit_ms,
        );
    }
}

/// Build a minimal YAML model with a single dimension `Slot` of N
/// elements + a single Input measure `value`. The `Measure` dim has one
/// element (also called `value`) so the recipe's measure column maps
/// 1:1.
fn build_perf_model_yaml(n_slots: usize) -> String {
    let mut s = String::new();
    s.push_str("model_format_version: 1\n");
    s.push_str("metadata:\n");
    s.push_str("  name: \"perf_100k\"\n");
    s.push_str("  description: \"Phase 5A Stream D 100K perf test fixture\"\n");
    s.push_str("dimensions:\n");
    s.push_str("  - name: \"Slot\"\n");
    s.push_str("    kind: \"Standard\"\n");
    s.push_str("    elements:\n");
    for i in 0..n_slots {
        s.push_str(&format!("      - {{ name: \"s_{i}\" }}\n"));
    }
    s.push_str("  - name: \"Measure\"\n");
    s.push_str("    kind: \"Measure\"\n");
    s.push_str("    elements:\n");
    s.push_str("      - { name: \"value\" }\n");
    s.push_str("measures:\n");
    s.push_str(
        "  - { name: \"value\", role: \"Input\", data_type: \"F64\", aggregation: \"Sum\" }\n",
    );
    s.push_str("rules: []\n");
    s
}

/// Build a SQLite database with `n` rows of (slot_name TEXT, value REAL).
fn populate_sqlite(path: &Path, n: usize) {
    let conn = Connection::open(path).expect("open sqlite");
    conn.execute_batch(
        "CREATE TABLE rows (slot TEXT NOT NULL, val REAL NOT NULL); BEGIN TRANSACTION;",
    )
    .expect("schema");
    {
        let mut stmt = conn
            .prepare("INSERT INTO rows (slot, val) VALUES (?1, ?2)")
            .expect("prepare insert");
        for i in 0..n {
            // Deterministic value so re-runs are repeatable.
            let v = (i as f64) * 0.5 + 1.0;
            stmt.execute(rusqlite::params![format!("s_{i}"), v])
                .expect("insert");
        }
    }
    conn.execute_batch("COMMIT;").expect("commit");
}

const PERF_RECIPE_YAML: &str = r#"version: 1
name: perf_100k
description: "100K SQLite import perf gate"
model: ./perf.yaml
source:
  driver: sqlite
  path: ./perf.sqlite
  query: "SELECT slot, val FROM rows ORDER BY slot"
columns:
  - { source: slot, dimension: Slot }
  - { source: val, measure: value, type: f64 }
write_disposition: replace
incremental: false
batch:
  size: 50000
on_error: abort
on_missing_element: error
"#;

fn make_tempdir(label: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("mc-tessera-test-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("create tempdir");
    base
}
