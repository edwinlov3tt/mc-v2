//! Phase 7A.6 D2 regression tests for narrative rich diagnostics.

use mc_diagnostics::{render_diagnostic, ColorMode, SourceSpan};
use mc_narrative::NarrativeError;

// -----------------------------------------------------------------------
// 1. MC7050 renders as rich diagnostic
// -----------------------------------------------------------------------
#[test]
fn test_narrative_mc7050_renders_as_rich() {
    let err = NarrativeError::ExplanationPriorityCollision {
        finding_id: "revenue_change".into(),
        priority: 10,
        template_a: "revenue_up".into(),
        template_b: "revenue_down".into(),
    };

    let rich = err.to_rich();
    assert_eq!(rich.code, "MC7050");
    assert!(rich.message.contains("revenue_up"));
    assert!(rich.message.contains("revenue_down"));

    let rendered = render_diagnostic(&rich, |_| None, ColorMode::Never);
    assert!(
        rendered.contains("error[MC7050]"),
        "should have error header"
    );
}

// -----------------------------------------------------------------------
// 2. MC7050 with two related spans (canonical multi-location case)
// -----------------------------------------------------------------------
#[test]
fn test_narrative_mc7050_two_related_spans() {
    let err = NarrativeError::ExplanationPriorityCollision {
        finding_id: "revenue_change".into(),
        priority: 10,
        template_a: "revenue_up".into(),
        template_b: "revenue_down".into(),
    };

    let mut rich = err.to_rich();
    // Manually attach source spans (in production, a LocationMap would provide these)
    rich.primary_span = Some(SourceSpan::new("templates/revenue.yaml", 100, 102));
    rich = rich.with_related(
        SourceSpan::new("templates/revenue_alt.yaml", 50, 52),
        "also has priority 10",
    );

    let source_a = "templates:\n  - id: revenue_up\n    finding_id: revenue_change\n    explanation_priority: 10\n    body: \"Revenue is up\"";
    let source_b = "templates:\n  - id: revenue_down\n    finding_id: revenue_change\n    explanation_priority: 10\n    body: \"Revenue is down\"";

    let rendered = render_diagnostic(
        &rich,
        |path| match path {
            "templates/revenue.yaml" => Some(source_a.to_string()),
            "templates/revenue_alt.yaml" => Some(source_b.to_string()),
            _ => None,
        },
        ColorMode::Never,
    );

    assert!(
        rendered.contains("--> templates/revenue.yaml:"),
        "primary location missing"
    );
    assert!(
        rendered.contains("--> templates/revenue_alt.yaml:"),
        "related location missing"
    );
    assert!(
        rendered.contains("also has priority 10"),
        "related label missing"
    );
}

// -----------------------------------------------------------------------
// 3. Narrative error renders with underline
// -----------------------------------------------------------------------
#[test]
fn test_narrative_error_renders_with_underline() {
    let err = NarrativeError::UnknownMeasure {
        template_id: "revenue_summary".into(),
        measure: "Revnue".into(),
    };

    let mut rich = err.to_rich();
    rich.primary_span = Some(SourceSpan::new("templates.yaml", 57, 63)); // "Revnue"

    let source = "templates:\n  - id: revenue_summary\n    measures:\n      - Revnue";

    let rendered = render_diagnostic(&rich, |_| Some(source.to_string()), ColorMode::Never);
    assert!(rendered.contains("error[MC7001]"));
    assert!(rendered.contains("Revnue"), "source line should appear");
    assert!(
        rendered.contains("^^^^^^"),
        "should underline 'Revnue' (6 chars)"
    );
}
