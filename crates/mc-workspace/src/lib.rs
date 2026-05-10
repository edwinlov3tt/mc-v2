//! `mc-workspace` — Mosaic workspace + org manifest layer (Phase 4C).
//!
//! Implements ADR-0026's organizational container model as a manifest +
//! CLI orchestration layer above the existing per-cube engine. The kernel
//! (`mc-core`) is unchanged; the workspace resolves `$ref` directives by
//! inlining shared resources into synthesized cube YAML, then passes each
//! cube through the unchanged `mc-model::load_str()` pipeline.
//!
//! # Architecture
//!
//! ```text
//! workspace.yaml / org.yaml
//!     │  parse  (serde_yaml deserialization)
//!     ▼
//! ParsedWorkspace / ParsedOrg
//!     │  resolve  ($ref inlining — catalog elements baked into cube YAML)
//!     │  validate (MC5001–MC5008 workspace-level checks)
//!     │  lint     (MC5010–MC5012 workspace-level lint)
//!     ▼
//! Per-cube: mc_model::load_str() → CompiledCube
//! ```

#![deny(rust_2018_idioms)]
#![warn(missing_debug_implementations)]

pub mod diagnostic;
pub mod init;
pub mod inspect;
pub mod lint;
pub mod parse;
pub mod resolve;
pub mod schema;
pub mod validate;

// Public API re-exports.
pub use diagnostic::{
    diagnostics_to_json, diagnostics_to_text, sort_diagnostics, Severity, WorkspaceDiagnostic,
    WorkspaceError,
};
pub use init::init_workspace;
pub use inspect::{inspect_json, inspect_text, inspect_workspace, WorkspaceSummary};
pub use lint::lint_workspace;
pub use parse::{parse_org, parse_org_str, parse_workspace, parse_workspace_str};
pub use resolve::{has_refs, resolve_refs};
pub use schema::{
    CartridgeRef, CatalogElement, CubeEntry, CubeLink, DimensionCatalog, LinkKind, ParsedOrg,
    ParsedWorkspace, SharedArtifact, SharedCatalog, WorkspaceEntry,
};
pub use validate::{has_errors, validate_workspace};
