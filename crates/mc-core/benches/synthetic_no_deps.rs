//! Phase 2A benchmark: synthetic no-deps write.
//!
//! Closes the Phase 1B benchmark-scope mismatch documented in
//! [`docs/PERF.md`](../../../docs/PERF.md) §7.3 + §9.3 and CLAUDE.md
//! §6.4 caveat #2: the brief's §11.1 `bench_write_input_leaf_no_deps`
//! ceiling (1A < 50 µs) was implicitly calibrated against a synthetic
//! cube with no hierarchies and no derived measures — every Acme write
//! pays the hierarchy ancestor mark walk regardless of rule fan-out,
//! so the existing `leaf_read_write::bench_write_input_leaf_no_deps`
//! (~165 µs on Acme) is not measuring what the brief intended.
//!
//! ## What this measures
//!
//! `bench_write_input_leaf_no_deps_synthetic` writes Spend at the
//! single leaf coord of the cube returned by
//! [`mc_fixtures::build_minimal_cube`] — a 2-dim cube (Time + Measure)
//! with NO hierarchies and NO derived measures. The dirty-set delta
//! after one write is exactly 0 (unit-tested in `mc-fixtures` via
//! `build_minimal_cube_single_write_produces_zero_dirty_delta`):
//!
//! - `mark_closure(coord, deps)` — empty rule graph → empty closure.
//! - `compute_dirty_ancestors(coord, spend)` — no Time hierarchy edges
//!   and no Derived measures → empty ancestor list.
//!
//! So the timed body covers only the per-write fixed costs:
//! permission check, cube-id / arity check, consolidated-coord check,
//! derived-measure check, version check, lock check, intent
//! application, type check, NaN check, optimistic-concurrency check,
//! revision bump, store write, dirty mark/closure (no-op),
//! `compute_dirty_ancestors` (no-op), soft-lock walk. This is the row
//! the brief §11.1 50 µs 1A ceiling was meant to gate.
//!
//! ## Sanity checks before timing
//!
//! Per the Phase 2A handoff:
//! - `assert!` no non-Measure dim has hierarchy edges.
//! - `assert_eq!` derived measure count = 0.
//! - After a write, `WritebackResult.invalidated.is_empty()`.
//!
//! These run inside a one-time `preflight()` before the bench loop.

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use mc_core::{DimensionKind, MeasureRole, ScalarValue, WriteIntent, WritebackRequest};
use mc_fixtures::{build_minimal_cube, minimal_coord};

/// One-time invariant assertion. Runs before the bench loop so a
/// future maintainer cannot accidentally turn this bench into a
/// hierarchy-ancestor-walk measurement.
fn preflight() {
    let (mut cube, refs) = build_minimal_cube().expect("build_minimal_cube");

    // No hierarchies on any non-Measure dim.
    for dim in cube.dimensions() {
        if dim.kind != DimensionKind::Measure {
            assert!(
                dim.default_hierarchy().edges.is_empty(),
                "synthetic_no_deps preflight: dim {} must have no hierarchy edges",
                dim.name
            );
        }
    }
    // No Derived measures.
    let derived_count = cube
        .measure_dimension()
        .elements
        .iter()
        .filter(|e| {
            e.measure_meta()
                .map(|m| m.role == MeasureRole::Derived)
                .unwrap_or(false)
        })
        .count();
    assert_eq!(
        derived_count, 0,
        "synthetic_no_deps preflight: derived measure count must be 0"
    );

    // After one write, invalidated must be empty.
    let coord = minimal_coord(&refs);
    let result = cube
        .write(WritebackRequest {
            coord,
            new_value: ScalarValue::F64(123.0),
            principal: refs.root_principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("preflight write must succeed");
    assert!(
        result.invalidated.is_empty(),
        "synthetic_no_deps preflight: invalidated must be empty, got len={}",
        result.invalidated.len()
    );
    eprintln!(
        "[synthetic_no_deps preflight] dirty_set len after write: {}; invalidated.len: {}",
        cube.dirty().len(),
        result.invalidated.len()
    );
}

fn bench_write_input_leaf_no_deps_synthetic(c: &mut Criterion) {
    preflight();
    c.bench_function("write_input_leaf_no_deps_synthetic", |b| {
        b.iter_batched_ref(
            || build_minimal_cube().expect("build_minimal_cube"),
            |(cube, refs)| {
                let coord = minimal_coord(refs);
                let result = cube
                    .write(WritebackRequest {
                        coord,
                        new_value: ScalarValue::F64(99_000.0),
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

criterion_group!(benches, bench_write_input_leaf_no_deps_synthetic);
criterion_main!(benches);
