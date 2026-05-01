//! Phase 1B benchmark: dirty mark / closure timing after a single write.
//!
//! Maps to brief §11.3 (`full_recompute.rs::bench_full_recompute_after_one_write`)
//! and the Phase 1B handoff's benchmark category 4 ("Dirty propagation").
//!
//! ## What this measures
//!
//! Writes Spend at the brief's anchor coord (Mar_2026 × Paid_Search ×
//! Tampa) **after** `materialize_all_dependencies` has populated the
//! lazy dep graph. The bench timer covers the `cube.write()` call —
//! which includes permission/type/lock checks, store write, revision
//! bump, dirty mark, and `mark_closure` walk over the rev-edge graph
//! plus hierarchy ancestors per spec §8.
//!
//! ## Sanity checks before timing (per Phase 1B handoff)
//!
//! - **Required-present**: a known-dependent derived coord (Revenue at
//!   the anchor) is dirty *after* the timed write.
//! - **Required-absent**: an unrelated coord (Spend at a different
//!   leaf) is *not* dirty after the timed write.
//! - **Delta size**: dirty-set length captured before and after the
//!   timed write; logged in the `dirty_set_delta` non-criterion print.
//!
//! These run once outside the criterion sample loop so the recorded
//! timing is purely the write path.

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use mc_core::{CellCoordinate, Cube, ScalarValue, WriteIntent, WritebackRequest};
use mc_fixtures::{
    build_acme_cube, coord, materialize_all_dependencies, write_canonical_inputs, AcmeRefs,
};

fn build_materialized() -> (Cube, AcmeRefs) {
    let (mut cube, refs) = build_acme_cube().expect("acme fixture must build");
    write_canonical_inputs(&mut cube, &refs).expect("canonical inputs");
    materialize_all_dependencies(&mut cube, &refs).expect("materialize");
    (cube, refs)
}

fn anchor(cube: &Cube, refs: &AcmeRefs, measure: mc_core::ElementId) -> CellCoordinate {
    coord(
        cube.id,
        refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        measure,
    )
}

/// Pre-flight sanity. Per Phase 1B handoff: "Include required-present
/// and required-absent sanity checks before timing." Also reports the
/// dirty-set delta size to stderr — handy for PERF.md cross-checks
/// against the §10.1 bound (≤ 215 per single-write delta; the demo
/// shows ~19,919 for 2,520 inputs because it's an accumulated
/// post-`write_canonical_inputs` figure, see Phase 1A completion
/// report §3 deviation 2).
fn preflight() {
    let (mut cube, refs) = build_materialized();
    let anchor_revenue = anchor(&cube, &refs, refs.revenue);
    let anchor_spend = anchor(&cube, &refs, refs.spend);
    // Distant coord, no Spend dependency on the anchor write.
    let distant_spend = coord(
        cube.id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.dec_2026,
        refs.organic,
        refs.boston,
        refs.spend,
    );

    let dirty_before = cube.dirty().len();
    let result = cube
        .write(WritebackRequest {
            coord: anchor_spend.clone(),
            new_value: ScalarValue::F64(50_000.0),
            principal: refs.root_principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write must succeed");
    let dirty_after = cube.dirty().len();

    // required-present: Revenue at the anchor must be dirty (it
    // depends transitively on Spend@anchor through the Acme rule
    // chain).
    assert!(
        cube.dirty().is_dirty(&anchor_revenue),
        "preflight: Revenue@anchor must be dirty after Spend@anchor write"
    );
    // required-absent: an unrelated leaf Spend should not be dirty.
    // The Acme cube has no cross-cell dependencies between distinct
    // leaf Spends, so writing one must not dirty the other.
    assert!(
        !cube.dirty().is_dirty(&distant_spend),
        "preflight: distant leaf Spend must remain clean after anchor Spend write"
    );

    eprintln!(
        "[dirty_propagation preflight] dirty_set: {dirty_before} -> {dirty_after} (delta {}); invalidated.len={}",
        dirty_after as i64 - dirty_before as i64,
        result.invalidated.len()
    );
}

fn bench_dirty_after_one_spend_write(c: &mut Criterion) {
    preflight();
    c.bench_function("dirty_propagation/spend_at_anchor", |b| {
        b.iter_batched_ref(
            build_materialized,
            |(cube, refs)| {
                let coord = anchor(cube, refs, refs.spend);
                let result = cube
                    .write(WritebackRequest {
                        coord,
                        new_value: ScalarValue::F64(50_000.0),
                        principal: refs.root_principal,
                        intent: WriteIntent::Set,
                        expected_revision: None,
                        now_unix_seconds: 0,
                    })
                    .expect("write must succeed");
                black_box(result);
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, bench_dirty_after_one_spend_write);
criterion_main!(benches);
