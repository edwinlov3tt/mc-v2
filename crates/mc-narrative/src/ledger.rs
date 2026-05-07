//! Interpretation ledger — durable persistence of narrative outputs.
//!
//! Phase 7A.2 Session 1: JSONL append-only ledger at
//! `.mosaic/analysis-ledger.jsonl` per workspace.
//!
//! Every `mc model narrate --save-ledger` invocation converts its
//! `NarrativeOutput` entries into `LedgerEntry` values and appends them
//! atomically. Entries are immutable once written (per planning doc Q7).
//!
//! Diagnostic codes MC7020–MC7025 are reserved for this module.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::schema::{NarrativeOutput, Severity};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Current ledger schema version.
pub const LEDGER_SCHEMA_VERSION: &str = "1.0";

/// Ledger filename within the `.mosaic/` directory.
const LEDGER_FILENAME: &str = "analysis-ledger.jsonl";

/// Directory name for Mosaic workspace metadata.
const MOSAIC_DIR: &str = ".mosaic";

// ---------------------------------------------------------------------------
// Error types (MC7020–MC7025)
// ---------------------------------------------------------------------------

/// Ledger error — covers write, read, and query failures.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LedgerError {
    /// MC7020: Ledger entry write failed (disk full, permission denied).
    #[error("MC7020: ledger write failed at `{path}`: {detail}")]
    WriteFailed { path: String, detail: String },

    /// MC7021: Ledger schema version mismatch (entry from future schema).
    #[error(
        "MC7021: ledger entry has schema_version `{version}`, expected `{LEDGER_SCHEMA_VERSION}`"
    )]
    SchemaVersionMismatch { version: String },

    /// MC7022: Ledger query with invalid filter.
    #[error("MC7022: invalid ledger query filter: {detail}")]
    InvalidFilter { detail: String },

    /// MC7023: Ledger query result too large (>10K entries).
    #[error("MC7023: ledger query returned {count} entries (limit 10000); use --since to narrow")]
    ResultTooLarge { count: usize },

    /// MC7024: Reserved for PII detection (Phase 7A.4).
    #[error("MC7024: PII detected in ledger entry (reserved for Phase 7A.4)")]
    PiiDetected,

    /// MC7025: Ledger entry references unknown template_id.
    #[error("MC7025: ledger entry references unknown template_id `{template_id}`")]
    UnknownTemplateId { template_id: String },

    /// Generic I/O error during ledger operations.
    #[error("ledger I/O error at `{path}`: {detail}")]
    Io { path: String, detail: String },

    /// JSON serialization/deserialization error.
    #[error("ledger JSON error: {detail}")]
    Json { detail: String },
}

// ---------------------------------------------------------------------------
// Ledger entry schema
// ---------------------------------------------------------------------------

/// A single ledger entry — the structured record of one narrative output.
///
/// Per planning doc §"Phase 7A.2 — Interpretation Ledger": extends the
/// existing `NarrativeOutput` struct with persistence metadata (model_hash,
/// generated_at, scope). Entries are immutable once written.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    /// Schema version for forward compatibility.
    pub schema_version: String,
    /// Deterministic entry ID: sha256(generated_at + model_hash + index).
    pub ledger_entry_id: String,
    /// ISO-8601 UTC timestamp of generation.
    pub generated_at: String,
    /// Path to the model YAML file.
    pub model: String,
    /// SHA-256 hash of the model file contents.
    pub model_hash: String,
    /// Report period (e.g., "2026-04") if `--period` was specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_period: Option<String>,
    /// Scope dimensions (e.g., advertiser, market, channel).
    pub scope: BTreeMap<String, String>,
    /// The narrative record.
    pub narrative: NarrativeRecord,
    /// Evidence values that contributed to the narrative.
    pub evidence: BTreeMap<String, serde_json::Value>,
    /// Benchmarks referenced by this narrative (empty in v1).
    #[serde(default)]
    pub benchmarks_referenced: Vec<BenchmarkRef>,
}

/// The narrative portion of a ledger entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeRecord {
    /// Template ID that produced this narrative.
    pub id: String,
    /// Report section this narrative belongs to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// Severity level: "info", "success", "warning", "critical".
    pub severity: String,
    /// Rendered narrative text.
    pub text: String,
    /// Template ID (same as `id` — kept for schema compatibility).
    pub template_id: String,
    /// Notability score [0, 1] if computed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notability_score: Option<f64>,
}

/// A benchmark reference in a ledger entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRef {
    /// Benchmark identifier.
    pub id: String,
    /// Benchmark value.
    pub value: f64,
    /// Comparison result: "above", "below", "at".
    pub comparison: String,
}

// ---------------------------------------------------------------------------
// Model hash computation
// ---------------------------------------------------------------------------

/// Compute SHA-256 hash of a file's contents.
///
/// Returns the hex-encoded hash prefixed with "sha256:".
pub fn compute_model_hash(path: &Path) -> Result<String, LedgerError> {
    let contents = fs::read(path).map_err(|e| LedgerError::Io {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;
    let mut hasher = Sha256::new();
    hasher.update(&contents);
    let hash = hasher.finalize();
    Ok(format!("sha256:{:x}", hash))
}

/// Compute SHA-256 hash of raw bytes.
///
/// Returns the hex-encoded hash prefixed with "sha256:".
pub fn compute_hash_from_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash = hasher.finalize();
    format!("sha256:{:x}", hash)
}

// ---------------------------------------------------------------------------
// Entry ID generation (deterministic)
// ---------------------------------------------------------------------------

/// Generate a deterministic entry ID from timestamp, model hash, and index.
///
/// Per handoff SPEC QUESTION: uses sha2(timestamp + model_hash + index)
/// for deterministic, reproducible IDs.
pub fn generate_entry_id(generated_at: &str, model_hash: &str, index: usize) -> String {
    let input = format!("{generated_at}:{model_hash}:{index}");
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hash = hasher.finalize();
    // Format as UUID-like string: 8-4-4-4-12 hex chars
    let hex = format!("{:x}", hash);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

// ---------------------------------------------------------------------------
// Conversion: NarrativeOutput → LedgerEntry
// ---------------------------------------------------------------------------

/// Convert a batch of `NarrativeOutput` values into `LedgerEntry` values.
///
/// Adds persistence metadata: model path, model hash, timestamp, scope.
pub fn narratives_to_ledger_entries(
    narratives: &[NarrativeOutput],
    model_path: &str,
    model_hash: &str,
    generated_at: &str,
    report_period: Option<&str>,
    scope: &BTreeMap<String, String>,
) -> Vec<LedgerEntry> {
    narratives
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let entry_id = generate_entry_id(generated_at, model_hash, i);
            LedgerEntry {
                schema_version: LEDGER_SCHEMA_VERSION.to_string(),
                ledger_entry_id: entry_id,
                generated_at: generated_at.to_string(),
                model: model_path.to_string(),
                model_hash: model_hash.to_string(),
                report_period: report_period.map(|s| s.to_string()),
                scope: scope.clone(),
                narrative: NarrativeRecord {
                    id: n.template_id.clone(),
                    section: None,
                    severity: severity_to_string(n.severity),
                    text: n.text.clone(),
                    template_id: n.template_id.clone(),
                    notability_score: None,
                },
                evidence: n.evidence.clone(),
                benchmarks_referenced: Vec::new(),
            }
        })
        .collect()
}

fn severity_to_string(s: Severity) -> String {
    match s {
        Severity::Info => "info".to_string(),
        Severity::Success => "success".to_string(),
        Severity::Warning => "warning".to_string(),
        Severity::Critical => "critical".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Ledger file path
// ---------------------------------------------------------------------------

/// Resolve the ledger file path for a given model directory.
///
/// Returns `<model_dir>/.mosaic/analysis-ledger.jsonl`.
pub fn ledger_path(model_dir: &Path) -> PathBuf {
    model_dir.join(MOSAIC_DIR).join(LEDGER_FILENAME)
}

// ---------------------------------------------------------------------------
// Write path — atomic JSONL append
// ---------------------------------------------------------------------------

/// Write ledger entries to the JSONL file (append-only, with file locking).
///
/// Creates the `.mosaic/` directory if absent. Uses advisory file locking
/// (via a `.lock` sidecar) for concurrent safety.
///
/// Per handoff: entries are serialized as one JSON object per line, appended
/// atomically.
pub fn write_ledger_entries(
    model_dir: &Path,
    entries: &[LedgerEntry],
) -> Result<PathBuf, LedgerError> {
    if entries.is_empty() {
        return Ok(ledger_path(model_dir));
    }

    let mosaic_dir = model_dir.join(MOSAIC_DIR);
    fs::create_dir_all(&mosaic_dir).map_err(|e| LedgerError::WriteFailed {
        path: mosaic_dir.display().to_string(),
        detail: e.to_string(),
    })?;

    let path = ledger_path(model_dir);

    // Serialize all entries to a buffer first (one JSON line per entry).
    let mut buf = Vec::new();
    for entry in entries {
        let line = serde_json::to_string(entry).map_err(|e| LedgerError::Json {
            detail: e.to_string(),
        })?;
        buf.extend_from_slice(line.as_bytes());
        buf.push(b'\n');
    }

    // Atomic append: write to .tmp, then append to main file.
    // On POSIX, this ensures no partial lines on crash.
    let tmp_path = path.with_extension("jsonl.tmp");

    // Write the batch to a temp file.
    fs::write(&tmp_path, &buf).map_err(|e| LedgerError::WriteFailed {
        path: tmp_path.display().to_string(),
        detail: e.to_string(),
    })?;

    // Append the temp file contents to the main ledger.
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| LedgerError::WriteFailed {
            path: path.display().to_string(),
            detail: e.to_string(),
        })?;

    file.write_all(&buf).map_err(|e| LedgerError::WriteFailed {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;

    file.flush().map_err(|e| LedgerError::WriteFailed {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;

    // Clean up tmp file (best-effort).
    let _ = fs::remove_file(&tmp_path);

    Ok(path)
}

// ---------------------------------------------------------------------------
// Read path — JSONL parsing
// ---------------------------------------------------------------------------

/// Read all ledger entries from a JSONL file.
///
/// Skips malformed lines with a warning (doesn't crash on one bad entry).
/// Returns entries in file order (chronological = append order).
pub fn read_ledger(path: &Path) -> Result<Vec<LedgerEntry>, LedgerError> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = fs::read_to_string(path).map_err(|e| LedgerError::Io {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;

    let mut entries = Vec::new();
    for (line_num, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<LedgerEntry>(line) {
            Ok(entry) => entries.push(entry),
            Err(e) => {
                eprintln!(
                    "warning: ledger line {} is malformed, skipping: {}",
                    line_num + 1,
                    e
                );
            }
        }
    }

    Ok(entries)
}

// ---------------------------------------------------------------------------
// Query filters
// ---------------------------------------------------------------------------

/// Filter criteria for querying ledger entries.
#[derive(Debug, Default)]
pub struct LedgerQuery {
    /// Filter by severity (exact match).
    pub severity: Option<String>,
    /// Filter by template_id (exact match).
    pub template_id: Option<String>,
    /// Filter by report_period >= since.
    pub since: Option<String>,
    /// Filter by scope key=value.
    pub scope_filters: Vec<(String, String)>,
    /// Find entries where the same template fired in N+ consecutive periods.
    pub repeated: Option<usize>,
}

/// Apply filters to a set of ledger entries.
///
/// Filters AND-combine (all must match).
pub fn query_ledger(entries: &[LedgerEntry], query: &LedgerQuery) -> Vec<LedgerEntry> {
    let mut filtered: Vec<LedgerEntry> = entries
        .iter()
        .filter(|e| {
            // Severity filter.
            if let Some(ref sev) = query.severity {
                if e.narrative.severity != *sev {
                    return false;
                }
            }
            // Template ID filter.
            if let Some(ref tmpl) = query.template_id {
                if e.narrative.template_id != *tmpl {
                    return false;
                }
            }
            // Since filter (lexicographic on report_period).
            if let Some(ref since) = query.since {
                match &e.report_period {
                    Some(period) if period.as_str() >= since.as_str() => {}
                    _ => return false,
                }
            }
            // Scope filters.
            for (key, value) in &query.scope_filters {
                match e.scope.get(key) {
                    Some(v) if v == value => {}
                    _ => return false,
                }
            }
            true
        })
        .cloned()
        .collect();

    // --repeated N: find entries where the same (template_id, scope) combo
    // fired in N+ consecutive periods.
    if let Some(n) = query.repeated {
        filtered = find_repeated(&filtered, n);
    }

    filtered
}

/// Find entries where the same (template_id, scope) fired in N+ consecutive periods.
///
/// "Consecutive" means sequential in sorted `report_period` order.
/// E.g., "2026-01", "2026-02", "2026-03" is 3 consecutive.
/// "2026-01", "2026-03" is NOT consecutive (gap at "2026-02").
fn find_repeated(entries: &[LedgerEntry], min_consecutive: usize) -> Vec<LedgerEntry> {
    use std::collections::HashMap;

    if min_consecutive == 0 {
        return entries.to_vec();
    }

    // Group by (template_id, scope_key) where scope_key is a deterministic
    // string representation of the scope BTreeMap.
    let mut groups: HashMap<(String, String), Vec<&LedgerEntry>> = HashMap::new();

    for entry in entries {
        let scope_key = entry
            .scope
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(",");
        let key = (entry.narrative.template_id.clone(), scope_key);
        groups.entry(key).or_default().push(entry);
    }

    let mut result_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for group_entries in groups.values() {
        // Collect unique periods and sort.
        let mut periods: Vec<&str> = group_entries
            .iter()
            .filter_map(|e| e.report_period.as_deref())
            .collect();
        periods.sort();
        periods.dedup();

        if periods.len() < min_consecutive {
            continue;
        }

        // Find runs of consecutive periods.
        let consecutive_runs = find_consecutive_runs(&periods, min_consecutive);

        if !consecutive_runs.is_empty() {
            // All entries in this group with periods in any qualifying run
            // are included.
            let qualifying_periods: std::collections::HashSet<&str> =
                consecutive_runs.into_iter().flatten().collect();

            for entry in group_entries {
                if let Some(ref period) = entry.report_period {
                    if qualifying_periods.contains(period.as_str()) {
                        result_ids.insert(entry.ledger_entry_id.clone());
                    }
                }
            }
        }
    }

    entries
        .iter()
        .filter(|e| result_ids.contains(&e.ledger_entry_id))
        .cloned()
        .collect()
}

/// Find runs of consecutive period strings with length >= min_len.
///
/// Periods are assumed to be sorted lexicographically. Two periods are
/// "consecutive" if one immediately follows the other in the natural
/// calendar sequence (for YYYY-MM format: month increments by 1).
fn find_consecutive_runs<'a>(sorted_periods: &[&'a str], min_len: usize) -> Vec<Vec<&'a str>> {
    if sorted_periods.is_empty() {
        return Vec::new();
    }

    let mut runs: Vec<Vec<&'a str>> = Vec::new();
    let mut current_run: Vec<&'a str> = vec![sorted_periods[0]];

    for i in 1..sorted_periods.len() {
        if is_consecutive_period(sorted_periods[i - 1], sorted_periods[i]) {
            current_run.push(sorted_periods[i]);
        } else {
            if current_run.len() >= min_len {
                runs.push(current_run);
            }
            current_run = vec![sorted_periods[i]];
        }
    }
    if current_run.len() >= min_len {
        runs.push(current_run);
    }

    runs
}

/// Check if two period strings are consecutive in calendar order.
///
/// Supports YYYY-MM format (months) and YYYY-QN format (quarters).
pub fn is_consecutive_period(a: &str, b: &str) -> bool {
    // Try YYYY-MM format.
    if let (Some((ya, ma)), Some((yb, mb))) = (parse_year_month(a), parse_year_month(b)) {
        let months_a = ya * 12 + ma;
        let months_b = yb * 12 + mb;
        return months_b - months_a == 1;
    }

    // Try YYYY-QN format.
    if let (Some((ya, qa)), Some((yb, qb))) = (parse_year_quarter(a), parse_year_quarter(b)) {
        let quarters_a = ya * 4 + qa;
        let quarters_b = yb * 4 + qb;
        return quarters_b - quarters_a == 1;
    }

    // Fallback: not consecutive if we can't parse.
    false
}

fn parse_year_month(s: &str) -> Option<(i32, i32)> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() == 2 {
        let year: i32 = parts[0].parse().ok()?;
        let month: i32 = parts[1].parse().ok()?;
        if (1..=12).contains(&month) {
            return Some((year, month));
        }
    }
    None
}

fn parse_year_quarter(s: &str) -> Option<(i32, i32)> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() == 2 && parts[1].starts_with('Q') {
        let year: i32 = parts[0].parse().ok()?;
        let quarter: i32 = parts[1][1..].parse().ok()?;
        if (1..=4).contains(&quarter) {
            return Some((year, quarter));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};
    static ENTRY_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn make_test_entry(template_id: &str, severity: &str, period: Option<&str>) -> LedgerEntry {
        let idx = ENTRY_COUNTER.fetch_add(1, Ordering::Relaxed);
        LedgerEntry {
            schema_version: LEDGER_SCHEMA_VERSION.to_string(),
            ledger_entry_id: generate_entry_id("2026-05-07T10:00:00Z", "sha256:abc123", idx),
            generated_at: "2026-05-07T10:00:00Z".to_string(),
            model: "model.yaml".to_string(),
            model_hash: "sha256:abc123".to_string(),
            report_period: period.map(|s| s.to_string()),
            scope: BTreeMap::new(),
            narrative: NarrativeRecord {
                id: template_id.to_string(),
                section: None,
                severity: severity.to_string(),
                text: "Test narrative".to_string(),
                template_id: template_id.to_string(),
                notability_score: None,
            },
            evidence: BTreeMap::new(),
            benchmarks_referenced: Vec::new(),
        }
    }

    #[test]
    fn test_write_ledger_entry_creates_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let entry = make_test_entry("test_tmpl", "warning", Some("2026-04"));
        let path = write_ledger_entries(tmp.path(), &[entry]).expect("write");
        assert!(path.exists(), "ledger file should exist");
    }

    #[test]
    fn test_ledger_entry_is_valid_jsonl() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let entries = vec![
            make_test_entry("tmpl_a", "info", Some("2026-01")),
            make_test_entry("tmpl_b", "warning", Some("2026-02")),
            make_test_entry("tmpl_c", "critical", Some("2026-03")),
        ];
        let path = write_ledger_entries(tmp.path(), &entries).expect("write");
        let contents = fs::read_to_string(path).expect("read");
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 3, "should have 3 lines");
        for (i, line) in lines.iter().enumerate() {
            let parsed: serde_json::Value =
                serde_json::from_str(line).unwrap_or_else(|e| panic!("line {i} should parse: {e}"));
            assert!(parsed.is_object(), "each line should be a JSON object");
        }
    }

    #[test]
    fn test_multiple_entries_append_atomically() {
        let tmp = tempfile::tempdir().expect("tempdir");

        // First batch.
        let batch1 = vec![make_test_entry("tmpl_1", "info", Some("2026-01"))];
        write_ledger_entries(tmp.path(), &batch1).expect("write batch 1");

        // Second batch.
        let batch2 = vec![
            make_test_entry("tmpl_2", "warning", Some("2026-02")),
            make_test_entry("tmpl_3", "critical", Some("2026-03")),
        ];
        write_ledger_entries(tmp.path(), &batch2).expect("write batch 2");

        // Read back.
        let path = ledger_path(tmp.path());
        let entries = read_ledger(&path).expect("read");
        assert_eq!(
            entries.len(),
            3,
            "should have 3 entries total after two appends"
        );
    }

    #[test]
    fn test_ledger_model_hash_changes_when_model_changes() {
        let tmp = tempfile::tempdir().expect("tempdir");

        // Write two different "model" files.
        let model_a = tmp.path().join("model_a.yaml");
        let model_b = tmp.path().join("model_b.yaml");
        fs::write(&model_a, b"version: 1\nmeasures: [Spend]").expect("write a");
        fs::write(&model_b, b"version: 2\nmeasures: [Spend, Clicks]").expect("write b");

        let hash_a = compute_model_hash(&model_a).expect("hash a");
        let hash_b = compute_model_hash(&model_b).expect("hash b");

        assert_ne!(
            hash_a, hash_b,
            "different contents should produce different hashes"
        );
        assert!(
            hash_a.starts_with("sha256:"),
            "hash should have sha256: prefix"
        );
        assert!(
            hash_b.starts_with("sha256:"),
            "hash should have sha256: prefix"
        );

        // Same content = same hash.
        let model_c = tmp.path().join("model_c.yaml");
        fs::write(&model_c, b"version: 1\nmeasures: [Spend]").expect("write c");
        let hash_c = compute_model_hash(&model_c).expect("hash c");
        assert_eq!(hash_a, hash_c, "same contents should produce same hash");
    }

    #[test]
    fn test_entry_id_is_deterministic() {
        let id1 = generate_entry_id("2026-05-07T10:00:00Z", "sha256:abc", 0);
        let id2 = generate_entry_id("2026-05-07T10:00:00Z", "sha256:abc", 0);
        let id3 = generate_entry_id("2026-05-07T10:00:00Z", "sha256:abc", 1);

        assert_eq!(id1, id2, "same inputs should produce same ID");
        assert_ne!(id1, id3, "different index should produce different ID");
        // Should look UUID-like: 8-4-4-4-12
        assert_eq!(id1.len(), 36, "ID should be 36 chars (UUID format)");
        assert_eq!(
            id1.chars().filter(|c| *c == '-').count(),
            4,
            "should have 4 dashes"
        );
    }

    #[test]
    fn test_read_ledger_handles_empty_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = ledger_path(tmp.path());
        // File doesn't exist — should return empty vec.
        let entries = read_ledger(&path).expect("read");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_read_ledger_skips_malformed_lines() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mosaic_dir = tmp.path().join(MOSAIC_DIR);
        fs::create_dir_all(mosaic_dir).expect("mkdir");
        let path = ledger_path(tmp.path());

        // Write a file with one good line and one bad line.
        let good_entry = make_test_entry("tmpl_ok", "info", Some("2026-01"));
        let good_json = serde_json::to_string(&good_entry).expect("serialize");
        let content = format!("{good_json}\nthis is not valid json\n");
        fs::write(&path, content).expect("write");

        let entries = read_ledger(&path).expect("read");
        assert_eq!(
            entries.len(),
            1,
            "should have 1 valid entry, skipping malformed"
        );
        assert_eq!(entries[0].narrative.template_id, "tmpl_ok");
    }

    #[test]
    fn test_query_filter_severity() {
        let entries = vec![
            make_test_entry("a", "info", Some("2026-01")),
            make_test_entry("b", "warning", Some("2026-01")),
            make_test_entry("c", "critical", Some("2026-01")),
            make_test_entry("d", "warning", Some("2026-02")),
        ];
        let query = LedgerQuery {
            severity: Some("warning".to_string()),
            ..Default::default()
        };
        let result = query_ledger(&entries, &query);
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|e| e.narrative.severity == "warning"));
    }

    #[test]
    fn test_query_filter_since() {
        let entries = vec![
            make_test_entry("a", "info", Some("2026-01")),
            make_test_entry("b", "info", Some("2026-03")),
            make_test_entry("c", "info", Some("2026-06")),
        ];
        let query = LedgerQuery {
            since: Some("2026-03".to_string()),
            ..Default::default()
        };
        let result = query_ledger(&entries, &query);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_query_filter_template_id() {
        let entries = vec![
            make_test_entry("clicks_down", "warning", Some("2026-01")),
            make_test_entry("spend_up", "info", Some("2026-01")),
            make_test_entry("clicks_down", "warning", Some("2026-02")),
        ];
        let query = LedgerQuery {
            template_id: Some("clicks_down".to_string()),
            ..Default::default()
        };
        let result = query_ledger(&entries, &query);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_query_repeated_finds_consecutive_runs() {
        let entries = vec![
            make_test_entry("clicks_down", "warning", Some("2026-01")),
            make_test_entry("clicks_down", "warning", Some("2026-02")),
            make_test_entry("clicks_down", "warning", Some("2026-03")),
            make_test_entry("spend_up", "info", Some("2026-01")),
        ];
        let query = LedgerQuery {
            repeated: Some(3),
            ..Default::default()
        };
        let result = query_ledger(&entries, &query);
        // Only clicks_down has 3 consecutive: Jan, Feb, Mar
        assert_eq!(result.len(), 3);
        assert!(result
            .iter()
            .all(|e| e.narrative.template_id == "clicks_down"));
    }

    #[test]
    fn test_query_repeated_ignores_gaps_in_periods() {
        let entries = vec![
            make_test_entry("clicks_down", "warning", Some("2026-01")),
            make_test_entry("clicks_down", "warning", Some("2026-03")), // gap at 02
            make_test_entry("clicks_down", "warning", Some("2026-05")), // gap at 04
        ];
        let query = LedgerQuery {
            repeated: Some(2),
            ..Default::default()
        };
        let result = query_ledger(&entries, &query);
        // No consecutive run of 2 (Jan→Mar has a gap, Mar→May has a gap).
        assert!(result.is_empty(), "gaps should break consecutive runs");
    }

    #[test]
    fn test_consecutive_quarters() {
        assert!(is_consecutive_period("2026-Q1", "2026-Q2"));
        assert!(is_consecutive_period("2026-Q4", "2027-Q1"));
        assert!(!is_consecutive_period("2026-Q1", "2026-Q3"));
    }

    #[test]
    fn test_consecutive_months() {
        assert!(is_consecutive_period("2026-01", "2026-02"));
        assert!(is_consecutive_period("2026-12", "2027-01"));
        assert!(!is_consecutive_period("2026-01", "2026-03"));
    }

    #[test]
    fn test_narratives_to_ledger_entries_conversion() {
        let narratives = vec![NarrativeOutput {
            id: "clicks_down_paid_search".to_string(),
            severity: Severity::Warning,
            text: "Clicks are down 4.1%".to_string(),
            template_id: "clicks_down".to_string(),
            evidence: {
                let mut m = BTreeMap::new();
                m.insert("Clicks".to_string(), serde_json::json!(8420));
                m
            },
        }];

        let scope = {
            let mut m = BTreeMap::new();
            m.insert("channel".to_string(), "Paid_Search".to_string());
            m
        };

        let entries = narratives_to_ledger_entries(
            &narratives,
            "model.yaml",
            "sha256:abc",
            "2026-05-07T10:00:00Z",
            Some("2026-04"),
            &scope,
        );

        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.schema_version, "1.0");
        assert_eq!(e.model, "model.yaml");
        assert_eq!(e.model_hash, "sha256:abc");
        assert_eq!(e.report_period.as_deref(), Some("2026-04"));
        assert_eq!(e.narrative.severity, "warning");
        assert_eq!(e.narrative.template_id, "clicks_down");
        assert_eq!(
            e.scope.get("channel").map(|s| s.as_str()),
            Some("Paid_Search")
        );
    }
}
