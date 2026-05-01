//! Brief §10.4 — trace-tree shape and content.
//!
//! Per CLAUDE.md §3.3 / brief §10 test names are contractual.

use mc_core::{
    PrincipalId, Provenance, ScalarValue, TraceNode, TraceOp, WriteIntent, WritebackRequest,
};
use mc_fixtures::{build_acme_cube, coord, write_canonical_inputs};

/// Longest root-to-leaf node count in a trace tree (1 for a single-node
/// trace).
fn trace_depth(node: &TraceNode) -> usize {
    if node.children.is_empty() {
        1
    } else {
        1 + node.children.iter().map(trace_depth).max().unwrap_or(0)
    }
}

#[test]
fn t_trace_for_input_cell_is_single_node() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let c = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    let v = cube
        .read_with_trace(&c, refs.root_principal)
        .expect("trace ok");
    let trace = v.trace.expect("trace requested");
    assert_eq!(trace.root.children.len(), 0, "input has no children");
    assert!(
        matches!(trace.root.operation, TraceOp::InputLookup { .. }),
        "got {:?}",
        trace.root.operation
    );
}

#[test]
fn t_trace_for_clicks_has_two_input_children() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let c = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.clicks,
    );
    let v = cube
        .read_with_trace(&c, refs.root_principal)
        .expect("trace ok");
    let trace = v.trace.expect("trace requested");
    match trace.root.operation {
        TraceOp::RuleEvaluation { rule_id, .. } => {
            assert_eq!(rule_id, refs.rule_clicks, "expected rule_clicks");
        }
        other => panic!("expected RuleEvaluation, got {other:?}"),
    }
    assert_eq!(trace.root.children.len(), 2, "Clicks = Spend / CPC");
    for child in &trace.root.children {
        assert!(
            matches!(child.operation, TraceOp::InputLookup { .. }),
            "child must be InputLookup, got {:?}",
            child.operation
        );
    }
}

#[test]
fn t_trace_depth_for_revenue() {
    // Revenue → Customers → Leads → Clicks → Spend|CPC. Five-node
    // longest path: Revenue, Customers, Leads, Clicks, Spend (or CPC).
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let c = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.revenue,
    );
    let v = cube
        .read_with_trace(&c, refs.root_principal)
        .expect("trace ok");
    let trace = v.trace.expect("trace");
    let d = trace_depth(&trace.root);
    assert_eq!(
        d, 5,
        "Revenue trace depth (longest root-to-leaf node count)"
    );
}

#[test]
fn t_trace_depth_for_gross_profit() {
    // Gross_Profit → Revenue → Customers → Leads → Clicks → Spend|CPC.
    // Six-node longest path.
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let c = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.gross_profit,
    );
    let v = cube
        .read_with_trace(&c, refs.root_principal)
        .expect("trace ok");
    let trace = v.trace.expect("trace");
    let d = trace_depth(&trace.root);
    assert_eq!(d, 6, "Gross_Profit trace depth");
}

#[test]
fn t_trace_for_consolidated_cell_has_correct_child_count() {
    // Q1 × Paid_Search × Tampa × Spend → 3 month leaves.
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let c = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.q1_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    let v = cube
        .read_with_trace(&c, refs.root_principal)
        .expect("trace ok");
    let trace = v.trace.expect("trace");
    assert!(
        matches!(trace.root.operation, TraceOp::Consolidation { .. }),
        "got {:?}",
        trace.root.operation
    );
    assert_eq!(trace.root.children.len(), 3, "Q1 has 3 month leaves");
}

#[test]
fn t_trace_for_triple_consolidated_revenue() {
    // Q1 × Paid_Media × Florida × Revenue. Time has 3 leaves; Channel's
    // Paid_Media has 3 leaves; Market's Florida has 3 leaves. Cartesian
    // product = 27. Each child is a Revenue subtree.
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let c = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.q1_2026,
        refs.paid_media,
        refs.florida,
        refs.revenue,
    );
    let v = cube
        .read_with_trace(&c, refs.root_principal)
        .expect("trace ok");
    let trace = v.trace.expect("trace");
    assert!(
        matches!(trace.root.operation, TraceOp::Consolidation { .. }),
        "expected Consolidation root"
    );
    assert_eq!(trace.root.children.len(), 27, "3×3×3 leaf coords");
    for child in &trace.root.children {
        assert!(
            matches!(child.operation, TraceOp::RuleEvaluation { .. }),
            "each leaf-coord Revenue must be a rule evaluation"
        );
    }
}

#[test]
fn t_trace_root_value_equals_cell_value_property() {
    // Brief §10.4 marks the proptest variant DEFERRED per §0.A. The
    // deterministic equivalent runs in `tests/acme_demo.rs`. We keep
    // this test name in place as a stub so the file lists the
    // contracted name; the assertion is the single hand-picked subset
    // that does NOT require proptest.
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let c = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.q1_2026,
        refs.paid_media,
        refs.florida,
        refs.revenue,
    );
    let v = cube
        .read_with_trace(&c, refs.root_principal)
        .expect("trace ok");
    let trace = v.trace.expect("trace");
    let v_via_root = trace.root.value.as_f64().expect("F64");
    let v_via_value = v.value.as_f64().expect("F64");
    assert!(
        (v_via_root - v_via_value).abs() < 1e-9,
        "root.value must equal CellValue.value"
    );
    // TODO(proptest): when proptest returns to mc-core dev-deps (see
    // brief §0.A), expand to 100 random coords.
}

#[test]
fn t_trace_records_input_provenance_correctly() {
    // Read an Input cell with trace; assert the InputLookup node carries
    // the same `written_at`/`written_by` we passed at write time.
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let c = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    let written_at = 1_700_000_000_u64;
    let written_by = PrincipalId(1); // root principal is PrincipalId(1) per IdGenerator
    cube.write(WritebackRequest {
        coord: c.clone(),
        new_value: ScalarValue::F64(11_500.0),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: written_at,
    })
    .expect("write");
    assert_eq!(
        refs.root_principal, written_by,
        "fixture sanity: root is PrincipalId(1)"
    );
    let v = cube
        .read_with_trace(&c, refs.root_principal)
        .expect("trace");
    let trace = v.trace.expect("trace");
    match trace.root.operation {
        TraceOp::InputLookup {
            written_at: wa,
            written_by: wb,
        } => {
            assert_eq!(wa, written_at);
            assert_eq!(wb, written_by);
        }
        other => panic!("expected InputLookup, got {other:?}"),
    }
}

#[test]
fn t_trace_with_null_input_emits_null_poison() {
    // Don't write Spend (or CPC). Read Clicks → rule evaluates Spend/CPC
    // with a Null operand → result is Null per spec §7. Assert the
    // root's value is Null and at least one child is an InputLookup
    // with a Null value (the null poison source). The brief allows
    // either an explicit NullPoison op OR Null propagating through the
    // arithmetic; Phase 1 takes the latter path.
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let c = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.clicks,
    );
    let v = cube
        .read_with_trace(&c, refs.root_principal)
        .expect("trace");
    assert!(
        matches!(v.value, ScalarValue::Null),
        "Clicks with no Spend/CPC must be Null, got {:?}",
        v.value
    );
    let trace = v.trace.expect("trace");
    let null_input_seen = trace.root.children.iter().any(|child| {
        matches!(
            child.operation,
            TraceOp::InputLookup { .. } | TraceOp::DefaultFallback { .. }
        ) && matches!(child.value, ScalarValue::Null)
    });
    assert!(
        null_input_seen,
        "trace must show at least one Null-valued input as the poison source"
    );
    assert!(
        matches!(v.provenance, Provenance::Rule { .. }),
        "Clicks read returns Rule provenance even when Null"
    );
}
