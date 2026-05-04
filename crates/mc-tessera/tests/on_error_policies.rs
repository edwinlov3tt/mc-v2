//! `on_error: skip_row` skips bad rows; `on_error: quarantine` writes
//! them to the quarantine log; `on_error: abort` (default) fails fast.

use std::fs;
use std::path::Path;

use mc_tessera::{Sidecar, Tessera, TesseraError};

#[test]
fn skip_row_skips_bad_dim_value() {
    let dir = make_tempdir("skip_row");
    copy_acme_assets(&dir);

    fs::write(
        dir.join("recipe.yaml"),
        r#"version: 1
name: skip_test
model: ./acme.yaml
source:
  driver: csv
  path: ./inputs.csv
columns:
  - { source: Channel, dimension: Channel }
  - { source: Spend, measure: Spend, type: f64 }
defaults:
  Scenario: Baseline
  Version: Working
  Time: Jan_2026
  Market: Tampa
on_error: skip_row
"#,
    )
    .unwrap();

    // 3 rows: 1 bad (unknown channel), 2 good.
    fs::write(
        dir.join("inputs.csv"),
        "Channel,Spend\nPaid_Search,1.0\nNotAChannel,2.0\nDisplay,3.0\n",
    )
    .unwrap();

    let prepared = Tessera::prepare(&dir.join("recipe.yaml")).unwrap();
    let report = Tessera::apply(prepared).unwrap();
    assert_eq!(report.rows_written, 2);
    assert_eq!(report.rows_failed, 1);
    assert_eq!(report.rows_processed, 3);
}

#[test]
fn quarantine_writes_to_jsonl() {
    let dir = make_tempdir("quarantine");
    copy_acme_assets(&dir);

    fs::write(
        dir.join("recipe.yaml"),
        r#"version: 1
name: quarantine_test
model: ./acme.yaml
source:
  driver: csv
  path: ./inputs.csv
columns:
  - { source: Channel, dimension: Channel }
  - { source: Spend, measure: Spend, type: f64 }
defaults:
  Scenario: Baseline
  Version: Working
  Time: Jan_2026
  Market: Tampa
on_error: quarantine
"#,
    )
    .unwrap();
    fs::write(
        dir.join("inputs.csv"),
        "Channel,Spend\nNotAChannel,1.0\nDisplay,2.0\n",
    )
    .unwrap();

    let prepared = Tessera::prepare(&dir.join("recipe.yaml")).unwrap();
    let report = Tessera::apply(prepared).unwrap();
    assert_eq!(report.rows_written, 1);
    assert_eq!(report.rows_failed, 1);

    let sidecar = Sidecar::at_model_dir(&dir).unwrap();
    let quarantine_path = sidecar.quarantine_path(&report.import_id);
    let body = fs::read_to_string(quarantine_path).unwrap();
    assert!(body.contains("NotAChannel"), "quarantine: {body}");
}

#[test]
fn abort_fails_fast_no_partial_commit() {
    let dir = make_tempdir("abort");
    copy_acme_assets(&dir);
    fs::write(
        dir.join("recipe.yaml"),
        r#"version: 1
name: abort_test
model: ./acme.yaml
source:
  driver: csv
  path: ./inputs.csv
columns:
  - { source: Channel, dimension: Channel }
  - { source: Spend, measure: Spend, type: f64 }
defaults:
  Scenario: Baseline
  Version: Working
  Time: Jan_2026
  Market: Tampa
on_error: abort
"#,
    )
    .unwrap();
    fs::write(
        dir.join("inputs.csv"),
        "Channel,Spend\nDisplay,1.0\nNotAChannel,2.0\n",
    )
    .unwrap();

    let prepared = Tessera::prepare(&dir.join("recipe.yaml")).unwrap();
    let err = Tessera::apply(prepared).unwrap_err();
    assert!(matches!(err, TesseraError::AbortedImport { .. }));

    // Manifest should NOT contain any active import for this run.
    let sidecar = Sidecar::at_model_dir(&dir).unwrap();
    let manifest = mc_tessera::read_manifest(&sidecar).unwrap();
    assert!(
        manifest.imports.is_empty(),
        "abort path leaked an active import: {manifest:?}"
    );
}

fn copy_acme_assets(dir: &Path) {
    let examples_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("mc-model")
        .join("examples");
    fs::copy(examples_dir.join("acme.yaml"), dir.join("acme.yaml")).unwrap();
    fs::copy(
        examples_dir.join("acme.inputs.csv"),
        dir.join("acme.inputs.csv"),
    )
    .unwrap();
}

fn make_tempdir(label: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("mc-tessera-test-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    base
}
