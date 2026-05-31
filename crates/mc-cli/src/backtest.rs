//! `mc model backtest` — parameter sweep × holdout evaluation (Phase 10C.1, ADR-0036).
//!
//! Sweeps one or more axes (a `param(name)` scalar, a fitted-model
//! coefficient, or an Input-measure value) across a grid; at each grid cell
//! runs the full `grade`-style holdout evaluation
//! ([`crate::eval_common::evaluate`]); reports the metric surface and flags
//! the best cell by an objective. Composes `grade`'s evaluation engine with
//! `sweep`'s override mechanics — multi-domain by mandate (the engine ships
//! no domain metric; every metric is a generic reduction over author-named
//! measures).
//!
//! **The spike guardrail (Phase 10C.0, GREEN).** A swept `param`/`coef`
//! value lives in `reference_data`, which is OUTSIDE dirty propagation
//! (cube.rs:3069) and is NOT restored by `rollback_to`. If we mutated it in
//! place on an already-evaluated cube and re-read, the derived cells would
//! serve STALE cache from the prior grid cell. The fix the spike proved:
//! per grid cell, `rollback_to(snapshot)` FIRST (it bumps the revision and
//! prunes every `Provenance::Rule` cell — busting both the derived-leaf and
//! consolidated caches), THEN apply the swept values, THEN evaluate. Every
//! cell re-applies every axis, so reference_data never leaks between cells.
//! This mirrors `sweep.rs`'s coefficient loop exactly.
//!
//! Per ADR-0036 Decision 7 / AC #17, this is CLI-only: zero `mc-core` /
//! `mc-model` change. `--simulate` is deferred (Amendment 4); the
//! `variant:` axis is deferred (Amendment 5). v1 is reduction-only and
//! provably domain-neutral.

use crate::eval_common::{evaluate, parse_metric_expr, EvalSpec, MetricExpr, WilsonNullPolicy};
use crate::loader::{load_model_with_policy, LoadPolicy};
use crate::query::{format_f64, parse_coord_string, push_json_str};
use std::collections::BTreeMap;
use std::fmt::Write;

/// Tolerance for the inclusive range upper bound (mirrors sweep.rs:166 so
/// `start:stop:step` includes `stop` despite float drift).
const RANGE_EPS_FACTOR: f64 = 0.01;

/// Output format (`text | json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BacktestFormat {
    Text,
    Json,
}

/// Objective direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Goal {
    Maximize,
    Minimize,
}

/// Objective scope (Amendment 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BestBy {
    Total,
    Segment,
}

/// A fully-parsed `mc model backtest` invocation. The axis specs are kept
/// as raw strings and resolved after the cube loads (coef/input axes need
/// the cube to validate the model/coefficient/coordinate).
#[derive(Debug)]
pub struct BacktestCommand {
    pub path: String,
    pub unit: String,
    pub holdout: Option<String>,
    pub sweeps: Vec<String>,
    pub metrics: Vec<MetricExpr>,
    pub group_by: Vec<String>,
    pub buckets: BTreeMap<String, Vec<f64>>,
    pub objective: Option<String>,
    pub goal: Goal,
    pub best_by: BestBy,
    pub min_n: usize,
    pub max_segments: usize,
    pub max_grid: usize,
    pub wilson_null: WilsonNullPolicy,
    pub include_writes: bool,
    pub format: BacktestFormat,
    pub emit_grid: Option<String>,
    pub dry_run: bool,
}

fn help_text() -> String {
    "\
mc model backtest <cartridge.yaml> — parameter sweep × holdout evaluation

Sweeps one or more axes across a grid; at each grid cell runs the full
holdout evaluation (filter → group-by → reductions, identical to `grade`)
and reports the metric surface, flagging the best cell by an objective.

USAGE:
    mc model backtest <path> --unit <dim> --sweep <axis> --metric <expr> [options]

REQUIRED:
    <path>                 cartridge YAML
    --unit <dim>           dimension whose leaves are the analysis units
    --sweep <axis-spec>    what varies (repeatable → cartesian grid); see AXES
    --metric \"<expr>\"      one or more metrics (repeatable); see GRAMMAR

AXES (--sweep):
    param:<name>=<spec>                 a parameters: scalar (param(name))
    coef:<model>.<name>=<spec>          a fitted-model coefficient (absolute)
    coef:<model>.<name>=<m>x<lo>:<hi>:<step>   coefficient × multiplier (stress)
    input:<measure>@<k=v,...>=<spec>    an Input value at a coordinate (transient)
  <spec> is a range start:stop:step OR a value list [v1,v2,v3].

OPTIONS:
    --holdout \"<filter>\"   restrict units (same grammar as `query --where`)
    --group-by <key>       segment by a dimension or measure (repeatable)
    --bucket <measure> <edges>   band a continuous measure for grouping
    --objective <metric>   pick the best grid cell by this metric
    --goal maximize|minimize   objective direction (default maximize)
    --best-by total|segment    objective scope (default total)
    --min-n <int>          mark segments below n (default 0)
    --max-segments <int>   cap resolved segment count (default 50)
    --max-grid <int>       hard-error if the grid exceeds this (default 1000)
    --wilson-null error|drop   Null Wilson indicator policy (default error)
    --include-writes       fold in operational writes (default: Reproducible)
    --format text|json     output format (default text)
    --emit-grid <path>     write the surface as jsonl (one row per cell × segment)
    --dry-run              print resolved axes + grid count + sample cells, no eval
    -h, --help             show this help

GRAMMAR (--metric):
    name=reduction(ingredient[,ingredient])
    reductions: count, mean, sum, ratio, std, min, max, wilson_lower, wilson_upper, rmse
    (ratio takes 2 ingredients; all others take 1)
"
    .to_string()
}

/// Parse `mc model backtest` arguments.
pub fn parse(args: &[String]) -> Result<BacktestCommand, String> {
    let mut path: Option<String> = None;
    let mut unit: Option<String> = None;
    let mut holdout: Option<String> = None;
    let mut sweeps: Vec<String> = Vec::new();
    let mut metric_strs: Vec<String> = Vec::new();
    let mut group_by: Vec<String> = Vec::new();
    let mut buckets: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut objective: Option<String> = None;
    let mut goal = Goal::Maximize;
    let mut best_by = BestBy::Total;
    let mut min_n: usize = 0;
    let mut max_segments: usize = 50;
    let mut max_grid: usize = 1000;
    let mut wilson_null = WilsonNullPolicy::Error;
    let mut include_writes = false;
    let mut format = BacktestFormat::Text;
    let mut emit_grid: Option<String> = None;
    let mut dry_run = false;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
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
            "--sweep" => match iter.next() {
                Some(v) => sweeps.push(v.clone()),
                None => return Err("--sweep requires an axis spec".into()),
            },
            "--metric" => match iter.next() {
                Some(v) => metric_strs.push(v.clone()),
                None => return Err("--metric requires an expression".into()),
            },
            "--group-by" => match iter.next() {
                Some(v) => group_by.push(v.clone()),
                None => return Err("--group-by requires a dimension or measure name".into()),
            },
            "--bucket" => {
                let measure = match iter.next() {
                    Some(v) => v.clone(),
                    None => return Err("--bucket requires a measure name and edges".into()),
                };
                let edges_str = match iter.next() {
                    Some(v) => v.clone(),
                    None => return Err(format!("--bucket {measure} requires edges (e.g. 0:0.5:1.0)")),
                };
                let edges = crate::eval_common::parse_bucket_edges(&edges_str)
                    .map_err(|e| format!("--bucket {measure}: {e}"))?;
                buckets.insert(measure, edges);
            }
            "--objective" => match iter.next() {
                Some(v) => objective = Some(v.clone()),
                None => return Err("--objective requires a metric name".into()),
            },
            "--goal" => match iter.next() {
                Some(v) if v == "maximize" => goal = Goal::Maximize,
                Some(v) if v == "minimize" => goal = Goal::Minimize,
                Some(v) => return Err(format!("--goal must be maximize|minimize, got {v:?}")),
                None => return Err("--goal requires maximize|minimize".into()),
            },
            "--best-by" => match iter.next() {
                Some(v) if v == "total" => best_by = BestBy::Total,
                Some(v) if v == "segment" => best_by = BestBy::Segment,
                Some(v) => return Err(format!("--best-by must be total|segment, got {v:?}")),
                None => return Err("--best-by requires total|segment".into()),
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
            "--max-grid" => match iter.next() {
                Some(v) => {
                    max_grid = v
                        .parse()
                        .map_err(|_| format!("--max-grid must be a positive integer, got {v:?}"))?
                }
                None => return Err("--max-grid requires an integer".into()),
            },
            "--wilson-null" => match iter.next() {
                Some(v) if v == "error" => wilson_null = WilsonNullPolicy::Error,
                Some(v) if v == "drop" => wilson_null = WilsonNullPolicy::Drop,
                Some(v) => return Err(format!("--wilson-null must be error|drop, got {v:?}")),
                None => return Err("--wilson-null requires error|drop".into()),
            },
            "--include-writes" => include_writes = true,
            "--format" => match iter.next() {
                Some(v) if v == "text" => format = BacktestFormat::Text,
                Some(v) if v == "json" => format = BacktestFormat::Json,
                Some(v) => return Err(format!("--format must be text|json, got {v:?}")),
                None => return Err("--format requires an argument".into()),
            },
            "--emit-grid" => match iter.next() {
                Some(v) => emit_grid = Some(v.clone()),
                None => return Err("--emit-grid requires a path".into()),
            },
            "--dry-run" => dry_run = true,
            other if !other.starts_with("--") && path.is_none() => {
                path = Some(other.to_string());
            }
            other => return Err(format!("unknown argument: {other:?}")),
        }
    }

    let path = path.ok_or("`mc model backtest` requires a YAML model path")?;
    let unit =
        unit.ok_or("--unit is required (the dimension whose leaves are the analysis units)")?;
    if sweeps.is_empty() {
        return Err("at least one --sweep axis is required".into());
    }
    if metric_strs.is_empty() {
        return Err("at least one --metric is required".into());
    }
    if max_grid == 0 {
        return Err("--max-grid must be at least 1".into());
    }
    if best_by == BestBy::Segment && group_by.is_empty() {
        return Err("--best-by segment requires at least one --group-by".into());
    }
    let metrics: Vec<MetricExpr> = metric_strs
        .iter()
        .map(|s| parse_metric_expr(s))
        .collect::<Result<_, _>>()?;

    Ok(BacktestCommand {
        path,
        unit,
        holdout,
        sweeps,
        metrics,
        group_by,
        buckets,
        objective,
        goal,
        best_by,
        min_n,
        max_segments,
        max_grid,
        wilson_null,
        include_writes,
        format,
        emit_grid,
        dry_run,
    })
}

// ===========================================================================
// Axis resolution (Step 2 — the --sweep parser)
// ===========================================================================

/// What a resolved axis overrides per grid cell.
enum AxisKind {
    /// A `parameters:` scalar — `cube.reference_data.parameters.insert(name, v)`.
    Param { name: String },
    /// A fitted-model coefficient. `multiplier` → applied = `original * v`;
    /// absolute → applied = `v`. `original` is the fitted value captured at
    /// load (before any override), so the apply is independent of cell order.
    Coef {
        model: String,
        index: usize,
        original: f64,
        multiplier: bool,
    },
    /// An Input value at a coordinate — `cube.write(coord, F64(v))`. Store-
    /// backed, so `rollback_to` restores it cleanly each cell.
    Input { coord: mc_core::CellCoordinate },
}

/// A resolved sweep axis: its display spec, what it overrides, and its grid points.
struct Axis {
    spec: String,
    /// Short label for table columns / JSON keys (e.g. `param:threshold`).
    label: String,
    kind: AxisKind,
    points: Vec<f64>,
}

/// Parse a numeric value-spec: a value list `[v1,v2,v3]` or a range
/// `start:stop:step`. Returns the enumerated points.
fn parse_points(spec: &str) -> Result<Vec<f64>, String> {
    let s = spec.trim();
    if let Some(inner) = s.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
        // Value-list form (Amendment 3). Enumerated values, so the float
        // literals are exact grid points, not a computed comparison (§3.1 ok).
        let vals: Vec<f64> = inner
            .split(',')
            .map(|p| {
                p.trim()
                    .parse::<f64>()
                    .map_err(|_| format!("invalid value {p:?} in list (expected a number)"))
            })
            .collect::<Result<_, _>>()?;
        if vals.is_empty() {
            return Err("value list [...] is empty".into());
        }
        return Ok(vals);
    }
    // Range form start:stop:step.
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return Err(format!(
            "value-spec {s:?} must be a range start:stop:step or a list [v1,v2,v3]"
        ));
    }
    let start: f64 = parts[0]
        .trim()
        .parse()
        .map_err(|_| format!("invalid range start {:?}", parts[0]))?;
    let stop: f64 = parts[1]
        .trim()
        .parse()
        .map_err(|_| format!("invalid range stop {:?}", parts[1]))?;
    let step: f64 = parts[2]
        .trim()
        .parse()
        .map_err(|_| format!("invalid range step {:?}", parts[2]))?;
    if step <= 0.0 {
        return Err(format!("range step must be positive, got {step}"));
    }
    if stop < start {
        return Err(format!("range stop {stop} is less than start {start}"));
    }
    let mut points = Vec::new();
    let mut v = start;
    while v <= stop + step * RANGE_EPS_FACTOR {
        points.push(v);
        v += step;
    }
    Ok(points)
}

/// Resolve one `--sweep` spec against the loaded cube.
fn resolve_axis(spec: &str, cube: &mc_core::Cube, refs: &mc_model::ModelRefs) -> Result<Axis, String> {
    let (kind_str, rest) = spec
        .split_once(':')
        .ok_or_else(|| format!("--sweep {spec:?}: missing axis kind (param:/coef:/input:)"))?;
    // The value-spec never contains '=', so the LAST '=' separates the
    // target from the spec — this is safe even for input: whose coord
    // contains '=' (e.g. input:Spend@Time=2025=0:5:1).
    let (target, value_spec) = rest
        .rsplit_once('=')
        .ok_or_else(|| format!("--sweep {spec:?}: missing '=<value-spec>'"))?;
    let target = target.trim();

    match kind_str.trim() {
        "param" => {
            if !cube.reference_data.parameters.contains_key(target) {
                return Err(format!(
                    "--sweep param:{target}: no parameter named {target:?} in this cartridge"
                ));
            }
            let points = parse_points(value_spec)?;
            Ok(Axis {
                spec: spec.to_string(),
                label: format!("param:{target}"),
                kind: AxisKind::Param {
                    name: target.to_string(),
                },
                points,
            })
        }
        "coef" => {
            let (model, coef) = target.split_once('.').ok_or_else(|| {
                format!("--sweep coef:{target}: expected coef:<model>.<name>")
            })?;
            let model_data = cube.reference_data.fitted_models.get(model).ok_or_else(|| {
                format!("--sweep coef:{target}: no fitted model named {model:?}")
            })?;
            let index = model_data
                .coefficients
                .iter()
                .position(|(name, _)| name == coef)
                .ok_or_else(|| {
                    format!("--sweep coef:{target}: coefficient {coef:?} not found in model {model:?}")
                })?;
            let original = model_data.coefficients[index].1;

            // Multiplier form `<m>x<range>`: the value before 'x' is the
            // nominal reference (conventionally 1.0, documenting "1.0 = the
            // fitted value") and the points are multipliers applied to the
            // fitted coefficient (EXP-042 stress test). No 'x' → absolute.
            let (points, multiplier) = if let Some((nominal, mult_spec)) = value_spec.split_once('x')
            {
                nominal.trim().parse::<f64>().map_err(|_| {
                    format!(
                        "--sweep coef:{target}: multiplier form is <nominal>x<lo>:<hi>:<step>; \
                         {nominal:?} before 'x' is not a number"
                    )
                })?;
                (parse_points(mult_spec)?, true)
            } else {
                (parse_points(value_spec)?, false)
            };
            Ok(Axis {
                spec: spec.to_string(),
                label: format!("coef:{model}.{coef}"),
                kind: AxisKind::Coef {
                    model: model.to_string(),
                    index,
                    original,
                    multiplier,
                },
                points,
            })
        }
        "input" => {
            let (measure, coord_str) = target.split_once('@').ok_or_else(|| {
                format!("--sweep input:{target}: expected input:<measure>@<k=v,...>")
            })?;
            let measure = measure.trim();
            let measure_dim = cube
                .dimensions()
                .iter()
                .find(|d| d.kind == mc_core::DimensionKind::Measure)
                .ok_or("cartridge has no Measure dimension")?;
            let mut names = parse_coord_string(coord_str);
            names.insert(measure_dim.name.clone(), measure.to_string());
            let coord = refs.coord_from_names(&names).ok_or_else(|| {
                format!(
                    "--sweep input:{target}: could not resolve coordinate (measure {measure:?} \
                     + {coord_str:?}); pin every non-Measure dimension"
                )
            })?;
            let points = parse_points(value_spec)?;
            Ok(Axis {
                spec: spec.to_string(),
                label: format!("input:{measure}"),
                kind: AxisKind::Input { coord },
                points,
            })
        }
        other => Err(format!(
            "--sweep {spec:?}: unknown axis kind {other:?} (expected param, coef, or input)"
        )),
    }
}

/// Apply one axis's value for a grid cell to the cube (the override).
fn apply_axis(
    cube: &mut mc_core::Cube,
    principal: mc_core::PrincipalId,
    axis: &Axis,
    value: f64,
) -> Result<(), String> {
    match &axis.kind {
        AxisKind::Param { name } => {
            // Spike finding: pub-field insert mirrors override_coefficient.
            cube.reference_data.parameters.insert(name.clone(), value);
        }
        AxisKind::Coef {
            model,
            index,
            original,
            multiplier,
        } => {
            let applied = if *multiplier { original * value } else { value };
            let model_data = cube
                .reference_data
                .fitted_models
                .get_mut(model)
                .ok_or_else(|| format!("fitted model {model:?} vanished mid-sweep"))?;
            model_data.coefficients[*index].1 = applied;
        }
        AxisKind::Input { coord } => {
            cube.write(mc_core::WritebackRequest {
                coord: coord.clone(),
                new_value: mc_core::ScalarValue::F64(value),
                principal,
                intent: mc_core::WriteIntent::Set,
                expected_revision: None,
                now_unix_seconds: 0,
            })
            .map_err(|e| format!("input override write failed: {e}"))?;
        }
    }
    Ok(())
}

// ===========================================================================
// Grid orchestration (Step 3) + per-cell records
// ===========================================================================

/// One segment's metrics within a grid cell (for grouped / best-by-segment).
#[derive(Clone, Debug)]
struct CellSegment {
    keys: Vec<(String, String)>,
    n_units: usize,
    metrics: Vec<Option<f64>>,
}

/// One grid cell's result.
#[derive(Debug)]
struct GridCell {
    /// One value per axis, in axis-declaration order.
    values: Vec<f64>,
    total_metrics: Vec<Option<f64>>,
    total_n: usize,
    segments: Vec<CellSegment>,
}

/// best-by-segment winner: (segment display keys, best cell index, value).
type SegmentBest = (Vec<(String, String)>, usize, f64);

/// The full backtest result, ready to format.
#[derive(Debug)]
struct BacktestResult {
    metric_names: Vec<String>,
    metric_is_count: Vec<bool>,
    axis_labels: Vec<String>,
    cells: Vec<GridCell>,
    warnings: Vec<String>,
    /// `Some(cell_index)` per the total objective; `None` if no objective or
    /// all cells Null. (best-by total)
    best_total: Option<usize>,
    /// best-by segment winners, in first-appearance segment order.
    best_by_segment: Vec<SegmentBest>,
}

/// The integer total of a cartesian grid, or an overflow error.
fn grid_total(axes: &[Axis]) -> Result<usize, String> {
    let mut total: usize = 1;
    for a in axes {
        total = total
            .checked_mul(a.points.len())
            .ok_or("grid size overflows usize")?;
    }
    Ok(total)
}

/// Decode grid cell `idx` into one value per axis, first axis slowest
/// (most-significant), per Decision 8 / ADR-0034 A12.
fn decode_cell(axes: &[Axis], idx: usize) -> Vec<f64> {
    let mut values = vec![0.0; axes.len()];
    let mut rem = idx;
    for i in (0..axes.len()).rev() {
        let len = axes[i].points.len();
        values[i] = axes[i].points[rem % len];
        rem /= len;
    }
    values
}

/// Resolve axes, walk the grid (the spike guardrail per cell), and select
/// the objective. Pure orchestration over `eval_common::evaluate` — the
/// metric math lives entirely in the shared engine.
fn run_grid(
    cmd: &BacktestCommand,
    cube: &mut mc_core::Cube,
    refs: &mc_model::ModelRefs,
    principal: mc_core::PrincipalId,
) -> Result<BacktestResult, String> {
    let mut axes: Vec<Axis> = Vec::with_capacity(cmd.sweeps.len());
    for s in &cmd.sweeps {
        axes.push(resolve_axis(s, cube, refs)?);
    }

    let total = grid_total(&axes)?;
    if total > cmd.max_grid {
        return Err(format!(
            "grid has {total} cells, which exceeds --max-grid {} — narrow the ranges, \
             coarsen the steps, or raise --max-grid",
            cmd.max_grid
        ));
    }

    // One snapshot of the freshly-loaded cube (no derived cache yet). Each
    // cell rolls back to it FIRST — the spike guardrail (cube.rs:2801 busts
    // the derived-leaf + consolidated caches), so reference_data overrides
    // from the prior cell can never serve stale derived values.
    let snapshot = cube.snapshot(Some("phase-10c1:backtest:pre-grid"));

    let mut cells: Vec<GridCell> = Vec::with_capacity(total);
    let mut warnings: Vec<String> = Vec::new();
    let mut metric_names: Option<Vec<String>> = None;
    let mut metric_is_count: Option<Vec<bool>> = None;

    for idx in 0..total {
        let values = decode_cell(&axes, idx);

        // GUARDRAIL ORDER: rollback → apply → evaluate. Never reorder.
        cube.rollback_to(&snapshot)
            .map_err(|e| format!("snapshot rollback failed at cell {idx}: {e}"))?;
        for (ax, &v) in axes.iter().zip(values.iter()) {
            apply_axis(cube, principal, ax, v)?;
        }

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
        let report = evaluate(cube, refs, principal, &spec)?;

        if metric_names.is_none() {
            metric_names = Some(report.metric_names.clone());
            metric_is_count = Some(report.metric_is_count.clone());
        }
        for w in &report.warnings {
            if !warnings.contains(w) {
                warnings.push(w.clone());
            }
        }

        let segments = report
            .segments
            .iter()
            .map(|s| CellSegment {
                keys: s.keys.clone(),
                n_units: s.n_units,
                metrics: s.metrics.clone(),
            })
            .collect();

        cells.push(GridCell {
            values,
            total_metrics: report.total.metrics.clone(),
            total_n: report.total.n_units,
            segments,
        });
    }

    let metric_names = metric_names.expect("grid has >= 1 cell");
    let metric_is_count = metric_is_count.expect("grid has >= 1 cell");
    let axis_labels: Vec<String> = axes.iter().map(|a| a.label.clone()).collect();

    // --- Objective selection (Decision 5 + Amendments 6/7). --------------
    let obj_idx = match &cmd.objective {
        Some(o) => Some(metric_names.iter().position(|m| m == o).ok_or_else(|| {
            format!(
                "--objective {o:?} is not a defined metric; metrics: {}",
                metric_names.join(", ")
            )
        })?),
        None => None,
    };

    let mut best_total: Option<usize> = None;
    let mut best_by_segment: Vec<SegmentBest> = Vec::new();

    if let Some(oi) = obj_idx {
        match cmd.best_by {
            BestBy::Total => {
                let mut best: Option<(usize, f64)> = None;
                for (i, cell) in cells.iter().enumerate() {
                    if let Some(Some(v)) = cell.total_metrics.get(oi) {
                        // Strict improvement only → ties resolve to the
                        // first cell in grid order (Amendment 7).
                        let better = match best {
                            None => true,
                            Some((_, b)) => is_better(*v, b, cmd.goal),
                        };
                        if better {
                            best = Some((i, *v));
                        }
                    }
                }
                if best.is_none() {
                    let obj = cmd.objective.as_deref().unwrap_or("");
                    return Err(format!(
                        "objective {obj:?} is Null in every grid cell — nothing to select"
                    ));
                }
                best_total = best.map(|(i, _)| i);
            }
            BestBy::Segment => {
                for (i, cell) in cells.iter().enumerate() {
                    for seg in &cell.segments {
                        if let Some(Some(v)) = seg.metrics.get(oi) {
                            if let Some(entry) =
                                best_by_segment.iter_mut().find(|(k, _, _)| *k == seg.keys)
                            {
                                if is_better(*v, entry.2, cmd.goal) {
                                    entry.1 = i;
                                    entry.2 = *v;
                                }
                            } else {
                                best_by_segment.push((seg.keys.clone(), i, *v));
                            }
                        }
                    }
                }
                if best_by_segment.is_empty() {
                    let obj = cmd.objective.as_deref().unwrap_or("");
                    return Err(format!(
                        "objective {obj:?} is Null in every (grid cell × segment) — nothing to select"
                    ));
                }
            }
        }
    }

    Ok(BacktestResult {
        metric_names,
        metric_is_count,
        axis_labels,
        cells,
        warnings,
        best_total,
        best_by_segment,
    })
}

/// Is `candidate` strictly better than `incumbent` under `goal`?
fn is_better(candidate: f64, incumbent: f64, goal: Goal) -> bool {
    match goal {
        Goal::Maximize => candidate > incumbent,
        Goal::Minimize => candidate < incumbent,
    }
}

// ===========================================================================
// Output (Step 4): text surface, JSON, --emit-grid jsonl, --dry-run
// ===========================================================================

fn fmt_opt(v: Option<f64>, is_count: bool) -> String {
    match v {
        None => "null".to_string(),
        Some(x) if is_count => format!("{}", x.round() as i64),
        Some(x) => format_f64(x),
    }
}

fn format_dry_run(cmd: &BacktestCommand, axes: &[Axis], total: usize) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "BACKTEST DRY RUN: {}", cmd.path);
    let _ = writeln!(out, "axes ({}):", axes.len());
    for a in axes {
        let preview: Vec<String> = a.points.iter().map(|p| format_f64(*p)).collect();
        let shown = if preview.len() > 6 {
            format!(
                "{}, … , {} ({} points)",
                preview[..3].join(", "),
                preview[preview.len() - 1],
                preview.len()
            )
        } else {
            preview.join(", ")
        };
        let _ = writeln!(out, "  {} → [{}]", a.spec, shown);
    }
    let _ = writeln!(out, "grid cells: {total}");
    // First/last few cells.
    let sample = total.min(3);
    let _ = writeln!(out, "first {sample} cell(s):");
    for idx in 0..sample {
        let v = decode_cell(axes, idx);
        let _ = writeln!(out, "  {}", fmt_cell_values(axes, &v));
    }
    if total > sample {
        let last_from = total.saturating_sub(2);
        let _ = writeln!(out, "last cell(s):");
        for idx in last_from..total {
            let v = decode_cell(axes, idx);
            let _ = writeln!(out, "  {}", fmt_cell_values(axes, &v));
        }
    }
    out
}

/// `param:threshold=0.1, coef:m.b=2` — axis labels paired with cell values.
fn fmt_cell_values(axes: &[Axis], values: &[f64]) -> String {
    axes.iter()
        .zip(values.iter())
        .map(|(a, v)| format!("{}={}", a.label, format_f64(*v)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_result(cmd: &BacktestCommand, result: &BacktestResult) -> String {
    match cmd.format {
        BacktestFormat::Text => format_text(cmd, result),
        BacktestFormat::Json => format_json(cmd, result),
    }
}

fn format_text(cmd: &BacktestCommand, result: &BacktestResult) -> String {
    let mut out = String::new();
    let holdout = cmd.holdout.as_deref().unwrap_or("(all units)");
    let _ = writeln!(
        out,
        "BACKTEST: {}  (holdout: {}; unit: {})",
        cmd.path, holdout, cmd.unit
    );
    let _ = writeln!(out, "axes: {}\n", cmd.sweeps.join("; "));

    if cmd.best_by == BestBy::Segment && cmd.objective.is_some() {
        out.push_str(&format_text_segment(cmd, result));
    } else {
        out.push_str(&format_text_total(cmd, result));
    }

    if !result.warnings.is_empty() {
        out.push('\n');
        for w in &result.warnings {
            let _ = writeln!(out, "warning: {w}");
        }
    }
    out
}

/// The grid-surface table: one row per cell, axis columns then metric columns.
fn format_text_total(cmd: &BacktestCommand, result: &BacktestResult) -> String {
    // Suppress the built-in unit-count `n` column when the user already
    // defined a metric named `n` (mirrors grade's dedup) — else two `n`s.
    let show_n = !result.metric_names.iter().any(|m| m == "n");
    let mut headers: Vec<String> = result.axis_labels.clone();
    if show_n {
        headers.push("n".to_string());
    }
    for m in &result.metric_names {
        headers.push(m.clone());
    }
    if cmd.objective.is_some() {
        headers.push("best".to_string());
    }

    let mut rows: Vec<Vec<String>> = Vec::with_capacity(result.cells.len());
    for (i, cell) in result.cells.iter().enumerate() {
        let mut row: Vec<String> = cell.values.iter().map(|v| format_f64(*v)).collect();
        if show_n {
            row.push(format!("{}", cell.total_n));
        }
        for (mi, mv) in cell.total_metrics.iter().enumerate() {
            row.push(fmt_opt(*mv, result.metric_is_count[mi]));
        }
        if cmd.objective.is_some() {
            row.push(if result.best_total == Some(i) {
                "*".to_string()
            } else {
                String::new()
            });
        }
        rows.push(row);
    }

    let mut out = render_table(&headers, &rows);
    if let (Some(obj), Some(bi)) = (&cmd.objective, result.best_total) {
        let oi = result
            .metric_names
            .iter()
            .position(|m| m == obj)
            .expect("objective validated");
        let best_cell = &result.cells[bi];
        let coords = fmt_cell_values_from_labels(&result.axis_labels, &best_cell.values);
        let val = fmt_opt(best_cell.total_metrics[oi], result.metric_is_count[oi]);
        let _ = writeln!(out, "\nbest ({} {}): {{ {}, {}={} }}", goal_word(cmd.goal), obj, coords, obj, val);
    }
    out
}

/// best-by-segment: one row per segment with its winning grid cell.
fn format_text_segment(cmd: &BacktestCommand, result: &BacktestResult) -> String {
    let obj = cmd.objective.as_deref().unwrap_or("");
    let oi = result
        .metric_names
        .iter()
        .position(|m| m == obj)
        .expect("objective validated");

    let mut headers: Vec<String> = cmd.group_by.clone();
    for label in &result.axis_labels {
        headers.push(format!("best:{label}"));
    }
    headers.push(obj.to_string());

    let mut rows: Vec<Vec<String>> = Vec::new();
    for (keys, cell_idx, value) in &result.best_by_segment {
        let mut row: Vec<String> = keys.iter().map(|(_, v)| v.clone()).collect();
        let cell = &result.cells[*cell_idx];
        for v in &cell.values {
            row.push(format_f64(*v));
        }
        row.push(fmt_opt(Some(*value), result.metric_is_count[oi]));
        rows.push(row);
    }
    let mut out = format!("best {} {} per segment:\n", goal_word(cmd.goal), obj);
    out.push_str(&render_table(&headers, &rows));
    out
}

fn fmt_cell_values_from_labels(labels: &[String], values: &[f64]) -> String {
    labels
        .iter()
        .zip(values.iter())
        .map(|(l, v)| format!("{}={}", l, format_f64(*v)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn goal_word(goal: Goal) -> &'static str {
    match goal {
        Goal::Maximize => "max",
        Goal::Minimize => "min",
    }
}

/// Left-aligned `a | b | c` table with a dashed separator under the header.
fn render_table(headers: &[String], rows: &[Vec<String>]) -> String {
    let ncols = headers.len();
    let mut widths = vec![0usize; ncols];
    for (i, h) in headers.iter().enumerate() {
        widths[i] = widths[i].max(h.len());
    }
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < ncols {
                widths[i] = widths[i].max(cell.len());
            }
        }
    }
    let mut out = String::new();
    let render = |row: &[String], out: &mut String| {
        let cells: Vec<String> = (0..ncols)
            .map(|i| {
                let cell = row.get(i).map(String::as_str).unwrap_or("");
                format!("{:<width$}", cell, width = widths[i])
            })
            .collect();
        let _ = writeln!(out, "{}", cells.join(" | ").trim_end());
    };
    render(headers, &mut out);
    let sep: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    let _ = writeln!(out, "{}", sep.join("-+-"));
    for row in rows {
        render(row, &mut out);
    }
    out
}

fn format_json(cmd: &BacktestCommand, result: &BacktestResult) -> String {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"schema_version\": \"1.0\",\n");
    out.push_str("  \"cartridge\": ");
    push_json_str(&mut out, &cmd.path);
    out.push_str(",\n  \"unit\": ");
    push_json_str(&mut out, &cmd.unit);
    out.push_str(",\n  \"holdout\": ");
    match &cmd.holdout {
        Some(h) => push_json_str(&mut out, h),
        None => out.push_str("null"),
    }
    out.push_str(",\n  \"axes\": [");
    for (i, s) in cmd.sweeps.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        push_json_str(&mut out, s);
    }
    out.push_str("],\n  \"metrics\": [");
    for (i, m) in result.metric_names.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        push_json_str(&mut out, m);
    }
    out.push_str("],\n");
    out.push_str("  \"objective\": ");
    match &cmd.objective {
        Some(o) => push_json_str(&mut out, o),
        None => out.push_str("null"),
    }
    let _ = write!(out, ",\n  \"goal\": \"{}\"", goal_word(cmd.goal));
    let _ = write!(
        out,
        ",\n  \"best_by\": \"{}\"",
        match cmd.best_by {
            BestBy::Total => "total",
            BestBy::Segment => "segment",
        }
    );
    out.push_str(",\n  \"grid\": [\n");
    for (ci, cell) in result.cells.iter().enumerate() {
        out.push_str("    { \"sweep_values\": ");
        push_sweep_values(&mut out, &result.axis_labels, &cell.values);
        out.push_str(", \"n\": ");
        let _ = write!(out, "{}", cell.total_n);
        out.push_str(", \"metrics\": ");
        push_metrics_obj(&mut out, &result.metric_names, &cell.total_metrics, &result.metric_is_count);
        if !cmd.group_by.is_empty() {
            out.push_str(", \"segments\": [");
            for (si, seg) in cell.segments.iter().enumerate() {
                if si > 0 {
                    out.push_str(", ");
                }
                out.push_str("{ \"keys\": ");
                push_keys_obj(&mut out, &seg.keys);
                out.push_str(", \"n\": ");
                let _ = write!(out, "{}", seg.n_units);
                out.push_str(", \"metrics\": ");
                push_metrics_obj(&mut out, &result.metric_names, &seg.metrics, &result.metric_is_count);
                out.push_str(" }");
            }
            out.push(']');
        }
        out.push_str(" }");
        if ci + 1 < result.cells.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ],\n");

    // best
    out.push_str("  \"best\": ");
    push_best_json(&mut out, cmd, result);
    out.push_str(",\n");

    // warnings
    out.push_str("  \"warnings\": [");
    for (i, w) in result.warnings.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        push_json_str(&mut out, w);
    }
    out.push_str("]\n}\n");
    out
}

fn push_sweep_values(out: &mut String, labels: &[String], values: &[f64]) {
    out.push('{');
    for (i, (l, v)) in labels.iter().zip(values.iter()).enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        push_json_str(out, l);
        out.push_str(": ");
        out.push_str(&format_f64(*v));
    }
    out.push('}');
}

fn push_metrics_obj(out: &mut String, names: &[String], values: &[Option<f64>], is_count: &[bool]) {
    out.push('{');
    for (i, name) in names.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push(' ');
        push_json_str(out, name);
        out.push_str(": ");
        match values[i] {
            None => out.push_str("null"),
            Some(v) if is_count[i] => {
                let _ = write!(out, "{}", v.round() as i64);
            }
            Some(v) => out.push_str(&format_f64(v)),
        }
    }
    out.push_str(" }");
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

fn push_best_json(out: &mut String, cmd: &BacktestCommand, result: &BacktestResult) {
    if cmd.objective.is_none() {
        out.push_str("null");
        return;
    }
    match cmd.best_by {
        BestBy::Total => match result.best_total {
            None => out.push_str("null"),
            Some(bi) => {
                let cell = &result.cells[bi];
                out.push_str("{ \"sweep_values\": ");
                push_sweep_values(out, &result.axis_labels, &cell.values);
                out.push_str(", \"metrics\": ");
                push_metrics_obj(out, &result.metric_names, &cell.total_metrics, &result.metric_is_count);
                out.push_str(" }");
            }
        },
        BestBy::Segment => {
            out.push('[');
            for (i, (keys, cell_idx, value)) in result.best_by_segment.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str("{ \"keys\": ");
                push_keys_obj(out, keys);
                out.push_str(", \"sweep_values\": ");
                push_sweep_values(out, &result.axis_labels, &result.cells[*cell_idx].values);
                let _ = write!(out, ", \"value\": {}", format_f64(*value));
                out.push_str(" }");
            }
            out.push(']');
        }
    }
}

/// Write the surface as jsonl: one line per grid cell, or per cell × segment
/// when grouped. The downstream-plotting contract (Decision 6).
fn write_emit_grid(path: &str, cmd: &BacktestCommand, result: &BacktestResult) -> Result<(), String> {
    let mut out = String::new();
    for cell in &result.cells {
        if cmd.group_by.is_empty() {
            out.push_str("{\"sweep_values\": ");
            push_sweep_values(&mut out, &result.axis_labels, &cell.values);
            out.push_str(", \"n\": ");
            let _ = write!(out, "{}", cell.total_n);
            out.push_str(", \"metrics\": ");
            push_metrics_obj(&mut out, &result.metric_names, &cell.total_metrics, &result.metric_is_count);
            out.push_str("}\n");
        } else {
            for seg in &cell.segments {
                out.push_str("{\"sweep_values\": ");
                push_sweep_values(&mut out, &result.axis_labels, &cell.values);
                out.push_str(", \"keys\": ");
                push_keys_obj(&mut out, &seg.keys);
                out.push_str(", \"n\": ");
                let _ = write!(out, "{}", seg.n_units);
                out.push_str(", \"metrics\": ");
                push_metrics_obj(&mut out, &result.metric_names, &seg.metrics, &result.metric_is_count);
                out.push_str("}\n");
            }
        }
    }
    std::fs::write(path, out).map_err(|e| format!("could not write --emit-grid {path}: {e}"))
}

// ===========================================================================
// Entry points
// ===========================================================================

/// Execute `mc model backtest` and print the result.
pub fn run(cmd: BacktestCommand) -> i32 {
    let (code, output) = run_captured(cmd);
    if !output.is_empty() {
        print!("{output}");
    }
    code
}

/// Execute and return `(exit_code, output)`. Used by MCP to capture output.
pub fn run_captured(cmd: BacktestCommand) -> (i32, String) {
    // Reproducible by default (ADR-0034 / sweep precedent, ADR-0036 Amdt 8);
    // overrides are transient and the cube is never persisted.
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

    // --dry-run: resolve axes + grid count + sample cells, no evaluation.
    if cmd.dry_run {
        let mut axes: Vec<Axis> = Vec::with_capacity(cmd.sweeps.len());
        for s in &cmd.sweeps {
            match resolve_axis(s, &cube, refs) {
                Ok(a) => axes.push(a),
                Err(e) => return (1, format!("error: {e}\n")),
            }
        }
        let total = match grid_total(&axes) {
            Ok(t) => t,
            Err(e) => return (1, format!("error: {e}\n")),
        };
        if total > cmd.max_grid {
            return (
                1,
                format!(
                    "error: grid has {total} cells, which exceeds --max-grid {}\n",
                    cmd.max_grid
                ),
            );
        }
        return (0, format_dry_run(&cmd, &axes, total));
    }

    match run_grid(&cmd, &mut cube, refs, principal) {
        Ok(result) => {
            if let Some(path) = &cmd.emit_grid {
                if let Err(e) = write_emit_grid(path, &cmd, &result) {
                    return (1, format!("error: {e}\n"));
                }
            }
            (0, format_result(&cmd, &result))
        }
        Err(e) => (1, format!("error: {e}\n")),
    }
}

#[cfg(test)]
mod tests {
    include!("backtest_tests.rs");
}
