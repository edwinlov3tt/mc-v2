//! Phase 1B benchmark: end-to-end demo path.
//!
//! Maps to the Phase 1B handoff's benchmark category 5 ("Demo path"):
//! build the Acme cube, write canonical inputs, materialize derived
//! cells, run the same reads `mc demo` performs, report total elapsed
//! time. Doesn't map directly to one brief §11 row — the closest is
//! brief §11.3 `bench_load_canonical_inputs` (broken out as a separate
//! sub-bench here) and `bench_full_revenue_slice` (the 420-cell warm
//! Revenue slice).
//!
//! ## Sub-benches
//!
//! - `demo_path/build_only` — `build_acme_cube` only.
//! - `demo_path/build_and_load` — build + `write_canonical_inputs`.
//! - `demo_path/build_load_materialize` — build + load + materialize
//!   all dependencies.
//! - `demo_path/full_demo_reads` — on a pre-built, pre-loaded,
//!   pre-materialized cube: run the demo's full read loop (6 leaf reads
//!   + 5 consolidated reads + 1 traced read + 1 write + 2 re-reads).

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use mc_core::{CellCoordinate, Cube};
use mc_fixtures::{
    build_acme_cube, coord, materialize_all_dependencies, write_canonical_inputs, AcmeRefs,
};

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

fn at(
    cube: &Cube,
    refs: &AcmeRefs,
    time: mc_core::ElementId,
    channel: mc_core::ElementId,
    market: mc_core::ElementId,
    measure: mc_core::ElementId,
) -> CellCoordinate {
    coord(
        cube.id,
        refs,
        refs.scen_baseline,
        refs.ver_working,
        time,
        channel,
        market,
        measure,
    )
}

fn bench_build_only(c: &mut Criterion) {
    c.bench_function("demo_path/build_only", |b| {
        b.iter(|| {
            let (cube, refs) = build_acme_cube().expect("build");
            black_box((cube, refs));
        });
    });
}

fn bench_build_and_load(c: &mut Criterion) {
    c.bench_function("demo_path/build_and_load", |b| {
        b.iter_batched(
            || (),
            |_| {
                let (mut cube, refs) = build_acme_cube().expect("build");
                let n = write_canonical_inputs(&mut cube, &refs).expect("inputs");
                assert_eq!(n, 2_520);
                black_box((cube, refs));
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_build_load_materialize(c: &mut Criterion) {
    c.bench_function("demo_path/build_load_materialize", |b| {
        b.iter_batched(
            || (),
            |_| {
                let (mut cube, refs) = build_acme_cube().expect("build");
                write_canonical_inputs(&mut cube, &refs).expect("inputs");
                let n = materialize_all_dependencies(&mut cube, &refs).expect("materialize");
                assert_eq!(n, 2_100);
                black_box((cube, refs));
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_full_demo_reads(c: &mut Criterion) {
    // Pre-build cube once outside the timer. Each bench iteration runs
    // the demo's read sequence on the same hot cube — same as `mc demo`
    // but without the println formatting cost.
    let (mut cube, refs) = build_acme_cube().expect("build");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    materialize_all_dependencies(&mut cube, &refs).expect("materialize");
    // Run once to warm caches (consolidated + derived) for the demo's
    // exact read set.
    do_demo_reads(&mut cube, &refs);

    c.bench_function("demo_path/full_demo_reads (warm)", |b| {
        b.iter(|| {
            do_demo_reads(&mut cube, &refs);
        });
    });
}

/// The demo's read sequence (mirrors `crates/mc-cli/src/main.rs`):
/// 6 leaf reads (Spend through Gross_Profit at the anchor),
/// 5 consolidated reads (4 Spend roll-ups + 1 weighted-average CPC),
/// 1 traced read (Revenue at the anchor).
fn do_demo_reads(cube: &mut Cube, refs: &AcmeRefs) {
    let principal = refs.root_principal;
    for measure in [
        refs.spend,
        refs.clicks,
        refs.leads,
        refs.customers,
        refs.revenue,
        refs.gross_profit,
    ] {
        let v = cube
            .read(&anchor(cube, refs, measure), principal)
            .expect("read");
        black_box(v);
    }
    let consolidated = [
        (refs.q1_2026, refs.paid_search, refs.tampa, refs.spend),
        (refs.mar_2026, refs.paid_search, refs.florida, refs.spend),
        (refs.mar_2026, refs.paid_media, refs.tampa, refs.spend),
        (refs.q1_2026, refs.paid_media, refs.florida, refs.spend),
        (refs.q1_2026, refs.paid_search, refs.florida, refs.cpc),
    ];
    for (t, c, m, meas) in consolidated {
        let v = cube
            .read(&at(cube, refs, t, c, m, meas), principal)
            .expect("read");
        black_box(v);
    }
    let v = cube
        .read_with_trace(&anchor(cube, refs, refs.revenue), principal)
        .expect("trace read");
    black_box(v);
}

fn bench_full_revenue_slice_warm(c: &mut Criterion) {
    // Per brief §11.3 `bench_full_revenue_slice_warm`: read every
    // (12 × 5 × 7) = 420 leaf Revenue cells with the cache hot.
    let (mut cube, refs) = build_acme_cube().expect("build");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    materialize_all_dependencies(&mut cube, &refs).expect("materialize");

    let times: [mc_core::ElementId; 12] = [
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
    let channels: [mc_core::ElementId; 5] = [
        refs.paid_search,
        refs.paid_social,
        refs.display,
        refs.email,
        refs.organic,
    ];
    let markets: [mc_core::ElementId; 7] = [
        refs.tampa,
        refs.orlando,
        refs.miami,
        refs.atlanta,
        refs.charlotte,
        refs.new_york_city,
        refs.boston,
    ];

    // Hold all 420 coords in a Vec so the bench timer doesn't include
    // coord construction. (Coord construction is part of `at` above
    // and not what we're measuring here.)
    let mut coords: Vec<CellCoordinate> =
        Vec::with_capacity(times.len() * channels.len() * markets.len());
    for &t in &times {
        for &c in &channels {
            for &m in &markets {
                coords.push(at(&cube, &refs, t, c, m, refs.revenue));
            }
        }
    }
    assert_eq!(coords.len(), 420);

    // Warm everything once.
    for c in &coords {
        let _ = cube.read(c, refs.root_principal).expect("warm read");
    }

    c.bench_function("demo_path/full_revenue_slice_warm (420 cells)", |b| {
        b.iter(|| {
            for coord in &coords {
                let v = cube.read(coord, refs.root_principal).expect("read");
                black_box(v);
            }
        });
    });
}

fn bench_load_canonical_inputs(c: &mut Criterion) {
    // Per brief §11.3 `bench_load_canonical_inputs`: write 2,520 input
    // cells onto a fresh cube. Setup cost is `build_acme_cube` (no
    // inputs yet); timed body is `write_canonical_inputs`.
    c.bench_function("demo_path/load_canonical_inputs (2520 writes)", |b| {
        b.iter_batched_ref(
            || build_acme_cube().expect("build"),
            |(cube, refs)| {
                let n = write_canonical_inputs(cube, refs).expect("inputs");
                assert_eq!(n, 2_520);
                black_box(n);
            },
            BatchSize::SmallInput,
        );
    });
}

// ---------------------------------------------------------------------------
// Phase 2C — scaled bulk-ingest variants of `bench_load_canonical_inputs`.
//
// Per `docs/handoffs/phase-2c-handoff.md` §"Phase 2C scope" item 2:
// extend the bulk-ingest row at 10× / 50× / 100× to measure ingest
// scaling shape against ADR-0003 Decision 5's "ingest is the gating
// budget" hypothesis. Each scaled row writes 2,520 × scale input cells
// onto a fresh scaled-Acme cube. Asserts the count exactly.
// ---------------------------------------------------------------------------

use mc_fixtures::{
    build_scaled_acme_cube_100x, build_scaled_acme_cube_10x, build_scaled_acme_cube_50x,
    write_canonical_inputs_scaled,
};

fn bench_load_canonical_inputs_scaled(c: &mut Criterion, scale: u32) {
    let cells = 2_520 * scale as usize;
    let label = format!("demo_path/load_canonical_inputs/{scale}x ({cells} writes)");
    c.bench_function(&label, |b| {
        b.iter_batched_ref(
            || {
                match scale {
                    10 => build_scaled_acme_cube_10x(),
                    50 => build_scaled_acme_cube_50x(),
                    100 => build_scaled_acme_cube_100x(),
                    other => panic!("unsupported scale: {other}"),
                }
                .expect("scaled fixture must build")
            },
            |(cube, refs)| {
                let n = write_canonical_inputs_scaled(cube, refs).expect("scaled inputs");
                assert_eq!(n, cells);
                black_box(n);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_load_canonical_inputs_10x(c: &mut Criterion) {
    bench_load_canonical_inputs_scaled(c, 10);
}
fn bench_load_canonical_inputs_50x(c: &mut Criterion) {
    bench_load_canonical_inputs_scaled(c, 50);
}
fn bench_load_canonical_inputs_100x(c: &mut Criterion) {
    bench_load_canonical_inputs_scaled(c, 100);
}

criterion_group!(
    benches,
    bench_build_only,
    bench_build_and_load,
    bench_build_load_materialize,
    bench_full_demo_reads,
    bench_full_revenue_slice_warm,
    bench_load_canonical_inputs,
    bench_load_canonical_inputs_10x,
    bench_load_canonical_inputs_50x,
    bench_load_canonical_inputs_100x,
);
criterion_main!(benches);
