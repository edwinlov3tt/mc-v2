// Tests for `mc model grade` (Phase 10B, ADR-0034). Included into
// `grade.rs` as `mod tests`, so `super::*` exposes the private engine.
//
// Per CLAUDE.md §4.5: every inline cube uses SINGLE braces `{ }`. The
// construct-then-assert integration tests build a real cube via the
// loader (write temp YAML + inputs CSV, then `load_model_with_policy`);
// a cube that fails to *load* surfaces as a panic in the test, distinct
// from an assertion mismatch.

use super::*;
// Phase 10C.1 (ADR-0036 Amdt 8): the metric grammar, reductions, bucket
// assignment, Filter guard, and segment-sort moved to `eval_common`. grade
// re-uses them; these tests reach the engine primitives directly from there.
// (Names grade's own non-test code uses — parse_metric_expr, MetricExpr,
// SegmentStatus, etc. — arrive via `super::*` and are not re-imported here.)
use crate::eval_common::{
    assign_bucket, cmp_sort_vecs, guard_filter_f64_equality, reduce_max, reduce_mean, reduce_min,
    reduce_std, reduce_sum, BandAssignment, Reduction, SortKey,
};
// The holdout-filter guard tests construct `Filter` ASTs directly; these
// come from `query` (grade's non-test code only needs `CmpOp`, which arrives
// via `super::*`). The Wilson compute fns live in `mc-core`.
use crate::query::{Filter, FilterAtom, FilterValue};
use mc_core::rule::{wilson_ci_lower_compute, wilson_ci_upper_compute};
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

const TOL_HEADLINE: f64 = 1e-3;
const TOL_EXACT: f64 = 1e-9;

// ---------------------------------------------------------------------------
// Metric-expression parser (Amendment 11)
// ---------------------------------------------------------------------------

#[test]
fn t_parse_metric_each_reduction() {
    for (s, red, arity) in [
        ("n=count(x)", Reduction::Count, 1),
        ("m=mean(x)", Reduction::Mean, 1),
        ("s=sum(x)", Reduction::Sum, 1),
        ("d=std(x)", Reduction::Std, 1),
        ("lo=min(x)", Reduction::Min, 1),
        ("hi=max(x)", Reduction::Max, 1),
        ("wl=wilson_lower(x)", Reduction::WilsonLower, 1),
        ("wu=wilson_upper(x)", Reduction::WilsonUpper, 1),
        ("r=ratio(a,b)", Reduction::Ratio, 2),
    ] {
        let m = parse_metric_expr(s).expect("valid metric");
        assert_eq!(m.reduction, red, "reduction for {s}");
        assert_eq!(m.ingredients.len(), arity, "arity for {s}");
    }
}

#[test]
fn t_parse_metric_whitespace_tolerant() {
    let a = parse_metric_expr("win_rate = mean( direction_correct )").expect("ws form");
    let b = parse_metric_expr("win_rate=mean(direction_correct)").expect("tight form");
    assert_eq!(a.name, "win_rate");
    assert_eq!(a.name, b.name);
    assert_eq!(a.ingredients, b.ingredients);
    let r = parse_metric_expr("roi = ratio( pnl , stake )").expect("ratio ws");
    assert_eq!(r.ingredients, vec!["pnl".to_string(), "stake".to_string()]);
}

#[test]
fn t_parse_metric_unknown_reduction_error_ux() {
    let err = parse_metric_expr("x=avgg(y)").unwrap_err();
    assert!(err.contains("unknown reduction"), "got: {err}");
    assert!(err.contains("count, mean, sum, ratio"), "expected list in: {err}");
    assert!(err.contains("wilson_upper"), "full list in: {err}");
}

#[test]
fn t_parse_metric_wrong_arity() {
    let e1 = parse_metric_expr("r=ratio(a)").unwrap_err();
    assert!(e1.contains("ratio() takes exactly 2"), "got: {e1}");
    let e2 = parse_metric_expr("r=ratio(a,b,c)").unwrap_err();
    assert!(e2.contains("ratio() takes exactly 2"), "got: {e2}");
    let e3 = parse_metric_expr("m=mean(a,b)").unwrap_err();
    assert!(e3.contains("mean() takes exactly 1"), "got: {e3}");
}

#[test]
fn t_parse_metric_malformed() {
    assert!(parse_metric_expr("noequals").is_err());
    assert!(parse_metric_expr("=mean(x)").is_err());
    assert!(parse_metric_expr("m=mean(x").is_err()); // missing close
    assert!(parse_metric_expr("m=mean()").is_err()); // empty ingredient
    assert!(parse_metric_expr("m=mean(a,)").is_err()); // stray comma
}

// ---------------------------------------------------------------------------
// Bucket parsing + assignment (Amendment 2 / Decision 2)
// ---------------------------------------------------------------------------

#[test]
fn t_parse_bucket_edges() {
    assert_eq!(parse_bucket_edges("0:0.5:1.0").unwrap(), vec![0.0, 0.5, 1.0]);
    assert!(parse_bucket_edges("0").is_err(), "single edge");
    assert!(parse_bucket_edges("1:0").is_err(), "descending");
    assert!(parse_bucket_edges("0:0:1").is_err(), "zero-width band");
    assert!(parse_bucket_edges("a:b").is_err(), "non-numeric");
}

#[test]
fn t_assign_bucket_left_closed_right_open() {
    let edges = [0.0, 0.10, 0.20, 1.0];
    // 0.0 → first band [0,0.10)
    match assign_bucket(0.0, &edges) {
        BandAssignment::Band { lower, .. } => assert!((lower - 0.0).abs() < TOL_EXACT),
        other => panic!("expected band, got {other:?}"),
    }
    // 0.10 is the boundary → upper band [0.10,0.20) (left-closed)
    match assign_bucket(0.10, &edges) {
        BandAssignment::Band { lower, .. } => assert!((lower - 0.10).abs() < TOL_EXACT),
        other => panic!("boundary should land in upper band, got {other:?}"),
    }
    // 1.0 → last band [0.20,1.0] is right-CLOSED
    match assign_bucket(1.0, &edges) {
        BandAssignment::Band { lower, .. } => assert!((lower - 0.20).abs() < TOL_EXACT),
        other => panic!("right edge of last band should be included, got {other:?}"),
    }
}

#[test]
fn t_assign_bucket_out_of_range() {
    let edges = [0.0, 0.5, 1.0];
    assert_eq!(assign_bucket(-0.1, &edges), BandAssignment::OutOfRange);
    assert_eq!(assign_bucket(1.1, &edges), BandAssignment::OutOfRange);
}

// ---------------------------------------------------------------------------
// Reductions (Amendment 7 vocabulary; Amendment 3/6 semantics)
// ---------------------------------------------------------------------------

#[test]
fn t_reduce_basic() {
    let v = [1.0, 2.0, 3.0, 4.0, 5.0];
    assert!((reduce_sum(&v) - 15.0).abs() < TOL_EXACT);
    assert!((reduce_mean(&v).unwrap() - 3.0).abs() < TOL_EXACT);
    // Sample std (ddof=1) — matches Phase 10A fixture.
    assert!((reduce_std(&v).unwrap() - 1.5811388300841898).abs() < 1e-9);
    assert!((reduce_min(&v).unwrap() - 1.0).abs() < TOL_EXACT);
    assert!((reduce_max(&v).unwrap() - 5.0).abs() < TOL_EXACT);
}

#[test]
fn t_reduce_empty_and_singleton() {
    let empty: [f64; 0] = [];
    assert!(reduce_mean(&empty).is_none());
    assert!(reduce_std(&empty).is_none());
    assert!(reduce_min(&empty).is_none());
    assert!(reduce_max(&empty).is_none());
    assert!((reduce_sum(&empty)).abs() < TOL_EXACT);
    assert!(reduce_std(&[42.0]).is_none(), "std undefined for n<2");
}

#[test]
fn t_wilson_reduction_parity_exp048_under() {
    // The EXP-048 UNDER segment: 295 correct of 449 → p≈0.6570.
    // Wilson 95% CI must match the documented [0.6119, 0.6994] (and the
    // metrics.rs continuous-p reference) within the 1e-3 headline tol.
    let mut vals = vec![1.0; 295];
    vals.extend(std::iter::repeat(0.0).take(154));
    assert_eq!(vals.len(), 449);
    let n = vals.len() as f64;
    let p = reduce_sum(&vals) / n;
    let lo = wilson_ci_lower_compute(p, n).unwrap();
    let hi = wilson_ci_upper_compute(p, n).unwrap();
    assert!((p - 0.6570).abs() < TOL_HEADLINE, "p: {p}");
    assert!((lo - 0.6119).abs() < TOL_HEADLINE, "wilson lower: {lo}");
    assert!((hi - 0.6994).abs() < TOL_HEADLINE, "wilson upper: {hi}");
}

// ---------------------------------------------------------------------------
// Holdout F64-equality guard (Amendment 1)
// ---------------------------------------------------------------------------

#[test]
fn t_guard_rejects_bare_f64_measure_equality() {
    let f = Filter::Compare(
        FilterAtom::Measure("line".into()),
        CmpOp::Eq,
        FilterValue::Number(9.0),
    );
    let err = guard_filter_f64_equality(&f).unwrap_err();
    assert!(err.contains("bare equality"), "got: {err}");
    assert!(err.contains("line"), "names the measure: {err}");
}

#[test]
fn t_guard_allows_dimension_eq_and_measure_range() {
    // Dimension pin with a string literal is fine.
    let dim = Filter::Compare(
        FilterAtom::Dimension("Time".into()),
        CmpOp::Eq,
        FilterValue::StringLit("2025".into()),
    );
    assert!(guard_filter_f64_equality(&dim).is_ok());

    // Range predicate on a measure is fine: line >= 8.99 and line <= 9.01.
    let range = Filter::And(
        Box::new(Filter::Compare(
            FilterAtom::Measure("line".into()),
            CmpOp::Gte,
            FilterValue::Number(8.99),
        )),
        Box::new(Filter::Compare(
            FilterAtom::Measure("line".into()),
            CmpOp::Lte,
            FilterValue::Number(9.01),
        )),
    );
    assert!(guard_filter_f64_equality(&range).is_ok());

    // But equality buried in an AND is still caught.
    let buried = Filter::And(
        Box::new(dim_ok()),
        Box::new(Filter::Compare(
            FilterAtom::Measure("line".into()),
            CmpOp::Eq,
            FilterValue::Number(9.0),
        )),
    );
    assert!(guard_filter_f64_equality(&buried).is_err());
}

fn dim_ok() -> Filter {
    Filter::Compare(
        FilterAtom::Dimension("Time".into()),
        CmpOp::Eq,
        FilterValue::StringLit("2025".into()),
    )
}

// ---------------------------------------------------------------------------
// Flag predicate
// ---------------------------------------------------------------------------

#[test]
fn t_flag_predicate_parse_and_eval() {
    let names = vec!["wr_lower_95".to_string(), "n".to_string()];
    let p = FlagPredicate::parse("wr_lower_95 < 0.50", &names).expect("valid");
    assert_eq!(p.metric_index, 0);
    assert!(p.eval(0.49));
    assert!(!p.eval(0.51));

    assert!(FlagPredicate::parse("unknown > 1", &names).is_err());
    assert!(FlagPredicate::parse("wr_lower_95 ~ 1", &names).is_err());
    assert!(FlagPredicate::parse("wr_lower_95 < x", &names).is_err());

    // == uses an epsilon, never raw float equality.
    let eqp = FlagPredicate::parse("n == 7", &names).expect("eq");
    assert!(eqp.eval(7.0));
    assert!(!eqp.eval(7.5));
}

// ---------------------------------------------------------------------------
// Segment ordering (Amendment 12): numeric bands sort by lower edge.
// ---------------------------------------------------------------------------

#[test]
fn t_segment_ordering_numeric_bands() {
    // Lexicographically "[0.1..." < "[0.0..." would be WRONG; numeric
    // ordering by lower edge must put 0.0 before 0.1 before 0.2.
    let mut v = [
        vec![SortKey::Num(0.2)],
        vec![SortKey::Num(0.0)],
        vec![SortKey::Special(1)], // out-of-range sorts last
        vec![SortKey::Num(0.1)],
    ];
    v.sort_by(|a, b| cmp_sort_vecs(a, b));
    let firsts: Vec<f64> = v
        .iter()
        .map(|k| match &k[0] {
            SortKey::Num(x) => *x,
            SortKey::Special(_) => 999.0,
            SortKey::Text(_) => -1.0,
        })
        .collect();
    assert_eq!(firsts, vec![0.0, 0.1, 0.2, 999.0]);
}

// ---------------------------------------------------------------------------
// Integration: temp-cube helpers
// ---------------------------------------------------------------------------

static TMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Write a YAML model + inputs CSV to a unique temp dir; return the YAML
/// path. Uniqueness without RNG/clock: pid + a monotonic counter.
fn temp_model(yaml: &str, inputs_csv: &str) -> std::path::PathBuf {
    let n = TMP_COUNTER.fetch_add(1, AtomicOrdering::SeqCst);
    let dir = std::env::temp_dir().join(format!("mc_grade_{}_{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    std::fs::write(dir.join("inputs.csv"), inputs_csv).expect("write inputs");
    let yaml_path = dir.join("model.yaml");
    std::fs::write(&yaml_path, yaml).expect("write yaml");
    yaml_path
}

const MINIMAL_HEADER: &str = r#"model_format_version: 1
canonical_inputs:
  source: "inputs.csv"
  columns: ["Scenario", "Version", "Game", "Measure", "value"]
metadata:
  name: "grade_test"
  description: "phase-10b test cube"
  author: "test"
  created: "2026-05-30"
"#;

/// EXP-048 reproduction cube: 449 UNDER (bet_side=0, 295 correct) + 7 OVER
/// (bet_side=1, 3 correct). Single scenario so units == games.
fn exp048_model() -> (String, String) {
    let yaml = format!(
        "{MINIMAL_HEADER}\
dimensions:
  - {{ name: Scenario, kind: Scenario, elements: [ {{ name: base }} ] }}
  - {{ name: Version, kind: Version, elements: [ {{ name: Working }} ] }}
  - {{ name: Game, kind: Standard, elements: [] }}
  - {{ name: Measure, kind: Measure, elements: [] }}
measures:
  - {{ name: bet_side, role: Input, data_type: F64, aggregation: Sum }}
  - {{ name: direction_correct, role: Input, data_type: F64, aggregation: Sum }}
"
    );
    let mut csv = String::from("Scenario,Version,Game,Measure,value\n");
    for i in 0..449 {
        let dc = if i < 295 { 1.0 } else { 0.0 };
        let _ = writeln!(csv, "base,Working,under_{i},bet_side,0");
        let _ = writeln!(csv, "base,Working,under_{i},direction_correct,{dc}");
    }
    for i in 0..7 {
        let dc = if i < 3 { 1.0 } else { 0.0 };
        let _ = writeln!(csv, "base,Working,over_{i},bet_side,1");
        let _ = writeln!(csv, "base,Working,over_{i},direction_correct,{dc}");
    }
    (yaml, csv)
}

fn metric(s: &str) -> MetricExpr {
    parse_metric_expr(s).expect("test metric parses")
}

fn load(yaml: &str, csv: &str) -> (mc_core::Cube, mc_model::ModelRefs, mc_core::PrincipalId) {
    let path = temp_model(yaml, csv);
    let loaded = load_model_with_policy(path.to_str().unwrap(), LoadPolicy::Reproducible)
        .expect("test cube loads");
    (loaded.cube, loaded.refs, loaded.root_principal)
}

// ---------------------------------------------------------------------------
// Integration: EXP-048 headline reproduction (AC #16)
// ---------------------------------------------------------------------------

#[test]
fn t_exp048_reproduction_bet_side_buckets() {
    let (yaml, csv) = exp048_model();
    let (mut cube, refs, principal) = load(&yaml, &csv);

    let mut buckets = BTreeMap::new();
    buckets.insert("bet_side".to_string(), vec![0.0, 0.5, 1.0]);
    let cmd = GradeCommand {
        path: "exp048.yaml".into(),
        unit: "Game".into(),
        holdout: None,
        group_by: vec!["bet_side".into()],
        metrics: vec![
            metric("n=count(direction_correct)"),
            metric("win_rate=mean(direction_correct)"),
            metric("wr_lower_95=wilson_lower(direction_correct)"),
            metric("wr_upper_95=wilson_upper(direction_correct)"),
        ],
        buckets,
        flag_if: None,
        min_n: 0,
        max_segments: 50,
        wilson_null: WilsonNullPolicy::Error,
        include_writes: false,
        format: GradeFormat::Json,
    };

    let report = grade_cube(&mut cube, &refs, principal, &cmd).expect("grade runs");
    assert_eq!(report.segments.len(), 2, "two bet-side bands");

    // Ordering: lower edge 0.0 (UNDER) before 0.5 (OVER).
    let under = &report.segments[0];
    let over = &report.segments[1];
    assert_eq!(under.n_units, 449, "UNDER n");
    assert_eq!(over.n_units, 7, "OVER n");

    // metrics: [n, win_rate, wr_lower_95, wr_upper_95]
    assert!((under.metrics[0].unwrap() - 449.0).abs() < TOL_EXACT);
    assert!((under.metrics[1].unwrap() - 0.6570).abs() < TOL_HEADLINE, "UNDER wr");
    assert!((under.metrics[2].unwrap() - 0.6119).abs() < TOL_HEADLINE, "UNDER wilson lo");
    assert!((under.metrics[3].unwrap() - 0.6994).abs() < TOL_HEADLINE, "UNDER wilson hi");
    assert!((over.metrics[1].unwrap() - 0.4286).abs() < TOL_HEADLINE, "OVER wr");

    // TOTAL inclusive of both bands.
    assert_eq!(report.total.n_units, 456, "TOTAL n");
    assert!((report.total.metrics[1].unwrap() - 0.6535).abs() < TOL_HEADLINE, "TOTAL wr");
}

// ---------------------------------------------------------------------------
// Integration: continuous group-by without --bucket is a hard error (AC #25)
// ---------------------------------------------------------------------------

#[test]
fn t_continuous_groupby_without_bucket_errors() {
    let (yaml, csv) = exp048_model();
    let (mut cube, refs, principal) = load(&yaml, &csv);
    let cmd = GradeCommand {
        path: "x.yaml".into(),
        unit: "Game".into(),
        holdout: None,
        group_by: vec!["bet_side".into()], // F64 measure, no --bucket
        metrics: vec![metric("n=count(direction_correct)")],
        buckets: BTreeMap::new(),
        flag_if: None,
        min_n: 0,
        max_segments: 50,
        wilson_null: WilsonNullPolicy::Error,
        include_writes: false,
        format: GradeFormat::Text,
    };
    let err = grade_cube(&mut cube, &refs, principal, &cmd).unwrap_err();
    assert!(err.contains("continuous measure"), "got: {err}");
    assert!(err.contains("--bucket"), "suggests bucket: {err}");
}

// ---------------------------------------------------------------------------
// Phase 10B.1: grouping a non-numeric measure by distinct value is a hard
// error (not the old silent "distinct value" path). A `data_type: Bool`
// input measure is the reachable trigger — `parse_value` produces a stored
// `ScalarValue::Bool` cell, which `read_measure_at` returns and the group-key
// resolver rejects.
// ---------------------------------------------------------------------------

fn bool_measure_model() -> (String, String) {
    let yaml = format!(
        "{MINIMAL_HEADER}\
dimensions:
  - {{ name: Scenario, kind: Scenario, elements: [ {{ name: base }} ] }}
  - {{ name: Version, kind: Version, elements: [ {{ name: Working }} ] }}
  - {{ name: Game, kind: Standard, elements: [] }}
  - {{ name: Measure, kind: Measure, elements: [] }}
measures:
  - {{ name: is_home, role: Input, data_type: Bool, aggregation: Sum }}
  - {{ name: pnl, role: Input, data_type: F64, aggregation: Sum }}
"
    );
    let mut csv = String::from("Scenario,Version,Game,Measure,value\n");
    for g in 0..4 {
        let home = if g % 2 == 0 { "true" } else { "false" };
        let _ = writeln!(csv, "base,Working,g{g},is_home,{home}");
        let _ = writeln!(csv, "base,Working,g{g},pnl,1.0");
    }
    (yaml, csv)
}

#[test]
fn t_string_measure_groupby_errors() {
    let (yaml, csv) = bool_measure_model();
    let (mut cube, refs, principal) = load(&yaml, &csv);
    let cmd = GradeCommand {
        path: "x.yaml".into(),
        unit: "Game".into(),
        holdout: None,
        group_by: vec!["is_home".into()], // non-numeric measure → hard error
        metrics: vec![metric("total=sum(pnl)")],
        buckets: BTreeMap::new(),
        flag_if: None,
        min_n: 0,
        max_segments: 50,
        wilson_null: WilsonNullPolicy::Error,
        include_writes: false,
        format: GradeFormat::Text,
    };
    let err = grade_cube(&mut cube, &refs, principal, &cmd).unwrap_err();
    assert!(err.contains("non-numeric"), "got: {err}");
    assert!(
        err.contains("not supported") && err.contains("dimension"),
        "actionable alternative: {err}"
    );
}

#[test]
fn t_max_segments_cap_errors() {
    let (yaml, csv) = exp048_model();
    let (mut cube, refs, principal) = load(&yaml, &csv);
    let mut buckets = BTreeMap::new();
    buckets.insert("bet_side".to_string(), vec![0.0, 0.5, 1.0]);
    let cmd = GradeCommand {
        path: "x.yaml".into(),
        unit: "Game".into(),
        holdout: None,
        group_by: vec!["bet_side".into()],
        metrics: vec![metric("n=count(direction_correct)")],
        buckets,
        flag_if: None,
        min_n: 0,
        max_segments: 1, // 2 bands > 1 → error
        wilson_null: WilsonNullPolicy::Error,
        include_writes: false,
        format: GradeFormat::Text,
    };
    let err = grade_cube(&mut cube, &refs, principal, &cmd).unwrap_err();
    assert!(err.contains("max-segments"), "got: {err}");
}

// ---------------------------------------------------------------------------
// Integration: --min-n marks small segment, TOTAL still inclusive (AC #12)
// ---------------------------------------------------------------------------

#[test]
fn t_min_n_excludes_from_flags_keeps_in_total() {
    let (yaml, csv) = exp048_model();
    let (mut cube, refs, principal) = load(&yaml, &csv);
    let mut buckets = BTreeMap::new();
    buckets.insert("bet_side".to_string(), vec![0.0, 0.5, 1.0]);
    let cmd = GradeCommand {
        path: "x.yaml".into(),
        unit: "Game".into(),
        holdout: None,
        group_by: vec!["bet_side".into()],
        metrics: vec![
            metric("n=count(direction_correct)"),
            metric("win_rate=mean(direction_correct)"),
        ],
        buckets,
        // OVER (n=7) has win_rate 0.4286 < 0.5; would flag if eligible.
        flag_if: Some("win_rate < 0.50".into()),
        min_n: 25,
        max_segments: 50,
        wilson_null: WilsonNullPolicy::Error,
        include_writes: false,
        format: GradeFormat::Text,
    };
    let report = grade_cube(&mut cube, &refs, principal, &cmd).expect("grade runs");
    let over = report
        .segments
        .iter()
        .find(|s| s.n_units == 7)
        .expect("OVER segment present");
    assert_eq!(over.status, SegmentStatus::BelowMinN, "OVER below min-n");
    assert!(over.flagged.is_empty(), "below-min-n excluded from flagging");
    assert_eq!(report.flagged_count, 0, "no segment flagged");
    // TOTAL still counts all 456 units (Amendment 9).
    assert_eq!(report.total.n_units, 456);
}

// ---------------------------------------------------------------------------
// Integration: dimension grouping + cartesian + TOTAL (AC #2, #12)
// ---------------------------------------------------------------------------

fn two_scenario_model() -> (String, String) {
    let yaml = format!(
        "{MINIMAL_HEADER}\
dimensions:
  - {{ name: Scenario, kind: Scenario, elements: [ {{ name: s1 }}, {{ name: s2 }} ] }}
  - {{ name: Version, kind: Version, elements: [ {{ name: Working }} ] }}
  - {{ name: Game, kind: Standard, elements: [] }}
  - {{ name: Measure, kind: Measure, elements: [] }}
measures:
  - {{ name: val, role: Input, data_type: F64, aggregation: Sum }}
"
    );
    let mut csv = String::from("Scenario,Version,Game,Measure,value\n");
    for s in ["s1", "s2"] {
        for g in 0..4 {
            let _ = writeln!(csv, "{s},Working,g{g},val,{}", g as f64 + 1.0);
        }
    }
    (yaml, csv)
}

#[test]
fn t_dimension_grouping() {
    let (yaml, csv) = two_scenario_model();
    let (mut cube, refs, principal) = load(&yaml, &csv);
    let cmd = GradeCommand {
        path: "x.yaml".into(),
        unit: "Game".into(),
        holdout: None,
        group_by: vec!["Scenario".into()],
        metrics: vec![metric("n=count(val)"), metric("avg=mean(val)")],
        buckets: BTreeMap::new(),
        flag_if: None,
        min_n: 0,
        max_segments: 50,
        wilson_null: WilsonNullPolicy::Error,
        include_writes: false,
        format: GradeFormat::Text,
    };
    let report = grade_cube(&mut cube, &refs, principal, &cmd).expect("grade runs");
    assert_eq!(report.segments.len(), 2, "one segment per scenario element");
    assert_eq!(report.segments[0].keys[0].1, "s1");
    assert_eq!(report.segments[1].keys[0].1, "s2");
    // mean of val [1,2,3,4] = 2.5 per scenario.
    assert!((report.segments[0].metrics[1].unwrap() - 2.5).abs() < TOL_EXACT);
    assert_eq!(report.total.n_units, 8, "cartesian TOTAL = 2 scenarios × 4 games");
}

// ---------------------------------------------------------------------------
// Integration: ratio denom-zero → Null + diagnostic (AC #7, Amendment 6)
// ---------------------------------------------------------------------------

fn ratio_zero_model() -> (String, String) {
    let yaml = format!(
        "{MINIMAL_HEADER}\
dimensions:
  - {{ name: Scenario, kind: Scenario, elements: [ {{ name: base }} ] }}
  - {{ name: Version, kind: Version, elements: [ {{ name: Working }} ] }}
  - {{ name: Game, kind: Standard, elements: [] }}
  - {{ name: Measure, kind: Measure, elements: [] }}
measures:
  - {{ name: pnl, role: Input, data_type: F64, aggregation: Sum }}
  - {{ name: stake, role: Input, data_type: F64, aggregation: Sum }}
"
    );
    let mut csv = String::from("Scenario,Version,Game,Measure,value\n");
    for g in 0..3 {
        let _ = writeln!(csv, "base,Working,g{g},pnl,1.0");
        let _ = writeln!(csv, "base,Working,g{g},stake,0.0"); // denominator all zero
    }
    (yaml, csv)
}

#[test]
fn t_ratio_denominator_zero_is_null() {
    let (yaml, csv) = ratio_zero_model();
    let (mut cube, refs, principal) = load(&yaml, &csv);
    let cmd = GradeCommand {
        path: "x.yaml".into(),
        unit: "Game".into(),
        holdout: None,
        group_by: vec![],
        metrics: vec![metric("roi=ratio(pnl,stake)")],
        buckets: BTreeMap::new(),
        flag_if: None,
        min_n: 0,
        max_segments: 50,
        wilson_null: WilsonNullPolicy::Error,
        include_writes: false,
        format: GradeFormat::Text,
    };
    let report = grade_cube(&mut cube, &refs, principal, &cmd).expect("grade runs");
    // No group-by → only TOTAL carries the metric.
    assert!(report.total.metrics[0].is_none(), "roi must be Null, not inf/NaN/0");
    assert!(
        report.warnings.iter().any(|w| w.contains("denominator")),
        "diagnostic warning present"
    );
}

// ---------------------------------------------------------------------------
// Integration: Wilson Null indicator → hard error / drop (AC #9, Amendment 3)
// ---------------------------------------------------------------------------

fn null_indicator_model() -> (String, String) {
    // `ind` is Null when base <= 0.5 (the cookbook footgun) — half the
    // games get a Null indicator.
    let yaml = format!(
        "{MINIMAL_HEADER}\
dimensions:
  - {{ name: Scenario, kind: Scenario, elements: [ {{ name: base }} ] }}
  - {{ name: Version, kind: Version, elements: [ {{ name: Working }} ] }}
  - {{ name: Game, kind: Standard, elements: [] }}
  - {{ name: Measure, kind: Measure, elements: [] }}
measures:
  - {{ name: raw, role: Input, data_type: F64, aggregation: Sum }}
  - {{ name: ind, role: Derived, data_type: F64, aggregation: Sum }}
rules:
  - {{ name: r_ind, target_measure: ind, body: \"if(raw > 0.5, 1.0, Null)\", scope: AllLeaves, declared_dependencies: [raw] }}
"
    );
    let mut csv = String::from("Scenario,Version,Game,Measure,value\n");
    for g in 0..4 {
        let raw = if g % 2 == 0 { 1.0 } else { 0.0 };
        let _ = writeln!(csv, "base,Working,g{g},raw,{raw}");
    }
    (yaml, csv)
}

#[test]
fn t_wilson_null_indicator_hard_errors_by_default() {
    let (yaml, csv) = null_indicator_model();
    let (mut cube, refs, principal) = load(&yaml, &csv);
    let cmd = GradeCommand {
        path: "x.yaml".into(),
        unit: "Game".into(),
        holdout: None,
        group_by: vec![],
        metrics: vec![metric("wl=wilson_lower(ind)")],
        buckets: BTreeMap::new(),
        flag_if: None,
        min_n: 0,
        max_segments: 50,
        wilson_null: WilsonNullPolicy::Error,
        include_writes: false,
        format: GradeFormat::Text,
    };
    let err = grade_cube(&mut cube, &refs, principal, &cmd).unwrap_err();
    assert!(err.contains("Null"), "mentions Null: {err}");
    assert!(err.contains("--wilson-null drop"), "suggests escape hatch: {err}");
}

#[test]
fn t_wilson_null_drop_excludes_and_warns() {
    let (yaml, csv) = null_indicator_model();
    let (mut cube, refs, principal) = load(&yaml, &csv);
    let cmd = GradeCommand {
        path: "x.yaml".into(),
        unit: "Game".into(),
        holdout: None,
        group_by: vec![],
        metrics: vec![
            metric("n=count(ind)"),
            metric("wl=wilson_lower(ind)"),
        ],
        buckets: BTreeMap::new(),
        flag_if: None,
        min_n: 0,
        max_segments: 50,
        wilson_null: WilsonNullPolicy::Drop,
        include_writes: false,
        format: GradeFormat::Text,
    };
    let report = grade_cube(&mut cube, &refs, principal, &cmd).expect("drop policy runs");
    // 2 of 4 games have ind=1.0; the other 2 are Null and excluded.
    assert!((report.total.metrics[0].unwrap() - 2.0).abs() < TOL_EXACT, "n excludes nulls");
    assert!(report.total.metrics[1].is_some(), "wilson computed on n=2");
    assert!(
        report.warnings.iter().any(|w| w.contains("dropped")),
        "drop warning present"
    );
}

// ---------------------------------------------------------------------------
// Integration: holdout reuses Filter grammar (AC #24) + Reproducible default
// ---------------------------------------------------------------------------

#[test]
fn t_holdout_filter_dimension_pin() {
    let (yaml, csv) = two_scenario_model();
    let (mut cube, refs, principal) = load(&yaml, &csv);
    let cmd = GradeCommand {
        path: "x.yaml".into(),
        unit: "Game".into(),
        // Pin Scenario to s1 via the Filter grammar (== with a string lit).
        holdout: Some("Scenario == \"s1\"".into()),
        group_by: vec!["Scenario".into()],
        metrics: vec![metric("n=count(val)")],
        buckets: BTreeMap::new(),
        flag_if: None,
        min_n: 0,
        max_segments: 50,
        wilson_null: WilsonNullPolicy::Error,
        include_writes: false,
        format: GradeFormat::Text,
    };
    let report = grade_cube(&mut cube, &refs, principal, &cmd).expect("grade runs");
    assert_eq!(report.segments.len(), 1, "holdout pinned to one scenario");
    assert_eq!(report.segments[0].keys[0].1, "s1");
    assert_eq!(report.total.n_units, 4, "only s1's 4 games survive the filter");
}

#[test]
fn t_holdout_bare_f64_equality_rejected_end_to_end() {
    let (yaml, csv) = exp048_model();
    let (mut cube, refs, principal) = load(&yaml, &csv);
    let cmd = GradeCommand {
        path: "x.yaml".into(),
        unit: "Game".into(),
        holdout: Some("bet_side == 0".into()), // bare F64 equality
        group_by: vec![],
        metrics: vec![metric("n=count(direction_correct)")],
        buckets: BTreeMap::new(),
        flag_if: None,
        min_n: 0,
        max_segments: 50,
        wilson_null: WilsonNullPolicy::Error,
        include_writes: false,
        format: GradeFormat::Text,
    };
    let err = grade_cube(&mut cube, &refs, principal, &cmd).unwrap_err();
    assert!(err.contains("bare equality") || err.contains("hazardous"), "got: {err}");
}

// ---------------------------------------------------------------------------
// Integration: JSON shape (AC #14, #28) + determinism (AC #15, #23)
// ---------------------------------------------------------------------------

fn exp048_cmd(path: &str, format: GradeFormat) -> GradeCommand {
    let mut buckets = BTreeMap::new();
    buckets.insert("bet_side".to_string(), vec![0.0, 0.5, 1.0]);
    GradeCommand {
        path: path.to_string(),
        unit: "Game".into(),
        holdout: None,
        group_by: vec!["bet_side".into()],
        metrics: vec![
            metric("n=count(direction_correct)"),
            metric("win_rate=mean(direction_correct)"),
            metric("wr_lower_95=wilson_lower(direction_correct)"),
        ],
        buckets,
        flag_if: Some("wr_lower_95 < 0.50".into()),
        min_n: 0,
        max_segments: 50,
        wilson_null: WilsonNullPolicy::Error,
        include_writes: false,
        format,
    }
}

#[test]
fn t_json_shape_has_amendment5_fields() {
    let (yaml, csv) = exp048_model();
    let (mut cube, refs, principal) = load(&yaml, &csv);
    let cmd = exp048_cmd("exp048.yaml", GradeFormat::Json);
    let report = grade_cube(&mut cube, &refs, principal, &cmd).expect("grade runs");
    let json = format_json(&cmd, &report);
    for needle in [
        "\"schema_version\": \"1.0\"",
        "\"group_by\": [\"bet_side\"]",
        "\"bucket\":",
        "\"status\":",
        "\"null_counts\":",
        "\"warnings\":",
        "\"denominator_zero_segments\":",
        "\"flagged_count\":",
        "\"subtotals\": []",
        "\"total\":",
    ] {
        assert!(json.contains(needle), "JSON missing {needle}:\n{json}");
    }
}

#[test]
fn t_duplicate_n_column_suppressed() {
    // Phase 10B.1: the canonical EXP-048 form defines `--metric n=count(...)`.
    // The built-in unit-count column/key must NOT also appear, or text shows
    // two `n` columns and JSON carries a duplicate `"n"` key.
    let (yaml, csv) = exp048_model();
    let (mut cube, refs, principal) = load(&yaml, &csv);
    let cmd = exp048_cmd("exp048.yaml", GradeFormat::Text);
    let report = grade_cube(&mut cube, &refs, principal, &cmd).expect("grade runs");

    // Text: header row has exactly one `n` cell.
    let text = format_text(&cmd, &report);
    let header_line = text.lines().find(|l| l.contains("bet_side")).expect("header");
    let n_cols = header_line.split('|').filter(|c| c.trim() == "n").count();
    assert_eq!(n_cols, 1, "exactly one `n` column, got header: {header_line:?}");

    // JSON: each metrics object has exactly one `"n"` key.
    let json = format_json(&cmd, &report);
    let dup = json.matches("\"n\":").count();
    // segments (2) + total (1) = 3 objects, one `n` each.
    assert_eq!(dup, 3, "one \"n\" per metrics object (2 segments + total): {json}");

    // And the metric `n` still carries the count (449 in the UNDER band).
    let under = &report.segments[0];
    assert!((under.metrics[0].unwrap() - 449.0).abs() < TOL_EXACT, "metric n preserved");
}

#[test]
fn t_no_metric_named_n_keeps_builtin_column() {
    // When the user does NOT define a metric named `n`, the built-in
    // unit-count column/key is present (back-compat with the original shape).
    let (yaml, csv) = exp048_model();
    let (mut cube, refs, principal) = load(&yaml, &csv);
    let mut buckets = BTreeMap::new();
    buckets.insert("bet_side".to_string(), vec![0.0, 0.5, 1.0]);
    let cmd = GradeCommand {
        path: "exp048.yaml".into(),
        unit: "Game".into(),
        holdout: None,
        group_by: vec!["bet_side".into()],
        metrics: vec![metric("win_rate=mean(direction_correct)")],
        buckets,
        flag_if: None,
        min_n: 0,
        max_segments: 50,
        wilson_null: WilsonNullPolicy::Error,
        include_writes: false,
        format: GradeFormat::Json,
    };
    let report = grade_cube(&mut cube, &refs, principal, &cmd).expect("grade runs");
    let json = format_json(&cmd, &report);
    assert!(json.contains("\"n\":"), "built-in n present when no metric named n");
    let text = format_text(&cmd, &report);
    let header_line = text.lines().find(|l| l.contains("bet_side")).expect("header");
    assert!(
        header_line.split('|').any(|c| c.trim() == "n"),
        "built-in n column present: {header_line:?}"
    );
}

#[test]
fn t_bucket_label_trailing_zeros_trimmed() {
    // Phase 10B.1 cosmetic: bucket band labels drop trailing zeros.
    let edges = [0.0, 0.03, 0.10, 1.0];
    match assign_bucket(0.05, &edges) {
        BandAssignment::Band { label, .. } => {
            assert_eq!(label, "[0.03,0.1)", "trimmed label, got {label:?}");
        }
        other => panic!("expected band, got {other:?}"),
    }
    match assign_bucket(0.0, &edges) {
        BandAssignment::Band { label, .. } => {
            assert_eq!(label, "[0,0.03)", "integer edge bare, got {label:?}");
        }
        other => panic!("expected band, got {other:?}"),
    }
}

#[test]
fn t_determinism_identical_across_runs() {
    let (yaml, csv) = exp048_model();
    let path = temp_model(&yaml, &csv);
    let p = path.to_str().unwrap().to_string();

    let out1 = run_captured(exp048_cmd(&p, GradeFormat::Text));
    let out2 = run_captured(exp048_cmd(&p, GradeFormat::Text));
    assert_eq!(out1.0, 0, "exit code");
    assert_eq!(out1, out2, "two runs must be byte-identical");

    let j1 = run_captured(exp048_cmd(&p, GradeFormat::Json));
    let j2 = run_captured(exp048_cmd(&p, GradeFormat::Json));
    assert_eq!(j1, j2, "JSON runs must be byte-identical");
}
