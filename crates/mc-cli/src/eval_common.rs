//! `eval_common` — the shared holdout-evaluation engine (Phase 10C.1, ADR-0036 Amendment 8).
//!
//! This module is the single source of truth for the metric-expression
//! grammar, the reduction vocabulary, the holdout `Filter` F64-equality
//! guard, bucket/group resolution, and the per-segment reduction engine.
//! Both `mc model grade` and `mc model backtest` call it: grade runs it
//! once and formats the segment surface; backtest runs it once per grid
//! cell and selects an objective.
//!
//! Lifted verbatim from `grade.rs` (Phase 10B, ADR-0034) per ADR-0036
//! Amendment 8 ("shared code, not subprocess, not duplication"). The
//! behavior is unchanged from grade's tested engine; the only additive
//! change is the `rmse` reduction (Amendment 7 — the 10th reduction, so
//! the forecasting multi-domain example is expressible). The binding
//! amendments folded in are grade's A1/A2/A3/A6/A7/A9/A11/A12 — see the
//! grade.rs module header and ADR-0034 for the originals.
//!
//! Everything here is `pub(crate)`: it's an internal mc-cli library, not a
//! public API. Per ADR-0036 Decision 7 / AC #17, there is **zero**
//! `mc-core`/`mc-model` change — this is pure CLI composition.

use crate::query::{
    enumerate_leaf_coords, eval_filter, read_measure_at, CmpOp, Filter, FilterAtom, FilterValue,
};
use mc_core::rule::{wilson_ci_lower_compute, wilson_ci_upper_compute};
use mc_core::ScalarValue;
use std::collections::BTreeMap;

/// Float-zero threshold for `ratio` denominators (CLAUDE.md §7 / ADR-0034 Amdt 6).
pub(crate) const ZERO_EPS: f64 = 1e-300;

// ===========================================================================
// Metric-expression grammar (ADR-0034 Amendment 11; ADR-0036 Amendment 7)
// ===========================================================================

/// One reduction in the closed metric vocabulary. ADR-0034 Amendment 7
/// shipped 9 (count/mean/sum/ratio/std/min/max/wilson_*); ADR-0036
/// Amendment 7 adds `rmse` (the 10th), so `sqrt(mean(squared_error))` is
/// expressible for the forecasting multi-domain proof point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reduction {
    Count,
    Mean,
    Sum,
    Ratio,
    Std,
    Min,
    Max,
    WilsonLower,
    WilsonUpper,
    /// `rmse(m)` = `sqrt(mean(m))` where `m` is a per-unit squared-error
    /// measure (ADR-0036 Amendment 7). Domain-neutral: the cartridge author
    /// supplies the squared-error measure; the engine ships no metric.
    Rmse,
}

impl Reduction {
    /// The canonical spelling used in `--metric` expressions and in error
    /// messages.
    pub(crate) fn name(self) -> &'static str {
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
            Reduction::Rmse => "rmse",
        }
    }

    /// Number of ingredient measures this reduction consumes (Amdt 11
    /// arity rule: `ratio` = 2, all others = 1).
    pub(crate) fn arity(self) -> usize {
        match self {
            Reduction::Ratio => 2,
            _ => 1,
        }
    }

    pub(crate) fn from_str(s: &str) -> Option<Reduction> {
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
            "rmse" => Reduction::Rmse,
            _ => return None,
        })
    }
}

/// A parsed `name=reduction(ingredient[,ingredient])` metric.
#[derive(Debug, Clone)]
pub(crate) struct MetricExpr {
    pub name: String,
    pub reduction: Reduction,
    pub ingredients: Vec<String>,
}

pub(crate) const REDUCTION_LIST: &str =
    "count, mean, sum, ratio, std, min, max, wilson_lower, wilson_upper, rmse";

/// Parse one metric expression per the Amendment 11 grammar:
///
/// ```text
/// metric_expr := IDENT '=' REDUCTION_NAME '(' ingredient (',' ingredient)* ')'
/// ```
///
/// Whitespace is tolerated around `=`, `,`, and the parens but not within
/// identifiers. Ingredient existence in the cartridge is validated later
/// (in [`evaluate`]) where the cube is available; this function owns the
/// grammar + arity + reduction-name checks.
pub(crate) fn parse_metric_expr(input: &str) -> Result<MetricExpr, String> {
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

/// Parse a colon-separated, strictly-ascending edge list (`0:0.03:0.10`).
/// Requires at least 2 edges (one band).
pub(crate) fn parse_bucket_edges(s: &str) -> Result<Vec<f64>, String> {
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
// Bucket assignment (ADR-0034 Amendment 2 / Decision 2)
// ===========================================================================

/// The outcome of assigning a continuous value to a bucket band.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BandAssignment {
    /// Band index `i` covering `[edges[i], edges[i+1])` (last band is
    /// right-closed). `lower` is the band's lower edge (the sort key).
    Band { label: String, lower: f64 },
    /// Value fell outside every band — surfaced, never silently dropped.
    OutOfRange,
}

/// Assign `value` to a left-closed / right-open band (the final band is
/// right-closed), per Decision 2. Uses only range comparisons (`>=`/`<`/`<=`)
/// — never float `==`.
pub(crate) fn assign_bucket(value: f64, edges: &[f64]) -> BandAssignment {
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
            let label = format!("[{},{}{}", fmt_edge(lo), fmt_edge(hi), close);
            return BandAssignment::Band { label, lower: lo };
        }
    }
    BandAssignment::OutOfRange
}

/// Format a bucket edge for display: integers print bare, fractional values
/// drop trailing zeros (`0.030000` → `0.03`).
pub(crate) fn fmt_edge(v: f64) -> String {
    if v.is_finite() && v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

// ===========================================================================
// Holdout F64-equality guard (ADR-0034 Amendment 1)
// ===========================================================================

/// Walk a parsed holdout [`Filter`] and reject bare equality / inequality
/// against a numeric literal on a *measure* atom. No discrete-measure
/// metadata exists in `mc-model`, so every measure is treated as
/// continuous F64 for this guard: `line == 9.0` is a hard error. Dimension
/// pins (`Time == "2025"`), string-valued measure equality, and range
/// predicates (`line >= 8.99 and line <= 9.01`) are all allowed.
pub(crate) fn guard_filter_f64_equality(filter: &Filter) -> Result<(), String> {
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
// The evaluation spec + report types
// ===========================================================================

/// Policy for Wilson reductions when the indicator has Null values in a
/// segment (ADR-0034 Amendment 3). Defaults to `Error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WilsonNullPolicy {
    Error,
    Drop,
}

/// The inputs `evaluate` needs, borrowed from whichever command built them
/// (`grade` from its `GradeCommand`, `backtest` from its per-cell config).
/// Borrowing keeps both callers zero-copy and avoids a second owned mirror
/// of the metric/bucket vectors.
pub(crate) struct EvalSpec<'a> {
    /// The dimension whose leaves are the analysis units.
    pub unit: &'a str,
    /// Holdout filter expression (same grammar as `query --where`); parsed
    /// and F64-equality-guarded inside `evaluate`.
    pub holdout: Option<&'a str>,
    pub group_by: &'a [String],
    pub metrics: &'a [MetricExpr],
    /// Measure name → ascending bucket edges.
    pub buckets: &'a BTreeMap<String, Vec<f64>>,
    pub min_n: usize,
    pub max_segments: usize,
    pub wilson_null: WilsonNullPolicy,
}

/// A segment's classification for reporting (ADR-0034 Amendment 5 `status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SegmentStatus {
    Ok,
    BelowMinN,
    OutOfRange,
}

impl SegmentStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            SegmentStatus::Ok => "ok",
            SegmentStatus::BelowMinN => "below_min_n",
            SegmentStatus::OutOfRange => "out_of_range",
        }
    }
}

/// A computed segment row.
#[derive(Debug)]
pub(crate) struct SegmentResult {
    /// `(group_by_key, display_value)` pairs in flag order.
    pub keys: Vec<(String, String)>,
    pub n_units: usize,
    /// One metric value per `report.metric_names`, in order. `None` = Null.
    pub metrics: Vec<Option<f64>>,
    /// Per-ingredient Null counts (ADR-0034 Amendment 5).
    pub null_counts: BTreeMap<String, usize>,
    pub status: SegmentStatus,
    /// Populated by grade's flag pass post-evaluation; `evaluate` leaves it
    /// empty (flagging is grade-specific UX, not shared eval logic).
    pub flagged: Vec<String>,
}

/// The full evaluation result, ready to format or to feed objective
/// selection. (Was grade's `GradeReport`; renamed since backtest also
/// consumes it.)
#[derive(Debug)]
pub(crate) struct EvalReport {
    pub metric_names: Vec<String>,
    /// True for each metric whose reduction is `count` (integer display).
    pub metric_is_count: Vec<bool>,
    pub group_by: Vec<String>,
    pub segments: Vec<SegmentResult>,
    pub total: SegmentResult,
    pub warnings: Vec<String>,
    /// Display key-vecs of segments where a `ratio` denominator was zero.
    pub denom_zero_segments: Vec<Vec<(String, String)>>,
    /// Buckets actually applied (measure → edges), for JSON metadata.
    pub bucket_meta: BTreeMap<String, Vec<f64>>,
    /// Set by grade's flag pass; 0 from `evaluate`.
    pub flagged_count: usize,
}

// ===========================================================================
// The evaluation core (was grade_cube; flag application lifted out)
// ===========================================================================

/// Run the holdout evaluation against a loaded cube: validate → filter →
/// segment (group-by/bucket) → reduce. Returns the segment surface. Does
/// NOT apply `--flag-if` (grade-specific; grade applies it post-hoc) and
/// does NOT load/format (the callers own that).
pub(crate) fn evaluate(
    cube: &mut mc_core::Cube,
    refs: &mc_model::ModelRefs,
    principal: mc_core::PrincipalId,
    spec: &EvalSpec<'_>,
) -> Result<EvalReport, String> {
    // --- Validate --unit names a real, non-Measure dimension. ------------
    let unit_ok = cube
        .dimensions()
        .iter()
        .any(|d| d.name == spec.unit && d.kind != mc_core::DimensionKind::Measure);
    if !unit_ok {
        return Err(format!(
            "--unit {:?} is not a dimension in this cartridge",
            spec.unit
        ));
    }

    // --- Validate metric ingredients exist as measures. ------------------
    for m in spec.metrics {
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
    let mut kinds: Vec<GroupKind> = Vec::with_capacity(spec.group_by.len());
    for key in spec.group_by {
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
    let filter = match spec.holdout {
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

        for (key_name, kind) in spec.group_by.iter().zip(kinds.iter()) {
            let (display, sort, oor) =
                resolve_group_component(coord, key_name, kind, spec.buckets, cube, refs, principal)?;
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

    // --- Enforce --max-segments (ADR-0034 Amendment 2). ------------------
    if builds.len() > spec.max_segments {
        return Err(format!(
            "resolved {} segments, which exceeds --max-segments {} — narrow the holdout, \
             coarsen the buckets, or raise --max-segments",
            builds.len(),
            spec.max_segments
        ));
    }

    // --- Deterministic ordering (ADR-0034 Amendment 12). -----------------
    builds.sort_by(|a, b| cmp_sort_vecs(&a.sorts, &b.sorts));

    // --- The set of ingredient measures, for null_counts. ----------------
    let mut ingredient_set: Vec<String> = Vec::new();
    for m in spec.metrics {
        for ing in &m.ingredients {
            if !ingredient_set.contains(ing) {
                ingredient_set.push(ing.clone());
            }
        }
    }

    let metric_names: Vec<String> = spec.metrics.iter().map(|m| m.name.clone()).collect();
    let metric_is_count: Vec<bool> = spec
        .metrics
        .iter()
        .map(|m| m.reduction == Reduction::Count)
        .collect();

    // --- Reduce each segment. --------------------------------------------
    let mut segments: Vec<SegmentResult> = Vec::with_capacity(builds.len());
    let mut denom_zero_segments: Vec<Vec<(String, String)>> = Vec::new();

    for b in &builds {
        let keys: Vec<(String, String)> = spec
            .group_by
            .iter()
            .cloned()
            .zip(b.displays.iter().cloned())
            .collect();

        let reduced = reduce_segment(
            &b.coords,
            spec.metrics,
            &ingredient_set,
            cube,
            refs,
            principal,
            spec.wilson_null,
            &b.displays,
            &mut warnings,
        )?;

        let status = if b.out_of_range {
            SegmentStatus::OutOfRange
        } else if b.coords.len() < spec.min_n {
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

    // --- TOTAL row: aggregate over ALL holdout units (ADR-0034 Amendment 9
    //     — inclusive of min-n-excluded and out-of-range segments). -------
    let total_reduced = reduce_segment(
        &unit_coords,
        spec.metrics,
        &ingredient_set,
        cube,
        refs,
        principal,
        spec.wilson_null,
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

    Ok(EvalReport {
        metric_names,
        metric_is_count,
        group_by: spec.group_by.to_vec(),
        segments,
        total,
        warnings,
        denom_zero_segments,
        bucket_meta: spec.buckets.clone(),
        flagged_count: 0,
    })
}

/// Result of reducing one coord set across all metrics.
pub(crate) struct ReducedSegment {
    pub metrics: Vec<Option<f64>>,
    pub null_counts: BTreeMap<String, usize>,
    pub denom_zero: bool,
}

/// Apply every metric to one segment's coordinate set.
#[allow(clippy::too_many_arguments)]
pub(crate) fn reduce_segment(
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
            // ADR-0036 Amendment 7: rmse(m) = sqrt(mean(m)). The ingredient
            // is a per-unit squared-error measure; we take the mean then the
            // root. `filter(is_finite)` guards the misuse case (a negative
            // mean → NaN) so the engine never emits NaN/inf (CLAUDE.md §2.5).
            Reduction::Rmse => {
                reduce_mean(&columns[&m.ingredients[0]].values).map(|mean| mean.sqrt())
            }
            Reduction::Ratio => {
                let num = &columns[&m.ingredients[0]].values;
                let den = &columns[&m.ingredients[1]].values;
                let den_sum = reduce_sum(den);
                if den.is_empty() || den_sum.abs() < ZERO_EPS {
                    // Per ADR-0034 Amendment 6: Null + diagnostic, never inf/NaN/0.
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
                // Wilson Null-indicator contract (ADR-0034 Amendment 3).
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

pub(crate) fn reduce_sum(values: &[f64]) -> f64 {
    values.iter().sum()
}

pub(crate) fn reduce_mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

/// Sample standard deviation (ddof=1), matching Phase 10A `std_over`.
pub(crate) fn reduce_std(values: &[f64]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let ss: f64 = values.iter().map(|v| (v - mean) * (v - mean)).sum();
    Some((ss / (n - 1.0)).sqrt())
}

pub(crate) fn reduce_min(values: &[f64]) -> Option<f64> {
    values.iter().copied().fold(None, |acc, v| match acc {
        None => Some(v),
        Some(m) => Some(if v < m { v } else { m }),
    })
}

pub(crate) fn reduce_max(values: &[f64]) -> Option<f64> {
    values.iter().copied().fold(None, |acc, v| match acc {
        None => Some(v),
        Some(m) => Some(if v > m { v } else { m }),
    })
}

// ===========================================================================
// Group-by resolution + segment ordering (ADR-0034 Amendment 12)
// ===========================================================================

/// How a `--group-by` key resolves against the cube.
enum GroupKind {
    /// A non-Measure dimension: the segment is the element name at that slot.
    Dimension { dim_index: usize },
    /// A measure: the segment is the per-leaf value (string/category direct,
    /// continuous-F64 bucketed).
    Measure { name: String },
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
                ScalarValue::Str(_) | ScalarValue::Category(_) | ScalarValue::Bool(_) => {
                    Err(format!(
                        "--group-by {key_name:?} evaluates to a non-numeric (string/category/bool) \
                         measure value; grouping a non-numeric measure by distinct value is not \
                         supported in this phase. Group by a dimension instead, or author a \
                         discrete numeric slice measure and --bucket it."
                    ))
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

/// Sort key for one segment-key component (ADR-0034 Amendment 12). Numeric
/// bands sort by lower edge; categorical values sort lexicographically;
/// special catch-all segments (`(out-of-range)`, `(null)`) sort last.
#[derive(Debug, Clone)]
pub(crate) enum SortKey {
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
        (SortKey::Num(_), SortKey::Text(_)) => Ordering::Less,
        (SortKey::Text(_), SortKey::Num(_)) => Ordering::Greater,
        _ => Ordering::Equal,
    }
}

pub(crate) fn cmp_sort_vecs(a: &[SortKey], b: &[SortKey]) -> std::cmp::Ordering {
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
pub(crate) fn is_measure(cube: &mc_core::Cube, name: &str) -> bool {
    cube.dimensions()
        .iter()
        .find(|d| d.kind == mc_core::DimensionKind::Measure)
        .map(|d| d.element_by_name(name).is_some())
        .unwrap_or(false)
}
