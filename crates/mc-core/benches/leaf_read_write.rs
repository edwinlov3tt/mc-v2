//! Phase 1B benchmark: leaf read/write timings.
//!
//! Maps to brief §11.1 (`leaf_read_write.rs`) and the Phase 1B handoff's
//! benchmark category 1 ("Leaf read/write"). Uses Acme fixture
//! coordinates per the handoff. Results land in `docs/PERF.md`.
//!
//! ## What this measures
//!
//! - `read_input_leaf_cold`  — first `cube.read()` of an input leaf cell on
//!   a freshly-built, freshly-loaded cube. There is no derived-leaf cache
//!   for inputs (the cache is in `read_derived_leaf`); this measures the
//!   permission check + store lookup path.
//! - `read_input_leaf_warm`  — repeated `cube.read()` of the same input
//!   leaf at the same revision. Same path as cold but state is hot in
//!   the OS / allocator caches.
//! - `write_input_leaf`      — single `cube.write()` of Spend at one leaf
//!   coordinate **after** dependencies have been materialized. Includes
//!   permission check, type check, store write, and dirty mark/closure
//!   over the full Acme rev-edge graph.
//! - `write_input_leaf_no_deps` — write of Spend at the same leaf coord
//!   on a cube where `materialize_all_dependencies` has NOT been called.
//!   The dep graph is empty, so dirty-closure walks zero rev edges. Acts
//!   as the "synthetic zero-dependents" comparison the brief §11.1 calls
//!   for, without leaving the Acme fixture.
//!
//! Per Phase 1B handoff hard rules: no behavior change. Reads and writes
//! go through the exact same public API the integration tests use.

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use mc_core::{ScalarValue, WriteIntent, WritebackRequest};
use mc_fixtures::{
    build_acme_cube, coord, materialize_all_dependencies, write_canonical_inputs, AcmeRefs,
};

/// Build Acme cube + write all canonical inputs. Used as a setup helper
/// for cold-state benches.
fn build_loaded() -> (mc_core::Cube, AcmeRefs) {
    let (mut cube, refs) = build_acme_cube().expect("acme fixture must build");
    write_canonical_inputs(&mut cube, &refs).expect("canonical inputs must load");
    (cube, refs)
}

/// Build a loaded cube with `materialize_all_dependencies` called so
/// the dep graph + derived-leaf cache are populated. This is the
/// "ready-to-use" baseline for write benches and derived-read benches.
fn build_materialized() -> (mc_core::Cube, AcmeRefs) {
    let (mut cube, refs) = build_loaded();
    materialize_all_dependencies(&mut cube, &refs).expect("materialize must succeed");
    (cube, refs)
}

fn anchor_input_coord(cube: &mc_core::Cube, refs: &AcmeRefs) -> mc_core::CellCoordinate {
    // Mar_2026 / Paid_Search / Tampa / Spend — the brief's anchor cell.
    coord(
        cube.id,
        refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    )
}

fn bench_read_input_leaf_cold(c: &mut Criterion) {
    c.bench_function("read_input_leaf_cold", |b| {
        b.iter_batched_ref(
            build_loaded,
            |(cube, refs)| {
                let coord = anchor_input_coord(cube, refs);
                let v = cube
                    .read(&coord, refs.root_principal)
                    .expect("read must succeed");
                black_box(v);
            },
            // SmallInput keeps the per-iter budget reasonable; setup is
            // ~ms-scale (write_canonical_inputs writes 2520 cells), so
            // criterion runs ~10s wall-clock per bench at default config.
            BatchSize::SmallInput,
        );
    });
}

fn bench_read_input_leaf_warm(c: &mut Criterion) {
    let (mut cube, refs) = build_loaded();
    let coord = anchor_input_coord(&cube, &refs);
    // Warm up once so the call path is hot.
    let _ = cube
        .read(&coord, refs.root_principal)
        .expect("warmup read must succeed");
    c.bench_function("read_input_leaf_warm", |b| {
        b.iter(|| {
            let v = cube
                .read(black_box(&coord), refs.root_principal)
                .expect("read must succeed");
            black_box(v);
        });
    });
}

fn bench_write_input_leaf(c: &mut Criterion) {
    // Write Spend at the anchor coord on a fully materialized cube. The
    // dep graph is fully populated, so dirty-closure walks the full
    // rev-edge index for that coord (≈19,919 dependents per the demo).
    c.bench_function("write_input_leaf", |b| {
        b.iter_batched_ref(
            build_materialized,
            |(cube, refs)| {
                let coord = anchor_input_coord(cube, refs);
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

fn bench_write_input_leaf_no_deps(c: &mut Criterion) {
    // Same write, but on a cube where `materialize_all_dependencies` has
    // NOT been called. The dep graph is empty (lazy), so the dirty
    // closure walks zero rev edges. Approximates the brief §11.1
    // "no-dependents" synthetic without leaving the Acme fixture.
    c.bench_function("write_input_leaf_no_deps", |b| {
        b.iter_batched_ref(
            build_loaded,
            |(cube, refs)| {
                let coord = anchor_input_coord(cube, refs);
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

criterion_group!(
    benches,
    bench_read_input_leaf_cold,
    bench_read_input_leaf_warm,
    bench_write_input_leaf,
    bench_write_input_leaf_no_deps,
);
criterion_main!(benches);
