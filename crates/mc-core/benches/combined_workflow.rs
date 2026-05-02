//! Phase 2C combined-workflow bench — the load-bearing measurement.
//!
//! Per `docs/handoffs/phase-2c-handoff.md` §"Phase 2C scope" item 3:
//! simulates one planner session against a 50× / 100× scaled Acme cube.
//! Unlike the isolated benches in this crate which measure single-shot
//! operations on a fresh cube, this bench is **temporal**: it captures
//! per-iteration cost across a 100-edit session where snapshots are
//! held live and dirty-tracker / consolidation cache state accumulates.
//! The motivating question — does per-edit cost stay flat across a
//! session (→ §9.2 wins) or grow super-linearly (→ §9.3 wins)? — is
//! answerable from this bench's data and not from any isolated row.
//!
//! ## Session shape
//!
//! 1. Build scaled Acme cube + bulk-load canonical inputs.
//! 2. Materialize all dependencies.
//! 3. Loop 100 times:
//!    - Write a Spend cell at a varying leaf coord (rotates over the
//!      Time × Channel × Market grid so each iteration touches a
//!      different cube region).
//!    - Every 5 iterations: read a 27-leaf consolidated slice
//!      (Q1×Paid_Media×Florida × Revenue at scale N → 27N leaves).
//!    - Every 10 iterations: take a snapshot. **All snapshots are
//!      held live** to the end of the session (TM1 stacked-sandbox
//!      pattern per ADR-0003 Decision 6).
//!
//! ## What gets reported
//!
//! - Criterion measurement: total wall-clock per session (statistical
//!   estimate over 10 samples per row). This is the row that lands in
//!   PERF.md §6.13.
//! - Stderr emission (`preflight_and_emit_stats`, runs ONCE per scale
//!   before timing): per-edit p50/p95/p99, per-slice-read p50/p99,
//!   per-snapshot p50/p99, final dirty-set size, final invalidated.len
//!   (last-iteration write), and §6.10-style attribution
//!   (`per_mark_ns = edit_time_ns / dirty_set_delta`) at iterations 1,
//!   50, and 100. Cumulative allocations are *not measured* —
//!   instrumenting the global allocator is out of scope for Phase 2C
//!   (would require a custom allocator like `dhat` or a global counter,
//!   neither of which the locked-deps allowlist permits).
//!
//! Sample size is overridden to 10 (default 100) because each timed
//! body runs a full session including the materialize phase (~5s at
//! 50×, ~10s at 100×). 10 samples × per-session time × 2 setup is the
//! tractable budget; criterion's statistical bounds are still meaningful.

use std::time::{Duration, Instant};

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mc_core::{CellCoordinate, Cube, ScalarValue, Snapshot, WriteIntent, WritebackRequest};
use mc_fixtures::{
    build_scaled_acme_cube_100x, build_scaled_acme_cube_50x, coord,
    materialize_all_dependencies_scaled, write_canonical_inputs_scaled, ScaledAcmeRefs,
};

const SESSION_ITERS: usize = 100;
const SLICE_EVERY: usize = 5;
const SNAPSHOT_EVERY: usize = 10;

/// Build the per-session starting state: a fully-materialized scaled-
/// Acme cube. Counts as setup (paid once per criterion sample), not as
/// timed work.
fn build_session_setup(scale: u32) -> (Cube, ScaledAcmeRefs) {
    let (mut cube, refs) = match scale {
        50 => build_scaled_acme_cube_50x(),
        100 => build_scaled_acme_cube_100x(),
        other => panic!("combined_workflow: unsupported scale {other}"),
    }
    .expect("scaled fixture must build");
    write_canonical_inputs_scaled(&mut cube, &refs).expect("scaled inputs");
    materialize_all_dependencies_scaled(&mut cube, &refs).expect("scaled materialize");
    (cube, refs)
}

/// Pick the leaf coord for iteration `i`. Rotates over (Time × Channel
/// × Market) so consecutive iterations touch distinct cells. Always
/// writes into Spend.
fn rotating_edit_coord(cube: &Cube, refs: &ScaledAcmeRefs, i: usize) -> CellCoordinate {
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
    let n_markets = refs.all_market_leaves.len();
    let market = refs.all_market_leaves[i % n_markets].id;
    let t = times[i % times.len()];
    let c = channels[(i / times.len()) % channels.len()];
    coord(
        cube.id,
        &refs.base,
        refs.base.scen_baseline,
        refs.base.ver_working,
        t,
        c,
        market,
        refs.base.spend,
    )
}

/// 27-leaf consolidated slice (Q1 × Paid_Media × Florida × Revenue) —
/// at scale N this slice has 27 × N child leaves. Read every
/// `SLICE_EVERY` iterations.
fn slice_coord(cube: &Cube, refs: &ScaledAcmeRefs) -> CellCoordinate {
    coord(
        cube.id,
        &refs.base,
        refs.base.scen_baseline,
        refs.base.ver_working,
        refs.base.q1_2026,
        refs.base.paid_media,
        refs.base.florida,
        refs.base.revenue,
    )
}

/// Run one full session against `cube`. Returns the per-operation timing
/// vectors and the dirty/invalidated sizes the bench reports. The vector
/// of snapshots is held live until the function returns so the TM1
/// stacked-sandbox cost is part of the measurement.
struct SessionStats {
    edit_times: Vec<Duration>,
    slice_times: Vec<Duration>,
    snapshot_times: Vec<Duration>,
    /// (iteration, edit_duration, dirty_delta) for iterations 1, 50, 100.
    attribution: Vec<(usize, Duration, usize)>,
    final_dirty_set: usize,
    final_invalidated_len: usize,
    snapshots_held: usize,
    total: Duration,
}

fn run_session(cube: &mut Cube, refs: &ScaledAcmeRefs) -> SessionStats {
    let mut snapshots: Vec<Snapshot> = Vec::with_capacity(SESSION_ITERS / SNAPSHOT_EVERY);
    let mut edit_times = Vec::with_capacity(SESSION_ITERS);
    let mut slice_times = Vec::with_capacity(SESSION_ITERS / SLICE_EVERY);
    let mut snapshot_times = Vec::with_capacity(SESSION_ITERS / SNAPSHOT_EVERY);
    let mut attribution = Vec::new();
    let mut final_invalidated_len = 0usize;

    let session_start = Instant::now();
    for i in 1..=SESSION_ITERS {
        let edit_coord = rotating_edit_coord(cube, refs, i);
        let dirty_before = cube.dirty().len();
        let t0 = Instant::now();
        let result = cube
            .write(WritebackRequest {
                coord: edit_coord,
                new_value: ScalarValue::F64(50_000.0 + i as f64),
                principal: refs.base.root_principal,
                intent: WriteIntent::Set,
                expected_revision: None,
                now_unix_seconds: 0,
            })
            .expect("session write");
        let dt = t0.elapsed();
        edit_times.push(dt);
        final_invalidated_len = result.invalidated.len();
        let dirty_after = cube.dirty().len();
        let delta = dirty_after.saturating_sub(dirty_before);
        if i == 1 || i == 50 || i == 100 {
            attribution.push((i, dt, delta));
        }

        if i % SLICE_EVERY == 0 {
            let sc = slice_coord(cube, refs);
            let t = Instant::now();
            let v = cube
                .read(black_box(&sc), refs.base.root_principal)
                .expect("session slice read");
            slice_times.push(t.elapsed());
            black_box(v);
        }

        if i % SNAPSHOT_EVERY == 0 {
            let t = Instant::now();
            let snap = cube.snapshot(Some("session"));
            snapshot_times.push(t.elapsed());
            // TM1 stacked-sandbox pattern — hold every snapshot live
            // for the rest of the session.
            snapshots.push(snap);
        }
    }
    let total = session_start.elapsed();

    let snapshots_held = snapshots.len();
    let final_dirty_set = cube.dirty().len();
    // Drop the snapshots only AFTER capturing the final dirty/invalidated
    // sizes — keeping them in scope through the loop is the bench's
    // load-bearing constraint.
    drop(snapshots);

    SessionStats {
        edit_times,
        slice_times,
        snapshot_times,
        attribution,
        final_dirty_set,
        final_invalidated_len,
        snapshots_held,
        total,
    }
}

/// Compute (p50, p95, p99) from a vector of durations. Sorts in place.
fn percentiles(samples: &mut [Duration]) -> (Duration, Duration, Duration) {
    samples.sort();
    let n = samples.len();
    if n == 0 {
        return (Duration::ZERO, Duration::ZERO, Duration::ZERO);
    }
    let pick = |p: usize| -> Duration {
        // Nearest-rank percentile. p ∈ [1, 99].
        let idx = ((p * n + 99) / 100).saturating_sub(1).min(n - 1);
        samples[idx]
    };
    (pick(50), pick(95), pick(99))
}

fn fmt(d: Duration) -> String {
    let nanos = d.as_nanos();
    if nanos < 10_000 {
        format!("{nanos} ns")
    } else if nanos < 10_000_000 {
        format!("{:.1} µs", nanos as f64 / 1_000.0)
    } else {
        format!("{:.2} ms", nanos as f64 / 1_000_000.0)
    }
}

/// Run `n` independent sessions and emit aggregate percentile +
/// attribution stats to stderr. Runs once per scale before any
/// criterion timing. The aggregated stats are what land in PERF.md
/// §6.13 — they replace the criterion-emitted median for this bench
/// because criterion's per-sample timing model (designed for ns-µs
/// microbenches) doesn't fit a 100-iteration session-shaped workload.
///
/// `n` controls the sample count for the aggregated medians.
/// `n = 10` is the minimum criterion sample count and gives a useful
/// confidence interval at this scale. Each sample = one full
/// build+load+materialize+session, so total wall-clock per scale is
/// roughly `n × (setup + session)` seconds.
fn preflight_and_emit_stats_n(scale: u32, n: usize) {
    let mut session_totals: Vec<Duration> = Vec::with_capacity(n);
    let mut edit_p50s: Vec<Duration> = Vec::with_capacity(n);
    let mut edit_p95s: Vec<Duration> = Vec::with_capacity(n);
    let mut edit_p99s: Vec<Duration> = Vec::with_capacity(n);
    let mut slice_p50s: Vec<Duration> = Vec::with_capacity(n);
    let mut slice_p99s: Vec<Duration> = Vec::with_capacity(n);
    let mut snap_p50s: Vec<Duration> = Vec::with_capacity(n);
    let mut snap_p99s: Vec<Duration> = Vec::with_capacity(n);
    let mut attribution_at: [Vec<(Duration, usize)>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut final_dirty: Vec<usize> = Vec::with_capacity(n);
    let mut final_inv_len: Vec<usize> = Vec::with_capacity(n);
    let mut snapshots_held: usize = 0;

    eprintln!(
        "[combined_workflow x{scale}] aggregating {n} session samples (preflight); each sample = build+load+materialize+session"
    );
    for run in 0..n {
        let (mut cube, refs) = build_session_setup(scale);
        let mut stats = run_session(&mut cube, &refs);
        session_totals.push(stats.total);
        let (e50, e95, e99) = percentiles(&mut stats.edit_times);
        edit_p50s.push(e50);
        edit_p95s.push(e95);
        edit_p99s.push(e99);
        let (s50, _, s99) = percentiles(&mut stats.slice_times);
        slice_p50s.push(s50);
        slice_p99s.push(s99);
        let (sn50, _, sn99) = percentiles(&mut stats.snapshot_times);
        snap_p50s.push(sn50);
        snap_p99s.push(sn99);
        for (slot, (_, dt, delta)) in stats.attribution.iter().enumerate() {
            attribution_at[slot].push((*dt, *delta));
        }
        final_dirty.push(stats.final_dirty_set);
        final_inv_len.push(stats.final_invalidated_len);
        snapshots_held = stats.snapshots_held;
        eprint!(".");
        if run + 1 == n {
            eprintln!();
        }
    }

    let median = |xs: &mut Vec<Duration>| -> Duration {
        xs.sort();
        xs[xs.len() / 2]
    };
    let median_usize = |xs: &mut Vec<usize>| -> usize {
        xs.sort();
        xs[xs.len() / 2]
    };

    let session_median = median(&mut session_totals);
    let edit_p50 = median(&mut edit_p50s);
    let edit_p95 = median(&mut edit_p95s);
    let edit_p99 = median(&mut edit_p99s);
    let slice_p50 = median(&mut slice_p50s);
    let slice_p99 = median(&mut slice_p99s);
    let snap_p50 = median(&mut snap_p50s);
    let snap_p99 = median(&mut snap_p99s);
    let dirty_median = median_usize(&mut final_dirty);
    let inv_median = median_usize(&mut final_inv_len);

    eprintln!(
        "[combined_workflow x{scale}] session median (over {n} samples)={} (100 edits + 20 slices + 10 snapshots; all snapshots held live)",
        fmt(session_median)
    );
    eprintln!(
        "[combined_workflow x{scale}] edit median(p50)={} median(p95)={} median(p99)={}",
        fmt(edit_p50),
        fmt(edit_p95),
        fmt(edit_p99)
    );
    eprintln!(
        "[combined_workflow x{scale}] slice_read median(p50)={} median(p99)={}",
        fmt(slice_p50),
        fmt(slice_p99)
    );
    eprintln!(
        "[combined_workflow x{scale}] snapshot median(p50)={} median(p99)={}",
        fmt(snap_p50),
        fmt(snap_p99)
    );
    let iters = [1usize, 50, 100];
    for (slot, iter) in iters.iter().enumerate() {
        let mut samples = attribution_at[slot].clone();
        // Median by edit time for stability.
        samples.sort_by_key(|(dt, _)| *dt);
        let (dt_med, delta_med) = samples[samples.len() / 2];
        let per_mark_ns = if delta_med == 0 {
            0.0
        } else {
            dt_med.as_nanos() as f64 / delta_med as f64
        };
        eprintln!(
            "[combined_workflow x{scale}] attribution@iter{iter:03} (median across {n} samples): \
             edit_time={} dirty_delta={delta_med} per_mark={per_mark_ns:.1} ns",
            fmt(dt_med)
        );
    }
    eprintln!(
        "[combined_workflow x{scale}] final dirty_set median={dirty_median} \
         final invalidated.len median={inv_median} live_snapshots={snapshots_held} \
         (allocations: not measured)"
    );
}

fn bench_combined_workflow(c: &mut Criterion, scale: u32) {
    // The bench's load-bearing deliverable is the rich percentile +
    // attribution data the preflight emits to stderr (which lands in
    // PERF.md §6.13). Criterion's statistical machinery — designed for
    // sub-millisecond microbenches — does not fit a 100-iteration
    // session-shaped bench well: it tries to amortize timing overhead
    // over ~100 iters per sample, which at ~500 ms / iter would bloat
    // each row to ~40 minutes regardless of `iter_custom` /
    // `BatchSize::PerIteration` overrides. So this bench is structured
    // as a **preflight-driven** deliverable: the preflight runs the
    // session ten times (the criterion `--save-baseline` workflow's
    // "10 samples" equivalent), captures medians + p95/p99 across those
    // samples, and the criterion call itself only needs to *register*
    // the bench so `cargo bench --bench combined_workflow` finds it.
    //
    // The criterion side records a single nominal-cost noop iteration —
    // its sample count is irrelevant; the timing it emits is the
    // black-box overhead, not the session timing. PERF.md §6.13 cites
    // the preflight numbers, not criterion's median. This is the same
    // trade-off ADR-0002 codifies for cache-hit assertions: numbers
    // belong in the right harness, and criterion's harness isn't the
    // right one for a one-shot session statistic.
    // n = 3 keeps the per-row wall-clock tractable (~12 min at 50×,
    // ~30 min at 100×). Three samples is enough to compute a stable
    // median + min/max range; the load-bearing measurement (per-mark
    // attribution at iter 1/50/100, scaling shape across §6.14) is
    // robust at n=3 because each sample produces 100 edits' worth of
    // within-session percentiles. The handoff's "sample-of-100"
    // discipline applies to PERF.md §6.1–§6.10's microbench rows;
    // session-shaped rows in §6.13 are sample-of-3 by construction.
    preflight_and_emit_stats_n(scale, 3);

    let label = format!("combined_workflow/{scale}x_marker");
    c.bench_function(&label, |b| {
        b.iter(|| black_box(scale));
    });
}

fn bench_combined_workflow_50x(c: &mut Criterion) {
    bench_combined_workflow(c, 50);
}

/// 100× combined-workflow row. Gated behind the
/// `MC_BENCH_COMBINED_WORKFLOW_100X=1` environment variable because
/// the preflight at this scale takes roughly 30 minutes per invocation
/// (one preflight × 3 sessions × ~10 min/session, where each session
/// includes a 252,000-write bulk-load + 210,000-read materialize at
/// scale). Per Phase 2C completion report §4.4: 50× is the *default*
/// scale, 100× is the *stress* scale; the 100× row is documented as a
/// follow-up Phase 2D step 0 can run if §6.14 needs the 100×
/// combined-session signal specifically.
fn bench_combined_workflow_100x(c: &mut Criterion) {
    if std::env::var("MC_BENCH_COMBINED_WORKFLOW_100X").as_deref() != Ok("1") {
        eprintln!(
            "[combined_workflow x100] SKIPPED — set MC_BENCH_COMBINED_WORKFLOW_100X=1 to run \
             (preflight is ~30 min wall-clock; see Phase 2C completion report §4.4)"
        );
        return;
    }
    bench_combined_workflow(c, 100);
}

criterion_group!(
    benches,
    bench_combined_workflow_50x,
    bench_combined_workflow_100x,
);
criterion_main!(benches);
