//! YAML deserialization for workspace.yaml and org.yaml manifests.

use std::path::Path;

use crate::diagnostic::WorkspaceError;
use crate::schema::{ParsedOrg, ParsedWorkspace};

/// Parse a `workspace.yaml` from a directory path. Looks for
/// `workspace.yaml` in the given directory.
pub fn parse_workspace(workspace_dir: &Path) -> Result<ParsedWorkspace, WorkspaceError> {
    let manifest_path = workspace_dir.join("workspace.yaml");
    let yaml = std::fs::read_to_string(&manifest_path).map_err(|e| {
        WorkspaceError::ManifestParseFailure {
            message: format!("could not read {}: {e}", manifest_path.display()),
        }
    })?;
    parse_workspace_str(&yaml)
}

/// Parse a `workspace.yaml` from an in-memory string.
pub fn parse_workspace_str(yaml: &str) -> Result<ParsedWorkspace, WorkspaceError> {
    serde_yaml::from_str(yaml).map_err(|e| WorkspaceError::ManifestParseFailure {
        message: format!("workspace.yaml parse error: {e}"),
    })
}

/// Parse an `org.yaml` from a directory path. Looks for `org.yaml`
/// in the given directory.
pub fn parse_org(org_dir: &Path) -> Result<ParsedOrg, WorkspaceError> {
    let manifest_path = org_dir.join("org.yaml");
    let yaml = std::fs::read_to_string(&manifest_path).map_err(|e| {
        WorkspaceError::ManifestParseFailure {
            message: format!("could not read {}: {e}", manifest_path.display()),
        }
    })?;
    parse_org_str(&yaml)
}

/// Parse an `org.yaml` from an in-memory string.
pub fn parse_org_str(yaml: &str) -> Result<ParsedOrg, WorkspaceError> {
    serde_yaml::from_str(yaml).map_err(|e| WorkspaceError::ManifestParseFailure {
        message: format!("org.yaml parse error: {e}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_workspace() {
        let yaml = r#"
workspace_format_version: 1
name: "test-workspace"
id: "test-ws"
cubes:
  - path: "cubes/test.yaml"
"#;
        let ws = parse_workspace_str(yaml).unwrap();
        assert_eq!(ws.name, "test-workspace");
        assert_eq!(ws.id, "test-ws");
        assert_eq!(ws.workspace_format_version, 1);
        assert_eq!(ws.cubes.len(), 1);
    }

    #[test]
    fn parse_full_workspace() {
        let yaml = r#"
workspace_format_version: 1
name: "acme-marketing"
id: "acme-mktg"
description: "Acme marketing finance workspace"
domain: "marketing-mix"
org_id: "acme-org"
shared_dimensions:
  - id: "Channel"
    source: "catalogs/channels.yaml"
  - id: "Market"
    source: "catalogs/markets.yaml"
shared_fitted_models:
  - id: "lasso_v3"
    source: "fitted/lasso-v3.yaml"
cubes:
  - path: "cubes/marketing-finance.yaml"
    name: "Marketing Finance"
links:
  - from_cube: "marketing-finance"
    from_measure: "Revenue"
    to_cube: "brand-awareness"
    to_measure: "Revenue_Ref"
    kind: ReadOnly
    description: "Brand cube references marketing revenue"
golden_suites:
  - "goldens/pipeline.golden.yaml"
"#;
        let ws = parse_workspace_str(yaml).unwrap();
        assert_eq!(ws.shared_dimensions.len(), 2);
        assert_eq!(ws.shared_fitted_models.len(), 1);
        assert_eq!(ws.links.len(), 1);
        assert_eq!(ws.golden_suites.len(), 1);
        assert_eq!(
            ws.description.as_deref(),
            Some("Acme marketing finance workspace")
        );
    }

    #[test]
    fn parse_minimal_org() {
        let yaml = r#"
org_format_version: 1
name: "Acme Corp"
id: "acme"
"#;
        let org = parse_org_str(yaml).unwrap();
        assert_eq!(org.name, "Acme Corp");
        assert_eq!(org.id, "acme");
        assert!(org.workspaces.is_empty());
    }

    #[test]
    fn parse_full_org() {
        let yaml = r#"
org_format_version: 1
name: "Acme Corp"
id: "acme"
description: "Acme Corporation"
installed_cartridges:
  - name: "marketing-mix"
    version: "1.0"
org_templates_path: "templates/"
org_benchmarks_path: "benchmarks/"
workspaces:
  - path: "workspaces/marketing"
    name: "Marketing"
  - path: "workspaces/finance"
    name: "Finance"
"#;
        let org = parse_org_str(yaml).unwrap();
        assert_eq!(org.workspaces.len(), 2);
        assert_eq!(org.installed_cartridges.len(), 1);
    }

    #[test]
    fn parse_invalid_yaml_returns_mc5001() {
        let yaml = "not: valid: yaml: [";
        let err = parse_workspace_str(yaml).unwrap_err();
        assert_eq!(err.code(), "MC5001");
    }
}
