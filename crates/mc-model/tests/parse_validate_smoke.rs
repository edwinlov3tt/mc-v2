//! Smoke test: parse + validate + compile the canonical Acme YAML
//! without errors. Runs before the heavier structural / golden tests so
//! a parser-schema or compile-stage mismatch surfaces with a focused
//! error message.

use mc_model::{compile, parse, validate};

const ACME: &str = include_str!("../examples/acme.yaml");

#[test]
fn parse_acme_yaml_succeeds() {
    let parsed = parse(ACME, Some("examples/acme.yaml".into())).unwrap_or_else(|e| {
        panic!("parse failed: {e}");
    });
    assert_eq!(parsed.model_format_version, 1);
    assert_eq!(parsed.metadata.name, "Acme_MarketingFinance");
    assert_eq!(parsed.dimensions.len(), 6);
    assert_eq!(parsed.measures.len(), 11);
    assert_eq!(parsed.rules.len(), 5);
}

#[test]
fn validate_acme_yaml_succeeds() {
    let parsed = parse(ACME, Some("examples/acme.yaml".into())).unwrap_or_else(|e| {
        panic!("parse failed: {e}");
    });
    let validated = validate(parsed).unwrap_or_else(|errs| {
        let mut s = String::new();
        for e in &errs {
            s.push_str(&format!("  - {e}\n"));
        }
        panic!("validation failed:\n{s}");
    });
    assert_eq!(validated.parsed.dimensions.len(), 6);
    assert_eq!(validated.measure_dim_index, 5);
}

#[test]
fn compile_acme_yaml_succeeds() {
    let parsed = parse(ACME, Some("examples/acme.yaml".into())).unwrap_or_else(|e| {
        panic!("parse failed: {e}");
    });
    let validated = validate(parsed).unwrap_or_else(|errs| {
        panic!("validation failed: {errs:?}");
    });
    let compiled = compile(validated).unwrap_or_else(|e| {
        panic!("compile failed: {e}");
    });
    assert_eq!(compiled.cube.dimensions().len(), 6);
    assert_eq!(compiled.cube.rules().len(), 5);
    assert_eq!(compiled.refs.dimension_order.len(), 6);
}
