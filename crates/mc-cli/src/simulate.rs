//! `mc model simulate` — chronological bankroll replay (Phase 10F, ADR-0035).
//!
//! Consumes a **bet-record file** (NOT the cube), sizes each bet via a
//! closed Kelly vocabulary, walks the bankroll forward in time order, and
//! computes `final_bank`/`roi`/`max_drawdown`/`recovery_bets`/`sharpe`,
//! plus optional Monte Carlo percentile bands. This is the first `mc model`
//! verb whose cartridge argument is optional — the simulation needs only
//! the records; the cartridge, when present, is a column-name validator /
//! provenance source (Decision 1, Amendment 10/11).
//!
//! Per ADR-0035 Decision 8 + Amendment 16, the entire command lives in
//! `mc-cli`. There is **zero `mc-core` change** — bankroll replay is
//! reporting logic, not kernel logic. The PRNG (Amendment 5) is hand-rolled
//! here; no `rand` crate. Parquet input reuses the existing DuckDB path
//! through `mc-drivers` (Amendment 4); curve output is jsonl in v1.
//!
//! Binding amendments folded in (see ADR-0035 "Acceptance amendments"):
//! - A1: same-timestamp bets = simultaneous batch by default.
//! - A2: 4-state outcome required; binary hard-errors unless
//!   `--outcome-mode legacy-binary`; `--derive-pushes` repair.
//! - A3: bankruptcy/ruin — cap stake at bankroll, never negative, ruin
//!   skips remaining; batch over-stake scales proportionally.
//! - A5: pinned splitmix64 PRNG.
//! - A6: iid/block bootstrap, sample length = path length, nearest-rank.
//! - A7: metric edge cases (recovery null+status, sharpe null on n<2/0-std).
//! - A8: `stake_hint` only via explicit `--sizing from_column:stake_hint`.
//! - A9: `--odds` applies to BOTH sizing and settlement.
//! - A12: filter first, window second.
//! - A13: `roi` = cumulative; `roi_per_bet` separate.
//! - A14: curve invariants.
//! - A16: expanded JSON.
//! - A17: `--replay batch|sequential` (default batch); sequential =
//!   stable-sort timestamp, compound in `sequence`-col-or-file order.

use crate::loader::{load_model_with_policy, LoadPolicy};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// Float-zero threshold (CLAUDE.md §7 / §3.1). Used for div-by-zero and
/// zero-stddev guards — never a bare `== 0.0`.
const ZERO_EPS: f64 = 1e-300;

/// Output schema version for the JSON envelope (matches the Phase 3B-style
/// `schema_version` contract that downstream codegen pins).
const SIMULATE_SCHEMA_VERSION: &str = "1.0";

// ===========================================================================
// Record model
// ===========================================================================

/// A single raw cell value read from the bet-record file. `Null` is a
/// distinct state, never conflated with `0.0` (CLAUDE.md §2.5).
#[derive(Debug, Clone)]
pub enum RecordValue {
    Num(f64),
    Text(String),
    Null,
}

impl RecordValue {
    fn as_num(&self) -> Option<f64> {
        match self {
            RecordValue::Num(n) => Some(*n),
            _ => None,
        }
    }
    fn as_text(&self) -> Option<&str> {
        match self {
            RecordValue::Text(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

/// A column-name-keyed raw table, preserving original file row order.
#[derive(Debug, Default)]
pub struct RawTable {
    /// Column names in source order.
    pub columns: Vec<String>,
    /// One row per record; each is a map from source column name to value.
    pub rows: Vec<BTreeMap<String, RecordValue>>,
}

/// The 4-state outcome enum (Decision 3 / Amendment 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Win,
    Loss,
    Push,
    Void,
}

/// A normalized, replay-ready bet record.
#[derive(Debug, Clone)]
pub struct BetRecord {
    /// Original read order — the stable tiebreak for sequential replay (A17).
    pub file_index: usize,
    pub bet_id: String,
    /// Numeric sort key derived from the timestamp (epoch seconds).
    pub ts_key: i64,
    pub timestamp: String,
    pub p_bet_side: f64,
    pub decimal_odds: f64,
    pub outcome: Outcome,
    /// Optional intra-timestamp ordering (the `sequence` column, A1/A17).
    pub sequence: Option<f64>,
    /// All source columns, for `--filter` evaluation over flat record columns.
    pub cells: BTreeMap<String, RecordValue>,
}

impl BetRecord {
    fn num(&self, col: &str) -> Option<f64> {
        self.cells.get(col).and_then(RecordValue::as_num)
    }
}

// ===========================================================================
// Timestamp parsing (no chrono dependency)
// ===========================================================================

/// Parse an RFC3339 / ISO-8601 timestamp or a bare epoch number into a
/// comparable epoch-seconds key. Returns `None` if unparseable.
///
/// Accepts `YYYY-MM-DD[T ]hh:mm[:ss[.fff]][Z|±hh:mm]` and bare integers/
/// floats (treated as already-epoch, used verbatim as the key). Sub-second
/// precision is truncated for the sort key; offsets are applied to UTC.
fn parse_timestamp_key(raw: &RecordValue) -> Option<i64> {
    match raw {
        RecordValue::Num(n) => Some(*n as i64),
        RecordValue::Text(s) => parse_rfc3339_seconds(s),
        RecordValue::Null => None,
    }
}

/// Days from the civil 1970-01-01 epoch (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Parse an RFC3339-ish string to epoch seconds. Date-only strings resolve
/// to 00:00:00 UTC. Used both for record ordering and `--window` bounds.
fn parse_rfc3339_seconds(s: &str) -> Option<i64> {
    let s = s.trim();
    // Bare epoch number?
    if let Ok(n) = s.parse::<i64>() {
        // Heuristic: a 4-digit "year-like" value is almost certainly a
        // mis-typed date, but we honor explicit numeric epochs as-is.
        return Some(n);
    }
    let bytes = s.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    if bytes[4] != b'-' {
        return None;
    }
    let month: i64 = s.get(5..7)?.parse().ok()?;
    if bytes[7] != b'-' {
        return None;
    }
    let day: i64 = s.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let mut secs = days_from_civil(year, month, day) * 86_400;

    // Optional time component after a 'T' or space separator.
    if bytes.len() > 10 {
        let sep = bytes[10];
        if sep != b'T' && sep != b't' && sep != b' ' {
            return None;
        }
        let rest = &s[11..];
        // Split off any zone suffix.
        let (time_part, zone) = split_zone(rest);
        let mut it = time_part.split(':');
        let hh: i64 = it.next()?.parse().ok()?;
        let mm: i64 = it.next().unwrap_or("0").parse().ok()?;
        let ss: i64 = it
            .next()
            .map(|t| t.split('.').next().unwrap_or("0"))
            .unwrap_or("0")
            .parse()
            .ok()?;
        secs += hh * 3600 + mm * 60 + ss;
        secs -= zone; // subtract the offset to normalize to UTC
    }
    Some(secs)
}

/// Split a time string's trailing zone designator, returning
/// `(time_without_zone, offset_seconds)`. `Z` / no zone → 0.
fn split_zone(rest: &str) -> (&str, i64) {
    if let Some(stripped) = rest.strip_suffix('Z').or_else(|| rest.strip_suffix('z')) {
        return (stripped, 0);
    }
    // Look for a +hh:mm / -hh:mm suffix (but not the date-internal '-').
    let bytes = rest.as_bytes();
    for i in (1..bytes.len()).rev() {
        if bytes[i] == b'+' || bytes[i] == b'-' {
            let zone = &rest[i..];
            let sign = if bytes[i] == b'+' { 1 } else { -1 };
            let z = &zone[1..];
            let mut parts = z.split(':');
            if let (Some(h), m) = (parts.next(), parts.next()) {
                if let Ok(hh) = h.parse::<i64>() {
                    let mm = m.and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
                    return (&rest[..i], sign * (hh * 3600 + mm * 60));
                }
            }
        }
    }
    (rest, 0)
}

// ===========================================================================
// Sizing vocabulary (Decision 4, Amendment 8/9)
// ===========================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizingKind {
    /// `flat:pct=X` — X × start_bankroll (constant).
    Flat,
    /// `flat_current:pct=X` — X × current(batch-start) bankroll.
    FlatCurrent,
    /// `kelly:fraction=F` — F × kelly_fraction × current bankroll.
    Kelly,
    /// `from_column:<col>` — pre-computed stake read verbatim from a column.
    FromColumn,
}

/// A fully-parsed sizing rule with universal modifiers.
#[derive(Debug, Clone)]
pub struct SizingRule {
    pub kind: SizingKind,
    /// `pct` for flat/flat_current; `fraction` for kelly.
    pub param: f64,
    /// Column name for `from_column`.
    pub column: Option<String>,
    /// `cap=X` — stake_pct capped at X (fraction of the sizing basis).
    pub cap: Option<f64>,
    /// `shrink=X` — subtract X from `p` before Kelly (CI haircut).
    pub shrink: f64,
    /// `min_odds=X` — skip bets below X decimal odds.
    pub min_odds: Option<f64>,
    /// `floor=X` — minimum stake fraction (of basis) below which skip.
    pub floor: Option<f64>,
}

/// Pinned Kelly fraction: `b = d − 1`, `f = (b·p − (1−p)) / b`, clamped to
/// `[0, ∞)`. Returns 0.0 for non-positive-edge bets (caller skips).
fn kelly_fraction(p: f64, decimal_odds: f64) -> f64 {
    let b = decimal_odds - 1.0;
    if b.abs() < ZERO_EPS {
        return 0.0;
    }
    let f = (b * p - (1.0 - p)) / b;
    if f < 0.0 {
        0.0
    } else {
        f
    }
}

/// Why a candidate bet was not placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    BelowMinOdds,
    NegativeKelly,
    BelowFloor,
    RuinSkipped,
}

impl SkipReason {
    fn key(self) -> &'static str {
        match self {
            SkipReason::BelowMinOdds => "below_min_odds",
            SkipReason::NegativeKelly => "negative_kelly",
            SkipReason::BelowFloor => "below_floor",
            SkipReason::RuinSkipped => "ruin_skipped",
        }
    }
}

/// The result of sizing a single bet against a basis bankroll.
enum SizeOutcome {
    /// Place with this stake (already capped to `[0, basis]`).
    Stake(f64),
    /// Do not place; record the reason.
    Skip(SkipReason),
}

impl SizingRule {
    /// Compute the stake for one bet. `basis` is the bankroll the sizing
    /// percentages apply to (start bankroll for `flat`, batch-start
    /// bankroll otherwise). `odds` is the resolved decimal odds (A9).
    fn size(&self, rec: &BetRecord, odds: f64, start_bankroll: f64, basis: f64) -> SizeOutcome {
        if let Some(min_odds) = self.min_odds {
            if odds < min_odds {
                return SizeOutcome::Skip(SkipReason::BelowMinOdds);
            }
        }
        let pct = match self.kind {
            SizingKind::Flat => self.param * start_bankroll / basis.max(ZERO_EPS),
            SizingKind::FlatCurrent => self.param,
            SizingKind::Kelly => {
                let p = rec.p_bet_side - self.shrink;
                let kf = kelly_fraction(p, odds);
                if kf.abs() < ZERO_EPS {
                    return SizeOutcome::Skip(SkipReason::NegativeKelly);
                }
                self.param * kf
            }
            SizingKind::FromColumn => {
                let col = self.column.as_deref().unwrap_or("stake_hint");
                match rec.num(col) {
                    Some(s) if s > 0.0 => s / basis.max(ZERO_EPS),
                    _ => return SizeOutcome::Skip(SkipReason::BelowFloor),
                }
            }
        };
        let mut pct = pct;
        if let Some(cap) = self.cap {
            if pct > cap {
                pct = cap;
            }
        }
        if let Some(floor) = self.floor {
            if pct < floor {
                return SizeOutcome::Skip(SkipReason::BelowFloor);
            }
        }
        if pct <= 0.0 {
            return SizeOutcome::Skip(SkipReason::NegativeKelly);
        }
        let mut stake = pct * basis;
        // A3: never stake more than the bankroll on hand (per-bet cap).
        if stake > basis {
            stake = basis;
        }
        SizeOutcome::Stake(stake)
    }
}

/// Parse a `--sizing` string: `rule[:param=value,...]`.
pub fn parse_sizing(input: &str) -> Result<SizingRule, String> {
    let trimmed = input.trim();
    let (head, params_str) = match trimmed.split_once(':') {
        Some((h, p)) => (h.trim(), p.trim()),
        None => (trimmed, ""),
    };

    let mut rule = SizingRule {
        kind: SizingKind::Kelly,
        param: 0.0,
        column: None,
        cap: None,
        shrink: 0.0,
        min_odds: None,
        floor: None,
    };

    match head {
        "flat" => rule.kind = SizingKind::Flat,
        "flat_current" => rule.kind = SizingKind::FlatCurrent,
        "kelly" => rule.kind = SizingKind::Kelly,
        "quarter_kelly" => {
            rule.kind = SizingKind::Kelly;
            rule.param = 0.25;
        }
        "half_kelly" => {
            rule.kind = SizingKind::Kelly;
            rule.param = 0.5;
        }
        "from_column" => rule.kind = SizingKind::FromColumn,
        other => {
            return Err(format!(
                "unknown sizing rule {other:?}; valid: flat, flat_current, kelly, \
                 quarter_kelly, half_kelly, from_column"
            ));
        }
    }

    // `from_column:stake_hint` — the param segment is the column name.
    if rule.kind == SizingKind::FromColumn {
        if params_str.is_empty() {
            return Err("from_column requires a column name (e.g. from_column:stake_hint)".into());
        }
        // Allow `from_column:stake_hint` and modifiers after a comma.
        let mut parts = params_str.splitn(2, ',');
        let col = parts.next().unwrap_or("").trim();
        if col.is_empty() || col.contains('=') {
            return Err(
                "from_column requires a bare column name first (from_column:stake_hint)".into(),
            );
        }
        rule.column = Some(col.to_string());
        let rest = parts.next().unwrap_or("");
        apply_modifiers(&mut rule, rest)?;
        return Ok(rule);
    }

    apply_modifiers(&mut rule, params_str)?;

    // shorthands already set param; explicit kelly/flat needs its param.
    match rule.kind {
        SizingKind::Flat | SizingKind::FlatCurrent => {
            if rule.param.abs() < ZERO_EPS {
                return Err(format!("{head} requires pct= (e.g. {head}:pct=0.02)"));
            }
        }
        SizingKind::Kelly => {
            if rule.param.abs() < ZERO_EPS && head == "kelly" {
                return Err("kelly requires fraction= (e.g. kelly:fraction=0.25)".into());
            }
        }
        _ => {}
    }
    Ok(rule)
}

/// Apply `key=value` modifier pairs (and the rule's primary param) to a rule.
fn apply_modifiers(rule: &mut SizingRule, params_str: &str) -> Result<(), String> {
    if params_str.trim().is_empty() {
        return Ok(());
    }
    for pair in params_str.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair
            .split_once('=')
            .ok_or_else(|| format!("sizing param {pair:?} must be key=value"))?;
        let k = k.trim();
        let v: f64 = v
            .trim()
            .parse()
            .map_err(|_| format!("sizing param {k}={:?}: not a number", v.trim()))?;
        match k {
            "pct" => rule.param = v,
            "fraction" => rule.param = v,
            "cap" => rule.cap = Some(v),
            "shrink" => rule.shrink = v,
            "min_odds" => rule.min_odds = Some(v),
            "floor" => rule.floor = Some(v),
            other => {
                return Err(format!(
                    "unknown sizing param {other:?}; valid: pct, fraction, cap, shrink, \
                     min_odds, floor"
                ));
            }
        }
    }
    Ok(())
}

// ===========================================================================
// Flat-record filter grammar (Amendment 12; same syntax as `query --where`,
// separate parser over flat record columns rather than cube measures)
// ===========================================================================

#[derive(Debug)]
pub enum Filter {
    And(Box<Filter>, Box<Filter>),
    Or(Box<Filter>, Box<Filter>),
    Not(Box<Filter>),
    Cmp(String, CmpOp, FilterVal),
}

#[derive(Debug, Clone, Copy)]
pub enum CmpOp {
    Eq,
    Neq,
    Gt,
    Lt,
    Gte,
    Lte,
}

#[derive(Debug, Clone)]
pub enum FilterVal {
    Num(f64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Num(f64),
    Str(String),
    Op(CmpOpTok),
    And,
    Or,
    Not,
    LParen,
    RParen,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CmpOpTok {
    Eq,
    Neq,
    Gt,
    Lt,
    Gte,
    Lte,
}

fn tokenize_filter(input: &str) -> Result<Vec<Tok>, String> {
    let mut toks = Vec::new();
    let b = input.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '(' => {
                toks.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                toks.push(Tok::RParen);
                i += 1;
            }
            '"' | '\'' => {
                let quote = c;
                let start = i + 1;
                let mut j = start;
                while j < b.len() && b[j] as char != quote {
                    j += 1;
                }
                if j >= b.len() {
                    return Err(format!("unterminated string literal in filter: {input:?}"));
                }
                toks.push(Tok::Str(input[start..j].to_string()));
                i = j + 1;
            }
            '>' | '<' | '=' | '!' => {
                let two = if i + 1 < b.len() {
                    &input[i..i + 2]
                } else {
                    ""
                };
                let (op, len) = match two {
                    ">=" => (CmpOpTok::Gte, 2),
                    "<=" => (CmpOpTok::Lte, 2),
                    "==" => (CmpOpTok::Eq, 2),
                    "!=" => (CmpOpTok::Neq, 2),
                    _ => match c {
                        '>' => (CmpOpTok::Gt, 1),
                        '<' => (CmpOpTok::Lt, 1),
                        '=' => (CmpOpTok::Eq, 1),
                        _ => return Err(format!("unexpected '!' in filter: {input:?}")),
                    },
                };
                toks.push(Tok::Op(op));
                i += len;
            }
            _ => {
                // Identifier, number, or keyword (AND/OR/NOT).
                let start = i;
                while i < b.len() {
                    let ch = b[i] as char;
                    if ch.is_alphanumeric() || ch == '_' || ch == '.' || ch == '-' || ch == ':' {
                        i += 1;
                    } else {
                        break;
                    }
                }
                let word = &input[start..i];
                if word.is_empty() {
                    return Err(format!("unexpected character {c:?} in filter: {input:?}"));
                }
                match word.to_ascii_uppercase().as_str() {
                    "AND" => toks.push(Tok::And),
                    "OR" => toks.push(Tok::Or),
                    "NOT" => toks.push(Tok::Not),
                    _ => {
                        if let Ok(n) = word.parse::<f64>() {
                            toks.push(Tok::Num(n));
                        } else {
                            toks.push(Tok::Ident(word.to_string()));
                        }
                    }
                }
            }
        }
    }
    Ok(toks)
}

struct FilterParser {
    toks: Vec<Tok>,
    pos: usize,
}

impl FilterParser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }
    fn parse_or(&mut self) -> Result<Filter, String> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), Some(Tok::Or)) {
            self.pos += 1;
            let right = self.parse_and()?;
            left = Filter::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }
    fn parse_and(&mut self) -> Result<Filter, String> {
        let mut left = self.parse_unary()?;
        while matches!(self.peek(), Some(Tok::And)) {
            self.pos += 1;
            let right = self.parse_unary()?;
            left = Filter::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }
    fn parse_unary(&mut self) -> Result<Filter, String> {
        if matches!(self.peek(), Some(Tok::Not)) {
            self.pos += 1;
            let inner = self.parse_unary()?;
            return Ok(Filter::Not(Box::new(inner)));
        }
        if matches!(self.peek(), Some(Tok::LParen)) {
            self.pos += 1;
            let inner = self.parse_or()?;
            match self.next() {
                Some(Tok::RParen) => return Ok(inner),
                _ => return Err("expected ')' in filter".into()),
            }
        }
        self.parse_cmp()
    }
    fn parse_cmp(&mut self) -> Result<Filter, String> {
        let col = match self.next() {
            Some(Tok::Ident(s)) => s,
            other => return Err(format!("expected a column name in filter, got {other:?}")),
        };
        let op = match self.next() {
            Some(Tok::Op(o)) => match o {
                CmpOpTok::Eq => CmpOp::Eq,
                CmpOpTok::Neq => CmpOp::Neq,
                CmpOpTok::Gt => CmpOp::Gt,
                CmpOpTok::Lt => CmpOp::Lt,
                CmpOpTok::Gte => CmpOp::Gte,
                CmpOpTok::Lte => CmpOp::Lte,
            },
            other => {
                return Err(format!(
                    "expected a comparison operator in filter, got {other:?}"
                ))
            }
        };
        let val = match self.next() {
            Some(Tok::Num(n)) => FilterVal::Num(n),
            Some(Tok::Str(s)) => FilterVal::Text(s),
            Some(Tok::Ident(s)) => FilterVal::Text(s),
            other => return Err(format!("expected a value in filter, got {other:?}")),
        };
        Ok(Filter::Cmp(col, op, val))
    }
}

fn parse_filter(input: &str) -> Result<Filter, String> {
    let toks = tokenize_filter(input)?;
    if toks.is_empty() {
        return Err("empty filter expression".into());
    }
    let mut p = FilterParser { toks, pos: 0 };
    let f = p.parse_or()?;
    if p.pos != p.toks.len() {
        return Err(format!("trailing tokens in filter: {input:?}"));
    }
    Ok(f)
}

fn eval_filter(f: &Filter, rec: &BetRecord) -> bool {
    match f {
        Filter::And(a, b) => eval_filter(a, rec) && eval_filter(b, rec),
        Filter::Or(a, b) => eval_filter(a, rec) || eval_filter(b, rec),
        Filter::Not(a) => !eval_filter(a, rec),
        Filter::Cmp(col, op, val) => {
            let cell = rec.cells.get(col);
            match (cell, val) {
                (Some(RecordValue::Num(n)), FilterVal::Num(target)) => cmp_num(*n, *op, *target),
                (Some(RecordValue::Num(n)), FilterVal::Text(t)) => {
                    // numeric column compared to a bare/quoted number-like token
                    match t.parse::<f64>() {
                        Ok(target) => cmp_num(*n, *op, target),
                        Err(_) => false,
                    }
                }
                (Some(RecordValue::Text(s)), FilterVal::Text(t)) => cmp_text(s, *op, t),
                (Some(RecordValue::Text(s)), FilterVal::Num(t)) => {
                    cmp_text(s, *op, &format_num(*t))
                }
                _ => false,
            }
        }
    }
}

fn cmp_num(a: f64, op: CmpOp, b: f64) -> bool {
    match op {
        // Float equality via epsilon (CLAUDE.md §3.1 — never bare ==).
        CmpOp::Eq => (a - b).abs() < 1e-9,
        CmpOp::Neq => (a - b).abs() >= 1e-9,
        CmpOp::Gt => a > b,
        CmpOp::Lt => a < b,
        CmpOp::Gte => a >= b,
        CmpOp::Lte => a <= b,
    }
}

fn cmp_text(a: &str, op: CmpOp, b: &str) -> bool {
    match op {
        CmpOp::Eq => a == b,
        CmpOp::Neq => a != b,
        CmpOp::Gt => a > b,
        CmpOp::Lt => a < b,
        CmpOp::Gte => a >= b,
        CmpOp::Lte => a <= b,
    }
}

fn format_num(n: f64) -> String {
    if n.fract().abs() < ZERO_EPS {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

// ===========================================================================
// Pinned PRNG — splitmix64 (Amendment 5; no `rand` crate)
// ===========================================================================

/// splitmix64 — a tiny, self-contained, platform-independent PRNG. Same
/// seed → byte-identical sequence on every platform (Amendment 5 / AC #26).
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        SplitMix64 { state: seed }
    }
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform index in `[0, n)`. `n` must be > 0.
    fn index(&mut self, n: usize) -> usize {
        (self.next_u64() % (n as u64)) as usize
    }
}

// ===========================================================================
// The replay engine (Decision 5; Amendments 1, 3, 12, 17)
// ===========================================================================

/// Same-timestamp replay discipline (Amendment 17).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayMode {
    /// A1 simultaneous-batch — the realistic default.
    Batch,
    /// Sequential per-bet compounding (legacy-headline reproduction).
    Sequential,
}

/// One emitted curve row (Amendment 14).
#[derive(Debug, Clone)]
pub struct CurveRow {
    pub timestamp: String,
    pub bet_id: String,
    pub season: Option<f64>,
    pub side: Option<String>,
    pub p_bet_side: f64,
    pub abs_edge_pp: Option<f64>,
    pub stake: f64,
    pub outcome: Outcome,
    pub bankroll_after: f64,
    pub batch_id: usize,
}

/// The accumulated outcome of a single replay path.
#[derive(Debug, Clone)]
pub struct ReplayResult {
    pub start_bankroll: f64,
    pub final_bank: f64,
    pub total_staked: f64,
    pub total_pnl: f64,
    pub wins: usize,
    pub losses: usize,
    pub pushes: usize,
    pub voids: usize,
    pub n_bets: usize,
    pub ruin: bool,
    pub ruin_index: Option<usize>,
    pub skip_counts: BTreeMap<String, usize>,
    /// Per-placed-bet returns (pnl/stake), for Sharpe.
    pub per_bet_returns: Vec<f64>,
    /// Bankroll after each placed bet, in curve order (for drawdown scans).
    pub bankroll_path: Vec<f64>,
    pub curve: Vec<CurveRow>,
}

fn resolve_odds(rec: &BetRecord, odds_override: &OddsSource) -> Option<f64> {
    match odds_override {
        OddsSource::Record => Some(rec.decimal_odds),
        OddsSource::Fixed(d) => Some(*d),
        OddsSource::Column(c) => rec.num(c),
    }
}

/// Apply one placed bet's outcome to the bankroll, returning (pnl, new_bank).
fn apply_outcome(outcome: Outcome, stake: f64, odds: f64, bank: f64) -> (f64, f64) {
    match outcome {
        Outcome::Win => {
            let pnl = stake * (odds - 1.0);
            (pnl, bank + pnl)
        }
        Outcome::Loss => (-stake, bank - stake),
        Outcome::Push => (0.0, bank),
        // Void is never placed; callers exclude it before reaching here.
        Outcome::Void => (0.0, bank),
    }
}

/// Replay the ordered, filtered, windowed records into a single path.
///
/// `ordered` must already be in replay order (batch: sorted by
/// `(ts_key, bet_id)`; sequential: stable file/sequence order). Grouping
/// into same-timestamp batches happens here for batch mode.
fn replay(
    ordered: &[BetRecord],
    start_bankroll: f64,
    sizing: &SizingRule,
    odds_src: &OddsSource,
    mode: ReplayMode,
    max_stake: Option<f64>,
) -> ReplayResult {
    let mut res = ReplayResult {
        start_bankroll,
        final_bank: start_bankroll,
        total_staked: 0.0,
        total_pnl: 0.0,
        wins: 0,
        losses: 0,
        pushes: 0,
        voids: 0,
        n_bets: 0,
        ruin: false,
        ruin_index: None,
        skip_counts: BTreeMap::new(),
        per_bet_returns: Vec::new(),
        bankroll_path: Vec::new(),
        curve: Vec::new(),
    };
    let mut bank = start_bankroll;
    let mut batch_id = 0usize;

    // Partition `ordered` into batches. In Sequential mode every bet is its
    // own batch (so it sizes off the running bankroll); in Batch mode
    // consecutive same-`ts_key` bets share a batch.
    let mut i = 0usize;
    'outer: while i < ordered.len() {
        let mut j = i + 1;
        if mode == ReplayMode::Batch {
            while j < ordered.len() && ordered[j].ts_key == ordered[i].ts_key {
                j += 1;
            }
        }
        let batch = &ordered[i..j];
        let basis = bank; // bankroll as of batch start (A1)

        // Size every bet in the batch against the batch-start bankroll.
        let mut staked: Vec<(usize, f64, f64)> = Vec::new(); // (idx, stake, odds)
        for (k, rec) in batch.iter().enumerate() {
            // A1/Decision 3: void bets are not placed — excluded from count,
            // P&L, and the curve entirely.
            if rec.outcome == Outcome::Void {
                res.voids += 1;
                continue;
            }
            let odds = match resolve_odds(rec, odds_src) {
                Some(o) if o > 1.0 => o,
                _ => {
                    // Unusable odds → treat as a skip (below_min_odds bucket).
                    *res.skip_counts
                        .entry("unusable_odds".to_string())
                        .or_insert(0) += 1;
                    continue;
                }
            };
            match sizing.size(rec, odds, start_bankroll, basis) {
                // A19: --max-stake is an absolute-dollar cap applied AFTER the
                // sizing rule + fractional cap= (which size() already applied).
                SizeOutcome::Stake(s) => {
                    let s = match max_stake {
                        Some(ms) => s.min(ms),
                        None => s,
                    };
                    staked.push((k, s, odds))
                }
                SizeOutcome::Skip(reason) => {
                    *res.skip_counts.entry(reason.key().to_string()).or_insert(0) += 1;
                }
            }
        }

        // A3: if the batch's total stake exceeds the batch-start bankroll,
        // scale all stakes down proportionally so the batch never stakes
        // more than what's on hand.
        let total_stake: f64 = staked.iter().map(|(_, s, _)| *s).sum();
        if total_stake > basis && total_stake > ZERO_EPS {
            let scale = basis / total_stake;
            for entry in &mut staked {
                entry.1 *= scale;
            }
        }

        // Apply outcomes. In batch mode the bankroll updates once (atomic);
        // intra-batch curve rows carry the batch-end bankroll (A14).
        for (k, stake, odds) in &staked {
            let rec = &batch[*k];
            let (pnl, new_bank) = apply_outcome(rec.outcome, *stake, *odds, bank);
            bank = new_bank;
            res.total_staked += *stake;
            res.total_pnl += pnl;
            match rec.outcome {
                Outcome::Win => res.wins += 1,
                Outcome::Loss => res.losses += 1,
                Outcome::Push => res.pushes += 1,
                Outcome::Void => {}
            }
            res.n_bets += 1;
            // Per-bet return basis = pnl / stake (A7).
            if *stake > ZERO_EPS {
                res.per_bet_returns.push(pnl / *stake);
            }
        }

        // Stamp curve rows for the batch (bankroll_after = batch-end bank).
        for (k, stake, _odds) in &staked {
            let rec = &batch[*k];
            res.curve.push(CurveRow {
                timestamp: rec.timestamp.clone(),
                bet_id: rec.bet_id.clone(),
                season: rec.num("season"),
                side: rec
                    .cells
                    .get("side")
                    .and_then(|v| v.as_text())
                    .map(str::to_string),
                p_bet_side: rec.p_bet_side,
                abs_edge_pp: rec.num("abs_edge_pp").or_else(|| rec.num("edge_pp")),
                stake: *stake,
                outcome: rec.outcome,
                bankroll_after: bank,
                batch_id,
            });
            res.bankroll_path.push(bank);
        }

        batch_id += 1;

        // A3 ruin check: bankroll ≤ 0 ends the replay; remaining bets are
        // skipped (not placed).
        if bank <= ZERO_EPS {
            res.ruin = true;
            res.ruin_index = Some(res.n_bets.saturating_sub(1));
            bank = bank.max(0.0);
            // Count the remaining un-placed candidate bets as ruin-skipped.
            let remaining: usize = ordered[j..].len();
            if remaining > 0 {
                *res.skip_counts
                    .entry(SkipReason::RuinSkipped.key().to_string())
                    .or_insert(0) += remaining;
            }
            break 'outer;
        }

        i = j;
    }

    res.final_bank = bank;
    res
}

// ===========================================================================
// Odds source (Amendment 9)
// ===========================================================================

#[derive(Debug, Clone)]
pub enum OddsSource {
    /// Use each record's `decimal_odds`.
    Record,
    /// `--odds fixed:<d>` — same odds for every bet.
    Fixed(f64),
    /// `--odds column:<name>` — read odds from a named column.
    Column(String),
}

fn parse_odds(input: &str) -> Result<OddsSource, String> {
    let trimmed = input.trim();
    if let Some(rest) = trimmed.strip_prefix("fixed:") {
        let d: f64 = rest
            .trim()
            .parse()
            .map_err(|_| format!("--odds fixed: expects a number, got {rest:?}"))?;
        if d <= 1.0 {
            return Err(format!("--odds fixed:{d} must be > 1.0"));
        }
        Ok(OddsSource::Fixed(d))
    } else if let Some(rest) = trimmed.strip_prefix("column:") {
        let c = rest.trim();
        if c.is_empty() {
            return Err("--odds column: requires a column name".into());
        }
        Ok(OddsSource::Column(c.to_string()))
    } else {
        Err(format!(
            "--odds must be 'fixed:<decimal>' or 'column:<name>', got {trimmed:?}"
        ))
    }
}

// (Reader, outcome normalization, metrics, monte carlo, window, command +
//  parse + run live in the sections below.)

include!("simulate_reader.rs");
include!("simulate_metrics.rs");
include!("simulate_command.rs");

#[cfg(test)]
mod tests {
    include!("simulate_tests.rs");
}
