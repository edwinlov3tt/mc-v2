//! `$ref` resolution — compile-time bake of shared resources into cube YAML.
//!
//! When a cube's dimension contains `$ref: "workspace://shared_dimensions/Channel"`,
//! the resolver:
//! 1. Looks up `Channel` in `workspace.shared_dimensions`
//! 2. Reads the catalog source file
//! 3. Produces a modified YAML string with the catalog's elements and hierarchy
//!    inlined into the dimension
//!
//! This is a compile-time bake — the resolved YAML is passed to
//! `mc_model::load_str()`. No hot-reload.

use std::collections::HashMap;
use std::path::Path;

use crate::diagnostic::WorkspaceError;
use crate::schema::{DimensionCatalog, ParsedWorkspace};

/// The `$ref` URI prefix for workspace-scoped references.
const WORKSPACE_REF_PREFIX: &str = "workspace://shared_dimensions/";

/// Scan a cube YAML string for `$ref: "workspace://..."` directives
/// and inline the referenced catalog's elements and hierarchy.
///
/// Returns the modified YAML string ready for `mc_model::load_str()`.
pub fn resolve_refs(
    cube_yaml: &str,
    workspace: &ParsedWorkspace,
    workspace_dir: &Path,
) -> Result<String, Vec<WorkspaceError>> {
    // Build a lookup from catalog ID → source path.
    let catalog_index: HashMap<&str, &Path> = workspace
        .shared_dimensions
        .iter()
        .map(|c| (c.id.as_str(), c.source.as_path()))
        .collect();

    let mut errors = Vec::new();

    // Simple line-based resolution: find lines with `$ref:` and replace
    // the containing dimension block's elements with the catalog's.
    // We parse the YAML as a serde_yaml::Value for precise manipulation.
    let mut doc: serde_yaml::Value = serde_yaml::from_str(cube_yaml).map_err(|e| {
        vec![WorkspaceError::ManifestParseFailure {
            message: format!("cube YAML parse error during $ref resolution: {e}"),
        }]
    })?;

    let dimensions = match doc.get_mut("dimensions") {
        Some(serde_yaml::Value::Sequence(seq)) => seq,
        _ => {
            // No dimensions block — nothing to resolve.
            return Ok(cube_yaml.to_string());
        }
    };

    for dim in dimensions.iter_mut() {
        let dim_map = match dim.as_mapping_mut() {
            Some(m) => m,
            None => continue,
        };

        // Check for $ref field.
        let ref_key = serde_yaml::Value::String("$ref".to_string());
        let ref_val = match dim_map.get(&ref_key) {
            Some(serde_yaml::Value::String(s)) => s.clone(),
            _ => continue,
        };

        // Parse the $ref URI.
        if !ref_val.starts_with(WORKSPACE_REF_PREFIX) {
            errors.push(WorkspaceError::RefTargetNotFound {
                ref_id: ref_val.clone(),
                cube: "(unknown)".to_string(),
            });
            continue;
        }
        let catalog_id = &ref_val[WORKSPACE_REF_PREFIX.len()..];

        // Look up the catalog source.
        let catalog_source = match catalog_index.get(catalog_id) {
            Some(p) => *p,
            None => {
                errors.push(WorkspaceError::RefTargetNotFound {
                    ref_id: catalog_id.to_string(),
                    cube: dim_map
                        .get(&serde_yaml::Value::String("name".to_string()))
                        .and_then(|v| v.as_str())
                        .unwrap_or("(unknown)")
                        .to_string(),
                });
                continue;
            }
        };

        // Read and parse the catalog file.
        let catalog_path = workspace_dir.join(catalog_source);

        // MC5013: Verify catalog path stays within the workspace directory.
        if let (Ok(cat_canon), Ok(ws_canon)) = (
            std::fs::canonicalize(&catalog_path),
            std::fs::canonicalize(workspace_dir),
        ) {
            if !cat_canon.starts_with(&ws_canon) {
                errors.push(WorkspaceError::PathEscapesWorkspace {
                    path: catalog_source.display().to_string(),
                });
                continue;
            }
        }

        let catalog_yaml = match std::fs::read_to_string(&catalog_path) {
            Ok(s) => s,
            Err(e) => {
                errors.push(WorkspaceError::RefTargetNotFound {
                    ref_id: format!("{catalog_id} (file read error: {e})"),
                    cube: "(file)".to_string(),
                });
                continue;
            }
        };
        let catalog: DimensionCatalog = match serde_yaml::from_str(&catalog_yaml) {
            Ok(c) => c,
            Err(e) => {
                errors.push(WorkspaceError::ManifestParseFailure {
                    message: format!("catalog {catalog_id:?} parse error: {e}"),
                });
                continue;
            }
        };

        // Remove the $ref key.
        dim_map.remove(&ref_key);

        // Build the elements sequence from the catalog.
        let elements: Vec<serde_yaml::Value> = catalog
            .elements
            .iter()
            .map(|elem| {
                let mut map = serde_yaml::Mapping::new();
                map.insert(
                    serde_yaml::Value::String("name".to_string()),
                    serde_yaml::Value::String(elem.name.clone()),
                );
                if let Some(ref d) = elem.date {
                    map.insert(
                        serde_yaml::Value::String("date".to_string()),
                        serde_yaml::Value::String(d.clone()),
                    );
                }
                if let Some(ref ps) = elem.period_start {
                    map.insert(
                        serde_yaml::Value::String("period_start".to_string()),
                        serde_yaml::Value::String(ps.clone()),
                    );
                }
                if let Some(ref pe) = elem.period_end_exclusive {
                    map.insert(
                        serde_yaml::Value::String("period_end_exclusive".to_string()),
                        serde_yaml::Value::String(pe.clone()),
                    );
                }
                serde_yaml::Value::Mapping(map)
            })
            .collect();

        // Replace/insert elements.
        let elements_key = serde_yaml::Value::String("elements".to_string());
        dim_map.insert(elements_key, serde_yaml::Value::Sequence(elements));
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    // Serialize back to YAML.
    let output = serde_yaml::to_string(&doc).map_err(|e| {
        vec![WorkspaceError::ManifestParseFailure {
            message: format!("failed to serialize resolved YAML: {e}"),
        }]
    })?;

    Ok(output)
}

/// Check whether a YAML string contains any `$ref:` directives.
pub fn has_refs(yaml: &str) -> bool {
    yaml.contains("$ref:")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{CubeEntry, SharedCatalog};
    use std::path::PathBuf;

    fn minimal_workspace(catalogs: Vec<SharedCatalog>) -> ParsedWorkspace {
        ParsedWorkspace {
            workspace_format_version: 1,
            name: "test".to_string(),
            id: "test".to_string(),
            description: None,
            domain: None,
            org_id: None,
            shared_dimensions: catalogs,
            shared_fitted_models: vec![],
            shared_calibration_maps: vec![],
            shared_lookup_tables: vec![],
            cubes: vec![CubeEntry {
                path: PathBuf::from("cubes/test.yaml"),
                name: None,
            }],
            links: vec![],
            golden_suites: vec![],
        }
    }

    #[test]
    fn has_refs_detects_ref_directives() {
        assert!(has_refs(
            "  $ref: \"workspace://shared_dimensions/Channel\""
        ));
        assert!(!has_refs("  name: Channel"));
    }

    #[test]
    fn resolve_refs_no_refs_passthrough() {
        let yaml = r#"
model_format_version: 1
metadata:
  name: "Test"
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    elements:
      - name: "Baseline"
measures:
  - name: "Spend"
    role: "Input"
    data_type: "F64"
    aggregation: "Sum"
"#;
        let ws = minimal_workspace(vec![]);
        let result = resolve_refs(yaml, &ws, Path::new("/tmp")).unwrap();
        // Should parse back without error.
        assert!(result.contains("Scenario"));
    }

    #[test]
    fn resolve_refs_missing_catalog_returns_mc5003() {
        let yaml = r#"
model_format_version: 1
metadata:
  name: "Test"
dimensions:
  - name: "Channel"
    kind: "Standard"
    $ref: "workspace://shared_dimensions/NoSuchCatalog"
measures:
  - name: "Spend"
    role: "Input"
    data_type: "F64"
    aggregation: "Sum"
"#;
        let ws = minimal_workspace(vec![]);
        let errs = resolve_refs(yaml, &ws, Path::new("/tmp")).unwrap_err();
        assert_eq!(errs[0].code(), "MC5003");
    }
}
