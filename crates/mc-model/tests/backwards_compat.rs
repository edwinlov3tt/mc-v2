//! Phase 3D backwards-compatibility tests.
//!
//! Headline contract: structured-form YAMLs (`body: { mul: [...] }`)
//! authored before Phase 3D continue to load identically — same
//! `ValidatedModel` shape, same MC2xxx errors, same goldens. The
//! `ParsedRuleBodyForm` wrapper added in Phase 3D is invisible to
//! downstream consumers (`compile`, `lint`, `inspect`, `resolve_inputs`)
//! because the validate stage flattens it into bare `ParsedRuleBody`.
//!
//! `_acme_with_bad_golden.yaml` is the canary — a Phase 3C-authored
//! lint fixture that uses the structured form. It lives under
//! `tests/lint_fixtures/` and is NOT migrated to formula form.

use mc_model::schema::{ParsedRuleBody, ParsedRuleBodyForm};
use mc_model::{parse, validate};

const STRUCTURED_FIXTURE: &str = "tests/lint_fixtures/_acme_with_bad_golden.yaml";

#[test]
fn structured_form_fixture_still_parses() {
    let yaml = std::fs::read_to_string(STRUCTURED_FIXTURE).expect("read structured fixture");
    let parsed = parse(&yaml, Some(STRUCTURED_FIXTURE.into())).expect("parse must succeed");
    // Every rule body in this fixture was authored as a YAML mapping →
    // serde must dispatch to ParsedRuleBodyForm::Structured.
    for r in &parsed.rules {
        match &r.body {
            ParsedRuleBodyForm::Structured(_) => {}
            ParsedRuleBodyForm::Formula(_) => panic!(
                "rule {:?} authored as structured YAML must dispatch to Structured(_); got Formula",
                r.name
            ),
        }
    }
}

#[test]
fn structured_form_fixture_validates_to_flat_body() {
    let yaml = std::fs::read_to_string(STRUCTURED_FIXTURE).expect("read structured fixture");
    let parsed = parse(&yaml, Some(STRUCTURED_FIXTURE.into())).expect("parse must succeed");
    let validated = validate(parsed).expect("validate must succeed (this fixture is well-formed)");
    // After validate(), every ValidatedRule.body is a flat
    // ParsedRuleBody — no ParsedRuleBodyForm wrapper visible.
    assert_eq!(
        validated.rules.len(),
        validated.parsed.rules.len(),
        "validated.rules length must match parsed.rules length"
    );
    for r in &validated.rules {
        // The body type is `ParsedRuleBody`, not `ParsedRuleBodyForm`.
        // Compile-time check: this matches against ParsedRuleBody
        // variants directly.
        match &r.body {
            ParsedRuleBody::Const(_)
            | ParsedRuleBody::Ref(_)
            | ParsedRuleBody::Add(_)
            | ParsedRuleBody::Sub(_)
            | ParsedRuleBody::Mul(_)
            | ParsedRuleBody::Div(_)
            | ParsedRuleBody::IfNull(_) => {}
        }
    }
}

#[test]
fn formula_and_structured_acme_produce_equivalent_validated_rules() {
    // Formula-form Acme (the canonical Phase 3D acme.yaml) and
    // structured-form Acme (_acme_with_bad_golden.yaml) authoring of
    // the same rules must yield byte-for-byte identical
    // ValidatedRule.body trees (modulo the Gross_Profit rule's
    // const-1.0 representation).
    //
    // We compare debug formatting of the body trees rule-by-rule. The
    // bad-golden fixture has a different goldens block (intentionally
    // wrong) and a different metadata, but its 5 rules match
    // examples/acme.yaml's 5 rules structurally.
    let formula_yaml = std::fs::read_to_string("examples/acme.yaml").expect("read acme");
    let formula_parsed = parse(&formula_yaml, Some("examples/acme.yaml".into())).expect("parse");
    let formula_validated = validate(formula_parsed).expect("validate formula");

    let structured_yaml = std::fs::read_to_string(STRUCTURED_FIXTURE).expect("read structured");
    let structured_parsed =
        parse(&structured_yaml, Some(STRUCTURED_FIXTURE.into())).expect("parse");
    let structured_validated = validate(structured_parsed).expect("validate structured");

    assert_eq!(formula_validated.rules.len(), 5);
    assert_eq!(structured_validated.rules.len(), 5);
    for (f, s) in formula_validated
        .rules
        .iter()
        .zip(structured_validated.rules.iter())
    {
        assert_eq!(f.name, s.name, "rule names diverged");
        assert_eq!(
            format!("{:?}", f.body),
            format!("{:?}", s.body),
            "rule {:?} body trees diverged between formula and structured authoring forms",
            f.name
        );
    }
}
