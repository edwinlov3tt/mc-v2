//! YAML-driven narrative template engine — per ADR-0019 Decision 4.
//!
//! Phase 7A.1: extract this module to `crates/mc-narrative`.
//! Public boundary: `evaluate_all(templates, cubes) -> Vec<NarrativeOutput>`.
//!
//! Templates live in `demo/narratives/*.yaml`. The engine:
//! 1. Parses YAML at startup into `Vec<TemplateDefinition>`.
//! 2. Per request: builds a pre-computed context from cube data,
//!    evaluates `when:` predicates, resolves `bindings:`, and
//!    substitutes `{placeholders}` into the template string.
//!
//! Adding a new template = appending ~10 lines of YAML. Zero Rust changes.

use crate::ingest::{CellEntry, IngestedCube};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

// ─── Public types (unchanged from before the refactor) ──────────────

/// A rendered narrative paragraph.
#[derive(Debug, Clone, Serialize)]
pub struct NarrativeOutput {
    pub id: String,
    pub severity: Severity,
    pub text: String,
    pub template_id: String,
    pub evidence: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

// ─── YAML schema ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TemplateFile {
    #[allow(dead_code)]
    pub narrative_format_version: u32,
    pub templates: Vec<TemplateDefinition>,
}

#[derive(Debug, Deserialize)]
pub struct TemplateDefinition {
    pub id: String,
    #[allow(dead_code)]
    pub family: Vec<String>,
    pub severity: Severity,
    pub table_types: Vec<String>,
    #[serde(default)]
    pub sort_order: i32,
    pub when: String,
    pub template: String,
    #[serde(default)]
    pub bindings: BTreeMap<String, String>,
}

/// Load all template YAML files from a directory.
pub fn load_templates(dir: &str) -> Vec<TemplateDefinition> {
    let mut all = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return all,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(true, |e| e != "yaml" && e != "yml") {
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("  \x1b[33mwarn\x1b[0m: cannot read {}: {e}", path.display());
                continue;
            }
        };
        match serde_yaml::from_str::<TemplateFile>(&content) {
            Ok(tf) => all.extend(tf.templates),
            Err(e) => {
                eprintln!(
                    "  \x1b[33mwarn\x1b[0m: cannot parse {}: {e}",
                    path.display()
                );
            }
        }
    }
    all.sort_by_key(|t| t.sort_order);
    all
}

// ─── Eval value type ────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Val {
    Num(f64),
    Str(String),
    Bool(bool),
    Null,
}

impl Val {
    fn as_num(&self) -> Option<f64> {
        match self {
            Val::Num(n) => Some(*n),
            Val::Bool(true) => Some(1.0),
            Val::Bool(false) => Some(0.0),
            _ => None,
        }
    }
    fn is_truthy(&self) -> bool {
        match self {
            Val::Num(n) => *n != 0.0,
            Val::Bool(b) => *b,
            Val::Str(s) => !s.is_empty(),
            Val::Null => false,
        }
    }
    fn to_display(&self) -> String {
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

type Ctx = HashMap<String, Val>;

// ─── Public API (unchanged signature) ───────────────────────────────

/// Evaluate all applicable templates against a set of ingested cubes.
pub fn evaluate_all(
    templates: &[TemplateDefinition],
    cubes: &[IngestedCube],
) -> Vec<NarrativeOutput> {
    let mut narratives = Vec::new();
    let mut fired_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for cube in cubes {
        let ctx = build_context(cube);
        let table_lower = cube.table_name.to_lowercase();

        for tmpl in templates {
            // Table type filter.
            let table_match = tmpl
                .table_types
                .iter()
                .any(|t| table_lower.contains(&t.to_lowercase()));
            if !table_match {
                continue;
            }

            // Deduplicate: certain templates should fire at most once.
            let once_only = matches!(
                tmpl.id.as_str(),
                "conversion_alarm"
                    | "data_sufficiency"
                    | "small_sample_warning"
                    | "zero_engagement_alarm"
            );
            if once_only && fired_ids.contains(&tmpl.id) {
                continue;
            }

            // Evaluate when predicate.
            let when_val = eval_expr(&tmpl.when, &ctx);
            if !when_val.is_truthy() {
                continue;
            }

            // Resolve bindings.
            let mut resolved: HashMap<String, Val> = HashMap::new();
            // First pass: bindings that don't reference other bindings.
            for (name, expr) in &tmpl.bindings {
                resolved.insert(name.clone(), eval_expr(expr, &ctx));
            }
            // Second pass: re-evaluate with resolved bindings available.
            let mut merged = ctx.clone();
            for (k, v) in &resolved {
                merged.insert(k.clone(), v.clone());
            }
            for (name, expr) in &tmpl.bindings {
                resolved.insert(name.clone(), eval_expr(expr, &merged));
            }

            // Also add tactic_name to resolved.
            resolved.insert("tactic_name".to_string(), Val::Str(cube.subproduct.clone()));

            // Substitute into template string.
            let text = substitute(&tmpl.template, &resolved, &ctx);

            // Build evidence from numeric bindings.
            let mut evidence = BTreeMap::new();
            for (k, v) in &resolved {
                if let Val::Num(n) = v {
                    evidence.insert(k.clone(), serde_json::json!(n));
                }
            }

            narratives.push(NarrativeOutput {
                id: format!("{}_{}", tmpl.id, cube.source_file.replace(".csv", "")),
                severity: tmpl.severity,
                text,
                template_id: tmpl.id.clone(),
                evidence,
            });

            if once_only {
                fired_ids.insert(tmpl.id.clone());
            }
        }
    }

    narratives
}

// ─── Context builder ────────────────────────────────────────────────

fn build_context(cube: &IngestedCube) -> Ctx {
    let mut ctx = Ctx::new();

    // Tactic metadata.
    ctx.insert("tactic_name".into(), Val::Str(cube.subproduct.clone()));
    ctx.insert("table_name".into(), Val::Str(cube.table_name.clone()));

    // Period info: count time-series entries (from first measure).
    let period_count = cube.values.values().next().map(|v| v.len()).unwrap_or(0);
    ctx.insert("period_count".into(), Val::Num(period_count as f64));

    // Determine the "category" dimension name for this cube.
    let geo_dim = if cube.table_name.to_lowercase().contains("city")
        || cube.table_name.to_lowercase().contains("zip")
    {
        "geo"
    } else if cube.table_name.to_lowercase().contains("device") {
        "Device"
    } else if cube.table_name.to_lowercase().contains("creative") {
        "Creative"
    } else {
        "Category"
    };

    // Per-measure aggregates.
    for (measure, entries) in &cube.values {
        let n = entries.len();
        if n == 0 {
            continue;
        }

        // current (last) and prev (second-to-last).
        let current = entries[n - 1].value;
        let prev = if n >= 2 { entries[n - 2].value } else { 0.0 };
        ctx.insert(format!("current.{measure}"), Val::Num(current));
        ctx.insert(format!("prev.{measure}"), Val::Num(prev));

        // Period names: set once (first measure defines them).
        if !ctx.contains_key("current.period_name") {
            ctx.insert(
                "current.period_name".into(),
                Val::Str(readable_name(&entries[n - 1].category)),
            );
            // Also alias as prev_period / current_period for template convenience.
            ctx.insert(
                "current_period".into(),
                Val::Str(readable_name(&entries[n - 1].category)),
            );
            if n >= 2 {
                ctx.insert(
                    "prev.period_name".into(),
                    Val::Str(readable_name(&entries[n - 2].category)),
                );
                ctx.insert(
                    "prev_period".into(),
                    Val::Str(readable_name(&entries[n - 2].category)),
                );
            }
        }

        // Sum and average.
        let sum: f64 = entries.iter().map(|e| e.value).sum();
        let avg = sum / n as f64;
        ctx.insert(format!("sum.{measure}"), Val::Num(sum));
        ctx.insert(format!("campaign_avg.{measure}"), Val::Num(avg));

        // Also insert without prefix for conversion_alarm ("sum.Conversions" alias).
        // The YAML references "sum.Conversions" for the total-conversions check.
        // Some CSVs call it "Total_Conversions", so alias both.
        if measure.to_lowercase().contains("conversion") {
            ctx.insert("sum.Conversions".into(), Val::Num(sum));
        }

        // Element count for the category dimension.
        ctx.insert(format!("element_count({geo_dim})"), Val::Num(n as f64));
        ctx.insert(format!("element_count({})", measure), Val::Num(n as f64));

        // max_by / min_by across the category dimension.
        if let Some(max_entry) = entries.iter().max_by(|a, b| {
            a.value
                .partial_cmp(&b.value)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            ctx.insert(
                format!("max_by.{geo_dim}.{measure}.name"),
                Val::Str(readable_name(&max_entry.category)),
            );
            ctx.insert(
                format!("max_by.{geo_dim}.{measure}.value"),
                Val::Num(max_entry.value),
            );
        }
        if let Some(min_entry) = entries.iter().min_by(|a, b| {
            a.value
                .partial_cmp(&b.value)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            ctx.insert(
                format!("min_by.{geo_dim}.{measure}.name"),
                Val::Str(readable_name(&min_entry.category)),
            );
            ctx.insert(
                format!("min_by.{geo_dim}.{measure}.value"),
                Val::Num(min_entry.value),
            );
        }
    }

    // Pre-computed growth rates (if we have 2+ periods).
    if let (Some(Val::Num(cur_i)), Some(Val::Num(prev_i))) =
        (ctx.get("current.Impressions"), ctx.get("prev.Impressions"))
    {
        if *prev_i > 0.0 {
            ctx.insert(
                "impr_growth".into(),
                Val::Num((cur_i - prev_i) / prev_i * 100.0),
            );
        }
    }
    if let (Some(Val::Num(cur_c)), Some(Val::Num(prev_c))) =
        (ctx.get("current.Clicks"), ctx.get("prev.Clicks"))
    {
        if *prev_c > 0.0 {
            ctx.insert(
                "click_growth".into(),
                Val::Num((cur_c - prev_c) / prev_c * 100.0),
            );
        }
    }

    // count_where / any_where / names_where / first_where for geo data.
    build_where_functions(cube, &mut ctx, geo_dim);

    ctx
}

/// Build count_where, any_where, names_where, first_where for common conditions.
fn build_where_functions(cube: &IngestedCube, ctx: &mut Ctx, dim_name: &str) {
    let impressions = cube.values.get("Impressions");
    let clicks = cube.values.get("Clicks");

    // Impressions < 500
    if let Some(imp) = impressions {
        let small: Vec<&CellEntry> = imp
            .iter()
            .filter(|e| e.value < 500.0 && e.value > 0.0)
            .collect();
        ctx.insert(
            format!("count_where(Impressions < 500, {dim_name})"),
            Val::Num(small.len() as f64),
        );
        // Also alias for "geo_dimension"
        ctx.insert(
            "count_where(Impressions < 500, geo_dimension)".into(),
            Val::Num(small.len() as f64),
        );
        let names: Vec<String> = small.iter().map(|e| readable_name(&e.category)).collect();
        ctx.insert(
            format!("names_where(Impressions < 500, {dim_name})"),
            Val::Str(names.join(", ")),
        );
        ctx.insert(
            "names_where(Impressions < 500, geo_dimension)".into(),
            Val::Str(names.join(", ")),
        );
    }

    // Clicks == 0 AND Impressions > 50
    if let (Some(imp), Some(clk)) = (impressions, clicks) {
        let zero_engage: Vec<(usize, &CellEntry)> = imp
            .iter()
            .enumerate()
            .filter(|(i, e)| e.value > 50.0 && clk.get(*i).map_or(true, |c| c.value < 1.0))
            .collect();

        ctx.insert(
            "any_where(Clicks == 0 AND Impressions > 50, geo_dimension)".into(),
            Val::Bool(!zero_engage.is_empty()),
        );
        if let Some((idx, entry)) = zero_engage.first() {
            ctx.insert(
                "first_where(Clicks == 0 AND Impressions > 50, geo_dimension).name".into(),
                Val::Str(readable_name(&entry.category)),
            );
            ctx.insert(
                "first_where(Clicks == 0 AND Impressions > 50, geo_dimension).Impressions".into(),
                Val::Num(entry.value),
            );
            let _ = idx; // used only for filtering
        }
    }
}

// ─── Expression evaluator ───────────────────────────────────────────

fn eval_expr(expr: &str, ctx: &Ctx) -> Val {
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

    // AND / OR (lowest precedence, split from left)
    if let Some(val) = try_logical(expr, ctx) {
        return val;
    }

    // Comparisons: >=, <=, !=, ==, >, <
    if let Some(val) = try_comparison(expr, ctx) {
        return val;
    }

    // Addition / subtraction (left-to-right, respecting parens)
    if let Some(val) = try_additive(expr, ctx) {
        return val;
    }

    // Multiplication / division
    if let Some(val) = try_multiplicative(expr, ctx) {
        return val;
    }

    // Unary / atoms
    eval_atom(expr, ctx)
}

fn try_logical(expr: &str, ctx: &Ctx) -> Option<Val> {
    // Split on AND / OR at the top level (not inside parens).
    for keyword in &[" AND ", " OR "] {
        if let Some(pos) = find_top_level(expr, keyword) {
            let left = eval_expr(&expr[..pos], ctx);
            let right = eval_expr(&expr[pos + keyword.len()..], ctx);
            return Some(match *keyword {
                " AND " => Val::Bool(left.is_truthy() && right.is_truthy()),
                " OR " => Val::Bool(left.is_truthy() || right.is_truthy()),
                _ => unreachable!(),
            });
        }
    }
    None
}

fn try_comparison(expr: &str, ctx: &Ctx) -> Option<Val> {
    for op in &[">=", "<=", "!=", "==", ">", "<"] {
        if let Some(pos) = find_top_level(expr, op) {
            let left = eval_expr(&expr[..pos], ctx);
            let right = eval_expr(&expr[pos + op.len()..], ctx);
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

fn try_additive(expr: &str, ctx: &Ctx) -> Option<Val> {
    // Find the rightmost top-level + or - (for left-to-right precedence).
    let bytes = expr.as_bytes();
    let mut depth = 0i32;
    let mut last_pos: Option<(usize, u8)> = None;
    for i in 0..bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'+' | b'-' if depth == 0 && i > 0 => {
                // Don't split on negative sign after an operator or at start.
                let prev_char = bytes[i - 1];
                if prev_char != b'*' && prev_char != b'/' && prev_char != b'(' {
                    last_pos = Some((i, bytes[i]));
                }
            }
            _ => {}
        }
    }
    if let Some((pos, op)) = last_pos {
        let left = eval_expr(&expr[..pos], ctx);
        let right = eval_expr(&expr[pos + 1..], ctx);
        if let (Some(l), Some(r)) = (left.as_num(), right.as_num()) {
            return Some(Val::Num(if op == b'+' { l + r } else { l - r }));
        }
    }
    None
}

fn try_multiplicative(expr: &str, ctx: &Ctx) -> Option<Val> {
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
        let left = eval_expr(&expr[..pos], ctx);
        let right = eval_expr(&expr[pos + 1..], ctx);
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

fn eval_atom(expr: &str, ctx: &Ctx) -> Val {
    let expr = expr.trim();

    // Parenthesized expression.
    if expr.starts_with('(') && expr.ends_with(')') && matching_paren(expr) == expr.len() - 1 {
        return eval_expr(&expr[1..expr.len() - 1], ctx);
    }

    // String literal: 'text'
    if expr.starts_with('\'') && expr.ends_with('\'') && expr.len() >= 2 {
        return Val::Str(expr[1..expr.len() - 1].to_string());
    }

    // Numeric literal.
    if let Ok(n) = expr.parse::<f64>() {
        return Val::Num(n);
    }

    // Function calls: abs(...), if(...), ...
    if let Some(inner) = strip_func(expr, "abs") {
        return match eval_expr(inner, ctx).as_num() {
            Some(n) => Val::Num(n.abs()),
            None => Val::Null,
        };
    }
    if let Some(inner) = strip_func(expr, "if") {
        return eval_if(inner, ctx);
    }
    if let Some(inner) = strip_func(expr, "element_count") {
        // element_count(Dim) — lookup from context.
        let key = format!("element_count({})", inner.trim());
        return ctx.get(&key).cloned().unwrap_or(Val::Num(0.0));
    }

    // Context variable lookup (the common case).
    if let Some(val) = ctx.get(expr) {
        return val.clone();
    }

    // Fallback: try with period_name aliases.
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

fn eval_if(args_str: &str, ctx: &Ctx) -> Val {
    // Split on top-level commas: if(cond, then, else)
    let parts = split_top_level_commas(args_str);
    if parts.len() < 3 {
        return Val::Null;
    }
    let cond = eval_expr(parts[0], ctx);
    if cond.is_truthy() {
        eval_expr(parts[1], ctx)
    } else {
        eval_expr(parts[2], ctx)
    }
}

// ─── Template string substitution ───────────────────────────────────

fn substitute(template: &str, bindings: &HashMap<String, Val>, ctx: &Ctx) -> String {
    let mut result = String::with_capacity(template.len() * 2);
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' {
            let mut placeholder = String::new();
            while let Some(&c) = chars.peek() {
                if c == '}' {
                    chars.next();
                    break;
                }
                placeholder.push(c);
                chars.next();
            }
            // Parse format spec: {name:format} or just {name}
            let (name, fmt_spec) = match placeholder.split_once(':') {
                Some((n, f)) => (n.trim(), Some(f.trim())),
                None => (placeholder.trim(), None),
            };

            // Lookup in bindings first, then context.
            let val = bindings
                .get(name)
                .or_else(|| ctx.get(name))
                .cloned()
                .unwrap_or(Val::Str("N/A".into()));

            result.push_str(&format_val(&val, fmt_spec));
        } else {
            result.push(ch);
        }
    }

    // Clean up whitespace: collapse internal runs of whitespace (from YAML multiline).
    let mut cleaned = String::with_capacity(result.len());
    let mut prev_space = false;
    for ch in result.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                cleaned.push(' ');
            }
            prev_space = true;
        } else {
            cleaned.push(ch);
            prev_space = false;
        }
    }
    cleaned.trim().to_string()
}

fn format_val(val: &Val, fmt: Option<&str>) -> String {
    let fmt = match fmt {
        Some(f) => f,
        None => return val.to_display(),
    };

    match val {
        Val::Num(n) => {
            let use_comma = fmt.contains(',');
            // Extract decimal places from format like ".0f", ".1f", ".2f"
            let decimals = fmt
                .chars()
                .skip_while(|c| *c != '.')
                .skip(1)
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<usize>()
                .unwrap_or(0);

            if use_comma {
                format_comma(*n, decimals)
            } else {
                format!("{:.prec$}", n, prec = decimals)
            }
        }
        _ => val.to_display(),
    }
}

fn format_comma(n: f64, decimals: usize) -> String {
    let rounded = if decimals == 0 {
        n.round() as i64
    } else {
        let factor = 10f64.powi(decimals as i32);
        (n * factor).round() as i64 / factor.round() as i64
    };
    // Integer part with commas.
    let abs = rounded.unsigned_abs();
    let s = abs.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    if rounded < 0 {
        result.push('-');
    }
    result.chars().rev().collect()
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
fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut in_quote = false;
    let mut start = 0;
    // Use char_indices for correct byte offsets with multi-byte UTF-8.
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

fn readable_name(s: &str) -> String {
    let out = s.replace('_', " ");
    let mut result = String::with_capacity(out.len());
    let mut prev_space = false;
    for c in out.chars() {
        if c == ' ' {
            if !prev_space {
                result.push(' ');
            }
            prev_space = true;
        } else {
            result.push(c);
            prev_space = false;
        }
    }
    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx(pairs: &[(&str, f64)]) -> Ctx {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), Val::Num(*v)))
            .collect()
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
    fn test_eval_if() {
        let ctx = make_ctx(&[("x", 5.0)]);
        let val = eval_expr("if(x > 3, 'high', 'low')", &ctx);
        assert!(matches!(val, Val::Str(s) if s == "high"));
    }

    #[test]
    fn test_confidence_from_actual_yaml() {
        // Load actual templates and find data_sufficiency.
        let templates = {
            let t = load_templates("demo/narratives");
            if t.is_empty() {
                load_templates("../../demo/narratives")
            } else {
                t
            }
        };
        let ds = templates
            .iter()
            .find(|t| t.id == "data_sufficiency")
            .expect("data_sufficiency template");
        let confidence_expr = ds.bindings.get("confidence").expect("confidence binding");
        eprintln!("confidence expr repr: {:?}", confidence_expr);

        let mut ctx: Ctx = HashMap::new();
        ctx.insert("period_count".into(), Val::Num(2.0));
        let val = eval_expr(confidence_expr, &ctx);
        eprintln!("confidence val: {:?}", val);
        match &val {
            Val::Str(s) => assert!(s.contains("Directional") || s.contains("trend"), "got: {s}"),
            other => panic!("expected Str, got {other:?}"),
        }
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

    #[test]
    fn test_substitute() {
        let mut bindings = HashMap::new();
        bindings.insert("name".into(), Val::Str("Tablet".into()));
        bindings.insert("pct".into(), Val::Num(83.5));
        let ctx = HashMap::new();
        let result = substitute("Device {name} at {pct:.1f}%", &bindings, &ctx);
        assert_eq!(result, "Device Tablet at 83.5%");
    }

    #[test]
    fn test_format_comma() {
        assert_eq!(format_comma(55757.0, 0), "55,757");
        assert_eq!(format_comma(1000000.0, 0), "1,000,000");
        assert_eq!(format_comma(42.0, 0), "42");
    }

    #[test]
    fn test_load_templates() {
        // Tests may run from crate dir or repo root; try both.
        let templates = {
            let t = load_templates("demo/narratives");
            if t.is_empty() {
                load_templates("../../demo/narratives")
            } else {
                t
            }
        };
        assert!(!templates.is_empty(), "should load at least 1 template");
        assert!(
            templates.len() >= 13,
            "expected >= 13 templates, got {}",
            templates.len()
        );
    }
}
