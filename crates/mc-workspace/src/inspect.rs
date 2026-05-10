//! Workspace inspector — produces text and JSON summaries.

use std::path::Path;

use crate::diagnostic::WorkspaceDiagnostic;
use crate::resolve;
use crate::schema::ParsedWorkspace;

/// Summary of a workspace for display.
#[derive(Clone, Debug)]
pub struct WorkspaceSummary {
    pub name: String,
    pub id: String,
    pub description: Option<String>,
    pub domain: Option<String>,
    pub cube_count: usize,
    pub cubes: Vec<CubeSummary>,
    pub shared_dimension_count: usize,
    pub shared_dimensions: Vec<String>,
    pub link_count: usize,
    pub links: Vec<LinkSummary>,
    pub golden_suite_count: usize,
}

/// Summary of one cube within the workspace.
#[derive(Clone, Debug)]
pub struct CubeSummary {
    pub name: String,
    pub path: String,
    pub dimension_count: usize,
    pub measure_count: usize,
    pub rule_count: usize,
    pub description: Option<String>,
}

/// Summary of one inter-cube link.
#[derive(Clone, Debug)]
pub struct LinkSummary {
    pub from: String,
    pub to: String,
    pub description: Option<String>,
}

/// Build a workspace summary by scanning all cubes.
pub fn inspect_workspace(workspace: &ParsedWorkspace, workspace_dir: &Path) -> WorkspaceSummary {
    let mut cubes = Vec::new();

    for entry in &workspace.cubes {
        let cube_path = workspace_dir.join(&entry.path);
        let cube_name = entry
            .name
            .clone()
            .unwrap_or_else(|| entry.path.display().to_string());

        let yaml = match std::fs::read_to_string(&cube_path) {
            Ok(s) => s,
            Err(_) => {
                cubes.push(CubeSummary {
                    name: cube_name,
                    path: entry.path.display().to_string(),
                    dimension_count: 0,
                    measure_count: 0,
                    rule_count: 0,
                    description: None,
                });
                continue;
            }
        };

        let resolved = if resolve::has_refs(&yaml) {
            resolve::resolve_refs(&yaml, workspace, workspace_dir).unwrap_or(yaml)
        } else {
            yaml
        };

        match mc_model::parse(&resolved, Some(cube_path.display().to_string())) {
            Ok(parsed) => {
                cubes.push(CubeSummary {
                    name: cube_name,
                    path: entry.path.display().to_string(),
                    dimension_count: parsed.dimensions.len(),
                    measure_count: parsed.measures.len(),
                    rule_count: parsed.rules.len(),
                    description: parsed.metadata.description.clone(),
                });
            }
            Err(_) => {
                cubes.push(CubeSummary {
                    name: cube_name,
                    path: entry.path.display().to_string(),
                    dimension_count: 0,
                    measure_count: 0,
                    rule_count: 0,
                    description: None,
                });
            }
        }
    }

    let links: Vec<LinkSummary> = workspace
        .links
        .iter()
        .map(|l| LinkSummary {
            from: format!("{}.{}", l.from_cube, l.from_measure),
            to: format!("{}.{}", l.to_cube, l.to_measure),
            description: l.description.clone(),
        })
        .collect();

    let shared_dims: Vec<String> = workspace
        .shared_dimensions
        .iter()
        .map(|c| c.id.clone())
        .collect();

    WorkspaceSummary {
        name: workspace.name.clone(),
        id: workspace.id.clone(),
        description: workspace.description.clone(),
        domain: workspace.domain.clone(),
        cube_count: cubes.len(),
        cubes,
        shared_dimension_count: shared_dims.len(),
        shared_dimensions: shared_dims,
        link_count: links.len(),
        links,
        golden_suite_count: workspace.golden_suites.len(),
    }
}

/// Render the summary as human-readable text.
pub fn inspect_text(summary: &WorkspaceSummary, diags: &[WorkspaceDiagnostic]) -> String {
    let mut out = String::new();

    out.push_str(&format!("Workspace: {}\n", summary.name));
    out.push_str(&format!("ID: {}\n", summary.id));
    if let Some(ref desc) = summary.description {
        out.push_str(&format!("Description: {desc}\n"));
    }
    if let Some(ref domain) = summary.domain {
        out.push_str(&format!("Domain: {domain}\n"));
    }
    out.push('\n');

    out.push_str(&format!("Cubes: {}\n", summary.cube_count));
    for cube in &summary.cubes {
        out.push_str(&format!(
            "  {} ({}): {} dims, {} measures, {} rules\n",
            cube.name, cube.path, cube.dimension_count, cube.measure_count, cube.rule_count
        ));
        if let Some(ref desc) = cube.description {
            out.push_str(&format!("    {desc}\n"));
        }
    }
    out.push('\n');

    if !summary.shared_dimensions.is_empty() {
        out.push_str(&format!(
            "Shared Dimensions: {}\n",
            summary.shared_dimension_count
        ));
        for dim in &summary.shared_dimensions {
            out.push_str(&format!("  {dim}\n"));
        }
        out.push('\n');
    }

    if !summary.links.is_empty() {
        out.push_str(&format!("Links: {}\n", summary.link_count));
        for link in &summary.links {
            out.push_str(&format!("  {} -> {}", link.from, link.to));
            if let Some(ref desc) = link.description {
                out.push_str(&format!(" ({desc})"));
            }
            out.push('\n');
        }
        out.push('\n');
    }

    out.push_str(&format!("Golden Suites: {}\n", summary.golden_suite_count));

    if !diags.is_empty() {
        out.push_str(&format!("\nDiagnostics: {}\n", diags.len()));
        let errors = diags
            .iter()
            .filter(|d| d.severity == crate::diagnostic::Severity::Error)
            .count();
        let warnings = diags
            .iter()
            .filter(|d| d.severity == crate::diagnostic::Severity::Warning)
            .count();
        let info = diags
            .iter()
            .filter(|d| d.severity == crate::diagnostic::Severity::Info)
            .count();
        if errors > 0 {
            out.push_str(&format!("  {errors} error(s)"));
        }
        if warnings > 0 {
            if errors > 0 {
                out.push_str(", ");
            } else {
                out.push_str("  ");
            }
            out.push_str(&format!("{warnings} warning(s)"));
        }
        if info > 0 {
            if errors > 0 || warnings > 0 {
                out.push_str(", ");
            } else {
                out.push_str("  ");
            }
            out.push_str(&format!("{info} info"));
        }
        out.push('\n');
    }

    out
}

/// Render the summary as JSON.
pub fn inspect_json(summary: &WorkspaceSummary, diags: &[WorkspaceDiagnostic]) -> String {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"workspace\": {\n");
    out.push_str(&format!("    \"name\": {:?},\n", summary.name));
    out.push_str(&format!("    \"id\": {:?},\n", summary.id));
    out.push_str(&format!(
        "    \"description\": {},\n",
        opt_json_str(&summary.description)
    ));
    out.push_str(&format!(
        "    \"domain\": {},\n",
        opt_json_str(&summary.domain)
    ));
    out.push_str(&format!("    \"cube_count\": {},\n", summary.cube_count));
    out.push_str("    \"cubes\": [\n");
    for (i, cube) in summary.cubes.iter().enumerate() {
        out.push_str("      {\n");
        out.push_str(&format!("        \"name\": {:?},\n", cube.name));
        out.push_str(&format!("        \"path\": {:?},\n", cube.path));
        out.push_str(&format!(
            "        \"dimension_count\": {},\n",
            cube.dimension_count
        ));
        out.push_str(&format!(
            "        \"measure_count\": {},\n",
            cube.measure_count
        ));
        out.push_str(&format!("        \"rule_count\": {},\n", cube.rule_count));
        out.push_str(&format!(
            "        \"description\": {}\n",
            opt_json_str(&cube.description)
        ));
        out.push_str("      }");
        if i + 1 < summary.cubes.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("    ],\n");
    out.push_str(&format!(
        "    \"shared_dimension_count\": {},\n",
        summary.shared_dimension_count
    ));
    out.push_str(&format!("    \"link_count\": {},\n", summary.link_count));
    out.push_str(&format!(
        "    \"golden_suite_count\": {}\n",
        summary.golden_suite_count
    ));
    out.push_str("  },\n");
    out.push_str(&format!("  \"diagnostic_count\": {}\n", diags.len()));
    out.push_str("}\n");
    out
}

fn opt_json_str(v: &Option<String>) -> String {
    match v {
        Some(s) => format!("{s:?}"),
        None => "null".to_string(),
    }
}
