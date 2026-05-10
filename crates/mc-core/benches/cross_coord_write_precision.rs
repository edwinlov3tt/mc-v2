//! ADR-0027 benchmarks: cross-coordinate write precision.
//!
//! Bench 1 (write-dependent): write one input -> read one dependent derived
//! cell. Latency should be proportional to actual fan-out, not cube size.
//!
//! Bench 2 (visible-grid-unrelated-write): hot grid + unrelated write ->
//! all cache hits. Validates Phase 8 daemon grid-editing performance.

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use mc_core::{
    AggregationRule, CellCoordinate, CellDataType, Cube, CubeId, Dimension, DimensionKind, Element,
    ElementId, Expr, IdGenerator, MeasureRole, PrincipalId, Rule, ScalarValue, Scope, WriteIntent,
    WritebackRequest,
};

/// Build a cube for cross-coord precision benchmarks.
///
/// Shape: 10 channels x 5 markets x 12 time periods x 4 measures
///   - 2 input measures (Spend, CPC)
///   - 2 derived with prev() (PrevSpend, PrevCPC)
///
/// Total cells: 10 * 5 * 12 * 4 = 2400
/// Derived cells: 10 * 5 * 12 * 2 = 1200
struct BenchCube {
    cube: Cube,
    cube_id: CubeId,
    channels: Vec<ElementId>,
    markets: Vec<ElementId>,
    times: Vec<ElementId>,
    spend: ElementId,
    cpc: ElementId,
    prev_spend: ElementId,
    prev_cpc: ElementId,
    principal: PrincipalId,
}

fn build_bench_cube() -> BenchCube {
    let g = IdGenerator::new();
    let cube_id = g.cube();
    let channel_dim_id = g.dimension();
    let market_dim_id = g.dimension();
    let time_dim_id = g.dimension();
    let measure_dim_id = g.dimension();
    let principal = g.principal();

    // 10 channels
    let mut channel_builder =
        Dimension::builder(channel_dim_id, "Channel", DimensionKind::Standard);
    let mut channels = Vec::new();
    for i in 0..10 {
        let id = g.element();
        channels.push(id);
        channel_builder = channel_builder
            .add_element(Element::leaf(id, format!("Ch{i}"), channel_dim_id))
            .expect("ok");
    }
    let channel_dim = channel_builder.build().expect("channel dim");

    // 5 markets
    let mut market_builder = Dimension::builder(market_dim_id, "Market", DimensionKind::Standard);
    let mut markets = Vec::new();
    for i in 0..5 {
        let id = g.element();
        markets.push(id);
        market_builder = market_builder
            .add_element(Element::leaf(id, format!("Mkt{i}"), market_dim_id))
            .expect("ok");
    }
    let market_dim = market_builder.build().expect("market dim");

    // 12 time periods
    let mut time_builder = Dimension::builder(time_dim_id, "Time", DimensionKind::Standard);
    let mut times = Vec::new();
    for i in 0..12 {
        let id = g.element();
        times.push(id);
        time_builder = time_builder
            .add_element(Element::leaf(id, format!("T{i}"), time_dim_id))
            .expect("ok");
    }
    let time_dim = time_builder.build().expect("time dim");

    // 4 measures: Spend (input), CPC (input), PrevSpend (derived), PrevCPC (derived)
    let spend = g.element();
    let cpc = g.element();
    let prev_spend = g.element();
    let prev_cpc = g.element();

    let measure_dim = Dimension::builder(measure_dim_id, "Measure", DimensionKind::Measure)
        .add_element(Element::measure(
            spend,
            "Spend",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Input,
            AggregationRule::Sum,
        ))
        .expect("ok")
        .add_element(Element::measure(
            cpc,
            "CPC",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Input,
            AggregationRule::Sum,
        ))
        .expect("ok")
        .add_element(Element::measure(
            prev_spend,
            "PrevSpend",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Derived,
            AggregationRule::Sum,
        ))
        .expect("ok")
        .add_element(Element::measure(
            prev_cpc,
            "PrevCPC",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Derived,
            AggregationRule::Sum,
        ))
        .expect("ok")
        .build()
        .expect("measure dim");

    // PrevSpend = prev(Spend)
    let prev_spend_rule = Rule {
        id: g.rule(),
        cube: cube_id,
        target_measure: prev_spend,
        scope: Scope::AllLeaves,
        body: Expr::Prev(spend),
        declared_dependencies: vec![mc_core::DependencyDecl {
            measure: spend,
            coord_pattern: mc_core::CoordPattern::SameAsTarget,
        }],
    };

    // PrevCPC = prev(CPC)
    let prev_cpc_rule = Rule {
        id: g.rule(),
        cube: cube_id,
        target_measure: prev_cpc,
        scope: Scope::AllLeaves,
        body: Expr::Prev(cpc),
        declared_dependencies: vec![mc_core::DependencyDecl {
            measure: cpc,
            coord_pattern: mc_core::CoordPattern::SameAsTarget,
        }],
    };

    let cube = Cube::builder(cube_id, "CrossCoordBench")
        .add_dimension(channel_dim)
        .add_dimension(market_dim)
        .add_dimension(time_dim)
        .add_dimension(measure_dim)
        .measure_dimension("Measure")
        .root_principal(principal)
        .add_rule(prev_spend_rule)
        .expect("ok")
        .add_rule(prev_cpc_rule)
        .expect("ok")
        .build()
        .expect("cube build");

    BenchCube {
        cube,
        cube_id,
        channels,
        markets,
        times,
        spend,
        cpc,
        prev_spend,
        prev_cpc,
        principal,
    }
}

fn coord4(
    cube_id: CubeId,
    ch: ElementId,
    mkt: ElementId,
    time: ElementId,
    measure: ElementId,
) -> CellCoordinate {
    CellCoordinate::from_parts(cube_id, [ch, mkt, time, measure])
}

fn setup_populated(bc: &mut BenchCube) {
    let p = bc.principal;
    // Write all input cells.
    for &ch in &bc.channels {
        for &mkt in &bc.markets {
            for (ti, &t) in bc.times.iter().enumerate() {
                for &m in &[bc.spend, bc.cpc] {
                    bc.cube
                        .write(WritebackRequest {
                            coord: coord4(bc.cube_id, ch, mkt, t, m),
                            new_value: ScalarValue::F64((ti + 1) as f64 * 100.0),
                            principal: p,
                            intent: WriteIntent::Set,
                            expected_revision: None,
                            now_unix_seconds: 0,
                        })
                        .expect("write input");
                }
            }
        }
    }

    // Read all derived cells to populate graph edges.
    for &ch in &bc.channels {
        for &mkt in &bc.markets {
            for &t in &bc.times {
                for &m in &[bc.prev_spend, bc.prev_cpc] {
                    let _ = bc.cube.read(&coord4(bc.cube_id, ch, mkt, t, m), p);
                }
            }
        }
    }
}

/// Bench 1: write one input cell -> read one dependent derived cell.
/// Measures the write-then-read latency. With precise invalidation,
/// only ~5-10 cells are dirty (the fan-out of one input cell), not
/// all 1200 derived cells.
fn bench_write_dependent(c: &mut Criterion) {
    c.bench_function("cross_coord/write_dependent", |b| {
        b.iter_batched(
            || {
                let mut bc = build_bench_cube();
                setup_populated(&mut bc);
                bc
            },
            |mut bc| {
                let p = bc.principal;
                // Write Spend at (Ch0, Mkt0, T5).
                let write_coord = coord4(
                    bc.cube_id,
                    bc.channels[0],
                    bc.markets[0],
                    bc.times[5],
                    bc.spend,
                );
                bc.cube
                    .write(WritebackRequest {
                        coord: write_coord,
                        new_value: ScalarValue::F64(999.0),
                        principal: p,
                        intent: WriteIntent::Set,
                        expected_revision: None,
                        now_unix_seconds: 0,
                    })
                    .expect("write");

                // Read the dependent: PrevSpend at (Ch0, Mkt0, T6).
                // prev(Spend)[T6] reads Spend[T5] which was just written.
                let read_coord = coord4(
                    bc.cube_id,
                    bc.channels[0],
                    bc.markets[0],
                    bc.times[6],
                    bc.prev_spend,
                );
                black_box(bc.cube.read(&read_coord, p).expect("read"));
            },
            BatchSize::SmallInput,
        );
    });
}

/// Bench 2: hot grid + unrelated write -> all cache hits.
/// Write an input cell that affects NONE of the 200 cached cells,
/// then re-read all 200. With precise invalidation, all 200 should
/// be cache hits (zero recomputation).
fn bench_visible_grid_unrelated_write(c: &mut Criterion) {
    c.bench_function("cross_coord/visible_grid_unrelated_write", |b| {
        b.iter_batched(
            || {
                let mut bc = build_bench_cube();
                setup_populated(&mut bc);

                // Collect a "visible grid" of 200 derived cells at Ch0.
                let mut grid: Vec<CellCoordinate> = Vec::new();
                for &mkt in &bc.markets {
                    for &t in &bc.times {
                        for &m in &[bc.prev_spend, bc.prev_cpc] {
                            grid.push(coord4(bc.cube_id, bc.channels[0], mkt, t, m));
                        }
                    }
                }
                (bc, grid)
            },
            |(mut bc, grid)| {
                let p = bc.principal;
                // Write an input cell at Ch9 — completely unrelated to
                // the grid at Ch0.
                let write_coord = coord4(
                    bc.cube_id,
                    bc.channels[9],
                    bc.markets[4],
                    bc.times[0],
                    bc.spend,
                );
                bc.cube
                    .write(WritebackRequest {
                        coord: write_coord,
                        new_value: ScalarValue::F64(999.0),
                        principal: p,
                        intent: WriteIntent::Set,
                        expected_revision: None,
                        now_unix_seconds: 0,
                    })
                    .expect("write");

                // Re-read the entire visible grid. With precise
                // invalidation, all cells should be cache hits.
                for coord in &grid {
                    black_box(bc.cube.read(coord, p).expect("read"));
                }
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    benches,
    bench_write_dependent,
    bench_visible_grid_unrelated_write
);
criterion_main!(benches);
