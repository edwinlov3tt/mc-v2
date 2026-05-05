//! `mc model whatif` — override one input, report deltas.
//!
//! Atomicity invariant: load → snapshot → override → compute → report → rollback.
//! The source CSV is never modified. No persistent side effects.

use crate::query::{
    format_f64, format_scalar, load_model, parse_coord_string, push_json_str, OutputFormat,
};
use mc_core::{ScalarValue, WriteIntent, WritebackRequest};
use std::fmt::Write;

pub struct WhatifCommand {
    pub path: String,
    pub format: OutputFormat,
    pub set_coord: String,
    pub value: f64,
    pub show: Vec<String>,
    pub dry_run: bool,
    pub time_anchor: Option<String>,
}

pub fn parse(args: &[String]) -> Result<WhatifCommand, String> {
    let mut path: Option<String> = None;
    let mut format = OutputFormat::Text;
    let mut set_coord: Option<String> = None;
    let mut value: Option<f64> = None;
    let mut show: Option<Vec<String>> = None;
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
            "--set" => match iter.next() {
                Some(v) => set_coord = Some(v.clone()),
                None => return Err("--set requires a coordinate string".into()),
            },
            "--value" => match iter.next() {
                Some(v) => {
                    value = Some(
                        v.parse::<f64>()
                            .map_err(|_| format!("--value must be a number, got {v:?}"))?,
                    )
                }
                None => return Err("--value requires a number".into()),
            },
            "--show" => match iter.next() {
                Some(v) => show = Some(v.split(',').map(|s| s.trim().to_string()).collect()),
                None => return Err("--show requires a comma-separated list".into()),
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
    let path = path.ok_or("`mc model whatif` requires a YAML model path")?;
    let set_coord = set_coord.ok_or("--set is required")?;
    let value = value.ok_or("--value is required")?;
    let show = show.unwrap_or_default();
    if show.is_empty() {
        return Err("--show is required (comma-separated measure names)".into());
    }
    Ok(WhatifCommand {
        path,
        format,
        set_coord,
        value,
        show,
        dry_run,
        time_anchor,
    })
}

pub fn run(cmd: WhatifCommand) -> i32 {
    let (code, output) = run_captured(cmd);
    if !output.is_empty() {
        print!("{output}");
    }
    code
}

/// Execute the whatif verb and return (exit_code, output_string).
/// Used by MCP to capture output without printing to stdout.
pub fn run_captured(cmd: WhatifCommand) -> (i32, String) {
    let loaded = match load_model(&cmd.path) {
        Ok(l) => l,
        Err(msg) => {
            eprintln!("error: {msg}");
            return (1, String::new());
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
        match anchor_idx {
            Some(idx) => cube.reference_data.time_anchor_index = Some(idx),
            None => {
                eprintln!("error: --time-anchor '{anchor_name}' does not match any element");
                return (1, String::new());
            }
        }
    }

    // Parse the set coordinate
    let coord_names = parse_coord_string(&cmd.set_coord);
    let coord = match refs.coord_from_names(&coord_names) {
        Some(c) => c,
        None => {
            eprintln!(
                "error: could not resolve --set coordinate: {}",
                cmd.set_coord
            );
            return (1, String::new());
        }
    };

    // Read "before" value at the overridden cell
    let before_value = match cube.read(&coord, principal) {
        Ok(cell) => cell.value,
        Err(e) => {
            eprintln!("error: could not read target cell: {e}");
            return (1, String::new());
        }
    };

    // Read "before" values for --show measures at the same non-measure coord
    let before_measures = read_show_measures(&mut cube, refs, principal, &coord, &cmd.show);

    if cmd.dry_run {
        // Just show what would change
        let output_str = format_dry_run(&cmd, &before_value, &before_measures, cmd.format);
        return (0, output_str);
    }

    // Write the override
    let write_result = cube.write(WritebackRequest {
        coord: coord.clone(),
        new_value: ScalarValue::F64(cmd.value),
        principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    });
    if let Err(e) = write_result {
        eprintln!("error: write failed: {e}");
        return (1, String::new());
    }

    // Read "after" values for --show measures
    let after_measures = read_show_measures(&mut cube, refs, principal, &coord, &cmd.show);

    // Build deltas
    let mut affected: Vec<AffectedMeasure> = Vec::new();
    for (i, name) in cmd.show.iter().enumerate() {
        let before = before_measures[i];
        let after = after_measures[i];
        let delta = match (before, after) {
            (Some(b), Some(a)) => Some(a - b),
            _ => None,
        };
        affected.push(AffectedMeasure {
            name: name.clone(),
            before,
            after,
            delta,
        });
    }

    let output_str = format_whatif_output(
        &cmd.set_coord,
        &before_value,
        cmd.value,
        &affected,
        cmd.format,
    );
    (0, output_str)
}

struct AffectedMeasure {
    name: String,
    before: Option<f64>,
    after: Option<f64>,
    delta: Option<f64>,
}

fn read_show_measures(
    cube: &mut mc_core::Cube,
    refs: &mc_model::ModelRefs,
    principal: mc_core::PrincipalId,
    base_coord: &mc_core::CellCoordinate,
    show: &[String],
) -> Vec<Option<f64>> {
    let measure_dim_idx = cube
        .dimensions()
        .iter()
        .position(|d| d.kind == mc_core::DimensionKind::Measure)
        .unwrap_or(0);

    show.iter()
        .map(|name| {
            let measure_dim = &cube.dimensions()[measure_dim_idx];
            let elem = measure_dim.element_by_name(name)?;
            let mut slots = base_coord.elements().to_vec();
            slots[measure_dim_idx] = elem.id;
            let coord = mc_core::CellCoordinate::from_parts(cube.id, slots);
            match cube.read(&coord, principal) {
                Ok(cell) => match cell.value {
                    ScalarValue::F64(f) => Some(f),
                    _ => None,
                },
                Err(_) => None,
            }
        })
        .collect()
}

fn format_whatif_output(
    set_coord: &str,
    before_value: &ScalarValue,
    after_value: f64,
    affected: &[AffectedMeasure],
    format: OutputFormat,
) -> String {
    match format {
        OutputFormat::Json => {
            let mut out = String::new();
            out.push_str("{\n  \"cell_overridden\": {\n    \"coord\": ");
            push_json_str(&mut out, set_coord);
            out.push_str(",\n    \"before\": ");
            push_scalar_val(&mut out, before_value);
            out.push_str(",\n    \"after\": ");
            let _ = write!(out, "{after_value}");
            out.push_str("\n  },\n  \"affected_measures\": [\n");
            for (i, m) in affected.iter().enumerate() {
                out.push_str("    {\"measure\":");
                push_json_str(&mut out, &m.name);
                out.push_str(",\"before\":");
                push_opt_f64(&mut out, m.before);
                out.push_str(",\"after\":");
                push_opt_f64(&mut out, m.after);
                out.push_str(",\"delta\":");
                push_opt_f64(&mut out, m.delta);
                out.push('}');
                if i + 1 < affected.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str("  ]\n}\n");
            out
        }
        OutputFormat::Text => {
            let mut out = String::new();
            let _ = writeln!(out, "What-if: set {set_coord} = {after_value}");
            let _ = writeln!(
                out,
                "  Override: {} → {after_value}",
                format_scalar(before_value)
            );
            let _ = writeln!(out, "\n  Affected measures:");
            for m in affected {
                let before_s = m.before.map(format_f64).unwrap_or_else(|| "null".into());
                let after_s = m.after.map(format_f64).unwrap_or_else(|| "null".into());
                let delta_s = m
                    .delta
                    .map(|v| {
                        if v >= 0.0 {
                            format!("+{}", format_f64(v))
                        } else {
                            format_f64(v)
                        }
                    })
                    .unwrap_or_else(|| "n/a".into());
                let _ = writeln!(out, "    {:<20} {before_s} → {after_s} ({delta_s})", m.name);
            }
            out
        }
        OutputFormat::Csv => {
            let mut out = String::from("measure,before,after,delta\n");
            for m in affected {
                let _ = writeln!(
                    out,
                    "{},{},{},{}",
                    m.name,
                    m.before.map(format_f64).unwrap_or_default(),
                    m.after.map(format_f64).unwrap_or_default(),
                    m.delta.map(format_f64).unwrap_or_default(),
                );
            }
            out
        }
    }
}

fn format_dry_run(
    cmd: &WhatifCommand,
    before_value: &ScalarValue,
    before_measures: &[Option<f64>],
    format: OutputFormat,
) -> String {
    match format {
        OutputFormat::Json => {
            let mut out = String::new();
            out.push_str("{\n  \"dry_run\": true,\n  \"would_override\": {\n    \"coord\": ");
            push_json_str(&mut out, &cmd.set_coord);
            out.push_str(",\n    \"current_value\": ");
            push_scalar_val(&mut out, before_value);
            out.push_str(",\n    \"new_value\": ");
            let _ = write!(out, "{}", cmd.value);
            out.push_str("\n  },\n  \"would_affect\": [");
            for (i, name) in cmd.show.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_json_str(&mut out, name);
            }
            out.push_str("]\n}\n");
            out
        }
        _ => {
            let mut out = String::new();
            let _ = writeln!(out, "[dry-run] Would set {} = {}", cmd.set_coord, cmd.value);
            let _ = writeln!(out, "  Current value: {}", format_scalar(before_value));
            let _ = writeln!(out, "  Would affect: {}", cmd.show.join(", "));
            out
        }
    }
}

fn push_scalar_val(out: &mut String, v: &ScalarValue) {
    match v {
        ScalarValue::F64(f) => {
            let _ = write!(out, "{f}");
        }
        ScalarValue::Null => out.push_str("null"),
        other => {
            out.push('"');
            out.push_str(&crate::query::format_scalar(other));
            out.push('"');
        }
    }
}

fn push_opt_f64(out: &mut String, v: Option<f64>) {
    match v {
        Some(f) => {
            let _ = write!(out, "{f}");
        }
        None => out.push_str("null"),
    }
}
