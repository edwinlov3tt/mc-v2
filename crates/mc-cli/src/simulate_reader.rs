// ===========================================================================
// Bet-record reader (Step 1; Decision 2, Amendment 4)
//
// Included into `simulate.rs`. Reads parquet (via the DuckDB path in
// `mc-drivers`) or jsonl into a normalized `Vec<BetRecord>`, resolving
// column aliases and normalizing the 4-state outcome (Amendment 2).
// ===========================================================================

use mc_drivers::{duckdb_driver, ColumnData, SourceDriver};
use std::fs;

/// The input serialization the records came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFormat {
    Parquet,
    Jsonl,
}

impl InputFormat {
    fn as_str(self) -> &'static str {
        match self {
            InputFormat::Parquet => "parquet",
            InputFormat::Jsonl => "jsonl",
        }
    }
}

/// How the outcome column was interpreted (Amendment 2 / 16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutcomeMode {
    /// A 4-state `outcome` enum was present and used as-is.
    Canonical,
    /// Binary 0/1 scored 1→win, 0→loss (explicit `--outcome-mode legacy-binary`).
    LegacyBinary,
    /// Pushes reconstructed from `actual_total == line` (`--derive-pushes`).
    DerivedPushes,
}

impl OutcomeMode {
    fn as_str(self) -> &'static str {
        match self {
            OutcomeMode::Canonical => "canonical",
            OutcomeMode::LegacyBinary => "legacy-binary",
            OutcomeMode::DerivedPushes => "derived-pushes",
        }
    }
}

/// Outcome-mode request from the CLI (`--outcome-mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutcomeModeRequest {
    /// Default: require a 4-state enum; bare 0/1 hard-errors.
    Canonical,
    /// Allow binary 0/1 (1→win, 0→loss).
    LegacyBinary,
}

/// A `--derive-pushes <actual>=<line>` request.
#[derive(Debug, Clone)]
pub struct DerivePushes {
    pub actual_col: String,
    pub line_col: String,
}

/// The resolved canonical→source column mapping (for JSON output, A16).
pub type SchemaMapping = BTreeMap<String, String>;

/// The fully-read, normalized record set plus reader-side metadata.
pub struct ReadResult {
    pub records: Vec<BetRecord>,
    pub format: InputFormat,
    pub outcome_mode: OutcomeMode,
    pub schema_mapping: SchemaMapping,
    pub warnings: Vec<String>,
}

/// Canonical field → known source aliases (first match wins after the
/// canonical name itself and any `--columns`/sidecar override).
fn aliases(canonical: &str) -> &'static [&'static str] {
    match canonical {
        "bet_id" => &["bet_id", "game_pk", "id"],
        "timestamp" => &["timestamp", "commence_time", "ts", "date"],
        "p_bet_side" => &["p_bet_side", "p", "prob"],
        "decimal_odds" => &["decimal_odds", "odds"],
        "outcome" => &["outcome", "won", "result"],
        _ => &[],
    }
}

/// Resolve a canonical field to a source column present in `columns`,
/// honoring an explicit override first.
fn resolve_column(
    canonical: &str,
    columns: &[String],
    overrides: &BTreeMap<String, String>,
) -> Option<String> {
    if let Some(src) = overrides.get(canonical) {
        return Some(src.clone());
    }
    for cand in aliases(canonical) {
        if columns.iter().any(|c| c == cand) {
            return Some((*cand).to_string());
        }
    }
    None
}

/// Read a sidecar `<records>.schema.json` if present: a flat JSON object
/// mapping canonical → source names. Returns an empty map if absent.
fn load_sidecar(path: &str) -> Result<BTreeMap<String, String>, String> {
    let sidecar = format!("{path}.schema.json");
    if !Path::new(&sidecar).exists() {
        return Ok(BTreeMap::new());
    }
    let text = fs::read_to_string(&sidecar)
        .map_err(|e| format!("failed to read sidecar {sidecar}: {e}"))?;
    let v: Value = serde_json::from_str(&text)
        .map_err(|e| format!("sidecar {sidecar} is not valid JSON: {e}"))?;
    let obj = v
        .as_object()
        .ok_or_else(|| format!("sidecar {sidecar} must be a JSON object of canonical→source"))?;
    let mut out = BTreeMap::new();
    for (k, val) in obj {
        if let Some(s) = val.as_str() {
            out.insert(k.clone(), s.to_string());
        }
    }
    Ok(out)
}

/// Read the raw table from parquet (DuckDB) or jsonl, dispatching on
/// extension.
fn read_raw_table(path: &str) -> Result<(RawTable, InputFormat), String> {
    if !Path::new(path).exists() {
        return Err(format!("bet-record file not found: {path}"));
    }
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".parquet") || lower.ends_with(".pq") {
        Ok((read_parquet(path)?, InputFormat::Parquet))
    } else if lower.ends_with(".jsonl") || lower.ends_with(".ndjson") || lower.ends_with(".json") {
        Ok((read_jsonl(path)?, InputFormat::Jsonl))
    } else {
        Err(format!(
            "unrecognized bet-record extension for {path:?}; expected .parquet or .jsonl"
        ))
    }
}

/// Read a parquet file through the existing DuckDB driver (Amendment 4 —
/// no new Arrow/parquet dependency). `read_parquet` is DuckDB's native
/// table function.
fn read_parquet(path: &str) -> Result<RawTable, String> {
    let escaped = path.replace('\'', "''");
    let query = format!("SELECT * FROM read_parquet('{escaped}')");
    let mut driver = duckdb_driver(Path::new(":memory:"), &query)
        .map_err(|e| format!("failed to open parquet {path}: {e}"))?;
    let schema = driver
        .schema()
        .map_err(|e| format!("failed to read parquet schema for {path}: {e}"))?;
    let columns: Vec<String> = schema.iter().map(|c| c.name.clone()).collect();

    let mut rows: Vec<BTreeMap<String, RecordValue>> = Vec::new();
    while let Some(batch) = driver
        .fetch_batch(65_536)
        .map_err(|e| format!("failed to read parquet rows for {path}: {e}"))?
    {
        let start = rows.len();
        rows.resize_with(start + batch.row_count, BTreeMap::new);
        for col in &batch.columns {
            for r in 0..batch.row_count {
                let val = column_value_at(&col.data, r);
                rows[start + r].insert(col.name.clone(), val);
            }
        }
    }
    Ok(RawTable { columns, rows })
}

fn column_value_at(data: &ColumnData, r: usize) -> RecordValue {
    match data {
        ColumnData::F64(v) => v[r].map(RecordValue::Num).unwrap_or(RecordValue::Null),
        ColumnData::I64(v) => v[r]
            .map(|i| RecordValue::Num(i as f64))
            .unwrap_or(RecordValue::Null),
        ColumnData::Str(v) => v[r]
            .clone()
            .map(RecordValue::Text)
            .unwrap_or(RecordValue::Null),
        ColumnData::Bool(v) => v[r]
            .map(|b| RecordValue::Num(if b { 1.0 } else { 0.0 }))
            .unwrap_or(RecordValue::Null),
        // `ColumnData` is #[non_exhaustive]; any future variant is read as Null.
        _ => RecordValue::Null,
    }
}

/// Read newline-delimited JSON objects (dependency: serde_json, already a
/// `mc-cli` dep). Column order = first-seen across all rows.
fn read_jsonl(path: &str) -> Result<RawTable, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("failed to read {path}: {e}"))?;
    let mut columns: Vec<String> = Vec::new();
    let mut seen: BTreeMap<String, ()> = BTreeMap::new();
    let mut rows: Vec<BTreeMap<String, RecordValue>> = Vec::new();
    for (lineno, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line)
            .map_err(|e| format!("{path}:{}: invalid JSON: {e}", lineno + 1))?;
        let obj = v.as_object().ok_or_else(|| {
            format!(
                "{path}:{}: each jsonl line must be a JSON object",
                lineno + 1
            )
        })?;
        let mut row = BTreeMap::new();
        for (k, val) in obj {
            if seen.insert(k.clone(), ()).is_none() {
                columns.push(k.clone());
            }
            row.insert(k.clone(), json_to_record_value(val));
        }
        rows.push(row);
    }
    Ok(RawTable { columns, rows })
}

fn json_to_record_value(v: &Value) -> RecordValue {
    match v {
        Value::Null => RecordValue::Null,
        Value::Bool(b) => RecordValue::Num(if *b { 1.0 } else { 0.0 }),
        Value::Number(n) => n
            .as_f64()
            .map(RecordValue::Num)
            .unwrap_or(RecordValue::Null),
        Value::String(s) => RecordValue::Text(s.clone()),
        // Nested arrays/objects are not scalar record cells; stringify so
        // they remain inspectable but unusable as numerics.
        other => RecordValue::Text(other.to_string()),
    }
}

/// Normalize one row's outcome cell into the 4-state enum, given the
/// resolved mode. `derive` (when present) overrides via actual==line.
fn normalize_outcome(
    row: &BTreeMap<String, RecordValue>,
    outcome_col: &str,
    mode: OutcomeMode,
    derive: Option<&DerivePushes>,
) -> Result<Outcome, String> {
    // Derive-pushes path: reconstruct push from actual==line, else win/loss
    // from the binary outcome.
    if mode == OutcomeMode::DerivedPushes {
        if let Some(d) = derive {
            let actual = row.get(&d.actual_col).and_then(RecordValue::as_num);
            let line = row.get(&d.line_col).and_then(RecordValue::as_num);
            if let (Some(a), Some(l)) = (actual, line) {
                if (a - l).abs() < 1e-9 {
                    return Ok(Outcome::Push);
                }
            }
        }
        // Not a push → fall through to binary scoring of the outcome col.
        return binary_outcome(row, outcome_col);
    }

    let cell = row.get(outcome_col);
    match cell {
        Some(RecordValue::Text(s)) => parse_outcome_enum(s),
        Some(RecordValue::Num(n)) => {
            if mode == OutcomeMode::LegacyBinary {
                binary_from_num(*n, outcome_col)
            } else {
                Err(format!(
                    "outcome column {outcome_col:?} is numeric 0/1 (binary). The 4-state \
                     outcome (win/loss/push/void) is required. Re-run with \
                     `--outcome-mode legacy-binary` to score 1→win/0→loss (pushes/voids \
                     approximated), or `--derive-pushes actual_total=line` to reconstruct \
                     pushes."
                ))
            }
        }
        _ => Err(format!(
            "outcome column {outcome_col:?} is missing/null in a record"
        )),
    }
}

fn parse_outcome_enum(s: &str) -> Result<Outcome, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "win" | "won" | "w" => Ok(Outcome::Win),
        "loss" | "lose" | "lost" | "l" => Ok(Outcome::Loss),
        "push" | "tie" | "p" => Ok(Outcome::Push),
        "void" | "v" | "no_action" | "postponed" => Ok(Outcome::Void),
        // A bare "1"/"0" string is still binary — reject under canonical.
        "1" | "0" => Err(format!(
            "outcome value {s:?} is binary; pass --outcome-mode legacy-binary"
        )),
        other => Err(format!(
            "unknown outcome value {other:?}; expected win/loss/push/void"
        )),
    }
}

fn binary_outcome(
    row: &BTreeMap<String, RecordValue>,
    outcome_col: &str,
) -> Result<Outcome, String> {
    match row.get(outcome_col) {
        Some(RecordValue::Num(n)) => binary_from_num(*n, outcome_col),
        Some(RecordValue::Text(s)) => match s.trim() {
            "1" => Ok(Outcome::Win),
            "0" => Ok(Outcome::Loss),
            _ => parse_outcome_enum(s),
        },
        _ => Err(format!(
            "outcome column {outcome_col:?} is missing/null in a record"
        )),
    }
}

fn binary_from_num(n: f64, col: &str) -> Result<Outcome, String> {
    if (n - 1.0).abs() < 1e-9 {
        Ok(Outcome::Win)
    } else if n.abs() < 1e-9 {
        Ok(Outcome::Loss)
    } else {
        Err(format!(
            "outcome column {col:?} has non-binary numeric value {n}; expected 0 or 1"
        ))
    }
}

/// Determine whether the outcome column is a 4-state enum, binary, or mixed
/// by scanning the rows. Used to pick the effective `OutcomeMode`.
fn detect_outcome_kind(rows: &[BTreeMap<String, RecordValue>], outcome_col: &str) -> OutcomeKind {
    let mut saw_enum = false;
    let mut saw_binary = false;
    for row in rows {
        match row.get(outcome_col) {
            Some(RecordValue::Text(s)) => {
                let t = s.trim();
                if t == "0" || t == "1" {
                    saw_binary = true;
                } else {
                    saw_enum = true;
                }
            }
            Some(RecordValue::Num(n)) => {
                if (n - 1.0).abs() < 1e-9 || n.abs() < 1e-9 {
                    saw_binary = true;
                } else {
                    // out-of-range numeric — treat as enum-ish (will error later)
                    saw_enum = true;
                }
            }
            _ => {}
        }
    }
    match (saw_enum, saw_binary) {
        (true, false) => OutcomeKind::Enum,
        (false, true) => OutcomeKind::Binary,
        (false, false) => OutcomeKind::Empty,
        (true, true) => OutcomeKind::Mixed,
    }
}

enum OutcomeKind {
    Enum,
    Binary,
    Empty,
    Mixed,
}

/// The default score/line column pair simulate auto-derives pushes from
/// (Amendment 18). claw-core's schema uses `actual_total`/`line`. Returns
/// the pair only when BOTH are present — a file without them cannot derive
/// pushes and falls to legacy-binary + the escalated warning (not an error).
fn can_derive_pushes(columns: &[String]) -> Option<DerivePushes> {
    let actual = "actual_total";
    let line = "line";
    if columns.iter().any(|c| c == actual) && columns.iter().any(|c| c == line) {
        Some(DerivePushes {
            actual_col: actual.to_string(),
            line_col: line.to_string(),
        })
    } else {
        None
    }
}

/// Top-level reader entry: read, resolve columns, normalize, validate.
#[allow(clippy::too_many_arguments)]
pub fn read_records(
    path: &str,
    cli_columns: &BTreeMap<String, String>,
    mode_req: OutcomeModeRequest,
    derive: Option<&DerivePushes>,
    no_derive: bool,
) -> Result<ReadResult, String> {
    let (table, format) = read_raw_table(path)?;
    if table.rows.is_empty() {
        return Err(format!("bet-record file {path} has zero rows"));
    }

    // Merge column overrides: CLI > sidecar.
    let mut overrides = load_sidecar(path)?;
    for (k, v) in cli_columns {
        overrides.insert(k.clone(), v.clone());
    }

    let mut schema_mapping: SchemaMapping = BTreeMap::new();
    let resolve = |canonical: &str| -> Option<String> {
        resolve_column(canonical, &table.columns, &overrides)
    };

    let bet_id_col = resolve("bet_id").ok_or_else(|| missing_col("bet_id", &table.columns))?;
    let ts_col = resolve("timestamp").ok_or_else(|| missing_col("timestamp", &table.columns))?;
    let p_col = resolve("p_bet_side").ok_or_else(|| missing_col("p_bet_side", &table.columns))?;
    let odds_col =
        resolve("decimal_odds").ok_or_else(|| missing_col("decimal_odds", &table.columns))?;
    let outcome_col = resolve("outcome").ok_or_else(|| missing_col("outcome", &table.columns))?;

    for (canonical, src) in [
        ("bet_id", &bet_id_col),
        ("timestamp", &ts_col),
        ("p_bet_side", &p_col),
        ("decimal_odds", &odds_col),
        ("outcome", &outcome_col),
    ] {
        schema_mapping.insert(canonical.to_string(), src.clone());
    }

    // Decide the effective outcome mode (Amendment 18). Push-accuracy is the
    // DEFAULT whenever it's derivable. Precedence for derivation:
    //   --no-derive-pushes > explicit --derive-pushes a=b > auto-derive-default
    // and only over a binary outcome column (a 4-state enum is authoritative).
    let mut warnings: Vec<String> = Vec::new();
    let kind = detect_outcome_kind(&table.rows, &outcome_col);
    let binary_ish = matches!(kind, OutcomeKind::Binary | OutcomeKind::Mixed);

    let effective_derive: Option<DerivePushes> = if no_derive {
        // --no-derive-pushes wins over everything: reproduce prior behavior.
        None
    } else if let Some(d) = derive {
        // Explicit pair must be present (named non-default columns).
        if !table.columns.iter().any(|c| c == &d.actual_col)
            || !table.columns.iter().any(|c| c == &d.line_col)
        {
            return Err(format!(
                "--derive-pushes needs both {:?} and {:?} columns; present: {}",
                d.actual_col,
                d.line_col,
                table.columns.join(", ")
            ));
        }
        Some(d.clone())
    } else if binary_ish {
        // Auto-derive default: only meaningful over a binary outcome column.
        can_derive_pushes(&table.columns)
    } else {
        None
    };

    let effective_mode = if effective_derive.is_some() {
        OutcomeMode::DerivedPushes
    } else {
        match kind {
            OutcomeKind::Enum => OutcomeMode::Canonical,
            OutcomeKind::Binary => {
                if mode_req == OutcomeModeRequest::LegacyBinary {
                    OutcomeMode::LegacyBinary
                } else {
                    return Err(format!(
                        "outcome column {outcome_col:?} is binary 0/1 and no push-derivable \
                         score columns (actual_total + line) were found. The 4-state outcome \
                         (win/loss/push/void) is required by default. Provide a 4-state outcome \
                         column, pass `--derive-pushes <actual>=<line>` to name your score/line \
                         columns, or `--outcome-mode legacy-binary` to score 1→win/0→loss \
                         (INACCURATE: any integer-line push is mis-scored)."
                    ));
                }
            }
            OutcomeKind::Empty => {
                return Err(format!(
                    "outcome column {outcome_col:?} has no usable values"
                ))
            }
            OutcomeKind::Mixed => {
                if mode_req == OutcomeModeRequest::LegacyBinary {
                    OutcomeMode::LegacyBinary
                } else {
                    OutcomeMode::Canonical
                }
            }
        }
    };

    if effective_mode == OutcomeMode::LegacyBinary {
        // Escalated (Amendment 18 §2): legacy-binary is INACCURATE, not just
        // approximate, when undetected pushes are scored as wins/losses.
        warnings.push(
            "outcome scored as legacy-binary (1→win, 0→loss): pushes are counted as \
             wins/losses, so the bankroll is INACCURATE — not merely approximate. Any \
             integer-line game landing exactly on the line is a push being mis-scored; for \
             direction-skewed models (mostly-OVER or mostly-UNDER) the error COMPOUNDS across \
             the season. `win_rate` may also be inflated by these undetected pushes. \
             legacy-binary is intended only for reproducing a known-published number; for an \
             accurate bankroll provide a 4-state outcome column or score/line columns \
             (actual_total + line) so pushes auto-derive."
                .to_string(),
        );
    }
    if let Some(d) = &effective_derive {
        warnings.push(format!(
            "pushes auto-derived where ({} - {}).abs() < 1e-9 → push (4-state outcome \
             reconstructed from the binary score + line columns); pushes are neutral \
             (stake returned).",
            d.actual_col, d.line_col
        ));
    }

    let seq_present = table.columns.iter().any(|c| c == "sequence");

    // Build normalized records.
    let mut records: Vec<BetRecord> = Vec::with_capacity(table.rows.len());
    for (idx, row) in table.rows.into_iter().enumerate() {
        let bet_id = match row.get(&bet_id_col) {
            Some(RecordValue::Text(s)) => s.clone(),
            Some(RecordValue::Num(n)) => format_num(*n),
            _ => return Err(format!("record {idx}: missing bet_id ({bet_id_col})")),
        };
        let ts_raw = row
            .get(&ts_col)
            .ok_or_else(|| format!("record {idx}: missing timestamp ({ts_col})"))?;
        let ts_key = parse_timestamp_key(ts_raw).ok_or_else(|| {
            format!("record {idx}: unparseable timestamp in {ts_col}: {ts_raw:?}")
        })?;
        let timestamp = match ts_raw {
            RecordValue::Text(s) => s.clone(),
            RecordValue::Num(n) => format_num(*n),
            RecordValue::Null => return Err(format!("record {idx}: null timestamp")),
        };
        let p_bet_side = row
            .get(&p_col)
            .and_then(RecordValue::as_num)
            .ok_or_else(|| format!("record {idx}: missing/non-numeric p_bet_side ({p_col})"))?;
        if !(0.0..=1.0).contains(&p_bet_side) {
            return Err(format!(
                "record {idx}: p_bet_side={p_bet_side} out of range [0,1] ({p_col})"
            ));
        }
        let decimal_odds = row
            .get(&odds_col)
            .and_then(RecordValue::as_num)
            .ok_or_else(|| {
                format!("record {idx}: missing/non-numeric decimal_odds ({odds_col})")
            })?;
        if decimal_odds <= 1.0 {
            return Err(format!(
                "record {idx}: decimal_odds={decimal_odds} must be > 1.0 ({odds_col})"
            ));
        }
        let outcome = normalize_outcome(
            &row,
            &outcome_col,
            effective_mode,
            effective_derive.as_ref(),
        )
        .map_err(|e| format!("record {idx}: {e}"))?;
        let sequence = if seq_present {
            row.get("sequence").and_then(RecordValue::as_num)
        } else {
            None
        };

        records.push(BetRecord {
            file_index: idx,
            bet_id,
            ts_key,
            timestamp,
            p_bet_side,
            decimal_odds,
            outcome,
            sequence,
            cells: row,
        });
    }

    Ok(ReadResult {
        records,
        format,
        outcome_mode: effective_mode,
        schema_mapping,
        warnings,
    })
}

fn missing_col(canonical: &str, columns: &[String]) -> String {
    format!(
        "required column for {canonical:?} not found (looked for {}). Present columns: {}. \
         Use --columns {canonical}=<source> or a {{records}}.schema.json sidecar.",
        aliases(canonical).join("/"),
        columns.join(", ")
    )
}

// ===========================================================================
// Window (Decision 5; filter-first, window-second is enforced by the caller)
// ===========================================================================

#[derive(Debug, Clone)]
pub enum Window {
    All,
    First(usize),
    Range(i64, i64),
}

fn parse_window(input: &str) -> Result<Window, String> {
    let t = input.trim();
    if t == "all" {
        return Ok(Window::All);
    }
    if let Some(rest) = t.strip_prefix("first:") {
        let n: usize = rest
            .trim()
            .parse()
            .map_err(|_| format!("--window first: expects an integer, got {rest:?}"))?;
        return Ok(Window::First(n));
    }
    if let Some(rest) = t.strip_prefix("range:") {
        let (a, b) = rest
            .split_once(':')
            .ok_or_else(|| format!("--window range: expects range:<start>:<end>, got {rest:?}"))?;
        let start = parse_date_bound(a.trim(), false)
            .ok_or_else(|| format!("--window range start unparseable: {a:?}"))?;
        let end = parse_date_bound(b.trim(), true)
            .ok_or_else(|| format!("--window range end unparseable: {b:?}"))?;
        return Ok(Window::Range(start, end));
    }
    Err(format!(
        "--window must be all|first:<n>|range:<start>:<end>, got {t:?}"
    ))
}

/// Parse a `--window range` bound. Date-only `is_end` bounds resolve to the
/// last second of the day (inclusive).
fn parse_date_bound(s: &str, is_end: bool) -> Option<i64> {
    let base = parse_rfc3339_seconds(s)?;
    // If the string is date-only (length 10, no time component), an end
    // bound is inclusive through 23:59:59.
    if is_end && s.len() == 10 {
        return Some(base + 86_399);
    }
    Some(base)
}

/// Apply the window to an already-time-ordered slice (Amendment 12: the
/// caller filters first, then windows).
fn apply_window(records: Vec<BetRecord>, window: &Window) -> Vec<BetRecord> {
    match window {
        Window::All => records,
        Window::First(n) => records.into_iter().take(*n).collect(),
        Window::Range(a, b) => records
            .into_iter()
            .filter(|r| r.ts_key >= *a && r.ts_key <= *b)
            .collect(),
    }
}
