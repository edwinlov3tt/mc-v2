// Tests for `mc model simulate` (Phase 10F, ADR-0035). Included into
// `simulate.rs` under `mod tests`. Inline JSON fixtures use SINGLE braces
// (CLAUDE.md §4.5 — these are real JSON objects, not templates).

use super::*;
use serde_json::Value;

/// Write a jsonl fixture to a uniquely-named temp file; return its path.
fn write_jsonl(key: &str, lines: &[&str]) -> String {
    let path = std::env::temp_dir().join(format!("mc_sim_{key}.jsonl"));
    let body = lines.join("\n");
    std::fs::write(&path, body).expect("write fixture");
    path.to_string_lossy().to_string()
}

/// Run `simulate` with the given args, returning parsed JSON (asserts exit 0).
fn sim_ok(args: &[&str]) -> Value {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let cmd = parse(&owned).expect("parse");
    let (code, out) = run_captured(cmd);
    assert_eq!(code, 0, "expected success, got error:\n{out}");
    serde_json::from_str(&out).expect("json output")
}

/// Run `simulate` expecting a non-zero exit; return the error text.
fn sim_err(args: &[&str]) -> String {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    match parse(&owned) {
        Err(e) => e,
        Ok(cmd) => {
            let (code, out) = run_captured(cmd);
            assert_ne!(code, 0, "expected error, got success:\n{out}");
            out
        }
    }
}

fn metric_f64(v: &Value, name: &str) -> f64 {
    v["metrics"][name].as_f64().unwrap_or(f64::NAN)
}

// --------------------------------------------------------------------------
// Kelly + sizing
// --------------------------------------------------------------------------

#[test]
fn t_kelly_fraction_known_value() {
    // p=0.55, decimal_odds=2.0 → b=1, f=(1*0.55-0.45)/1 = 0.10.
    assert!((kelly_fraction(0.55, 2.0) - 0.10).abs() < 1e-9);
}

#[test]
fn t_kelly_negative_edge_is_zero() {
    assert!(kelly_fraction(0.40, 2.0).abs() < 1e-12);
}

#[test]
fn t_sizing_parse_quarter_kelly_with_modifiers() {
    let r = parse_sizing("quarter_kelly:cap=0.025,shrink=0.02").expect("parse");
    assert_eq!(r.kind, SizingKind::Kelly);
    assert!((r.param - 0.25).abs() < 1e-12);
    assert!((r.cap.unwrap() - 0.025).abs() < 1e-12);
    assert!((r.shrink - 0.02).abs() < 1e-12);
}

#[test]
fn t_sizing_unknown_rule_errors() {
    assert!(parse_sizing("martingale:pct=0.1").is_err());
}

#[test]
fn t_sizing_from_column_requires_column() {
    assert!(parse_sizing("from_column").is_err());
    let r = parse_sizing("from_column:stake_hint").expect("parse");
    assert_eq!(r.kind, SizingKind::FromColumn);
    assert_eq!(r.column.as_deref(), Some("stake_hint"));
}

// --------------------------------------------------------------------------
// Reader + aliasing + validation
// --------------------------------------------------------------------------

#[test]
fn t_reader_jsonl_canonical_4state() {
    let path = write_jsonl(
        "canon",
        &[
            r#"{ "bet_id": "a", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win" }"#,
            r#"{ "bet_id": "b", "timestamp": "2025-01-02T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "loss" }"#,
        ],
    );
    let v = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat:pct=0.1",
        "--format",
        "json",
    ]);
    assert_eq!(v["outcome_mode"], "canonical");
    assert_eq!(v["metrics"]["n_bets"], 2);
}

#[test]
fn t_reader_alias_resolution() {
    // game_pk→bet_id, commence_time→timestamp; won is binary (needs flag).
    let path = write_jsonl(
        "alias",
        &[
            r#"{ "game_pk": 1, "commence_time": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "won": 1 }"#,
        ],
    );
    let v = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat:pct=0.1",
        "--outcome-mode",
        "legacy-binary",
        "--format",
        "json",
    ]);
    assert_eq!(v["schema_mapping"]["bet_id"], "game_pk");
    assert_eq!(v["schema_mapping"]["timestamp"], "commence_time");
    assert_eq!(v["schema_mapping"]["outcome"], "won");
}

#[test]
fn t_reader_missing_required_column_errors() {
    let path = write_jsonl(
        "missingcol",
        &[
            r#"{ "bet_id": "a", "timestamp": "2025-01-01T00:00:00Z", "decimal_odds": 2.0, "outcome": "win" }"#,
        ],
    );
    let e = sim_err(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat:pct=0.1",
    ]);
    assert!(e.contains("p_bet_side"), "got: {e}");
}

#[test]
fn t_reader_bad_odds_errors() {
    let path = write_jsonl(
        "badodds",
        &[
            r#"{ "bet_id": "a", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 0.9, "outcome": "win" }"#,
        ],
    );
    let e = sim_err(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat:pct=0.1",
    ]);
    assert!(e.contains("decimal_odds"), "got: {e}");
}

// --------------------------------------------------------------------------
// Outcome normalization (A2)
// --------------------------------------------------------------------------

#[test]
fn t_outcome_binary_hard_errors_by_default() {
    let path = write_jsonl(
        "binhard",
        &[
            r#"{ "bet_id": "a", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "won": 1 }"#,
        ],
    );
    let e = sim_err(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat:pct=0.1",
    ]);
    assert!(e.contains("legacy-binary"), "got: {e}");
}

#[test]
fn t_outcome_derive_pushes_reconstructs_push() {
    // actual_total == line → push; the binary won says 1 (would be a win).
    let path = write_jsonl(
        "derive",
        &[
            r#"{ "bet_id": "a", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "won": 1, "actual_total": 8.0, "line": 8.0 }"#,
            r#"{ "bet_id": "b", "timestamp": "2025-01-02T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "won": 1, "actual_total": 9.0, "line": 8.0 }"#,
        ],
    );
    let v = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat:pct=0.1",
        "--derive-pushes",
        "actual_total=line",
        "--format",
        "json",
    ]);
    assert_eq!(v["outcome_mode"], "derived-pushes");
    assert_eq!(v["outcome_counts"]["push"], 1);
    assert_eq!(v["outcome_counts"]["win"], 1);
}

// --------------------------------------------------------------------------
// Single-path replay (hand-computed)
// --------------------------------------------------------------------------

#[test]
fn t_single_path_hand_computed() {
    // flat_current:pct=0.10, start 1000, odds 2.0, distinct timestamps.
    // win→+10%, loss→-stake, push→unchanged.
    let path = write_jsonl(
        "single",
        &[
            r#"{ "bet_id": "1", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win" }"#,
            r#"{ "bet_id": "2", "timestamp": "2025-01-02T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win" }"#,
            r#"{ "bet_id": "3", "timestamp": "2025-01-03T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "loss" }"#,
            r#"{ "bet_id": "4", "timestamp": "2025-01-04T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "push" }"#,
            r#"{ "bet_id": "5", "timestamp": "2025-01-05T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "loss" }"#,
        ],
    );
    let v = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat_current:pct=0.10",
        "--format",
        "json",
    ]);
    // 1000→1100→1210→1089→1089→980.1
    assert!((metric_f64(&v, "final_bank") - 980.1).abs() < 1e-6, "{v}");
    assert_eq!(v["metrics"]["n_bets"], 5);
    assert_eq!(v["metrics"]["wins"], 2);
    assert_eq!(v["metrics"]["pushes"], 1);
    // max drawdown = (1210-980.1)/1210
    assert!((metric_f64(&v, "max_drawdown") - (1210.0 - 980.1) / 1210.0).abs() < 1e-6);
    assert_eq!(v["metrics"]["recovery_status"], "unrecovered");
}

#[test]
fn t_push_leaves_bankroll_unchanged() {
    let path = write_jsonl(
        "push",
        &[
            r#"{ "bet_id": "1", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "push" }"#,
        ],
    );
    let v = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat:pct=0.5",
        "--format",
        "json",
    ]);
    assert!((metric_f64(&v, "final_bank") - 1000.0).abs() < 1e-9);
    assert_eq!(v["metrics"]["pushes"], 1);
}

// --------------------------------------------------------------------------
// Batch vs sequential (A1, A17)
// --------------------------------------------------------------------------

#[test]
fn t_batch_vs_sequential_differ_on_same_timestamp() {
    // Two bets, SAME timestamp, flat_current:pct=0.5, both win, odds 2.0.
    // sequential: 1000→1500→2250. batch: both off 1000 → 1500→2000.
    let path = write_jsonl(
        "bvs",
        &[
            r#"{ "bet_id": "1", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win" }"#,
            r#"{ "bet_id": "2", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win" }"#,
        ],
    );
    let seq = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat_current:pct=0.5",
        "--replay",
        "sequential",
        "--format",
        "json",
    ]);
    let batch = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat_current:pct=0.5",
        "--replay",
        "batch",
        "--format",
        "json",
    ]);
    assert!((metric_f64(&seq, "final_bank") - 2250.0).abs() < 1e-6);
    assert!((metric_f64(&batch, "final_bank") - 2000.0).abs() < 1e-6);
}

#[test]
fn t_batch_overstake_scales_proportionally() {
    // Same timestamp, two bets each flat_current:pct=0.8 → total 1.6×bank.
    // Scaled to 1.0×: each 500. one win one loss, odds 2.0 → net 0.
    let path = write_jsonl(
        "overstake",
        &[
            r#"{ "bet_id": "1", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win" }"#,
            r#"{ "bet_id": "2", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "loss" }"#,
        ],
    );
    let curve = std::env::temp_dir().join("mc_sim_overstake_curve.jsonl");
    let v = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat_current:pct=0.8",
        "--replay",
        "batch",
        "--emit-curve",
        curve.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert!((metric_f64(&v, "final_bank") - 1000.0).abs() < 1e-6);
    // Curve stakes scaled to 500 each (not 800).
    let body = std::fs::read_to_string(&curve).expect("curve");
    let first: Value = serde_json::from_str(body.lines().next().unwrap()).unwrap();
    assert!(
        (first["stake"].as_f64().unwrap() - 500.0).abs() < 1e-6,
        "{first}"
    );
}

// --------------------------------------------------------------------------
// Ruin (A3)
// --------------------------------------------------------------------------

#[test]
fn t_ruin_stops_replay() {
    // start 100, flat_current:pct=1.0, first bet loss → bankrupt, rest skipped.
    let path = write_jsonl(
        "ruin",
        &[
            r#"{ "bet_id": "1", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "loss" }"#,
            r#"{ "bet_id": "2", "timestamp": "2025-01-02T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win" }"#,
            r#"{ "bet_id": "3", "timestamp": "2025-01-03T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win" }"#,
        ],
    );
    let v = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "100",
        "--sizing",
        "flat_current:pct=1.0",
        "--format",
        "json",
    ]);
    assert_eq!(v["ruin"], true);
    assert!((metric_f64(&v, "final_bank")).abs() < 1e-9);
    assert_eq!(v["skip_counts"]["ruin_skipped"], 2);
    assert_eq!(v["metrics"]["n_bets"], 1);
}

// --------------------------------------------------------------------------
// Drawdown + recovery (Decision 7, A7)
// --------------------------------------------------------------------------

#[test]
fn t_drawdown_and_recovery_known_curve() {
    let (dd, rec, status) = drawdown_scan(1000.0, &[1200.0, 900.0, 1100.0, 1300.0]);
    // peak 1200, trough 900 → dd = 300/1200 = 0.25. recovers to 1200 at idx3
    // (offset 2 from the trough at idx1).
    assert!((dd - 0.25).abs() < 1e-9);
    assert_eq!(rec, Some(2));
    assert_eq!(status, RecoveryStatus::Recovered);
}

#[test]
fn t_drawdown_never_underwater() {
    let (dd, rec, status) = drawdown_scan(1000.0, &[1100.0, 1200.0, 1300.0]);
    assert!(dd.abs() < 1e-12);
    assert_eq!(rec, None);
    assert_eq!(status, RecoveryStatus::NeverUnderwater);
}

#[test]
fn t_sharpe_null_on_single_bet() {
    let path = write_jsonl(
        "sharpe1",
        &[
            r#"{ "bet_id": "1", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win" }"#,
        ],
    );
    let v = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat:pct=0.1",
        "--format",
        "json",
    ]);
    assert!(v["metrics"]["sharpe"].is_null());
}

// --------------------------------------------------------------------------
// roi cumulative vs roi_per_bet (A13)
// --------------------------------------------------------------------------

#[test]
fn t_roi_cumulative_vs_per_bet() {
    let path = write_jsonl(
        "roi",
        &[
            r#"{ "bet_id": "1", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win" }"#,
            r#"{ "bet_id": "2", "timestamp": "2025-01-02T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win" }"#,
        ],
    );
    // flat_current 0.1: 1000→1100→1210. cumulative roi=0.21.
    // total_staked=100+110=210, pnl=210 → roi_per_bet=1.0.
    let v = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat_current:pct=0.1",
        "--format",
        "json",
    ]);
    assert!((metric_f64(&v, "roi") - 0.21).abs() < 1e-6);
    assert!((metric_f64(&v, "roi_per_bet") - 1.0).abs() < 1e-6);
}

// --------------------------------------------------------------------------
// Filter first, window second (A12)
// --------------------------------------------------------------------------

#[test]
fn t_filter_first_window_second() {
    let path = write_jsonl(
        "fw",
        &[
            r#"{ "bet_id": "1", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win", "edge": 0.05 }"#,
            r#"{ "bet_id": "2", "timestamp": "2025-01-02T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win", "edge": 0.20 }"#,
            r#"{ "bet_id": "3", "timestamp": "2025-01-03T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win", "edge": 0.20 }"#,
            r#"{ "bet_id": "4", "timestamp": "2025-01-04T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win", "edge": 0.20 }"#,
        ],
    );
    // edge>=0.10 keeps bets 2,3,4; first:2 → bets 2,3 → n_bets=2.
    let v = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat:pct=0.1",
        "--filter",
        "edge >= 0.10",
        "--window",
        "first:2",
        "--format",
        "json",
    ]);
    assert_eq!(v["metrics"]["n_bets"], 2);
}

// --------------------------------------------------------------------------
// --odds applies to sizing AND settlement (A9)
// --------------------------------------------------------------------------

#[test]
fn t_odds_fixed_applies_to_settlement() {
    let path = write_jsonl(
        "odds",
        &[
            r#"{ "bet_id": "1", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win" }"#,
        ],
    );
    // odds fixed:3.0, flat:pct=0.1 → stake 100, win pays 100*(3-1)=200 → 1200.
    let v = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat:pct=0.1",
        "--odds",
        "fixed:3.0",
        "--format",
        "json",
    ]);
    assert!((metric_f64(&v, "final_bank") - 1200.0).abs() < 1e-6);
}

// --------------------------------------------------------------------------
// from_column sizing (A8)
// --------------------------------------------------------------------------

#[test]
fn t_from_column_uses_stake_hint() {
    let path = write_jsonl(
        "fromcol",
        &[
            r#"{ "bet_id": "1", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win", "stake_hint": 250.0 }"#,
        ],
    );
    let v = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "from_column:stake_hint",
        "--format",
        "json",
    ]);
    // stake 250, win odds 2.0 → +250 → 1250. total_staked=250.
    assert!((metric_f64(&v, "final_bank") - 1250.0).abs() < 1e-6);
    assert!((metric_f64(&v, "total_staked") - 250.0).abs() < 1e-6);
}

#[test]
fn t_bare_stake_hint_column_ignored_without_rule() {
    // stake_hint present but sizing=flat → hint ignored, flat used.
    let path = write_jsonl(
        "ignorehint",
        &[
            r#"{ "bet_id": "1", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win", "stake_hint": 999.0 }"#,
        ],
    );
    let v = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat:pct=0.1",
        "--format",
        "json",
    ]);
    // flat 0.1 → stake 100 (not 999). win → 1100.
    assert!((metric_f64(&v, "final_bank") - 1100.0).abs() < 1e-6);
}

// --------------------------------------------------------------------------
// Monte Carlo (A5, A6)
// --------------------------------------------------------------------------

#[test]
fn t_monte_carlo_seed_determinism() {
    let path = write_jsonl(
        "mc",
        &[
            r#"{ "bet_id": "1", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win" }"#,
            r#"{ "bet_id": "2", "timestamp": "2025-01-02T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "loss" }"#,
            r#"{ "bet_id": "3", "timestamp": "2025-01-03T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win" }"#,
        ],
    );
    let a = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat_current:pct=0.1",
        "--monte-carlo",
        "200",
        "--seed",
        "7",
        "--format",
        "json",
    ]);
    let b = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat_current:pct=0.1",
        "--monte-carlo",
        "200",
        "--seed",
        "7",
        "--format",
        "json",
    ]);
    assert_eq!(a["monte_carlo"], b["monte_carlo"]);
}

#[test]
fn t_monte_carlo_requires_seed() {
    let path = write_jsonl(
        "mcnoseed",
        &[
            r#"{ "bet_id": "1", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win" }"#,
        ],
    );
    let e = sim_err(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat:pct=0.1",
        "--monte-carlo",
        "100",
    ]);
    assert!(e.contains("seed"), "got: {e}");
}

#[test]
fn t_splitmix64_known_sequence() {
    // splitmix64(seed=0) first outputs are stable across platforms.
    let mut rng = SplitMix64::new(0);
    assert_eq!(rng.next_u64(), 16294208416658607535);
    assert_eq!(rng.next_u64(), 7960286522194355700);
}

#[test]
fn t_nearest_rank_percentile() {
    let sorted = [10.0, 20.0, 30.0, 40.0, 50.0];
    assert!((nearest_rank(&sorted, 50.0) - 30.0).abs() < 1e-9);
    assert!((nearest_rank(&sorted, 100.0) - 50.0).abs() < 1e-9);
    assert!((nearest_rank(&sorted, 5.0) - 10.0).abs() < 1e-9);
}

// --------------------------------------------------------------------------
// Curve invariants (A14)
// --------------------------------------------------------------------------

#[test]
fn t_curve_excludes_voids_includes_pushes() {
    let path = write_jsonl(
        "curveinv",
        &[
            r#"{ "bet_id": "1", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win" }"#,
            r#"{ "bet_id": "2", "timestamp": "2025-01-02T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "push" }"#,
            r#"{ "bet_id": "3", "timestamp": "2025-01-03T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "void" }"#,
        ],
    );
    let curve = std::env::temp_dir().join("mc_sim_curveinv_curve.jsonl");
    sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat:pct=0.1",
        "--emit-curve",
        curve.to_str().unwrap(),
        "--format",
        "json",
    ]);
    let body = std::fs::read_to_string(&curve).expect("curve");
    let rows: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();
    // win + push placed (2 rows); void excluded.
    assert_eq!(rows.len(), 2, "curve: {body}");
    assert!(body.contains("\"push\""));
    assert!(!body.contains("\"void\""));
}

// --------------------------------------------------------------------------
// EXP-049 reproduction (AC #12) — runs against claw-core's real file when
// present. Skips cleanly when the artifact is absent (keeps the workspace
// suite green on machines without claw-core checked out).
// --------------------------------------------------------------------------

#[test]
fn t_exp049_reproduction_legacy_and_push_accurate() {
    let real =
        "/Users/edwinlovettiii/Projects/claw-core/training/mlb/artifacts/exp028_bets.parquet";
    if !std::path::Path::new(real).exists() {
        eprintln!("skipping EXP-049 repro: {real} not present");
        return;
    }
    let filter = "abs_edge_pp >= 0.10 AND season == 2025";

    // (a) LEGACY published number — reproduces claw-core's HISTORICAL headline,
    // which is now known to OVERSTATE by ~38% due to push mis-scoring (integer-
    // line pushes counted as wins). Pinned for audit fidelity, NOT correctness.
    // Requires --no-derive-pushes to suppress the new auto-derive default.
    let legacy = sim_ok(&[
        "--bets",
        real,
        "--start-bankroll",
        "1000",
        "--sizing",
        "quarter_kelly:cap=0.025,shrink=0.02",
        "--filter",
        filter,
        "--replay",
        "sequential",
        "--outcome-mode",
        "legacy-binary",
        "--no-derive-pushes",
        "--format",
        "json",
    ]);
    let legacy_final = metric_f64(&legacy, "final_bank");
    assert!(
        (legacy_final - 2962.1596994721717).abs() / 2962.1596994721717 < 1e-3,
        "legacy EXP-049 final_bank {legacy_final} not within 0.1% of 2962.16"
    );
    assert_eq!(legacy["metrics"]["n_bets"], 376);
    assert_eq!(legacy["metrics"]["wins"], 222); // includes 24 phantom push-wins
    assert!((metric_f64(&legacy, "max_drawdown") - 0.2905842740982206).abs() < 1e-4);

    // (b) PUSH-ACCURATE number — the CORRECT result. Auto-derive is now the
    // default (actual_total + line columns present), so no outcome flag is
    // needed; pushes are reclassified out of the win column. ~$1,829 (−38%).
    let correct = sim_ok(&[
        "--bets",
        real,
        "--start-bankroll",
        "1000",
        "--sizing",
        "quarter_kelly:cap=0.025,shrink=0.02",
        "--filter",
        filter,
        "--format",
        "json",
    ]);
    assert_eq!(correct["outcome_mode"], "derived-pushes");
    let correct_final = metric_f64(&correct, "final_bank");
    assert!(
        (correct_final - 1829.3675124850377).abs() / 1829.3675124850377 < 1e-3,
        "push-accurate EXP-049 final_bank {correct_final} not within 0.1% of 1829.37"
    );
    // Pushes detected and excluded from the win count.
    assert!(correct["metrics"]["pushes"].as_u64().unwrap() > 0);
    assert!(
        correct["metrics"]["wins"].as_u64().unwrap() < 222,
        "push-accurate wins should drop below the legacy 222"
    );
    // The correction is ~38% — vastly larger than any batch/sequential delta.
    assert!(correct_final < legacy_final * 0.7);
}

// --------------------------------------------------------------------------
// Phase 10F.1 — push-accuracy default + EXP-029 gaps (Amendments 18-19)
// --------------------------------------------------------------------------

/// A binary `won` fixture with score/line columns. Bet 1: actual==line (a
/// push the binary `won=1` mis-scores as a win). Bet 2: a genuine win.
fn push_fixture(key: &str) -> String {
    write_jsonl(
        key,
        &[
            r#"{ "bet_id": "1", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "won": 1, "actual_total": 8.0, "line": 8.0 }"#,
            r#"{ "bet_id": "2", "timestamp": "2025-01-02T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "won": 1, "actual_total": 9.0, "line": 8.0 }"#,
        ],
    )
}

#[test]
fn t_auto_derive_pushes_is_default() {
    // AC #35: binary won + actual_total/line present → push-accurate by default,
    // NO outcome flag passed. Bet 1 (actual==line) becomes a push.
    let path = push_fixture("autoderive");
    let v = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat:pct=0.1",
        "--format",
        "json",
    ]);
    assert_eq!(v["outcome_mode"], "derived-pushes");
    assert_eq!(v["metrics"]["pushes"], 1);
    assert_eq!(v["metrics"]["wins"], 1);
    // push neutral; flat:pct=0.1 → win pays 100 → 1100 (push leaves 1000→same).
    assert!((metric_f64(&v, "final_bank") - 1100.0).abs() < 1e-6);
}

#[test]
fn t_no_derive_pushes_opt_out() {
    // AC #35: --no-derive-pushes forces prior behavior (legacy-binary scores
    // the push as a win → no pushes, higher bankroll).
    let path = push_fixture("optout");
    let v = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat:pct=0.1",
        "--outcome-mode",
        "legacy-binary",
        "--no-derive-pushes",
        "--format",
        "json",
    ]);
    assert_eq!(v["outcome_mode"], "legacy-binary");
    assert_eq!(v["metrics"]["pushes"], 0);
    assert_eq!(v["metrics"]["wins"], 2); // the push is scored as a win
                                         // both "wins" pay 100 → 1200.
    assert!((metric_f64(&v, "final_bank") - 1200.0).abs() < 1e-6);
}

#[test]
fn t_win_rate_excludes_pushes() {
    // AC #36: win_rate = wins/(wins+losses), pushes out of num AND denom.
    // 1 push + 1 win + 1 loss → win_rate = 1/2 = 0.5 (NOT 1/3 or 2/3).
    let path = write_jsonl(
        "winrate",
        &[
            r#"{ "bet_id": "1", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win" }"#,
            r#"{ "bet_id": "2", "timestamp": "2025-01-02T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "push" }"#,
            r#"{ "bet_id": "3", "timestamp": "2025-01-03T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "loss" }"#,
        ],
    );
    let v = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat:pct=0.1",
        "--format",
        "json",
    ]);
    assert!((metric_f64(&v, "win_rate") - 0.5).abs() < 1e-9);
    assert_eq!(v["metrics"]["n_bets"], 3); // pushes counted as placed
}

#[test]
fn t_legacy_binary_escalated_warning() {
    // AC #37: legacy-binary WITHOUT derivable score columns → escalated warning
    // (INACCURATE, names the compounding risk).
    let path = write_jsonl(
        "escalate",
        &[
            r#"{ "bet_id": "1", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "won": 1 }"#,
        ],
    );
    let v = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat:pct=0.1",
        "--outcome-mode",
        "legacy-binary",
        "--format",
        "json",
    ]);
    let warnings = v["warnings"].as_array().unwrap();
    let joined: String = warnings.iter().filter_map(|w| w.as_str()).collect();
    assert!(
        joined.contains("INACCURATE"),
        "warning not escalated: {joined}"
    );
    assert!(joined.contains("COMPOUNDS"));
}

#[test]
fn t_max_stake_absolute_dollar_cap() {
    // AC #39: --max-stake clamps in absolute dollars AFTER fractional sizing.
    // flat_current:pct=0.5 → 500, but --max-stake 100 → stake 100.
    let path = write_jsonl(
        "maxstake",
        &[
            r#"{ "bet_id": "1", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win" }"#,
        ],
    );
    let curve = std::env::temp_dir().join("mc_sim_maxstake_curve.jsonl");
    let v = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat_current:pct=0.5",
        "--max-stake",
        "100",
        "--emit-curve",
        curve.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert!((metric_f64(&v, "final_bank") - 1100.0).abs() < 1e-6);
    let body = std::fs::read_to_string(&curve).expect("curve");
    let first: Value = serde_json::from_str(body.lines().next().unwrap()).unwrap();
    assert!(
        (first["stake"].as_f64().unwrap() - 100.0).abs() < 1e-6,
        "{first}"
    );
}

#[test]
fn t_window_first_is_count_based_post_filter() {
    // AC #40: --window first:<n> selects the first N placed bets chronologically
    // AFTER --filter, count-based (not date-based).
    let path = write_jsonl(
        "firstn",
        &[
            r#"{ "bet_id": "1", "timestamp": "2025-01-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win", "edge": 0.05 }"#,
            r#"{ "bet_id": "2", "timestamp": "2025-06-01T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win", "edge": 0.20 }"#,
            r#"{ "bet_id": "3", "timestamp": "2025-06-02T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win", "edge": 0.20 }"#,
            r#"{ "bet_id": "4", "timestamp": "2025-06-03T00:00:00Z", "p_bet_side": 0.6, "decimal_odds": 2.0, "outcome": "win", "edge": 0.20 }"#,
        ],
    );
    // edge>=0.10 drops bet 1; first:2 → bets 2,3 (count-based, chronological).
    let v = sim_ok(&[
        "--bets",
        &path,
        "--start-bankroll",
        "1000",
        "--sizing",
        "flat:pct=0.1",
        "--filter",
        "edge >= 0.10",
        "--window",
        "first:2",
        "--format",
        "json",
    ]);
    assert_eq!(v["metrics"]["n_bets"], 2);
}
