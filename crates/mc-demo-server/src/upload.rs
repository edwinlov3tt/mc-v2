//! Upload handler — per ADR-0019 Decision 2.
//!
//! POST /api/upload: accepts multipart form data with a zip file,
//! extracts in-memory (Decision 11 optimization #1), matches each
//! CSV against the registry, and returns detection results.

use crate::ingest::IdGen;
use crate::registry::{DetectionResult, Registry};
use crate::timing::PipelineTimer;
use crate::workspace::{self, TacticGroup, WorkspaceSummary};
use serde::Serialize;
use std::io::Cursor;

/// Shared application state passed to handlers.
pub struct AppState {
    pub registry: Registry,
}

/// Response from POST /api/upload.
#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub schema_version: &'static str,
    pub processing_time_ms: f64,
    pub timing: crate::timing::PipelineTiming,
    pub csv_count: usize,
    pub tactic_count: usize,
    pub label: String,
    pub detections: Vec<DetectionResult>,
    /// Per-tactic groups with cubes + narratives (Session 4).
    pub tactics: Vec<TacticGroup>,
    /// Cross-tactic summary (Session 4).
    pub summary: WorkspaceSummary,
}

/// A parsed CSV from the zip archive.
#[derive(Debug, Clone)]
pub struct ParsedCsv {
    pub filename: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Extract a zip file in memory and parse all CSVs.
/// Per Decision 11 optimization #1: no temp files written to disk.
pub fn extract_zip(bytes: &[u8]) -> Result<Vec<ParsedCsv>, String> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| format!("invalid zip file: {e}"))?;

    let mut csvs = Vec::new();
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("zip entry {i}: {e}"))?;

        let name = file.name().to_string();
        // Skip directories, non-CSV files, and macOS resource fork metadata.
        if name.ends_with('/')
            || !name.to_lowercase().ends_with(".csv")
            || name.starts_with("__MACOSX")
            || name.contains("/.__")
            || name.contains("/._")
        {
            continue;
        }

        // Read the CSV content
        let mut content = String::new();
        use std::io::Read;
        file.read_to_string(&mut content)
            .map_err(|e| format!("reading {name}: {e}"))?;

        // Parse the CSV
        let filename = std::path::Path::new(&name)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or(name);

        if let Some(csv) = parse_csv_content(&filename, &content) {
            csvs.push(csv);
        }
    }

    Ok(csvs)
}

/// Parse CSV content into headers + rows.
fn parse_csv_content(filename: &str, content: &str) -> Option<ParsedCsv> {
    let mut lines = content.lines();
    let header_line = lines.next()?;
    if header_line.trim().is_empty() {
        return None;
    }

    let headers: Vec<String> = split_csv_line(header_line);
    let mut rows = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        rows.push(split_csv_line(line));
    }

    Some(ParsedCsv {
        filename: filename.to_string(),
        headers,
        rows,
    })
}

/// Split a CSV line into fields, handling quoted strings.
fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in line.chars() {
        if in_quotes {
            if ch == '"' {
                in_quotes = false;
            } else {
                current.push(ch);
            }
        } else if ch == '"' {
            in_quotes = true;
        } else if ch == ',' {
            fields.push(current.trim().to_string());
            current.clear();
        } else {
            current.push(ch);
        }
    }
    fields.push(current.trim().to_string());
    fields
}

/// Run the detection pipeline: match each CSV against the registry.
pub fn detect_tactics(registry: &Registry, csvs: &[ParsedCsv]) -> Vec<DetectionResult> {
    let mut results = Vec::with_capacity(csvs.len());

    for csv in csvs {
        match registry.detect(&csv.filename) {
            Some(spec) => {
                let (pct, missing, extra) = Registry::match_headers(spec, &csv.headers);
                results.push(DetectionResult {
                    filename: csv.filename.clone(),
                    matched: true,
                    spec: Some(spec.clone()),
                    header_match_pct: pct,
                    missing_headers: missing,
                    extra_headers: extra,
                });
            }
            None => {
                results.push(DetectionResult {
                    filename: csv.filename.clone(),
                    matched: false,
                    spec: None,
                    header_match_pct: 0.0,
                    missing_headers: Vec::new(),
                    extra_headers: csv.headers.clone(),
                });
            }
        }
    }

    results
}

/// Derive a label from the detected CSVs (advertiser name or first filename).
pub fn derive_label(csvs: &[ParsedCsv]) -> String {
    // Try to find an advertiser name from a campaign-performance CSV.
    for csv in csvs {
        if csv.filename.contains("campaign-performance") {
            if let Some(row) = csv.rows.first() {
                if let Some(name) = row.first() {
                    // Extract advertiser from campaign name like "Scotts RV Truck and Auto Repair_Primary_AAT-DISP"
                    if let Some(idx) = name.find('_') {
                        return name[..idx].to_string();
                    }
                    if !name.is_empty() {
                        return name.clone();
                    }
                }
            }
        }
    }
    // Fallback: use first CSV filename
    csvs.first()
        .map(|c| c.filename.replace(".csv", ""))
        .unwrap_or_else(|| "Unknown".to_string())
}

/// Count unique tactics detected.
pub fn count_unique_tactics(detections: &[DetectionResult]) -> usize {
    let mut products = std::collections::HashSet::new();
    for d in detections {
        if let Some(spec) = &d.spec {
            products.insert(format!("{}/{}", spec.product_name, spec.subproduct_name));
        }
    }
    products.len()
}

/// Process a full upload: extract zip → detect → build response.
pub fn process_upload(registry: &Registry, bytes: &[u8]) -> Result<UploadResponse, String> {
    let mut timer = PipelineTimer::start();

    // Extract zip in memory (Decision 11 optimization #1)
    let csvs = extract_zip(bytes)?;

    // Detect tactics against registry
    let detections = detect_tactics(registry, &csvs);
    timer.mark_registry_done();

    // Build tactic groups: route CSVs by product/subproduct, ingest
    // cubes, evaluate per-tactic narratives (Session 4).
    let mut ids = IdGen::new();
    let tactics = workspace::build_tactic_groups(&csvs, &detections, &mut ids);
    timer.mark_compile_done();

    // Populate is part of ingest (same step for the demo).
    timer.mark_populate_done();

    // Build cross-tactic summary with headline narratives.
    let label = derive_label(&csvs);
    let summary = workspace::build_summary(&label, &tactics);
    timer.mark_narrative_done();

    let csv_count = csvs.len();
    let tactic_count = count_unique_tactics(&detections);

    // Serialize (the timer captures this stage too)
    timer.mark_serialize_done();

    let timing = timer.finish();
    let processing_time_ms = timing.total_ms();

    // Print timing to terminal
    timer.print_to_terminal(&label, csv_count, tactic_count);

    Ok(UploadResponse {
        schema_version: "1.0",
        processing_time_ms,
        timing,
        csv_count,
        tactic_count,
        label,
        detections,
        tactics,
        summary,
    })
}
