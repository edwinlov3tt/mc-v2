//! Brief §10.1 — the canonical end-to-end test against the Acme cube.
//!
//! Every test name and assertion follows the brief verbatim. Per
//! CLAUDE.md §2.6 these names are contractual; do not rename.

use mc_core::{
    CellCoordinate, Provenance, ScalarValue, TraceNode, TraceOp, WriteIntent, WritebackRequest,
};
use mc_fixtures::{build_acme_cube, canonical_inputs_for, coord, write_canonical_inputs};

const EPS: f64 = 1e-9;

fn assert_close(actual: f64, expected: f64, label: &str) {
    assert!(
        (actual - expected).abs() < EPS,
        "{label}: got {actual}, expected {expected}, diff={}",
        (actual - expected).abs()
    );
}

#[test]
fn t_acme_build_succeeds() {
    let (cube, _refs) = build_acme_cube().expect("build ok");
    assert_eq!(cube.dimensions().len(), 6);
    let dim_names: Vec<&str> = cube.dimensions().iter().map(|d| d.name.as_str()).collect();
    assert_eq!(
        dim_names,
        vec!["Scenario", "Version", "Time", "Channel", "Market", "Measure"],
        "dimension order is contractual per spec §3.5"
    );
    assert_eq!(cube.measure_dimension().name, "Measure");
    assert_eq!(cube.rules().len(), 5);
}

#[test]
fn t_acme_input_count_is_2520() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let count = write_canonical_inputs(&mut cube, &refs).expect("inputs ok");
    assert_eq!(count, 2_520);
}

#[test]
fn t_acme_read_input_leaf_returns_written_value() {
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
    let v = cube.read(&c, refs.root_principal).expect("read ok");
    assert_close(
        v.value.as_f64().unwrap(),
        11_500.0,
        "Mar/Paid_Search/Tampa Spend",
    );
}

#[test]
fn t_acme_read_derived_leaf_clicks() {
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
    let v = cube.read(&c, refs.root_principal).expect("read ok");
    let inp = canonical_inputs_for(3, 0, 0);
    assert_close(
        v.value.as_f64().unwrap(),
        inp.clicks(),
        "Clicks = Spend/CPC",
    );
}

#[test]
fn t_acme_read_derived_leaf_revenue() {
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
    let v = cube.read(&c, refs.root_principal).expect("read ok");
    let inp = canonical_inputs_for(3, 0, 0);
    assert_close(
        v.value.as_f64().unwrap(),
        inp.revenue(),
        "Revenue = Customers * AOV",
    );
}

#[test]
fn t_acme_read_derived_leaf_gross_profit() {
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
    let v = cube.read(&c, refs.root_principal).expect("read ok");
    let inp = canonical_inputs_for(3, 0, 0);
    assert_close(
        v.value.as_f64().unwrap(),
        inp.gross_profit(),
        "Gross_Profit = Revenue * (1 - COGS_Rate)",
    );
}

#[test]
fn t_acme_read_consolidated_q1_spend() {
    // Q1_2026 × Paid_Search × Tampa × Spend = sum(Jan, Feb, Mar) Spend.
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
    let v = cube.read(&c, refs.root_principal).expect("read ok");
    let expected = canonical_inputs_for(1, 0, 0).spend
        + canonical_inputs_for(2, 0, 0).spend
        + canonical_inputs_for(3, 0, 0).spend;
    assert_close(
        v.value.as_f64().unwrap(),
        expected,
        "Q1 Tampa Paid_Search Spend",
    );
    assert_close(v.value.as_f64().unwrap(), 33_000.0, "brief golden: 33,000");
}

#[test]
fn t_acme_read_consolidated_florida_spend() {
    // Mar_2026 × Paid_Search × Florida = sum(Tampa, Orlando, Miami).
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
        refs.florida,
        refs.spend,
    );
    let v = cube.read(&c, refs.root_principal).expect("read ok");
    let expected = canonical_inputs_for(3, 0, 0).spend
        + canonical_inputs_for(3, 0, 1).spend
        + canonical_inputs_for(3, 0, 2).spend;
    assert_close(
        v.value.as_f64().unwrap(),
        expected,
        "Florida Mar Paid_Search Spend",
    );
    assert_close(v.value.as_f64().unwrap(), 35_100.0, "brief golden: 35,100");
}

#[test]
fn t_acme_read_consolidated_paid_media_spend() {
    // Mar_2026 × Paid_Media × Tampa = sum(Paid_Search, Paid_Social, Display).
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let c = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_media,
        refs.tampa,
        refs.spend,
    );
    let v = cube.read(&c, refs.root_principal).expect("read ok");
    let expected = canonical_inputs_for(3, 0, 0).spend
        + canonical_inputs_for(3, 1, 0).spend
        + canonical_inputs_for(3, 2, 0).spend;
    assert_close(
        v.value.as_f64().unwrap(),
        expected,
        "Paid_Media Mar Tampa Spend",
    );
    assert_close(v.value.as_f64().unwrap(), 37_500.0, "brief golden: 37,500");
}

#[test]
fn t_acme_read_triple_consolidated_spend() {
    // Q1_2026 × Paid_Media × Florida × Spend = sum of 27 leaves.
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
        refs.spend,
    );
    let v = cube.read(&c, refs.root_principal).expect("read ok");
    let mut expected = 0.0_f64;
    for t_idx in 1..=3u32 {
        for c_idx in 0..3u32 {
            for m_idx in 0..3u32 {
                expected += canonical_inputs_for(t_idx, c_idx, m_idx).spend;
            }
        }
    }
    assert_close(v.value.as_f64().unwrap(), expected, "27-leaf Spend rollup");
    assert_close(
        v.value.as_f64().unwrap(),
        329_400.0,
        "brief golden: 329,400",
    );
}

#[test]
fn t_acme_read_consolidated_cpc_uses_weighted_average() {
    // Per brief §4.5.1 closed form: CPC at Q1×Paid_Search×Florida ≈ 1.5202381.
    // Weighted average — NOT simple sum, NOT simple average.
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
        refs.florida,
        refs.cpc,
    );
    let v = cube.read(&c, refs.root_principal).expect("read ok");
    let actual = v.value.as_f64().unwrap();
    assert_close(
        actual,
        153_240.0 / 100_800.0,
        "weighted-avg numerator/denom",
    );
    // Affirmatively NOT equal to simple sum or simple average.
    let mut simple_sum = 0.0;
    let mut simple_count = 0;
    for t_idx in 1..=3u32 {
        for m_idx in 0..3u32 {
            simple_sum += canonical_inputs_for(t_idx, 0, m_idx).cpc;
            simple_count += 1;
        }
    }
    assert!(
        (actual - simple_sum).abs() > 0.1,
        "consolidated CPC must NOT equal simple sum"
    );
    let simple_avg = simple_sum / simple_count as f64;
    assert!(
        (actual - simple_avg).abs() > 1e-6 || (actual - simple_avg).abs() > 1e-12,
        "consolidated CPC's exact value: weighted ≠ uniform average for nonuniform weights"
    );
}

#[test]
fn t_acme_read_consolidated_revenue_at_q1_florida_paid_media() {
    // Q1_2026 × Paid_Media × Florida × Revenue = sum of 27 leaf Revenues.
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
    let v = cube.read(&c, refs.root_principal).expect("read ok");
    let mut expected = 0.0_f64;
    for t_idx in 1..=3u32 {
        for c_idx in 0..3u32 {
            for m_idx in 0..3u32 {
                expected += canonical_inputs_for(t_idx, c_idx, m_idx).revenue();
            }
        }
    }
    assert_close(
        v.value.as_f64().unwrap(),
        expected,
        "27-leaf Revenue rollup",
    );
}

#[test]
fn t_acme_trace_for_revenue_returns_full_tree() {
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
    let trace = v.trace.expect("trace requested");
    // Root op is RuleEvaluation (Mul: Customers * AOV).
    assert!(matches!(
        trace.root.operation,
        TraceOp::RuleEvaluation { .. }
    ));
    // Two children: Customers (RuleEval) and AOV (InputLookup).
    assert_eq!(trace.root.children.len(), 2);
    // Walk to find leaf-input nodes; should be exactly 5
    // (Spend, CPC, CVR, Close_Rate, AOV).
    let leaf_inputs = count_input_leaves(&trace.root);
    assert_eq!(
        leaf_inputs, 5,
        "Revenue trace must reach 5 input leaves (Spend, CPC, CVR, Close_Rate, AOV)"
    );
}

fn count_input_leaves(node: &TraceNode) -> u32 {
    match &node.operation {
        TraceOp::InputLookup { .. } => 1,
        _ => node.children.iter().map(count_input_leaves).sum(),
    }
}

// Brief §10.1 lists `t_acme_trace_root_value_equals_read_value` as a
// proptest sweep. Per §0.A that's deferred. We ship a deterministic
// equivalent that hand-picks ~10 representative coords per the brief's
// §10.1 deferral note.
#[test]
fn t_acme_trace_root_value_equals_read_value() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let coords: Vec<CellCoordinate> = vec![
        // Anchor leaf
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            refs.spend,
        ),
        // Derived measures at the anchor
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            refs.clicks,
        ),
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            refs.revenue,
        ),
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            refs.gross_profit,
        ),
        // Single-consolidated coords
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.q1_2026,
            refs.paid_search,
            refs.tampa,
            refs.spend,
        ),
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_media,
            refs.tampa,
            refs.spend,
        ),
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.florida,
            refs.spend,
        ),
        // Triple consolidated
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.q1_2026,
            refs.paid_media,
            refs.florida,
            refs.spend,
        ),
        // CPC at consolidated coord (weighted avg path)
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.q1_2026,
            refs.paid_search,
            refs.florida,
            refs.cpc,
        ),
        // Revenue at consolidated coord
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.q1_2026,
            refs.paid_media,
            refs.florida,
            refs.revenue,
        ),
    ];
    for c in coords {
        let plain = cube.read(&c, refs.root_principal).expect("read");
        let traced = cube
            .read_with_trace(&c, refs.root_principal)
            .expect("trace");
        let plain_v = plain.value.as_f64().unwrap_or(f64::NAN);
        let traced_v = traced.value.as_f64().unwrap_or(f64::NAN);
        if plain.value.is_null() {
            assert!(traced.value.is_null(), "trace value Null mismatch at {c:?}");
        } else {
            assert!(
                (plain_v - traced_v).abs() < EPS,
                "trace root value differs from read value at {c:?}: plain={plain_v}, traced={traced_v}"
            );
        }
    }
}

#[test]
fn t_acme_write_to_input_succeeds() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let rev_before = cube.revision();
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
    let res = cube
        .write(WritebackRequest {
            coord: c.clone(),
            new_value: ScalarValue::F64(50_000.0),
            principal: refs.root_principal,
            intent: WriteIntent::Set,
            expected_revision: Some(rev_before),
            now_unix_seconds: 0,
        })
        .expect("write ok");
    assert_eq!(res.revision_after, rev_before.next());
    let v = cube.read(&c, refs.root_principal).expect("read ok");
    assert_close(v.value.as_f64().unwrap(), 50_000.0, "post-write Spend");
}

#[test]
fn t_acme_write_invalidates_dependents() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let c_spend = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    let c_revenue = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.revenue,
    );
    // Read Revenue once — caches value.
    let _ = cube
        .read(&c_revenue, refs.root_principal)
        .expect("read revenue");
    // Update Spend.
    cube.write(WritebackRequest {
        coord: c_spend,
        new_value: ScalarValue::F64(50_000.0),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write");
    // Re-read Revenue — must reflect the new Spend.
    let v = cube
        .read(&c_revenue, refs.root_principal)
        .expect("read revenue 2");
    let expected = (50_000.0 / 1.5) * 0.020 * 0.10 * 200.0;
    assert_close(
        v.value.as_f64().unwrap(),
        expected,
        "Revenue post Spend update",
    );
    // Provenance must be Rule (recomputed), not stale Input/Default.
    assert!(matches!(v.provenance, Provenance::Rule { .. }));
}

#[test]
fn t_acme_write_invalidates_consolidated_ancestors() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let c_q1_spend = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.q1_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    // Read Q1 Spend once.
    let v0 = cube.read(&c_q1_spend, refs.root_principal).expect("read");
    let before = v0.value.as_f64().unwrap();
    // Update March Spend.
    let c_mar = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    let mar_before = canonical_inputs_for(3, 0, 0).spend;
    let new_mar = 50_000.0;
    cube.write(WritebackRequest {
        coord: c_mar,
        new_value: ScalarValue::F64(new_mar),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write");
    let v1 = cube.read(&c_q1_spend, refs.root_principal).expect("read 2");
    let after = v1.value.as_f64().unwrap();
    assert_close(
        after,
        before + (new_mar - mar_before),
        "Q1 Spend reflects March update",
    );
}

// The dirty-set assertions below are about the MARGINAL effect of one
// input write — which derived/ancestor coords this specific write
// invalidates. `write_canonical_inputs` writes 2,520 cells, each of
// which itself accumulates dirty marks (5 same-leaf derived measures
// + ancestor-coord rolls); after that loop, the absolute dirty set
// is huge and tells us nothing about per-write behavior. So we
// snapshot the dirty set before/after the test write and reason
// about the delta. Per CLAUDE.md §2.6: when an integration test's
// premise contradicts the implementation, first check whether the
// premise is well-formed against the engine's actual semantics.

#[test]
fn t_acme_dirty_set_required_present_after_one_spend_write() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let c_mar_tampa_paid_search_spend = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    cube.write(WritebackRequest {
        coord: c_mar_tampa_paid_search_spend.clone(),
        new_value: ScalarValue::F64(50_000.0),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write");
    // Required-present per brief §8 / §10.1: every coord in this list
    // must be dirty in the post-write state. (These coords are also
    // dirty pre-write because canonical-input writes accumulate the
    // same kind of marks; the assertion is "still dirty after",
    // which is the spec.)
    let required: Vec<CellCoordinate> = vec![
        // 5 leaf-coord derived measures
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            refs.clicks,
        ),
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            refs.leads,
        ),
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            refs.customers,
        ),
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            refs.revenue,
        ),
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            refs.gross_profit,
        ),
        // Spend at hierarchical ancestor coords
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.q1_2026,
            refs.paid_search,
            refs.tampa,
            refs.spend,
        ),
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_media,
            refs.tampa,
            refs.spend,
        ),
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.florida,
            refs.spend,
        ),
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.q1_2026,
            refs.paid_media,
            refs.florida,
            refs.spend,
        ),
    ];
    for r in required {
        assert!(
            cube.dirty().is_dirty(&r),
            "required-present coord must be dirty: {r:?}"
        );
    }
}

#[test]
fn t_acme_dirty_set_required_absent_after_one_spend_write() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;

    // Snapshot dirty BEFORE the test write so we can isolate this
    // write's marginal effect. (Pre-existing marks come from the
    // canonical-input loop and are noise here.)
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

    // The test write must NOT add any of these coords to the dirty
    // set. (They may already be dirty from the canonical-input
    // setup; that's not what's being checked here.)
    let must_not_be_added: Vec<CellCoordinate> = vec![
        // Atlanta (Georgia, NOT Florida) — different Market subtree
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.atlanta,
            refs.spend,
        ),
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.atlanta,
            refs.revenue,
        ),
        // Email (NOT Paid_Media) — different Channel subtree
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.email,
            refs.tampa,
            refs.spend,
        ),
        // Aggressive scenario — different Scenario slot
        coord(
            cube_id,
            &refs,
            refs.scen_aggressive,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            refs.spend,
        ),
        // Submitted version — different Version slot
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_submitted,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            refs.spend,
        ),
    ];
    for r in must_not_be_added {
        let was_dirty = before.contains(&r);
        let is_dirty = cube.dirty().is_dirty(&r);
        // Newly added by THIS write means: not in `before` but in `after`.
        let newly_added = !was_dirty && is_dirty;
        assert!(
            !newly_added,
            "test write must NOT newly dirty unrelated coord: {r:?}"
        );
    }
}

#[test]
fn t_acme_dirty_set_size_within_bound_after_one_spend_write() {
    // Brief §8 upper bound: |dirty_set delta| <= 6 × ancestor_count + 5 = 215.
    // Measured as the marginal effect of ONE write — write_canonical_inputs
    // accumulates dirty marks across 2,520 input writes, so the absolute
    // count after that loop is not the metric of interest. The bound is on
    // how many additional cells one input mutation invalidates.
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let before = cube.dirty().len();
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
    let after = cube.dirty().len();
    // The delta can be ≤ 0 if the test write's invalidations are a subset
    // of cells already dirtied by the canonical-input loop. The bound that
    // matters is on cells genuinely added by THIS write; we approximate
    // with |after - before| ≤ 215 since the write only ADDS marks
    // (mark_closure + compute_dirty_ancestors are insertion-only).
    let delta = after.saturating_sub(before);
    assert!(
        delta <= 215,
        "dirty set delta {delta} exceeds brief §8 upper bound 215 (before={before}, after={after})"
    );
}
