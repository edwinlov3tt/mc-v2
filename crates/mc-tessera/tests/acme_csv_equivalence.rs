//! HEADLINE ACCEPTANCE TEST — Phase 5A Stream D acceptance gate #1 + #6.
//!
//! Per ADR-0010 Decision 1 item 6 + Stream D handoff §11: this test
//! proves that ingesting the Acme canonical inputs through a Tessera
//! recipe produces **byte-identical** cube state to the Rust-fixture
//! reference, `mc_fixtures::write_canonical_inputs()`.
//!
//! ## Phase 5A.1: long-format recipe reads the actual `acme.inputs.csv`
//!
//! `crates/mc-model/examples/acme.inputs.csv` is committed in **long
//! format** — 7 columns (`Scenario, Version, Time, Channel, Market,
//! Measure, value`) with the Measure dim NAME carried in a column and
//! each row representing one cell (2,520 rows total).
//!
//! Per ADR-0010 Amendment 2, this test uses a `format: long` recipe that
//! reads the actual `acme.inputs.csv` directly, with
//! `long_format: { measure_column: Measure, value_column: value }` and
//! dimension columns for Scenario/Version/Time/Channel/Market.

use std::fs;
use std::path::Path;

use mc_core::{CellCoordinate, ScalarValue};
use mc_fixtures::{build_acme_cube, canonical_inputs_for, write_canonical_inputs, AcmeRefs};
use mc_model::ModelRefs;
use mc_tessera::Tessera;

#[test]
fn acme_csv_equivalence() {
    // ------------------------------------------------------------------
    // 1. Workspace: a fresh tempdir under `target/` + a copy of the Acme
    //    YAML + the actual long-format `acme.inputs.csv`.
    // ------------------------------------------------------------------
    let tempdir = make_tempdir("acme_csv_equivalence");
    let model_dst = tempdir.join("acme.yaml");
    let examples_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("mc-model")
        .join("examples");
    let model_src = examples_dir.join("acme.yaml");
    fs::copy(model_src, &model_dst).expect("copy acme.yaml");
    // Copy the actual long-format CSV (used by both the model's
    // canonical_inputs resolver AND the long-format recipe).
    fs::copy(
        examples_dir.join("acme.inputs.csv"),
        tempdir.join("acme.inputs.csv"),
    )
    .expect("copy acme.inputs.csv");

    // ------------------------------------------------------------------
    // 2. Long-format recipe targeting the actual `acme.inputs.csv`.
    //    All 5 dimensions come from source columns; measures come from
    //    the `Measure` column; values from the `value` column.
    // ------------------------------------------------------------------
    let recipe_path = tempdir.join("acme-import.recipe.yaml");
    fs::write(&recipe_path, ACME_RECIPE_YAML).expect("write recipe");

    // ------------------------------------------------------------------
    // 4. Run Tessera apply.
    // ------------------------------------------------------------------
    let prepared = Tessera::prepare(&recipe_path).expect("prepare");
    let report = Tessera::apply(prepared).expect("apply");
    assert_eq!(
        report.rows_written, 2_520,
        "expected 2520 cells (420 rows × 6 measures), got {}",
        report.rows_written
    );
    assert_eq!(report.rows_failed, 0);

    // ------------------------------------------------------------------
    // 5. Build the gold-standard cube via the existing Rust fixture.
    // ------------------------------------------------------------------
    let (mut gold, gold_refs) = build_acme_cube().expect("gold build");
    let count = write_canonical_inputs(&mut gold, &gold_refs).expect("gold write");
    assert_eq!(count, 2_520);

    // ------------------------------------------------------------------
    // 6. Re-load the Tessera-imported cube via Tessera::load_active so
    //    we exercise the replay path AND get a populated cube to
    //    compare against the gold cube.
    // ------------------------------------------------------------------
    let loaded = Tessera::load_active(&model_dst).expect("load_active");
    let mut tessera_cube = loaded.cube;
    let tessera_refs = loaded.refs;
    let principal = loaded.principal;

    // ------------------------------------------------------------------
    // 7. Cell-by-cell byte-identity comparison. Walk the (Scenario,
    //    Version, Time, Channel, Market, Measure) cartesian product
    //    that `write_canonical_inputs` writes to. For each gold coord,
    //    look up the Tessera coord by element NAME (the IDs differ
    //    across cubes because each cube has its own IdGenerator) and
    //    compare ScalarValue::F64 via `to_bits()`.
    // ------------------------------------------------------------------
    let gold_root = gold_refs.root_principal;
    let gold_cube_id = gold.id;
    let mut compared = 0usize;
    for (time_idx, time_name) in TIME_NAMES.iter().enumerate() {
        for (channel_idx, channel_name) in CHANNEL_NAMES.iter().enumerate() {
            for (market_idx, market_name) in MARKET_NAMES.iter().enumerate() {
                let inputs = canonical_inputs_for(
                    (time_idx + 1) as u32,
                    channel_idx as u32,
                    market_idx as u32,
                );
                for (measure_name, expected) in [
                    ("Spend", inputs.spend),
                    ("CPC", inputs.cpc),
                    ("CVR", inputs.cvr),
                    ("Close_Rate", inputs.close_rate),
                    ("AOV", inputs.aov),
                    ("COGS_Rate", inputs.cogs_rate),
                ] {
                    let gold_coord = name_coord_gold(
                        gold_cube_id,
                        &gold_refs,
                        "Baseline",
                        "Working",
                        time_name,
                        channel_name,
                        market_name,
                        measure_name,
                    );
                    let tessera_coord = name_coord_tessera(
                        &tessera_refs,
                        "Baseline",
                        "Working",
                        time_name,
                        channel_name,
                        market_name,
                        measure_name,
                    );

                    let g = gold.read(&gold_coord, gold_root).expect("gold cell").value;
                    let t = tessera_cube
                        .read(&tessera_coord, principal)
                        .expect("tessera cell")
                        .value;

                    let g_f = scalar_to_f64(&g);
                    let t_f = scalar_to_f64(&t);

                    assert_eq!(
                        g_f.to_bits(),
                        t_f.to_bits(),
                        "byte mismatch at {time_name}/{channel_name}/{market_name}/{measure_name}: \
                         gold={g_f:?} (bits {:#x}) tessera={t_f:?} (bits {:#x}) expected={expected:?}",
                        g_f.to_bits(),
                        t_f.to_bits(),
                    );
                    compared += 1;
                }
            }
        }
    }
    assert_eq!(compared, 2_520, "expected 2520 cell comparisons");
}

const TIME_NAMES: [&str; 12] = [
    "Jan_2026", "Feb_2026", "Mar_2026", "Apr_2026", "May_2026", "Jun_2026", "Jul_2026", "Aug_2026",
    "Sep_2026", "Oct_2026", "Nov_2026", "Dec_2026",
];
const CHANNEL_NAMES: [&str; 5] = ["Paid_Search", "Paid_Social", "Display", "Email", "Organic"];
const MARKET_NAMES: [&str; 7] = [
    "Tampa",
    "Orlando",
    "Miami",
    "Atlanta",
    "Charlotte",
    "New_York_City",
    "Boston",
];

const ACME_RECIPE_YAML: &str = r#"version: 1
name: acme_csv_equivalence
description: "Phase 5A.1 long-format test recipe — reads the actual acme.inputs.csv."
model: ./acme.yaml
source:
  driver: csv
  path: ./acme.inputs.csv
  format: long
  long_format:
    measure_column: Measure
    value_column: value
columns:
  - { source: Scenario, dimension: Scenario }
  - { source: Version, dimension: Version }
  - { source: Time, dimension: Time }
  - { source: Channel, dimension: Channel }
  - { source: Market, dimension: Market }
defaults: {}
write_disposition: replace
incremental: false
batch:
  size: 500
on_error: abort
on_missing_element: error
"#;

#[allow(clippy::too_many_arguments)]
fn name_coord_gold(
    cube_id: mc_core::CubeId,
    refs: &AcmeRefs,
    scenario: &str,
    version: &str,
    time: &str,
    channel: &str,
    market: &str,
    measure: &str,
) -> CellCoordinate {
    let scen = match scenario {
        "Baseline" => refs.scen_baseline,
        other => panic!("unknown scenario {other:?}"),
    };
    let ver = match version {
        "Working" => refs.ver_working,
        other => panic!("unknown version {other:?}"),
    };
    let t = match time {
        "Jan_2026" => refs.jan_2026,
        "Feb_2026" => refs.feb_2026,
        "Mar_2026" => refs.mar_2026,
        "Apr_2026" => refs.apr_2026,
        "May_2026" => refs.may_2026,
        "Jun_2026" => refs.jun_2026,
        "Jul_2026" => refs.jul_2026,
        "Aug_2026" => refs.aug_2026,
        "Sep_2026" => refs.sep_2026,
        "Oct_2026" => refs.oct_2026,
        "Nov_2026" => refs.nov_2026,
        "Dec_2026" => refs.dec_2026,
        other => panic!("unknown time {other:?}"),
    };
    let c = match channel {
        "Paid_Search" => refs.paid_search,
        "Paid_Social" => refs.paid_social,
        "Display" => refs.display,
        "Email" => refs.email,
        "Organic" => refs.organic,
        other => panic!("unknown channel {other:?}"),
    };
    let m = match market {
        "Tampa" => refs.tampa,
        "Orlando" => refs.orlando,
        "Miami" => refs.miami,
        "Atlanta" => refs.atlanta,
        "Charlotte" => refs.charlotte,
        "New_York_City" => refs.new_york_city,
        "Boston" => refs.boston,
        other => panic!("unknown market {other:?}"),
    };
    let me = match measure {
        "Spend" => refs.spend,
        "CPC" => refs.cpc,
        "CVR" => refs.cvr,
        "Close_Rate" => refs.close_rate,
        "AOV" => refs.aov,
        "COGS_Rate" => refs.cogs_rate,
        other => panic!("unknown measure {other:?}"),
    };
    mc_fixtures::coord(cube_id, refs, scen, ver, t, c, m, me)
}

fn name_coord_tessera(
    refs: &ModelRefs,
    scenario: &str,
    version: &str,
    time: &str,
    channel: &str,
    market: &str,
    measure: &str,
) -> CellCoordinate {
    let mut names = std::collections::BTreeMap::new();
    names.insert("Scenario".to_string(), scenario.to_string());
    names.insert("Version".to_string(), version.to_string());
    names.insert("Time".to_string(), time.to_string());
    names.insert("Channel".to_string(), channel.to_string());
    names.insert("Market".to_string(), market.to_string());
    names.insert("Measure".to_string(), measure.to_string());
    refs.coord_from_names(&names)
        .unwrap_or_else(|| panic!("coord lookup failed for tessera refs"))
}

fn scalar_to_f64(v: &ScalarValue) -> f64 {
    match v {
        ScalarValue::F64(x) => *x,
        ScalarValue::Null => f64::NAN,
        other => panic!("unexpected ScalarValue: {other:?}"),
    }
}

fn make_tempdir(label: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("mc-tessera-test-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("create tempdir");
    base
}
