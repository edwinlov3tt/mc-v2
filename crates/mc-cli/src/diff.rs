//! `mc model diff` — compare two cube states.
//!
//! Reports cells where values differ between two states, sorted by abs(delta).
//! Use cases: detect line movements, compare scenarios, track changes since
//! last import.

use crate::query::{
    format_f64, load_model, parse_coord_string, push_json_envelope_header, push_json_str,
    OutputFormat,
};
use mc_core::{DimensionKind, ScalarValue};
use std::collections::BTreeMap;
use std::fmt::Write;

pub struct DiffCommand {
    pub path: String,
    pub format: OutputFormat,
    pub left: Option<String>,
    pub right: Option<String>,
    pub limit: usize,
    pub time_anchor: Option<String>,
    /// Phase 4D: enrich text output with measure descriptions.
    pub verbose: bool,
}

pub fn parse(args: &[String]) -> Result<DiffCommand, String> {
    let mut path: Option<String> = None;
    let mut format = OutputFormat::Text;
    let mut left: Option<String> = None;
    let mut right: Option<String> = None;
    let mut limit = 50usize;
    let mut time_anchor: Option<String> = None;
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
            "--left" => match iter.next() {
                Some(v) => left = Some(v.clone()),
                None => return Err("--left requires a filter expression".into()),
            },
            "--right" => match iter.next() {
                Some(v) => right = Some(v.clone()),
                None => return Err("--right requires a filter expression".into()),
            },
            "--limit" => match iter.next() {
                Some(v) => {
                    limit = v
                        .parse::<usize>()
                        .map_err(|_| format!("--limit must be a number, got {v:?}"))?;
                }
                None => return Err("--limit requires a number".into()),
            },
            "--time-anchor" => match iter.next() {
                Some(v) => time_anchor = Some(v.clone()),
                None => return Err("--time-anchor requires an element name".into()),
            },
            "--verbose" | "-v" => verbose = true,
            other if !other.starts_with("--") && path.is_none() => {
                path = Some(other.to_string());
            }
            other => return Err(format!("unknown argument: {other:?}")),
        }
    }
    let path = path.ok_or("`mc model diff` requires a YAML model path")?;
    if left.is_none() || right.is_none() {
        return Err("--left and --right are both required for diff".into());
    }
    Ok(DiffCommand {
        path,
        format,
        left,
        right,
        limit,
        time_anchor,
        verbose,
    })
}

pub fn run(cmd: DiffCommand) -> i32 {
    let (code, output) = run_captured(cmd);
    if !output.is_empty() {
        print!("{output}");
    }
    code
}

/// Execute the diff verb and return (exit_code, output_string).
/// Used by MCP to capture output without printing to stdout.
pub fn run_captured(cmd: DiffCommand) -> (i32, String) {
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
    let measure_descs = &loaded.measure_descriptions;

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

    // Diff between two scenario/dimension filters
    // --left "Scenario=Base" --right "Scenario=Forecast"
    // We interpret these as coord overrides: for each leaf coord, read with
    // left's overrides vs right's overrides.
    let left_overrides = parse_coord_string(cmd.left.as_deref().unwrap_or(""));
    let right_overrides = parse_coord_string(cmd.right.as_deref().unwrap_or(""));

    // Enumerate all leaf coordinates
    let all_coords = crate::query::enumerate_leaf_coords(&cube, refs);

    // Get measure dimension info
    let measure_dim_idx = cube
        .dimensions()
        .iter()
        .position(|d| d.kind == DimensionKind::Measure)
        .unwrap_or(0);
    let measure_names: Vec<String> = cube.dimensions()[measure_dim_idx]
        .elements
        .iter()
        .map(|e| e.name.clone())
        .collect();

    let mut changes: Vec<DiffEntry> = Vec::new();
    let mut cells_increased = 0usize;
    let mut cells_decreased = 0usize;
    let mut measures_affected = std::collections::BTreeSet::new();

    for base_coord in &all_coords {
        for measure_name in &measure_names {
            // Build left coord (apply left overrides)
            let left_val = read_with_overrides(
                &mut cube,
                refs,
                principal,
                base_coord,
                measure_name,
                &left_overrides,
                measure_dim_idx,
            );
            // Build right coord (apply right overrides)
            let right_val = read_with_overrides(
                &mut cube,
                refs,
                principal,
                base_coord,
                measure_name,
                &right_overrides,
                measure_dim_idx,
            );

            match (left_val, right_val) {
                (Some(l), Some(r)) if (l - r).abs() > 1e-9 => {
                    let delta = r - l;
                    if delta > 0.0 {
                        cells_increased += 1;
                    } else {
                        cells_decreased += 1;
                    }
                    measures_affected.insert(measure_name.clone());

                    // Build coord description
                    let coord_desc = build_coord_desc(base_coord, measure_name, &cube);
                    changes.push(DiffEntry {
                        coord: coord_desc,
                        left: l,
                        right: r,
                        delta,
                    });
                }
                _ => {}
            }
        }
    }

    // Sort by abs(delta) descending
    changes.sort_by(|a, b| {
        b.delta
            .abs()
            .partial_cmp(&a.delta.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    changes.truncate(cmd.limit);

    let total_changed = cells_increased + cells_decreased;
    let max_abs_delta = changes.first().map(|c| c.delta.abs()).unwrap_or(0.0);
    let measures_list: Vec<String> = measures_affected.into_iter().collect();

    let output_str = format_diff_output(
        &changes,
        total_changed,
        cells_increased,
        cells_decreased,
        max_abs_delta,
        &measures_list,
        &cmd.left,
        &cmd.right,
        cmd.format,
        cmd.verbose,
        measure_descs,
    );
    (0, output_str)
}

struct DiffEntry {
    coord: String,
    left: f64,
    right: f64,
    delta: f64,
}

fn read_with_overrides(
    cube: &mut mc_core::Cube,
    _refs: &mc_model::ModelRefs,
    principal: mc_core::PrincipalId,
    base_coord: &mc_core::CellCoordinate,
    measure_name: &str,
    overrides: &BTreeMap<String, String>,
    measure_dim_idx: usize,
) -> Option<f64> {
    let dims = cube.dimensions();
    let mut slots = base_coord.elements().to_vec();

    // Apply dimension overrides (e.g., Scenario=Forecast)
    for (dim_name, elem_name) in overrides {
        if let Some(dim_idx) = dims.iter().position(|d| d.name == *dim_name) {
            if let Some(elem) = dims[dim_idx].element_by_name(elem_name) {
                slots[dim_idx] = elem.id;
            } else {
                return None;
            }
        }
    }

    // Set measure
    let measure_dim = &dims[measure_dim_idx];
    let measure_elem = measure_dim.element_by_name(measure_name)?;
    slots[measure_dim_idx] = measure_elem.id;

    let coord = mc_core::CellCoordinate::from_parts(cube.id, slots);
    match cube.read(&coord, principal) {
        Ok(cell) => match cell.value {
            ScalarValue::F64(f) => Some(f),
            _ => None,
        },
        Err(_) => None,
    }
}

fn build_coord_desc(
    coord: &mc_core::CellCoordinate,
    measure: &str,
    cube: &mc_core::Cube,
) -> String {
    let dims = cube.dimensions();
    let mut parts: Vec<String> = Vec::new();
    for (idx, dim) in dims.iter().enumerate() {
        if dim.kind == DimensionKind::Measure {
            parts.push(format!("Measure={measure}"));
        } else {
            let elem_id = coord.elements()[idx];
            let name = dim.element(elem_id).map(|e| e.name.as_str()).unwrap_or("?");
            parts.push(format!("{}={name}", dim.name));
        }
    }
    parts.join(",")
}

#[allow(clippy::too_many_arguments)]
fn format_diff_output(
    changes: &[DiffEntry],
    total_changed: usize,
    cells_increased: usize,
    cells_decreased: usize,
    max_abs_delta: f64,
    measures_affected: &[String],
    left_desc: &Option<String>,
    right_desc: &Option<String>,
    format: OutputFormat,
    verbose: bool,
    measure_descs: &std::collections::HashMap<String, String>,
) -> String {
    let comparison = format!(
        "{} vs {}",
        left_desc.as_deref().unwrap_or("left"),
        right_desc.as_deref().unwrap_or("right")
    );

    match format {
        OutputFormat::Json => {
            let mut out = String::new();
            push_json_envelope_header(&mut out);
            out.push_str("\"comparison\": ");
            push_json_str(&mut out, &comparison);
            let _ = write!(out, ",\n  \"changed_cells\": {total_changed}");
            out.push_str(",\n  \"top_changes\": [\n");
            for (i, entry) in changes.iter().enumerate() {
                out.push_str("    {\"coord\":");
                push_json_str(&mut out, &entry.coord);
                let _ = write!(
                    out,
                    ",\"left\":{},\"right\":{},\"delta\":{}",
                    entry.left, entry.right, entry.delta
                );
                out.push('}');
                if i + 1 < changes.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str("  ],\n  \"summary\": {\n");
            let _ = writeln!(out, "    \"cells_increased\": {cells_increased},");
            let _ = writeln!(out, "    \"cells_decreased\": {cells_decreased},");
            let _ = writeln!(out, "    \"max_abs_delta\": {max_abs_delta},");
            out.push_str("    \"measures_affected\": [");
            for (i, m) in measures_affected.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_json_str(&mut out, m);
            }
            out.push_str("]\n  }\n}\n");
            out
        }
        OutputFormat::Text => {
            let mut out = String::new();
            let _ = writeln!(out, "Diff: {comparison}");
            let _ = writeln!(out, "Changed cells: {total_changed} ({cells_increased} increased, {cells_decreased} decreased)");
            let _ = writeln!(out, "Max |delta|: {}\n", format_f64(max_abs_delta));
            let _ = writeln!(out, "Top changes (by |delta|):");
            let _ = writeln!(
                out,
                "{:<60} {:>10} {:>10} {:>10}",
                "Coord", "Left", "Right", "Delta"
            );
            let _ = writeln!(out, "{}", "-".repeat(92));
            for entry in changes {
                let delta_str = if entry.delta >= 0.0 {
                    format!("+{}", format_f64(entry.delta))
                } else {
                    format_f64(entry.delta)
                };
                let _ = writeln!(
                    out,
                    "{:<60} {:>10} {:>10} {:>10}",
                    &entry.coord[..entry.coord.len().min(60)],
                    format_f64(entry.left),
                    format_f64(entry.right),
                    delta_str
                );
            }
            // Phase 4D: verbose descriptions for affected measures.
            if verbose {
                let mut descs_shown = false;
                for name in measures_affected {
                    if let Some(desc) = crate::verbose::measure_description(measure_descs, name) {
                        if !descs_shown {
                            out.push('\n');
                            descs_shown = true;
                        }
                        let _ = writeln!(out, "{name}:");
                        out.push_str(&crate::verbose::format_description_line(desc, None));
                    }
                }
            }
            out
        }
        OutputFormat::Csv => {
            let mut out = String::from("coord,left,right,delta\n");
            for entry in changes {
                let _ = writeln!(
                    out,
                    "{},{},{},{}",
                    entry.coord, entry.left, entry.right, entry.delta
                );
            }
            out
        }
    }
}
