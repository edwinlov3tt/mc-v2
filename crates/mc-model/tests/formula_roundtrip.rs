//! Phase 3D round-trip stability test (the explicit risky-case list per
//! handoff §"Acceptance gate" item 9 + Phase 3D handoff §C).
//!
//! Contract: `parse(serialize(parse(s))) == parse(s)` for every input.
//! Equality is on the [`ParsedRuleBody`] AST — whitespace differences in
//! the intermediate string don't matter, but tree shape must be exact.
//!
//! The risky shapes the test pins (per the handoff's enumerated list +
//! the project owner's tightened serializer rule):
//!
//! 1. **Subtraction associativity**: `a - (b - c)` ≠ `(a - b) - c`.
//! 2. **Division associativity**: `a / (b / c)` ≠ `(a / b) / c`.
//! 3. **Mul-with-Div on right**: `a * (b / c)` ≠ `(a * b) / c`. (Without
//!    the right-side same-prec paren rule, this round-trip silently
//!    breaks because `*` and `/` left-associate to one another.)
//! 4. **Nested binary on both sides**: `(a + b) * (c - d)`.
//! 5. **Acme `Gross_Profit`**: `Revenue * (1 - COGS_Rate)`.
//! 6. **Unary minus**: `-Spend` round-trips through
//!    `Sub([Const(F64(0.0)), Ref("Spend")])` and back to `-Spend` (NOT
//!    `0 - Spend`).
//! 7. **Right-nested same-prec Add/Mul** (per project-owner serializer
//!    tightening): `Add(a, Add(b, c))` and `Mul(a, Mul(b, c))` must
//!    keep their tree shape — i.e. serialize as `a + (b + c)` and
//!    `a * (b * c)`.

use mc_model::formula;

/// Assert AST-equality on round-trip. We compare the debug formatting
/// since `ParsedRuleBody` doesn't derive `PartialEq` (the underlying
/// `f64` would block it). `Debug` is structural and stable enough for
/// this purpose.
fn assert_round_trip(input: &str) {
    let parsed_once =
        formula::parse(input).unwrap_or_else(|e| panic!("parse failed for {input:?}: {e:?}"));
    let serialized = formula::serialize(&parsed_once);
    let parsed_twice = formula::parse(&serialized).unwrap_or_else(|e| {
        panic!("round-trip parse failed: input={input:?} serialized={serialized:?} err={e:?}")
    });
    assert_eq!(
        format!("{parsed_once:?}"),
        format!("{parsed_twice:?}"),
        "round-trip drifted: input={input:?} serialized={serialized:?}"
    );
}

/// Like `assert_round_trip`, plus pin the exact serialized form. Use
/// when the serializer's paren placement is itself the contract under
/// test (e.g., the canonical unary `-x` form, or the right-side
/// same-prec parens).
fn assert_round_trip_serializes_to(input: &str, expected_serialized: &str) {
    let parsed =
        formula::parse(input).unwrap_or_else(|e| panic!("parse failed for {input:?}: {e:?}"));
    let serialized = formula::serialize(&parsed);
    assert_eq!(
        serialized, expected_serialized,
        "serializer output mismatch for {input:?}"
    );
    let reparsed = formula::parse(&serialized).expect("reparse");
    assert_eq!(format!("{parsed:?}"), format!("{reparsed:?}"));
}

// ---------------------------------------------------------------------------
// 1. Subtraction associativity
// ---------------------------------------------------------------------------

#[test]
fn round_trip_subtraction_left_grouped() {
    assert_round_trip_serializes_to("a - b - c", "a - b - c");
}

#[test]
fn round_trip_subtraction_right_grouped_keeps_parens() {
    assert_round_trip_serializes_to("a - (b - c)", "a - (b - c)");
    // Confirm the two trees are NOT the same.
    let left_tree = formula::parse("a - b - c").unwrap();
    let right_tree = formula::parse("a - (b - c)").unwrap();
    assert_ne!(format!("{left_tree:?}"), format!("{right_tree:?}"));
}

// ---------------------------------------------------------------------------
// 2. Division associativity
// ---------------------------------------------------------------------------

#[test]
fn round_trip_division_left_grouped() {
    assert_round_trip_serializes_to("a / b / c", "a / b / c");
}

#[test]
fn round_trip_division_right_grouped_keeps_parens() {
    assert_round_trip_serializes_to("a / (b / c)", "a / (b / c)");
}

// ---------------------------------------------------------------------------
// 3. Mul with Div on right (the project-owner-tightened rule)
// ---------------------------------------------------------------------------

#[test]
fn round_trip_mul_with_div_on_right_keeps_parens() {
    // Without the right-side same-prec paren, this would serialize as
    // "a * b / c" and reparse to Div(Mul(a, b), c) — different tree.
    assert_round_trip_serializes_to("a * (b / c)", "a * (b / c)");
}

#[test]
fn round_trip_div_with_mul_on_right_keeps_parens() {
    assert_round_trip_serializes_to("a / (b * c)", "a / (b * c)");
}

// ---------------------------------------------------------------------------
// 4. Nested binary expressions on both sides
// ---------------------------------------------------------------------------

#[test]
fn round_trip_nested_add_times_sub() {
    // (a + b) * (c - d) — both children lower prec → both paren.
    assert_round_trip_serializes_to("(a + b) * (c - d)", "(a + b) * (c - d)");
}

#[test]
fn round_trip_nested_mul_into_add() {
    // a + b * c — Mul has higher prec than Add → no parens.
    assert_round_trip_serializes_to("a + b * c", "a + b * c");
}

// ---------------------------------------------------------------------------
// 5. Acme Gross_Profit
// ---------------------------------------------------------------------------

#[test]
fn round_trip_acme_gross_profit() {
    assert_round_trip_serializes_to("Revenue * (1 - COGS_Rate)", "Revenue * (1 - COGS_Rate)");
}

// ---------------------------------------------------------------------------
// 6. Unary minus
// ---------------------------------------------------------------------------

#[test]
fn round_trip_unary_minus_canonical_form() {
    // -Spend round-trips through Sub(0, Ref) and back to "-Spend",
    // NOT "0 - Spend".
    assert_round_trip_serializes_to("-Spend", "-Spend");
}

#[test]
fn round_trip_unary_minus_on_negative_literal() {
    // -1.5 parses directly to Const(-1.5) (numeric fold). Round-trip
    // serializes it as "-1.5" and reparses to the same Const(-1.5).
    assert_round_trip_serializes_to("-1.5", "-1.5");
}

#[test]
fn round_trip_unary_minus_inside_mul_no_parens_needed() {
    // Mul(a, Sub(0, b)) — the unary form `-b` is treated as a factor, no
    // parens needed.
    assert_round_trip_serializes_to("a * -b", "a * -b");
}

// ---------------------------------------------------------------------------
// 7. Right-nested same-prec Add and Mul (project-owner tightening)
// ---------------------------------------------------------------------------

#[test]
fn round_trip_right_nested_add_keeps_parens() {
    // Add(a, Add(b, c)) must serialize as "a + (b + c)" — not "a + b + c"
    // which would reparse left-grouped to Add(Add(a, b), c).
    assert_round_trip_serializes_to("a + (b + c)", "a + (b + c)");
}

#[test]
fn round_trip_right_nested_mul_keeps_parens() {
    assert_round_trip_serializes_to("a * (b * c)", "a * (b * c)");
}

// ---------------------------------------------------------------------------
// All five Acme formulas (the headline gate)
// ---------------------------------------------------------------------------

#[test]
fn round_trip_all_five_acme_formulas() {
    let acme_formulas = [
        "Spend / CPC",
        "Clicks * CVR",
        "Leads * Close_Rate",
        "Customers * AOV",
        "Revenue * (1 - COGS_Rate)",
    ];
    for f in acme_formulas {
        assert_round_trip(f);
    }
}

// ---------------------------------------------------------------------------
// if_null and mixed shapes
// ---------------------------------------------------------------------------

#[test]
fn round_trip_if_null() {
    assert_round_trip_serializes_to("if_null(Spend, 0)", "if_null(Spend, 0)");
}

#[test]
fn round_trip_if_null_with_complex_args() {
    assert_round_trip_serializes_to("if_null(a / b, c + d)", "if_null(a / b, c + d)");
}

#[test]
fn round_trip_deeply_nested_expression() {
    assert_round_trip("(a + b * c) / (d - e)");
    assert_round_trip("if_null(a * (b - c), d / (e + f))");
}
