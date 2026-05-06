//! `mc model sweep` — parameter sensitivity analysis.
//!
//! Loops whatif over a range of parameter values, records a metric at each
//! point, and reports the optimal value. Named selectors, in-memory struct
//! override, baseline comparison by default.

use crate::query::{
    format_f64, load_model, push_json_envelope_header, push_json_str, OutputFormat,
};
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

    // For each sweep point, load model fresh, apply override, evaluate metric.
    // (The true baseline is computed separately below; the first-point reading
    // here is no longer wired into the output.)
    let mut results: Vec<SweepPoint> = Vec::new();

    for &point_value in points.iter() {
        let loaded = match load_model(&cmd.path) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("error: {e}");
                return (e.exit_code(), String::new());
            }
        };
        let mut cube = loaded.cube;
        let principal = loaded.root_principal;
        let refs = &loaded.refs;

        // Apply time-anchor override
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

        // Apply the override: either coefficient override or cell override
        if let Some(coeff_name) = &cmd.coefficient {
            // Override a fitted model coefficient
            if let Some(model_name) = &cmd.model_name {
                let coeff_idx = match find_coefficient_index(&cmd.path, model_name, coeff_name) {
                    Some(idx) => idx,
                    None => {
                        eprintln!(
                            "error: coefficient '{coeff_name}' not found in model '{model_name}'"
                        );
                        return (1, String::new());
                    }
                };
                let overridden =
                    override_coefficient(&mut cube, model_name, coeff_idx, point_value);
                if !overridden {
                    eprintln!(
                        "error: could not override coefficient {coeff_name} in model {model_name}"
                    );
                    return (1, String::new());
                }
            } else {
                eprintln!("error: --model is required when using --coefficient");
                return (2, String::new());
            }
        } else if let Some(set_str) = &cmd.set_coord {
            // Override a specific cell
            let coord_names = crate::query::parse_coord_string(set_str);
            let coord = match refs.coord_from_names(&coord_names) {
                Some(c) => c,
                None => {
                    eprintln!("error: could not resolve --set coordinate: {set_str}");
                    return (1, String::new());
                }
            };
            let write_result = cube.write(WritebackRequest {
                coord,
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
        } else {
            eprintln!("error: either --coefficient (with --model) or --set is required");
            return (2, String::new());
        }

        // Evaluate metric
        let metric_value = eval_metric(&mut cube, refs, principal, metric_fn);

        results.push(SweepPoint {
            parameter_value: point_value,
            metric_value,
        });
    }

    // Also get the un-overridden baseline
    let baseline_result = {
        let loaded = match load_model(&cmd.path) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("error: {e}");
                return (e.exit_code(), String::new());
            }
        };
        let mut cube = loaded.cube;
        let principal = loaded.root_principal;
        let refs = &loaded.refs;
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
        eval_metric(&mut cube, refs, principal, metric_fn)
    };

    // Find optimal
    let optimal = if cmd.goal == "minimize" {
        results.iter().min_by(|a, b| {
            a.metric_value
                .partial_cmp(&b.metric_value)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    } else {
        results.iter().max_by(|a, b| {
            a.metric_value
                .partial_cmp(&b.metric_value)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    };

    let output_str = format_sweep_output(&cmd, &results, baseline_result, optimal, cmd.format);
    (0, output_str)
}

struct SweepPoint {
    parameter_value: f64,
    metric_value: f64,
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

fn eval_metric(
    cube: &mut mc_core::Cube,
    refs: &mc_model::ModelRefs,
    principal: mc_core::PrincipalId,
    metric_expr: &str,
) -> f64 {
    // Parse metric: mean(Measure), sum(Measure), max(Measure), etc.
    let trimmed = metric_expr.trim();
    if let Some(inner) = strip_fn("mean", trimmed) {
        let coords = crate::query::enumerate_leaf_coords(cube, refs);
        let mut sum = 0.0;
        let mut count = 0usize;
        for coord in &coords {
            if let ScalarValue::F64(v) =
                crate::query::read_measure_at(cube, refs, principal, coord, inner)
            {
                sum += v;
                count += 1;
            }
        }
        if count > 0 {
            sum / count as f64
        } else {
            0.0
        }
    } else if let Some(inner) = strip_fn("sum", trimmed) {
        let coords = crate::query::enumerate_leaf_coords(cube, refs);
        let mut sum = 0.0;
        for coord in &coords {
            if let ScalarValue::F64(v) =
                crate::query::read_measure_at(cube, refs, principal, coord, inner)
            {
                sum += v;
            }
        }
        sum
    } else if let Some(inner) = strip_fn("max", trimmed) {
        let coords = crate::query::enumerate_leaf_coords(cube, refs);
        let mut max = f64::NEG_INFINITY;
        for coord in &coords {
            if let ScalarValue::F64(v) =
                crate::query::read_measure_at(cube, refs, principal, coord, inner)
            {
                if v > max {
                    max = v;
                }
            }
        }
        if max == f64::NEG_INFINITY {
            0.0
        } else {
            max
        }
    } else if let Some(inner) = strip_fn("min", trimmed) {
        let coords = crate::query::enumerate_leaf_coords(cube, refs);
        let mut min = f64::INFINITY;
        for coord in &coords {
            if let ScalarValue::F64(v) =
                crate::query::read_measure_at(cube, refs, principal, coord, inner)
            {
                if v < min {
                    min = v;
                }
            }
        }
        if min == f64::INFINITY {
            0.0
        } else {
            min
        }
    } else {
        // Treat as single measure name — return mean across all coords
        let coords = crate::query::enumerate_leaf_coords(cube, refs);
        let mut sum = 0.0;
        let mut count = 0usize;
        for coord in &coords {
            if let ScalarValue::F64(v) =
                crate::query::read_measure_at(cube, refs, principal, coord, trimmed)
            {
                sum += v;
                count += 1;
            }
        }
        if count > 0 {
            sum / count as f64
        } else {
            0.0
        }
    }
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
    baseline: f64,
    optimal: Option<&SweepPoint>,
    format: OutputFormat,
) -> String {
    match format {
        OutputFormat::Json => {
            let mut out = String::new();
            push_json_envelope_header(&mut out);
            out.push_str("\"metric\": ");
            push_json_str(&mut out, &cmd.metric);
            out.push_str(",\n  \"goal\": ");
            push_json_str(&mut out, &cmd.goal);
            out.push_str(",\n  \"baseline\": ");
            let _ = write!(out, "{baseline}");
            out.push_str(",\n  \"sweep\": [\n");
            for (i, point) in results.iter().enumerate() {
                let delta = point.metric_value - baseline;
                let _ = write!(
                    out,
                    "    {{\"value\":{},\"metric\":{},\"delta_from_baseline\":{}}}",
                    point.parameter_value, point.metric_value, delta
                );
                if i + 1 < results.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str("  ],\n  \"optimal\": ");
            if let Some(opt) = optimal {
                let _ = write!(
                    out,
                    "{{\"value\":{},\"metric\":{}}}",
                    opt.parameter_value, opt.metric_value
                );
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
            let _ = writeln!(out, "Baseline: {}\n", format_f64(baseline));
            let _ = writeln!(out, "{:<12} {:<15} {:<15}", "Value", "Metric", "Delta");
            let _ = writeln!(out, "{}", "-".repeat(42));
            for point in results {
                let delta = point.metric_value - baseline;
                let delta_str = if delta >= 0.0 {
                    format!("+{}", format_f64(delta))
                } else {
                    format_f64(delta)
                };
                let _ = writeln!(
                    out,
                    "{:<12} {:<15} {:<15}",
                    format_f64(point.parameter_value),
                    format_f64(point.metric_value),
                    delta_str
                );
            }
            if let Some(opt) = optimal {
                let _ = writeln!(
                    out,
                    "\nOptimal: {} = {} (metric = {})",
                    param_label,
                    format_f64(opt.parameter_value),
                    format_f64(opt.metric_value)
                );
            }
            out
        }
        OutputFormat::Csv => {
            let mut out = String::from("parameter_value,metric_value,delta_from_baseline\n");
            for point in results {
                let delta = point.metric_value - baseline;
                let _ = writeln!(
                    out,
                    "{},{},{delta}",
                    point.parameter_value, point.metric_value
                );
            }
            out
        }
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
