//! Brief §10.5 — dependency graph + invalidation.
//!
//! Per CLAUDE.md §3.3 / brief §10 test names are contractual.

use mc_core::{
    AggregationRule, CellCoordinate, CellDataType, CoordPattern, Cube, CubeId, DependencyDecl,
    Dimension, DimensionKind, Element, EngineError, Expr, IdGenerator, MeasureRole, Rule, RuleId,
    ScalarValue, Scope, WriteIntent, WritebackRequest,
};
use mc_fixtures::{build_acme_cube, coord, materialize_all_dependencies, write_canonical_inputs};

// ===========================================================================
// Lazy population
// ===========================================================================

#[test]
fn t_dependency_graph_is_empty_immediately_after_cube_build() {
    let (cube, _refs) = build_acme_cube().expect("build ok");
    // Phase 1: lazy dep graph. Pre-read, no rule edges exist.
    // Hierarchy edges are NOT folded into DependencyGraph in this
    // implementation — hierarchy walks happen in `consolidation.rs`
    // directly off the per-dim hierarchies.
    assert!(
        cube.deps().is_empty(),
        "dep graph must be empty before any read"
    );
    assert_eq!(cube.deps().forward_edge_count(), 0);
}

#[test]
fn t_dependency_graph_populates_on_first_read() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let before = cube.deps().forward_edge_count();
    let revenue_at_one_leaf = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.revenue,
    );
    // First read at this coord materializes the rule chain
    // Revenue → Customers → Leads → Clicks → Spend|CPC|...
    cube.read(&revenue_at_one_leaf, refs.root_principal)
        .expect("read");
    let after_one = cube.deps().forward_edge_count();
    assert!(
        after_one > before,
        "first read must materialize edges (before={before}, after={after_one})"
    );

    // Read at a DIFFERENT leaf coord materializes a separate set of
    // per-coord edges.
    let revenue_at_other_leaf = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.feb_2026,
        refs.paid_search,
        refs.tampa,
        refs.revenue,
    );
    cube.read(&revenue_at_other_leaf, refs.root_principal)
        .expect("read 2");
    let after_two = cube.deps().forward_edge_count();
    assert!(
        after_two > after_one,
        "second leaf-coord read must add more edges (after_one={after_one}, after_two={after_two})"
    );
}

#[test]
fn t_dependency_graph_validates_full_fixture_when_forced() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let count = materialize_all_dependencies(&mut cube, &refs).expect("materialize");
    assert_eq!(count, 2_100, "5 derived × 12 × 5 × 7 = 2,100 reads");
    let edges = cube.deps().forward_edge_count();
    // Brief §10.5: edge count is "between 2,100 and 4,200" — between
    // the lower bound (one edge per derived coord) and the upper
    // bound (2 deps per rule × 2,100 derived coords).
    assert!(
        (2_100..=4_200).contains(&edges),
        "forward edge count {edges} outside expected range [2100, 4200]"
    );
}

// ===========================================================================
// Cycle detection at rule registration
// ===========================================================================

#[test]
fn t_dependency_graph_detects_cycle_at_rule_addition() {
    // Build a cube with rules A=B+1, B=C+1; then attempt C=A+1 to close
    // the A→B→C→A cycle. The third add must error.
    let g = IdGenerator::new();
    let cube_id = g.cube();
    let market_dim_id = g.dimension();
    let measure_dim_id = g.dimension();
    let only_market = g.element();
    let a = g.element();
    let b = g.element();
    let c = g.element();
    let market_dim = Dimension::builder(market_dim_id, "Market", DimensionKind::Standard)
        .add_element(Element::leaf(only_market, "X", market_dim_id))
        .expect("ok")
        .build()
        .expect("market dim");
    let measure_dim = Dimension::builder(measure_dim_id, "Measure", DimensionKind::Measure)
        .add_element(Element::measure(
            a,
            "A",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Derived,
            AggregationRule::Sum,
        ))
        .expect("ok")
        .add_element(Element::measure(
            b,
            "B",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Derived,
            AggregationRule::Sum,
        ))
        .expect("ok")
        .add_element(Element::measure(
            c,
            "C",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Derived,
            AggregationRule::Sum,
        ))
        .expect("ok")
        .build()
        .expect("measure dim");
    let root = g.principal();
    let rule_a = Rule {
        id: g.rule(),
        cube: cube_id,
        target_measure: a,
        scope: Scope::AllLeaves,
        body: Expr::Add(
            Box::new(Expr::SelfRef(b)),
            Box::new(Expr::Const(ScalarValue::F64(1.0))),
        ),
        declared_dependencies: vec![DependencyDecl {
            measure: b,
            coord_pattern: CoordPattern::SameAsTarget,
        }],
    };
    let rule_b = Rule {
        id: g.rule(),
        cube: cube_id,
        target_measure: b,
        scope: Scope::AllLeaves,
        body: Expr::Add(
            Box::new(Expr::SelfRef(c)),
            Box::new(Expr::Const(ScalarValue::F64(1.0))),
        ),
        declared_dependencies: vec![DependencyDecl {
            measure: c,
            coord_pattern: CoordPattern::SameAsTarget,
        }],
    };
    let rule_c_cycle = Rule {
        id: g.rule(),
        cube: cube_id,
        target_measure: c,
        scope: Scope::AllLeaves,
        body: Expr::Add(
            Box::new(Expr::SelfRef(a)),
            Box::new(Expr::Const(ScalarValue::F64(1.0))),
        ),
        declared_dependencies: vec![DependencyDecl {
            measure: a,
            coord_pattern: CoordPattern::SameAsTarget,
        }],
    };

    // Cycle detection in this implementation runs during the
    // `RuleSet::add` calls inside `CubeBuilder::build()`. The staged
    // `add_rule(...)` only validates well-typedness; cycles must wait
    // until all rules have been collected.
    let res = Cube::builder(cube_id, "Cycle")
        .add_dimension(market_dim)
        .add_dimension(measure_dim)
        .measure_dimension("Measure")
        .root_principal(root)
        .add_rule(rule_a)
        .expect("rule A ok")
        .add_rule(rule_b)
        .expect("rule B ok")
        .add_rule(rule_c_cycle)
        .expect("rule C stages ok (cycle check is at build())")
        .build();
    let err = res.expect_err("cycle must be rejected at build");
    assert!(
        matches!(err, EngineError::DependencyCycle { .. }),
        "got {err:?}"
    );
}

// ===========================================================================
// Undeclared dependency rejection
// ===========================================================================

#[test]
fn t_dependency_graph_rejects_undeclared_dependency_in_test_mode() {
    // A rule body that SelfRef's measure X but does not declare X as a
    // dependency. Phase 1 catches this STRUCTURALLY at registration time
    // (RuleSet::add) rather than waiting for first eval — that's the
    // stronger guarantee the brief asks for ("every read tracks actual
    // dependencies and asserts they match declarations"; we move that
    // assertion to compile-of-the-rule time so no malformed rule ever
    // reaches eval). The error variant is `RuleBodyTypeMismatch` with a
    // `does not declare it` detail string per rule.rs.
    let g = IdGenerator::new();
    let cube_id = g.cube();
    let market_dim_id = g.dimension();
    let measure_dim_id = g.dimension();
    let only_market = g.element();
    let spend = g.element();
    let cpc = g.element();
    let clicks = g.element();
    let market_dim = Dimension::builder(market_dim_id, "Market", DimensionKind::Standard)
        .add_element(Element::leaf(only_market, "X", market_dim_id))
        .expect("ok")
        .build()
        .expect("market");
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
            clicks,
            "Clicks",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Derived,
            AggregationRule::Sum,
        ))
        .expect("ok")
        .build()
        .expect("measure");
    let root = g.principal();
    // Body refs both Spend AND CPC; declared_dependencies omits CPC.
    let bad_rule = Rule {
        id: g.rule(),
        cube: cube_id,
        target_measure: clicks,
        scope: Scope::AllLeaves,
        body: Expr::Div(Box::new(Expr::SelfRef(spend)), Box::new(Expr::SelfRef(cpc))),
        declared_dependencies: vec![DependencyDecl {
            measure: spend, // CPC missing!
            coord_pattern: CoordPattern::SameAsTarget,
        }],
    };

    // Undeclared-dep check fires at `RuleSet::add` time, which the
    // builder runs during `.build()`. The staging `add_rule(...)` step
    // only validates well-typedness.
    let res = Cube::builder(cube_id, "Bad")
        .add_dimension(market_dim)
        .add_dimension(measure_dim)
        .measure_dimension("Measure")
        .root_principal(root)
        .add_rule(bad_rule)
        .expect("rule stages (well-typed)")
        .build();
    let err = res.expect_err("undeclared dep must be rejected at build");
    match err {
        EngineError::RuleBodyTypeMismatch { detail } => {
            assert!(
                detail.contains("does not declare"),
                "expected detail to mention the missing declaration: {detail}"
            );
        }
        other => panic!("expected RuleBodyTypeMismatch, got {other:?}"),
    }
}

// ===========================================================================
// Dirty-set invalidation (mirrors the acme_demo §10.1 dirty tests but
// scoped to dependency-graph propagation specifically)
// ===========================================================================

#[test]
fn t_dependency_invalidation_walks_full_closure() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
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
        new_value: ScalarValue::F64(50_000.0),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write");
    // Expect every same-coord derived measure to be in the dirty set
    // along with at least one hierarchical ancestor.
    let same_coord_derived = [
        refs.clicks,
        refs.leads,
        refs.customers,
        refs.revenue,
        refs.gross_profit,
    ];
    for &m in &same_coord_derived {
        let c = coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            m,
        );
        assert!(
            cube.dirty().is_dirty(&c),
            "same-coord derived {m:?} must be dirty"
        );
    }
    let q1_ancestor = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.q1_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    assert!(
        cube.dirty().is_dirty(&q1_ancestor),
        "Q1 ancestor must be dirty"
    );
}

#[test]
fn t_dependency_does_not_invalidate_unrelated_cells() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    // Capture the dirty set BEFORE the targeted write so we can isolate
    // its marginal effect (the canonical-input loop accumulates marks
    // across 2,520 writes; per `tests/acme_demo.rs` the brief assertions
    // are about the per-write delta).
    let before: std::collections::HashSet<CellCoordinate> = cube.dirty().iter().cloned().collect();
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
        new_value: ScalarValue::F64(50_000.0),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write");
    // Atlanta (different Market subtree) must not be NEWLY dirtied.
    let atlanta_spend = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.atlanta,
        refs.spend,
    );
    let was_dirty = before.contains(&atlanta_spend);
    let is_dirty = cube.dirty().is_dirty(&atlanta_spend);
    assert!(
        was_dirty || !is_dirty,
        "Tampa-Spend write must not newly dirty Atlanta-Spend"
    );
    // Belt: also check we never see Atlanta-Revenue freshly dirtied.
    let atlanta_revenue = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.atlanta,
        refs.revenue,
    );
    let was_dirty_rev = before.contains(&atlanta_revenue);
    let is_dirty_rev = cube.dirty().is_dirty(&atlanta_revenue);
    assert!(
        was_dirty_rev || !is_dirty_rev,
        "Tampa-Spend write must not newly dirty Atlanta-Revenue"
    );
    // Suppress unused warnings on the destructured ids.
    let _ = (RuleId(0), CubeId(0));
}
