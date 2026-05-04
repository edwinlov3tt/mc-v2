//! Dry-run validation produces MC5xxx diagnostics for broken recipes.

use std::fs;
use std::path::Path;

use mc_recipe::{diagnostics_to_json, sort_diagnostics};
use mc_tessera::{Tessera, TesseraError};

#[test]
fn dry_run_succeeds_on_valid_recipe() {
    let dir = make_tempdir("dry_run_ok");
    copy_acme_assets(&dir);
    fs::write(dir.join("recipe.yaml"), VALID_RECIPE).expect("write recipe");
    fs::write(dir.join("inputs.csv"), "Channel,Spend\nPaid_Search,1\n").expect("write csv");

    let prepared = Tessera::prepare(&dir.join("recipe.yaml")).expect("prepare");
    let report = Tessera::dry_run(&prepared).expect("dry-run");
    assert_eq!(report.recipe_name, "valid_dry_run");
    assert!(report.mapped_columns >= 2);
    assert!(report.diagnostics.is_empty());
}

#[test]
fn dry_run_emits_mc5004_for_unknown_dimension() {
    let dir = make_tempdir("dry_run_mc5004");
    copy_acme_assets(&dir);
    fs::write(dir.join("recipe.yaml"), BAD_DIM_RECIPE).expect("write recipe");
    fs::write(dir.join("inputs.csv"), "Region,Spend\nx,1\n").expect("write csv");

    let err = Tessera::prepare(&dir.join("recipe.yaml")).unwrap_err();
    let diags = match err {
        TesseraError::Recipe { ref errors } => {
            errors.iter().map(|e| e.to_diagnostic()).collect::<Vec<_>>()
        }
        other => panic!("expected Recipe error, got {other:?}"),
    };
    assert!(diags.iter().any(|d| d.code == "MC5004"));

    // Envelope renders with schema_version 1.0.
    let mut sorted = diags.clone();
    sort_diagnostics(&mut sorted);
    let json = diagnostics_to_json(&sorted);
    assert!(json.contains("\"schema_version\": \"1.0\""));
}

#[test]
fn dry_run_emits_mc5018_for_derived_measure() {
    let dir = make_tempdir("dry_run_mc5018");
    copy_acme_assets(&dir);
    fs::write(dir.join("recipe.yaml"), DERIVED_MEASURE_RECIPE).expect("write recipe");
    fs::write(dir.join("inputs.csv"), "Channel,Clicks\nPaid_Search,1\n").expect("write csv");

    let err = Tessera::prepare(&dir.join("recipe.yaml")).unwrap_err();
    let diags = match err {
        TesseraError::Recipe { ref errors } => {
            errors.iter().map(|e| e.to_diagnostic()).collect::<Vec<_>>()
        }
        other => panic!("expected Recipe error, got {other:?}"),
    };
    assert!(
        diags.iter().any(|d| d.code == "MC5018"),
        "expected MC5018 (derived measure rejection), got {diags:?}",
    );
}

const VALID_RECIPE: &str = r#"version: 1
name: valid_dry_run
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
"#;

const BAD_DIM_RECIPE: &str = r#"version: 1
name: bad_dim
model: ./acme.yaml
source:
  driver: csv
  path: ./inputs.csv
columns:
  - { source: Region, dimension: Region }
  - { source: Spend, measure: Spend, type: f64 }
defaults:
  Scenario: Baseline
  Version: Working
  Time: Jan_2026
  Channel: Paid_Search
  Market: Tampa
"#;

const DERIVED_MEASURE_RECIPE: &str = r#"version: 1
name: derived
model: ./acme.yaml
source:
  driver: csv
  path: ./inputs.csv
columns:
  - { source: Channel, dimension: Channel }
  - { source: Clicks, measure: Clicks, type: f64 }
defaults:
  Scenario: Baseline
  Version: Working
  Time: Jan_2026
  Market: Tampa
"#;

fn copy_acme_assets(dir: &Path) {
    let examples_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("mc-model")
        .join("examples");
    fs::copy(examples_dir.join("acme.yaml"), dir.join("acme.yaml")).expect("copy acme.yaml");
    fs::copy(
        examples_dir.join("acme.inputs.csv"),
        dir.join("acme.inputs.csv"),
    )
    .expect("copy acme.inputs.csv");
}

fn make_tempdir(label: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("mc-tessera-test-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("create tempdir");
    base
}
