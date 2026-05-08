//! Registry parser — per ADR-0019 Decision 3.
//!
//! Parses `performance_tables.csv` into a lookup structure at startup.
//! Each row maps a `file_name` to its product, sub-product, table type,
//! and expected headers.

use serde::Serialize;
use std::collections::HashMap;

/// A single row from the performance_tables registry.
#[derive(Debug, Clone, Serialize)]
pub struct TacticSpec {
    pub product_name: String,
    pub subproduct_name: String,
    pub table_name: String,
    pub file_name: String,
    pub headers: Vec<String>,
    pub description: String,
    pub is_required: bool,
    pub sort_order: u32,
}

/// Pre-warmed registry: filename → TacticSpec.
/// Built once at startup per Decision 11 optimization #3.
#[derive(Debug, Clone)]
pub struct Registry {
    /// Lookup by exact filename (without .csv extension).
    by_filename: HashMap<String, TacticSpec>,
    /// All specs in original order, for the GET /api/registry endpoint.
    all: Vec<TacticSpec>,
}

/// Result of matching a CSV filename against the registry.
#[derive(Debug, Clone, Serialize)]
pub struct DetectionResult {
    pub filename: String,
    pub matched: bool,
    pub spec: Option<TacticSpec>,
    pub header_match_pct: f64,
    pub missing_headers: Vec<String>,
    pub extra_headers: Vec<String>,
}

impl Registry {
    /// Parse the performance_tables.csv content into a Registry.
    pub fn from_csv(csv_content: &str) -> Result<Self, String> {
        let mut by_filename = HashMap::new();
        let mut all = Vec::new();

        let mut lines = csv_content.lines();
        // Skip header row
        let header_line = lines.next().ok_or("empty CSV")?;
        // Validate expected columns
        if !header_line.starts_with("product_name,") {
            return Err(format!("unexpected header row: {header_line}"));
        }

        for (line_num, line) in lines.enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            match parse_csv_row(line) {
                Ok(spec) => {
                    by_filename.insert(spec.file_name.clone(), spec.clone());
                    all.push(spec);
                }
                Err(e) => {
                    return Err(format!("line {}: {e}", line_num + 2));
                }
            }
        }

        Ok(Registry { by_filename, all })
    }

    /// Load from a file path.
    pub fn from_file(path: &str) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read registry file {path}: {e}"))?;
        Self::from_csv(&content)
    }

    /// Look up a spec by CSV filename.
    ///
    /// Strips the `.csv` extension and tries an exact match first,
    /// then falls back to prefix matching.
    pub fn detect(&self, filename: &str) -> Option<&TacticSpec> {
        let name = filename
            .strip_suffix(".csv")
            .unwrap_or(filename)
            .to_lowercase();

        // Exact match
        if let Some(spec) = self.by_filename.get(&name) {
            return Some(spec);
        }

        // Prefix match: find the longest registry filename that matches
        // as a prefix of the uploaded filename (or vice versa).
        let mut best: Option<(&TacticSpec, usize)> = None;
        for spec in &self.all {
            let reg_name = spec.file_name.to_lowercase();
            if name.starts_with(&reg_name) || reg_name.starts_with(&name) {
                let overlap = reg_name.len().min(name.len());
                if best.map_or(true, |(_, prev)| overlap > prev) {
                    best = Some((spec, overlap));
                }
            }
        }
        best.map(|(spec, _)| spec)
    }

    /// Match a CSV's actual headers against a spec's expected headers.
    /// Returns (match_percentage, missing_headers, extra_headers).
    pub fn match_headers(
        spec: &TacticSpec,
        actual_headers: &[String],
    ) -> (f64, Vec<String>, Vec<String>) {
        let expected: Vec<String> = spec.headers.iter().map(|h| normalize_header(h)).collect();
        let actual: Vec<String> = actual_headers.iter().map(|h| normalize_header(h)).collect();

        let mut matched = 0usize;
        let mut missing = Vec::new();
        for exp in &expected {
            if actual.iter().any(|a| a == exp) {
                matched += 1;
            } else {
                missing.push(exp.clone());
            }
        }

        let extra: Vec<String> = actual
            .iter()
            .filter(|a| !expected.iter().any(|e| e == *a))
            .cloned()
            .collect();

        let pct = if expected.is_empty() {
            0.0
        } else {
            (matched as f64 / expected.len() as f64) * 100.0
        };

        (pct, missing, extra)
    }

    /// Find the best-matching spec by header overlap.
    ///
    /// Used as a fallback when filename-based detection fails (e.g., for PPTX
    /// tables where filenames are derived from slide titles, not registry names).
    /// Returns the spec with the highest header match percentage, provided it
    /// exceeds `min_match_pct` (0-100).
    pub fn detect_by_headers(
        &self,
        actual_headers: &[String],
        min_match_pct: f64,
    ) -> Option<&TacticSpec> {
        let mut best: Option<(&TacticSpec, f64)> = None;

        for spec in &self.all {
            let (pct, _, _) = Self::match_headers(spec, actual_headers);
            if pct >= min_match_pct
                && best.map_or(true, |(_, prev_pct)| {
                    pct > prev_pct
                        || (pct == prev_pct && spec.headers.len() > best.unwrap().0.headers.len())
                })
            {
                best = Some((spec, pct));
            }
        }

        best.map(|(spec, _)| spec)
    }

    /// All specs (for GET /api/registry).
    pub fn all_specs(&self) -> &[TacticSpec] {
        &self.all
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.all.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
    }
}

/// Normalize a header name for comparison: lowercase, trim, strip special chars.
fn normalize_header(h: &str) -> String {
    h.trim()
        .to_lowercase()
        .replace(['(', ')', '%'], "")
        .trim()
        .to_string()
}

/// Parse a single CSV row respecting quoted fields.
fn parse_csv_row(line: &str) -> Result<TacticSpec, String> {
    let fields = parse_csv_fields(line);
    if fields.len() < 8 {
        return Err(format!(
            "expected 8 fields, got {}: {:?}",
            fields.len(),
            line
        ));
    }

    let headers: Vec<String> = fields[4]
        .split(';')
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty())
        .collect();

    let is_required = fields[6].eq_ignore_ascii_case("true");
    let sort_order = fields[7].trim().parse::<u32>().unwrap_or(0);

    Ok(TacticSpec {
        product_name: fields[0].clone(),
        subproduct_name: fields[1].clone(),
        table_name: fields[2].clone(),
        file_name: fields[3].clone(),
        headers,
        description: fields[5].clone(),
        is_required,
        sort_order,
    })
}

/// Parse CSV fields handling quoted strings with commas.
fn parse_csv_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    // Escaped quote
                    chars.next();
                    current.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                current.push(ch);
            }
        } else if ch == '"' {
            in_quotes = true;
        } else if ch == ',' {
            fields.push(current.clone());
            current.clear();
        } else {
            current.push(ch);
        }
    }
    fields.push(current);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_csv_fields() {
        let line = r#""STV","Hulu - RON","Monthly Performance","report-hulu-ron-monthly-performance","Date; Impressions; Video Views","",TRUE,0"#;
        let fields = parse_csv_fields(line);
        assert_eq!(fields[0], "STV");
        assert_eq!(fields[1], "Hulu - RON");
        assert_eq!(fields[3], "report-hulu-ron-monthly-performance");
        assert_eq!(fields[4], "Date; Impressions; Video Views");
    }

    #[test]
    fn test_registry_detect_exact() {
        let csv = "product_name,subproduct_name,table_name,file_name,headers,description,is_required,sort_order\n\
                   \"Blended Tactics\",\"Targeted Display\",\"Monthly Performance\",\"report-targeteddisplay-monthly-performance\",\"Date; Impressions; Clicks\",\"\",TRUE,0";
        let reg = Registry::from_csv(csv).unwrap();
        let spec = reg.detect("report-targeteddisplay-monthly-performance.csv");
        assert!(spec.is_some());
        assert_eq!(spec.unwrap().subproduct_name, "Targeted Display");
    }

    #[test]
    fn test_header_matching() {
        let spec = TacticSpec {
            product_name: "Test".into(),
            subproduct_name: "Test".into(),
            table_name: "Test".into(),
            file_name: "test".into(),
            headers: vec![
                "Date".into(),
                "Impressions".into(),
                "Clicks".into(),
                "CTR".into(),
            ],
            description: String::new(),
            is_required: true,
            sort_order: 0,
        };
        let actual = vec![
            "Date".to_string(),
            "Impressions".to_string(),
            "Clicks".to_string(),
            "CTR(%)".to_string(),
            "Total Conversions".to_string(),
        ];
        let (pct, missing, extra) = Registry::match_headers(&spec, &actual);
        assert!(pct > 99.0); // All 4 expected headers match (CTR normalizes)
        assert!(missing.is_empty());
        assert_eq!(extra.len(), 1); // "total conversions"
    }
}
