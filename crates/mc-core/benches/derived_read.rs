//! Phase 1B benchmark: derived (rule-evaluated) leaf reads.
//!
//! Maps to brief §11.1's `bench_read_derived_leaf_*` rows and the Phase
//! 1B handoff's benchmark category 2 ("Derived read"). Reads each of
//! the 5 derived measures at the brief's anchor coord (Mar_2026 /
//! Paid_Search / Tampa) under cold and warm conditions.
//!
//! ## Cold vs warm definitions
//!
//! - **Warm** = re-read at the same revision after the value was already
//!   materialized into the derived-leaf cache (`cube.rs::read_derived_leaf`
//!   path). Should be the cache-hit path.
//! - **Cold** = first read at the current revision after the upstream
//!   inputs were dirtied (`mark_closure` invalidated the cached entry).
//!   The bench writes Spend at the same coord first; that bumps the
//!   revision and marks the derived cell dirty, forcing the next read
//!   to recompute the full rule chain from inputs.
//!
//! Confirms results match brief §4.5.1 golden values *before* timing.

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use mc_core::{CellCoordinate, Cube, ScalarValue, WriteIntent, WritebackRequest};
use mc_fixtures::{
    build_acme_cube, coord, materialize_all_dependencies, write_canonical_inputs, AcmeRefs,
};

/// Build Acme + write inputs + materialize all deps + warm the cache by
/// reading every derived measure at the anchor coord. Returns a cube
/// where every named derived chain at the anchor is hot.
fn build_warm() -> (Cube, AcmeRefs) {
    let (mut cube, refs) = build_acme_cube().expect("acme fixture must build");
    write_canonical_inputs(&mut cube, &refs).expect("canonical inputs");
    materialize_all_dependencies(&mut cube, &refs).expect("materialize");
    // Warm the anchor coord's derived chain explicitly so the next read
    // is a cache hit (revision unchanged, dirty bit clear).
    for measure in [
        refs.clicks,
        refs.leads,
        refs.customers,
        refs.revenue,
        refs.gross_profit,
    ] {
        let c = anchor(&cube, &refs, measure);
        let _ = cube
            .read(&c, refs.root_principal)
            .expect("warmup read must succeed");
    }
    (cube, refs)
}

/// Build a cube with the anchor coord's chain warm, then write Spend at
/// the anchor to dirty its dependents. Returns the cube — the next read
/// of any derived at the anchor coord is a *cold* read (cache miss).
fn build_cold() -> (Cube, AcmeRefs) {
    let (mut cube, refs) = build_warm();
    let spend_coord = anchor(&cube, &refs, refs.spend);
    cube.write(WritebackRequest {
        coord: spend_coord,
        new_value: ScalarValue::F64(50_000.0),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("dirty write must succeed");
    (cube, refs)
}

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

/// Sanity-check derived golden values per brief §4.5.1 before any
/// timing. Per Phase 1B handoff benchmark category 2: "Confirm results
/// still match golden values before timing".
fn assert_anchor_golden(cube: &mut Cube, refs: &AcmeRefs) {
    let principal = refs.root_principal;
    let read = |c: &CellCoordinate, cube: &mut Cube| -> f64 {
        cube.read(c, principal)
            .expect("read")
            .value
            .as_f64()
            .expect("F64")
    };
    // Anchor-cell golden chain (Mar/Paid_Search/Tampa from brief §4.5.1):
    //   Spend=11_500; Clicks=23_000/3 ≈ 7_666.67; Leads=460/3 ≈ 153.33;
    //   Customers=46/3 ≈ 15.33; Revenue=9_200/3 ≈ 3_066.67;
    //   Gross_Profit=6_440/3 ≈ 2_146.67.
    let clicks = read(&anchor(cube, refs, refs.clicks), cube);
    let leads = read(&anchor(cube, refs, refs.leads), cube);
    let customers = read(&anchor(cube, refs, refs.customers), cube);
    let revenue = read(&anchor(cube, refs, refs.revenue), cube);
    let gp = read(&anchor(cube, refs, refs.gross_profit), cube);
    assert!(
        (clicks - 23_000.0 / 3.0).abs() < 1e-6,
        "Clicks golden mismatch: {clicks}"
    );
    assert!(
        (leads - 460.0 / 3.0).abs() < 1e-6,
        "Leads golden mismatch: {leads}"
    );
    assert!(
        (customers - 46.0 / 3.0).abs() < 1e-6,
        "Customers golden mismatch: {customers}"
    );
    assert!(
        (revenue - 9_200.0 / 3.0).abs() < 1e-6,
        "Revenue golden mismatch: {revenue}"
    );
    assert!(
        (gp - 6_440.0 / 3.0).abs() < 1e-6,
        "Gross_Profit golden mismatch: {gp}"
    );
}

fn bench_warm_derived(c: &mut Criterion, label: &str, target: fn(&AcmeRefs) -> mc_core::ElementId) {
    let (mut cube, refs) = build_warm();
    assert_anchor_golden(&mut cube, &refs);
    let coord = anchor(&cube, &refs, target(&refs));
    c.bench_function(label, |b| {
        b.iter(|| {
            let v = cube
                .read(black_box(&coord), refs.root_principal)
                .expect("read");
            black_box(v);
        });
    });
}

fn bench_cold_derived(c: &mut Criterion, label: &str, target: fn(&AcmeRefs) -> mc_core::ElementId) {
    c.bench_function(label, |b| {
        b.iter_batched_ref(
            || {
                // Per-iteration setup: build a cube where the anchor
                // chain is dirty. After the first read inside the
                // closure, the cache is hot — so we only get one cold
                // read per setup. SmallInput sizes batches small enough
                // that this stays representative.
                build_cold()
            },
            |(cube, refs)| {
                let coord = anchor(cube, refs, target(refs));
                let v = cube.read(&coord, refs.root_principal).expect("read");
                black_box(v);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_warm_clicks(c: &mut Criterion) {
    bench_warm_derived(c, "read_derived_leaf_warm/Clicks", |r| r.clicks);
}
fn bench_warm_leads(c: &mut Criterion) {
    bench_warm_derived(c, "read_derived_leaf_warm/Leads", |r| r.leads);
}
fn bench_warm_customers(c: &mut Criterion) {
    bench_warm_derived(c, "read_derived_leaf_warm/Customers", |r| r.customers);
}
fn bench_warm_revenue(c: &mut Criterion) {
    bench_warm_derived(c, "read_derived_leaf_warm/Revenue", |r| r.revenue);
}
fn bench_warm_gross_profit(c: &mut Criterion) {
    bench_warm_derived(c, "read_derived_leaf_warm/Gross_Profit", |r| r.gross_profit);
}

fn bench_cold_clicks(c: &mut Criterion) {
    bench_cold_derived(c, "read_derived_leaf_cold/Clicks", |r| r.clicks);
}
fn bench_cold_leads(c: &mut Criterion) {
    bench_cold_derived(c, "read_derived_leaf_cold/Leads", |r| r.leads);
}
fn bench_cold_customers(c: &mut Criterion) {
    bench_cold_derived(c, "read_derived_leaf_cold/Customers", |r| r.customers);
}
fn bench_cold_revenue(c: &mut Criterion) {
    bench_cold_derived(c, "read_derived_leaf_cold/Revenue", |r| r.revenue);
}
fn bench_cold_gross_profit(c: &mut Criterion) {
    bench_cold_derived(c, "read_derived_leaf_cold/Gross_Profit", |r| r.gross_profit);
}

// ---------------------------------------------------------------------------
// Phase 2C — scaled Revenue cold-read variants.
//
// Per `docs/handoffs/phase-2c-handoff.md` §"Phase 2C scope" item 2: extend
// `bench_read_derived_leaf_cold` for Revenue (the rule-chain depth-5 row)
// at 10× / 50× / 100×. Anchor coord (Mar/Paid_Search/Tampa) is preserved
// across scales — only total cube size + cache state at scale change.
// ---------------------------------------------------------------------------

use mc_fixtures::{
    build_scaled_acme_cube_100x, build_scaled_acme_cube_10x, build_scaled_acme_cube_50x,
    materialize_all_dependencies_scaled, write_canonical_inputs_scaled, ScaledAcmeRefs,
};

fn build_warm_scaled(scale: u32) -> (Cube, ScaledAcmeRefs) {
    let (mut cube, refs) = match scale {
        10 => build_scaled_acme_cube_10x(),
        50 => build_scaled_acme_cube_50x(),
        100 => build_scaled_acme_cube_100x(),
        other => panic!("unsupported scale: {other}"),
    }
    .expect("scaled acme fixture must build");
    write_canonical_inputs_scaled(&mut cube, &refs).expect("scaled inputs");
    materialize_all_dependencies_scaled(&mut cube, &refs).expect("scaled materialize");
    // Warm the anchor coord's derived chain explicitly, matching the
    // 1× build_warm helper above.
    for measure in [
        refs.base.clicks,
        refs.base.leads,
        refs.base.customers,
        refs.base.revenue,
        refs.base.gross_profit,
    ] {
        let c = anchor_scaled(&cube, &refs, measure);
        let _ = cube
            .read(&c, refs.base.root_principal)
            .expect("warmup read must succeed");
    }
    (cube, refs)
}

fn build_cold_scaled(scale: u32) -> (Cube, ScaledAcmeRefs) {
    let (mut cube, refs) = build_warm_scaled(scale);
    let spend_coord = anchor_scaled(&cube, &refs, refs.base.spend);
    cube.write(WritebackRequest {
        coord: spend_coord,
        new_value: ScalarValue::F64(50_000.0),
        principal: refs.base.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("dirty write must succeed");
    (cube, refs)
}

fn anchor_scaled(
    cube: &Cube,
    refs: &ScaledAcmeRefs,
    measure: mc_core::ElementId,
) -> CellCoordinate {
    coord(
        cube.id,
        &refs.base,
        refs.base.scen_baseline,
        refs.base.ver_working,
        refs.base.mar_2026,
        refs.base.paid_search,
        refs.base.tampa,
        measure,
    )
}

fn bench_cold_revenue_scaled(c: &mut Criterion, scale: u32) {
    let label = format!("read_derived_leaf_cold/Revenue/{scale}x");
    c.bench_function(&label, |b| {
        b.iter_batched_ref(
            || build_cold_scaled(scale),
            |(cube, refs)| {
                let coord = anchor_scaled(cube, refs, refs.base.revenue);
                let v = cube.read(&coord, refs.base.root_principal).expect("read");
                black_box(v);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_cold_revenue_10x(c: &mut Criterion) {
    bench_cold_revenue_scaled(c, 10);
}
fn bench_cold_revenue_50x(c: &mut Criterion) {
    bench_cold_revenue_scaled(c, 50);
}
fn bench_cold_revenue_100x(c: &mut Criterion) {
    bench_cold_revenue_scaled(c, 100);
}

criterion_group!(
    benches,
    bench_warm_clicks,
    bench_warm_leads,
    bench_warm_customers,
    bench_warm_revenue,
    bench_warm_gross_profit,
    bench_cold_clicks,
    bench_cold_leads,
    bench_cold_customers,
    bench_cold_revenue,
    bench_cold_gross_profit,
    bench_cold_revenue_10x,
    bench_cold_revenue_50x,
    bench_cold_revenue_100x,
);
criterion_main!(benches);
