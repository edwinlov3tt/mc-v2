//! `mc model grade` — segmented holdout evaluation (Phase 10B, ADR-0034).
//!
//! Groups a holdout set by one or more attributes (a dimension, a
//! discrete/string measure value, or a bucketed continuous measure),
//! computes per-segment metrics via the Phase 10A primitives, flags
//! segments crossing a threshold, and emits a text table + JSON. This
//! reproduces claw-core's EXP-048 segment-table workflow in one command.
//!
//! Per ADR-0034 Amendment 4, the grouped-reduction engine lives entirely
//! in `mc-cli` — it *composes* the existing 10A primitives by restricting
//! the per-leaf eval traversal to a segment's leaves. There is no
//! `mc-core` change.
//!
//! Binding amendments folded in (see ADR-0034 "Acceptance amendments"):
//! - A1: `--holdout` reuses the existing [`crate::query::Filter`] grammar;
//!   bare F64-measure equality is a hard error.
//! - A2: continuous-F64 `--group-by` requires `--bucket`; `--max-segments`
//!   caps the segment count (default 50). (No discrete-measure metadata
//!   exists in `mc-model`, so the documented fallback applies: F64 always
//!   needs a bucket; non-F64/string measures group by distinct value.)
//! - A3: Wilson Null indicator → hard error by default (`--wilson-null`).
//! - A5: expanded JSON schema (status, null_counts, warnings, bucket
//!   metadata, denominator_zero_segments, reserved subtotals).
//! - A6: `ratio` denom-zero → Null + diagnostic, never inf/NaN/0.
//! - A7: 9 reductions (count/mean/sum/ratio/std/min/max/wilson_*).
//! - A8: `LoadPolicy::Reproducible` default; `--include-writes` opt-in.
//! - A9: TOTAL row inclusive of min-n-excluded segments.
//! - A11: formal metric-expression grammar + error UX.
//! - A12: lexicographic segment ordering, first group-by flag slowest.

use crate::loader::{load_model_with_policy, LoadPolicy};
use crate::query::{
    enumerate_leaf_coords, eval_filter, format_f64, push_json_str, read_measure_at, CmpOp, Filter,
    FilterAtom, FilterValue,
};
use mc_core::rule::{wilson_ci_lower_compute, wilson_ci_upper_compute};
use mc_core::ScalarValue;
use std::collections::BTreeMap;
use std::fmt::Write;

/// Float-zero threshold for `ratio` denominators (CLAUDE.md §7 / Amdt 6).
const ZERO_EPS: f64 = 1e-300;

// ===========================================================================
// Metric-expression grammar (Amendment 11)
// ===========================================================================

/// One reduction in the closed metric vocabulary (Amendment 7: 9 total).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reduction {
    Count,
    Mean,
    Sum,
    Ratio,
    Std,
    Min,
    Max,
    WilsonLower,
    WilsonUpper,
}

impl Reduction {
    /// The canonical spelling used in `--metric` expressions and in error
    /// messages.
    fn name(self) -> &'static str {
        match self {
            Reduction::Count => "count",
            Reduction::Mean => "mean",
            Reduction::Sum => "sum",
            Reduction::Ratio => "ratio",
            Reduction::Std => "std",
            Reduction::Min => "min",
            Reduction::Max => "max",
            Reduction::WilsonLower => "wilson_lower",
            Reduction::WilsonUpper => "wilson_upper",
        }
    }

    /// Number of ingredient measures this reduction consumes (Amdt 11
    /// arity rule: `ratio` = 2, all others = 1).
    fn arity(self) -> usize {
        match self {
            Reduction::Ratio => 2,
            _ => 1,
        }
    }

    fn from_str(s: &str) -> Option<Reduction> {
        Some(match s {
            "count" => Reduction::Count,
            "mean" => Reduction::Mean,
            "sum" => Reduction::Sum,
            "ratio" => Reduction::Ratio,
            "std" => Reduction::Std,
            "min" => Reduction::Min,
            "max" => Reduction::Max,
            "wilson_lower" => Reduction::WilsonLower,
            "wilson_upper" => Reduction::WilsonUpper,
            _ => return None,
        })
    }
}

/// A parsed `name=reduction(ingredient[,ingredient])` metric.
#[derive(Debug, Clone)]
pub struct MetricExpr {
    pub name: String,
    pub reduction: Reduction,
    pub ingredients: Vec<String>,
}

const REDUCTION_LIST: &str = "count, mean, sum, ratio, std, min, max, wilson_lower, wilson_upper";

/// Parse one metric expression per the Amendment 11 grammar:
///
/// ```text
/// metric_expr := IDENT '=' REDUCTION_NAME '(' ingredient (',' ingredient)* ')'
/// ```
///
/// Whitespace is tolerated around `=`, `,`, and the parens but not within
/// identifiers. Ingredient existence in the cartridge is validated later
/// (in [`grade_cube`]) where the cube is available; this function owns the
/// grammar + arity + reduction-name checks.
pub fn parse_metric_expr(input: &str) -> Result<MetricExpr, String> {
    let trimmed = input.trim();
    let eq = trimmed.find('=').ok_or_else(|| {
        format!("metric {input:?} must be 'name=reduction(ingredient[,ingredient])'")
    })?;
    let name = trimmed[..eq].trim();
    let rest = trimmed[eq + 1..].trim();
    if name.is_empty() {
        return Err(format!("metric {input:?} is missing a name before '='"));
    }
    if !is_bare_ident(name) {
        return Err(format!(
            "metric name {name:?} must be a bare identifier (letters, digits, underscore)"
        ));
    }

    let open = rest
        .find('(')
        .ok_or_else(|| format!("metric {input:?}: expected 'reduction(...)' after '='"))?;
    if !rest.ends_with(')') {
        return Err(format!("metric {input:?}: missing closing ')'"));
    }
    let reduction_name = rest[..open].trim();
    let reduction = Reduction::from_str(reduction_name).ok_or_else(|| {
        format!("unknown reduction {reduction_name:?}; expected one of: {REDUCTION_LIST}")
    })?;

    let args_str = &rest[open + 1..rest.len() - 1];
    let ingredients: Vec<String> = args_str.split(',').map(|a| a.trim().to_string()).collect();
    if ingredients.iter().any(|a| a.is_empty()) {
        return Err(format!(
            "metric {input:?}: empty ingredient (check for a stray comma)"
        ));
    }
    for ing in &ingredients {
        if !is_bare_ident(ing) {
            return Err(format!(
                "metric {input:?}: ingredient {ing:?} must be a bare measure name"
            ));
        }
    }
    let want = reduction.arity();
    if ingredients.len() != want {
        return Err(format!(
            "{}() takes exactly {} ingredient{}, got {} in {input:?}",
            reduction.name(),
            want,
            if want == 1 { "" } else { "s" },
            ingredients.len()
        ));
    }

    Ok(MetricExpr {
        name: name.to_string(),
        reduction,
        ingredients,
    })
}

/// Bare-identifier rule shared by metric names and ingredients: non-empty,
/// ASCII alphanumeric or `_`, not starting with a digit.
fn is_bare_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// ===========================================================================
// Command struct + CLI parsing
// ===========================================================================

/// Output format for `grade` (Decision 1: `text | json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradeFormat {
    Text,
    Json,
}

/// Policy for Wilson reductions when the indicator has Null values in a
/// segment (Amendment 3). Defaults to `Error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WilsonNullPolicy {
    Error,
    Drop,
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

/// Parse a colon-separated, strictly-ascending edge list (`0:0.03:0.10`).
/// Requires at least 2 edges (one band).
fn parse_bucket_edges(s: &str) -> Result<Vec<f64>, String> {
    let edges: Vec<f64> = s
        .split(':')
        .map(|p| {
            p.trim()
                .parse::<f64>()
                .map_err(|_| format!("invalid edge {p:?} (expected a number)"))
        })
        .collect::<Result<_, _>>()?;
    if edges.len() < 2 {
        return Err("need at least 2 edges to define a band".into());
    }
    for w in edges.windows(2) {
        // Require strict ascent: the next edge must exceed the current one
        // (a non-strict `<=` would allow a zero-width band).
        if w[1] <= w[0] {
            return Err(format!(
                "edges must be strictly ascending, got {} then {}",
                w[0], w[1]
            ));
        }
    }
    Ok(edges)
}

// ===========================================================================
// Bucket assignment (Amendment 2 / Decision 2)
// ===========================================================================

/// The outcome of assigning a continuous value to a bucket band.
#[derive(Debug, Clone, PartialEq)]
enum BandAssignment {
    /// Band index `i` covering `[edges[i], edges[i+1])` (last band is
    /// right-closed). `lower` is the band's lower edge (the sort key).
    Band { label: String, lower: f64 },
    /// Value fell outside every band — surfaced, never silently dropped.
    OutOfRange,
}

/// Assign `value` to a left-closed / right-open band (the final band is
/// right-closed), per Decision 2. Uses only range comparisons (`>=`/`<`/`<=`)
/// — never float `==`.
fn assign_bucket(value: f64, edges: &[f64]) -> BandAssignment {
    let n_bands = edges.len() - 1;
    for i in 0..n_bands {
        let lo = edges[i];
        let hi = edges[i + 1];
        let last = i == n_bands - 1;
        let in_band = if last {
            value >= lo && value <= hi
        } else {
            value >= lo && value < hi
        };
        if in_band {
            let close = if last { ']' } else { ')' };
            let label = format!("[{},{}{}", format_f64(lo), format_f64(hi), close);
            return BandAssignment::Band { label, lower: lo };
        }
    }
    BandAssignment::OutOfRange
}

// ===========================================================================
// Holdout F64-equality guard (Amendment 1)
// ===========================================================================

/// Walk a parsed holdout [`Filter`] and reject bare equality / inequality
/// against a numeric literal on a *measure* atom. No discrete-measure
/// metadata exists in `mc-model`, so every measure is treated as
/// continuous F64 for this guard: `line == 9.0` is a hard error. Dimension
/// pins (`Time == "2025"`), string-valued measure equality, and range
/// predicates (`line >= 8.99 and line <= 9.01`) are all allowed.
fn guard_filter_f64_equality(filter: &Filter) -> Result<(), String> {
    match filter {
        Filter::And(l, r) | Filter::Or(l, r) => {
            guard_filter_f64_equality(l)?;
            guard_filter_f64_equality(r)
        }
        Filter::Not(inner) => guard_filter_f64_equality(inner),
        Filter::Compare(atom, op, value) => {
            let is_eq = matches!(op, CmpOp::Eq | CmpOp::Neq);
            let measure_atom = matches!(atom, FilterAtom::Measure(_));
            let numeric_rhs = matches!(value, FilterValue::Number(_));
            if is_eq && measure_atom && numeric_rhs {
                let name = match atom {
                    FilterAtom::Measure(n) => n.as_str(),
                    _ => "?",
                };
                return Err(format!(
                    "holdout filter uses bare equality on F64 measure {name:?}: float `==` is \
                     hazardous and no discrete-marking exists. Use a range \
                     ({name} >= LO and {name} <= HI) or an explicit tolerance instead."
                ));
            }
            Ok(())
        }
        // Function-call-shaped predicates (parsed into Filter::Expr) are
        // not the documented holdout hazard; leave them to the formula
        // layer's own validation.
        Filter::Expr(_) => Ok(()),
    }
}

// ===========================================================================
// Grouped-reduction engine (the core — Amendment 4: stays in mc-cli)
// ===========================================================================

/// How a `--group-by` key resolves against the cube.
enum GroupKind {
    /// A non-Measure dimension: the segment is the element name at that slot.
    Dimension { dim_index: usize },
    /// A measure: the segment is the per-leaf value (string/category direct,
    /// continuous-F64 bucketed).
    Measure { name: String },
}

/// A segment's classification for reporting (Amendment 5 `status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SegmentStatus {
    Ok,
    BelowMinN,
    OutOfRange,
}

impl SegmentStatus {
    fn as_str(self) -> &'static str {
        match self {
            SegmentStatus::Ok => "ok",
            SegmentStatus::BelowMinN => "below_min_n",
            SegmentStatus::OutOfRange => "out_of_range",
        }
    }
}

/// A computed segment row.
#[derive(Debug)]
struct SegmentResult {
    /// `(group_by_key, display_value)` pairs in flag order.
    keys: Vec<(String, String)>,
    n_units: usize,
    /// One metric value per `report.metric_names`, in order. `None` = Null.
    metrics: Vec<Option<f64>>,
    /// Per-ingredient Null counts (Amendment 5).
    null_counts: BTreeMap<String, usize>,
    status: SegmentStatus,
    flagged: Vec<String>,
}

/// The full grade result, ready to format.
#[derive(Debug)]
struct GradeReport {
    metric_names: Vec<String>,
    /// True for each metric whose reduction is `count` (integer display).
    metric_is_count: Vec<bool>,
    group_by: Vec<String>,
    segments: Vec<SegmentResult>,
    total: SegmentResult,
    warnings: Vec<String>,
    /// Display key-vecs of segments where a `ratio` denominator was zero.
    denom_zero_segments: Vec<Vec<(String, String)>>,
    /// Buckets actually applied (measure → edges), for JSON metadata.
    bucket_meta: BTreeMap<String, Vec<f64>>,
    flagged_count: usize,
}

/// Run the grade analysis against a loaded cube. This is the testable core;
/// `run`/`run_captured` wrap it with loading + formatting.
fn grade_cube(
    cube: &mut mc_core::Cube,
    refs: &mc_model::ModelRefs,
    principal: mc_core::PrincipalId,
    cmd: &GradeCommand,
) -> Result<GradeReport, String> {
    // --- Validate --unit names a real, non-Measure dimension. ------------
    let unit_ok = cube
        .dimensions()
        .iter()
        .any(|d| d.name == cmd.unit && d.kind != mc_core::DimensionKind::Measure);
    if !unit_ok {
        return Err(format!(
            "--unit {:?} is not a dimension in this cartridge",
            cmd.unit
        ));
    }

    // --- Validate metric ingredients exist as measures. ------------------
    for m in &cmd.metrics {
        for ing in &m.ingredients {
            if !is_measure(cube, ing) {
                return Err(format!(
                    "metric {:?}: ingredient {:?} is not a measure in this cartridge",
                    m.name, ing
                ));
            }
        }
    }

    // --- Classify each group-by key. -------------------------------------
    let mut kinds: Vec<GroupKind> = Vec::with_capacity(cmd.group_by.len());
    for key in &cmd.group_by {
        if let Some(dim_index) = cube
            .dimensions()
            .iter()
            .position(|d| d.name == *key && d.kind != mc_core::DimensionKind::Measure)
        {
            kinds.push(GroupKind::Dimension { dim_index });
        } else if is_measure(cube, key) {
            kinds.push(GroupKind::Measure { name: key.clone() });
        } else {
            return Err(format!(
                "--group-by {key:?} is neither a dimension nor a measure in this cartridge"
            ));
        }
    }

    // --- Parse + guard the holdout filter. -------------------------------
    let filter = match &cmd.holdout {
        Some(expr) => {
            let f = Filter::parse(expr, refs, cube)
                .map_err(|e| format!("invalid --holdout filter: {e}"))?;
            guard_filter_f64_equality(&f)?;
            Some(f)
        }
        None => None,
    };

    // --- Collect the unit leaves (holdout-filtered). ---------------------
    let all_coords = enumerate_leaf_coords(cube, refs);
    let unit_coords: Vec<mc_core::CellCoordinate> = match &filter {
        Some(f) => all_coords
            .into_iter()
            .filter(|c| eval_filter(f, c, cube, principal, refs))
            .collect(),
        None => all_coords,
    };

    let mut warnings: Vec<String> = Vec::new();

    // --- Assign each unit to a segment. ----------------------------------
    // Group key = the vec of display strings; we keep a parallel sort-key
    // vec for deterministic ordering (Amendment 12). HashMap-free: a Vec
    // of builders keyed by linear scan keeps ordering fully under our
    // control and segment counts are small (capped by --max-segments).
    struct SegBuild {
        displays: Vec<String>,
        sorts: Vec<SortKey>,
        coords: Vec<mc_core::CellCoordinate>,
        out_of_range: bool,
    }
    let mut builds: Vec<SegBuild> = Vec::new();

    for coord in &unit_coords {
        let mut displays: Vec<String> = Vec::with_capacity(kinds.len());
        let mut sorts: Vec<SortKey> = Vec::with_capacity(kinds.len());
        let mut any_oor = false;

        for (key_name, kind) in cmd.group_by.iter().zip(kinds.iter()) {
            let (display, sort, oor) = resolve_group_component(
                coord,
                key_name,
                kind,
                &cmd.buckets,
                cube,
                refs,
                principal,
            )?;
            if oor {
                any_oor = true;
            }
            displays.push(display);
            sorts.push(sort);
        }

        if let Some(b) = builds.iter_mut().find(|b| b.displays == displays) {
            b.coords.push(coord.clone());
        } else {
            builds.push(SegBuild {
                displays,
                sorts,
                coords: vec![coord.clone()],
                out_of_range: any_oor,
            });
        }
    }

    // --- Enforce --max-segments (Amendment 2). ---------------------------
    if builds.len() > cmd.max_segments {
        return Err(format!(
            "resolved {} segments, which exceeds --max-segments {} — narrow the holdout, \
             coarsen the buckets, or raise --max-segments",
            builds.len(),
            cmd.max_segments
        ));
    }

    // --- Deterministic ordering (Amendment 12): lexicographic by group-by
    //     flag order, first flag slowest; out-of-range / null components
    //     sort last within their column. ---------------------------------
    builds.sort_by(|a, b| cmp_sort_vecs(&a.sorts, &b.sorts));

    // --- The set of ingredient measures, for null_counts. ----------------
    let mut ingredient_set: Vec<String> = Vec::new();
    for m in &cmd.metrics {
        for ing in &m.ingredients {
            if !ingredient_set.contains(ing) {
                ingredient_set.push(ing.clone());
            }
        }
    }

    let metric_names: Vec<String> = cmd.metrics.iter().map(|m| m.name.clone()).collect();
    let metric_is_count: Vec<bool> = cmd
        .metrics
        .iter()
        .map(|m| m.reduction == Reduction::Count)
        .collect();

    // --- Reduce each segment. --------------------------------------------
    let mut segments: Vec<SegmentResult> = Vec::with_capacity(builds.len());
    let mut denom_zero_segments: Vec<Vec<(String, String)>> = Vec::new();

    for b in &builds {
        let keys: Vec<(String, String)> = cmd
            .group_by
            .iter()
            .cloned()
            .zip(b.displays.iter().cloned())
            .collect();

        let reduced = reduce_segment(
            &b.coords,
            &cmd.metrics,
            &ingredient_set,
            cube,
            refs,
            principal,
            cmd.wilson_null,
            &b.displays,
            &mut warnings,
        )?;

        let status = if b.out_of_range {
            SegmentStatus::OutOfRange
        } else if b.coords.len() < cmd.min_n {
            SegmentStatus::BelowMinN
        } else {
            SegmentStatus::Ok
        };

        if reduced.denom_zero {
            denom_zero_segments.push(keys.clone());
        }

        segments.push(SegmentResult {
            keys,
            n_units: b.coords.len(),
            metrics: reduced.metrics,
            null_counts: reduced.null_counts,
            status,
            flagged: Vec::new(),
        });
    }

    // --- TOTAL row: aggregate over ALL holdout units (Amendment 9 —
    //     inclusive of min-n-excluded and out-of-range segments). ---------
    let total_reduced = reduce_segment(
        &unit_coords,
        &cmd.metrics,
        &ingredient_set,
        cube,
        refs,
        principal,
        cmd.wilson_null,
        &["TOTAL".to_string()],
        &mut warnings,
    )?;
    let total = SegmentResult {
        keys: Vec::new(),
        n_units: unit_coords.len(),
        metrics: total_reduced.metrics,
        null_counts: total_reduced.null_counts,
        status: SegmentStatus::Ok,
        flagged: Vec::new(),
    };

    // --- Flag evaluation (Amendment 9: skip below-min-n + out-of-range). -
    let flag_pred = match &cmd.flag_if {
        Some(s) => Some(FlagPredicate::parse(s, &metric_names)?),
        None => None,
    };
    let mut flagged_count = 0usize;
    if let Some(pred) = &flag_pred {
        for seg in &mut segments {
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

    Ok(GradeReport {
        metric_names,
        metric_is_count,
        group_by: cmd.group_by.clone(),
        segments,
        total,
        warnings,
        denom_zero_segments,
        bucket_meta: cmd.buckets.clone(),
        flagged_count,
    })
}

/// Result of reducing one coord set across all metrics.
struct ReducedSegment {
    metrics: Vec<Option<f64>>,
    null_counts: BTreeMap<String, usize>,
    denom_zero: bool,
}

/// Apply every metric to one segment's coordinate set.
#[allow(clippy::too_many_arguments)]
fn reduce_segment(
    coords: &[mc_core::CellCoordinate],
    metrics: &[MetricExpr],
    ingredient_set: &[String],
    cube: &mut mc_core::Cube,
    refs: &mc_model::ModelRefs,
    principal: mc_core::PrincipalId,
    wilson_null: WilsonNullPolicy,
    seg_label: &[String],
    warnings: &mut Vec<String>,
) -> Result<ReducedSegment, String> {
    // Cache each ingredient's collected column once per segment.
    let mut columns: BTreeMap<String, Column> = BTreeMap::new();
    for ing in ingredient_set {
        columns.insert(
            ing.clone(),
            collect_column(coords, ing, cube, refs, principal),
        );
    }

    let null_counts: BTreeMap<String, usize> = columns
        .iter()
        .map(|(name, col)| (name.clone(), col.nulls))
        .collect();

    let mut out: Vec<Option<f64>> = Vec::with_capacity(metrics.len());
    let mut denom_zero = false;

    for m in metrics {
        let value = match m.reduction {
            Reduction::Count => {
                let col = &columns[&m.ingredients[0]];
                Some(col.values.len() as f64)
            }
            Reduction::Mean => reduce_mean(&columns[&m.ingredients[0]].values),
            Reduction::Sum => Some(reduce_sum(&columns[&m.ingredients[0]].values)),
            Reduction::Std => reduce_std(&columns[&m.ingredients[0]].values),
            Reduction::Min => reduce_min(&columns[&m.ingredients[0]].values),
            Reduction::Max => reduce_max(&columns[&m.ingredients[0]].values),
            Reduction::Ratio => {
                let num = &columns[&m.ingredients[0]].values;
                let den = &columns[&m.ingredients[1]].values;
                let den_sum = reduce_sum(den);
                if den.is_empty() || den_sum.abs() < ZERO_EPS {
                    // Per Amendment 6: Null + diagnostic, never inf/NaN/0.
                    denom_zero = true;
                    warnings.push(format!(
                        "ratio({}, {}) in segment {}: denominator sum is zero/empty — value is Null",
                        m.ingredients[0],
                        m.ingredients[1],
                        seg_label.join(",")
                    ));
                    None
                } else {
                    Some(reduce_sum(num) / den_sum)
                }
            }
            Reduction::WilsonLower | Reduction::WilsonUpper => {
                let ing = &m.ingredients[0];
                let col = &columns[ing];
                // Wilson Null-indicator contract (Amendment 3).
                if col.nulls > 0 {
                    match wilson_null {
                        WilsonNullPolicy::Error => {
                            let total = col.nulls + col.values.len();
                            return Err(format!(
                                "{}({ing}): {} of {} units in segment {} have Null {ing}. Wilson n \
                                 requires a non-Null 1.0/0.0 indicator for every unit. Fix the \
                                 indicator (use if(cond, 1.0, 0.0) — never Null), or pass \
                                 --wilson-null drop to exclude Null units (changes n).",
                                m.reduction.name(),
                                col.nulls,
                                total,
                                seg_label.join(",")
                            ));
                        }
                        WilsonNullPolicy::Drop => {
                            warnings.push(format!(
                                "{}({ing}) in segment {}: dropped {} Null unit(s); Wilson n = {}",
                                m.reduction.name(),
                                seg_label.join(","),
                                col.nulls,
                                col.values.len()
                            ));
                        }
                    }
                }
                let n = col.values.len();
                if n == 0 {
                    None
                } else {
                    let p = reduce_sum(&col.values) / n as f64;
                    let nf = n as f64;
                    if m.reduction == Reduction::WilsonLower {
                        wilson_ci_lower_compute(p, nf)
                    } else {
                        wilson_ci_upper_compute(p, nf)
                    }
                }
            }
        };
        out.push(value);
    }

    Ok(ReducedSegment {
        metrics: out,
        null_counts,
        denom_zero,
    })
}

/// A measure's evaluated values over a coordinate set, with the count of
/// units whose value was Null / non-numeric.
struct Column {
    values: Vec<f64>,
    nulls: usize,
}

/// Read `measure` at every coord, splitting F64 values from Null/non-numeric.
fn collect_column(
    coords: &[mc_core::CellCoordinate],
    measure: &str,
    cube: &mut mc_core::Cube,
    refs: &mc_model::ModelRefs,
    principal: mc_core::PrincipalId,
) -> Column {
    let mut values = Vec::with_capacity(coords.len());
    let mut nulls = 0usize;
    for coord in coords {
        match read_measure_at(cube, refs, principal, coord, measure) {
            ScalarValue::F64(v) => values.push(v),
            _ => nulls += 1,
        }
    }
    Column { values, nulls }
}

fn reduce_sum(values: &[f64]) -> f64 {
    values.iter().sum()
}

fn reduce_mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

/// Sample standard deviation (ddof=1), matching Phase 10A `std_over`.
fn reduce_std(values: &[f64]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let ss: f64 = values.iter().map(|v| (v - mean) * (v - mean)).sum();
    Some((ss / (n - 1.0)).sqrt())
}

fn reduce_min(values: &[f64]) -> Option<f64> {
    values.iter().copied().fold(None, |acc, v| match acc {
        None => Some(v),
        Some(m) => Some(if v < m { v } else { m }),
    })
}

fn reduce_max(values: &[f64]) -> Option<f64> {
    values.iter().copied().fold(None, |acc, v| match acc {
        None => Some(v),
        Some(m) => Some(if v > m { v } else { m }),
    })
}

/// Resolve one group-by component for a unit leaf. Returns
/// `(display, sort_key, is_out_of_range)`.
#[allow(clippy::too_many_arguments)]
fn resolve_group_component(
    coord: &mc_core::CellCoordinate,
    key_name: &str,
    kind: &GroupKind,
    buckets: &BTreeMap<String, Vec<f64>>,
    cube: &mut mc_core::Cube,
    refs: &mc_model::ModelRefs,
    principal: mc_core::PrincipalId,
) -> Result<(String, SortKey, bool), String> {
    match kind {
        GroupKind::Dimension { dim_index } => {
            let elem_id = coord.elements()[*dim_index];
            let name = cube.dimensions()[*dim_index]
                .element(elem_id)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| "(unknown)".to_string());
            Ok((name.clone(), SortKey::Text(name), false))
        }
        GroupKind::Measure { name } => {
            let value = read_measure_at(cube, refs, principal, coord, name);
            match value {
                ScalarValue::Str(s) => Ok((s.clone(), SortKey::Text(s), false)),
                ScalarValue::Category(c) => {
                    let label = format!("cat:{c}");
                    Ok((label.clone(), SortKey::Text(label), false))
                }
                ScalarValue::Bool(b) => {
                    let label = if b { "true" } else { "false" }.to_string();
                    Ok((label.clone(), SortKey::Text(label), false))
                }
                ScalarValue::Null => Ok(("(null)".to_string(), SortKey::Special(2), false)),
                ScalarValue::F64(v) => match buckets.get(key_name) {
                    None => Err(format!(
                        "--group-by {key_name:?} is a continuous measure; provide \
                         --bucket {key_name} <edges> to group it (no discrete-marking exists)"
                    )),
                    Some(edges) => match assign_bucket(v, edges) {
                        BandAssignment::Band { label, lower } => {
                            Ok((label, SortKey::Num(lower), false))
                        }
                        BandAssignment::OutOfRange => {
                            Ok(("(out-of-range)".to_string(), SortKey::Special(1), true))
                        }
                    },
                },
                ScalarValue::I64(v) => match buckets.get(key_name) {
                    None => Err(format!(
                        "--group-by {key_name:?} is a numeric measure; provide \
                         --bucket {key_name} <edges> to group it"
                    )),
                    Some(edges) => match assign_bucket(v as f64, edges) {
                        BandAssignment::Band { label, lower } => {
                            Ok((label, SortKey::Num(lower), false))
                        }
                        BandAssignment::OutOfRange => {
                            Ok(("(out-of-range)".to_string(), SortKey::Special(1), true))
                        }
                    },
                },
            }
        }
    }
}

/// Sort key for one segment-key component (Amendment 12). Numeric bands
/// sort by lower edge; categorical values sort lexicographically; special
/// catch-all segments (`(out-of-range)`, `(null)`) sort last.
#[derive(Debug, Clone)]
enum SortKey {
    Num(f64),
    Text(String),
    Special(u8),
}

fn rank(k: &SortKey) -> u8 {
    match k {
        SortKey::Num(_) => 0,
        SortKey::Text(_) => 0,
        SortKey::Special(_) => 1,
    }
}

fn cmp_sort_keys(a: &SortKey, b: &SortKey) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (ra, rb) = (rank(a), rank(b));
    if ra != rb {
        return ra.cmp(&rb);
    }
    match (a, b) {
        (SortKey::Num(x), SortKey::Num(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (SortKey::Text(x), SortKey::Text(y)) => x.cmp(y),
        (SortKey::Special(x), SortKey::Special(y)) => x.cmp(y),
        // Mixed Num/Text within a column shouldn't occur (a column is one
        // kind), but define a stable tiebreak anyway.
        (SortKey::Num(_), SortKey::Text(_)) => Ordering::Less,
        (SortKey::Text(_), SortKey::Num(_)) => Ordering::Greater,
        _ => Ordering::Equal,
    }
}

fn cmp_sort_vecs(a: &[SortKey], b: &[SortKey]) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    for (x, y) in a.iter().zip(b.iter()) {
        let c = cmp_sort_keys(x, y);
        if c != Ordering::Equal {
            return c;
        }
    }
    a.len().cmp(&b.len())
}

/// True if `name` is a measure (an element of the Measure dimension).
fn is_measure(cube: &mc_core::Cube, name: &str) -> bool {
    cube.dimensions()
        .iter()
        .find(|d| d.kind == mc_core::DimensionKind::Measure)
        .map(|d| d.element_by_name(name).is_some())
        .unwrap_or(false)
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

fn format_report(cmd: &GradeCommand, report: &GradeReport) -> String {
    match cmd.format {
        GradeFormat::Text => format_text(cmd, report),
        GradeFormat::Json => format_json(cmd, report),
    }
}

fn format_text(cmd: &GradeCommand, report: &GradeReport) -> String {
    let mut out = String::new();
    let holdout = cmd.holdout.as_deref().unwrap_or("(all units)");
    let _ = writeln!(
        out,
        "SEGMENT GRADE: {}  (holdout: {}; unit: {})\n",
        cmd.path, holdout, cmd.unit
    );

    // Header columns: group-by keys, n, each metric, flag.
    let mut headers: Vec<String> = report.group_by.clone();
    headers.push("n".to_string());
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
    total_row.push(format!("{}", report.total.n_units));
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

/// One text-table row for a segment (group-by displays, n, metrics, flag).
fn segment_row(seg: &SegmentResult, report: &GradeReport) -> Vec<String> {
    let mut row: Vec<String> = seg.keys.iter().map(|(_, v)| v.clone()).collect();
    if report.group_by.is_empty() {
        row.push("(all)".to_string());
    }
    row.push(format!("{}", seg.n_units));
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

fn format_json(cmd: &GradeCommand, report: &GradeReport) -> String {
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
            out.push_str(&format_f64(*e));
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
fn push_metrics_obj(out: &mut String, seg: &SegmentResult, report: &GradeReport) {
    out.push_str("{ \"n\": ");
    let _ = write!(out, "{}", seg.n_units);
    for (i, name) in report.metric_names.iter().enumerate() {
        out.push_str(", ");
        push_json_str(out, name);
        out.push_str(": ");
        push_metric_value(out, seg.metrics[i], report.metric_is_count[i]);
    }
    out.push_str(" }");
}

fn push_segment_json(out: &mut String, seg: &SegmentResult, report: &GradeReport) {
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
