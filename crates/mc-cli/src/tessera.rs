//! `mc tessera *` subcommand implementations.
//!
//! Phase 5A Stream D verbs:
//!
//! - `mc tessera apply <recipe.yaml> [--format text|json]`
//! - `mc tessera dry-run <recipe.yaml> [--format text|json]`
//! - `mc tessera history <model_dir> [--format text|json]`
//! - `mc tessera rollback <import_id> --model-dir <path> [--format text|json]`
//! - `mc tessera audit <model_dir> [--format text|json]`
//!
//! `--format json` emits the Phase 3B `schema_version: "1.0"` envelope
//! shape for diagnostic output and a structured JSON object for the
//! result of `apply` / `dry-run` / `history` / `audit`.

use std::path::{Path, PathBuf};

use mc_recipe::{diagnostics_to_json, sort_diagnostics, Diagnostic};
use mc_tessera::{Tessera, TesseraError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Text,
    Json,
}

#[derive(Debug)]
pub enum Command {
    Apply {
        recipe: String,
        format: Format,
    },
    DryRun {
        recipe: String,
        format: Format,
    },
    History {
        model_dir: String,
        format: Format,
    },
    Rollback {
        import_id: String,
        model_dir: String,
        format: Format,
    },
    Audit {
        model_dir: String,
        format: Format,
    },
}

pub fn parse(args: &[String]) -> Result<Command, String> {
    if args.is_empty() {
        return Err("`mc tessera` requires a verb (apply|dry-run|history|rollback|audit)".into());
    }
    let verb = args[0].as_str();
    let rest = &args[1..];

    match verb {
        "apply" | "dry-run" => {
            let (positional, format) = parse_positional_with_format(rest, 1)?;
            let recipe = positional[0].clone();
            Ok(if verb == "apply" {
                Command::Apply { recipe, format }
            } else {
                Command::DryRun { recipe, format }
            })
        }
        "history" | "audit" => {
            let (positional, format) = parse_positional_with_format(rest, 1)?;
            let model_dir = positional[0].clone();
            Ok(if verb == "history" {
                Command::History { model_dir, format }
            } else {
                Command::Audit { model_dir, format }
            })
        }
        "rollback" => {
            // Form: `mc tessera rollback <import_id> --model-dir <path>
            //        [--format text|json]`
            let mut positional: Vec<String> = Vec::new();
            let mut format = Format::Text;
            let mut model_dir: Option<String> = None;
            let mut iter = rest.iter();
            while let Some(arg) = iter.next() {
                match arg.as_str() {
                    "--format" => match iter.next() {
                        Some(v) if v == "text" => format = Format::Text,
                        Some(v) if v == "json" => format = Format::Json,
                        Some(v) => {
                            return Err(format!("--format must be `text` or `json`, got {v:?}"));
                        }
                        None => return Err("--format requires an argument".into()),
                    },
                    "--model-dir" => match iter.next() {
                        Some(v) => model_dir = Some(v.clone()),
                        None => return Err("--model-dir requires an argument".into()),
                    },
                    other if !other.starts_with("--") => positional.push(other.to_string()),
                    other => return Err(format!("unknown argument: {other:?}")),
                }
            }
            if positional.len() != 1 {
                return Err(
                    "`mc tessera rollback <import_id> --model-dir <path>` needs exactly one import_id"
                        .into(),
                );
            }
            let model_dir = model_dir
                .ok_or_else(|| "`mc tessera rollback` requires --model-dir".to_string())?;
            Ok(Command::Rollback {
                import_id: positional.remove(0),
                model_dir,
                format,
            })
        }
        other => Err(format!("unknown tessera verb: {other:?}")),
    }
}

fn parse_positional_with_format(
    args: &[String],
    expected_positional: usize,
) -> Result<(Vec<String>, Format), String> {
    let mut positional: Vec<String> = Vec::new();
    let mut format = Format::Text;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--format" => match iter.next() {
                Some(v) if v == "text" => format = Format::Text,
                Some(v) if v == "json" => format = Format::Json,
                Some(v) => return Err(format!("--format must be `text` or `json`, got {v:?}")),
                None => return Err("--format requires an argument".into()),
            },
            other if !other.starts_with("--") => positional.push(other.to_string()),
            other => return Err(format!("unknown argument: {other:?}")),
        }
    }
    if positional.len() != expected_positional {
        return Err(format!(
            "expected {expected_positional} positional argument(s), got {}",
            positional.len()
        ));
    }
    Ok((positional, format))
}

/// Run the parsed command. Returns the process exit code.
pub fn run(cmd: Command) -> i32 {
    match cmd {
        Command::Apply { recipe, format } => run_apply(&recipe, format),
        Command::DryRun { recipe, format } => run_dry_run(&recipe, format),
        Command::History { model_dir, format } => run_history(&model_dir, format),
        Command::Rollback {
            import_id,
            model_dir,
            format,
        } => run_rollback(&import_id, &model_dir, format),
        Command::Audit { model_dir, format } => run_audit(&model_dir, format),
    }
}

fn run_apply(recipe_path: &str, format: Format) -> i32 {
    let recipe_path = Path::new(recipe_path);
    let prepared = match Tessera::prepare(recipe_path) {
        Ok(p) => p,
        Err(e) => return emit_error(&e, format),
    };
    let report = match Tessera::apply(prepared) {
        Ok(r) => r,
        Err(e) => return emit_error(&e, format),
    };
    match format {
        Format::Text => {
            println!("Tessera apply: {}", report.recipe_name);
            println!("  import_id      : {}", report.import_id);
            println!("  rows_written   : {}", report.rows_written);
            println!("  rows_failed    : {}", report.rows_failed);
            println!("  rows_processed : {}", report.rows_processed);
            println!("  snapshot_id    : {}", report.snapshot_id);
            println!(
                "  revision       : {} → {}",
                report.revision_before, report.revision_after
            );
            println!(
                "  dirty (cumul/  : {} / {}",
                report.dirty_count_after, report.newly_dirtied_count
            );
            println!("       newly)");
            println!(
                "  timing (ms)    : fetch={} transform={} validate={} commit={} total={}",
                report.timing.fetch_ms,
                report.timing.transform_ms,
                report.timing.validate_ms,
                report.timing.commit_ms,
                report.timing.total_ms,
            );
            println!("  audit_path     : {}", report.audit_path.display());
        }
        Format::Json => match serde_json::to_string_pretty(&report) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("internal: report JSON encoding failed: {e}");
                return 2;
            }
        },
    }
    0
}

fn run_dry_run(recipe_path: &str, format: Format) -> i32 {
    let recipe_path = Path::new(recipe_path);
    let prepared = match Tessera::prepare(recipe_path) {
        Ok(p) => p,
        Err(e) => return emit_error(&e, format),
    };
    let report = match Tessera::dry_run(&prepared) {
        Ok(r) => r,
        Err(e) => return emit_error(&e, format),
    };
    match format {
        Format::Text => {
            println!("Tessera dry-run: {}", report.recipe_name);
            println!("  model            : {}", report.model_path);
            println!("  mapped columns   : {}", report.mapped_columns);
            println!("  default dims     : {}", report.default_dimensions);
            println!("  driver columns   : {:?}", report.driver_columns);
            println!("  effective batch  : {}", report.batch_size);
            if report.diagnostics.is_empty() {
                println!("  diagnostics      : none");
            } else {
                println!("  diagnostics      : {}", report.diagnostics.len());
                for d in &report.diagnostics {
                    println!("    - {d}");
                }
            }
        }
        Format::Json => match serde_json::to_string_pretty(&report) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("internal: report JSON encoding failed: {e}");
                return 2;
            }
        },
    }
    0
}

fn run_history(model_dir: &str, format: Format) -> i32 {
    let dir = PathBuf::from(model_dir);
    let history = match Tessera::history(&dir) {
        Ok(h) => h,
        Err(e) => return emit_error(&e, format),
    };
    match format {
        Format::Text => {
            if history.is_empty() {
                println!("(no imports recorded)");
                return 0;
            }
            println!(
                "{:<28}  {:<8}  {:<24}  {:<10}  {:<8}",
                "import_id", "event", "timestamp", "rows", "failed"
            );
            for h in &history {
                println!(
                    "{:<28}  {:<8}  {:<24}  {:<10}  {:<8}",
                    truncate(&h.import_id, 28),
                    h.event,
                    h.timestamp,
                    h.rows_written,
                    h.rows_failed,
                );
            }
        }
        Format::Json => match serde_json::to_string_pretty(&history) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("internal: history JSON encoding failed: {e}");
                return 2;
            }
        },
    }
    0
}

fn run_audit(model_dir: &str, format: Format) -> i32 {
    let dir = PathBuf::from(model_dir);
    let history = match Tessera::history(&dir) {
        Ok(h) => h,
        Err(e) => return emit_error(&e, format),
    };
    match format {
        Format::Text => {
            if history.is_empty() {
                println!("(no imports recorded)");
                return 0;
            }
            for (i, rec) in history.iter().enumerate() {
                println!("--- record {i} ---");
                println!("  import_id          : {}", rec.import_id);
                println!("  event              : {}", rec.event);
                println!("  recipe_name        : {}", rec.recipe_name);
                println!("  recipe_path        : {}", rec.recipe_path);
                println!("  model_path         : {}", rec.model_path);
                println!("  source_summary     : {}", rec.source_summary);
                println!("  timestamp          : {}", rec.timestamp);
                println!("  rows_written       : {}", rec.rows_written);
                println!("  rows_failed        : {}", rec.rows_failed);
                println!("  snapshot_id        : {}", rec.snapshot_id);
                println!(
                    "  revision           : {} → {}",
                    rec.revision_before, rec.revision_after
                );
                println!("  dirty_count_after  : {}", rec.dirty_count_after);
                println!("  newly_dirtied_count: {}", rec.newly_dirtied_count);
            }
        }
        Format::Json => match serde_json::to_string_pretty(&history) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("internal: audit JSON encoding failed: {e}");
                return 2;
            }
        },
    }
    0
}

fn run_rollback(import_id: &str, model_dir: &str, format: Format) -> i32 {
    let dir = PathBuf::from(model_dir);
    match Tessera::rollback(&dir, import_id) {
        Ok(()) => {
            match format {
                Format::Text => {
                    println!("rolled back import {import_id}");
                }
                Format::Json => {
                    let body = serde_json::json!({
                        "rolled_back": import_id,
                        "model_dir": model_dir,
                    });
                    match serde_json::to_string_pretty(&body) {
                        Ok(s) => println!("{s}"),
                        Err(e) => {
                            eprintln!("internal: rollback JSON encoding failed: {e}");
                            return 2;
                        }
                    }
                }
            }
            0
        }
        Err(e) => emit_error(&e, format),
    }
}

fn emit_error(err: &TesseraError, format: Format) -> i32 {
    let mut diags: Vec<Diagnostic> = err.recipe_diagnostics();
    if let Some(secret_diag) = err.secret_diagnostic() {
        diags.push(secret_diag);
    }
    if let TesseraError::Driver(d) = err {
        diags.push(TesseraError::driver_diagnostic("/source", d));
    }

    if !diags.is_empty() {
        sort_diagnostics(&mut diags);
        match format {
            Format::Text => {
                eprintln!("{} diagnostic(s):", diags.len());
                for d in &diags {
                    eprintln!("  [{}] {}: {}", d.code, d.severity.label(), d.message);
                }
            }
            Format::Json => {
                println!("{}", diagnostics_to_json(&diags));
            }
        }
    } else {
        match format {
            Format::Text => eprintln!("error: {err}"),
            Format::Json => {
                let body = serde_json::json!({
                    "schema_version": "1.0",
                    "diagnostics": [{
                        "code": "MC5xxx",
                        "severity": "error",
                        "path": "/",
                        "message": err.to_string(),
                    }],
                });
                if let Ok(s) = serde_json::to_string_pretty(&body) {
                    println!("{s}");
                }
            }
        }
    }
    2
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n.saturating_sub(1)])
    }
}
