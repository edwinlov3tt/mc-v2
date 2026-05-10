//! Workspace-level lint rules (MC5010–MC5012).
//!
//! These are advisory checks that run after validation passes.
//! They detect cross-cube naming drift, measure collisions without
//! explicit links, and missing descriptions.

use std::collections::HashMap;
use std::path::Path;

use crate::diagnostic::{WorkspaceDiagnostic, WorkspaceError};
use crate::resolve;
use crate::schema::ParsedWorkspace;

/// Run workspace-level lint rules. Returns advisory diagnostics.
pub fn lint_workspace(
    workspace: &ParsedWorkspace,
    workspace_dir: &Path,
) -> Vec<WorkspaceDiagnostic> {
    let mut diags = Vec::new();

    // Collect dimension names and measure names per cube.
    let mut cube_dims: HashMap<String, Vec<String>> = HashMap::new();
    let mut cube_measures: HashMap<String, Vec<String>> = HashMap::new();
    let mut cube_descriptions: HashMap<String, Option<String>> = HashMap::new();

    for entry in &workspace.cubes {
        let cube_path = workspace_dir.join(&entry.path);
        let yaml = match std::fs::read_to_string(&cube_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Resolve refs to get the full cube YAML.
        let resolved = if resolve::has_refs(&yaml) {
            match resolve::resolve_refs(&yaml, workspace, workspace_dir) {
                Ok(r) => r,
                Err(_) => continue,
            }
        } else {
            yaml
        };

        let parsed = match mc_model::parse(&resolved, Some(cube_path.display().to_string())) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let cube_name = entry
            .name
            .clone()
            .unwrap_or_else(|| entry.path.display().to_string());

        cube_dims.insert(
            cube_name.clone(),
            parsed.dimensions.iter().map(|d| d.name.clone()).collect(),
        );
        cube_measures.insert(
            cube_name.clone(),
            parsed.measures.iter().map(|m| m.name.clone()).collect(),
        );
        cube_descriptions.insert(cube_name.clone(), parsed.metadata.description.clone());
    }

    // MC5010: Dimension naming drift across cubes.
    // Check if dimensions with the same semantic role use different names.
    let mut dim_name_variants: HashMap<String, Vec<String>> = HashMap::new();
    for (cube, dims) in &cube_dims {
        for dim in dims {
            let normalized = dim.to_lowercase().replace(['-', ' '], "_");
            dim_name_variants
                .entry(normalized)
                .or_default()
                .push(format!("{cube}:{dim}"));
        }
    }
    for (normalized, variants) in &dim_name_variants {
        // Check if there are different original spellings.
        let unique_names: std::collections::HashSet<&str> = variants
            .iter()
            .filter_map(|v| v.split(':').nth(1))
            .collect();
        if unique_names.len() > 1 {
            let names: Vec<&str> = unique_names.into_iter().collect();
            diags.push(WorkspaceDiagnostic::from_error(
                &WorkspaceError::DimensionNamingDrift {
                    message: format!(
                        "dimension concept {:?} has variants: {}",
                        normalized,
                        names.join(", ")
                    ),
                },
            ));
        }
    }

    // MC5011: Measure name collision across cubes without explicit link.
    let linked_measures: std::collections::HashSet<(&str, &str)> = workspace
        .links
        .iter()
        .flat_map(|link| {
            vec![
                (link.from_cube.as_str(), link.from_measure.as_str()),
                (link.to_cube.as_str(), link.to_measure.as_str()),
            ]
        })
        .collect();

    let mut measure_to_cubes: HashMap<&str, Vec<&str>> = HashMap::new();
    for (cube, measures) in &cube_measures {
        for m in measures {
            measure_to_cubes
                .entry(m.as_str())
                .or_default()
                .push(cube.as_str());
        }
    }
    for (measure, cubes) in &measure_to_cubes {
        if cubes.len() > 1 {
            // Check if ALL occurrences are covered by a link.
            let all_linked = cubes
                .iter()
                .all(|cube| linked_measures.contains(&(*cube, *measure)));
            if !all_linked {
                diags.push(WorkspaceDiagnostic::from_error(
                    &WorkspaceError::MeasureNameCollisionNoLink {
                        measure: measure.to_string(),
                        cubes: cubes.iter().map(|s| s.to_string()).collect(),
                    },
                ));
            }
        }
    }

    // MC5012: Cube has no description.
    for (cube, desc) in &cube_descriptions {
        if desc.is_none() {
            diags.push(WorkspaceDiagnostic::from_error(
                &WorkspaceError::CubeNoDescription { cube: cube.clone() },
            ));
        }
    }

    diags
}
