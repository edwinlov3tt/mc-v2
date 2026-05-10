//! Workspace scaffolding — `mc workspace init`.

use std::path::Path;

/// Scaffold a new workspace directory with a `workspace.yaml` template.
pub fn init_workspace(name: &str, dir: &Path, domain: Option<&str>) -> Result<(), String> {
    // Create directory structure.
    std::fs::create_dir_all(dir.join("cubes"))
        .map_err(|e| format!("failed to create cubes/: {e}"))?;
    std::fs::create_dir_all(dir.join("catalogs"))
        .map_err(|e| format!("failed to create catalogs/: {e}"))?;
    std::fs::create_dir_all(dir.join("fixtures"))
        .map_err(|e| format!("failed to create fixtures/: {e}"))?;

    // Generate workspace.yaml.
    let domain_line = match domain {
        Some(d) => format!("domain: {d:?}\n"),
        None => String::new(),
    };

    let yaml = format!(
        r#"workspace_format_version: 1
name: {name:?}
id: {name:?}
{domain_line}
# Shared dimension catalogs — referenced by cubes via $ref.
shared_dimensions: []

# Shared fitted models.
shared_fitted_models: []

# Cubes participating in this workspace.
cubes: []

# Inter-cube links (declarative — validated at workspace-validate time).
links: []

# Workspace-level golden suites.
golden_suites: []
"#
    );

    std::fs::write(dir.join("workspace.yaml"), yaml)
        .map_err(|e| format!("failed to write workspace.yaml: {e}"))?;

    Ok(())
}
