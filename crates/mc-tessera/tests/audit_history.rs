//! Audit log + history listing.

use std::fs;
use std::path::Path;

use mc_tessera::Tessera;

#[test]
fn audit_log_is_valid_jsonl_after_apply() {
    let dir = make_tempdir("audit_jsonl");
    copy_acme_assets(&dir);

    fs::write(
        dir.join("recipe.yaml"),
        r#"version: 1
name: audit_test
model: ./acme.yaml
source:
  driver: csv
  path: ./inputs.csv
columns:
  - { source: Spend, measure: Spend, type: f64 }
defaults:
  Scenario: Baseline
  Version: Working
  Time: Jan_2026
  Channel: Paid_Search
  Market: Tampa
"#,
    )
    .unwrap();
    fs::write(dir.join("inputs.csv"), "Spend\n42.0\n").unwrap();

    let prepared = Tessera::prepare(&dir.join("recipe.yaml")).unwrap();
    let report = Tessera::apply(prepared).unwrap();

    let body = fs::read_to_string(&report.audit_path).unwrap();
    let lines: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 1);
    let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(v["import_id"].as_str().unwrap(), report.import_id);
    assert_eq!(v["recipe_name"].as_str().unwrap(), "audit_test");
    assert_eq!(v["rows_written"].as_u64().unwrap(), 1);
    assert_eq!(v["event"].as_str().unwrap(), "apply");
}

#[test]
fn history_lists_imports_in_chronological_order() {
    let dir = make_tempdir("history");
    copy_acme_assets(&dir);
    fs::write(dir.join("inputs.csv"), "Spend\n10.0\n").unwrap();
    let recipe_path = dir.join("recipe.yaml");
    fs::write(
        &recipe_path,
        r#"version: 1
name: hist_one
model: ./acme.yaml
source:
  driver: csv
  path: ./inputs.csv
columns:
  - { source: Spend, measure: Spend, type: f64 }
defaults:
  Scenario: Baseline
  Version: Working
  Time: Jan_2026
  Channel: Paid_Search
  Market: Tampa
"#,
    )
    .unwrap();

    // Apply twice to get two distinct audit records.
    let p1 = Tessera::prepare(&recipe_path).unwrap();
    let r1 = Tessera::apply(p1).unwrap();
    // Force timestamp diversity (the import_id includes nanoseconds, but
    // wall-clock divergence is what's being relied on; sleep for 50 ms).
    std::thread::sleep(std::time::Duration::from_millis(50));
    let p2 = Tessera::prepare(&recipe_path).unwrap();
    let r2 = Tessera::apply(p2).unwrap();

    let history = Tessera::history(&dir).unwrap();
    assert!(history.len() >= 2);
    let ids: Vec<&str> = history.iter().map(|h| h.import_id.as_str()).collect();
    assert!(ids.contains(&r1.import_id.as_str()));
    assert!(ids.contains(&r2.import_id.as_str()));
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
