//! `mc model whatif` — override one or more input cells, report deltas.
//!
//! Atomicity invariant: load → snapshot → override-all → compute → report → rollback.
//! The source CSV is never modified. No persistent side effects.
//!
//! Phase 6A.3 item 1: `--set` is repeatable. Each invocation contributes one
//! override; all overrides are validated up front and applied as an atomic
//! group via `Cube::snapshot`/`rollback_to`. If any override fails validation,
//! every override is rolled back and exit code 1 is returned.
//!
//! Two `--set` forms are accepted:
//!
//! - **Legacy single-cell form** (existing callers): `--set <coord> --value <n>`.
//!   Exactly one `--set`; the value comes from `--value`.
//! - **New repeatable form** (Phase 6A.3): `--set "<coord>=<value>"` repeated
//!   one or more times. The trailing `=<value>` carries the override; `--value`
//!   must be omitted.

use crate::query::{
    format_f64, format_scalar, load_model, parse_coord_string, push_json_envelope_header,
    push_json_str, OutputFormat,
};
use mc_core::{ScalarValue, WriteIntent, WritebackRequest};
use std::fmt::Write;

/// One coordinate-and-value pair the user wants to override.
#[derive(Debug, Clone)]
pub struct Override {
    pub coord_str: String,
    pub value: f64,
}

pub struct WhatifCommand {
    pub path: String,
    pub format: OutputFormat,
    /// Phase 6A.3 item 1: one entry per `--set` flag, in the order supplied.
    /// At least one entry is always present (parser rejects zero).
    pub overrides: Vec<Override>,
    pub show: Vec<String>,
    pub dry_run: bool,
    pub time_anchor: Option<String>,
}

pub fn parse(args: &[String]) -> Result<WhatifCommand, String> {
    let mut path: Option<String> = None;
    let mut format = OutputFormat::Text;
    // Raw --set strings (not yet split into coord+value); collected in order.
    let mut raw_sets: Vec<String> = Vec::new();
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
                Some(v) => raw_sets.push(v.clone()),
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
    if raw_sets.is_empty() {
        return Err("--set is required (repeatable; use --set \"coord=value\" or --set <coord> --value <n>)".into());
    }
    let show = show.unwrap_or_default();
    if show.is_empty() {
        return Err("--show is required (comma-separated measure names)".into());
    }

    // Phase 6A.3 item 1 W1: resolve --set forms.
    //
    // - If --value is given: legacy single-cell form. Exactly one --set,
    //   treated as a bare coordinate; --value supplies the override.
    // - Otherwise: each --set must be `coord=value`. The trailing `=NUM`
    //   on the LAST `=` separates coord from value (a coord cannot
    //   legally end in `Measure=<elem>=<num>` so the rsplit is unambiguous).
    let overrides: Vec<Override> = if let Some(legacy_value) = value {
        if raw_sets.len() != 1 {
            return Err(
                "--value is incompatible with multiple --set flags; use --set \"coord=value\" form for each override".into(),
            );
        }
        vec![Override {
            coord_str: raw_sets.into_iter().next().unwrap(),
            value: legacy_value,
        }]
    } else {
        let mut overrides = Vec::with_capacity(raw_sets.len());
        for raw in raw_sets {
            let (coord_str, value_str) = raw.rsplit_once('=').ok_or_else(|| {
                format!(
                    "--set must be either `<coord>=<value>` or paired with --value; got {raw:?}"
                )
            })?;
            let v = value_str.trim().parse::<f64>().map_err(|_| {
                format!("--set {raw:?}: trailing `=<value>` must be a number, got {value_str:?}")
            })?;
            overrides.push(Override {
                coord_str: coord_str.trim().to_string(),
                value: v,
            });
        }
        overrides
    };

    Ok(WhatifCommand {
        path,
        format,
        overrides,
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
        match anchor_idx {
            Some(idx) => cube.reference_data.time_anchor_index = Some(idx),
            None => {
                eprintln!("error: --time-anchor '{anchor_name}' does not match any element");
                return (1, String::new());
            }
        }
    }

    // Phase 6A.3 item 1: resolve every --set coord BEFORE any write.
    //
    // The list-resolve pattern collects every error so the user sees them
    // all at once (handoff Decision Matrix W3) and exits 1 without any
    // partial mutation. Subsequent kernel-level errors during the actual
    // write loop trigger a snapshot rollback (see below).
    let mut resolved: Vec<(Override, mc_core::CellCoordinate)> =
        Vec::with_capacity(cmd.overrides.len());
    let mut resolve_errors: Vec<String> = Vec::new();
    for (i, ov) in cmd.overrides.iter().enumerate() {
        let coord_names = parse_coord_string(&ov.coord_str);
        match refs.coord_from_names(&coord_names) {
            Some(c) => resolved.push((ov.clone(), c)),
            None => resolve_errors.push(format!(
                "--set #{}: could not resolve coordinate {:?}",
                i + 1,
                ov.coord_str
            )),
        }
    }
    if !resolve_errors.is_empty() {
        for e in &resolve_errors {
            eprintln!("error: {e}");
        }
        return (1, String::new());
    }

    // Read "before" values at every override coordinate (pre-mutation).
    let mut before_overrides: Vec<ScalarValue> = Vec::with_capacity(resolved.len());
    for (_, coord) in &resolved {
        match cube.read(coord, principal) {
            Ok(cell) => before_overrides.push(cell.value),
            Err(e) => {
                eprintln!("error: could not read target cell: {e}");
                return (1, String::new());
            }
        }
    }

    // Anchor for --show reads: the first override's non-measure coord. The
    // existing single-cell semantic (read --show at the override's
    // non-measure coord with the measure swapped) is preserved exactly when
    // there is one override.
    let anchor_coord = resolved[0].1.clone();

    // Read "before" values for --show measures at the anchor coord.
    let before_measures = read_show_measures(&mut cube, principal, &anchor_coord, &cmd.show);

    if cmd.dry_run {
        // No mutation — describe the would-be overrides.
        let output_str = format_dry_run(&cmd, &before_overrides, cmd.format);
        return (0, output_str);
    }

    // Phase 6A.3 item 1 W3: snapshot before applying any override; if any
    // kernel-level write fails, rollback to the snapshot and exit 1 with
    // every error reported. This gives atomic semantics — the in-process
    // cube is never observed in a half-written state by the --show pass.
    let snapshot = cube.snapshot(Some("phase-6a-3:whatif:pre-overrides"));
    let mut write_errors: Vec<String> = Vec::new();
    for (i, (ov, coord)) in resolved.iter().enumerate() {
        let req = WritebackRequest {
            coord: coord.clone(),
            new_value: ScalarValue::F64(ov.value),
            principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        };
        if let Err(e) = cube.write(req) {
            write_errors.push(format!("--set #{}: write failed: {e}", i + 1));
        }
    }
    if !write_errors.is_empty() {
        // Rollback before reporting so the agent observes no partial state.
        if let Err(e) = cube.rollback_to(&snapshot) {
            eprintln!("error: rollback after failed override also failed: {e}");
        }
        for e in &write_errors {
            eprintln!("error: {e}");
        }
        return (1, String::new());
    }

    // Read "after" values for --show measures at the anchor coord.
    let after_measures = read_show_measures(&mut cube, principal, &anchor_coord, &cmd.show);

    // Build deltas for each --show measure.
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

    // Build per-override before/after rows (for the new `overrides` array).
    let mut override_rows: Vec<OverrideRow> = Vec::with_capacity(resolved.len());
    for (i, (ov, _)) in resolved.iter().enumerate() {
        override_rows.push(OverrideRow {
            coord_str: ov.coord_str.clone(),
            before: before_overrides[i].clone(),
            after: ov.value,
        });
    }

    let output_str = format_whatif_output(&override_rows, &affected, cmd.format);
    (0, output_str)
}

struct AffectedMeasure {
    name: String,
    before: Option<f64>,
    after: Option<f64>,
    delta: Option<f64>,
}

struct OverrideRow {
    coord_str: String,
    before: ScalarValue,
    after: f64,
}

fn read_show_measures(
    cube: &mut mc_core::Cube,
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
    overrides: &[OverrideRow],
    affected: &[AffectedMeasure],
    format: OutputFormat,
) -> String {
    match format {
        OutputFormat::Json => {
            let mut out = String::new();
            push_json_envelope_header(&mut out);
            // `cell_overridden` (backward-compat single-override echo for
            // pre-Phase 6A.3 agents). Always equals overrides[0].
            let head = &overrides[0];
            out.push_str("\"cell_overridden\": {\n    \"coord\": ");
            push_json_str(&mut out, &head.coord_str);
            out.push_str(",\n    \"before\": ");
            push_scalar_val(&mut out, &head.before);
            out.push_str(",\n    \"after\": ");
            let _ = write!(out, "{}", head.after);
            out.push_str("\n  },\n  \"overrides\": [\n");
            for (i, ov) in overrides.iter().enumerate() {
                out.push_str("    {\"coord\":");
                push_json_str(&mut out, &ov.coord_str);
                out.push_str(",\"before\":");
                push_scalar_val(&mut out, &ov.before);
                out.push_str(",\"after\":");
                let _ = write!(out, "{}", ov.after);
                out.push('}');
                if i + 1 < overrides.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str("  ],\n  \"affected_measures\": [\n");
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
            if overrides.len() == 1 {
                let head = &overrides[0];
                let _ = writeln!(out, "What-if: set {} = {}", head.coord_str, head.after);
                let _ = writeln!(
                    out,
                    "  Override: {} → {}",
                    format_scalar(&head.before),
                    head.after
                );
            } else {
                let _ = writeln!(out, "What-if: {} overrides applied:", overrides.len());
                for ov in overrides {
                    let _ = writeln!(
                        out,
                        "  {} : {} → {}",
                        ov.coord_str,
                        format_scalar(&ov.before),
                        ov.after
                    );
                }
            }
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
    before_overrides: &[ScalarValue],
    format: OutputFormat,
) -> String {
    match format {
        OutputFormat::Json => {
            let mut out = String::new();
            push_json_envelope_header(&mut out);
            // Backward-compat `would_override` echoes the first override.
            let head_ov = &cmd.overrides[0];
            out.push_str("\"dry_run\": true,\n  \"would_override\": {\n    \"coord\": ");
            push_json_str(&mut out, &head_ov.coord_str);
            out.push_str(",\n    \"current_value\": ");
            push_scalar_val(&mut out, &before_overrides[0]);
            out.push_str(",\n    \"new_value\": ");
            let _ = write!(out, "{}", head_ov.value);
            out.push_str("\n  },\n  \"overrides\": [\n");
            for (i, ov) in cmd.overrides.iter().enumerate() {
                out.push_str("    {\"coord\":");
                push_json_str(&mut out, &ov.coord_str);
                out.push_str(",\"current_value\":");
                push_scalar_val(&mut out, &before_overrides[i]);
                out.push_str(",\"new_value\":");
                let _ = write!(out, "{}", ov.value);
                out.push('}');
                if i + 1 < cmd.overrides.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            // Phase 6A.3 item 7: rename `would_affect` → `requested_outputs`.
            // The previous name was misleading (it echoed --show, not the
            // computed dependent closure). The new name is honest about
            // what the field contains.
            out.push_str("  ],\n  \"requested_outputs\": [");
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
            if cmd.overrides.len() == 1 {
                let head = &cmd.overrides[0];
                let _ = writeln!(
                    out,
                    "[dry-run] Would set {} = {}",
                    head.coord_str, head.value
                );
                let _ = writeln!(
                    out,
                    "  Current value: {}",
                    format_scalar(&before_overrides[0])
                );
            } else {
                let _ = writeln!(
                    out,
                    "[dry-run] Would apply {} overrides:",
                    cmd.overrides.len()
                );
                for (i, ov) in cmd.overrides.iter().enumerate() {
                    let _ = writeln!(
                        out,
                        "  {} : {} → {}",
                        ov.coord_str,
                        format_scalar(&before_overrides[i]),
                        ov.value
                    );
                }
            }
            let _ = writeln!(out, "  Requested outputs: {}", cmd.show.join(", "));
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
