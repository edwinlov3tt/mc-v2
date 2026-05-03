//! Phase 3D formula parser unit tests.
//!
//! Covers operators, precedence, parens, unary, identifiers, numbers, and
//! scientific notation. The round-trip stability cases live in
//! `tests/formula_roundtrip.rs`; the negative MC1003–MC1006 cases live in
//! `tests/formula_validators.rs`.

use mc_model::formula;
use mc_model::schema::{ParsedRuleBody, ParsedScalar};

fn as_ref(body: &ParsedRuleBody) -> &str {
    match body {
        ParsedRuleBody::Ref(r) => &r.measure,
        _ => panic!("expected Ref; got {body:?}"),
    }
}

fn as_const_f64(body: &ParsedRuleBody) -> f64 {
    match body {
        ParsedRuleBody::Const(c) => match c.value {
            ParsedScalar::Float(v) => v,
            _ => panic!("expected Float const; got {:?}", c.value),
        },
        _ => panic!("expected Const; got {body:?}"),
    }
}

// ---------------------------------------------------------------------------
// Identifiers
// ---------------------------------------------------------------------------

#[test]
fn parse_bare_identifier() {
    let b = formula::parse("Spend").unwrap();
    assert_eq!(as_ref(&b), "Spend");
}

#[test]
fn parse_underscore_identifier() {
    let b = formula::parse("Close_Rate").unwrap();
    assert_eq!(as_ref(&b), "Close_Rate");
}

#[test]
fn parse_underscore_leading_identifier() {
    let b = formula::parse("_internal_x9").unwrap();
    assert_eq!(as_ref(&b), "_internal_x9");
}

#[test]
fn identifiers_are_case_sensitive() {
    let lower = formula::parse("spend").unwrap();
    let upper = formula::parse("Spend").unwrap();
    assert_eq!(as_ref(&lower), "spend");
    assert_eq!(as_ref(&upper), "Spend");
    assert_ne!(as_ref(&lower), as_ref(&upper));
}

// ---------------------------------------------------------------------------
// Numbers
// ---------------------------------------------------------------------------

#[test]
fn parse_integer_literal_promotes_to_f64() {
    let b = formula::parse("42").unwrap();
    assert_eq!(as_const_f64(&b), 42.0);
}

#[test]
fn parse_decimal_literal() {
    let b = formula::parse("1.5").unwrap();
    assert_eq!(as_const_f64(&b), 1.5);
}

#[test]
fn parse_scientific_notation() {
    let b = formula::parse("1.5e2").unwrap();
    assert!((as_const_f64(&b) - 150.0).abs() < 1e-9);
}

#[test]
fn parse_scientific_notation_signed_exponent() {
    let b = formula::parse("2.5e-3").unwrap();
    assert!((as_const_f64(&b) - 0.0025).abs() < 1e-12);
    let b = formula::parse("2e+4").unwrap();
    assert!((as_const_f64(&b) - 20000.0).abs() < 1e-9);
}

// ---------------------------------------------------------------------------
// Whitespace handling
// ---------------------------------------------------------------------------

#[test]
fn whitespace_around_operators_is_ignored() {
    let a = formula::parse("Spend/CPC").unwrap();
    let b = formula::parse("Spend / CPC").unwrap();
    let c = formula::parse("  Spend   /   CPC  ").unwrap();
    // Same AST shape — Div of two Refs.
    for body in [a, b, c] {
        let ParsedRuleBody::Div(d) = body else {
            panic!("expected Div");
        };
        assert_eq!(d.div.len(), 2);
        assert_eq!(as_ref(&d.div[0]), "Spend");
        assert_eq!(as_ref(&d.div[1]), "CPC");
    }
}

#[test]
fn whitespace_inside_function_call_args_is_ignored() {
    let body = formula::parse("if_null( Spend , CPC )").unwrap();
    let ParsedRuleBody::IfNull(b) = body else {
        panic!("expected IfNull");
    };
    assert_eq!(b.if_null.len(), 2);
    assert_eq!(as_ref(&b.if_null[0]), "Spend");
    assert_eq!(as_ref(&b.if_null[1]), "CPC");
}

// ---------------------------------------------------------------------------
// Precedence — multiplicative binds tighter than additive (left-assoc).
// ---------------------------------------------------------------------------

#[test]
fn mul_binds_tighter_than_add() {
    // a + b * c → Add(a, Mul(b, c))
    let body = formula::parse("a + b * c").unwrap();
    let ParsedRuleBody::Add(top) = body else {
        panic!("expected Add at top");
    };
    assert_eq!(as_ref(&top.add[0]), "a");
    let ParsedRuleBody::Mul(right) = &top.add[1] else {
        panic!("expected Mul on right");
    };
    assert_eq!(as_ref(&right.mul[0]), "b");
    assert_eq!(as_ref(&right.mul[1]), "c");
}

#[test]
fn add_left_associates() {
    // a + b + c → Add(Add(a, b), c)
    let body = formula::parse("a + b + c").unwrap();
    let ParsedRuleBody::Add(top) = body else {
        panic!("expected Add at top");
    };
    let ParsedRuleBody::Add(left) = &top.add[0] else {
        panic!("expected Add on left");
    };
    assert_eq!(as_ref(&left.add[0]), "a");
    assert_eq!(as_ref(&left.add[1]), "b");
    assert_eq!(as_ref(&top.add[1]), "c");
}

#[test]
fn sub_left_associates() {
    // a - b - c → Sub(Sub(a, b), c)
    let body = formula::parse("a - b - c").unwrap();
    let ParsedRuleBody::Sub(top) = body else {
        panic!("expected Sub at top");
    };
    let ParsedRuleBody::Sub(left) = &top.sub[0] else {
        panic!("expected Sub on left");
    };
    assert_eq!(as_ref(&left.sub[0]), "a");
    assert_eq!(as_ref(&left.sub[1]), "b");
    assert_eq!(as_ref(&top.sub[1]), "c");
}

#[test]
fn parens_force_right_grouping() {
    // a - (b - c) → Sub(a, Sub(b, c))  (different from a - b - c)
    let body = formula::parse("a - (b - c)").unwrap();
    let ParsedRuleBody::Sub(top) = body else {
        panic!("expected Sub at top");
    };
    assert_eq!(as_ref(&top.sub[0]), "a");
    let ParsedRuleBody::Sub(right) = &top.sub[1] else {
        panic!("expected Sub on right");
    };
    assert_eq!(as_ref(&right.sub[0]), "b");
    assert_eq!(as_ref(&right.sub[1]), "c");
}

// ---------------------------------------------------------------------------
// Unary minus / plus
// ---------------------------------------------------------------------------

#[test]
fn unary_plus_is_noop() {
    let body = formula::parse("+Spend").unwrap();
    assert_eq!(as_ref(&body), "Spend");
}

#[test]
fn unary_minus_on_ref_desugars_to_sub_of_zero() {
    // Per acceptance amendment #22: -x → Sub([Const(F64(0.0)), x]).
    let body = formula::parse("-Spend").unwrap();
    let ParsedRuleBody::Sub(s) = body else {
        panic!("expected Sub");
    };
    assert_eq!(s.sub.len(), 2);
    let ParsedRuleBody::Const(c) = &s.sub[0] else {
        panic!("expected Const(0.0) on left");
    };
    match c.value {
        ParsedScalar::Float(v) => assert_eq!(v.to_bits(), 0u64, "must be exact +0.0"),
        _ => panic!("expected Float"),
    }
    assert_eq!(as_ref(&s.sub[1]), "Spend");
}

#[test]
fn unary_minus_on_number_folds_to_negative_const() {
    // -2.5 parses directly as Const(-2.5), NOT Sub(Const(0), Const(2.5)).
    // This makes negative literals round-trip through serialize → parse.
    // (We use 2.5 not 3.14 to avoid clippy::approx_constant matching PI.)
    let body = formula::parse("-2.5").unwrap();
    assert!((as_const_f64(&body) - (-2.5)).abs() < 1e-12);
}

#[test]
fn unary_minus_inside_expression() {
    // a + -b → Add(a, Sub(0, b))
    let body = formula::parse("a + -b").unwrap();
    let ParsedRuleBody::Add(top) = body else {
        panic!("expected Add");
    };
    assert_eq!(as_ref(&top.add[0]), "a");
    let ParsedRuleBody::Sub(s) = &top.add[1] else {
        panic!("expected Sub on right");
    };
    assert_eq!(as_ref(&s.sub[1]), "b");
}

#[test]
fn double_unary_minus_chains() {
    // --c parses fine: outer unary on inner unary on identifier.
    let body = formula::parse("--c").unwrap();
    let ParsedRuleBody::Sub(outer) = body else {
        panic!("expected Sub at top");
    };
    let ParsedRuleBody::Sub(inner) = &outer.sub[1] else {
        panic!("expected nested Sub");
    };
    assert_eq!(as_ref(&inner.sub[1]), "c");
}

// ---------------------------------------------------------------------------
// if_null
// ---------------------------------------------------------------------------

#[test]
fn parse_if_null_with_two_args() {
    let body = formula::parse("if_null(Spend, 0)").unwrap();
    let ParsedRuleBody::IfNull(b) = body else {
        panic!("expected IfNull");
    };
    assert_eq!(b.if_null.len(), 2);
    assert_eq!(as_ref(&b.if_null[0]), "Spend");
    assert_eq!(as_const_f64(&b.if_null[1]), 0.0);
}

#[test]
fn if_null_args_can_be_complex_expressions() {
    let body = formula::parse("if_null(a + b, c * d)").unwrap();
    let ParsedRuleBody::IfNull(b) = body else {
        panic!("expected IfNull");
    };
    assert!(matches!(&b.if_null[0], ParsedRuleBody::Add(_)));
    assert!(matches!(&b.if_null[1], ParsedRuleBody::Mul(_)));
}

#[test]
fn nested_if_null() {
    let body = formula::parse("if_null(if_null(a, b), c)").unwrap();
    let ParsedRuleBody::IfNull(outer) = body else {
        panic!("expected outer IfNull");
    };
    assert!(matches!(&outer.if_null[0], ParsedRuleBody::IfNull(_)));
    assert_eq!(as_ref(&outer.if_null[1]), "c");
}

// ---------------------------------------------------------------------------
// Acme rules — validate the exact AST shape a formula author would produce.
// ---------------------------------------------------------------------------

#[test]
fn acme_clicks_formula_parses_to_div() {
    let body = formula::parse("Spend / CPC").unwrap();
    let ParsedRuleBody::Div(d) = body else {
        panic!("expected Div");
    };
    assert_eq!(as_ref(&d.div[0]), "Spend");
    assert_eq!(as_ref(&d.div[1]), "CPC");
}

#[test]
fn acme_gross_profit_formula_has_mul_wrapping_sub_with_const() {
    // body: "Revenue * (1 - COGS_Rate)"
    // → Mul(Ref("Revenue"), Sub(Const(1.0), Ref("COGS_Rate")))
    let body = formula::parse("Revenue * (1 - COGS_Rate)").unwrap();
    let ParsedRuleBody::Mul(m) = body else {
        panic!("expected Mul at top");
    };
    assert_eq!(as_ref(&m.mul[0]), "Revenue");
    let ParsedRuleBody::Sub(s) = &m.mul[1] else {
        panic!("expected Sub on right");
    };
    assert_eq!(as_const_f64(&s.sub[0]), 1.0);
    assert_eq!(as_ref(&s.sub[1]), "COGS_Rate");
}
