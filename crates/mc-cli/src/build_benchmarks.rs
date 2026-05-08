//! `mc model build-benchmarks` — build workspace-local benchmark library from ledger.
//!
//! Phase 7A.4 Session 1: reads `.mosaic/analysis-ledger.jsonl`, groups evidence
//! values by (metric, scope), computes percentile distributions, writes
//! `.mosaic/benchmark-library.json`.
//!
//! Per ADR-0021: data never leaves the workspace.

use mc_narrative::benchmark;
use mc_narrative::ledger;
use std::path::Path;

// ---------------------------------------------------------------------------
// Command
// ---------------------------------------------------------------------------

pub struct BuildBenchmarksCommand {
    pub path: String,
    pub since: Option<String>,
}

pub fn parse(args: &[String]) -> Result<BuildBenchmarksCommand, String> {
    if args.is_empty() {
        return Err("`mc model build-benchmarks` requires a model directory path".into());
    }
    let mut path: Option<String> = None;
    let mut since: Option<String> = None;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--since" => match iter.next() {
                Some(v) => since = Some(v.clone()),
                None => return Err("--since requires a period argument (e.g. 2025-11)".into()),
            },
            other if !other.starts_with("--") && path.is_none() => {
                path = Some(other.to_string());
            }
            other => return Err(format!("unknown argument: {other:?}")),
        }
    }
    let path = path.ok_or("`mc model build-benchmarks` requires a model directory path")?;
    Ok(BuildBenchmarksCommand { path, since })
}

pub fn run(cmd: BuildBenchmarksCommand) -> i32 {
    // Resolve model directory from the path (may be a model.yaml file or directory).
    let model_path = Path::new(&cmd.path);
    let model_dir = if model_path.is_dir() {
        model_path.to_path_buf()
    } else {
        model_path.parent().unwrap_or(Path::new(".")).to_path_buf()
    };

    // Workspace name: directory's file_name component.
    let workspace = model_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // 1. Read the ledger.
    let ledger_path = ledger::ledger_path(&model_dir);
    let entries = match ledger::read_ledger(&ledger_path) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: could not read ledger: {e}");
            return 1;
        }
    };

    if entries.is_empty() {
        eprintln!(
            "warning: no ledger entries found at {}. \
             Run `mc model narrate --save-ledger` first to build history.",
            ledger_path.display()
        );
    }

    // 2. Build the benchmark library.
    let lib = benchmark::build_benchmark_library(&entries, &workspace, cmd.since.as_deref());

    // MC7043: warn if fewer than 2 periods.
    if lib.period_count < 2 && lib.period_count > 0 {
        eprintln!(
            "[warn MC7043] Benchmark library built from only {} period — results may be unreliable",
            lib.period_count
        );
    }

    // 3. Write the library.
    match benchmark::write_benchmark_library(&model_dir, &lib) {
        Ok(path) => {
            // Print summary.
            eprintln!(
                "[benchmarks] Built from {} ledger entries across {} periods ({} → {})",
                entries.len(),
                lib.period_count,
                lib.period_range.from,
                lib.period_range.to,
            );

            for (key, bench) in &lib.benchmarks {
                eprintln!(
                    "[benchmarks] {}  p50={}  ({} samples)",
                    key,
                    format_value(bench.p50),
                    bench.sample_count,
                );
            }

            eprintln!("[benchmarks] Wrote {}", path.display());
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

/// Format a benchmark value for display (simple number formatting).
fn format_value(v: f64) -> String {
    if v.abs() >= 1000.0 {
        // Large numbers: comma-separated integer.
        let i = v.round() as i64;
        let s = i.to_string();
        let mut result = String::new();
        for (idx, c) in s.chars().rev().enumerate() {
            if idx > 0 && idx % 3 == 0 && c != '-' {
                result.push(',');
            }
            result.push(c);
        }
        result.chars().rev().collect()
    } else if v.abs() < 0.01 {
        format!("{v:.4}")
    } else {
        format!("{v:.2}")
    }
}
