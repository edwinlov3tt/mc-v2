//! `mc model query` — read cells by coordinate filter.
//!
//! The most important Phase 6A verb. Everything else (whatif, sweep, diff)
//! builds on the query infrastructure. Supports --where, --show, --coord,
//! --aggregate, --output, and --format json|csv|text.

use mc_core::{CellCoordinate, PrincipalId, ScalarValue};
use mc_model::ModelRefs;
use std::collections::BTreeMap;
use std::fmt::Write as FmtWrite;

// Phase 6A.2 item 1.1: `LoadedModel` and `load_model` now live in
// `crate::loader` (the four-source state model). Re-export here so
// the existing public surface (used by every Phase 6A verb's CLI
// module) is unchanged. New callers that need to opt out of the
// post-hoc writes log replay should use
// `crate::loader::load_model_with_policy(path, LoadPolicy::Reproducible)`.
#[allow(unused_imports)]
pub use crate::loader::{
    load_model, load_model_with_policy, LoadModelError, LoadPolicy, LoadedModel,
};

// ---------------------------------------------------------------------------
// Public entry point (called from main.rs dispatch)
// ---------------------------------------------------------------------------

pub struct QueryCommand {
    pub path: String,
    pub format: OutputFormat,
    pub where_expr: Option<String>,
    pub show: Option<Vec<String>>,
    pub coord: Option<String>,
    pub aggregate: Option<Vec<String>>,
    pub output: Option<String>,
    pub limit: Option<usize>,
    /// Phase 6A.2 item 1.7: skip the first N matched rows before
    /// applying `--limit`. Default 0. With the default `--limit
    /// 10000`, `--offset 100` returns up to 10000 rows starting at
    /// match index 100.
    pub offset: Option<usize>,
    pub time_anchor: Option<String>,
    /// Phase 6A.3 item 4: dimensions to partition matched coordinates by
    /// before aggregating. Repeatable (`--group-by Market --group-by
    /// Time`) and cross-product semantics — one row per (Market, Time)
    /// tuple. Mutually exclusive with `--show`. Requires `--aggregate`.
    pub group_by: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
    Csv,
}

pub fn parse(args: &[String]) -> Result<QueryCommand, String> {
    if args.is_empty() {
        return Err("`mc model query` requires a YAML model path".into());
    }
    let mut path: Option<String> = None;
    let mut format = OutputFormat::Text;
    let mut where_expr: Option<String> = None;
    let mut show: Option<Vec<String>> = None;
    let mut coord: Option<String> = None;
    let mut aggregate: Option<Vec<String>> = None;
    let mut output: Option<String> = None;
    let mut limit: Option<usize> = None;
    let mut time_anchor: Option<String> = None;

    let mut offset: Option<usize> = None;
    let mut group_by: Vec<String> = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--format" => match iter.next() {
                Some(v) if v == "text" => format = OutputFormat::Text,
                Some(v) if v == "json" => format = OutputFormat::Json,
                Some(v) if v == "csv" => format = OutputFormat::Csv,
                Some(v) => return Err(format!("--format must be text|json|csv, got {v:?}")),
                None => return Err("--format requires an argument".into()),
            },
            "--where" => match iter.next() {
                Some(v) => where_expr = Some(v.clone()),
                None => return Err("--where requires an expression argument".into()),
            },
            "--show" => match iter.next() {
                Some(v) => show = Some(v.split(',').map(|s| s.trim().to_string()).collect()),
                None => return Err("--show requires a comma-separated list".into()),
            },
            "--coord" => match iter.next() {
                Some(v) => coord = Some(v.clone()),
                None => return Err("--coord requires a coordinate string".into()),
            },
            "--aggregate" => match iter.next() {
                Some(v) => aggregate = Some(v.split(',').map(|s| s.trim().to_string()).collect()),
                None => return Err("--aggregate requires a function list".into()),
            },
            "--output" => match iter.next() {
                Some(v) => output = Some(v.clone()),
                None => return Err("--output requires a file path".into()),
            },
            "--limit" => match iter.next() {
                Some(v) => {
                    limit = Some(
                        v.parse::<usize>()
                            .map_err(|_| format!("--limit must be a number, got {v:?}"))?,
                    )
                }
                None => return Err("--limit requires a number".into()),
            },
            "--offset" => match iter.next() {
                Some(v) => {
                    offset = Some(
                        v.parse::<usize>()
                            .map_err(|_| format!("--offset must be a number, got {v:?}"))?,
                    )
                }
                None => return Err("--offset requires a number".into()),
            },
            "--time-anchor" => match iter.next() {
                Some(v) => time_anchor = Some(v.clone()),
                None => return Err("--time-anchor requires an element name".into()),
            },
            "--group-by" => match iter.next() {
                Some(v) => group_by.push(v.clone()),
                None => return Err("--group-by requires a dimension name".into()),
            },
            other if !other.starts_with("--") && path.is_none() => {
                path = Some(other.to_string());
            }
            other => return Err(format!("unknown argument: {other:?}")),
        }
    }
    let path = path.ok_or("`mc model query` requires a YAML model path")?;
    Ok(QueryCommand {
        path,
        format,
        where_expr,
        show,
        coord,
        aggregate,
        output,
        limit,
        offset,
        time_anchor,
        group_by,
    })
}

pub fn run(cmd: QueryCommand) -> i32 {
    let (code, output) = run_captured(cmd);
    if !output.is_empty() {
        print!("{output}");
    }
    code
}

/// Execute the query verb and return (exit_code, output_string).
/// Used by MCP to capture output without printing to stdout.
pub fn run_captured(cmd: QueryCommand) -> (i32, String) {
    let compiled = match load_model(&cmd.path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return (e.exit_code(), String::new());
        }
    };
    let mut cube = compiled.cube;
    let principal = compiled.root_principal;
    let refs = &compiled.refs;

    // Apply time-anchor override if provided.
    if let Some(anchor_name) = &cmd.time_anchor {
        let anchor_idx = cube.dimensions().iter().find_map(|dim| {
            dim.elements.iter().enumerate().find_map(|(idx, elem)| {
                if elem.name == *anchor_name {
                    Some(idx)
                } else {
                    None
                }
            })
        });
        match anchor_idx {
            Some(idx) => cube.reference_data.time_anchor_index = Some(idx),
            None => {
                eprintln!("error: --time-anchor '{anchor_name}' does not match any element");
                return (1, String::new());
            }
        }
    }

    // Single-coord shortcut
    if let Some(coord_str) = &cmd.coord {
        return run_single_coord(
            &mut cube,
            refs,
            principal,
            coord_str,
            cmd.format,
            &cmd.output,
        );
    }

    // Phase 6A.3 item 4 W4: --group-by + --show is mutually exclusive
    // (different output shapes — group rows vs. raw cells). The user
    // typo'd one or the other; better to fail than silently honor one.
    if !cmd.group_by.is_empty() && cmd.show.is_some() {
        eprintln!("error: --group-by and --show cannot be combined; use one or the other");
        return (2, String::new());
    }
    // --group-by requires --aggregate (otherwise there's nothing to compute per group).
    if !cmd.group_by.is_empty() && cmd.aggregate.is_none() {
        eprintln!(
            "error: --group-by requires --aggregate (e.g., --aggregate \"sum(Spend)\" --group-by Market)"
        );
        return (2, String::new());
    }
    // Validate every --group-by name is a real dimension.
    for dim_name in &cmd.group_by {
        if !cube.dimensions().iter().any(|d| &d.name == dim_name) {
            eprintln!("error: --group-by dimension {dim_name:?} does not exist in the model");
            return (1, String::new());
        }
    }

    // Build the filter
    let filter = match &cmd.where_expr {
        Some(expr) => match Filter::parse(expr, refs, &cube) {
            Ok(f) => Some(f),
            Err(e) => {
                eprintln!("error: invalid --where expression: {e}");
                return (2, String::new());
            }
        },
        None => None,
    };

    // Determine which measures to show
    let show_measures = resolve_show_measures(&cmd.show, refs, &cube);
    let limit = cmd.limit.unwrap_or(10000);
    let offset = cmd.offset.unwrap_or(0);

    // Enumerate all leaf coordinates and filter
    let all_leaf_coords = enumerate_leaf_coords(&cube, refs);

    if !cmd.group_by.is_empty() {
        // group-by + aggregate path
        return run_group_by(
            &mut cube,
            principal,
            refs,
            &all_leaf_coords,
            filter.as_ref(),
            cmd.aggregate.as_deref().unwrap(),
            &cmd.group_by,
            limit,
            offset,
            cmd.where_expr.as_deref(),
            cmd.format,
            &cmd.output,
        );
    }

    if let Some(agg_exprs) = &cmd.aggregate {
        // Aggregate mode
        return run_aggregate(
            &mut cube,
            principal,
            refs,
            &all_leaf_coords,
            filter.as_ref(),
            agg_exprs,
            cmd.format,
            &cmd.output,
        );
    }

    // Standard row-output mode. Phase 6A.2 item 1.7: track skipped /
    // returned counts separately so we can emit `truncated` and
    // `next_offset` in the envelope.
    let mut results: Vec<QueryRow> = Vec::new();
    let mut skipped = 0usize;
    let mut returned = 0usize;
    let mut truncated = false;

    for coord in &all_leaf_coords {
        if let Some(f) = &filter {
            if !eval_filter(f, coord, &mut cube, principal, refs) {
                continue;
            }
        }
        if skipped < offset {
            skipped += 1;
            continue;
        }
        if returned >= limit {
            truncated = true;
            break;
        }
        let mut values: BTreeMap<String, ScalarValue> = BTreeMap::new();
        let coord_names = coord_to_names(coord, &cube, refs);
        for measure_name in &show_measures {
            // Check if it's a dimension name — if so, use the coord value
            if is_dimension_name(measure_name, &cube) {
                let dim_val = coord_names.get(measure_name).cloned().unwrap_or_default();
                values.insert(measure_name.clone(), ScalarValue::Str(dim_val));
            } else {
                let val = read_measure_at(&mut cube, refs, principal, coord, measure_name);
                values.insert(measure_name.clone(), val);
            }
        }
        results.push(QueryRow {
            coord: coord_names,
            values,
        });
        returned += 1;
    }

    let pagination = Pagination {
        limit,
        offset,
        count: returned,
        truncated,
        next_offset: if truncated {
            Some(offset + returned)
        } else {
            None
        },
    };
    let output_str = format_results(&results, &cmd.where_expr, cmd.format, &pagination);
    let captured = capture_output(&output_str, &cmd.output);
    (0, captured)
}

/// Phase 6A.2 item 1.7: pagination metadata that flows from
/// `run_captured` into the JSON envelope.
struct Pagination {
    limit: usize,
    offset: usize,
    count: usize,
    truncated: bool,
    next_offset: Option<usize>,
}

// ---------------------------------------------------------------------------
// Model loading (re-exported from `crate::loader` — see Phase 6A.2 item 1.1)
// ---------------------------------------------------------------------------

/// Phase 6A.1 CRIT-2: emit the Phase 3B-style envelope header at the
/// start of every Phase 6A verb's `--format json` output. Pairs with
/// the agent contract that every JSON response carries
/// `schema_version: "1.0"` as its first field.
pub fn push_json_envelope_header(out: &mut String) {
    out.push_str("{\n  \"schema_version\": \"");
    out.push_str(mc_model::SCHEMA_VERSION);
    out.push_str("\",\n  ");
}

// ---------------------------------------------------------------------------
// Filter parsing and evaluation
// ---------------------------------------------------------------------------

/// A parsed filter expression. We implement a simple recursive-descent
/// parser here because the formula parser doesn't support string literals
/// in general expressions (only inside function args).
/// Phase 6A.3 item 3: lifted to `pub(crate)` so `sweep.rs` can reuse the
/// query filter parser for `--metric-where`. No external surface change.
#[derive(Debug)]
pub(crate) enum Filter {
    And(Box<Filter>, Box<Filter>),
    Or(Box<Filter>, Box<Filter>),
    Not(Box<Filter>),
    Compare(FilterAtom, CmpOp, FilterValue),
}

#[derive(Debug, Clone)]
pub(crate) enum FilterAtom {
    /// A measure reference — will read the measure value at the current coord.
    Measure(String),
    /// A dimension reference — will resolve to the current element name.
    Dimension(String),
}

#[derive(Debug, Clone)]
pub(crate) enum FilterValue {
    Number(f64),
    StringLit(String),
    Atom(FilterAtom),
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum CmpOp {
    Eq,
    Neq,
    Gt,
    Lt,
    Gte,
    Lte,
}

impl Filter {
    pub(crate) fn parse(
        input: &str,
        refs: &ModelRefs,
        cube: &mc_core::Cube,
    ) -> Result<Filter, String> {
        let tokens = tokenize_filter(input)?;
        let mut pos = 0;
        let result = parse_or(&tokens, &mut pos, refs, cube)?;
        if pos < tokens.len() {
            return Err(format!(
                "unexpected token at position {}: {:?}",
                pos, tokens[pos]
            ));
        }
        Ok(result)
    }
}

#[derive(Debug, Clone)]
enum Token {
    Ident(String),
    Number(f64),
    StringLit(String),
    Op(CmpOp),
    And,
    Or,
    Not,
    LParen,
    RParen,
}

fn tokenize_filter(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            b')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            b'>' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Token::Op(CmpOp::Gte));
                    i += 2;
                } else {
                    tokens.push(Token::Op(CmpOp::Gt));
                    i += 1;
                }
            }
            b'<' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Token::Op(CmpOp::Lte));
                    i += 2;
                } else {
                    tokens.push(Token::Op(CmpOp::Lt));
                    i += 1;
                }
            }
            b'=' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Token::Op(CmpOp::Eq));
                    i += 2;
                } else {
                    return Err(format!("unexpected '=' at position {i} (use '==')"));
                }
            }
            b'!' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Token::Op(CmpOp::Neq));
                    i += 2;
                } else {
                    tokens.push(Token::Not);
                    i += 1;
                }
            }
            b'\'' | b'"' => {
                let quote = bytes[i];
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != quote {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                if i >= bytes.len() {
                    return Err("unterminated string literal".into());
                }
                let s = String::from_utf8_lossy(&bytes[start..i]).to_string();
                tokens.push(Token::StringLit(s));
                i += 1; // closing quote
            }
            c if c.is_ascii_digit()
                || (c == b'-' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit()) =>
            {
                let start = i;
                if c == b'-' {
                    i += 1;
                }
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                    i += 1;
                }
                let num_str = &input[start..i];
                let n: f64 = num_str
                    .parse()
                    .map_err(|_| format!("invalid number: {num_str}"))?;
                tokens.push(Token::Number(n));
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let word = &input[start..i];
                match word {
                    "and" | "AND" => tokens.push(Token::And),
                    "or" | "OR" => tokens.push(Token::Or),
                    "not" | "NOT" => tokens.push(Token::Not),
                    _ => tokens.push(Token::Ident(word.to_string())),
                }
            }
            other => return Err(format!("unexpected character: {:?}", other as char)),
        }
    }
    Ok(tokens)
}

fn parse_or(
    tokens: &[Token],
    pos: &mut usize,
    refs: &ModelRefs,
    cube: &mc_core::Cube,
) -> Result<Filter, String> {
    let mut left = parse_and(tokens, pos, refs, cube)?;
    while *pos < tokens.len() {
        if matches!(&tokens[*pos], Token::Or) {
            *pos += 1;
            let right = parse_and(tokens, pos, refs, cube)?;
            left = Filter::Or(Box::new(left), Box::new(right));
        } else {
            break;
        }
    }
    Ok(left)
}

fn parse_and(
    tokens: &[Token],
    pos: &mut usize,
    refs: &ModelRefs,
    cube: &mc_core::Cube,
) -> Result<Filter, String> {
    let mut left = parse_not(tokens, pos, refs, cube)?;
    while *pos < tokens.len() {
        if matches!(&tokens[*pos], Token::And) {
            *pos += 1;
            let right = parse_not(tokens, pos, refs, cube)?;
            left = Filter::And(Box::new(left), Box::new(right));
        } else {
            break;
        }
    }
    Ok(left)
}

fn parse_not(
    tokens: &[Token],
    pos: &mut usize,
    refs: &ModelRefs,
    cube: &mc_core::Cube,
) -> Result<Filter, String> {
    if *pos < tokens.len() && matches!(&tokens[*pos], Token::Not) {
        *pos += 1;
        let inner = parse_not(tokens, pos, refs, cube)?;
        return Ok(Filter::Not(Box::new(inner)));
    }
    parse_comparison(tokens, pos, refs, cube)
}

fn parse_comparison(
    tokens: &[Token],
    pos: &mut usize,
    refs: &ModelRefs,
    cube: &mc_core::Cube,
) -> Result<Filter, String> {
    if *pos < tokens.len() && matches!(&tokens[*pos], Token::LParen) {
        *pos += 1;
        let inner = parse_or(tokens, pos, refs, cube)?;
        if *pos >= tokens.len() || !matches!(&tokens[*pos], Token::RParen) {
            return Err("missing closing parenthesis".into());
        }
        *pos += 1;
        return Ok(inner);
    }

    // Expect: atom op value
    let lhs = parse_filter_atom(tokens, pos, refs, cube)?;
    if *pos >= tokens.len() {
        return Err("expected comparison operator".into());
    }
    let op = match &tokens[*pos] {
        Token::Op(op) => *op,
        other => return Err(format!("expected comparison operator, got {:?}", other)),
    };
    *pos += 1;
    let rhs = parse_filter_value(tokens, pos, refs, cube)?;
    Ok(Filter::Compare(lhs, op, rhs))
}

fn parse_filter_atom(
    tokens: &[Token],
    pos: &mut usize,
    _refs: &ModelRefs,
    cube: &mc_core::Cube,
) -> Result<FilterAtom, String> {
    if *pos >= tokens.len() {
        return Err("expected identifier".into());
    }
    match &tokens[*pos] {
        Token::Ident(name) => {
            *pos += 1;
            // Check if it's a dimension name first, then measure
            if is_dimension_name(name, cube) {
                Ok(FilterAtom::Dimension(name.clone()))
            } else {
                Ok(FilterAtom::Measure(name.clone()))
            }
        }
        other => Err(format!("expected identifier, got {:?}", other)),
    }
}

fn parse_filter_value(
    tokens: &[Token],
    pos: &mut usize,
    _refs: &ModelRefs,
    cube: &mc_core::Cube,
) -> Result<FilterValue, String> {
    if *pos >= tokens.len() {
        return Err("expected value".into());
    }
    match &tokens[*pos] {
        Token::Number(n) => {
            let v = *n;
            *pos += 1;
            Ok(FilterValue::Number(v))
        }
        Token::StringLit(s) => {
            let v = s.clone();
            *pos += 1;
            Ok(FilterValue::StringLit(v))
        }
        Token::Ident(name) => {
            *pos += 1;
            if is_dimension_name(name, cube) {
                Ok(FilterValue::Atom(FilterAtom::Dimension(name.clone())))
            } else {
                Ok(FilterValue::Atom(FilterAtom::Measure(name.clone())))
            }
        }
        other => Err(format!("expected value, got {:?}", other)),
    }
}

fn is_dimension_name(name: &str, cube: &mc_core::Cube) -> bool {
    cube.dimensions().iter().any(|d| d.name == name)
}

/// Phase 6A.3 item 3: lifted to `pub(crate)` for reuse from `sweep.rs`.
pub(crate) fn eval_filter(
    filter: &Filter,
    coord: &CellCoordinate,
    cube: &mut mc_core::Cube,
    principal: PrincipalId,
    refs: &ModelRefs,
) -> bool {
    match filter {
        Filter::And(l, r) => {
            eval_filter(l, coord, cube, principal, refs)
                && eval_filter(r, coord, cube, principal, refs)
        }
        Filter::Or(l, r) => {
            eval_filter(l, coord, cube, principal, refs)
                || eval_filter(r, coord, cube, principal, refs)
        }
        Filter::Not(inner) => !eval_filter(inner, coord, cube, principal, refs),
        Filter::Compare(atom, op, value) => {
            let lhs = resolve_atom(atom, coord, cube, principal, refs);
            let rhs = resolve_value(value, coord, cube, principal, refs);
            compare_values(&lhs, *op, &rhs)
        }
    }
}

#[derive(Debug)]
enum ResolvedValue {
    F64(f64),
    Str(String),
    Null,
}

fn resolve_atom(
    atom: &FilterAtom,
    coord: &CellCoordinate,
    cube: &mut mc_core::Cube,
    principal: PrincipalId,
    refs: &ModelRefs,
) -> ResolvedValue {
    match atom {
        FilterAtom::Dimension(dim_name) => {
            // Resolve to the element name at this coordinate's position in that dimension
            let dim_idx = cube.dimensions().iter().position(|d| d.name == *dim_name);
            match dim_idx {
                Some(idx) => {
                    let elem_id = coord.elements()[idx];
                    let dim = &cube.dimensions()[idx];
                    match dim.element(elem_id) {
                        Some(elem) => ResolvedValue::Str(elem.name.clone()),
                        None => ResolvedValue::Null,
                    }
                }
                None => ResolvedValue::Null,
            }
        }
        FilterAtom::Measure(measure_name) => {
            // Build a coord with this measure and read it
            let val = read_measure_at(cube, refs, principal, coord, measure_name);
            match val {
                ScalarValue::F64(f) => ResolvedValue::F64(f),
                ScalarValue::I64(i) => ResolvedValue::F64(i as f64),
                ScalarValue::Bool(b) => ResolvedValue::F64(if b { 1.0 } else { 0.0 }),
                ScalarValue::Str(s) => ResolvedValue::Str(s),
                ScalarValue::Category(_) | ScalarValue::Null => ResolvedValue::Null,
            }
        }
    }
}

fn resolve_value(
    value: &FilterValue,
    coord: &CellCoordinate,
    cube: &mut mc_core::Cube,
    principal: PrincipalId,
    refs: &ModelRefs,
) -> ResolvedValue {
    match value {
        FilterValue::Number(n) => ResolvedValue::F64(*n),
        FilterValue::StringLit(s) => ResolvedValue::Str(s.clone()),
        FilterValue::Atom(atom) => resolve_atom(atom, coord, cube, principal, refs),
    }
}

fn compare_values(lhs: &ResolvedValue, op: CmpOp, rhs: &ResolvedValue) -> bool {
    match (lhs, rhs) {
        (ResolvedValue::F64(a), ResolvedValue::F64(b)) => match op {
            CmpOp::Eq => (*a - *b).abs() < 1e-9,
            CmpOp::Neq => (*a - *b).abs() >= 1e-9,
            CmpOp::Gt => *a > *b,
            CmpOp::Lt => *a < *b,
            CmpOp::Gte => *a >= *b || (*a - *b).abs() < 1e-9,
            CmpOp::Lte => *a <= *b || (*a - *b).abs() < 1e-9,
        },
        (ResolvedValue::Str(a), ResolvedValue::Str(b)) => match op {
            CmpOp::Eq => a == b,
            CmpOp::Neq => a != b,
            _ => false, // string ordering not supported
        },
        // Comparing a number == 1 with a string (for Should_Bet == 1 style)
        (ResolvedValue::F64(_a), ResolvedValue::Str(_)) => false,
        (ResolvedValue::Str(_), ResolvedValue::F64(_)) => false,
        (ResolvedValue::Null, _) | (_, ResolvedValue::Null) => match op {
            CmpOp::Eq => matches!((lhs, rhs), (ResolvedValue::Null, ResolvedValue::Null)),
            CmpOp::Neq => !matches!((lhs, rhs), (ResolvedValue::Null, ResolvedValue::Null)),
            _ => false,
        },
    }
}

// ---------------------------------------------------------------------------
// Coordinate enumeration
// ---------------------------------------------------------------------------

/// Enumerate all leaf coordinates in the cube (excluding the Measure dimension).
/// Returns coords for every combination of leaf elements across all non-Measure dims,
/// paired with every measure. For query, we iterate only the specific measures
/// requested via --show after filtering, so we return coords without measure fixed.
pub(crate) fn enumerate_leaf_coords(
    cube: &mc_core::Cube,
    _refs: &ModelRefs,
) -> Vec<CellCoordinate> {
    let dims = cube.dimensions();
    // For each non-Measure dimension, collect leaf elements.
    // For Measure dimension, we'll use the first measure as a placeholder
    // (the actual measure read happens per --show column).
    let mut dim_leaves: Vec<Vec<mc_core::ElementId>> = Vec::new();

    for dim in dims {
        if dim.kind == mc_core::DimensionKind::Measure {
            // Use first element as placeholder — we swap measures during read
            if let Some(first) = dim.elements.first() {
                dim_leaves.push(vec![first.id]);
            } else {
                dim_leaves.push(vec![]);
            }
        } else {
            let hierarchy = dim.default_hierarchy();
            let leaves: Vec<mc_core::ElementId> = if hierarchy.edges.is_empty() {
                // Flat dimension (no hierarchy) — all elements are leaves
                dim.elements.iter().map(|e| e.id).collect()
            } else {
                dim.elements
                    .iter()
                    .filter(|e| hierarchy.is_leaf(e.id))
                    .map(|e| e.id)
                    .collect()
            };
            dim_leaves.push(leaves);
        }
    }

    // Cartesian product of all dim leaves
    let mut coords = Vec::new();
    // Guard: if any dimension has 0 elements, no coords can be built
    if dim_leaves.iter().any(|v| v.is_empty()) {
        return coords;
    }
    let mut indices = vec![0usize; dim_leaves.len()];
    let cube_id = cube.id;

    loop {
        // Build coord from current indices
        let slots: Vec<mc_core::ElementId> = indices
            .iter()
            .enumerate()
            .map(|(dim_idx, &elem_idx)| dim_leaves[dim_idx][elem_idx])
            .collect();
        coords.push(CellCoordinate::from_parts(cube_id, slots));

        // Advance indices (rightmost first)
        let mut carry = true;
        for d in (0..dim_leaves.len()).rev() {
            if !carry {
                break;
            }
            indices[d] += 1;
            if indices[d] >= dim_leaves[d].len() {
                indices[d] = 0;
            } else {
                carry = false;
            }
        }
        if carry {
            break;
        }
    }

    coords
}

/// Read a specific measure at a coordinate (swapping the measure dimension slot).
pub(crate) fn read_measure_at(
    cube: &mut mc_core::Cube,
    _refs: &ModelRefs,
    principal: PrincipalId,
    base_coord: &CellCoordinate,
    measure_name: &str,
) -> ScalarValue {
    let measure_dim_idx = cube
        .dimensions()
        .iter()
        .position(|d| d.kind == mc_core::DimensionKind::Measure);
    let Some(measure_dim_idx) = measure_dim_idx else {
        return ScalarValue::Null;
    };
    let measure_dim = &cube.dimensions()[measure_dim_idx];
    let measure_elem = measure_dim.element_by_name(measure_name);
    let Some(measure_elem) = measure_elem else {
        return ScalarValue::Null;
    };

    let mut slots = base_coord.elements().to_vec();
    slots[measure_dim_idx] = measure_elem.id;
    let coord = CellCoordinate::from_parts(cube.id, slots);

    match cube.read(&coord, principal) {
        Ok(cell) => cell.value,
        Err(_) => ScalarValue::Null,
    }
}

fn resolve_show_measures(
    show: &Option<Vec<String>>,
    _refs: &ModelRefs,
    cube: &mc_core::Cube,
) -> Vec<String> {
    match show {
        Some(names) => names.clone(),
        None => {
            // Show all measures
            let measure_dim = cube
                .dimensions()
                .iter()
                .find(|d| d.kind == mc_core::DimensionKind::Measure);
            match measure_dim {
                Some(dim) => dim.elements.iter().map(|e| e.name.clone()).collect(),
                None => vec![],
            }
        }
    }
}

fn coord_to_names(
    coord: &CellCoordinate,
    cube: &mc_core::Cube,
    _refs: &ModelRefs,
) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let dims = cube.dimensions();
    for (idx, dim) in dims.iter().enumerate() {
        if dim.kind == mc_core::DimensionKind::Measure {
            continue; // Skip measure dim in coord output
        }
        let elem_id = coord.elements()[idx];
        if let Some(elem) = dim.element(elem_id) {
            map.insert(dim.name.clone(), elem.name.clone());
        }
    }
    map
}

// ---------------------------------------------------------------------------
// Single-coord read
// ---------------------------------------------------------------------------

fn run_single_coord(
    cube: &mut mc_core::Cube,
    refs: &ModelRefs,
    principal: PrincipalId,
    coord_str: &str,
    format: OutputFormat,
    output: &Option<String>,
) -> (i32, String) {
    let names = parse_coord_string(coord_str);
    let coord = match refs.coord_from_names(&names) {
        Some(c) => c,
        None => {
            eprintln!("error: could not resolve coordinate: {coord_str}");
            return (1, String::new());
        }
    };
    match cube.read(&coord, principal) {
        Ok(cell) => {
            let result_str = match format {
                OutputFormat::Json => {
                    let mut out = String::new();
                    push_json_envelope_header(&mut out);
                    out.push_str("\"coord\": ");
                    out.push_str(&format_coord_json(&names));
                    out.push_str(",\n  \"value\": ");
                    push_scalar_json(&mut out, &cell.value);
                    out.push_str("\n}\n");
                    out
                }
                OutputFormat::Text => {
                    format!("{}\n", format_scalar(&cell.value))
                }
                OutputFormat::Csv => {
                    let mut out = String::new();
                    for k in names.keys() {
                        let _ = write!(out, "{k},");
                    }
                    out.push_str("value\n");
                    for v in names.values() {
                        let _ = write!(out, "{v},");
                    }
                    out.push_str(&format_scalar(&cell.value));
                    out.push('\n');
                    out
                }
            };
            let captured = capture_output(&result_str, output);
            (0, captured)
        }
        Err(e) => {
            eprintln!("error: read failed: {e}");
            (1, String::new())
        }
    }
}

// ---------------------------------------------------------------------------
// Aggregate mode
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn run_aggregate(
    cube: &mut mc_core::Cube,
    principal: PrincipalId,
    refs: &ModelRefs,
    all_coords: &[CellCoordinate],
    filter: Option<&Filter>,
    agg_exprs: &[String],
    format: OutputFormat,
    output: &Option<String>,
) -> (i32, String) {
    // Parse aggregate expressions: mean(Measure), sum(Measure), count(predicate),
    // min(Measure), max(Measure)
    let mut agg_results: Vec<(String, f64)> = Vec::new();

    // First pass: collect matching rows
    let mut matching_coords: Vec<&CellCoordinate> = Vec::new();
    for coord in all_coords {
        if let Some(f) = filter {
            if !eval_filter(f, coord, cube, principal, refs) {
                continue;
            }
        }
        matching_coords.push(coord);
    }
    let matched_count = matching_coords.len();

    for expr_str in agg_exprs {
        let expr_str = expr_str.trim();
        if let Some(inner) = strip_fn("mean", expr_str) {
            let sum: f64 = matching_coords
                .iter()
                .filter_map(|c| match read_measure_at(cube, refs, principal, c, inner) {
                    ScalarValue::F64(v) => Some(v),
                    _ => None,
                })
                .sum();
            let count = matching_coords
                .iter()
                .filter(|c| {
                    matches!(
                        read_measure_at(cube, refs, principal, c, inner),
                        ScalarValue::F64(_)
                    )
                })
                .count();
            let mean = if count > 0 { sum / count as f64 } else { 0.0 };
            agg_results.push((expr_str.to_string(), mean));
        } else if let Some(inner) = strip_fn("sum", expr_str) {
            let sum: f64 = matching_coords
                .iter()
                .filter_map(|c| match read_measure_at(cube, refs, principal, c, inner) {
                    ScalarValue::F64(v) => Some(v),
                    _ => None,
                })
                .sum();
            agg_results.push((expr_str.to_string(), sum));
        } else if let Some(inner) = strip_fn("min", expr_str) {
            let min = matching_coords
                .iter()
                .filter_map(|c| match read_measure_at(cube, refs, principal, c, inner) {
                    ScalarValue::F64(v) => Some(v),
                    _ => None,
                })
                .fold(f64::INFINITY, f64::min);
            let min = if min == f64::INFINITY { 0.0 } else { min };
            agg_results.push((expr_str.to_string(), min));
        } else if let Some(inner) = strip_fn("max", expr_str) {
            let max = matching_coords
                .iter()
                .filter_map(|c| match read_measure_at(cube, refs, principal, c, inner) {
                    ScalarValue::F64(v) => Some(v),
                    _ => None,
                })
                .fold(f64::NEG_INFINITY, f64::max);
            let max = if max == f64::NEG_INFINITY { 0.0 } else { max };
            agg_results.push((expr_str.to_string(), max));
        } else if let Some(inner) = strip_fn("count", expr_str) {
            // count(predicate) — parse inner as a filter
            let count = if inner.contains("==")
                || inner.contains('>')
                || inner.contains('<')
                || inner.contains("!=")
            {
                match Filter::parse(inner, refs, cube) {
                    Ok(f) => matching_coords
                        .iter()
                        .filter(|c| eval_filter(&f, c, cube, principal, refs))
                        .count(),
                    Err(_) => 0,
                }
            } else {
                // count(Measure) — count non-null values
                matching_coords
                    .iter()
                    .filter(|c| {
                        matches!(
                            read_measure_at(cube, refs, principal, c, inner),
                            ScalarValue::F64(_)
                        )
                    })
                    .count()
            };
            agg_results.push((expr_str.to_string(), count as f64));
        } else {
            eprintln!("warning: unrecognized aggregate: {expr_str}");
            agg_results.push((expr_str.to_string(), 0.0));
        }
    }

    let output_str = match format {
        OutputFormat::Json => {
            let mut out = String::new();
            push_json_envelope_header(&mut out);
            out.push_str("\"query\": null,\n  \"results\": null,\n  \"count\": ");
            let _ = write!(out, "{matched_count}");
            out.push_str(",\n  \"aggregates\": {");
            for (i, (name, val)) in agg_results.iter().enumerate() {
                out.push_str("\n    ");
                push_json_str(&mut out, name);
                out.push_str(": ");
                push_f64_json(&mut out, *val);
                if i + 1 < agg_results.len() {
                    out.push(',');
                }
            }
            out.push_str("\n  }\n}\n");
            out
        }
        OutputFormat::Text => {
            let mut out = String::new();
            let _ = writeln!(out, "Aggregates ({matched_count} rows matched):");
            for (name, val) in &agg_results {
                let _ = writeln!(out, "  {name}: {}", format_f64(*val));
            }
            out
        }
        OutputFormat::Csv => {
            let mut out = String::new();
            out.push_str("aggregate,value\n");
            for (name, val) in &agg_results {
                let _ = writeln!(out, "{name},{}", format_f64(*val));
            }
            out
        }
    };
    let captured = capture_output(&output_str, output);
    (0, captured)
}

// ---------------------------------------------------------------------------
// Phase 6A.3 item 4 — group-by aggregate
// ---------------------------------------------------------------------------

/// One output row of a `--group-by` query — a group-key tuple plus the
/// computed aggregates for the matched coordinates in that group.
struct GroupRow {
    /// Ordered (DimName, ElementName) pairs in the order the user passed
    /// `--group-by`. Preserved as a Vec rather than BTreeMap so the
    /// dimension order is the user's order, not lexicographic.
    key: Vec<(String, String)>,
    /// `(aggregate_expr, value)` pairs in the order the user passed
    /// `--aggregate`.
    aggregates: Vec<(String, f64)>,
}

#[allow(clippy::too_many_arguments)]
fn run_group_by(
    cube: &mut mc_core::Cube,
    principal: PrincipalId,
    refs: &ModelRefs,
    all_coords: &[CellCoordinate],
    filter: Option<&Filter>,
    agg_exprs: &[String],
    group_dims: &[String],
    limit: usize,
    offset: usize,
    where_expr: Option<&str>,
    format: OutputFormat,
    output: &Option<String>,
) -> (i32, String) {
    // Resolve group-by dim indices in the cube's dimension array. Each
    // element of `group_dim_indices` is the position of that group dim
    // in the canonical [Scenario, Version, Time, Channel, Market, Measure]
    // ordering; lookups use this index to extract the group key from a
    // coordinate's slot vector. Pre-build (dim_idx → BTreeMap<ElementId,
    // ElementName>) so the bucketing loop holds no live borrow on the
    // cube and is free to call eval_filter (which needs &mut Cube).
    use std::collections::BTreeMap as Map;
    let mut group_dim_indices: Vec<usize> = Vec::with_capacity(group_dims.len());
    let mut name_tables: Vec<Map<mc_core::ElementId, String>> =
        Vec::with_capacity(group_dims.len());
    {
        let dims = cube.dimensions();
        for name in group_dims {
            match dims.iter().position(|d| &d.name == name) {
                Some(idx) => {
                    group_dim_indices.push(idx);
                    let dim = &dims[idx];
                    let mut tbl: Map<mc_core::ElementId, String> = Map::new();
                    for e in &dim.elements {
                        tbl.insert(e.id, e.name.clone());
                    }
                    name_tables.push(tbl);
                }
                None => {
                    // Should be unreachable — caller validated names already.
                    eprintln!("error: --group-by dimension {name:?} not found");
                    return (1, String::new());
                }
            }
        }
    }

    // Bucket matched coords by their group-key tuple. The tuple is built
    // from element NAMES (not IDs) so the lexicographic sort below is
    // stable across model reloads (W5).
    let mut buckets: Map<Vec<String>, Vec<CellCoordinate>> = Map::new();
    for coord in all_coords {
        if let Some(f) = filter {
            if !eval_filter(f, coord, cube, principal, refs) {
                continue;
            }
        }
        // Build the (Dim1=Elem1, Dim2=Elem2, ...) name tuple.
        let mut key: Vec<String> = Vec::with_capacity(group_dim_indices.len());
        let mut name_lookup_failed = false;
        for (slot_idx, &dim_idx) in group_dim_indices.iter().enumerate() {
            let elem_id = coord.elements()[dim_idx];
            match name_tables[slot_idx].get(&elem_id) {
                Some(n) => key.push(n.clone()),
                None => {
                    name_lookup_failed = true;
                    break;
                }
            }
        }
        if name_lookup_failed {
            continue;
        }
        buckets.entry(key).or_default().push(coord.clone());
    }

    // Compute aggregates per group. BTreeMap iteration is in-order over
    // the keys, which gives lexicographic ordering on the (ElementName)
    // tuples — exactly what W5 requires.
    let mut group_rows: Vec<GroupRow> = Vec::new();
    for (key, coords) in &buckets {
        // W6: skip empty groups. Defensive — buckets only contain non-empty
        // entries by construction, but keep the check explicit.
        if coords.is_empty() {
            continue;
        }
        let aggregates = compute_aggregates_for_group(cube, refs, principal, coords, agg_exprs);
        let key_pairs: Vec<(String, String)> = group_dims
            .iter()
            .zip(key.iter())
            .map(|(d, e)| (d.clone(), e.clone()))
            .collect();
        group_rows.push(GroupRow {
            key: key_pairs,
            aggregates,
        });
    }

    // W7: --limit/--offset paginate the group rows (NOT the underlying
    // matched coords).
    let total = group_rows.len();
    let start = offset.min(total);
    let end = (offset + limit).min(total);
    let truncated = end < total;
    let next_offset = if truncated { Some(end) } else { None };
    let page: &[GroupRow] = &group_rows[start..end];

    let pagination = Pagination {
        limit,
        offset,
        count: page.len(),
        truncated,
        next_offset,
    };
    let output_str = format_group_rows(page, where_expr, &pagination, format);
    let captured = capture_output(&output_str, output);
    (0, captured)
}

/// Run every aggregate in `agg_exprs` over `coords`. Returns
/// `(expr, value)` pairs in the order supplied. Mirrors run_aggregate's
/// per-expression dispatch but operates on a pre-filtered subset.
fn compute_aggregates_for_group(
    cube: &mut mc_core::Cube,
    refs: &ModelRefs,
    principal: PrincipalId,
    coords: &[CellCoordinate],
    agg_exprs: &[String],
) -> Vec<(String, f64)> {
    let mut out = Vec::with_capacity(agg_exprs.len());
    for expr_str in agg_exprs {
        let expr_str = expr_str.trim();
        let value = if let Some(inner) = strip_fn("mean", expr_str) {
            let mut sum = 0.0;
            let mut count = 0usize;
            for c in coords {
                if let ScalarValue::F64(v) = read_measure_at(cube, refs, principal, c, inner) {
                    sum += v;
                    count += 1;
                }
            }
            if count > 0 {
                sum / count as f64
            } else {
                0.0
            }
        } else if let Some(inner) = strip_fn("sum", expr_str) {
            let mut sum = 0.0;
            for c in coords {
                if let ScalarValue::F64(v) = read_measure_at(cube, refs, principal, c, inner) {
                    sum += v;
                }
            }
            sum
        } else if let Some(inner) = strip_fn("min", expr_str) {
            let mut min = f64::INFINITY;
            for c in coords {
                if let ScalarValue::F64(v) = read_measure_at(cube, refs, principal, c, inner) {
                    if v < min {
                        min = v;
                    }
                }
            }
            if min == f64::INFINITY {
                0.0
            } else {
                min
            }
        } else if let Some(inner) = strip_fn("max", expr_str) {
            let mut max = f64::NEG_INFINITY;
            for c in coords {
                if let ScalarValue::F64(v) = read_measure_at(cube, refs, principal, c, inner) {
                    if v > max {
                        max = v;
                    }
                }
            }
            if max == f64::NEG_INFINITY {
                0.0
            } else {
                max
            }
        } else if let Some(inner) = strip_fn("count", expr_str) {
            // count(predicate) — parse as a Filter; otherwise count(measure).
            let count = if inner.contains("==")
                || inner.contains('>')
                || inner.contains('<')
                || inner.contains("!=")
            {
                match Filter::parse(inner, refs, cube) {
                    Ok(f) => coords
                        .iter()
                        .filter(|c| eval_filter(&f, c, cube, principal, refs))
                        .count(),
                    Err(_) => 0,
                }
            } else {
                coords
                    .iter()
                    .filter(|c| {
                        matches!(
                            read_measure_at(cube, refs, principal, c, inner),
                            ScalarValue::F64(_)
                        )
                    })
                    .count()
            };
            count as f64
        } else {
            0.0
        };
        out.push((expr_str.to_string(), value));
    }
    out
}

fn format_group_rows(
    rows: &[GroupRow],
    where_expr: Option<&str>,
    pagination: &Pagination,
    format: OutputFormat,
) -> String {
    match format {
        OutputFormat::Json => {
            let mut out = String::new();
            push_json_envelope_header(&mut out);
            out.push_str("\"query\": ");
            match where_expr {
                Some(q) => push_json_str(&mut out, q),
                None => out.push_str("null"),
            }
            let _ = write!(
                out,
                ",\n  \"limit\": {},\n  \"offset\": {},\n  \"count\": {},\n  \"truncated\": {}",
                pagination.limit, pagination.offset, pagination.count, pagination.truncated
            );
            out.push_str(",\n  \"next_offset\": ");
            match pagination.next_offset {
                Some(n) => {
                    let _ = write!(out, "{n}");
                }
                None => out.push_str("null"),
            }
            out.push_str(",\n  \"rows\": [");
            for (i, row) in rows.iter().enumerate() {
                out.push_str(if i == 0 { "\n    " } else { ",\n    " });
                out.push_str("{\"group\":{");
                for (j, (k, v)) in row.key.iter().enumerate() {
                    if j > 0 {
                        out.push(',');
                    }
                    push_json_str(&mut out, k);
                    out.push(':');
                    push_json_str(&mut out, v);
                }
                out.push_str("},\"aggregates\":{");
                for (j, (name, val)) in row.aggregates.iter().enumerate() {
                    if j > 0 {
                        out.push(',');
                    }
                    push_json_str(&mut out, name);
                    out.push(':');
                    push_f64_json(&mut out, *val);
                }
                out.push_str("}}");
            }
            if !rows.is_empty() {
                out.push('\n');
                out.push_str("  ");
            }
            out.push_str("]\n}\n");
            out
        }
        OutputFormat::Text => {
            let mut out = String::new();
            let _ = writeln!(out, "Groups: {} (page; total may be larger)", rows.len());
            for row in rows {
                let key_str = row
                    .key
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let _ = writeln!(out, "  {key_str}");
                for (name, val) in &row.aggregates {
                    let _ = writeln!(out, "    {name} = {}", format_f64(*val));
                }
            }
            out
        }
        OutputFormat::Csv => {
            let mut out = String::new();
            // Header: group dim names, then one column per aggregate.
            if let Some(first) = rows.first() {
                for (i, (k, _)) in first.key.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(k);
                }
                for (name, _) in &first.aggregates {
                    out.push(',');
                    out.push_str(name);
                }
                out.push('\n');
            }
            for row in rows {
                for (i, (_, v)) in row.key.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(v);
                }
                for (_, val) in &row.aggregates {
                    out.push(',');
                    out.push_str(&format_f64(*val));
                }
                out.push('\n');
            }
            out
        }
    }
}

fn strip_fn<'a>(name: &str, expr: &'a str) -> Option<&'a str> {
    let trimmed = expr.trim();
    if trimmed.starts_with(name) && trimmed[name.len()..].starts_with('(') && trimmed.ends_with(')')
    {
        Some(&trimmed[name.len() + 1..trimmed.len() - 1])
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Output formatting
// ---------------------------------------------------------------------------

struct QueryRow {
    coord: BTreeMap<String, String>,
    values: BTreeMap<String, ScalarValue>,
}

fn format_results(
    results: &[QueryRow],
    where_expr: &Option<String>,
    format: OutputFormat,
    pagination: &Pagination,
) -> String {
    match format {
        OutputFormat::Json => format_json(results, where_expr, pagination),
        OutputFormat::Text => format_text(results, pagination),
        OutputFormat::Csv => format_csv(results),
    }
}

fn format_json(
    results: &[QueryRow],
    where_expr: &Option<String>,
    pagination: &Pagination,
) -> String {
    let mut out = String::new();
    push_json_envelope_header(&mut out);
    out.push_str("\"query\": ");
    match where_expr {
        Some(q) => push_json_str(&mut out, q),
        None => out.push_str("null"),
    }
    // Phase 6A.2 item 1.7: pagination fields are additive (no
    // schema_version bump per handoff matrix W8).
    let _ = write!(out, ",\n  \"limit\": {}", pagination.limit);
    let _ = write!(out, ",\n  \"offset\": {}", pagination.offset);
    let _ = write!(out, ",\n  \"count\": {}", pagination.count);
    let _ = write!(out, ",\n  \"truncated\": {}", pagination.truncated);
    out.push_str(",\n  \"next_offset\": ");
    match pagination.next_offset {
        Some(n) => {
            let _ = write!(out, "{n}");
        }
        None => out.push_str("null"),
    }
    out.push_str(",\n  \"results\": [\n");
    for (i, row) in results.iter().enumerate() {
        out.push_str("    {\"coord\": ");
        out.push_str(&format_coord_json(&row.coord));
        out.push_str(", \"values\": {");
        let mut first = true;
        for (k, v) in &row.values {
            if !first {
                out.push(',');
            }
            first = false;
            push_json_str(&mut out, k);
            out.push(':');
            push_scalar_json(&mut out, v);
        }
        out.push_str("}}");
        if i + 1 < results.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ],\n  \"aggregates\": null\n}\n");
    out
}

fn format_text(results: &[QueryRow], pagination: &Pagination) -> String {
    let count = pagination.count;
    if results.is_empty() {
        return format!("No results ({count} rows matched)\n");
    }
    let mut out = String::new();
    // Collect value column names from first row
    let value_cols: Vec<&String> = results[0].values.keys().collect();
    let coord_cols: Vec<&String> = results[0].coord.keys().collect();

    // Header
    for c in &coord_cols {
        let _ = write!(out, "{:<15}", c);
    }
    for c in &value_cols {
        let _ = write!(out, "{:<15}", c);
    }
    out.push('\n');

    // Rows
    for row in results {
        for c in &coord_cols {
            let val = row.coord.get(*c).map(|s| s.as_str()).unwrap_or("-");
            let _ = write!(out, "{:<15}", val);
        }
        for c in &value_cols {
            let val = row
                .values
                .get(*c)
                .map(format_scalar)
                .unwrap_or_else(|| "-".into());
            let _ = write!(out, "{:<15}", val);
        }
        out.push('\n');
    }
    let _ = writeln!(out, "\n{count} rows");
    if pagination.truncated {
        if let Some(next) = pagination.next_offset {
            let _ = writeln!(
                out,
                "...truncated at limit {}; pass --offset {next} to continue.",
                pagination.limit
            );
        }
    }
    out
}

fn format_csv(results: &[QueryRow]) -> String {
    if results.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let coord_cols: Vec<&String> = results[0].coord.keys().collect();
    let value_cols: Vec<&String> = results[0].values.keys().collect();

    // Header
    for (i, c) in coord_cols.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(c);
    }
    for c in &value_cols {
        out.push(',');
        out.push_str(c);
    }
    out.push('\n');

    // Rows
    for row in results {
        for (i, c) in coord_cols.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(row.coord.get(*c).map(|s| s.as_str()).unwrap_or(""));
        }
        for c in &value_cols {
            out.push(',');
            if let Some(ScalarValue::F64(f)) = row.values.get(*c) {
                let _ = write!(out, "{f}");
            }
        }
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn parse_coord_string(s: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for part in s.split(',') {
        let part = part.trim();
        if let Some((k, v)) = part.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

fn format_coord_json(coord: &BTreeMap<String, String>) -> String {
    let mut out = String::from("{");
    let mut first = true;
    for (k, v) in coord {
        if !first {
            out.push(',');
        }
        first = false;
        push_json_str(&mut out, k);
        out.push(':');
        push_json_str(&mut out, v);
    }
    out.push('}');
    out
}

pub(crate) fn push_json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn push_scalar_json(out: &mut String, v: &ScalarValue) {
    match v {
        ScalarValue::F64(f) => push_f64_json(out, *f),
        ScalarValue::I64(i) => {
            let _ = write!(out, "{i}");
        }
        ScalarValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        ScalarValue::Str(s) => push_json_str(out, s),
        ScalarValue::Category(c) => {
            let _ = write!(out, "{c}");
        }
        ScalarValue::Null => out.push_str("null"),
    }
}

fn push_f64_json(out: &mut String, f: f64) {
    if f.is_finite() {
        // Avoid unnecessary trailing zeros for integers
        if f == f.trunc() && f.abs() < 1e15 {
            let _ = write!(out, "{}", f as i64);
        } else {
            let _ = write!(out, "{f}");
        }
    } else {
        out.push_str("null");
    }
}

pub fn format_scalar(v: &ScalarValue) -> String {
    match v {
        ScalarValue::F64(f) => format_f64(*f),
        ScalarValue::I64(i) => format!("{i}"),
        ScalarValue::Bool(b) => format!("{b}"),
        ScalarValue::Str(s) => s.clone(),
        ScalarValue::Category(c) => format!("cat({c})"),
        ScalarValue::Null => "null".to_string(),
    }
}

pub fn format_f64(f: f64) -> String {
    if f.is_finite() {
        if f == f.trunc() && f.abs() < 1e15 {
            format!("{}", f as i64)
        } else {
            format!("{f:.6}")
        }
    } else {
        "null".to_string()
    }
}

/// Write output to a file (if output_path is Some) or return it as a string.
/// Used by verb `run_captured` functions so output is never printed directly
/// to stdout — callers (CLI main.rs or MCP) decide where the output goes.
pub fn capture_output(content: &str, output_path: &Option<String>) -> String {
    match output_path {
        Some(path) => {
            if let Err(e) = std::fs::write(path, content) {
                eprintln!("error: could not write to {path}: {e}");
            }
            String::new()
        }
        None => content.to_string(),
    }
}
