//! Phase 3C perf gate
//! (per ADR-0006 Decision 9 #11 + acceptance amendment #17).
//!
//! `mc model test crates/mc-model/examples/acme.yaml` must complete
//! in **< 500 ms** wall-clock on the Phase 1B reference machine. The
//! stretch goal from amendment #17 is < 200 ms; the Phase 3C
//! completion report measures the actual and considers tightening if
//! there's headroom.
//!
//! This test invokes the in-process pipeline (parse → validate →
//! resolve_inputs → compile → apply_canonical_inputs → snapshot →
//! 9 reads) rather than spawning the `mc` binary so it works from
//! `cargo test` regardless of build profile. The binary version of
//! the gate runs in the workspace `validate gate` and is reported in
//! the completion report.

use std::path::Path;
use std::time::Instant;

const ACME_YAML: &str = "examples/acme.yaml";
/// Contractual gate from ADR-0006 Decision 9 #11: < 500 ms in release.
/// In debug builds we use a much-looser bound so `cargo test` (without
/// `--release`) doesn't fail on the optimization gap. The completion
/// report measures the release number directly.
const GATE_MS: u128 = if cfg!(debug_assertions) { 5_000 } else { 500 };

#[test]
fn mc_model_test_acme_in_process_under_500ms() {
    let start = Instant::now();

    let yaml = std::fs::read_to_string(ACME_YAML).expect("read acme.yaml");
    let parsed = mc_model::parse(&yaml, Some(ACME_YAML.to_string())).expect("parse");
    let validated = mc_model::validate(parsed).expect("validate");
    let inputs = mc_model::resolve_inputs(&validated, Path::new(ACME_YAML).parent())
        .expect("resolve_inputs");
    let compiled = mc_model::compile(validated).expect("compile");
    let mut cube = compiled.cube;
    let principal = compiled.root_principal;
    mc_model::apply_canonical_inputs(&mut cube, &compiled.refs, principal, &inputs)
        .expect("apply_canonical_inputs");
    let snap = cube.snapshot(None);

    // Re-fetch the goldens from disk via parse so this test is
    // independent of whatever golden_tests acme.yaml currently
    // declares (Phase 3C ships with 9; future phases may add more).
    let parsed_again = mc_model::parse(&yaml, Some(ACME_YAML.to_string())).expect("parse");
    let validated_again = mc_model::validate(parsed_again).expect("validate (reparse)");
    let goldens = &validated_again.parsed.golden_tests;

    for golden in goldens {
        // No fixture overlay on Acme — read-only goldens; no rollback
        // needed (the snapshot is here to mirror the production flow).
        let coord = compiled
            .refs
            .coord_from_names(&golden.coord)
            .expect("coord");
        cube.read(&coord, principal).expect("read");
    }
    // Touch the snapshot so it isn't optimized away.
    let _ = snap.revision;

    let elapsed_ms = start.elapsed().as_millis();
    assert!(
        elapsed_ms < GATE_MS,
        "perf gate failed: {elapsed_ms} ms >= {GATE_MS} ms (in-process Acme `mc model test`)"
    );
}
