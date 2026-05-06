//! Phase 3D+3E+3F+3G friendly-formula parser + serializer.
//!
//! Translates between human-authored formula strings and the
//! [`ParsedRuleBody`] AST. The parser is a hand-rolled recursive-descent
//! parser with the following precedence levels (lowest to highest):
//!
//! ```text
//! or_expression      = and_expression ("or" and_expression)*
//! and_expression     = not_expression ("and" not_expression)*
//! not_expression     = "not" not_expression | comparison
//! comparison         = expression ((">" | "<" | ">=" | "<=" | "==" | "!=") expression)?
//! expression         = term (("+" | "-") term)*
//! term               = factor (("*" | "/") factor)*
//! factor             = "(" or_expression ")"
//!                    | ("+" | "-") factor
//!                    | "Null"
//!                    | identifier "(" args ")"    // function call
//!                    | identifier                  // measure ref
//!                    | number
//! ```
//!
//! # Round-trip contract
//!
//! `parse(serialize(parse(s))) == parse(s)` for every valid formula.

use crate::schema::{
    ParsedActualRefBody, ParsedAddBody, ParsedBenchmarkRefBody, ParsedBinopBody, ParsedBucketBody,
    ParsedCalibrateBody, ParsedClampBody, ParsedConstBody, ParsedCurrentElementBody, ParsedDivBody,
    ParsedIfBody, ParsedIfNullBody, ParsedIsElementBody, ParsedLagBody, ParsedLookupRefBody,
    ParsedMeasureRefBody, ParsedModBody, ParsedMulBody, ParsedNormCdfBody, ParsedNormInvBody,
    ParsedParamRefBody, ParsedPowBody, ParsedPredictBody, ParsedRefBody, ParsedRollingAvgBody,
    ParsedRuleBody, ParsedSafeDivBody, ParsedScalar, ParsedScenarioRefBody, ParsedStrLiteralBody,
    ParsedSubBody, ParsedSumOverBody, ParsedUnaryBody, ParsedVarargBody,
};

// ---------------------------------------------------------------------------
// Public surface
// ---------------------------------------------------------------------------

/// Parse a formula string into a [`ParsedRuleBody`] tree.
pub fn parse(input: &str) -> Result<ParsedRuleBody, FormulaError> {
    let mut p = Parser::new(input);
    p.skip_ws();
    let body = p.parse_or_expression()?;
    p.skip_ws();
    if p.pos < p.input.len() {
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

/// Phase 3I item 8 W1: public alias for [`parse`]. Lets `mc-cli`'s
/// `--where` filter parser delegate to the same recursive-descent
/// expression grammar instead of maintaining a duplicate. Returns the
/// parsed expression as [`ParsedRuleBody`]; the caller is responsible
/// for any additional well-formedness checks (e.g., rejecting
/// cross-coord operators when used as a single-coord predicate).
pub fn parse_expression(input: &str) -> Result<ParsedRuleBody, FormulaError> {
    parse(input)
}

/// Render a [`ParsedRuleBody`] tree back to formula text.
pub fn serialize(body: &ParsedRuleBody) -> String {
    let mut out = String::new();
    write_node(&mut out, body, 0, false);
    out
}

// ---------------------------------------------------------------------------
// Internal error type
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct FormulaError {
    pub code: &'static str,
    pub offset: usize,
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
    fn unknown_function(offset: usize, message: String) -> Self {
        Self {
            code: "MC1007",
            offset,
            message,
        }
    }
    fn wrong_arg_count(offset: usize, message: String) -> Self {
        Self {
            code: "MC1008",
            offset,
            message,
        }
    }
    fn actual_ref_non_identifier(offset: usize, message: String) -> Self {
        Self {
            code: "MC1009",
            offset,
            message,
        }
    }
    #[allow(dead_code)]
    fn cross_coord_nesting(offset: usize, message: String) -> Self {
        Self {
            code: "MC1013",
            offset,
            message,
        }
    }
    /// Phase 3I item 1 W4: string literal appeared outside the second
    /// argument of `is_element()`. Phase 3I keeps `ScalarValue::Str`
    /// out of the AST; general string-literal support is deferred.
    #[allow(dead_code)]
    pub(crate) fn string_literal_misplaced(offset: usize, message: String) -> Self {
        Self {
            code: "MC1024",
            offset,
            message,
        }
    }
    /// Phase 3I item 8 W2: cross-coord operator used inside a
    /// `--where` filter expression. Filters evaluate against single
    /// coordinates; cross-coord ops are deferred.
    #[allow(dead_code)]
    pub(crate) fn cross_coord_in_filter(offset: usize, message: String) -> Self {
        Self {
            code: "MC1025",
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

    /// Check if current position starts with the given keyword, followed
    /// by a non-alphanumeric/non-underscore character (i.e., it's a full
    /// keyword, not a prefix of an identifier).
    fn peek_keyword(&self, kw: &str) -> bool {
        let remaining = &self.input[self.pos..];
        if !remaining.starts_with(kw) {
            return false;
        }
        let after = self.pos + kw.len();
        match self.input.as_bytes().get(after) {
            None => true,
            Some(b) => !b.is_ascii_alphanumeric() && *b != b'_',
        }
    }

    /// Consume a keyword, advancing past it.
    fn consume_keyword(&mut self, kw: &str) {
        self.pos += kw.len();
    }

    // -- Precedence level 1 (lowest): or --

    fn parse_or_expression(&mut self) -> Result<ParsedRuleBody, FormulaError> {
        let mut left = self.parse_and_expression()?;
        loop {
            self.skip_ws();
            if !self.peek_keyword("or") {
                break;
            }
            self.consume_keyword("or");
            self.skip_ws();
            let right = self.parse_and_expression()?;
            left = ParsedRuleBody::Or(ParsedBinopBody {
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    // -- Precedence level 2: and --

    fn parse_and_expression(&mut self) -> Result<ParsedRuleBody, FormulaError> {
        let mut left = self.parse_not_expression()?;
        loop {
            self.skip_ws();
            if !self.peek_keyword("and") {
                break;
            }
            self.consume_keyword("and");
            self.skip_ws();
            let right = self.parse_not_expression()?;
            left = ParsedRuleBody::And(ParsedBinopBody {
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    // -- Precedence level 3: not (unary) --

    fn parse_not_expression(&mut self) -> Result<ParsedRuleBody, FormulaError> {
        self.skip_ws();
        if self.peek_keyword("not") {
            self.consume_keyword("not");
            self.skip_ws();
            let operand = self.parse_not_expression()?;
            return Ok(ParsedRuleBody::Not(ParsedUnaryBody {
                operand: Box::new(operand),
            }));
        }
        self.parse_comparison()
    }

    // -- Precedence level 4: comparisons (non-associative) --

    fn parse_comparison(&mut self) -> Result<ParsedRuleBody, FormulaError> {
        let left = self.parse_expression()?;
        self.skip_ws();
        let _cmp_start = self.pos;
        let op = self.try_comparison_op();
        let Some(op_kind) = op else {
            return Ok(left);
        };
        self.skip_ws();
        let right = self.parse_expression()?;
        // Non-associative: if another comparison follows, fire MC1008.
        self.skip_ws();
        if self.try_peek_comparison_op() {
            return Err(FormulaError::wrong_arg_count(
                self.pos,
                "chained comparison operators are not allowed; use 'and'/'or' to combine".into(),
            ));
        }
        let node = match op_kind {
            CmpOp::Gt => ParsedRuleBody::Gt,
            CmpOp::Lt => ParsedRuleBody::Lt,
            CmpOp::Gte => ParsedRuleBody::Gte,
            CmpOp::Lte => ParsedRuleBody::Lte,
            CmpOp::Eq => ParsedRuleBody::Eq,
            CmpOp::Neq => ParsedRuleBody::Neq,
        };
        Ok(node(ParsedBinopBody {
            left: Box::new(left),
            right: Box::new(right),
        }))
    }

    fn try_comparison_op(&mut self) -> Option<CmpOp> {
        let b1 = self.peek_byte()?;
        match b1 {
            b'>' => {
                self.advance();
                if self.peek_byte() == Some(b'=') {
                    self.advance();
                    Some(CmpOp::Gte)
                } else {
                    Some(CmpOp::Gt)
                }
            }
            b'<' => {
                self.advance();
                if self.peek_byte() == Some(b'=') {
                    self.advance();
                    Some(CmpOp::Lte)
                } else {
                    Some(CmpOp::Lt)
                }
            }
            b'=' => {
                if self.input.as_bytes().get(self.pos + 1) == Some(&b'=') {
                    self.advance();
                    self.advance();
                    Some(CmpOp::Eq)
                } else {
                    None
                }
            }
            b'!' => {
                if self.input.as_bytes().get(self.pos + 1) == Some(&b'=') {
                    self.advance();
                    self.advance();
                    Some(CmpOp::Neq)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn try_peek_comparison_op(&self) -> bool {
        let Some(b1) = self.peek_byte() else {
            return false;
        };
        matches!(b1, b'>' | b'<')
            || (b1 == b'=' && self.input.as_bytes().get(self.pos + 1) == Some(&b'='))
            || (b1 == b'!' && self.input.as_bytes().get(self.pos + 1) == Some(&b'='))
    }

    // -- Precedence level 5: addition (+, -) --

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

    // -- Precedence level 6: multiplication (*, /) --

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

    // -- Precedence level 7+8: unary arithmetic + primary --

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
                let inner = self.parse_or_expression()?;
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
                self.advance();
                self.parse_factor()
            }
            b'-' => {
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
            // Phase 3J item 1: string literals are first-class in
            // expression evaluation. The parser produces a `StrLiteral`
            // node; subsequent type-context validation (mc-model
            // `validate.rs`) rejects Str values in arithmetic (MC1026),
            // numeric comparisons (MC1028), truthy contexts like `if` /
            // `and` / `or` / `not` operands (MC1027), or as a rule body's
            // outermost return value (MC2058). The parse-time MC1024
            // ("string literal misplaced") is reserved for backstop use
            // — the validator-side type-context check is the primary
            // enforcement.
            b'"' => {
                let s = self.parse_string_literal_inline()?;
                Ok(ParsedRuleBody::StrLiteral(ParsedStrLiteralBody {
                    str_literal: s,
                }))
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
        let after_ident = self.pos;

        // Check for Null literal
        if name == "Null" {
            self.skip_ws();
            if self.peek_byte() != Some(b'(') {
                return Ok(ParsedRuleBody::Const(ParsedConstBody {
                    value: ParsedScalar::Null,
                }));
            }
        }

        self.skip_ws();
        if self.peek_byte() == Some(b'(') {
            // Function call dispatch.
            let call_start = start;
            self.advance(); // consume '('
            match name {
                "if_null" => {
                    let args = self.parse_arg_list()?;
                    self.expect_close_paren("if_null")?;
                    if args.len() != 2 {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            format!("if_null expects exactly 2 arguments, got {}", args.len()),
                        ));
                    }
                    let [a, b] = take2(args);
                    Ok(if_null_node(a, b))
                }
                "if" => {
                    let args = self.parse_arg_list()?;
                    self.expect_close_paren("if")?;
                    if args.len() != 3 {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            format!("if expects exactly 3 arguments, got {}", args.len()),
                        ));
                    }
                    let [a, b, c] = take3(args);
                    Ok(ParsedRuleBody::If(ParsedIfBody {
                        condition: Box::new(a),
                        then_branch: Box::new(b),
                        else_branch: Box::new(c),
                    }))
                }
                "min" => {
                    let args = self.parse_arg_list()?;
                    self.expect_close_paren("min")?;
                    if args.len() < 2 {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            format!("min expects at least 2 arguments, got {}", args.len()),
                        ));
                    }
                    Ok(ParsedRuleBody::Min(ParsedVarargBody { args }))
                }
                "max" => {
                    let args = self.parse_arg_list()?;
                    self.expect_close_paren("max")?;
                    if args.len() < 2 {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            format!("max expects at least 2 arguments, got {}", args.len()),
                        ));
                    }
                    Ok(ParsedRuleBody::Max(ParsedVarargBody { args }))
                }
                "abs" => {
                    let args = self.parse_arg_list()?;
                    self.expect_close_paren("abs")?;
                    if args.len() != 1 {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            format!("abs expects exactly 1 argument, got {}", args.len()),
                        ));
                    }
                    let [operand] = take1(args);
                    Ok(ParsedRuleBody::Abs(ParsedUnaryBody {
                        operand: Box::new(operand),
                    }))
                }
                "not" => {
                    let args = self.parse_arg_list()?;
                    self.expect_close_paren("not")?;
                    if args.len() != 1 {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            format!("not expects exactly 1 argument, got {}", args.len()),
                        ));
                    }
                    let [operand] = take1(args);
                    Ok(ParsedRuleBody::Not(ParsedUnaryBody {
                        operand: Box::new(operand),
                    }))
                }
                "safe_div" => {
                    let args = self.parse_arg_list()?;
                    self.expect_close_paren("safe_div")?;
                    if args.len() != 3 {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            format!("safe_div expects exactly 3 arguments, got {}", args.len()),
                        ));
                    }
                    let [a, b, c] = take3(args);
                    Ok(ParsedRuleBody::SafeDiv(ParsedSafeDivBody {
                        numerator: Box::new(a),
                        denominator: Box::new(b),
                        default: Box::new(c),
                    }))
                }
                "clamp" => {
                    let args = self.parse_arg_list()?;
                    self.expect_close_paren("clamp")?;
                    if args.len() != 3 {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            format!("clamp expects exactly 3 arguments, got {}", args.len()),
                        ));
                    }
                    let [a, b, c] = take3(args);
                    Ok(ParsedRuleBody::Clamp(ParsedClampBody {
                        value: Box::new(a),
                        lo: Box::new(b),
                        hi: Box::new(c),
                    }))
                }
                "coalesce" => {
                    let args = self.parse_arg_list()?;
                    self.expect_close_paren("coalesce")?;
                    if args.is_empty() {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            "coalesce expects at least 1 argument, got 0".into(),
                        ));
                    }
                    Ok(ParsedRuleBody::Coalesce(ParsedVarargBody { args }))
                }
                "actual_ref" => {
                    self.skip_ws();
                    let measure = self.parse_bare_identifier("actual_ref", call_start)?;
                    self.skip_ws();
                    // Phase 3J item 6: optional 2nd arg is a fallback
                    // expression. Per Amendment §3, cross-coord
                    // functions are allowed inside the fallback (the
                    // MC1013 nesting prohibition is relaxed for this
                    // slot only).
                    let fallback = if self.peek_byte() == Some(b',') {
                        self.advance();
                        let fb = self.parse_or_expression()?;
                        self.skip_ws();
                        Some(Box::new(fb))
                    } else {
                        None
                    };
                    self.expect_close_paren("actual_ref")?;
                    Ok(ParsedRuleBody::ActualRef(ParsedActualRefBody {
                        measure,
                        fallback,
                    }))
                }
                // Phase 3J item 6: scenario_ref(measure, "ScenarioName").
                "scenario_ref" => {
                    self.skip_ws();
                    let measure = self.parse_bare_identifier("scenario_ref", call_start)?;
                    self.skip_ws();
                    if self.peek_byte() != Some(b',') {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            "scenario_ref expects 2 arguments: scenario_ref(measure, \"ScenarioName\")".into(),
                        ));
                    }
                    self.advance();
                    self.skip_ws();
                    let scenario = self.parse_string_literal("scenario_ref", call_start)?;
                    self.skip_ws();
                    self.expect_close_paren("scenario_ref")?;
                    Ok(ParsedRuleBody::ScenarioRef(ParsedScenarioRefBody {
                        measure,
                        scenario,
                    }))
                }
                "prev" => {
                    self.skip_ws();
                    let measure = self.parse_bare_identifier("prev", call_start)?;
                    self.skip_ws();
                    self.expect_close_paren("prev")?;
                    Ok(ParsedRuleBody::Prev(ParsedMeasureRefBody { measure }))
                }
                "cumulative" => {
                    self.skip_ws();
                    let measure = self.parse_bare_identifier("cumulative", call_start)?;
                    self.skip_ws();
                    self.expect_close_paren("cumulative")?;
                    Ok(ParsedRuleBody::Cumulative(ParsedMeasureRefBody { measure }))
                }
                "lag" => {
                    self.skip_ws();
                    let measure = self.parse_bare_identifier("lag", call_start)?;
                    self.skip_ws();
                    if self.peek_byte() != Some(b',') {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            "lag expects exactly 2 arguments: lag(measure, periods)".into(),
                        ));
                    }
                    self.advance(); // consume ','
                    let periods = self.parse_or_expression()?;
                    self.skip_ws();
                    self.expect_close_paren("lag")?;
                    Ok(ParsedRuleBody::Lag(ParsedLagBody {
                        measure,
                        periods: Box::new(periods),
                    }))
                }
                "rolling_avg" => {
                    self.skip_ws();
                    let measure = self.parse_bare_identifier("rolling_avg", call_start)?;
                    self.skip_ws();
                    if self.peek_byte() != Some(b',') {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            "rolling_avg expects exactly 2 arguments: rolling_avg(measure, window)"
                                .into(),
                        ));
                    }
                    self.advance(); // consume ','
                    let window = self.parse_or_expression()?;
                    self.skip_ws();
                    self.expect_close_paren("rolling_avg")?;
                    Ok(ParsedRuleBody::RollingAvg(ParsedRollingAvgBody {
                        measure,
                        window: Box::new(window),
                    }))
                }
                "period_index" => {
                    self.skip_ws();
                    self.expect_close_paren("period_index")?;
                    Ok(ParsedRuleBody::PeriodIndex(
                        crate::schema::ParsedPeriodIndexBody::new(),
                    ))
                }
                "anchor_index" => {
                    self.skip_ws();
                    self.expect_close_paren("anchor_index")?;
                    Ok(ParsedRuleBody::AnchorIndex(
                        crate::schema::ParsedPeriodIndexBody::new(),
                    ))
                }
                "is_past" => {
                    self.skip_ws();
                    self.expect_close_paren("is_past")?;
                    Ok(ParsedRuleBody::IsPast(
                        crate::schema::ParsedPeriodIndexBody::new(),
                    ))
                }
                "is_current" => {
                    self.skip_ws();
                    self.expect_close_paren("is_current")?;
                    Ok(ParsedRuleBody::IsCurrent(
                        crate::schema::ParsedPeriodIndexBody::new(),
                    ))
                }
                "is_future" => {
                    self.skip_ws();
                    self.expect_close_paren("is_future")?;
                    Ok(ParsedRuleBody::IsFuture(
                        crate::schema::ParsedPeriodIndexBody::new(),
                    ))
                }
                "periods_since_anchor" => {
                    self.skip_ws();
                    self.expect_close_paren("periods_since_anchor")?;
                    Ok(ParsedRuleBody::PeriodsSinceAnchor(
                        crate::schema::ParsedPeriodIndexBody::new(),
                    ))
                }
                "periods_to_end" => {
                    self.skip_ws();
                    self.expect_close_paren("periods_to_end")?;
                    Ok(ParsedRuleBody::PeriodsToEnd(
                        crate::schema::ParsedPeriodIndexBody::new(),
                    ))
                }
                "benchmark" => {
                    self.skip_ws();
                    let bname = self.parse_string_literal("benchmark", call_start)?;
                    self.skip_ws();
                    if self.peek_byte() != Some(b',') {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            "benchmark expects 2 arguments: benchmark(\"name\", key_expr)".into(),
                        ));
                    }
                    self.advance(); // consume ','
                    let key_expr = self.parse_or_expression()?;
                    self.skip_ws();
                    self.expect_close_paren("benchmark")?;
                    Ok(ParsedRuleBody::Benchmark(ParsedBenchmarkRefBody {
                        name: bname,
                        key_expr: Box::new(key_expr),
                    }))
                }
                "lookup" => {
                    // Phase 3I item 3: variadic — lookup("name", k1) for
                    // single-key (Phase 3G shape, unchanged) and
                    // lookup("name", k1, k2, ...) for multi-key.
                    self.skip_ws();
                    let tname = self.parse_string_literal("lookup", call_start)?;
                    self.skip_ws();
                    if self.peek_byte() != Some(b',') {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            "lookup expects at least 2 arguments: lookup(\"table\", key_expr, ...)"
                                .into(),
                        ));
                    }
                    let mut keys = Vec::new();
                    while self.peek_byte() == Some(b',') {
                        self.advance(); // consume ','
                        let k = self.parse_or_expression()?;
                        keys.push(Box::new(k));
                        self.skip_ws();
                    }
                    self.expect_close_paren("lookup")?;
                    Ok(ParsedRuleBody::Lookup(ParsedLookupRefBody {
                        table: tname,
                        key_exprs: keys,
                    }))
                }
                "bucket" => {
                    let value = self.parse_or_expression()?;
                    self.skip_ws();
                    if self.peek_byte() != Some(b',') {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            "bucket expects 2 arguments: bucket(value, \"threshold\")".into(),
                        ));
                    }
                    self.advance(); // consume ','
                    self.skip_ws();
                    let threshold_name = self.parse_string_literal("bucket", call_start)?;
                    self.skip_ws();
                    self.expect_close_paren("bucket")?;
                    Ok(ParsedRuleBody::Bucket(ParsedBucketBody {
                        value: Box::new(value),
                        threshold_name,
                    }))
                }
                "sum_over" => {
                    self.skip_ws();
                    let dimension = self.parse_bare_identifier("sum_over", call_start)?;
                    self.skip_ws();
                    if self.peek_byte() != Some(b',') {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            "sum_over expects 2 arguments: sum_over(dimension, measure)".into(),
                        ));
                    }
                    self.advance(); // consume ','
                    self.skip_ws();
                    let measure = self.parse_bare_identifier("sum_over", call_start)?;
                    self.skip_ws();
                    self.expect_close_paren("sum_over")?;
                    Ok(ParsedRuleBody::SumOver(ParsedSumOverBody {
                        dimension,
                        measure,
                    }))
                }
                // Phase 3H: fitted-model evaluation
                "predict" => {
                    self.skip_ws();
                    let model_id = self.parse_string_literal("predict", call_start)?;
                    self.skip_ws();
                    let mut features = Vec::new();
                    while self.peek_byte() == Some(b',') {
                        self.advance(); // consume ','
                        let feature = self.parse_or_expression()?;
                        features.push(Box::new(feature));
                        self.skip_ws();
                    }
                    self.expect_close_paren("predict")?;
                    Ok(ParsedRuleBody::Predict(ParsedPredictBody {
                        model_id,
                        features,
                    }))
                }
                "calibrate" => {
                    let value = self.parse_or_expression()?;
                    self.skip_ws();
                    if self.peek_byte() != Some(b',') {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            "calibrate expects 2 arguments: calibrate(value, \"map_name\")".into(),
                        ));
                    }
                    self.advance(); // consume ','
                    self.skip_ws();
                    let map_id = self.parse_string_literal("calibrate", call_start)?;
                    self.skip_ws();
                    self.expect_close_paren("calibrate")?;
                    Ok(ParsedRuleBody::Calibrate(ParsedCalibrateBody {
                        value: Box::new(value),
                        map_id,
                    }))
                }
                "exp" => {
                    let args = self.parse_arg_list()?;
                    self.expect_close_paren("exp")?;
                    if args.len() != 1 {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            format!("exp expects exactly 1 argument, got {}", args.len()),
                        ));
                    }
                    let [operand] = take1(args);
                    Ok(ParsedRuleBody::Exp(ParsedUnaryBody {
                        operand: Box::new(operand),
                    }))
                }
                "norm_cdf" => {
                    let args = self.parse_arg_list()?;
                    self.expect_close_paren("norm_cdf")?;
                    if args.len() != 3 {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            format!("norm_cdf expects exactly 3 arguments, got {}", args.len()),
                        ));
                    }
                    let [x, mu, sigma] = take3(args);
                    Ok(ParsedRuleBody::NormCdf(ParsedNormCdfBody {
                        x: Box::new(x),
                        mu: Box::new(mu),
                        sigma: Box::new(sigma),
                    }))
                }
                // -- Phase 3I: math primitives --
                "pow" => {
                    let args = self.parse_arg_list()?;
                    self.expect_close_paren("pow")?;
                    if args.len() != 2 {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            format!("pow expects exactly 2 arguments, got {}", args.len()),
                        ));
                    }
                    let [base, exponent] = take2(args);
                    Ok(ParsedRuleBody::Pow(ParsedPowBody {
                        base: Box::new(base),
                        exponent: Box::new(exponent),
                    }))
                }
                "sqrt" => unary_math_call(self, name, call_start, ParsedRuleBody::Sqrt),
                "ln" => unary_math_call(self, name, call_start, ParsedRuleBody::Ln),
                "log10" => unary_math_call(self, name, call_start, ParsedRuleBody::Log10),
                "round" => unary_math_call(self, name, call_start, ParsedRuleBody::Round),
                "floor" => unary_math_call(self, name, call_start, ParsedRuleBody::Floor),
                "ceil" => unary_math_call(self, name, call_start, ParsedRuleBody::Ceil),
                "mod" => {
                    let args = self.parse_arg_list()?;
                    self.expect_close_paren("mod")?;
                    if args.len() != 2 {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            format!("mod expects exactly 2 arguments, got {}", args.len()),
                        ));
                    }
                    let [dividend, divisor] = take2(args);
                    Ok(ParsedRuleBody::Mod(ParsedModBody {
                        dividend: Box::new(dividend),
                        divisor: Box::new(divisor),
                    }))
                }
                "norm_inv" => {
                    let args = self.parse_arg_list()?;
                    self.expect_close_paren("norm_inv")?;
                    if args.len() != 3 {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            format!("norm_inv expects exactly 3 arguments, got {}", args.len()),
                        ));
                    }
                    let [p, mu, sigma] = take3(args);
                    Ok(ParsedRuleBody::NormInv(ParsedNormInvBody {
                        p: Box::new(p),
                        mu: Box::new(mu),
                        sigma: Box::new(sigma),
                    }))
                }
                // -- Phase 3J item 3: param(name) — named scalar constant --
                "param" => {
                    self.skip_ws();
                    let name = self.parse_bare_identifier("param", call_start)?;
                    self.skip_ws();
                    self.expect_close_paren("param")?;
                    Ok(ParsedRuleBody::ParamRef(ParsedParamRefBody { param: name }))
                }
                // -- Phase 3J item 2: current_element(Dim) -> Str --
                "current_element" => {
                    self.skip_ws();
                    let dimension = self.parse_bare_identifier("current_element", call_start)?;
                    self.skip_ws();
                    self.expect_close_paren("current_element")?;
                    Ok(ParsedRuleBody::CurrentElement(ParsedCurrentElementBody {
                        current_element: dimension,
                    }))
                }
                // -- Phase 3I item 1: is_element narrow numeric form --
                "is_element" => {
                    self.skip_ws();
                    let dimension = self.parse_bare_identifier("is_element", call_start)?;
                    self.skip_ws();
                    if self.peek_byte() != Some(b',') {
                        return Err(FormulaError::wrong_arg_count(
                            call_start,
                            "is_element expects 2 arguments: is_element(Dim, \"Element\")".into(),
                        ));
                    }
                    self.advance(); // consume ','
                    self.skip_ws();
                    let element = self.parse_string_literal("is_element", call_start)?;
                    self.skip_ws();
                    self.expect_close_paren("is_element")?;
                    Ok(ParsedRuleBody::IsElement(ParsedIsElementBody {
                        dimension,
                        element,
                    }))
                }
                // -- Phase 3I item 5: avg/min/max/wavg over a dimension --
                "avg_over" => parse_simple_over(self, "avg_over", call_start, OverKind::Avg),
                "min_over" => parse_simple_over(self, "min_over", call_start, OverKind::Min),
                "max_over" => parse_simple_over(self, "max_over", call_start, OverKind::Max),
                "wavg_over" => parse_wavg_over(self, call_start),
                // -- Phase 3I item 6: ifs() / switch() — desugar to nested If --
                "ifs" => {
                    let args = self.parse_arg_list()?;
                    self.expect_close_paren("ifs")?;
                    desugar_ifs(call_start, args)
                }
                "switch" => {
                    let args = self.parse_arg_list()?;
                    self.expect_close_paren("switch")?;
                    desugar_switch(call_start, args)
                }
                _ => Err(FormulaError::unknown_function(
                    call_start,
                    format!("unknown function '{name}'"),
                )),
            }
        } else {
            // Bare identifier → measure ref or keyword.
            // `and`, `or`, `not` are keywords handled at parse_or/and/not
            // levels, so they don't reach here as bare identifiers in
            // operator position. But if they appear in primary position
            // (e.g., a measure named `and`), they're treated as refs.
            self.pos = after_ident;
            Ok(ref_node(name.to_string()))
        }
    }

    /// Parse a comma-separated argument list (already past the opening paren).
    /// Returns the list of parsed expressions. Does NOT consume the closing
    /// paren.
    fn parse_arg_list(&mut self) -> Result<Vec<ParsedRuleBody>, FormulaError> {
        let mut args = Vec::new();
        self.skip_ws();
        // Handle empty arg list (e.g., `period_index()`)
        if self.peek_byte() == Some(b')') {
            return Ok(args);
        }
        args.push(self.parse_or_expression()?);
        loop {
            self.skip_ws();
            if self.peek_byte() != Some(b',') {
                break;
            }
            self.advance(); // consume ','
            args.push(self.parse_or_expression()?);
        }
        Ok(args)
    }

    /// Parse a bare identifier (not an expression). Used for functions that
    /// take measure/dimension names as arguments (actual_ref, prev, lag, etc.).
    fn parse_bare_identifier(
        &mut self,
        fn_name: &str,
        _call_start: usize,
    ) -> Result<String, FormulaError> {
        self.skip_ws();
        let ident_start = self.pos;
        let Some(c) = self.peek_byte() else {
            return Err(FormulaError::actual_ref_non_identifier(
                self.pos,
                format!("{fn_name} expects a bare identifier argument"),
            ));
        };
        if !c.is_ascii_alphabetic() && c != b'_' {
            return Err(FormulaError::actual_ref_non_identifier(
                self.pos,
                format!(
                    "{fn_name} expects a bare identifier argument, got {:?}",
                    c as char
                ),
            ));
        }
        while let Some(b) = self.peek_byte() {
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.advance();
            } else {
                break;
            }
        }
        Ok(self.input[ident_start..self.pos].to_string())
    }

    /// Phase 3J item 1: parse a quoted string literal at primary position
    /// (outside any function call). Mirrors `parse_string_literal` but
    /// uses a `&'static` context label so the diagnostic stays useful
    /// without a function name to point at.
    fn parse_string_literal_inline(&mut self) -> Result<String, FormulaError> {
        self.skip_ws();
        if self.peek_byte() != Some(b'"') {
            return Err(FormulaError::unexpected_token(
                self.pos,
                "expected a quoted string literal".into(),
            ));
        }
        self.advance();
        let start = self.pos;
        while let Some(b) = self.peek_byte() {
            if b == b'"' {
                let s = self.input[start..self.pos].to_string();
                self.advance();
                return Ok(s);
            }
            if b == b'\\' {
                self.advance();
            }
            self.advance();
        }
        Err(FormulaError::unexpected_token(
            start,
            "unterminated string literal".into(),
        ))
    }

    /// Parse a double-quoted string literal (for benchmark/lookup names).
    fn parse_string_literal(
        &mut self,
        fn_name: &str,
        _call_start: usize,
    ) -> Result<String, FormulaError> {
        self.skip_ws();
        if self.peek_byte() != Some(b'"') {
            return Err(FormulaError::unexpected_token(
                self.pos,
                format!("{fn_name} expects a quoted string argument"),
            ));
        }
        self.advance(); // consume opening quote
        let start = self.pos;
        while let Some(b) = self.peek_byte() {
            if b == b'"' {
                let s = self.input[start..self.pos].to_string();
                self.advance(); // consume closing quote
                return Ok(s);
            }
            if b == b'\\' {
                self.advance(); // skip escape character
            }
            self.advance();
        }
        Err(FormulaError::unexpected_token(
            start,
            format!("{fn_name}: unterminated string literal"),
        ))
    }

    fn expect_close_paren(&mut self, fn_name: &str) -> Result<(), FormulaError> {
        self.skip_ws();
        match self.peek_byte() {
            Some(b')') => {
                self.advance();
                Ok(())
            }
            _ => Err(FormulaError::unbalanced_paren(
                self.pos,
                format!("missing closing paren ')' on {fn_name} call"),
            )),
        }
    }

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

#[derive(Clone, Copy)]
enum CmpOp {
    Gt,
    Lt,
    Gte,
    Lte,
    Eq,
    Neq,
}

/// Phase 3I item 5: shared shape for `avg_over` / `min_over` / `max_over`.
/// `wavg_over` has its own dedicated body (3 args) and bypasses this enum.
#[derive(Clone, Copy)]
enum OverKind {
    Avg,
    Min,
    Max,
}

/// Phase 3I item 2: helper for unary math primitives that take exactly
/// one argument (sqrt/ln/log10/round/floor/ceil). Centralizes the arity
/// check and `take1` extraction.
fn unary_math_call(
    p: &mut Parser<'_>,
    fn_name: &str,
    call_start: usize,
    ctor: fn(ParsedUnaryBody) -> ParsedRuleBody,
) -> Result<ParsedRuleBody, FormulaError> {
    let args = p.parse_arg_list()?;
    p.expect_close_paren(fn_name)?;
    if args.len() != 1 {
        return Err(FormulaError::wrong_arg_count(
            call_start,
            format!("{fn_name} expects exactly 1 argument, got {}", args.len()),
        ));
    }
    let [operand] = take1(args);
    Ok(ctor(ParsedUnaryBody {
        operand: Box::new(operand),
    }))
}

/// Phase 3I item 5: parse `avg_over(measure, dim)` / `min_over(...)` /
/// `max_over(...)`. Same shape as `sum_over` (Phase 3G).
fn parse_simple_over(
    p: &mut Parser<'_>,
    fn_name: &str,
    call_start: usize,
    kind: OverKind,
) -> Result<ParsedRuleBody, FormulaError> {
    p.skip_ws();
    let measure = p.parse_bare_identifier(fn_name, call_start)?;
    p.skip_ws();
    if p.peek_byte() != Some(b',') {
        return Err(FormulaError::wrong_arg_count(
            call_start,
            format!("{fn_name} expects 2 arguments: {fn_name}(measure, dim)"),
        ));
    }
    p.advance(); // consume ','
    p.skip_ws();
    let dimension = p.parse_bare_identifier(fn_name, call_start)?;
    p.skip_ws();
    p.expect_close_paren(fn_name)?;
    let body = ParsedSumOverBody { dimension, measure };
    Ok(match kind {
        OverKind::Avg => ParsedRuleBody::AvgOver(body),
        OverKind::Min => ParsedRuleBody::MinOver(body),
        OverKind::Max => ParsedRuleBody::MaxOver(body),
    })
}

/// Phase 3I item 5: `wavg_over(measure, dim, weight_measure)`.
fn parse_wavg_over(p: &mut Parser<'_>, call_start: usize) -> Result<ParsedRuleBody, FormulaError> {
    p.skip_ws();
    let value_measure = p.parse_bare_identifier("wavg_over", call_start)?;
    p.skip_ws();
    if p.peek_byte() != Some(b',') {
        return Err(FormulaError::wrong_arg_count(
            call_start,
            "wavg_over expects 3 arguments: wavg_over(measure, dim, weight_measure)".into(),
        ));
    }
    p.advance();
    p.skip_ws();
    let dimension = p.parse_bare_identifier("wavg_over", call_start)?;
    p.skip_ws();
    if p.peek_byte() != Some(b',') {
        return Err(FormulaError::wrong_arg_count(
            call_start,
            "wavg_over expects 3 arguments: wavg_over(measure, dim, weight_measure)".into(),
        ));
    }
    p.advance();
    p.skip_ws();
    let weight_measure = p.parse_bare_identifier("wavg_over", call_start)?;
    p.skip_ws();
    p.expect_close_paren("wavg_over")?;
    Ok(ParsedRuleBody::WAvgOver(
        crate::schema::ParsedWAvgOverBody {
            dimension,
            value_measure,
            weight_measure,
        },
    ))
}

/// Phase 3I item 6: desugar `ifs(c1, v1, c2, v2, ..., default)` to
/// `if(c1, v1, if(c2, v2, ..., default))`. Total args must be odd
/// (2N+1: N condition/value pairs plus a mandatory default). The
/// degenerate one-arg form (just the default) emits a bare `Const` /
/// reference for the default. No new `Expr` variant.
fn desugar_ifs(
    call_start: usize,
    args: Vec<ParsedRuleBody>,
) -> Result<ParsedRuleBody, FormulaError> {
    if args.is_empty() {
        return Err(FormulaError::wrong_arg_count(
            call_start,
            "ifs expects at least 1 argument (the default value)".into(),
        ));
    }
    if args.len() % 2 == 0 {
        return Err(FormulaError::wrong_arg_count(
            call_start,
            format!(
                "ifs expects an odd argument count (N condition/value pairs + 1 default), got {}",
                args.len()
            ),
        ));
    }
    // Per handoff item 6 W5: one-arg form (just default) is allowed —
    // identical to a bare expression / constant.
    let mut iter = args.into_iter().rev();
    // SAFETY: len >= 1 verified above.
    let mut result = match iter.next() {
        Some(d) => d,
        None => {
            return Err(FormulaError::wrong_arg_count(
                call_start,
                "ifs: internal — empty after non-empty check".into(),
            ));
        }
    };
    while let Some(value) = iter.next() {
        let condition = match iter.next() {
            Some(c) => c,
            None => {
                return Err(FormulaError::wrong_arg_count(
                    call_start,
                    "ifs: internal — orphan value without condition".into(),
                ));
            }
        };
        result = ParsedRuleBody::If(ParsedIfBody {
            condition: Box::new(condition),
            then_branch: Box::new(value),
            else_branch: Box::new(result),
        });
    }
    Ok(result)
}

/// Phase 3I item 6: desugar `switch(expr, m1, v1, m2, v2, ..., default)`
/// to `if(expr == m1, v1, if(expr == m2, v2, ..., default))`. The
/// initial `expr` is cloned at each step (cheap — these are AST nodes).
/// Total args must be even (1 expr + 2N for N match/value pairs + 1
/// default = 2N+2). The minimum is 2 (expr + default; a "no matches"
/// degenerate form that always returns default).
fn desugar_switch(
    call_start: usize,
    args: Vec<ParsedRuleBody>,
) -> Result<ParsedRuleBody, FormulaError> {
    if args.len() < 2 {
        return Err(FormulaError::wrong_arg_count(
            call_start,
            "switch expects at least 2 arguments: switch(expr, default)".into(),
        ));
    }
    if args.len() % 2 != 0 {
        return Err(FormulaError::wrong_arg_count(
            call_start,
            format!(
                "switch expects an even argument count (initial expr + N match/value pairs + 1 default), got {}",
                args.len()
            ),
        ));
    }
    let mut iter = args.into_iter();
    let scrutinee = match iter.next() {
        Some(s) => s,
        None => {
            return Err(FormulaError::wrong_arg_count(
                call_start,
                "switch: internal — empty after length check".into(),
            ));
        }
    };
    // Collect remaining as Vec so we can pop the default off the end.
    let mut remaining: Vec<ParsedRuleBody> = iter.collect();
    let default = remaining.pop().unwrap_or_else(|| {
        ParsedRuleBody::Const(ParsedConstBody {
            value: ParsedScalar::Null,
        })
    });
    let mut pairs_iter = remaining.into_iter().rev();
    let mut result = default;
    while let Some(value) = pairs_iter.next() {
        let match_target = match pairs_iter.next() {
            Some(t) => t,
            None => {
                return Err(FormulaError::wrong_arg_count(
                    call_start,
                    "switch: internal — orphan value without match target".into(),
                ));
            }
        };
        let condition = ParsedRuleBody::Eq(ParsedBinopBody {
            left: Box::new(scrutinee.clone()),
            right: Box::new(match_target),
        });
        result = ParsedRuleBody::If(ParsedIfBody {
            condition: Box::new(condition),
            then_branch: Box::new(value),
            else_branch: Box::new(result),
        });
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Safe vec-to-array extraction helpers (avoid unwrap in library code)
// ---------------------------------------------------------------------------

/// Extract exactly 1 element from a vec. Caller must validate len == 1 first.
fn take1<T>(mut v: Vec<T>) -> [T; 1] {
    debug_assert_eq!(v.len(), 1);
    let a = v.swap_remove(0);
    [a]
}

/// Extract exactly 2 elements from a vec. Caller must validate len == 2 first.
fn take2<T>(mut v: Vec<T>) -> [T; 2] {
    debug_assert_eq!(v.len(), 2);
    let b = v.swap_remove(1);
    let a = v.swap_remove(0);
    [a, b]
}

/// Extract exactly 3 elements from a vec. Caller must validate len == 3 first.
fn take3<T>(mut v: Vec<T>) -> [T; 3] {
    debug_assert_eq!(v.len(), 3);
    let c = v.swap_remove(2);
    let b = v.swap_remove(1);
    let a = v.swap_remove(0);
    [a, b, c]
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
        ParsedScalar::Float(v) if v.to_bits() == 0u64 => Some(&s.sub[1]),
        _ => None,
    }
}

/// Precedence table for the round-trip paren rule.
///
/// | Level | Nodes                                                |
/// |-------|------------------------------------------------------|
/// |   8   | Const, Ref, function calls, PeriodIndex, unary minus |
/// |   7   | Mul, Div                                             |
/// |   6   | Add, Sub                                             |
/// |   5   | Gt, Lt, Gte, Lte, Eq, Neq                           |
/// |   4   | Not                                                  |
/// |   3   | And                                                  |
/// |   2   | Or                                                   |
fn prec(body: &ParsedRuleBody) -> u8 {
    if unary_minus_inner(body).is_some() {
        return 8;
    }
    match body {
        // Atomic / function-call level
        ParsedRuleBody::Const(_)
        | ParsedRuleBody::Ref(_)
        | ParsedRuleBody::IfNull(_)
        | ParsedRuleBody::If(_)
        | ParsedRuleBody::Min(_)
        | ParsedRuleBody::Max(_)
        | ParsedRuleBody::Abs(_)
        | ParsedRuleBody::SafeDiv(_)
        | ParsedRuleBody::Clamp(_)
        | ParsedRuleBody::Coalesce(_)
        | ParsedRuleBody::ActualRef(_)
        | ParsedRuleBody::Prev(_)
        | ParsedRuleBody::Lag(_)
        | ParsedRuleBody::Cumulative(_)
        | ParsedRuleBody::RollingAvg(_)
        | ParsedRuleBody::PeriodIndex(_)
        | ParsedRuleBody::AnchorIndex(_)
        | ParsedRuleBody::IsPast(_)
        | ParsedRuleBody::IsCurrent(_)
        | ParsedRuleBody::IsFuture(_)
        | ParsedRuleBody::PeriodsSinceAnchor(_)
        | ParsedRuleBody::PeriodsToEnd(_)
        | ParsedRuleBody::Benchmark(_)
        | ParsedRuleBody::Lookup(_)
        | ParsedRuleBody::Bucket(_)
        | ParsedRuleBody::SumOver(_)
        | ParsedRuleBody::Predict(_)
        | ParsedRuleBody::Calibrate(_)
        | ParsedRuleBody::Exp(_)
        | ParsedRuleBody::NormCdf(_)
        | ParsedRuleBody::Pow(_)
        | ParsedRuleBody::Sqrt(_)
        | ParsedRuleBody::Ln(_)
        | ParsedRuleBody::Log10(_)
        | ParsedRuleBody::Round(_)
        | ParsedRuleBody::Floor(_)
        | ParsedRuleBody::Ceil(_)
        | ParsedRuleBody::Mod(_)
        | ParsedRuleBody::NormInv(_)
        | ParsedRuleBody::IsElement(_)
        | ParsedRuleBody::AvgOver(_)
        | ParsedRuleBody::MinOver(_)
        | ParsedRuleBody::MaxOver(_)
        | ParsedRuleBody::WAvgOver(_)
        // Phase 3J: string literal + current_element + param are atomic primaries
        | ParsedRuleBody::StrLiteral(_)
        | ParsedRuleBody::CurrentElement(_)
        | ParsedRuleBody::ParamRef(_)
        | ParsedRuleBody::ScenarioRef(_) => 8,
        // Multiplicative
        ParsedRuleBody::Mul(_) | ParsedRuleBody::Div(_) => 7,
        // Additive
        ParsedRuleBody::Add(_) | ParsedRuleBody::Sub(_) => 6,
        // Comparison
        ParsedRuleBody::Gt(_)
        | ParsedRuleBody::Lt(_)
        | ParsedRuleBody::Gte(_)
        | ParsedRuleBody::Lte(_)
        | ParsedRuleBody::Eq(_)
        | ParsedRuleBody::Neq(_) => 5,
        // Not
        ParsedRuleBody::Not(_) => 4,
        // And
        ParsedRuleBody::And(_) => 3,
        // Or
        ParsedRuleBody::Or(_) => 2,
    }
}

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
        out.push('-');
        write_node(out, inner, 8, false);
        return;
    }
    match body {
        ParsedRuleBody::Const(c) => write_const(out, &c.value),
        ParsedRuleBody::Ref(r) => out.push_str(&r.measure),
        ParsedRuleBody::IfNull(b) => {
            out.push_str("if_null(");
            if let Some(a) = b.if_null.first() {
                write_node(out, a, 0, false);
            }
            out.push_str(", ");
            if let Some(a) = b.if_null.get(1) {
                write_node(out, a, 0, false);
            }
            out.push(')');
        }
        ParsedRuleBody::Add(b) => write_binop_vec(out, &b.add, "+", 6),
        ParsedRuleBody::Sub(b) => write_binop_vec(out, &b.sub, "-", 6),
        ParsedRuleBody::Mul(b) => write_binop_vec(out, &b.mul, "*", 7),
        ParsedRuleBody::Div(b) => write_binop_vec(out, &b.div, "/", 7),

        // Phase 3E: comparisons
        ParsedRuleBody::Gt(b) => write_binop_pair(out, &b.left, &b.right, ">", 5),
        ParsedRuleBody::Lt(b) => write_binop_pair(out, &b.left, &b.right, "<", 5),
        ParsedRuleBody::Gte(b) => write_binop_pair(out, &b.left, &b.right, ">=", 5),
        ParsedRuleBody::Lte(b) => write_binop_pair(out, &b.left, &b.right, "<=", 5),
        ParsedRuleBody::Eq(b) => write_binop_pair(out, &b.left, &b.right, "==", 5),
        ParsedRuleBody::Neq(b) => write_binop_pair(out, &b.left, &b.right, "!=", 5),

        // Phase 3E: logical
        ParsedRuleBody::And(b) => write_binop_pair(out, &b.left, &b.right, "and", 3),
        ParsedRuleBody::Or(b) => write_binop_pair(out, &b.left, &b.right, "or", 2),
        ParsedRuleBody::Not(b) => {
            out.push_str("not ");
            write_node(out, &b.operand, 4, false);
        }

        // Phase 3E: functions
        ParsedRuleBody::If(b) => {
            out.push_str("if(");
            write_node(out, &b.condition, 0, false);
            out.push_str(", ");
            write_node(out, &b.then_branch, 0, false);
            out.push_str(", ");
            write_node(out, &b.else_branch, 0, false);
            out.push(')');
        }
        ParsedRuleBody::Min(b) => write_vararg_fn(out, "min", &b.args),
        ParsedRuleBody::Max(b) => write_vararg_fn(out, "max", &b.args),
        ParsedRuleBody::Abs(b) => {
            out.push_str("abs(");
            write_node(out, &b.operand, 0, false);
            out.push(')');
        }
        ParsedRuleBody::SafeDiv(b) => {
            out.push_str("safe_div(");
            write_node(out, &b.numerator, 0, false);
            out.push_str(", ");
            write_node(out, &b.denominator, 0, false);
            out.push_str(", ");
            write_node(out, &b.default, 0, false);
            out.push(')');
        }
        ParsedRuleBody::Clamp(b) => {
            out.push_str("clamp(");
            write_node(out, &b.value, 0, false);
            out.push_str(", ");
            write_node(out, &b.lo, 0, false);
            out.push_str(", ");
            write_node(out, &b.hi, 0, false);
            out.push(')');
        }
        ParsedRuleBody::Coalesce(b) => write_vararg_fn(out, "coalesce", &b.args),
        ParsedRuleBody::ActualRef(b) => {
            out.push_str("actual_ref(");
            out.push_str(&b.measure);
            // Phase 3J item 6: optional fallback. When present, emit
            // `, <fallback>` so round-trip preserves the 2-arg form.
            if let Some(fb) = &b.fallback {
                out.push_str(", ");
                write_node(out, fb, 0, false);
            }
            out.push(')');
        }

        // Phase 3F: time-series
        ParsedRuleBody::Prev(b) => {
            out.push_str("prev(");
            out.push_str(&b.measure);
            out.push(')');
        }
        ParsedRuleBody::Lag(b) => {
            out.push_str("lag(");
            out.push_str(&b.measure);
            out.push_str(", ");
            write_node(out, &b.periods, 0, false);
            out.push(')');
        }
        ParsedRuleBody::Cumulative(b) => {
            out.push_str("cumulative(");
            out.push_str(&b.measure);
            out.push(')');
        }
        ParsedRuleBody::RollingAvg(b) => {
            out.push_str("rolling_avg(");
            out.push_str(&b.measure);
            out.push_str(", ");
            write_node(out, &b.window, 0, false);
            out.push(')');
        }
        ParsedRuleBody::PeriodIndex(_) => {
            out.push_str("period_index()");
        }
        ParsedRuleBody::AnchorIndex(_) => {
            out.push_str("anchor_index()");
        }
        ParsedRuleBody::IsPast(_) => {
            out.push_str("is_past()");
        }
        ParsedRuleBody::IsCurrent(_) => {
            out.push_str("is_current()");
        }
        ParsedRuleBody::IsFuture(_) => {
            out.push_str("is_future()");
        }
        ParsedRuleBody::PeriodsSinceAnchor(_) => {
            out.push_str("periods_since_anchor()");
        }
        ParsedRuleBody::PeriodsToEnd(_) => {
            out.push_str("periods_to_end()");
        }

        // Phase 3G: reference-data
        ParsedRuleBody::Benchmark(b) => {
            out.push_str("benchmark(\"");
            out.push_str(&b.name);
            out.push_str("\", ");
            write_node(out, &b.key_expr, 0, false);
            out.push(')');
        }
        ParsedRuleBody::Lookup(b) => {
            out.push_str("lookup(\"");
            out.push_str(&b.table);
            out.push('"');
            for k in &b.key_exprs {
                out.push_str(", ");
                write_node(out, k, 0, false);
            }
            out.push(')');
        }
        ParsedRuleBody::Bucket(b) => {
            out.push_str("bucket(");
            write_node(out, &b.value, 0, false);
            out.push_str(", \"");
            out.push_str(&b.threshold_name);
            out.push_str("\")");
        }
        ParsedRuleBody::SumOver(b) => {
            out.push_str("sum_over(");
            out.push_str(&b.dimension);
            out.push_str(", ");
            out.push_str(&b.measure);
            out.push(')');
        }

        // Phase 3H: fitted-model evaluation
        ParsedRuleBody::Predict(b) => {
            out.push_str("predict(\"");
            out.push_str(&b.model_id);
            out.push('"');
            for f in &b.features {
                out.push_str(", ");
                write_node(out, f, 0, false);
            }
            out.push(')');
        }
        ParsedRuleBody::Calibrate(b) => {
            out.push_str("calibrate(");
            write_node(out, &b.value, 0, false);
            out.push_str(", \"");
            out.push_str(&b.map_id);
            out.push_str("\")");
        }
        ParsedRuleBody::Exp(b) => {
            out.push_str("exp(");
            write_node(out, &b.operand, 0, false);
            out.push(')');
        }
        ParsedRuleBody::NormCdf(b) => {
            out.push_str("norm_cdf(");
            write_node(out, &b.x, 0, false);
            out.push_str(", ");
            write_node(out, &b.mu, 0, false);
            out.push_str(", ");
            write_node(out, &b.sigma, 0, false);
            out.push(')');
        }
        // Phase 3I: math primitives
        ParsedRuleBody::Pow(b) => {
            out.push_str("pow(");
            write_node(out, &b.base, 0, false);
            out.push_str(", ");
            write_node(out, &b.exponent, 0, false);
            out.push(')');
        }
        ParsedRuleBody::Sqrt(b) => write_unary(out, "sqrt", &b.operand),
        ParsedRuleBody::Ln(b) => write_unary(out, "ln", &b.operand),
        ParsedRuleBody::Log10(b) => write_unary(out, "log10", &b.operand),
        ParsedRuleBody::Round(b) => write_unary(out, "round", &b.operand),
        ParsedRuleBody::Floor(b) => write_unary(out, "floor", &b.operand),
        ParsedRuleBody::Ceil(b) => write_unary(out, "ceil", &b.operand),
        ParsedRuleBody::Mod(b) => {
            out.push_str("mod(");
            write_node(out, &b.dividend, 0, false);
            out.push_str(", ");
            write_node(out, &b.divisor, 0, false);
            out.push(')');
        }
        ParsedRuleBody::NormInv(b) => {
            out.push_str("norm_inv(");
            write_node(out, &b.p, 0, false);
            out.push_str(", ");
            write_node(out, &b.mu, 0, false);
            out.push_str(", ");
            write_node(out, &b.sigma, 0, false);
            out.push(')');
        }
        // Phase 3I: is_element narrow numeric form
        ParsedRuleBody::IsElement(b) => {
            out.push_str("is_element(");
            out.push_str(&b.dimension);
            out.push_str(", \"");
            out.push_str(&b.element);
            out.push_str("\")");
        }
        // Phase 3I: cross-coord scans
        ParsedRuleBody::AvgOver(b) => {
            out.push_str("avg_over(");
            out.push_str(&b.measure);
            out.push_str(", ");
            out.push_str(&b.dimension);
            out.push(')');
        }
        ParsedRuleBody::MinOver(b) => {
            out.push_str("min_over(");
            out.push_str(&b.measure);
            out.push_str(", ");
            out.push_str(&b.dimension);
            out.push(')');
        }
        ParsedRuleBody::MaxOver(b) => {
            out.push_str("max_over(");
            out.push_str(&b.measure);
            out.push_str(", ");
            out.push_str(&b.dimension);
            out.push(')');
        }
        ParsedRuleBody::WAvgOver(b) => {
            out.push_str("wavg_over(");
            out.push_str(&b.value_measure);
            out.push_str(", ");
            out.push_str(&b.dimension);
            out.push_str(", ");
            out.push_str(&b.weight_measure);
            out.push(')');
        }
        // Phase 3J: string literal — emit double-quoted, escape inner
        // backslashes and quotes for round-trip stability.
        ParsedRuleBody::StrLiteral(b) => {
            out.push('"');
            for ch in b.str_literal.chars() {
                match ch {
                    '\\' => out.push_str("\\\\"),
                    '"' => out.push_str("\\\""),
                    other => out.push(other),
                }
            }
            out.push('"');
        }
        // Phase 3J: current_element(Dim).
        ParsedRuleBody::CurrentElement(b) => {
            out.push_str("current_element(");
            out.push_str(&b.current_element);
            out.push(')');
        }
        // Phase 3J: param(name) — named scalar constant.
        ParsedRuleBody::ParamRef(b) => {
            out.push_str("param(");
            out.push_str(&b.param);
            out.push(')');
        }
        // Phase 3J item 6: scenario_ref(measure, "ScenarioName").
        ParsedRuleBody::ScenarioRef(b) => {
            out.push_str("scenario_ref(");
            out.push_str(&b.measure);
            out.push_str(", \"");
            out.push_str(&b.scenario);
            out.push_str("\")");
        }
    }
}

/// Phase 3I serializer helper: `name(operand)` for unary math primitives.
fn write_unary(out: &mut String, name: &str, operand: &ParsedRuleBody) {
    out.push_str(name);
    out.push('(');
    write_node(out, operand, 0, false);
    out.push(')');
}

fn write_binop_vec(out: &mut String, args: &[ParsedRuleBody], op: &str, op_prec: u8) {
    if args.len() != 2 {
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

fn write_binop_pair(
    out: &mut String,
    left: &ParsedRuleBody,
    right: &ParsedRuleBody,
    op: &str,
    op_prec: u8,
) {
    write_node(out, left, op_prec, false);
    out.push(' ');
    out.push_str(op);
    out.push(' ');
    write_node(out, right, op_prec, true);
}

fn write_vararg_fn(out: &mut String, name: &str, args: &[ParsedRuleBody]) {
    out.push_str(name);
    out.push('(');
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write_node(out, arg, 0, false);
    }
    out.push(')');
}

fn write_const(out: &mut String, value: &ParsedScalar) {
    match value {
        ParsedScalar::Float(v) => out.push_str(&v.to_string()),
        ParsedScalar::Int(v) => out.push_str(&v.to_string()),
        ParsedScalar::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
        ParsedScalar::Null => out.push_str("Null"),
    }
}

/// Check if a `ParsedRuleBody` contains a cross-coordinate function
/// (actual_ref, prev, lag, cumulative, rolling_avg, sum_over).
pub fn contains_cross_coord(body: &ParsedRuleBody) -> bool {
    match body {
        ParsedRuleBody::ActualRef(_)
        | ParsedRuleBody::Prev(_)
        | ParsedRuleBody::Lag(_)
        | ParsedRuleBody::Cumulative(_)
        | ParsedRuleBody::RollingAvg(_)
        | ParsedRuleBody::SumOver(_)
        | ParsedRuleBody::AvgOver(_)
        | ParsedRuleBody::MinOver(_)
        | ParsedRuleBody::MaxOver(_)
        | ParsedRuleBody::WAvgOver(_)
        // Phase 3J item 6: scenario_ref is cross-coord (Scenario shift).
        | ParsedRuleBody::ScenarioRef(_) => true,
        ParsedRuleBody::Const(_)
        | ParsedRuleBody::Ref(_)
        | ParsedRuleBody::PeriodIndex(_)
        | ParsedRuleBody::AnchorIndex(_)
        | ParsedRuleBody::IsPast(_)
        | ParsedRuleBody::IsCurrent(_)
        | ParsedRuleBody::IsFuture(_)
        | ParsedRuleBody::PeriodsSinceAnchor(_)
        | ParsedRuleBody::PeriodsToEnd(_) => false,
        ParsedRuleBody::Add(b) => b.add.iter().any(contains_cross_coord),
        ParsedRuleBody::Sub(b) => b.sub.iter().any(contains_cross_coord),
        ParsedRuleBody::Mul(b) => b.mul.iter().any(contains_cross_coord),
        ParsedRuleBody::Div(b) => b.div.iter().any(contains_cross_coord),
        ParsedRuleBody::IfNull(b) => b.if_null.iter().any(contains_cross_coord),
        ParsedRuleBody::Gt(b)
        | ParsedRuleBody::Lt(b)
        | ParsedRuleBody::Gte(b)
        | ParsedRuleBody::Lte(b)
        | ParsedRuleBody::Eq(b)
        | ParsedRuleBody::Neq(b)
        | ParsedRuleBody::And(b)
        | ParsedRuleBody::Or(b) => contains_cross_coord(&b.left) || contains_cross_coord(&b.right),
        ParsedRuleBody::Not(b) | ParsedRuleBody::Abs(b) => contains_cross_coord(&b.operand),
        ParsedRuleBody::If(b) => {
            contains_cross_coord(&b.condition)
                || contains_cross_coord(&b.then_branch)
                || contains_cross_coord(&b.else_branch)
        }
        ParsedRuleBody::Min(b) | ParsedRuleBody::Max(b) | ParsedRuleBody::Coalesce(b) => {
            b.args.iter().any(contains_cross_coord)
        }
        ParsedRuleBody::SafeDiv(b) => {
            contains_cross_coord(&b.numerator)
                || contains_cross_coord(&b.denominator)
                || contains_cross_coord(&b.default)
        }
        ParsedRuleBody::Clamp(b) => {
            contains_cross_coord(&b.value)
                || contains_cross_coord(&b.lo)
                || contains_cross_coord(&b.hi)
        }
        ParsedRuleBody::Benchmark(b) => contains_cross_coord(&b.key_expr),
        ParsedRuleBody::Lookup(b) => b.key_exprs.iter().any(|k| contains_cross_coord(k)),
        ParsedRuleBody::Bucket(b) => contains_cross_coord(&b.value),
        // Phase 3H: predict/calibrate/exp/norm_cdf are not cross-coord functions
        // themselves, but their arguments may contain cross-coord functions.
        ParsedRuleBody::Predict(b) => b.features.iter().any(|f| contains_cross_coord(f)),
        ParsedRuleBody::Calibrate(b) => contains_cross_coord(&b.value),
        ParsedRuleBody::Exp(b) => contains_cross_coord(&b.operand),
        ParsedRuleBody::NormCdf(b) => {
            contains_cross_coord(&b.x)
                || contains_cross_coord(&b.mu)
                || contains_cross_coord(&b.sigma)
        }
        // Phase 3I: math primitives are local — no cross-coord on their own.
        ParsedRuleBody::Pow(b) => {
            contains_cross_coord(&b.base) || contains_cross_coord(&b.exponent)
        }
        ParsedRuleBody::Sqrt(b)
        | ParsedRuleBody::Ln(b)
        | ParsedRuleBody::Log10(b)
        | ParsedRuleBody::Round(b)
        | ParsedRuleBody::Floor(b)
        | ParsedRuleBody::Ceil(b) => contains_cross_coord(&b.operand),
        ParsedRuleBody::Mod(b) => {
            contains_cross_coord(&b.dividend) || contains_cross_coord(&b.divisor)
        }
        ParsedRuleBody::NormInv(b) => {
            contains_cross_coord(&b.p)
                || contains_cross_coord(&b.mu)
                || contains_cross_coord(&b.sigma)
        }
        // Phase 3I: is_element is local (parse-time element resolution).
        ParsedRuleBody::IsElement(_) => false,
        // Phase 3J: string literal, current_element, and param are
        // local; they resolve at the current coordinate (or via a
        // simple HashMap lookup for param) without crossing coord axes.
        ParsedRuleBody::StrLiteral(_)
        | ParsedRuleBody::CurrentElement(_)
        | ParsedRuleBody::ParamRef(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Unit tests
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

    fn assert_round_trip(input: &str) {
        let parsed = parse(input).unwrap_or_else(|e| panic!("parse failed for {input:?}: {e:?}"));
        let serialized = serialize(&parsed);
        let reparsed = parse(&serialized).unwrap_or_else(|e| {
            panic!("round-trip parse failed: input={input:?} serialized={serialized:?} err={e:?}")
        });
        assert_eq!(
            format!("{parsed:?}"),
            format!("{reparsed:?}"),
            "round-trip drifted: input={input:?} serialized={serialized:?}"
        );
    }

    fn assert_round_trip_exact(input: &str, expected: &str) {
        let parsed = parse(input).unwrap_or_else(|e| panic!("parse failed for {input:?}: {e:?}"));
        let serialized = serialize(&parsed);
        assert_eq!(
            serialized, expected,
            "serializer output mismatch for {input:?}"
        );
        let reparsed = parse(&serialized).expect("reparse");
        assert_eq!(format!("{parsed:?}"), format!("{reparsed:?}"));
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
        assert_eq!(serialize(&const_f64(1.5)), "1.5");
        assert_eq!(serialize(&const_f64(0.1)), "0.1");
        assert_eq!(serialize(&const_f64(1.0)), "1");
    }

    #[test]
    fn serialize_unary_minus_canonical() {
        let b = sub_node(const_f64(0.0), ref_node("Spend".into()));
        assert_eq!(serialize(&b), "-Spend");
    }

    #[test]
    fn serialize_subtraction_associativity() {
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
        let inner = div_node(ref_node("b".into()), ref_node("c".into()));
        let b = mul_node(ref_node("a".into()), inner);
        assert_eq!(serialize(&b), "a * (b / c)");
    }

    // Phase 3E: MC1007 for unknown functions (was MC1004 in Phase 3D)
    #[test]
    fn parse_unknown_function_call_fires_mc1007() {
        let err = parse("foo(Spend, CPC)").unwrap_err();
        assert_eq!(err.code, "MC1007");
    }

    // Phase 3E: min/max now parse successfully (was MC1004 in Phase 3D)
    #[test]
    fn parse_min_succeeds() {
        let b = parse("min(Spend, CPC)").unwrap();
        assert!(matches!(b, ParsedRuleBody::Min(_)));
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
    fn parse_if_null_arity_fires_mc1008() {
        let err = parse("if_null(Spend)").unwrap_err();
        assert_eq!(err.code, "MC1008");
        let err = parse("if_null(Spend, CPC, AOV)").unwrap_err();
        assert_eq!(err.code, "MC1008");
    }

    // -- Phase 3E new tests --

    #[test]
    fn parse_comparison_operators() {
        assert!(matches!(parse("a > b").unwrap(), ParsedRuleBody::Gt(_)));
        assert!(matches!(parse("a < b").unwrap(), ParsedRuleBody::Lt(_)));
        assert!(matches!(parse("a >= b").unwrap(), ParsedRuleBody::Gte(_)));
        assert!(matches!(parse("a <= b").unwrap(), ParsedRuleBody::Lte(_)));
        assert!(matches!(parse("a == b").unwrap(), ParsedRuleBody::Eq(_)));
        assert!(matches!(parse("a != b").unwrap(), ParsedRuleBody::Neq(_)));
    }

    #[test]
    fn parse_chained_comparison_fires_mc1008() {
        let err = parse("a > b > c").unwrap_err();
        assert_eq!(err.code, "MC1008");
    }

    #[test]
    fn parse_logical_operators() {
        assert!(matches!(parse("a and b").unwrap(), ParsedRuleBody::And(_)));
        assert!(matches!(parse("a or b").unwrap(), ParsedRuleBody::Or(_)));
        assert!(matches!(parse("not a").unwrap(), ParsedRuleBody::Not(_)));
    }

    #[test]
    fn parse_if_function() {
        let b = parse("if(a > 0, a, 0)").unwrap();
        assert!(matches!(b, ParsedRuleBody::If(_)));
    }

    #[test]
    fn parse_abs() {
        let b = parse("abs(x)").unwrap();
        assert!(matches!(b, ParsedRuleBody::Abs(_)));
    }

    #[test]
    fn parse_safe_div() {
        let b = parse("safe_div(a, b, 0)").unwrap();
        assert!(matches!(b, ParsedRuleBody::SafeDiv(_)));
    }

    #[test]
    fn parse_clamp() {
        let b = parse("clamp(x, 0, 100)").unwrap();
        assert!(matches!(b, ParsedRuleBody::Clamp(_)));
    }

    #[test]
    fn parse_coalesce() {
        let b = parse("coalesce(a, b, c)").unwrap();
        assert!(matches!(b, ParsedRuleBody::Coalesce(_)));
    }

    #[test]
    fn parse_actual_ref() {
        let b = parse("actual_ref(Spend)").unwrap();
        match &b {
            ParsedRuleBody::ActualRef(a) => assert_eq!(a.measure, "Spend"),
            _ => panic!("expected ActualRef"),
        }
    }

    #[test]
    fn parse_actual_ref_non_identifier_fires_mc1009() {
        let err = parse("actual_ref(1 + 2)").unwrap_err();
        assert_eq!(err.code, "MC1009");
    }

    #[test]
    fn parse_null_literal() {
        let b = parse("Null").unwrap();
        match &b {
            ParsedRuleBody::Const(c) => assert_eq!(c.value, ParsedScalar::Null),
            _ => panic!("expected Const(Null)"),
        }
    }

    #[test]
    fn round_trip_comparison() {
        assert_round_trip_exact("a > b", "a > b");
        assert_round_trip_exact("a >= b", "a >= b");
        assert_round_trip_exact("a == b", "a == b");
        assert_round_trip_exact("a != b", "a != b");
    }

    #[test]
    fn round_trip_logical() {
        assert_round_trip_exact("a and b", "a and b");
        assert_round_trip_exact("a or b", "a or b");
        assert_round_trip_exact("not a", "not a");
    }

    #[test]
    fn round_trip_precedence_or_and() {
        // or binds looser than and
        assert_round_trip_exact("a or b and c", "a or b and c");
        assert_round_trip_exact("(a or b) and c", "(a or b) and c");
    }

    #[test]
    fn round_trip_functions() {
        assert_round_trip("if(a > 0, a, 0)");
        assert_round_trip("min(Spend, CPC)");
        assert_round_trip("max(a, b, c)");
        assert_round_trip("abs(x)");
        assert_round_trip("safe_div(a, b, 0)");
        assert_round_trip("clamp(x, 0, 100)");
        assert_round_trip("coalesce(a, b, c)");
        assert_round_trip("actual_ref(Spend)");
    }

    #[test]
    fn round_trip_prev_lag() {
        assert_round_trip("prev(Spend)");
        assert_round_trip("lag(Spend, 3)");
        assert_round_trip("cumulative(Revenue)");
        assert_round_trip("rolling_avg(CPC, 3)");
        assert_round_trip("period_index()");
    }

    #[test]
    fn round_trip_ref_data() {
        assert_round_trip_exact(
            "benchmark(\"industry_cpc\", Channel)",
            "benchmark(\"industry_cpc\", Channel)",
        );
        assert_round_trip_exact(
            "lookup(\"tax_rate\", Market)",
            "lookup(\"tax_rate\", Market)",
        );
        assert_round_trip_exact("bucket(CPC, \"cpc_health\")", "bucket(CPC, \"cpc_health\")");
        assert_round_trip_exact("sum_over(Channel, Spend)", "sum_over(Channel, Spend)");
    }

    #[test]
    fn round_trip_null_literal() {
        assert_round_trip_exact("Null", "Null");
        assert_round_trip("if_null(Spend, Null)");
    }

    #[test]
    fn round_trip_complex_phase3e() {
        assert_round_trip("if(Spend > 1000 and CPC < 5, min(Spend, Budget), 0)");
        assert_round_trip("safe_div(Revenue - prev(Revenue), prev(Revenue), 0)");
    }

    // -- Existing round-trip cases preserved --

    #[test]
    fn round_trip_all_five_acme_formulas() {
        for f in [
            "Spend / CPC",
            "Clicks * CVR",
            "Leads * Close_Rate",
            "Customers * AOV",
            "Revenue * (1 - COGS_Rate)",
        ] {
            assert_round_trip(f);
        }
    }

    // -- Phase 3F.1: anchor function tests --

    #[test]
    fn parse_anchor_functions() {
        assert!(matches!(
            parse("anchor_index()").unwrap(),
            ParsedRuleBody::AnchorIndex(_)
        ));
        assert!(matches!(
            parse("is_past()").unwrap(),
            ParsedRuleBody::IsPast(_)
        ));
        assert!(matches!(
            parse("is_current()").unwrap(),
            ParsedRuleBody::IsCurrent(_)
        ));
        assert!(matches!(
            parse("is_future()").unwrap(),
            ParsedRuleBody::IsFuture(_)
        ));
        assert!(matches!(
            parse("periods_since_anchor()").unwrap(),
            ParsedRuleBody::PeriodsSinceAnchor(_)
        ));
        assert!(matches!(
            parse("periods_to_end()").unwrap(),
            ParsedRuleBody::PeriodsToEnd(_)
        ));
    }

    #[test]
    fn round_trip_anchor_functions() {
        assert_round_trip_exact("anchor_index()", "anchor_index()");
        assert_round_trip_exact("is_past()", "is_past()");
        assert_round_trip_exact("is_current()", "is_current()");
        assert_round_trip_exact("is_future()", "is_future()");
        assert_round_trip_exact("periods_since_anchor()", "periods_since_anchor()");
        assert_round_trip_exact("periods_to_end()", "periods_to_end()");
    }

    #[test]
    fn round_trip_anchor_in_expression() {
        assert_round_trip("if(is_past(), actual_ref(Revenue), Revenue * 1.1)");
        assert_round_trip("periods_since_anchor() + periods_to_end()");
    }

    // -- Phase 3H: fitted-model evaluation tests --

    #[test]
    fn parse_predict() {
        let b = parse("predict(\"my_model\", a, b, c)").unwrap();
        match &b {
            ParsedRuleBody::Predict(p) => {
                assert_eq!(p.model_id, "my_model");
                assert_eq!(p.features.len(), 3);
            }
            _ => panic!("expected Predict"),
        }
    }

    #[test]
    fn parse_predict_no_features() {
        let b = parse("predict(\"my_model\")").unwrap();
        match &b {
            ParsedRuleBody::Predict(p) => {
                assert_eq!(p.model_id, "my_model");
                assert_eq!(p.features.len(), 0);
            }
            _ => panic!("expected Predict"),
        }
    }

    #[test]
    fn parse_calibrate() {
        let b = parse("calibrate(x, \"my_map\")").unwrap();
        match &b {
            ParsedRuleBody::Calibrate(c) => {
                assert_eq!(c.map_id, "my_map");
            }
            _ => panic!("expected Calibrate"),
        }
    }

    #[test]
    fn parse_calibrate_missing_map_fires_mc1004() {
        let err = parse("calibrate(x)").unwrap_err();
        assert_eq!(err.code, "MC1008");
    }

    #[test]
    fn parse_exp() {
        let b = parse("exp(x)").unwrap();
        assert!(matches!(b, ParsedRuleBody::Exp(_)));
    }

    #[test]
    fn parse_exp_arity_fires_mc1008() {
        let err = parse("exp(a, b)").unwrap_err();
        assert_eq!(err.code, "MC1008");
    }

    #[test]
    fn parse_norm_cdf() {
        let b = parse("norm_cdf(x, 0, 1)").unwrap();
        assert!(matches!(b, ParsedRuleBody::NormCdf(_)));
    }

    #[test]
    fn parse_norm_cdf_arity_fires_mc1008() {
        let err = parse("norm_cdf(x, y)").unwrap_err();
        assert_eq!(err.code, "MC1008");
    }

    #[test]
    fn round_trip_predict() {
        assert_round_trip_exact(
            "predict(\"nba_model\", pace, rating, total)",
            "predict(\"nba_model\", pace, rating, total)",
        );
    }

    #[test]
    fn round_trip_predict_no_features() {
        assert_round_trip_exact("predict(\"model\")", "predict(\"model\")");
    }

    #[test]
    fn round_trip_calibrate() {
        assert_round_trip_exact(
            "calibrate(raw_prob, \"my_map\")",
            "calibrate(raw_prob, \"my_map\")",
        );
    }

    #[test]
    fn round_trip_exp() {
        assert_round_trip_exact("exp(x)", "exp(x)");
        assert_round_trip("exp(x + 1)");
    }

    #[test]
    fn round_trip_norm_cdf() {
        assert_round_trip_exact("norm_cdf(x, 0, 1)", "norm_cdf(x, 0, 1)");
        assert_round_trip("norm_cdf(Market_Line, Predicted_Total, 17.251)");
    }

    #[test]
    fn round_trip_fitted_model_complex() {
        assert_round_trip("1 - norm_cdf(Market_Line, predict(\"model\", a, b), 17.25)");
        assert_round_trip("calibrate(predict(\"model\", x), \"cal_map\")");
    }
}
