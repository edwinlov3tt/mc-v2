//! Phase 2A benchmark: snapshot clone + rollback by store cardinality.
//!
//! Per the Phase 2A handoff: "Snapshot is a deep clone of HashMapStore"
//! ([`docs/PERF.md`](../../../docs/PERF.md) §8.3, Phase 1A completion
//! report §8 follow-up #3). Phase 1A ships a deliberately simple
//! `Cube::snapshot()` that clones the whole store. The cost is O(N)
//! over store size with a per-entry constant of "memcpy a fixed
//! struct"; this bench surfaces the actual constant + linear factor
//! across the four cardinality landmarks the handoff calls out:
//!
//! - 0 cells (`fresh`) — `build_acme_cube()` with no inputs.
//! - 100 cells — fresh + a small loop writing 100 Spend cells.
//! - 2,520 cells (`loaded`) — fresh + `write_canonical_inputs()`.
//! - materialized (~25K cells) — `loaded` +
//!   `materialize_all_dependencies()`.
//!
//! Each cardinality has two rows:
//! - `snapshot/N_cells` — `Cube::snapshot(None)`, the clone hot path.
//! - `rollback/N_cells` — `Cube::rollback_to(&snap)`, which clones
//!   the snapshot's store back AND prunes Rule-provenance cells
//!   ([`crates/mc-core/src/cube.rs:1027`](../../../crates/mc-core/src/cube.rs#L1027)).
//!
//! ## Round-trip integrity check
//!
//! Per the Phase 2A handoff sanity-check requirement: take a snapshot,
//! mutate, rollback, read — values must match pre-mutation values.
//! [`integrity_roundtrip`] runs once before the bench loop.

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use mc_core::{Cube, ScalarValue, WriteIntent, WritebackRequest};
use mc_fixtures::{
    build_acme_cube, coord, materialize_all_dependencies, write_canonical_inputs, AcmeRefs,
};

// ----- Cube state builders, one per benched cardinality -----

fn build_fresh() -> (Cube, AcmeRefs) {
    build_acme_cube().expect("acme fixture must build")
}

fn build_with_n_writes(n: usize) -> (Cube, AcmeRefs) {
    let (mut cube, refs) = build_fresh();
    // Write `n` Spend cells, walking the canonical input grid in
    // (time, channel, market) order. The Acme grid has 12 × 5 × 7 =
    // 420 (time, channel, market) triples; n ≤ 420 covers our
    // landmark of 100.
    let times = [
        refs.jan_2026,
        refs.feb_2026,
        refs.mar_2026,
        refs.apr_2026,
        refs.may_2026,
        refs.jun_2026,
        refs.jul_2026,
        refs.aug_2026,
        refs.sep_2026,
        refs.oct_2026,
        refs.nov_2026,
        refs.dec_2026,
    ];
    let channels = [
        refs.paid_search,
        refs.paid_social,
        refs.display,
        refs.email,
        refs.organic,
    ];
    let markets = [
        refs.tampa,
        refs.orlando,
        refs.miami,
        refs.atlanta,
        refs.charlotte,
        refs.new_york_city,
        refs.boston,
    ];
    let mut written = 0;
    'outer: for &t in &times {
        for &c in &channels {
            for &m in &markets {
                if written >= n {
                    break 'outer;
                }
                let coord = coord(
                    cube.id,
                    &refs,
                    refs.scen_baseline,
                    refs.ver_working,
                    t,
                    c,
                    m,
                    refs.spend,
                );
                cube.write(WritebackRequest {
                    coord,
                    new_value: ScalarValue::F64(1_000.0 + written as f64),
                    principal: refs.root_principal,
                    intent: WriteIntent::Set,
                    expected_revision: None,
                    now_unix_seconds: 0,
                })
                .expect("write");
                written += 1;
            }
        }
    }
    (cube, refs)
}

fn build_loaded() -> (Cube, AcmeRefs) {
    let (mut cube, refs) = build_fresh();
    write_canonical_inputs(&mut cube, &refs).expect("canonical inputs");
    (cube, refs)
}

fn build_materialized() -> (Cube, AcmeRefs) {
    let (mut cube, refs) = build_loaded();
    materialize_all_dependencies(&mut cube, &refs).expect("materialize");
    (cube, refs)
}

// ----- Round-trip integrity preflight -----

/// Per Phase 2A handoff: "Round-trip — take a snapshot, mutate,
/// rollback, read; values must match pre-mutation values."
fn integrity_roundtrip() {
    let (mut cube, refs) = build_loaded();
    let anchor = coord(
        cube.id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    let pre = cube
        .read(&anchor, refs.root_principal)
        .expect("pre read")
        .value
        .as_f64()
        .expect("F64");
    let snap = cube.snapshot(Some("integrity_pre"));
    cube.write(WritebackRequest {
        coord: anchor.clone(),
        new_value: ScalarValue::F64(pre + 12_345.0),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("mutate");
    let mid = cube
        .read(&anchor, refs.root_principal)
        .expect("mid read")
        .value
        .as_f64()
        .expect("F64");
    assert!(
        (mid - (pre + 12_345.0)).abs() < 1e-9,
        "mid value did not reflect mutation: pre={pre} mid={mid}"
    );
    cube.rollback_to(&snap).expect("rollback");
    let post = cube
        .read(&anchor, refs.root_principal)
        .expect("post read")
        .value
        .as_f64()
        .expect("F64");
    assert!(
        (post - pre).abs() < 1e-9,
        "rollback did not restore pre-mutation value: pre={pre} post={post}"
    );
    eprintln!("[snapshot_clone integrity] pre={pre} mid={mid} post={post} (rollback restored pre)");
}

// ----- Snapshot benches by cardinality -----

fn bench_snapshot(c: &mut Criterion, label: &str, build: fn() -> (Cube, AcmeRefs)) {
    let (cube, _refs) = build();
    c.bench_function(label, |b| {
        b.iter(|| {
            let snap = cube.snapshot(black_box(None));
            black_box(snap);
        });
    });
}

fn bench_rollback(c: &mut Criterion, label: &str, build: fn() -> (Cube, AcmeRefs)) {
    c.bench_function(label, |b| {
        b.iter_batched_ref(
            || {
                // Per-iteration setup: build the cube at the target
                // cardinality, take a snapshot, mutate (so rollback
                // has work to do), then return (cube, snap).
                let (mut cube, refs) = build();
                let snap = cube.snapshot(None);
                let mutate_coord = coord(
                    cube.id,
                    &refs,
                    refs.scen_baseline,
                    refs.ver_working,
                    refs.mar_2026,
                    refs.paid_search,
                    refs.tampa,
                    refs.spend,
                );
                cube.write(WritebackRequest {
                    coord: mutate_coord,
                    new_value: ScalarValue::F64(99_999.0),
                    principal: refs.root_principal,
                    intent: WriteIntent::Set,
                    expected_revision: None,
                    now_unix_seconds: 0,
                })
                .expect("mutate");
                (cube, snap)
            },
            |(cube, snap)| {
                let r = cube.rollback_to(black_box(snap)).expect("rollback");
                black_box(r);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_snapshot_fresh(c: &mut Criterion) {
    // Round-trip integrity check runs once before any timing —
    // attached to the first registered bench so it always executes,
    // exactly once, regardless of which benches are filtered in.
    integrity_roundtrip();
    bench_snapshot(c, "snapshot/0_cells_fresh", build_fresh);
}
fn bench_snapshot_100(c: &mut Criterion) {
    bench_snapshot(c, "snapshot/100_cells", || build_with_n_writes(100));
}
fn bench_snapshot_2520(c: &mut Criterion) {
    bench_snapshot(c, "snapshot/2520_cells_loaded", build_loaded);
}
fn bench_snapshot_materialized(c: &mut Criterion) {
    bench_snapshot(c, "snapshot/materialized", build_materialized);
}

fn bench_rollback_fresh(c: &mut Criterion) {
    bench_rollback(c, "rollback/0_cells_fresh", build_fresh);
}
fn bench_rollback_100(c: &mut Criterion) {
    bench_rollback(c, "rollback/100_cells", || build_with_n_writes(100));
}
fn bench_rollback_2520(c: &mut Criterion) {
    bench_rollback(c, "rollback/2520_cells_loaded", build_loaded);
}
fn bench_rollback_materialized(c: &mut Criterion) {
    bench_rollback(c, "rollback/materialized", build_materialized);
}

criterion_group!(
    benches,
    bench_snapshot_fresh,
    bench_snapshot_100,
    bench_snapshot_2520,
    bench_snapshot_materialized,
    bench_rollback_fresh,
    bench_rollback_100,
    bench_rollback_2520,
    bench_rollback_materialized,
);
criterion_main!(benches);
