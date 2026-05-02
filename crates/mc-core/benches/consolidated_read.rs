//! Phase 1B / 2A benchmark: consolidated reads.
//!
//! Maps to brief §11.2 (`consolidation_read.rs`) and the Phase 1B
//! handoff's benchmark category 3 ("Consolidated read"). Reads the
//! Q1_2026 × Paid_Media × Florida slice for Spend, CPC, Revenue, and
//! Gross_Profit, then a wider FY × All_Channels × USA Spend roll-up.
//!
//! ## Cache state — warm vs cold
//!
//! - **Warm** (`_warm` labels) — Phase 1B path. The setup calls
//!   `materialize_all_dependencies` and reads the target consolidation
//!   once before timing, so `Cube::read_consolidated` is on its
//!   cache-hit path (~67 ns). These rows measure the cache lookup,
//!   not the consolidation walk.
//! - **Cold** (`_cold` labels) — Phase 2A addition. The per-iteration
//!   setup builds + materializes the cube, then issues an idempotent
//!   write at a child leaf inside the target consolidation's subtree
//!   to mark the consolidated coord dirty. The bench timer covers the
//!   full `Cube::read` recompute path: `is_consolidated_coord` →
//!   cache miss → `Consolidator::read` walking every child leaf and
//!   running the per-measure aggregation. This is the operation brief
//!   §11.2's 1A/1B ceilings (50 µs / 1 ms / 20 ms / 5 ms / 2 ms range)
//!   were calibrated against, closing PERF.md §6.3's deferral note.
//!
//! ## Cold-state verification (Phase 2A handoff requirement)
//!
//! Each cold variant's per-iteration setup `assert!`s
//! `cube.dirty().is_dirty(&target_coord)` before the timed read so a
//! future maintainer cannot accidentally measure a warm hit. Goldens
//! are verified once on a cold-state preflight (`assert_cold_golden`)
//! before any sample is recorded; if any cold value drifts from the
//! brief §4.5.1 numbers the preflight aborts the bench.

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use mc_core::{CellCoordinate, Cube, ScalarValue, WriteIntent, WritebackRequest};
use mc_fixtures::{
    build_acme_cube, coord, materialize_all_dependencies, write_canonical_inputs, AcmeRefs,
};

fn build_for_consolidation() -> (Cube, AcmeRefs) {
    let (mut cube, refs) = build_acme_cube().expect("acme fixture must build");
    write_canonical_inputs(&mut cube, &refs).expect("canonical inputs");
    materialize_all_dependencies(&mut cube, &refs).expect("materialize");
    (cube, refs)
}

fn consol(
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

/// Brief §4.5.1 golden values for the four consolidated cells in this
/// suite (the demo CLI also asserts on these). Per Phase 1B handoff:
/// "Confirm results still match golden values before timing."
fn assert_consolidated_golden(cube: &mut Cube, refs: &AcmeRefs) {
    let principal = refs.root_principal;
    let read = |c: &CellCoordinate, cube: &mut Cube| -> f64 {
        cube.read(c, principal)
            .expect("read")
            .value
            .as_f64()
            .expect("F64")
    };

    // Q1 × Paid_Search × Tampa Spend = 33_000 (3 leaves).
    let q1ps_tampa_spend = consol(
        cube,
        refs,
        refs.q1_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    let v = read(&q1ps_tampa_spend, cube);
    assert!(
        (v - 33_000.0).abs() < 1e-6,
        "Q1×Paid_Search×Tampa Spend mismatch: {v}"
    );

    // Mar × Paid_Search × Florida Spend = 35_100 (3 leaves).
    let mar_ps_fla_spend = consol(
        cube,
        refs,
        refs.mar_2026,
        refs.paid_search,
        refs.florida,
        refs.spend,
    );
    let v = read(&mar_ps_fla_spend, cube);
    assert!(
        (v - 35_100.0).abs() < 1e-6,
        "Mar×Paid_Search×Florida Spend mismatch: {v}"
    );

    // Q1 × Paid_Media × Florida Spend = 329_400 (27 leaves).
    let q1pm_fla_spend = consol(
        cube,
        refs,
        refs.q1_2026,
        refs.paid_media,
        refs.florida,
        refs.spend,
    );
    let v = read(&q1pm_fla_spend, cube);
    assert!(
        (v - 329_400.0).abs() < 1e-6,
        "Q1×Paid_Media×Florida Spend mismatch: {v}"
    );

    // Q1 × Paid_Search × Florida CPC ≈ 1.5202381 (weighted average,
    // 9 leaves: 3 months × 1 channel × 3 markets).
    let q1ps_fla_cpc = consol(
        cube,
        refs,
        refs.q1_2026,
        refs.paid_search,
        refs.florida,
        refs.cpc,
    );
    let v = read(&q1ps_fla_cpc, cube);
    assert!(
        (v - 1.520_238_1).abs() < 1e-6,
        "Q1×Paid_Search×Florida CPC mismatch: {v}"
    );
}

fn bench_warm(c: &mut Criterion, label: &str, target: fn(&Cube, &AcmeRefs) -> CellCoordinate) {
    let (mut cube, refs) = build_for_consolidation();
    assert_consolidated_golden(&mut cube, &refs);
    let coord = target(&cube, &refs);
    // Warm the consolidation cache.
    let _ = cube.read(&coord, refs.root_principal).expect("warmup read");
    c.bench_function(label, |b| {
        b.iter(|| {
            let v = cube
                .read(black_box(&coord), refs.root_principal)
                .expect("read");
            black_box(v);
        });
    });
}

// ---- 27-leaf consolidation: Q1 × Paid_Media × Florida ----
fn bench_consol_q1_pm_fla_spend(c: &mut Criterion) {
    bench_warm(
        c,
        "consolidation_warm/Q1_PaidMedia_Florida/Spend (27 leaves)",
        |cube, refs| {
            consol(
                cube,
                refs,
                refs.q1_2026,
                refs.paid_media,
                refs.florida,
                refs.spend,
            )
        },
    );
}

fn bench_consol_q1_pm_fla_cpc(c: &mut Criterion) {
    // Q1 × Paid_Media × Florida CPC: weighted average, 27 child leaves.
    // Per brief §11.2 `bench_consolidation_weighted_avg_27`.
    bench_warm(
        c,
        "consolidation_warm/Q1_PaidMedia_Florida/CPC (27 leaves, weighted avg)",
        |cube, refs| {
            consol(
                cube,
                refs,
                refs.q1_2026,
                refs.paid_media,
                refs.florida,
                refs.cpc,
            )
        },
    );
}

fn bench_consol_q1_pm_fla_revenue(c: &mut Criterion) {
    // Per brief §11.2 `bench_consolidation_revenue_27_leaves` — every
    // child leaf evaluates the full Revenue rule chain.
    bench_warm(
        c,
        "consolidation_warm/Q1_PaidMedia_Florida/Revenue (27 leaves, rule chain)",
        |cube, refs| {
            consol(
                cube,
                refs,
                refs.q1_2026,
                refs.paid_media,
                refs.florida,
                refs.revenue,
            )
        },
    );
}

fn bench_consol_q1_pm_fla_gross_profit(c: &mut Criterion) {
    bench_warm(
        c,
        "consolidation_warm/Q1_PaidMedia_Florida/Gross_Profit (27 leaves, rule chain)",
        |cube, refs| {
            consol(
                cube,
                refs,
                refs.q1_2026,
                refs.paid_media,
                refs.florida,
                refs.gross_profit,
            )
        },
    );
}

// ---- 3-leaf and 420-leaf reference points (brief §11.2). ----
fn bench_consol_q1_ps_tampa_spend(c: &mut Criterion) {
    bench_warm(
        c,
        "consolidation_warm/Q1_PaidSearch_Tampa/Spend (3 leaves)",
        |cube, refs| {
            consol(
                cube,
                refs,
                refs.q1_2026,
                refs.paid_search,
                refs.tampa,
                refs.spend,
            )
        },
    );
}

fn bench_consol_fy_all_usa_spend(c: &mut Criterion) {
    // FY × All_Channels × USA Spend = 12 months × 5 channels × 7 markets
    // = 420 leaf reads.
    bench_warm(
        c,
        "consolidation_warm/FY_AllChannels_USA/Spend (420 leaves)",
        |cube, refs| {
            consol(
                cube,
                refs,
                refs.fy_2026,
                refs.all_channels,
                refs.usa,
                refs.spend,
            )
        },
    );
}

// ---------------------------------------------------------------------------
// Phase 2A: cold consolidation reads
// ---------------------------------------------------------------------------

/// What measure we're going to write at the invalidating leaf to force
/// the consolidated cache miss. For Spend consolidations we write
/// Spend; for the CPC weighted-average we write CPC; for Revenue (a
/// derived measure that cannot be written directly) we write Spend
/// because `compute_dirty_ancestors` includes every Derived measure
/// when a leaf input is written.
#[derive(Clone, Copy, Debug)]
enum InvalidatingMeasure {
    Spend,
    Cpc,
}

fn invalidating_leaf_coord(
    cube: &Cube,
    refs: &AcmeRefs,
    measure: InvalidatingMeasure,
) -> CellCoordinate {
    let measure_id = match measure {
        InvalidatingMeasure::Spend => refs.spend,
        InvalidatingMeasure::Cpc => refs.cpc,
    };
    // Mar_2026 / Paid_Search / Tampa is a leaf in every benched
    // subtree (Q1×Paid_Search×Tampa, Q1×Paid_Media×Florida,
    // FY×All_Channels×USA), so a single invalidation point covers
    // every cold variant.
    coord(
        cube.id,
        refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        measure_id,
    )
}

/// Issue a single idempotent write at the invalidating leaf, asserting
/// the consolidated `target` coord becomes dirty. Returns once the
/// cube is in a verified cold state for `target`.
///
/// Idempotent in the sense that we re-write the same canonical Spend
/// or CPC value the leaf already holds — the consolidated value at
/// the ancestor is unchanged, but the revision bumps and the cache
/// entry is invalidated. This lets us run the brief §4.5.1 golden
/// check on the cold path without separately tracking pre-/post-write
/// expected numbers.
fn force_cold(
    cube: &mut Cube,
    refs: &AcmeRefs,
    target: &CellCoordinate,
    measure: InvalidatingMeasure,
) {
    let leaf = invalidating_leaf_coord(cube, refs, measure);
    // Mar=time_idx=3, Paid_Search=channel_idx=0, Tampa=market_idx=0.
    let canon = mc_fixtures::canonical_inputs_for(3, 0, 0);
    let value = match measure {
        InvalidatingMeasure::Spend => canon.spend,
        InvalidatingMeasure::Cpc => canon.cpc,
    };
    cube.write(WritebackRequest {
        coord: leaf,
        new_value: ScalarValue::F64(value),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("invalidating write must succeed");
    assert!(
        cube.dirty().is_dirty(target),
        "cold-read setup failed: target consolidated coord is not dirty"
    );
}

/// Phase 2A handoff requirement: confirm cold-path results match brief
/// §4.5.1 goldens before any cold timing is recorded. Builds a fresh
/// cube, forces it cold for each target, performs the cold read,
/// asserts the value matches the golden. Run once before the cold
/// bench loop; the per-iteration `force_cold` call relies on the same
/// codepath this preflight exercises.
fn assert_cold_golden(measure: InvalidatingMeasure, target_label: &str, golden: f64) {
    let (mut cube, refs) = build_for_consolidation();
    let target = match target_label {
        "Q1_PaidSearch_Tampa_Spend" => consol(
            &cube,
            &refs,
            refs.q1_2026,
            refs.paid_search,
            refs.tampa,
            refs.spend,
        ),
        "Q1_PaidMedia_Florida_Spend" => consol(
            &cube,
            &refs,
            refs.q1_2026,
            refs.paid_media,
            refs.florida,
            refs.spend,
        ),
        "Q1_PaidMedia_Florida_CPC" => consol(
            &cube,
            &refs,
            refs.q1_2026,
            refs.paid_media,
            refs.florida,
            refs.cpc,
        ),
        "Q1_PaidMedia_Florida_Revenue" => consol(
            &cube,
            &refs,
            refs.q1_2026,
            refs.paid_media,
            refs.florida,
            refs.revenue,
        ),
        "FY_AllChannels_USA_Spend" => consol(
            &cube,
            &refs,
            refs.fy_2026,
            refs.all_channels,
            refs.usa,
            refs.spend,
        ),
        other => panic!("unknown cold-golden target label: {other}"),
    };
    force_cold(&mut cube, &refs, &target, measure);
    let v = cube
        .read(&target, refs.root_principal)
        .expect("cold read")
        .value
        .as_f64()
        .expect("F64");
    assert!(
        (v - golden).abs() < 1e-6,
        "cold-path golden mismatch for {target_label}: got {v}, expected {golden}"
    );
}

fn bench_cold(
    c: &mut Criterion,
    label: &str,
    measure: InvalidatingMeasure,
    target: fn(&Cube, &AcmeRefs) -> CellCoordinate,
) {
    c.bench_function(label, |b| {
        b.iter_batched_ref(
            || {
                let (mut cube, refs) = build_for_consolidation();
                let coord = target(&cube, &refs);
                force_cold(&mut cube, &refs, &coord, measure);
                (cube, refs, coord)
            },
            |(cube, refs, coord)| {
                let v = cube
                    .read(black_box(coord), refs.root_principal)
                    .expect("cold read");
                black_box(v);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_cold_q1_ps_tampa_spend(c: &mut Criterion) {
    // Brief §11.2 `bench_consolidation_3_leaves` ceiling 1A < 50 µs,
    // 1B < 3 µs. 3 child leaves: Jan/Feb/Mar × Paid_Search × Tampa.
    assert_cold_golden(
        InvalidatingMeasure::Spend,
        "Q1_PaidSearch_Tampa_Spend",
        33_000.0,
    );
    bench_cold(
        c,
        "consolidation_cold/Q1_PaidSearch_Tampa/Spend (3 leaves)",
        InvalidatingMeasure::Spend,
        |cube, refs| {
            consol(
                cube,
                refs,
                refs.q1_2026,
                refs.paid_search,
                refs.tampa,
                refs.spend,
            )
        },
    );
}

fn bench_cold_q1_pm_fla_spend(c: &mut Criterion) {
    // Brief §11.2 `bench_consolidation_27_leaves` ceiling 1A < 1 ms,
    // 1B < 30 µs. 27 child leaves: Q1 × Paid_Media × Florida.
    assert_cold_golden(
        InvalidatingMeasure::Spend,
        "Q1_PaidMedia_Florida_Spend",
        329_400.0,
    );
    bench_cold(
        c,
        "consolidation_cold/Q1_PaidMedia_Florida/Spend (27 leaves)",
        InvalidatingMeasure::Spend,
        |cube, refs| {
            consol(
                cube,
                refs,
                refs.q1_2026,
                refs.paid_media,
                refs.florida,
                refs.spend,
            )
        },
    );
}

fn bench_cold_q1_pm_fla_cpc(c: &mut Criterion) {
    // Brief §11.2 `bench_consolidation_weighted_avg_27` ceiling 1A < 2
    // ms, 1B < 100 µs. CPC is an Input that aggregates with
    // WeightedAverage(weight=Spend); writing CPC at one leaf is the
    // only way to dirty the consolidated CPC cache (Inputs are not in
    // measures_to_mark when a different Input is written). Brief
    // §4.5.1 golden for Q1×Paid_Search×Florida CPC ≈ 1.5202381 (9
    // leaves: 3 months × 1 channel × 3 markets); Q1×Paid_Media×Florida
    // CPC golden recomputed below.
    let canon_27_cpc = q1_paid_media_florida_cpc_golden();
    assert_cold_golden(
        InvalidatingMeasure::Cpc,
        "Q1_PaidMedia_Florida_CPC",
        canon_27_cpc,
    );
    bench_cold(
        c,
        "consolidation_cold/Q1_PaidMedia_Florida/CPC (27 leaves, weighted avg)",
        InvalidatingMeasure::Cpc,
        |cube, refs| {
            consol(
                cube,
                refs,
                refs.q1_2026,
                refs.paid_media,
                refs.florida,
                refs.cpc,
            )
        },
    );
}

fn bench_cold_q1_pm_fla_revenue(c: &mut Criterion) {
    // Brief §11.2 `bench_consolidation_revenue_27_leaves` ceiling 1A <
    // 5 ms, 1B < 200 µs. Revenue is Derived; writing Spend at any
    // child leaf marks every consolidated Derived coord dirty (per
    // `compute_dirty_ancestors`'s measures_to_mark), so the standard
    // Spend invalidation path applies.
    let canon_27_rev = q1_paid_media_florida_revenue_golden();
    assert_cold_golden(
        InvalidatingMeasure::Spend,
        "Q1_PaidMedia_Florida_Revenue",
        canon_27_rev,
    );
    bench_cold(
        c,
        "consolidation_cold/Q1_PaidMedia_Florida/Revenue (27 leaves, rule chain)",
        InvalidatingMeasure::Spend,
        |cube, refs| {
            consol(
                cube,
                refs,
                refs.q1_2026,
                refs.paid_media,
                refs.florida,
                refs.revenue,
            )
        },
    );
}

fn bench_cold_fy_all_usa_spend(c: &mut Criterion) {
    // Brief §11.2 `bench_consolidation_420_leaves` ceiling 1A < 20 ms,
    // 1B < 500 µs. 420 child leaves: 12 months × 5 channels × 7
    // markets.
    let canon_420 = fy_all_channels_usa_spend_golden();
    assert_cold_golden(
        InvalidatingMeasure::Spend,
        "FY_AllChannels_USA_Spend",
        canon_420,
    );
    bench_cold(
        c,
        "consolidation_cold/FY_AllChannels_USA/Spend (420 leaves)",
        InvalidatingMeasure::Spend,
        |cube, refs| {
            consol(
                cube,
                refs,
                refs.fy_2026,
                refs.all_channels,
                refs.usa,
                refs.spend,
            )
        },
    );
}

// --- Closed-form goldens for the cold-only consolidation targets the
// warm `assert_consolidated_golden` doesn't already cover. ---

/// Q1 × Paid_Media × Florida CPC weighted by Spend, computed from the
/// brief §4.5 closed-form inputs (3 months × 3 channels × 3 markets).
fn q1_paid_media_florida_cpc_golden() -> f64 {
    let times: [u32; 3] = [1, 2, 3]; // Jan, Feb, Mar
    let channels: [u32; 3] = [0, 1, 2]; // Paid_Search, Paid_Social, Display
    let markets: [u32; 3] = [0, 1, 2]; // Tampa, Orlando, Miami
    let mut numer = 0.0;
    let mut denom = 0.0;
    for &t in &times {
        for &c in &channels {
            for &m in &markets {
                let inp = mc_fixtures::canonical_inputs_for(t, c, m);
                numer += inp.cpc * inp.spend;
                denom += inp.spend;
            }
        }
    }
    numer / denom
}

/// Q1 × Paid_Media × Florida Revenue (sum of leaf Revenue values
/// computed via the canonical rule chain Spend → Clicks → Leads →
/// Customers → Revenue).
fn q1_paid_media_florida_revenue_golden() -> f64 {
    let times: [u32; 3] = [1, 2, 3];
    let channels: [u32; 3] = [0, 1, 2];
    let markets: [u32; 3] = [0, 1, 2];
    let mut sum = 0.0;
    for &t in &times {
        for &c in &channels {
            for &m in &markets {
                sum += mc_fixtures::canonical_inputs_for(t, c, m).revenue();
            }
        }
    }
    sum
}

/// FY × All_Channels × USA Spend = sum over all 420 leaves of the
/// canonical Spend formula.
fn fy_all_channels_usa_spend_golden() -> f64 {
    let mut sum = 0.0;
    for t in 1..=12 {
        for c in 0..5 {
            for m in 0..7 {
                sum += mc_fixtures::canonical_inputs_for(t, c, m).spend;
            }
        }
    }
    sum
}

// ---------------------------------------------------------------------------
// Phase 2C — scaled cold consolidation variants.
//
// Per `docs/handoffs/phase-2c-handoff.md` §"Phase 2C scope" item 2: extend
// `bench_consolidation_cold` at the 27-leaf and 420-leaf fan-outs
// (Q1×Paid_Media×Florida × Spend and FY×All_Channels×USA × Spend) for
// 10× / 50× / 100× cubes. The same logical coord is read at every scale;
// because Florida widens with scale (original 3 cities plus 3 extras per
// scale tick), the actual leaf count under Q1×Paid_Media×Florida is
// 27 × scale at scale N, and FY×All_Channels×USA is 420 × scale.
//
// Cold-state goldens are computed self-consistently from `canonical_inputs_for`
// against the cube's actual market-leaf list (the scale=1 case is
// independently verified against brief §4.5.1 by `assert_cold_golden`
// above and by the equivalence test in mc-fixtures).
// ---------------------------------------------------------------------------

use mc_fixtures::{
    build_scaled_acme_cube_100x, build_scaled_acme_cube_10x, build_scaled_acme_cube_50x,
    materialize_all_dependencies_scaled, write_canonical_inputs_scaled, ScaledAcmeRefs,
    ScaledMarketLeaf,
};

fn build_for_consolidation_scaled(scale: u32) -> (Cube, ScaledAcmeRefs) {
    let (mut cube, refs) = match scale {
        10 => build_scaled_acme_cube_10x(),
        50 => build_scaled_acme_cube_50x(),
        100 => build_scaled_acme_cube_100x(),
        other => panic!("unsupported scale: {other}"),
    }
    .expect("scaled acme fixture must build");
    write_canonical_inputs_scaled(&mut cube, &refs).expect("scaled inputs");
    materialize_all_dependencies_scaled(&mut cube, &refs).expect("scaled materialize");
    (cube, refs)
}

fn consol_scaled(
    cube: &Cube,
    refs: &ScaledAcmeRefs,
    time: mc_core::ElementId,
    channel: mc_core::ElementId,
    market: mc_core::ElementId,
    measure: mc_core::ElementId,
) -> CellCoordinate {
    coord(
        cube.id,
        &refs.base,
        refs.base.scen_baseline,
        refs.base.ver_working,
        time,
        channel,
        market,
        measure,
    )
}

fn invalidating_leaf_coord_scaled(cube: &Cube, refs: &ScaledAcmeRefs) -> CellCoordinate {
    // Mar_2026 / Paid_Search / Tampa is a leaf inside both
    // Q1×Paid_Media×Florida and FY×All_Channels×USA at every scale —
    // Tampa is one of the seven preserved Acme base cities.
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

fn force_cold_scaled(cube: &mut Cube, refs: &ScaledAcmeRefs, target: &CellCoordinate) {
    let leaf = invalidating_leaf_coord_scaled(cube, refs);
    let canon = mc_fixtures::canonical_inputs_for(3, 0, 0); // Mar=3, Paid_Search=0, Tampa=0
    cube.write(WritebackRequest {
        coord: leaf,
        new_value: ScalarValue::F64(canon.spend),
        principal: refs.base.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("invalidating write must succeed");
    assert!(
        cube.dirty().is_dirty(target),
        "cold-read setup failed: target consolidated coord is not dirty"
    );
}

/// Self-consistent golden for Q1×Paid_Media×Florida × Spend at scale N:
/// sum over (3 Q1 months × 3 Paid_Media leaves × all-Florida-cities) of
/// `canonical_inputs_for(t, c, m_idx).spend`. Florida-cities = base 3
/// (market_idx 0, 1, 2) + extras whose `parent_state` is Florida.
fn q1_paid_media_florida_spend_golden(refs: &ScaledAcmeRefs) -> f64 {
    let times: [u32; 3] = [1, 2, 3]; // Jan, Feb, Mar
    let channels: [u32; 3] = [0, 1, 2]; // Paid_Search, Paid_Social, Display
    let florida_leaves = florida_market_leaves(refs);
    let mut sum = 0.0;
    for &t in &times {
        for &c in &channels {
            for leaf in &florida_leaves {
                sum += mc_fixtures::canonical_inputs_for(t, c, leaf.market_idx).spend;
            }
        }
    }
    sum
}

/// Self-consistent golden for FY×All_Channels×USA × Spend at scale N:
/// sum over the entire 12 × 5 × (7N) input grid.
fn fy_all_channels_usa_spend_golden_scaled(refs: &ScaledAcmeRefs) -> f64 {
    let mut sum = 0.0;
    for t in 1..=12 {
        for c in 0..5 {
            for leaf in &refs.all_market_leaves {
                sum += mc_fixtures::canonical_inputs_for(t, c, leaf.market_idx).spend;
            }
        }
    }
    sum
}

/// Filter Market leaves whose parent state is Florida. Used by the
/// 27-leaf cold golden helper. Identifies "Florida" by referencing the
/// known base-city Tampa's parent state on `refs.base`.
fn florida_market_leaves(refs: &ScaledAcmeRefs) -> Vec<ScaledMarketLeaf> {
    let florida = refs.base.florida;
    refs.all_market_leaves
        .iter()
        .filter(|l| l.parent_state == florida)
        .cloned()
        .collect()
}

fn assert_cold_golden_scaled(scale: u32, target_label: &str, golden: f64) {
    let (mut cube, refs) = build_for_consolidation_scaled(scale);
    let target = match target_label {
        "Q1_PaidMedia_Florida_Spend" => consol_scaled(
            &cube,
            &refs,
            refs.base.q1_2026,
            refs.base.paid_media,
            refs.base.florida,
            refs.base.spend,
        ),
        "FY_AllChannels_USA_Spend" => consol_scaled(
            &cube,
            &refs,
            refs.base.fy_2026,
            refs.base.all_channels,
            refs.base.usa,
            refs.base.spend,
        ),
        other => panic!("unknown scaled cold-golden target: {other}"),
    };
    force_cold_scaled(&mut cube, &refs, &target);
    let v = cube
        .read(&target, refs.base.root_principal)
        .expect("cold read")
        .value
        .as_f64()
        .expect("F64");
    assert!(
        (v - golden).abs() / golden.abs().max(1.0) < 1e-9,
        "scaled cold-path golden mismatch for {target_label} at {scale}x: \
         got {v}, expected {golden}"
    );
}

fn bench_cold_scaled(
    c: &mut Criterion,
    label: &str,
    scale: u32,
    target: fn(&Cube, &ScaledAcmeRefs) -> CellCoordinate,
) {
    c.bench_function(label, |b| {
        b.iter_batched_ref(
            || {
                let (mut cube, refs) = build_for_consolidation_scaled(scale);
                let coord = target(&cube, &refs);
                force_cold_scaled(&mut cube, &refs, &coord);
                (cube, refs, coord)
            },
            |(cube, refs, coord)| {
                let v = cube
                    .read(black_box(coord), refs.base.root_principal)
                    .expect("cold read");
                black_box(v);
            },
            BatchSize::SmallInput,
        );
    });
}

/// Gate the scaled cold-consolidation benches behind an env var. The
/// reason: each setup call to `build_for_consolidation_scaled` does a
/// fresh build + 2520×scale canonical writes + 2100×scale cold reads
/// (materialize). At 100× that's ~10 minutes per setup invocation,
/// and criterion calls setup MANY times (one per iteration). The
/// targeted Phase 2C gate (run_phase_2c_targeted.sh) skips these by
/// default; set `MC_BENCH_CONSOL_SCALED=1` to opt in for a re-run when
/// Phase 2D actually needs the data.
fn scaled_consol_disabled() -> bool {
    std::env::var("MC_BENCH_CONSOL_SCALED").as_deref() != Ok("1")
}

// 27-leaf at 1× → 27×scale leaves at scale N (Q1 × Paid_Media × Florida).
fn bench_cold_q1_pm_fla_spend_scaled(c: &mut Criterion, scale: u32) {
    if scaled_consol_disabled() {
        eprintln!(
            "[consolidation_cold/Q1_PaidMedia_Florida/Spend/{scale}x] SKIPPED — set MC_BENCH_CONSOL_SCALED=1 to run \
             (per-setup build+load+materialize takes ~minutes at 100×; see Phase 2C completion report §6 deferrals)"
        );
        return;
    }
    let (_, refs) = build_for_consolidation_scaled(scale);
    let golden = q1_paid_media_florida_spend_golden(&refs);
    drop(refs);
    assert_cold_golden_scaled(scale, "Q1_PaidMedia_Florida_Spend", golden);
    let leaves = 27 * scale;
    let label = format!("consolidation_cold/Q1_PaidMedia_Florida/Spend/{scale}x ({leaves} leaves)");
    bench_cold_scaled(c, &label, scale, |cube, refs| {
        consol_scaled(
            cube,
            refs,
            refs.base.q1_2026,
            refs.base.paid_media,
            refs.base.florida,
            refs.base.spend,
        )
    });
}

// 420-leaf at 1× → 420×scale leaves at scale N (FY × All_Channels × USA).
fn bench_cold_fy_all_usa_spend_scaled(c: &mut Criterion, scale: u32) {
    if scaled_consol_disabled() {
        eprintln!(
            "[consolidation_cold/FY_AllChannels_USA/Spend/{scale}x] SKIPPED — set MC_BENCH_CONSOL_SCALED=1 to run"
        );
        return;
    }
    let (_, refs) = build_for_consolidation_scaled(scale);
    let golden = fy_all_channels_usa_spend_golden_scaled(&refs);
    drop(refs);
    assert_cold_golden_scaled(scale, "FY_AllChannels_USA_Spend", golden);
    let leaves = 420 * scale;
    let label = format!("consolidation_cold/FY_AllChannels_USA/Spend/{scale}x ({leaves} leaves)");
    bench_cold_scaled(c, &label, scale, |cube, refs| {
        consol_scaled(
            cube,
            refs,
            refs.base.fy_2026,
            refs.base.all_channels,
            refs.base.usa,
            refs.base.spend,
        )
    });
}

fn bench_cold_q1_pm_fla_spend_10x(c: &mut Criterion) {
    bench_cold_q1_pm_fla_spend_scaled(c, 10);
}
fn bench_cold_q1_pm_fla_spend_50x(c: &mut Criterion) {
    bench_cold_q1_pm_fla_spend_scaled(c, 50);
}
fn bench_cold_q1_pm_fla_spend_100x(c: &mut Criterion) {
    bench_cold_q1_pm_fla_spend_scaled(c, 100);
}
fn bench_cold_fy_all_usa_spend_10x(c: &mut Criterion) {
    bench_cold_fy_all_usa_spend_scaled(c, 10);
}
fn bench_cold_fy_all_usa_spend_50x(c: &mut Criterion) {
    bench_cold_fy_all_usa_spend_scaled(c, 50);
}
fn bench_cold_fy_all_usa_spend_100x(c: &mut Criterion) {
    bench_cold_fy_all_usa_spend_scaled(c, 100);
}

criterion_group!(
    benches,
    bench_consol_q1_ps_tampa_spend,
    bench_consol_q1_pm_fla_spend,
    bench_consol_q1_pm_fla_cpc,
    bench_consol_q1_pm_fla_revenue,
    bench_consol_q1_pm_fla_gross_profit,
    bench_consol_fy_all_usa_spend,
    bench_cold_q1_ps_tampa_spend,
    bench_cold_q1_pm_fla_spend,
    bench_cold_q1_pm_fla_cpc,
    bench_cold_q1_pm_fla_revenue,
    bench_cold_fy_all_usa_spend,
    // Phase 2C scaled cold variants.
    bench_cold_q1_pm_fla_spend_10x,
    bench_cold_q1_pm_fla_spend_50x,
    bench_cold_q1_pm_fla_spend_100x,
    bench_cold_fy_all_usa_spend_10x,
    bench_cold_fy_all_usa_spend_50x,
    bench_cold_fy_all_usa_spend_100x,
);
criterion_main!(benches);
