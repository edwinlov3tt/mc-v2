//! Narrative engine bridge — thin wrapper around `mc-narrative`.
//!
//! Phase 7A.1: the real engine lives in `crates/mc-narrative`.
//! This module re-exports types and provides conversion from the
//! demo server's `IngestedCube` to `mc_narrative::CubeData`.

use crate::ingest::IngestedCube;

// Re-export types that the rest of the demo server uses.
pub use mc_narrative::{NarrativeOutput, Severity, TemplateDefinition};

/// Load all template definitions from YAML files in a directory.
pub fn load_templates(dir: &str) -> Vec<TemplateDefinition> {
    mc_narrative::load_templates(dir)
}

/// Evaluate all applicable templates against a set of ingested cubes.
///
/// Converts `IngestedCube` → `mc_narrative::CubeData` and delegates
/// to the narrative engine.
pub fn evaluate_all(
    templates: &[TemplateDefinition],
    cubes: &[IngestedCube],
) -> Vec<NarrativeOutput> {
    let cube_data: Vec<mc_narrative::CubeData> = cubes.iter().map(convert_cube).collect();
    mc_narrative::evaluate_all(templates, &cube_data)
}

/// Convert an `IngestedCube` (demo server type) to `CubeData` (narrative engine type).
fn convert_cube(cube: &IngestedCube) -> mc_narrative::CubeData {
    let values = cube
        .values
        .iter()
        .map(|(measure, entries)| {
            let converted = entries
                .iter()
                .map(|e| mc_narrative::CellEntry {
                    category: e.category.clone(),
                    value: e.value,
                })
                .collect();
            (measure.clone(), converted)
        })
        .collect();

    mc_narrative::CubeData {
        table_name: cube.table_name.clone(),
        subproduct: cube.subproduct.clone(),
        source_file: cube.source_file.clone(),
        dimension_name: None, // Demo server uses heuristic inference.
        values,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_templates() {
        let templates = {
            let t = load_templates("demo/narratives");
            if t.is_empty() {
                load_templates("../../demo/narratives")
            } else {
                t
            }
        };
        assert!(!templates.is_empty(), "should load at least 1 template");
        assert!(
            templates.len() >= 13,
            "expected >= 13 templates, got {}",
            templates.len()
        );
    }
}
