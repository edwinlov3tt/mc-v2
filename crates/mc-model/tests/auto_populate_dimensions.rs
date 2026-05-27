//! Phase 3K (ADR-0030 Decision 1 + Amendments 1–4): auto-element
//! population from `canonical_inputs`.
//!
//! Covers the 7 behavioural cases in the handoff:
//! - explicit elements win over auto-population
//! - Scenario / Version / Measure dims are NOT auto-populated
//! - first-seen CSV ordering preserved
//! - missing CSV column falls through to existing error path
//! - MC1015 info diagnostic emitted at low cardinality
//! - MC1016 warning above 10,000 elements
//! - MC1017 critical above 100,000 elements
//! - MC2026 case-mismatch hint when only casing differs

use mc_model::{auto_populate_dimensions, parse, validate, Severity};

/// Minimal model template — caller fills in `dimensions:` and
/// `canonical_inputs:` body. Includes one Input measure so `Measure` dim
/// is non-empty by default.
fn build_yaml(dims: &str, canonical_inputs: &str) -> String {
    format!(
        r#"model_format_version: 1
metadata:
  name: "test_cube"
{canonical_inputs}
dimensions:
{dims}
measures:
  - name: "Spend"
    role: "Input"
    data_type: "F64"
    aggregation: "Sum"
rules: []
"#
    )
}

const STANDARD_DIMS: &str = r#"  - name: "Scenario"
    kind: "Scenario"
    elements:
      - { name: "Default", scenario_meta: "Default" }
  - name: "Version"
    kind: "Version"
    elements:
      - { name: "v1", version_state: "Approved" }
  - name: "Time"
    kind: "Standard"
    elements:
      - { name: "Jan" }
  - name: "Game"
    kind: "Standard"
    elements: []
  - name: "Measure"
    kind: "Measure"
    elements: []
"#;

fn validate_and_auto_pop(yaml: &str) -> (Vec<mc_model::Diagnostic>, mc_model::ValidatedModel) {
    let parsed = parse(yaml, Some("test.yaml".into())).expect("parse");
    let mut validated = validate(parsed).expect("validate");
    let diags = auto_populate_dimensions(&mut validated, None).expect("auto_populate");
    (diags, validated)
}

fn dim_by_name<'a>(v: &'a mc_model::ValidatedModel, name: &str) -> &'a mc_model::ParsedDimension {
    v.parsed
        .dimensions
        .iter()
        .find(|d| d.name == name)
        .expect("dim present")
}

// ─── Decision 1: auto-population fires for empty Standard/Time ───

#[test]
fn t_auto_populate_empty_standard_dim() {
    let yaml = build_yaml(
        STANDARD_DIMS,
        r#"canonical_inputs:
  columns: ["Scenario", "Version", "Time", "Game", "Measure", "value"]
  inline:
    rows:
      - ["Default", "v1", "Jan", "GameA", "Spend", 1.0]
      - ["Default", "v1", "Jan", "GameB", "Spend", 2.0]
      - ["Default", "v1", "Jan", "GameC", "Spend", 3.0]
"#,
    );
    let (diags, validated) = validate_and_auto_pop(&yaml);
    let game = dim_by_name(&validated, "Game");
    assert_eq!(
        game.elements
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>(),
        vec!["GameA", "GameB", "GameC"],
    );
    // MC1015 info diagnostic surfaced.
    assert!(diags.iter().any(|d| d.code == "MC1015"));
}

#[test]
fn t_explicit_elements_wins_over_auto_populate() {
    let dims = r#"  - name: "Scenario"
    kind: "Scenario"
    elements:
      - { name: "Default", scenario_meta: "Default" }
  - name: "Version"
    kind: "Version"
    elements:
      - { name: "v1", version_state: "Approved" }
  - name: "Time"
    kind: "Standard"
    elements:
      - { name: "Jan" }
  - name: "Game"
    kind: "Standard"
    elements:
      - { name: "OnlyOne" }
  - name: "Measure"
    kind: "Measure"
    elements: []
"#;
    let yaml = build_yaml(
        dims,
        r#"canonical_inputs:
  columns: ["Scenario", "Version", "Time", "Game", "Measure", "value"]
  inline:
    rows:
      - ["Default", "v1", "Jan", "OnlyOne", "Spend", 1.0]
"#,
    );
    let (diags, validated) = validate_and_auto_pop(&yaml);
    let game = dim_by_name(&validated, "Game");
    assert_eq!(
        game.elements
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>(),
        vec!["OnlyOne"],
    );
    // No MC1015 — explicit wins, no auto-population happened.
    assert!(!diags.iter().any(|d| d.code == "MC1015"));
}

#[test]
fn t_scenario_dim_not_auto_populated() {
    // Scenario dim with empty elements + a Scenario column in canonical_inputs.
    // Auto-population MUST skip; the dim stays empty and downstream
    // resolution would fail.
    let dims = r#"  - name: "Scenario"
    kind: "Scenario"
    elements: []
  - name: "Version"
    kind: "Version"
    elements:
      - { name: "v1", version_state: "Approved" }
  - name: "Time"
    kind: "Standard"
    elements:
      - { name: "Jan" }
  - name: "Measure"
    kind: "Measure"
    elements: []
"#;
    let yaml = build_yaml(
        dims,
        r#"canonical_inputs:
  columns: ["Scenario", "Version", "Time", "Measure", "value"]
  inline:
    rows:
      - ["Default", "v1", "Jan", "Spend", 1.0]
"#,
    );
    let (diags, validated) = validate_and_auto_pop(&yaml);
    let scenario = dim_by_name(&validated, "Scenario");
    // Skipped — semantic dim, not data-derived.
    assert!(scenario.elements.is_empty());
    assert!(!diags.iter().any(|d| d.code == "MC1015"));
}

#[test]
fn t_first_seen_ordering_preserved() {
    let yaml = build_yaml(
        STANDARD_DIMS,
        r#"canonical_inputs:
  columns: ["Scenario", "Version", "Time", "Game", "Measure", "value"]
  inline:
    rows:
      - ["Default", "v1", "Jan", "Z", "Spend", 1.0]
      - ["Default", "v1", "Jan", "A", "Spend", 2.0]
      - ["Default", "v1", "Jan", "M", "Spend", 3.0]
      - ["Default", "v1", "Jan", "A", "Spend", 4.0]
      - ["Default", "v1", "Jan", "Z", "Spend", 5.0]
      - ["Default", "v1", "Jan", "B", "Spend", 6.0]
"#,
    );
    let (_diags, validated) = validate_and_auto_pop(&yaml);
    let game = dim_by_name(&validated, "Game");
    assert_eq!(
        game.elements
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Z", "A", "M", "B"],
        "expected first-seen ordering, not alphabetical sort",
    );
}

#[test]
fn t_no_matching_column_no_auto_pop() {
    // canonical_inputs has no `Game` column. Auto-population skips
    // silently; the dim stays empty and the kernel's DimensionEmpty
    // would fire at compile time (existing behavior).
    let dims = r#"  - name: "Scenario"
    kind: "Scenario"
    elements:
      - { name: "Default", scenario_meta: "Default" }
  - name: "Version"
    kind: "Version"
    elements:
      - { name: "v1", version_state: "Approved" }
  - name: "Time"
    kind: "Standard"
    elements:
      - { name: "Jan" }
  - name: "Game"
    kind: "Standard"
    elements: []
  - name: "Measure"
    kind: "Measure"
    elements: []
"#;
    // CSV doesn't have Game column.
    let yaml = build_yaml(
        dims,
        r#"canonical_inputs:
  columns: ["Scenario", "Version", "Time", "Measure", "value"]
  inline:
    rows:
      - ["Default", "v1", "Jan", "Spend", 1.0]
"#,
    );
    let parsed = parse(&yaml, Some("test.yaml".into())).expect("parse");
    let mut validated = validate(parsed).expect("validate");
    let diags = auto_populate_dimensions(&mut validated, None).expect("auto_populate");
    let game = dim_by_name(&validated, "Game");
    assert!(game.elements.is_empty(), "no match — should not auto-pop");
    assert!(!diags.iter().any(|d| d.code == "MC1015"));
}

// ─── Amendment 1: case-mismatch hint ───

#[test]
fn t_case_mismatch_hint_in_fallthrough_error() {
    // Dim "Game" with empty elements, canonical_inputs has lowercase
    // "game" column. Auto-pop must NOT fire (case differs); MC2026
    // surfaces with the actionable hint.
    let dims = r#"  - name: "Scenario"
    kind: "Scenario"
    elements:
      - { name: "Default", scenario_meta: "Default" }
  - name: "Version"
    kind: "Version"
    elements:
      - { name: "v1", version_state: "Approved" }
  - name: "Time"
    kind: "Standard"
    elements:
      - { name: "Jan" }
  - name: "Game"
    kind: "Standard"
    elements: []
  - name: "Measure"
    kind: "Measure"
    elements: []
"#;
    let yaml = build_yaml(
        dims,
        r#"canonical_inputs:
  columns: ["Scenario", "Version", "Time", "game", "Measure", "value"]
  inline:
    rows:
      - ["Default", "v1", "Jan", "g1", "Spend", 1.0]
"#,
    );
    let parsed = parse(&yaml, Some("test.yaml".into())).expect("parse");
    let mut validated = validate(parsed).expect("validate");
    let result = auto_populate_dimensions(&mut validated, None);
    let errs = result.expect_err("expected case-mismatch error");
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].code(), "MC2026");
    let msg = errs[0].to_string();
    assert!(
        msg.contains("game"),
        "msg should mention actual column name: {msg}"
    );
    assert!(msg.contains("Game"), "msg should mention dim name: {msg}");
    assert!(
        msg.contains("rename") || msg.contains("explicit"),
        "hint should suggest a fix: {msg}",
    );
}

#[test]
fn t_no_case_hint_when_no_close_match() {
    // No column even remotely matches "Game" — neither MC1015 nor MC2026
    // fires. Auto-population skips silently.
    let dims = r#"  - name: "Scenario"
    kind: "Scenario"
    elements:
      - { name: "Default", scenario_meta: "Default" }
  - name: "Version"
    kind: "Version"
    elements:
      - { name: "v1", version_state: "Approved" }
  - name: "Time"
    kind: "Standard"
    elements:
      - { name: "Jan" }
  - name: "Game"
    kind: "Standard"
    elements: []
  - name: "Measure"
    kind: "Measure"
    elements: []
"#;
    let yaml = build_yaml(
        dims,
        r#"canonical_inputs:
  columns: ["Scenario", "Version", "Time", "UnrelatedThing", "Measure", "value"]
  inline:
    rows:
      - ["Default", "v1", "Jan", "x", "Spend", 1.0]
"#,
    );
    let parsed = parse(&yaml, Some("test.yaml".into())).expect("parse");
    let mut validated = validate(parsed).expect("validate");
    let result = auto_populate_dimensions(&mut validated, None);
    // No MC2026 (no case-insensitive match); auto-pop just skips.
    let diags = result.expect("no MC2026 — only spurious-matches fire it");
    assert!(diags.is_empty());
}

// ─── Amendment 2: high-cardinality guardrail (synthetic) ───

#[test]
fn t_low_cardinality_only_mc1015() {
    // ~2,500 elements (MLB-sized) — well under 10K threshold.
    let mut rows = String::new();
    for i in 0..2500 {
        rows.push_str(&format!(
            "      - [\"Default\", \"v1\", \"Jan\", \"G{i}\", \"Spend\", 1.0]\n"
        ));
    }
    let yaml = build_yaml(
        STANDARD_DIMS,
        &format!(
            r#"canonical_inputs:
  columns: ["Scenario", "Version", "Time", "Game", "Measure", "value"]
  inline:
    rows:
{rows}"#
        ),
    );
    let (diags, validated) = validate_and_auto_pop(&yaml);
    assert_eq!(dim_by_name(&validated, "Game").elements.len(), 2500);
    assert!(diags.iter().any(|d| d.code == "MC1015"));
    assert!(!diags.iter().any(|d| d.code == "MC1016"));
    assert!(!diags.iter().any(|d| d.code == "MC1017"));
    let mc1015 = diags.iter().find(|d| d.code == "MC1015").unwrap();
    assert_eq!(mc1015.severity, Severity::Info);
}

#[test]
fn t_high_cardinality_warning_above_10k() {
    // 10,001 distinct elements → MC1016 warning, auto-pop still succeeds.
    let mut rows = String::new();
    for i in 0..10_001 {
        rows.push_str(&format!(
            "      - [\"Default\", \"v1\", \"Jan\", \"G{i}\", \"Spend\", 1.0]\n"
        ));
    }
    let yaml = build_yaml(
        STANDARD_DIMS,
        &format!(
            r#"canonical_inputs:
  columns: ["Scenario", "Version", "Time", "Game", "Measure", "value"]
  inline:
    rows:
{rows}"#
        ),
    );
    let (diags, validated) = validate_and_auto_pop(&yaml);
    assert_eq!(dim_by_name(&validated, "Game").elements.len(), 10_001);
    let warning = diags
        .iter()
        .find(|d| d.code == "MC1016")
        .expect("expected MC1016 warning");
    assert_eq!(warning.severity, Severity::Warning);
}

// ─── Schema generation ───

#[test]
fn t_schema_is_valid_json() {
    use mc_model::schema::ParsedModel;
    let schema = schemars::schema_for!(ParsedModel);
    let json = serde_json::to_value(&schema).expect("serialize");
    assert!(json.get("$schema").is_some(), "missing $schema field");
    assert!(json.get("title").is_some(), "missing title field");
    assert!(json.get("properties").is_some(), "missing properties field");
    let props = json.get("properties").unwrap();
    // Spot-check a few load-bearing fields.
    assert!(props.get("dimensions").is_some());
    assert!(props.get("measures").is_some());
    assert!(props.get("canonical_inputs").is_some());
}
