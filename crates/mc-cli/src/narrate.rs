//! `mc model narrate` — generate narrative report from templates + populated cube.
//!
//! Phase 7A.1 Session 3: loads a model, populates the cube via canonical_inputs,
//! discovers narrative templates, and evaluates them against the cube data.
//!
//! Output formats: `json` (structured findings per the planning doc contract),
//! `text` (plain text with severity prefixes), `markdown`.
//!
//! Template auto-discovery: looks for `narratives/` directory relative to the
//! model file path. Override with `--templates <dir>`.

use crate::loader::load_model;
use mc_core::{DimensionKind, PrincipalId, ScalarValue};
use mc_narrative::ledger;
use mc_narrative::{CellEntry, CubeData, NarrativeOutput, Severity};
use std::collections::BTreeMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub struct NarrateCommand {
    pub path: String,
    pub format: NarrateFormat,
    pub templates_dir: Option<String>,
    /// Phase 7A.2: when true, write ledger entries to .mosaic/analysis-ledger.jsonl.
    pub save_ledger: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NarrateFormat {
    Json,
    Text,
    Markdown,
}

pub fn parse(args: &[String]) -> Result<NarrateCommand, String> {
    if args.is_empty() {
        return Err("`mc model narrate` requires a YAML model path".into());
    }
    let mut path: Option<String> = None;
    let mut format = NarrateFormat::Text;
    let mut templates_dir: Option<String> = None;
    let mut save_ledger = false;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--format" => match iter.next() {
                Some(v) if v == "text" => format = NarrateFormat::Text,
                Some(v) if v == "json" => format = NarrateFormat::Json,
                Some(v) if v == "markdown" || v == "md" => format = NarrateFormat::Markdown,
                Some(v) => {
                    return Err(format!(
                        "--format must be text, json, or markdown; got {v:?}"
                    ))
                }
                None => return Err("--format requires an argument".into()),
            },
            "--templates" => match iter.next() {
                Some(d) => templates_dir = Some(d.clone()),
                None => return Err("--templates requires a directory path".into()),
            },
            "--save-ledger" => save_ledger = true,
            other if !other.starts_with("--") && path.is_none() => {
                path = Some(other.to_string());
            }
            other => return Err(format!("unknown argument: {other:?}")),
        }
    }
    let path = path.ok_or("`mc model narrate` requires a YAML model path")?;
    Ok(NarrateCommand {
        path,
        format,
        templates_dir,
        save_ledger,
    })
}

pub fn run(cmd: NarrateCommand) -> i32 {
    let (code, output) = run_captured(cmd);
    if !output.is_empty() {
        print!("{output}");
    }
    code
}

pub fn run_captured(cmd: NarrateCommand) -> (i32, String) {
    // 1. Load and populate the cube.
    let loaded = match load_model(&cmd.path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: {e}");
            return (e.exit_code(), String::new());
        }
    };
    let mut cube = loaded.cube;
    let principal = loaded.root_principal;
    let refs = loaded.refs;

    // 2. Discover templates directory.
    let templates_dir = match &cmd.templates_dir {
        Some(d) => d.clone(),
        None => discover_templates_dir(&cmd.path),
    };

    // 3. Load templates.
    let templates = mc_narrative::load_templates(&templates_dir);
    if templates.is_empty() {
        eprintln!(
            "warning: no narrative templates found in {templates_dir:?}; \
             use --templates <dir> to specify"
        );
        return (0, render_empty(cmd.format));
    }

    // 4. Convert populated Cube → CubeData for the narrative engine.
    let cube_data = cube_to_cube_data(&mut cube, &refs, principal);

    if cube_data.is_empty() {
        eprintln!("warning: no cube data to evaluate narratives against");
        return (0, render_empty(cmd.format));
    }

    // 5. Load benchmark library if present (Phase 7A.4).
    let model_file = std::path::Path::new(&cmd.path);
    let model_dir = model_file.parent().unwrap_or(std::path::Path::new("."));
    let benchmark_lib = mc_narrative::benchmark::read_benchmark_library(model_dir).ok();

    // 6. Evaluate templates.
    let narratives =
        mc_narrative::evaluate_all(&templates, &cube_data, None, benchmark_lib.as_ref(), None);

    // 6. Phase 7A.2: write ledger entries if --save-ledger is set.
    if cmd.save_ledger && !narratives.is_empty() {
        write_ledger(&cmd.path, &narratives);
    }

    // 7. Render output.
    let output = match cmd.format {
        NarrateFormat::Json => render_json(&narratives),
        NarrateFormat::Text => render_text(&narratives),
        NarrateFormat::Markdown => render_markdown(&narratives),
    };

    (0, output)
}

// ---------------------------------------------------------------------------
// Ledger write (Phase 7A.2)
// ---------------------------------------------------------------------------

/// Write narrative outputs to the interpretation ledger.
///
/// Computes the model hash, generates a timestamp, converts narratives
/// to ledger entries, and appends them to `.mosaic/analysis-ledger.jsonl`.
fn write_ledger(model_path: &str, narratives: &[NarrativeOutput]) {
    let model_file = Path::new(model_path);
    let model_dir = model_file.parent().unwrap_or(Path::new("."));

    // Compute model hash.
    let model_hash = match ledger::compute_model_hash(model_file) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("warning: could not compute model hash: {e}");
            "sha256:unknown".to_string()
        }
    };

    // Generate timestamp (ISO-8601 UTC).
    let generated_at = utc_now_iso8601();

    // Build scope from model path (minimal in v1).
    let scope = BTreeMap::new();

    // Convert narratives to ledger entries.
    let entries = ledger::narratives_to_ledger_entries(
        narratives,
        model_path,
        &model_hash,
        &generated_at,
        None, // report_period — not available from CLI args yet
        &scope,
    );

    // Write to ledger.
    match ledger::write_ledger_entries(model_dir, &entries) {
        Ok(path) => {
            eprintln!(
                "[ledger] Wrote {} entries to {}",
                entries.len(),
                path.display()
            );
        }
        Err(e) => {
            eprintln!("warning: ledger write failed: {e}");
        }
    }
}

/// Get current UTC time as ISO-8601 string.
///
/// Uses UNIX timestamp to avoid pulling in chrono. Format: YYYY-MM-DDTHH:MM:SSZ.
fn utc_now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Convert UNIX seconds to date-time components.
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Convert days since epoch to Y-M-D (simplified leap year handling).
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Convert days since UNIX epoch to (year, month, day).
fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Algorithm from Howard Hinnant's date library.
    days += 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// ---------------------------------------------------------------------------
// Template directory discovery
// ---------------------------------------------------------------------------

/// Auto-discover the narratives directory relative to the model file.
///
/// Checks: `<model_dir>/narratives/`, `./narratives/`, `./demo/narratives/`.
fn discover_templates_dir(model_path: &str) -> String {
    let model_dir = std::path::Path::new(model_path).parent();

    let candidates: Vec<std::path::PathBuf> = {
        let mut v = Vec::new();
        if let Some(dir) = model_dir {
            v.push(dir.join("narratives"));
        }
        v.push(std::path::PathBuf::from("narratives"));
        v.push(std::path::PathBuf::from("demo/narratives"));
        v
    };

    for candidate in &candidates {
        if candidate.is_dir() {
            return candidate.to_string_lossy().into_owned();
        }
    }

    // Fallback: first candidate (will produce "no templates found" warning).
    candidates[0].to_string_lossy().into_owned()
}

// ---------------------------------------------------------------------------
// Cube → CubeData conversion
// ---------------------------------------------------------------------------

/// Convert a populated mc-core Cube into mc-narrative CubeData.
///
/// For each non-Scenario, non-Version, non-Measure dimension (the "category"
/// dimensions), builds a CubeData with measure values indexed by category element.
pub fn cube_to_cube_data(
    cube: &mut mc_core::Cube,
    _refs: &mc_model::ModelRefs,
    principal: PrincipalId,
) -> Vec<CubeData> {
    // Collect dimension metadata first (immutable borrow).
    let dims = cube.dimensions().to_vec();
    let measure_dim = cube.measure_dimension().clone();

    let scenario_id = dims
        .iter()
        .find(|d| d.kind == DimensionKind::Scenario)
        .and_then(|d| d.elements.first().map(|e| e.id));
    let version_id = dims
        .iter()
        .find(|d| d.kind == DimensionKind::Version)
        .and_then(|d| d.elements.first().map(|e| e.id));

    let (scenario_id, version_id) = match (scenario_id, version_id) {
        (Some(s), Some(v)) => (s, v),
        _ => return Vec::new(),
    };

    // Find category dimensions (not Scenario, Version, or Measure).
    let cat_dims: Vec<mc_core::Dimension> = dims
        .iter()
        .filter(|d| {
            d.kind != DimensionKind::Scenario
                && d.kind != DimensionKind::Version
                && d.kind != DimensionKind::Measure
        })
        .cloned()
        .collect();

    if cat_dims.is_empty() {
        return Vec::new();
    }

    let cube_id = cube.id;
    let all_dims = dims;

    // Build one CubeData per category dimension.
    let mut result = Vec::new();

    for cat_dim in &cat_dims {
        let leaf_elements: Vec<&mc_core::Element> = cat_dim
            .elements
            .iter()
            .filter(|e| {
                !cat_dim
                    .default_hierarchy()
                    .edges
                    .iter()
                    .any(|edge| edge.parent == e.id)
            })
            .collect();

        if leaf_elements.is_empty() {
            continue;
        }

        let mut values: BTreeMap<String, Vec<CellEntry>> = BTreeMap::new();

        for measure_elem in &measure_dim.elements {
            let measure_name = &measure_elem.name;
            let mut entries = Vec::new();

            for cat_elem in &leaf_elements {
                let coord = build_coord_from_dims(
                    cube_id,
                    &all_dims,
                    scenario_id,
                    version_id,
                    cat_dim.id,
                    cat_elem.id,
                    measure_elem.id,
                    &cat_dims,
                );

                if let Some(coord) = coord {
                    if let Ok(cell) = cube.read(&coord, principal) {
                        if let ScalarValue::F64(v) = cell.value {
                            if v.is_finite() {
                                entries.push(CellEntry {
                                    category: cat_elem.name.clone(),
                                    value: v,
                                });
                            }
                        }
                    }
                }
            }

            if !entries.is_empty() {
                values.insert(measure_name.clone(), entries);
            }
        }

        if !values.is_empty() {
            let table_name = format!("{} Performance", cat_dim.name);
            result.push(CubeData {
                table_name,
                subproduct: all_dims.first().map(|d| d.name.clone()).unwrap_or_default(),
                source_file: format!("{}.cube", cat_dim.name.to_lowercase()),
                dimension_name: Some(cat_dim.name.clone()),
                values,
            });
        }
    }

    result
}

/// Build a cell coordinate from pre-collected dimension metadata.
#[allow(clippy::too_many_arguments)]
fn build_coord_from_dims(
    cube_id: mc_core::CubeId,
    all_dims: &[mc_core::Dimension],
    scenario_id: mc_core::ElementId,
    version_id: mc_core::ElementId,
    target_dim_id: mc_core::DimensionId,
    target_elem_id: mc_core::ElementId,
    measure_id: mc_core::ElementId,
    cat_dims: &[mc_core::Dimension],
) -> Option<mc_core::CellCoordinate> {
    let mut slots = Vec::with_capacity(all_dims.len());

    for dim in all_dims {
        if dim.kind == DimensionKind::Scenario {
            slots.push(scenario_id);
        } else if dim.kind == DimensionKind::Version {
            slots.push(version_id);
        } else if dim.kind == DimensionKind::Measure {
            slots.push(measure_id);
        } else if dim.id == target_dim_id {
            slots.push(target_elem_id);
        } else if cat_dims.iter().any(|cd| cd.id == dim.id) {
            // For non-target category dims, use the first leaf element.
            let first_leaf = dim
                .elements
                .iter()
                .find(|e| {
                    !dim.default_hierarchy()
                        .edges
                        .iter()
                        .any(|edge| edge.parent == e.id)
                })
                .map(|e| e.id);
            match first_leaf {
                Some(id) => slots.push(id),
                None => return None,
            }
        } else {
            match dim.elements.first() {
                Some(e) => slots.push(e.id),
                None => return None,
            }
        }
    }

    Some(mc_core::CellCoordinate::from_parts(cube_id, slots))
}

// ---------------------------------------------------------------------------
// Output rendering
// ---------------------------------------------------------------------------

/// Render narratives in the specified format (shared with narrate-trends).
pub fn render_narratives(narratives: &[NarrativeOutput], format: NarrateFormat) -> String {
    if narratives.is_empty() {
        return render_empty(format);
    }
    match format {
        NarrateFormat::Json => render_json(narratives),
        NarrateFormat::Text => render_text(narratives),
        NarrateFormat::Markdown => render_markdown(narratives),
    }
}

fn render_empty(format: NarrateFormat) -> String {
    match format {
        NarrateFormat::Json => "{\"schema_version\": \"1.0\", \"narratives\": []}\n".into(),
        NarrateFormat::Text | NarrateFormat::Markdown => String::new(),
    }
}

fn render_json(narratives: &[NarrativeOutput]) -> String {
    let mut out = String::new();
    out.push_str("{\n  \"schema_version\": \"1.0\",\n  \"narratives\": [");
    if narratives.is_empty() {
        out.push_str("]\n}\n");
        return out;
    }
    out.push('\n');
    for (i, n) in narratives.iter().enumerate() {
        out.push_str("    {\n");
        out.push_str(&format!("      \"id\": {},\n", json_str(&n.id)));
        out.push_str(&format!(
            "      \"template_id\": {},\n",
            json_str(&n.template_id)
        ));
        out.push_str(&format!(
            "      \"severity\": {},\n",
            json_str(severity_str(n.severity))
        ));
        out.push_str(&format!("      \"text\": {},\n", json_str(&n.text)));
        out.push_str("      \"evidence\": {");
        if n.evidence.is_empty() {
            out.push('}');
        } else {
            out.push('\n');
            let ev_count = n.evidence.len();
            for (j, (k, v)) in n.evidence.iter().enumerate() {
                out.push_str(&format!("        {}: {v}", json_str(k)));
                if j + 1 < ev_count {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str("      }");
        }
        out.push('\n');
        out.push_str("    }");
        if i + 1 < narratives.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]\n}\n");
    out
}

fn render_text(narratives: &[NarrativeOutput]) -> String {
    let mut out = String::new();
    for n in narratives {
        let prefix = match n.severity {
            Severity::Critical => "[CRITICAL] ",
            Severity::Warning => "[WARNING]  ",
            Severity::Info => "[INFO]     ",
            Severity::Success => "[SUCCESS]  ",
            _ => "[NOTE]     ",
        };
        out.push_str(prefix);
        out.push_str(&n.text);
        out.push('\n');
    }
    out
}

fn render_markdown(narratives: &[NarrativeOutput]) -> String {
    let mut out = String::new();
    out.push_str("# Narrative Report\n\n");

    // Group by severity.
    let critical: Vec<&NarrativeOutput> = narratives
        .iter()
        .filter(|n| matches!(n.severity, Severity::Critical))
        .collect();
    let warnings: Vec<&NarrativeOutput> = narratives
        .iter()
        .filter(|n| matches!(n.severity, Severity::Warning))
        .collect();
    let info: Vec<&NarrativeOutput> = narratives
        .iter()
        .filter(|n| matches!(n.severity, Severity::Info | Severity::Success))
        .collect();

    if !critical.is_empty() {
        out.push_str("## Critical\n\n");
        for n in &critical {
            out.push_str(&format!("- **{}**: {}\n", n.template_id, n.text));
        }
        out.push('\n');
    }
    if !warnings.is_empty() {
        out.push_str("## Warnings\n\n");
        for n in &warnings {
            out.push_str(&format!("- **{}**: {}\n", n.template_id, n.text));
        }
        out.push('\n');
    }
    if !info.is_empty() {
        out.push_str("## Insights\n\n");
        for n in &info {
            out.push_str(&format!("- {}\n", n.text));
        }
        out.push('\n');
    }

    out
}

fn severity_str(s: Severity) -> &'static str {
    match s {
        Severity::Info => "info",
        Severity::Success => "success",
        Severity::Warning => "warning",
        Severity::Critical => "critical",
        _ => "info",
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
