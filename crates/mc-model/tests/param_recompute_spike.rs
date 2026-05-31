//! Phase 10C.0 — Param-Recompute Spike (gates all of Phase 10C).
//!
//! SPIKE CODE, not shipped product. This test exists to answer ONE
//! question that gates whether `mc model backtest` (Phase 10C.1) is a CLI
//! composition or a kernel change:
//!
//!   When you change a `param(name)` value and re-read a Derived measure
//!   whose rule depends on it, does the value MOVE — or serve stale cache?
//!
//! See `docs/handoffs/phase-10c0-param-recompute-spike-handoff.md` and
//! `docs/reports/phase-10c0-spike-report.md` for the verdict.
//!
//! Sub-questions answered here:
//!   (a) SETTER: is there an in-place `param(name)` override API on a
//!       loaded cube? Finding: no dedicated `set_parameter`, but
//!       `Cube.reference_data.parameters` is a `pub` field, so a caller
//!       CAN mutate it in place (`AHashMap<String, f64>`).
//!   (b) RECOMPUTE/CACHE: after mutating the param in place and re-reading,
//!       does the derived cell recompute, or serve a cached value? Finding:
//!       it serves STALE cache (see assertions below). cube.rs:3069 —
//!       params "don't participate in dirty propagation," so the dirty bit
//!       that normally busts the derived-leaf / consolidated caches never
//!       fires on a param change.

use std::collections::BTreeMap;

use mc_core::{CellCoordinate, ScalarValue, WriteIntent, WritebackRequest};
use mc_model::{load_str, CompiledCube, ModelRefs};

const BASE: &str = r#"
model_format_version: 1
metadata:
  name: "P10C0Spike"
  description: "param-recompute spike"
  author: "spike"
  created: "2026-05-31"
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    elements: [{ name: "Base", scenario_meta: "Default" }]
  - name: "Version"
    kind: "Version"
    elements: [{ name: "Working", version_state: "Draft" }]
  - name: "Time"
    kind: "Time"
    elements: [{ name: "P1" }]
  - name: "Channel"
    kind: "Standard"
    elements: [{ name: "Web" }]
  - name: "Market"
    kind: "Standard"
    elements: [{ name: "US" }]
  - name: "Measure"
    kind: "Measure"
    elements: []
"#;

/// A cube whose Derived `should_flag` materially depends on
/// `param(threshold)`: `should_flag = if(signal >= param(threshold), 1, 0)`.
/// With `signal = 0.15`: threshold 0.10 -> 1.0 ; threshold 0.20 -> 0.0.
fn cube_yaml(threshold: f64) -> String {
    format!(
        r#"{BASE}
parameters:
  - name: "threshold"
    value: {threshold}
    description: "decision threshold (the swept axis)"
measures:
  - {{ name: "signal", role: "Input", data_type: "F64", aggregation: "Sum" }}
  - {{ name: "should_flag", role: "Derived", data_type: "F64", aggregation: "Sum" }}
rules:
  - name: "rule_should_flag"
    target_measure: "should_flag"
    scope: "AllLeaves"
    body: 'if(signal >= param(threshold), 1.0, 0.0)'
    declared_dependencies: ["signal"]
"#
    )
}

fn build(yaml: &str) -> CompiledCube {
    load_str(yaml, Some("spike".into())).unwrap_or_else(|errs| {
        for e in &errs {
            eprintln!("  error: {e}");
        }
        panic!("model failed to load");
    })
}

fn coord(refs: &ModelRefs, measure: &str) -> CellCoordinate {
    let map: BTreeMap<String, String> = [
        ("Scenario", "Base"),
        ("Version", "Working"),
        ("Time", "P1"),
        ("Channel", "Web"),
        ("Market", "US"),
        ("Measure", measure),
    ]
    .iter()
    .map(|(d, e)| (d.to_string(), e.to_string()))
    .collect();
    refs.coord_from_names(&map)
        .unwrap_or_else(|| panic!("coord_from_names failed for {measure}"))
}

fn write_signal(
    cube: &mut mc_core::Cube,
    refs: &ModelRefs,
    principal: mc_core::PrincipalId,
    v: f64,
) {
    cube.write(WritebackRequest {
        coord: coord(refs, "signal"),
        new_value: ScalarValue::F64(v),
        principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .unwrap_or_else(|e| panic!("write signal failed: {e}"));
}

fn read_flag(cube: &mut mc_core::Cube, refs: &ModelRefs, principal: mc_core::PrincipalId) -> f64 {
    let cv = cube
        .read(&coord(refs, "should_flag"), principal)
        .unwrap_or_else(|e| panic!("read should_flag failed: {e}"));
    match cv.value {
        ScalarValue::F64(v) => v,
        other => panic!("expected F64, got {other:?}"),
    }
}

/// CONTROL: prove the formula is genuinely param-sensitive. Two cubes
/// compiled fresh from YAML — one with threshold 0.10, one with 0.20 —
/// give different `should_flag` for the same `signal = 0.15`. This
/// isolates the *recompute* question from the *setter* question: the
/// value SHOULD move with the param; the only question is whether an
/// in-place override on a single loaded cube makes it move.
#[test]
fn spike_control_fresh_cubes_show_param_matters() {
    // threshold 0.10, signal 0.15  ->  0.15 >= 0.10  ->  1.0
    let lo = build(&cube_yaml(0.10));
    let mut lo_cube = lo.cube;
    write_signal(&mut lo_cube, &lo.refs, lo.root_principal, 0.15);
    let flag_lo = read_flag(&mut lo_cube, &lo.refs, lo.root_principal);

    // threshold 0.20, signal 0.15  ->  0.15 >= 0.20  ->  0.0
    let hi = build(&cube_yaml(0.20));
    let mut hi_cube = hi.cube;
    write_signal(&mut hi_cube, &hi.refs, hi.root_principal, 0.15);
    let flag_hi = read_flag(&mut hi_cube, &hi.refs, hi.root_principal);

    assert!(
        (flag_lo - 1.0).abs() < 1e-9,
        "threshold 0.10 must flag (1.0), got {flag_lo}"
    );
    assert!(
        (flag_hi - 0.0).abs() < 1e-9,
        "threshold 0.20 must not flag (0.0), got {flag_hi}"
    );
    assert!(
        (flag_lo - flag_hi).abs() > 1e-9,
        "control: the param MUST materially change the derived output"
    );
}

/// SUB-QUESTION (a) SETTER + first half of (b): mutating the param
/// in place via the `pub reference_data.parameters` field BEFORE the
/// first read of the derived cell IS picked up — proving param
/// *resolution* works and the field is mutable. There is no cache yet,
/// so eval reads the fresh value. This is the "no cache to fight" case.
#[test]
fn spike_param_mutation_before_first_read_is_picked_up() {
    let compiled = build(&cube_yaml(0.10));
    let mut cube = compiled.cube;
    let refs = &compiled.refs;
    let principal = compiled.root_principal;

    write_signal(&mut cube, refs, principal, 0.15);

    // In-place override BEFORE any read of should_flag. No setter API
    // exists; we mutate the pub field directly (spike mechanism).
    cube.reference_data
        .parameters
        .insert("threshold".to_string(), 0.20);

    // First-ever read of should_flag -> computes fresh against 0.20.
    let flag = read_flag(&mut cube, refs, principal);
    assert!(
        (flag - 0.0).abs() < 1e-9,
        "param override before first read MUST be honored (0.15 >= 0.20 is false -> 0.0), got {flag}"
    );
}

/// THE GATING TEST — sub-question (b) RECOMPUTE/CACHE, the realistic
/// backtest sweep order: read once (materializes + caches the derived
/// cell), THEN override the param, THEN re-read.
///
/// FINDING: the re-read serves the STALE cached value. A param change
/// does not mark the derived cell dirty (cube.rs:3069) and does not bump
/// the cube revision, so `read_derived_leaf`'s `cached_fresh` check
/// (cube.rs:~516) stays true and returns the pre-override value.
///
/// The assertions below encode the OBSERVED behavior. If a future kernel
/// change makes param overrides bust the cache, `flag_after` would equal
/// `flag_control` (0.0) and these assertions would fail — that failure is
/// the signal that the spike's RED/YELLOW finding has been resolved.
#[test]
fn spike_param_mutation_after_first_read_serves_stale_cache() {
    let compiled = build(&cube_yaml(0.10));
    let mut cube = compiled.cube;
    let refs = &compiled.refs;
    let principal = compiled.root_principal;

    write_signal(&mut cube, refs, principal, 0.15);

    // value A: first read at threshold 0.10 -> 1.0, and it CACHES.
    let flag_before = read_flag(&mut cube, refs, principal);
    assert!(
        (flag_before - 1.0).abs() < 1e-9,
        "baseline: 0.15 >= 0.10 -> 1.0, got {flag_before}"
    );

    // In-place override of the swept axis (the backtest param: mechanism).
    cube.reference_data
        .parameters
        .insert("threshold".to_string(), 0.20);

    // value B: re-read after override. Control says this SHOULD be 0.0.
    let flag_after = read_flag(&mut cube, refs, principal);

    // THE VERDICT in code: the value did NOT move. It served stale cache.
    assert!(
        (flag_after - 1.0).abs() < 1e-9,
        "SPIKE FINDING: expected STALE cache (still 1.0). If this is now \
         0.0, param overrides bust the cache and the spike verdict flips \
         toward GREEN. Got {flag_after}"
    );
    assert!(
        (flag_after - flag_before).abs() < 1e-9,
        "SPIKE FINDING: derived value did NOT move with the in-place param \
         override (stale cache). before={flag_before} after={flag_after}"
    );
}

/// THE GREEN PATH — replicates the EXACT shipped `sweep.rs` loop shape
/// (Phase 6A.3): baseline eval, ONE snapshot, then per sweep point:
/// `rollback_to(snapshot)` -> mutate the override in `reference_data`
/// -> eval. This is precisely how `override_coefficient` sweeps work
/// today (sweep.rs:283-296), and coefficients are `reference_data`
/// fields with the SAME "no dirty propagation" property as parameters.
///
/// `rollback_to` (cube.rs:2801) is the cache-bust: it bumps the revision
/// (busts the consolidated cache, which keys on `s.revision == revision`)
/// AND prunes every `Provenance::Rule` cell from the store (busts the
/// derived-leaf cache, which then finds no stored value and recomputes).
///
/// FINDING: with the rollback-per-point pattern, the param override DOES
/// move the derived measure. Zero kernel change. This is the GREEN path
/// for 10C.1's `param:` axis — it parallels `coef:` exactly.
#[test]
fn spike_sweep_pattern_rollback_makes_param_move() {
    let compiled = build(&cube_yaml(0.10));
    let mut cube = compiled.cube;
    let refs = &compiled.refs;
    let principal = compiled.root_principal;

    write_signal(&mut cube, refs, principal, 0.15);

    // Baseline eval (populates caches), then ONE snapshot — exactly the
    // sweep.rs ordering (baseline_result, then baseline_snapshot).
    let _baseline = read_flag(&mut cube, refs, principal);
    let snap = cube.snapshot(Some("spike:sweep:pre-overrides"));

    // Sweep two param points the way sweep.rs sweeps coefficients.
    let mut swept: Vec<f64> = Vec::new();
    for &threshold in &[0.10_f64, 0.20_f64] {
        cube.rollback_to(&snap)
            .unwrap_or_else(|e| panic!("rollback failed: {e}"));
        cube.reference_data
            .parameters
            .insert("threshold".to_string(), threshold);
        swept.push(read_flag(&mut cube, refs, principal));
    }

    // threshold 0.10 -> 1.0 ; threshold 0.20 -> 0.0. The axis MOVES.
    assert!(
        (swept[0] - 1.0).abs() < 1e-9,
        "point 0.10 must be 1.0, got {}",
        swept[0]
    );
    assert!(
        (swept[1] - 0.0).abs() < 1e-9,
        "point 0.20 must be 0.0, got {}",
        swept[1]
    );
    assert!(
        (swept[0] - swept[1]).abs() > 1e-9,
        "GREEN: rollback-per-point makes the param axis move with zero kernel change"
    );
}
