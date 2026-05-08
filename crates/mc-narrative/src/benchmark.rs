//! Benchmark library — workspace-local percentile distributions from ledger data.
//!
//! Phase 7A.4 Session 1: reads evidence values from the interpretation ledger,
//! groups by (metric, scope), computes percentile distributions (p10/p25/p50/p75/p90),
//! and writes `.mosaic/benchmark-library.json`.
//!
//! Data never leaves the workspace — no cross-customer aggregation, no servers,
//! no anonymization. Per ADR-0021.
//!
//! Diagnostic codes MC7040–MC7044 are reserved for this module.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ledger::LedgerEntry;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Current benchmark library schema version.
pub const BENCHMARK_SCHEMA_VERSION: &str = "1.0";

/// Benchmark library filename within the `.mosaic/` directory.
const BENCHMARK_FILENAME: &str = "benchmark-library.json";

/// Directory name for Mosaic workspace metadata.
const MOSAIC_DIR: &str = ".mosaic";

// ---------------------------------------------------------------------------
// Error types (MC7040–MC7044)
// ---------------------------------------------------------------------------

/// Benchmark error — covers build, write, and read failures.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BenchmarkError {
    /// MC7040: Benchmark library schema version mismatch (built by newer version).
    #[error(
        "MC7040: benchmark library schema version mismatch: expected {expected}, got {actual}"
    )]
    SchemaVersionMismatch { expected: String, actual: String },

    /// MC7041: Benchmark function references a metric not found in the library.
    #[error("MC7041: benchmark metric `{metric}` not found in library (scope: `{scope_key}`)")]
    MetricNotFound { metric: String, scope_key: String },

    /// MC7042: Benchmark library is stale (ledger has entries newer than library).
    #[error(
        "MC7042: benchmark library may be stale: ledger has entries up to {ledger_latest}, \
         library covers through {library_latest}. Run `mc model build-benchmarks` to update."
    )]
    StaleLibrary {
        ledger_latest: String,
        library_latest: String,
    },

    /// MC7043: build-benchmarks ran with fewer than 2 ledger periods.
    #[error("MC7043: benchmark library built from only {period_count} period(s) — results may be unreliable")]
    FewPeriods { period_count: usize },

    /// MC7044: Benchmark library write failed (disk full, permission denied).
    #[error("MC7044: benchmark library write failed at `{path}`: {detail}")]
    WriteFailed { path: String, detail: String },

    /// File not found (no benchmark library exists yet).
    #[error("benchmark library not found at `{path}`")]
    NotFound { path: String },

    /// JSON serialization/deserialization error.
    #[error("benchmark library JSON error: {detail}")]
    Json { detail: String },

    /// File I/O error (not covered by WriteFailed or NotFound).
    #[error("benchmark library I/O error at `{path}`: {detail}")]
    Io { path: String, detail: String },
}

// ---------------------------------------------------------------------------
// Schema types
// ---------------------------------------------------------------------------

/// The workspace-local benchmark library — percentile distributions from ledger evidence.
///
/// Per ADR-0021: data never leaves the workspace. Built from the workspace's own
/// `.mosaic/analysis-ledger.jsonl` and stored in `.mosaic/benchmark-library.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkLibrary {
    /// Schema version for forward compatibility.
    pub schema_version: String,
    /// ISO-8601 UTC timestamp of generation.
    pub generated_at: String,
    /// Workspace name (informational — model directory's file_name component).
    pub workspace: String,
    /// Range of report periods covered by the benchmarks.
    pub period_range: PeriodRange,
    /// Number of distinct report periods contributing to the benchmarks.
    pub period_count: usize,
    /// Benchmarks keyed by "metric::scope_key" (e.g., "CTR::channel=Targeted Display").
    pub benchmarks: BTreeMap<String, MetricBenchmark>,
}

/// Range of report periods in the benchmark library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeriodRange {
    /// Earliest report_period in the sample.
    pub from: String,
    /// Latest report_period in the sample.
    pub to: String,
}

/// Percentile distribution for a single (metric, scope) combination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricBenchmark {
    /// The metric name (e.g., "CTR", "Impressions").
    pub metric: String,
    /// Scope dimensions (e.g., { "channel": "Targeted Display" }).
    pub scope: BTreeMap<String, String>,
    /// 10th percentile.
    pub p10: f64,
    /// 25th percentile.
    pub p25: f64,
    /// 50th percentile (median).
    pub p50: f64,
    /// 75th percentile.
    pub p75: f64,
    /// 90th percentile.
    pub p90: f64,
    /// Arithmetic mean.
    pub mean: f64,
    /// Standard deviation.
    pub stddev: f64,
    /// Number of samples contributing to this benchmark.
    pub sample_count: usize,
}

// ---------------------------------------------------------------------------
// Build pipeline
// ---------------------------------------------------------------------------

/// Build a benchmark library from ledger entries.
///
/// Per ADR-0021 Decision 3: reads all entries, groups by (metric, scope_key),
/// extracts numeric evidence values, computes percentile distributions.
///
/// Deduplicates by `report_period` within each (metric, scope_key) group —
/// latest entry per period wins.
pub fn build_benchmark_library(
    ledger: &[LedgerEntry],
    workspace: &str,
    since: Option<&str>,
) -> BenchmarkLibrary {
    // 1. Filter by --since if given.
    let filtered: Vec<&LedgerEntry> = ledger
        .iter()
        .filter(|e| {
            if let Some(since) = since {
                e.report_period.as_deref().map_or(false, |p| p >= since)
            } else {
                true
            }
        })
        .collect();

    // 2. Collect all distinct periods.
    let mut periods: Vec<String> = filtered
        .iter()
        .filter_map(|e| e.report_period.clone())
        .collect();
    periods.sort();
    periods.dedup();

    let period_range = if periods.is_empty() {
        PeriodRange {
            from: String::new(),
            to: String::new(),
        }
    } else {
        PeriodRange {
            from: periods[0].clone(),
            to: periods[periods.len() - 1].clone(),
        }
    };

    // 3. Group numeric evidence values by (metric, scope_key, period).
    //    Deduplicate: latest entry per (metric, scope_key, period) wins.
    //    Key: (field_name, scope_key) → BTreeMap<period, f64>
    let mut groups: BTreeMap<(String, String), BTreeMap<String, f64>> = BTreeMap::new();

    for entry in &filtered {
        let scope_key = entry
            .scope
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(",");

        let period = match &entry.report_period {
            Some(p) => p.clone(),
            None => continue, // Skip entries without a period.
        };

        for (field, value) in &entry.evidence {
            if let Some(num) = value.as_f64() {
                let key = (field.clone(), scope_key.clone());
                groups.entry(key).or_default().insert(period.clone(), num);
            }
        }
    }

    // 4. Compute percentile distributions for each group.
    let mut benchmarks = BTreeMap::new();
    for ((metric, scope_key), period_values) in &groups {
        let mut values: Vec<f64> = period_values.values().copied().collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let scope: BTreeMap<String, String> = if scope_key.is_empty() {
            BTreeMap::new()
        } else {
            scope_key
                .split(',')
                .filter_map(|kv| {
                    let mut parts = kv.splitn(2, '=');
                    match (parts.next(), parts.next()) {
                        (Some(k), Some(v)) => Some((k.to_string(), v.to_string())),
                        _ => None,
                    }
                })
                .collect()
        };

        let bench = MetricBenchmark {
            metric: metric.clone(),
            scope,
            p10: percentile(&values, 10.0),
            p25: percentile(&values, 25.0),
            p50: percentile(&values, 50.0),
            p75: percentile(&values, 75.0),
            p90: percentile(&values, 90.0),
            mean: mean(&values),
            stddev: stddev(&values),
            sample_count: values.len(),
        };

        let lib_key = if scope_key.is_empty() {
            metric.clone()
        } else {
            format!("{metric}::{scope_key}")
        };
        benchmarks.insert(lib_key, bench);
    }

    let now = chrono_utc_now();

    BenchmarkLibrary {
        schema_version: BENCHMARK_SCHEMA_VERSION.to_string(),
        generated_at: now,
        workspace: workspace.to_string(),
        period_range,
        period_count: periods.len(),
        benchmarks,
    }
}

// ---------------------------------------------------------------------------
// Percentile and statistical helpers
// ---------------------------------------------------------------------------

/// Compute the p-th percentile of a sorted slice using nearest-rank method.
///
/// Per handoff: sort ascending, index at the right position. No interpolation
/// needed for Phase 7A.4 (nearest-rank is sufficient for 6-24 samples).
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (p / 100.0 * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Arithmetic mean of a slice.
fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// Population standard deviation.
fn stddev(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let m = mean(values);
    let variance = values.iter().map(|v| (v - m).powi(2)).sum::<f64>() / values.len() as f64;
    variance.sqrt()
}

/// Simple ISO-8601 UTC timestamp without chrono dependency.
fn chrono_utc_now() -> String {
    // Use UNIX_EPOCH + SystemTime for a chrono-free timestamp.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Simple UTC formatting: days since epoch → date components.
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Convert days since 1970-01-01 to y/m/d.
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm from Howard Hinnant's civil_from_days.
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u64, m, d)
}

// ---------------------------------------------------------------------------
// Write path — atomic JSON write
// ---------------------------------------------------------------------------

/// Path to the benchmark library file.
pub fn benchmark_library_path(model_dir: &Path) -> PathBuf {
    model_dir.join(MOSAIC_DIR).join(BENCHMARK_FILENAME)
}

/// Write a benchmark library to `.mosaic/benchmark-library.json` atomically.
///
/// Per ADR-0021 Decision 3: atomic write via tmp + rename, same pattern as
/// `write_ledger_entries` in `ledger.rs`.
pub fn write_benchmark_library(
    model_dir: &Path,
    lib: &BenchmarkLibrary,
) -> Result<PathBuf, BenchmarkError> {
    let mosaic_dir = model_dir.join(MOSAIC_DIR);
    fs::create_dir_all(&mosaic_dir).map_err(|e| BenchmarkError::WriteFailed {
        path: mosaic_dir.display().to_string(),
        detail: e.to_string(),
    })?;

    let path = benchmark_library_path(model_dir);
    let tmp_path = path.with_extension("json.tmp");

    // Serialize to pretty JSON.
    let json = serde_json::to_string_pretty(lib).map_err(|e| BenchmarkError::Json {
        detail: e.to_string(),
    })?;

    // Write to tmp file.
    let mut file = fs::File::create(&tmp_path).map_err(|e| BenchmarkError::WriteFailed {
        path: tmp_path.display().to_string(),
        detail: e.to_string(),
    })?;
    file.write_all(json.as_bytes())
        .map_err(|e| BenchmarkError::WriteFailed {
            path: tmp_path.display().to_string(),
            detail: e.to_string(),
        })?;
    file.flush().map_err(|e| BenchmarkError::WriteFailed {
        path: tmp_path.display().to_string(),
        detail: e.to_string(),
    })?;

    // Atomic rename.
    fs::rename(&tmp_path, &path).map_err(|e| BenchmarkError::WriteFailed {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;

    Ok(path)
}

// ---------------------------------------------------------------------------
// Read path
// ---------------------------------------------------------------------------

/// Read the benchmark library from `.mosaic/benchmark-library.json`.
///
/// Returns `BenchmarkError::NotFound` if the file is absent (callers treat
/// this as "no benchmarks available, skip benchmark templates").
pub fn read_benchmark_library(model_dir: &Path) -> Result<BenchmarkLibrary, BenchmarkError> {
    let path = benchmark_library_path(model_dir);
    if !path.exists() {
        return Err(BenchmarkError::NotFound {
            path: path.display().to_string(),
        });
    }

    let contents = fs::read_to_string(&path).map_err(|e| BenchmarkError::Io {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;

    let lib: BenchmarkLibrary =
        serde_json::from_str(&contents).map_err(|e| BenchmarkError::Json {
            detail: e.to_string(),
        })?;

    // MC7040: schema version check.
    if lib.schema_version != BENCHMARK_SCHEMA_VERSION {
        return Err(BenchmarkError::SchemaVersionMismatch {
            expected: BENCHMARK_SCHEMA_VERSION.to_string(),
            actual: lib.schema_version.clone(),
        });
    }

    Ok(lib)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::{LedgerEntry, NarrativeRecord};

    /// Create a minimal ledger entry for testing.
    fn make_entry(period: &str, scope: &[(&str, &str)], evidence: &[(&str, f64)]) -> LedgerEntry {
        LedgerEntry {
            schema_version: "1.0".to_string(),
            ledger_entry_id: format!("test-{period}"),
            generated_at: "2026-05-07T10:00:00Z".to_string(),
            model: "model.yaml".to_string(),
            model_hash: "sha256:test".to_string(),
            report_period: Some(period.to_string()),
            scope: scope
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            narrative: NarrativeRecord {
                id: "test".to_string(),
                section: None,
                severity: "info".to_string(),
                text: "test narrative".to_string(),
                template_id: "test".to_string(),
                notability_score: None,
            },
            evidence: evidence
                .iter()
                .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
                .collect(),
            benchmarks_referenced: vec![],
        }
    }

    #[test]
    fn test_build_benchmark_library_computes_percentiles() {
        let entries = vec![
            make_entry("2026-01", &[("channel", "Display")], &[("CTR", 0.10)]),
            make_entry("2026-02", &[("channel", "Display")], &[("CTR", 0.20)]),
            make_entry("2026-03", &[("channel", "Display")], &[("CTR", 0.30)]),
            make_entry("2026-04", &[("channel", "Display")], &[("CTR", 0.40)]),
            make_entry("2026-05", &[("channel", "Display")], &[("CTR", 0.50)]),
            make_entry("2026-06", &[("channel", "Display")], &[("CTR", 0.60)]),
        ];
        let lib = build_benchmark_library(&entries, "test-workspace", None);

        assert_eq!(lib.period_count, 6);
        assert_eq!(lib.workspace, "test-workspace");
        assert_eq!(lib.period_range.from, "2026-01");
        assert_eq!(lib.period_range.to, "2026-06");

        let bench = lib.benchmarks.get("CTR::channel=Display").unwrap();
        assert_eq!(bench.sample_count, 6);
        assert_eq!(bench.metric, "CTR");

        // p50 of [0.10, 0.20, 0.30, 0.40, 0.50, 0.60]:
        // idx = round(0.50 * 5) = round(2.5) = 3 → sorted[3] = 0.40
        assert!((bench.p50 - 0.40).abs() < 1e-9, "p50 was {}", bench.p50);

        // mean = (0.10 + 0.20 + 0.30 + 0.40 + 0.50 + 0.60) / 6 = 0.35
        assert!((bench.mean - 0.35).abs() < 1e-9, "mean was {}", bench.mean);

        // stddev > 0
        assert!(bench.stddev > 0.0, "stddev should be positive");
    }

    #[test]
    fn test_build_benchmark_library_groups_by_scope() {
        let entries = vec![
            make_entry("2026-01", &[("channel", "Display")], &[("CTR", 0.10)]),
            make_entry("2026-01", &[("channel", "Search")], &[("CTR", 0.50)]),
            make_entry("2026-02", &[("channel", "Display")], &[("CTR", 0.20)]),
            make_entry("2026-02", &[("channel", "Search")], &[("CTR", 0.60)]),
        ];
        let lib = build_benchmark_library(&entries, "test", None);

        assert_eq!(lib.benchmarks.len(), 2);
        assert!(lib.benchmarks.contains_key("CTR::channel=Display"));
        assert!(lib.benchmarks.contains_key("CTR::channel=Search"));

        let display = lib.benchmarks.get("CTR::channel=Display").unwrap();
        let search = lib.benchmarks.get("CTR::channel=Search").unwrap();

        assert_eq!(display.sample_count, 2);
        assert_eq!(search.sample_count, 2);
        assert!(display.mean < search.mean);
    }

    #[test]
    fn test_write_and_read_benchmark_library_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let entries = vec![
            make_entry("2026-01", &[("channel", "Display")], &[("CTR", 0.15)]),
            make_entry("2026-02", &[("channel", "Display")], &[("CTR", 0.25)]),
        ];
        let lib = build_benchmark_library(&entries, "roundtrip-test", None);

        let path = write_benchmark_library(dir.path(), &lib).unwrap();
        assert!(path.exists());

        let loaded = read_benchmark_library(dir.path()).unwrap();
        assert_eq!(loaded.workspace, "roundtrip-test");
        assert_eq!(loaded.period_count, 2);
        assert_eq!(loaded.benchmarks.len(), 1);

        let bench = loaded.benchmarks.get("CTR::channel=Display").unwrap();
        assert_eq!(bench.sample_count, 2);
    }

    #[test]
    fn test_build_benchmarks_since_filter() {
        let entries = vec![
            make_entry("2025-10", &[("channel", "Display")], &[("CTR", 0.05)]),
            make_entry("2025-11", &[("channel", "Display")], &[("CTR", 0.10)]),
            make_entry("2026-01", &[("channel", "Display")], &[("CTR", 0.20)]),
            make_entry("2026-02", &[("channel", "Display")], &[("CTR", 0.30)]),
        ];

        // With --since 2026-01: only the last 2 entries.
        let lib = build_benchmark_library(&entries, "test", Some("2026-01"));
        assert_eq!(lib.period_count, 2);

        let bench = lib.benchmarks.get("CTR::channel=Display").unwrap();
        assert_eq!(bench.sample_count, 2);
        // mean of [0.20, 0.30] = 0.25
        assert!((bench.mean - 0.25).abs() < 1e-9);
    }

    #[test]
    fn test_build_benchmarks_empty_ledger_produces_empty_library() {
        let lib = build_benchmark_library(&[], "empty-test", None);
        assert_eq!(lib.period_count, 0);
        assert!(lib.benchmarks.is_empty());
        assert!(lib.period_range.from.is_empty());
        assert!(lib.period_range.to.is_empty());
    }

    #[test]
    fn test_build_benchmarks_deduplicates_by_period() {
        // Two entries for the same period → latest wins (only 1 sample per period).
        let entries = vec![
            make_entry("2026-01", &[("channel", "Display")], &[("CTR", 0.10)]),
            make_entry("2026-01", &[("channel", "Display")], &[("CTR", 0.90)]),
            make_entry("2026-02", &[("channel", "Display")], &[("CTR", 0.20)]),
        ];
        let lib = build_benchmark_library(&entries, "test", None);
        let bench = lib.benchmarks.get("CTR::channel=Display").unwrap();
        // Deduplicated: period 2026-01 → 0.90 (latest wins), 2026-02 → 0.20.
        assert_eq!(bench.sample_count, 2);
    }

    #[test]
    fn test_read_benchmark_library_not_found() {
        let dir = tempfile::tempdir().unwrap();
        match read_benchmark_library(dir.path()) {
            Err(BenchmarkError::NotFound { .. }) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn test_percentile_helpers() {
        let sorted = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert!((percentile(&sorted, 50.0) - 3.0).abs() < 1e-9);
        assert!((mean(&sorted) - 3.0).abs() < 1e-9);
        assert!(stddev(&sorted) > 0.0);
        assert!((percentile(&[], 50.0)).abs() < 1e-9);
    }
}
