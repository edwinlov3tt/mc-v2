//! YAML template schema types for the narrative engine.
//!
//! These types define the structure of template YAML files loaded
//! from `narratives/*.yaml`. The schema is the source of truth —
//! adding a template means adding YAML, zero Rust changes.
//!
//! Session 4: adds `format` hints map, `on_null` per-binding behavior,
//! and `notability_base` field.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Top-level YAML file structure containing template definitions.
#[derive(Debug, Deserialize)]
pub struct TemplateFile {
    /// Schema version for forward compatibility.
    #[allow(dead_code)]
    pub narrative_format_version: u32,
    /// The template definitions in this file.
    pub templates: Vec<TemplateDefinition>,
}

/// A single narrative template definition parsed from YAML.
#[derive(Debug, Deserialize)]
pub struct TemplateDefinition {
    /// Unique identifier for this template.
    pub id: String,
    /// Which tactic families this template applies to.
    #[allow(dead_code)]
    pub family: Vec<String>,
    /// Severity level of narratives produced by this template.
    pub severity: Severity,
    /// Which report table types this template matches against.
    pub table_types: Vec<String>,
    /// Sort order for template evaluation (lower fires first).
    #[serde(default)]
    pub sort_order: i32,
    /// Predicate expression — template fires only when this evaluates truthy.
    pub when: String,
    /// Output template string with `{placeholder}` substitution.
    pub template: String,
    /// Named bindings: expressions evaluated and available for substitution.
    #[serde(default)]
    pub bindings: BTreeMap<String, String>,
    /// If true, this template fires at most once per evaluation batch.
    /// Finding #2: replaces the Phase 6D hardcoded `matches!()` list.
    #[serde(default)]
    pub deduplicate: bool,
    /// Named format hints for binding values.
    /// Maps binding name → format hint name (e.g., "currency", "percent_1").
    /// Session 4: overrides inline format specs when present.
    #[serde(default)]
    pub format: BTreeMap<String, String>,
    /// Static notability base score [0, 1]. The engine adjusts based on
    /// deviation magnitude from the `when:` predicate's values.
    #[serde(default)]
    pub notability_base: Option<f64>,

    // ─── Phase 7A.5: Explanation chains (ADR-0022) ───────────────────
    /// Finding group — templates sharing the same `finding_id` form an
    /// explanation chain evaluated in priority order. Templates without
    /// `finding_id` fire independently (zero behavior change).
    #[serde(default)]
    pub finding_id: Option<String>,

    /// Priority within an explanation group (lower = fires first).
    /// Default: 500. Templates without `finding_id` ignore this field.
    #[serde(default = "default_explanation_priority")]
    pub explanation_priority: u32,
}

/// Default explanation priority for templates that don't specify one.
fn default_explanation_priority() -> u32 {
    500
}

/// Severity level for a narrative output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Severity {
    /// Notable but not actionable.
    Info,
    /// Positive outcome.
    Success,
    /// Action recommended.
    Warning,
    /// Action required.
    Critical,
}

/// A rendered narrative paragraph — the structured output contract.
#[derive(Debug, Clone, Serialize)]
pub struct NarrativeOutput {
    /// Unique ID for this narrative instance (template_id + source context).
    pub id: String,
    /// Severity level.
    pub severity: Severity,
    /// The rendered narrative text.
    pub text: String,
    /// Which template produced this narrative.
    pub template_id: String,
    /// Evidence: binding values that contributed to this narrative.
    pub evidence: BTreeMap<String, serde_json::Value>,

    // ─── Phase 7A.5: Explanation chain metadata (ADR-0022 Decision 9) ─
    /// Which finding group produced this narrative (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finding_id: Option<String>,

    /// Templates skipped because a higher-priority explanation matched
    /// first (never evaluated). Per ADR-0022 Decision 9.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped_explanations: Vec<String>,

    /// Templates evaluated but whose `when:` predicate returned false
    /// (considered and rejected). Per ADR-0022 Decision 9.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rejected_explanations: Vec<String>,
}
