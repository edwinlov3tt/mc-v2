//! `Tessera::rollback` marks an import inactive in
//! `active-imports.json`; subsequent `Tessera::load_active` skips it
//! and the cube state matches the pre-import state exactly.

use std::fs;
use std::path::Path;

use mc_core::ScalarValue;
use mc_tessera::Tessera;

#[test]
fn rollback_restores_pre_import_state() {
    let dir = make_tempdir("rollback");
    copy_acme_assets(&dir);

    fs::write(
        dir.join("recipe.yaml"),
        r#"version: 1
name: tampa_only
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
    fs::write(dir.join("inputs.csv"), "Spend\n12345.0\n").unwrap();

    // Apply.
    let prepared = Tessera::prepare(&dir.join("recipe.yaml")).unwrap();
    let report = Tessera::apply(prepared).unwrap();
    assert_eq!(report.rows_written, 1);
    let import_id = report.import_id.clone();

    // Verify the cell is present via load_active.
    let loaded = Tessera::load_active(&dir.join("acme.yaml")).unwrap();
    let mut cube = loaded.cube;
    let principal = loaded.principal;
    let coord = name_coord(&loaded.refs);
    let cell = cube.read(&coord, principal).unwrap();
    assert!(matches!(cell.value, ScalarValue::F64(v) if (v - 12345.0).abs() < 1e-9));

    // Rollback.
    Tessera::rollback(&dir, &import_id).unwrap();

    // Reload — the cell should now be absent (cube returns Default).
    let loaded2 = Tessera::load_active(&dir.join("acme.yaml")).unwrap();
    let mut cube2 = loaded2.cube;
    let coord2 = name_coord(&loaded2.refs);
    let cell2 = cube2.read(&coord2, loaded2.principal).unwrap();
    // After rollback there are no active imports → no input cells →
    // read returns ScalarValue::Null (Default provenance) per the
    // kernel's missing-cell semantics.
    assert!(matches!(cell2.value, ScalarValue::Null));
}

#[test]
fn rollback_unknown_import_id_errors() {
    let dir = make_tempdir("rollback_unknown");
    copy_acme_assets(&dir);
    let r = Tessera::rollback(&dir, "imp_does_not_exist_8675309");
    assert!(r.is_err());
}

fn name_coord(refs: &mc_model::ModelRefs) -> mc_core::CellCoordinate {
    let mut names = std::collections::BTreeMap::new();
    names.insert("Scenario".into(), "Baseline".into());
    names.insert("Version".into(), "Working".into());
    names.insert("Time".into(), "Jan_2026".into());
    names.insert("Channel".into(), "Paid_Search".into());
    names.insert("Market".into(), "Tampa".into());
    names.insert("Measure".into(), "Spend".into());
    refs.coord_from_names(&names).unwrap()
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
