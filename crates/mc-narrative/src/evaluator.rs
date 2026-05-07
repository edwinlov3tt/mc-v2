//! Expression evaluator for template `when:` predicates and `bindings:`.
//!
//! Session 2 upgrade: the evaluator is now **cube-aware**. Aggregate
//! functions (`count_where`, `any_where`, `names_where`, `first_where`)
//! are evaluated generically at eval time by iterating over dimension
//! elements and evaluating arbitrary predicate expressions per-element.
//! This closes Finding #1 (pre-computed aggregates → generic evaluation).
//!
//! Also adds `NOT` operator support (Finding #4) and `not()` function.

use crate::context::CubeData;
use crate::renderer::readable_name;
use std::collections::HashMap;

/// Runtime value type for the expression evaluator.
#[derive(Debug, Clone)]
pub enum Val {
    /// Numeric value.
    Num(f64),
    /// String value.
    Str(String),
    /// Boolean value.
    Bool(bool),
    /// Null / missing value.
    Null,
}

impl Val {
    /// Try to interpret this value as a number.
    pub fn as_num(&self) -> Option<f64> {
        match self {
            Val::Num(n) => Some(*n),
            Val::Bool(true) => Some(1.0),
            Val::Bool(false) => Some(0.0),
            _ => None,
        }
    }

    /// Whether this value is truthy (non-zero, non-empty, non-null).
    pub fn is_truthy(&self) -> bool {
        match self {
            Val::Num(n) => *n != 0.0,
            Val::Bool(b) => *b,
            Val::Str(s) => !s.is_empty(),
            Val::Null => false,
        }
    }

    /// Display this value as a string for template substitution.
    pub fn to_display(&self) -> String {
        match self {
            Val::Num(n) => {
                if (*n - n.round()).abs() < 1e-9 {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            Val::Str(s) => s.clone(),
            Val::Bool(b) => b.to_string(),
            Val::Null => "N/A".to_string(),
        }
    }
}

/// Evaluation context: a flat map of named values.
pub type Ctx = HashMap<String, Val>;

/// Evaluate an expression against a context only (no cube data access).
///
/// Use `eval_expr_with_cube` when aggregate functions may appear.
pub fn eval_expr(expr: &str, ctx: &Ctx) -> Val {
    eval_expr_with_cube(expr, ctx, None)
}

/// Evaluate an expression against a context + optional cube data.
///
/// Session 2: when `cube` is `Some`, aggregate functions like
/// `count_where(predicate, dimension)` are evaluated generically
/// by iterating over dimension elements. When `cube` is `None`,
/// falls back to context variable lookup (backward compat).
pub fn eval_expr_with_cube(expr: &str, ctx: &Ctx, cube: Option<&CubeData>) -> Val {
    // Normalize whitespace: YAML folded scalars can embed newlines.
    let expr = expr.replace('\n', " ");
    let expr = expr.trim();
    if expr.is_empty() {
        return Val::Null;
    }

    // Literal "true" / "false"
    if expr == "true" {
        return Val::Bool(true);
    }
    if expr == "false" {
        return Val::Bool(false);
    }

    // NOT prefix (Finding #4): "NOT expr" or "not expr"
    if let Some(rest) = expr
        .strip_prefix("NOT ")
        .or_else(|| expr.strip_prefix("not "))
    {
        let val = eval_expr_with_cube(rest, ctx, cube);
        return Val::Bool(!val.is_truthy());
    }

    // AND / OR (lowest precedence, split from left)
    if let Some(val) = try_logical(expr, ctx, cube) {
        return val;
    }

    // Comparisons: >=, <=, !=, ==, >, <
    if let Some(val) = try_comparison(expr, ctx, cube) {
        return val;
    }

    // Addition / subtraction (left-to-right, respecting parens)
    if let Some(val) = try_additive(expr, ctx, cube) {
        return val;
    }

    // Multiplication / division
    if let Some(val) = try_multiplicative(expr, ctx, cube) {
        return val;
    }

    // Unary / atoms
    eval_atom(expr, ctx, cube)
}

fn try_logical(expr: &str, ctx: &Ctx, cube: Option<&CubeData>) -> Option<Val> {
    for keyword in &[" AND ", " OR "] {
        if let Some(pos) = find_top_level(expr, keyword) {
            let left = eval_expr_with_cube(&expr[..pos], ctx, cube);
            let right = eval_expr_with_cube(&expr[pos + keyword.len()..], ctx, cube);
            return Some(match *keyword {
                " AND " => Val::Bool(left.is_truthy() && right.is_truthy()),
                " OR " => Val::Bool(left.is_truthy() || right.is_truthy()),
                _ => unreachable!(),
            });
        }
    }
    None
}

fn try_comparison(expr: &str, ctx: &Ctx, cube: Option<&CubeData>) -> Option<Val> {
    for op in &[">=", "<=", "!=", "==", ">", "<"] {
        if let Some(pos) = find_top_level(expr, op) {
            let left = eval_expr_with_cube(&expr[..pos], ctx, cube);
            let right = eval_expr_with_cube(&expr[pos + op.len()..], ctx, cube);
            let (l, r) = match (left.as_num(), right.as_num()) {
                (Some(l), Some(r)) => (l, r),
                _ => return Some(Val::Bool(false)),
            };
            return Some(Val::Bool(match *op {
                ">=" => l >= r,
                "<=" => l <= r,
                "!=" => (l - r).abs() > 1e-9,
                "==" => (l - r).abs() < 1e-9,
                ">" => l > r,
                "<" => l < r,
                _ => false,
            }));
        }
    }
    None
}

fn try_additive(expr: &str, ctx: &Ctx, cube: Option<&CubeData>) -> Option<Val> {
    let bytes = expr.as_bytes();
    let mut depth = 0i32;
    let mut last_pos: Option<(usize, u8)> = None;
    for i in 0..bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'+' | b'-' if depth == 0 && i > 0 => {
                let prev_char = bytes[i - 1];
                if prev_char != b'*' && prev_char != b'/' && prev_char != b'(' {
                    last_pos = Some((i, bytes[i]));
                }
            }
            _ => {}
        }
    }
    if let Some((pos, op)) = last_pos {
        let left = eval_expr_with_cube(&expr[..pos], ctx, cube);
        let right = eval_expr_with_cube(&expr[pos + 1..], ctx, cube);
        if let (Some(l), Some(r)) = (left.as_num(), right.as_num()) {
            return Some(Val::Num(if op == b'+' { l + r } else { l - r }));
        }
    }
    None
}

fn try_multiplicative(expr: &str, ctx: &Ctx, cube: Option<&CubeData>) -> Option<Val> {
    let bytes = expr.as_bytes();
    let mut depth = 0i32;
    let mut last_pos: Option<(usize, u8)> = None;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'*' | b'/' if depth == 0 => {
                last_pos = Some((i, b));
            }
            _ => {}
        }
    }
    if let Some((pos, op)) = last_pos {
        let left = eval_expr_with_cube(&expr[..pos], ctx, cube);
        let right = eval_expr_with_cube(&expr[pos + 1..], ctx, cube);
        if let (Some(l), Some(r)) = (left.as_num(), right.as_num()) {
            return Some(Val::Num(if op == b'*' {
                l * r
            } else if r.abs() < 1e-15 {
                0.0
            } else {
                l / r
            }));
        }
    }
    None
}

fn eval_atom(expr: &str, ctx: &Ctx, cube: Option<&CubeData>) -> Val {
    let expr = expr.trim();

    // Parenthesized expression.
    if expr.starts_with('(') && expr.ends_with(')') && matching_paren(expr) == expr.len() - 1 {
        return eval_expr_with_cube(&expr[1..expr.len() - 1], ctx, cube);
    }

    // String literal: 'text'
    if expr.starts_with('\'') && expr.ends_with('\'') && expr.len() >= 2 {
        return Val::Str(expr[1..expr.len() - 1].to_string());
    }

    // Numeric literal.
    if let Ok(n) = expr.parse::<f64>() {
        return Val::Num(n);
    }

    // Function calls.
    if let Some(inner) = strip_func(expr, "abs") {
        return match eval_expr_with_cube(inner, ctx, cube).as_num() {
            Some(n) => Val::Num(n.abs()),
            None => Val::Null,
        };
    }
    if let Some(inner) = strip_func(expr, "not") {
        // Finding #4: NOT operator as function call.
        let val = eval_expr_with_cube(inner, ctx, cube);
        return Val::Bool(!val.is_truthy());
    }
    if let Some(inner) = strip_func(expr, "if") {
        return eval_if(inner, ctx, cube);
    }
    if let Some(inner) = strip_func(expr, "element_count") {
        let key = format!("element_count({})", inner.trim());
        return ctx.get(&key).cloned().unwrap_or(Val::Num(0.0));
    }

    // Generic aggregate functions (Finding #1: evaluate arbitrary predicates).
    if let Some(inner) = strip_func(expr, "count_where") {
        return eval_count_where(inner, ctx, cube);
    }
    if let Some(inner) = strip_func(expr, "any_where") {
        return eval_any_where(inner, ctx, cube);
    }
    if let Some(inner) = strip_func(expr, "names_where") {
        return eval_names_where(inner, ctx, cube);
    }
    // first_where has dotted access: first_where(pred, dim).field
    if expr.starts_with("first_where(") {
        return eval_first_where(expr, ctx, cube);
    }

    // Context variable lookup (the common case).
    if let Some(val) = ctx.get(expr) {
        return val.clone();
    }

    // Fallback: period_name aliases.
    if expr == "prev_period" || expr == "prev.period_name" {
        return ctx
            .get("prev.period_name")
            .cloned()
            .unwrap_or(Val::Str("N/A".into()));
    }
    if expr == "current_period" || expr == "current.period_name" {
        return ctx
            .get("current.period_name")
            .cloned()
            .unwrap_or(Val::Str("N/A".into()));
    }

    Val::Null
}

fn eval_if(args_str: &str, ctx: &Ctx, cube: Option<&CubeData>) -> Val {
    let parts = split_top_level_commas(args_str);
    if parts.len() < 3 {
        return Val::Null;
    }
    let cond = eval_expr_with_cube(parts[0], ctx, cube);
    if cond.is_truthy() {
        eval_expr_with_cube(parts[1], ctx, cube)
    } else {
        eval_expr_with_cube(parts[2], ctx, cube)
    }
}

// ─── Generic aggregate functions (Finding #1) ──────────────────────

/// Build a per-element context for evaluating predicates.
///
/// For element at index `idx` in the cube's data, populates a context
/// with each measure's value at that index so that arbitrary predicate
/// expressions can reference any measure by name.
fn build_element_ctx(cube: &CubeData, idx: usize, base_ctx: &Ctx) -> Ctx {
    let mut elem_ctx = base_ctx.clone();
    for (measure, entries) in &cube.values {
        if let Some(entry) = entries.get(idx) {
            elem_ctx.insert(measure.clone(), Val::Num(entry.value));
        }
    }
    elem_ctx
}

/// Get the number of elements (rows) in the cube.
fn element_count(cube: &CubeData) -> usize {
    cube.values.values().next().map(|v| v.len()).unwrap_or(0)
}

/// `count_where(predicate, dimension)` — count elements matching predicate.
///
/// Finding #1: evaluates ANY predicate expression generically, not just
/// pre-computed conditions. E.g., `count_where(Impressions < 200, City)`
/// iterates all City elements, evaluates `Impressions < 200` per-element.
fn eval_count_where(args: &str, ctx: &Ctx, cube: Option<&CubeData>) -> Val {
    // Try context lookup first (backward compat with pre-computed values).
    let key = format!("count_where({args})");
    if let Some(val) = ctx.get(key.trim()) {
        return val.clone();
    }

    let cube = match cube {
        Some(c) => c,
        None => return Val::Num(0.0),
    };

    let parts = split_top_level_commas(args);
    if parts.is_empty() {
        return Val::Num(0.0);
    }
    let predicate = parts[0].trim();

    let count = (0..element_count(cube))
        .filter(|&idx| {
            let elem_ctx = build_element_ctx(cube, idx, ctx);
            eval_expr_with_cube(predicate, &elem_ctx, Some(cube)).is_truthy()
        })
        .count();

    Val::Num(count as f64)
}

/// `any_where(predicate, dimension)` — true if any element matches.
fn eval_any_where(args: &str, ctx: &Ctx, cube: Option<&CubeData>) -> Val {
    let key = format!("any_where({args})");
    if let Some(val) = ctx.get(key.trim()) {
        return val.clone();
    }

    let cube = match cube {
        Some(c) => c,
        None => return Val::Bool(false),
    };

    let parts = split_top_level_commas(args);
    if parts.is_empty() {
        return Val::Bool(false);
    }
    let predicate = parts[0].trim();

    let found = (0..element_count(cube)).any(|idx| {
        let elem_ctx = build_element_ctx(cube, idx, ctx);
        eval_expr_with_cube(predicate, &elem_ctx, Some(cube)).is_truthy()
    });

    Val::Bool(found)
}

/// `names_where(predicate, dimension)` — comma-separated names of matching elements.
fn eval_names_where(args: &str, ctx: &Ctx, cube: Option<&CubeData>) -> Val {
    let key = format!("names_where({args})");
    if let Some(val) = ctx.get(key.trim()) {
        return val.clone();
    }

    let cube = match cube {
        Some(c) => c,
        None => return Val::Str(String::new()),
    };

    let parts = split_top_level_commas(args);
    if parts.is_empty() {
        return Val::Str(String::new());
    }
    let predicate = parts[0].trim();

    // Use the first measure's entries for category names.
    let entries = match cube.values.values().next() {
        Some(e) => e,
        None => return Val::Str(String::new()),
    };

    let names: Vec<String> = (0..element_count(cube))
        .filter(|&idx| {
            let elem_ctx = build_element_ctx(cube, idx, ctx);
            eval_expr_with_cube(predicate, &elem_ctx, Some(cube)).is_truthy()
        })
        .filter_map(|idx| entries.get(idx).map(|e| readable_name(&e.category)))
        .collect();

    Val::Str(names.join(", "))
}

/// `first_where(predicate, dimension).field` — first matching element's field.
///
/// Handles dotted access: `first_where(pred, dim).name` returns the
/// category name; `first_where(pred, dim).Measure` returns the measure value.
fn eval_first_where(full_expr: &str, ctx: &Ctx, cube: Option<&CubeData>) -> Val {
    // first_where is often accessed with dot-notation:
    //   first_where(Clicks == 0 AND Impressions > 50, geo_dimension).name
    //   first_where(Clicks == 0 AND Impressions > 50, geo_dimension).Impressions
    // The full_expr includes the dot-access; strip_func only gave us the parens content.

    // Try context lookup first.
    if let Some(val) = ctx.get(full_expr.trim()) {
        return val.clone();
    }

    let cube = match cube {
        Some(c) => c,
        None => return Val::Null,
    };

    // Parse the full expression to find the dot-access field.
    // Shape: first_where(predicate, dim).field
    let after_close = match full_expr.find(").") {
        Some(pos) => &full_expr[pos + 2..],
        None => return Val::Null,
    };
    let field = after_close.trim();

    // Re-extract the args from the full expression.
    let paren_start = match full_expr.find('(') {
        Some(pos) => pos + 1,
        None => return Val::Null,
    };
    let paren_end = match full_expr.rfind(").") {
        Some(pos) => pos,
        None => return Val::Null,
    };
    let args = &full_expr[paren_start..paren_end];
    let parts = split_top_level_commas(args);
    if parts.is_empty() {
        return Val::Null;
    }
    let predicate = parts[0].trim();

    let entries = match cube.values.values().next() {
        Some(e) => e,
        None => return Val::Null,
    };

    // Find first matching element.
    let idx = (0..element_count(cube)).find(|&idx| {
        let elem_ctx = build_element_ctx(cube, idx, ctx);
        eval_expr_with_cube(predicate, &elem_ctx, Some(cube)).is_truthy()
    });

    let idx = match idx {
        Some(i) => i,
        None => return Val::Null,
    };

    if field == "name" {
        entries
            .get(idx)
            .map(|e| Val::Str(readable_name(&e.category)))
            .unwrap_or(Val::Null)
    } else {
        // Field is a measure name — look up the value at this index.
        cube.values
            .get(field)
            .and_then(|m| m.get(idx))
            .map(|e| Val::Num(e.value))
            .unwrap_or(Val::Null)
    }
}

// ─── Parsing helpers ────────────────────────────────────────────────

/// Find the position of `needle` at the top level (not inside parens or quotes).
fn find_top_level(haystack: &str, needle: &str) -> Option<usize> {
    let bytes = haystack.as_bytes();
    let needle_bytes = needle.as_bytes();
    let mut depth = 0i32;
    let mut in_quote = false;
    let mut i = 0;
    while i + needle_bytes.len() <= bytes.len() {
        let b = bytes[i];
        if b == b'\'' {
            in_quote = !in_quote;
        }
        if !in_quote {
            if b == b'(' {
                depth += 1;
            } else if b == b')' {
                depth -= 1;
            }
            if depth == 0 && &bytes[i..i + needle_bytes.len()] == needle_bytes {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Find the index of the closing paren matching the opening paren at position 0.
fn matching_paren(s: &str) -> usize {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    s.len()
}

/// Strip a function call: "func(args)" -> Some("args")
fn strip_func<'a>(expr: &'a str, func_name: &str) -> Option<&'a str> {
    let expr = expr.trim();
    if expr.starts_with(func_name)
        && expr[func_name.len()..].starts_with('(')
        && expr.ends_with(')')
    {
        Some(&expr[func_name.len() + 1..expr.len() - 1])
    } else {
        None
    }
}

/// Split a string on top-level commas (not inside parens or quotes).
pub fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut in_quote = false;
    let mut start = 0;
    for (byte_pos, c) in s.char_indices() {
        match c {
            '\'' => in_quote = !in_quote,
            '(' if !in_quote => depth += 1,
            ')' if !in_quote => depth -= 1,
            ',' if depth == 0 && !in_quote => {
                parts.push(&s[start..byte_pos]);
                start = byte_pos + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CellEntry;
    use std::collections::BTreeMap;

    fn make_ctx(pairs: &[(&str, f64)]) -> Ctx {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), Val::Num(*v)))
            .collect()
    }

    fn make_geo_cube() -> CubeData {
        CubeData {
            table_name: "Performance by City".into(),
            subproduct: "Targeted Display".into(),
            source_file: "test.csv".into(),
            dimension_name: Some("City".into()),
            values: BTreeMap::from([
                (
                    "Impressions".into(),
                    vec![
                        CellEntry {
                            category: "Rockford".into(),
                            value: 45000.0,
                        },
                        CellEntry {
                            category: "Springfield".into(),
                            value: 300.0,
                        },
                        CellEntry {
                            category: "Peoria".into(),
                            value: 80.0,
                        },
                    ],
                ),
                (
                    "Clicks".into(),
                    vec![
                        CellEntry {
                            category: "Rockford".into(),
                            value: 150.0,
                        },
                        CellEntry {
                            category: "Springfield".into(),
                            value: 5.0,
                        },
                        CellEntry {
                            category: "Peoria".into(),
                            value: 0.0,
                        },
                    ],
                ),
            ]),
        }
    }

    #[test]
    fn test_eval_arithmetic() {
        let ctx = make_ctx(&[("x", 10.0), ("y", 3.0)]);
        assert_eq!(eval_expr("x + y", &ctx).as_num().unwrap(), 13.0);
        assert_eq!(eval_expr("x - y", &ctx).as_num().unwrap(), 7.0);
        assert_eq!(eval_expr("x * y", &ctx).as_num().unwrap(), 30.0);
        assert!((eval_expr("x / y", &ctx).as_num().unwrap() - 3.333333).abs() < 0.001);
    }

    #[test]
    fn test_eval_comparison() {
        let ctx = make_ctx(&[("a", 5.0), ("b", 3.0)]);
        assert!(eval_expr("a > b", &ctx).is_truthy());
        assert!(!eval_expr("a < b", &ctx).is_truthy());
        assert!(eval_expr("a >= 5", &ctx).is_truthy());
        assert!(eval_expr("a == 5", &ctx).is_truthy());
    }

    #[test]
    fn test_eval_logical() {
        let ctx = make_ctx(&[("x", 1.0), ("y", 0.0)]);
        assert!(eval_expr("x > 0 AND y == 0", &ctx).is_truthy());
        assert!(eval_expr("x > 0 OR y > 0", &ctx).is_truthy());
        assert!(!eval_expr("x > 0 AND y > 0", &ctx).is_truthy());
    }

    #[test]
    fn test_eval_not_operator() {
        // Finding #4: NOT operator support.
        let ctx = make_ctx(&[("x", 1.0), ("y", 0.0)]);
        assert!(!eval_expr("NOT x > 0", &ctx).is_truthy());
        assert!(eval_expr("NOT y > 0", &ctx).is_truthy());
        assert!(eval_expr("not(y > 0)", &ctx).is_truthy());
        assert!(!eval_expr("not(x > 0)", &ctx).is_truthy());
    }

    #[test]
    fn test_eval_if() {
        let ctx = make_ctx(&[("x", 5.0)]);
        let val = eval_expr("if(x > 3, 'high', 'low')", &ctx);
        assert!(matches!(val, Val::Str(s) if s == "high"));
    }

    #[test]
    fn test_eval_nested_if() {
        let ctx = make_ctx(&[("period_count", 2.0)]);
        let val = eval_expr(
            "if(period_count == 1, 'one', if(period_count == 2, 'two', 'many'))",
            &ctx,
        );
        match &val {
            Val::Str(s) => assert_eq!(s, "two", "nested if should return 'two'"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn test_eval_abs() {
        let ctx = make_ctx(&[("x", -42.0)]);
        assert_eq!(eval_expr("abs(x)", &ctx).as_num().unwrap(), 42.0);
    }

    // ─── Generic aggregate function tests (Finding #1) ──────────────

    #[test]
    fn test_count_where_generic() {
        let cube = make_geo_cube();
        let ctx = crate::context::build_context(&cube);

        // count_where(Impressions < 500, City) should match Springfield (300) and Peoria (80).
        let result = eval_expr_with_cube("count_where(Impressions < 500, City)", &ctx, Some(&cube));
        assert_eq!(
            result.as_num().unwrap(),
            2.0,
            "should find 2 cities with < 500 impressions"
        );

        // count_where(Clicks == 0, City) should match only Peoria.
        let result = eval_expr_with_cube("count_where(Clicks == 0, City)", &ctx, Some(&cube));
        assert_eq!(
            result.as_num().unwrap(),
            1.0,
            "should find 1 city with 0 clicks"
        );

        // Arbitrary predicate: count_where(Impressions > 100 AND Clicks > 0, City)
        let result = eval_expr_with_cube(
            "count_where(Impressions > 100 AND Clicks > 0, City)",
            &ctx,
            Some(&cube),
        );
        assert_eq!(
            result.as_num().unwrap(),
            2.0,
            "Rockford and Springfield have >100 impr and >0 clicks"
        );
    }

    #[test]
    fn test_any_where_generic() {
        let cube = make_geo_cube();
        let ctx = crate::context::build_context(&cube);

        let result = eval_expr_with_cube(
            "any_where(Clicks == 0 AND Impressions > 50, City)",
            &ctx,
            Some(&cube),
        );
        assert!(result.is_truthy(), "Peoria has 0 clicks and 80 impressions");
    }

    #[test]
    fn test_names_where_generic() {
        let cube = make_geo_cube();
        let ctx = crate::context::build_context(&cube);

        let result = eval_expr_with_cube("names_where(Impressions < 500, City)", &ctx, Some(&cube));
        match result {
            Val::Str(s) => {
                assert!(s.contains("Springfield"), "should include Springfield: {s}");
                assert!(s.contains("Peoria"), "should include Peoria: {s}");
            }
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn test_first_where_generic() {
        let cube = make_geo_cube();
        let ctx = crate::context::build_context(&cube);

        let result = eval_expr_with_cube(
            "first_where(Clicks == 0 AND Impressions > 50, City).name",
            &ctx,
            Some(&cube),
        );
        match result {
            Val::Str(s) => assert_eq!(s, "Peoria", "first zero-click city should be Peoria"),
            other => panic!("expected Str, got {other:?}"),
        }

        let result = eval_expr_with_cube(
            "first_where(Clicks == 0 AND Impressions > 50, City).Impressions",
            &ctx,
            Some(&cube),
        );
        assert_eq!(result.as_num().unwrap(), 80.0, "Peoria has 80 impressions");
    }

    #[test]
    fn test_count_where_arbitrary_threshold() {
        // Finding #1: an analyst can write count_where(Impressions < 200, City)
        // even though that specific threshold was never pre-computed.
        let cube = make_geo_cube();
        let ctx = crate::context::build_context(&cube);

        let result = eval_expr_with_cube("count_where(Impressions < 200, City)", &ctx, Some(&cube));
        assert_eq!(
            result.as_num().unwrap(),
            1.0,
            "only Peoria (80) has < 200 impressions"
        );
    }
}
