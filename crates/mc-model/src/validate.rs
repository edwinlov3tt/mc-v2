//! Stage 2: `ParsedModel` → `ValidatedModel`.
//!
//! Implements the nine validators in ADR-0004 Decision 6's table plus the
//! Phase 3D formula-parse step. Every validator runs in a single pass;
//! errors accumulate into a `Vec` so the caller sees every problem at
//! once rather than first-error-then-stop.
//!
//! Stages (in order):
//!
//!  0. **Phase 3D formula parse.** For every rule whose `body:` was
//!     authored as `ParsedRuleBodyForm::Formula(s)`, parse the formula
//!     string into a [`ParsedRuleBody`] tree via [`crate::formula::parse`].
//!     Failures emit `ParseError::Formula*` with codes MC1003–MC1006.
//!     Success populates `ValidatedModel.rules[i].body` with the flat
//!     tree; downstream stages see no `ParsedRuleBodyForm` wrapper.
//!  1. `check_model_format_version` — must be `1` (Decision 6).
//!  2. `check_duplicate_names` — dim names, element names per dim, measure
//!     names, rule names.
//!  3. `check_missing_dimensions` — every hierarchy/measure-weight/rule
//!     reference resolves.
//!  4. `check_invalid_hierarchy_edges` — every edge endpoint is a member
//!     of the parent dim.
//!  5. `check_hierarchy_cycles` — DFS over each hierarchy.
//!  6. `check_rules_reference_known_measures` — every `Ref` in a rule body
//!     is a declared measure.
//!  7. `check_derived_measures_have_rules` / `check_input_measures_have_no_rules`
//!     — measure role ↔ rule target consistency.
//!  8. `check_rule_cycles` — DFS over rule-target → dep-measure graph.
//!  9. `check_aggregation_methods_supported` — agg method is one of the
//!     four `mc_core::AggregationRule` variants; `WeightedAverage` carries
//!     a valid `weight_measure`.
//!
//! Golden test mismatch detection (the 10th row in Decision 6) is handled
//! at the test layer (`tests/golden_acme.rs`) rather than as a stage-2
//! validator, since it requires a fully-built `Cube` to evaluate. The
//! validator surfaces *structural* problems with golden tests (unknown
//! coord names, both `expect` and `expect_within_epsilon` set, etc.).
//!
//! # Phase 3D return-type change
//!
//! Phase 3D extends the error mix: formula parse errors (MC1003–MC1006)
//! come back as [`ParseError`], semantic-validation errors (MC2xxx)
//! come back as [`ValidationError`]. Both are wrapped in the unified
//! [`Error`] enum. Callers that previously matched on
//! `Vec<ValidationError>` now `filter_map` for `Error::Validation(_)`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::error::{Error, ParseError, Span, ValidationError};
use crate::formula;
use crate::schema::{
    ParsedModel, ParsedRule, ParsedRuleBody, ParsedRuleBodyForm, ValidatedModel, ValidatedRule,
};

/// Run every Decision 6 validator + the Phase 3D formula-parse step, and
/// either return a [`ValidatedModel`] (every check passed) or the full
/// list of errors that fired.
///
/// Phase 3D return change: errors are now `Vec<Error>` (mixing
/// `Error::Parse(ParseError)` from formula syntax errors with
/// `Error::Validation(ValidationError)` from semantic checks). Callers
/// that previously took `Vec<ValidationError>` should `filter_map` for
/// `Error::Validation(v) => v` to recover the prior shape.
pub fn validate(parsed: ParsedModel) -> Result<ValidatedModel, Vec<Error>> {
    let mut errors: Vec<Error> = Vec::new();

    // Phase 3D step 0: parse every Formula(s) body into a flat
    // ParsedRuleBody, building the ValidatedRule list. On parse failure,
    // record an Error::Parse with MC1003–MC1006 and emit a placeholder
    // body so subsequent validators don't dereference a half-state.
    let validated_rules = parse_rule_formulas(&parsed.rules, &mut errors);

    let mut val_errors: Vec<ValidationError> = Vec::new();
    check_model_format_version(&parsed, &mut val_errors);
    check_metadata(&parsed, &mut val_errors);
    check_duplicate_names(&parsed, &mut val_errors);
    check_dimension_kinds(&parsed, &mut val_errors);
    check_missing_dimensions(&parsed, &mut val_errors);
    check_invalid_hierarchy_edges(&parsed, &mut val_errors);
    check_hierarchy_cycles(&parsed, &mut val_errors);
    check_aggregation_methods_supported(&parsed, &mut val_errors);
    check_rules_reference_known_measures(&parsed, &validated_rules, &mut val_errors);
    check_derived_measures_have_rules(&parsed, &mut val_errors);
    check_input_measures_have_no_rules(&parsed, &mut val_errors);
    check_rule_cycles(&parsed, &mut val_errors);
    check_golden_test_shape(&parsed, &mut val_errors);

    errors.extend(val_errors.into_iter().map(Error::Validation));

    if !errors.is_empty() {
        return Err(errors);
    }

    // Build the canonical maps used by `compile`. Safe to do here only
    // because every check above passed.
    let dim_index_by_name: BTreeMap<String, usize> = parsed
        .dimensions
        .iter()
        .enumerate()
        .map(|(i, d)| (d.name.clone(), i))
        .collect();

    let element_index_by_name: Vec<BTreeMap<String, usize>> = parsed
        .dimensions
        .iter()
        .map(|d| {
            d.elements
                .iter()
                .enumerate()
                .map(|(i, e)| (e.name.clone(), i))
                .collect::<BTreeMap<_, _>>()
        })
        .collect();

    let measure_index_by_name: BTreeMap<String, usize> = parsed
        .measures
        .iter()
        .enumerate()
        .map(|(i, m)| (m.name.clone(), i))
        .collect();

    // Find the Measure dim. The dim-kind check above already enforced
    // that exactly one Measure dim exists, so this find is total.
    let measure_dim_index = parsed
        .dimensions
        .iter()
        .position(|d| d.kind == "Measure")
        .unwrap_or(0);

    let dimension_order: Vec<usize> = (0..parsed.dimensions.len()).collect();

    Ok(ValidatedModel {
        parsed,
        rules: validated_rules,
        dimension_order,
        measure_dim_index,
        dim_index_by_name,
        element_index_by_name,
        measure_index_by_name,
    })
}

// ---------------------------------------------------------------------------
// Phase 3D step 0: formula-parse pre-step.
// ---------------------------------------------------------------------------

/// Walk every parsed rule and normalize its body to a flat
/// [`ParsedRuleBody`]. For `ParsedRuleBodyForm::Formula(s)`, call
/// [`formula::parse`]; on failure, push a `ParseError::Formula*` and
/// substitute a placeholder body so semantic validators that walk the
/// rule body still have a structurally valid tree to recurse over.
///
/// Per acceptance amendment #23, the returned [`ValidatedRule`] list is
/// the canonical representation downstream consumers see — no
/// `ParsedRuleBodyForm` reaches `compile` / `lint` / `inspect`.
fn parse_rule_formulas(rules: &[ParsedRule], errors: &mut Vec<Error>) -> Vec<ValidatedRule> {
    rules
        .iter()
        .map(|r| {
            let body = match &r.body {
                ParsedRuleBodyForm::Structured(b) => b.clone(),
                ParsedRuleBodyForm::Formula(text) => match formula::parse(text) {
                    Ok(b) => b,
                    Err(fe) => {
                        errors.push(Error::Parse(formula_error_to_parse_error(&r.name, fe)));
                        // Placeholder body so subsequent semantic
                        // validators don't have to special-case this
                        // rule. Const(0.0) is shape-valid and contains
                        // no Refs, so no spurious "unknown measure"
                        // errors fire.
                        crate::schema::ParsedRuleBody::Const(crate::schema::ParsedConstBody {
                            value: crate::schema::ParsedScalar::Float(0.0),
                        })
                    }
                },
            };
            ValidatedRule {
                name: r.name.clone(),
                target_measure: r.target_measure.clone(),
                scope: r.scope.clone(),
                description: r.description.clone(),
                body,
                declared_dependencies: r.declared_dependencies.clone(),
            }
        })
        .collect()
}

/// Convert an internal [`formula::FormulaError`] to a [`ParseError`]
/// variant. The YAML-line span is left as a zero placeholder — Phase 3D
/// does not implement YAML-side line tracking for embedded formula
/// strings; the rule name + offset within the formula text carries
/// enough context for the diagnostic message.
fn formula_error_to_parse_error(rule_name: &str, fe: formula::FormulaError) -> ParseError {
    let span = Span {
        file: None::<PathBuf>,
        line: 0,
        column: 0,
    };
    let rule_name = rule_name.to_string();
    match fe.code {
        "MC1003" => ParseError::FormulaUnbalancedParen {
            span,
            rule_name,
            offset: fe.offset,
            message: fe.message,
        },
        "MC1005" => ParseError::FormulaExpectedExpression {
            span,
            rule_name,
            offset: fe.offset,
            message: fe.message,
        },
        "MC1006" => ParseError::FormulaInvalidNumber {
            span,
            rule_name,
            offset: fe.offset,
            message: fe.message,
        },
        // MC1004 is the catch-all (incl. unknown function calls per
        // acceptance amendment #25). Any unrecognized code is treated
        // as MC1004 for safety; in practice formula::parse only emits
        // MC1003-MC1006.
        _ => ParseError::FormulaUnexpectedToken {
            span,
            rule_name,
            offset: fe.offset,
            message: fe.message,
        },
    }
}

// ---------------------------------------------------------------------------
// 1. Model format version
// ---------------------------------------------------------------------------

fn check_model_format_version(parsed: &ParsedModel, errors: &mut Vec<ValidationError>) {
    // Per ADR-0004 Decision 6 + the model_format_version risk row: ship v1
    // only; integer (not semver). Anything other than 1 is rejected.
    if parsed.model_format_version != 1 {
        errors.push(ValidationError::Schema {
            message: format!(
                "model_format_version must be 1 (got {})",
                parsed.model_format_version
            ),
        });
    }
}

fn check_metadata(parsed: &ParsedModel, errors: &mut Vec<ValidationError>) {
    if parsed.metadata.name.trim().is_empty() {
        errors.push(ValidationError::Schema {
            message: "metadata.name must not be empty".into(),
        });
    }
}

// ---------------------------------------------------------------------------
// 2. Duplicate names
// ---------------------------------------------------------------------------

fn check_duplicate_names(parsed: &ParsedModel, errors: &mut Vec<ValidationError>) {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for d in &parsed.dimensions {
        *seen.entry(d.name.clone()).or_default() += 1;
    }
    for (name, count) in &seen {
        if *count > 1 {
            errors.push(ValidationError::DuplicateName {
                kind: "dimension".into(),
                name: name.clone(),
            });
        }
    }

    for d in &parsed.dimensions {
        let mut elem_seen: BTreeMap<String, usize> = BTreeMap::new();
        for e in &d.elements {
            *elem_seen.entry(e.name.clone()).or_default() += 1;
        }
        for (name, count) in &elem_seen {
            if *count > 1 {
                errors.push(ValidationError::DuplicateName {
                    kind: format!("element in dim {:?}", d.name),
                    name: name.clone(),
                });
            }
        }
    }

    let mut measure_seen: BTreeMap<String, usize> = BTreeMap::new();
    for m in &parsed.measures {
        *measure_seen.entry(m.name.clone()).or_default() += 1;
    }
    for (name, count) in &measure_seen {
        if *count > 1 {
            errors.push(ValidationError::DuplicateName {
                kind: "measure".into(),
                name: name.clone(),
            });
        }
    }

    let mut rule_seen: BTreeMap<String, usize> = BTreeMap::new();
    for r in &parsed.rules {
        *rule_seen.entry(r.name.clone()).or_default() += 1;
    }
    for (name, count) in &rule_seen {
        if *count > 1 {
            errors.push(ValidationError::DuplicateName {
                kind: "rule".into(),
                name: name.clone(),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Dimension-kind sanity (one Measure dim; kinds are valid)
// ---------------------------------------------------------------------------

fn check_dimension_kinds(parsed: &ParsedModel, errors: &mut Vec<ValidationError>) {
    let mut measure_count = 0;
    for d in &parsed.dimensions {
        match d.kind.as_str() {
            "Standard" | "Measure" | "Scenario" | "Version" => {}
            other => errors.push(ValidationError::Schema {
                message: format!("dim {:?}: unknown kind {:?}", d.name, other),
            }),
        }
        if d.kind == "Measure" {
            measure_count += 1;
        }
    }
    if measure_count == 0 {
        errors.push(ValidationError::Schema {
            message: "model has no Measure dimension".into(),
        });
    } else if measure_count > 1 {
        errors.push(ValidationError::Schema {
            message: format!("model has {measure_count} Measure dimensions; exactly one allowed"),
        });
    }
}

// ---------------------------------------------------------------------------
// 3. Missing dimensions (referenced but not declared)
// ---------------------------------------------------------------------------

fn check_missing_dimensions(parsed: &ParsedModel, errors: &mut Vec<ValidationError>) {
    let known: BTreeSet<&str> = parsed.dimensions.iter().map(|d| d.name.as_str()).collect();
    for h in &parsed.hierarchies {
        if !known.contains(h.dimension.as_str()) {
            errors.push(ValidationError::MissingDimension {
                name: h.dimension.clone(),
                referenced_by: format!("hierarchy {:?}", h.name),
            });
        }
    }
    for g in &parsed.golden_tests {
        for dim_name in g.coord.keys() {
            if !known.contains(dim_name.as_str()) {
                errors.push(ValidationError::MissingDimension {
                    name: dim_name.clone(),
                    referenced_by: format!("golden_test {:?}", g.name),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 4. Invalid hierarchy edges (endpoint not in dim)
// ---------------------------------------------------------------------------

fn check_invalid_hierarchy_edges(parsed: &ParsedModel, errors: &mut Vec<ValidationError>) {
    let dim_by_name: BTreeMap<&str, &Vec<crate::schema::ParsedElement>> = parsed
        .dimensions
        .iter()
        .map(|d| (d.name.as_str(), &d.elements))
        .collect();
    for h in &parsed.hierarchies {
        let Some(elements) = dim_by_name.get(h.dimension.as_str()) else {
            // Caught by check_missing_dimensions; skip here.
            continue;
        };
        let element_names: BTreeSet<&str> = elements.iter().map(|e| e.name.as_str()).collect();
        for edge in &h.edges {
            if !element_names.contains(edge.parent.as_str()) {
                errors.push(ValidationError::InvalidHierarchyEdge {
                    dim: h.dimension.clone(),
                    element: edge.parent.clone(),
                });
            }
            if !element_names.contains(edge.child.as_str()) {
                errors.push(ValidationError::InvalidHierarchyEdge {
                    dim: h.dimension.clone(),
                    element: edge.child.clone(),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 5. Hierarchy cycles
// ---------------------------------------------------------------------------

fn check_hierarchy_cycles(parsed: &ParsedModel, errors: &mut Vec<ValidationError>) {
    for h in &parsed.hierarchies {
        // children_of: parent → list of children. Cycle means starting at
        // some node and following children leads back to that node.
        let mut children_of: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for edge in &h.edges {
            children_of
                .entry(edge.parent.as_str())
                .or_default()
                .push(edge.child.as_str());
        }
        // DFS from each node.
        let mut visited: BTreeSet<&str> = BTreeSet::new();
        let mut on_stack: BTreeSet<&str> = BTreeSet::new();
        for &start in children_of.keys() {
            if visited.contains(start) {
                continue;
            }
            let mut path: Vec<&str> = Vec::new();
            if dfs_cycle(start, &children_of, &mut visited, &mut on_stack, &mut path) {
                errors.push(ValidationError::HierarchyCycle {
                    dim: h.dimension.clone(),
                    path: path.join(" -> "),
                });
                break; // one cycle per hierarchy is enough
            }
        }
    }
}

fn dfs_cycle<'a>(
    node: &'a str,
    children_of: &BTreeMap<&'a str, Vec<&'a str>>,
    visited: &mut BTreeSet<&'a str>,
    on_stack: &mut BTreeSet<&'a str>,
    path: &mut Vec<&'a str>,
) -> bool {
    visited.insert(node);
    on_stack.insert(node);
    path.push(node);
    if let Some(children) = children_of.get(node) {
        for &child in children {
            if on_stack.contains(child) {
                path.push(child);
                return true;
            }
            if !visited.contains(child) && dfs_cycle(child, children_of, visited, on_stack, path) {
                return true;
            }
        }
    }
    on_stack.remove(node);
    path.pop();
    false
}

// ---------------------------------------------------------------------------
// 6. Rules referencing unknown measures
// ---------------------------------------------------------------------------

fn check_rules_reference_known_measures(
    parsed: &ParsedModel,
    validated_rules: &[ValidatedRule],
    errors: &mut Vec<ValidationError>,
) {
    let known_measures: BTreeSet<&str> = parsed.measures.iter().map(|m| m.name.as_str()).collect();
    // Walk the validated rules (post-formula-parse) so the body refs we
    // collect represent the actual semantic shape — not the
    // `ParsedRuleBodyForm` wrapper. Length matches `parsed.rules`.
    for r in validated_rules {
        // Binary-op arity check — every Add/Sub/Mul/Div/IfNull body needs
        // exactly 2 args. Surfaced as `Schema` rather than as its own
        // Decision-6 row because it's a structural malformation that the
        // serde Vec deserialization can't reject on its own.
        check_binop_arity(&r.body, &r.name, errors);
        if !known_measures.contains(r.target_measure.as_str()) {
            errors.push(ValidationError::RuleReferencesUnknownMeasure {
                rule_name: r.name.clone(),
                measure_name: r.target_measure.clone(),
            });
        }
        for dep in &r.declared_dependencies {
            if !known_measures.contains(dep.as_str()) {
                errors.push(ValidationError::RuleReferencesUnknownMeasure {
                    rule_name: r.name.clone(),
                    measure_name: dep.clone(),
                });
            }
        }
        let mut body_refs: BTreeSet<String> = BTreeSet::new();
        collect_body_refs(&r.body, &mut body_refs);
        for ref_name in &body_refs {
            if !known_measures.contains(ref_name.as_str()) {
                errors.push(ValidationError::RuleReferencesUnknownMeasure {
                    rule_name: r.name.clone(),
                    measure_name: ref_name.clone(),
                });
            }
        }
        // Body refs must be a subset of declared_dependencies (matches the
        // kernel's `RuleSet::add` declared-dep-superset check, surfaced here
        // with model context). Per spec §3.10 doctrine_no_silent_dependency_miss.
        let declared: BTreeSet<String> = r.declared_dependencies.iter().cloned().collect();
        for ref_name in &body_refs {
            if !declared.contains(ref_name) {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "rule {:?}: body references measure {:?} but it is not in declared_dependencies",
                        r.name, ref_name
                    ),
                });
            }
        }
    }
}

fn check_binop_arity(body: &ParsedRuleBody, rule_name: &str, errors: &mut Vec<ValidationError>) {
    let (op_name, args) = match body {
        ParsedRuleBody::Const(_) | ParsedRuleBody::Ref(_) => return,
        ParsedRuleBody::Add(b) => ("add", &b.add),
        ParsedRuleBody::Sub(b) => ("sub", &b.sub),
        ParsedRuleBody::Mul(b) => ("mul", &b.mul),
        ParsedRuleBody::Div(b) => ("div", &b.div),
        ParsedRuleBody::IfNull(b) => ("if_null", &b.if_null),
    };
    if args.len() != 2 {
        errors.push(ValidationError::Schema {
            message: format!(
                "rule {rule_name:?}: {op_name} expects exactly 2 args, got {}",
                args.len()
            ),
        });
    }
    for a in args {
        check_binop_arity(a, rule_name, errors);
    }
}

fn collect_body_refs(body: &ParsedRuleBody, out: &mut BTreeSet<String>) {
    match body {
        ParsedRuleBody::Const(_) => {}
        ParsedRuleBody::Ref(r) => {
            out.insert(r.measure.clone());
        }
        ParsedRuleBody::Add(b) => walk_args(&b.add, out),
        ParsedRuleBody::Sub(b) => walk_args(&b.sub, out),
        ParsedRuleBody::Mul(b) => walk_args(&b.mul, out),
        ParsedRuleBody::Div(b) => walk_args(&b.div, out),
        ParsedRuleBody::IfNull(b) => walk_args(&b.if_null, out),
    }
}

fn walk_args(args: &[ParsedRuleBody], out: &mut BTreeSet<String>) {
    for a in args {
        collect_body_refs(a, out);
    }
}

// ---------------------------------------------------------------------------
// 7. Derived measures must have rules; input measures must NOT have rules.
// ---------------------------------------------------------------------------

fn check_derived_measures_have_rules(parsed: &ParsedModel, errors: &mut Vec<ValidationError>) {
    let rule_targets: BTreeSet<&str> = parsed
        .rules
        .iter()
        .map(|r| r.target_measure.as_str())
        .collect();
    for m in &parsed.measures {
        if m.role == "Derived" && !rule_targets.contains(m.name.as_str()) {
            errors.push(ValidationError::DerivedMeasureWithoutRule {
                measure_name: m.name.clone(),
            });
        }
    }
}

fn check_input_measures_have_no_rules(parsed: &ParsedModel, errors: &mut Vec<ValidationError>) {
    let role_by_name: BTreeMap<&str, &str> = parsed
        .measures
        .iter()
        .map(|m| (m.name.as_str(), m.role.as_str()))
        .collect();
    for r in &parsed.rules {
        if let Some(&role) = role_by_name.get(r.target_measure.as_str()) {
            if role == "Input" {
                errors.push(ValidationError::InputMeasureHasRule {
                    measure_name: r.target_measure.clone(),
                    rule_name: r.name.clone(),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 8. Rule dependency cycles
// ---------------------------------------------------------------------------

fn check_rule_cycles(parsed: &ParsedModel, errors: &mut Vec<ValidationError>) {
    // Edge target_measure → each measure in declared_dependencies for any
    // rule whose body actually reads them.
    let mut deps_of: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for r in &parsed.rules {
        let entry = deps_of.entry(r.target_measure.as_str()).or_default();
        for dep in &r.declared_dependencies {
            entry.push(dep.as_str());
        }
    }
    let mut visited: BTreeSet<&str> = BTreeSet::new();
    let mut on_stack: BTreeSet<&str> = BTreeSet::new();
    for &start in deps_of.keys() {
        if visited.contains(start) {
            continue;
        }
        let mut path: Vec<&str> = Vec::new();
        if dfs_cycle(start, &deps_of, &mut visited, &mut on_stack, &mut path) {
            errors.push(ValidationError::RuleCycle {
                path: path.join(" -> "),
            });
            return; // one cycle per model is enough
        }
    }
}

// ---------------------------------------------------------------------------
// 9. Aggregation methods supported by mc_core::AggregationRule
// ---------------------------------------------------------------------------

fn check_aggregation_methods_supported(parsed: &ParsedModel, errors: &mut Vec<ValidationError>) {
    let known_measures: BTreeSet<&str> = parsed.measures.iter().map(|m| m.name.as_str()).collect();
    for m in &parsed.measures {
        match m.aggregation.as_str() {
            "Sum" | "Min" | "Max" => {
                if m.weight_measure.is_some() {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "measure {:?}: aggregation {:?} does not take a weight_measure",
                            m.name, m.aggregation
                        ),
                    });
                }
            }
            "WeightedAverage" => match &m.weight_measure {
                // Per ADR-0005 amendment #4 (Phase 3B): promoted from lint
                // (formerly MC3008) to a typed validator error (MC2011).
                // Blocks `mc_model::load()`; the kernel cannot meaningfully
                // consolidate a WeightedAverage measure without a weight.
                None => errors.push(ValidationError::WeightedAverageMissingWeight {
                    measure_name: m.name.clone(),
                }),
                Some(w) => {
                    if !known_measures.contains(w.as_str()) {
                        errors.push(ValidationError::RuleReferencesUnknownMeasure {
                            rule_name: format!("measure {:?} weight_measure", m.name),
                            measure_name: w.clone(),
                        });
                    }
                }
            },
            other => errors.push(ValidationError::UnsupportedAggregation {
                measure_name: m.name.clone(),
                method: other.to_string(),
            }),
        }
        match m.role.as_str() {
            "Input" | "Derived" => {}
            other => errors.push(ValidationError::Schema {
                message: format!("measure {:?}: unknown role {:?}", m.name, other),
            }),
        }
        match m.data_type.as_str() {
            "F64" | "I64" | "Bool" => {
                if m.category_domain.is_some() {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "measure {:?}: category_domain only valid when data_type is Category",
                            m.name
                        ),
                    });
                }
            }
            "Category" => {
                if m.category_domain.is_none() {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "measure {:?}: data_type Category requires category_domain",
                            m.name
                        ),
                    });
                }
            }
            other => errors.push(ValidationError::Schema {
                message: format!("measure {:?}: unknown data_type {:?}", m.name, other),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Golden test shape (pre-cube checks: structural, not value)
// ---------------------------------------------------------------------------

fn check_golden_test_shape(parsed: &ParsedModel, errors: &mut Vec<ValidationError>) {
    for g in &parsed.golden_tests {
        match (g.expect, g.expect_within_epsilon.is_some()) {
            (None, false) => errors.push(ValidationError::Schema {
                message: format!(
                    "golden_test {:?}: must set either `expect` or `expect_within_epsilon`",
                    g.name
                ),
            }),
            (Some(_), true) => errors.push(ValidationError::Schema {
                message: format!(
                    "golden_test {:?}: cannot set both `expect` and `expect_within_epsilon`",
                    g.name
                ),
            }),
            _ => {}
        }
    }
}
