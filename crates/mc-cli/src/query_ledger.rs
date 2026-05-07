//! `mc model query-ledger` — query the interpretation ledger.
//!
//! Phase 7A.2 Session 2: reads `.mosaic/analysis-ledger.jsonl` and applies
//! filters (severity, template, since, scope, repeated). Outputs filtered
//! entries as JSON or text.

use mc_narrative::ledger::{self, LedgerEntry, LedgerQuery};
use std::path::Path;

// ---------------------------------------------------------------------------
// Command shape
// ---------------------------------------------------------------------------

pub struct QueryLedgerCommand {
    /// Directory containing `.mosaic/analysis-ledger.jsonl`.
    pub model_dir: String,
    /// Filter by severity (info|success|warning|critical).
    pub severity: Option<String>,
    /// Filter by template_id.
    pub template_id: Option<String>,
    /// Filter by report_period >= since.
    pub since: Option<String>,
    /// Filter by scope key=value pairs.
    pub scope_filters: Vec<(String, String)>,
    /// Find entries where the same template fired in N+ consecutive periods.
    pub repeated: Option<usize>,
    /// Output format.
    pub format: QueryLedgerFormat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryLedgerFormat {
    Json,
    Text,
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

pub fn parse(args: &[String]) -> Result<QueryLedgerCommand, String> {
    if args.is_empty() {
        return Err("`mc model query-ledger` requires a model directory path".into());
    }

    let mut model_dir: Option<String> = None;
    let mut severity: Option<String> = None;
    let mut template_id: Option<String> = None;
    let mut since: Option<String> = None;
    let mut scope_filters: Vec<(String, String)> = Vec::new();
    let mut repeated: Option<usize> = None;
    let mut format = QueryLedgerFormat::Text;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--severity" => match iter.next() {
                Some(v) => {
                    let valid = ["info", "success", "warning", "critical"];
                    if !valid.contains(&v.as_str()) {
                        return Err(format!("--severity must be one of {valid:?}; got {v:?}"));
                    }
                    severity = Some(v.clone());
                }
                None => return Err("--severity requires an argument".into()),
            },
            "--template" => match iter.next() {
                Some(v) => template_id = Some(v.clone()),
                None => return Err("--template requires an argument".into()),
            },
            "--since" => match iter.next() {
                Some(v) => since = Some(v.clone()),
                None => return Err("--since requires a period argument (e.g., 2026-01)".into()),
            },
            "--scope" => match iter.next() {
                Some(v) => {
                    let parts: Vec<&str> = v.splitn(2, '=').collect();
                    if parts.len() != 2 {
                        return Err(format!("--scope must be key=value; got {v:?}"));
                    }
                    scope_filters.push((parts[0].to_string(), parts[1].to_string()));
                }
                None => return Err("--scope requires a key=value argument".into()),
            },
            "--repeated" => match iter.next() {
                Some(v) => {
                    let n: usize = v.parse().map_err(|_| {
                        format!("--repeated requires a positive integer; got {v:?}")
                    })?;
                    if n == 0 {
                        return Err("--repeated requires a positive integer".into());
                    }
                    repeated = Some(n);
                }
                None => return Err("--repeated requires a number".into()),
            },
            "--format" => match iter.next() {
                Some(v) if v == "json" => format = QueryLedgerFormat::Json,
                Some(v) if v == "text" => format = QueryLedgerFormat::Text,
                Some(v) => return Err(format!("--format must be text or json; got {v:?}")),
                None => return Err("--format requires an argument".into()),
            },
            other if !other.starts_with("--") && model_dir.is_none() => {
                model_dir = Some(other.to_string());
            }
            other => return Err(format!("unknown argument: {other:?}")),
        }
    }

    let model_dir = model_dir.ok_or("`mc model query-ledger` requires a model directory path")?;

    Ok(QueryLedgerCommand {
        model_dir,
        severity,
        template_id,
        since,
        scope_filters,
        repeated,
        format,
    })
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

pub fn run(cmd: QueryLedgerCommand) -> i32 {
    let (code, output) = run_captured(cmd);
    if !output.is_empty() {
        print!("{output}");
    }
    code
}

pub fn run_captured(cmd: QueryLedgerCommand) -> (i32, String) {
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

    let query = LedgerQuery {
        severity: cmd.severity,
        template_id: cmd.template_id,
        since: cmd.since,
        scope_filters: cmd.scope_filters,
        repeated: cmd.repeated,
    };

    let filtered = ledger::query_ledger(&entries, &query);

    let output = match cmd.format {
        QueryLedgerFormat::Json => render_json(&filtered),
        QueryLedgerFormat::Text => render_text(&filtered),
    };

    (0, output)
}

// ---------------------------------------------------------------------------
// Output rendering
// ---------------------------------------------------------------------------

fn render_json(entries: &[LedgerEntry]) -> String {
    let mut out = String::new();
    out.push_str("{\n  \"schema_version\": \"1.0\",\n  \"count\": ");
    out.push_str(&entries.len().to_string());
    out.push_str(",\n  \"entries\": [");
    if entries.is_empty() {
        out.push_str("]\n}\n");
        return out;
    }
    out.push('\n');
    for (i, entry) in entries.iter().enumerate() {
        // Use serde_json for each entry to keep it simple and correct.
        let json = serde_json::to_string(entry).unwrap_or_else(|_| "{}".to_string());
        out.push_str("    ");
        out.push_str(&json);
        if i + 1 < entries.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]\n}\n");
    out
}

fn render_text(entries: &[LedgerEntry]) -> String {
    let mut out = String::new();
    if entries.is_empty() {
        out.push_str("No matching ledger entries.\n");
        return out;
    }
    for entry in entries {
        let severity_badge = match entry.narrative.severity.as_str() {
            "critical" => "[CRITICAL]",
            "warning" => "[WARNING] ",
            "success" => "[SUCCESS] ",
            "info" => "[INFO]    ",
            _ => "[NOTE]    ",
        };
        let period = entry.report_period.as_deref().unwrap_or("(no period)");
        out.push_str(&format!(
            "{severity_badge} {period} | {} | {}\n",
            entry.narrative.template_id, entry.narrative.text
        ));
    }
    out.push_str(&format!("\n{} entries.\n", entries.len()));
    out
}
