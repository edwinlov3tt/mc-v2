//! Phase 6A.1 MAJ-1 regression test: `time_format` is consumed at row
//! transform time so non-ISO date columns land at the correct Time
//! element instead of being silently dropped or matched as-is.
//!
//! See `docs/handoffs/phase-6a-1-fixes-handoff.md` §"Block 1.2".

use std::collections::BTreeMap;
use std::fs;

use mc_core::ScalarValue;
use mc_tessera::Tessera;

/// Minimal model with a single Time dim (monthly granularity) + one
/// measure. The Time dim's elements are ISO `YYYY-MM` strings; the
/// import CSV has US-locale `MM/DD/YYYY` dates that must be parsed +
/// canonicalized to month before matching.
const MODEL_YAML: &str = r#"
model_format_version: 1
metadata:
  name: "TimeFormatTest"
  description: "Phase 6A.1 MAJ-1 regression"
  author: "test"
  created: "2026-05-05"
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    elements:
      - { name: "Base", scenario_meta: "Default" }
  - name: "Version"
    kind: "Version"
    elements:
      - { name: "Working", version_state: "Draft" }
  - name: "Time"
    kind: "Time"
    granularity: "month"
    elements:
      - { name: "2026-01" }
      - { name: "2026-02" }
      - { name: "2026-03" }
  - name: "Channel"
    kind: "Standard"
    elements:
      - { name: "Web" }
  - name: "Market"
    kind: "Standard"
    elements:
      - { name: "US" }
  - name: "Measure"
    kind: "Measure"
    elements: []
measures:
  - { name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }
rules: []
"#;

const CSV: &str = "\
date,spend
01/15/2026,100.0
02/03/2026,200.0
03/27/2026,300.0
";

/// Recipe: declare US-locale `time_format` + `map_to_period: month` so
/// the dates get parsed and canonicalized to `YYYY-MM`. Without the
/// Phase 6A.1 fix, `time_format` would be ignored and the raw
/// `01/15/2026` strings would fail to match any Time element (or land
/// in the dynamic-create namespace under `on_missing_element: create`).
fn recipe_yaml(model_relpath: &str, csv_relpath: &str) -> String {
    format!(
        r#"version: 1
name: time_format_test
description: "Phase 6A.1 MAJ-1 regression"
model: "{model_relpath}"
source:
  driver: csv
  path: "{csv_relpath}"
columns:
  - source: date
    dimension: Time
    time_format: "%m/%d/%Y"
    map_to_period: month
  - source: spend
    measure: Spend
defaults:
  Scenario: Base
  Version: Working
  Channel: Web
  Market: US
write_disposition: replace
on_error: abort
on_missing_element: error
"#
    )
}

#[test]
fn time_format_parses_us_locale_dates_and_lands_at_correct_time_element() {
    let tempdir = make_tempdir("time_format_us_locale");
    let model_path = tempdir.join("model.yaml");
    let csv_path = tempdir.join("inputs.csv");
    let recipe_path = tempdir.join("recipe.yaml");

    fs::write(model_path.as_path(), MODEL_YAML).expect("write model");
    fs::write(csv_path.as_path(), CSV).expect("write csv");
    fs::write(&recipe_path, recipe_yaml("model.yaml", "inputs.csv")).expect("write recipe");

    let prepared = Tessera::prepare(&recipe_path).expect("prepare");
    let report = Tessera::apply(prepared).expect("apply");
    assert_eq!(
        report.rows_written, 3,
        "expected 3 cells (one per CSV row), got {} (failures: {})",
        report.rows_written, report.rows_failed
    );
    assert_eq!(report.rows_failed, 0, "no rows should fail with the fix");

    // Reload via load_active and verify each cell landed at the
    // expected (canonicalized) Time element.
    let loaded = Tessera::load_active(&model_path).expect("load_active");
    let mut cube = loaded.cube;
    let refs = loaded.refs;
    let principal = loaded.principal;

    for (time_name, expected) in [
        ("2026-01", 100.0_f64),
        ("2026-02", 200.0),
        ("2026-03", 300.0),
    ] {
        let mut slots = BTreeMap::new();
        slots.insert("Scenario".to_string(), "Base".to_string());
        slots.insert("Version".to_string(), "Working".to_string());
        slots.insert("Time".to_string(), time_name.to_string());
        slots.insert("Channel".to_string(), "Web".to_string());
        slots.insert("Market".to_string(), "US".to_string());
        slots.insert("Measure".to_string(), "Spend".to_string());
        let coord = refs
            .coord_from_names(&slots)
            .unwrap_or_else(|| panic!("coord_from_names failed for Time={time_name}"));
        let cell = cube
            .read(&coord, principal)
            .unwrap_or_else(|e| panic!("read failed at Time={time_name}: {e}"));
        match cell.value {
            ScalarValue::F64(v) => {
                assert!(
                    (v - expected).abs() < 1e-9,
                    "Time={time_name}: got {v}, expected {expected}",
                );
            }
            other => panic!("expected F64 at Time={time_name}, got {other:?}"),
        }
    }
}

#[test]
fn time_format_parse_error_records_per_row_failure() {
    // CSV has one valid row and one row whose date doesn't match the
    // declared format — verify the bad row produces a per-row failure
    // with the MC5034 code rather than silently succeeding or aborting
    // the whole batch (default on_error: abort would still abort, so
    // we use skip_row).
    const BAD_CSV: &str = "\
date,spend
01/15/2026,100.0
not-a-date,200.0
";
    let tempdir = make_tempdir("time_format_parse_error");
    let model_path = tempdir.join("model.yaml");
    let csv_path = tempdir.join("inputs.csv");
    let recipe_path = tempdir.join("recipe.yaml");

    fs::write(model_path.as_path(), MODEL_YAML).expect("write model");
    fs::write(csv_path.as_path(), BAD_CSV).expect("write csv");
    let recipe = String::from(
        r#"version: 1
name: time_format_parse_error
description: "Phase 6A.1 MAJ-1: per-row failure on bad date"
model: model.yaml
source:
  driver: csv
  path: inputs.csv
columns:
  - source: date
    dimension: Time
    time_format: "%m/%d/%Y"
    map_to_period: month
  - source: spend
    measure: Spend
defaults:
  Scenario: Base
  Version: Working
  Channel: Web
  Market: US
write_disposition: replace
on_error: skip_row
on_missing_element: error
"#,
    );
    fs::write(&recipe_path, recipe).expect("write recipe");

    let prepared = Tessera::prepare(&recipe_path).expect("prepare");
    let report = Tessera::apply(prepared).expect("apply");
    assert_eq!(report.rows_written, 1, "one good row should land");
    assert_eq!(report.rows_failed, 1, "one bad row should be recorded");
}

fn make_tempdir(label: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!(
        "mc-tessera-time-format-{label}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("create tempdir");
    base
}
