//! `mc model narrate-trends` — cross-period trend analysis from ledger data.
//!
//! Phase 7A.3 Session 2: loads a model, evaluates BOTH regular and trend
//! templates with the interpretation ledger, producing cross-period narratives.
//!
//! This verb is "what's been happening OVER TIME" vs `narrate` which is
//! "what's happening NOW."

use crate::loader::load_model;
use crate::narrate::{render_narratives, NarrateFormat};
use mc_narrative::ledger;
use std::path::Path;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub struct NarrateTrendsCommand {
    pub path: String,
    pub format: NarrateFormat,
    pub templates_dir: Option<String>,
    /// Load a specific ledger file instead of auto-discovering.
    pub mock_ledger: Option<String>,
    /// Only show trend template outputs (not regular templates).
    pub trends_only: bool,
}

pub fn parse(args: &[String]) -> Result<NarrateTrendsCommand, String> {
    if args.is_empty() {
        return Err("`mc model narrate-trends` requires a YAML model path".into());
    }
    let mut path: Option<String> = None;
    let mut format = NarrateFormat::Text;
    let mut templates_dir: Option<String> = None;
    let mut mock_ledger: Option<String> = None;
    let mut trends_only = false;

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
            "--mock-ledger" => match iter.next() {
                Some(p) => mock_ledger = Some(p.clone()),
                None => return Err("--mock-ledger requires a file path".into()),
            },
            "--trends-only" => trends_only = true,
            other if !other.starts_with("--") && path.is_none() => {
                path = Some(other.to_string());
            }
            other => return Err(format!("unknown argument: {other:?}")),
        }
    }
    let path = path.ok_or("`mc model narrate-trends` requires a YAML model path")?;
    Ok(NarrateTrendsCommand {
        path,
        format,
        templates_dir,
        mock_ledger,
        trends_only,
    })
}

pub fn run(cmd: NarrateTrendsCommand) -> i32 {
    // 1. Load and populate the cube.
    let loaded = match load_model(&cmd.path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: {e}");
            return e.exit_code();
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

    // 3. Load ALL templates (regular + trend).
    let templates = mc_narrative::load_templates(&templates_dir);
    if templates.is_empty() {
        eprintln!(
            "warning: no narrative templates found in {templates_dir:?}; \
             use --templates <dir> to specify"
        );
        return 0;
    }

    // 4. Load ledger.
    let ledger_entries = load_ledger(&cmd.path, cmd.mock_ledger.as_deref());

    if ledger_entries.is_empty() {
        eprintln!(
            "info: no ledger data available — trend templates will not fire. \
             Run `mc model narrate --save-ledger` first to build history."
        );
    }

    // 5. Convert populated Cube → CubeData.
    let cube_data = crate::narrate::cube_to_cube_data(&mut cube, &refs, principal);

    if cube_data.is_empty() {
        eprintln!("warning: no cube data to evaluate narratives against");
        return 0;
    }

    // 6. Evaluate templates with ledger context.
    let ledger_slice = if ledger_entries.is_empty() {
        None
    } else {
        Some(ledger_entries.as_slice())
    };
    let narratives = mc_narrative::evaluate_all(&templates, &cube_data, ledger_slice, None);

    // 7. Optionally filter to trend templates only.
    let narratives = if cmd.trends_only {
        let trend_ids: std::collections::HashSet<&str> = templates
            .iter()
            .filter(|t| t.family.iter().any(|f| f == "trend"))
            .map(|t| t.id.as_str())
            .collect();
        narratives
            .into_iter()
            .filter(|n| trend_ids.contains(n.template_id.as_str()))
            .collect()
    } else {
        narratives
    };

    // 8. Render output.
    let output = render_narratives(&narratives, cmd.format);
    if !output.is_empty() {
        print!("{output}");
    }

    0
}

// ---------------------------------------------------------------------------
// Ledger loading
// ---------------------------------------------------------------------------

/// Load ledger entries from auto-discovered path or --mock-ledger override.
fn load_ledger(model_path: &str, mock_ledger: Option<&str>) -> Vec<mc_narrative::LedgerEntry> {
    let path = match mock_ledger {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            let model_file = Path::new(model_path);
            let model_dir = model_file.parent().unwrap_or(Path::new("."));
            ledger::ledger_path(model_dir)
        }
    };

    match ledger::read_ledger(&path) {
        Ok(entries) => {
            if !entries.is_empty() {
                eprintln!(
                    "[ledger] Loaded {} entries from {}",
                    entries.len(),
                    path.display()
                );
            }
            entries
        }
        Err(e) => {
            eprintln!("warning: could not read ledger: {e}");
            Vec::new()
        }
    }
}

/// Auto-discover the narratives directory relative to the model file.
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

    candidates[0].to_string_lossy().into_owned()
}
