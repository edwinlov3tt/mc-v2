//! `mc model write` — set one cell without editing CSV.
//!
//! Persists the write to an append-only log at `<model_dir>/.tessera/writes.jsonl`.
//! On next model load, the write log is replayed on top of canonical_inputs.

use crate::query::{
    format_scalar, load_model, parse_coord_string, push_json_envelope_header, push_json_str,
    OutputFormat,
};
use mc_core::{ScalarValue, WriteIntent, WritebackRequest};
use std::fmt::Write;

pub struct WriteCommand {
    pub path: String,
    pub format: OutputFormat,
    pub coord: String,
    pub value: f64,
    pub dry_run: bool,
    pub time_anchor: Option<String>,
}

pub fn parse(args: &[String]) -> Result<WriteCommand, String> {
    let mut path: Option<String> = None;
    let mut format = OutputFormat::Text;
    let mut coord: Option<String> = None;
    let mut value: Option<f64> = None;
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
            "--coord" => match iter.next() {
                Some(v) => coord = Some(v.clone()),
                None => return Err("--coord requires a coordinate string".into()),
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
    let path = path.ok_or("`mc model write` requires a YAML model path")?;
    let coord = coord.ok_or("--coord is required")?;
    let value = value.ok_or("--value is required")?;
    Ok(WriteCommand {
        path,
        format,
        coord,
        value,
        dry_run,
        time_anchor,
    })
}

pub fn run(cmd: WriteCommand) -> i32 {
    let (code, output) = run_captured(cmd);
    if !output.is_empty() {
        print!("{output}");
    }
    code
}

/// Execute the write verb and return (exit_code, output_string).
/// Used by MCP to capture output without printing to stdout.
pub fn run_captured(cmd: WriteCommand) -> (i32, String) {
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

    // Resolve coord
    let coord_names = parse_coord_string(&cmd.coord);
    let coord = match refs.coord_from_names(&coord_names) {
        Some(c) => c,
        None => {
            eprintln!("error: could not resolve coordinate: {}", cmd.coord);
            return (1, String::new());
        }
    };

    // Read current value
    let before = match cube.read(&coord, principal) {
        Ok(cell) => cell.value,
        Err(e) => {
            eprintln!("error: could not read current value: {e}");
            return (1, String::new());
        }
    };

    if cmd.dry_run {
        let output_str = match cmd.format {
            OutputFormat::Json => {
                let mut out = String::new();
                push_json_envelope_header(&mut out);
                out.push_str("\"dry_run\": true,\n  \"coord\": ");
                push_json_str(&mut out, &cmd.coord);
                out.push_str(",\n  \"current_value\": ");
                push_scalar_val(&mut out, &before);
                out.push_str(",\n  \"new_value\": ");
                let _ = write!(out, "{}", cmd.value);
                out.push_str("\n}\n");
                out
            }
            _ => {
                format!(
                    "[dry-run] Would write {} = {} (currently {})\n",
                    cmd.coord,
                    cmd.value,
                    format_scalar(&before)
                )
            }
        };
        return (0, output_str);
    }

    // Perform the write
    let write_result = cube.write(WritebackRequest {
        coord: coord.clone(),
        new_value: ScalarValue::F64(cmd.value),
        principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    });
    match write_result {
        Ok(result) => {
            // Persist to writes.jsonl
            let model_dir = std::path::Path::new(&cmd.path)
                .parent()
                .unwrap_or(std::path::Path::new("."));
            let tessera_dir = model_dir.join(".tessera");
            if !tessera_dir.exists() {
                if let Err(e) = std::fs::create_dir_all(&tessera_dir) {
                    eprintln!("warning: could not create .tessera directory: {e}");
                }
            }
            let log_path = tessera_dir.join("writes.jsonl");
            let timestamp = chrono_now_iso();
            let log_entry = format!(
                "{{\"timestamp\":\"{timestamp}\",\"coord\":{},\"value\":{},\"source\":\"mc model write\"}}\n",
                serde_json::to_string(&cmd.coord).unwrap_or_else(|_| format!("\"{}\"", cmd.coord)),
                cmd.value
            );
            if let Err(e) = append_to_file(&log_path, &log_entry) {
                eprintln!("warning: could not persist to writes.jsonl: {e}");
            }

            let invalidated = result.invalidated.len();
            let output_str = match cmd.format {
                OutputFormat::Json => {
                    let mut out = String::new();
                    push_json_envelope_header(&mut out);
                    out.push_str("\"coord\": ");
                    push_json_str(&mut out, &cmd.coord);
                    out.push_str(",\n  \"before\": ");
                    push_scalar_val(&mut out, &before);
                    let _ = write!(out, ",\n  \"after\": {}", cmd.value);
                    let _ = write!(out, ",\n  \"invalidated_cells\": {invalidated}");
                    out.push_str("\n}\n");
                    out
                }
                _ => {
                    format!(
                        "Written: {} = {} (was {})\nInvalidated: {invalidated} derived cells\n",
                        cmd.coord,
                        cmd.value,
                        format_scalar(&before)
                    )
                }
            };
            (0, output_str)
        }
        Err(e) => {
            eprintln!("error: write failed: {e}");
            (1, String::new())
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

fn chrono_now_iso() -> String {
    // Simple timestamp without chrono dependency
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Format as ISO 8601 (approximate — no timezone handling)
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    // Rough year/month/day (good enough for logging)
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn days_to_ymd(days_since_epoch: u64) -> (u64, u64, u64) {
    // Simplified civil date from days since 1970-01-01
    let mut y = 1970u64;
    let mut remaining = days_since_epoch;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let months: [u64; 12] = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 1u64;
    for &days_in_month in &months {
        if remaining < days_in_month {
            break;
        }
        remaining -= days_in_month;
        m += 1;
    }
    (y, m, remaining + 1)
}

fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn append_to_file(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    use std::io::Write as IoWrite;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(content.as_bytes())
}
