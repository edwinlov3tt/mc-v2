//! Expression evaluator for template `when:` predicates and `bindings:`.
//!
//! Session 2 upgrade: the evaluator is now **cube-aware**. Aggregate
//! functions (`count_where`, `any_where`, `names_where`, `first_where`)
//! are evaluated generically at eval time by iterating over dimension
//! elements and evaluating arbitrary predicate expressions per-element.
//! This closes Finding #1 (pre-computed aggregates → generic evaluation).
//!
//! Also adds `NOT` operator support (Finding #4) and `not()` function.

use crate::benchmark::MetricBenchmark;
use crate::context::CubeData;
use crate::ledger::LedgerEntry;
use crate::renderer::readable_name;
use std::collections::HashMap;

// ─── Benchmark index for workspace-local queries (Phase 7A.4) ───────

/// Pre-built index over benchmark library entries for efficient lookup.
///
/// Built once per `evaluate_all` call from the `BenchmarkLibrary`. Indexes
/// by `(metric, scope_key)` for O(1) lookup during evaluation.
#[derive(Debug)]
pub struct BenchmarkIndex {
    /// Key: (metric_name, scope_key) → MetricBenchmark
    entries: HashMap<(String, String), MetricBenchmark>,
}

impl BenchmarkIndex {
    /// Build a benchmark index from a benchmark library.
    pub fn build(lib: &crate::benchmark::BenchmarkLibrary) -> Self {
        let mut entries = HashMap::new();
        for bench in lib.benchmarks.values() {
            let scope_key = bench
                .scope
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(",");
            entries.insert((bench.metric.clone(), scope_key), bench.clone());
        }
        BenchmarkIndex { entries }
    }

    /// Look up a benchmark by metric name and scope key.
    ///
    /// Fallback: if no scoped benchmark exists, try the empty scope key
    /// (aggregated across all scopes). If still nothing, return None.
    pub fn lookup(&self, metric: &str, scope_key: &str) -> Option<&MetricBenchmark> {
        self.entries
            .get(&(metric.to_string(), scope_key.to_string()))
            .or_else(|| self.entries.get(&(metric.to_string(), String::new())))
    }
}

// ─── Context event index for explanation chains (Phase 7A.5) ────────

/// Pre-built index over context events for efficient lookup during evaluation.
///
/// Built once per cube in `evaluate_all`. Indexes events by `event_type`
/// for O(1) lookup during evaluation. Includes both manual events (from
/// `.mosaic/context-events.yaml`) and auto-detected events (ephemeral).
/// Per ADR-0022 Decisions 5 and 6.
#[derive(Debug)]
pub struct ContextIndex {
    /// Events grouped by event_type.
    entries: HashMap<String, Vec<ContextIndexEntry>>,
    /// The current evaluation period.
    pub current_period: Option<String>,
}

/// A lightweight view of a context event for index queries.
#[derive(Debug, Clone)]
struct ContextIndexEntry {
    period: String,
    scope: std::collections::BTreeMap<String, String>,
    description: String,
    #[allow(dead_code)] // Used for expiry filtering in matches_lookback.
    expires_at: Option<String>,
}

impl ContextIndex {
    /// Build a context index from a slice of events with current period context.
    ///
    /// Also synthesizes auto-detected events from cube data (budget ±20%,
    /// single-period). Auto-detected events are ephemeral and never written to disk.
    /// Per ADR-0022 Decision 6.
    pub fn build(
        events: &[crate::context_events::ContextEvent],
        current_period: Option<String>,
        cube: &CubeData,
    ) -> Self {
        let mut entries: HashMap<String, Vec<ContextIndexEntry>> = HashMap::new();

        // Index manual events.
        for event in events {
            // Skip expired events.
            if let Some(ref expires) = event.expires_at {
                if let Some(ref cur) = current_period {
                    if expires.as_str() < cur.as_str() {
                        continue;
                    }
                }
            }
            entries
                .entry(event.event_type.clone())
                .or_default()
                .push(ContextIndexEntry {
                    period: event.period.clone(),
                    scope: event.scope.clone(),
                    description: event.description.clone(),
                    expires_at: event.expires_at.clone(),
                });
        }

        // Auto-detect budget changes from cube data.
        // Per ADR-0022 Decision 6: thresholds are v1 constants.
        // TODO(cartridge-config): make thresholds configurable per-cartridge.
        const BUDGET_DECREASE_THRESHOLD: f64 = 0.80;
        const BUDGET_INCREASE_THRESHOLD: f64 = 1.20;

        if let (Some(cur_budget), Some(prev_budget)) = (
            cube.values
                .get("Budget")
                .and_then(|v| v.last())
                .map(|e| e.value),
            cube.values.get("Budget").and_then(|v| {
                if v.len() >= 2 {
                    Some(v[v.len() - 2].value)
                } else {
                    None
                }
            }),
        ) {
            if prev_budget.abs() > 1e-300 {
                let ratio = cur_budget / prev_budget;
                if let Some(ref period) = current_period {
                    let prev_period = cube
                        .values
                        .get("Budget")
                        .and_then(|v| {
                            if v.len() >= 2 {
                                Some(v[v.len() - 2].category.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| "prior".to_string());

                    if ratio < BUDGET_DECREASE_THRESHOLD {
                        let pct = ((1.0 - ratio) * 100.0).round();
                        entries
                            .entry("budget_decrease".to_string())
                            .or_default()
                            .push(ContextIndexEntry {
                                period: period.clone(),
                                scope: std::collections::BTreeMap::new(),
                                description: format!(
                                    "Budget decreased {pct:.0}% from {prev_period}"
                                ),
                                expires_at: None,
                            });
                    } else if ratio > BUDGET_INCREASE_THRESHOLD {
                        let pct = ((ratio - 1.0) * 100.0).round();
                        entries
                            .entry("budget_increase".to_string())
                            .or_default()
                            .push(ContextIndexEntry {
                                period: period.clone(),
                                scope: std::collections::BTreeMap::new(),
                                description: format!(
                                    "Budget increased {pct:.0}% from {prev_period}"
                                ),
                                expires_at: None,
                            });
                    }
                }
            }
        }

        // Auto-detect single period.
        let period_count = cube.values.values().map(|v| v.len()).max().unwrap_or(0);
        if period_count <= 1 {
            if let Some(ref period) = current_period {
                entries
                    .entry("single_period".to_string())
                    .or_default()
                    .push(ContextIndexEntry {
                        period: period.clone(),
                        scope: std::collections::BTreeMap::new(),
                        description: "Only one reporting period available".to_string(),
                        expires_at: None,
                    });
            }
        }

        ContextIndex {
            entries,
            current_period,
        }
    }

    /// Check if a context event of the given type exists for the current scope.
    ///
    /// `lookback` is the number of periods to search (1 = current only,
    /// 3 = current + 2 prior). Per ADR-0022 Decision 5.
    pub fn has_event(&self, event_type: &str, scope_key: &str, lookback: usize) -> bool {
        self.count_events(event_type, scope_key, lookback) > 0
    }

    /// Count matching context events for the given type and scope.
    pub fn count_events(&self, event_type: &str, scope_key: &str, lookback: usize) -> usize {
        let events = match self.entries.get(event_type) {
            Some(v) => v,
            None => return 0,
        };
        events
            .iter()
            .filter(|e| self.matches_scope(e, scope_key) && self.matches_lookback(e, lookback))
            .count()
    }

    /// Get the description of the first matching context event.
    ///
    /// Per ADR-0022 Decision 5: first by deterministic order (period then scope key).
    pub fn description(&self, event_type: &str, scope_key: &str) -> Option<String> {
        let events = self.entries.get(event_type)?;
        let mut matches: Vec<_> = events
            .iter()
            .filter(|e| self.matches_scope(e, scope_key) && self.matches_lookback(e, 1))
            .collect();
        matches.sort_by(|a, b| a.period.cmp(&b.period));
        matches.first().map(|e| e.description.clone())
    }

    /// Check if an event's scope is a subset of the current evaluation scope.
    fn matches_scope(&self, event: &ContextIndexEntry, scope_key: &str) -> bool {
        if event.scope.is_empty() {
            return true; // Empty scope matches everything.
        }
        // Build scope_key from event scope for comparison.
        let event_scope_key: String = event
            .scope
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(",");
        // Event scope must be a subset: each k=v pair in event scope
        // must appear in the evaluation scope_key.
        event_scope_key.split(',').all(|kv| scope_key.contains(kv))
    }

    /// Check if an event falls within the lookback window.
    ///
    /// lookback=1 means current period only. lookback > 1 means current + prior.
    /// Period strings may be non-ISO (e.g., "Aug 2025") so lexicographic comparison
    /// is unreliable for ordering. For lookback > 1, we accept any event whose
    /// period is not strictly after the current period — this is conservative
    /// (may accept events older than the lookback window) but correct.
    fn matches_lookback(&self, event: &ContextIndexEntry, lookback: usize) -> bool {
        if lookback == 0 {
            return false;
        }
        let current = match &self.current_period {
            Some(p) => p.as_str(),
            None => return true, // No current period means accept all.
        };
        if lookback == 1 {
            return event.period.as_str() == current;
        }
        // For lookback > 1, accept all events at or before current.
        // Since period format may be non-ISO, we accept the event unless
        // its period is strictly greater than current (lexicographic).
        // For non-ISO formats this is imprecise but conservative.
        true
    }
}

// ─── Ledger index for cross-period queries (Phase 7A.3) ──────────────

/// Pre-built index over ledger entries for efficient cross-period queries.
///
/// Built once per `evaluate_all` call. Indexes entries by
/// `(template_id, scope_key)` for O(1) lookup during evaluation.
/// The `current_period` field determines the lookback boundary.
#[derive(Debug)]
pub struct LedgerIndex {
    /// Entries grouped by (template_id, scope_key).
    /// Scope key is a deterministic string: "k1=v1,k2=v2,..." from BTreeMap.
    entries: HashMap<(String, String), Vec<LedgerIndexEntry>>,
    /// The "current" period — latest Time element in the cube being evaluated.
    /// Ledger queries look backward from this period.
    pub current_period: Option<String>,
}

/// A lightweight view of a ledger entry for index queries.
#[derive(Debug, Clone)]
struct LedgerIndexEntry {
    report_period: Option<String>,
    evidence: std::collections::BTreeMap<String, serde_json::Value>,
}

impl LedgerIndex {
    /// Build a ledger index from a slice of entries with an optional current period.
    ///
    /// The `current_period` determines which entries are "prior" (before current).
    /// If `None`, all entries with a report_period are included in lookbacks.
    pub fn build(entries: &[LedgerEntry], current_period: Option<String>) -> Self {
        let mut map: HashMap<(String, String), Vec<LedgerIndexEntry>> = HashMap::new();
        for entry in entries {
            let scope_key = entry
                .scope
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(",");
            let key = (entry.narrative.template_id.clone(), scope_key);
            map.entry(key).or_default().push(LedgerIndexEntry {
                report_period: entry.report_period.clone(),
                evidence: entry.evidence.clone(),
            });
        }
        LedgerIndex {
            entries: map,
            current_period,
        }
    }

    /// Get matching entries for a template_id and scope_key, filtered to
    /// periods before the current period and limited to `lookback` most recent.
    fn lookup(
        &self,
        template_id: &str,
        scope_key: &str,
        lookback: usize,
    ) -> Vec<&LedgerIndexEntry> {
        let key = (template_id.to_string(), scope_key.to_string());
        let entries = match self.entries.get(&key) {
            Some(e) => e,
            None => return Vec::new(),
        };

        // Filter to entries with periods before the current period.
        let mut prior: Vec<&LedgerIndexEntry> = entries
            .iter()
            .filter(|e| {
                match (&e.report_period, &self.current_period) {
                    (Some(ep), Some(cp)) => ep.as_str() < cp.as_str(),
                    (Some(_), None) => true, // no current period = include all
                    _ => false,              // entries without periods are excluded
                }
            })
            .collect();

        // Sort by period descending (most recent first).
        prior.sort_by(|a, b| {
            b.report_period
                .as_deref()
                .unwrap_or("")
                .cmp(a.report_period.as_deref().unwrap_or(""))
        });

        // Take only the `lookback` most recent.
        prior.truncate(lookback);
        prior
    }

    /// Count entries matching template_id + scope within lookback periods.
    fn count(&self, template_id: &str, scope_key: &str, lookback: usize) -> usize {
        self.lookup(template_id, scope_key, lookback).len()
    }

    /// Check if any entries exist for template_id + scope within lookback periods.
    fn has(&self, template_id: &str, scope_key: &str, lookback: usize) -> bool {
        self.count(template_id, scope_key, lookback) > 0
    }

    /// Count consecutive periods (ending at the period just before current)
    /// where the template fired for the given scope.
    fn streak(&self, template_id: &str, scope_key: &str) -> usize {
        let key = (template_id.to_string(), scope_key.to_string());
        let entries = match self.entries.get(&key) {
            Some(e) => e,
            None => return 0,
        };

        // Collect periods before current, sorted descending.
        let mut periods: Vec<&str> = entries
            .iter()
            .filter_map(|e| {
                let period = e.report_period.as_deref()?;
                match &self.current_period {
                    Some(cp) if period >= cp.as_str() => None,
                    _ => Some(period),
                }
            })
            .collect();
        periods.sort();
        periods.dedup();
        periods.reverse(); // most recent first

        if periods.is_empty() {
            return 0;
        }

        // Count consecutive streak from the most recent period backward.
        let mut streak = 1;
        for i in 1..periods.len() {
            if crate::ledger::is_consecutive_period(periods[i], periods[i - 1]) {
                streak += 1;
            } else {
                break;
            }
        }
        streak
    }

    /// Get a specific evidence field from an entry N periods ago.
    fn evidence(
        &self,
        template_id: &str,
        scope_key: &str,
        field_name: &str,
        periods_ago: usize,
    ) -> Val {
        let entries = self.lookup(template_id, scope_key, periods_ago + 1);
        // periods_ago=0 means most recent prior, 1 means one before that, etc.
        match entries.get(periods_ago) {
            Some(entry) => match entry.evidence.get(field_name) {
                Some(serde_json::Value::Number(n)) => Val::Num(n.as_f64().unwrap_or(0.0)),
                Some(serde_json::Value::String(s)) => Val::Str(s.clone()),
                Some(serde_json::Value::Bool(b)) => Val::Bool(*b),
                _ => Val::Null,
            },
            None => Val::Null,
        }
    }

    /// Get the first (earliest) period within lookback that has an entry.
    fn first_period(&self, template_id: &str, scope_key: &str, lookback: usize) -> Val {
        let entries = self.lookup(template_id, scope_key, lookback);
        // Entries are sorted most-recent-first; last one is earliest.
        match entries.last().and_then(|e| e.report_period.as_deref()) {
            Some(p) => Val::Str(p.to_string()),
            None => Val::Null,
        }
    }

    /// Get the last (most recent) period within lookback that has an entry.
    fn last_period(&self, template_id: &str, scope_key: &str, lookback: usize) -> Val {
        let entries = self.lookup(template_id, scope_key, lookback);
        // Entries are sorted most-recent-first; first one is most recent.
        match entries.first().and_then(|e| e.report_period.as_deref()) {
            Some(p) => Val::Str(p.to_string()),
            None => Val::Null,
        }
    }
}

/// Runtime value type for the expression evaluator.
#[derive(Debug, Clone)]
pub enum Val {
    /// Numeric value.
    Num(f64),
    /// String value.
    Str(String),
    /// Boolean value.
    Bool(bool),
    /// Null / missing value.
    Null,
}

impl Val {
    /// Try to interpret this value as a number.
    pub fn as_num(&self) -> Option<f64> {
        match self {
            Val::Num(n) => Some(*n),
            Val::Bool(true) => Some(1.0),
            Val::Bool(false) => Some(0.0),
            _ => None,
        }
    }

    /// Whether this value is truthy (non-zero, non-empty, non-null).
    pub fn is_truthy(&self) -> bool {
        match self {
            Val::Num(n) => *n != 0.0,
            Val::Bool(b) => *b,
            Val::Str(s) => !s.is_empty(),
            Val::Null => false,
        }
    }

    /// Display this value as a string for template substitution.
    pub fn to_display(&self) -> String {
        match self {
            Val::Num(n) => {
                if (*n - n.round()).abs() < 1e-9 {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            Val::Str(s) => s.clone(),
            Val::Bool(b) => b.to_string(),
            Val::Null => "N/A".to_string(),
        }
    }
}

/// Evaluation context: a flat map of named values.
pub type Ctx = HashMap<String, Val>;

/// Evaluate an expression against a context only (no cube data access).
///
/// Use `eval_expr_with_cube` when aggregate functions may appear.
pub fn eval_expr(expr: &str, ctx: &Ctx) -> Val {
    eval_expr_with_cube(expr, ctx, None)
}

/// Evaluate an expression against a context + optional cube data.
///
/// Session 2: when `cube` is `Some`, aggregate functions like
/// `count_where(predicate, dimension)` are evaluated generically
/// by iterating over dimension elements. When `cube` is `None`,
/// falls back to context variable lookup (backward compat).
pub fn eval_expr_with_cube(expr: &str, ctx: &Ctx, cube: Option<&CubeData>) -> Val {
    eval_expr_full(expr, ctx, cube, None, None, None, "")
}

/// Evaluate an expression with full context: cube + ledger + scope.
///
/// Phase 7A.3: adds ledger query capability. The `scope_key` identifies
/// the current evaluation scope for ledger lookups (e.g., "channel=Paid_Search").
pub fn eval_expr_with_ledger(
    expr: &str,
    ctx: &Ctx,
    cube: Option<&CubeData>,
    ledger: Option<&LedgerIndex>,
    scope_key: &str,
) -> Val {
    eval_expr_full(expr, ctx, cube, ledger, None, None, scope_key)
}

/// Evaluate an expression with full context: cube + ledger + benchmark + scope.
///
/// Phase 7A.4: adds benchmark query capability alongside ledger queries.
pub fn eval_expr_with_benchmark(
    expr: &str,
    ctx: &Ctx,
    cube: Option<&CubeData>,
    ledger: Option<&LedgerIndex>,
    benchmark: Option<&BenchmarkIndex>,
    scope_key: &str,
) -> Val {
    eval_expr_full(expr, ctx, cube, ledger, benchmark, None, scope_key)
}

/// Evaluate an expression with full context including context events.
/// Per ADR-0022 Decision 5.
///
/// Phase 7A.5: adds context event query capability alongside ledger + benchmark.
/// `has_context_event()`, `context_description()`, `context_event_count()`.
pub fn eval_expr_with_context(
    expr: &str,
    ctx: &Ctx,
    cube: Option<&CubeData>,
    ledger: Option<&LedgerIndex>,
    benchmark: Option<&BenchmarkIndex>,
    context: Option<&ContextIndex>,
    scope_key: &str,
) -> Val {
    eval_expr_full(expr, ctx, cube, ledger, benchmark, context, scope_key)
}

fn eval_expr_full(
    expr: &str,
    ctx: &Ctx,
    cube: Option<&CubeData>,
    ledger: Option<&LedgerIndex>,
    benchmark: Option<&BenchmarkIndex>,
    context: Option<&ContextIndex>,
    scope_key: &str,
) -> Val {
    // Normalize whitespace: YAML folded scalars can embed newlines.
    let expr = expr.replace('\n', " ");
    let expr = expr.trim();
    if expr.is_empty() {
        return Val::Null;
    }

    // Literal "true" / "false"
    if expr == "true" {
        return Val::Bool(true);
    }
    if expr == "false" {
        return Val::Bool(false);
    }

    // NOT prefix (Finding #4): "NOT expr" or "not expr"
    if let Some(rest) = expr
        .strip_prefix("NOT ")
        .or_else(|| expr.strip_prefix("not "))
    {
        let val = eval_expr_full(rest, ctx, cube, ledger, benchmark, context, scope_key);
        return Val::Bool(!val.is_truthy());
    }

    // AND / OR (lowest precedence, split from left)
    if let Some(val) = try_logical(expr, ctx, cube, ledger, benchmark, context, scope_key) {
        return val;
    }

    // Comparisons: >=, <=, !=, ==, >, <
    if let Some(val) = try_comparison(expr, ctx, cube, ledger, benchmark, context, scope_key) {
        return val;
    }

    // Addition / subtraction (left-to-right, respecting parens)
    if let Some(val) = try_additive(expr, ctx, cube, ledger, benchmark, context, scope_key) {
        return val;
    }

    // Multiplication / division
    if let Some(val) = try_multiplicative(expr, ctx, cube, ledger, benchmark, context, scope_key) {
        return val;
    }

    // Unary / atoms
    eval_atom(expr, ctx, cube, ledger, benchmark, context, scope_key)
}

fn try_logical(
    expr: &str,
    ctx: &Ctx,
    cube: Option<&CubeData>,
    ledger: Option<&LedgerIndex>,
    benchmark: Option<&BenchmarkIndex>,
    context: Option<&ContextIndex>,
    scope_key: &str,
) -> Option<Val> {
    for keyword in &[" AND ", " OR "] {
        if let Some(pos) = find_top_level(expr, keyword) {
            let left = eval_expr_full(
                &expr[..pos],
                ctx,
                cube,
                ledger,
                benchmark,
                context,
                scope_key,
            );
            let right = eval_expr_full(
                &expr[pos + keyword.len()..],
                ctx,
                cube,
                ledger,
                benchmark,
                context,
                scope_key,
            );
            return Some(match *keyword {
                " AND " => Val::Bool(left.is_truthy() && right.is_truthy()),
                " OR " => Val::Bool(left.is_truthy() || right.is_truthy()),
                _ => unreachable!(),
            });
        }
    }
    None
}

fn try_comparison(
    expr: &str,
    ctx: &Ctx,
    cube: Option<&CubeData>,
    ledger: Option<&LedgerIndex>,
    benchmark: Option<&BenchmarkIndex>,
    context: Option<&ContextIndex>,
    scope_key: &str,
) -> Option<Val> {
    for op in &[">=", "<=", "!=", "==", ">", "<"] {
        if let Some(pos) = find_top_level(expr, op) {
            let left = eval_expr_full(
                &expr[..pos],
                ctx,
                cube,
                ledger,
                benchmark,
                context,
                scope_key,
            );
            let right = eval_expr_full(
                &expr[pos + op.len()..],
                ctx,
                cube,
                ledger,
                benchmark,
                context,
                scope_key,
            );
            let (l, r) = match (left.as_num(), right.as_num()) {
                (Some(l), Some(r)) => (l, r),
                _ => return Some(Val::Bool(false)),
            };
            return Some(Val::Bool(match *op {
                ">=" => l >= r,
                "<=" => l <= r,
                "!=" => (l - r).abs() > 1e-9,
                "==" => (l - r).abs() < 1e-9,
                ">" => l > r,
                "<" => l < r,
                _ => false,
            }));
        }
    }
    None
}

fn try_additive(
    expr: &str,
    ctx: &Ctx,
    cube: Option<&CubeData>,
    ledger: Option<&LedgerIndex>,
    benchmark: Option<&BenchmarkIndex>,
    context: Option<&ContextIndex>,
    scope_key: &str,
) -> Option<Val> {
    let bytes = expr.as_bytes();
    let mut depth = 0i32;
    let mut last_pos: Option<(usize, u8)> = None;
    for i in 0..bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'+' | b'-' if depth == 0 && i > 0 => {
                let prev_char = bytes[i - 1];
                if prev_char != b'*' && prev_char != b'/' && prev_char != b'(' {
                    last_pos = Some((i, bytes[i]));
                }
            }
            _ => {}
        }
    }
    if let Some((pos, op)) = last_pos {
        let left = eval_expr_full(
            &expr[..pos],
            ctx,
            cube,
            ledger,
            benchmark,
            context,
            scope_key,
        );
        let right = eval_expr_full(
            &expr[pos + 1..],
            ctx,
            cube,
            ledger,
            benchmark,
            context,
            scope_key,
        );
        if let (Some(l), Some(r)) = (left.as_num(), right.as_num()) {
            return Some(Val::Num(if op == b'+' { l + r } else { l - r }));
        }
    }
    None
}

fn try_multiplicative(
    expr: &str,
    ctx: &Ctx,
    cube: Option<&CubeData>,
    ledger: Option<&LedgerIndex>,
    benchmark: Option<&BenchmarkIndex>,
    context: Option<&ContextIndex>,
    scope_key: &str,
) -> Option<Val> {
    let bytes = expr.as_bytes();
    let mut depth = 0i32;
    let mut last_pos: Option<(usize, u8)> = None;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'*' | b'/' if depth == 0 => {
                last_pos = Some((i, b));
            }
            _ => {}
        }
    }
    if let Some((pos, op)) = last_pos {
        let left = eval_expr_full(
            &expr[..pos],
            ctx,
            cube,
            ledger,
            benchmark,
            context,
            scope_key,
        );
        let right = eval_expr_full(
            &expr[pos + 1..],
            ctx,
            cube,
            ledger,
            benchmark,
            context,
            scope_key,
        );
        if let (Some(l), Some(r)) = (left.as_num(), right.as_num()) {
            return Some(Val::Num(if op == b'*' {
                l * r
            } else if r.abs() < 1e-15 {
                0.0
            } else {
                l / r
            }));
        }
    }
    None
}

fn eval_atom(
    expr: &str,
    ctx: &Ctx,
    cube: Option<&CubeData>,
    ledger: Option<&LedgerIndex>,
    benchmark: Option<&BenchmarkIndex>,
    context: Option<&ContextIndex>,
    scope_key: &str,
) -> Val {
    let expr = expr.trim();

    // Parenthesized expression.
    if expr.starts_with('(') && expr.ends_with(')') && matching_paren(expr) == expr.len() - 1 {
        return eval_expr_full(
            &expr[1..expr.len() - 1],
            ctx,
            cube,
            ledger,
            benchmark,
            context,
            scope_key,
        );
    }

    // String literal: 'text'
    if expr.starts_with('\'') && expr.ends_with('\'') && expr.len() >= 2 {
        return Val::Str(expr[1..expr.len() - 1].to_string());
    }

    // Numeric literal.
    if let Ok(n) = expr.parse::<f64>() {
        return Val::Num(n);
    }

    // Function calls.
    if let Some(inner) = strip_func(expr, "abs") {
        return match eval_expr_full(inner, ctx, cube, ledger, benchmark, context, scope_key)
            .as_num()
        {
            Some(n) => Val::Num(n.abs()),
            None => Val::Null,
        };
    }
    if let Some(inner) = strip_func(expr, "not") {
        // Finding #4: NOT operator as function call.
        let val = eval_expr_full(inner, ctx, cube, ledger, benchmark, context, scope_key);
        return Val::Bool(!val.is_truthy());
    }
    if let Some(inner) = strip_func(expr, "if") {
        return eval_if(inner, ctx, cube, ledger, benchmark, context, scope_key);
    }
    if let Some(inner) = strip_func(expr, "element_count") {
        let key = format!("element_count({})", inner.trim());
        return ctx.get(&key).cloned().unwrap_or(Val::Num(0.0));
    }

    // ─── Ledger query functions (Phase 7A.3) ──────────────────────────
    if let Some(inner) = strip_func(expr, "ledger_count") {
        return eval_ledger_count(inner, ctx, cube, ledger, scope_key);
    }
    if let Some(inner) = strip_func(expr, "ledger_has") {
        return eval_ledger_has(inner, ctx, cube, ledger, scope_key);
    }
    if let Some(inner) = strip_func(expr, "ledger_streak") {
        return eval_ledger_streak(inner, ledger, scope_key);
    }
    if let Some(inner) = strip_func(expr, "ledger_evidence") {
        return eval_ledger_evidence(inner, ctx, cube, ledger, scope_key);
    }
    if let Some(inner) = strip_func(expr, "ledger_first_period") {
        return eval_ledger_first_period(inner, ctx, cube, ledger, scope_key);
    }
    if let Some(inner) = strip_func(expr, "ledger_last_period") {
        return eval_ledger_last_period(inner, ctx, cube, ledger, scope_key);
    }

    // ─── Benchmark query functions (Phase 7A.4) ──────────────────────
    if let Some(inner) = strip_func(expr, "benchmark_p10") {
        return eval_benchmark_field(inner, benchmark, context, scope_key, |b| b.p10);
    }
    if let Some(inner) = strip_func(expr, "benchmark_p25") {
        return eval_benchmark_field(inner, benchmark, context, scope_key, |b| b.p25);
    }
    if let Some(inner) = strip_func(expr, "benchmark_p50") {
        return eval_benchmark_field(inner, benchmark, context, scope_key, |b| b.p50);
    }
    if let Some(inner) = strip_func(expr, "benchmark_p75") {
        return eval_benchmark_field(inner, benchmark, context, scope_key, |b| b.p75);
    }
    if let Some(inner) = strip_func(expr, "benchmark_p90") {
        return eval_benchmark_field(inner, benchmark, context, scope_key, |b| b.p90);
    }
    if let Some(inner) = strip_func(expr, "benchmark_mean") {
        return eval_benchmark_field(inner, benchmark, context, scope_key, |b| b.mean);
    }
    if let Some(inner) = strip_func(expr, "benchmark_sample_count") {
        return eval_benchmark_field(inner, benchmark, context, scope_key, |b| {
            b.sample_count as f64
        });
    }
    if let Some(inner) = strip_func(expr, "benchmark_percentile") {
        return eval_benchmark_percentile(inner, ctx, cube, ledger, benchmark, context, scope_key);
    }
    if let Some(inner) = strip_func(expr, "benchmark_above_median") {
        return eval_benchmark_above_median(
            inner, ctx, cube, ledger, benchmark, context, scope_key,
        );
    }
    if let Some(inner) = strip_func(expr, "benchmark_z_score") {
        return eval_benchmark_z_score(inner, ctx, cube, ledger, benchmark, context, scope_key);
    }

    // ─── Context event functions (Phase 7A.5, ADR-0022 Decision 5) ─────
    if let Some(inner) = strip_func(expr, "has_context_event") {
        return eval_has_context_event(inner, context, scope_key);
    }
    if let Some(inner) = strip_func(expr, "context_description") {
        return eval_context_description(inner, context, scope_key);
    }
    if let Some(inner) = strip_func(expr, "context_event_count") {
        return eval_context_event_count(inner, context, scope_key);
    }

    // Generic aggregate functions (Finding #1: evaluate arbitrary predicates).
    if let Some(inner) = strip_func(expr, "count_where") {
        return eval_count_where(inner, ctx, cube);
    }
    if let Some(inner) = strip_func(expr, "any_where") {
        return eval_any_where(inner, ctx, cube);
    }
    if let Some(inner) = strip_func(expr, "names_where") {
        return eval_names_where(inner, ctx, cube);
    }
    // first_where has dotted access: first_where(pred, dim).field
    if expr.starts_with("first_where(") {
        return eval_first_where(expr, ctx, cube);
    }

    // Context variable lookup (the common case).
    if let Some(val) = ctx.get(expr) {
        return val.clone();
    }

    // Fallback: period_name aliases.
    if expr == "prev_period" || expr == "prev.period_name" {
        return ctx
            .get("prev.period_name")
            .cloned()
            .unwrap_or(Val::Str("N/A".into()));
    }
    if expr == "current_period" || expr == "current.period_name" {
        return ctx
            .get("current.period_name")
            .cloned()
            .unwrap_or(Val::Str("N/A".into()));
    }

    Val::Null
}

fn eval_if(
    args_str: &str,
    ctx: &Ctx,
    cube: Option<&CubeData>,
    ledger: Option<&LedgerIndex>,
    benchmark: Option<&BenchmarkIndex>,
    context: Option<&ContextIndex>,
    scope_key: &str,
) -> Val {
    let parts = split_top_level_commas(args_str);
    if parts.len() < 3 {
        return Val::Null;
    }
    let cond = eval_expr_full(parts[0], ctx, cube, ledger, benchmark, context, scope_key);
    if cond.is_truthy() {
        eval_expr_full(parts[1], ctx, cube, ledger, benchmark, context, scope_key)
    } else {
        eval_expr_full(parts[2], ctx, cube, ledger, benchmark, context, scope_key)
    }
}

// ─── Generic aggregate functions (Finding #1) ──────────────────────

/// Build a per-element context for evaluating predicates.
///
/// For element at index `idx` in the cube's data, populates a context
/// with each measure's value at that index so that arbitrary predicate
/// expressions can reference any measure by name.
fn build_element_ctx(cube: &CubeData, idx: usize, base_ctx: &Ctx) -> Ctx {
    let mut elem_ctx = base_ctx.clone();
    for (measure, entries) in &cube.values {
        if let Some(entry) = entries.get(idx) {
            elem_ctx.insert(measure.clone(), Val::Num(entry.value));
        }
    }
    elem_ctx
}

/// Get the number of elements (rows) in the cube.
fn element_count(cube: &CubeData) -> usize {
    cube.values.values().next().map(|v| v.len()).unwrap_or(0)
}

/// `count_where(predicate, dimension)` — count elements matching predicate.
///
/// Finding #1: evaluates ANY predicate expression generically, not just
/// pre-computed conditions. E.g., `count_where(Impressions < 200, City)`
/// iterates all City elements, evaluates `Impressions < 200` per-element.
fn eval_count_where(args: &str, ctx: &Ctx, cube: Option<&CubeData>) -> Val {
    // Try context lookup first (backward compat with pre-computed values).
    let key = format!("count_where({args})");
    if let Some(val) = ctx.get(key.trim()) {
        return val.clone();
    }

    let cube = match cube {
        Some(c) => c,
        None => return Val::Num(0.0),
    };

    let parts = split_top_level_commas(args);
    if parts.is_empty() {
        return Val::Num(0.0);
    }
    let predicate = parts[0].trim();

    let count = (0..element_count(cube))
        .filter(|&idx| {
            let elem_ctx = build_element_ctx(cube, idx, ctx);
            eval_expr_with_cube(predicate, &elem_ctx, Some(cube)).is_truthy()
        })
        .count();

    Val::Num(count as f64)
}

/// `any_where(predicate, dimension)` — true if any element matches.
fn eval_any_where(args: &str, ctx: &Ctx, cube: Option<&CubeData>) -> Val {
    let key = format!("any_where({args})");
    if let Some(val) = ctx.get(key.trim()) {
        return val.clone();
    }

    let cube = match cube {
        Some(c) => c,
        None => return Val::Bool(false),
    };

    let parts = split_top_level_commas(args);
    if parts.is_empty() {
        return Val::Bool(false);
    }
    let predicate = parts[0].trim();

    let found = (0..element_count(cube)).any(|idx| {
        let elem_ctx = build_element_ctx(cube, idx, ctx);
        eval_expr_with_cube(predicate, &elem_ctx, Some(cube)).is_truthy()
    });

    Val::Bool(found)
}

/// `names_where(predicate, dimension)` — comma-separated names of matching elements.
fn eval_names_where(args: &str, ctx: &Ctx, cube: Option<&CubeData>) -> Val {
    let key = format!("names_where({args})");
    if let Some(val) = ctx.get(key.trim()) {
        return val.clone();
    }

    let cube = match cube {
        Some(c) => c,
        None => return Val::Str(String::new()),
    };

    let parts = split_top_level_commas(args);
    if parts.is_empty() {
        return Val::Str(String::new());
    }
    let predicate = parts[0].trim();

    // Use the first measure's entries for category names.
    let entries = match cube.values.values().next() {
        Some(e) => e,
        None => return Val::Str(String::new()),
    };

    let names: Vec<String> = (0..element_count(cube))
        .filter(|&idx| {
            let elem_ctx = build_element_ctx(cube, idx, ctx);
            eval_expr_with_cube(predicate, &elem_ctx, Some(cube)).is_truthy()
        })
        .filter_map(|idx| entries.get(idx).map(|e| readable_name(&e.category)))
        .collect();

    Val::Str(names.join(", "))
}

/// `first_where(predicate, dimension).field` — first matching element's field.
///
/// Handles dotted access: `first_where(pred, dim).name` returns the
/// category name; `first_where(pred, dim).Measure` returns the measure value.
fn eval_first_where(full_expr: &str, ctx: &Ctx, cube: Option<&CubeData>) -> Val {
    // first_where is often accessed with dot-notation:
    //   first_where(Clicks == 0 AND Impressions > 50, geo_dimension).name
    //   first_where(Clicks == 0 AND Impressions > 50, geo_dimension).Impressions
    // The full_expr includes the dot-access; strip_func only gave us the parens content.

    // Try context lookup first.
    if let Some(val) = ctx.get(full_expr.trim()) {
        return val.clone();
    }

    let cube = match cube {
        Some(c) => c,
        None => return Val::Null,
    };

    // Parse the full expression to find the dot-access field.
    // Shape: first_where(predicate, dim).field
    let after_close = match full_expr.find(").") {
        Some(pos) => &full_expr[pos + 2..],
        None => return Val::Null,
    };
    let field = after_close.trim();

    // Re-extract the args from the full expression.
    let paren_start = match full_expr.find('(') {
        Some(pos) => pos + 1,
        None => return Val::Null,
    };
    let paren_end = match full_expr.rfind(").") {
        Some(pos) => pos,
        None => return Val::Null,
    };
    let args = &full_expr[paren_start..paren_end];
    let parts = split_top_level_commas(args);
    if parts.is_empty() {
        return Val::Null;
    }
    let predicate = parts[0].trim();

    let entries = match cube.values.values().next() {
        Some(e) => e,
        None => return Val::Null,
    };

    // Find first matching element.
    let idx = (0..element_count(cube)).find(|&idx| {
        let elem_ctx = build_element_ctx(cube, idx, ctx);
        eval_expr_with_cube(predicate, &elem_ctx, Some(cube)).is_truthy()
    });

    let idx = match idx {
        Some(i) => i,
        None => return Val::Null,
    };

    if field == "name" {
        entries
            .get(idx)
            .map(|e| Val::Str(readable_name(&e.category)))
            .unwrap_or(Val::Null)
    } else {
        // Field is a measure name — look up the value at this index.
        cube.values
            .get(field)
            .and_then(|m| m.get(idx))
            .map(|e| Val::Num(e.value))
            .unwrap_or(Val::Null)
    }
}

// ─── Ledger query function implementations (Phase 7A.3) ─────────────

/// `ledger_count(template_id, lookback_periods)` → number.
///
/// Counts how many of the last N periods have a ledger entry for the
/// given template_id at the current evaluation scope.
fn eval_ledger_count(
    args: &str,
    ctx: &Ctx,
    cube: Option<&CubeData>,
    ledger: Option<&LedgerIndex>,
    scope_key: &str,
) -> Val {
    let ledger = match ledger {
        Some(l) => l,
        None => return Val::Num(0.0),
    };
    let parts = split_top_level_commas(args);
    if parts.len() < 2 {
        return Val::Num(0.0);
    }
    let template_id = extract_string_arg(parts[0], ctx, cube, ledger, scope_key);
    let lookback = eval_expr_full(
        parts[1].trim(),
        ctx,
        cube,
        Some(ledger),
        None,
        None,
        scope_key,
    )
    .as_num()
    .unwrap_or(6.0) as usize;

    Val::Num(ledger.count(&template_id, scope_key, lookback) as f64)
}

/// `ledger_has(template_id, lookback_periods)` → boolean (1.0/0.0).
///
/// Returns 1.0 if any ledger entry exists for the template_id within
/// the lookback window at the current scope; 0.0 otherwise.
fn eval_ledger_has(
    args: &str,
    ctx: &Ctx,
    cube: Option<&CubeData>,
    ledger: Option<&LedgerIndex>,
    scope_key: &str,
) -> Val {
    let ledger = match ledger {
        Some(l) => l,
        None => return Val::Num(0.0),
    };
    let parts = split_top_level_commas(args);
    if parts.len() < 2 {
        return Val::Num(0.0);
    }
    let template_id = extract_string_arg(parts[0], ctx, cube, ledger, scope_key);
    let lookback = eval_expr_full(
        parts[1].trim(),
        ctx,
        cube,
        Some(ledger),
        None,
        None,
        scope_key,
    )
    .as_num()
    .unwrap_or(6.0) as usize;

    Val::Num(if ledger.has(&template_id, scope_key, lookback) {
        1.0
    } else {
        0.0
    })
}

/// `ledger_streak(template_id)` → number.
///
/// Counts consecutive periods ending at the period just before the
/// current one where the template fired for the current scope.
fn eval_ledger_streak(args: &str, ledger: Option<&LedgerIndex>, scope_key: &str) -> Val {
    let ledger = match ledger {
        Some(l) => l,
        None => return Val::Num(0.0),
    };
    let template_id = args.trim().trim_matches('\'').trim_matches('"');
    Val::Num(ledger.streak(template_id, scope_key) as f64)
}

/// `ledger_evidence(template_id, field_name, periods_ago)` → number/string.
///
/// Retrieves a specific evidence field from a ledger entry N periods ago.
fn eval_ledger_evidence(
    args: &str,
    ctx: &Ctx,
    cube: Option<&CubeData>,
    ledger: Option<&LedgerIndex>,
    scope_key: &str,
) -> Val {
    let ledger = match ledger {
        Some(l) => l,
        None => return Val::Null,
    };
    let parts = split_top_level_commas(args);
    if parts.len() < 3 {
        return Val::Null;
    }
    let template_id = extract_string_arg(parts[0], ctx, cube, ledger, scope_key);
    let field_name = extract_string_arg(parts[1], ctx, cube, ledger, scope_key);
    let periods_ago = eval_expr_full(
        parts[2].trim(),
        ctx,
        cube,
        Some(ledger),
        None,
        None,
        scope_key,
    )
    .as_num()
    .unwrap_or(0.0) as usize;

    ledger.evidence(&template_id, scope_key, &field_name, periods_ago)
}

/// `ledger_first_period(template_id, lookback_periods)` → string.
///
/// Returns the earliest period within the lookback window that has an entry.
fn eval_ledger_first_period(
    args: &str,
    ctx: &Ctx,
    cube: Option<&CubeData>,
    ledger: Option<&LedgerIndex>,
    scope_key: &str,
) -> Val {
    let ledger = match ledger {
        Some(l) => l,
        None => return Val::Null,
    };
    let parts = split_top_level_commas(args);
    if parts.len() < 2 {
        return Val::Null;
    }
    let template_id = extract_string_arg(parts[0], ctx, cube, ledger, scope_key);
    let lookback = eval_expr_full(
        parts[1].trim(),
        ctx,
        cube,
        Some(ledger),
        None,
        None,
        scope_key,
    )
    .as_num()
    .unwrap_or(6.0) as usize;

    ledger.first_period(&template_id, scope_key, lookback)
}

/// `ledger_last_period(template_id, lookback_periods)` → string.
///
/// Returns the most recent period within the lookback window that has an entry.
fn eval_ledger_last_period(
    args: &str,
    ctx: &Ctx,
    cube: Option<&CubeData>,
    ledger: Option<&LedgerIndex>,
    scope_key: &str,
) -> Val {
    let ledger = match ledger {
        Some(l) => l,
        None => return Val::Null,
    };
    let parts = split_top_level_commas(args);
    if parts.len() < 2 {
        return Val::Null;
    }
    let template_id = extract_string_arg(parts[0], ctx, cube, ledger, scope_key);
    let lookback = eval_expr_full(
        parts[1].trim(),
        ctx,
        cube,
        Some(ledger),
        None,
        None,
        scope_key,
    )
    .as_num()
    .unwrap_or(6.0) as usize;

    ledger.last_period(&template_id, scope_key, lookback)
}

// ─── Benchmark query functions (Phase 7A.4) ──────────────────────────

/// Helper for single-arg benchmark functions (benchmark_p10, _p25, _p50, etc.).
///
/// Extracts the metric name from the argument, looks up the benchmark at the
/// current scope, and returns the requested field via the `field_fn` closure.
fn eval_benchmark_field(
    args: &str,
    benchmark: Option<&BenchmarkIndex>,
    _context: Option<&ContextIndex>,
    scope_key: &str,
    field_fn: impl Fn(&MetricBenchmark) -> f64,
) -> Val {
    let benchmark = match benchmark {
        Some(b) => b,
        None => return Val::Num(0.0),
    };
    let metric = args.trim().trim_matches('\'').trim_matches('"');
    match benchmark.lookup(metric, scope_key) {
        Some(b) => Val::Num(field_fn(b)),
        None => Val::Num(0.0),
    }
}

/// `benchmark_percentile(metric, value)` → f64 (0-100).
///
/// Where does `value` fall in the historical distribution? Returns a 0-100
/// percentile rank using nearest-breakpoint. Per handoff: linear interpolation
/// between breakpoints is a Phase 7B refinement.
fn eval_benchmark_percentile(
    args: &str,
    ctx: &Ctx,
    cube: Option<&CubeData>,
    ledger: Option<&LedgerIndex>,
    benchmark: Option<&BenchmarkIndex>,
    context: Option<&ContextIndex>,
    scope_key: &str,
) -> Val {
    let benchmark = match benchmark {
        Some(b) => b,
        None => return Val::Num(0.0),
    };
    let parts = split_top_level_commas(args);
    if parts.len() < 2 {
        return Val::Num(0.0);
    }
    let metric = parts[0].trim().trim_matches('\'').trim_matches('"');
    let value = eval_expr_full(
        parts[1].trim(),
        ctx,
        cube,
        ledger,
        Some(benchmark),
        context,
        scope_key,
    )
    .as_num()
    .unwrap_or(0.0);

    match benchmark.lookup(metric, scope_key) {
        Some(b) => Val::Num(benchmark_percentile_rank(b, value)),
        None => Val::Num(0.0),
    }
}

/// `benchmark_above_median(metric)` → f64 (1.0 if current value > p50, 0.0 otherwise).
///
/// Shorthand for `current.Metric > benchmark_p50(Metric)`. The evaluator reads
/// the current cube value and compares to the p50 from the benchmark.
fn eval_benchmark_above_median(
    args: &str,
    ctx: &Ctx,
    cube: Option<&CubeData>,
    ledger: Option<&LedgerIndex>,
    benchmark: Option<&BenchmarkIndex>,
    context: Option<&ContextIndex>,
    scope_key: &str,
) -> Val {
    let benchmark = match benchmark {
        Some(b) => b,
        None => return Val::Num(0.0),
    };
    let metric = args.trim().trim_matches('\'').trim_matches('"');

    // Read current value: look up "current.{metric}" in the eval context.
    let current_key = format!("current.{metric}");
    let current_val = ctx
        .get(&current_key)
        .and_then(|v| v.as_num())
        .unwrap_or_else(|| {
            // Fallback: evaluate the expression "current.{metric}" against the context.
            eval_expr_full(
                &current_key,
                ctx,
                cube,
                ledger,
                Some(benchmark),
                context,
                scope_key,
            )
            .as_num()
            .unwrap_or(0.0)
        });

    match benchmark.lookup(metric, scope_key) {
        Some(b) => Val::Num(if current_val > b.p50 { 1.0 } else { 0.0 }),
        None => Val::Num(0.0),
    }
}

/// `benchmark_z_score(metric, value)` → f64.
///
/// Computes (value - mean) / stddev. Returns 0.0 if stddev is 0 (all samples
/// identical — z-score of zero is correct).
fn eval_benchmark_z_score(
    args: &str,
    ctx: &Ctx,
    cube: Option<&CubeData>,
    ledger: Option<&LedgerIndex>,
    benchmark: Option<&BenchmarkIndex>,
    context: Option<&ContextIndex>,
    scope_key: &str,
) -> Val {
    let benchmark = match benchmark {
        Some(b) => b,
        None => return Val::Num(0.0),
    };
    let parts = split_top_level_commas(args);
    if parts.len() < 2 {
        return Val::Num(0.0);
    }
    let metric = parts[0].trim().trim_matches('\'').trim_matches('"');
    let value = eval_expr_full(
        parts[1].trim(),
        ctx,
        cube,
        ledger,
        Some(benchmark),
        context,
        scope_key,
    )
    .as_num()
    .unwrap_or(0.0);

    match benchmark.lookup(metric, scope_key) {
        Some(b) => {
            if b.stddev.abs() < 1e-15 {
                Val::Num(0.0)
            } else {
                Val::Num((value - b.mean) / b.stddev)
            }
        }
        None => Val::Num(0.0),
    }
}

/// Compute percentile rank of a value against a benchmark's breakpoints.
///
/// Returns a 0-100 value. Nearest-breakpoint method — no interpolation.
fn benchmark_percentile_rank(bench: &MetricBenchmark, value: f64) -> f64 {
    if value <= bench.p10 {
        10.0
    } else if value <= bench.p25 {
        25.0
    } else if value <= bench.p50 {
        50.0
    } else if value <= bench.p75 {
        75.0
    } else if value <= bench.p90 {
        90.0
    } else {
        100.0
    }
}

/// Extract a string argument (strip quotes if present, or evaluate as expression).
fn extract_string_arg(
    arg: &str,
    ctx: &Ctx,
    cube: Option<&CubeData>,
    ledger: &LedgerIndex,
    scope_key: &str,
) -> String {
    let arg = arg.trim();
    // String literal: 'text' or "text"
    if (arg.starts_with('\'') && arg.ends_with('\''))
        || (arg.starts_with('"') && arg.ends_with('"'))
    {
        return arg[1..arg.len() - 1].to_string();
    }
    // Otherwise evaluate and convert to string.
    eval_expr_full(arg, ctx, cube, Some(ledger), None, None, scope_key).to_display()
}

// ─── Context event evaluator functions (Phase 7A.5) ─────────────────

/// `has_context_event(type)` → 1.0 if event exists for current period/scope, else 0.0.
/// `has_context_event(type, lookback_periods)` → same with N-period lookback.
/// Per ADR-0022 Decision 5.
fn eval_has_context_event(args: &str, context: Option<&ContextIndex>, scope_key: &str) -> Val {
    let context = match context {
        Some(c) => c,
        None => return Val::Num(0.0),
    };
    let parts = split_top_level_commas(args);
    let event_type = parts[0].trim().trim_matches('\'').trim_matches('"');
    let lookback = if parts.len() >= 2 {
        parts[1].trim().parse::<usize>().unwrap_or(1)
    } else {
        1
    };
    Val::Num(if context.has_event(event_type, scope_key, lookback) {
        1.0
    } else {
        0.0
    })
}

/// `context_description(type)` → string description of first matching event.
/// Per ADR-0022 Decision 5: returns empty string if no match.
fn eval_context_description(args: &str, context: Option<&ContextIndex>, scope_key: &str) -> Val {
    let context = match context {
        Some(c) => c,
        None => return Val::Str(String::new()),
    };
    let event_type = args.trim().trim_matches('\'').trim_matches('"');
    match context.description(event_type, scope_key) {
        Some(desc) => Val::Str(desc),
        None => Val::Str(String::new()),
    }
}

/// `context_event_count(type)` → number of matching events for current period/scope.
/// `context_event_count(type, lookback_periods)` → same with N-period lookback.
fn eval_context_event_count(args: &str, context: Option<&ContextIndex>, scope_key: &str) -> Val {
    let context = match context {
        Some(c) => c,
        None => return Val::Num(0.0),
    };
    let parts = split_top_level_commas(args);
    let event_type = parts[0].trim().trim_matches('\'').trim_matches('"');
    let lookback = if parts.len() >= 2 {
        parts[1].trim().parse::<usize>().unwrap_or(1)
    } else {
        1
    };
    Val::Num(context.count_events(event_type, scope_key, lookback) as f64)
}

// ─── Parsing helpers ────────────────────────────────────────────────

/// Find the position of `needle` at the top level (not inside parens or quotes).
fn find_top_level(haystack: &str, needle: &str) -> Option<usize> {
    let bytes = haystack.as_bytes();
    let needle_bytes = needle.as_bytes();
    let mut depth = 0i32;
    let mut in_quote = false;
    let mut i = 0;
    while i + needle_bytes.len() <= bytes.len() {
        let b = bytes[i];
        if b == b'\'' {
            in_quote = !in_quote;
        }
        if !in_quote {
            if b == b'(' {
                depth += 1;
            } else if b == b')' {
                depth -= 1;
            }
            if depth == 0 && &bytes[i..i + needle_bytes.len()] == needle_bytes {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Find the index of the closing paren matching the opening paren at position 0.
fn matching_paren(s: &str) -> usize {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    s.len()
}

/// Strip a function call: "func(args)" -> Some("args")
fn strip_func<'a>(expr: &'a str, func_name: &str) -> Option<&'a str> {
    let expr = expr.trim();
    if expr.starts_with(func_name)
        && expr[func_name.len()..].starts_with('(')
        && expr.ends_with(')')
    {
        Some(&expr[func_name.len() + 1..expr.len() - 1])
    } else {
        None
    }
}

/// Split a string on top-level commas (not inside parens or quotes).
pub fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut in_quote = false;
    let mut start = 0;
    for (byte_pos, c) in s.char_indices() {
        match c {
            '\'' => in_quote = !in_quote,
            '(' if !in_quote => depth += 1,
            ')' if !in_quote => depth -= 1,
            ',' if depth == 0 && !in_quote => {
                parts.push(&s[start..byte_pos]);
                start = byte_pos + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CellEntry;
    use std::collections::BTreeMap;

    fn make_ctx(pairs: &[(&str, f64)]) -> Ctx {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), Val::Num(*v)))
            .collect()
    }

    fn make_geo_cube() -> CubeData {
        CubeData {
            table_name: "Performance by City".into(),
            subproduct: "Targeted Display".into(),
            source_file: "test.csv".into(),
            dimension_name: Some("City".into()),
            values: BTreeMap::from([
                (
                    "Impressions".into(),
                    vec![
                        CellEntry {
                            category: "Rockford".into(),
                            value: 45000.0,
                        },
                        CellEntry {
                            category: "Springfield".into(),
                            value: 300.0,
                        },
                        CellEntry {
                            category: "Peoria".into(),
                            value: 80.0,
                        },
                    ],
                ),
                (
                    "Clicks".into(),
                    vec![
                        CellEntry {
                            category: "Rockford".into(),
                            value: 150.0,
                        },
                        CellEntry {
                            category: "Springfield".into(),
                            value: 5.0,
                        },
                        CellEntry {
                            category: "Peoria".into(),
                            value: 0.0,
                        },
                    ],
                ),
            ]),
        }
    }

    #[test]
    fn test_eval_arithmetic() {
        let ctx = make_ctx(&[("x", 10.0), ("y", 3.0)]);
        assert_eq!(eval_expr("x + y", &ctx).as_num().unwrap(), 13.0);
        assert_eq!(eval_expr("x - y", &ctx).as_num().unwrap(), 7.0);
        assert_eq!(eval_expr("x * y", &ctx).as_num().unwrap(), 30.0);
        assert!((eval_expr("x / y", &ctx).as_num().unwrap() - 3.333333).abs() < 0.001);
    }

    #[test]
    fn test_eval_comparison() {
        let ctx = make_ctx(&[("a", 5.0), ("b", 3.0)]);
        assert!(eval_expr("a > b", &ctx).is_truthy());
        assert!(!eval_expr("a < b", &ctx).is_truthy());
        assert!(eval_expr("a >= 5", &ctx).is_truthy());
        assert!(eval_expr("a == 5", &ctx).is_truthy());
    }

    #[test]
    fn test_eval_logical() {
        let ctx = make_ctx(&[("x", 1.0), ("y", 0.0)]);
        assert!(eval_expr("x > 0 AND y == 0", &ctx).is_truthy());
        assert!(eval_expr("x > 0 OR y > 0", &ctx).is_truthy());
        assert!(!eval_expr("x > 0 AND y > 0", &ctx).is_truthy());
    }

    #[test]
    fn test_eval_not_operator() {
        // Finding #4: NOT operator support.
        let ctx = make_ctx(&[("x", 1.0), ("y", 0.0)]);
        assert!(!eval_expr("NOT x > 0", &ctx).is_truthy());
        assert!(eval_expr("NOT y > 0", &ctx).is_truthy());
        assert!(eval_expr("not(y > 0)", &ctx).is_truthy());
        assert!(!eval_expr("not(x > 0)", &ctx).is_truthy());
    }

    #[test]
    fn test_eval_if() {
        let ctx = make_ctx(&[("x", 5.0)]);
        let val = eval_expr("if(x > 3, 'high', 'low')", &ctx);
        assert!(matches!(val, Val::Str(s) if s == "high"));
    }

    #[test]
    fn test_eval_nested_if() {
        let ctx = make_ctx(&[("period_count", 2.0)]);
        let val = eval_expr(
            "if(period_count == 1, 'one', if(period_count == 2, 'two', 'many'))",
            &ctx,
        );
        match &val {
            Val::Str(s) => assert_eq!(s, "two", "nested if should return 'two'"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn test_eval_abs() {
        let ctx = make_ctx(&[("x", -42.0)]);
        assert_eq!(eval_expr("abs(x)", &ctx).as_num().unwrap(), 42.0);
    }

    // ─── Generic aggregate function tests (Finding #1) ──────────────

    #[test]
    fn test_count_where_generic() {
        let cube = make_geo_cube();
        let ctx = crate::context::build_context(&cube);

        // count_where(Impressions < 500, City) should match Springfield (300) and Peoria (80).
        let result = eval_expr_with_cube("count_where(Impressions < 500, City)", &ctx, Some(&cube));
        assert_eq!(
            result.as_num().unwrap(),
            2.0,
            "should find 2 cities with < 500 impressions"
        );

        // count_where(Clicks == 0, City) should match only Peoria.
        let result = eval_expr_with_cube("count_where(Clicks == 0, City)", &ctx, Some(&cube));
        assert_eq!(
            result.as_num().unwrap(),
            1.0,
            "should find 1 city with 0 clicks"
        );

        // Arbitrary predicate: count_where(Impressions > 100 AND Clicks > 0, City)
        let result = eval_expr_with_cube(
            "count_where(Impressions > 100 AND Clicks > 0, City)",
            &ctx,
            Some(&cube),
        );
        assert_eq!(
            result.as_num().unwrap(),
            2.0,
            "Rockford and Springfield have >100 impr and >0 clicks"
        );
    }

    #[test]
    fn test_any_where_generic() {
        let cube = make_geo_cube();
        let ctx = crate::context::build_context(&cube);

        let result = eval_expr_with_cube(
            "any_where(Clicks == 0 AND Impressions > 50, City)",
            &ctx,
            Some(&cube),
        );
        assert!(result.is_truthy(), "Peoria has 0 clicks and 80 impressions");
    }

    #[test]
    fn test_names_where_generic() {
        let cube = make_geo_cube();
        let ctx = crate::context::build_context(&cube);

        let result = eval_expr_with_cube("names_where(Impressions < 500, City)", &ctx, Some(&cube));
        match result {
            Val::Str(s) => {
                assert!(s.contains("Springfield"), "should include Springfield: {s}");
                assert!(s.contains("Peoria"), "should include Peoria: {s}");
            }
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn test_first_where_generic() {
        let cube = make_geo_cube();
        let ctx = crate::context::build_context(&cube);

        let result = eval_expr_with_cube(
            "first_where(Clicks == 0 AND Impressions > 50, City).name",
            &ctx,
            Some(&cube),
        );
        match result {
            Val::Str(s) => assert_eq!(s, "Peoria", "first zero-click city should be Peoria"),
            other => panic!("expected Str, got {other:?}"),
        }

        let result = eval_expr_with_cube(
            "first_where(Clicks == 0 AND Impressions > 50, City).Impressions",
            &ctx,
            Some(&cube),
        );
        assert_eq!(result.as_num().unwrap(), 80.0, "Peoria has 80 impressions");
    }

    #[test]
    fn test_count_where_arbitrary_threshold() {
        // Finding #1: an analyst can write count_where(Impressions < 200, City)
        // even though that specific threshold was never pre-computed.
        let cube = make_geo_cube();
        let ctx = crate::context::build_context(&cube);

        let result = eval_expr_with_cube("count_where(Impressions < 200, City)", &ctx, Some(&cube));
        assert_eq!(
            result.as_num().unwrap(),
            1.0,
            "only Peoria (80) has < 200 impressions"
        );
    }

    // ─── Ledger query function tests (Phase 7A.3) ─────────────────────

    fn make_ledger_entry(
        template_id: &str,
        period: &str,
        scope: &[(&str, &str)],
        evidence: &[(&str, f64)],
    ) -> crate::ledger::LedgerEntry {
        let scope_map: BTreeMap<String, String> = scope
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let evidence_map: BTreeMap<String, serde_json::Value> = evidence
            .iter()
            .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
            .collect();
        crate::ledger::LedgerEntry {
            schema_version: "1.0".to_string(),
            ledger_entry_id: format!("{template_id}-{period}"),
            generated_at: "2026-05-07T10:00:00Z".to_string(),
            model: "test.yaml".to_string(),
            model_hash: "sha256:test".to_string(),
            report_period: Some(period.to_string()),
            scope: scope_map,
            narrative: crate::ledger::NarrativeRecord {
                id: template_id.to_string(),
                section: None,
                severity: "warning".to_string(),
                text: "test".to_string(),
                template_id: template_id.to_string(),
                notability_score: None,
            },
            evidence: evidence_map,
            benchmarks_referenced: Vec::new(),
        }
    }

    #[test]
    fn test_ledger_count_returns_matching_entry_count() {
        let entries = vec![
            make_ledger_entry("clicks_down", "2026-01", &[("channel", "Display")], &[]),
            make_ledger_entry("clicks_down", "2026-02", &[("channel", "Display")], &[]),
            make_ledger_entry("clicks_down", "2026-03", &[("channel", "Display")], &[]),
            make_ledger_entry("spend_up", "2026-02", &[("channel", "Display")], &[]),
        ];
        let index = LedgerIndex::build(&entries, Some("2026-04".to_string()));
        let ctx = HashMap::new();

        let result = eval_expr_with_ledger(
            "ledger_count('clicks_down', 6)",
            &ctx,
            None,
            Some(&index),
            "channel=Display",
        );
        assert_eq!(
            result.as_num().unwrap(),
            3.0,
            "should find 3 entries for clicks_down"
        );

        // With lookback of 2, only get the 2 most recent.
        let result = eval_expr_with_ledger(
            "ledger_count('clicks_down', 2)",
            &ctx,
            None,
            Some(&index),
            "channel=Display",
        );
        assert_eq!(
            result.as_num().unwrap(),
            2.0,
            "lookback 2 should return only 2 most recent"
        );
    }

    #[test]
    fn test_ledger_streak_counts_consecutive_periods() {
        let entries = vec![
            make_ledger_entry(
                "impressions_mom_decline",
                "2026-01",
                &[("channel", "Search")],
                &[],
            ),
            make_ledger_entry(
                "impressions_mom_decline",
                "2026-02",
                &[("channel", "Search")],
                &[],
            ),
            make_ledger_entry(
                "impressions_mom_decline",
                "2026-03",
                &[("channel", "Search")],
                &[],
            ),
        ];
        let index = LedgerIndex::build(&entries, Some("2026-04".to_string()));
        let ctx = HashMap::new();

        let result = eval_expr_with_ledger(
            "ledger_streak('impressions_mom_decline')",
            &ctx,
            None,
            Some(&index),
            "channel=Search",
        );
        assert_eq!(
            result.as_num().unwrap(),
            3.0,
            "3 consecutive months should give streak of 3"
        );
    }

    #[test]
    fn test_ledger_streak_resets_on_gap() {
        let entries = vec![
            make_ledger_entry("clicks_down", "2026-01", &[("channel", "Display")], &[]),
            // gap at 2026-02
            make_ledger_entry("clicks_down", "2026-03", &[("channel", "Display")], &[]),
            make_ledger_entry("clicks_down", "2026-04", &[("channel", "Display")], &[]),
        ];
        let index = LedgerIndex::build(&entries, Some("2026-05".to_string()));
        let ctx = HashMap::new();

        let result = eval_expr_with_ledger(
            "ledger_streak('clicks_down')",
            &ctx,
            None,
            Some(&index),
            "channel=Display",
        );
        assert_eq!(
            result.as_num().unwrap(),
            2.0,
            "gap at 2026-02 should reset streak; only Mar+Apr are consecutive"
        );
    }

    #[test]
    fn test_ledger_has_returns_boolean() {
        let entries = vec![make_ledger_entry(
            "device_underperformance",
            "2026-03",
            &[("channel", "Display")],
            &[],
        )];
        let index = LedgerIndex::build(&entries, Some("2026-04".to_string()));
        let ctx = HashMap::new();

        let result = eval_expr_with_ledger(
            "ledger_has('device_underperformance', 3)",
            &ctx,
            None,
            Some(&index),
            "channel=Display",
        );
        assert_eq!(result.as_num().unwrap(), 1.0, "should return 1.0 (true)");

        let result = eval_expr_with_ledger(
            "ledger_has('nonexistent_template', 3)",
            &ctx,
            None,
            Some(&index),
            "channel=Display",
        );
        assert_eq!(result.as_num().unwrap(), 0.0, "should return 0.0 (false)");
    }

    #[test]
    fn test_ledger_evidence_reads_specific_field() {
        let entries = vec![
            make_ledger_entry(
                "impressions_mom",
                "2026-02",
                &[("channel", "Display")],
                &[("prev_value", 5000.0), ("current_value", 4200.0)],
            ),
            make_ledger_entry(
                "impressions_mom",
                "2026-03",
                &[("channel", "Display")],
                &[("prev_value", 4200.0), ("current_value", 3800.0)],
            ),
        ];
        let index = LedgerIndex::build(&entries, Some("2026-04".to_string()));
        let ctx = HashMap::new();

        // periods_ago=0 → most recent (2026-03)
        let result = eval_expr_with_ledger(
            "ledger_evidence('impressions_mom', 'prev_value', 0)",
            &ctx,
            None,
            Some(&index),
            "channel=Display",
        );
        assert_eq!(
            result.as_num().unwrap(),
            4200.0,
            "most recent entry's prev_value should be 4200"
        );

        // periods_ago=1 → one before (2026-02)
        let result = eval_expr_with_ledger(
            "ledger_evidence('impressions_mom', 'prev_value', 1)",
            &ctx,
            None,
            Some(&index),
            "channel=Display",
        );
        assert_eq!(
            result.as_num().unwrap(),
            5000.0,
            "second-most-recent entry's prev_value should be 5000"
        );
    }

    #[test]
    fn test_ledger_query_with_no_ledger_returns_zero() {
        let ctx = HashMap::new();

        let result = eval_expr_with_ledger(
            "ledger_count('clicks_down', 6)",
            &ctx,
            None,
            None,
            "channel=Display",
        );
        assert_eq!(result.as_num().unwrap(), 0.0, "no ledger should return 0");

        let result = eval_expr_with_ledger(
            "ledger_streak('clicks_down')",
            &ctx,
            None,
            None,
            "channel=Display",
        );
        assert_eq!(
            result.as_num().unwrap(),
            0.0,
            "no ledger should return 0 for streak"
        );

        let result = eval_expr_with_ledger(
            "ledger_has('clicks_down', 3)",
            &ctx,
            None,
            None,
            "channel=Display",
        );
        assert_eq!(
            result.as_num().unwrap(),
            0.0,
            "no ledger should return 0 for has"
        );
    }
}
