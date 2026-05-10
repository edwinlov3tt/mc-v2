//! Workspace and org manifest types — the YAML mirrors for `workspace.yaml`
//! and `org.yaml`.
//!
//! Per ADR-0026 Decision 1: the four-entity model is Org → Workspace → Cube → Cell.
//! This module defines the parsed manifest types for the top two layers.
//! The Cube layer is already handled by `mc-model`; Cell is `mc-core`.

use std::path::PathBuf;

use serde::Deserialize;

// ---------------------------------------------------------------------------
// workspace.yaml
// ---------------------------------------------------------------------------

/// Top-level parsed workspace manifest. Mirrors `workspace.yaml` 1:1.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedWorkspace {
    pub workspace_format_version: u32,
    pub name: String,
    pub id: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub org_id: Option<String>,
    #[serde(default)]
    pub shared_dimensions: Vec<SharedCatalog>,
    #[serde(default)]
    pub shared_fitted_models: Vec<SharedArtifact>,
    #[serde(default)]
    pub shared_calibration_maps: Vec<SharedArtifact>,
    #[serde(default)]
    pub shared_lookup_tables: Vec<SharedArtifact>,
    pub cubes: Vec<CubeEntry>,
    #[serde(default)]
    pub links: Vec<CubeLink>,
    #[serde(default)]
    pub golden_suites: Vec<PathBuf>,
}

/// A shared dimension catalog entry. References an external YAML file
/// containing dimension elements and optional hierarchy.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedCatalog {
    pub id: String,
    pub source: PathBuf,
}

/// A shared artifact entry (fitted model, calibration map, lookup table).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedArtifact {
    pub id: String,
    pub source: PathBuf,
}

/// One cube participating in the workspace.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CubeEntry {
    pub path: PathBuf,
    #[serde(default)]
    pub name: Option<String>,
}

/// Declarative inter-cube link. Phase 4C: documentation-only; the engine
/// does NOT enforce cross-cube dataflow. Phase 5+ (Tier C) wires these
/// into the kernel's dependency graph.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CubeLink {
    pub from_cube: String,
    pub from_measure: String,
    pub to_cube: String,
    pub to_measure: String,
    #[serde(default = "default_link_kind")]
    pub kind: LinkKind,
    #[serde(default)]
    pub description: Option<String>,
}

/// Link directionality. Phase 4C supports `ReadOnly` only.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub enum LinkKind {
    ReadOnly,
}

fn default_link_kind() -> LinkKind {
    LinkKind::ReadOnly
}

// ---------------------------------------------------------------------------
// org.yaml
// ---------------------------------------------------------------------------

/// Top-level parsed org manifest. Mirrors `org.yaml` 1:1.
/// Per ADR-0026 Decision 1: Organization is the top-level ownership,
/// trust, billing, and security boundary.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedOrg {
    pub org_format_version: u32,
    pub name: String,
    pub id: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub installed_cartridges: Vec<CartridgeRef>,
    #[serde(default)]
    pub org_templates_path: Option<PathBuf>,
    #[serde(default)]
    pub org_benchmarks_path: Option<PathBuf>,
    #[serde(default)]
    pub workspaces: Vec<WorkspaceEntry>,
}

/// Reference to an installed cartridge (org-scoped).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CartridgeRef {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

/// Pointer from org manifest to a workspace directory.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceEntry {
    pub path: PathBuf,
    pub name: String,
}

// ---------------------------------------------------------------------------
// Shared dimension catalog file format
// ---------------------------------------------------------------------------

/// Content of a shared dimension catalog file (e.g., `catalogs/channels.yaml`).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DimensionCatalog {
    pub catalog_format_version: u32,
    pub dimension: String,
    pub elements: Vec<CatalogElement>,
    #[serde(default)]
    pub hierarchy: Vec<CatalogHierarchyNode>,
}

/// One element in a shared catalog.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogElement {
    pub name: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub period_start: Option<String>,
    #[serde(default)]
    pub period_end_exclusive: Option<String>,
}

/// One hierarchy node in a shared catalog.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogHierarchyNode {
    pub name: String,
    pub children: Vec<String>,
    #[serde(default = "default_weight")]
    pub weight: f64,
}

fn default_weight() -> f64 {
    1.0
}
