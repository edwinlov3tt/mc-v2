// Tests for `mc model backtest` (Phase 10C.1, ADR-0036). Included into
// `backtest.rs` as `mod tests`, so `super::*` exposes the private engine.
//
// Per CLAUDE.md §4.5: every inline cube uses SINGLE braces `{ }`. The
// construct-then-assert integration tests build a real cube via the loader
// (write temp YAML + inputs CSV, then `load_model_with_policy`); a cube that
// fails to *load* surfaces as a panic, distinct from an assertion mismatch.

use super::*; // brings backtest's `use std::fmt::Write` (for writeln! below) into scope
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

const TOL: f64 = 1e-9;
const TOL_RMSE: f64 = 1e-6;

// ---------------------------------------------------------------------------
// Pure parser tests (no cube needed)
// ---------------------------------------------------------------------------

#[test]
fn t_parse_points_range() {
    let p = parse_points("0:1:0.25").expect("range");
    assert_eq!(p.len(), 5);
    assert!((p[0] - 0.0).abs() < TOL);
    assert!((p[4] - 1.0).abs() < TOL);
}

#[test]
fn t_parse_points_value_list() {
    let p = parse_points("[0.1,0.2,0.35]").expect("list");
    assert_eq!(p.len(), 3);
    assert!((p[2] - 0.35).abs() < TOL);
}

#[test]
fn t_parse_points_rejects_bad() {
    assert!(parse_points("0:1").is_err(), "two-part range");
    assert!(parse_points("[]").is_err(), "empty list");
    assert!(parse_points("0:1:0").is_err(), "zero step");
    assert!(parse_points("5:1:1").is_err(), "stop < start");
    assert!(parse_points("a:b:c").is_err(), "non-numeric");
}

#[test]
fn t_decode_cell_first_axis_slowest() {
    // Two axes: A=[0,1], B=[10,20]. Grid order (first slowest):
    // (0,10),(0,20),(1,10),(1,20).
    let axes = vec![
        Axis {
            spec: "param:a=0:1:1".into(),
            label: "param:a".into(),
            kind: AxisKind::Param { name: "a".into() },
            points: vec![0.0, 1.0],
        },
        Axis {
            spec: "param:b=10:20:10".into(),
            label: "param:b".into(),
            kind: AxisKind::Param { name: "b".into() },
            points: vec![10.0, 20.0],
        },
    ];
    let expect = [[0.0, 10.0], [0.0, 20.0], [1.0, 10.0], [1.0, 20.0]];
    for (idx, e) in expect.iter().enumerate() {
        let got = decode_cell(&axes, idx);
        assert!((got[0] - e[0]).abs() < TOL && (got[1] - e[1]).abs() < TOL, "cell {idx}");
    }
}

#[test]
fn t_parse_requires_sweep_and_metric() {
    let err = parse(&["m.yaml".into(), "--unit".into(), "Game".into()]).unwrap_err();
    assert!(err.contains("--sweep"), "got {err}");
}

#[test]
fn t_parse_best_by_segment_requires_group_by() {
    let args: Vec<String> = vec![
        "m.yaml".into(),
        "--unit".into(),
        "Game".into(),
        "--sweep".into(),
        "param:x=0:1:1".into(),
        "--metric".into(),
        "w=mean(y)".into(),
        "--objective".into(),
        "w".into(),
        "--best-by".into(),
        "segment".into(),
    ];
    let err = parse(&args).unwrap_err();
    assert!(err.contains("--group-by"), "got {err}");
}

// ---------------------------------------------------------------------------
// Cube fixtures + helpers
// ---------------------------------------------------------------------------

static TMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp_model(yaml: &str, inputs_csv: &str, unit: &str) -> std::path::PathBuf {
    let n = TMP_COUNTER.fetch_add(1, AtomicOrdering::SeqCst);
    let dir = std::env::temp_dir().join(format!("mc_backtest_{}_{}_{}", unit, std::process::id(), n));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    std::fs::write(dir.join("inputs.csv"), inputs_csv).expect("write inputs");
    let yaml_path = dir.join("model.yaml");
    std::fs::write(&yaml_path, yaml).expect("write yaml");
    yaml_path
}

fn load(yaml: &str, csv: &str, unit: &str) -> (mc_core::Cube, mc_model::ModelRefs, mc_core::PrincipalId) {
    let path = temp_model(yaml, csv, unit);
    let loaded = load_model_with_policy(path.to_str().unwrap(), LoadPolicy::Reproducible)
        .unwrap_or_else(|e| panic!("test cube failed to load: {}", e.message()));
    (loaded.cube, loaded.refs, loaded.root_principal)
}

fn metric(s: &str) -> MetricExpr {
    parse_metric_expr(s).expect("test metric parses")
}

/// A backtest command with sensible defaults for tests; override fields per case.
fn base_cmd(path: &str, unit: &str, sweeps: Vec<&str>, metrics: Vec<&str>) -> BacktestCommand {
    BacktestCommand {
        path: path.into(),
        unit: unit.into(),
        holdout: None,
        sweeps: sweeps.into_iter().map(String::from).collect(),
        metrics: metrics.into_iter().map(metric).collect(),
        group_by: Vec::new(),
        buckets: BTreeMap::new(),
        objective: None,
        goal: Goal::Maximize,
        best_by: BestBy::Total,
        min_n: 0,
        max_segments: 50,
        max_grid: 1000,
        wilson_null: WilsonNullPolicy::Error,
        include_writes: false,
        format: BacktestFormat::Text,
        emit_grid: None,
        dry_run: false,
    }
}

/// EXP-033-style betting fixture: 5 bets with edge/pnl/stake, a swept
/// `edge_threshold` param gating qualification. Hand-computed ground truth:
///   thr=0.00 → all 5 qualify; pnl sum 4, stake 5, roi 0.8
///   thr=0.05 → 4 qualify (edge 0.02 out); pnl 5, stake 4, roi 1.25
///   thr=0.10 → 2 qualify (edge 0.12,0.15); pnl 4, stake 2, roi 2.0
/// So roi maximize → threshold 0.10. (Edges avoid boundary float-equality.)
fn exp033_betting() -> (String, String) {
    let yaml = r#"model_format_version: 1
canonical_inputs:
  source: "inputs.csv"
  columns: ["Scenario", "Version", "Bet", "Measure", "value"]
metadata:
  name: "exp033_backtest"
  description: "edge-threshold sweep fixture"
  author: "test"
  created: "2026-05-31"
parameters:
  - { name: edge_threshold, value: 0.0, description: "min edge to bet" }
dimensions:
  - { name: Scenario, kind: Scenario, elements: [ { name: base } ] }
  - { name: Version, kind: Version, elements: [ { name: Working } ] }
  - { name: Bet, kind: Standard, elements: [] }
  - { name: Measure, kind: Measure, elements: [] }
measures:
  - { name: edge, role: Input, data_type: F64, aggregation: Sum }
  - { name: pnl, role: Input, data_type: F64, aggregation: Sum }
  - { name: stake, role: Input, data_type: F64, aggregation: Sum }
  - { name: qualified, role: Derived, data_type: F64, aggregation: Sum }
  - { name: q_pnl, role: Derived, data_type: F64, aggregation: Sum }
  - { name: q_stake, role: Derived, data_type: F64, aggregation: Sum }
rules:
  - { name: r_qualified, target_measure: qualified, scope: AllLeaves, body: 'if(edge >= param(edge_threshold), 1.0, 0.0)', declared_dependencies: ["edge"] }
  - { name: r_q_pnl, target_measure: q_pnl, scope: AllLeaves, body: 'qualified * pnl', declared_dependencies: ["qualified", "pnl"] }
  - { name: r_q_stake, target_measure: q_stake, scope: AllLeaves, body: 'qualified * stake', declared_dependencies: ["qualified", "stake"] }
"#;
    let bets = [
        (0.02f64, -1.0f64),
        (0.06, 2.0),
        (0.12, 1.0),
        (0.08, -1.0),
        (0.15, 3.0),
    ];
    let mut csv = String::from("Scenario,Version,Bet,Measure,value\n");
    for (i, (edge, pnl)) in bets.iter().enumerate() {
        let _ = writeln!(csv, "base,Working,bet_{i},edge,{edge}");
        let _ = writeln!(csv, "base,Working,bet_{i},pnl,{pnl}");
        let _ = writeln!(csv, "base,Working,bet_{i},stake,1");
    }
    (yaml.to_string(), csv)
}

/// Non-betting forecasting fixture (the multi-domain spine, AC #15/#22):
/// blend a model forecast with a baseline, sweep the blend, score with RMSE.
/// model perfectly predicts; baseline is 0. Hand-computed:
///   blend=0.0 → forecast=0 → SE 100,400 → mean 250 → rmse sqrt(250)≈15.8114
///   blend=0.5 → forecast=5,10 → SE 25,100 → mean 62.5 → rmse ≈7.9057
///   blend=1.0 → forecast=actual → SE 0 → rmse 0
/// So rmse minimize → blend 1.0. Zero betting vocabulary anywhere.
fn forecasting_multidomain() -> (String, String) {
    let yaml = r#"model_format_version: 1
canonical_inputs:
  source: "inputs.csv"
  columns: ["Scenario", "Version", "Period", "Measure", "value"]
metadata:
  name: "forecast_blend_backtest"
  description: "smoothing-blend RMSE sweep (non-betting multi-domain)"
  author: "test"
  created: "2026-05-31"
parameters:
  - { name: blend, value: 0.0, description: "model vs baseline blend weight" }
dimensions:
  - { name: Scenario, kind: Scenario, elements: [ { name: base } ] }
  - { name: Version, kind: Version, elements: [ { name: Working } ] }
  - { name: Period, kind: Standard, elements: [] }
  - { name: Measure, kind: Measure, elements: [] }
measures:
  - { name: actual, role: Input, data_type: F64, aggregation: Sum }
  - { name: model_forecast, role: Input, data_type: F64, aggregation: Sum }
  - { name: baseline_forecast, role: Input, data_type: F64, aggregation: Sum }
  - { name: forecast, role: Derived, data_type: F64, aggregation: Sum }
  - { name: squared_error, role: Derived, data_type: F64, aggregation: Sum }
rules:
  - { name: r_forecast, target_measure: forecast, scope: AllLeaves, body: 'param(blend) * model_forecast + (1.0 - param(blend)) * baseline_forecast', declared_dependencies: ["model_forecast", "baseline_forecast"] }
  - { name: r_se, target_measure: squared_error, scope: AllLeaves, body: '(forecast - actual) * (forecast - actual)', declared_dependencies: ["forecast", "actual"] }
"#;
    let periods = [(10.0f64, 10.0f64), (20.0, 20.0)]; // (actual, model_forecast); baseline 0
    let mut csv = String::from("Scenario,Version,Period,Measure,value\n");
    for (i, (actual, model)) in periods.iter().enumerate() {
        let _ = writeln!(csv, "base,Working,p_{i},actual,{actual}");
        let _ = writeln!(csv, "base,Working,p_{i},model_forecast,{model}");
        let _ = writeln!(csv, "base,Working,p_{i},baseline_forecast,0");
    }
    (yaml.to_string(), csv)
}

fn cell_with_value(result: &BacktestResult, axis0: f64) -> &GridCell {
    result
        .cells
        .iter()
        .find(|c| (c.values[0] - axis0).abs() < TOL)
        .unwrap_or_else(|| panic!("no grid cell with axis0={axis0}"))
}

fn metric_idx(result: &BacktestResult, name: &str) -> usize {
    result
        .metric_names
        .iter()
        .position(|m| m == name)
        .unwrap_or_else(|| panic!("no metric {name}"))
}

// ---------------------------------------------------------------------------
// The guardrail wired correctly: param sweep MOVES the derived metric
// ---------------------------------------------------------------------------

#[test]
fn t_param_recompute_via_command_moves_per_cell() {
    // The spike proved the mechanism in isolation; this proves the COMMAND
    // wires rollback_to → apply → eval per cell (AC #25). If the guardrail
    // regressed (override-without-rollback), every cell would serve cell 0's
    // cached derived values and roi would be flat.
    let (yaml, csv) = exp033_betting();
    let (mut cube, refs, principal) = load(&yaml, &csv, "Bet");
    let cmd = base_cmd(
        "exp033.yaml",
        "Bet",
        vec!["param:edge_threshold=0:0.1:0.05"],
        vec!["n=sum(qualified)", "roi=ratio(q_pnl,q_stake)"],
    );
    let result = run_grid(&cmd, &mut cube, &refs, principal).expect("grid runs");
    assert_eq!(result.cells.len(), 3, "thresholds 0, 0.05, 0.10");
    let roi = metric_idx(&result, "roi");
    let r0 = cell_with_value(&result, 0.0).total_metrics[roi].unwrap();
    let r1 = cell_with_value(&result, 0.05).total_metrics[roi].unwrap();
    let r2 = cell_with_value(&result, 0.10).total_metrics[roi].unwrap();
    // The three ROI values are DISTINCT — the derived measures recomputed.
    assert!((r0 - r1).abs() > 1e-6 && (r1 - r2).abs() > 1e-6, "roi must move per cell: {r0},{r1},{r2}");
}

// ---------------------------------------------------------------------------
// EXP-033 reproduction: edge-threshold sweep → optimal threshold + per-point
// ---------------------------------------------------------------------------

#[test]
fn t_exp033_reproduction_threshold_roi_surface() {
    let (yaml, csv) = exp033_betting();
    let (mut cube, refs, principal) = load(&yaml, &csv, "Bet");
    let mut cmd = base_cmd(
        "exp033.yaml",
        "Bet",
        vec!["param:edge_threshold=0:0.1:0.05"],
        vec!["n=sum(qualified)", "roi=ratio(q_pnl,q_stake)"],
    );
    cmd.objective = Some("roi".into());
    cmd.goal = Goal::Maximize;
    let result = run_grid(&cmd, &mut cube, &refs, principal).expect("grid runs");

    let roi = metric_idx(&result, "roi");
    let nq = metric_idx(&result, "n");
    // Hand-computed surface.
    let r0 = cell_with_value(&result, 0.0);
    assert!((r0.total_metrics[roi].unwrap() - 0.8).abs() < TOL_RMSE, "thr 0 roi");
    assert!((r0.total_metrics[nq].unwrap() - 5.0).abs() < TOL, "thr 0 n");
    let r1 = cell_with_value(&result, 0.05);
    assert!((r1.total_metrics[roi].unwrap() - 1.25).abs() < TOL_RMSE, "thr 0.05 roi");
    assert!((r1.total_metrics[nq].unwrap() - 4.0).abs() < TOL, "thr 0.05 n");
    let r2 = cell_with_value(&result, 0.10);
    assert!((r2.total_metrics[roi].unwrap() - 2.0).abs() < TOL_RMSE, "thr 0.10 roi");
    assert!((r2.total_metrics[nq].unwrap() - 2.0).abs() < TOL, "thr 0.10 n");

    // Optimal threshold = 0.10 (max roi).
    let best = result.best_total.expect("best selected");
    assert!((result.cells[best].values[0] - 0.10).abs() < TOL, "optimal threshold 0.10");
}

// ---------------------------------------------------------------------------
// MANDATORY multi-domain test (the spine, AC #15) — forecasting + RMSE
// ---------------------------------------------------------------------------

#[test]
fn t_multidomain_forecasting_rmse_sweep() {
    let (yaml, csv) = forecasting_multidomain();
    let (mut cube, refs, principal) = load(&yaml, &csv, "Period");
    let mut cmd = base_cmd(
        "forecast.yaml",
        "Period",
        vec!["param:blend=0:1:0.5"],
        vec!["rmse=rmse(squared_error)"],
    );
    cmd.objective = Some("rmse".into());
    cmd.goal = Goal::Minimize;
    let result = run_grid(&cmd, &mut cube, &refs, principal).expect("grid runs");

    assert_eq!(result.cells.len(), 3, "blends 0, 0.5, 1.0");
    let rmse = metric_idx(&result, "rmse");
    // RMSE correctness: rmse(SE) = sqrt(mean(SE)).
    let b0 = cell_with_value(&result, 0.0).total_metrics[rmse].unwrap();
    assert!((b0 - 250.0_f64.sqrt()).abs() < TOL_RMSE, "blend 0 rmse sqrt(250), got {b0}");
    let b5 = cell_with_value(&result, 0.5).total_metrics[rmse].unwrap();
    assert!((b5 - 62.5_f64.sqrt()).abs() < TOL_RMSE, "blend 0.5 rmse sqrt(62.5), got {b5}");
    let b1 = cell_with_value(&result, 1.0).total_metrics[rmse].unwrap();
    assert!(b1.abs() < TOL_RMSE, "blend 1.0 rmse 0, got {b1}");

    // Minimize → blend 1.0 wins.
    let best = result.best_total.expect("best selected");
    assert!((result.cells[best].values[0] - 1.0).abs() < TOL, "optimal blend 1.0");
}

// ---------------------------------------------------------------------------
// Multi-axis cartesian grid: size + fixed order
// ---------------------------------------------------------------------------

#[test]
fn t_multi_axis_cartesian_size_and_order() {
    let (yaml, csv) = exp033_betting();
    let (mut cube, refs, principal) = load(&yaml, &csv, "Bet");
    // param axis (3 points) × input axis (2 points) = 6 cells.
    let cmd = base_cmd(
        "exp033.yaml",
        "Bet",
        vec![
            "param:edge_threshold=0:0.1:0.05",
            "input:stake@Scenario=base,Version=Working,Bet=bet_0=[1,2]",
        ],
        vec!["roi=ratio(q_pnl,q_stake)"],
    );
    let result = run_grid(&cmd, &mut cube, &refs, principal).expect("grid runs");
    assert_eq!(result.cells.len(), 6, "3 × 2 cartesian");
    // First axis slowest: thresholds repeat in blocks of 2.
    let a0: Vec<f64> = result.cells.iter().map(|c| c.values[0]).collect();
    assert!((a0[0] - 0.0).abs() < TOL && (a0[1] - 0.0).abs() < TOL, "first two cells share threshold 0");
    assert!((a0[2] - 0.05).abs() < TOL && (a0[3] - 0.05).abs() < TOL, "next two share 0.05");
}

// ---------------------------------------------------------------------------
// --max-grid hard-errors
// ---------------------------------------------------------------------------

#[test]
fn t_max_grid_hard_errors() {
    let (yaml, csv) = exp033_betting();
    let (mut cube, refs, principal) = load(&yaml, &csv, "Bet");
    let mut cmd = base_cmd(
        "exp033.yaml",
        "Bet",
        vec!["param:edge_threshold=0:1:0.1"], // 11 points
        vec!["roi=ratio(q_pnl,q_stake)"],
    );
    cmd.max_grid = 5;
    let err = run_grid(&cmd, &mut cube, &refs, principal).unwrap_err();
    assert!(err.contains("max-grid"), "got {err}");
}

// ---------------------------------------------------------------------------
// Objective edge cases (Amendment 7)
// ---------------------------------------------------------------------------

#[test]
fn t_objective_minimize_picks_low() {
    // roi minimize → threshold 0 (roi 0.8 is the lowest of 0.8/1.25/2.0).
    let (yaml, csv) = exp033_betting();
    let (mut cube, refs, principal) = load(&yaml, &csv, "Bet");
    let mut cmd = base_cmd(
        "exp033.yaml",
        "Bet",
        vec!["param:edge_threshold=0:0.1:0.05"],
        vec!["roi=ratio(q_pnl,q_stake)"],
    );
    cmd.objective = Some("roi".into());
    cmd.goal = Goal::Minimize;
    let result = run_grid(&cmd, &mut cube, &refs, principal).expect("grid runs");
    let best = result.best_total.expect("best");
    assert!((result.cells[best].values[0] - 0.0).abs() < TOL, "minimize picks threshold 0");
}

#[test]
fn t_objective_all_null_hard_errors() {
    // A param threshold so high NOTHING qualifies → q_stake sum 0 → roi Null
    // in every cell → all-Null objective hard-errors (Amendment 7).
    let (yaml, csv) = exp033_betting();
    let (mut cube, refs, principal) = load(&yaml, &csv, "Bet");
    let mut cmd = base_cmd(
        "exp033.yaml",
        "Bet",
        vec!["param:edge_threshold=1:2:0.5"], // all edges < 1, nothing qualifies
        vec!["roi=ratio(q_pnl,q_stake)"],
    );
    cmd.objective = Some("roi".into());
    let err = run_grid(&cmd, &mut cube, &refs, principal).unwrap_err();
    assert!(err.contains("Null in every"), "got {err}");
}

#[test]
fn t_objective_tie_breaks_to_first() {
    // Two blends give the same RMSE only if data is symmetric; instead make a
    // flat objective via a constant metric (sum of a constant-ish measure).
    // Simpler: sweep a param the metric does NOT depend on → identical metric
    // across cells → tie → first cell wins.
    let (yaml, csv) = forecasting_multidomain();
    let (mut cube, refs, principal) = load(&yaml, &csv, "Period");
    let mut cmd = base_cmd(
        "forecast.yaml",
        "Period",
        // sweep blend but objective on a metric independent of blend:
        // n=count(actual) is constant (2) across all cells.
        vec!["param:blend=0:1:0.5"],
        vec!["n=count(actual)"],
    );
    cmd.objective = Some("n".into());
    cmd.goal = Goal::Maximize;
    let result = run_grid(&cmd, &mut cube, &refs, principal).expect("grid runs");
    let best = result.best_total.expect("best");
    // All cells tie at n=2; first cell (blend 0) wins.
    assert_eq!(best, 0, "ties resolve to the first grid cell");
}

// ---------------------------------------------------------------------------
// --best-by segment (Amendment 6)
// ---------------------------------------------------------------------------

#[test]
fn t_best_by_segment_per_group() {
    // Group bets into two edge buckets; find the best threshold per bucket.
    let (yaml, csv) = exp033_betting();
    let (mut cube, refs, principal) = load(&yaml, &csv, "Bet");
    let mut cmd = base_cmd(
        "exp033.yaml",
        "Bet",
        vec!["param:edge_threshold=0:0.1:0.05"],
        vec!["roi=ratio(q_pnl,q_stake)"],
    );
    cmd.group_by = vec!["edge".into()];
    cmd.buckets.insert("edge".into(), vec![0.0, 0.1, 0.2]);
    cmd.objective = Some("roi".into());
    cmd.goal = Goal::Maximize;
    cmd.best_by = BestBy::Segment;
    let result = run_grid(&cmd, &mut cube, &refs, principal).expect("grid runs");
    // Two edge buckets → at most two best-per-segment rows.
    assert!(!result.best_by_segment.is_empty(), "best-by-segment populated");
    assert!(result.best_by_segment.len() <= 2, "two edge buckets");
}

// ---------------------------------------------------------------------------
// --dry-run prints without evaluating
// ---------------------------------------------------------------------------

#[test]
fn t_dry_run_prints_grid_without_eval() {
    let (yaml, csv) = exp033_betting();
    let path = temp_model(&yaml, &csv, "Bet");
    let mut cmd = base_cmd(
        path.to_str().unwrap(),
        "Bet",
        vec!["param:edge_threshold=0:0.1:0.05"],
        vec!["roi=ratio(q_pnl,q_stake)"],
    );
    cmd.dry_run = true;
    let (code, out) = run_captured(cmd);
    assert_eq!(code, 0);
    assert!(out.contains("DRY RUN"), "dry-run banner");
    assert!(out.contains("grid cells: 3"), "grid count, got: {out}");
}

// ---------------------------------------------------------------------------
// Determinism: 10 identical runs
// ---------------------------------------------------------------------------

#[test]
fn t_determinism_ten_runs() {
    let (yaml, csv) = forecasting_multidomain();
    let path = temp_model(&yaml, &csv, "Period");
    let make = || {
        let mut c = base_cmd(
            path.to_str().unwrap(),
            "Period",
            vec!["param:blend=0:1:0.25"],
            vec!["rmse=rmse(squared_error)"],
        );
        c.objective = Some("rmse".into());
        c.goal = Goal::Minimize;
        c.format = BacktestFormat::Json;
        c
    };
    let (_, first) = run_captured(make());
    for i in 0..10 {
        let (_, out) = run_captured(make());
        assert_eq!(out, first, "run {i} differs from first");
    }
}

// ---------------------------------------------------------------------------
// coef axis (absolute) sweeps a fitted-model coefficient
// ---------------------------------------------------------------------------

#[test]
fn t_coef_axis_absolute() {
    // Minimal fitted model with one coefficient; predict() is linear so the
    // derived prediction moves with the swept coefficient.
    let yaml = r#"model_format_version: 1
canonical_inputs:
  source: "inputs.csv"
  columns: ["Scenario", "Version", "Row", "Measure", "value"]
metadata:
  name: "coef_backtest"
  description: "coefficient sweep fixture"
  author: "test"
  created: "2026-05-31"
fitted_models:
  - name: linmod
    method: linear
    intercept: 0.0
    coefficients: [ { feature: x, weight: 1.0 } ]
dimensions:
  - { name: Scenario, kind: Scenario, elements: [ { name: base } ] }
  - { name: Version, kind: Version, elements: [ { name: Working } ] }
  - { name: Row, kind: Standard, elements: [] }
  - { name: Measure, kind: Measure, elements: [] }
measures:
  - { name: x, role: Input, data_type: F64, aggregation: Sum }
  - { name: yhat, role: Derived, data_type: F64, aggregation: Sum }
rules:
  - { name: r_yhat, target_measure: yhat, scope: AllLeaves, body: 'predict("linmod", x)', declared_dependencies: ["x"] }
"#;
    let mut csv = String::from("Scenario,Version,Row,Measure,value\n");
    for i in 0..3 {
        let _ = writeln!(csv, "base,Working,r_{i},x,2");
    }
    let (mut cube, refs, principal) = load(yaml, &csv, "Row");
    let cmd = base_cmd(
        "coef.yaml",
        "Row",
        vec!["coef:linmod.x=1:3:1"], // weight 1, 2, 3
        vec!["mean_yhat=mean(yhat)"],
    );
    let result = run_grid(&cmd, &mut cube, &refs, principal).expect("grid runs");
    assert_eq!(result.cells.len(), 3);
    let mi = metric_idx(&result, "mean_yhat");
    // x=2, weight w → yhat=2w → mean 2w. weights 1,2,3 → 2,4,6.
    let v1 = cell_with_value(&result, 1.0).total_metrics[mi].unwrap();
    let v3 = cell_with_value(&result, 3.0).total_metrics[mi].unwrap();
    assert!((v1 - 2.0).abs() < TOL_RMSE, "weight 1 → mean yhat 2, got {v1}");
    assert!((v3 - 6.0).abs() < TOL_RMSE, "weight 3 → mean yhat 6, got {v3}");
}
