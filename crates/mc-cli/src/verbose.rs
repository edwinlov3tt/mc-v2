//! Shared helpers for `--verbose` / `-v` mode across Phase 6A CLI verbs.
//!
//! Verbose mode enriches text output with prose descriptions from the
//! model's `measures[].description` field. JSON and CSV output are
//! unaffected. Phase 4D scope — no new crates, no kernel changes.

use std::collections::HashMap;

/// Format a verbose description line for a measure.
///
/// If `description` contains `{value}`, it is replaced with the
/// formatted cell value. Returns a 2-space-indented line with a
/// trailing newline, ready to append to text output.
pub fn format_description_line(description: &str, formatted_value: Option<&str>) -> String {
    let desc = if let Some(val) = formatted_value {
        description.replace("{value}", val)
    } else {
        description.replace("{value}", "")
    };
    format!("  {desc}\n")
}

/// Look up a measure's description from the descriptions map.
/// Returns `None` if the measure has no description (graceful
/// degradation per Phase 4D acceptance criterion 4).
pub fn measure_description<'a>(
    descriptions: &'a HashMap<String, String>,
    measure_name: &str,
) -> Option<&'a str> {
    descriptions.get(measure_name).map(|s| s.as_str())
}

/// Extract the measure name from a canonical coord string
/// (`"Scenario=X,Version=Y,...,Measure=Z"`).
pub fn measure_name_from_coord(coord_str: &str) -> Option<&str> {
    for part in coord_str.split(',') {
        let part = part.trim();
        if let Some((key, value)) = part.split_once('=') {
            if key.trim() == "Measure" {
                return Some(value.trim());
            }
        }
    }
    None
}
