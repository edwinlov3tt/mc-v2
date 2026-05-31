// ===========================================================================
// Command parsing, orchestration, and output (Steps 8-11; Decisions 1, 9,
// Amendments 9, 11, 14, 16). Included into `simulate.rs`.
// ===========================================================================

use std::collections::BTreeSet;
use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimFormat {
    Text,
    Json,
}

/// A fully-parsed `mc model simulate` invocation.
#[derive(Debug)]
pub struct SimulateCommand {
    pub cartridge: Option<String>,
    pub bets: String,
    pub start_bankroll: f64,
    pub sizing: SizingRule,
    pub sizing_str: String,
    pub filter: Option<(Filter, String)>,
    pub odds: OddsSource,
    pub odds_str: String,
    pub monte_carlo: Option<usize>,
    pub resample: Resample,
    pub resample_str: String,
    pub window: Window,
    pub window_str: String,
    pub seed: Option<u64>,
    pub format: SimFormat,
    pub emit_curve: Option<String>,
    pub columns: BTreeMap<String, String>,
    pub outcome_mode: OutcomeModeRequest,
    pub derive: Option<DerivePushes>,
    pub replay: ReplayMode,
    pub metrics: Vec<String>,
}

const KNOWN_METRICS: &[&str] = &[
    "final_bank",
    "roi",
    "roi_per_bet",
    "total_staked",
    "n_bets",
    "win_rate",
    "max_drawdown",
    "recovery_bets",
    "sharpe",
    "p_underwater",
    "terminal_p5",
    "terminal_p50",
    "terminal_p95",
];

fn help_text() -> String {
    "\
mc model simulate [<cartridge.yaml>] — chronological bankroll replay

Consumes a bet-record file (parquet or jsonl), sizes each bet via a Kelly
vocabulary, walks the bankroll forward in time order, and reports
final_bank / roi / max_drawdown / recovery / sharpe (+ optional Monte
Carlo bands). The cartridge positional is OPTIONAL (column-name provenance).

USAGE:
    mc model simulate [<cartridge.yaml>] --bets <file> --start-bankroll <amt> \\
        --sizing <rule>[:param=value,...] [options]

REQUIRED:
    --bets <file>            bet records (.parquet via DuckDB, or .jsonl)
    --start-bankroll <amt>   starting capital
    --sizing <rule>          flat:pct=X | flat_current:pct=X | kelly:fraction=F
                             | quarter_kelly | half_kelly | from_column:<col>
                             modifiers: cap=X,shrink=X,min_odds=X,floor=X

OPTIONS:
    --filter \"<predicate>\"   restrict the bet pool (e.g. \"abs_edge_pp >= 0.10\")
    --window all|first:<n>|range:<start>:<end>   filter FIRST, window SECOND
    --replay batch|sequential   same-timestamp discipline (default batch)
    --outcome-mode canonical|legacy-binary   default canonical (4-state req'd)
    --derive-pushes <actual>=<line>   reconstruct pushes from score columns
    --odds fixed:<d>|column:<name>   override odds for sizing AND settlement
    --columns <canon>=<src>,...   column aliasing override
    --monte-carlo <n>        run N resampled simulations
    --resample iid|block:<len>   bootstrap mode (default iid)
    --seed <int>             PRNG seed (required with --monte-carlo)
    --metric <name>          select metrics to display (repeatable)
    --format text|json       output format (default text)
    --emit-curve <path>      write the per-bet bankroll curve as jsonl
    -h, --help               show this help

REPLAY MODES:
    batch       same-commence-time bets sized off the bankroll at batch start,
                outcomes applied atomically (realistic default).
    sequential  compound each bet in order (sequence column or file order);
                reproduces legacy headline numbers.
"
    .to_string()
}

/// Parse `mc model simulate` arguments.
pub fn parse(args: &[String]) -> Result<SimulateCommand, String> {
    let mut cartridge: Option<String> = None;
    let mut bets: Option<String> = None;
    let mut start_bankroll: Option<f64> = None;
    let mut sizing_str: Option<String> = None;
    let mut filter_str: Option<String> = None;
    let mut odds_str: Option<String> = None;
    let mut monte_carlo: Option<usize> = None;
    let mut resample_str: Option<String> = None;
    let mut window_str: Option<String> = None;
    let mut seed: Option<u64> = None;
    let mut format = SimFormat::Text;
    let mut emit_curve: Option<String> = None;
    let mut columns: BTreeMap<String, String> = BTreeMap::new();
    let mut outcome_mode = OutcomeModeRequest::Canonical;
    let mut derive: Option<DerivePushes> = None;
    let mut replay = ReplayMode::Batch;
    let mut metrics: Vec<String> = Vec::new();

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print!("{}", help_text());
                std::process::exit(0);
            }
            "--bets" => bets = Some(next_val(&mut iter, "--bets")?),
            "--start-bankroll" => {
                let v = next_val(&mut iter, "--start-bankroll")?;
                let n: f64 = v
                    .parse()
                    .map_err(|_| format!("--start-bankroll expects a number, got {v:?}"))?;
                if n <= 0.0 {
                    return Err("--start-bankroll must be > 0".into());
                }
                start_bankroll = Some(n);
            }
            "--sizing" => sizing_str = Some(next_val(&mut iter, "--sizing")?),
            "--filter" => filter_str = Some(next_val(&mut iter, "--filter")?),
            "--odds" => odds_str = Some(next_val(&mut iter, "--odds")?),
            "--monte-carlo" => {
                let v = next_val(&mut iter, "--monte-carlo")?;
                let n: usize = v
                    .parse()
                    .map_err(|_| format!("--monte-carlo expects an integer, got {v:?}"))?;
                monte_carlo = Some(n);
            }
            "--resample" => resample_str = Some(next_val(&mut iter, "--resample")?),
            "--window" => window_str = Some(next_val(&mut iter, "--window")?),
            "--seed" => {
                let v = next_val(&mut iter, "--seed")?;
                let n: u64 = v
                    .parse()
                    .map_err(|_| format!("--seed expects an unsigned integer, got {v:?}"))?;
                seed = Some(n);
            }
            "--format" => {
                let v = next_val(&mut iter, "--format")?;
                format = match v.as_str() {
                    "text" => SimFormat::Text,
                    "json" => SimFormat::Json,
                    _ => return Err(format!("--format must be text|json, got {v:?}")),
                };
            }
            "--emit-curve" => emit_curve = Some(next_val(&mut iter, "--emit-curve")?),
            "--columns" => {
                let v = next_val(&mut iter, "--columns")?;
                for pair in v.split(',') {
                    let pair = pair.trim();
                    if pair.is_empty() {
                        continue;
                    }
                    let (k, src) = pair.split_once('=').ok_or_else(|| {
                        format!("--columns entry {pair:?} must be canonical=source")
                    })?;
                    columns.insert(k.trim().to_string(), src.trim().to_string());
                }
            }
            "--outcome-mode" => {
                let v = next_val(&mut iter, "--outcome-mode")?;
                outcome_mode = match v.as_str() {
                    "canonical" => OutcomeModeRequest::Canonical,
                    "legacy-binary" => OutcomeModeRequest::LegacyBinary,
                    _ => {
                        return Err(format!(
                            "--outcome-mode must be canonical|legacy-binary, got {v:?}"
                        ))
                    }
                };
            }
            "--derive-pushes" => {
                let v = next_val(&mut iter, "--derive-pushes")?;
                let (a, l) = v
                    .split_once('=')
                    .ok_or_else(|| format!("--derive-pushes expects <actual>=<line>, got {v:?}"))?;
                derive = Some(DerivePushes {
                    actual_col: a.trim().to_string(),
                    line_col: l.trim().to_string(),
                });
            }
            "--replay" => {
                let v = next_val(&mut iter, "--replay")?;
                replay = match v.as_str() {
                    "batch" => ReplayMode::Batch,
                    "sequential" => ReplayMode::Sequential,
                    _ => return Err(format!("--replay must be batch|sequential, got {v:?}")),
                };
            }
            "--metric" => {
                let v = next_val(&mut iter, "--metric")?;
                if !KNOWN_METRICS.contains(&v.as_str()) {
                    return Err(format!(
                        "unknown metric {v:?}; valid: {}",
                        KNOWN_METRICS.join(", ")
                    ));
                }
                metrics.push(v);
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown argument: {other:?}"));
            }
            other => {
                if cartridge.is_none() {
                    cartridge = Some(other.to_string());
                } else {
                    return Err(format!("unexpected positional argument: {other:?}"));
                }
            }
        }
    }

    let bets = bets.ok_or("--bets is required")?;
    let start_bankroll = start_bankroll.ok_or("--start-bankroll is required")?;
    let sizing_str = sizing_str.ok_or("--sizing is required")?;
    let sizing = parse_sizing(&sizing_str)?;

    if monte_carlo.is_some() && seed.is_none() {
        return Err("--seed is required when --monte-carlo is set (Amendment 5)".into());
    }

    let filter = match filter_str {
        Some(s) => Some((parse_filter(&s)?, s)),
        None => None,
    };
    let odds = match &odds_str {
        Some(s) => parse_odds(s)?,
        None => OddsSource::Record,
    };
    let window = match &window_str {
        Some(s) => parse_window(s)?,
        None => Window::All,
    };
    let resample = match &resample_str {
        Some(s) => parse_resample(s)?,
        None => Resample::Iid,
    };

    Ok(SimulateCommand {
        cartridge,
        bets,
        start_bankroll,
        sizing,
        sizing_str,
        filter,
        odds,
        odds_str: odds_str.unwrap_or_else(|| "record".to_string()),
        monte_carlo,
        resample,
        resample_str: resample_str.unwrap_or_else(|| "iid".to_string()),
        window,
        window_str: window_str.unwrap_or_else(|| "all".to_string()),
        seed,
        format,
        emit_curve,
        columns,
        outcome_mode,
        derive,
        replay,
        metrics,
    })
}

fn next_val(iter: &mut std::slice::Iter<'_, String>, flag: &str) -> Result<String, String> {
    iter.next()
        .cloned()
        .ok_or_else(|| format!("{flag} requires an argument"))
}

fn parse_resample(input: &str) -> Result<Resample, String> {
    let t = input.trim();
    if t == "iid" {
        return Ok(Resample::Iid);
    }
    if let Some(rest) = t.strip_prefix("block:") {
        let len: usize = rest
            .trim()
            .parse()
            .map_err(|_| format!("--resample block: expects an integer length, got {rest:?}"))?;
        if len == 0 {
            return Err("--resample block length must be >= 1".into());
        }
        return Ok(Resample::Block(len));
    }
    if t == "block" {
        // length filled in later from the pool size (default sqrt(N)).
        return Ok(Resample::Block(0));
    }
    Err(format!("--resample must be iid|block:<len>, got {t:?}"))
}

// ---------------------------------------------------------------------------
// Filter column references (for cartridge validation, A11)
// ---------------------------------------------------------------------------

fn collect_filter_columns(f: &Filter, out: &mut BTreeSet<String>) {
    match f {
        Filter::And(a, b) | Filter::Or(a, b) => {
            collect_filter_columns(a, out);
            collect_filter_columns(b, out);
        }
        Filter::Not(a) => collect_filter_columns(a, out),
        Filter::Cmp(col, _, _) => {
            out.insert(col.clone());
        }
    }
}

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

pub fn run(cmd: SimulateCommand) -> i32 {
    let (code, output) = run_captured(cmd);
    if !output.is_empty() {
        print!("{output}");
    }
    code
}

/// Execute and return `(exit_code, output)`.
pub fn run_captured(cmd: SimulateCommand) -> (i32, String) {
    match run_inner(&cmd) {
        Ok(out) => (0, out),
        Err(e) => (1, format!("error: {e}\n")),
    }
}

fn run_inner(cmd: &SimulateCommand) -> Result<String, String> {
    // Step 1-2: read + normalize records.
    let mut read = read_records(
        &cmd.bets,
        &cmd.columns,
        cmd.outcome_mode,
        cmd.derive.as_ref(),
        cmd.replay == ReplayMode::Sequential,
    )?;
    let mut warnings = read.warnings.clone();

    // Step 10: cartridge column-name provenance (A11), best-effort.
    if let Some(cart) = &cmd.cartridge {
        match load_model_with_policy(cart, LoadPolicy::Reproducible) {
            Ok(loaded) => {
                let measures: BTreeSet<String> = loaded
                    .cube
                    .measure_dimension()
                    .element_by_name
                    .keys()
                    .cloned()
                    .collect();
                let mut referenced: BTreeSet<String> = BTreeSet::new();
                if let Some((f, _)) = &cmd.filter {
                    collect_filter_columns(f, &mut referenced);
                }
                if let SizingKind::FromColumn = cmd.sizing.kind {
                    if let Some(c) = &cmd.sizing.column {
                        referenced.insert(c.clone());
                    }
                }
                if let OddsSource::Column(c) = &cmd.odds {
                    referenced.insert(c.clone());
                }
                for r in &referenced {
                    if !measures.contains(r) {
                        warnings.push(format!(
                            "cartridge provenance: referenced column {r:?} is not a declared \
                             measure in {cart} (column-name provenance is best-effort)"
                        ));
                    }
                }
            }
            Err(e) => {
                warnings.push(format!(
                    "cartridge {cart} could not be loaded for provenance ({}); continuing on \
                     records alone",
                    e.message()
                ));
            }
        }
    }

    // Step 5 (A12): filter FIRST.
    let mut records = std::mem::take(&mut read.records);
    if let Some((f, _)) = &cmd.filter {
        records.retain(|r| eval_filter(f, r));
    }

    // Order by time (replay-mode dependent tiebreak), then window SECOND.
    order_records(&mut records, cmd.replay);
    let ordered = apply_window(records, &cmd.window);

    if ordered.is_empty() {
        warnings.push("filtered/windowed bet pool is empty — no bets placed".to_string());
    }

    // Step 4-5: single-path replay.
    let result = replay(
        &ordered,
        cmd.start_bankroll,
        &cmd.sizing,
        &cmd.odds,
        cmd.replay,
    );
    let metrics = compute_metrics(&result);

    // Step 7: Monte Carlo (optional).
    let mc = if let Some(runs) = cmd.monte_carlo {
        let seed = cmd.seed.unwrap_or(0);
        let resample = match &cmd.resample {
            Resample::Block(0) => Resample::Block(default_block_len(ordered.len())),
            other => other.clone(),
        };
        Some(run_monte_carlo(
            &ordered,
            cmd.start_bankroll,
            &cmd.sizing,
            &cmd.odds,
            runs,
            &resample,
            seed,
        ))
    } else {
        None
    };

    // Step 9: optional curve emission.
    let mut curve_path: Option<String> = None;
    if let Some(path) = &cmd.emit_curve {
        write_curve(path, &result.curve)?;
        curve_path = Some(path.clone());
        if result.curve.is_empty() {
            warnings.push(format!("curve {path} is header-only (empty bet pool)"));
        }
    }

    // Output.
    let out = match cmd.format {
        SimFormat::Text => format_text(cmd, &read, &result, &metrics, mc.as_ref(), &warnings),
        SimFormat::Json => format_json(
            cmd,
            &read,
            &result,
            &metrics,
            mc.as_ref(),
            &warnings,
            curve_path.as_deref(),
        ),
    };
    Ok(out)
}

/// Order records for replay. Batch: `(ts_key, bet_id)`. Sequential: stable
/// by `ts_key`, intra-timestamp by `sequence` column if present, else stable
/// file order (Amendment 17 — never re-sort by bet_id).
fn order_records(records: &mut [BetRecord], mode: ReplayMode) {
    match mode {
        ReplayMode::Batch => {
            records.sort_by(|a, b| a.ts_key.cmp(&b.ts_key).then_with(|| cmp_bet_id(a, b)));
        }
        ReplayMode::Sequential => {
            // Stable sort: equal ts with no sequence → preserve file order.
            records.sort_by(|a, b| {
                a.ts_key
                    .cmp(&b.ts_key)
                    .then_with(|| match (a.sequence, b.sequence) {
                        (Some(x), Some(y)) => {
                            x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
                        }
                        _ => std::cmp::Ordering::Equal,
                    })
            });
        }
    }
}

fn cmp_bet_id(a: &BetRecord, b: &BetRecord) -> std::cmp::Ordering {
    match (a.bet_id.parse::<f64>(), b.bet_id.parse::<f64>()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        _ => a.bet_id.cmp(&b.bet_id),
    }
}

// ---------------------------------------------------------------------------
// Curve output (jsonl, Amendment 4/14)
// ---------------------------------------------------------------------------

fn outcome_str(o: Outcome) -> &'static str {
    match o {
        Outcome::Win => "win",
        Outcome::Loss => "loss",
        Outcome::Push => "push",
        Outcome::Void => "void",
    }
}

fn write_curve(path: &str, curve: &[CurveRow]) -> Result<(), String> {
    let mut buf = String::new();
    for row in curve {
        let mut obj = Map::new();
        obj.insert("timestamp".into(), json!(row.timestamp));
        obj.insert("bet_id".into(), json!(row.bet_id));
        if let Some(s) = row.season {
            obj.insert("season".into(), json!(s));
        }
        if let Some(side) = &row.side {
            obj.insert("side".into(), json!(side));
        }
        obj.insert("p_bet_side".into(), json!(row.p_bet_side));
        if let Some(e) = row.abs_edge_pp {
            obj.insert("abs_edge_pp".into(), json!(e));
        }
        obj.insert("stake".into(), json!(row.stake));
        obj.insert("outcome".into(), json!(outcome_str(row.outcome)));
        obj.insert("bankroll_after".into(), json!(row.bankroll_after));
        obj.insert("batch_id".into(), json!(row.batch_id));
        let line = serde_json::to_string(&Value::Object(obj))
            .map_err(|e| format!("failed to serialize curve row: {e}"))?;
        buf.push_str(&line);
        buf.push('\n');
    }
    fs::write(path, buf).map_err(|e| format!("failed to write curve {path}: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Text output (Decision 9)
// ---------------------------------------------------------------------------

fn want_metric(cmd: &SimulateCommand, name: &str) -> bool {
    cmd.metrics.is_empty() || cmd.metrics.iter().any(|m| m == name)
}

fn fmt_money(v: f64) -> String {
    format!("{v:.2}")
}

fn fmt_pct(v: f64) -> String {
    format!("{:+.2}%", v * 100.0)
}

fn fmt_opt<F: Fn(f64) -> String>(v: Option<f64>, f: F) -> String {
    match v {
        Some(x) => f(x),
        None => "null".to_string(),
    }
}

fn format_text(
    cmd: &SimulateCommand,
    read: &ReadResult,
    result: &ReplayResult,
    m: &Metrics,
    mc: Option<&MonteCarloResult>,
    warnings: &[String],
) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "mc model simulate — bankroll replay");
    let _ = writeln!(
        o,
        "  input: {} ({}), replay={}, outcome_mode={}",
        cmd.bets,
        read.format.as_str(),
        match cmd.replay {
            ReplayMode::Batch => "batch",
            ReplayMode::Sequential => "sequential",
        },
        read.outcome_mode.as_str()
    );
    let _ = writeln!(
        o,
        "  sizing: {} | filter: {} | window: {} | odds: {}",
        cmd.sizing_str,
        cmd.filter
            .as_ref()
            .map(|(_, s)| s.as_str())
            .unwrap_or("(none)"),
        cmd.window_str,
        cmd.odds_str
    );
    let _ = writeln!(o);
    let _ = writeln!(o, "  start bankroll : {}", fmt_money(m.start_bankroll));
    if want_metric(cmd, "final_bank") {
        let _ = writeln!(o, "  final bankroll : {}", fmt_money(m.final_bank));
    }
    if want_metric(cmd, "roi") {
        let _ = writeln!(o, "  roi (cumulative): {}", fmt_opt(m.roi, fmt_pct));
    }
    if want_metric(cmd, "roi_per_bet") {
        let _ = writeln!(o, "  roi per bet    : {}", fmt_opt(m.roi_per_bet, fmt_pct));
    }
    if want_metric(cmd, "total_staked") {
        let _ = writeln!(o, "  total staked   : {}", fmt_money(m.total_staked));
    }
    if want_metric(cmd, "n_bets") {
        let _ = writeln!(
            o,
            "  bets placed    : {} ({} win / {} loss / {} push)",
            m.n_bets, m.wins, m.losses, m.pushes
        );
    }
    if want_metric(cmd, "win_rate") {
        let _ = writeln!(
            o,
            "  win rate       : {}",
            fmt_opt(m.win_rate, |v| format!("{:.4}", v))
        );
    }
    if want_metric(cmd, "max_drawdown") {
        let _ = writeln!(o, "  max drawdown   : {}", fmt_pct_pos(m.max_drawdown));
    }
    if want_metric(cmd, "recovery_bets") {
        let _ = writeln!(
            o,
            "  recovery bets  : {} ({})",
            m.recovery_bets
                .map(|n| n.to_string())
                .unwrap_or_else(|| "null".to_string()),
            m.recovery_status.as_str()
        );
    }
    if want_metric(cmd, "sharpe") {
        let _ = writeln!(
            o,
            "  sharpe         : {}",
            fmt_opt(m.sharpe, |v| format!("{:.4}", v))
        );
    }
    if result.ruin {
        let _ = writeln!(
            o,
            "  RUIN at bet index {} — remaining bets skipped",
            result.ruin_index.map(|i| i.to_string()).unwrap_or_default()
        );
    }

    if let Some(mc) = mc {
        let _ = writeln!(o);
        let _ = writeln!(
            o,
            "  Monte Carlo ({} runs, resample={}):",
            mc.runs, mc.resample
        );
        let _ = writeln!(
            o,
            "    metric          P5        P25       P50       P75       P95"
        );
        let _ = writeln!(
            o,
            "    final_bank   {}",
            fmt_band(&mc.final_bank, fmt_money)
        );
        let _ = writeln!(o, "    roi          {}", fmt_band(&mc.roi, fmt_pct));
        let _ = writeln!(
            o,
            "    max_drawdown {}",
            fmt_band(&mc.max_drawdown, fmt_pct_pos)
        );
        let _ = writeln!(o, "    p_underwater : {:.4}", mc.p_underwater);
    }

    if !warnings.is_empty() {
        let _ = writeln!(o);
        for w in warnings {
            let _ = writeln!(o, "  warning: {w}");
        }
    }
    o
}

fn fmt_pct_pos(v: f64) -> String {
    format!("{:.2}%", v * 100.0)
}

fn fmt_band<F: Fn(f64) -> String>(b: &Bands, f: F) -> String {
    format!(
        "{:>9} {:>9} {:>9} {:>9} {:>9}",
        f(b.p5),
        f(b.p25),
        f(b.p50),
        f(b.p75),
        f(b.p95)
    )
}

// ---------------------------------------------------------------------------
// JSON output (Decision 9, Amendment 16 — the codegen contract)
// ---------------------------------------------------------------------------

fn f64_or_null(v: Option<f64>) -> Value {
    match v {
        Some(x) => json!(x),
        None => Value::Null,
    }
}

fn bands_json(b: &Bands) -> Value {
    json!({
        "p5": b.p5,
        "p25": b.p25,
        "p50": b.p50,
        "p75": b.p75,
        "p95": b.p95,
    })
}

#[allow(clippy::too_many_arguments)]
fn format_json(
    cmd: &SimulateCommand,
    read: &ReadResult,
    result: &ReplayResult,
    m: &Metrics,
    mc: Option<&MonteCarloResult>,
    warnings: &[String],
    curve_path: Option<&str>,
) -> String {
    let outcome_counts = json!({
        "win": result.wins,
        "loss": result.losses,
        "push": result.pushes,
        "void": result.voids,
    });
    let skip_counts: Map<String, Value> = result
        .skip_counts
        .iter()
        .map(|(k, v)| (k.clone(), json!(v)))
        .collect();

    let schema_mapping: Map<String, Value> = read
        .schema_mapping
        .iter()
        .map(|(k, v)| (k.clone(), json!(v)))
        .collect();

    let metrics_obj = json!({
        "start_bankroll": m.start_bankroll,
        "final_bank": m.final_bank,
        "roi": f64_or_null(m.roi),
        "roi_per_bet": f64_or_null(m.roi_per_bet),
        "total_staked": m.total_staked,
        "total_pnl": m.total_pnl,
        "n_bets": m.n_bets,
        "wins": m.wins,
        "losses": m.losses,
        "pushes": m.pushes,
        "win_rate": f64_or_null(m.win_rate),
        "max_drawdown": m.max_drawdown,
        "recovery_bets": m.recovery_bets.map(|n| json!(n)).unwrap_or(Value::Null),
        "recovery_status": m.recovery_status.as_str(),
        "sharpe": f64_or_null(m.sharpe),
    });

    let run_config = json!({
        "cartridge": cmd.cartridge.clone().map(Value::String).unwrap_or(Value::Null),
        "bets": cmd.bets,
        "start_bankroll": cmd.start_bankroll,
        "sizing": cmd.sizing_str,
        "filter": cmd.filter.as_ref().map(|(_, s)| Value::String(s.clone())).unwrap_or(Value::Null),
        "window": cmd.window_str,
        "odds": cmd.odds_str,
        "replay": match cmd.replay { ReplayMode::Batch => "batch", ReplayMode::Sequential => "sequential" },
        "seed": cmd.seed.map(|s| json!(s)).unwrap_or(Value::Null),
        "monte_carlo": cmd.monte_carlo.map(|n| json!(n)).unwrap_or(Value::Null),
        "resample": cmd.resample_str,
    });

    let mut root = Map::new();
    root.insert("schema_version".into(), json!(SIMULATE_SCHEMA_VERSION));
    root.insert("command".into(), json!("simulate"));
    root.insert("input_format".into(), json!(read.format.as_str()));
    root.insert("outcome_mode".into(), json!(read.outcome_mode.as_str()));
    root.insert("schema_mapping".into(), Value::Object(schema_mapping));
    root.insert("metrics".into(), metrics_obj);
    root.insert("outcome_counts".into(), outcome_counts);
    root.insert("skip_counts".into(), Value::Object(skip_counts));
    root.insert("ruin".into(), json!(result.ruin));
    root.insert(
        "ruin_index".into(),
        result.ruin_index.map(|i| json!(i)).unwrap_or(Value::Null),
    );
    root.insert("recovery_status".into(), json!(m.recovery_status.as_str()));
    root.insert(
        "curve_path".into(),
        curve_path.map(|p| json!(p)).unwrap_or(Value::Null),
    );
    root.insert("run_config".into(), run_config);
    root.insert(
        "warnings".into(),
        Value::Array(warnings.iter().map(|w| json!(w)).collect()),
    );

    if let Some(mc) = mc {
        root.insert(
            "monte_carlo".into(),
            json!({
                "runs": mc.runs,
                "resample": mc.resample,
                "p_underwater": mc.p_underwater,
                "final_bank": bands_json(&mc.final_bank),
                "roi": bands_json(&mc.roi),
                "max_drawdown": bands_json(&mc.max_drawdown),
            }),
        );
    }

    let mut s =
        serde_json::to_string_pretty(&Value::Object(root)).unwrap_or_else(|_| "{}".to_string());
    s.push('\n');
    s
}
