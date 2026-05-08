//! `mc model show-benchmarks` — display the workspace benchmark library.
//!
//! Phase 7A.4 Session 2: reads `.mosaic/benchmark-library.json` and displays
//! it in text or JSON format. Does NOT rebuild — just reads the existing library.

use mc_narrative::benchmark;
use std::path::Path;

// ---------------------------------------------------------------------------
// Command
// ---------------------------------------------------------------------------

pub struct ShowBenchmarksCommand {
    pub path: String,
    pub metric: Option<String>,
    pub format: OutputFormat,
}

#[derive(Clone, Copy)]
pub enum OutputFormat {
    Text,
    Json,
}

pub fn parse(args: &[String]) -> Result<ShowBenchmarksCommand, String> {
    if args.is_empty() {
        return Err("`mc model show-benchmarks` requires a model directory path".into());
    }
    let mut path: Option<String> = None;
    let mut metric: Option<String> = None;
    let mut format = OutputFormat::Text;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--metric" => match iter.next() {
                Some(v) => metric = Some(v.clone()),
                None => return Err("--metric requires a metric name".into()),
            },
            "--format" => match iter.next() {
                Some(v) if v == "json" => format = OutputFormat::Json,
                Some(v) if v == "text" => format = OutputFormat::Text,
                Some(v) => return Err(format!("--format must be text or json; got {v:?}")),
                None => return Err("--format requires an argument".into()),
            },
            other if !other.starts_with("--") && path.is_none() => {
                path = Some(other.to_string());
            }
            other => return Err(format!("unknown argument: {other:?}")),
        }
    }
    let path = path.ok_or("`mc model show-benchmarks` requires a model directory path")?;
    Ok(ShowBenchmarksCommand {
        path,
        metric,
        format,
    })
}

pub fn run(cmd: ShowBenchmarksCommand) -> i32 {
    let model_path = Path::new(&cmd.path);
    let model_dir = if model_path.is_dir() {
        model_path.to_path_buf()
    } else {
        model_path.parent().unwrap_or(Path::new(".")).to_path_buf()
    };

    let lib = match benchmark::read_benchmark_library(&model_dir) {
        Ok(lib) => lib,
        Err(benchmark::BenchmarkError::NotFound { path }) => {
            eprintln!(
                "No benchmark library found at {path}. \
                 Run `mc model build-benchmarks` first."
            );
            return 1;
        }
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };

    match cmd.format {
        OutputFormat::Json => {
            // Pretty-print the raw library JSON.
            match serde_json::to_string_pretty(&lib) {
                Ok(json) => println!("{json}"),
                Err(e) => {
                    eprintln!("error: could not serialize benchmark library: {e}");
                    return 1;
                }
            }
        }
        OutputFormat::Text => {
            println!("Benchmark Library — {}", lib.workspace);
            println!(
                "Built: {}  |  Periods: {} → {}  |  {} periods",
                lib.generated_at
                    .split('T')
                    .next()
                    .unwrap_or(&lib.generated_at),
                lib.period_range.from,
                lib.period_range.to,
                lib.period_count,
            );
            println!();

            for (key, bench) in &lib.benchmarks {
                // Filter by --metric if specified.
                if let Some(ref filter) = cmd.metric {
                    if !bench.metric.eq_ignore_ascii_case(filter) {
                        continue;
                    }
                }

                let scope_display = if bench.scope.is_empty() {
                    String::new()
                } else {
                    let parts: Vec<String> = bench.scope.values().cloned().collect();
                    format!(" ({})", parts.join(", "))
                };

                println!(
                    "{}{}\t{} samples",
                    bench.metric, scope_display, bench.sample_count
                );
                println!(
                    "  p10={}  p25={}  p50={}  p75={}  p90={}",
                    format_stat(bench.p10, key),
                    format_stat(bench.p25, key),
                    format_stat(bench.p50, key),
                    format_stat(bench.p75, key),
                    format_stat(bench.p90, key),
                );
                println!(
                    "  mean={}  stddev={}",
                    format_stat(bench.mean, key),
                    format_stat(bench.stddev, key),
                );
                println!();
            }
        }
    }

    0
}

/// Format a stat value — percentage-like metrics get % suffix, large numbers get commas.
fn format_stat(v: f64, _key: &str) -> String {
    if v.abs() >= 1000.0 {
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
    } else {
        format!("{v:.4}")
    }
}
