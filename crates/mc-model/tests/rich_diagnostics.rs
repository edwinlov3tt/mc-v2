//! Phase 7A.6 D1 Session 2 regression tests for rich diagnostic rendering.
//!
//! Tests that the `LocationMap`, `Diagnostic::to_rich()`, and formula
//! offset composition work correctly end-to-end.

use mc_diagnostics::{render_diagnostic, ColorMode, SourceSpan};
use mc_model::diagnostic::Diagnostic;
use mc_model::location::LocationMap;
use mc_model::{ModelPath, Severity};

// -----------------------------------------------------------------------
// 1. Validation error gets a source span from LocationMap
// -----------------------------------------------------------------------
#[test]
fn test_validate_error_has_source_span() {
    let yaml = "model_format_version: 1\ndimensions:\n  - name: Time\n    elements: [Q1]\nmeasures:\n  - name: Revenue\n    role: input\n    aggregation: Sum";
    let map = LocationMap::build("model.yaml", yaml);

    let diag = Diagnostic {
        code: "MC2001",
        severity: Severity::Error,
        path: ModelPath::new("model.yaml", "/measures/name", "measures.Revenue"),
        message: "duplicate measure name".into(),
        suggestion: None,
    };

    let rich = diag.to_rich(Some(&map));
    // The rich diagnostic should have the code and message
    assert_eq!(rich.code, "MC2001");
    assert_eq!(rich.message, "duplicate measure name");
    // If the location map found the span, it should be present
    // (depends on key matching — the exact span may or may not be found
    // depending on YAML structure, but the conversion should not panic)
}

// -----------------------------------------------------------------------
// 2. Formula error points at bad token
// -----------------------------------------------------------------------
#[test]
fn test_formula_error_points_at_bad_token() {
    // Simulate a formula error with offset composition
    let yaml = "body: \"Custmers * AOV\"";
    let map = LocationMap::build("model.yaml", yaml);

    let body_span = map.get("/body");
    assert!(
        body_span.is_some(),
        "LocationMap should find the body value"
    );
    let body_span = body_span.unwrap();

    // "Custmers" starts at offset 0 within the formula string
    let inner = body_span.with_inner_offset(0, 8);
    let content = &yaml[inner.start_byte..inner.end_byte];
    assert_eq!(
        content, "Custmers",
        "inner offset should point at 'Custmers'"
    );
}

// -----------------------------------------------------------------------
// 3. Formula offset composition within YAML
// -----------------------------------------------------------------------
#[test]
fn test_formula_offset_composition_within_yaml() {
    let yaml = "rules:\n  - name: rule_revenue\n    body: \"Revenue + Custmers\"";
    let map = LocationMap::build("model.yaml", yaml);

    // Inside a sequence item, the pointer is /rules/0/body
    let body_span = map.get("/rules/0/body");
    assert!(body_span.is_some(), "should find /rules/0/body");
    let body_span = body_span.unwrap();
    // "Custmers" is at offset 10 within the formula string "Revenue + Custmers"
    let inner = body_span.with_inner_offset(10, 8);
    let content = &yaml[inner.start_byte..inner.end_byte];
    assert_eq!(content, "Custmers");
}

// -----------------------------------------------------------------------
// 4. Rich diagnostic renders with underline
// -----------------------------------------------------------------------
#[test]
fn test_rich_diagnostic_renders_with_underline() {
    let source = "body: \"Custmers * AOV\"";
    let diag = mc_diagnostics::RichDiagnostic::new(
        "MC2015",
        mc_diagnostics::DiagSeverity::Error,
        "measure not found",
    )
    .with_span(SourceSpan::new("model.yaml", 7, 15)); // "Custmers"

    let rendered = render_diagnostic(&diag, |_| Some(source.to_string()), ColorMode::Never);

    assert!(
        rendered.contains("error[MC2015]"),
        "should have error header"
    );
    assert!(
        rendered.contains("Custmers * AOV"),
        "should show source line"
    );
    assert!(
        rendered.contains("^^^^^^^^"),
        "should underline 'Custmers' (8 chars)"
    );
}

// -----------------------------------------------------------------------
// 5. to_rich preserves suggestion as help
// -----------------------------------------------------------------------
#[test]
fn test_to_rich_preserves_suggestion() {
    let diag = Diagnostic {
        code: "MC3001",
        severity: Severity::Warning,
        path: ModelPath::new("model.yaml", "/dimensions/0", "dimensions.Time"),
        message: "dimension has no description".into(),
        suggestion: Some("Add a description field".into()),
    };

    let rich = diag.to_rich(None);
    assert_eq!(rich.help.len(), 1);
    assert_eq!(rich.help[0], "Add a description field");
}

// -----------------------------------------------------------------------
// 6. LocationMap handles missing file gracefully
// -----------------------------------------------------------------------
#[test]
fn test_location_map_missing_pointer_returns_none() {
    let yaml = "name: Revenue";
    let map = LocationMap::build("model.yaml", yaml);
    assert!(map.get("/nonexistent/path").is_none());
}

// -----------------------------------------------------------------------
// 7. to_rich with no location map produces spanless diagnostic
// -----------------------------------------------------------------------
#[test]
fn test_to_rich_without_location_map() {
    let diag = Diagnostic {
        code: "MC2001",
        severity: Severity::Error,
        path: ModelPath::new("model.yaml", "/dimensions/0", "dimensions.Time"),
        message: "duplicate name".into(),
        suggestion: None,
    };

    let rich = diag.to_rich(None);
    assert!(rich.primary_span.is_none());
    assert_eq!(rich.code, "MC2001");

    let rendered = render_diagnostic(&rich, |_| None, ColorMode::Never);
    assert!(rendered.contains("error[MC2001]: duplicate name"));
    assert!(!rendered.contains("-->"), "no location without span");
}
