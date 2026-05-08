//! Mosaic narrative template engine — YAML-driven deterministic report generation.
//!
//! Session 4: DAG-ordered binding resolution (Finding #3), named format
//! hints, `deduplicate: true` YAML field migration, MC7001-MC7010
//! validation at load time.

pub mod benchmark;
pub mod context;
pub mod error;
pub mod evaluator;
pub mod ledger;
pub mod renderer;
pub mod schema;

pub use benchmark::{BenchmarkError, BenchmarkLibrary, MetricBenchmark};
pub use context::{CellEntry, CubeData};
pub use error::NarrativeError;
pub use evaluator::{Ctx, LedgerIndex, Val};
pub use ledger::LedgerEntry;
pub use renderer::{format_comma, format_val, readable_name};
pub use schema::{NarrativeOutput, Severity, TemplateDefinition, TemplateFile};

use std::collections::{BTreeMap, HashMap, HashSet};

/// Load all template definitions from YAML files in a directory.
///
/// Reads every `.yaml` / `.yml` file, parses the `TemplateFile` schema,
/// and returns all template definitions sorted by `sort_order`.
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

/// Validate loaded templates, returning any MC7001-MC7010 errors.
pub fn validate_templates(templates: &[TemplateDefinition]) -> Vec<NarrativeError> {
    let mut errors = Vec::new();
    let mut seen_ids: HashSet<&str> = HashSet::new();

    for tmpl in templates {
        // MC7008: duplicate template ID.
        if !seen_ids.insert(&tmpl.id) {
            errors.push(NarrativeError::DuplicateTemplateId {
                template_id: tmpl.id.clone(),
            });
        }

        // MC7006: invalid severity (caught by serde, but check notability_base).
        // MC7010: notability_base outside [0, 1].
        if let Some(nb) = tmpl.notability_base {
            if !(0.0..=1.0).contains(&nb) {
                errors.push(NarrativeError::NotabilityOutOfRange {
                    template_id: tmpl.id.clone(),
                    value: nb,
                });
            }
        }

        // MC7005: unresolved placeholders in template body.
        // Check that every {name} in the template has a matching binding
        // (or is a known context variable like tactic_name, prev_period, etc.).
        let placeholders = extract_placeholders(&tmpl.template);
        let known_bindings: HashSet<&str> = tmpl.bindings.keys().map(|s| s.as_str()).collect();
        let context_vars: HashSet<&str> = [
            "tactic_name",
            "prev_period",
            "current_period",
            "period_count",
        ]
        .into_iter()
        .collect();
        for ph in &placeholders {
            if !known_bindings.contains(ph.as_str()) && !context_vars.contains(ph.as_str()) {
                // Don't error on dotted paths (current.X, prev.X, etc.) — those are context vars.
                if !ph.contains('.') {
                    errors.push(NarrativeError::UnresolvedPlaceholder {
                        template_id: tmpl.id.clone(),
                        placeholder: ph.clone(),
                    });
                }
            }
        }

        // MC7004: unknown format hints.
        let known_formats = [
            "currency",
            "percent_0",
            "percent_1",
            "percent_2",
            "count",
            "count_short",
            "delta_signed",
            "date_short",
            "date_long",
            "period_relative",
            "decimal_2",
        ];
        for (binding_name, hint) in &tmpl.format {
            if !known_formats.contains(&hint.as_str()) {
                errors.push(NarrativeError::UnknownFormatHint {
                    template_id: tmpl.id.clone(),
                    hint: hint.clone(),
                });
            }
            if !tmpl.bindings.contains_key(binding_name) {
                errors.push(NarrativeError::UnresolvedPlaceholder {
                    template_id: tmpl.id.clone(),
                    placeholder: binding_name.clone(),
                });
            }
        }
    }

    errors
}

/// Extract placeholder names from a template string (the `name` in `{name}` or `{name:fmt}`).
fn extract_placeholders(template: &str) -> Vec<String> {
    let mut placeholders = Vec::new();
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            let mut ph = String::new();
            while let Some(&c) = chars.peek() {
                if c == '}' {
                    chars.next();
                    break;
                }
                ph.push(c);
                chars.next();
            }
            let name = match ph.split_once(':') {
                Some((n, _)) => n.trim(),
                None => ph.trim(),
            };
            if !name.is_empty() {
                placeholders.push(name.to_string());
            }
        }
    }
    placeholders
}

/// Evaluate all applicable templates against a set of cube data sources.
///
/// Session 4: bindings are resolved in **DAG topological order** (Finding #3).
/// A binding that references another binding works to any depth. Cycle
/// detection prevents infinite loops.
///
/// Phase 7A.3: optional `ledger` parameter enables cross-period analysis.
/// When `Some`, ledger query functions (`ledger_count`, `ledger_streak`, etc.)
/// have data to search. When `None`, they return 0/false/Null and templates
/// with ledger predicates in `when:` silently don't fire.
pub fn evaluate_all(
    templates: &[TemplateDefinition],
    cubes: &[CubeData],
    ledger: Option<&[LedgerEntry]>,
) -> Vec<NarrativeOutput> {
    let mut narratives = Vec::new();
    let mut fired_ids: HashSet<String> = HashSet::new();

    for cube in cubes {
        let ctx = context::build_context(cube);
        let table_lower = cube.table_name.to_lowercase();

        // Derive scope key from cube metadata for ledger lookups.
        let scope_key = format!("channel={}", cube.subproduct);

        // Determine current period from context for ledger lookback boundary.
        let current_period = ctx.get("current.period_name").and_then(|v| match v {
            Val::Str(s) => Some(s.clone()),
            _ => None,
        });

        // Build ledger index with current period context.
        let ledger_index = ledger.map(|entries| LedgerIndex::build(entries, current_period));
        let ledger_ref = ledger_index.as_ref();

        for tmpl in templates {
            // Table type filter.
            let table_match = tmpl
                .table_types
                .iter()
                .any(|t| table_lower.contains(&t.to_lowercase()));
            if !table_match {
                continue;
            }

            // Deduplicate: templates with `deduplicate: true` fire at most once.
            if tmpl.deduplicate && fired_ids.contains(&tmpl.id) {
                continue;
            }

            // Evaluate when predicate (with ledger access).
            let when_val = evaluator::eval_expr_with_ledger(
                &tmpl.when,
                &ctx,
                Some(cube),
                ledger_ref,
                &scope_key,
            );
            if !when_val.is_truthy() {
                continue;
            }

            // DAG-ordered binding resolution (Finding #3) with ledger access.
            let resolved = resolve_bindings_dag(&tmpl.bindings, &ctx, cube, ledger_ref, &scope_key);

            // Add tactic_name to resolved bindings.
            let mut resolved = resolved;
            resolved.insert("tactic_name".to_string(), Val::Str(cube.subproduct.clone()));

            // Substitute into template string with format hints.
            let text = renderer::substitute(&tmpl.template, &resolved, &ctx, &tmpl.format);

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

            if tmpl.deduplicate {
                fired_ids.insert(tmpl.id.clone());
            }
        }
    }

    narratives
}

// ─── DAG-ordered binding resolution (Finding #3) ───────────────────

/// Resolve bindings in dependency order using topological sort.
///
/// Each binding expression may reference other bindings by name.
/// The DAG is built by scanning binding expressions for references
/// to other binding names, then resolved in topological order.
/// Cycles are detected and broken (the cyclic binding evaluates to Null).
fn resolve_bindings_dag(
    bindings: &BTreeMap<String, String>,
    base_ctx: &Ctx,
    cube: &CubeData,
    ledger: Option<&LedgerIndex>,
    scope_key: &str,
) -> HashMap<String, Val> {
    if bindings.is_empty() {
        return HashMap::new();
    }

    let binding_names: HashSet<&str> = bindings.keys().map(|s| s.as_str()).collect();

    // Build dependency edges: binding A depends on binding B if A's expression
    // contains B as an identifier token.
    let mut deps: HashMap<&str, Vec<&str>> = HashMap::new();
    for (name, expr) in bindings {
        let mut name_deps = Vec::new();
        for other_name in &binding_names {
            if *other_name != name.as_str() && expr_references(expr, other_name) {
                name_deps.push(*other_name);
            }
        }
        deps.insert(name.as_str(), name_deps);
    }

    // Topological sort (Kahn's algorithm).
    let order = topo_sort(&binding_names, &deps);

    // Resolve in topological order.
    let mut resolved: HashMap<String, Val> = HashMap::new();
    let mut eval_ctx = base_ctx.clone();

    for name in &order {
        if let Some(expr) = bindings.get(*name) {
            let val =
                evaluator::eval_expr_with_ledger(expr, &eval_ctx, Some(cube), ledger, scope_key);
            eval_ctx.insert(name.to_string(), val.clone());
            resolved.insert(name.to_string(), val);
        }
    }

    resolved
}

/// Check if an expression string references a given identifier.
///
/// Simple heuristic: the identifier appears as a word boundary
/// (not inside a longer identifier). Handles the common cases in
/// template expressions.
fn expr_references(expr: &str, name: &str) -> bool {
    let expr_bytes = expr.as_bytes();
    let name_bytes = name.as_bytes();
    if name_bytes.len() > expr_bytes.len() {
        return false;
    }

    let mut i = 0;
    while i + name_bytes.len() <= expr_bytes.len() {
        if &expr_bytes[i..i + name_bytes.len()] == name_bytes {
            // Check word boundaries.
            let before_ok = i == 0 || !is_ident_char(expr_bytes[i - 1]);
            let after_ok = i + name_bytes.len() == expr_bytes.len()
                || !is_ident_char(expr_bytes[i + name_bytes.len()]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Topological sort of binding names by their dependencies.
/// Returns names in resolution order (dependencies first).
/// Cycle-breaking: nodes in cycles are appended last.
///
/// `deps` maps name → [names it depends on]. So deps["verb"] = ["abs_pct"]
/// means `verb`'s expression references `abs_pct`, and `abs_pct` must be
/// resolved before `verb`.
fn topo_sort<'a>(names: &HashSet<&'a str>, deps: &HashMap<&'a str, Vec<&'a str>>) -> Vec<&'a str> {
    // in_degree[name] = number of unresolved dependencies.
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    // reverse_deps[dep] = [names that depend on dep].
    let mut reverse_deps: HashMap<&str, Vec<&str>> = HashMap::new();

    for &name in names {
        let dep_list = deps.get(name).map(|v| v.as_slice()).unwrap_or(&[]);
        let valid_deps: Vec<&str> = dep_list
            .iter()
            .filter(|d| names.contains(*d))
            .copied()
            .collect();
        in_degree.insert(name, valid_deps.len());
        for dep in valid_deps {
            reverse_deps.entry(dep).or_default().push(name);
        }
    }

    // Kahn's: start with nodes that have zero unresolved deps.
    let mut queue: Vec<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&name, _)| name)
        .collect();
    queue.sort(); // deterministic order
    let mut result = Vec::with_capacity(names.len());

    while let Some(name) = queue.pop() {
        result.push(name);
        // All nodes that depend on `name` have one fewer unresolved dep.
        if let Some(dependents) = reverse_deps.get(name) {
            for &dependent in dependents {
                if let Some(deg) = in_degree.get_mut(dependent) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push(dependent);
                        queue.sort(); // maintain deterministic order
                    }
                }
            }
        }
    }

    // Any remaining nodes are in cycles — append them.
    for &name in names {
        if !result.contains(&name) {
            result.push(name);
        }
    }

    result
}
