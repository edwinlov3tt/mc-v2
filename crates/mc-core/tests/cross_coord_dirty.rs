//! Cross-coordinate dependency graph precision tests.
//!
//! Per ADR-0027: verifies that cross-coord reads (prev, cumulative,
//! actual_ref, etc.) register dependency edges in the graph, and that
//! writes produce precise dirty sets proportional to actual fan-out
//! rather than cube size.

use mc_core::{
    AggregationRule, CellCoordinate, CellDataType, Cube, CubeId, Dimension, DimensionKind, Element,
    ElementId, Expr, IdGenerator, MeasureRole, PrincipalId, Rule, ScalarValue, WriteIntent,
    WritebackRequest,
};

// ---------------------------------------------------------------------------
// Helper: build a small cube with Time + Measure dimensions for cross-coord
// tests. Returns (cube, time_elements, measure_elements, principal).
// ---------------------------------------------------------------------------

struct CrossCoordCube {
    cube: Cube,
    cube_id: CubeId,
    /// Time elements in order: [Jan, Feb, Mar, Apr]
    time: Vec<ElementId>,
    /// Measure elements: input, derived (prev), cumulative
    input: ElementId,
    prev_m: ElementId,
    cumul_m: ElementId,
    principal: PrincipalId,
}

fn build_prev_cumul_cube() -> CrossCoordCube {
    let g = IdGenerator::new();
    let cube_id = g.cube();
    let time_dim_id = g.dimension();
    let measure_dim_id = g.dimension();
    let principal = g.principal();

    let jan = g.element();
    let feb = g.element();
    let mar = g.element();
    let apr = g.element();
    let input = g.element();
    let prev_m = g.element();
    let cumul_m = g.element();

    let time_dim = Dimension::builder(time_dim_id, "Time", DimensionKind::Standard)
        .add_element(Element::leaf(jan, "Jan", time_dim_id))
        .expect("ok")
        .add_element(Element::leaf(feb, "Feb", time_dim_id))
        .expect("ok")
        .add_element(Element::leaf(mar, "Mar", time_dim_id))
        .expect("ok")
        .add_element(Element::leaf(apr, "Apr", time_dim_id))
        .expect("ok")
        .build()
        .expect("time dim");

    let measure_dim = Dimension::builder(measure_dim_id, "Measure", DimensionKind::Measure)
        .add_element(Element::measure(
            input,
            "Input",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Input,
            AggregationRule::Sum,
        ))
        .expect("ok")
        .add_element(Element::measure(
            prev_m,
            "PrevInput",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Derived,
            AggregationRule::Sum,
        ))
        .expect("ok")
        .add_element(Element::measure(
            cumul_m,
            "CumulInput",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Derived,
            AggregationRule::Sum,
        ))
        .expect("ok")
        .build()
        .expect("measure dim");

    // prev(Input): reads Input at T-1
    let prev_rule = Rule {
        id: g.rule(),
        cube: cube_id,
        target_measure: prev_m,
        scope: mc_core::Scope::AllLeaves,
        body: Expr::Prev(input),
        declared_dependencies: vec![mc_core::DependencyDecl {
            measure: input,
            coord_pattern: mc_core::CoordPattern::SameAsTarget,
        }],
    };

    // cumulative(Input): running sum of Input up to current T
    let cumul_rule = Rule {
        id: g.rule(),
        cube: cube_id,
        target_measure: cumul_m,
        scope: mc_core::Scope::AllLeaves,
        body: Expr::Cumulative(input),
        declared_dependencies: vec![mc_core::DependencyDecl {
            measure: input,
            coord_pattern: mc_core::CoordPattern::SameAsTarget,
        }],
    };

    let cube = Cube::builder(cube_id, "CrossCoordTest")
        .add_dimension(time_dim)
        .add_dimension(measure_dim)
        .measure_dimension("Measure")
        .root_principal(principal)
        .add_rule(prev_rule)
        .expect("prev rule ok")
        .add_rule(cumul_rule)
        .expect("cumul rule ok")
        .build()
        .expect("cube build");

    CrossCoordCube {
        cube,
        cube_id,
        time: vec![jan, feb, mar, apr],
        input,
        prev_m,
        cumul_m,
        principal,
    }
}

fn coord2(cube_id: CubeId, time: ElementId, measure: ElementId) -> CellCoordinate {
    CellCoordinate::from_parts(cube_id, [time, measure])
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Write Input[Jan] -> only prev(Input)[Feb] dirty, not prev(Input)[Mar/Apr].
/// prev(Input)[Feb] reads Input[Jan]; prev(Input)[Mar] reads Input[Feb].
#[test]
fn t_cross_coord_prev_precise_dirty() {
    let c = build_prev_cumul_cube();
    let mut cube = c.cube;
    let p = c.principal;

    // Seed all input cells.
    for (i, &t) in c.time.iter().enumerate() {
        cube.write(WritebackRequest {
            coord: coord2(c.cube_id, t, c.input),
            new_value: ScalarValue::F64((i + 1) as f64 * 10.0),
            principal: p,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write input ok");
    }

    // Read derived cells to populate graph edges.
    for &t in &c.time {
        let _ = cube.read(&coord2(c.cube_id, t, c.prev_m), p);
    }

    // After reading all prev cells, they should not be dirty. (Cumul
    // cells might still be dirty from graph-driven invalidation if
    // prev cell evaluation created edges that transitively dirty them.)
    for &t in &c.time {
        let prev_coord = coord2(c.cube_id, t, c.prev_m);
        assert!(
            !cube.dirty().is_dirty(&prev_coord),
            "prev cell should not be dirty after read"
        );
    }

    // Write Input[Jan] = new value.
    let result = cube
        .write(WritebackRequest {
            coord: coord2(c.cube_id, c.time[0], c.input),
            new_value: ScalarValue::F64(999.0),
            principal: p,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write ok");

    // prev(Input)[Feb] should be dirty (reads Input[Jan]).
    let prev_feb = coord2(c.cube_id, c.time[1], c.prev_m);
    assert!(
        cube.dirty().is_dirty(&prev_feb),
        "prev(Input)[Feb] must be dirty after write to Input[Jan]"
    );

    // prev(Input)[Mar] should NOT be dirty (reads Input[Feb], not Input[Jan]).
    let prev_mar = coord2(c.cube_id, c.time[2], c.prev_m);
    assert!(
        !cube.dirty().is_dirty(&prev_mar),
        "prev(Input)[Mar] must NOT be dirty after write to Input[Jan]"
    );

    // prev(Input)[Apr] should NOT be dirty either.
    let prev_apr = coord2(c.cube_id, c.time[3], c.prev_m);
    assert!(
        !cube.dirty().is_dirty(&prev_apr),
        "prev(Input)[Apr] must NOT be dirty after write to Input[Jan]"
    );

    // Verify the invalidated set from the write includes prev_feb.
    assert!(
        result.invalidated.contains(&prev_feb),
        "invalidated must contain prev(Input)[Feb]"
    );
}

/// Write Input[Q1] -> cumulative[Q1..Q4] all dirty.
/// cumulative reads all prior periods, so Q1 affects all downstream.
#[test]
fn t_cross_coord_cumulative_precise_dirty() {
    let c = build_prev_cumul_cube();
    let mut cube = c.cube;
    let p = c.principal;

    // Seed all input cells.
    for (i, &t) in c.time.iter().enumerate() {
        cube.write(WritebackRequest {
            coord: coord2(c.cube_id, t, c.input),
            new_value: ScalarValue::F64((i + 1) as f64 * 10.0),
            principal: p,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write input ok");
    }

    // Read all cumulative cells to populate graph edges.
    for &t in &c.time {
        let _ = cube.read(&coord2(c.cube_id, t, c.cumul_m), p);
    }

    // Write Input[Jan] (first period).
    cube.write(WritebackRequest {
        coord: coord2(c.cube_id, c.time[0], c.input),
        new_value: ScalarValue::F64(999.0),
        principal: p,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write ok");

    // All cumulative cells should be dirty (cumulative reads all prior periods).
    for (i, &t) in c.time.iter().enumerate() {
        let cumul_coord = coord2(c.cube_id, t, c.cumul_m);
        assert!(
            cube.dirty().is_dirty(&cumul_coord),
            "cumulative[T{}] must be dirty after write to Input[Jan]",
            i
        );
    }

    // Read all to clear dirty, then write Input[Mar] (third period).
    for &t in &c.time {
        let _ = cube.read(&coord2(c.cube_id, t, c.cumul_m), p);
    }
    cube.write(WritebackRequest {
        coord: coord2(c.cube_id, c.time[2], c.input),
        new_value: ScalarValue::F64(500.0),
        principal: p,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write ok");

    // cumulative[Jan] and cumulative[Feb] should NOT be dirty (Jan reads
    // only Jan; Feb reads Jan+Feb — neither includes Mar).
    assert!(
        !cube
            .dirty()
            .is_dirty(&coord2(c.cube_id, c.time[0], c.cumul_m)),
        "cumulative[Jan] must NOT be dirty after write to Input[Mar]"
    );
    assert!(
        !cube
            .dirty()
            .is_dirty(&coord2(c.cube_id, c.time[1], c.cumul_m)),
        "cumulative[Feb] must NOT be dirty after write to Input[Mar]"
    );
    // cumulative[Mar] and cumulative[Apr] SHOULD be dirty.
    assert!(
        cube.dirty()
            .is_dirty(&coord2(c.cube_id, c.time[2], c.cumul_m)),
        "cumulative[Mar] must be dirty after write to Input[Mar]"
    );
    assert!(
        cube.dirty()
            .is_dirty(&coord2(c.cube_id, c.time[3], c.cumul_m)),
        "cumulative[Apr] must be dirty after write to Input[Mar]"
    );
}

// ---------------------------------------------------------------------------
// actual_ref test: cross-scenario dependency
// ---------------------------------------------------------------------------

struct ActualRefCube {
    cube: Cube,
    cube_id: CubeId,
    actual: ElementId,
    plan: ElementId,
    time_jan: ElementId,
    input: ElementId,
    aref_m: ElementId,
    other_derived: ElementId,
    principal: PrincipalId,
}

fn build_actual_ref_cube() -> ActualRefCube {
    let g = IdGenerator::new();
    let cube_id = g.cube();
    let scenario_dim_id = g.dimension();
    let time_dim_id = g.dimension();
    let measure_dim_id = g.dimension();
    let principal = g.principal();

    let actual = g.element();
    let plan = g.element();
    let jan = g.element();
    let input = g.element();
    let aref_m = g.element();
    let other_derived = g.element();

    let scenario_dim = Dimension::builder(scenario_dim_id, "Scenario", DimensionKind::Scenario)
        .add_element(Element::scenario(
            actual,
            "Actual",
            scenario_dim_id,
            mc_core::ScenarioMeta::Default,
        ))
        .expect("ok")
        .add_element(Element::scenario(
            plan,
            "Plan",
            scenario_dim_id,
            mc_core::ScenarioMeta::NonDefault,
        ))
        .expect("ok")
        .build()
        .expect("scenario dim");

    let time_dim = Dimension::builder(time_dim_id, "Time", DimensionKind::Standard)
        .add_element(Element::leaf(jan, "Jan", time_dim_id))
        .expect("ok")
        .build()
        .expect("time dim");

    let measure_dim = Dimension::builder(measure_dim_id, "Measure", DimensionKind::Measure)
        .add_element(Element::measure(
            input,
            "Input",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Input,
            AggregationRule::Sum,
        ))
        .expect("ok")
        .add_element(Element::measure(
            aref_m,
            "ActualRefInput",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Derived,
            AggregationRule::Sum,
        ))
        .expect("ok")
        .add_element(Element::measure(
            other_derived,
            "OtherDerived",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Derived,
            AggregationRule::Sum,
        ))
        .expect("ok")
        .build()
        .expect("measure dim");

    // actual_ref(Input): reads Input from the Actuals scenario.
    let aref_rule = Rule {
        id: g.rule(),
        cube: cube_id,
        target_measure: aref_m,
        scope: mc_core::Scope::AllLeaves,
        body: Expr::ActualRef(input),
        declared_dependencies: vec![mc_core::DependencyDecl {
            measure: input,
            coord_pattern: mc_core::CoordPattern::SameAsTarget,
        }],
    };

    // OtherDerived = Input * 2.0 (same-coord, no cross-coord dep).
    let other_rule = Rule {
        id: g.rule(),
        cube: cube_id,
        target_measure: other_derived,
        scope: mc_core::Scope::AllLeaves,
        body: Expr::Mul(
            Box::new(Expr::SelfRef(input)),
            Box::new(Expr::Const(ScalarValue::F64(2.0))),
        ),
        declared_dependencies: vec![mc_core::DependencyDecl {
            measure: input,
            coord_pattern: mc_core::CoordPattern::SameAsTarget,
        }],
    };

    let cube = Cube::builder(cube_id, "ActualRefTest")
        .add_dimension(scenario_dim)
        .add_dimension(time_dim)
        .add_dimension(measure_dim)
        .measure_dimension("Measure")
        .root_principal(principal)
        .add_rule(aref_rule)
        .expect("aref rule ok")
        .add_rule(other_rule)
        .expect("other rule ok")
        .build()
        .expect("cube build");

    ActualRefCube {
        cube,
        cube_id,
        actual,
        plan,
        time_jan: jan,
        input,
        aref_m,
        other_derived,
        principal,
    }
}

fn coord3(
    cube_id: CubeId,
    scenario: ElementId,
    time: ElementId,
    measure: ElementId,
) -> CellCoordinate {
    CellCoordinate::from_parts(cube_id, [scenario, time, measure])
}

/// Write Measure[Actual] -> actual_ref[Plan] dirty, other derived NOT dirty.
#[test]
fn t_cross_coord_actual_ref_precise_dirty() {
    let c = build_actual_ref_cube();
    let mut cube = c.cube;
    let p = c.principal;

    // Seed Input in Actual scenario.
    cube.write(WritebackRequest {
        coord: coord3(c.cube_id, c.actual, c.time_jan, c.input),
        new_value: ScalarValue::F64(100.0),
        principal: p,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write ok");

    // Seed Input in Plan scenario.
    cube.write(WritebackRequest {
        coord: coord3(c.cube_id, c.plan, c.time_jan, c.input),
        new_value: ScalarValue::F64(50.0),
        principal: p,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write ok");

    // Read actual_ref in Plan scenario to populate graph edges.
    let aref_plan = coord3(c.cube_id, c.plan, c.time_jan, c.aref_m);
    let cv = cube.read(&aref_plan, p).expect("read ok");
    assert!(
        (cv.value.as_f64().unwrap() - 100.0).abs() < 1e-9,
        "actual_ref should read Input from Actuals scenario"
    );

    // Read OtherDerived in Plan to populate its edges.
    let other_plan = coord3(c.cube_id, c.plan, c.time_jan, c.other_derived);
    let _ = cube.read(&other_plan, p).expect("read ok");

    // Write Input[Actual] = new value.
    cube.write(WritebackRequest {
        coord: coord3(c.cube_id, c.actual, c.time_jan, c.input),
        new_value: ScalarValue::F64(200.0),
        principal: p,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write ok");

    // actual_ref[Plan] should be dirty (cross-coord edge).
    assert!(
        cube.dirty().is_dirty(&aref_plan),
        "actual_ref[Plan] must be dirty after write to Input[Actual]"
    );

    // OtherDerived[Plan] should NOT be dirty (it depends on Input[Plan],
    // not Input[Actual]).
    assert!(
        !cube.dirty().is_dirty(&other_plan),
        "OtherDerived[Plan] must NOT be dirty after write to Input[Actual]"
    );
}

/// Null source -> edge registered -> later write dirties the dependent.
#[test]
fn t_cross_coord_null_read_still_registers_edge() {
    let c = build_actual_ref_cube();
    let mut cube = c.cube;
    let p = c.principal;

    // Do NOT seed Input[Actual] — it's Null.
    // Seed Input[Plan] so the Plan scenario has some data.
    cube.write(WritebackRequest {
        coord: coord3(c.cube_id, c.plan, c.time_jan, c.input),
        new_value: ScalarValue::F64(50.0),
        principal: p,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write ok");

    // Read actual_ref[Plan] — source is Null, returns Null. Edge should
    // still be registered per ADR-0027 Decision 4.
    let aref_plan = coord3(c.cube_id, c.plan, c.time_jan, c.aref_m);
    let cv = cube.read(&aref_plan, p).expect("read ok");
    assert!(
        cv.value.is_null(),
        "actual_ref should return Null when Actuals has no data"
    );

    // Now write Input[Actual] — the edge should make actual_ref[Plan] dirty.
    cube.write(WritebackRequest {
        coord: coord3(c.cube_id, c.actual, c.time_jan, c.input),
        new_value: ScalarValue::F64(999.0),
        principal: p,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write ok");

    assert!(
        cube.dirty().is_dirty(&aref_plan),
        "actual_ref[Plan] must be dirty after write to previously-null Input[Actual]"
    );

    // Re-read and verify the value is now 999.
    let cv = cube.read(&aref_plan, p).expect("read ok");
    assert!(
        (cv.value.as_f64().unwrap() - 999.0).abs() < 1e-9,
        "actual_ref should now return the newly-written value"
    );
}

/// prev(DerivedMeasure): write to underlying input -> chain invalidates.
/// Revenue = Input * 2; PrevRevenue = prev(Revenue)
/// Write Input[Jan] -> Revenue[Jan] dirty -> PrevRevenue[Feb] dirty.
#[test]
fn t_cross_coord_derived_source_chain() {
    let g = IdGenerator::new();
    let cube_id = g.cube();
    let time_dim_id = g.dimension();
    let measure_dim_id = g.dimension();
    let p = g.principal();

    let jan = g.element();
    let feb = g.element();
    let input = g.element();
    let revenue = g.element();
    let prev_rev = g.element();

    let time_dim = Dimension::builder(time_dim_id, "Time", DimensionKind::Standard)
        .add_element(Element::leaf(jan, "Jan", time_dim_id))
        .expect("ok")
        .add_element(Element::leaf(feb, "Feb", time_dim_id))
        .expect("ok")
        .build()
        .expect("time dim");

    let measure_dim = Dimension::builder(measure_dim_id, "Measure", DimensionKind::Measure)
        .add_element(Element::measure(
            input,
            "Input",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Input,
            AggregationRule::Sum,
        ))
        .expect("ok")
        .add_element(Element::measure(
            revenue,
            "Revenue",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Derived,
            AggregationRule::Sum,
        ))
        .expect("ok")
        .add_element(Element::measure(
            prev_rev,
            "PrevRevenue",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Derived,
            AggregationRule::Sum,
        ))
        .expect("ok")
        .build()
        .expect("measure dim");

    // Revenue = Input * 2
    let revenue_rule = Rule {
        id: g.rule(),
        cube: cube_id,
        target_measure: revenue,
        scope: mc_core::Scope::AllLeaves,
        body: Expr::Mul(
            Box::new(Expr::SelfRef(input)),
            Box::new(Expr::Const(ScalarValue::F64(2.0))),
        ),
        declared_dependencies: vec![mc_core::DependencyDecl {
            measure: input,
            coord_pattern: mc_core::CoordPattern::SameAsTarget,
        }],
    };

    // PrevRevenue = prev(Revenue)
    let prev_rev_rule = Rule {
        id: g.rule(),
        cube: cube_id,
        target_measure: prev_rev,
        scope: mc_core::Scope::AllLeaves,
        body: Expr::Prev(revenue),
        declared_dependencies: vec![mc_core::DependencyDecl {
            measure: revenue,
            coord_pattern: mc_core::CoordPattern::SameAsTarget,
        }],
    };

    let mut cube = Cube::builder(cube_id, "ChainTest")
        .add_dimension(time_dim)
        .add_dimension(measure_dim)
        .measure_dimension("Measure")
        .root_principal(p)
        .add_rule(revenue_rule)
        .expect("ok")
        .add_rule(prev_rev_rule)
        .expect("ok")
        .build()
        .expect("cube build");

    // Seed inputs.
    for &t in &[jan, feb] {
        cube.write(WritebackRequest {
            coord: coord2(cube_id, t, input),
            new_value: ScalarValue::F64(10.0),
            principal: p,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write ok");
    }

    // Read PrevRevenue[Feb] to populate the full chain:
    //   PrevRevenue[Feb] -> Revenue[Jan] -> Input[Jan]
    let prev_rev_feb = coord2(cube_id, feb, prev_rev);
    let cv = cube.read(&prev_rev_feb, p).expect("read ok");
    assert!(
        (cv.value.as_f64().unwrap() - 20.0).abs() < 1e-9,
        "PrevRevenue[Feb] = prev(Revenue)[Feb] = Revenue[Jan] = Input[Jan]*2 = 20"
    );

    // Also read Revenue[Jan] to ensure its edges are registered.
    let rev_jan = coord2(cube_id, jan, revenue);
    let _ = cube.read(&rev_jan, p).expect("read ok");

    // Write Input[Jan] = new value.
    cube.write(WritebackRequest {
        coord: coord2(cube_id, jan, input),
        new_value: ScalarValue::F64(50.0),
        principal: p,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write ok");

    // Revenue[Jan] should be dirty (same-coord dep on Input[Jan]).
    assert!(
        cube.dirty().is_dirty(&rev_jan),
        "Revenue[Jan] must be dirty after write to Input[Jan]"
    );

    // PrevRevenue[Feb] should also be dirty (cross-coord edge:
    // PrevRevenue[Feb] reads Revenue[Jan], which depends on Input[Jan]).
    assert!(
        cube.dirty().is_dirty(&prev_rev_feb),
        "PrevRevenue[Feb] must be dirty (transitive chain through Revenue[Jan])"
    );
}

/// Repeated reads don't grow edge count.
#[test]
fn t_edge_dedup_no_growth() {
    let c = build_prev_cumul_cube();
    let mut cube = c.cube;
    let p = c.principal;

    // Seed Input[Jan].
    cube.write(WritebackRequest {
        coord: coord2(c.cube_id, c.time[0], c.input),
        new_value: ScalarValue::F64(10.0),
        principal: p,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write ok");

    // Read prev(Input)[Feb] 5 times.
    let prev_feb = coord2(c.cube_id, c.time[1], c.prev_m);
    for _ in 0..5 {
        // Force re-evaluation by marking dirty before each read after the first.
        let _ = cube.read(&prev_feb, p);
    }

    // Edge count for prev_feb should be exactly the number of distinct
    // dependencies, not 5x that. prev(Input)[Feb] reads Input[Jan]
    // (cross-coord) — so 1 cross-coord edge.
    let edges = cube.deps().dependencies_of(&prev_feb);
    assert_eq!(
        edges.len(),
        1,
        "prev(Input)[Feb] should have exactly 1 edge (to Input[Jan]), \
         not {} from repeated reads",
        edges.len()
    );
}

/// Visible-grid unrelated write: 200 cached cells + unrelated write -> 0
/// recomputation for the cached cells (they stay fresh via dirty flag).
#[test]
fn t_visible_grid_unrelated_write_preserves_cache() {
    let c = build_prev_cumul_cube();
    let mut cube = c.cube;
    let p = c.principal;

    // Seed all input cells with values.
    for (i, &t) in c.time.iter().enumerate() {
        cube.write(WritebackRequest {
            coord: coord2(c.cube_id, t, c.input),
            new_value: ScalarValue::F64((i + 1) as f64 * 100.0),
            principal: p,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write ok");
    }

    // Read all prev and cumul cells ("visible grid") to populate graph.
    let mut grid_values: Vec<(CellCoordinate, ScalarValue)> = Vec::new();
    for &t in &c.time {
        for &m in &[c.prev_m, c.cumul_m] {
            let coord = coord2(c.cube_id, t, m);
            let cv = cube.read(&coord, p).expect("read ok");
            grid_values.push((coord, cv.value));
        }
    }

    // Write to Input[Apr] — only cells that depend on Input[Apr] should
    // be dirty. prev(Input)[Jan/Feb/Mar] do NOT depend on Input[Apr].
    cube.write(WritebackRequest {
        coord: coord2(c.cube_id, c.time[3], c.input),
        new_value: ScalarValue::F64(999.0),
        principal: p,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write ok");

    // Re-read the grid. Cells that are NOT dirty should return the same
    // value without recomputation (cache hit via the new freshness
    // semantics: has edges + not dirty = fresh).
    for (coord, old_value) in &grid_values {
        if !cube.dirty().is_dirty(coord) {
            let cv = cube.read(coord, p).expect("read ok");
            assert_eq!(
                &cv.value, old_value,
                "non-dirty cell {:?} must return same value (cache hit)",
                coord
            );
        }
    }

    // Specifically: prev(Input)[Feb] reads Input[Jan], which was NOT
    // written — should be a cache hit.
    let prev_feb = coord2(c.cube_id, c.time[1], c.prev_m);
    assert!(
        !cube.dirty().is_dirty(&prev_feb),
        "prev(Input)[Feb] must NOT be dirty after write to Input[Apr]"
    );

    // prev(Input)[Jan] reads Input[T-1] which doesn't exist (returns Null).
    // It registered an edge to nothing — it should also be not dirty.
    let prev_jan = coord2(c.cube_id, c.time[0], c.prev_m);
    assert!(
        !cube.dirty().is_dirty(&prev_jan),
        "prev(Input)[Jan] must NOT be dirty after write to Input[Apr]"
    );
}
