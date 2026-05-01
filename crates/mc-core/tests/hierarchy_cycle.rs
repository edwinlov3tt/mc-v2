//! Hierarchy cycle / single-parent / weight validation tests.
//!
//! These pin spec §3 I-Hier-1 (forest, no cycles), I-Hier-3 (finite
//! weights), and the build-brief §3.6 contract.

use mc_core::{DimensionId, ElementId, EngineError, Hierarchy, HierarchyId};

fn dim() -> DimensionId {
    DimensionId(1)
}

fn h() -> HierarchyId {
    HierarchyId(1)
}

#[test]
fn cycle_two_node_back_edge_rejected() {
    // A → B → A
    let a = ElementId(10);
    let b = ElementId(20);
    let err = Hierarchy::builder(h(), "Cycle2", dim())
        .add_edge(a, b, 1.0)
        .add_edge(b, a, 1.0)
        .build()
        .expect_err("two-node cycle must be rejected");
    match err {
        EngineError::HierarchyCycle { path } => {
            assert!(path.contains(&a) && path.contains(&b));
        }
        other => panic!("expected HierarchyCycle, got {other:?}"),
    }
}

#[test]
fn cycle_three_node_back_edge_rejected() {
    // A → B, B → C, C → A
    let a = ElementId(10);
    let b = ElementId(20);
    let c = ElementId(30);
    let err = Hierarchy::builder(h(), "Cycle3", dim())
        .add_edge(a, b, 1.0)
        .add_edge(b, c, 1.0)
        .add_edge(c, a, 1.0)
        .build()
        .expect_err("three-node cycle must be rejected");
    assert!(matches!(err, EngineError::HierarchyCycle { .. }));
}

#[test]
fn cycle_self_loop_rejected() {
    // A → A
    let a = ElementId(10);
    let err = Hierarchy::builder(h(), "SelfLoop", dim())
        .add_edge(a, a, 1.0)
        .build()
        .expect_err("self-loop must be rejected");
    assert!(matches!(err, EngineError::HierarchyCycle { .. }));
}

#[test]
fn long_chain_no_cycle_succeeds() {
    // Q1 → Jan, Q1 → Feb, Q1 → Mar; FY → Q1. No cycles.
    let fy = ElementId(1000);
    let q1 = ElementId(100);
    let jan = ElementId(1);
    let feb = ElementId(2);
    let mar = ElementId(3);
    let hier = Hierarchy::builder(h(), "Calendar", dim())
        .add_edge(fy, q1, 1.0)
        .add_edge(q1, jan, 1.0)
        .add_edge(q1, feb, 1.0)
        .add_edge(q1, mar, 1.0)
        .build()
        .expect("non-cyclic forest must build");
    assert_eq!(hier.roots, vec![fy]);
    assert_eq!(hier.leaves.len(), 3);
    // Q1 is consolidated (has children) but not a root.
    assert!(hier.is_consolidated(q1));
    assert!(hier.is_consolidated(fy));
    assert!(!hier.is_consolidated(jan));
}

#[test]
fn nan_weight_rejected() {
    let p = ElementId(100);
    let c = ElementId(1);
    let err = Hierarchy::builder(h(), "NaN", dim())
        .add_edge(p, c, f64::NAN)
        .build()
        .expect_err("NaN weight must be rejected");
    assert!(matches!(err, EngineError::InvalidWeight(w) if w.is_nan()));
}

#[test]
fn positive_inf_weight_rejected() {
    let p = ElementId(100);
    let c = ElementId(1);
    let err = Hierarchy::builder(h(), "Inf", dim())
        .add_edge(p, c, f64::INFINITY)
        .build()
        .expect_err("+Inf weight must be rejected");
    assert!(matches!(err, EngineError::InvalidWeight(w) if w.is_infinite()));
}

#[test]
fn negative_inf_weight_rejected() {
    let p = ElementId(100);
    let c = ElementId(1);
    let err = Hierarchy::builder(h(), "NegInf", dim())
        .add_edge(p, c, f64::NEG_INFINITY)
        .build()
        .expect_err("-Inf weight must be rejected");
    assert!(matches!(err, EngineError::InvalidWeight(w) if w.is_infinite()));
}

#[test]
fn duplicate_edge_rejected() {
    let p = ElementId(100);
    let c = ElementId(1);
    let err = Hierarchy::builder(h(), "Dup", dim())
        .add_edge(p, c, 1.0)
        .add_edge(p, c, 1.0)
        .build()
        .expect_err("duplicate (parent, child) edge must be rejected");
    assert!(matches!(
        err,
        EngineError::DuplicateHierarchyEdge { parent, child }
        if parent == p && child == c
    ));
}

#[test]
fn multi_parent_rejected() {
    // Tampa → Florida AND Tampa → SoutheastDirect (single-parent forest only).
    let florida = ElementId(100);
    let southeast_direct = ElementId(200);
    let tampa = ElementId(1);
    let err = Hierarchy::builder(h(), "MultiParent", dim())
        .add_edge(florida, tampa, 1.0)
        .add_edge(southeast_direct, tampa, 1.0)
        .build()
        .expect_err("multi-parent must be rejected");
    assert!(matches!(
        err,
        EngineError::MultipleParents { element, .. } if element == tampa
    ));
}

#[test]
fn weighted_descendants_compose_correctly() {
    // FY → H1 (w=2.0), H1 → Jan (w=0.5). Cumulative weight at Jan = 1.0.
    let fy = ElementId(1000);
    let h1 = ElementId(100);
    let jan = ElementId(1);
    let hier = Hierarchy::builder(h(), "Weighted", dim())
        .add_edge(fy, h1, 2.0)
        .add_edge(h1, jan, 0.5)
        .build()
        .expect("weighted hierarchy must build");
    let desc = hier.descendants(fy);
    assert_eq!(desc.len(), 1);
    let (leaf, w) = desc[0];
    assert_eq!(leaf, jan);
    assert!((w - 1.0).abs() < 1e-12);
}
