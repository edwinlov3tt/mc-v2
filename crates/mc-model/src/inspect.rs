//! Phase 3B `inspect` module — structured one-screen summary of a
//! [`ValidatedModel`] per [ADR-0005](../../../docs/decisions/0005-phase-3b-model-qa-linter-diagnostics.md)
//! Decision 4.
//!
//! Two outputs:
//!
//! - [`inspect_text`] — human-readable; the `mc model inspect <path>`
//!   default. Shape is fixed so snapshot tests can lock the format.
//! - [`inspect_json`] — `{ schema_version, model: {...}, diagnostics: [...] }`
//!   for Phase 4 LLM authoring + Phase 6 UI editor consumption. The
//!   `schema_version` field is mandatory per ADR-0005 amendment #13.
//!
//! [`ModelSummary`] is the structured-data form the JSON renderer reads
//! from. It also serves as a stable Rust API for programmatic callers
//! (a future UI may want to render the data without the text layer).

use std::collections::BTreeMap;

use crate::diagnostic::{
    diagnostics_to_json, write_json_string, Diagnostic, Severity, SCHEMA_VERSION,
};
use crate::formula;
use crate::inputs::ResolvedInputs;
use crate::schema::{ParsedHierarchy, ParsedRuleBody, ValidatedModel};

/// Structured summary of a validated model. Mirrors the fields the
/// Decision-4 default summary prints. Programmatic callers (Phase 6 UI,
/// future tooling) can read this directly instead of re-parsing the
/// text output.
#[derive(Clone, Debug)]
pub struct ModelSummary {
    pub name: String,
    pub format_version: i64,
    pub description: Option<String>,
    pub author: Option<String>,
    pub created: Option<String>,
    pub dimensions: Vec<DimensionSummary>,
    pub measures: MeasureBreakdown,
    pub rules: RuleBreakdown,
    pub golden_test_count: usize,
    pub cardinality: u128,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    /// Phase 3C: canonical_inputs summary. `None` if the model didn't
    /// declare canonical_inputs OR if no `ResolvedInputs` was passed
    /// to the summarize call.
    pub canonical_inputs: Option<InputSetSummary>,
    /// Phase 3C: per-fixture summary. Empty when no fixtures declared
    /// OR when no `ResolvedInputs` was passed.
    pub test_fixtures: Vec<FixtureSummary>,
}

#[derive(Clone, Debug)]
pub struct InputSetSummary {
    pub source_label: String,
    pub row_count: usize,
}

#[derive(Clone, Debug)]
pub struct FixtureSummary {
    pub name: String,
    pub source_label: String,
    pub row_count: usize,
}

#[derive(Clone, Debug)]
pub struct DimensionSummary {
    pub name: String,
    pub kind: String,
    pub element_count: usize,
    pub leaf_count: usize,
    pub consolidated_count: usize,
    pub default_hierarchy: Option<HierarchySummary>,
}

#[derive(Clone, Debug)]
pub struct HierarchySummary {
    pub name: String,
    pub edge_count: usize,
    pub depth: usize,
}

#[derive(Clone, Debug)]
pub struct MeasureBreakdown {
    pub total: usize,
    pub input_names: Vec<String>,
    pub derived_names: Vec<String>,
    /// Counts by aggregation method, lexicographic key order
    /// (`Min` < `Max` < `Sum` < `WeightedAverage` happens to be the
    /// canonical-string-order; the BTreeMap enforces deterministic
    /// emission).
    pub aggregation_counts: BTreeMap<String, usize>,
}

#[derive(Clone, Debug)]
pub struct RuleSummary {
    pub name: String,
    pub target_measure: String,
    pub body_shape: String,
}

#[derive(Clone, Debug)]
pub struct RuleBreakdown {
    pub total: usize,
    pub items: Vec<RuleSummary>,
    pub longest_chain_depth: usize,
}

/// Build a [`ModelSummary`] for the given model + diagnostic list +
/// optional resolved inputs.
///
/// `diagnostics` is the (already-sorted) lint output from
/// [`crate::lint`]; counts are derived from severity. Pass an empty
/// slice if you have no diagnostics yet.
///
/// `inputs` is the Phase 3C resolve-inputs stage output. When `Some`,
/// the summary's `canonical_inputs` and `test_fixtures` fields are
/// populated with row counts. When `None` (or when the model declared
/// no inputs), they are empty.
pub fn summarize(
    model: &ValidatedModel,
    diagnostics: &[Diagnostic],
    inputs: Option<&ResolvedInputs>,
) -> ModelSummary {
    let dimensions: Vec<DimensionSummary> = model
        .parsed
        .dimensions
        .iter()
        .map(|d| {
            let default_hier =
                pick_default_hierarchy(&d.name, &model.parsed.hierarchies).map(|h| {
                    HierarchySummary {
                        name: h.name.clone(),
                        edge_count: h.edges.len(),
                        depth: hierarchy_depth(h),
                    }
                });

            // Measure dim's element count comes from the top-level
            // `measures:` block (validator pre-cleared). Other dims use
            // their inline `elements`.
            let element_count = if d.kind == "Measure" {
                model.parsed.measures.len()
            } else {
                d.elements.len()
            };

            // Leaves vs consolidated: a "consolidated" element appears as
            // some edge's parent in the default hierarchy. For dims with
            // no default hierarchy (Scenario, Version, Measure on Acme),
            // every element is a leaf. The Measure dim's leaf count is
            // measures.len() since its inline elements list is empty by
            // construction.
            let (leaf, consolidated) = if d.kind == "Measure" {
                (model.parsed.measures.len(), 0)
            } else {
                count_leaves(d, &model.parsed.hierarchies)
            };

            DimensionSummary {
                name: d.name.clone(),
                kind: d.kind.clone(),
                element_count,
                leaf_count: leaf,
                consolidated_count: consolidated,
                default_hierarchy: default_hier,
            }
        })
        .collect();

    let mut input_names = Vec::new();
    let mut derived_names = Vec::new();
    let mut agg_counts: BTreeMap<String, usize> = BTreeMap::new();
    for m in &model.parsed.measures {
        match m.role.as_str() {
            "Input" => input_names.push(m.name.clone()),
            "Derived" => derived_names.push(m.name.clone()),
            _ => {}
        }
        *agg_counts.entry(m.aggregation.clone()).or_insert(0) += 1;
    }

    let depths = compute_chain_depths(model);
    let longest_chain_depth = depths.values().copied().max().unwrap_or(0);
    // Phase 3D acceptance amendment #24: rule bodies render as friendly
    // formulas regardless of authoring form. The structured-vs-formula
    // distinction lives in `ParsedModel`; by the time `summarize` runs
    // we have a flat `ParsedRuleBody`, and `formula::serialize` is the
    // uniform renderer.
    let rule_items: Vec<RuleSummary> = model
        .rules
        .iter()
        .map(|r| RuleSummary {
            name: r.name.clone(),
            target_measure: r.target_measure.clone(),
            body_shape: formula::serialize(&r.body),
        })
        .collect();
    let rules_total = rule_items.len();

    let cardinality: u128 = dimensions.iter().map(|d| d.element_count as u128).product();

    let mut error_count = 0;
    let mut warning_count = 0;
    let mut info_count = 0;
    for d in diagnostics {
        match d.severity {
            Severity::Error => error_count += 1,
            Severity::Warning => warning_count += 1,
            Severity::Info => info_count += 1,
        }
    }

    let canonical_inputs = inputs.and_then(|i| {
        i.canonical.as_ref().map(|c| InputSetSummary {
            source_label: c.source_label.clone(),
            row_count: c.rows.len(),
        })
    });
    let test_fixtures: Vec<FixtureSummary> = inputs
        .map(|i| {
            i.fixtures
                .iter()
                .map(|f| FixtureSummary {
                    name: f.name.clone(),
                    source_label: f.source_label.clone(),
                    row_count: f.rows.len(),
                })
                .collect()
        })
        .unwrap_or_default();

    ModelSummary {
        name: model.parsed.metadata.name.clone(),
        format_version: model.parsed.model_format_version,
        description: model.parsed.metadata.description.clone(),
        author: model.parsed.metadata.author.clone(),
        created: model.parsed.metadata.created.clone(),
        dimensions,
        measures: MeasureBreakdown {
            total: model.parsed.measures.len(),
            input_names,
            derived_names,
            aggregation_counts: agg_counts,
        },
        rules: RuleBreakdown {
            total: rules_total,
            items: rule_items,
            longest_chain_depth,
        },
        golden_test_count: model.parsed.golden_tests.len(),
        cardinality,
        error_count,
        warning_count,
        info_count,
        canonical_inputs,
        test_fixtures,
    }
}

/// Render the Decision-4 default summary as plain text. Layout is
/// snapshot-locked by `tests/cli_snapshot.rs`; do not change without
/// updating the expected fixtures in lock-step.
pub fn inspect_text(model: &ValidatedModel) -> String {
    inspect_text_with_diagnostics(model, &[], None)
}

/// Like [`inspect_text`] but factors in a diagnostic list (for the
/// `Diagnostics:` summary line) and an optional [`ResolvedInputs`]
/// (for the Phase 3C `Canonical inputs:` / `Test fixtures:` lines).
pub fn inspect_text_with_diagnostics(
    model: &ValidatedModel,
    diagnostics: &[Diagnostic],
    inputs: Option<&ResolvedInputs>,
) -> String {
    let summary = summarize(model, diagnostics, inputs);
    let mut out = String::new();
    out.push_str(&format!(
        "Model: {} (format v{})\n",
        summary.name, summary.format_version
    ));
    if let Some(desc) = &summary.description {
        out.push_str(&format!("  Description: {desc}\n"));
    }
    if let Some(author) = &summary.author {
        out.push_str(&format!("  Author: {author}\n"));
    }
    if let Some(created) = &summary.created {
        out.push_str(&format!("  Created: {created}\n"));
    }
    out.push('\n');

    out.push_str(&format!("Dimensions: {}\n", summary.dimensions.len()));
    for d in &summary.dimensions {
        let hier = match &d.default_hierarchy {
            Some(h) => format!(
                "; default hierarchy '{}' with {} edges, depth {}",
                h.name, h.edge_count, h.depth
            ),
            None => String::new(),
        };
        out.push_str(&format!(
            "  - {} ({}) — {} elements ({} leaves, {} consolidated{})\n",
            d.name, d.kind, d.element_count, d.leaf_count, d.consolidated_count, hier
        ));
    }
    out.push('\n');

    out.push_str(&format!("Measures: {}\n", summary.measures.total));
    out.push_str(&format!(
        "  Input ({}): {}\n",
        summary.measures.input_names.len(),
        if summary.measures.input_names.is_empty() {
            String::from("(none)")
        } else {
            summary.measures.input_names.join(", ")
        }
    ));
    out.push_str(&format!(
        "  Derived ({}): {}\n",
        summary.measures.derived_names.len(),
        if summary.measures.derived_names.is_empty() {
            String::from("(none)")
        } else {
            summary.measures.derived_names.join(", ")
        }
    ));
    let agg_entries: Vec<String> = summary
        .measures
        .aggregation_counts
        .iter()
        .map(|(k, v)| format!("{k} ({v})"))
        .collect();
    out.push_str(&format!("  Aggregations: {}\n", agg_entries.join(", ")));
    out.push('\n');

    out.push_str(&format!("Rules: {}\n", summary.rules.total));
    for r in &summary.rules.items {
        out.push_str(&format!(
            "  - {}: {} = {}\n",
            r.name, r.target_measure, r.body_shape
        ));
    }
    out.push_str(&format!(
        "  Longest rule chain depth: {}\n",
        summary.rules.longest_chain_depth
    ));
    out.push('\n');

    out.push_str(&format!(
        "Cardinality (Cartesian product across all dim elements): {}\n",
        summary.cardinality
    ));
    out.push_str(&format!("Golden tests: {}\n", summary.golden_test_count));
    // Phase 3C input declarations. Show what the model declared even
    // when no ResolvedInputs was passed (count is omitted then).
    match (
        &summary.canonical_inputs,
        model.parsed.canonical_inputs.is_some(),
    ) {
        (Some(c), _) => out.push_str(&format!(
            "Canonical inputs: {} cells from {}\n",
            c.row_count, c.source_label
        )),
        (None, true) => out.push_str("Canonical inputs: declared (run `mc model test` to load)\n"),
        (None, false) => out.push_str("Canonical inputs: (none declared)\n"),
    }
    if !summary.test_fixtures.is_empty() {
        out.push_str(&format!("Test fixtures: {}\n", summary.test_fixtures.len()));
        for f in &summary.test_fixtures {
            out.push_str(&format!(
                "  - {}: {} cells from {}\n",
                f.name, f.row_count, f.source_label
            ));
        }
    } else if !model.parsed.test_fixtures.is_empty() {
        out.push_str(&format!(
            "Test fixtures: {} declared (run `mc model test` to load)\n",
            model.parsed.test_fixtures.len()
        ));
    } else {
        out.push_str("Test fixtures: (none declared)\n");
    }
    out.push_str(&format!(
        "Diagnostics: {} errors, {} warnings, {} info\n",
        summary.error_count, summary.warning_count, summary.info_count
    ));
    out
}

/// Render the model summary as JSON with the Phase 3B envelope:
///
/// ```json
/// {
///   "schema_version": "1.0",
///   "model": { ... },
///   "diagnostics": [ ... ]
/// }
/// ```
///
/// `schema_version` is unconditional (amendment #13). The diagnostics
/// array is the same shape `mc model lint --format json` produces.
pub fn inspect_json(
    model: &ValidatedModel,
    diagnostics: &[Diagnostic],
    inputs: Option<&ResolvedInputs>,
) -> String {
    let summary = summarize(model, diagnostics, inputs);
    let mut out = String::new();
    out.push_str("{\n  \"schema_version\": \"");
    out.push_str(SCHEMA_VERSION);
    out.push_str("\",\n  \"model\": ");
    write_summary_json(&mut out, &summary, 2);
    out.push_str(",\n  \"diagnostics\": ");
    let diag_envelope = diagnostics_to_json(diagnostics);
    // diagnostics_to_json returns a full envelope; we only want its
    // "diagnostics" array. Extract it cheaply by string substring.
    let arr = extract_diagnostics_array(&diag_envelope);
    out.push_str(arr.trim());
    out.push_str("\n}\n");
    out
}

fn extract_diagnostics_array(envelope: &str) -> String {
    // The diagnostic envelope is hand-rolled: `{ "schema_version": "1.0",\n  "diagnostics": [...] }`.
    // Locate `"diagnostics":` and return the array (including brackets).
    let key = "\"diagnostics\":";
    if let Some(start) = envelope.find(key) {
        let after_key = &envelope[start + key.len()..];
        // Skip leading whitespace
        let trimmed = after_key.trim_start();
        // Find the closing brace of the envelope's top-level object —
        // this is the last `}` on the last line. We want everything
        // between the start of the array and the matching `]`.
        if let Some(end_arr) = find_matching_close(trimmed, '[', ']') {
            return trimmed[..=end_arr].to_string();
        }
    }
    "[]".to_string()
}

fn find_matching_close(s: &str, open: char, close: char) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut i = 0usize;
    let mut in_string = false;
    let mut escape = false;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
        } else if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn write_summary_json(out: &mut String, s: &ModelSummary, indent: usize) {
    let pad = " ".repeat(indent);
    let pad2 = " ".repeat(indent + 2);
    out.push_str("{\n");
    out.push_str(&pad2);
    out.push_str("\"name\": ");
    write_json_string(out, &s.name);
    out.push_str(",\n");

    out.push_str(&pad2);
    out.push_str("\"format_version\": ");
    out.push_str(&s.format_version.to_string());
    out.push_str(",\n");

    out.push_str(&pad2);
    out.push_str("\"description\": ");
    write_optional_string(out, &s.description);
    out.push_str(",\n");

    out.push_str(&pad2);
    out.push_str("\"author\": ");
    write_optional_string(out, &s.author);
    out.push_str(",\n");

    out.push_str(&pad2);
    out.push_str("\"created\": ");
    write_optional_string(out, &s.created);
    out.push_str(",\n");

    out.push_str(&pad2);
    out.push_str("\"cardinality\": ");
    out.push_str(&s.cardinality.to_string());
    out.push_str(",\n");

    out.push_str(&pad2);
    out.push_str("\"golden_test_count\": ");
    out.push_str(&s.golden_test_count.to_string());
    out.push_str(",\n");

    out.push_str(&pad2);
    out.push_str("\"longest_rule_chain_depth\": ");
    out.push_str(&s.rules.longest_chain_depth.to_string());
    out.push_str(",\n");

    out.push_str(&pad2);
    out.push_str("\"dimensions\": [");
    if s.dimensions.is_empty() {
        out.push_str("],\n");
    } else {
        out.push('\n');
        for (i, d) in s.dimensions.iter().enumerate() {
            write_dim_json(out, d, indent + 4);
            if i + 1 < s.dimensions.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str(&pad2);
        out.push_str("],\n");
    }

    out.push_str(&pad2);
    out.push_str("\"measures\": ");
    write_measure_breakdown_json(out, &s.measures, indent + 2);
    out.push_str(",\n");

    out.push_str(&pad2);
    out.push_str("\"rules\": [");
    if s.rules.items.is_empty() {
        out.push_str("],\n");
    } else {
        out.push('\n');
        for (i, r) in s.rules.items.iter().enumerate() {
            write_rule_json(out, r, indent + 4);
            if i + 1 < s.rules.items.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str(&pad2);
        out.push_str("],\n");
    }

    out.push_str(&pad2);
    out.push_str("\"diagnostic_counts\": {\"errors\": ");
    out.push_str(&s.error_count.to_string());
    out.push_str(", \"warnings\": ");
    out.push_str(&s.warning_count.to_string());
    out.push_str(", \"info\": ");
    out.push_str(&s.info_count.to_string());
    out.push_str("},\n");

    // Phase 3C: canonical_inputs + test_fixtures.
    out.push_str(&pad2);
    out.push_str("\"canonical_inputs\": ");
    match &s.canonical_inputs {
        Some(c) => {
            out.push_str("{\"source_label\": ");
            write_json_string(out, &c.source_label);
            out.push_str(", \"row_count\": ");
            out.push_str(&c.row_count.to_string());
            out.push('}');
        }
        None => out.push_str("null"),
    }
    out.push_str(",\n");

    out.push_str(&pad2);
    out.push_str("\"test_fixtures\": [");
    if s.test_fixtures.is_empty() {
        out.push(']');
    } else {
        out.push('\n');
        for (i, f) in s.test_fixtures.iter().enumerate() {
            out.push_str(&" ".repeat(indent + 4));
            out.push_str("{\"name\": ");
            write_json_string(out, &f.name);
            out.push_str(", \"source_label\": ");
            write_json_string(out, &f.source_label);
            out.push_str(", \"row_count\": ");
            out.push_str(&f.row_count.to_string());
            out.push('}');
            if i + 1 < s.test_fixtures.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str(&pad2);
        out.push(']');
    }
    out.push('\n');

    out.push_str(&pad);
    out.push('}');
}

fn write_dim_json(out: &mut String, d: &DimensionSummary, indent: usize) {
    let pad = " ".repeat(indent);
    out.push_str(&pad);
    out.push_str("{\"name\": ");
    write_json_string(out, &d.name);
    out.push_str(", \"kind\": ");
    write_json_string(out, &d.kind);
    out.push_str(", \"element_count\": ");
    out.push_str(&d.element_count.to_string());
    out.push_str(", \"leaf_count\": ");
    out.push_str(&d.leaf_count.to_string());
    out.push_str(", \"consolidated_count\": ");
    out.push_str(&d.consolidated_count.to_string());
    out.push_str(", \"default_hierarchy\": ");
    match &d.default_hierarchy {
        Some(h) => {
            out.push_str("{\"name\": ");
            write_json_string(out, &h.name);
            out.push_str(", \"edge_count\": ");
            out.push_str(&h.edge_count.to_string());
            out.push_str(", \"depth\": ");
            out.push_str(&h.depth.to_string());
            out.push('}');
        }
        None => out.push_str("null"),
    }
    out.push('}');
}

fn write_measure_breakdown_json(out: &mut String, m: &MeasureBreakdown, indent: usize) {
    let pad = " ".repeat(indent);
    let pad2 = " ".repeat(indent + 2);
    out.push_str("{\n");
    out.push_str(&pad2);
    out.push_str("\"total\": ");
    out.push_str(&m.total.to_string());
    out.push_str(",\n");

    out.push_str(&pad2);
    out.push_str("\"input\": [");
    write_string_list(out, &m.input_names);
    out.push_str("],\n");

    out.push_str(&pad2);
    out.push_str("\"derived\": [");
    write_string_list(out, &m.derived_names);
    out.push_str("],\n");

    out.push_str(&pad2);
    out.push_str("\"aggregation_counts\": {");
    let entries: Vec<String> = m
        .aggregation_counts
        .iter()
        .map(|(k, v)| {
            let mut s = String::new();
            write_json_string(&mut s, k);
            s.push_str(": ");
            s.push_str(&v.to_string());
            s
        })
        .collect();
    out.push_str(&entries.join(", "));
    out.push_str("}\n");

    out.push_str(&pad);
    out.push('}');
}

fn write_string_list(out: &mut String, items: &[String]) {
    let parts: Vec<String> = items
        .iter()
        .map(|s| {
            let mut buf = String::new();
            write_json_string(&mut buf, s);
            buf
        })
        .collect();
    out.push_str(&parts.join(", "));
}

fn write_rule_json(out: &mut String, r: &RuleSummary, indent: usize) {
    let pad = " ".repeat(indent);
    out.push_str(&pad);
    out.push_str("{\"name\": ");
    write_json_string(out, &r.name);
    out.push_str(", \"target_measure\": ");
    write_json_string(out, &r.target_measure);
    out.push_str(", \"body_shape\": ");
    write_json_string(out, &r.body_shape);
    out.push('}');
}

fn write_optional_string(out: &mut String, s: &Option<String>) {
    match s {
        Some(v) => write_json_string(out, v),
        None => out.push_str("null"),
    }
}

// ---------------------------------------------------------------------------
// Helpers shared with the lint module's depth + hierarchy calculations.
// ---------------------------------------------------------------------------

fn pick_default_hierarchy<'a>(
    dim_name: &str,
    hierarchies: &'a [ParsedHierarchy],
) -> Option<&'a ParsedHierarchy> {
    let candidates: Vec<&ParsedHierarchy> = hierarchies
        .iter()
        .filter(|h| h.dimension == dim_name)
        .collect();
    candidates
        .iter()
        .copied()
        .find(|h| h.default == Some(true))
        .or_else(|| candidates.first().copied())
}

/// Depth of the deepest path from any root to any leaf in `h`. A root is
/// a node that appears as a parent but never as a child; a leaf is a node
/// that appears as a child but never as a parent. Returns 0 for an empty
/// hierarchy.
fn hierarchy_depth(h: &ParsedHierarchy) -> usize {
    use std::collections::BTreeMap as Map;
    let mut children_of: Map<&str, Vec<&str>> = Map::new();
    let mut all: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let mut has_parent: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for edge in &h.edges {
        children_of
            .entry(edge.parent.as_str())
            .or_default()
            .push(edge.child.as_str());
        all.insert(edge.parent.as_str());
        all.insert(edge.child.as_str());
        has_parent.insert(edge.child.as_str());
    }
    let roots: Vec<&str> = all.difference(&has_parent).copied().collect();
    let mut max_depth = 0usize;
    for &root in &roots {
        let d = walk_depth(root, &children_of, 0);
        if d > max_depth {
            max_depth = d;
        }
    }
    max_depth
}

fn walk_depth(
    node: &str,
    children_of: &std::collections::BTreeMap<&str, Vec<&str>>,
    here: usize,
) -> usize {
    match children_of.get(node) {
        None => here,
        Some(kids) => kids
            .iter()
            .map(|k| walk_depth(k, children_of, here + 1))
            .max()
            .unwrap_or(here),
    }
}

fn count_leaves(
    dim: &crate::schema::ParsedDimension,
    hierarchies: &[ParsedHierarchy],
) -> (usize, usize) {
    let Some(h) = pick_default_hierarchy(&dim.name, hierarchies) else {
        // No hierarchy: every element is a leaf (zero consolidated).
        return (dim.elements.len(), 0);
    };
    if h.edges.is_empty() {
        return (dim.elements.len(), 0);
    }
    let mut consolidated: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for edge in &h.edges {
        consolidated.insert(edge.parent.as_str());
    }
    let mut leaves = 0usize;
    let mut cons = 0usize;
    for e in &dim.elements {
        if consolidated.contains(e.name.as_str()) {
            cons += 1;
        } else {
            leaves += 1;
        }
    }
    (leaves, cons)
}

fn compute_chain_depths(model: &ValidatedModel) -> BTreeMap<String, usize> {
    use std::collections::BTreeSet;
    let mut depths: BTreeMap<String, usize> = BTreeMap::new();
    for m in &model.parsed.measures {
        if m.role == "Input" {
            depths.insert(m.name.clone(), 0);
        }
    }
    let n = model.rules.len();
    for _ in 0..=n {
        let mut changed = false;
        for r in &model.rules {
            let mut refs = BTreeSet::new();
            collect_refs(&r.body, &mut refs);
            let mut max_dep = 0usize;
            let mut all_resolved = true;
            for ref_name in &refs {
                match depths.get(ref_name) {
                    Some(&d) => max_dep = max_dep.max(d),
                    None => {
                        all_resolved = false;
                        break;
                    }
                }
            }
            if all_resolved {
                let new_depth = max_dep + 1;
                let entry = depths.entry(r.target_measure.clone()).or_insert(0);
                if *entry < new_depth {
                    *entry = new_depth;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    depths
}

fn collect_refs(body: &ParsedRuleBody, out: &mut std::collections::BTreeSet<String>) {
    // Delegate to validate.rs's exhaustive walker via formula::contains_cross_coord
    // pattern. We only need measure Ref names for chain depth calculation.
    match body {
        ParsedRuleBody::Const(_)
        | ParsedRuleBody::PeriodIndex(_)
        | ParsedRuleBody::AnchorIndex(_)
        | ParsedRuleBody::IsPast(_)
        | ParsedRuleBody::IsCurrent(_)
        | ParsedRuleBody::IsFuture(_)
        | ParsedRuleBody::PeriodsSinceAnchor(_)
        | ParsedRuleBody::PeriodsToEnd(_) => {}
        ParsedRuleBody::Ref(r) => {
            out.insert(r.measure.clone());
        }
        ParsedRuleBody::Add(b) => walk(&b.add, out),
        ParsedRuleBody::Sub(b) => walk(&b.sub, out),
        ParsedRuleBody::Mul(b) => walk(&b.mul, out),
        ParsedRuleBody::Div(b) => walk(&b.div, out),
        ParsedRuleBody::IfNull(b) => walk(&b.if_null, out),
        ParsedRuleBody::Gt(b)
        | ParsedRuleBody::Lt(b)
        | ParsedRuleBody::Gte(b)
        | ParsedRuleBody::Lte(b)
        | ParsedRuleBody::Eq(b)
        | ParsedRuleBody::Neq(b)
        | ParsedRuleBody::And(b)
        | ParsedRuleBody::Or(b) => {
            collect_refs(&b.left, out);
            collect_refs(&b.right, out);
        }
        ParsedRuleBody::Not(b) | ParsedRuleBody::Abs(b) => collect_refs(&b.operand, out),
        ParsedRuleBody::If(b) => {
            collect_refs(&b.condition, out);
            collect_refs(&b.then_branch, out);
            collect_refs(&b.else_branch, out);
        }
        ParsedRuleBody::Min(b) | ParsedRuleBody::Max(b) | ParsedRuleBody::Coalesce(b) => {
            for a in &b.args {
                collect_refs(a, out);
            }
        }
        ParsedRuleBody::SafeDiv(b) => {
            collect_refs(&b.numerator, out);
            collect_refs(&b.denominator, out);
            collect_refs(&b.default, out);
        }
        ParsedRuleBody::Clamp(b) => {
            collect_refs(&b.value, out);
            collect_refs(&b.lo, out);
            collect_refs(&b.hi, out);
        }
        ParsedRuleBody::ActualRef(b) => {
            out.insert(b.measure.clone());
        }
        ParsedRuleBody::Prev(b) | ParsedRuleBody::Cumulative(b) => {
            out.insert(b.measure.clone());
        }
        ParsedRuleBody::Lag(b) => {
            out.insert(b.measure.clone());
            collect_refs(&b.periods, out);
        }
        ParsedRuleBody::RollingAvg(b) => {
            out.insert(b.measure.clone());
            collect_refs(&b.window, out);
        }
        ParsedRuleBody::Benchmark(b) => collect_refs(&b.key_expr, out),
        ParsedRuleBody::Lookup(b) => {
            for k in &b.key_exprs {
                collect_refs(k, out);
            }
        }
        ParsedRuleBody::Bucket(b) => collect_refs(&b.value, out),
        ParsedRuleBody::SumOver(b) => {
            out.insert(b.measure.clone());
        }
        // Phase 3H
        ParsedRuleBody::Predict(b) => {
            for f in &b.features {
                collect_refs(f, out);
            }
        }
        ParsedRuleBody::Calibrate(b) => collect_refs(&b.value, out),
        ParsedRuleBody::Exp(b) => collect_refs(&b.operand, out),
        ParsedRuleBody::NormCdf(b) => {
            collect_refs(&b.x, out);
            collect_refs(&b.mu, out);
            collect_refs(&b.sigma, out);
        }
        // Phase 3I
        ParsedRuleBody::Pow(b) => {
            collect_refs(&b.base, out);
            collect_refs(&b.exponent, out);
        }
        ParsedRuleBody::Sqrt(b)
        | ParsedRuleBody::Ln(b)
        | ParsedRuleBody::Log10(b)
        | ParsedRuleBody::Round(b)
        | ParsedRuleBody::Floor(b)
        | ParsedRuleBody::Ceil(b) => collect_refs(&b.operand, out),
        ParsedRuleBody::Mod(b) => {
            collect_refs(&b.dividend, out);
            collect_refs(&b.divisor, out);
        }
        ParsedRuleBody::NormInv(b) => {
            collect_refs(&b.p, out);
            collect_refs(&b.mu, out);
            collect_refs(&b.sigma, out);
        }
        ParsedRuleBody::IsElement(_) => {}
        ParsedRuleBody::AvgOver(b) | ParsedRuleBody::MinOver(b) | ParsedRuleBody::MaxOver(b) => {
            out.insert(b.measure.clone());
        }
        ParsedRuleBody::WAvgOver(b) => {
            out.insert(b.value_measure.clone());
            out.insert(b.weight_measure.clone());
        }
    }
}

fn walk(args: &[ParsedRuleBody], out: &mut std::collections::BTreeSet<String>) {
    for a in args {
        collect_refs(a, out);
    }
}
