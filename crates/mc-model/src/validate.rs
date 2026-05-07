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
    // Phase 3E+: cross-coordinate and time-dimension checks
    check_time_dimension_requirements(&parsed, &validated_rules, &mut val_errors);
    check_actual_ref_requirements(&parsed, &validated_rules, &mut val_errors);
    check_cross_coord_nesting(&validated_rules, &mut errors);
    // Phase 3G: reference-data block validation
    check_reference_data_blocks(&parsed, &mut val_errors);
    // Phase 3H: fitted-model + calibration-map validation
    check_fitted_model_blocks(&parsed, &mut val_errors);
    // Phase 3F.1: time metadata + anchor validation
    check_time_metadata(&parsed, &mut val_errors);
    check_anchor_requirements(&parsed, &validated_rules, &mut val_errors);
    // Phase 3I item 1: validate is_element(Dim, "Element") references.
    // MC1023 (unknown dim) + MC1022 (unknown element).
    // Phase 3I item 5: validate avg/min/max/wavg_over dim + measure refs.
    check_is_element_and_over_refs(&parsed, &validated_rules, &mut val_errors);
    // Phase 3I item 4: validate predict() arity against fitted-model
    // coefficient counts. Requires fitted_models block. MC2057
    // (handoff said MC2053 but that was already shipped by Phase 3H —
    // see check_predict_arity doc comment for the audit-trail note).
    check_predict_arity(&parsed, &validated_rules, &mut val_errors);
    // Phase 3J item 1 + Amendment §1: type-context validation for the
    // newly first-class `Str` value. Reject Str in arithmetic (MC1026),
    // type mismatch in == / != (MC1027), Str in numeric ordering
    // (MC1028), and Str-typed rule body (MC2058). All emitted as
    // `ValidationError::Schema` with the code suffix in the message
    // (matches the MC2057 / MC2058+ pattern).
    check_str_type_context(&parsed, &validated_rules, &mut val_errors);
    // Phase 3J item 3: parameters: block validation. Checks name
    // collisions with measures (MC2060) and dim element names
    // (MC2061), and unresolved `param(name)` references (MC2062).
    // Decision 6: only `f64` values; non-numeric values caught at
    // YAML parse time (serde rejects).
    check_parameters_block(&parsed, &validated_rules, &mut val_errors);
    // Phase 3J item 4: Indicator measure validation. MC2063 rejects
    // Indicator measures declaring a rule body (or `inputs:`); MC2064
    // rejects Indicator measures missing the required `dimension:` /
    // `element:` fields. Element existence within the named dim is
    // checked via the existing MC1022 / MC1023 paths after compile
    // synthesizes the equivalent is_element rule body.
    check_indicator_measures(&parsed, &mut val_errors);
    // Phase 3J item 5: Scope variants. MC1029 rejects unknown scope
    // names at parse-time; MC2069 (Amendment §4) rejects non-AllLeaves
    // scopes when `time_anchor` is not configured on the Time dim.
    check_scope_variants(&parsed, &mut val_errors);
    // Phase 3J item 6: scenario_ref + actual_ref(measure, fallback).
    // MC2065 rejects scenario_ref against unknown scenario element
    // names; MC2066 catches actual_ref fallback type mismatches (best-
    // effort static type analysis).
    check_scenario_ref_and_fallback(&parsed, &validated_rules, &mut val_errors);
    // Phase 3J item 7 + Amendment §11: extrapolate_last_value used at
    // a scope other than `FutureLeaves` requires the rule to set
    // `allow_past_extrapolation: true`. MC2067 fires otherwise.
    check_extrapolate_scope(&validated_rules, &mut val_errors);

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
                allow_past_extrapolation: r.allow_past_extrapolation,
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
        "MC1007" => ParseError::FormulaUnknownFunction {
            span,
            rule_name,
            offset: fe.offset,
            message: fe.message,
        },
        "MC1008" => ParseError::FormulaWrongArgCount {
            span,
            rule_name,
            offset: fe.offset,
            message: fe.message,
        },
        "MC1009" => ParseError::FormulaActualRefNonIdentifier {
            span,
            rule_name,
            offset: fe.offset,
            message: fe.message,
        },
        "MC1013" => ParseError::FormulaCrossCoordNesting {
            span,
            rule_name,
            offset: fe.offset,
            message: fe.message,
        },
        "MC1024" => ParseError::FormulaStringLiteralMisplaced {
            span,
            rule_name,
            offset: fe.offset,
            message: fe.message,
        },
        // MC1004 is the catch-all for unexpected tokens.
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
            "Standard" | "Measure" | "Scenario" | "Version" | "Time" => {}
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
    // Dimension names are valid as identifiers in lookup/benchmark key
    // expressions (they resolve to the current element name at eval time).
    // Don't fire MC2005 for these.
    let known_dimensions: BTreeSet<&str> =
        parsed.dimensions.iter().map(|d| d.name.as_str()).collect();
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
            // Dimension names used as lookup/benchmark keys are not measure refs.
            if known_dimensions.contains(ref_name.as_str()) {
                continue;
            }
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
            // Dimension names are not measure dependencies.
            if known_dimensions.contains(ref_name.as_str()) {
                continue;
            }
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
    let (op_name, args): (&str, Option<&[ParsedRuleBody]>) = match body {
        ParsedRuleBody::Const(_)
        | ParsedRuleBody::Ref(_)
        | ParsedRuleBody::PeriodIndex(_)
        | ParsedRuleBody::AnchorIndex(_)
        | ParsedRuleBody::IsPast(_)
        | ParsedRuleBody::IsCurrent(_)
        | ParsedRuleBody::IsFuture(_)
        | ParsedRuleBody::PeriodsSinceAnchor(_)
        | ParsedRuleBody::PeriodsToEnd(_) => return,
        ParsedRuleBody::Add(b) => ("add", Some(&b.add[..])),
        ParsedRuleBody::Sub(b) => ("sub", Some(&b.sub[..])),
        ParsedRuleBody::Mul(b) => ("mul", Some(&b.mul[..])),
        ParsedRuleBody::Div(b) => ("div", Some(&b.div[..])),
        ParsedRuleBody::IfNull(b) => ("if_null", Some(&b.if_null[..])),
        // Phase 3E+ variants: recurse into children
        ParsedRuleBody::Gt(b)
        | ParsedRuleBody::Lt(b)
        | ParsedRuleBody::Gte(b)
        | ParsedRuleBody::Lte(b)
        | ParsedRuleBody::Eq(b)
        | ParsedRuleBody::Neq(b)
        | ParsedRuleBody::And(b)
        | ParsedRuleBody::Or(b) => {
            check_binop_arity(&b.left, rule_name, errors);
            check_binop_arity(&b.right, rule_name, errors);
            return;
        }
        ParsedRuleBody::Not(b) | ParsedRuleBody::Abs(b) => {
            check_binop_arity(&b.operand, rule_name, errors);
            return;
        }
        ParsedRuleBody::If(b) => {
            check_binop_arity(&b.condition, rule_name, errors);
            check_binop_arity(&b.then_branch, rule_name, errors);
            check_binop_arity(&b.else_branch, rule_name, errors);
            return;
        }
        ParsedRuleBody::Min(b) | ParsedRuleBody::Max(b) | ParsedRuleBody::Coalesce(b) => {
            for a in &b.args {
                check_binop_arity(a, rule_name, errors);
            }
            return;
        }
        ParsedRuleBody::SafeDiv(b) => {
            check_binop_arity(&b.numerator, rule_name, errors);
            check_binop_arity(&b.denominator, rule_name, errors);
            check_binop_arity(&b.default, rule_name, errors);
            return;
        }
        ParsedRuleBody::Clamp(b) => {
            check_binop_arity(&b.value, rule_name, errors);
            check_binop_arity(&b.lo, rule_name, errors);
            check_binop_arity(&b.hi, rule_name, errors);
            return;
        }
        ParsedRuleBody::ActualRef(_)
        | ParsedRuleBody::ScenarioRef(_)
        | ParsedRuleBody::ExtrapolateLastValue(_)
        | ParsedRuleBody::Prev(_)
        | ParsedRuleBody::Cumulative(_)
        | ParsedRuleBody::SumOver(_) => return,
        ParsedRuleBody::Lag(b) => {
            check_binop_arity(&b.periods, rule_name, errors);
            return;
        }
        ParsedRuleBody::RollingAvg(b) => {
            check_binop_arity(&b.window, rule_name, errors);
            return;
        }
        ParsedRuleBody::Benchmark(b) => {
            check_binop_arity(&b.key_expr, rule_name, errors);
            return;
        }
        ParsedRuleBody::Lookup(b) => {
            for k in &b.key_exprs {
                check_binop_arity(k, rule_name, errors);
            }
            return;
        }
        ParsedRuleBody::Bucket(b) => {
            check_binop_arity(&b.value, rule_name, errors);
            return;
        }
        // Phase 3H
        ParsedRuleBody::Predict(b) => {
            for f in &b.features {
                check_binop_arity(f, rule_name, errors);
            }
            return;
        }
        ParsedRuleBody::Calibrate(b) => {
            check_binop_arity(&b.value, rule_name, errors);
            return;
        }
        ParsedRuleBody::Exp(b) => {
            check_binop_arity(&b.operand, rule_name, errors);
            return;
        }
        ParsedRuleBody::NormCdf(b) => {
            check_binop_arity(&b.x, rule_name, errors);
            check_binop_arity(&b.mu, rule_name, errors);
            check_binop_arity(&b.sigma, rule_name, errors);
            return;
        }
        // Phase 3I
        ParsedRuleBody::Pow(b) => {
            check_binop_arity(&b.base, rule_name, errors);
            check_binop_arity(&b.exponent, rule_name, errors);
            return;
        }
        ParsedRuleBody::Sqrt(b)
        | ParsedRuleBody::Ln(b)
        | ParsedRuleBody::Log10(b)
        | ParsedRuleBody::Round(b)
        | ParsedRuleBody::Floor(b)
        | ParsedRuleBody::Ceil(b) => {
            check_binop_arity(&b.operand, rule_name, errors);
            return;
        }
        ParsedRuleBody::Mod(b) => {
            check_binop_arity(&b.dividend, rule_name, errors);
            check_binop_arity(&b.divisor, rule_name, errors);
            return;
        }
        ParsedRuleBody::NormInv(b) => {
            check_binop_arity(&b.p, rule_name, errors);
            check_binop_arity(&b.mu, rule_name, errors);
            check_binop_arity(&b.sigma, rule_name, errors);
            return;
        }
        ParsedRuleBody::IsElement(_)
        | ParsedRuleBody::AvgOver(_)
        | ParsedRuleBody::MinOver(_)
        | ParsedRuleBody::MaxOver(_)
        | ParsedRuleBody::WAvgOver(_) => return,
        // Phase 3J: string literal and current_element are atomic — no
        // sub-expressions to descend into.
        ParsedRuleBody::StrLiteral(_)
        | ParsedRuleBody::CurrentElement(_)
        | ParsedRuleBody::ParamRef(_) => return,
    };
    if let Some(args) = args {
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
}

fn collect_body_refs(body: &ParsedRuleBody, out: &mut BTreeSet<String>) {
    match body {
        ParsedRuleBody::Const(_)
        | ParsedRuleBody::PeriodIndex(_)
        | ParsedRuleBody::AnchorIndex(_)
        | ParsedRuleBody::IsPast(_)
        | ParsedRuleBody::IsCurrent(_)
        | ParsedRuleBody::IsFuture(_)
        | ParsedRuleBody::PeriodsSinceAnchor(_)
        | ParsedRuleBody::PeriodsToEnd(_) => {}
        ParsedRuleBody::Ref(r) => {
            out.insert(r.measure.clone());
        }
        ParsedRuleBody::Add(b) => walk_args(&b.add, out),
        ParsedRuleBody::Sub(b) => walk_args(&b.sub, out),
        ParsedRuleBody::Mul(b) => walk_args(&b.mul, out),
        ParsedRuleBody::Div(b) => walk_args(&b.div, out),
        ParsedRuleBody::IfNull(b) => walk_args(&b.if_null, out),
        // Phase 3E: comparisons + logical
        ParsedRuleBody::Gt(b)
        | ParsedRuleBody::Lt(b)
        | ParsedRuleBody::Gte(b)
        | ParsedRuleBody::Lte(b)
        | ParsedRuleBody::Eq(b)
        | ParsedRuleBody::Neq(b)
        | ParsedRuleBody::And(b)
        | ParsedRuleBody::Or(b) => {
            collect_body_refs(&b.left, out);
            collect_body_refs(&b.right, out);
        }
        ParsedRuleBody::Not(b) | ParsedRuleBody::Abs(b) => collect_body_refs(&b.operand, out),
        ParsedRuleBody::If(b) => {
            collect_body_refs(&b.condition, out);
            collect_body_refs(&b.then_branch, out);
            collect_body_refs(&b.else_branch, out);
        }
        ParsedRuleBody::Min(b) | ParsedRuleBody::Max(b) | ParsedRuleBody::Coalesce(b) => {
            for a in &b.args {
                collect_body_refs(a, out);
            }
        }
        ParsedRuleBody::SafeDiv(b) => {
            collect_body_refs(&b.numerator, out);
            collect_body_refs(&b.denominator, out);
            collect_body_refs(&b.default, out);
        }
        ParsedRuleBody::Clamp(b) => {
            collect_body_refs(&b.value, out);
            collect_body_refs(&b.lo, out);
            collect_body_refs(&b.hi, out);
        }
        // Cross-coordinate: the measure name IS a dependency
        ParsedRuleBody::ActualRef(b) => {
            out.insert(b.measure.clone());
            // Phase 3J item 6: descend into the optional fallback
            // expression so its measure refs participate in the
            // declared-deps check.
            if let Some(fb) = &b.fallback {
                collect_body_refs(fb, out);
            }
        }
        // Phase 3J item 6: scenario_ref's measure participates in deps.
        ParsedRuleBody::ScenarioRef(b) => {
            out.insert(b.measure.clone());
        }
        // Phase 3J item 7: extrapolate_last_value's measure dep.
        ParsedRuleBody::ExtrapolateLastValue(b) => {
            out.insert(b.measure.clone());
        }
        ParsedRuleBody::Prev(b) | ParsedRuleBody::Cumulative(b) => {
            out.insert(b.measure.clone());
        }
        ParsedRuleBody::Lag(b) => {
            out.insert(b.measure.clone());
            collect_body_refs(&b.periods, out);
        }
        ParsedRuleBody::RollingAvg(b) => {
            out.insert(b.measure.clone());
            collect_body_refs(&b.window, out);
        }
        ParsedRuleBody::Benchmark(b) => collect_body_refs(&b.key_expr, out),
        ParsedRuleBody::Lookup(b) => {
            for k in &b.key_exprs {
                collect_body_refs(k, out);
            }
        }
        ParsedRuleBody::Bucket(b) => collect_body_refs(&b.value, out),
        ParsedRuleBody::SumOver(b) => {
            out.insert(b.measure.clone());
        }
        // Phase 3H
        ParsedRuleBody::Predict(b) => {
            for f in &b.features {
                collect_body_refs(f, out);
            }
        }
        ParsedRuleBody::Calibrate(b) => collect_body_refs(&b.value, out),
        ParsedRuleBody::Exp(b) => collect_body_refs(&b.operand, out),
        ParsedRuleBody::NormCdf(b) => {
            collect_body_refs(&b.x, out);
            collect_body_refs(&b.mu, out);
            collect_body_refs(&b.sigma, out);
        }
        // Phase 3I
        ParsedRuleBody::Pow(b) => {
            collect_body_refs(&b.base, out);
            collect_body_refs(&b.exponent, out);
        }
        ParsedRuleBody::Sqrt(b)
        | ParsedRuleBody::Ln(b)
        | ParsedRuleBody::Log10(b)
        | ParsedRuleBody::Round(b)
        | ParsedRuleBody::Floor(b)
        | ParsedRuleBody::Ceil(b) => collect_body_refs(&b.operand, out),
        ParsedRuleBody::Mod(b) => {
            collect_body_refs(&b.dividend, out);
            collect_body_refs(&b.divisor, out);
        }
        ParsedRuleBody::NormInv(b) => {
            collect_body_refs(&b.p, out);
            collect_body_refs(&b.mu, out);
            collect_body_refs(&b.sigma, out);
        }
        ParsedRuleBody::IsElement(_) => {} // no measure dep
        ParsedRuleBody::AvgOver(b) | ParsedRuleBody::MinOver(b) | ParsedRuleBody::MaxOver(b) => {
            out.insert(b.measure.clone());
        }
        ParsedRuleBody::WAvgOver(b) => {
            out.insert(b.value_measure.clone());
            out.insert(b.weight_measure.clone());
        }
        // Phase 3J: string literal and current_element introduce no
        // measure dependency. current_element resolves via the dim axis.
        ParsedRuleBody::StrLiteral(_)
        | ParsedRuleBody::CurrentElement(_)
        | ParsedRuleBody::ParamRef(_) => {}
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
        // Phase 3J item 4: Indicator measures DON'T need a user rule —
        // their body is synthesized at compile time from `dimension:`
        // and `element:` fields per ADR-0016 Amendment §6. Only Derived
        // measures require an explicit rule.
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
            // Phase 3J item 4: also reject user-supplied rules
            // targeting an `Indicator` measure — Indicator bodies are
            // synthesized per Amendment §6, so an explicit user rule
            // creates an ambiguous double-binding.
            if role == "Input" || role == "Indicator" {
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
            "Input" | "Derived" | "Indicator" => {}
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

// ---------------------------------------------------------------------------
// Phase 3E/3F: Time dimension + actual_ref + cross-coord nesting checks
// ---------------------------------------------------------------------------

/// Check if any rule body uses a time-series function (prev, lag, cumulative,
/// rolling_avg, period_index).
fn uses_time_series(body: &ParsedRuleBody) -> bool {
    match body {
        ParsedRuleBody::Prev(_)
        | ParsedRuleBody::Lag(_)
        | ParsedRuleBody::Cumulative(_)
        | ParsedRuleBody::RollingAvg(_)
        | ParsedRuleBody::PeriodIndex(_)
        | ParsedRuleBody::AnchorIndex(_)
        | ParsedRuleBody::IsPast(_)
        | ParsedRuleBody::IsCurrent(_)
        | ParsedRuleBody::IsFuture(_)
        | ParsedRuleBody::PeriodsSinceAnchor(_)
        | ParsedRuleBody::PeriodsToEnd(_) => true,
        ParsedRuleBody::Const(_) | ParsedRuleBody::Ref(_) => false,
        ParsedRuleBody::Add(b) => b.add.iter().any(uses_time_series),
        ParsedRuleBody::Sub(b) => b.sub.iter().any(uses_time_series),
        ParsedRuleBody::Mul(b) => b.mul.iter().any(uses_time_series),
        ParsedRuleBody::Div(b) => b.div.iter().any(uses_time_series),
        ParsedRuleBody::IfNull(b) => b.if_null.iter().any(uses_time_series),
        ParsedRuleBody::Gt(b)
        | ParsedRuleBody::Lt(b)
        | ParsedRuleBody::Gte(b)
        | ParsedRuleBody::Lte(b)
        | ParsedRuleBody::Eq(b)
        | ParsedRuleBody::Neq(b)
        | ParsedRuleBody::And(b)
        | ParsedRuleBody::Or(b) => uses_time_series(&b.left) || uses_time_series(&b.right),
        ParsedRuleBody::Not(b) | ParsedRuleBody::Abs(b) => uses_time_series(&b.operand),
        ParsedRuleBody::If(b) => {
            uses_time_series(&b.condition)
                || uses_time_series(&b.then_branch)
                || uses_time_series(&b.else_branch)
        }
        ParsedRuleBody::Min(b) | ParsedRuleBody::Max(b) | ParsedRuleBody::Coalesce(b) => {
            b.args.iter().any(uses_time_series)
        }
        ParsedRuleBody::SafeDiv(b) => {
            uses_time_series(&b.numerator)
                || uses_time_series(&b.denominator)
                || uses_time_series(&b.default)
        }
        ParsedRuleBody::Clamp(b) => {
            uses_time_series(&b.value) || uses_time_series(&b.lo) || uses_time_series(&b.hi)
        }
        // Phase 3J item 6: actual_ref's fallback may use time-series.
        ParsedRuleBody::ActualRef(b) => match &b.fallback {
            None => false,
            Some(fb) => uses_time_series(fb),
        },
        ParsedRuleBody::ScenarioRef(_)
        | ParsedRuleBody::ExtrapolateLastValue(_)
        | ParsedRuleBody::SumOver(_) => false,
        ParsedRuleBody::Benchmark(b) => uses_time_series(&b.key_expr),
        ParsedRuleBody::Lookup(b) => b.key_exprs.iter().any(|k| uses_time_series(k)),
        ParsedRuleBody::Bucket(b) => uses_time_series(&b.value),
        // Phase 3H
        ParsedRuleBody::Predict(b) => b.features.iter().any(|f| uses_time_series(f)),
        ParsedRuleBody::Calibrate(b) => uses_time_series(&b.value),
        ParsedRuleBody::Exp(b) => uses_time_series(&b.operand),
        ParsedRuleBody::NormCdf(b) => {
            uses_time_series(&b.x) || uses_time_series(&b.mu) || uses_time_series(&b.sigma)
        }
        // Phase 3I: math primitives are local — recurse into operands.
        ParsedRuleBody::Pow(b) => uses_time_series(&b.base) || uses_time_series(&b.exponent),
        ParsedRuleBody::Sqrt(b)
        | ParsedRuleBody::Ln(b)
        | ParsedRuleBody::Log10(b)
        | ParsedRuleBody::Round(b)
        | ParsedRuleBody::Floor(b)
        | ParsedRuleBody::Ceil(b) => uses_time_series(&b.operand),
        ParsedRuleBody::Mod(b) => uses_time_series(&b.dividend) || uses_time_series(&b.divisor),
        ParsedRuleBody::NormInv(b) => {
            uses_time_series(&b.p) || uses_time_series(&b.mu) || uses_time_series(&b.sigma)
        }
        // Phase 3I: is_element + *_over family are non-time-series cross-coord scans.
        ParsedRuleBody::IsElement(_)
        | ParsedRuleBody::AvgOver(_)
        | ParsedRuleBody::MinOver(_)
        | ParsedRuleBody::MaxOver(_)
        | ParsedRuleBody::WAvgOver(_) => false,
        // Phase 3J: string-domain expressions are not time-series.
        ParsedRuleBody::StrLiteral(_)
        | ParsedRuleBody::CurrentElement(_)
        | ParsedRuleBody::ParamRef(_) => false,
    }
}

/// Check if any rule body uses actual_ref.
fn uses_actual_ref(body: &ParsedRuleBody) -> bool {
    match body {
        ParsedRuleBody::ActualRef(_) => true,
        // Phase 3J item 6: scenario_ref does NOT require the Scenario
        // dim's `actuals_element` to be configured — it targets a
        // user-named scenario element directly. So it doesn't
        // participate in the MC2037 check.
        ParsedRuleBody::ScenarioRef(_) | ParsedRuleBody::ExtrapolateLastValue(_) => false,
        ParsedRuleBody::Const(_)
        | ParsedRuleBody::Ref(_)
        | ParsedRuleBody::PeriodIndex(_)
        | ParsedRuleBody::AnchorIndex(_)
        | ParsedRuleBody::IsPast(_)
        | ParsedRuleBody::IsCurrent(_)
        | ParsedRuleBody::IsFuture(_)
        | ParsedRuleBody::PeriodsSinceAnchor(_)
        | ParsedRuleBody::PeriodsToEnd(_) => false,
        ParsedRuleBody::Add(b) => b.add.iter().any(uses_actual_ref),
        ParsedRuleBody::Sub(b) => b.sub.iter().any(uses_actual_ref),
        ParsedRuleBody::Mul(b) => b.mul.iter().any(uses_actual_ref),
        ParsedRuleBody::Div(b) => b.div.iter().any(uses_actual_ref),
        ParsedRuleBody::IfNull(b) => b.if_null.iter().any(uses_actual_ref),
        ParsedRuleBody::Gt(b)
        | ParsedRuleBody::Lt(b)
        | ParsedRuleBody::Gte(b)
        | ParsedRuleBody::Lte(b)
        | ParsedRuleBody::Eq(b)
        | ParsedRuleBody::Neq(b)
        | ParsedRuleBody::And(b)
        | ParsedRuleBody::Or(b) => uses_actual_ref(&b.left) || uses_actual_ref(&b.right),
        ParsedRuleBody::Not(b) | ParsedRuleBody::Abs(b) => uses_actual_ref(&b.operand),
        ParsedRuleBody::If(b) => {
            uses_actual_ref(&b.condition)
                || uses_actual_ref(&b.then_branch)
                || uses_actual_ref(&b.else_branch)
        }
        ParsedRuleBody::Min(b) | ParsedRuleBody::Max(b) | ParsedRuleBody::Coalesce(b) => {
            b.args.iter().any(uses_actual_ref)
        }
        ParsedRuleBody::SafeDiv(b) => {
            uses_actual_ref(&b.numerator)
                || uses_actual_ref(&b.denominator)
                || uses_actual_ref(&b.default)
        }
        ParsedRuleBody::Clamp(b) => {
            uses_actual_ref(&b.value) || uses_actual_ref(&b.lo) || uses_actual_ref(&b.hi)
        }
        ParsedRuleBody::Prev(_)
        | ParsedRuleBody::Lag(_)
        | ParsedRuleBody::Cumulative(_)
        | ParsedRuleBody::RollingAvg(_)
        | ParsedRuleBody::SumOver(_) => false,
        ParsedRuleBody::Benchmark(b) => uses_actual_ref(&b.key_expr),
        ParsedRuleBody::Lookup(b) => b.key_exprs.iter().any(|k| uses_actual_ref(k)),
        ParsedRuleBody::Bucket(b) => uses_actual_ref(&b.value),
        // Phase 3H
        ParsedRuleBody::Predict(b) => b.features.iter().any(|f| uses_actual_ref(f)),
        ParsedRuleBody::Calibrate(b) => uses_actual_ref(&b.value),
        ParsedRuleBody::Exp(b) => uses_actual_ref(&b.operand),
        ParsedRuleBody::NormCdf(b) => {
            uses_actual_ref(&b.x) || uses_actual_ref(&b.mu) || uses_actual_ref(&b.sigma)
        }
        // Phase 3I
        ParsedRuleBody::Pow(b) => uses_actual_ref(&b.base) || uses_actual_ref(&b.exponent),
        ParsedRuleBody::Sqrt(b)
        | ParsedRuleBody::Ln(b)
        | ParsedRuleBody::Log10(b)
        | ParsedRuleBody::Round(b)
        | ParsedRuleBody::Floor(b)
        | ParsedRuleBody::Ceil(b) => uses_actual_ref(&b.operand),
        ParsedRuleBody::Mod(b) => uses_actual_ref(&b.dividend) || uses_actual_ref(&b.divisor),
        ParsedRuleBody::NormInv(b) => {
            uses_actual_ref(&b.p) || uses_actual_ref(&b.mu) || uses_actual_ref(&b.sigma)
        }
        ParsedRuleBody::IsElement(_)
        | ParsedRuleBody::AvgOver(_)
        | ParsedRuleBody::MinOver(_)
        | ParsedRuleBody::MaxOver(_)
        | ParsedRuleBody::WAvgOver(_) => false,
        // Phase 3J: string-domain expressions never reference actuals.
        ParsedRuleBody::StrLiteral(_)
        | ParsedRuleBody::CurrentElement(_)
        | ParsedRuleBody::ParamRef(_) => false,
    }
}

/// MC2035: no Time dim but time-series functions used.
/// MC2036: multiple Time dims.
fn check_time_dimension_requirements(
    parsed: &ParsedModel,
    validated_rules: &[ValidatedRule],
    errors: &mut Vec<ValidationError>,
) {
    let time_dim_count = parsed
        .dimensions
        .iter()
        .filter(|d| d.kind == "Time")
        .count();

    // MC2036: multiple Time dims
    if time_dim_count > 1 {
        errors.push(ValidationError::Schema {
            message: format!(
                "model has {} Time dimensions; exactly one allowed (MC2036)",
                time_dim_count
            ),
        });
    }

    // MC2035: time-series function used but no Time dim
    if time_dim_count == 0 {
        let has_time_series = validated_rules.iter().any(|r| uses_time_series(&r.body));
        if has_time_series {
            errors.push(ValidationError::Schema {
                message: "time-series function (prev/lag/cumulative/rolling_avg/period_index) \
                          used but no dimension with kind: \"Time\" declared (MC2035)"
                    .into(),
            });
        }
    }
}

/// MC2037: actual_ref used but no actuals_element on Scenario dim.
fn check_actual_ref_requirements(
    parsed: &ParsedModel,
    validated_rules: &[ValidatedRule],
    errors: &mut Vec<ValidationError>,
) {
    let has_actual_ref = validated_rules.iter().any(|r| uses_actual_ref(&r.body));
    if !has_actual_ref {
        return;
    }

    // Find the Scenario-kind dimension and check for actuals_element
    let scenario_dims: Vec<&crate::schema::ParsedDimension> = parsed
        .dimensions
        .iter()
        .filter(|d| d.kind == "Scenario")
        .collect();

    if scenario_dims.is_empty() {
        errors.push(ValidationError::Schema {
            message: "actual_ref used but no Scenario-kind dimension declared".into(),
        });
        return;
    }

    let has_actuals_element = scenario_dims.iter().any(|d| d.actuals_element.is_some());

    if !has_actuals_element {
        errors.push(ValidationError::Schema {
            message: "actual_ref used but no actuals_element field declared on Scenario \
                      dimension (MC2037)"
                .into(),
        });
    }
}

/// MC1013: cross-coordinate function nesting.
/// Rejects formulas where cross-coord functions appear nested inside each other.
fn check_cross_coord_nesting(validated_rules: &[ValidatedRule], errors: &mut Vec<Error>) {
    use crate::error::Span;
    for r in validated_rules {
        if let Some(msg) = find_cross_coord_nesting(&r.body) {
            let span = Span {
                file: None::<PathBuf>,
                line: 0,
                column: 0,
            };
            errors.push(Error::Parse(ParseError::FormulaCrossCoordNesting {
                span,
                rule_name: r.name.clone(),
                offset: 0,
                message: msg,
            }));
        }
    }
}

/// Walk a rule body looking for nested cross-coordinate functions.
/// Returns a diagnostic message if nesting is found.
fn find_cross_coord_nesting(body: &ParsedRuleBody) -> Option<String> {
    match body {
        // Cross-coord functions: check if their arguments contain another cross-coord
        // Phase 3J item 6 + Amendment §3: actual_ref's `fallback`
        // expression IS a relaxation point — cross-coord functions
        // (scenario_ref, prev, lag, lookup, etc.) are explicitly
        // allowed inside the fallback. Other cross-coord nesting
        // patterns (e.g., `prev(actual_ref(...))`) remain rejected by
        // MC1013 elsewhere in this walk.
        ParsedRuleBody::ActualRef(_) => None,
        // Phase 3J item 6: scenario_ref takes a bare measure name and a
        // string literal — no nestable expression slots.
        ParsedRuleBody::ScenarioRef(_) | ParsedRuleBody::ExtrapolateLastValue(_) => None,
        ParsedRuleBody::Prev(_) | ParsedRuleBody::Cumulative(_) => None, // leaf measure ref
        ParsedRuleBody::Lag(b) => {
            if crate::formula::contains_cross_coord(&b.periods) {
                Some("cross-coordinate function nested inside lag() (MC1013)".into())
            } else {
                find_cross_coord_nesting(&b.periods)
            }
        }
        ParsedRuleBody::RollingAvg(b) => {
            if crate::formula::contains_cross_coord(&b.window) {
                Some("cross-coordinate function nested inside rolling_avg() (MC1013)".into())
            } else {
                find_cross_coord_nesting(&b.window)
            }
        }
        ParsedRuleBody::SumOver(_) => None, // leaf
        // Non-cross-coord nodes: recurse into children
        ParsedRuleBody::Const(_)
        | ParsedRuleBody::Ref(_)
        | ParsedRuleBody::PeriodIndex(_)
        | ParsedRuleBody::AnchorIndex(_)
        | ParsedRuleBody::IsPast(_)
        | ParsedRuleBody::IsCurrent(_)
        | ParsedRuleBody::IsFuture(_)
        | ParsedRuleBody::PeriodsSinceAnchor(_)
        | ParsedRuleBody::PeriodsToEnd(_) => None,
        ParsedRuleBody::Add(b) => walk_nesting_args(&b.add),
        ParsedRuleBody::Sub(b) => walk_nesting_args(&b.sub),
        ParsedRuleBody::Mul(b) => walk_nesting_args(&b.mul),
        ParsedRuleBody::Div(b) => walk_nesting_args(&b.div),
        ParsedRuleBody::IfNull(b) => walk_nesting_args(&b.if_null),
        ParsedRuleBody::Gt(b)
        | ParsedRuleBody::Lt(b)
        | ParsedRuleBody::Gte(b)
        | ParsedRuleBody::Lte(b)
        | ParsedRuleBody::Eq(b)
        | ParsedRuleBody::Neq(b)
        | ParsedRuleBody::And(b)
        | ParsedRuleBody::Or(b) => {
            find_cross_coord_nesting(&b.left).or_else(|| find_cross_coord_nesting(&b.right))
        }
        ParsedRuleBody::Not(b) | ParsedRuleBody::Abs(b) => find_cross_coord_nesting(&b.operand),
        ParsedRuleBody::If(b) => find_cross_coord_nesting(&b.condition)
            .or_else(|| find_cross_coord_nesting(&b.then_branch))
            .or_else(|| find_cross_coord_nesting(&b.else_branch)),
        ParsedRuleBody::Min(b) | ParsedRuleBody::Max(b) | ParsedRuleBody::Coalesce(b) => {
            b.args.iter().find_map(find_cross_coord_nesting)
        }
        ParsedRuleBody::SafeDiv(b) => find_cross_coord_nesting(&b.numerator)
            .or_else(|| find_cross_coord_nesting(&b.denominator))
            .or_else(|| find_cross_coord_nesting(&b.default)),
        ParsedRuleBody::Clamp(b) => find_cross_coord_nesting(&b.value)
            .or_else(|| find_cross_coord_nesting(&b.lo))
            .or_else(|| find_cross_coord_nesting(&b.hi)),
        ParsedRuleBody::Benchmark(b) => find_cross_coord_nesting(&b.key_expr),
        ParsedRuleBody::Lookup(b) => b.key_exprs.iter().find_map(|k| find_cross_coord_nesting(k)),
        ParsedRuleBody::Bucket(b) => find_cross_coord_nesting(&b.value),
        // Phase 3H
        ParsedRuleBody::Predict(b) => b.features.iter().find_map(|f| find_cross_coord_nesting(f)),
        ParsedRuleBody::Calibrate(b) => find_cross_coord_nesting(&b.value),
        ParsedRuleBody::Exp(b) => find_cross_coord_nesting(&b.operand),
        ParsedRuleBody::NormCdf(b) => find_cross_coord_nesting(&b.x)
            .or_else(|| find_cross_coord_nesting(&b.mu))
            .or_else(|| find_cross_coord_nesting(&b.sigma)),
        // Phase 3I
        ParsedRuleBody::Pow(b) => {
            find_cross_coord_nesting(&b.base).or_else(|| find_cross_coord_nesting(&b.exponent))
        }
        ParsedRuleBody::Sqrt(b)
        | ParsedRuleBody::Ln(b)
        | ParsedRuleBody::Log10(b)
        | ParsedRuleBody::Round(b)
        | ParsedRuleBody::Floor(b)
        | ParsedRuleBody::Ceil(b) => find_cross_coord_nesting(&b.operand),
        ParsedRuleBody::Mod(b) => {
            find_cross_coord_nesting(&b.dividend).or_else(|| find_cross_coord_nesting(&b.divisor))
        }
        ParsedRuleBody::NormInv(b) => find_cross_coord_nesting(&b.p)
            .or_else(|| find_cross_coord_nesting(&b.mu))
            .or_else(|| find_cross_coord_nesting(&b.sigma)),
        // Phase 3I: avg_over/min_over/max_over/wavg_over are leaf cross-coord
        // operators (their args are bare measure/dim names, not expressions).
        ParsedRuleBody::AvgOver(_)
        | ParsedRuleBody::MinOver(_)
        | ParsedRuleBody::MaxOver(_)
        | ParsedRuleBody::WAvgOver(_)
        | ParsedRuleBody::IsElement(_) => None,
        // Phase 3J: string-domain primitives are local — no cross-coord.
        ParsedRuleBody::StrLiteral(_)
        | ParsedRuleBody::CurrentElement(_)
        | ParsedRuleBody::ParamRef(_) => None,
    }
}

fn walk_nesting_args(args: &[ParsedRuleBody]) -> Option<String> {
    args.iter().find_map(find_cross_coord_nesting)
}

// ---------------------------------------------------------------------------
// Phase 3G: Reference-data block validation
// ---------------------------------------------------------------------------

fn check_reference_data_blocks(parsed: &ParsedModel, errors: &mut Vec<ValidationError>) {
    let dim_names: BTreeSet<&str> = parsed.dimensions.iter().map(|d| d.name.as_str()).collect();
    let elem_by_dim: BTreeMap<&str, BTreeSet<&str>> = parsed
        .dimensions
        .iter()
        .map(|d| {
            let elems: BTreeSet<&str> = d.elements.iter().map(|e| e.name.as_str()).collect();
            (d.name.as_str(), elems)
        })
        .collect();

    // Collect all reference-data names for uniqueness check (MC2037)
    let mut all_ref_names: BTreeMap<&str, &str> = BTreeMap::new(); // name → block type

    for b in &parsed.benchmarks {
        if let Some(existing) = all_ref_names.get(b.name.as_str()) {
            errors.push(ValidationError::Schema {
                message: format!(
                    "duplicate reference-data name {:?} (already in {existing} block) (MC2037)",
                    b.name
                ),
            });
        } else {
            all_ref_names.insert(&b.name, "benchmarks");
        }

        // MC2038: key_dimension must reference a declared dimension
        if !dim_names.contains(b.key_dimension.as_str()) {
            errors.push(ValidationError::Schema {
                message: format!(
                    "benchmark {:?}: key_dimension {:?} is not a declared dimension (MC2038)",
                    b.name, b.key_dimension
                ),
            });
        } else {
            // MC2039: value keys must be valid elements in key dimension
            if let Some(elements) = elem_by_dim.get(b.key_dimension.as_str()) {
                for key in b.values.keys() {
                    if !elements.contains(key.as_str()) {
                        errors.push(ValidationError::Schema {
                            message: format!(
                                "benchmark {:?}: value key {:?} is not an element of dimension {:?} (MC2039)",
                                b.name, key, b.key_dimension
                            ),
                        });
                    }
                }
            }
        }
    }

    for lt in &parsed.lookup_tables {
        if let Some(existing) = all_ref_names.get(lt.name.as_str()) {
            errors.push(ValidationError::Schema {
                message: format!(
                    "duplicate reference-data name {:?} (already in {existing} block) (MC2037)",
                    lt.name
                ),
            });
        } else {
            all_ref_names.insert(&lt.name, "lookup_tables");
        }

        // Phase 3I item 3: enforce exactly-one-of (key_dimension XOR
        // key_dimensions). MC2050 fires if both are set.
        match (&lt.key_dimension, &lt.key_dimensions) {
            (Some(_), Some(_)) => {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "lookup_table {:?}: cannot set both key_dimension and key_dimensions; pick one (MC2050)",
                        lt.name
                    ),
                });
                continue;
            }
            (None, None) => {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "lookup_table {:?}: must set either key_dimension (single-key) or key_dimensions (multi-key)",
                        lt.name
                    ),
                });
                continue;
            }
            _ => {}
        }

        let key_dims: Vec<&str> = lt.key_dims();

        // MC2038: each key dimension must be declared
        let mut all_dims_known = true;
        for d in &key_dims {
            if !dim_names.contains(d) {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "lookup_table {:?}: key dimension {:?} is not a declared dimension (MC2038)",
                        lt.name, d
                    ),
                });
                all_dims_known = false;
            }
        }
        if !all_dims_known {
            continue;
        }

        if key_dims.len() == 1 {
            // Single-key: each value key must be an element of the dim.
            let dim = key_dims[0];
            if let Some(elements) = elem_by_dim.get(dim) {
                for key in lt.values.keys() {
                    if !elements.contains(key.as_str()) {
                        errors.push(ValidationError::Schema {
                            message: format!(
                                "lookup_table {:?}: value key {:?} is not an element of dimension {:?} (MC2039)",
                                lt.name, key, dim
                            ),
                        });
                    }
                }
            }
        } else {
            // Phase 3I item 3 W2/W3: multi-key. Each value key is
            // pipe-joined element names in the declared key_dimensions
            // order. Validate (a) no element-name contains `|` (MC2051),
            // (b) the joined-key arity matches len(key_dimensions)
            // (MC2052), (c) each component is a valid element of the
            // corresponding dim (MC2039).
            for (dim_idx, d) in key_dims.iter().enumerate() {
                if let Some(elements) = elem_by_dim.get(*d) {
                    for elem in elements {
                        if elem.contains('|') {
                            errors.push(ValidationError::Schema {
                                message: format!(
                                    "lookup_table {:?}: dimension {:?} has element name {:?} containing the multi-key separator '|' (MC2051)",
                                    lt.name, d, elem
                                ),
                            });
                            // Don't return — keep accumulating diagnostics.
                        }
                    }
                    let _ = dim_idx; // unused; reserved for future positional checks
                }
            }
            for key in lt.values.keys() {
                let parts: Vec<&str> = key.split('|').collect();
                if parts.len() != key_dims.len() {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "lookup_table {:?}: key {:?} has {} parts but key_dimensions has {} (MC2052)",
                            lt.name,
                            key,
                            parts.len(),
                            key_dims.len()
                        ),
                    });
                    continue;
                }
                for (i, part) in parts.iter().enumerate() {
                    let dim = key_dims[i];
                    if let Some(elements) = elem_by_dim.get(dim) {
                        if !elements.contains(part) {
                            errors.push(ValidationError::Schema {
                                message: format!(
                                    "lookup_table {:?}: key {:?} part {:?} is not an element of dimension {:?} (MC2039)",
                                    lt.name, key, part, dim
                                ),
                            });
                        }
                    }
                }
            }
        }
    }

    for st in &parsed.status_thresholds {
        if let Some(existing) = all_ref_names.get(st.name.as_str()) {
            errors.push(ValidationError::Schema {
                message: format!(
                    "duplicate reference-data name {:?} (already in {existing} block) (MC2037)",
                    st.name
                ),
            });
        } else {
            all_ref_names.insert(&st.name, "status_thresholds");
        }

        // MC2040: at least 2 bands
        if st.bands.len() < 2 {
            errors.push(ValidationError::Schema {
                message: format!(
                    "status_threshold {:?}: must have at least 2 bands, got {} (MC2040)",
                    st.name,
                    st.bands.len()
                ),
            });
            continue;
        }

        // MC2042: last band must have no max (unbounded)
        if let Some(last) = st.bands.last() {
            if last.max.is_some() {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "status_threshold {:?}: last band {:?} must have no max (unbounded above) (MC2042)",
                        st.name, last.label
                    ),
                });
            }
        }

        // MC2041: bands must have ascending max values
        let mut prev_max: Option<f64> = None;
        for (i, band) in st.bands.iter().enumerate() {
            if i == st.bands.len() - 1 {
                break; // last band has no max
            }
            match band.max {
                None => {
                    if i < st.bands.len() - 1 {
                        errors.push(ValidationError::Schema {
                            message: format!(
                                "status_threshold {:?}: non-last band {:?} must have a max value (MC2041)",
                                st.name, band.label
                            ),
                        });
                    }
                }
                Some(max) => {
                    if let Some(pm) = prev_max {
                        if max <= pm {
                            errors.push(ValidationError::Schema {
                                message: format!(
                                    "status_threshold {:?}: band {:?} max ({}) must be greater than previous band max ({}) (MC2041)",
                                    st.name, band.label, max, pm
                                ),
                            });
                        }
                    }
                    prev_max = Some(max);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 3F.1: Time metadata validation (MC2043-MC2048) + anchor (MC1017)
// ---------------------------------------------------------------------------

/// Validates ISO 8601 date format: YYYY-MM-DD (exactly 10 chars).
fn is_valid_iso_date(s: &str) -> bool {
    if s.len() != 10 {
        return false;
    }
    let bytes = s.as_bytes();
    // Pattern: DDDD-DD-DD where D is digit
    bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[..4].iter().all(|b| b.is_ascii_digit())
        && bytes[5..7].iter().all(|b| b.is_ascii_digit())
        && bytes[8..10].iter().all(|b| b.is_ascii_digit())
}

/// Returns true if the string looks like a timestamp (contains 'T'),
/// indicating it's an ISO 8601 date-time rather than just a date.
fn is_timestamp_shaped(s: &str) -> bool {
    s.contains('T')
}

/// Convert a YYYY-MM-DD date string to days since a reference epoch using
/// Hinnant's algorithm (civil_from_days inverse). Returns None if the string
/// is not a valid 10-char ISO date.
fn date_to_days(s: &str) -> Option<i64> {
    if s.len() < 10 {
        return None;
    }
    let y: i64 = s[..4].parse().ok()?;
    let m: u32 = s[5..7].parse().ok()?;
    let d: u32 = s[8..10].parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Hinnant's algorithm: days_from_civil
    let y_adj = if m <= 2 { y - 1 } else { y };
    let era = if y_adj >= 0 {
        y_adj / 400
    } else {
        (y_adj - 399) / 400
    };
    let yoe = (y_adj - era * 400) as u32; // year of era [0, 399]
    let m_adj = if m > 2 { m - 3 } else { m + 9 }; // [0, 11]
    let doy = (153 * m_adj + 2) / 5 + d - 1; // day of year [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // day of era [0, 146096]
    Some(era * 146097 + doe as i64 - 719468)
}

/// MC2043-MC2048: validate time metadata on Time-kind dimensions.
fn check_time_metadata(parsed: &ParsedModel, errors: &mut Vec<ValidationError>) {
    for dim in &parsed.dimensions {
        if dim.kind != "Time" {
            continue;
        }

        // MC2048: time_anchor names a non-existent element
        if let Some(anchor) = &dim.time_anchor {
            let elem_names: BTreeSet<&str> = dim.elements.iter().map(|e| e.name.as_str()).collect();
            if !elem_names.contains(anchor.as_str()) {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "Time dimension {:?}: time_anchor {:?} is not a declared element (MC2048)",
                        dim.name, anchor
                    ),
                });
            }
        }

        // Validate granularity legal values
        if let Some(g) = &dim.granularity {
            match g.as_str() {
                "day" | "week" | "month" | "quarter" | "year" => {}
                other => {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "Time dimension {:?}: unknown granularity {:?} \
                             (expected day/week/month/quarter/year)",
                            dim.name, other
                        ),
                    });
                }
            }
        }

        // Validate period_start/period_end_exclusive on elements
        let mut period_intervals: Vec<(&str, &str, &str)> = Vec::new(); // (elem_name, start, end)

        for elem in &dim.elements {
            // MC2043: period_start must be valid ISO 8601 YYYY-MM-DD
            if let Some(start) = &elem.period_start {
                if !is_valid_iso_date(start) {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "Time element {:?}: period_start {:?} is not valid ISO 8601 \
                             YYYY-MM-DD (MC2043)",
                            elem.name, start
                        ),
                    });
                }
                // MC2044: timestamps must be UTC (end with Z)
                if is_timestamp_shaped(start) && !start.ends_with('Z') {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "Time element {:?}: period_start {:?} is a timestamp but does not \
                             end with 'Z' (must be UTC) (MC2044)",
                            elem.name, start
                        ),
                    });
                }
            }

            if let Some(end) = &elem.period_end_exclusive {
                if !is_valid_iso_date(end) {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "Time element {:?}: period_end_exclusive {:?} is not valid ISO 8601 \
                             YYYY-MM-DD (MC2043)",
                            elem.name, end
                        ),
                    });
                }
                // MC2044: timestamps must be UTC (end with Z)
                if is_timestamp_shaped(end) && !end.ends_with('Z') {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "Time element {:?}: period_end_exclusive {:?} is a timestamp but \
                             does not end with 'Z' (must be UTC) (MC2044)",
                            elem.name, end
                        ),
                    });
                }
            }

            // Collect intervals for gap/overlap checks
            if let (Some(start), Some(end)) = (&elem.period_start, &elem.period_end_exclusive) {
                if is_valid_iso_date(start) && is_valid_iso_date(end) {
                    period_intervals.push((&elem.name, start, end));
                }
            }
        }

        // MC2045: granularity mismatch — check that each element's interval
        // matches the declared granularity (approximate ranges for calendar
        // months/quarters/years).
        if let Some(g) = &dim.granularity {
            let (min_days, max_days): (i64, i64) = match g.as_str() {
                "day" => (1, 1),
                "week" => (7, 7),
                "month" => (28, 31),
                "quarter" => (89, 92),
                "year" => (365, 366),
                _ => (0, i64::MAX), // unknown granularity; already flagged above
            };
            if min_days > 0 && max_days < i64::MAX {
                for &(elem_name, start, end) in &period_intervals {
                    if let (Some(start_days), Some(end_days)) =
                        (date_to_days(start), date_to_days(end))
                    {
                        let interval = end_days - start_days;
                        if interval < min_days || interval > max_days {
                            errors.push(ValidationError::Schema {
                                message: format!(
                                    "Time element {:?}: interval is {} days but granularity \
                                     {:?} expects {}-{} days (MC2045)",
                                    elem_name, interval, g, min_days, max_days
                                ),
                            });
                        }
                    }
                }
            }
        }

        // MC2046/MC2047: check for gaps and overlaps between consecutive elements
        if period_intervals.len() >= 2 {
            for i in 0..period_intervals.len() - 1 {
                let (name_a, _start_a, end_a) = period_intervals[i];
                let (name_b, start_b, _end_b) = period_intervals[i + 1];
                match end_a.cmp(start_b) {
                    std::cmp::Ordering::Less => {
                        errors.push(ValidationError::Schema {
                            message: format!(
                                "Time dimension {:?}: gap between elements {:?} (ends {}) \
                                 and {:?} (starts {}) (MC2046)",
                                dim.name, name_a, end_a, name_b, start_b
                            ),
                        });
                    }
                    std::cmp::Ordering::Greater => {
                        errors.push(ValidationError::Schema {
                            message: format!(
                                "Time dimension {:?}: overlap between elements {:?} (ends {}) \
                                 and {:?} (starts {}) (MC2047)",
                                dim.name, name_a, end_a, name_b, start_b
                            ),
                        });
                    }
                    std::cmp::Ordering::Equal => {} // contiguous — correct
                }
            }
        }
    }
}

/// Check if any rule body uses an anchor function.
fn uses_anchor_function(body: &ParsedRuleBody) -> bool {
    match body {
        ParsedRuleBody::AnchorIndex(_)
        | ParsedRuleBody::IsPast(_)
        | ParsedRuleBody::IsCurrent(_)
        | ParsedRuleBody::IsFuture(_)
        | ParsedRuleBody::PeriodsSinceAnchor(_)
        | ParsedRuleBody::PeriodsToEnd(_) => true,
        ParsedRuleBody::Const(_) | ParsedRuleBody::Ref(_) | ParsedRuleBody::PeriodIndex(_) => false,
        ParsedRuleBody::Add(b) => b.add.iter().any(uses_anchor_function),
        ParsedRuleBody::Sub(b) => b.sub.iter().any(uses_anchor_function),
        ParsedRuleBody::Mul(b) => b.mul.iter().any(uses_anchor_function),
        ParsedRuleBody::Div(b) => b.div.iter().any(uses_anchor_function),
        ParsedRuleBody::IfNull(b) => b.if_null.iter().any(uses_anchor_function),
        ParsedRuleBody::Gt(b)
        | ParsedRuleBody::Lt(b)
        | ParsedRuleBody::Gte(b)
        | ParsedRuleBody::Lte(b)
        | ParsedRuleBody::Eq(b)
        | ParsedRuleBody::Neq(b)
        | ParsedRuleBody::And(b)
        | ParsedRuleBody::Or(b) => uses_anchor_function(&b.left) || uses_anchor_function(&b.right),
        ParsedRuleBody::Not(b) | ParsedRuleBody::Abs(b) => uses_anchor_function(&b.operand),
        ParsedRuleBody::If(b) => {
            uses_anchor_function(&b.condition)
                || uses_anchor_function(&b.then_branch)
                || uses_anchor_function(&b.else_branch)
        }
        ParsedRuleBody::Min(b) | ParsedRuleBody::Max(b) | ParsedRuleBody::Coalesce(b) => {
            b.args.iter().any(uses_anchor_function)
        }
        ParsedRuleBody::SafeDiv(b) => {
            uses_anchor_function(&b.numerator)
                || uses_anchor_function(&b.denominator)
                || uses_anchor_function(&b.default)
        }
        ParsedRuleBody::Clamp(b) => {
            uses_anchor_function(&b.value)
                || uses_anchor_function(&b.lo)
                || uses_anchor_function(&b.hi)
        }
        ParsedRuleBody::ActualRef(_)
        | ParsedRuleBody::ScenarioRef(_)
        | ParsedRuleBody::ExtrapolateLastValue(_)
        | ParsedRuleBody::Prev(_)
        | ParsedRuleBody::Cumulative(_)
        | ParsedRuleBody::SumOver(_) => false,
        ParsedRuleBody::Lag(b) => uses_anchor_function(&b.periods),
        ParsedRuleBody::RollingAvg(b) => uses_anchor_function(&b.window),
        ParsedRuleBody::Benchmark(b) => uses_anchor_function(&b.key_expr),
        ParsedRuleBody::Lookup(b) => b.key_exprs.iter().any(|k| uses_anchor_function(k)),
        ParsedRuleBody::Bucket(b) => uses_anchor_function(&b.value),
        // Phase 3H
        ParsedRuleBody::Predict(b) => b.features.iter().any(|f| uses_anchor_function(f)),
        ParsedRuleBody::Calibrate(b) => uses_anchor_function(&b.value),
        ParsedRuleBody::Exp(b) => uses_anchor_function(&b.operand),
        ParsedRuleBody::NormCdf(b) => {
            uses_anchor_function(&b.x)
                || uses_anchor_function(&b.mu)
                || uses_anchor_function(&b.sigma)
        }
        // Phase 3I
        ParsedRuleBody::Pow(b) => {
            uses_anchor_function(&b.base) || uses_anchor_function(&b.exponent)
        }
        ParsedRuleBody::Sqrt(b)
        | ParsedRuleBody::Ln(b)
        | ParsedRuleBody::Log10(b)
        | ParsedRuleBody::Round(b)
        | ParsedRuleBody::Floor(b)
        | ParsedRuleBody::Ceil(b) => uses_anchor_function(&b.operand),
        ParsedRuleBody::Mod(b) => {
            uses_anchor_function(&b.dividend) || uses_anchor_function(&b.divisor)
        }
        ParsedRuleBody::NormInv(b) => {
            uses_anchor_function(&b.p)
                || uses_anchor_function(&b.mu)
                || uses_anchor_function(&b.sigma)
        }
        ParsedRuleBody::IsElement(_)
        | ParsedRuleBody::AvgOver(_)
        | ParsedRuleBody::MinOver(_)
        | ParsedRuleBody::MaxOver(_)
        | ParsedRuleBody::WAvgOver(_) => false,
        // Phase 3J: string-domain primitives don't reference anchors.
        ParsedRuleBody::StrLiteral(_)
        | ParsedRuleBody::CurrentElement(_)
        | ParsedRuleBody::ParamRef(_) => false,
    }
}

/// MC1017: anchor function used but no time_anchor configured.
/// MC2048 is handled in check_time_metadata.
fn check_anchor_requirements(
    parsed: &ParsedModel,
    validated_rules: &[ValidatedRule],
    errors: &mut Vec<ValidationError>,
) {
    let has_anchor_fn = validated_rules
        .iter()
        .any(|r| uses_anchor_function(&r.body));
    if !has_anchor_fn {
        return;
    }

    // Check if any Time dim has a time_anchor
    let has_time_anchor = parsed
        .dimensions
        .iter()
        .any(|d| d.kind == "Time" && d.time_anchor.is_some());

    if !has_time_anchor {
        errors.push(ValidationError::Schema {
            message: "anchor function (anchor_index/is_past/is_current/is_future/\
                      periods_since_anchor/periods_to_end) used but no time_anchor \
                      configured on Time dimension (MC1017)"
                .into(),
        });
    }
}

// ---------------------------------------------------------------------------
// Phase 3H: Fitted-model + calibration-map validation
// ---------------------------------------------------------------------------

fn check_fitted_model_blocks(parsed: &ParsedModel, errors: &mut Vec<ValidationError>) {
    // Collect names for uniqueness check (MC2053)
    let mut all_names: BTreeMap<&str, &str> = BTreeMap::new(); // name → block type

    for fm in &parsed.fitted_models {
        // MC2053: duplicate name
        if let Some(existing) = all_names.get(fm.name.as_str()) {
            errors.push(ValidationError::Schema {
                message: format!(
                    "duplicate fitted-artifact name {:?} (already in {existing} block) (MC2053)",
                    fm.name
                ),
            });
        } else {
            all_names.insert(&fm.name, "fitted_models");
        }

        // Validate method
        match fm.method.as_str() {
            "linear" | "logistic" => {}
            other => {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "fitted_model {:?}: unknown method {:?} (expected \"linear\" or \"logistic\")",
                        fm.name, other
                    ),
                });
            }
        }

        // Validate coefficients not empty
        if fm.coefficients.is_empty() {
            errors.push(ValidationError::Schema {
                message: format!("fitted_model {:?}: coefficients list is empty", fm.name),
            });
        }

        // MC2056: standardization declares a feature not in coefficients list
        if let Some(std_config) = &fm.standardization {
            let coeff_features: BTreeSet<&str> =
                fm.coefficients.iter().map(|c| c.feature.as_str()).collect();
            for param in &std_config.params {
                if !coeff_features.contains(param.feature.as_str()) {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "fitted_model {:?}: standardization param feature {:?} \
                             is not in coefficients list (MC2056)",
                            fm.name, param.feature
                        ),
                    });
                }
                // Validate std > 0
                if param.std <= 0.0 {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "fitted_model {:?}: standardization param for {:?} \
                             has std <= 0 ({})",
                            fm.name, param.feature, param.std
                        ),
                    });
                }
            }
        }

        // Phase 3H.1 (ADR-0017 Decision 4): MC2070 — `output_bound.min`
        // and `output_bound.max` both set, but `min >= max`. One-sided
        // bounds are valid; only reject the contradictory two-sided case.
        // NaN/infinite values are rejected by serde_yaml at parse time.
        if let Some(bound) = &fm.output_bound {
            if let (Some(min), Some(max)) = (bound.min, bound.max) {
                if min >= max {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "fitted_model {:?}: output_bound min ({}) must be \
                             strictly less than max ({}) (MC2070)",
                            fm.name, min, max
                        ),
                    });
                }
            }
        }
    }

    for cm in &parsed.calibration_maps {
        // MC2053: duplicate name (shared namespace with fitted_models)
        if let Some(existing) = all_names.get(cm.name.as_str()) {
            errors.push(ValidationError::Schema {
                message: format!(
                    "duplicate fitted-artifact name {:?} (already in {existing} block) (MC2053)",
                    cm.name
                ),
            });
        } else {
            all_names.insert(&cm.name, "calibration_maps");
        }

        match cm.method.as_str() {
            "pava" => {
                match &cm.points {
                    None => {
                        errors.push(ValidationError::Schema {
                            message: format!(
                                "calibration_map {:?}: method \"pava\" requires points (MC2055)",
                                cm.name
                            ),
                        });
                    }
                    Some(points) => {
                        // MC2055: < 2 points
                        if points.len() < 2 {
                            errors.push(ValidationError::Schema {
                                message: format!(
                                    "calibration_map {:?}: must have at least 2 points, got {} (MC2055)",
                                    cm.name,
                                    points.len()
                                ),
                            });
                        }
                        // MC2054: points not in ascending raw order
                        for i in 1..points.len() {
                            if points[i].raw <= points[i - 1].raw {
                                errors.push(ValidationError::Schema {
                                    message: format!(
                                        "calibration_map {:?}: points not in ascending raw order \
                                         (raw[{}]={} <= raw[{}]={}) (MC2054)",
                                        cm.name,
                                        i,
                                        points[i].raw,
                                        i - 1,
                                        points[i - 1].raw
                                    ),
                                });
                                break;
                            }
                        }
                    }
                }
            }
            "platt" => {
                if cm.platt_params.is_none() {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "calibration_map {:?}: method \"platt\" requires platt_params",
                            cm.name
                        ),
                    });
                }
            }
            other => {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "calibration_map {:?}: unknown method {:?} (expected \"pava\" or \"platt\")",
                        cm.name, other
                    ),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 3I — items 1 + 5 ref-resolution
// ---------------------------------------------------------------------------

/// Walk every rule body. For each `IsElement(dim, elem)` node:
///   - MC1023 if `dim` is not a declared dimension.
///   - MC1022 if `elem` is not a declared element of `dim`.
/// For each `*_over(measure, dim, ...)` node (avg/min/max/wavg):
///   - MC1016 if `dim` is not a declared dimension.
///   - MC1018 if any measure ref is not a declared measure.
fn check_is_element_and_over_refs(
    parsed: &ParsedModel,
    validated_rules: &[ValidatedRule],
    errors: &mut Vec<ValidationError>,
) {
    let dim_names: BTreeMap<&str, &crate::schema::ParsedDimension> = parsed
        .dimensions
        .iter()
        .map(|d| (d.name.as_str(), d))
        .collect();
    let known_measures: BTreeSet<&str> = parsed.measures.iter().map(|m| m.name.as_str()).collect();

    for r in validated_rules {
        walk_is_element_and_over(&r.body, &r.name, &dim_names, &known_measures, errors);
    }
}

fn walk_is_element_and_over(
    body: &ParsedRuleBody,
    rule_name: &str,
    dim_names: &BTreeMap<&str, &crate::schema::ParsedDimension>,
    known_measures: &BTreeSet<&str>,
    errors: &mut Vec<ValidationError>,
) {
    match body {
        ParsedRuleBody::IsElement(b) => {
            // MC1023: unknown dimension
            let dim = match dim_names.get(b.dimension.as_str()) {
                Some(d) => d,
                None => {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "rule {rule_name:?}: is_element references unknown dimension {:?} (MC1023)",
                            b.dimension
                        ),
                    });
                    return;
                }
            };
            // MC1022: unknown element in that dimension
            let known_elems: BTreeSet<&str> =
                dim.elements.iter().map(|e| e.name.as_str()).collect();
            if !known_elems.contains(b.element.as_str()) {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "rule {rule_name:?}: is_element references unknown element {:?} in dimension {:?} (MC1022)",
                        b.element, b.dimension
                    ),
                });
            }
        }
        ParsedRuleBody::AvgOver(b) | ParsedRuleBody::MinOver(b) | ParsedRuleBody::MaxOver(b) => {
            if !dim_names.contains_key(b.dimension.as_str()) {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "rule {rule_name:?}: *_over references unknown dimension {:?} (MC1016)",
                        b.dimension
                    ),
                });
            }
            if !known_measures.contains(b.measure.as_str()) {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "rule {rule_name:?}: *_over references unknown measure {:?} (MC1018)",
                        b.measure
                    ),
                });
            }
        }
        ParsedRuleBody::WAvgOver(b) => {
            if !dim_names.contains_key(b.dimension.as_str()) {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "rule {rule_name:?}: wavg_over references unknown dimension {:?} (MC1016)",
                        b.dimension
                    ),
                });
            }
            for m in [&b.value_measure, &b.weight_measure] {
                if !known_measures.contains(m.as_str()) {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "rule {rule_name:?}: wavg_over references unknown measure {:?} (MC1018)",
                            m
                        ),
                    });
                }
            }
        }
        // Recurse into composite nodes
        ParsedRuleBody::Const(_)
        | ParsedRuleBody::Ref(_)
        | ParsedRuleBody::PeriodIndex(_)
        | ParsedRuleBody::AnchorIndex(_)
        | ParsedRuleBody::IsPast(_)
        | ParsedRuleBody::IsCurrent(_)
        | ParsedRuleBody::IsFuture(_)
        | ParsedRuleBody::PeriodsSinceAnchor(_)
        | ParsedRuleBody::PeriodsToEnd(_)
        | ParsedRuleBody::ActualRef(_)
        | ParsedRuleBody::ScenarioRef(_)
        | ParsedRuleBody::ExtrapolateLastValue(_)
        | ParsedRuleBody::Prev(_)
        | ParsedRuleBody::Cumulative(_)
        | ParsedRuleBody::SumOver(_) => {}
        ParsedRuleBody::Add(b) => {
            for a in &b.add {
                walk_is_element_and_over(a, rule_name, dim_names, known_measures, errors);
            }
        }
        ParsedRuleBody::Sub(b) => {
            for a in &b.sub {
                walk_is_element_and_over(a, rule_name, dim_names, known_measures, errors);
            }
        }
        ParsedRuleBody::Mul(b) => {
            for a in &b.mul {
                walk_is_element_and_over(a, rule_name, dim_names, known_measures, errors);
            }
        }
        ParsedRuleBody::Div(b) => {
            for a in &b.div {
                walk_is_element_and_over(a, rule_name, dim_names, known_measures, errors);
            }
        }
        ParsedRuleBody::IfNull(b) => {
            for a in &b.if_null {
                walk_is_element_and_over(a, rule_name, dim_names, known_measures, errors);
            }
        }
        ParsedRuleBody::Gt(b)
        | ParsedRuleBody::Lt(b)
        | ParsedRuleBody::Gte(b)
        | ParsedRuleBody::Lte(b)
        | ParsedRuleBody::Eq(b)
        | ParsedRuleBody::Neq(b)
        | ParsedRuleBody::And(b)
        | ParsedRuleBody::Or(b) => {
            walk_is_element_and_over(&b.left, rule_name, dim_names, known_measures, errors);
            walk_is_element_and_over(&b.right, rule_name, dim_names, known_measures, errors);
        }
        ParsedRuleBody::Not(b) | ParsedRuleBody::Abs(b) => {
            walk_is_element_and_over(&b.operand, rule_name, dim_names, known_measures, errors);
        }
        ParsedRuleBody::If(b) => {
            walk_is_element_and_over(&b.condition, rule_name, dim_names, known_measures, errors);
            walk_is_element_and_over(&b.then_branch, rule_name, dim_names, known_measures, errors);
            walk_is_element_and_over(&b.else_branch, rule_name, dim_names, known_measures, errors);
        }
        ParsedRuleBody::Min(b) | ParsedRuleBody::Max(b) | ParsedRuleBody::Coalesce(b) => {
            for a in &b.args {
                walk_is_element_and_over(a, rule_name, dim_names, known_measures, errors);
            }
        }
        ParsedRuleBody::SafeDiv(b) => {
            walk_is_element_and_over(&b.numerator, rule_name, dim_names, known_measures, errors);
            walk_is_element_and_over(&b.denominator, rule_name, dim_names, known_measures, errors);
            walk_is_element_and_over(&b.default, rule_name, dim_names, known_measures, errors);
        }
        ParsedRuleBody::Clamp(b) => {
            walk_is_element_and_over(&b.value, rule_name, dim_names, known_measures, errors);
            walk_is_element_and_over(&b.lo, rule_name, dim_names, known_measures, errors);
            walk_is_element_and_over(&b.hi, rule_name, dim_names, known_measures, errors);
        }
        ParsedRuleBody::Lag(b) => {
            walk_is_element_and_over(&b.periods, rule_name, dim_names, known_measures, errors);
        }
        ParsedRuleBody::RollingAvg(b) => {
            walk_is_element_and_over(&b.window, rule_name, dim_names, known_measures, errors);
        }
        ParsedRuleBody::Benchmark(b) => {
            walk_is_element_and_over(&b.key_expr, rule_name, dim_names, known_measures, errors);
        }
        ParsedRuleBody::Lookup(b) => {
            for k in &b.key_exprs {
                walk_is_element_and_over(k, rule_name, dim_names, known_measures, errors);
            }
        }
        ParsedRuleBody::Bucket(b) => {
            walk_is_element_and_over(&b.value, rule_name, dim_names, known_measures, errors);
        }
        ParsedRuleBody::Predict(b) => {
            for f in &b.features {
                walk_is_element_and_over(f, rule_name, dim_names, known_measures, errors);
            }
        }
        ParsedRuleBody::Calibrate(b) => {
            walk_is_element_and_over(&b.value, rule_name, dim_names, known_measures, errors);
        }
        ParsedRuleBody::Exp(b) => {
            walk_is_element_and_over(&b.operand, rule_name, dim_names, known_measures, errors);
        }
        ParsedRuleBody::NormCdf(b) => {
            walk_is_element_and_over(&b.x, rule_name, dim_names, known_measures, errors);
            walk_is_element_and_over(&b.mu, rule_name, dim_names, known_measures, errors);
            walk_is_element_and_over(&b.sigma, rule_name, dim_names, known_measures, errors);
        }
        ParsedRuleBody::Pow(b) => {
            walk_is_element_and_over(&b.base, rule_name, dim_names, known_measures, errors);
            walk_is_element_and_over(&b.exponent, rule_name, dim_names, known_measures, errors);
        }
        ParsedRuleBody::Sqrt(b)
        | ParsedRuleBody::Ln(b)
        | ParsedRuleBody::Log10(b)
        | ParsedRuleBody::Round(b)
        | ParsedRuleBody::Floor(b)
        | ParsedRuleBody::Ceil(b) => {
            walk_is_element_and_over(&b.operand, rule_name, dim_names, known_measures, errors);
        }
        ParsedRuleBody::Mod(b) => {
            walk_is_element_and_over(&b.dividend, rule_name, dim_names, known_measures, errors);
            walk_is_element_and_over(&b.divisor, rule_name, dim_names, known_measures, errors);
        }
        ParsedRuleBody::NormInv(b) => {
            walk_is_element_and_over(&b.p, rule_name, dim_names, known_measures, errors);
            walk_is_element_and_over(&b.mu, rule_name, dim_names, known_measures, errors);
            walk_is_element_and_over(&b.sigma, rule_name, dim_names, known_measures, errors);
        }
        // Phase 3J: string literal + param — atomic, no descent.
        // current_element — MC1023 check on the dim name.
        ParsedRuleBody::StrLiteral(_) | ParsedRuleBody::ParamRef(_) => {}
        ParsedRuleBody::CurrentElement(b) => {
            if !dim_names.contains_key(b.current_element.as_str()) {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "rule {rule_name:?}: current_element references unknown dimension {:?} (MC1023)",
                        b.current_element
                    ),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 3I — item 4: predict() arity validation (MC2057)
// ---------------------------------------------------------------------------

/// Walk every rule body. For each `predict("model_name", f1, f2, ...)`
/// call, look up the named fitted model and compare the call's feature
/// count to the model's coefficient count. Mismatch → MC2057.
///
/// **NB**: handoff item 4 W1 specified MC2053, but MC2053 was already
/// shipped by Phase 3H for "duplicate fitted-artifact name" in
/// [`check_fitted_model_blocks`]. Per process-notes Rule 3 (CVE-style
/// code retirement) we cannot reuse MC2053; MC2057 is the next free
/// slot above the existing 2050-2056 range. Surfaced during the Phase
/// 3I self-audit (section G); see completion report §"Drift vs
/// handoff" for the audit trail.
fn check_predict_arity(
    parsed: &ParsedModel,
    validated_rules: &[ValidatedRule],
    errors: &mut Vec<ValidationError>,
) {
    use crate::schema::ParsedFittedModel;
    let models: BTreeMap<&str, &ParsedFittedModel> = parsed
        .fitted_models
        .iter()
        .map(|m| (m.name.as_str(), m))
        .collect();
    for r in validated_rules {
        walk_predict_arity(&r.body, &r.name, &models, errors);
    }
}

fn walk_predict_arity(
    body: &ParsedRuleBody,
    rule_name: &str,
    models: &BTreeMap<&str, &crate::schema::ParsedFittedModel>,
    errors: &mut Vec<ValidationError>,
) {
    match body {
        ParsedRuleBody::Predict(b) => {
            if let Some(model) = models.get(b.model_id.as_str()) {
                let expected = model.coefficients.len();
                let actual = b.features.len();
                if actual != expected {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "rule {rule_name:?}: predict({:?}, ...) has {actual} feature \
                             argument(s) but fitted_model {:?} declares {expected} coefficient(s) (MC2057)",
                            b.model_id, b.model_id
                        ),
                    });
                }
            }
            // Unknown model_id is left to the runtime (returns Null) — this
            // check is purely about arity, not model existence.
            for f in &b.features {
                walk_predict_arity(f, rule_name, models, errors);
            }
        }
        ParsedRuleBody::Const(_)
        | ParsedRuleBody::Ref(_)
        | ParsedRuleBody::PeriodIndex(_)
        | ParsedRuleBody::AnchorIndex(_)
        | ParsedRuleBody::IsPast(_)
        | ParsedRuleBody::IsCurrent(_)
        | ParsedRuleBody::IsFuture(_)
        | ParsedRuleBody::PeriodsSinceAnchor(_)
        | ParsedRuleBody::PeriodsToEnd(_)
        | ParsedRuleBody::ActualRef(_) | ParsedRuleBody::ScenarioRef(_) | ParsedRuleBody::ExtrapolateLastValue(_)
        | ParsedRuleBody::Prev(_)
        | ParsedRuleBody::Cumulative(_)
        | ParsedRuleBody::SumOver(_)
        | ParsedRuleBody::IsElement(_)
        | ParsedRuleBody::AvgOver(_)
        | ParsedRuleBody::MinOver(_)
        | ParsedRuleBody::MaxOver(_)
        | ParsedRuleBody::WAvgOver(_)
        // Phase 3J: string-domain leaves + param ref
        | ParsedRuleBody::StrLiteral(_)
        | ParsedRuleBody::CurrentElement(_)
        | ParsedRuleBody::ParamRef(_) => {}
        ParsedRuleBody::Add(b) => b
            .add
            .iter()
            .for_each(|a| walk_predict_arity(a, rule_name, models, errors)),
        ParsedRuleBody::Sub(b) => b
            .sub
            .iter()
            .for_each(|a| walk_predict_arity(a, rule_name, models, errors)),
        ParsedRuleBody::Mul(b) => b
            .mul
            .iter()
            .for_each(|a| walk_predict_arity(a, rule_name, models, errors)),
        ParsedRuleBody::Div(b) => b
            .div
            .iter()
            .for_each(|a| walk_predict_arity(a, rule_name, models, errors)),
        ParsedRuleBody::IfNull(b) => b
            .if_null
            .iter()
            .for_each(|a| walk_predict_arity(a, rule_name, models, errors)),
        ParsedRuleBody::Gt(b)
        | ParsedRuleBody::Lt(b)
        | ParsedRuleBody::Gte(b)
        | ParsedRuleBody::Lte(b)
        | ParsedRuleBody::Eq(b)
        | ParsedRuleBody::Neq(b)
        | ParsedRuleBody::And(b)
        | ParsedRuleBody::Or(b) => {
            walk_predict_arity(&b.left, rule_name, models, errors);
            walk_predict_arity(&b.right, rule_name, models, errors);
        }
        ParsedRuleBody::Not(b) | ParsedRuleBody::Abs(b) => {
            walk_predict_arity(&b.operand, rule_name, models, errors);
        }
        ParsedRuleBody::If(b) => {
            walk_predict_arity(&b.condition, rule_name, models, errors);
            walk_predict_arity(&b.then_branch, rule_name, models, errors);
            walk_predict_arity(&b.else_branch, rule_name, models, errors);
        }
        ParsedRuleBody::Min(b) | ParsedRuleBody::Max(b) | ParsedRuleBody::Coalesce(b) => {
            for a in &b.args {
                walk_predict_arity(a, rule_name, models, errors);
            }
        }
        ParsedRuleBody::SafeDiv(b) => {
            walk_predict_arity(&b.numerator, rule_name, models, errors);
            walk_predict_arity(&b.denominator, rule_name, models, errors);
            walk_predict_arity(&b.default, rule_name, models, errors);
        }
        ParsedRuleBody::Clamp(b) => {
            walk_predict_arity(&b.value, rule_name, models, errors);
            walk_predict_arity(&b.lo, rule_name, models, errors);
            walk_predict_arity(&b.hi, rule_name, models, errors);
        }
        ParsedRuleBody::Lag(b) => walk_predict_arity(&b.periods, rule_name, models, errors),
        ParsedRuleBody::RollingAvg(b) => walk_predict_arity(&b.window, rule_name, models, errors),
        ParsedRuleBody::Benchmark(b) => walk_predict_arity(&b.key_expr, rule_name, models, errors),
        ParsedRuleBody::Lookup(b) => {
            for k in &b.key_exprs {
                walk_predict_arity(k, rule_name, models, errors);
            }
        }
        ParsedRuleBody::Bucket(b) => walk_predict_arity(&b.value, rule_name, models, errors),
        ParsedRuleBody::Calibrate(b) => walk_predict_arity(&b.value, rule_name, models, errors),
        ParsedRuleBody::Exp(b) => walk_predict_arity(&b.operand, rule_name, models, errors),
        ParsedRuleBody::NormCdf(b) => {
            walk_predict_arity(&b.x, rule_name, models, errors);
            walk_predict_arity(&b.mu, rule_name, models, errors);
            walk_predict_arity(&b.sigma, rule_name, models, errors);
        }
        ParsedRuleBody::Pow(b) => {
            walk_predict_arity(&b.base, rule_name, models, errors);
            walk_predict_arity(&b.exponent, rule_name, models, errors);
        }
        ParsedRuleBody::Sqrt(b)
        | ParsedRuleBody::Ln(b)
        | ParsedRuleBody::Log10(b)
        | ParsedRuleBody::Round(b)
        | ParsedRuleBody::Floor(b)
        | ParsedRuleBody::Ceil(b) => walk_predict_arity(&b.operand, rule_name, models, errors),
        ParsedRuleBody::Mod(b) => {
            walk_predict_arity(&b.dividend, rule_name, models, errors);
            walk_predict_arity(&b.divisor, rule_name, models, errors);
        }
        ParsedRuleBody::NormInv(b) => {
            walk_predict_arity(&b.p, rule_name, models, errors);
            walk_predict_arity(&b.mu, rule_name, models, errors);
            walk_predict_arity(&b.sigma, rule_name, models, errors);
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 3J item 1 + Amendment §1: Str type-context validation
//
// `ScalarValue::Str` is first-class in expression evaluation but bounded —
// it is allowed in: string-literal positions, `current_element` returns,
// `is_element` / `scenario_ref` second-arg slots, and string-equality
// (`==` / `!=`) operands. It is rejected in: arithmetic operators (MC1026),
// numeric ordering (`<`, `>`, `<=`, `>=` — MC1028), truthy contexts like
// `if` conditions / `and` / `or` / `not` operands (MC1027 extended per
// Amendment §1), comparisons mixing Str and F64 (MC1027), and as a rule
// body's outermost return value (MC2058).
//
// Codes are emitted as `ValidationError::Schema { message: "... (MCxxxx)" }`
// matching the Phase 3I MC2057 pattern. The literal code text in the
// message is the contract — tests assert `format!("{e:?}").contains("MC1026")`.
// ---------------------------------------------------------------------------

/// Static type produced by an expression. Used by the type-context check
/// to decide whether a Str-domain value flows into a numeric or truthy
/// context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExprStaticType {
    /// Statically known F64 (or Null). Numeric domain.
    F64,
    /// Statically known Str. Returned by `StrLiteral`, `current_element`,
    /// and string equality operators (which return F64(0/1) — but for
    /// type-context purposes the value is numeric; see special handling
    /// below for `==` / `!=` operands themselves).
    Str,
    /// Indeterminate — could be either domain (e.g., a measure ref might
    /// return Null at runtime, or `Coalesce` of a mixed-type vararg, or
    /// any function whose return is conditional on input). The type
    /// check is permissive when either operand is Indeterminate; the
    /// runtime safety net (eval-time type checks in mc-core) catches
    /// what static analysis misses.
    Indeterminate,
}

/// Walk a parsed body and infer its static return type. This is a
/// best-effort check — `Indeterminate` is returned for any node whose
/// type depends on runtime values. The check is sound (never reports
/// `Str` for a numeric expression or vice versa) but incomplete (some
/// Str-typed expressions may be reported as Indeterminate, in which case
/// the runtime safety net catches them).
fn expr_static_type(body: &ParsedRuleBody) -> ExprStaticType {
    use ExprStaticType as T;
    match body {
        // Phase 3J: string-domain primitives — return Str.
        ParsedRuleBody::StrLiteral(_) | ParsedRuleBody::CurrentElement(_) => T::Str,
        // Phase 3J: param(name) returns F64 per Decision 6 (v1 supports
        // only `f64` parameter values; non-numeric values are rejected
        // at validate time).
        ParsedRuleBody::ParamRef(_) => T::F64,
        // Phase 3J: scenario_ref returns the measure's domain — F64
        // for the only measure data_types Phase 3J supports for cross-
        // scenario reads.
        ParsedRuleBody::ScenarioRef(_) | ParsedRuleBody::ExtrapolateLastValue(_) => T::F64,
        // Numeric primitives + every arithmetic / comparison / logical /
        // function call returning F64.
        ParsedRuleBody::Const(_)
        | ParsedRuleBody::PeriodIndex(_)
        | ParsedRuleBody::AnchorIndex(_)
        | ParsedRuleBody::IsPast(_)
        | ParsedRuleBody::IsCurrent(_)
        | ParsedRuleBody::IsFuture(_)
        | ParsedRuleBody::PeriodsSinceAnchor(_)
        | ParsedRuleBody::PeriodsToEnd(_)
        | ParsedRuleBody::Add(_)
        | ParsedRuleBody::Sub(_)
        | ParsedRuleBody::Mul(_)
        | ParsedRuleBody::Div(_)
        | ParsedRuleBody::Gt(_)
        | ParsedRuleBody::Lt(_)
        | ParsedRuleBody::Gte(_)
        | ParsedRuleBody::Lte(_)
        | ParsedRuleBody::Eq(_)
        | ParsedRuleBody::Neq(_)
        | ParsedRuleBody::And(_)
        | ParsedRuleBody::Or(_)
        | ParsedRuleBody::Not(_)
        | ParsedRuleBody::Abs(_)
        | ParsedRuleBody::SafeDiv(_)
        | ParsedRuleBody::Clamp(_)
        | ParsedRuleBody::ActualRef(_)
        | ParsedRuleBody::Prev(_)
        | ParsedRuleBody::Lag(_)
        | ParsedRuleBody::Cumulative(_)
        | ParsedRuleBody::RollingAvg(_)
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
        | ParsedRuleBody::WAvgOver(_) => T::F64,
        // `Ref` is statically F64 — measure values flow through eval as
        // F64 or Null; Phase 3J explicitly forbids Str-stored cells.
        ParsedRuleBody::Ref(_) => T::F64,
        // Conditional return type: `if`, `if_null`, and `coalesce` may
        // mix Str and F64 across branches; we cannot statically prove
        // the resulting type without unifying the branches. Return
        // Indeterminate; the runtime check catches Str-in-numeric
        // contexts.
        ParsedRuleBody::If(_) | ParsedRuleBody::IfNull(_) | ParsedRuleBody::Coalesce(_) => {
            T::Indeterminate
        }
        ParsedRuleBody::Min(_) | ParsedRuleBody::Max(_) => T::F64,
    }
}

/// Top-level entry: walk every rule body and emit the four type-context
/// errors (MC1026, MC1027, MC1028, MC2058).
fn check_str_type_context(
    _parsed: &ParsedModel,
    validated_rules: &[ValidatedRule],
    errors: &mut Vec<ValidationError>,
) {
    for r in validated_rules {
        // MC2058: rule body's outermost expression is statically Str.
        // (e.g., `current_element(Channel)` or `"Houston"` as the body.)
        if expr_static_type(&r.body) == ExprStaticType::Str {
            errors.push(ValidationError::Schema {
                message: format!(
                    "rule {:?}: body returns a non-numeric (Str) value; \
                     rule bodies must evaluate to F64 or Null (MC2058)",
                    r.name
                ),
            });
        }
        check_str_type_context_walk(&r.body, &r.name, errors);
    }
}

/// Recursive walker. For each node that takes children, validate the
/// children's static types match the node's type contract, then recurse
/// into them.
fn check_str_type_context_walk(
    body: &ParsedRuleBody,
    rule_name: &str,
    errors: &mut Vec<ValidationError>,
) {
    use ExprStaticType as T;

    // Reject Str in arithmetic operands (MC1026).
    let mut check_numeric_args = |args: &[ParsedRuleBody], op: &str| {
        for a in args {
            if expr_static_type(a) == T::Str {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "rule {rule_name:?}: {op} operator received a non-numeric (Str) operand; \
                         arithmetic on strings is not supported (MC1026)"
                    ),
                });
            }
        }
    };

    // Reject Str in numeric ordering operands (MC1028).
    let check_ordering = |left: &ParsedRuleBody,
                          right: &ParsedRuleBody,
                          op: &str,
                          errors: &mut Vec<ValidationError>| {
        if expr_static_type(left) == T::Str || expr_static_type(right) == T::Str {
            errors.push(ValidationError::Schema {
                message: format!(
                    "rule {rule_name:?}: {op} operator received a Str operand; \
                     locale-dependent string ordering is not supported (MC1028)"
                ),
            });
        }
    };

    // Reject mixed-type or Str-in-truthy-context (MC1027). Used for
    // `==` / `!=` (mismatch) and for `if` / `and` / `or` / `not`
    // (truthy context per Amendment §1).
    let check_eq_types = |left: &ParsedRuleBody,
                          right: &ParsedRuleBody,
                          op: &str,
                          errors: &mut Vec<ValidationError>| {
        let (lt, rt) = (expr_static_type(left), expr_static_type(right));
        if (lt == T::Str && rt == T::F64) || (lt == T::F64 && rt == T::Str) {
            errors.push(ValidationError::Schema {
                message: format!(
                    "rule {rule_name:?}: {op} compares Str with F64; comparisons must \
                     have matching types (MC1027)"
                ),
            });
        }
    };

    let check_truthy = |operand: &ParsedRuleBody, op: &str, errors: &mut Vec<ValidationError>| {
        if expr_static_type(operand) == T::Str {
            errors.push(ValidationError::Schema {
                message: format!(
                    "rule {rule_name:?}: {op} received a Str operand in a truthy context; \
                     Str values must be consumed by == / != before reaching boolean \
                     logic (MC1027)"
                ),
            });
        }
    };

    match body {
        ParsedRuleBody::Add(b) => {
            check_numeric_args(&b.add, "+");
            for a in &b.add {
                check_str_type_context_walk(a, rule_name, errors);
            }
        }
        ParsedRuleBody::Sub(b) => {
            check_numeric_args(&b.sub, "-");
            for a in &b.sub {
                check_str_type_context_walk(a, rule_name, errors);
            }
        }
        ParsedRuleBody::Mul(b) => {
            check_numeric_args(&b.mul, "*");
            for a in &b.mul {
                check_str_type_context_walk(a, rule_name, errors);
            }
        }
        ParsedRuleBody::Div(b) => {
            check_numeric_args(&b.div, "/");
            for a in &b.div {
                check_str_type_context_walk(a, rule_name, errors);
            }
        }
        ParsedRuleBody::Mod(b) => {
            if expr_static_type(&b.dividend) == T::Str || expr_static_type(&b.divisor) == T::Str {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "rule {rule_name:?}: mod() received a Str operand; \
                         arithmetic on strings is not supported (MC1026)"
                    ),
                });
            }
            check_str_type_context_walk(&b.dividend, rule_name, errors);
            check_str_type_context_walk(&b.divisor, rule_name, errors);
        }
        ParsedRuleBody::Pow(b) => {
            if expr_static_type(&b.base) == T::Str || expr_static_type(&b.exponent) == T::Str {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "rule {rule_name:?}: pow() received a Str operand; \
                         arithmetic on strings is not supported (MC1026)"
                    ),
                });
            }
            check_str_type_context_walk(&b.base, rule_name, errors);
            check_str_type_context_walk(&b.exponent, rule_name, errors);
        }
        // Numeric ordering — MC1028.
        ParsedRuleBody::Gt(b) => {
            check_ordering(&b.left, &b.right, ">", errors);
            check_str_type_context_walk(&b.left, rule_name, errors);
            check_str_type_context_walk(&b.right, rule_name, errors);
        }
        ParsedRuleBody::Lt(b) => {
            check_ordering(&b.left, &b.right, "<", errors);
            check_str_type_context_walk(&b.left, rule_name, errors);
            check_str_type_context_walk(&b.right, rule_name, errors);
        }
        ParsedRuleBody::Gte(b) => {
            check_ordering(&b.left, &b.right, ">=", errors);
            check_str_type_context_walk(&b.left, rule_name, errors);
            check_str_type_context_walk(&b.right, rule_name, errors);
        }
        ParsedRuleBody::Lte(b) => {
            check_ordering(&b.left, &b.right, "<=", errors);
            check_str_type_context_walk(&b.left, rule_name, errors);
            check_str_type_context_walk(&b.right, rule_name, errors);
        }
        // Equality — MC1027 type mismatch when sides differ.
        ParsedRuleBody::Eq(b) => {
            check_eq_types(&b.left, &b.right, "==", errors);
            check_str_type_context_walk(&b.left, rule_name, errors);
            check_str_type_context_walk(&b.right, rule_name, errors);
        }
        ParsedRuleBody::Neq(b) => {
            check_eq_types(&b.left, &b.right, "!=", errors);
            check_str_type_context_walk(&b.left, rule_name, errors);
            check_str_type_context_walk(&b.right, rule_name, errors);
        }
        // Truthy contexts (Amendment §1) — MC1027 extended.
        ParsedRuleBody::And(b) => {
            check_truthy(&b.left, "and", errors);
            check_truthy(&b.right, "and", errors);
            check_str_type_context_walk(&b.left, rule_name, errors);
            check_str_type_context_walk(&b.right, rule_name, errors);
        }
        ParsedRuleBody::Or(b) => {
            check_truthy(&b.left, "or", errors);
            check_truthy(&b.right, "or", errors);
            check_str_type_context_walk(&b.left, rule_name, errors);
            check_str_type_context_walk(&b.right, rule_name, errors);
        }
        ParsedRuleBody::Not(b) => {
            check_truthy(&b.operand, "not", errors);
            check_str_type_context_walk(&b.operand, rule_name, errors);
        }
        ParsedRuleBody::If(b) => {
            check_truthy(&b.condition, "if() condition", errors);
            check_str_type_context_walk(&b.condition, rule_name, errors);
            check_str_type_context_walk(&b.then_branch, rule_name, errors);
            check_str_type_context_walk(&b.else_branch, rule_name, errors);
        }
        // Composite numeric functions — recurse and forbid Str inputs.
        ParsedRuleBody::Abs(b) => {
            if expr_static_type(&b.operand) == T::Str {
                errors.push(ValidationError::Schema {
                    message: format!("rule {rule_name:?}: abs() received a Str operand (MC1026)"),
                });
            }
            check_str_type_context_walk(&b.operand, rule_name, errors);
        }
        ParsedRuleBody::Sqrt(b)
        | ParsedRuleBody::Ln(b)
        | ParsedRuleBody::Log10(b)
        | ParsedRuleBody::Round(b)
        | ParsedRuleBody::Floor(b)
        | ParsedRuleBody::Ceil(b)
        | ParsedRuleBody::Exp(b) => {
            if expr_static_type(&b.operand) == T::Str {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "rule {rule_name:?}: math primitive received a Str operand (MC1026)"
                    ),
                });
            }
            check_str_type_context_walk(&b.operand, rule_name, errors);
        }
        ParsedRuleBody::SafeDiv(b) => {
            if expr_static_type(&b.numerator) == T::Str
                || expr_static_type(&b.denominator) == T::Str
            {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "rule {rule_name:?}: safe_div() received a Str operand (MC1026)"
                    ),
                });
            }
            check_str_type_context_walk(&b.numerator, rule_name, errors);
            check_str_type_context_walk(&b.denominator, rule_name, errors);
            check_str_type_context_walk(&b.default, rule_name, errors);
        }
        ParsedRuleBody::Clamp(b) => {
            if expr_static_type(&b.value) == T::Str
                || expr_static_type(&b.lo) == T::Str
                || expr_static_type(&b.hi) == T::Str
            {
                errors.push(ValidationError::Schema {
                    message: format!("rule {rule_name:?}: clamp() received a Str operand (MC1026)"),
                });
            }
            check_str_type_context_walk(&b.value, rule_name, errors);
            check_str_type_context_walk(&b.lo, rule_name, errors);
            check_str_type_context_walk(&b.hi, rule_name, errors);
        }
        ParsedRuleBody::Min(b) | ParsedRuleBody::Max(b) => {
            for a in &b.args {
                if expr_static_type(a) == T::Str {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "rule {rule_name:?}: min/max received a Str operand (MC1026)"
                        ),
                    });
                }
                check_str_type_context_walk(a, rule_name, errors);
            }
        }
        // `coalesce` and `if_null` may legitimately take Str-typed
        // children if the resulting expression is consumed by another
        // string operator. We recurse without rejecting Str directly
        // here; the parent context catches Str in numeric / truthy
        // positions.
        ParsedRuleBody::Coalesce(b) => {
            for a in &b.args {
                check_str_type_context_walk(a, rule_name, errors);
            }
        }
        ParsedRuleBody::IfNull(b) => {
            for a in &b.if_null {
                check_str_type_context_walk(a, rule_name, errors);
            }
        }
        // NormCdf / NormInv take 3 numeric args.
        ParsedRuleBody::NormCdf(b) => {
            if expr_static_type(&b.x) == T::Str
                || expr_static_type(&b.mu) == T::Str
                || expr_static_type(&b.sigma) == T::Str
            {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "rule {rule_name:?}: norm_cdf received a Str operand (MC1026)"
                    ),
                });
            }
            check_str_type_context_walk(&b.x, rule_name, errors);
            check_str_type_context_walk(&b.mu, rule_name, errors);
            check_str_type_context_walk(&b.sigma, rule_name, errors);
        }
        ParsedRuleBody::NormInv(b) => {
            if expr_static_type(&b.p) == T::Str
                || expr_static_type(&b.mu) == T::Str
                || expr_static_type(&b.sigma) == T::Str
            {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "rule {rule_name:?}: norm_inv received a Str operand (MC1026)"
                    ),
                });
            }
            check_str_type_context_walk(&b.p, rule_name, errors);
            check_str_type_context_walk(&b.mu, rule_name, errors);
            check_str_type_context_walk(&b.sigma, rule_name, errors);
        }
        // Calibrate value must be numeric.
        ParsedRuleBody::Calibrate(b) => {
            if expr_static_type(&b.value) == T::Str {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "rule {rule_name:?}: calibrate() received a Str operand (MC1026)"
                    ),
                });
            }
            check_str_type_context_walk(&b.value, rule_name, errors);
        }
        // Predict's features must each be numeric.
        ParsedRuleBody::Predict(b) => {
            for f in &b.features {
                if expr_static_type(f) == T::Str {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "rule {rule_name:?}: predict() received a Str feature operand (MC1026)"
                        ),
                    });
                }
                check_str_type_context_walk(f, rule_name, errors);
            }
        }
        // Lag / RollingAvg's second arg is numeric (periods/window).
        ParsedRuleBody::Lag(b) => {
            if expr_static_type(&b.periods) == T::Str {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "rule {rule_name:?}: lag() periods argument must be numeric (MC1026)"
                    ),
                });
            }
            check_str_type_context_walk(&b.periods, rule_name, errors);
        }
        ParsedRuleBody::RollingAvg(b) => {
            if expr_static_type(&b.window) == T::Str {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "rule {rule_name:?}: rolling_avg() window must be numeric (MC1026)"
                    ),
                });
            }
            check_str_type_context_walk(&b.window, rule_name, errors);
        }
        // Bucket / Benchmark / Lookup keys are evaluated against ref-data
        // tables — they tolerate either F64 or Str values via lookup-key
        // coercion. No type-context check on those keys.
        ParsedRuleBody::Bucket(b) => {
            check_str_type_context_walk(&b.value, rule_name, errors);
        }
        ParsedRuleBody::Benchmark(b) => {
            check_str_type_context_walk(&b.key_expr, rule_name, errors);
        }
        ParsedRuleBody::Lookup(b) => {
            for k in &b.key_exprs {
                check_str_type_context_walk(k, rule_name, errors);
            }
        }
        // Leaves (no children to recurse into).
        ParsedRuleBody::Const(_)
        | ParsedRuleBody::Ref(_)
        | ParsedRuleBody::PeriodIndex(_)
        | ParsedRuleBody::AnchorIndex(_)
        | ParsedRuleBody::IsPast(_)
        | ParsedRuleBody::IsCurrent(_)
        | ParsedRuleBody::IsFuture(_)
        | ParsedRuleBody::PeriodsSinceAnchor(_)
        | ParsedRuleBody::PeriodsToEnd(_)
        | ParsedRuleBody::ActualRef(_)
        | ParsedRuleBody::ScenarioRef(_)
        | ParsedRuleBody::ExtrapolateLastValue(_)
        | ParsedRuleBody::Prev(_)
        | ParsedRuleBody::Cumulative(_)
        | ParsedRuleBody::SumOver(_)
        | ParsedRuleBody::IsElement(_)
        | ParsedRuleBody::AvgOver(_)
        | ParsedRuleBody::MinOver(_)
        | ParsedRuleBody::MaxOver(_)
        | ParsedRuleBody::WAvgOver(_)
        | ParsedRuleBody::StrLiteral(_)
        | ParsedRuleBody::CurrentElement(_)
        | ParsedRuleBody::ParamRef(_) => {}
    }
}

// ---------------------------------------------------------------------------
// Phase 3J item 3: parameters: block validation
//
// MC2060: parameter name collides with a declared measure name.
// MC2061: parameter name collides with a declared dim element name
//         (any dimension; ambiguity at the bare-identifier level).
// MC2062: a `param(name)` reference in a rule body names an
//         undeclared parameter.
// MC2063 / 2064 / 2065 / 2066 / 2067 / 2068 / 2069 are reserved for
// other Phase 3J items (Indicator measure role, scenario_ref, etc.).
// MC2070 (Phase 3H.1 / ADR-0017): fitted_model.output_bound.min >=
//         output_bound.max. Emitted by `check_fitted_model_blocks`.
// ---------------------------------------------------------------------------

fn check_parameters_block(
    parsed: &ParsedModel,
    validated_rules: &[ValidatedRule],
    errors: &mut Vec<ValidationError>,
) {
    let measure_names: BTreeSet<&str> = parsed.measures.iter().map(|m| m.name.as_str()).collect();
    let mut element_names: BTreeSet<&str> = BTreeSet::new();
    for d in &parsed.dimensions {
        for e in &d.elements {
            element_names.insert(e.name.as_str());
        }
    }
    let mut param_seen: BTreeMap<&str, usize> = BTreeMap::new();
    for p in &parsed.parameters {
        // Duplicate parameter name → DuplicateName.
        *param_seen.entry(p.name.as_str()).or_default() += 1;
        // MC2060: collides with a measure name.
        if measure_names.contains(p.name.as_str()) {
            errors.push(ValidationError::Schema {
                message: format!(
                    "parameter {:?} collides with a declared measure name (MC2060)",
                    p.name
                ),
            });
        }
        // MC2061: collides with a dim element name.
        if element_names.contains(p.name.as_str()) {
            errors.push(ValidationError::Schema {
                message: format!(
                    "parameter {:?} collides with a declared dim element name (MC2061)",
                    p.name
                ),
            });
        }
        // Reject NaN values up front (the kernel rejects NaN at write
        // time; mirror at validate so the diagnostic surfaces here).
        if p.value.is_nan() || !p.value.is_finite() {
            errors.push(ValidationError::Schema {
                message: format!(
                    "parameter {:?}: value must be a finite f64 (got {})",
                    p.name, p.value
                ),
            });
        }
    }
    for (name, count) in &param_seen {
        if *count > 1 {
            errors.push(ValidationError::DuplicateName {
                kind: "parameter".into(),
                name: (*name).to_string(),
            });
        }
    }
    // MC2062: walk every rule body and confirm each `param(name)`
    // reference resolves to a declared parameter.
    let declared_params: BTreeSet<&str> =
        parsed.parameters.iter().map(|p| p.name.as_str()).collect();
    for r in validated_rules {
        walk_param_refs(&r.body, &r.name, &declared_params, errors);
    }
}

fn walk_param_refs(
    body: &ParsedRuleBody,
    rule_name: &str,
    declared: &BTreeSet<&str>,
    errors: &mut Vec<ValidationError>,
) {
    match body {
        ParsedRuleBody::ParamRef(b) => {
            if !declared.contains(b.param.as_str()) {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "rule {rule_name:?}: param({:?}) references undeclared parameter (MC2062)",
                        b.param
                    ),
                });
            }
        }
        ParsedRuleBody::Const(_)
        | ParsedRuleBody::Ref(_)
        | ParsedRuleBody::PeriodIndex(_)
        | ParsedRuleBody::AnchorIndex(_)
        | ParsedRuleBody::IsPast(_)
        | ParsedRuleBody::IsCurrent(_)
        | ParsedRuleBody::IsFuture(_)
        | ParsedRuleBody::PeriodsSinceAnchor(_)
        | ParsedRuleBody::PeriodsToEnd(_)
        | ParsedRuleBody::ActualRef(_)
        | ParsedRuleBody::ScenarioRef(_)
        | ParsedRuleBody::ExtrapolateLastValue(_)
        | ParsedRuleBody::Prev(_)
        | ParsedRuleBody::Cumulative(_)
        | ParsedRuleBody::SumOver(_)
        | ParsedRuleBody::IsElement(_)
        | ParsedRuleBody::AvgOver(_)
        | ParsedRuleBody::MinOver(_)
        | ParsedRuleBody::MaxOver(_)
        | ParsedRuleBody::WAvgOver(_)
        | ParsedRuleBody::StrLiteral(_)
        | ParsedRuleBody::CurrentElement(_) => {}
        ParsedRuleBody::Add(b) => walk_param_args(&b.add, rule_name, declared, errors),
        ParsedRuleBody::Sub(b) => walk_param_args(&b.sub, rule_name, declared, errors),
        ParsedRuleBody::Mul(b) => walk_param_args(&b.mul, rule_name, declared, errors),
        ParsedRuleBody::Div(b) => walk_param_args(&b.div, rule_name, declared, errors),
        ParsedRuleBody::IfNull(b) => walk_param_args(&b.if_null, rule_name, declared, errors),
        ParsedRuleBody::Gt(b)
        | ParsedRuleBody::Lt(b)
        | ParsedRuleBody::Gte(b)
        | ParsedRuleBody::Lte(b)
        | ParsedRuleBody::Eq(b)
        | ParsedRuleBody::Neq(b)
        | ParsedRuleBody::And(b)
        | ParsedRuleBody::Or(b) => {
            walk_param_refs(&b.left, rule_name, declared, errors);
            walk_param_refs(&b.right, rule_name, declared, errors);
        }
        ParsedRuleBody::Not(b) | ParsedRuleBody::Abs(b) => {
            walk_param_refs(&b.operand, rule_name, declared, errors)
        }
        ParsedRuleBody::If(b) => {
            walk_param_refs(&b.condition, rule_name, declared, errors);
            walk_param_refs(&b.then_branch, rule_name, declared, errors);
            walk_param_refs(&b.else_branch, rule_name, declared, errors);
        }
        ParsedRuleBody::Min(b) | ParsedRuleBody::Max(b) | ParsedRuleBody::Coalesce(b) => {
            for a in &b.args {
                walk_param_refs(a, rule_name, declared, errors);
            }
        }
        ParsedRuleBody::SafeDiv(b) => {
            walk_param_refs(&b.numerator, rule_name, declared, errors);
            walk_param_refs(&b.denominator, rule_name, declared, errors);
            walk_param_refs(&b.default, rule_name, declared, errors);
        }
        ParsedRuleBody::Clamp(b) => {
            walk_param_refs(&b.value, rule_name, declared, errors);
            walk_param_refs(&b.lo, rule_name, declared, errors);
            walk_param_refs(&b.hi, rule_name, declared, errors);
        }
        ParsedRuleBody::Lag(b) => walk_param_refs(&b.periods, rule_name, declared, errors),
        ParsedRuleBody::RollingAvg(b) => walk_param_refs(&b.window, rule_name, declared, errors),
        ParsedRuleBody::Benchmark(b) => walk_param_refs(&b.key_expr, rule_name, declared, errors),
        ParsedRuleBody::Lookup(b) => {
            for k in &b.key_exprs {
                walk_param_refs(k, rule_name, declared, errors);
            }
        }
        ParsedRuleBody::Bucket(b) => walk_param_refs(&b.value, rule_name, declared, errors),
        ParsedRuleBody::Predict(b) => {
            for f in &b.features {
                walk_param_refs(f, rule_name, declared, errors);
            }
        }
        ParsedRuleBody::Calibrate(b) => walk_param_refs(&b.value, rule_name, declared, errors),
        ParsedRuleBody::Exp(b) => walk_param_refs(&b.operand, rule_name, declared, errors),
        ParsedRuleBody::NormCdf(b) => {
            walk_param_refs(&b.x, rule_name, declared, errors);
            walk_param_refs(&b.mu, rule_name, declared, errors);
            walk_param_refs(&b.sigma, rule_name, declared, errors);
        }
        ParsedRuleBody::Pow(b) => {
            walk_param_refs(&b.base, rule_name, declared, errors);
            walk_param_refs(&b.exponent, rule_name, declared, errors);
        }
        ParsedRuleBody::Sqrt(b)
        | ParsedRuleBody::Ln(b)
        | ParsedRuleBody::Log10(b)
        | ParsedRuleBody::Round(b)
        | ParsedRuleBody::Floor(b)
        | ParsedRuleBody::Ceil(b) => walk_param_refs(&b.operand, rule_name, declared, errors),
        ParsedRuleBody::Mod(b) => {
            walk_param_refs(&b.dividend, rule_name, declared, errors);
            walk_param_refs(&b.divisor, rule_name, declared, errors);
        }
        ParsedRuleBody::NormInv(b) => {
            walk_param_refs(&b.p, rule_name, declared, errors);
            walk_param_refs(&b.mu, rule_name, declared, errors);
            walk_param_refs(&b.sigma, rule_name, declared, errors);
        }
    }
}

fn walk_param_args(
    args: &[ParsedRuleBody],
    rule_name: &str,
    declared: &BTreeSet<&str>,
    errors: &mut Vec<ValidationError>,
) {
    for a in args {
        walk_param_refs(a, rule_name, declared, errors);
    }
}

// ---------------------------------------------------------------------------
// Phase 3J item 4: Indicator measure validation
//
// MC2063: Indicator measure declared with a `body:` (rule field) — but
//         since rule bodies live on the rule, not on the measure, we
//         test this via the rule list. An Indicator-targeted rule is
//         the rejection.
// MC2064: Indicator missing `dimension:` or `element:`.
//
// Element existence within the named dimension is caught by the
// synthesized rule's normal compile-time element resolution; if the
// dim or element name is unknown, MC1022 / MC1023 fire.
// ---------------------------------------------------------------------------

fn check_indicator_measures(parsed: &ParsedModel, errors: &mut Vec<ValidationError>) {
    let dim_by_name: BTreeMap<&str, &crate::schema::ParsedDimension> = parsed
        .dimensions
        .iter()
        .map(|d| (d.name.as_str(), d))
        .collect();
    for m in &parsed.measures {
        if m.role != "Indicator" {
            continue;
        }
        // MC2064: dimension + element are required for Indicator.
        if m.dimension.is_none() {
            errors.push(ValidationError::Schema {
                message: format!(
                    "Indicator measure {:?}: missing required `dimension:` field (MC2064)",
                    m.name
                ),
            });
        }
        if m.element.is_none() {
            errors.push(ValidationError::Schema {
                message: format!(
                    "Indicator measure {:?}: missing required `element:` field (MC2064)",
                    m.name
                ),
            });
        }
        // MC1022 / MC1023: validate the dim name + element name now so
        // the diagnostic surfaces against the measure rather than the
        // synthesized rule.
        if let (Some(dim_name), Some(elem_name)) = (&m.dimension, &m.element) {
            match dim_by_name.get(dim_name.as_str()) {
                None => {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "Indicator measure {:?}: references unknown dimension {:?} (MC1023)",
                            m.name, dim_name
                        ),
                    });
                }
                Some(dim) => {
                    let known: BTreeSet<&str> =
                        dim.elements.iter().map(|e| e.name.as_str()).collect();
                    if !known.contains(elem_name.as_str()) {
                        errors.push(ValidationError::Schema {
                            message: format!(
                                "Indicator measure {:?}: references unknown element {:?} \
                                 in dimension {:?} (MC1022)",
                                m.name, elem_name, dim_name
                            ),
                        });
                    }
                }
            }
        }
    }
    // MC2063: any rule that targets an Indicator measure is ambiguous —
    // Indicator bodies are synthesized; a user-supplied rule double-binds.
    let indicator_names: BTreeSet<&str> = parsed
        .measures
        .iter()
        .filter(|m| m.role == "Indicator")
        .map(|m| m.name.as_str())
        .collect();
    for r in &parsed.rules {
        if indicator_names.contains(r.target_measure.as_str()) {
            errors.push(ValidationError::Schema {
                message: format!(
                    "rule {:?}: target measure {:?} is an Indicator; Indicator measures must \
                     not have user-supplied rule bodies (MC2063)",
                    r.name, r.target_measure
                ),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 3J item 5: Scope variants validation (MC1029, MC2069)
//
// MC1029: unknown scope name in rule's `scope:` field.
// MC2069 (Amendment §4): a non-AllLeaves scope variant requires the
//        Time dim to have a `time_anchor:` configured.
// MC2068 (compile-time defense): Validator should catch unknown
//        scope names; if compile sees one it fires MC2068 (handled in
//        compile.rs's `EngineError::Internal` arm).
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Phase 3J item 6: scenario_ref + actual_ref(measure, fallback) validation
//
// MC2065: scenario_ref's `scenario` argument is not a declared element
//         of the Scenario-kind dim.
// MC2066: actual_ref's optional fallback expression has a static type
//         (Str) that mismatches the measure's domain (F64). Best-effort
//         — Indeterminate fallbacks (e.g., `if(...)`) are accepted and
//         the runtime checks again.
// ---------------------------------------------------------------------------

fn check_scenario_ref_and_fallback(
    parsed: &ParsedModel,
    validated_rules: &[ValidatedRule],
    errors: &mut Vec<ValidationError>,
) {
    // Collect Scenario element names.
    let scenario_elems: BTreeSet<&str> = parsed
        .dimensions
        .iter()
        .filter(|d| d.kind == "Scenario")
        .flat_map(|d| d.elements.iter().map(|e| e.name.as_str()))
        .collect();
    for r in validated_rules {
        walk_scenario_and_fallback(&r.body, &r.name, &scenario_elems, errors);
    }
}

fn walk_scenario_and_fallback(
    body: &ParsedRuleBody,
    rule_name: &str,
    scenario_elems: &BTreeSet<&str>,
    errors: &mut Vec<ValidationError>,
) {
    use ExprStaticType as T;
    match body {
        ParsedRuleBody::ScenarioRef(b) => {
            if !scenario_elems.contains(b.scenario.as_str()) {
                errors.push(ValidationError::Schema {
                    message: format!(
                        "rule {rule_name:?}: scenario_ref({}, {:?}) references unknown \
                         scenario element (MC2065)",
                        b.measure, b.scenario
                    ),
                });
            }
        }
        ParsedRuleBody::ActualRef(b) => {
            if let Some(fb) = &b.fallback {
                // MC2066: fallback type mismatch. F64 measures (the
                // only Phase 3J kind) require a numeric-typed fallback;
                // Str fallbacks are rejected.
                if expr_static_type(fb) == T::Str {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "rule {rule_name:?}: actual_ref({}, fallback) fallback returns Str \
                             but the measure is numeric (MC2066)",
                            b.measure
                        ),
                    });
                }
                walk_scenario_and_fallback(fb, rule_name, scenario_elems, errors);
            }
        }
        // Recurse through composite nodes.
        ParsedRuleBody::Const(_)
        | ParsedRuleBody::Ref(_)
        | ParsedRuleBody::PeriodIndex(_)
        | ParsedRuleBody::AnchorIndex(_)
        | ParsedRuleBody::IsPast(_)
        | ParsedRuleBody::IsCurrent(_)
        | ParsedRuleBody::IsFuture(_)
        | ParsedRuleBody::PeriodsSinceAnchor(_)
        | ParsedRuleBody::PeriodsToEnd(_)
        | ParsedRuleBody::Prev(_)
        | ParsedRuleBody::Cumulative(_)
        | ParsedRuleBody::SumOver(_)
        | ParsedRuleBody::IsElement(_)
        | ParsedRuleBody::AvgOver(_)
        | ParsedRuleBody::MinOver(_)
        | ParsedRuleBody::MaxOver(_)
        | ParsedRuleBody::WAvgOver(_)
        | ParsedRuleBody::StrLiteral(_)
        | ParsedRuleBody::CurrentElement(_)
        | ParsedRuleBody::ParamRef(_)
        | ParsedRuleBody::ExtrapolateLastValue(_) => {}
        ParsedRuleBody::Add(b) => {
            for a in &b.add {
                walk_scenario_and_fallback(a, rule_name, scenario_elems, errors);
            }
        }
        ParsedRuleBody::Sub(b) => {
            for a in &b.sub {
                walk_scenario_and_fallback(a, rule_name, scenario_elems, errors);
            }
        }
        ParsedRuleBody::Mul(b) => {
            for a in &b.mul {
                walk_scenario_and_fallback(a, rule_name, scenario_elems, errors);
            }
        }
        ParsedRuleBody::Div(b) => {
            for a in &b.div {
                walk_scenario_and_fallback(a, rule_name, scenario_elems, errors);
            }
        }
        ParsedRuleBody::IfNull(b) => {
            for a in &b.if_null {
                walk_scenario_and_fallback(a, rule_name, scenario_elems, errors);
            }
        }
        ParsedRuleBody::Gt(b)
        | ParsedRuleBody::Lt(b)
        | ParsedRuleBody::Gte(b)
        | ParsedRuleBody::Lte(b)
        | ParsedRuleBody::Eq(b)
        | ParsedRuleBody::Neq(b)
        | ParsedRuleBody::And(b)
        | ParsedRuleBody::Or(b) => {
            walk_scenario_and_fallback(&b.left, rule_name, scenario_elems, errors);
            walk_scenario_and_fallback(&b.right, rule_name, scenario_elems, errors);
        }
        ParsedRuleBody::Not(b) | ParsedRuleBody::Abs(b) => {
            walk_scenario_and_fallback(&b.operand, rule_name, scenario_elems, errors);
        }
        ParsedRuleBody::If(b) => {
            walk_scenario_and_fallback(&b.condition, rule_name, scenario_elems, errors);
            walk_scenario_and_fallback(&b.then_branch, rule_name, scenario_elems, errors);
            walk_scenario_and_fallback(&b.else_branch, rule_name, scenario_elems, errors);
        }
        ParsedRuleBody::Min(b) | ParsedRuleBody::Max(b) | ParsedRuleBody::Coalesce(b) => {
            for a in &b.args {
                walk_scenario_and_fallback(a, rule_name, scenario_elems, errors);
            }
        }
        ParsedRuleBody::SafeDiv(b) => {
            walk_scenario_and_fallback(&b.numerator, rule_name, scenario_elems, errors);
            walk_scenario_and_fallback(&b.denominator, rule_name, scenario_elems, errors);
            walk_scenario_and_fallback(&b.default, rule_name, scenario_elems, errors);
        }
        ParsedRuleBody::Clamp(b) => {
            walk_scenario_and_fallback(&b.value, rule_name, scenario_elems, errors);
            walk_scenario_and_fallback(&b.lo, rule_name, scenario_elems, errors);
            walk_scenario_and_fallback(&b.hi, rule_name, scenario_elems, errors);
        }
        ParsedRuleBody::Lag(b) => {
            walk_scenario_and_fallback(&b.periods, rule_name, scenario_elems, errors);
        }
        ParsedRuleBody::RollingAvg(b) => {
            walk_scenario_and_fallback(&b.window, rule_name, scenario_elems, errors);
        }
        ParsedRuleBody::Benchmark(b) => {
            walk_scenario_and_fallback(&b.key_expr, rule_name, scenario_elems, errors);
        }
        ParsedRuleBody::Lookup(b) => {
            for k in &b.key_exprs {
                walk_scenario_and_fallback(k, rule_name, scenario_elems, errors);
            }
        }
        ParsedRuleBody::Bucket(b) => {
            walk_scenario_and_fallback(&b.value, rule_name, scenario_elems, errors);
        }
        ParsedRuleBody::Predict(b) => {
            for f in &b.features {
                walk_scenario_and_fallback(f, rule_name, scenario_elems, errors);
            }
        }
        ParsedRuleBody::Calibrate(b) => {
            walk_scenario_and_fallback(&b.value, rule_name, scenario_elems, errors);
        }
        ParsedRuleBody::Exp(b) => {
            walk_scenario_and_fallback(&b.operand, rule_name, scenario_elems, errors);
        }
        ParsedRuleBody::NormCdf(b) => {
            walk_scenario_and_fallback(&b.x, rule_name, scenario_elems, errors);
            walk_scenario_and_fallback(&b.mu, rule_name, scenario_elems, errors);
            walk_scenario_and_fallback(&b.sigma, rule_name, scenario_elems, errors);
        }
        ParsedRuleBody::Pow(b) => {
            walk_scenario_and_fallback(&b.base, rule_name, scenario_elems, errors);
            walk_scenario_and_fallback(&b.exponent, rule_name, scenario_elems, errors);
        }
        ParsedRuleBody::Sqrt(b)
        | ParsedRuleBody::Ln(b)
        | ParsedRuleBody::Log10(b)
        | ParsedRuleBody::Round(b)
        | ParsedRuleBody::Floor(b)
        | ParsedRuleBody::Ceil(b) => {
            walk_scenario_and_fallback(&b.operand, rule_name, scenario_elems, errors);
        }
        ParsedRuleBody::Mod(b) => {
            walk_scenario_and_fallback(&b.dividend, rule_name, scenario_elems, errors);
            walk_scenario_and_fallback(&b.divisor, rule_name, scenario_elems, errors);
        }
        ParsedRuleBody::NormInv(b) => {
            walk_scenario_and_fallback(&b.p, rule_name, scenario_elems, errors);
            walk_scenario_and_fallback(&b.mu, rule_name, scenario_elems, errors);
            walk_scenario_and_fallback(&b.sigma, rule_name, scenario_elems, errors);
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 3J item 7 + Amendment §11: extrapolate_last_value scope check
//
// MC2067: a rule using `extrapolate_last_value(measure)` must have
//         `scope: FutureLeaves` OR `allow_past_extrapolation: true`.
//         The override flag (renamed from the original
//         `extrapolate_anywhere` per Amendment §11) is self-
//         documenting — the user reading the YAML knows immediately
//         what's being unlocked.
// ---------------------------------------------------------------------------

fn check_extrapolate_scope(validated_rules: &[ValidatedRule], errors: &mut Vec<ValidationError>) {
    for r in validated_rules {
        if !body_uses_extrapolate(&r.body) {
            continue;
        }
        if r.scope == "FutureLeaves" {
            continue;
        }
        if r.allow_past_extrapolation {
            continue;
        }
        errors.push(ValidationError::Schema {
            message: format!(
                "rule {:?}: extrapolate_last_value used at scope {:?}; requires \
                 scope: FutureLeaves OR allow_past_extrapolation: true (MC2067)",
                r.name, r.scope
            ),
        });
    }
}

fn body_uses_extrapolate(body: &ParsedRuleBody) -> bool {
    match body {
        ParsedRuleBody::ExtrapolateLastValue(_) => true,
        ParsedRuleBody::Const(_)
        | ParsedRuleBody::Ref(_)
        | ParsedRuleBody::PeriodIndex(_)
        | ParsedRuleBody::AnchorIndex(_)
        | ParsedRuleBody::IsPast(_)
        | ParsedRuleBody::IsCurrent(_)
        | ParsedRuleBody::IsFuture(_)
        | ParsedRuleBody::PeriodsSinceAnchor(_)
        | ParsedRuleBody::PeriodsToEnd(_)
        | ParsedRuleBody::ActualRef(_)
        | ParsedRuleBody::ScenarioRef(_)
        | ParsedRuleBody::Prev(_)
        | ParsedRuleBody::Cumulative(_)
        | ParsedRuleBody::SumOver(_)
        | ParsedRuleBody::IsElement(_)
        | ParsedRuleBody::AvgOver(_)
        | ParsedRuleBody::MinOver(_)
        | ParsedRuleBody::MaxOver(_)
        | ParsedRuleBody::WAvgOver(_)
        | ParsedRuleBody::StrLiteral(_)
        | ParsedRuleBody::CurrentElement(_)
        | ParsedRuleBody::ParamRef(_) => false,
        ParsedRuleBody::Add(b) => b.add.iter().any(body_uses_extrapolate),
        ParsedRuleBody::Sub(b) => b.sub.iter().any(body_uses_extrapolate),
        ParsedRuleBody::Mul(b) => b.mul.iter().any(body_uses_extrapolate),
        ParsedRuleBody::Div(b) => b.div.iter().any(body_uses_extrapolate),
        ParsedRuleBody::IfNull(b) => b.if_null.iter().any(body_uses_extrapolate),
        ParsedRuleBody::Gt(b)
        | ParsedRuleBody::Lt(b)
        | ParsedRuleBody::Gte(b)
        | ParsedRuleBody::Lte(b)
        | ParsedRuleBody::Eq(b)
        | ParsedRuleBody::Neq(b)
        | ParsedRuleBody::And(b)
        | ParsedRuleBody::Or(b) => {
            body_uses_extrapolate(&b.left) || body_uses_extrapolate(&b.right)
        }
        ParsedRuleBody::Not(b) | ParsedRuleBody::Abs(b) => body_uses_extrapolate(&b.operand),
        ParsedRuleBody::If(b) => {
            body_uses_extrapolate(&b.condition)
                || body_uses_extrapolate(&b.then_branch)
                || body_uses_extrapolate(&b.else_branch)
        }
        ParsedRuleBody::Min(b) | ParsedRuleBody::Max(b) | ParsedRuleBody::Coalesce(b) => {
            b.args.iter().any(body_uses_extrapolate)
        }
        ParsedRuleBody::SafeDiv(b) => {
            body_uses_extrapolate(&b.numerator)
                || body_uses_extrapolate(&b.denominator)
                || body_uses_extrapolate(&b.default)
        }
        ParsedRuleBody::Clamp(b) => {
            body_uses_extrapolate(&b.value)
                || body_uses_extrapolate(&b.lo)
                || body_uses_extrapolate(&b.hi)
        }
        ParsedRuleBody::Lag(b) => body_uses_extrapolate(&b.periods),
        ParsedRuleBody::RollingAvg(b) => body_uses_extrapolate(&b.window),
        ParsedRuleBody::Benchmark(b) => body_uses_extrapolate(&b.key_expr),
        ParsedRuleBody::Lookup(b) => b.key_exprs.iter().any(|k| body_uses_extrapolate(k)),
        ParsedRuleBody::Bucket(b) => body_uses_extrapolate(&b.value),
        ParsedRuleBody::Predict(b) => b.features.iter().any(|f| body_uses_extrapolate(f)),
        ParsedRuleBody::Calibrate(b) => body_uses_extrapolate(&b.value),
        ParsedRuleBody::Exp(b) => body_uses_extrapolate(&b.operand),
        ParsedRuleBody::NormCdf(b) => {
            body_uses_extrapolate(&b.x)
                || body_uses_extrapolate(&b.mu)
                || body_uses_extrapolate(&b.sigma)
        }
        ParsedRuleBody::Pow(b) => {
            body_uses_extrapolate(&b.base) || body_uses_extrapolate(&b.exponent)
        }
        ParsedRuleBody::Sqrt(b)
        | ParsedRuleBody::Ln(b)
        | ParsedRuleBody::Log10(b)
        | ParsedRuleBody::Round(b)
        | ParsedRuleBody::Floor(b)
        | ParsedRuleBody::Ceil(b) => body_uses_extrapolate(&b.operand),
        ParsedRuleBody::Mod(b) => {
            body_uses_extrapolate(&b.dividend) || body_uses_extrapolate(&b.divisor)
        }
        ParsedRuleBody::NormInv(b) => {
            body_uses_extrapolate(&b.p)
                || body_uses_extrapolate(&b.mu)
                || body_uses_extrapolate(&b.sigma)
        }
    }
}

fn check_scope_variants(parsed: &ParsedModel, errors: &mut Vec<ValidationError>) {
    // Determine if a `time_anchor` is configured on any Time dim.
    let time_anchor_configured = parsed
        .dimensions
        .iter()
        .any(|d| d.kind == "Time" && d.time_anchor.is_some());
    for r in &parsed.rules {
        match r.scope.as_str() {
            "AllLeaves" => {} // backward-compat; never requires time_anchor
            "FutureLeaves" | "PastLeaves" | "CurrentLeaves" => {
                if !time_anchor_configured {
                    errors.push(ValidationError::Schema {
                        message: format!(
                            "rule {:?}: scope {:?} requires `time_anchor` configured on the \
                             Time dimension (MC2069)",
                            r.name, r.scope
                        ),
                    });
                }
            }
            other => {
                errors.push(ValidationError::Schema {
                    message: format!("rule {:?}: unknown scope name {:?} (MC1029)", r.name, other),
                });
            }
        }
    }
}
