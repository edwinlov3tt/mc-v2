//! Phase 5A Stream A — per-cell write baselines (pre-WriteBatch).
//!
//! Per ADR-0010 amendment #12 ("baselines-first gate") and the Phase 5A
//! Stream A handoff: the FIRST commit on this branch must record measured
//! per-cell write costs at four scale points (1K / 10K / 100K / 1M)
//! BEFORE any `WriteBatch` code lands. This bench is that measurement.
//!
//! ## What this measures
//!
//! For each scale point N, the bench records the wall-clock time to call
//! `Cube::write()` N times, where each call writes Spend at a distinct
//! input-leaf coordinate. The cube is the 100× scaled Acme fixture
//! (`build_scaled_acme_cube_100x` — 254,520 input leaves) loaded with
//! canonical inputs and fully materialized so the dependency graph and
//! derived-leaf cache are populated. This puts each `Cube::write()` on
//! the hot path that PERF.md §6.1 / §7.3 already characterizes
//! (~165 µs/write on Acme, dominated by `compute_dirty_ancestors`).
//!
//! For N ≤ 254,520 the coord vector is a strict prefix of the leaf
//! grid (no repeats). For N = 1M the prefix is repeated four times so
//! cells at later positions are overwritten — per-cell cost is unchanged
//! (the dirty set is bounded by the per-write fan-out, not the
//! cumulative size; see PERF.md §6.14 / §9.3 + the bitset fast path
//! introduced in Phase 2D).
//!
//! ## Why only the 100× fixture
//!
//! `build_acme_cube` (2,520 leaves) is too small for any of the scale
//! points except 1K. The handoff's "use the existing
//! `build_acme_cube()`" is a directional pointer; the 100× fixture is
//! the existing public scaled wrapper that has enough leaves for 100K
//! distinct addresses, and it remains an Acme-shape cube (same dims,
//! same hierarchies, same rules). Stream A may not modify
//! `crates/mc-fixtures/`, so the 100× fixture is the largest available.
//!
//! ## Heavy-bench gating
//!
//! At ~165 µs/write the 100K and 1M rows take ~16.5 s and ~165 s per
//! sample respectively. With criterion's minimum sample_size of 10,
//! that's ~3 min and ~30 min per row. Default `cargo bench -p mc-core`
//! must stay reasonable for iteration, so 100K and 1M are gated behind
//! `MC_BENCH_BASELINE_HEAVY=1`. The first-commit baseline run sets the
//! env var; subsequent CI runs do not (the cheap rows still serve as
//! regression tripwires).
//!
//! Maps to ADR-0010 Decision 6 baselines table ("Baseline (extrapolated)"
//! column) and the Phase 5A Stream A handoff §"FIRST DELIVERABLE."

use std::time::{Duration, Instant};

use criterion::{black_box, criterion_group, criterion_main, Criterion, SamplingMode};
use mc_core::{CellCoordinate, Cube, ScalarValue, WriteIntent, WritebackRequest};
use mc_fixtures::{
    build_scaled_acme_cube_100x, coord, materialize_all_dependencies_scaled,
    write_canonical_inputs_scaled, ScaledAcmeRefs,
};

/// Build the 100× scaled Acme fixture, write canonical inputs, and
/// materialize the dependency graph. Returns the cube + refs ready for
/// per-cell write benching.
///
/// At scale=100 this is roughly: 254,520 canonical input writes +
/// 1.05M cold derived reads. Setup cost is ~3-5 s on the M-class
/// hardware listed in PERF.md.
fn build_steady_cube() -> (Cube, ScaledAcmeRefs) {
    let (mut cube, refs) = build_scaled_acme_cube_100x().expect("100x fixture must build");
    write_canonical_inputs_scaled(&mut cube, &refs).expect("canonical inputs must load");
    materialize_all_dependencies_scaled(&mut cube, &refs).expect("materialize must succeed");
    (cube, refs)
}

/// Generate `n` input-leaf coordinates by walking the (time, channel,
/// market, input-measure) Cartesian product on the scaled cube. The
/// walk order is (market_idx ascending → channel ascending → time
/// ascending → input-measure ascending) so successive coords share the
/// most slot prefixes — gives a realistic write pattern and exercises
/// the dirty-ancestor walk cache locality.
///
/// If `n` exceeds the available distinct leaves (12 × 5 × M × 6 where
/// M = `refs.all_market_leaves.len()`), the walk wraps around. At
/// scale=100 (M=707) the limit is 254,520 distinct coords; at n=1M the
/// vector contains ~3.93 cycles of the prefix.
fn build_write_coords(cube: &Cube, refs: &ScaledAcmeRefs, n: usize) -> Vec<CellCoordinate> {
    let cube_id = cube.id;
    let times = [
        refs.base.jan_2026,
        refs.base.feb_2026,
        refs.base.mar_2026,
        refs.base.apr_2026,
        refs.base.may_2026,
        refs.base.jun_2026,
        refs.base.jul_2026,
        refs.base.aug_2026,
        refs.base.sep_2026,
        refs.base.oct_2026,
        refs.base.nov_2026,
        refs.base.dec_2026,
    ];
    let channels = [
        refs.base.paid_search,
        refs.base.paid_social,
        refs.base.display,
        refs.base.email,
        refs.base.organic,
    ];
    let measures = [
        refs.base.spend,
        refs.base.cpc,
        refs.base.cvr,
        refs.base.close_rate,
        refs.base.aov,
        refs.base.cogs_rate,
    ];

    let mut out: Vec<CellCoordinate> = Vec::with_capacity(n);
    'outer: loop {
        for &t in &times {
            for &c in &channels {
                for leaf in &refs.all_market_leaves {
                    for &m in &measures {
                        out.push(coord(
                            cube_id,
                            &refs.base,
                            refs.base.scen_baseline,
                            refs.base.ver_working,
                            t,
                            c,
                            leaf.id,
                            m,
                        ));
                        if out.len() == n {
                            break 'outer;
                        }
                    }
                }
            }
        }
    }
    out
}

/// True iff the heavy-bench env var is set. 100K + 1M rows respect this.
fn heavy_baseline_enabled() -> bool {
    std::env::var("MC_BENCH_BASELINE_HEAVY").as_deref() == Ok("1")
}

/// Run a per-cell write baseline at scale `n`. Uses `iter_custom` so the
/// outer loop can choose to do exactly one full N-write pass per
/// criterion iteration (criterion auto-tunes the inner-iter count for
/// slow routines down to 1, which is what we want for 1M).
fn bench_baseline_n(c: &mut Criterion, n: usize, label: &str) {
    let (mut cube, refs) = build_steady_cube();
    let coords = build_write_coords(&cube, &refs, n);
    let principal = refs.base.root_principal;
    let value = ScalarValue::F64(50_000.0);

    let mut group = c.benchmark_group("baseline_writebatch");
    group.sample_size(10);
    // For the heavy rows (≥ 100K writes per iter) the routine cost
    // dominates; criterion's default Linear sampling would ramp the
    // inner-iter count from 7 up to 70 and easily blow past an hour.
    // Flat sampling sets `iters = 1` for every sample so we get
    // `sample_size` independent runs at the natural N-write cost.
    if n >= 100_000 {
        group.sampling_mode(SamplingMode::Flat);
    }
    // Measurement-time budget: enough to fit 10 single-pass samples at
    // the observed per-cell cost (~9 µs/write on M4 / Phase 2D path).
    let measurement_secs: u64 = if n <= 1_000 {
        5
    } else if n <= 10_000 {
        30
    } else if n <= 100_000 {
        120
    } else {
        300
    };
    group.measurement_time(Duration::from_secs(measurement_secs));
    // Warm-up is bounded too; large warm-ups would burn cycles before
    // the actual measurement.
    group.warm_up_time(Duration::from_secs(if n <= 10_000 { 1 } else { 3 }));

    group.bench_function(label, |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let start = Instant::now();
                for coord in &coords {
                    let result = cube
                        .write(WritebackRequest {
                            coord: coord.clone(),
                            new_value: value.clone(),
                            principal,
                            intent: WriteIntent::Set,
                            expected_revision: None,
                            now_unix_seconds: 0,
                        })
                        .expect("baseline write must succeed");
                    black_box(&result);
                }
                total += start.elapsed();
            }
            total
        });
    });
    group.finish();
}

fn bench_baseline_1k(c: &mut Criterion) {
    bench_baseline_n(c, 1_000, "per_cell/1K");
}

fn bench_baseline_10k(c: &mut Criterion) {
    bench_baseline_n(c, 10_000, "per_cell/10K");
}

fn bench_baseline_100k(c: &mut Criterion) {
    if !heavy_baseline_enabled() {
        eprintln!(
            "[baseline_writebatch/per_cell/100K] SKIPPED — set \
             MC_BENCH_BASELINE_HEAVY=1 to run (~3 min)"
        );
        return;
    }
    bench_baseline_n(c, 100_000, "per_cell/100K");
}

fn bench_baseline_1m(c: &mut Criterion) {
    if !heavy_baseline_enabled() {
        eprintln!(
            "[baseline_writebatch/per_cell/1M] SKIPPED — set \
             MC_BENCH_BASELINE_HEAVY=1 to run (~30 min)"
        );
        return;
    }
    bench_baseline_n(c, 1_000_000, "per_cell/1M");
}

criterion_group!(
    benches,
    bench_baseline_1k,
    bench_baseline_10k,
    bench_baseline_100k,
    bench_baseline_1m,
);
criterion_main!(benches);
