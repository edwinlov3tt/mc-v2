//! Phase 2A benchmark: hierarchy-ancestor mark walk microbench.
//!
//! Closes the cost-isolation gap [`docs/PERF.md`](../../../docs/PERF.md)
//! §8.1 + §9.3 calls out: every Acme write pays the hierarchy ancestor
//! mark walk regardless of rule fan-out, but Phase 1B couldn't tell
//! the per-ancestor marginal cost from the per-write fixed cost
//! because the only available fixture (`build_acme_cube`) bakes a
//! fixed hierarchy depth into every write.
//!
//! ## What this measures
//!
//! Four rows over graduated hierarchy depth on a 2-dim
//! "Time × Measure" cube produced by
//! [`mc_fixtures::build_graduated_hierarchy_cube`]:
//!
//! - depth 0 — no Time hierarchy → 0 ancestors per write.
//! - depth 1 — 1 ancestor per write.
//! - depth 2 — 2 ancestors per write.
//! - depth 3 — 3 ancestors per write.
//!
//! Both dims are otherwise identical (1 leaf, 1 Input "Spend"
//! measure, no Derived). Subtracting adjacent rows gives the marginal
//! cost per ancestor:
//!
//! ```text
//! marginal_cost_per_ancestor ≈ (depth_N - depth_(N-1))
//! ```
//!
//! Phase 2B can use this to evaluate Recommendation §9.3 ("reduce
//! hierarchy mark closure cost") with magnitudes from data, not from
//! guesswork.
//!
//! ## Sanity check before timing
//!
//! Per Phase 2A handoff: "emit a one-time stderr line per fixture
//! with `dirty_set delta` and `invalidated.len()` so the marginal
//! cost is auditable." [`preflight_for`] runs once per benched depth
//! and prints those numbers. The dirty_set delta is asserted to equal
//! `depth` (one consolidated coord per ancestor × the single Spend
//! measure).

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use mc_core::{ScalarValue, WriteIntent, WritebackRequest};
use mc_fixtures::{build_graduated_hierarchy_cube, graduated_leaf_coord};

/// One-time invariant assertion + stderr audit line for a given
/// `depth`. Asserts `dirty_set delta == depth` so a future maintainer
/// cannot accidentally make this microbench measure something other
/// than the hierarchy ancestor walk.
fn preflight_for(depth: u8) {
    let (mut cube, refs) =
        build_graduated_hierarchy_cube(depth).expect("build_graduated_hierarchy_cube");
    let coord = graduated_leaf_coord(&refs);
    let dirty_before = cube.dirty().len();
    let result = cube
        .write(WritebackRequest {
            coord,
            new_value: ScalarValue::F64(7.0),
            principal: refs.root_principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("preflight write must succeed");
    let dirty_after = cube.dirty().len();
    let delta = dirty_after - dirty_before;
    assert_eq!(
        delta, depth as usize,
        "hierarchy_mark preflight depth={depth}: \
         dirty-set delta {delta} ≠ expected ancestor count {depth}"
    );
    // Per Phase 2D handoff §A.7: under the corrected
    // `WritebackResult.invalidated` semantics (marginal coords this
    // write transitioned clean → dirty, *not* the cumulative dirty
    // set), `dirty_set_delta` and `invalidated.len` are the same
    // quantity and must agree. The phase-2c-era output of this line
    // showed `invalidated.len = <cumulative>` and was a Phase 1A
    // misimplementation — see PERF.md §6.15 for the correction.
    let invalidated_len = result.invalidated.len();
    debug_assert_eq!(
        delta, invalidated_len,
        "hierarchy_mark preflight depth={depth}: dirty_set_delta ({delta}) must equal invalidated.len ({invalidated_len}) under Phase 2D marginal semantics"
    );
    eprintln!(
        "[hierarchy_mark preflight] depth={depth}: cube.dirty delta={delta}, \
         WritebackResult.invalidated.len={invalidated_len} (must equal delta)"
    );
}

fn bench_for_depth(c: &mut Criterion, depth: u8) {
    preflight_for(depth);
    let label = format!("hierarchy_mark/depth_{depth}");
    c.bench_function(&label, |b| {
        b.iter_batched_ref(
            || build_graduated_hierarchy_cube(depth).expect("build_graduated_hierarchy_cube"),
            |(cube, refs)| {
                let coord = graduated_leaf_coord(refs);
                let result = cube
                    .write(WritebackRequest {
                        coord,
                        new_value: ScalarValue::F64(99.0),
                        principal: refs.root_principal,
                        intent: WriteIntent::Set,
                        expected_revision: None,
                        now_unix_seconds: 0,
                    })
                    .expect("write");
                black_box(result);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_depth_0(c: &mut Criterion) {
    bench_for_depth(c, 0);
}
fn bench_depth_1(c: &mut Criterion) {
    bench_for_depth(c, 1);
}
fn bench_depth_2(c: &mut Criterion) {
    bench_for_depth(c, 2);
}
fn bench_depth_3(c: &mut Criterion) {
    bench_for_depth(c, 3);
}

criterion_group!(
    benches,
    bench_depth_0,
    bench_depth_1,
    bench_depth_2,
    bench_depth_3,
);
criterion_main!(benches);
