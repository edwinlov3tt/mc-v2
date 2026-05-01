//! Brief §10.3 — consolidation strategy tests.
//!
//! Most cases run against the Acme cube fixture (which uses Sum for
//! `Spend` and WeightedAverage(weight=Spend) for `CPC`). Min/Max
//! aggregation is exercised against a tiny inline 1-dim fixture since
//! the Acme cube has no Min/Max measure.
//!
//! Per CLAUDE.md §3.3 the test names are contractual.

use mc_core::{
    AggregationRule, CellCoordinate, CellDataType, Cube, CubeBuilder, CubeId, Dimension,
    DimensionKind, Element, ElementId, EngineError, Hierarchy, IdGenerator, MeasureRole,
    PrincipalId, Provenance, ScalarValue, WriteIntent, WritebackRequest,
};
use mc_fixtures::{build_acme_cube, canonical_inputs_for, coord, write_canonical_inputs};

const EPS: f64 = 1e-9;

fn assert_close(actual: f64, expected: f64, label: &str) {
    assert!(
        (actual - expected).abs() < EPS,
        "{label}: got {actual}, expected {expected}"
    );
}

// ===========================================================================
// Acme-based: write only Jan/Feb/Mar in the Q1 subtree, then consolidate.
// ===========================================================================

fn write_q1_spend_subset(
    cube: &mut Cube,
    refs: &mc_fixtures::AcmeRefs,
    months: &[(ElementId, f64)],
) {
    let cube_id = cube.id;
    for (month, value) in months {
        cube.write(WritebackRequest {
            coord: coord(
                cube_id,
                refs,
                refs.scen_baseline,
                refs.ver_working,
                *month,
                refs.paid_search,
                refs.tampa,
                refs.spend,
            ),
            new_value: ScalarValue::F64(*value),
            principal: refs.root_principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write spend");
    }
}

#[test]
fn t_sum_aggregation_with_all_leaves_present() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_q1_spend_subset(
        &mut cube,
        &refs,
        &[
            (refs.jan_2026, 10.0),
            (refs.feb_2026, 20.0),
            (refs.mar_2026, 30.0),
        ],
    );
    let cube_id = cube.id;
    let q1 = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.q1_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    let v = cube.read(&q1, refs.root_principal).expect("read");
    assert_close(v.value.as_f64().expect("F64"), 60.0, "Q1 Spend = 10+20+30");
}

#[test]
fn t_sum_aggregation_with_one_null_leaf() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_q1_spend_subset(
        &mut cube,
        &refs,
        &[(refs.jan_2026, 10.0), (refs.mar_2026, 30.0)],
    );
    let cube_id = cube.id;
    let q1 = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.q1_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    let v = cube.read(&q1, refs.root_principal).expect("read");
    assert_close(
        v.value.as_f64().expect("F64"),
        40.0,
        "Q1 Spend = Jan + Mar (Feb null skipped)",
    );
    assert!(
        matches!(v.provenance, Provenance::Consolidation { .. }),
        "got {:?}",
        v.provenance
    );
}

#[test]
fn t_sum_aggregation_with_all_null_leaves() {
    // No writes — every Q1 leaf is Null.
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let q1 = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.q1_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    let v = cube.read(&q1, refs.root_principal).expect("read");
    assert!(
        matches!(v.value, ScalarValue::Null),
        "all-null leaves → Null, got {:?}",
        v.value
    );
    match v.provenance {
        Provenance::Consolidation { child_count, .. } => {
            assert_eq!(child_count, 3, "Q1 has 3 month leaves");
        }
        other => panic!("expected Consolidation provenance, got {other:?}"),
    }
}

// ===========================================================================
// WeightedAverage on Acme's CPC measure (weight = Spend).
// ===========================================================================

fn write_q1_spend_and_cpc(
    cube: &mut Cube,
    refs: &mc_fixtures::AcmeRefs,
    spends: &[(ElementId, Option<f64>)],
    cpcs: &[(ElementId, Option<f64>)],
) {
    let cube_id = cube.id;
    for (month, v) in spends {
        if let Some(val) = v {
            cube.write(WritebackRequest {
                coord: coord(
                    cube_id,
                    refs,
                    refs.scen_baseline,
                    refs.ver_working,
                    *month,
                    refs.paid_search,
                    refs.tampa,
                    refs.spend,
                ),
                new_value: ScalarValue::F64(*val),
                principal: refs.root_principal,
                intent: WriteIntent::Set,
                expected_revision: None,
                now_unix_seconds: 0,
            })
            .expect("write spend");
        }
    }
    for (month, v) in cpcs {
        if let Some(val) = v {
            cube.write(WritebackRequest {
                coord: coord(
                    cube_id,
                    refs,
                    refs.scen_baseline,
                    refs.ver_working,
                    *month,
                    refs.paid_search,
                    refs.tampa,
                    refs.cpc,
                ),
                new_value: ScalarValue::F64(*val),
                principal: refs.root_principal,
                intent: WriteIntent::Set,
                expected_revision: None,
                now_unix_seconds: 0,
            })
            .expect("write cpc");
        }
    }
}

#[test]
fn t_weighted_average_basic() {
    // spend = [10, 20, 30], cpc = [1, 2, 3]
    // Q1 CPC = (10*1 + 20*2 + 30*3) / (10+20+30) = 140 / 60.
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_q1_spend_and_cpc(
        &mut cube,
        &refs,
        &[
            (refs.jan_2026, Some(10.0)),
            (refs.feb_2026, Some(20.0)),
            (refs.mar_2026, Some(30.0)),
        ],
        &[
            (refs.jan_2026, Some(1.0)),
            (refs.feb_2026, Some(2.0)),
            (refs.mar_2026, Some(3.0)),
        ],
    );
    let cube_id = cube.id;
    let q1_cpc = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.q1_2026,
        refs.paid_search,
        refs.tampa,
        refs.cpc,
    );
    let v = cube.read(&q1_cpc, refs.root_principal).expect("read");
    assert_close(v.value.as_f64().expect("F64"), 140.0 / 60.0, "Q1 CPC");
}

#[test]
fn t_weighted_average_with_null_weight() {
    // Feb spend Null → Feb contributes nothing.
    // Q1 CPC = (10*1 + 30*3) / (10 + 30) = 100/40.
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_q1_spend_and_cpc(
        &mut cube,
        &refs,
        &[(refs.jan_2026, Some(10.0)), (refs.mar_2026, Some(30.0))],
        &[
            (refs.jan_2026, Some(1.0)),
            (refs.feb_2026, Some(2.0)),
            (refs.mar_2026, Some(3.0)),
        ],
    );
    let cube_id = cube.id;
    let q1_cpc = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.q1_2026,
        refs.paid_search,
        refs.tampa,
        refs.cpc,
    );
    let v = cube.read(&q1_cpc, refs.root_principal).expect("read");
    assert_close(
        v.value.as_f64().expect("F64"),
        100.0 / 40.0,
        "Q1 CPC excluding null-weight Feb",
    );
}

#[test]
fn t_weighted_average_zero_total_weight() {
    // spend = [0, 0, 0] → zero total weight → Null.
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_q1_spend_and_cpc(
        &mut cube,
        &refs,
        &[
            (refs.jan_2026, Some(0.0)),
            (refs.feb_2026, Some(0.0)),
            (refs.mar_2026, Some(0.0)),
        ],
        &[
            (refs.jan_2026, Some(1.0)),
            (refs.feb_2026, Some(2.0)),
            (refs.mar_2026, Some(3.0)),
        ],
    );
    let cube_id = cube.id;
    let q1_cpc = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.q1_2026,
        refs.paid_search,
        refs.tampa,
        refs.cpc,
    );
    let v = cube.read(&q1_cpc, refs.root_principal).expect("read");
    assert!(
        matches!(v.value, ScalarValue::Null),
        "zero total weight → Null, got {:?}",
        v.value
    );
}

// ===========================================================================
// Inline tiny fixture for Min / Max aggregation rules.
// ===========================================================================

struct MinMaxFixture {
    cube: Cube,
    cube_id: CubeId,
    root: PrincipalId,
    #[allow(dead_code)]
    jan: ElementId,
    feb: ElementId,
    mar: ElementId,
    q1: ElementId,
    spend_min: ElementId,
    spend_max: ElementId,
}

fn build_min_max_cube() -> Result<MinMaxFixture, EngineError> {
    let g = IdGenerator::new();
    let cube_id = g.cube();
    let root = g.principal();
    let time_dim_id = g.dimension();
    let measure_dim_id = g.dimension();
    let jan = g.element();
    let feb = g.element();
    let mar = g.element();
    let q1 = g.element();
    let spend_min = g.element();
    let spend_max = g.element();
    let h_id = g.hierarchy();

    let hier = Hierarchy::builder(h_id, "cal", time_dim_id)
        .add_edge(q1, jan, 1.0)
        .add_edge(q1, feb, 1.0)
        .add_edge(q1, mar, 1.0)
        .build()?;

    let time_dim = Dimension::builder(time_dim_id, "Time", DimensionKind::Standard)
        .add_element(Element::leaf(jan, "Jan", time_dim_id))?
        .add_element(Element::leaf(feb, "Feb", time_dim_id))?
        .add_element(Element::leaf(mar, "Mar", time_dim_id))?
        .add_element(Element::leaf(q1, "Q1", time_dim_id))?
        .add_hierarchy(hier)?
        .default_hierarchy("cal")
        .build()?;

    let measure_dim = Dimension::builder(measure_dim_id, "Measure", DimensionKind::Measure)
        .add_element(Element::measure(
            spend_min,
            "Spend_Min",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Input,
            AggregationRule::Min,
        ))?
        .add_element(Element::measure(
            spend_max,
            "Spend_Max",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Input,
            AggregationRule::Max,
        ))?
        .build()?;

    let cube = CubeBuilder::default_for_min_max(cube_id, "MinMax")
        .add_dimension(time_dim)
        .add_dimension(measure_dim)
        .measure_dimension("Measure")
        .root_principal(root)
        .build()?;

    Ok(MinMaxFixture {
        cube,
        cube_id,
        root,
        jan,
        feb,
        mar,
        q1,
        spend_min,
        spend_max,
    })
}

trait CubeBuilderHelpersForMinMax {
    fn default_for_min_max(id: CubeId, name: &str) -> CubeBuilder;
}

impl CubeBuilderHelpersForMinMax for CubeBuilder {
    fn default_for_min_max(id: CubeId, name: &str) -> CubeBuilder {
        Cube::builder(id, name)
    }
}

fn mm_coord(cube_id: CubeId, time: ElementId, measure: ElementId) -> CellCoordinate {
    CellCoordinate::from_parts(cube_id, [time, measure])
}

#[test]
fn t_min_aggregation_with_nulls() {
    let mut f = build_min_max_cube().expect("min/max cube");
    // Jan = Null, Feb = 5, Mar = 10 → min over non-null = 5.
    f.cube
        .write(WritebackRequest {
            coord: mm_coord(f.cube_id, f.feb, f.spend_min),
            new_value: ScalarValue::F64(5.0),
            principal: f.root,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write feb");
    f.cube
        .write(WritebackRequest {
            coord: mm_coord(f.cube_id, f.mar, f.spend_min),
            new_value: ScalarValue::F64(10.0),
            principal: f.root,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write mar");
    let v = f
        .cube
        .read(&mm_coord(f.cube_id, f.q1, f.spend_min), f.root)
        .expect("read");
    assert_close(v.value.as_f64().expect("F64"), 5.0, "min ignores Null Jan");
}

#[test]
fn t_max_aggregation_with_nulls() {
    let mut f = build_min_max_cube().expect("min/max cube");
    f.cube
        .write(WritebackRequest {
            coord: mm_coord(f.cube_id, f.feb, f.spend_max),
            new_value: ScalarValue::F64(5.0),
            principal: f.root,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write feb");
    f.cube
        .write(WritebackRequest {
            coord: mm_coord(f.cube_id, f.mar, f.spend_max),
            new_value: ScalarValue::F64(10.0),
            principal: f.root,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write mar");
    let v = f
        .cube
        .read(&mm_coord(f.cube_id, f.q1, f.spend_max), f.root)
        .expect("read");
    assert_close(v.value.as_f64().expect("F64"), 10.0, "max ignores Null Jan");
}

// ===========================================================================
// Cache + invalidation behavior.
// ===========================================================================

#[test]
fn t_consolidation_caches_value_within_revision() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let q1 = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.q1_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    // First read populates the consolidation cache.
    let t0 = std::time::Instant::now();
    let v1 = cube.read(&q1, refs.root_principal).expect("read 1");
    let d1 = t0.elapsed();
    // Second read at the same revision must hit the cache.
    let t1 = std::time::Instant::now();
    let v2 = cube.read(&q1, refs.root_principal).expect("read 2");
    let d2 = t1.elapsed();
    assert_eq!(
        v1.value.as_f64(),
        v2.value.as_f64(),
        "cached value must match"
    );
    // 10× speedup per brief §10.3. d1 includes the consolidation walk
    // (3 leaf reads + sum); d2 is a single hashmap lookup. We allow a
    // 10ns floor so the assertion isn't degenerate when d1 itself is
    // sub-microsecond on fast hardware.
    let d1_ns = d1.as_nanos().max(10);
    let d2_ns = d2.as_nanos().max(1);
    assert!(
        d2_ns * 10 <= d1_ns,
        "second read must be at least 10× faster: d1={d1:?}, d2={d2:?}"
    );
}

#[test]
fn t_consolidation_recomputes_after_dependent_dirty() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let q1 = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.q1_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    let v0 = cube
        .read(&q1, refs.root_principal)
        .expect("read 1")
        .value
        .as_f64()
        .expect("F64");
    // Update March Spend.
    let mar_before = canonical_inputs_for(3, 0, 0).spend;
    let new_mar = 50_000.0;
    cube.write(WritebackRequest {
        coord: coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            refs.spend,
        ),
        new_value: ScalarValue::F64(new_mar),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write mar");
    let v1 = cube
        .read(&q1, refs.root_principal)
        .expect("read 2")
        .value
        .as_f64()
        .expect("F64");
    assert_close(
        v1,
        v0 + (new_mar - mar_before),
        "Q1 must reflect Mar update",
    );
}

#[test]
fn t_consolidation_at_root_level_in_three_dims() {
    // FY × All_Channels × USA × Spend = sum over 12 × 5 × 7 = 420 leaves.
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let root = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.fy_2026,
        refs.all_channels,
        refs.usa,
        refs.spend,
    );
    let v = cube.read(&root, refs.root_principal).expect("read");
    let mut expected = 0.0_f64;
    for t in 1..=12u32 {
        for c in 0..5u32 {
            for m in 0..7u32 {
                expected += canonical_inputs_for(t, c, m).spend;
            }
        }
    }
    assert_close(v.value.as_f64().expect("F64"), expected, "FY×All×USA Spend");
}

#[test]
fn t_consolidation_provenance_has_correct_child_count() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    // Q1 × Paid_Search × Tampa: 3 leaves contributed (Jan/Feb/Mar).
    let q1 = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.q1_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    let v = cube.read(&q1, refs.root_principal).expect("read");
    match v.provenance {
        Provenance::Consolidation { child_count, .. } => {
            assert_eq!(child_count, 3, "Q1 has 3 month leaves");
        }
        other => panic!("expected Consolidation provenance, got {other:?}"),
    }
}
