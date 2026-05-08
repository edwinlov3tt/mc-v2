//! Upload handler — per ADR-0019 Decision 2.
//!
//! POST /api/upload: accepts multipart form data with a zip file,
//! extracts in-memory (Decision 11 optimization #1), matches each
//! CSV against the registry, and returns detection results.

use crate::ingest::IdGen;
use crate::narrative::TemplateDefinition;
use crate::registry::{DetectionResult, Registry};
use crate::timing::PipelineTimer;
use crate::workspace::{self, TacticGroup, WorkspaceSummary};
use serde::Serialize;
use std::io::Cursor;

/// Shared application state passed to handlers.
pub struct AppState {
    pub registry: Registry,
    pub templates: Vec<TemplateDefinition>,
    /// Phase 7A.4: workspace-local benchmark library (loaded at startup if present).
    pub benchmark_library: Option<mc_narrative::BenchmarkLibrary>,
    /// Pre-computed IDF weights for PPTX cascade matcher (ADR-0023 Decision 9).
    /// Built once at startup, shared via Arc.
    pub idf_table: std::sync::Arc<crate::pptx_match::IdfTable>,
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
///
/// Tries filename-based matching first, then falls back to header-based
/// matching (useful for PPTX tables where filenames are derived from slide
/// titles rather than registry naming conventions).
pub fn detect_tactics(registry: &Registry, csvs: &[ParsedCsv]) -> Vec<DetectionResult> {
    let mut results = Vec::with_capacity(csvs.len());

    for csv in csvs {
        // Try filename match first.
        let spec = registry
            .detect(&csv.filename)
            // Fallback: match by headers (≥60% overlap).
            .or_else(|| registry.detect_by_headers(&csv.headers, 60.0));

        match spec {
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

/// Process a full upload: extract zip/pptx → detect → build response.
///
/// Detects whether the uploaded file is a PPTX (PowerPoint) or a ZIP of CSVs,
/// and routes to the appropriate extractor. Both produce `Vec<ParsedCsv>` so
/// the rest of the pipeline (registry matching, cube construction, narratives)
/// works unchanged.
pub fn process_upload(
    registry: &Registry,
    templates: &[TemplateDefinition],
    bytes: &[u8],
    benchmark: Option<&mc_narrative::BenchmarkLibrary>,
    idf_table: &crate::pptx_match::IdfTable,
) -> Result<UploadResponse, String> {
    let mut timer = PipelineTimer::start();

    // Detect file type: PPTX or ZIP-of-CSVs, then extract accordingly.
    // PPTX uses the cascade matcher (ADR-0023); CSV zip uses the existing path.
    let csvs = if crate::pptx::is_pptx(bytes) {
        let tables = crate::pptx::extract_pptx_tables(bytes)?;
        let slide_infos = crate::pptx::extract_slide_infos(bytes)?;
        let cwd = std::env::current_dir().unwrap_or_default();
        // Try loading profile from demo/sample-data first, then cwd.
        let profile_dirs = [std::path::Path::new("demo/sample-data"), cwd.as_path()];
        let profile = profile_dirs
            .iter()
            .find_map(|d| crate::pptx_profile::load_profile(d, "lumina-charts"));

        let deck_result = crate::pptx_match::match_deck_with_slides(
            &tables,
            &slide_infos,
            registry,
            profile.as_ref(),
            idf_table,
        );

        // Print diagnostic summary to terminal.
        eprintln!(
            "  [pptx] {} tables: {} matched, {} skipped, {} review, {} dup, {} unmatched",
            deck_result.stats.total_tables,
            deck_result.stats.auto_resolved,
            deck_result.stats.skipped,
            deck_result.stats.review_needed,
            deck_result.stats.duplicates,
            deck_result.stats.unmatched,
        );
        for w in &deck_result.coverage_warnings {
            eprintln!("  [pptx] {w}");
        }

        pptx_matches_to_csvs(&deck_result.matches)
    } else {
        extract_zip(bytes)?
    };

    // Detect tactics against registry
    let detections = detect_tactics(registry, &csvs);
    timer.mark_registry_done();

    // Build tactic groups: route CSVs by product/subproduct, ingest
    // cubes, evaluate per-tactic narratives (Session 4).
    let mut ids = IdGen::new();
    let tactics =
        workspace::build_tactic_groups(&csvs, &detections, &mut ids, templates, benchmark);
    timer.mark_compile_done();

    // Populate is part of ingest (same step for the demo).
    timer.mark_populate_done();

    // Build cross-tactic summary with headline narratives.
    let label = derive_label(&csvs);
    let summary = workspace::build_summary(&label, &tactics);
    timer.mark_narrative_done();

    // Phase 7A.2: auto-write ledger entries for all narratives.
    write_demo_ledger(&label, &tactics);

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

/// Phase 7A.2: write all narrative outputs from this upload to the
/// interpretation ledger. Uses the current working directory as the
/// workspace root.
/// Convert PPTX cascade match results into `ParsedCsv` for the existing pipeline.
///
/// **INVARIANT (ADR-0023 Decision 8):** only `status = Matched` results pass through.
/// Debug builds assert this.
fn pptx_matches_to_csvs(results: &[crate::pptx_match::MatchResult]) -> Vec<ParsedCsv> {
    use crate::pptx_match::{sanitize_for_filename, MatchStatus};

    let matched: Vec<&crate::pptx_match::MatchResult> = results
        .iter()
        .filter(|r| r.status == MatchStatus::Matched)
        .collect();

    // Debug-assert the ingestion invariant.
    debug_assert!(
        matched.iter().all(|r| r.status == MatchStatus::Matched),
        "ingestion invariant violated: non-Matched result in pipeline"
    );

    matched
        .into_iter()
        .filter_map(|r| {
            let mapping = r.mapping.as_ref()?;
            let filename = format!(
                "report-{}-{}.csv",
                sanitize_for_filename(&mapping.subproduct_name),
                sanitize_for_filename(&mapping.table_name),
            );
            Some(ParsedCsv {
                filename,
                headers: r.table.headers.clone(),
                rows: r.table.rows.clone(),
            })
        })
        .collect()
}

fn write_demo_ledger(advertiser: &str, tactics: &[TacticGroup]) {
    use mc_narrative::ledger;
    use std::collections::BTreeMap;

    // Collect all narratives across all tactic groups.
    let all_narratives: Vec<&crate::narrative::NarrativeOutput> =
        tactics.iter().flat_map(|g| g.narratives.iter()).collect();

    if all_narratives.is_empty() {
        return;
    }

    let generated_at = {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        format!("{secs}")
    };

    let model_hash = ledger::compute_hash_from_bytes(b"demo-upload");

    let mut scope = BTreeMap::new();
    scope.insert("advertiser".to_string(), advertiser.to_string());

    let entries: Vec<ledger::LedgerEntry> = all_narratives
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let entry_id = ledger::generate_entry_id(&generated_at, &model_hash, i);
            ledger::LedgerEntry {
                schema_version: ledger::LEDGER_SCHEMA_VERSION.to_string(),
                ledger_entry_id: entry_id,
                generated_at: generated_at.clone(),
                model: "demo-upload".to_string(),
                model_hash: model_hash.clone(),
                report_period: None,
                scope: scope.clone(),
                narrative: ledger::NarrativeRecord {
                    id: n.template_id.clone(),
                    section: None,
                    severity: match n.severity {
                        crate::narrative::Severity::Info => "info",
                        crate::narrative::Severity::Success => "success",
                        crate::narrative::Severity::Warning => "warning",
                        crate::narrative::Severity::Critical => "critical",
                        _ => "info",
                    }
                    .to_string(),
                    text: n.text.clone(),
                    template_id: n.template_id.clone(),
                    notability_score: None,
                    finding_id: None,
                    skipped_explanations: Vec::new(),
                    rejected_explanations: Vec::new(),
                },
                evidence: n.evidence.clone(),
                benchmarks_referenced: Vec::new(),
            }
        })
        .collect();

    let cwd = std::path::Path::new(".");
    match ledger::write_ledger_entries(cwd, &entries) {
        Ok(path) => {
            eprintln!(
                "  [ledger] Wrote {} entries to {}",
                entries.len(),
                path.display()
            );
        }
        Err(e) => {
            eprintln!("  [ledger] warning: write failed: {e}");
        }
    }
}
