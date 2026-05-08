//! Workspace routing — per ADR-0019 Session 4.
//!
//! Groups uploaded CSVs by detected tactic (product/subproduct),
//! produces per-tactic cube + narrative bundles, and generates a
//! cross-tactic summary.

use crate::ingest::{self, IdGen, IngestedCube};
use crate::narrative::{self, NarrativeOutput, Severity, TemplateDefinition};
use crate::registry::DetectionResult;
use crate::upload::ParsedCsv;
use serde::Serialize;
use std::collections::BTreeMap;

/// A single tactic group: one product/subproduct with all its CSVs.
#[derive(Debug, Serialize)]
pub struct TacticGroup {
    pub product: String,
    pub subproduct: String,
    pub csv_count: usize,
    pub cubes: Vec<IngestedCube>,
    pub narratives: Vec<NarrativeOutput>,
}

/// Cross-tactic summary for the entire upload.
#[derive(Debug, Serialize)]
pub struct WorkspaceSummary {
    pub advertiser: String,
    pub tactic_count: usize,
    pub total_csvs: usize,
    pub total_cells: usize,
    pub total_narratives: usize,
    pub summary_narratives: Vec<NarrativeOutput>,
}

/// Route CSVs into tactic groups, ingest cubes, and evaluate narratives.
pub fn build_tactic_groups(
    csvs: &[ParsedCsv],
    detections: &[DetectionResult],
    ids: &mut IdGen,
    templates: &[TemplateDefinition],
    benchmark: Option<&mc_narrative::BenchmarkLibrary>,
) -> Vec<TacticGroup> {
    // Group by (product, subproduct).
    let mut groups: BTreeMap<(String, String), Vec<(usize, &ParsedCsv)>> = BTreeMap::new();

    for (i, (csv, detection)) in csvs.iter().zip(detections.iter()).enumerate() {
        if let Some(spec) = &detection.spec {
            let key = (spec.product_name.clone(), spec.subproduct_name.clone());
            groups.entry(key).or_default().push((i, csv));
        }
    }

    let mut tactic_groups = Vec::new();

    for ((product, subproduct), csv_entries) in &groups {
        let mut cubes = Vec::new();
        let csv_count = csv_entries.len();

        for (_, csv) in csv_entries {
            // Find the matching spec for this CSV.
            let spec_idx = csvs.iter().position(|c| std::ptr::eq(*csv, c));
            let spec = spec_idx.and_then(|i| detections[i].spec.as_ref());

            if let Some(spec) = spec {
                match ingest::ingest_csv(spec, csv, ids) {
                    Ok(ingested) => cubes.push(ingested),
                    Err(e) => {
                        eprintln!(
                            "  \x1b[33mwarn\x1b[0m: ingest failed for {}: {e}",
                            csv.filename,
                        );
                    }
                }
            }
        }

        // Evaluate narratives for this tactic's cubes (with benchmark if available).
        let narratives = narrative::evaluate_all_with_benchmark(templates, &cubes, None, benchmark);

        tactic_groups.push(TacticGroup {
            product: product.clone(),
            subproduct: subproduct.clone(),
            csv_count,
            cubes,
            narratives,
        });
    }

    tactic_groups
}

/// Generate a cross-tactic summary.
pub fn build_summary(advertiser: &str, groups: &[TacticGroup]) -> WorkspaceSummary {
    let tactic_count = groups.len();
    let total_csvs: usize = groups.iter().map(|g| g.csv_count).sum();
    let total_cells: usize = groups
        .iter()
        .flat_map(|g| g.cubes.iter())
        .map(|c| c.cells_written)
        .sum();
    let total_narratives: usize = groups.iter().map(|g| g.narratives.len()).sum();

    let mut summary_narratives = Vec::new();

    // Overall summary line.
    if tactic_count > 0 {
        let tactic_names: Vec<&str> = groups.iter().map(|g| g.subproduct.as_str()).collect();
        summary_narratives.push(NarrativeOutput {
            id: "workspace_summary".into(),
            severity: Severity::Info,
            text: format!(
                "{} tactic{} processed across {} CSVs: {}.",
                tactic_count,
                if tactic_count == 1 { "" } else { "s" },
                total_csvs,
                tactic_names.join(", "),
            ),
            template_id: "workspace_overview".into(),
            evidence: BTreeMap::new(),
        });
    }

    // Headline metrics per tactic.
    for group in groups {
        let total_impressions: f64 = group
            .cubes
            .iter()
            .flat_map(|c| c.values.get("Impressions"))
            .flatten()
            .map(|e| e.value)
            .sum();
        let total_clicks: f64 = group
            .cubes
            .iter()
            .flat_map(|c| c.values.get("Clicks"))
            .flatten()
            .map(|e| e.value)
            .sum();
        let avg_ctr = if total_impressions > 0.0 {
            (total_clicks / total_impressions) * 100.0
        } else {
            0.0
        };

        if total_impressions > 0.0 {
            summary_narratives.push(NarrativeOutput {
                id: format!(
                    "tactic_headline_{}",
                    group.subproduct.to_lowercase().replace(' ', "_")
                ),
                severity: Severity::Info,
                text: format!(
                    "{}: {} impressions, {} clicks, {:.2}% CTR.",
                    group.subproduct,
                    fmt_int(total_impressions),
                    fmt_int(total_clicks),
                    avg_ctr,
                ),
                template_id: "tactic_headline".into(),
                evidence: {
                    let mut e = BTreeMap::new();
                    e.insert(
                        "total_impressions".into(),
                        serde_json::json!(total_impressions),
                    );
                    e.insert("total_clicks".into(), serde_json::json!(total_clicks));
                    e.insert("avg_ctr".into(), serde_json::json!(avg_ctr));
                    e
                },
            });
        }
    }

    // Count warnings and critical narratives.
    let warning_count = groups
        .iter()
        .flat_map(|g| g.narratives.iter())
        .filter(|n| matches!(n.severity, Severity::Warning | Severity::Critical))
        .count();
    if warning_count > 0 {
        summary_narratives.push(NarrativeOutput {
            id: "workspace_alerts".into(),
            severity: Severity::Warning,
            text: format!(
                "{} alert{} flagged across all tactics — review items marked in red below.",
                warning_count,
                if warning_count == 1 { "" } else { "s" },
            ),
            template_id: "workspace_alert_count".into(),
            evidence: {
                let mut e = BTreeMap::new();
                e.insert("alert_count".into(), serde_json::json!(warning_count));
                e
            },
        });
    }

    WorkspaceSummary {
        advertiser: advertiser.to_string(),
        tactic_count,
        total_csvs,
        total_cells,
        total_narratives,
        summary_narratives,
    }
}

fn fmt_int(n: f64) -> String {
    let n = n.round() as i64;
    if n.abs() >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n.abs() >= 1_000 {
        let s = n.abs().to_string();
        let mut result = String::new();
        for (i, c) in s.chars().rev().enumerate() {
            if i > 0 && i % 3 == 0 {
                result.push(',');
            }
            result.push(c);
        }
        if n < 0 {
            result.push('-');
        }
        result.chars().rev().collect()
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::CellEntry;

    fn make_group(subproduct: &str, impressions: f64, clicks: f64) -> TacticGroup {
        let mut values = BTreeMap::new();
        values.insert(
            "Impressions".into(),
            vec![CellEntry {
                category: "Aug_2025".into(),
                value: impressions,
            }],
        );
        values.insert(
            "Clicks".into(),
            vec![CellEntry {
                category: "Aug_2025".into(),
                value: clicks,
            }],
        );
        TacticGroup {
            product: "Test".into(),
            subproduct: subproduct.into(),
            csv_count: 1,
            cubes: vec![IngestedCube {
                label: format!("{subproduct} — Monthly"),
                product: "Test".into(),
                subproduct: subproduct.into(),
                table_name: "Monthly Performance".into(),
                source_file: "test.csv".into(),
                dimension_count: 4,
                measure_count: 2,
                cells_written: 2,
                row_count: 1,
                values,
            }],
            narratives: Vec::new(),
        }
    }

    #[test]
    fn test_build_summary_single_tactic() {
        let groups = vec![make_group("Targeted Display", 55757.0, 245.0)];
        let summary = build_summary("Scotts RV", &groups);
        assert_eq!(summary.tactic_count, 1);
        assert_eq!(summary.total_csvs, 1);
        assert!(summary.summary_narratives.len() >= 2); // overview + headline
        assert!(summary.summary_narratives[0].text.contains("1 tactic"));
        assert!(summary.summary_narratives[1].text.contains("55,757"));
    }

    #[test]
    fn test_build_summary_multi_tactic() {
        let groups = vec![
            make_group("Targeted Display", 55757.0, 245.0),
            make_group("STV Hulu RON", 120000.0, 500.0),
        ];
        let summary = build_summary("Scotts RV", &groups);
        assert_eq!(summary.tactic_count, 2);
        assert!(summary.summary_narratives[0].text.contains("2 tactics"));
        assert!(summary.summary_narratives[0]
            .text
            .contains("Targeted Display"));
        assert!(summary.summary_narratives[0].text.contains("STV Hulu RON"));
    }
}
