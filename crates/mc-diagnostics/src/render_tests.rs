//! Golden renderer tests per ADR-0024 Decision 8.
//!
//! 10 tests covering the renderer's output contract:
//! 1. Single-line underline
//! 2. Multi-line underline
//! 3. Related spans in two locations
//! 4. Help and note rendering
//! 5. Suggestion::Replace rendering
//! 6. Tab alignment (tabs → 4 spaces)
//! 7. UTF-8 content before the span
//! 8. ColorMode::Never has zero ANSI escapes
//! 9. Missing source file degrades gracefully
//! 10. Empty/None span renders code+message only

use crate::{
    render_diagnostic, ColorMode, DiagSeverity, RichDiagnostic, SourceSpan, Suggestion,
    SuggestionKind,
};

fn provider_with(content: &str) -> impl Fn(&str) -> Option<String> + '_ {
    move |_path: &str| Some(content.to_string())
}

// -----------------------------------------------------------------------
// 1. Single-line underline
// -----------------------------------------------------------------------
#[test]
fn golden_single_line_underline() {
    let source = "dimensions:\n  - name: Time\n    elements: [Q1, Q2]\nmeasures:\n  - name: Revenue\n    body: \"Custmers * AOV\"";
    let diag = RichDiagnostic::new("MC2015", DiagSeverity::Error, "measure not found")
        .with_span(SourceSpan::new("model.yaml", 89, 97)); // "Custmers"

    let rendered = render_diagnostic(&diag, provider_with(source), ColorMode::Never);

    assert!(
        rendered.contains("error[MC2015]: measure not found"),
        "header missing"
    );
    assert!(
        rendered.contains("--> model.yaml:6:"),
        "location missing, got:\n{}",
        rendered
    );
    assert!(rendered.contains("Custmers * AOV"), "source line missing");
    assert!(
        rendered.contains("^^^^^^^^"),
        "underline missing or wrong length"
    );
}

// -----------------------------------------------------------------------
// 2. Multi-line underline
// -----------------------------------------------------------------------
#[test]
fn golden_multi_line_underline() {
    let source = "body: |\n  Customers\n    * AOV\n    + Misspeled";
    // Span covers "Customers\n    * AOV\n    + Misspeled" (bytes 8..44)
    let diag = RichDiagnostic::new("MC1004", DiagSeverity::Error, "invalid formula")
        .with_span(SourceSpan::new("model.yaml", 8, 44));

    let rendered = render_diagnostic(&diag, provider_with(source), ColorMode::Never);

    assert!(
        rendered.contains("error[MC1004]: invalid formula"),
        "header missing"
    );
    assert!(rendered.contains("Customers"), "first line missing");
    assert!(rendered.contains("Misspeled"), "last line missing");
    // Should have underlines on first and last line
    assert!(rendered.contains('^'), "underlines missing");
}

// -----------------------------------------------------------------------
// 3. Related spans in two locations
// -----------------------------------------------------------------------
#[test]
fn golden_related_spans_two_locations() {
    let source_a = "templates:\n  - name: revenue\n    priority: 10";
    let source_b = "templates:\n  - name: growth\n    priority: 10";

    let diag = RichDiagnostic::new(
        "MC7050",
        DiagSeverity::Warning,
        "priority collision between templates",
    )
    .with_span(SourceSpan::new("template_a.yaml", 38, 40))
    .with_related(
        SourceSpan::new("template_b.yaml", 37, 39),
        "also has priority 10",
    );

    let rendered = render_diagnostic(
        &diag,
        |path| match path {
            "template_a.yaml" => Some(source_a.to_string()),
            "template_b.yaml" => Some(source_b.to_string()),
            _ => None,
        },
        ColorMode::Never,
    );

    assert!(
        rendered.contains("--> template_a.yaml:"),
        "primary location missing"
    );
    assert!(
        rendered.contains("--> template_b.yaml:"),
        "related location missing"
    );
    assert!(
        rendered.contains("also has priority 10"),
        "related label missing"
    );
}

// -----------------------------------------------------------------------
// 4. Help and note rendering
// -----------------------------------------------------------------------
#[test]
fn golden_help_and_note() {
    let source = "measures:\n  - name: CPC\n    body: \"Spend / Clicks\"";
    let diag = RichDiagnostic::new("MC2015", DiagSeverity::Error, "measure not found")
        .with_span(SourceSpan::new("model.yaml", 36, 41)) // "Spend"
        .with_note("available measures: Customers, AOV, Revenue, Spend, Impressions")
        .with_help("check spelling of measure names");

    let rendered = render_diagnostic(&diag, provider_with(source), ColorMode::Never);

    assert!(
        rendered
            .contains("= note: available measures: Customers, AOV, Revenue, Spend, Impressions"),
        "note line missing"
    );
    assert!(
        rendered.contains("= help: check spelling of measure names"),
        "help line missing"
    );
}

// -----------------------------------------------------------------------
// 5. Suggestion::Replace rendering
// -----------------------------------------------------------------------
#[test]
fn golden_suggestion_replace() {
    let source = "measures:\n  - name: Revenue\n    body: \"Custmers * AOV\"";
    let bad_span = SourceSpan::new("model.yaml", 40, 48); // "Custmers"
    let diag = RichDiagnostic::new("MC2015", DiagSeverity::Error, "measure not found")
        .with_span(bad_span.clone())
        .with_suggestion(Suggestion {
            message: "did you mean".into(),
            kind: SuggestionKind::Replace {
                span: bad_span,
                replacement: "Customers".into(),
            },
        });

    let rendered = render_diagnostic(&diag, provider_with(source), ColorMode::Never);

    assert!(
        rendered.contains("= help: did you mean: `Customers`"),
        "suggestion replace missing, got:\n{}",
        rendered
    );
}

// -----------------------------------------------------------------------
// 6. Tab alignment (tabs → 4 spaces)
// -----------------------------------------------------------------------
#[test]
fn golden_tab_alignment() {
    let source = "body:\t\"bad token\"";
    // "bad token" starts after the tab + quote, at byte 7
    let diag = RichDiagnostic::new("MC1004", DiagSeverity::Error, "unexpected token")
        .with_span(SourceSpan::new("model.yaml", 6, 15)); // "bad token"

    let rendered = render_diagnostic(&diag, provider_with(source), ColorMode::Never);

    // The tab should be expanded; the underline should align
    assert!(
        !rendered.contains('\t'),
        "tabs should be expanded to spaces"
    );
    assert!(
        rendered.contains("^^^^^^^^^"),
        "underline for 'bad token' (9 chars)"
    );
}

// -----------------------------------------------------------------------
// 7. UTF-8 content before the span
// -----------------------------------------------------------------------
#[test]
fn golden_utf8_before_span() {
    // "café" is 5 bytes (c=1, a=1, f=1, é=2), then ": " (2 bytes), then "bad" at byte 7
    let source = "café: bad";
    let diag = RichDiagnostic::new("MC2001", DiagSeverity::Error, "invalid value")
        .with_span(SourceSpan::new("model.yaml", 7, 10)); // "bad"

    let rendered = render_diagnostic(&diag, provider_with(source), ColorMode::Never);

    assert!(
        rendered.contains("café: bad"),
        "source line should preserve UTF-8"
    );
    assert!(rendered.contains("^^^"), "underline for 'bad' (3 bytes)");
}

// -----------------------------------------------------------------------
// 8. ColorMode::Never has zero ANSI escapes
// -----------------------------------------------------------------------
#[test]
fn golden_no_ansi_in_never_mode() {
    let source = "body: \"Custmers * AOV\"";
    let diag = RichDiagnostic::new("MC2015", DiagSeverity::Error, "measure not found")
        .with_span(SourceSpan::new("model.yaml", 7, 15))
        .with_note("available measures: Customers")
        .with_help("check spelling");

    let rendered = render_diagnostic(&diag, provider_with(source), ColorMode::Never);

    assert!(
        !rendered.contains('\x1b'),
        "ColorMode::Never must produce zero ANSI escapes, got:\n{:?}",
        rendered
    );
}

// -----------------------------------------------------------------------
// 9. Missing source file degrades gracefully
// -----------------------------------------------------------------------
#[test]
fn golden_missing_source_degrades() {
    let diag = RichDiagnostic::new("MC2015", DiagSeverity::Error, "measure not found")
        .with_span(SourceSpan::new("missing.yaml", 100, 108));

    let rendered = render_diagnostic(&diag, |_| None, ColorMode::Never);

    assert!(
        rendered.contains("error[MC2015]: measure not found"),
        "header must still render"
    );
    assert!(rendered.contains("missing.yaml"), "file name should appear");
    // Should NOT contain underlines since there's no source
    assert!(!rendered.contains("^^^"), "no underlines without source");
}

// -----------------------------------------------------------------------
// 10. Empty/None span renders code+message only
// -----------------------------------------------------------------------
#[test]
fn golden_no_span_renders_code_and_message() {
    let diag = RichDiagnostic::new(
        "MC2001",
        DiagSeverity::Warning,
        "dimension has no description",
    );

    let rendered = render_diagnostic(&diag, |_| None, ColorMode::Never);

    assert!(
        rendered.contains("warning[MC2001]: dimension has no description"),
        "header must render"
    );
    // No location arrow, no source context
    assert!(!rendered.contains("-->"), "no location arrow without span");
    assert!(!rendered.contains(" | "), "no gutter without span");
}
