//! `mc tessera transform` — convert raw data to model-compatible format.
//!
//! Fetches from URL or local file, applies recipe-driven column mappings,
//! and outputs a clean long-format CSV matching the model's canonical_inputs shape.
//! Scope: simple HTTP GET only (no OAuth, no pagination, no retry).

use crate::query::{push_json_str, OutputFormat};
use std::fmt::Write;

pub struct TransformCommand {
    pub source: String,
    pub recipe: String,
    pub output: Option<String>,
    pub format: OutputFormat,
    pub preview: Option<usize>,
}

pub fn parse(args: &[String]) -> Result<TransformCommand, String> {
    let mut source: Option<String> = None;
    let mut recipe: Option<String> = None;
    let mut output: Option<String> = None;
    let mut format = OutputFormat::Csv;
    let mut preview: Option<usize> = None;

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
    // 1. Fetch source data
    let raw_data = match fetch_source(&cmd.source) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("error: could not fetch source: {e}");
            return (3, String::new()); // I/O error exit code
        }
    };

    // 2. Load recipe
    let recipe_yaml = match std::fs::read_to_string(&cmd.recipe) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read recipe: {e}");
            return (3, String::new());
        }
    };

    // 3. Parse recipe for column mappings and defaults
    let recipe_config = match parse_transform_recipe(&recipe_yaml) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: invalid recipe: {e}");
            return (1, String::new());
        }
    };

    // 4. Parse source data into rows
    let source_rows = if cmd.source.ends_with(".json")
        || cmd.source.starts_with("http")
        || raw_data.trim_start().starts_with('{')
        || raw_data.trim_start().starts_with('[')
    {
        match parse_json_source(&raw_data, &recipe_config) {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("error: could not parse JSON source: {e}");
                return (1, String::new());
            }
        }
    } else {
        match parse_csv_source(&raw_data, &recipe_config) {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("error: could not parse CSV source: {e}");
                return (1, String::new());
            }
        }
    };

    // 5. Apply transforms and build output rows
    let mut output_rows = apply_transforms(&source_rows, &recipe_config);

    // 6. Preview mode — truncate and display without writing
    if let Some(n) = cmd.preview {
        output_rows.truncate(n);
        let preview_str = format_output(&output_rows, &recipe_config, cmd.format);
        return (0, preview_str);
    }

    // 7. Format and emit output
    let output_str = format_output(&output_rows, &recipe_config, cmd.format);
    let captured = crate::query::capture_output(&output_str, &cmd.output);
    (0, captured)
}

// ---------------------------------------------------------------------------
// Source fetching
// ---------------------------------------------------------------------------

fn fetch_source(source: &str) -> Result<String, String> {
    if source.starts_with("http://") || source.starts_with("https://") {
        fetch_url(source)
    } else {
        std::fs::read_to_string(source).map_err(|e| format!("could not read file {source}: {e}"))
    }
}

fn fetch_url(url: &str) -> Result<String, String> {
    // Use a simple blocking HTTP GET via std::net (no external dep needed)
    // Since mc-drivers already depends on ureq, we could use that,
    // but mc-cli doesn't directly depend on ureq. We'll do a simple approach.
    //
    // Actually, let's use std::process::Command to invoke curl as a portable fallback,
    // or implement a minimal HTTP GET.
    // For maximum portability without new deps, use curl subprocess:
    let output = std::process::Command::new("curl")
        .args(["-sS", "-L", "--max-time", "30", url])
        .output()
        .map_err(|e| format!("failed to execute curl: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("curl failed: {stderr}"));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("response is not valid UTF-8: {e}"))
}

// ---------------------------------------------------------------------------
// Recipe parsing (minimal — just column mappings + defaults)
// ---------------------------------------------------------------------------

struct TransformRecipe {
    /// Column mappings: source_col → (target_dim_or_measure, options)
    mappings: Vec<ColumnMapping>,
    /// Default dimension values
    defaults: Vec<(String, String)>,
    /// JSON path to array (for JSON sources)
    json_path: Option<String>,
    /// Output columns in order
    output_columns: Vec<String>,
    /// Scale factors
    scales: Vec<(String, f64)>,
}

struct ColumnMapping {
    source: String,
    target: String,
    is_value: bool,
}

fn parse_transform_recipe(yaml_str: &str) -> Result<TransformRecipe, String> {
    // Minimal YAML parsing without serde_yaml dep — look for key patterns
    // We'll parse a simplified structure using line-by-line scanning.
    let mut mappings: Vec<ColumnMapping> = Vec::new();
    let mut defaults: Vec<(String, String)> = Vec::new();
    let mut json_path: Option<String> = None;
    let mut output_columns: Vec<String> = Vec::new();
    let mut scales: Vec<(String, f64)> = Vec::new();

    let mut in_mappings = false;
    let mut in_defaults = false;
    let mut current_mapping_source: Option<String> = None;
    let mut current_mapping_target: Option<String> = None;
    let mut current_mapping_is_value = false;

    for line in yaml_str.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Top-level keys
        if !line.starts_with(' ') && !line.starts_with('\t') {
            in_mappings =
                trimmed.starts_with("column_mappings:") || trimmed.starts_with("mappings:");
            in_defaults = trimmed.starts_with("defaults:");
            if trimmed.starts_with("json_path:") {
                json_path = extract_yaml_value(trimmed).map(|s| s.to_string());
            }
            if trimmed.starts_with("output_columns:") {
                // Will be parsed from subsequent lines
                output_columns.clear();
            }
            continue;
        }

        if in_defaults {
            // Parse "  dimension_name: "value""
            if let Some((k, v)) = trimmed.split_once(':') {
                let k = k.trim().trim_matches('-').trim();
                let v = v.trim().trim_matches('"').trim_matches('\'');
                if !k.is_empty() && !v.is_empty() {
                    defaults.push((k.to_string(), v.to_string()));
                }
            }
        }

        if in_mappings {
            if trimmed.starts_with("- source:") || trimmed.starts_with("source:") {
                // Flush previous
                if let (Some(src), Some(tgt)) =
                    (current_mapping_source.take(), current_mapping_target.take())
                {
                    mappings.push(ColumnMapping {
                        source: src,
                        target: tgt,
                        is_value: current_mapping_is_value,
                    });
                    current_mapping_is_value = false;
                }
                current_mapping_source = extract_yaml_value(trimmed).map(|s| s.to_string());
            } else if trimmed.starts_with("target:")
                || trimmed.starts_with("dimension:")
                || trimmed.starts_with("measure:")
            {
                current_mapping_target = extract_yaml_value(trimmed).map(|s| s.to_string());
            } else if trimmed.starts_with("is_value:") || trimmed.starts_with("value_column:") {
                current_mapping_is_value = trimmed.contains("true") || trimmed.contains("yes");
            } else if trimmed.starts_with("scale:") {
                if let Some(val) = extract_yaml_value(trimmed) {
                    if let Ok(scale) = val.parse::<f64>() {
                        if let Some(src) = &current_mapping_source {
                            scales.push((src.clone(), scale));
                        }
                    }
                }
            }
        }

        // output_columns list items
        if trimmed.starts_with("- ") && output_columns.is_empty() && !in_mappings && !in_defaults {
            let col = trimmed
                .trim_start_matches("- ")
                .trim()
                .trim_matches('"')
                .trim_matches('\'');
            output_columns.push(col.to_string());
        }
    }
    // Flush last mapping
    if let (Some(src), Some(tgt)) = (current_mapping_source, current_mapping_target) {
        mappings.push(ColumnMapping {
            source: src,
            target: tgt,
            is_value: current_mapping_is_value,
        });
    }

    // If no output columns specified, derive from mappings
    if output_columns.is_empty() {
        for m in &mappings {
            if !output_columns.contains(&m.target) {
                output_columns.push(m.target.clone());
            }
        }
        // Add defaults as dimensions
        for (k, _) in &defaults {
            if !output_columns.contains(k) {
                output_columns.insert(0, k.clone());
            }
        }
        // Ensure "value" column if any mapping is a value
        if mappings.iter().any(|m| m.is_value) && !output_columns.contains(&"value".to_string()) {
            output_columns.push("value".to_string());
        }
    }

    Ok(TransformRecipe {
        mappings,
        defaults,
        json_path,
        output_columns,
        scales,
    })
}

fn extract_yaml_value(line: &str) -> Option<&str> {
    let (_, val) = line.split_once(':')?;
    let val = val.trim().trim_matches('"').trim_matches('\'');
    if val.is_empty() {
        None
    } else {
        Some(val)
    }
}

// ---------------------------------------------------------------------------
// Source parsing
// ---------------------------------------------------------------------------

type Row = Vec<(String, String)>;

fn parse_json_source(data: &str, recipe: &TransformRecipe) -> Result<Vec<Row>, String> {
    // Simple JSON array parsing via serde_json
    let parsed: serde_json::Value =
        serde_json::from_str(data).map_err(|e| format!("JSON parse error: {e}"))?;

    // Navigate json_path if provided (simplified — just $.key or $.key[*])
    let array = if let Some(jp) = &recipe.json_path {
        navigate_json_path(&parsed, jp)?
    } else if parsed.is_array() {
        parsed.as_array().ok_or("expected JSON array")?.clone()
    } else if parsed.is_object() {
        // Try to find an array in the top-level object
        let obj = parsed.as_object().ok_or("expected JSON object")?;
        let mut found = None;
        for (_, v) in obj {
            if v.is_array() {
                found = Some(v.as_array().unwrap().clone());
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

fn parse_csv_source(data: &str, _recipe: &TransformRecipe) -> Result<Vec<Row>, String> {
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
// Transform application
// ---------------------------------------------------------------------------

fn apply_transforms(source_rows: &[Row], recipe: &TransformRecipe) -> Vec<Row> {
    let mut output_rows = Vec::new();

    for source_row in source_rows {
        let mut output_row: Row = Vec::new();

        // Apply defaults
        for (dim, val) in &recipe.defaults {
            output_row.push((dim.clone(), val.clone()));
        }

        // Apply column mappings
        for mapping in &recipe.mappings {
            if let Some((_, val)) = source_row.iter().find(|(k, _)| *k == mapping.source) {
                let mut val = val.clone();
                // Apply scale if any
                if let Some((_, scale)) = recipe.scales.iter().find(|(s, _)| *s == mapping.source) {
                    if let Ok(n) = val.parse::<f64>() {
                        val = format!("{}", n * scale);
                    }
                }
                output_row.push((mapping.target.clone(), val));
            }
        }

        output_rows.push(output_row);
    }

    output_rows
}

// ---------------------------------------------------------------------------
// Output formatting
// ---------------------------------------------------------------------------

fn format_output(rows: &[Row], recipe: &TransformRecipe, format: OutputFormat) -> String {
    match format {
        OutputFormat::Csv => format_csv_output(rows, recipe),
        OutputFormat::Json => format_json_output(rows, recipe),
        OutputFormat::Text => format_text_output(rows, recipe),
    }
}

fn format_csv_output(rows: &[Row], recipe: &TransformRecipe) -> String {
    let mut out = String::new();
    // Header
    let cols = &recipe.output_columns;
    if cols.is_empty() && !rows.is_empty() {
        // Derive from first row
        let first_row_cols: Vec<&str> = rows[0].iter().map(|(k, _)| k.as_str()).collect();
        out.push_str(&first_row_cols.join(","));
    } else {
        out.push_str(&cols.join(","));
    }
    out.push('\n');

    for row in rows {
        let cols_to_use: Vec<&str> = if cols.is_empty() {
            row.iter().map(|(k, _)| k.as_str()).collect()
        } else {
            cols.iter().map(|s| s.as_str()).collect()
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

fn format_json_output(rows: &[Row], _recipe: &TransformRecipe) -> String {
    let mut out = String::from("[\n");
    for (i, row) in rows.iter().enumerate() {
        out.push_str("  {");
        for (j, (k, v)) in row.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            push_json_str(&mut out, k);
            out.push(':');
            // Try to emit as number if possible
            if let Ok(n) = v.parse::<f64>() {
                let _ = write!(out, "{n}");
            } else if v.is_empty() {
                out.push_str("null");
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
    out.push_str("]\n");
    out
}

fn format_text_output(rows: &[Row], _recipe: &TransformRecipe) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Transform: {} rows", rows.len());
    // Show first few rows in table format
    if let Some(first) = rows.first() {
        let cols: Vec<&str> = first.iter().map(|(k, _)| k.as_str()).collect();
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
    }
    out
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn expand_env_vars(s: &str) -> String {
    let mut result = s.to_string();
    // Expand ${VAR_NAME} patterns
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
    // Also expand $VAR_NAME patterns (without braces)
    while let Some(start) = result.find('$') {
        if start + 1 < result.len() && result.as_bytes()[start + 1] == b'{' {
            break; // Already handled above
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
