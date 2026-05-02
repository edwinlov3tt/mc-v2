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

// ---------------------------------------------------------------------------
// Phase 2C — scaled-Acme variants (10× / 50× / 100×)
//
// Per `docs/handoffs/phase-2c-handoff.md` §"Phase 2C scope" item 2: the
// existing rows above stay byte-for-byte identical; this section adds
// 10× / 50× / 100× variants of `write_input_leaf`, `read_input_leaf_warm`,
// and `read_input_leaf_cold`. Anchor coord = Mar/Paid_Search/Tampa is
// preserved at every scale (Tampa is one of the seven base cities), so
// the scaled rows hit the same logical leaf and the only thing that
// varies between rows is total cube size + the per-write hierarchy mark
// closure / per-read store-lookup behavior at scale.
//
// Setup cost grows with scale: at 100× a fully-materialized cube takes
// roughly 3 seconds to build (252K input writes + 1.05M cold derived
// reads). Criterion's adaptive batching handles this; if the suite is
// too slow during iteration, smoke commands per the handoff's
// §"Iteration vs final-report bench discipline" cut the per-row budget
// to ~1 s at the cost of statistical confidence — full samples gate
// PERF.md.
// ---------------------------------------------------------------------------

use mc_fixtures::{
    build_scaled_acme_cube_100x, build_scaled_acme_cube_10x, build_scaled_acme_cube_50x,
    materialize_all_dependencies_scaled, write_canonical_inputs_scaled, ScaledAcmeRefs,
};

fn build_loaded_scaled(scale: u32) -> (mc_core::Cube, ScaledAcmeRefs) {
    let (mut cube, refs) = match scale {
        10 => build_scaled_acme_cube_10x(),
        50 => build_scaled_acme_cube_50x(),
        100 => build_scaled_acme_cube_100x(),
        other => panic!("unsupported scale: {other}"),
    }
    .expect("scaled acme fixture must build");
    write_canonical_inputs_scaled(&mut cube, &refs).expect("scaled canonical inputs must load");
    (cube, refs)
}

fn build_materialized_scaled(scale: u32) -> (mc_core::Cube, ScaledAcmeRefs) {
    let (mut cube, refs) = build_loaded_scaled(scale);
    materialize_all_dependencies_scaled(&mut cube, &refs).expect("scaled materialize must succeed");
    (cube, refs)
}

fn anchor_input_coord_scaled(
    cube: &mc_core::Cube,
    refs: &ScaledAcmeRefs,
) -> mc_core::CellCoordinate {
    // Same logical anchor at every scale: Mar/Paid_Search/Tampa.
    coord(
        cube.id,
        &refs.base,
        refs.base.scen_baseline,
        refs.base.ver_working,
        refs.base.mar_2026,
        refs.base.paid_search,
        refs.base.tampa,
        refs.base.spend,
    )
}

/// Per Phase 2C handoff §"Sanity checks before timing": one-time stderr
/// emission per scale with `populated_input_cells = N; dirty_set
/// initial = 0; rule_graph forward edges = M`. Asserts the structural
/// invariants the unit tests already cover so a future maintainer
/// cannot accidentally point a scaled bench at the wrong fixture.
fn scaled_preflight(scale: u32) {
    let (cube, refs) = build_loaded_scaled(scale);
    assert_eq!(cube.dimensions().len(), 6, "scaled cube must have 6 dims");
    assert_eq!(refs.scale, scale);
    let populated = (refs.all_market_leaves.len() * 12 * 5 * 6) as u64;
    let initial_dirty = cube.dirty().len();
    eprintln!(
        "[scaled_preflight x{scale}] populated_input_cells={populated}; \
         dirty_set initial={initial_dirty}; market_leaves={}",
        refs.all_market_leaves.len()
    );
}

fn bench_write_input_leaf_scaled(c: &mut Criterion, scale: u32) {
    scaled_preflight(scale);
    let label = format!("write_input_leaf/{scale}x");
    c.bench_function(&label, |b| {
        b.iter_batched_ref(
            || build_materialized_scaled(scale),
            |(cube, refs)| {
                let coord = anchor_input_coord_scaled(cube, refs);
                let result = cube
                    .write(WritebackRequest {
                        coord,
                        new_value: ScalarValue::F64(50_000.0),
                        principal: refs.base.root_principal,
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

fn bench_read_input_leaf_warm_scaled(c: &mut Criterion, scale: u32) {
    let (mut cube, refs) = build_loaded_scaled(scale);
    let coord = anchor_input_coord_scaled(&cube, &refs);
    let _ = cube
        .read(&coord, refs.base.root_principal)
        .expect("warmup read must succeed");
    let label = format!("read_input_leaf_warm/{scale}x");
    c.bench_function(&label, |b| {
        b.iter(|| {
            let v = cube
                .read(black_box(&coord), refs.base.root_principal)
                .expect("read must succeed");
            black_box(v);
        });
    });
}

fn bench_read_input_leaf_cold_scaled(c: &mut Criterion, scale: u32) {
    let label = format!("read_input_leaf_cold/{scale}x");
    c.bench_function(&label, |b| {
        b.iter_batched_ref(
            || build_loaded_scaled(scale),
            |(cube, refs)| {
                let coord = anchor_input_coord_scaled(cube, refs);
                let v = cube
                    .read(&coord, refs.base.root_principal)
                    .expect("read must succeed");
                black_box(v);
            },
            BatchSize::SmallInput,
        );
    });
}

/// Heavy scaled benches (50× / 100×) are gated behind an env var. Per
/// setup at these scales triggers `build_materialized_scaled` which
/// runs 126K / 252K canonical writes plus 105K / 210K cold reads;
/// criterion calls setup once per iteration, so a sample-size-10 run
/// at 100× takes ~10–30 minutes per row. Set
/// `MC_BENCH_LEAF_SCALED_HEAVY=1` to opt in (Phase 2D step 0 if §6.14
/// needs the cross-scale data the 10× rows don't already give).
fn leaf_scaled_heavy_disabled() -> bool {
    std::env::var("MC_BENCH_LEAF_SCALED_HEAVY").as_deref() != Ok("1")
}

fn bench_write_input_leaf_10x(c: &mut Criterion) {
    bench_write_input_leaf_scaled(c, 10);
}
fn bench_write_input_leaf_50x(c: &mut Criterion) {
    if leaf_scaled_heavy_disabled() {
        eprintln!("[write_input_leaf/50x] SKIPPED — set MC_BENCH_LEAF_SCALED_HEAVY=1 to run");
        return;
    }
    bench_write_input_leaf_scaled(c, 50);
}
fn bench_write_input_leaf_100x(c: &mut Criterion) {
    if leaf_scaled_heavy_disabled() {
        eprintln!("[write_input_leaf/100x] SKIPPED — set MC_BENCH_LEAF_SCALED_HEAVY=1 to run");
        return;
    }
    bench_write_input_leaf_scaled(c, 100);
}
fn bench_read_input_leaf_warm_10x(c: &mut Criterion) {
    bench_read_input_leaf_warm_scaled(c, 10);
}
fn bench_read_input_leaf_warm_50x(c: &mut Criterion) {
    if leaf_scaled_heavy_disabled() {
        eprintln!("[read_input_leaf_warm/50x] SKIPPED — set MC_BENCH_LEAF_SCALED_HEAVY=1 to run");
        return;
    }
    bench_read_input_leaf_warm_scaled(c, 50);
}
fn bench_read_input_leaf_warm_100x(c: &mut Criterion) {
    if leaf_scaled_heavy_disabled() {
        eprintln!("[read_input_leaf_warm/100x] SKIPPED — set MC_BENCH_LEAF_SCALED_HEAVY=1 to run");
        return;
    }
    bench_read_input_leaf_warm_scaled(c, 100);
}
fn bench_read_input_leaf_cold_10x(c: &mut Criterion) {
    bench_read_input_leaf_cold_scaled(c, 10);
}
fn bench_read_input_leaf_cold_50x(c: &mut Criterion) {
    if leaf_scaled_heavy_disabled() {
        eprintln!("[read_input_leaf_cold/50x] SKIPPED — set MC_BENCH_LEAF_SCALED_HEAVY=1 to run");
        return;
    }
    bench_read_input_leaf_cold_scaled(c, 50);
}
fn bench_read_input_leaf_cold_100x(c: &mut Criterion) {
    if leaf_scaled_heavy_disabled() {
        eprintln!("[read_input_leaf_cold/100x] SKIPPED — set MC_BENCH_LEAF_SCALED_HEAVY=1 to run");
        return;
    }
    bench_read_input_leaf_cold_scaled(c, 100);
}

criterion_group!(
    benches,
    bench_read_input_leaf_cold,
    bench_read_input_leaf_warm,
    bench_write_input_leaf,
    bench_write_input_leaf_no_deps,
    // Phase 2C scaled variants — Acme x10 / x50 / x100.
    bench_write_input_leaf_10x,
    bench_write_input_leaf_50x,
    bench_write_input_leaf_100x,
    bench_read_input_leaf_warm_10x,
    bench_read_input_leaf_warm_50x,
    bench_read_input_leaf_warm_100x,
    bench_read_input_leaf_cold_10x,
    bench_read_input_leaf_cold_50x,
    bench_read_input_leaf_cold_100x,
);
criterion_main!(benches);
