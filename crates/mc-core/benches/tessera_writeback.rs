//! Phase 5A Stream A — `WriteBatch::commit()` benchmarks.
//!
//! Companion to [`baseline_writebatch.rs`](./baseline_writebatch.rs).
//! Per ADR-0010 Decision 6 + the Phase 5A Stream A handoff §"SECOND
//! DELIVERABLE": once `WriteBatch` is implemented, this bench measures
//! `WriteBatch::commit()` at the same four scale points (1K / 10K /
//! 100K / 1M) so the before/after diff is reproducible.
//!
//! ## What this measures
//!
//! For each scale point N, the bench records the wall-clock time of a
//! single `WriteBatch::commit()` against the 100× scaled Acme cube
//! (loaded + materialized — same setup as the baseline bench). The
//! batch stages N (coord, value) tuples ahead of time and times only
//! the `commit()` call, which is the operation the gates are
//! calibrated against.
//!
//! Performance gates (ADR-0010 Decision 6):
//!
//! | Scale | Target |
//! |---|---:|
//! | `write_batch/commit/1K`   | ≤ 10 ms |
//! | `write_batch/commit/10K`  | ≤ 100 ms |
//! | `write_batch/commit/100K` | ≤ 1 s |
//! | `write_batch/commit/1M`   | ≤ 5 s |
//!
//! ## Heavy-bench gating
//!
//! Mirroring `baseline_writebatch.rs`: 100K and 1M rows are gated
//! behind `MC_BENCH_TESSERA_HEAVY=1` so default `cargo bench` stays
//! under a few minutes. The gate is OFF by default (default run
//! executes only 1K + 10K). The first commit recording WriteBatch
//! results sets the env var; subsequent CI runs do not (the cheap
//! rows still serve as regression tripwires).
//!
//! Both heavy rows use `SamplingMode::Flat` for the same reason as
//! `baseline_writebatch.rs` §6.16.5 — single-iter samples avoid
//! criterion's Linear ramp blowing the time budget.

use std::time::{Duration, Instant};

use criterion::{black_box, criterion_group, criterion_main, Criterion, SamplingMode};
use mc_core::{CellCoordinate, Cube, ScalarValue, WriteBatch, WritebackContext};
use mc_fixtures::{
    build_scaled_acme_cube_100x, coord, materialize_all_dependencies_scaled,
    write_canonical_inputs_scaled, ScaledAcmeRefs,
};

fn build_steady_cube() -> (Cube, ScaledAcmeRefs) {
    let (mut cube, refs) = build_scaled_acme_cube_100x().expect("100x fixture must build");
    write_canonical_inputs_scaled(&mut cube, &refs).expect("canonical inputs must load");
    materialize_all_dependencies_scaled(&mut cube, &refs).expect("materialize must succeed");
    (cube, refs)
}

/// Build N (coord, value) tuples for the batch. Same Cartesian walk as
/// the baseline bench so the two benches measure the same workload
/// shape — only the apply mechanism differs.
fn build_batch_rows(
    cube: &Cube,
    refs: &ScaledAcmeRefs,
    n: usize,
) -> Vec<(CellCoordinate, ScalarValue)> {
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

    let value = ScalarValue::F64(50_000.0);
    let mut out: Vec<(CellCoordinate, ScalarValue)> = Vec::with_capacity(n);
    'outer: loop {
        for &t in &times {
            for &c in &channels {
                for leaf in &refs.all_market_leaves {
                    for &m in &measures {
                        let coord = coord(
                            cube_id,
                            &refs.base,
                            refs.base.scen_baseline,
                            refs.base.ver_working,
                            t,
                            c,
                            leaf.id,
                            m,
                        );
                        out.push((coord, value.clone()));
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

fn heavy_tessera_enabled() -> bool {
    std::env::var("MC_BENCH_TESSERA_HEAVY").as_deref() == Ok("1")
}

fn bench_commit_n(c: &mut Criterion, n: usize, label: &str) {
    let (mut cube, refs) = build_steady_cube();
    let rows = build_batch_rows(&cube, &refs, n);

    let mut group = c.benchmark_group("write_batch");
    group.sample_size(10);
    if n >= 100_000 {
        group.sampling_mode(SamplingMode::Flat);
    }
    let measurement_secs: u64 = if n <= 1_000 {
        5
    } else if n <= 10_000 {
        15
    } else if n <= 100_000 {
        60
    } else {
        300
    };
    group.measurement_time(Duration::from_secs(measurement_secs));
    group.warm_up_time(Duration::from_secs(if n <= 10_000 { 1 } else { 3 }));

    group.bench_function(label, |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for i in 0..iters {
                // Each iter rebuilds the batch (push is O(N) Vec::push,
                // dwarfed by commit's per-cell cost). The cube is shared
                // across iterations — its revision and dirty set grow,
                // but per-batch commit cost is steady-state once dirty
                // is saturated (PERF.md §6.16 / §6.17).
                let import_id = format!("bench-{n}-{i}");
                let context = WritebackContext {
                    source_name: "tessera_writeback_bench".to_string(),
                    import_id,
                    principal: refs.base.root_principal,
                };
                let mut batch = WriteBatch::new(&mut cube, context);
                batch
                    .push_batch(&rows)
                    .expect("push_batch must succeed in bench");
                let start = Instant::now();
                let result = batch.commit().expect("commit must succeed in bench");
                total += start.elapsed();
                black_box(&result);
            }
            total
        });
    });
    group.finish();
}

fn bench_commit_1k(c: &mut Criterion) {
    bench_commit_n(c, 1_000, "commit/1K");
}

fn bench_commit_10k(c: &mut Criterion) {
    bench_commit_n(c, 10_000, "commit/10K");
}

fn bench_commit_100k(c: &mut Criterion) {
    if !heavy_tessera_enabled() {
        eprintln!(
            "[write_batch/commit/100K] SKIPPED — set \
             MC_BENCH_TESSERA_HEAVY=1 to run"
        );
        return;
    }
    bench_commit_n(c, 100_000, "commit/100K");
}

fn bench_commit_1m(c: &mut Criterion) {
    if !heavy_tessera_enabled() {
        eprintln!(
            "[write_batch/commit/1M] SKIPPED — set \
             MC_BENCH_TESSERA_HEAVY=1 to run"
        );
        return;
    }
    bench_commit_n(c, 1_000_000, "commit/1M");
}

criterion_group!(
    benches,
    bench_commit_1k,
    bench_commit_10k,
    bench_commit_100k,
    bench_commit_1m,
);
criterion_main!(benches);
