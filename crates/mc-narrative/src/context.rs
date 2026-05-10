//! Context builder — constructs the evaluation context from cube data.
//!
//! Session 2 upgrade: the context builder now stores dimension metadata
//! and the evaluator accesses cube data directly for aggregate functions
//! (count_where, any_where, etc.) instead of pre-computing specific
//! conditions. Finding #1 (generic aggregates) and Finding #5 (dimension
//! names from cube data) are closed by this change.

use crate::evaluator::{Ctx, Val};
use crate::renderer::readable_name;

/// A single data entry for narrative evaluation (measure value at a category element).
#[derive(Debug, Clone)]
pub struct CellEntry {
    /// Category element name (e.g., "Jul_2025", "Rockford", "Mobile_Phone").
    pub category: String,
    /// Numeric value at this category.
    pub value: f64,
}

/// Input data for the narrative engine — represents one ingested data source.
///
/// Session 2: added `dimension_name` for Finding #5 (dimension names from
/// cube data, not heuristic guessing).
#[derive(Debug, Clone)]
pub struct CubeData {
    /// Table type name (e.g., "Monthly Performance", "Performance by City").
    pub table_name: String,
    /// Tactic / subproduct name (e.g., "Targeted Display").
    pub subproduct: String,
    /// Source file identifier (for narrative ID generation).
    pub source_file: String,
    /// The name of the category dimension (e.g., "City", "Device", "Creative").
    /// Finding #5: read from `Cube::dimensions()` instead of guessing from table name.
    /// If `None`, falls back to heuristic inference from `table_name`.
    pub dimension_name: Option<String>,
    /// Per-measure data: measure_name → Vec<CellEntry>.
    pub values: std::collections::BTreeMap<String, Vec<CellEntry>>,
}

/// Build a flat evaluation context from cube data.
///
/// Pre-computes scalar values the evaluator can reference:
/// `current.<Measure>`, `prev.<Measure>`, `sum.<Measure>`, etc.
///
/// Session 2: aggregate functions (count_where, any_where, etc.) are NO
/// LONGER pre-computed here. They are evaluated generically at eval time
/// by the cube-aware evaluator. This closes Finding #1.
pub fn build_context(cube: &CubeData) -> Ctx {
    let mut ctx = Ctx::new();

    // Tactic metadata.
    ctx.insert("tactic_name".into(), Val::Str(cube.subproduct.clone()));
    ctx.insert("table_name".into(), Val::Str(cube.table_name.clone()));

    // Resolve dimension name: use explicit name if set, else infer from table.
    let dim_name = cube
        .dimension_name
        .as_deref()
        .unwrap_or_else(|| infer_dimension_name(&cube.table_name));
    ctx.insert("_dim_name".into(), Val::Str(dim_name.to_string()));

    // Period info: count time-series entries (from first measure).
    let period_count = cube.values.values().next().map(|v| v.len()).unwrap_or(0);
    ctx.insert("period_count".into(), Val::Num(period_count as f64));

    // Per-measure aggregates.
    // Sort entries chronologically to ensure "current" = latest period and
    // "prev" = second-latest, regardless of source row order (PPTX tables
    // may be newest-first while CSVs are typically oldest-first).
    for (measure, entries) in &cube.values {
        let mut sorted: Vec<&CellEntry> = entries.iter().collect();
        sorted.sort_by(|a, b| date_sort_key(&a.category).cmp(&date_sort_key(&b.category)));

        let n = sorted.len();
        if n == 0 {
            continue;
        }

        // current (last = latest chronologically) and prev (second-to-last).
        let current = sorted[n - 1].value;
        let prev = if n >= 2 { sorted[n - 2].value } else { 0.0 };
        ctx.insert(format!("current.{measure}"), Val::Num(current));
        ctx.insert(format!("prev.{measure}"), Val::Num(prev));

        // Period names: set once (first measure defines them).
        if !ctx.contains_key("current.period_name") {
            ctx.insert(
                "current.period_name".into(),
                Val::Str(readable_name(&sorted[n - 1].category)),
            );
            ctx.insert(
                "current_period".into(),
                Val::Str(readable_name(&sorted[n - 1].category)),
            );
            if n >= 2 {
                ctx.insert(
                    "prev.period_name".into(),
                    Val::Str(readable_name(&sorted[n - 2].category)),
                );
                ctx.insert(
                    "prev_period".into(),
                    Val::Str(readable_name(&sorted[n - 2].category)),
                );
            }
        }

        // Sum and average.
        let sum: f64 = entries.iter().map(|e| e.value).sum();
        let avg = sum / n as f64;
        ctx.insert(format!("sum.{measure}"), Val::Num(sum));
        ctx.insert(format!("campaign_avg.{measure}"), Val::Num(avg));

        // Conversions alias.
        if measure.to_lowercase().contains("conversion") {
            ctx.insert("sum.Conversions".into(), Val::Num(sum));
        }

        // Element count for the category dimension + per-measure.
        ctx.insert(format!("element_count({dim_name})"), Val::Num(n as f64));
        ctx.insert(format!("element_count({measure})"), Val::Num(n as f64));
        // Also alias common dim references for backward compat.
        ctx.insert("element_count(geo_dimension)".into(), Val::Num(n as f64));
        ctx.insert(
            "element_count(geo)".into(),
            Val::Num(cube.values.values().next().map(|v| v.len()).unwrap_or(0) as f64),
        );

        // max_by / min_by across the category dimension.
        if let Some(max_entry) = entries.iter().max_by(|a, b| {
            a.value
                .partial_cmp(&b.value)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            ctx.insert(
                format!("max_by.{dim_name}.{measure}.name"),
                Val::Str(readable_name(&max_entry.category)),
            );
            ctx.insert(
                format!("max_by.{dim_name}.{measure}.value"),
                Val::Num(max_entry.value),
            );
        }
        if let Some(min_entry) = entries.iter().min_by(|a, b| {
            a.value
                .partial_cmp(&b.value)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            ctx.insert(
                format!("min_by.{dim_name}.{measure}.name"),
                Val::Str(readable_name(&min_entry.category)),
            );
            ctx.insert(
                format!("min_by.{dim_name}.{measure}.value"),
                Val::Num(min_entry.value),
            );
        }
    }

    // Pre-computed growth rates (if we have 2+ periods).
    if let (Some(Val::Num(cur_i)), Some(Val::Num(prev_i))) =
        (ctx.get("current.Impressions"), ctx.get("prev.Impressions"))
    {
        if *prev_i > 0.0 {
            ctx.insert(
                "impr_growth".into(),
                Val::Num((cur_i - prev_i) / prev_i * 100.0),
            );
        }
    }
    if let (Some(Val::Num(cur_c)), Some(Val::Num(prev_c))) =
        (ctx.get("current.Clicks"), ctx.get("prev.Clicks"))
    {
        if *prev_c > 0.0 {
            ctx.insert(
                "click_growth".into(),
                Val::Num((cur_c - prev_c) / prev_c * 100.0),
            );
        }
    }

    ctx
}

/// Infer the category dimension name from the table name.
///
/// Fallback heuristic used when `CubeData.dimension_name` is `None`.
/// Finding #5 recommends using `Cube::dimensions()` directly — this
/// is the backward-compatible path for demo server data.
fn infer_dimension_name(table_name: &str) -> &'static str {
    let lower = table_name.to_lowercase();
    if lower.contains("city") || lower.contains("zip") {
        "geo"
    } else if lower.contains("device") {
        "Device"
    } else if lower.contains("creative") {
        "Creative"
    } else {
        "Category"
    }
}

/// Convert a sanitized date category (e.g., "Jul_2025", "Aug_2025",
/// "01-2026", "05-2026") to a sortable numeric key so entries sort
/// chronologically regardless of source row order.
///
/// Handles: "Mon_YYYY" (sanitized), "MM-YYYY" (raw), "MM/YYYY",
/// "YYYY-MM", "YYYY-MM-DD", "WW/DD/YYYY" (weekly). Falls back to
/// the original string for non-date categories (Device, City, etc.),
/// which preserves insertion order.
fn date_sort_key(category: &str) -> String {
    // Try "Mon_YYYY" (sanitized dates like "Jul_2025")
    let month_names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    if category.len() >= 8 {
        for (i, name) in month_names.iter().enumerate() {
            if category.starts_with(name) {
                if let Some(year_str) = category.get(4..) {
                    let year = year_str.trim_start_matches('_');
                    if year.len() == 4 && year.chars().all(|c| c.is_ascii_digit()) {
                        return format!("{year}-{:02}", i + 1);
                    }
                }
            }
        }
    }

    // Try "MM-YYYY" or "MM/YYYY" (raw dates like "05-2026")
    if category.len() >= 7 {
        let sep = if category.contains('-') {
            '-'
        } else if category.contains('/') {
            '/'
        } else {
            '\0'
        };
        if sep != '\0' {
            let parts: Vec<&str> = category.splitn(2, sep).collect();
            if parts.len() == 2 {
                let (a, b) = (parts[0], parts[1]);
                // MM-YYYY
                if a.len() <= 2 && b.len() == 4 {
                    if let (Ok(month), Ok(_year)) = (a.parse::<u32>(), b.parse::<u32>()) {
                        if (1..=12).contains(&month) {
                            return format!("{b}-{month:02}");
                        }
                    }
                }
                // YYYY-MM
                if a.len() == 4 && b.len() <= 2 {
                    if let (Ok(_year), Ok(month)) = (a.parse::<u32>(), b.parse::<u32>()) {
                        if (1..=12).contains(&month) {
                            return format!("{a}-{month:02}");
                        }
                    }
                }
            }
        }
    }

    // Non-date categories: return as-is (preserves insertion order).
    category.to_string()
}

/// Get the resolved dimension name for a cube data source.
pub fn resolved_dim_name(cube: &CubeData) -> String {
    cube.dimension_name
        .clone()
        .unwrap_or_else(|| infer_dimension_name(&cube.table_name).to_string())
}
