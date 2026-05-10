//! Workspace-level validators (MC5001–MC5008).
//!
//! Runs after manifest parsing and $ref resolution. Checks structural
//! consistency across cubes, links, and shared resources.

use std::collections::HashSet;
use std::path::Path;

use crate::diagnostic::{WorkspaceDiagnostic, WorkspaceError};
use crate::resolve;
use crate::schema::ParsedWorkspace;

/// Validate a workspace: check all cube files exist, $refs resolve,
/// links reference valid cubes/measures, and shared resources are used.
///
/// Returns a list of diagnostics (errors + warnings + info).
/// Errors are hard failures; warnings and info are advisory.
pub fn validate_workspace(
    workspace: &ParsedWorkspace,
    workspace_dir: &Path,
) -> Vec<WorkspaceDiagnostic> {
    let mut diags = Vec::new();

    // Canonicalize workspace_dir for path-containment checks (MC5013).
    // Falls back to the raw path if canonicalize fails (e.g., the dir
    // doesn't exist yet during init).
    let ws_canonical =
        std::fs::canonicalize(workspace_dir).unwrap_or_else(|_| workspace_dir.to_path_buf());

    // MC5009: Detect duplicate cube paths before the main loop.
    {
        let mut seen_paths: HashSet<String> = HashSet::new();
        for entry in &workspace.cubes {
            let key = entry.path.display().to_string();
            if !seen_paths.insert(key.clone()) {
                diags.push(WorkspaceDiagnostic::from_error(
                    &WorkspaceError::DuplicateCubePath { path: key },
                ));
            }
        }
    }

    // MC5002 + MC5013: Check all cube files exist and stay within workspace.
    let mut cube_names: HashSet<String> = HashSet::new();
    let mut cube_measures: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut cube_dimensions: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    for entry in &workspace.cubes {
        let cube_path = workspace_dir.join(&entry.path);
        if !cube_path.exists() {
            diags.push(WorkspaceDiagnostic::from_error(
                &WorkspaceError::CubeFileNotFound {
                    path: entry.path.display().to_string(),
                },
            ));
            continue;
        }

        // MC5013: Verify cube path stays within the workspace directory.
        if let Ok(cube_canonical) = std::fs::canonicalize(&cube_path) {
            if !cube_canonical.starts_with(&ws_canonical) {
                diags.push(WorkspaceDiagnostic::from_error(
                    &WorkspaceError::PathEscapesWorkspace {
                        path: entry.path.display().to_string(),
                    },
                ));
                continue;
            }
        }

        // Read the cube YAML to extract metadata for cross-cube checks.
        let yaml = match std::fs::read_to_string(&cube_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Resolve $refs if present.
        let resolved_yaml = if resolve::has_refs(&yaml) {
            match resolve::resolve_refs(&yaml, workspace, workspace_dir) {
                Ok(r) => r,
                Err(errs) => {
                    for e in &errs {
                        diags.push(WorkspaceDiagnostic::from_error(e));
                    }
                    continue;
                }
            }
        } else {
            yaml.clone()
        };

        // Parse cube to extract measures and dimensions for cross-cube checks.
        let cube_name = entry
            .name
            .clone()
            .unwrap_or_else(|| entry.path.display().to_string());
        cube_names.insert(cube_name.clone());

        // Try to validate through mc-model.
        match mc_model::parse(&resolved_yaml, Some(cube_path.display().to_string())) {
            Ok(parsed) => {
                // Collect measure names.
                let measures: Vec<String> =
                    parsed.measures.iter().map(|m| m.name.clone()).collect();
                cube_measures.insert(cube_name.clone(), measures);

                // Collect dimension names.
                let dims: Vec<String> = parsed.dimensions.iter().map(|d| d.name.clone()).collect();
                cube_dimensions.insert(cube_name.clone(), dims);

                // Try full validation.
                match mc_model::validate(parsed) {
                    Ok(_) => {}
                    Err(errs) => {
                        for e in errs {
                            diags.push(WorkspaceDiagnostic {
                                code: e.code(),
                                severity: crate::diagnostic::Severity::Error,
                                message: format!("cube {cube_name:?}: {e}"),
                            });
                        }
                    }
                }
            }
            Err(e) => {
                diags.push(WorkspaceDiagnostic {
                    code: "MC5001",
                    severity: crate::diagnostic::Severity::Error,
                    message: format!("cube {cube_name:?} parse error: {e}"),
                });
            }
        }
    }

    // MC5004: Check links reference valid cubes and measures.
    for link in &workspace.links {
        if !cube_names.contains(&link.from_cube) {
            diags.push(WorkspaceDiagnostic::from_error(
                &WorkspaceError::LinkReferencesNonexistent {
                    kind: "cube".to_string(),
                    name: link.from_cube.clone(),
                    cube: "(link source)".to_string(),
                },
            ));
        } else if let Some(measures) = cube_measures.get(&link.from_cube) {
            if !measures.contains(&link.from_measure) {
                diags.push(WorkspaceDiagnostic::from_error(
                    &WorkspaceError::LinkReferencesNonexistent {
                        kind: "measure".to_string(),
                        name: link.from_measure.clone(),
                        cube: link.from_cube.clone(),
                    },
                ));
            }
        }

        if !cube_names.contains(&link.to_cube) {
            diags.push(WorkspaceDiagnostic::from_error(
                &WorkspaceError::LinkReferencesNonexistent {
                    kind: "cube".to_string(),
                    name: link.to_cube.clone(),
                    cube: "(link target)".to_string(),
                },
            ));
        } else if let Some(measures) = cube_measures.get(&link.to_cube) {
            if !measures.contains(&link.to_measure) {
                diags.push(WorkspaceDiagnostic::from_error(
                    &WorkspaceError::LinkReferencesNonexistent {
                        kind: "measure".to_string(),
                        name: link.to_measure.clone(),
                        cube: link.to_cube.clone(),
                    },
                ));
            }
        }
    }

    // MC5006: Check for unused shared catalogs.
    // A catalog is "used" if any cube had a $ref pointing to it.
    // For this check, we scan cube YAMLs for $ref directives.
    let mut used_catalog_ids: HashSet<String> = HashSet::new();
    for entry in &workspace.cubes {
        let cube_path = workspace_dir.join(&entry.path);
        if let Ok(yaml) = std::fs::read_to_string(&cube_path) {
            for line in yaml.lines() {
                if let Some(pos) = line.find("workspace://shared_dimensions/") {
                    let after = &line[pos + "workspace://shared_dimensions/".len()..];
                    // Extract the catalog ID (up to the next quote or whitespace).
                    let id: String = after
                        .chars()
                        .take_while(|c| !c.is_whitespace() && *c != '"' && *c != '\'')
                        .collect();
                    if !id.is_empty() {
                        used_catalog_ids.insert(id);
                    }
                }
            }
        }
    }
    for catalog in &workspace.shared_dimensions {
        if !used_catalog_ids.contains(&catalog.id) {
            diags.push(WorkspaceDiagnostic::from_error(
                &WorkspaceError::UnusedSharedCatalog {
                    catalog_id: catalog.id.clone(),
                },
            ));
        }
    }

    // MC5008: Check for golden suites.
    if workspace.golden_suites.is_empty() {
        diags.push(WorkspaceDiagnostic::from_error(
            &WorkspaceError::NoGoldenSuites,
        ));
    }

    diags
}

/// Check whether any diagnostic is an error (severity = Error).
pub fn has_errors(diags: &[WorkspaceDiagnostic]) -> bool {
    diags
        .iter()
        .any(|d| d.severity == crate::diagnostic::Severity::Error)
}
