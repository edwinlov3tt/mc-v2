//! `mc model sweep` — parameter sensitivity analysis.
//!
//! Loops whatif over a range of parameter values, records a metric at each
//! point, and reports the optimal value. Named selectors, in-memory struct
//! override, baseline comparison by default.

// Phase 6A.2 item 1.1: sweep is a `Reproducible` policy verb (process-notes
// Rule 9) — sweep experiments with parameter values starting from the
// version-controlled model state, not from operational reality patched by
// `.tessera/writes.jsonl`.
use crate::loader::{load_model_with_policy, LoadPolicy};
use crate::query::{format_f64, push_json_envelope_header, push_json_str, OutputFormat};
use mc_core::{ScalarValue, WriteIntent, WritebackRequest};
use std::fmt::Write;

pub struct SweepCommand {
    pub path: String,
    pub format: OutputFormat,
    pub model_name: Option<String>,
    pub coefficient: Option<String>,
    pub set_coord: Option<String>,
    pub range: String,
    pub metric: String,
    pub goal: String, // "minimize" or "maximize"
    pub dry_run: bool,
    pub time_anchor: Option<String>,
    /// Phase 6A.3 item 3: optional filter expression that restricts the
    /// leaf coordinates the metric ranges over. Same syntax as
    /// `query --where`. Default (None) preserves the previous behaviour
    /// of evaluating the metric across every leaf coord.
    pub metric_where: Option<String>,
    /// Phase 4D: enrich text output with measure descriptions.
    pub verbose: bool,
}

pub fn parse(args: &[String]) -> Result<SweepCommand, String> {
    let mut path: Option<String> = None;
    let mut format = OutputFormat::Text;
    let mut model_name: Option<String> = None;
    let mut coefficient: Option<String> = None;
    let mut set_coord: Option<String> = None;
    let mut range: Option<String> = None;
    let mut metric: Option<String> = None;
    let mut goal = "minimize".to_string();
    let mut dry_run = false;
    let mut time_anchor: Option<String> = None;
    let mut metric_where: Option<String> = None;
    let mut verbose = false;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--format" => match iter.next() {
                Some(v) if v == "text" => format = OutputFormat::Text,
                Some(v) if v == "json" => format = OutputFormat::Json,
                Some(v) if v == "csv" => format = OutputFormat::Csv,
                Some(v) => return Err(format!("--format must be text|json|csv, got {v:?}")),
                None => return Err("--format requires an argument".into()),
            },
            "--model" => match iter.next() {
                Some(v) => model_name = Some(v.clone()),
                None => return Err("--model requires a name".into()),
            },
            "--coefficient" => match iter.next() {
                Some(v) => coefficient = Some(v.clone()),
                None => return Err("--coefficient requires a name".into()),
            },
            "--set" => match iter.next() {
                Some(v) => set_coord = Some(v.clone()),
                None => return Err("--set requires a coord string".into()),
            },
            "--range" => match iter.next() {
                Some(v) => range = Some(v.clone()),
                None => return Err("--range requires a range spec (start:end:step)".into()),
            },
            "--metric" => match iter.next() {
                Some(v) => metric = Some(v.clone()),
                None => return Err("--metric requires an expression".into()),
            },
            "--goal" => match iter.next() {
                Some(v) if v == "minimize" || v == "maximize" => goal.clone_from(v),
                Some(v) => return Err(format!("--goal must be minimize|maximize, got {v:?}")),
                None => return Err("--goal requires an argument".into()),
            },
            "--dry-run" => dry_run = true,
            "--time-anchor" => match iter.next() {
                Some(v) => time_anchor = Some(v.clone()),
                None => return Err("--time-anchor requires an element name".into()),
            },
            "--metric-where" => match iter.next() {
                Some(v) => metric_where = Some(v.clone()),
                None => return Err("--metric-where requires an expression argument".into()),
            },
            "--verbose" | "-v" => verbose = true,
            other if !other.starts_with("--") && path.is_none() => {
                path = Some(other.to_string());
            }
            other => return Err(format!("unknown argument: {other:?}")),
        }
    }
    let path = path.ok_or("`mc model sweep` requires a YAML model path")?;
    let range = range.ok_or("--range is required (format: start:end:step)")?;
    let metric = metric.ok_or("--metric is required")?;
    Ok(SweepCommand {
        path,
        format,
        model_name,
        coefficient,
        set_coord,
        range,
        metric,
        goal,
        dry_run,
        time_anchor,
        metric_where,
        verbose,
    })
}

pub fn run(cmd: SweepCommand) -> i32 {
    let (code, output) = run_captured(cmd);
    if !output.is_empty() {
        print!("{output}");
    }
    code
}

/// Execute the sweep verb and return (exit_code, output_string).
/// Used by MCP to capture output without printing to stdout.
pub fn run_captured(cmd: SweepCommand) -> (i32, String) {
    // Parse range spec: "start:end:step"
    let range_parts: Vec<&str> = cmd.range.split(':').collect();
    if range_parts.len() != 3 {
        eprintln!("error: --range must be start:end:step (e.g., '0:5:0.5')");
        return (2, String::new());
    }
    let start: f64 = match range_parts[0].parse() {
        Ok(v) => v,
        Err(_) => {
            eprintln!("error: invalid range start: {}", range_parts[0]);
            return (2, String::new());
        }
    };
    let end: f64 = match range_parts[1].parse() {
        Ok(v) => v,
        Err(_) => {
            eprintln!("error: invalid range end: {}", range_parts[1]);
            return (2, String::new());
        }
    };
    let step: f64 = match range_parts[2].parse() {
        Ok(v) => v,
        Err(_) => {
            eprintln!("error: invalid range step: {}", range_parts[2]);
            return (2, String::new());
        }
    };
    if step <= 0.0 {
        eprintln!("error: step must be positive");
        return (2, String::new());
    }

    // Generate sweep points
    let mut points: Vec<f64> = Vec::new();
    let mut v = start;
    while v <= end + step * 0.01 {
        points.push(v);
        v += step;
    }

    if cmd.dry_run {
        let output_str = format_dry_run(&cmd, &points);
        return (0, output_str);
    }

    // Parse metric expression: mean(Measure), sum(Measure), etc.
    let metric_fn = cmd.metric.trim();

    // Phase 6A.3 item 2: compile the model exactly once. Each sweep point
    // mutates the same cube via snapshot/rollback rather than reloading
    // the YAML. Combined with pre-resolving coefficient indices and the
    // --set coordinate before the loop, this turns 100 points × 2 YAML
    // reads into 1 YAML read + 100 cheap snapshot/rollback cycles.
    let loaded = match load_model_with_policy(&cmd.path, LoadPolicy::Reproducible) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: {e}");
            return (e.exit_code(), String::new());
        }
    };
    let mut cube = loaded.cube;
    let principal = loaded.root_principal;
    let refs = &loaded.refs;
    let measure_descs = &loaded.measure_descriptions;

    // Apply time-anchor override (carries through every iteration).
    if let Some(anchor_name) = &cmd.time_anchor {
        let anchor_idx = cube.dimensions().iter().find_map(|dim| {
            dim.elements.iter().enumerate().find_map(|(idx, elem)| {
                if elem.name == *anchor_name {
                    Some(idx)
                } else {
                    None
                }
            })
        });
        if let Some(idx) = anchor_idx {
            cube.reference_data.time_anchor_index = Some(idx);
        }
    }

    // Phase 6A.3 item 3: parse `--metric-where` once before the loop. The
    // filter expression is the same syntax as `query --where`; reusing
    // `Filter::parse` keeps the two parsers identical until Phase 3I
    // unifies them. Empty/absent filter falls back to "every leaf coord."
    let metric_filter = match &cmd.metric_where {
        Some(expr) => match crate::query::Filter::parse(expr, refs, &cube) {
            Ok(f) => Some(f),
            Err(e) => {
                eprintln!("error: invalid --metric-where expression: {e}");
                return (2, String::new());
            }
        },
        None => None,
    };

    // Phase 6A.3 item 2 W3: fail-fast resolution BEFORE entering the loop.
    // Either --coefficient (with --model) or --set is required; whichever
    // applies, validate it once. A typo in --coefficient now fails before
    // the first point's metric evaluation rather than after N iterations.
    enum SweepTarget {
        Coefficient { model_name: String, index: usize },
        Cell { coord: mc_core::CellCoordinate },
    }
    let target = if let Some(coeff_name) = &cmd.coefficient {
        let Some(model_name) = &cmd.model_name else {
            eprintln!("error: --model is required when using --coefficient");
            return (2, String::new());
        };
        let Some(coeff_idx) = find_coefficient_index(&cmd.path, model_name, coeff_name) else {
            eprintln!("error: coefficient '{coeff_name}' not found in model '{model_name}'");
            return (1, String::new());
        };
        SweepTarget::Coefficient {
            model_name: model_name.clone(),
            index: coeff_idx,
        }
    } else if let Some(set_str) = &cmd.set_coord {
        let coord_names = crate::query::parse_coord_string(set_str);
        let Some(coord) = refs.coord_from_names(&coord_names) else {
            eprintln!("error: could not resolve --set coordinate: {set_str}");
            return (1, String::new());
        };
        SweepTarget::Cell { coord }
    } else {
        eprintln!("error: either --coefficient (with --model) or --set is required");
        return (2, String::new());
    };

    // Phase 6A.3 item 2: baseline (un-overridden) metric evaluated against
    // the freshly-loaded cube before any iteration. Captured here so the
    // sweep loop's snapshot is the post-baseline starting point. Phase 6A.3
    // item 3: if `--metric-where` is set, the baseline is also restricted
    // to matching coords (so `delta_from_baseline` stays meaningful).
    let baseline_result = eval_metric(
        &mut cube,
        refs,
        principal,
        metric_fn,
        metric_filter.as_ref(),
    );

    // Phase 6A.3 item 2 W1: take ONE snapshot before the loop. Each
    // iteration rolls back to this snapshot, applies its override, and
    // evaluates the metric. Snapshot/rollback is O(store size); for Acme
    // (~25K cells) this is a few hundred microseconds vs. the multi-ms
    // YAML compile path.
    let baseline_snapshot = cube.snapshot(Some("phase-6a-3:sweep:pre-overrides"));
    let mut results: Vec<SweepPoint> = Vec::new();

    for &point_value in points.iter() {
        // Reset to the un-overridden baseline before each point.
        if let Err(e) = cube.rollback_to(&baseline_snapshot) {
            eprintln!("error: snapshot rollback failed: {e}");
            return (1, String::new());
        }

        match &target {
            SweepTarget::Coefficient { model_name, index } => {
                if !override_coefficient(&mut cube, model_name, *index, point_value) {
                    eprintln!(
                        "error: could not override coefficient at index {index} in model {model_name:?}"
                    );
                    return (1, String::new());
                }
            }
            SweepTarget::Cell { coord } => {
                let write_result = cube.write(WritebackRequest {
                    coord: coord.clone(),
                    new_value: ScalarValue::F64(point_value),
                    principal,
                    intent: WriteIntent::Set,
                    expected_revision: None,
                    now_unix_seconds: 0,
                });
                if let Err(e) = write_result {
                    eprintln!("error: write failed at point {point_value}: {e}");
                    return (1, String::new());
                }
            }
        }

        let metric_value = eval_metric(
            &mut cube,
            refs,
            principal,
            metric_fn,
            metric_filter.as_ref(),
        );
        results.push(SweepPoint {
            parameter_value: point_value,
            metric_value,
        });
    }

    // Find optimal among the points that produced a numeric metric. Points
    // where `--metric-where` matched zero coords return None and are
    // ignored when picking the optimal.
    let optimal: Option<&SweepPoint> = if cmd.goal == "minimize" {
        results
            .iter()
            .filter(|p| p.metric_value.is_some())
            .min_by(|a, b| {
                a.metric_value
                    .partial_cmp(&b.metric_value)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    } else {
        results
            .iter()
            .filter(|p| p.metric_value.is_some())
            .max_by(|a, b| {
                a.metric_value
                    .partial_cmp(&b.metric_value)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    };

    let output_str = format_sweep_output(
        &cmd,
        &results,
        baseline_result,
        optimal,
        cmd.format,
        cmd.verbose,
        measure_descs,
    );
    (0, output_str)
}

struct SweepPoint {
    parameter_value: f64,
    /// Phase 6A.3 item 3: `None` when `--metric-where` matches no coords
    /// at this point. Without `--metric-where` this is always `Some`
    /// (fall-through aggregations of an empty set return `Some(0.0)`).
    metric_value: Option<f64>,
}

fn override_coefficient(
    cube: &mut mc_core::Cube,
    model_name: &str,
    coeff_index: usize,
    value: f64,
) -> bool {
    if let Some(model_data) = cube.reference_data.fitted_models.get_mut(model_name) {
        if coeff_index < model_data.coefficients.len() {
            // Phase 6A.1 CRIT-1 reshape: keep the feature name, replace the weight.
            model_data.coefficients[coeff_index].1 = value;
            return true;
        }
    }
    false
}

/// Find the coefficient index by feature name in the parsed YAML.
fn find_coefficient_index(yaml_path: &str, model_name: &str, coeff_name: &str) -> Option<usize> {
    let yaml = std::fs::read_to_string(yaml_path).ok()?;
    let parsed = mc_model::parse(&yaml, Some(yaml_path.to_string())).ok()?;
    let validated = mc_model::validate(parsed).ok()?;
    for fm in &validated.parsed.fitted_models {
        if fm.name == model_name {
            return fm.coefficients.iter().position(|c| c.feature == coeff_name);
        }
    }
    None
}

/// Phase 6A.3 item 3: evaluate the metric over the leaf-coord set
/// optionally restricted by `filter`. Returns `None` when a non-empty
/// filter matches zero coordinates (handoff Decision Matrix W3); without
/// a filter, an empty leaf set falls through to the aggregator's
/// historical empty-result default (0.0 for all four aggregators).
fn eval_metric(
    cube: &mut mc_core::Cube,
    refs: &mc_model::ModelRefs,
    principal: mc_core::PrincipalId,
    metric_expr: &str,
    filter: Option<&crate::query::Filter>,
) -> Option<f64> {
    let trimmed = metric_expr.trim();
    let all_coords = crate::query::enumerate_leaf_coords(cube, refs);
    let coords: Vec<mc_core::CellCoordinate> = if let Some(f) = filter {
        all_coords
            .into_iter()
            .filter(|c| crate::query::eval_filter(f, c, cube, principal, refs))
            .collect()
    } else {
        all_coords
    };
    // W3: restricted-to-empty returns Null. The unrestricted-empty path
    // (cube genuinely has no leaves — extremely rare in practice) falls
    // through to the historical empty-aggregate default below for parity.
    if filter.is_some() && coords.is_empty() {
        return None;
    }

    // Pick the inner measure name + aggregator, or fall back to "treat
    // the whole expression as a measure name and mean it" for parity
    // with the previous behaviour.
    let (measure, agg) = if let Some(inner) = strip_fn("mean", trimmed) {
        (inner, Aggregator::Mean)
    } else if let Some(inner) = strip_fn("sum", trimmed) {
        (inner, Aggregator::Sum)
    } else if let Some(inner) = strip_fn("max", trimmed) {
        (inner, Aggregator::Max)
    } else if let Some(inner) = strip_fn("min", trimmed) {
        (inner, Aggregator::Min)
    } else {
        (trimmed, Aggregator::Mean)
    };

    let mut sum = 0.0;
    let mut count = 0usize;
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for coord in &coords {
        if let ScalarValue::F64(v) =
            crate::query::read_measure_at(cube, refs, principal, coord, measure)
        {
            sum += v;
            count += 1;
            if v < min {
                min = v;
            }
            if v > max {
                max = v;
            }
        }
    }
    let value = match agg {
        Aggregator::Mean => {
            if count > 0 {
                sum / count as f64
            } else {
                0.0
            }
        }
        Aggregator::Sum => sum,
        Aggregator::Max => {
            if max == f64::NEG_INFINITY {
                0.0
            } else {
                max
            }
        }
        Aggregator::Min => {
            if min == f64::INFINITY {
                0.0
            } else {
                min
            }
        }
    };
    Some(value)
}

enum Aggregator {
    Mean,
    Sum,
    Max,
    Min,
}

fn strip_fn<'a>(name: &str, expr: &'a str) -> Option<&'a str> {
    let trimmed = expr.trim();
    if trimmed.starts_with(name) && trimmed[name.len()..].starts_with('(') && trimmed.ends_with(')')
    {
        Some(&trimmed[name.len() + 1..trimmed.len() - 1])
    } else {
        None
    }
}

fn format_sweep_output(
    cmd: &SweepCommand,
    results: &[SweepPoint],
    baseline: Option<f64>,
    optimal: Option<&SweepPoint>,
    format: OutputFormat,
    verbose: bool,
    measure_descs: &std::collections::HashMap<String, String>,
) -> String {
    // Phase 6A.3 item 3: when `baseline` or a per-point metric is None
    // (the `--metric-where` filter matched zero coords), emit JSON `null`
    // and an "n/a" placeholder for text/CSV deltas.
    let delta_of = |m: Option<f64>| -> Option<f64> {
        match (m, baseline) {
            (Some(a), Some(b)) => Some(a - b),
            _ => None,
        }
    };
    match format {
        OutputFormat::Json => {
            let mut out = String::new();
            push_json_envelope_header(&mut out);
            out.push_str("\"metric\": ");
            push_json_str(&mut out, &cmd.metric);
            out.push_str(",\n  \"goal\": ");
            push_json_str(&mut out, &cmd.goal);
            out.push_str(",\n  \"baseline\": ");
            push_opt_f64_json(&mut out, baseline);
            out.push_str(",\n  \"sweep\": [\n");
            for (i, point) in results.iter().enumerate() {
                let _ = write!(out, "    {{\"value\":{},\"metric\":", point.parameter_value);
                push_opt_f64_json(&mut out, point.metric_value);
                out.push_str(",\"delta_from_baseline\":");
                push_opt_f64_json(&mut out, delta_of(point.metric_value));
                out.push('}');
                if i + 1 < results.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str("  ],\n  \"optimal\": ");
            if let Some(opt) = optimal {
                let _ = write!(out, "{{\"value\":{},\"metric\":", opt.parameter_value);
                push_opt_f64_json(&mut out, opt.metric_value);
                out.push('}');
            } else {
                out.push_str("null");
            }
            out.push_str("\n}\n");
            out
        }
        OutputFormat::Text => {
            let mut out = String::new();
            let param_label = cmd.coefficient.as_deref().unwrap_or("parameter");
            let _ = writeln!(out, "Sweep: {} over range {}", param_label, cmd.range);
            let _ = writeln!(out, "Metric: {} (goal: {})", cmd.metric, cmd.goal);
            // Phase 4D: verbose description for the swept metric.
            if verbose {
                let metric_name = extract_metric_measure_name(&cmd.metric);
                if let Some(desc) = crate::verbose::measure_description(measure_descs, metric_name)
                {
                    out.push_str(&crate::verbose::format_description_line(desc, None));
                }
            }
            let _ = writeln!(out, "Baseline: {}\n", format_opt_f64(baseline));
            let _ = writeln!(out, "{:<12} {:<15} {:<15}", "Value", "Metric", "Delta");
            let _ = writeln!(out, "{}", "-".repeat(42));
            for point in results {
                let delta = delta_of(point.metric_value);
                let delta_str = match delta {
                    Some(v) if v >= 0.0 => format!("+{}", format_f64(v)),
                    Some(v) => format_f64(v),
                    None => "n/a".into(),
                };
                let _ = writeln!(
                    out,
                    "{:<12} {:<15} {:<15}",
                    format_f64(point.parameter_value),
                    format_opt_f64(point.metric_value),
                    delta_str
                );
            }
            if let Some(opt) = optimal {
                let _ = writeln!(
                    out,
                    "\nOptimal: {} = {} (metric = {})",
                    param_label,
                    format_f64(opt.parameter_value),
                    format_opt_f64(opt.metric_value)
                );
            }
            out
        }
        OutputFormat::Csv => {
            let mut out = String::from("parameter_value,metric_value,delta_from_baseline\n");
            for point in results {
                let delta = delta_of(point.metric_value);
                let _ = writeln!(
                    out,
                    "{},{},{}",
                    point.parameter_value,
                    format_opt_f64(point.metric_value),
                    format_opt_f64(delta)
                );
            }
            out
        }
    }
}

fn push_opt_f64_json(out: &mut String, v: Option<f64>) {
    match v {
        Some(f) => {
            let _ = write!(out, "{f}");
        }
        None => out.push_str("null"),
    }
}

/// Extract the inner measure name from a metric expression like
/// `mean(Spend)`, `sum(Revenue)`, or just `Spend`.
fn extract_metric_measure_name(metric: &str) -> &str {
    let trimmed = metric.trim();
    if let Some(inner) = strip_fn("mean", trimmed)
        .or_else(|| strip_fn("sum", trimmed))
        .or_else(|| strip_fn("max", trimmed))
        .or_else(|| strip_fn("min", trimmed))
    {
        inner
    } else {
        trimmed
    }
}

fn format_opt_f64(v: Option<f64>) -> String {
    match v {
        Some(f) => format_f64(f),
        None => "null".into(),
    }
}

fn format_dry_run(cmd: &SweepCommand, points: &[f64]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "[dry-run] Sweep plan:");
    let _ = writeln!(out, "  Range: {} ({} points)", cmd.range, points.len());
    let _ = writeln!(out, "  Metric: {}", cmd.metric);
    let _ = writeln!(out, "  Goal: {}", cmd.goal);
    if let Some(m) = &cmd.model_name {
        let _ = writeln!(out, "  Model: {m}");
    }
    if let Some(c) = &cmd.coefficient {
        let _ = writeln!(out, "  Coefficient: {c}");
    }
    if let Some(s) = &cmd.set_coord {
        let _ = writeln!(out, "  Cell: {s}");
    }
    let _ = writeln!(out, "  Points: {:?}", points);
    out
}
