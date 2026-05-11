//! Cube ingestion pipeline — per ADR-0019 Session 2.
//!
//! Takes detected `TacticSpec` + parsed CSV rows, constructs a Mosaic
//! cube directly via `CubeBuilder` (Decision 11 optimization #2: skip
//! the YAML round-trip), and populates it with values from the CSV.

use crate::registry::TacticSpec;
use crate::upload::ParsedCsv;
use mc_core::{
    AggregationRule, CellCoordinate, CellDataType, Cube, CubeId, Dimension, DimensionId,
    DimensionKind, Element, ElementId, MeasureMeta, MeasureRole, PrincipalId, ScalarValue,
    WriteIntent, WritebackRequest,
};
use serde::Serialize;
use std::collections::BTreeMap;

/// Result of ingesting a single CSV into a cube.
#[derive(Debug, Serialize)]
pub struct IngestedCube {
    /// Human-readable label (e.g., "Targeted Display — Monthly Performance").
    pub label: String,
    /// Product from registry.
    pub product: String,
    /// Sub-product from registry.
    pub subproduct: String,
    /// Table type from registry.
    pub table_name: String,
    /// Source CSV filename.
    pub source_file: String,
    /// Number of dimensions in the cube.
    pub dimension_count: usize,
    /// Number of measures (numeric columns).
    pub measure_count: usize,
    /// Number of cells written.
    pub cells_written: usize,
    /// Total rows in the CSV.
    pub row_count: usize,
    /// Summary of populated values: measure_name → Vec<(category_element, value)>.
    pub values: BTreeMap<String, Vec<CellEntry>>,
}

/// A single cell value for display.
#[derive(Debug, Clone, Serialize)]
pub struct CellEntry {
    pub category: String,
    pub value: f64,
}

/// ID counter for generating unique IDs across cubes in a session.
pub struct IdGen {
    next_cube: u64,
    next_dim: u64,
    next_elem: u64,
}

impl Default for IdGen {
    fn default() -> Self {
        Self {
            next_cube: 1,
            next_dim: 1,
            next_elem: 1,
        }
    }
}

impl IdGen {
    pub fn new() -> Self {
        Self::default()
    }

    fn cube_id(&mut self) -> CubeId {
        let id = CubeId(self.next_cube);
        self.next_cube += 1;
        id
    }

    fn dim_id(&mut self) -> DimensionId {
        let id = DimensionId(self.next_dim);
        self.next_dim += 1;
        id
    }

    fn elem_id(&mut self) -> ElementId {
        let id = ElementId(self.next_elem);
        self.next_elem += 1;
        id
    }
}

/// Canonical name mapping for CSV headers.
///
/// Maps common column name variations to the canonical names that
/// narrative templates reference. This is what makes templates work
/// across Meta, YouTube, SEM, Display, etc. without per-tactic logic.
fn canonical_measure_name(raw_header: &str) -> String {
    let h = raw_header.trim().to_lowercase();
    let canonical = match h.as_str() {
        // Click variations
        "link clicks" | "link click" => "Clicks",
        "clicks" => "Clicks",
        // CTR variations
        "ctr(link click-through rate)" | "ctr (link click-through rate)" => "CTR",
        "ctr(%)" | "ctr" => "CTR",
        // Video metrics
        "vcr(%)" | "vcr" => "VCR",
        "video views" => "VideoViews",
        "100% completion" => "Completions",
        "25% complete" => "Complete25",
        "50% complete" => "Complete50",
        "75% complete" => "Complete75",
        // Conversion variations
        "primary conversions" | "conversions (default)" | "total conversions" => "Conversions",
        "primary conversions rate" | "all conversions rate" | "conversion rate" => "ConversionRate",
        "all conversions" => "AllConversions",
        // Engagement
        "post engagements" | "post engagement" => "Engagements",
        "total leads" | "leads" => "Leads",
        "interactions" => "Interactions",
        // Foot traffic
        "foot traffic visits" | "foot traffic" => "FootTraffic",
        // Reach / frequency
        "reach" => "Reach",
        "frequency" => "Frequency",
        // Cost
        "spend" => "Spend",
        "cpm" => "CPM",
        "cpc" => "CPC",
        "cpa" => "CPA",
        "cpl" => "CPL",
        "cplc" => "CPLC",
        "cpv" => "CPV",
        _ => return sanitize_name(raw_header),
    };
    canonical.to_string()
}

/// Returns true if a column header name refers to a geographic/identity
/// field that should always be treated as text, even if values look numeric.
fn is_forced_text_column(header: &str) -> bool {
    let h = header.trim().to_lowercase();
    h.contains("postal")
        || h.contains("zip")
        || h.contains("code")
        || h.contains("dma")
        || h.contains("fips")
        || h.contains("area code")
}

/// Score a column header for "how specific an identifier is this?"
/// Higher = more specific = better category column.
fn category_specificity(header: &str) -> u8 {
    let h = header.trim().to_lowercase();
    if h.contains("city") {
        return 10;
    }
    if h.contains("zip") || h.contains("postal") {
        return 9;
    }
    if h.contains("metro") {
        return 8;
    }
    if h.contains("campaign") || h.contains("ad set") || h.contains("ad group") {
        return 8;
    }
    if h.contains("creative") || h.contains("keyword") {
        return 8;
    }
    if h.contains("device") {
        return 7;
    }
    if h.contains("tactic") || h.contains("placement") {
        return 7;
    }
    if h.contains("age") || h.contains("gender") {
        return 6;
    }
    if h.contains("dma") || h.contains("market") {
        return 5;
    }
    if h.contains("region") || h.contains("state") {
        return 3;
    }
    if h == "date" || h.contains("week") || h.contains("month") {
        return 2;
    }
    1 // unknown
}

/// Ingest a single CSV into a Mosaic cube.
///
/// Cube shape:
///   - Scenario dim (1 element: "Actual")
///   - Version dim (1 element: "Current")
///   - Category dim (from the most-specific text column)
///   - Measure dim (one element per numeric column, with canonical name aliasing)
///
/// Decision 11 optimization #2: constructs the cube directly via
/// CubeBuilder — no YAML generation, no parsing overhead.
pub fn ingest_csv(
    spec: &TacticSpec,
    csv: &ParsedCsv,
    ids: &mut IdGen,
) -> Result<IngestedCube, String> {
    if csv.headers.is_empty() {
        return Err(format!("{}: no headers", csv.filename));
    }
    if csv.rows.is_empty() {
        return Err(format!("{}: no data rows", csv.filename));
    }

    let cube_id = ids.cube_id();
    let principal = PrincipalId(1);
    let cube_name = format!("{} — {}", spec.subproduct_name, spec.table_name);

    // Identify which columns are numeric (measures) vs categorical.
    // Columns with geo/identity headers (zip, postal code) are forced to text
    // even if their values parse as numbers.
    let mut non_numeric_cols: Vec<usize> = Vec::new();
    let mut measure_cols: Vec<(usize, String)> = Vec::new();
    for (i, header) in csv.headers.iter().enumerate() {
        if is_forced_text_column(header) {
            non_numeric_cols.push(i);
            continue;
        }
        let is_numeric = csv.rows.iter().any(|row| {
            row.get(i)
                .map(|v| parse_numeric(v).is_some())
                .unwrap_or(false)
        });
        if is_numeric {
            // Use canonical name aliasing so templates can reference "Clicks" not "Link_Clicks"
            measure_cols.push((i, canonical_measure_name(header)));
        } else {
            non_numeric_cols.push(i);
        }
    }

    // Pick the best category column: the most specific text column.
    // Prefer City > Zip > Metro > Campaign > Device > Region > Date > first.
    let category_col = non_numeric_cols
        .iter()
        .copied()
        .max_by_key(|&i| category_specificity(&csv.headers[i]))
        .unwrap_or(0);
    let category_header = &csv.headers[category_col];

    if measure_cols.is_empty() {
        return Err(format!("{}: no numeric columns found", csv.filename));
    }

    // Collect unique category values from the first column.
    let category_values: Vec<String> = csv
        .rows
        .iter()
        .filter_map(|row| row.get(category_col).map(|v| sanitize_name(v)))
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>();

    // Deduplicate while preserving order.
    let mut seen = std::collections::HashSet::new();
    let unique_categories: Vec<String> = category_values
        .iter()
        .filter(|v| seen.insert((*v).clone()))
        .cloned()
        .collect();

    if unique_categories.is_empty() {
        return Err(format!("{}: no category values", csv.filename));
    }

    // --- Build dimensions ---

    // 1. Scenario dimension (required by kernel).
    let scenario_dim_id = ids.dim_id();
    let actual_id = ids.elem_id();
    let scenario_dim = Dimension::builder(scenario_dim_id, "Scenario", DimensionKind::Scenario)
        .add_element(Element {
            id: actual_id,
            name: "Actual".to_string(),
            dimension: scenario_dim_id,
            measure_meta: None,
            version_state: None,
            scenario_meta: None,
        })
        .map_err(|e| format!("scenario dim: {e}"))?
        .build()
        .map_err(|e| format!("scenario dim build: {e}"))?;

    // 2. Version dimension (required by kernel).
    let version_dim_id = ids.dim_id();
    let current_id = ids.elem_id();
    let version_dim = Dimension::builder(version_dim_id, "Version", DimensionKind::Version)
        .add_element(Element {
            id: current_id,
            name: "Current".to_string(),
            dimension: version_dim_id,
            measure_meta: None,
            version_state: Some(mc_core::VersionState::Draft),
            scenario_meta: None,
        })
        .map_err(|e| format!("version dim: {e}"))?
        .build()
        .map_err(|e| format!("version dim build: {e}"))?;

    // 3. Category dimension (from first CSV column).
    let cat_dim_id = ids.dim_id();
    let cat_dim_name = dimension_name_for_header(category_header);
    let mut cat_builder = Dimension::builder(cat_dim_id, cat_dim_name, DimensionKind::Standard);
    let mut cat_elem_ids: BTreeMap<String, ElementId> = BTreeMap::new();
    for cat_name in &unique_categories {
        let eid = ids.elem_id();
        cat_builder = cat_builder
            .add_element(Element {
                id: eid,
                name: cat_name.clone(),
                dimension: cat_dim_id,
                measure_meta: None,
                version_state: None,
                scenario_meta: None,
            })
            .map_err(|e| format!("category element '{cat_name}': {e}"))?;
        cat_elem_ids.insert(cat_name.clone(), eid);
    }
    let cat_dim = cat_builder
        .build()
        .map_err(|e| format!("category dim build: {e}"))?;

    // 4. Measure dimension.
    let measure_dim_id = ids.dim_id();
    let mut measure_builder = Dimension::builder(measure_dim_id, "Measure", DimensionKind::Measure);
    let mut measure_elem_ids: BTreeMap<String, ElementId> = BTreeMap::new();
    for (_, mname) in &measure_cols {
        let eid = ids.elem_id();
        measure_builder = measure_builder
            .add_element(Element {
                id: eid,
                name: mname.clone(),
                dimension: measure_dim_id,
                measure_meta: Some(MeasureMeta {
                    dtype: CellDataType::F64,
                    role: MeasureRole::Input,
                    aggregation: AggregationRule::Sum,
                }),
                version_state: None,
                scenario_meta: None,
            })
            .map_err(|e| format!("measure element '{mname}': {e}"))?;
        measure_elem_ids.insert(mname.clone(), eid);
    }
    let measure_dim = measure_builder
        .build()
        .map_err(|e| format!("measure dim build: {e}"))?;

    // --- Build cube ---

    let cube = Cube::builder(cube_id, &cube_name)
        .add_dimension(scenario_dim)
        .add_dimension(version_dim)
        .add_dimension(cat_dim)
        .add_dimension(measure_dim)
        .measure_dimension("Measure")
        .root_principal(principal)
        .build()
        .map_err(|e| format!("cube build: {e}"))?;

    // --- Populate cube ---

    let (cells_written, values) = populate_cube(
        cube,
        cube_id,
        principal,
        actual_id,
        current_id,
        csv,
        category_col,
        &measure_cols,
        &cat_elem_ids,
        &measure_elem_ids,
    )?;

    Ok(IngestedCube {
        label: cube_name,
        product: spec.product_name.clone(),
        subproduct: spec.subproduct_name.clone(),
        table_name: spec.table_name.clone(),
        source_file: csv.filename.clone(),
        dimension_count: 4,
        measure_count: measure_cols.len(),
        cells_written,
        row_count: csv.rows.len(),
        values,
    })
}

/// Write CSV values into the cube and return (cells_written, values_map).
#[allow(clippy::too_many_arguments)]
fn populate_cube(
    mut cube: Cube,
    cube_id: CubeId,
    principal: PrincipalId,
    scenario_id: ElementId,
    version_id: ElementId,
    csv: &ParsedCsv,
    category_col: usize,
    measure_cols: &[(usize, String)],
    cat_elem_ids: &BTreeMap<String, ElementId>,
    measure_elem_ids: &BTreeMap<String, ElementId>,
) -> Result<(usize, BTreeMap<String, Vec<CellEntry>>), String> {
    let mut cells_written = 0usize;
    let mut values: BTreeMap<String, Vec<CellEntry>> = BTreeMap::new();

    for row in &csv.rows {
        let cat_raw = match row.get(category_col) {
            Some(v) => sanitize_name(v),
            None => continue,
        };
        if cat_raw.is_empty() {
            continue;
        }

        let cat_id = match cat_elem_ids.get(&cat_raw) {
            Some(id) => *id,
            None => continue,
        };

        for (col_idx, measure_name) in measure_cols {
            let raw_val = match row.get(*col_idx) {
                Some(v) => v,
                None => continue,
            };

            let num = match parse_numeric(raw_val) {
                Some(n) => n,
                None => continue,
            };

            let measure_id = match measure_elem_ids.get(measure_name) {
                Some(id) => *id,
                None => continue,
            };

            // Coordinate order: [Scenario, Version, Category, Measure]
            let coord =
                CellCoordinate::from_parts(cube_id, [scenario_id, version_id, cat_id, measure_id]);

            match cube.write(WritebackRequest {
                coord,
                new_value: ScalarValue::F64(num),
                principal,
                intent: WriteIntent::Set,
                expected_revision: None,
                now_unix_seconds: 0,
            }) {
                Ok(_) => {
                    cells_written += 1;
                    values
                        .entry(measure_name.clone())
                        .or_default()
                        .push(CellEntry {
                            category: cat_raw.clone(),
                            value: num,
                        });
                }
                Err(e) => {
                    // Non-fatal: log and continue.
                    eprintln!(
                        "  \x1b[33mwarn\x1b[0m: write failed for {cat_raw}/{measure_name}: {e}"
                    );
                }
            }
        }
    }

    Ok((cells_written, values))
}

/// Parse a string as a numeric value, handling commas and percentages.
fn parse_numeric(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // Remove commas (thousands separator) and try parsing.
    let cleaned = s.replace(',', "");
    // Try as-is.
    if let Ok(n) = cleaned.parse::<f64>() {
        if n.is_finite() {
            return Some(n);
        }
    }
    // Handle percentage strings like "0.31%".
    if let Some(pct) = cleaned.strip_suffix('%') {
        if let Ok(n) = pct.parse::<f64>() {
            if n.is_finite() {
                return Some(n);
            }
        }
    }
    None
}

/// Sanitize a string for use as a dimension element name.
/// Replaces spaces and special chars with underscores.
fn sanitize_name(s: &str) -> String {
    let trimmed = s.trim().trim_matches('"');
    if trimmed.is_empty() {
        return String::new();
    }
    // For date columns: convert "07-2025" → "Jul_2025"
    if let Some(date) = try_parse_date(trimmed) {
        return date;
    }
    // General sanitization: replace problematic chars.
    trimmed
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' | '.' => c,
            ' ' | '/' | '\\' | '(' | ')' => '_',
            _ => '_',
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

/// Try to parse a date string like "07-2025" or "08-2025" into "Jul_2025".
fn try_parse_date(s: &str) -> Option<String> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 2 {
        return None;
    }
    let month: u32 = parts[0].parse().ok()?;
    let year: u32 = parts[1].parse().ok()?;
    if !(1..=12).contains(&month) || !(2000..=2100).contains(&year) {
        return None;
    }
    let month_name = match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => return None,
    };
    Some(format!("{month_name}_{year}"))
}

/// Map a CSV header name to a dimension name.
fn dimension_name_for_header(header: &str) -> String {
    let h = header.trim().to_lowercase();
    if h == "date" || h.contains("month") || h.contains("period") {
        "Time".to_string()
    } else if h == "device" || h.contains("device") {
        "Device".to_string()
    } else if h.contains("campaign") {
        "Campaign".to_string()
    } else if h == "city" || h.contains("city") {
        "City".to_string()
    } else if h.contains("zip") || h.contains("postal") {
        "Zip".to_string()
    } else if h.contains("creative") {
        "Creative".to_string()
    } else if h.contains("tactic") {
        "Tactic".to_string()
    } else {
        // Fallback: use the header name itself, sanitized.
        sanitize_name(header)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_numeric() {
        assert!((parse_numeric("25,102").unwrap() - 25102.0).abs() < 1e-9);
        assert!((parse_numeric("0.31").unwrap() - 0.31).abs() < 1e-9);
        assert!((parse_numeric("0.31%").unwrap() - 0.31).abs() < 1e-9);
        assert!((parse_numeric("0").unwrap() - 0.0).abs() < 1e-9);
        assert!(parse_numeric("").is_none());
        assert!(parse_numeric("Scotts RV").is_none());
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("07-2025"), "Jul_2025");
        assert_eq!(sanitize_name("08-2025"), "Aug_2025");
        assert_eq!(sanitize_name("Mobile Phone"), "Mobile_Phone");
        assert_eq!(
            sanitize_name("PC (Desktop or Laptop)"),
            "PC__Desktop_or_Laptop"
        );
    }

    #[test]
    fn test_dimension_name_for_header() {
        assert_eq!(dimension_name_for_header("Date"), "Time");
        assert_eq!(dimension_name_for_header("Device"), "Device");
        assert_eq!(dimension_name_for_header("Campaign Name"), "Campaign");
        assert_eq!(dimension_name_for_header("City"), "City");
    }

    #[test]
    fn test_ingest_monthly_performance() {
        let spec = TacticSpec {
            product_name: "Blended Tactics".into(),
            subproduct_name: "Targeted Display".into(),
            table_name: "Monthly Performance".into(),
            file_name: "report-targeteddisplay-monthly-performance".into(),
            headers: vec!["Date".into(), "Impressions".into(), "Clicks".into()],
            description: String::new(),
            is_required: true,
            sort_order: 0,
        };
        let csv = ParsedCsv {
            filename: "report-targeteddisplay-monthly-performance.csv".into(),
            headers: vec![
                "Date".into(),
                "Impressions".into(),
                "Clicks".into(),
                "CTR(%)".into(),
                "Total Conversions".into(),
            ],
            rows: vec![
                vec![
                    "07-2025".into(),
                    "25,102".into(),
                    "79".into(),
                    "0.31".into(),
                    "0".into(),
                ],
                vec![
                    "08-2025".into(),
                    "30,655".into(),
                    "166".into(),
                    "0.54".into(),
                    "0".into(),
                ],
            ],
        };
        let mut ids = IdGen::new();
        let result = ingest_csv(&spec, &csv, &mut ids).unwrap();
        assert_eq!(result.label, "Targeted Display — Monthly Performance");
        assert_eq!(result.dimension_count, 4);
        assert_eq!(result.measure_count, 4); // Impressions, Clicks, CTR(%), Total Conversions
        assert_eq!(result.row_count, 2);
        assert_eq!(result.cells_written, 8); // 2 rows × 4 measures
                                             // Check specific values.
        let impressions = &result.values["Impressions"];
        assert_eq!(impressions.len(), 2);
        assert!((impressions[0].value - 25102.0).abs() < 1e-9);
        assert_eq!(impressions[0].category, "Jul_2025");
        assert!((impressions[1].value - 30655.0).abs() < 1e-9);
        assert_eq!(impressions[1].category, "Aug_2025");
    }
}
