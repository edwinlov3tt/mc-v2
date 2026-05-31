//! `mc model grade` — segmented holdout evaluation (Phase 10B, ADR-0034).
//!
//! Groups a holdout set by one or more attributes (a dimension, a
//! discrete/string measure value, or a bucketed continuous measure),
//! computes per-segment metrics via the Phase 10A primitives, flags
//! segments crossing a threshold, and emits a text table + JSON. This
//! reproduces claw-core's EXP-048 segment-table workflow in one command.
//!
//! Per ADR-0034 Amendment 4, the grouped-reduction engine lives entirely
//! in `mc-cli` — there is no `mc-core` change.
//!
//! **Phase 10C.1 (ADR-0036 Amendment 8):** the evaluation engine — the
//! metric-expression grammar, the reduction vocabulary, the holdout
//! `Filter` guard, bucket/group resolution, and the per-segment reduction
//! loop — was lifted verbatim into [`crate::eval_common`] so `mc model
//! backtest` can run the same evaluation per grid cell. `grade` now owns
//! only its CLI parsing, the `--flag-if` pass, and the output formatting;
//! `grade_cube` is a thin wrapper over [`crate::eval_common::evaluate`]
//! plus flagging. The behavior is unchanged; grade's tests pass against
//! the shared module.
//!
//! Binding amendments folded in (see ADR-0034 "Acceptance amendments"):
//! - A1: `--holdout` reuses the [`crate::query::Filter`] grammar;
//!   bare F64-measure equality is a hard error.
//! - A2: continuous-F64 `--group-by` requires `--bucket`; `--max-segments`
//!   caps the segment count (default 50).
//! - A3: Wilson Null indicator → hard error by default (`--wilson-null`).
//! - A5: expanded JSON schema (status, null_counts, warnings, bucket
//!   metadata, denominator_zero_segments, reserved subtotals).
//! - A6: `ratio` denom-zero → Null + diagnostic, never inf/NaN/0.
//! - A7: 9 reductions (count/mean/sum/ratio/std/min/max/wilson_*); ADR-0036
//!   Amendment 7 adds `rmse` in `eval_common` (the 10th), so grade gets it too.
//! - A8: `LoadPolicy::Reproducible` default; `--include-writes` opt-in.
//! - A9: TOTAL row inclusive of min-n-excluded segments.
//! - A11: formal metric-expression grammar + error UX.
//! - A12: lexicographic segment ordering, first group-by flag slowest.

use crate::eval_common::{
    evaluate, fmt_edge, parse_bucket_edges, parse_metric_expr, EvalReport, EvalSpec, MetricExpr,
    SegmentResult, SegmentStatus, WilsonNullPolicy,
};
use crate::loader::{load_model_with_policy, LoadPolicy};
use crate::query::{format_f64, push_json_str, CmpOp};
use std::collections::BTreeMap;
use std::fmt::Write;

// ===========================================================================
// Command struct + CLI parsing
// ===========================================================================

/// Output format for `grade` (Decision 1: `text | json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradeFormat {
    Text,
    Json,
}

/// A fully-parsed `mc model grade` invocation.
pub struct GradeCommand {
    pub path: String,
    pub unit: String,
    pub holdout: Option<String>,
    pub group_by: Vec<String>,
    pub metrics: Vec<MetricExpr>,
    /// Measure name → ascending bucket edges (`--bucket <measure> e0:e1:...`).
    pub buckets: BTreeMap<String, Vec<f64>>,
    pub flag_if: Option<String>,
    pub min_n: usize,
    pub max_segments: usize,
    pub wilson_null: WilsonNullPolicy,
    pub include_writes: bool,
    pub format: GradeFormat,
}

/// Usage text for `mc model grade --help` (Phase 10B.1).
fn help_text() -> String {
    "\
mc model grade <cartridge.yaml> — segmented holdout evaluation

Groups a holdout set into segments and computes per-segment metrics
(Phase 10A primitives), flagging segments that cross a threshold.

USAGE:
    mc model grade <path> --unit <dim> --metric <expr> [options]

REQUIRED:
    <path>                 cartridge YAML
    --unit <dim>           dimension whose leaves are the analysis units
    --metric \"<expr>\"      one or more metrics (repeatable); see GRAMMAR

OPTIONS:
    --holdout \"<filter>\"   restrict units (same grammar as `query --where`)
    --group-by <key>       segment by a dimension or measure (repeatable)
    --bucket <measure> <e0:e1:...>   band a continuous measure for grouping
    --flag-if \"<metric> <op> <value>\"   flag segments crossing a threshold
    --min-n <int>          mark segments below n; excluded from flagging (default 0)
    --max-segments <int>   cap resolved segment count (default 50)
    --wilson-null error|drop   Null Wilson indicator: hard error (default) or drop
    --include-writes       fold in operational writes (default: Reproducible)
    --format text|json     output format (default text)
    -h, --help             show this help

GRAMMAR (--metric):
    name=reduction(ingredient[,ingredient])
    reductions: count, mean, sum, ratio, std, min, max, wilson_lower, wilson_upper, rmse
    (ratio takes 2 ingredients; all others take 1)

NOTES:
    - A continuous F64 measure --group-by REQUIRES a matching --bucket.
    - Grouping a non-numeric (string/category) measure is not supported;
      group by a dimension, or author a discrete numeric slice measure.

EXAMPLE (EXP-048):
    mc model grade mlb-totals.yaml --unit Game \\
      --group-by bet_side --bucket bet_side 0:0.5:1.0 \\
      --metric \"n=count(direction_correct)\" \\
      --metric \"win_rate=mean(direction_correct)\" \\
      --metric \"wr_lower_95=wilson_lower(direction_correct)\" \\
      --flag-if \"wr_lower_95 < 0.50\" --format json
"
    .to_string()
}

/// Parse `mc model grade` arguments. Mirrors `sweep::parse` in structure.
pub fn parse(args: &[String]) -> Result<GradeCommand, String> {
    let mut path: Option<String> = None;
    let mut unit: Option<String> = None;
    let mut holdout: Option<String> = None;
    let mut group_by: Vec<String> = Vec::new();
    let mut metric_strs: Vec<String> = Vec::new();
    let mut buckets: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut flag_if: Option<String> = None;
    let mut min_n: usize = 0;
    let mut max_segments: usize = 50;
    let mut wilson_null = WilsonNullPolicy::Error;
    let mut include_writes = false;
    let mut format = GradeFormat::Text;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            // Phase 10B.1: per-command usage. Prints to stdout and exits 0,
            // matching the top-level `mc --help` idiom in main.rs.
            "--help" | "-h" => {
                print!("{}", help_text());
                std::process::exit(0);
            }
            "--unit" => match iter.next() {
                Some(v) => unit = Some(v.clone()),
                None => return Err("--unit requires a dimension name".into()),
            },
            "--holdout" => match iter.next() {
                Some(v) => holdout = Some(v.clone()),
                None => return Err("--holdout requires a filter expression".into()),
            },
            "--group-by" => match iter.next() {
                Some(v) => group_by.push(v.clone()),
                None => return Err("--group-by requires a dimension or measure name".into()),
            },
            "--metric" => match iter.next() {
                Some(v) => metric_strs.push(v.clone()),
                None => return Err("--metric requires an expression".into()),
            },
            "--bucket" => {
                let measure = match iter.next() {
                    Some(v) => v.clone(),
                    None => return Err("--bucket requires a measure name and edges".into()),
                };
                let edges_str = match iter.next() {
                    Some(v) => v.clone(),
                    None => {
                        return Err(format!(
                            "--bucket {measure} requires edges (e.g. 0:0.5:1.0)"
                        ))
                    }
                };
                let edges = parse_bucket_edges(&edges_str)
                    .map_err(|e| format!("--bucket {measure}: {e}"))?;
                buckets.insert(measure, edges);
            }
            "--flag-if" => match iter.next() {
                Some(v) => flag_if = Some(v.clone()),
                None => return Err("--flag-if requires a predicate".into()),
            },
            "--min-n" => match iter.next() {
                Some(v) => {
                    min_n = v
                        .parse()
                        .map_err(|_| format!("--min-n must be a non-negative integer, got {v:?}"))?
                }
                None => return Err("--min-n requires an integer".into()),
            },
            "--max-segments" => match iter.next() {
                Some(v) => {
                    max_segments = v.parse().map_err(|_| {
                        format!("--max-segments must be a positive integer, got {v:?}")
                    })?
                }
                None => return Err("--max-segments requires an integer".into()),
            },
            "--wilson-null" => match iter.next() {
                Some(v) if v == "error" => wilson_null = WilsonNullPolicy::Error,
                Some(v) if v == "drop" => wilson_null = WilsonNullPolicy::Drop,
                Some(v) => return Err(format!("--wilson-null must be error|drop, got {v:?}")),
                None => return Err("--wilson-null requires error|drop".into()),
            },
            "--include-writes" => include_writes = true,
            "--format" => match iter.next() {
                Some(v) if v == "text" => format = GradeFormat::Text,
                Some(v) if v == "json" => format = GradeFormat::Json,
                Some(v) => return Err(format!("--format must be text|json, got {v:?}")),
                None => return Err("--format requires an argument".into()),
            },
            other if !other.starts_with("--") && path.is_none() => {
                path = Some(other.to_string());
            }
            other => return Err(format!("unknown argument: {other:?}")),
        }
    }

    let path = path.ok_or("`mc model grade` requires a YAML model path")?;
    let unit =
        unit.ok_or("--unit is required (the dimension whose leaves are the analysis units)")?;
    if metric_strs.is_empty() {
        return Err("at least one --metric is required".into());
    }
    let metrics: Vec<MetricExpr> = metric_strs
        .iter()
        .map(|s| parse_metric_expr(s))
        .collect::<Result<_, _>>()?;

    Ok(GradeCommand {
        path,
        unit,
        holdout,
        group_by,
        metrics,
        buckets,
        flag_if,
        min_n,
        max_segments,
        wilson_null,
        include_writes,
        format,
    })
}

// ===========================================================================
// Grade evaluation: shared engine + grade-specific flag pass
// ===========================================================================

/// Run the grade analysis against a loaded cube. The segmentation +
/// reduction engine is [`crate::eval_common::evaluate`]; grade adds only
/// the `--flag-if` pass on top. This is the testable core; `run`/
/// `run_captured` wrap it with loading + formatting.
fn grade_cube(
    cube: &mut mc_core::Cube,
    refs: &mc_model::ModelRefs,
    principal: mc_core::PrincipalId,
    cmd: &GradeCommand,
) -> Result<EvalReport, String> {
    let spec = EvalSpec {
        unit: &cmd.unit,
        holdout: cmd.holdout.as_deref(),
        group_by: &cmd.group_by,
        metrics: &cmd.metrics,
        buckets: &cmd.buckets,
        min_n: cmd.min_n,
        max_segments: cmd.max_segments,
        wilson_null: cmd.wilson_null,
    };
    let mut report = evaluate(cube, refs, principal, &spec)?;

    // --- Flag evaluation (grade-specific; ADR-0034 Amendment 9: skip
    //     below-min-n + out-of-range). The shared engine leaves
    //     `segments[*].flagged` empty and `flagged_count` at 0. ----------
    let flag_pred = match &cmd.flag_if {
        Some(s) => Some(FlagPredicate::parse(s, &report.metric_names)?),
        None => None,
    };
    let mut flagged_count = 0usize;
    if let Some(pred) = &flag_pred {
        for seg in &mut report.segments {
            if seg.status != SegmentStatus::Ok {
                continue;
            }
            let idx = pred.metric_index;
            if let Some(Some(value)) = seg.metrics.get(idx) {
                if pred.eval(*value) {
                    seg.flagged.push(cmd.flag_if.clone().unwrap_or_default());
                    flagged_count += 1;
                }
            }
        }
    }
    report.flagged_count = flagged_count;

    Ok(report)
}

// ===========================================================================
// Flag predicate (`--flag-if "<metric> <op> <value>"`)
// ===========================================================================

struct FlagPredicate {
    metric_index: usize,
    op: CmpOp,
    threshold: f64,
}

impl FlagPredicate {
    fn parse(s: &str, metric_names: &[String]) -> Result<FlagPredicate, String> {
        let toks: Vec<&str> = s.split_whitespace().collect();
        if toks.len() != 3 {
            return Err(format!(
                "--flag-if {s:?} must be '<metric> <op> <value>' (e.g. 'wr_lower_95 < 0.50')"
            ));
        }
        let metric_index = metric_names
            .iter()
            .position(|m| m == toks[0])
            .ok_or_else(|| {
                format!(
                    "--flag-if references unknown metric {:?}; defined metrics: {}",
                    toks[0],
                    metric_names.join(", ")
                )
            })?;
        let op = match toks[1] {
            "<" => CmpOp::Lt,
            "<=" => CmpOp::Lte,
            ">" => CmpOp::Gt,
            ">=" => CmpOp::Gte,
            "==" => CmpOp::Eq,
            "!=" => CmpOp::Neq,
            other => {
                return Err(format!(
                    "--flag-if operator {other:?} must be one of <, <=, >, >=, ==, !="
                ))
            }
        };
        let threshold: f64 = toks[2]
            .parse()
            .map_err(|_| format!("--flag-if threshold {:?} is not a number", toks[2]))?;
        Ok(FlagPredicate {
            metric_index,
            op,
            threshold,
        })
    }

    fn eval(&self, value: f64) -> bool {
        // `==` / `!=` use an epsilon, never raw float equality (CLAUDE.md §3.1).
        match self.op {
            CmpOp::Lt => value < self.threshold,
            CmpOp::Lte => value <= self.threshold,
            CmpOp::Gt => value > self.threshold,
            CmpOp::Gte => value >= self.threshold,
            CmpOp::Eq => (value - self.threshold).abs() < 1e-9,
            CmpOp::Neq => (value - self.threshold).abs() >= 1e-9,
        }
    }
}

// ===========================================================================
// Output formatting
// ===========================================================================

fn fmt_metric(value: Option<f64>, is_count: bool) -> String {
    match value {
        None => "null".to_string(),
        Some(v) if is_count => format!("{}", v.round() as i64),
        Some(v) => format_f64(v),
    }
}

fn format_report(cmd: &GradeCommand, report: &EvalReport) -> String {
    match cmd.format {
        GradeFormat::Text => format_text(cmd, report),
        GradeFormat::Json => format_json(cmd, report),
    }
}

fn format_text(cmd: &GradeCommand, report: &EvalReport) -> String {
    let mut out = String::new();
    let holdout = cmd.holdout.as_deref().unwrap_or("(all units)");
    let _ = writeln!(
        out,
        "SEGMENT GRADE: {}  (holdout: {}; unit: {})\n",
        cmd.path, holdout, cmd.unit
    );

    // Header columns: group-by keys, n, each metric, flag.
    // Phase 10B.1: suppress the built-in unit-count `n` column when the user
    // already defined a metric named `n` (e.g. `--metric "n=count(...)"`, the
    // canonical EXP-048 form) — otherwise the table renders two `n` columns.
    let show_unit_n = shows_unit_n(report);
    let mut headers: Vec<String> = report.group_by.clone();
    if show_unit_n {
        headers.push("n".to_string());
    }
    for name in &report.metric_names {
        headers.push(name.clone());
    }
    headers.push("flag".to_string());

    // Build rows (segments + TOTAL).
    let mut rows: Vec<Vec<String>> = Vec::new();
    for seg in &report.segments {
        rows.push(segment_row(seg, report));
    }
    let mut total_row: Vec<String> = vec!["TOTAL".to_string()];
    for _ in 1..report.group_by.len() {
        total_row.push(String::new());
    }
    if report.group_by.is_empty() {
        total_row = vec!["TOTAL".to_string()];
    }
    if show_unit_n {
        total_row.push(format!("{}", report.total.n_units));
    }
    for (i, v) in report.total.metrics.iter().enumerate() {
        total_row.push(fmt_metric(*v, report.metric_is_count[i]));
    }
    total_row.push(String::new());

    // Column widths.
    let ncols = headers.len();
    let mut widths = vec![0usize; ncols];
    for (i, h) in headers.iter().enumerate() {
        widths[i] = widths[i].max(h.len());
    }
    for row in rows.iter().chain(std::iter::once(&total_row)) {
        for (i, cell) in row.iter().enumerate() {
            if i < ncols {
                widths[i] = widths[i].max(cell.len());
            }
        }
    }

    let render = |row: &[String], out: &mut String| {
        let cells: Vec<String> = (0..ncols)
            .map(|i| {
                let cell = row.get(i).map(String::as_str).unwrap_or("");
                format!("{:<width$}", cell, width = widths[i])
            })
            .collect();
        let _ = writeln!(out, "{}", cells.join(" | ").trim_end());
    };
    let sep = |out: &mut String| {
        let parts: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
        let _ = writeln!(out, "{}", parts.join("-+-"));
    };

    render(&headers, &mut out);
    sep(&mut out);
    for row in &rows {
        render(row, &mut out);
    }
    sep(&mut out);
    render(&total_row, &mut out);

    if let Some(flag) = &cmd.flag_if {
        let _ = writeln!(
            out,
            "\n{} segment(s) flagged ({}).",
            report.flagged_count, flag
        );
    }
    if !report.warnings.is_empty() {
        out.push('\n');
        for w in &report.warnings {
            let _ = writeln!(out, "warning: {w}");
        }
    }
    out
}

/// True when the built-in unit-count column/key should be emitted: only when
/// the user has NOT defined a metric named `n` (Phase 10B.1 dedup).
fn shows_unit_n(report: &EvalReport) -> bool {
    !report.metric_names.iter().any(|m| m == "n")
}

/// One text-table row for a segment (group-by displays, n, metrics, flag).
fn segment_row(seg: &SegmentResult, report: &EvalReport) -> Vec<String> {
    let mut row: Vec<String> = seg.keys.iter().map(|(_, v)| v.clone()).collect();
    if report.group_by.is_empty() {
        row.push("(all)".to_string());
    }
    if shows_unit_n(report) {
        row.push(format!("{}", seg.n_units));
    }
    for (i, v) in seg.metrics.iter().enumerate() {
        row.push(fmt_metric(*v, report.metric_is_count[i]));
    }
    let flag_cell = match seg.status {
        SegmentStatus::BelowMinN => "(below min-n)".to_string(),
        SegmentStatus::OutOfRange => "(out-of-range)".to_string(),
        SegmentStatus::Ok => {
            if seg.flagged.is_empty() {
                String::new()
            } else {
                format!("FLAG: {}", seg.flagged.join("; "))
            }
        }
    };
    row.push(flag_cell);
    row
}

fn format_json(cmd: &GradeCommand, report: &EvalReport) -> String {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"schema_version\": \"1.0\",\n");
    out.push_str("  \"cartridge\": ");
    push_json_str(&mut out, &cmd.path);
    out.push_str(",\n  \"holdout\": ");
    match &cmd.holdout {
        Some(h) => push_json_str(&mut out, h),
        None => out.push_str("null"),
    }
    out.push_str(",\n  \"unit\": ");
    push_json_str(&mut out, &cmd.unit);
    out.push_str(",\n  \"group_by\": [");
    for (i, g) in report.group_by.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        push_json_str(&mut out, g);
    }
    out.push_str("],\n");

    // bucket metadata
    out.push_str("  \"bucket\": {");
    let mut first = true;
    for (measure, edges) in &report.bucket_meta {
        if !first {
            out.push_str(", ");
        }
        first = false;
        push_json_str(&mut out, measure);
        out.push_str(": [");
        for (i, e) in edges.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&fmt_edge(*e));
        }
        out.push(']');
    }
    out.push_str("},\n");

    // segments
    out.push_str("  \"segments\": [\n");
    for (si, seg) in report.segments.iter().enumerate() {
        push_segment_json(&mut out, seg, report);
        if si + 1 < report.segments.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ],\n");

    // total
    out.push_str("  \"total\": ");
    push_metrics_obj(&mut out, &report.total, report);
    out.push_str(",\n");

    // warnings
    out.push_str("  \"warnings\": [");
    for (i, w) in report.warnings.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        push_json_str(&mut out, w);
    }
    out.push_str("],\n");

    // denominator_zero_segments
    out.push_str("  \"denominator_zero_segments\": [");
    for (i, keys) in report.denom_zero_segments.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        push_keys_obj(&mut out, keys);
    }
    out.push_str("],\n");

    let _ = writeln!(out, "  \"flagged_count\": {},", report.flagged_count);
    // Reserved for additive growth (Amendment 5 / Q6 deferral).
    out.push_str("  \"subtotals\": []\n");
    out.push_str("}\n");
    out
}

fn push_keys_obj(out: &mut String, keys: &[(String, String)]) {
    out.push('{');
    for (i, (k, v)) in keys.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        push_json_str(out, k);
        out.push_str(": ");
        push_json_str(out, v);
    }
    out.push('}');
}

fn push_metric_value(out: &mut String, v: Option<f64>, is_count: bool) {
    match v {
        None => out.push_str("null"),
        Some(val) if is_count => {
            let _ = write!(out, "{}", val.round() as i64);
        }
        Some(val) => out.push_str(&format_f64(val)),
    }
}

/// Emit `{ "n": N, "<metric>": v, ... }` for a segment/total.
///
/// Phase 10B.1: the built-in `"n"` (unit count) is suppressed when a metric
/// named `n` exists, so the object never carries a duplicate `"n"` key.
fn push_metrics_obj(out: &mut String, seg: &SegmentResult, report: &EvalReport) {
    out.push('{');
    let mut first = true;
    if shows_unit_n(report) {
        let _ = write!(out, " \"n\": {}", seg.n_units);
        first = false;
    }
    for (i, name) in report.metric_names.iter().enumerate() {
        if !first {
            out.push(',');
        }
        out.push(' ');
        push_json_str(out, name);
        out.push_str(": ");
        push_metric_value(out, seg.metrics[i], report.metric_is_count[i]);
        first = false;
    }
    out.push_str(" }");
}

fn push_segment_json(out: &mut String, seg: &SegmentResult, report: &EvalReport) {
    out.push_str("    { \"keys\": ");
    push_keys_obj(out, &seg.keys);
    out.push_str(", \"metrics\": ");
    push_metrics_obj(out, seg, report);
    out.push_str(", \"status\": ");
    push_json_str(out, seg.status.as_str());
    out.push_str(", \"null_counts\": {");
    for (i, (k, v)) in seg.null_counts.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        push_json_str(out, k);
        let _ = write!(out, ": {v}");
    }
    out.push_str("}, \"flagged\": [");
    for (i, f) in seg.flagged.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        push_json_str(out, f);
    }
    out.push_str("] }");
}

// ===========================================================================
// Entry points
// ===========================================================================

/// Execute `mc model grade` and print the result.
pub fn run(cmd: GradeCommand) -> i32 {
    let (code, output) = run_captured(cmd);
    if !output.is_empty() {
        print!("{output}");
    }
    code
}

/// Execute and return `(exit_code, output)`. Used by MCP to capture output.
pub fn run_captured(cmd: GradeCommand) -> (i32, String) {
    // Amendment 8: Reproducible by default; --include-writes folds in
    // operational `.tessera/writes.jsonl` post-hoc writes.
    let policy = if cmd.include_writes {
        LoadPolicy::CurrentReality
    } else {
        LoadPolicy::Reproducible
    };
    let loaded = match load_model_with_policy(&cmd.path, policy) {
        Ok(l) => l,
        Err(e) => return (e.exit_code(), format!("error: {}\n", e.message())),
    };
    let mut cube = loaded.cube;
    let refs = &loaded.refs;
    let principal = loaded.root_principal;

    match grade_cube(&mut cube, refs, principal, &cmd) {
        Ok(report) => (0, format_report(&cmd, &report)),
        Err(e) => (1, format!("error: {e}\n")),
    }
}

#[cfg(test)]
mod tests {
    include!("grade_tests.rs");
}
