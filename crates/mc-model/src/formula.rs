//! Phase 3D friendly-formula parser + serializer.
//!
//! Translates between human-authored formula strings (`"Customers * AOV"`,
//! `"Revenue * (1 - COGS_Rate)"`) and the existing structured-tree
//! [`ParsedRuleBody`] AST. **No new AST variants.** Formulas compile
//! DOWN to the same 7 [`ParsedRuleBody`] variants the structured form
//! produces — Phase 4 (LLM authoring) and Phase 6 (UI editor) consume the
//! tree, not the formula text.
//!
//! # Grammar (recursive descent, left-associative)
//!
//! ```text
//! expression  = term (("+" | "-") term)*
//! term        = factor (("*" | "/") factor)*
//! factor      = "(" expression ")"
//!             | ("+" | "-") factor                     // unary
//!             | identifier "(" expression "," expression ")"  // if_null only
//!             | identifier                              // measure ref
//!             | number
//! identifier  = [A-Za-z_] [A-Za-z0-9_]*
//! number      = digit+ ("." digit+)? ([eE] [+-]? digit+)?
//! ```
//!
//! - **Operators:** `+`, `-`, `*`, `/`, parens, unary `+`/`-`.
//! - **Function calls:** ONLY `if_null(primary, fallback)` — matches the
//!   existing `IfNull` AST variant. Per acceptance amendment #25, any
//!   other identifier-with-parens fires **MC1004** (unknown function
//!   counts as "unexpected token").
//! - **Identifiers:** case-sensitive bare measure names; `if_null` is
//!   the only reserved word.
//! - **Numbers:** all literal numbers map to [`ParsedScalar::Float`].
//!   Integer-shaped literals (`1`, `100`) auto-promote.
//! - **Whitespace:** ignored between tokens.
//! - **Comments:** none.
//!
//! # Unary minus desugaring
//!
//! Per acceptance amendment #22:
//! `-x` → `Sub([Const(F64(0.0)), x])` (NOT `Mul([Const(F64(-1.0)), x])`).
//! Reasons: preserves IEEE-754 signed-zero semantics, cleaner serialization,
//! matches mental model "negate = subtract from zero".
//!
//! Numeric literal folding: `-3.14` is parsed directly as
//! `Const(F64(-3.14))` rather than `Sub(Const(0.0), Const(3.14))`. This
//! makes negative constants round-trip stably (`Const(-1.5)` → `"-1.5"`
//! → `Const(-1.5)`).
//!
//! # Round-trip contract (the risky gate)
//!
//! `parse(serialize(parse(s))) == parse(s)` for every valid formula.
//! The serializer's paren rule pins this:
//!
//! - LEFT child of binary Op: paren iff `prec(left) < prec(Op)`.
//! - RIGHT child of binary Op: paren iff `prec(right) <= prec(Op)`.
//!   (`<=`, not `<`, because every operator is left-associative — same-prec
//!   on the right would otherwise re-group leftward and change the tree.)
//!
//! Precedence table:
//!
//! | Node                                   | Prec |
//! |----------------------------------------|------|
//! | `Const`, `Ref`, `IfNull`, unary minus  | 3    |
//! | `Mul`, `Div`                           | 2    |
//! | `Add`, `Sub`                           | 1    |
//!
//! Worked round-trip cases the test suite pins:
//! - `Sub(a, Sub(b, c))` → `"a - (b - c)"` (right-side same prec → paren)
//! - `Div(a, Div(b, c))` → `"a / (b / c)"`
//! - `Mul(a, Div(b, c))` → `"a * (b / c)"` (right-side same prec → paren)
//! - `Mul(Add(a, b), Sub(c, d))` → `"(a + b) * (c - d)"` (both children
//!   lower prec → both paren)
//! - `Sub(Const(0.0), x)` → `"-<serialize(x)>"` (canonical unary form)
//! - `Mul(Ref("Revenue"), Sub(Const(1.0), Ref("COGS_Rate")))`
//!   → `"Revenue * (1 - COGS_Rate)"` (Acme `Gross_Profit`)

use crate::schema::{
    ParsedAddBody, ParsedConstBody, ParsedDivBody, ParsedIfNullBody, ParsedMulBody, ParsedRefBody,
    ParsedRuleBody, ParsedScalar, ParsedSubBody,
};

// ---------------------------------------------------------------------------
// Public surface
// ---------------------------------------------------------------------------

/// Parse a formula string into a [`ParsedRuleBody`] tree. Whitespace is
/// ignored between tokens; the input must contain exactly one expression
/// (any trailing non-whitespace fires `MC1004`).
pub fn parse(input: &str) -> Result<ParsedRuleBody, FormulaError> {
    let mut p = Parser::new(input);
    p.skip_ws();
    let body = p.parse_expression()?;
    p.skip_ws();
    if p.pos < p.input.len() {
        // Per spec §3 grammar: a formula is one expression. Trailing
        // tokens (e.g., `"Spend ; CPC"`, `"a + b)"`) are MC1004.
        let extra = p.peek_byte().unwrap_or(b' ');
        if extra == b')' {
            return Err(FormulaError::unbalanced_paren(
                p.pos,
                "unexpected closing paren ')'".into(),
            ));
        }
        return Err(FormulaError::unexpected_token(
            p.pos,
            format!("unexpected character {:?} after expression", extra as char),
        ));
    }
    Ok(body)
}

/// Render a [`ParsedRuleBody`] tree back to formula text. The output is
/// minimally parenthesized — parens appear only where required to keep
/// the tree stable through `parse(serialize(_))`.
///
/// Numeric literals use [`f64::to_string`] (Ryu shortest-roundtrip) per
/// acceptance amendment #21 — `1.5` not `1.500000000000000`.
pub fn serialize(body: &ParsedRuleBody) -> String {
    let mut out = String::new();
    write_node(
        &mut out, body, /* outer_prec = */ 0, /* on_right_of_left_assoc = */ false,
    );
    out
}

// ---------------------------------------------------------------------------
// Internal error type
// ---------------------------------------------------------------------------

/// Internal parse-error shape. The validate-stage adapter wraps this in a
/// [`crate::error::ParseError`] variant carrying the rule name + YAML span.
#[derive(Clone, Debug)]
pub struct FormulaError {
    /// Stable diagnostic code (one of `"MC1003"` / `"MC1004"` /
    /// `"MC1005"` / `"MC1006"`).
    pub code: &'static str,
    /// Byte offset of the problem within the formula string.
    pub offset: usize,
    /// Human-readable message (without rule context — the adapter adds it).
    pub message: String,
}

impl FormulaError {
    fn unbalanced_paren(offset: usize, message: String) -> Self {
        Self {
            code: "MC1003",
            offset,
            message,
        }
    }
    fn unexpected_token(offset: usize, message: String) -> Self {
        Self {
            code: "MC1004",
            offset,
            message,
        }
    }
    fn expected_expression(offset: usize, message: String) -> Self {
        Self {
            code: "MC1005",
            offset,
            message,
        }
    }
    fn invalid_number(offset: usize, message: String) -> Self {
        Self {
            code: "MC1006",
            offset,
            message,
        }
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn peek_byte(&self) -> Option<u8> {
        self.input.as_bytes().get(self.pos).copied()
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    fn skip_ws(&mut self) {
        while let Some(b) = self.peek_byte() {
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn parse_expression(&mut self) -> Result<ParsedRuleBody, FormulaError> {
        let mut left = self.parse_term()?;
        loop {
            self.skip_ws();
            let Some(op) = self.peek_byte() else { break };
            if op != b'+' && op != b'-' {
                break;
            }
            self.advance();
            self.skip_ws();
            // A trailing operator (`"Spend +"` or end-of-input here) is
            // MC1005 — the tightest "expected expression" failure mode.
            if self.peek_byte().is_none() {
                return Err(FormulaError::expected_expression(
                    self.pos,
                    "expected expression after operator".into(),
                ));
            }
            let right = self.parse_term()?;
            left = match op {
                b'+' => add_node(left, right),
                b'-' => sub_node(left, right),
                _ => return Err(FormulaError::unexpected_token(self.pos, "internal".into())),
            };
        }
        Ok(left)
    }

    fn parse_term(&mut self) -> Result<ParsedRuleBody, FormulaError> {
        let mut left = self.parse_factor()?;
        loop {
            self.skip_ws();
            let Some(op) = self.peek_byte() else { break };
            if op != b'*' && op != b'/' {
                break;
            }
            self.advance();
            self.skip_ws();
            if self.peek_byte().is_none() {
                return Err(FormulaError::expected_expression(
                    self.pos,
                    "expected expression after operator".into(),
                ));
            }
            let right = self.parse_factor()?;
            left = match op {
                b'*' => mul_node(left, right),
                b'/' => div_node(left, right),
                _ => return Err(FormulaError::unexpected_token(self.pos, "internal".into())),
            };
        }
        Ok(left)
    }

    fn parse_factor(&mut self) -> Result<ParsedRuleBody, FormulaError> {
        self.skip_ws();
        let Some(c) = self.peek_byte() else {
            return Err(FormulaError::expected_expression(
                self.pos,
                "expected expression, found end of formula".into(),
            ));
        };
        match c {
            b'(' => {
                self.advance();
                let inner = self.parse_expression()?;
                self.skip_ws();
                match self.peek_byte() {
                    Some(b')') => {
                        self.advance();
                        Ok(inner)
                    }
                    _ => Err(FormulaError::unbalanced_paren(
                        self.pos,
                        "missing closing paren ')'".into(),
                    )),
                }
            }
            b'+' => {
                // Unary `+x` is a no-op — consume and recurse.
                self.advance();
                self.parse_factor()
            }
            b'-' => {
                // Unary minus. Per amendment #22, `-<expr>` desugars to
                // `Sub([Const(F64(0.0)), <expr>])`. Numeric literal
                // folding: `-3.14` → `Const(F64(-3.14))` directly, so
                // negative constants round-trip cleanly.
                self.advance();
                self.skip_ws();
                if let Some(d) = self.peek_byte() {
                    if d.is_ascii_digit() {
                        let n = self.parse_number()?;
                        return Ok(const_f64(-n));
                    }
                }
                let inner = self.parse_factor()?;
                Ok(sub_node(const_f64(0.0), inner))
            }
            c if c.is_ascii_alphabetic() || c == b'_' => self.parse_identifier_or_call(),
            c if c.is_ascii_digit() => {
                let n = self.parse_number()?;
                Ok(const_f64(n))
            }
            other => Err(FormulaError::unexpected_token(
                self.pos,
                format!("unexpected character {:?}", other as char),
            )),
        }
    }

    fn parse_identifier_or_call(&mut self) -> Result<ParsedRuleBody, FormulaError> {
        let start = self.pos;
        while let Some(b) = self.peek_byte() {
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.advance();
            } else {
                break;
            }
        }
        let name = &self.input[start..self.pos];
        // Save the post-identifier position before skip_ws so the
        // function-call probe doesn't mistake an identifier-then-paren
        // separated by a newline for something else.
        let after_ident = self.pos;
        self.skip_ws();
        if self.peek_byte() == Some(b'(') {
            // Function call. Phase 3D recognizes only `if_null`. Per
            // acceptance amendment #25, any other identifier-with-parens
            // fires MC1004.
            if name != "if_null" {
                return Err(FormulaError::unexpected_token(
                    start,
                    format!("unknown function call '{name}' (Phase 3D recognizes 'if_null' only)"),
                ));
            }
            self.advance(); // consume '('
            let arg1 = self.parse_expression()?;
            self.skip_ws();
            if self.peek_byte() != Some(b',') {
                return Err(FormulaError::unexpected_token(
                    self.pos,
                    "if_null expects exactly 2 arguments separated by ','".into(),
                ));
            }
            self.advance(); // consume ','
            let arg2 = self.parse_expression()?;
            self.skip_ws();
            if self.peek_byte() == Some(b',') {
                return Err(FormulaError::unexpected_token(
                    self.pos,
                    "if_null expects exactly 2 arguments (got more)".into(),
                ));
            }
            match self.peek_byte() {
                Some(b')') => {
                    self.advance();
                    Ok(if_null_node(arg1, arg2))
                }
                _ => Err(FormulaError::unbalanced_paren(
                    self.pos,
                    "missing closing paren ')' on if_null call".into(),
                )),
            }
        } else {
            // Bare identifier → measure ref. Restore pos so any trailing
            // whitespace stays unconsumed (the outer layer handles ws).
            self.pos = after_ident;
            Ok(ref_node(name.to_string()))
        }
    }

    /// Parse a numeric literal: `digit+ ("." digit+)? ([eE] [+-]? digit+)?`.
    /// Detects the canonical MC1006 shapes:
    ///
    /// - `1..5`  — `.` followed by `.` after the integer part.
    /// - `1e`    — exponent marker with no digits after.
    /// - `1.2.3` — second `.` after a complete number.
    /// - `1.`    — `.` with no fractional digits.
    fn parse_number(&mut self) -> Result<f64, FormulaError> {
        let start = self.pos;
        while self.peek_byte().is_some_and(|b| b.is_ascii_digit()) {
            self.advance();
        }
        if self.peek_byte() == Some(b'.') {
            self.advance();
            let frac_start = self.pos;
            while self.peek_byte().is_some_and(|b| b.is_ascii_digit()) {
                self.advance();
            }
            if self.pos == frac_start {
                // No digits after `.`; covers `1.`, `1..5`.
                return Err(FormulaError::invalid_number(
                    start,
                    "invalid number literal: '.' must be followed by digits".into(),
                ));
            }
        }
        if matches!(self.peek_byte(), Some(b'e') | Some(b'E')) {
            self.advance();
            if matches!(self.peek_byte(), Some(b'+') | Some(b'-')) {
                self.advance();
            }
            let exp_start = self.pos;
            while self.peek_byte().is_some_and(|b| b.is_ascii_digit()) {
                self.advance();
            }
            if self.pos == exp_start {
                return Err(FormulaError::invalid_number(
                    start,
                    "invalid number literal: exponent has no digits".into(),
                ));
            }
        }
        // Trailing `.` after a complete number → MC1006 (`1.2.3`).
        if self.peek_byte() == Some(b'.') {
            return Err(FormulaError::invalid_number(
                start,
                "invalid number literal: extra '.' after complete number".into(),
            ));
        }
        let s = &self.input[start..self.pos];
        s.parse::<f64>().map_err(|_| {
            FormulaError::invalid_number(start, format!("invalid number literal {s:?}"))
        })
    }
}

// ---------------------------------------------------------------------------
// AST helpers
// ---------------------------------------------------------------------------

fn const_f64(v: f64) -> ParsedRuleBody {
    ParsedRuleBody::Const(ParsedConstBody {
        value: ParsedScalar::Float(v),
    })
}

fn ref_node(name: String) -> ParsedRuleBody {
    ParsedRuleBody::Ref(ParsedRefBody { measure: name })
}

fn add_node(a: ParsedRuleBody, b: ParsedRuleBody) -> ParsedRuleBody {
    ParsedRuleBody::Add(ParsedAddBody { add: vec![a, b] })
}

fn sub_node(a: ParsedRuleBody, b: ParsedRuleBody) -> ParsedRuleBody {
    ParsedRuleBody::Sub(ParsedSubBody { sub: vec![a, b] })
}

fn mul_node(a: ParsedRuleBody, b: ParsedRuleBody) -> ParsedRuleBody {
    ParsedRuleBody::Mul(ParsedMulBody { mul: vec![a, b] })
}

fn div_node(a: ParsedRuleBody, b: ParsedRuleBody) -> ParsedRuleBody {
    ParsedRuleBody::Div(ParsedDivBody { div: vec![a, b] })
}

fn if_null_node(a: ParsedRuleBody, b: ParsedRuleBody) -> ParsedRuleBody {
    ParsedRuleBody::IfNull(ParsedIfNullBody {
        if_null: vec![a, b],
    })
}

// ---------------------------------------------------------------------------
// Serializer
// ---------------------------------------------------------------------------

/// Detect the canonical unary-minus shape: `Sub([Const(F64(0.0)), x])`.
/// Uses bit-exact zero detection so `-0.0` is NOT treated as the unary
/// sentinel (we always construct positive 0.0 in the parser).
fn unary_minus_inner(body: &ParsedRuleBody) -> Option<&ParsedRuleBody> {
    let ParsedRuleBody::Sub(s) = body else {
        return None;
    };
    if s.sub.len() != 2 {
        return None;
    }
    let ParsedRuleBody::Const(c) = &s.sub[0] else {
        return None;
    };
    match c.value {
        // Positive zero only. -0.0 has different bits and is NOT the
        // canonical sentinel.
        ParsedScalar::Float(v) if v.to_bits() == 0u64 => Some(&s.sub[1]),
        _ => None,
    }
}

/// Operator precedence used by the round-trip paren rule. Higher = binds
/// tighter. Atomic nodes (Const, Ref, IfNull, unary-minus) are 3.
fn prec(body: &ParsedRuleBody) -> u8 {
    if unary_minus_inner(body).is_some() {
        return 3;
    }
    match body {
        ParsedRuleBody::Const(_) | ParsedRuleBody::Ref(_) | ParsedRuleBody::IfNull(_) => 3,
        ParsedRuleBody::Mul(_) | ParsedRuleBody::Div(_) => 2,
        ParsedRuleBody::Add(_) | ParsedRuleBody::Sub(_) => 1,
    }
}

/// Render a node, optionally wrapping in parens to preserve the tree
/// shape under round-trip. Paren rules:
///
/// - `outer_prec = 0` means top-level (no parens needed).
/// - For LEFT child of binary Op (`on_right_of_left_assoc = false`):
///   wrap iff `prec(node) < outer_prec`.
/// - For RIGHT child of binary Op (`on_right_of_left_assoc = true`):
///   wrap iff `prec(node) <= outer_prec`. The `<=` (not `<`) handles
///   left-associativity — `a - (b - c)` must keep parens.
fn write_node(
    out: &mut String,
    body: &ParsedRuleBody,
    outer_prec: u8,
    on_right_of_left_assoc: bool,
) {
    let needs_paren = if outer_prec == 0 {
        false
    } else if on_right_of_left_assoc {
        prec(body) <= outer_prec
    } else {
        prec(body) < outer_prec
    };
    if needs_paren {
        out.push('(');
        write_node_bare(out, body);
        out.push(')');
    } else {
        write_node_bare(out, body);
    }
}

fn write_node_bare(out: &mut String, body: &ParsedRuleBody) {
    if let Some(inner) = unary_minus_inner(body) {
        // Canonical unary form. The inner expression is rendered as a
        // factor — i.e., wrap if it's not atomic. We treat the unary
        // minus as having precedence 3, so `-` followed by a binary
        // child of prec ≤ 2 needs parens (`-(a + b)`, `-(a * b)`).
        out.push('-');
        // The inner is rendered with outer_prec=3 (treat unary as a
        // factor-precedence operator) and not on the right of a
        // left-assoc op (so any prec < 3 wraps).
        write_node(out, inner, 3, false);
        return;
    }
    match body {
        ParsedRuleBody::Const(c) => write_const(out, &c.value),
        ParsedRuleBody::Ref(r) => out.push_str(&r.measure),
        ParsedRuleBody::IfNull(b) => {
            out.push_str("if_null(");
            // Function-call args are fresh expressions (no enclosing
            // operator context), so render with outer_prec=0.
            if let Some(a) = b.if_null.first() {
                write_node(out, a, 0, false);
            }
            out.push_str(", ");
            if let Some(a) = b.if_null.get(1) {
                write_node(out, a, 0, false);
            }
            out.push(')');
        }
        ParsedRuleBody::Add(b) => write_binop(out, &b.add, "+", 1),
        ParsedRuleBody::Sub(b) => write_binop(out, &b.sub, "-", 1),
        ParsedRuleBody::Mul(b) => write_binop(out, &b.mul, "*", 2),
        ParsedRuleBody::Div(b) => write_binop(out, &b.div, "/", 2),
    }
}

fn write_binop(out: &mut String, args: &[ParsedRuleBody], op: &str, op_prec: u8) {
    if args.len() != 2 {
        // Validator rejects non-binary op shapes; we never reach this in
        // the canonical pipeline. Fall back to a stable rendering rather
        // than panic so misuse via direct serialize() calls stays
        // diagnosable.
        out.push('?');
        out.push_str(op);
        out.push('?');
        return;
    }
    write_node(out, &args[0], op_prec, false);
    out.push(' ');
    out.push_str(op);
    out.push(' ');
    write_node(out, &args[1], op_prec, true);
}

fn write_const(out: &mut String, value: &ParsedScalar) {
    match value {
        // Per acceptance amendment #21: f64::to_string is Ryu's
        // shortest-roundtrip — `0.1_f64.to_string()` is `"0.1"`, not
        // `"0.100000000000000"`. Do NOT use format!("{:.15}", v).
        ParsedScalar::Float(v) => out.push_str(&v.to_string()),
        // I64 / Bool aren't reachable from formula syntax in Phase 3D,
        // but ParsedScalar is an open enum — render them explicitly so
        // structured-form authors who use them still get a serialization.
        ParsedScalar::Int(v) => out.push_str(&v.to_string()),
        ParsedScalar::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
    }
}

// ---------------------------------------------------------------------------
// Unit tests (the contract-level round-trip cases live in
// tests/formula_roundtrip.rs).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_ref(body: &ParsedRuleBody, expected: &str) {
        match body {
            ParsedRuleBody::Ref(r) => assert_eq!(r.measure, expected),
            _ => panic!("expected Ref({expected:?}); got {body:?}"),
        }
    }

    #[test]
    fn parse_simple_ref() {
        let b = parse("Spend").unwrap();
        assert_ref(&b, "Spend");
    }

    #[test]
    fn parse_simple_div() {
        let b = parse("Spend / CPC").unwrap();
        let ParsedRuleBody::Div(d) = b else {
            panic!("expected Div; got {b:?}");
        };
        assert_eq!(d.div.len(), 2);
        assert_ref(&d.div[0], "Spend");
        assert_ref(&d.div[1], "CPC");
    }

    #[test]
    fn parse_unary_minus_on_ref() {
        // -Spend → Sub(Const(0.0), Ref(Spend))
        let b = parse("-Spend").unwrap();
        let ParsedRuleBody::Sub(s) = b else {
            panic!("expected Sub for unary minus; got {b:?}");
        };
        let ParsedRuleBody::Const(c) = &s.sub[0] else {
            panic!("expected Const(0.0) on left");
        };
        match c.value {
            ParsedScalar::Float(v) => assert_eq!(v.to_bits(), 0u64),
            _ => panic!("expected F64(0.0)"),
        }
        assert_ref(&s.sub[1], "Spend");
    }

    #[test]
    fn parse_unary_minus_on_number_folds_to_negative_const() {
        let b = parse("-1.5").unwrap();
        let ParsedRuleBody::Const(c) = b else {
            panic!("expected Const for -1.5 fold; got {b:?}");
        };
        match c.value {
            ParsedScalar::Float(v) => assert!((v + 1.5).abs() < 1e-12),
            _ => panic!("expected Float"),
        }
    }

    #[test]
    fn serialize_const_uses_ryu() {
        // Per amendment #21: shortest-roundtrip, not fixed-precision.
        assert_eq!(serialize(&const_f64(1.5)), "1.5");
        assert_eq!(serialize(&const_f64(0.1)), "0.1");
        assert_eq!(serialize(&const_f64(1.0)), "1");
    }

    #[test]
    fn serialize_unary_minus_canonical() {
        // -Spend
        let b = sub_node(const_f64(0.0), ref_node("Spend".into()));
        assert_eq!(serialize(&b), "-Spend");
    }

    #[test]
    fn serialize_subtraction_associativity() {
        // a - (b - c) → "a - (b - c)"
        let inner = sub_node(ref_node("b".into()), ref_node("c".into()));
        let b = sub_node(ref_node("a".into()), inner);
        assert_eq!(serialize(&b), "a - (b - c)");
    }

    #[test]
    fn serialize_division_associativity() {
        let inner = div_node(ref_node("b".into()), ref_node("c".into()));
        let b = div_node(ref_node("a".into()), inner);
        assert_eq!(serialize(&b), "a / (b / c)");
    }

    #[test]
    fn serialize_mul_with_div_on_right_parens() {
        // a * (b / c) MUST keep the right-side parens. Without, it would
        // serialize as "a * b / c" and reparse as Div(Mul(a, b), c).
        let inner = div_node(ref_node("b".into()), ref_node("c".into()));
        let b = mul_node(ref_node("a".into()), inner);
        assert_eq!(serialize(&b), "a * (b / c)");
    }

    #[test]
    fn parse_unknown_function_call_fires_mc1004() {
        let err = parse("min(Spend, CPC)").unwrap_err();
        assert_eq!(err.code, "MC1004");
    }

    #[test]
    fn parse_trailing_operator_fires_mc1005() {
        let err = parse("Spend +").unwrap_err();
        assert_eq!(err.code, "MC1005");
    }

    #[test]
    fn parse_unbalanced_paren_fires_mc1003() {
        let err = parse("(Spend / CPC").unwrap_err();
        assert_eq!(err.code, "MC1003");
        let err = parse("Spend / CPC)").unwrap_err();
        assert_eq!(err.code, "MC1003");
    }

    #[test]
    fn parse_invalid_number_fires_mc1006() {
        for src in ["1..5", "1e", "1.2.3"] {
            let err = parse(src).unwrap_err();
            assert_eq!(err.code, "MC1006", "input was {src:?}");
        }
    }

    #[test]
    fn parse_if_null_arity_fires_mc1004() {
        let err = parse("if_null(Spend)").unwrap_err();
        assert_eq!(err.code, "MC1004");
        let err = parse("if_null(Spend, CPC, AOV)").unwrap_err();
        assert_eq!(err.code, "MC1004");
    }
}
