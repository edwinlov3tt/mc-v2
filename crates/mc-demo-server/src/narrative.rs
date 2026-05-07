//! Narrative template engine — per ADR-0019 Decision 4 / Session 3.
//!
//! Phase 7A.1: extract this module to `crates/mc-narrative`.
//! Templates are currently Rust functions; 7A.1 refactors to YAML-driven.
//! Public boundary: `evaluate_all(cubes) -> Vec<NarrativeOutput>`.
//!
//! Evaluates pre-compiled templates against populated cube data to
//! produce human-readable narrative paragraphs. Templates are defined
//! as Rust functions (Decision 11 optimization #5: pre-compiled at
//! startup, no per-request parsing).
//!
//! Each template has:
//!   - A `family` (display-like, video-like, search-like, social-like)
//!   - A `when` predicate (fires only when the data matches)
//!   - A `render` function that produces text + evidence
//!   - A `severity` (info, warning, critical)

use crate::ingest::{CellEntry, IngestedCube};
use serde::Serialize;
use std::collections::BTreeMap;

/// A rendered narrative paragraph.
#[derive(Debug, Clone, Serialize)]
pub struct NarrativeOutput {
    pub id: String,
    pub severity: Severity,
    pub text: String,
    pub template_id: String,
    pub evidence: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

/// Evaluate all applicable templates against a set of ingested cubes.
/// Returns narratives grouped by source cube.
pub fn evaluate_all(cubes: &[IngestedCube]) -> Vec<NarrativeOutput> {
    let mut narratives = Vec::new();

    // Data sufficiency disclosure — fires first (sort_order: -1).
    // Use the monthly-performance cube if available for period count.
    let monthly_cube = cubes
        .iter()
        .find(|c| c.table_name.to_lowercase().contains("monthly"));
    if let Some(cube) = monthly_cube {
        narratives.extend(eval_data_sufficiency(cube));
    }

    for cube in cubes {
        let table = cube.table_name.to_lowercase();

        // Time-series templates — only for "Monthly Performance" (has Date/Time dim).
        if table.contains("monthly") {
            narratives.extend(eval_time_series(cube));
            // Template 1: engagement velocity vs reach growth.
            narratives.extend(eval_engagement_acceleration(cube));
            // Template 2: industry benchmark comparison.
            narratives.extend(eval_benchmark_comparison(cube));
            // Template 3: uniform momentum detection.
            narratives.extend(eval_uniform_momentum(cube));
        }
        // Device ranking + underperformance alarm.
        if table.contains("device") {
            narratives.extend(eval_device_ranking(cube));
            // Template 5: device underperformance alarm.
            narratives.extend(eval_device_underperformance(cube));
        }
        // Creative ranking — only "creative by name" (human-readable names).
        if table.contains("creative by name") {
            narratives.extend(eval_creative_ranking(cube));
        }
        // Geo concentration + zero-engagement alarm + small-sample warning.
        if table.contains("city") {
            narratives.extend(eval_geo_concentration(cube));
            // Template 4: zero-engagement alarm.
            narratives.extend(eval_zero_engagement(cube));
            // Template 7: small-sample reliability warning.
            narratives.extend(eval_small_sample_warning(cube));
        }
    }

    // Conversion alarm: fire ONCE across all cubes (deduplicate).
    let best_cube = cubes
        .iter()
        .filter(|c| {
            c.values
                .get("Impressions")
                .map(|v| v.iter().map(|e| e.value).sum::<f64>() >= 100.0)
                .unwrap_or(false)
        })
        .max_by(|a, b| {
            let imp_a: f64 = a
                .values
                .get("Impressions")
                .map(|v| v.iter().map(|e| e.value).sum())
                .unwrap_or(0.0);
            let imp_b: f64 = b
                .values
                .get("Impressions")
                .map(|v| v.iter().map(|e| e.value).sum())
                .unwrap_or(0.0);
            imp_a
                .partial_cmp(&imp_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    if let Some(cube) = best_cube {
        narratives.extend(eval_conversion_alarm(cube));
    }

    narratives
}

// ---------------------------------------------------------------------------
// Time-series templates (Monthly Performance)
// ---------------------------------------------------------------------------

fn eval_time_series(cube: &IngestedCube) -> Vec<NarrativeOutput> {
    let mut out = Vec::new();
    let subproduct = &cube.subproduct;

    // Need at least 2 time periods for comparison.
    if let Some(impressions) = cube.values.get("Impressions") {
        if impressions.len() >= 2 {
            let prev = &impressions[impressions.len() - 2];
            let curr = &impressions[impressions.len() - 1];
            if prev.value > 0.0 {
                let pct_change = ((curr.value - prev.value) / prev.value) * 100.0;
                if pct_change.abs() > 5.0 {
                    let direction = if pct_change >= 0.0 {
                        "grew"
                    } else {
                        "declined"
                    };
                    out.push(NarrativeOutput {
                        id: format!("impressions_mom_{}", cube.source_file.replace(".csv", "")),
                        severity: Severity::Info,
                        text: format!(
                            "{subproduct} impressions {direction} {:.0}% from {} ({}) to {} ({}).",
                            pct_change.abs(),
                            readable_name(&prev.category),
                            fmt_int(prev.value),
                            readable_name(&curr.category),
                            fmt_int(curr.value),
                        ),
                        template_id: "impressions_mom_change".into(),
                        evidence: evidence(&[
                            ("prev_impressions", prev.value),
                            ("current_impressions", curr.value),
                            ("pct_change", pct_change),
                        ]),
                    });
                }
            }
        }
    }

    // Clicks trend
    if let Some(clicks) = cube.values.get("Clicks") {
        if clicks.len() >= 2 {
            let prev = &clicks[clicks.len() - 2];
            let curr = &clicks[clicks.len() - 1];
            if prev.value > 0.0 {
                let pct_change = ((curr.value - prev.value) / prev.value) * 100.0;
                if pct_change.abs() > 10.0 {
                    let direction = if pct_change >= 0.0 {
                        "increasing"
                    } else {
                        "decreasing"
                    };
                    let qualifier = if pct_change.abs() > 100.0 {
                        "more than doubled, "
                    } else if pct_change.abs() > 50.0 {
                        "surged, "
                    } else {
                        ""
                    };
                    out.push(NarrativeOutput {
                        id: format!("clicks_mom_{}", cube.source_file.replace(".csv", "")),
                        severity: Severity::Info,
                        text: format!(
                            "Clicks {qualifier}{direction} {:.0}% from {} ({}) to {} ({}).",
                            pct_change.abs(),
                            readable_name(&prev.category),
                            fmt_int(prev.value),
                            readable_name(&curr.category),
                            fmt_int(curr.value),
                        ),
                        template_id: "clicks_mom_change".into(),
                        evidence: evidence(&[
                            ("prev_clicks", prev.value),
                            ("current_clicks", curr.value),
                            ("pct_change", pct_change),
                        ]),
                    });
                }
            }
        }
    }

    // CTR trend
    if let Some(ctr) = cube.values.get("CTR") {
        if ctr.len() >= 2 {
            let prev = &ctr[ctr.len() - 2];
            let curr = &ctr[ctr.len() - 1];
            let direction = if curr.value > prev.value {
                "strengthened"
            } else if curr.value < prev.value {
                "weakened"
            } else {
                "held steady"
            };
            out.push(NarrativeOutput {
                id: format!("ctr_trend_{}", cube.source_file.replace(".csv", "")),
                severity: Severity::Info,
                text: format!(
                    "CTR {direction} from {:.2}% to {:.2}%.",
                    prev.value, curr.value,
                ),
                template_id: "ctr_trend".into(),
                evidence: evidence(&[("prev_ctr", prev.value), ("current_ctr", curr.value)]),
            });
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Device ranking templates
// ---------------------------------------------------------------------------

fn eval_device_ranking(cube: &IngestedCube) -> Vec<NarrativeOutput> {
    let mut out = Vec::new();

    if let Some(ctr_values) = cube.values.get("CTR") {
        if ctr_values.len() >= 2 {
            let mut sorted: Vec<&CellEntry> = ctr_values.iter().collect();
            sorted.sort_by(|a, b| {
                b.value
                    .partial_cmp(&a.value)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let best = sorted[0];
            let worst = sorted[sorted.len() - 1];

            // Calculate campaign average CTR
            let total_impressions: f64 = cube
                .values
                .get("Impressions")
                .map(|v| v.iter().map(|e| e.value).sum())
                .unwrap_or(0.0);
            let total_clicks: f64 = cube
                .values
                .get("Clicks")
                .map(|v| v.iter().map(|e| e.value).sum())
                .unwrap_or(0.0);
            let avg_ctr = if total_impressions > 0.0 {
                (total_clicks / total_impressions) * 100.0
            } else {
                0.0
            };

            let best_name = readable_name(&best.category);
            let worst_name = readable_name(&worst.category);

            let mut text = format!(
                "{} was the top-performing device by engagement: {:.2}% CTR",
                best_name, best.value,
            );
            if avg_ctr > 0.0 && best.value > avg_ctr * 1.5 {
                text.push_str(&format!(
                    " — nearly {:.0}x the campaign average",
                    best.value / avg_ctr,
                ));
            }
            text.push('.');

            if (best.value - worst.value).abs() > 0.1 {
                text.push_str(&format!(
                    " {} underperformed at {:.2}% CTR.",
                    worst_name, worst.value,
                ));
            }

            out.push(NarrativeOutput {
                id: format!("device_ranking_{}", cube.source_file.replace(".csv", "")),
                severity: Severity::Info,
                text,
                template_id: "device_ranking".into(),
                evidence: evidence(&[
                    ("best_device_ctr", best.value),
                    ("worst_device_ctr", worst.value),
                    ("avg_ctr", avg_ctr),
                ]),
            });
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Creative ranking templates
// ---------------------------------------------------------------------------

fn eval_creative_ranking(cube: &IngestedCube) -> Vec<NarrativeOutput> {
    let mut out = Vec::new();

    if let Some(ctr_values) = cube.values.get("CTR") {
        if ctr_values.len() >= 2 {
            let mut sorted: Vec<&CellEntry> = ctr_values.iter().collect();
            sorted.sort_by(|a, b| {
                b.value
                    .partial_cmp(&a.value)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let best = sorted[0];
            let best_name = readable_name(&best.category);

            let impressions_for_best = cube
                .values
                .get("Impressions")
                .and_then(|v| v.iter().find(|e| e.category == best.category))
                .map(|e| e.value)
                .unwrap_or(0.0);

            out.push(NarrativeOutput {
                id: format!("creative_ranking_{}", cube.source_file.replace(".csv", "")),
                severity: Severity::Info,
                text: format!(
                    "Top creative: \"{}\" at {:.2}% CTR across {} impressions.",
                    best_name,
                    best.value,
                    fmt_int(impressions_for_best),
                ),
                template_id: "creative_top_performer".into(),
                evidence: evidence(&[
                    ("best_creative_ctr", best.value),
                    ("best_creative_impressions", impressions_for_best),
                ]),
            });
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Geo concentration templates
// ---------------------------------------------------------------------------

fn eval_geo_concentration(cube: &IngestedCube) -> Vec<NarrativeOutput> {
    let mut out = Vec::new();

    if let Some(impressions) = cube.values.get("Impressions") {
        if impressions.len() >= 2 {
            let total: f64 = impressions.iter().map(|e| e.value).sum();
            if total > 0.0 {
                // Find top location by impression share
                let mut sorted: Vec<&CellEntry> = impressions.iter().collect();
                sorted.sort_by(|a, b| {
                    b.value
                        .partial_cmp(&a.value)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                let top = sorted[0];
                let share = (top.value / total) * 100.0;
                let top_name = readable_name(&top.category);

                if share > 50.0 {
                    out.push(NarrativeOutput {
                        id: format!("geo_concentration_{}", cube.source_file.replace(".csv", "")),
                        severity: if share > 80.0 {
                            Severity::Warning
                        } else {
                            Severity::Info
                        },
                        text: format!(
                            "{} accounts for {:.0}% of total impressions ({} of {}).",
                            top_name,
                            share,
                            fmt_int(top.value),
                            fmt_int(total),
                        ),
                        template_id: "geo_concentration".into(),
                        evidence: evidence(&[
                            ("top_location_share", share),
                            ("top_location_impressions", top.value),
                            ("total_impressions", total),
                        ]),
                    });
                }
            }
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Conversion alarm (fires for zero conversions — the "wow" moment)
// ---------------------------------------------------------------------------

fn eval_conversion_alarm(cube: &IngestedCube) -> Vec<NarrativeOutput> {
    // Look for conversion-related measures
    let conversion_keys: Vec<&String> = cube
        .values
        .keys()
        .filter(|k| {
            let lower = k.to_lowercase();
            lower.contains("conversion") || lower == "conversions"
        })
        .collect();

    if conversion_keys.is_empty() {
        return Vec::new();
    }

    for key in &conversion_keys {
        if let Some(values) = cube.values.get(*key) {
            let total: f64 = values.iter().map(|e| e.value).sum();
            if total > 0.0 {
                // Has conversions — no alarm
                return Vec::new();
            }
        }
    }

    // All conversion measures are zero
    let total_impressions: f64 = cube
        .values
        .get("Impressions")
        .map(|v| v.iter().map(|e| e.value).sum())
        .unwrap_or(0.0);

    // Only alarm if there are significant impressions (not just an empty CSV)
    if total_impressions < 100.0 {
        return Vec::new();
    }

    vec![NarrativeOutput {
        id: format!("zero_conversions_{}", cube.source_file.replace(".csv", "")),
        severity: Severity::Critical,
        text: format!(
            "Zero conversions recorded across {} impressions. Recommend verifying conversion pixel installation.",
            fmt_int(total_impressions),
        ),
        template_id: "zero_conversion_alarm".into(),
        evidence: evidence(&[
            ("total_conversions", 0.0),
            ("total_impressions", total_impressions),
        ]),
    }]
}

// ---------------------------------------------------------------------------
// Template 1 — Engagement Velocity vs Reach Growth
// ---------------------------------------------------------------------------

fn eval_engagement_acceleration(cube: &IngestedCube) -> Vec<NarrativeOutput> {
    let impressions = match cube.values.get("Impressions") {
        Some(v) if v.len() >= 2 => v,
        _ => return Vec::new(),
    };
    let clicks = match cube.values.get("Clicks") {
        Some(v) if v.len() >= 2 => v,
        _ => return Vec::new(),
    };
    let prev_imp = impressions[impressions.len() - 2].value;
    let curr_imp = impressions[impressions.len() - 1].value;
    let prev_clk = clicks[clicks.len() - 2].value;
    let curr_clk = clicks[clicks.len() - 1].value;

    if prev_imp <= 0.0 || prev_clk <= 0.0 {
        return Vec::new();
    }

    let impr_growth = ((curr_imp - prev_imp) / prev_imp) * 100.0;
    let click_growth = ((curr_clk - prev_clk) / prev_clk) * 100.0;

    // Only fire when click growth exceeds impression growth by 1.5x
    if click_growth.abs() <= impr_growth.abs() * 1.5 {
        return Vec::new();
    }

    vec![NarrativeOutput {
        id: format!(
            "engagement_acceleration_{}",
            cube.source_file.replace(".csv", "")
        ),
        severity: Severity::Info,
        text: format!(
            "Engagement is accelerating faster than reach: clicks grew {:.0}% while impressions grew only {:.0}%. The campaign is improving its ability to convert attention into action.",
            click_growth, impr_growth,
        ),
        template_id: "engagement_acceleration".into(),
        evidence: evidence(&[
            ("click_growth_pct", click_growth),
            ("impr_growth_pct", impr_growth),
        ]),
    }]
}

// ---------------------------------------------------------------------------
// Template 2 — Industry Benchmark Comparison
// ---------------------------------------------------------------------------

fn eval_benchmark_comparison(cube: &IngestedCube) -> Vec<NarrativeOutput> {
    let ctr_values = match cube.values.get("CTR") {
        Some(v) if !v.is_empty() => v,
        _ => return Vec::new(),
    };

    // Average CTR across all time periods.
    let sum: f64 = ctr_values.iter().map(|e| e.value).sum();
    let avg_ctr = sum / ctr_values.len() as f64;

    // Industry benchmark for Targeted Display: 0.10% CTR.
    let benchmark = 0.10;
    let multiple = avg_ctr / benchmark;

    let interpretation = if avg_ctr > 0.30 {
        "This significant outperformance indicates strong creative-audience alignment."
    } else if avg_ctr > 0.10 {
        "Performance is above industry norms — targeting appears effective."
    } else {
        "Performance is at or below industry norms — review targeting and creative."
    };

    vec![NarrativeOutput {
        id: format!(
            "ctr_vs_benchmark_{}",
            cube.source_file.replace(".csv", "")
        ),
        severity: Severity::Info,
        text: format!(
            "Campaign CTR of {:.2}% is {:.1}x the industry average for {} ({:.2}%). {interpretation}",
            avg_ctr, multiple, cube.subproduct, benchmark,
        ),
        template_id: "ctr_vs_benchmark".into(),
        evidence: evidence(&[
            ("campaign_ctr", avg_ctr),
            ("benchmark", benchmark),
            ("multiple", multiple),
        ]),
    }]
}

// ---------------------------------------------------------------------------
// Template 3 — Uniform Momentum Detection
// ---------------------------------------------------------------------------

fn eval_uniform_momentum(cube: &IngestedCube) -> Vec<NarrativeOutput> {
    let impressions = match cube.values.get("Impressions") {
        Some(v) if v.len() >= 2 => v,
        _ => return Vec::new(),
    };
    let clicks = match cube.values.get("Clicks") {
        Some(v) if v.len() >= 2 => v,
        _ => return Vec::new(),
    };
    let ctr = match cube.values.get("CTR") {
        Some(v) if v.len() >= 2 => v,
        _ => return Vec::new(),
    };

    let prev_i = impressions[impressions.len() - 2].value;
    let curr_i = impressions[impressions.len() - 1].value;
    let prev_c = clicks[clicks.len() - 2].value;
    let curr_c = clicks[clicks.len() - 1].value;
    let prev_ctr = ctr[ctr.len() - 2].value;
    let curr_ctr = ctr[ctr.len() - 1].value;

    // All three must increase.
    if curr_i <= prev_i || curr_c <= prev_c || curr_ctr <= prev_ctr {
        return Vec::new();
    }
    if prev_i <= 0.0 || prev_c <= 0.0 || prev_ctr <= 0.0 {
        return Vec::new();
    }

    let impr_pct = ((curr_i - prev_i) / prev_i) * 100.0;
    let click_pct = ((curr_c - prev_c) / prev_c) * 100.0;
    let ctr_pct = ((curr_ctr - prev_ctr) / prev_ctr) * 100.0;

    let prev_period = readable_name(&impressions[impressions.len() - 2].category);
    let curr_period = readable_name(&impressions[impressions.len() - 1].category);

    vec![NarrativeOutput {
        id: format!(
            "uniform_momentum_{}",
            cube.source_file.replace(".csv", "")
        ),
        severity: Severity::Info,
        text: format!(
            "All key metrics improved from {prev_period} to {curr_period}: impressions (+{:.0}%), clicks (+{:.0}%), and CTR (+{:.0}%). Uniform positive momentum across reach, engagement, and efficiency indicates the campaign is strengthening — not trading one metric for another.",
            impr_pct, click_pct, ctr_pct,
        ),
        template_id: "uniform_momentum".into(),
        evidence: evidence(&[
            ("impr_pct", impr_pct),
            ("click_pct", click_pct),
            ("ctr_pct", ctr_pct),
        ]),
    }]
}

// ---------------------------------------------------------------------------
// Template 4 — Zero-Engagement Alarm (geo areas with impressions but 0 clicks)
// ---------------------------------------------------------------------------

fn eval_zero_engagement(cube: &IngestedCube) -> Vec<NarrativeOutput> {
    let impressions = match cube.values.get("Impressions") {
        Some(v) => v,
        None => return Vec::new(),
    };
    let clicks = match cube.values.get("Clicks") {
        Some(v) => v,
        None => return Vec::new(),
    };

    let mut out = Vec::new();
    for (i, imp_entry) in impressions.iter().enumerate() {
        let click_val = clicks.get(i).map(|e| e.value).unwrap_or(0.0);
        if click_val < 1.0 && imp_entry.value > 50.0 {
            out.push(NarrativeOutput {
                id: format!(
                    "zero_engagement_{}_{}",
                    readable_name(&imp_entry.category).to_lowercase().replace(' ', "_"),
                    cube.source_file.replace(".csv", ""),
                ),
                severity: Severity::Warning,
                text: format!(
                    "{} received {} impressions with zero clicks. This area is consuming delivery with no engagement signal — evaluate whether geo-targeting includes this area intentionally.",
                    readable_name(&imp_entry.category),
                    fmt_int(imp_entry.value),
                ),
                template_id: "zero_engagement_alarm".into(),
                evidence: evidence(&[
                    ("impressions", imp_entry.value),
                    ("clicks", click_val),
                ]),
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Template 5 — Device Underperformance Alarm
// ---------------------------------------------------------------------------

fn eval_device_underperformance(cube: &IngestedCube) -> Vec<NarrativeOutput> {
    let ctr_values = match cube.values.get("CTR") {
        Some(v) if v.len() >= 2 => v,
        _ => return Vec::new(),
    };
    let impressions = cube.values.get("Impressions");

    let sum_ctr: f64 = ctr_values.iter().map(|e| e.value).sum();
    let avg_ctr = sum_ctr / ctr_values.len() as f64;

    // Find the worst device.
    let worst = ctr_values.iter().min_by(|a, b| {
        a.value
            .partial_cmp(&b.value)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let worst = match worst {
        Some(w) => w,
        None => return Vec::new(),
    };

    // Only fire if worst is < 25% of average (significant underperformance).
    if avg_ctr <= 0.0 || worst.value >= avg_ctr * 0.25 {
        return Vec::new();
    }

    let deficit_pct = (1.0 - worst.value / avg_ctr) * 100.0;
    let worst_impressions = impressions
        .and_then(|v| v.iter().find(|e| e.category == worst.category))
        .map(|e| e.value)
        .unwrap_or(0.0);
    let total_impressions: f64 = impressions
        .map(|v| v.iter().map(|e| e.value).sum())
        .unwrap_or(0.0);
    let share = if total_impressions > 0.0 {
        (worst_impressions / total_impressions) * 100.0
    } else {
        0.0
    };

    vec![NarrativeOutput {
        id: format!(
            "device_underperformance_{}",
            cube.source_file.replace(".csv", "")
        ),
        severity: Severity::Warning,
        text: format!(
            "{} is significantly underperforming at {:.2}% CTR — {:.0}% below the campaign average ({:.2}%). This device served {} impressions ({:.0}% of total) with minimal engagement.",
            readable_name(&worst.category),
            worst.value,
            deficit_pct,
            avg_ctr,
            fmt_int(worst_impressions),
            share,
        ),
        template_id: "device_underperformance".into(),
        evidence: evidence(&[
            ("worst_ctr", worst.value),
            ("avg_ctr", avg_ctr),
            ("deficit_pct", deficit_pct),
            ("worst_impressions", worst_impressions),
            ("worst_share", share),
        ]),
    }]
}

// ---------------------------------------------------------------------------
// Template 6 — Data Sufficiency Disclosure
// ---------------------------------------------------------------------------

fn eval_data_sufficiency(cube: &IngestedCube) -> Vec<NarrativeOutput> {
    // Count distinct time periods from any measure.
    let period_count = cube.values.values().next().map(|v| v.len()).unwrap_or(0);

    let plural = if period_count != 1 { "s" } else { "" };
    let confidence = match period_count {
        0 => return Vec::new(),
        1 => "Single-period snapshot — no trend analysis possible. All comparisons are against industry benchmarks only.",
        2 => "Directional trends are visible but 3+ periods are recommended for statistically confident trend assessment.",
        _ => "Sufficient data for meaningful trend analysis across all metrics.",
    };

    vec![NarrativeOutput {
        id: "data_sufficiency".into(),
        severity: Severity::Info,
        text: format!(
            "This analysis is based on {period_count} reporting period{plural}. {confidence}",
        ),
        template_id: "data_sufficiency".into(),
        evidence: {
            let mut e = BTreeMap::new();
            e.insert("period_count".into(), serde_json::json!(period_count));
            e
        },
    }]
}

// ---------------------------------------------------------------------------
// Template 7 — Small-Sample Reliability Warning
// ---------------------------------------------------------------------------

fn eval_small_sample_warning(cube: &IngestedCube) -> Vec<NarrativeOutput> {
    let impressions = match cube.values.get("Impressions") {
        Some(v) => v,
        None => return Vec::new(),
    };

    let small_areas: Vec<&CellEntry> = impressions
        .iter()
        .filter(|e| e.value < 500.0 && e.value > 0.0)
        .collect();

    if small_areas.is_empty() {
        return Vec::new();
    }

    let count = small_areas.len();
    let plural = if count != 1 { "s" } else { "" };
    let area_list: Vec<String> = small_areas
        .iter()
        .map(|e| readable_name(&e.category))
        .collect();
    let area_str = area_list.join(", ");

    vec![NarrativeOutput {
        id: format!(
            "small_sample_warning_{}",
            cube.source_file.replace(".csv", "")
        ),
        severity: Severity::Warning,
        text: format!(
            "{count} geographic area{plural} had fewer than 500 impressions ({area_str}). CTR values for these areas should be considered directionally indicative only — sample sizes are insufficient for confident performance assessment.",
        ),
        template_id: "small_sample_warning".into(),
        evidence: {
            let mut e = BTreeMap::new();
            e.insert("count".into(), serde_json::json!(count));
            e.insert("areas".into(), serde_json::json!(area_list));
            e
        },
    }]
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fmt_int(n: f64) -> String {
    let n = n.round() as i64;
    if n.abs() >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n.abs() >= 1_000 {
        // Format with comma thousands separator
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

fn readable_name(s: &str) -> String {
    let out = s.replace('_', " ");
    // Collapse multiple spaces into one.
    let mut result = String::with_capacity(out.len());
    let mut prev_space = false;
    for c in out.chars() {
        if c == ' ' {
            if !prev_space {
                result.push(' ');
            }
            prev_space = true;
        } else {
            result.push(c);
            prev_space = false;
        }
    }
    result.trim().to_string()
}

fn evidence(pairs: &[(&str, f64)]) -> BTreeMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cube(table_name: &str, values: BTreeMap<String, Vec<CellEntry>>) -> IngestedCube {
        IngestedCube {
            label: format!("Test — {table_name}"),
            product: "Test".into(),
            subproduct: "Targeted Display".into(),
            table_name: table_name.into(),
            source_file: "test.csv".into(),
            dimension_count: 4,
            measure_count: values.len(),
            cells_written: values.values().map(|v| v.len()).sum(),
            row_count: values.values().next().map(|v| v.len()).unwrap_or(0),
            values,
        }
    }

    #[test]
    fn test_impressions_mom_change() {
        let mut values = BTreeMap::new();
        values.insert(
            "Impressions".into(),
            vec![
                CellEntry {
                    category: "Jul_2025".into(),
                    value: 25102.0,
                },
                CellEntry {
                    category: "Aug_2025".into(),
                    value: 30655.0,
                },
            ],
        );
        values.insert(
            "Clicks".into(),
            vec![
                CellEntry {
                    category: "Jul_2025".into(),
                    value: 79.0,
                },
                CellEntry {
                    category: "Aug_2025".into(),
                    value: 166.0,
                },
            ],
        );
        values.insert(
            "CTR".into(),
            vec![
                CellEntry {
                    category: "Jul_2025".into(),
                    value: 0.31,
                },
                CellEntry {
                    category: "Aug_2025".into(),
                    value: 0.54,
                },
            ],
        );
        let cube = make_cube("Monthly Performance", values);
        let narratives = eval_time_series(&cube);

        // Should have impressions change, clicks change, and CTR trend
        assert!(narratives.len() >= 2);

        let imp = narratives
            .iter()
            .find(|n| n.template_id == "impressions_mom_change");
        assert!(imp.is_some());
        let imp = imp.unwrap();
        assert!(imp.text.contains("grew"));
        assert!(imp.text.contains("22%")); // ~22% change

        let clicks = narratives
            .iter()
            .find(|n| n.template_id == "clicks_mom_change");
        assert!(clicks.is_some());
        assert!(clicks.unwrap().text.contains("more than doubled"));
    }

    #[test]
    fn test_device_ranking() {
        let mut values = BTreeMap::new();
        values.insert(
            "Impressions".into(),
            vec![
                CellEntry {
                    category: "Mobile_Phone".into(),
                    value: 44280.0,
                },
                CellEntry {
                    category: "Tablet".into(),
                    value: 5870.0,
                },
                CellEntry {
                    category: "PC__Desktop_or_Laptop".into(),
                    value: 5607.0,
                },
            ],
        );
        values.insert(
            "Clicks".into(),
            vec![
                CellEntry {
                    category: "Mobile_Phone".into(),
                    value: 192.0,
                },
                CellEntry {
                    category: "Tablet".into(),
                    value: 49.0,
                },
                CellEntry {
                    category: "PC__Desktop_or_Laptop".into(),
                    value: 4.0,
                },
            ],
        );
        values.insert(
            "CTR".into(),
            vec![
                CellEntry {
                    category: "Mobile_Phone".into(),
                    value: 0.43,
                },
                CellEntry {
                    category: "Tablet".into(),
                    value: 0.83,
                },
                CellEntry {
                    category: "PC__Desktop_or_Laptop".into(),
                    value: 0.07,
                },
            ],
        );
        let cube = make_cube("Device Performance", values);
        let narratives = eval_device_ranking(&cube);

        assert_eq!(narratives.len(), 1);
        assert!(narratives[0].text.contains("Tablet"));
        assert!(narratives[0].text.contains("0.83%"));
        assert!(narratives[0].text.contains("PC"));
    }

    #[test]
    fn test_zero_conversion_alarm() {
        let mut values = BTreeMap::new();
        values.insert(
            "Impressions".into(),
            vec![CellEntry {
                category: "Jul_2025".into(),
                value: 25102.0,
            }],
        );
        values.insert(
            "Total_Conversions".into(),
            vec![CellEntry {
                category: "Jul_2025".into(),
                value: 0.0,
            }],
        );
        let cube = make_cube("Monthly Performance", values);
        let narratives = eval_conversion_alarm(&cube);

        assert_eq!(narratives.len(), 1);
        assert_eq!(narratives[0].severity, Severity::Critical);
        assert!(narratives[0].text.contains("Zero conversions"));
        assert!(narratives[0].text.contains("pixel"));
    }

    #[test]
    fn test_geo_concentration() {
        let mut values = BTreeMap::new();
        values.insert(
            "Impressions".into(),
            vec![
                CellEntry {
                    category: "Rockford".into(),
                    value: 45279.0,
                },
                CellEntry {
                    category: "Machesney_Park".into(),
                    value: 4592.0,
                },
                CellEntry {
                    category: "Loves_Park".into(),
                    value: 4468.0,
                },
            ],
        );
        let cube = make_cube("Performance by City", values);
        let narratives = eval_geo_concentration(&cube);

        assert_eq!(narratives.len(), 1);
        assert!(narratives[0].text.contains("Rockford"));
        assert!(narratives[0].text.contains("83%")); // ~83% share
    }

    #[test]
    fn test_fmt_int() {
        assert_eq!(fmt_int(25102.0), "25,102");
        assert_eq!(fmt_int(166.0), "166");
        assert_eq!(fmt_int(1000000.0), "1.0M");
        assert_eq!(fmt_int(0.0), "0");
    }
}
