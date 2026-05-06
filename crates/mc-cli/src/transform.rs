//! `mc tessera transform` — convert raw data to model-compatible format.
//!
//! Fetches from URL or local file, applies a real `mc-recipe` Recipe's
//! column mappings + defaults, and outputs a clean long-format CSV/JSON
//! matching the model's canonical_inputs shape. Scope: simple HTTP GET
//! only (no OAuth, no pagination, no retry).
//!
//! Phase 6A.2 item 1.5: the Phase 6A bespoke YAML line-scanner only
//! recognized `column_mappings:` / `mappings:` keys with `source` /
//! `target` fields, which `mc-recipe` does not emit. Real
//! `mc-recipe::Recipe` YAML uses `source: SourceConfig` +
//! `columns: Vec<ColumnMapping>` (each with `dimension` xor `measure`).
//! That mismatch silently dropped every mapped row, leaving only
//! defaults in the output. This module now goes through
//! `mc_recipe::parse(&yaml) -> Result<Recipe, RecipeError>` and
//! consults the parsed `Recipe.columns` + `Recipe.defaults` directly.

use crate::query::{push_json_envelope_header, push_json_str, OutputFormat};
use mc_recipe::{DriverKind, Recipe};
use std::fmt::Write;

pub struct TransformCommand {
    pub source: String,
    pub recipe: String,
    pub output: Option<String>,
    pub format: OutputFormat,
    pub preview: Option<usize>,
    /// Phase 6A.3 item 6 W3: HTTP request timeout in seconds for URL
    /// sources. Defaults to 30s. File sources ignore this flag.
    pub timeout_secs: u64,
}

pub fn parse(args: &[String]) -> Result<TransformCommand, String> {
    let mut source: Option<String> = None;
    let mut recipe: Option<String> = None;
    let mut output: Option<String> = None;
    let mut format = OutputFormat::Csv;
    let mut preview: Option<usize> = None;
    let mut timeout_secs: u64 = 30;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--source" => match iter.next() {
                Some(v) => source = Some(expand_env_vars(v)),
                None => return Err("--source requires a path or URL".into()),
            },
            "--recipe" => match iter.next() {
                Some(v) => recipe = Some(v.clone()),
                None => return Err("--recipe requires a YAML path".into()),
            },
            "--output" => match iter.next() {
                Some(v) => output = Some(v.clone()),
                None => return Err("--output requires a file path".into()),
            },
            "--format" => match iter.next() {
                Some(v) if v == "csv" => format = OutputFormat::Csv,
                Some(v) if v == "json" => format = OutputFormat::Json,
                Some(v) if v == "text" => format = OutputFormat::Text,
                Some(v) => return Err(format!("--format must be csv|json|text, got {v:?}")),
                None => return Err("--format requires an argument".into()),
            },
            "--preview" => match iter.next() {
                Some(v) => {
                    preview = Some(
                        v.parse::<usize>()
                            .map_err(|_| format!("--preview must be a number, got {v:?}"))?,
                    )
                }
                None => return Err("--preview requires a number".into()),
            },
            "--timeout-secs" => match iter.next() {
                Some(v) => {
                    timeout_secs = v
                        .parse::<u64>()
                        .map_err(|_| format!("--timeout-secs must be a number, got {v:?}"))?;
                }
                None => return Err("--timeout-secs requires a number".into()),
            },
            other => return Err(format!("unknown argument: {other:?}")),
        }
    }
    let source = source.ok_or("--source is required")?;
    let recipe = recipe.ok_or("--recipe is required")?;
    Ok(TransformCommand {
        source,
        recipe,
        output,
        format,
        preview,
        timeout_secs,
    })
}

pub fn run(cmd: TransformCommand) -> i32 {
    let (code, output) = run_captured(cmd);
    if !output.is_empty() {
        print!("{output}");
    }
    code
}

/// Execute the transform verb and return (exit_code, output_string).
/// Used by MCP to capture output without printing to stdout.
pub fn run_captured(cmd: TransformCommand) -> (i32, String) {
    // 1. Read recipe YAML and parse into a real `mc_recipe::Recipe`.
    let recipe_yaml = match std::fs::read_to_string(&cmd.recipe) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read recipe: {e}");
            return (3, String::new());
        }
    };
    let recipe = match mc_recipe::parse(&recipe_yaml) {
        Ok(r) => r,
        Err(e) => {
            // RecipeError carries an MC5xxx code via Display; surface
            // it as exit 1 (model/recipe class), matching `mc model
            // validate` behavior. Decision Matrix W2.
            eprintln!("error: recipe parse failed: {e}");
            return (1, String::new());
        }
    };

    // 2. Driver gate: transform only handles file/URL drivers (Decision
    // Matrix W3). DB drivers don't make sense here; fail fast with a
    // pointed message instead of silently producing nothing.
    match recipe.source.driver {
        DriverKind::Csv | DriverKind::HttpJson => {}
        other => {
            eprintln!(
                "error: `mc tessera transform` only supports csv / http_json drivers; \
                 recipe declares {other:?}. Use `mc tessera apply` for DB-backed drivers."
            );
            return (1, String::new());
        }
    }

    // 3. Fetch source data. CLI flag `--source` overrides the recipe's
    // `source.path` / `source.url` per Decision Matrix W5.
    let raw_data = match fetch_source(&cmd.source, cmd.timeout_secs) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("error: could not fetch source: {e}");
            return (3, String::new()); // I/O error
        }
    };

    // 4. Parse rows.
    let source_rows = match recipe.source.driver {
        DriverKind::HttpJson => match parse_json_source(&raw_data, &recipe) {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("error: could not parse JSON source: {e}");
                return (1, String::new());
            }
        },
        DriverKind::Csv => match parse_csv_source(&raw_data) {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("error: could not parse CSV source: {e}");
                return (1, String::new());
            }
        },
        _ => unreachable!("driver gated above"),
    };

    // 5. Apply mappings and emit.
    let mut output_rows = apply_recipe(&source_rows, &recipe);
    let output_columns = derive_output_columns(&recipe);

    if let Some(n) = cmd.preview {
        output_rows.truncate(n);
        let preview_str = format_output(&output_rows, &output_columns, cmd.format);
        return (0, preview_str);
    }

    let output_str = format_output(&output_rows, &output_columns, cmd.format);
    let captured = crate::query::capture_output(&output_str, &cmd.output);
    (0, captured)
}

// ---------------------------------------------------------------------------
// Source fetching
// ---------------------------------------------------------------------------

fn fetch_source(source: &str, timeout_secs: u64) -> Result<String, String> {
    if source.starts_with("http://") || source.starts_with("https://") {
        fetch_url(source, timeout_secs)
    } else {
        std::fs::read_to_string(source).map_err(|e| format!("could not read file {source}: {e}"))
    }
}

/// Maximum response body accepted from a URL fetch — 100 MB. Per
/// handoff Decision Matrix W4: agent-safe default; transform isn't
/// streaming-aware and a multi-GB download would OOM the process.
const DEFAULT_MAX_RESPONSE_BYTES: u64 = 100 * 1024 * 1024;

/// Resolve the response-byte cap, honoring the test-only escape hatch
/// `MC_TRANSFORM_MAX_BYTES`. Production users get the 100 MB default;
/// `test_transform_url_oversized_response_returns_error` sets the env
/// var to a tiny number so it can validate the cap behavior without
/// transferring 100 MB over loopback.
fn max_response_bytes() -> u64 {
    std::env::var("MC_TRANSFORM_MAX_BYTES")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_MAX_RESPONSE_BYTES)
}

fn fetch_url(url: &str, timeout_secs: u64) -> Result<String, String> {
    // Phase 6A.3 item 6: replace the curl subprocess with an in-process
    // ureq call. ureq is already pinned at the workspace level via
    // mc-drivers, so adding it as an explicit mc-cli dep does NOT
    // change Cargo.lock. The default timeout (W3) is 30 s, configurable
    // via --timeout-secs. The default response cap (W4) is 100 MB.
    // HTTPS cert validation uses the system root CAs (W5: no
    // `--insecure` flag).
    use std::io::Read;
    let response = ureq::get(url)
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(code, resp) => {
                let preview = resp.into_string().unwrap_or_default();
                let preview = preview.chars().take(200).collect::<String>();
                format!("HTTP {code} from {url}: {preview}")
            }
            ureq::Error::Transport(t) => format!("transport error fetching {url}: {t}"),
        })?;

    let cap = max_response_bytes();
    let mut reader = response.into_reader().take(cap + 1);
    let mut buf: Vec<u8> = Vec::new();
    reader
        .read_to_end(&mut buf)
        .map_err(|e| format!("read failed for {url}: {e}"))?;
    if buf.len() as u64 > cap {
        return Err(format!("response from {url} exceeded the {cap} byte cap"));
    }
    String::from_utf8(buf).map_err(|e| format!("response is not valid UTF-8: {e}"))
}

// ---------------------------------------------------------------------------
// Source parsing
// ---------------------------------------------------------------------------

type Row = Vec<(String, String)>;

fn parse_json_source(data: &str, recipe: &Recipe) -> Result<Vec<Row>, String> {
    let parsed: serde_json::Value =
        serde_json::from_str(data).map_err(|e| format!("JSON parse error: {e}"))?;

    let array = if let Some(jp) = recipe.source.json_path.as_deref() {
        navigate_json_path(&parsed, jp)?
    } else if parsed.is_array() {
        parsed.as_array().ok_or("expected JSON array")?.clone()
    } else if parsed.is_object() {
        let obj = parsed.as_object().ok_or("expected JSON object")?;
        let mut found = None;
        for (_, v) in obj {
            if v.is_array() {
                found = Some(v.as_array().ok_or("expected array")?.clone());
                break;
            }
        }
        found.ok_or_else(|| "no array found in JSON response".to_string())?
    } else {
        return Err("expected JSON array or object".into());
    };

    let mut rows = Vec::new();
    for item in &array {
        if let Some(obj) = item.as_object() {
            let row: Row = obj
                .iter()
                .map(|(k, v)| {
                    let val_str = match v {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Number(n) => n.to_string(),
                        serde_json::Value::Bool(b) => b.to_string(),
                        serde_json::Value::Null => String::new(),
                        other => other.to_string(),
                    };
                    (k.clone(), val_str)
                })
                .collect();
            rows.push(row);
        }
    }
    Ok(rows)
}

fn navigate_json_path(
    value: &serde_json::Value,
    path: &str,
) -> Result<Vec<serde_json::Value>, String> {
    // Simplified JSONPath: $.key.subkey[*] or just key.subkey
    let path = path.trim_start_matches("$.");
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = value.clone();
    for part in parts {
        let part = part.trim_end_matches("[*]");
        if part.is_empty() {
            continue;
        }
        current = current
            .get(part)
            .cloned()
            .ok_or_else(|| format!("JSON path not found: {part}"))?;
    }
    match current {
        serde_json::Value::Array(arr) => Ok(arr),
        other => Ok(vec![other]),
    }
}

fn parse_csv_source(data: &str) -> Result<Vec<Row>, String> {
    let mut lines = data.lines();
    let header_line = lines.next().ok_or("CSV is empty")?;
    let headers: Vec<&str> = header_line.split(',').map(|s| s.trim()).collect();

    let mut rows = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let values: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
        let row: Row = headers
            .iter()
            .zip(values.iter())
            .map(|(h, v)| (h.to_string(), v.to_string()))
            .collect();
        rows.push(row);
    }
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Recipe application
// ---------------------------------------------------------------------------

/// Apply the recipe to one batch of source rows. Each output row carries
/// (default_dim, element_name) entries first, then (target, value) for
/// each mapped column where `target` is the recipe's `dimension` /
/// `measure` name (or the literal `"value"` for measure values, with
/// the measure name as a separate column for long-format output).
fn apply_recipe(source_rows: &[Row], recipe: &Recipe) -> Vec<Row> {
    let mut output_rows = Vec::new();

    for source_row in source_rows {
        let mut output_row: Row = Vec::new();

        // 1. Apply defaults (in YAML iteration order — HashMap key order
        //    isn't deterministic, but that doesn't matter here because
        //    the canonical column ordering is computed separately by
        //    `derive_output_columns`).
        for (dim, val) in &recipe.defaults {
            output_row.push((dim.clone(), val.clone()));
        }

        // 2. Apply column mappings. For each mapping, look up the source
        //    column in this row; if found, write it under either the
        //    target `dimension` name or the long-format `value` slot
        //    (with the measure name carried as a separate column).
        let mut measure_name: Option<String> = None;
        let mut measure_value: Option<String> = None;
        for mapping in &recipe.columns {
            if mapping.skip == Some(true) {
                continue;
            }
            let raw_value = source_row
                .iter()
                .find(|(k, _)| *k == mapping.source)
                .map(|(_, v)| v.clone());
            let raw_value = match raw_value {
                Some(v) => v,
                None => continue,
            };
            let value = apply_scale(&raw_value, mapping.scale);

            if let Some(dim) = mapping.dimension.as_deref() {
                output_row.push((dim.to_string(), value));
            } else if let Some(measure) = mapping.measure.as_deref() {
                // Long-format output row: each input mapping that
                // targets a measure becomes one output row downstream.
                // For now we keep the wider "one source row → one
                // output row with all measures inline" shape — the same
                // shape the bespoke parser produced. Producing a true
                // long-format multi-row stream is Phase 5D scope.
                output_row.push((measure.to_string(), value.clone()));
                measure_name = Some(measure.to_string());
                measure_value = Some(value);
            }
        }

        // Carry the last-bound measure forward as a `value` column for
        // tooling that expects a long-form `Measure=...,value=...`
        // shape. Compatible with the legacy bespoke-parser output.
        if let (Some(name), Some(value)) = (measure_name, measure_value) {
            // Avoid clobbering an existing `Measure` column from
            // mappings (unlikely but defensive).
            if !output_row.iter().any(|(k, _)| k == "Measure") {
                output_row.push(("Measure".to_string(), name));
            }
            if !output_row.iter().any(|(k, _)| k == "value") {
                output_row.push(("value".to_string(), value));
            }
        }

        output_rows.push(output_row);
    }

    output_rows
}

fn apply_scale(value: &str, scale: Option<f64>) -> String {
    match scale {
        Some(s) if (s - 1.0).abs() > f64::EPSILON => match value.parse::<f64>() {
            Ok(n) => format!("{}", n * s),
            Err(_) => value.to_string(),
        },
        _ => value.to_string(),
    }
}

/// Compute a stable column order for the output: defaults first (sorted),
/// then mapping targets in YAML order (each appears once), then `Measure`
/// + `value` if any mapping carries a measure.
fn derive_output_columns(recipe: &Recipe) -> Vec<String> {
    let mut cols: Vec<String> = Vec::new();
    let mut default_keys: Vec<&String> = recipe.defaults.keys().collect();
    default_keys.sort();
    for k in default_keys {
        cols.push(k.clone());
    }
    let mut any_measure = false;
    for m in &recipe.columns {
        if m.skip == Some(true) {
            continue;
        }
        if let Some(dim) = m.dimension.as_deref() {
            if !cols.iter().any(|c| c == dim) {
                cols.push(dim.to_string());
            }
        } else if let Some(measure) = m.measure.as_deref() {
            if !cols.iter().any(|c| c == measure) {
                cols.push(measure.to_string());
            }
            any_measure = true;
        }
    }
    if any_measure {
        if !cols.iter().any(|c| c == "Measure") {
            cols.push("Measure".to_string());
        }
        if !cols.iter().any(|c| c == "value") {
            cols.push("value".to_string());
        }
    }
    cols
}

// ---------------------------------------------------------------------------
// Output formatting
// ---------------------------------------------------------------------------

fn format_output(rows: &[Row], columns: &[String], format: OutputFormat) -> String {
    match format {
        OutputFormat::Csv => format_csv_output(rows, columns),
        OutputFormat::Json => format_json_output(rows, columns),
        OutputFormat::Text => format_text_output(rows, columns),
    }
}

fn format_csv_output(rows: &[Row], columns: &[String]) -> String {
    let mut out = String::new();
    if columns.is_empty() && !rows.is_empty() {
        let first_row_cols: Vec<&str> = rows[0].iter().map(|(k, _)| k.as_str()).collect();
        out.push_str(&first_row_cols.join(","));
    } else {
        out.push_str(&columns.join(","));
    }
    out.push('\n');
    for row in rows {
        let cols_to_use: Vec<&str> = if columns.is_empty() {
            row.iter().map(|(k, _)| k.as_str()).collect()
        } else {
            columns.iter().map(|s| s.as_str()).collect()
        };
        let values: Vec<&str> = cols_to_use
            .iter()
            .map(|col| {
                row.iter()
                    .find(|(k, _)| k == col)
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("")
            })
            .collect();
        out.push_str(&values.join(","));
        out.push('\n');
    }
    out
}

fn format_json_output(rows: &[Row], columns: &[String]) -> String {
    // Phase 6A.2 item 1.5 (Codex bonus / COD-2): wrap in the canonical
    // `schema_version: "1.0"` envelope to match every other Phase 6A
    // verb. The previous transform output was a raw JSON array — agents
    // had to special-case the shape vs every sibling verb.
    let mut out = String::new();
    push_json_envelope_header(&mut out);
    let _ = write!(out, "\"count\": {}", rows.len());
    out.push_str(",\n  \"rows\": [\n");
    for (i, row) in rows.iter().enumerate() {
        out.push_str("    {");
        let cols_to_emit: Vec<&str> = if columns.is_empty() {
            row.iter().map(|(k, _)| k.as_str()).collect()
        } else {
            columns.iter().map(|s| s.as_str()).collect()
        };
        for (j, col) in cols_to_emit.iter().enumerate() {
            let v = row
                .iter()
                .find(|(k, _)| k == col)
                .map(|(_, v)| v.as_str())
                .unwrap_or("");
            if j > 0 {
                out.push(',');
            }
            push_json_str(&mut out, col);
            out.push(':');
            if v.is_empty() {
                out.push_str("null");
            } else if let Ok(n) = v.parse::<f64>() {
                if n == n.trunc() && n.abs() < 1e15 {
                    let _ = write!(out, "{}", n as i64);
                } else {
                    let _ = write!(out, "{n}");
                }
            } else {
                push_json_str(&mut out, v);
            }
        }
        out.push('}');
        if i + 1 < rows.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]\n}\n");
    out
}

fn format_text_output(rows: &[Row], columns: &[String]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Transform: {} rows", rows.len());
    let cols: Vec<&str> = if columns.is_empty() {
        match rows.first() {
            Some(first) => first.iter().map(|(k, _)| k.as_str()).collect(),
            None => return out,
        }
    } else {
        columns.iter().map(|s| s.as_str()).collect()
    };
    for col in &cols {
        let _ = write!(out, "{:<18}", col);
    }
    out.push('\n');
    for row in rows.iter().take(20) {
        for col in &cols {
            let val = row
                .iter()
                .find(|(k, _)| k == col)
                .map(|(_, v)| v.as_str())
                .unwrap_or("");
            let _ = write!(out, "{:<18}", &val[..val.len().min(16)]);
        }
        out.push('\n');
    }
    if rows.len() > 20 {
        let _ = writeln!(out, "... ({} more rows)", rows.len() - 20);
    }
    out
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn expand_env_vars(s: &str) -> String {
    let mut result = s.to_string();
    while let Some(start) = result.find("${") {
        if let Some(end) = result[start..].find('}') {
            let var_name = &result[start + 2..start + end];
            let value = std::env::var(var_name).unwrap_or_default();
            result = format!(
                "{}{}{}",
                &result[..start],
                value,
                &result[start + end + 1..]
            );
        } else {
            break;
        }
    }
    while let Some(start) = result.find('$') {
        if start + 1 < result.len() && result.as_bytes()[start + 1] == b'{' {
            break;
        }
        let rest = &result[start + 1..];
        let end = rest
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        if end == 0 {
            break;
        }
        let var_name = &rest[..end];
        let value = std::env::var(var_name).unwrap_or_default();
        result = format!("{}{}{}", &result[..start], value, &rest[end..]);
    }
    result
}
