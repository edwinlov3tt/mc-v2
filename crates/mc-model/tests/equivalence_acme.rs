//! Phase 3C HEADLINE acceptance gate.
//!
//! Compares the Rust-fixture canonical-input path against the YAML+CSV
//! canonical-input path on the Acme cube. Per ADR-0006 Decision 5 +
//! acceptance amendments #12 + (c):
//!
//! - **Path A** (existing reference, unchanged since Phase 1A):
//!   `mc_fixtures::build_acme_cube` + `mc_fixtures::write_canonical_inputs`.
//! - **Path B** (Phase 3C deliverable): `mc_model::parse` →
//!   `mc_model::validate` → `mc_model::resolve_inputs` →
//!   `mc_model::compile` → `mc_model::apply_canonical_inputs`.
//!
//! Both paths must produce **byte-identical store state** at every one
//! of the 2,520 canonical input coordinates AND identical answers to
//! every inline golden test in `acme.yaml`.
//!
//! Per ADR-0006 success-gate item 3: this test uses **only existing
//! public APIs** from `mc-core` + `mc-fixtures` (no new APIs added to
//! either crate). The 2,520 coords are enumerated inline (ADR-0006
//! Decision 5's "default 'enumerate inline' to keep the lock guarantee").

use std::collections::BTreeMap;
use std::path::Path;

use mc_core::{CellCoordinate, ScalarValue};

const ACME_YAML: &str = "examples/acme.yaml";

/// Headline equivalence check across all 2,520 canonical input
/// coordinates.
#[test]
fn yaml_plus_csv_path_matches_rust_fixture_on_canonical_inputs() {
    // ----- Path A: Rust fixture -----
    let (mut cube_a, refs_a) = mc_fixtures::build_acme_cube().expect("build_acme_cube");
    let written =
        mc_fixtures::write_canonical_inputs(&mut cube_a, &refs_a).expect("write_canonical_inputs");
    assert_eq!(written, 2520, "Rust path wrote unexpected row count");

    // ----- Path B: YAML+CSV via the new resolve-inputs stage -----
    let yaml = std::fs::read_to_string(ACME_YAML).expect("read acme.yaml");
    let parsed = mc_model::parse(&yaml, Some(ACME_YAML.to_string())).expect("parse");
    let validated = mc_model::validate(parsed).expect("validate");
    let inputs = mc_model::resolve_inputs(&validated, Path::new(ACME_YAML).parent())
        .expect("resolve_inputs");
    let compiled = mc_model::compile(validated).expect("compile");
    let mut cube_b = compiled.cube;
    let refs_b = compiled.refs;
    let principal_b = compiled.root_principal;
    let count_b = mc_model::apply_canonical_inputs(&mut cube_b, &refs_b, principal_b, &inputs)
        .expect("apply_canonical_inputs");
    assert_eq!(count_b, 2520, "YAML+CSV path applied unexpected row count");

    // ----- Enumerate the 2,520 canonical input coords inline -----
    // 1 scenario × 1 version × 12 months × 5 channels × 7 markets ×
    // 6 input measures = 2,520. Names match acme.yaml + AcmeRefs.
    let scenario_name = "Baseline";
    let version_name = "Working";
    let time_names = [
        "Jan_2026", "Feb_2026", "Mar_2026", "Apr_2026", "May_2026", "Jun_2026", "Jul_2026",
        "Aug_2026", "Sep_2026", "Oct_2026", "Nov_2026", "Dec_2026",
    ];
    let channel_names = ["Paid_Search", "Paid_Social", "Display", "Email", "Organic"];
    let market_names = [
        "Tampa",
        "Orlando",
        "Miami",
        "Atlanta",
        "Charlotte",
        "New_York_City",
        "Boston",
    ];
    let measure_names = ["Spend", "CPC", "CVR", "Close_Rate", "AOV", "COGS_Rate"];

    // Pre-resolve element IDs on each side via the public APIs:
    // - Rust path uses AcmeRefs's positional fields (already public).
    // - YAML path uses ModelRefs::element(dim_name, element_name).
    let acme_ref_for_time = |name: &str| match name {
        "Jan_2026" => refs_a.jan_2026,
        "Feb_2026" => refs_a.feb_2026,
        "Mar_2026" => refs_a.mar_2026,
        "Apr_2026" => refs_a.apr_2026,
        "May_2026" => refs_a.may_2026,
        "Jun_2026" => refs_a.jun_2026,
        "Jul_2026" => refs_a.jul_2026,
        "Aug_2026" => refs_a.aug_2026,
        "Sep_2026" => refs_a.sep_2026,
        "Oct_2026" => refs_a.oct_2026,
        "Nov_2026" => refs_a.nov_2026,
        "Dec_2026" => refs_a.dec_2026,
        _ => panic!("unknown time {name}"),
    };
    let acme_ref_for_channel = |name: &str| match name {
        "Paid_Search" => refs_a.paid_search,
        "Paid_Social" => refs_a.paid_social,
        "Display" => refs_a.display,
        "Email" => refs_a.email,
        "Organic" => refs_a.organic,
        _ => panic!("unknown channel {name}"),
    };
    let acme_ref_for_market = |name: &str| match name {
        "Tampa" => refs_a.tampa,
        "Orlando" => refs_a.orlando,
        "Miami" => refs_a.miami,
        "Atlanta" => refs_a.atlanta,
        "Charlotte" => refs_a.charlotte,
        "New_York_City" => refs_a.new_york_city,
        "Boston" => refs_a.boston,
        _ => panic!("unknown market {name}"),
    };
    let acme_ref_for_measure = |name: &str| match name {
        "Spend" => refs_a.spend,
        "CPC" => refs_a.cpc,
        "CVR" => refs_a.cvr,
        "Close_Rate" => refs_a.close_rate,
        "AOV" => refs_a.aov,
        "COGS_Rate" => refs_a.cogs_rate,
        _ => panic!("unknown measure {name}"),
    };

    let mut compared = 0usize;
    for &t in &time_names {
        for &c in &channel_names {
            for &m in &market_names {
                for &meas in &measure_names {
                    // Path A coord (Rust IDs).
                    let coord_a = mc_fixtures::coord(
                        cube_a.id,
                        &refs_a,
                        refs_a.scen_baseline,
                        refs_a.ver_working,
                        acme_ref_for_time(t),
                        acme_ref_for_channel(c),
                        acme_ref_for_market(m),
                        acme_ref_for_measure(meas),
                    );
                    // Path B coord (YAML IDs).
                    let mut name_map: BTreeMap<String, String> = BTreeMap::new();
                    name_map.insert("Scenario".into(), scenario_name.into());
                    name_map.insert("Version".into(), version_name.into());
                    name_map.insert("Time".into(), t.into());
                    name_map.insert("Channel".into(), c.into());
                    name_map.insert("Market".into(), m.into());
                    name_map.insert("Measure".into(), meas.into());
                    let coord_b: CellCoordinate = refs_b
                        .coord_from_names(&name_map)
                        .expect("YAML coord_from_names");

                    // Read both — these are input cells, so both paths
                    // should return F64 (no Null, no fallback).
                    let val_a = cube_a
                        .read(&coord_a, refs_a.root_principal)
                        .expect("read A");
                    let val_b = cube_b.read(&coord_b, principal_b).expect("read B");

                    match (val_a.value, val_b.value) {
                        (ScalarValue::F64(a), ScalarValue::F64(b)) => {
                            assert_eq!(
                                a.to_bits(),
                                b.to_bits(),
                                "mismatch at ({t}, {c}, {m}, {meas}): \
                                 Rust path = {a}, YAML+CSV path = {b}"
                            );
                        }
                        (a, b) => panic!(
                            "non-F64 value at ({t}, {c}, {m}, {meas}): \
                             Rust path = {a:?}, YAML+CSV path = {b:?}"
                        ),
                    }
                    compared += 1;
                }
            }
        }
    }
    assert_eq!(
        compared, 2520,
        "did not iterate the full canonical-input set"
    );
}

/// Headline equivalence check across all 9 inline goldens.
#[test]
fn yaml_plus_csv_path_matches_rust_fixture_on_inline_goldens() {
    // Same setup as the canonical-inputs test, but compare per-golden
    // values instead of per-coord values.
    let (mut cube_a, refs_a) = mc_fixtures::build_acme_cube().expect("build_acme_cube");
    mc_fixtures::write_canonical_inputs(&mut cube_a, &refs_a).expect("write_canonical_inputs");

    let yaml = std::fs::read_to_string(ACME_YAML).expect("read acme.yaml");
    let parsed = mc_model::parse(&yaml, Some(ACME_YAML.to_string())).expect("parse");
    let validated_for_compile = mc_model::validate(parsed.clone()).expect("validate");
    let inputs = mc_model::resolve_inputs(&validated_for_compile, Path::new(ACME_YAML).parent())
        .expect("resolve_inputs");
    let compiled = mc_model::compile(validated_for_compile).expect("compile");
    let mut cube_b = compiled.cube;
    let refs_b = compiled.refs;
    let principal_b = compiled.root_principal;
    mc_model::apply_canonical_inputs(&mut cube_b, &refs_b, principal_b, &inputs)
        .expect("apply_canonical_inputs");

    // Read the goldens from the parsed YAML directly (so the test stays
    // stable if `acme.yaml`'s golden list grows / shrinks).
    let validated = mc_model::validate(parsed).expect("validate (re)");
    assert!(
        !validated.parsed.golden_tests.is_empty(),
        "acme.yaml must declare at least one golden"
    );

    for golden in &validated.parsed.golden_tests {
        // Build path-A coord by name resolution against AcmeRefs. We
        // re-use the same name → ElementId lookup table form as the
        // canonical-inputs test, but generalized to handle consolidated
        // elements (e.g., Q1_2026, Paid_Search) that appear only in
        // goldens (not in canonical inputs).
        let coord_a = build_acme_coord_from_names(&cube_a, &refs_a, &golden.coord)
            .expect("acme coord from golden names");
        let coord_b = refs_b
            .coord_from_names(&golden.coord)
            .expect("yaml coord from golden names");

        let val_a = cube_a
            .read(&coord_a, refs_a.root_principal)
            .unwrap_or_else(|e| panic!("Rust path read failed for golden {:?}: {e}", golden.name));
        let val_b = cube_b
            .read(&coord_b, principal_b)
            .unwrap_or_else(|e| panic!("YAML path read failed for golden {:?}: {e}", golden.name));

        match (val_a.value, val_b.value) {
            (ScalarValue::F64(a), ScalarValue::F64(b)) => {
                // Use a tight epsilon, not bit-equality — derived
                // golden values can carry floating-point noise after
                // hierarchy rollups even when both paths follow the
                // same arithmetic sequence. Both paths read the SAME
                // input bits and traverse the SAME rule chain, so
                // they should agree to ~1e-9 if they agree at all.
                let delta = (a - b).abs();
                assert!(
                    delta < 1e-9,
                    "golden {:?}: Rust path = {a}, YAML+CSV path = {b}, Δ = {delta}",
                    golden.name
                );
            }
            (a, b) => panic!(
                "non-F64 result for golden {:?}: Rust = {a:?}, YAML+CSV = {b:?}",
                golden.name
            ),
        }
    }
}

/// Resolve a name-keyed coord against the Acme Rust refs. Uses the same
/// public `mc_fixtures::coord` builder as the rest of the fixture; the
/// per-name lookup is a trivial match table over `AcmeRefs`'s public
/// fields.
fn build_acme_coord_from_names(
    cube: &mc_core::Cube,
    refs: &mc_fixtures::AcmeRefs,
    names: &BTreeMap<String, String>,
) -> Option<CellCoordinate> {
    let scen = match names.get("Scenario")?.as_str() {
        "Baseline" => refs.scen_baseline,
        "Aggressive" => refs.scen_aggressive,
        "Conservative" => refs.scen_conservative,
        _ => return None,
    };
    let ver = match names.get("Version")?.as_str() {
        "Working" => refs.ver_working,
        "Submitted" => refs.ver_submitted,
        "Approved" => refs.ver_approved,
        _ => return None,
    };
    let time = match names.get("Time")?.as_str() {
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
        "Q1_2026" => refs.q1_2026,
        "Q2_2026" => refs.q2_2026,
        "Q3_2026" => refs.q3_2026,
        "Q4_2026" => refs.q4_2026,
        "FY_2026" => refs.fy_2026,
        _ => return None,
    };
    let chan = match names.get("Channel")?.as_str() {
        "Paid_Search" => refs.paid_search,
        "Paid_Social" => refs.paid_social,
        "Display" => refs.display,
        "Email" => refs.email,
        "Organic" => refs.organic,
        "Paid_Media" => refs.paid_media,
        "Owned_Earned" => refs.owned_earned,
        "All_Channels" => refs.all_channels,
        _ => return None,
    };
    let market = match names.get("Market")?.as_str() {
        "Tampa" => refs.tampa,
        "Orlando" => refs.orlando,
        "Miami" => refs.miami,
        "Atlanta" => refs.atlanta,
        "Charlotte" => refs.charlotte,
        "New_York_City" => refs.new_york_city,
        "Boston" => refs.boston,
        "Florida" => refs.florida,
        "Georgia" => refs.georgia,
        "North_Carolina" => refs.north_carolina,
        "New_York_State" => refs.new_york_state,
        "Massachusetts" => refs.massachusetts,
        "Southeast" => refs.southeast,
        "Northeast" => refs.northeast,
        "USA" => refs.usa,
        _ => return None,
    };
    let measure = match names.get("Measure")?.as_str() {
        "Spend" => refs.spend,
        "CPC" => refs.cpc,
        "CVR" => refs.cvr,
        "Close_Rate" => refs.close_rate,
        "AOV" => refs.aov,
        "COGS_Rate" => refs.cogs_rate,
        "Clicks" => refs.clicks,
        "Leads" => refs.leads,
        "Customers" => refs.customers,
        "Revenue" => refs.revenue,
        "Gross_Profit" => refs.gross_profit,
        _ => return None,
    };
    Some(mc_fixtures::coord(
        cube.id, refs, scen, ver, time, chan, market, measure,
    ))
}
