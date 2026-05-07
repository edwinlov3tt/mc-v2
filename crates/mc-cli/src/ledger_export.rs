//! `mc model ledger-export` — export the interpretation ledger.
//!
//! Phase 7A.2 Session 3: exports `.mosaic/analysis-ledger.jsonl` as
//! raw JSONL or flattened CSV.

use mc_narrative::ledger::{self, LedgerEntry};
use std::path::Path;

// ---------------------------------------------------------------------------
// Command shape
// ---------------------------------------------------------------------------

pub struct LedgerExportCommand {
    /// Directory containing `.mosaic/analysis-ledger.jsonl`.
    pub model_dir: String,
    /// Export format: JSONL or CSV.
    pub format: ExportFormat,
    /// Optional `--since` filter.
    pub since: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Jsonl,
    Csv,
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

pub fn parse(args: &[String]) -> Result<LedgerExportCommand, String> {
    if args.is_empty() {
        return Err("`mc model ledger-export` requires a model directory path".into());
    }

    let mut model_dir: Option<String> = None;
    let mut format = ExportFormat::Jsonl;
    let mut since: Option<String> = None;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--format" => match iter.next() {
                Some(v) if v == "jsonl" => format = ExportFormat::Jsonl,
                Some(v) if v == "csv" => format = ExportFormat::Csv,
                Some(v) => return Err(format!("--format must be jsonl or csv; got {v:?}")),
                None => return Err("--format requires an argument".into()),
            },
            "--since" => match iter.next() {
                Some(v) => since = Some(v.clone()),
                None => return Err("--since requires a period argument".into()),
            },
            other if !other.starts_with("--") && model_dir.is_none() => {
                model_dir = Some(other.to_string());
            }
            other => return Err(format!("unknown argument: {other:?}")),
        }
    }

    let model_dir = model_dir.ok_or("`mc model ledger-export` requires a model directory path")?;

    Ok(LedgerExportCommand {
        model_dir,
        format,
        since,
    })
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

pub fn run(cmd: LedgerExportCommand) -> i32 {
    let (code, output) = run_captured(cmd);
    if !output.is_empty() {
        print!("{output}");
    }
    code
}

pub fn run_captured(cmd: LedgerExportCommand) -> (i32, String) {
    let model_dir = Path::new(&cmd.model_dir);
    let ledger_file = ledger::ledger_path(model_dir);

    if !ledger_file.exists() {
        eprintln!("error: no ledger found at {}", ledger_file.display());
        eprintln!("hint: run `mc model narrate <model> --save-ledger` first");
        return (1, String::new());
    }

    let entries = match ledger::read_ledger(&ledger_file) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: {e}");
            return (1, String::new());
        }
    };

    // Apply --since filter if present.
    let filtered: Vec<&LedgerEntry> = entries
        .iter()
        .filter(|e| {
            if let Some(ref since) = cmd.since {
                match &e.report_period {
                    Some(period) => period.as_str() >= since.as_str(),
                    None => false,
                }
            } else {
                true
            }
        })
        .collect();

    let output = match cmd.format {
        ExportFormat::Jsonl => render_jsonl(&filtered),
        ExportFormat::Csv => render_csv(&filtered),
    };

    (0, output)
}

// ---------------------------------------------------------------------------
// Output rendering
// ---------------------------------------------------------------------------

fn render_jsonl(entries: &[&LedgerEntry]) -> String {
    let mut out = String::new();
    for entry in entries {
        let json = serde_json::to_string(entry).unwrap_or_else(|_| "{}".to_string());
        out.push_str(&json);
        out.push('\n');
    }
    out
}

fn render_csv(entries: &[&LedgerEntry]) -> String {
    let mut out = String::new();

    // Header row.
    out.push_str(
        "ledger_entry_id,generated_at,model,model_hash,report_period,\
         severity,template_id,text,notability_score\n",
    );

    for entry in entries {
        out.push_str(&csv_field(&entry.ledger_entry_id));
        out.push(',');
        out.push_str(&csv_field(&entry.generated_at));
        out.push(',');
        out.push_str(&csv_field(&entry.model));
        out.push(',');
        out.push_str(&csv_field(&entry.model_hash));
        out.push(',');
        out.push_str(&csv_field(entry.report_period.as_deref().unwrap_or("")));
        out.push(',');
        out.push_str(&csv_field(&entry.narrative.severity));
        out.push(',');
        out.push_str(&csv_field(&entry.narrative.template_id));
        out.push(',');
        out.push_str(&csv_field(&entry.narrative.text));
        out.push(',');
        if let Some(s) = entry.narrative.notability_score {
            out.push_str(&format!("{s}"));
        }
        out.push('\n');
    }

    out
}

/// Escape a CSV field: if it contains commas, quotes, or newlines,
/// wrap in double quotes and double any internal quotes.
fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        let escaped = s.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}
