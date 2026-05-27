//! Phase 3B lint module — 10 advisory rules over [`ValidatedModel`].
//!
//! Per [ADR-0005](../../../docs/decisions/0005-phase-3b-model-qa-linter-diagnostics.md)
//! Decision 2 + the handoff: lint is **always advisory**.
//! `mc_model::load()` IGNORES lint output entirely — the two paths are
//! decoupled at the library boundary so consumers (CLI, future Phase 4
//! LLM, future Phase 6 UI) can treat them independently.
//!
//! ### Rules (per ADR-0005 Decision 5 + amendments #4 / #5 / #11)
//!
//! | Code   | Severity | Rule                                                        |
//! |--------|----------|-------------------------------------------------------------|
//! | MC3001 | Warning  | Missing description on dimension                            |
//! | MC3002 | Warning  | Missing description on measure                              |
//! | MC3003 | Warning  | Missing description on rule                                 |
//! | MC3004 | Warning  | Model has no golden tests                                   |
//! | MC3005 | Warning  | Orphan element (in dim, not in default hierarchy)           |
//! | MC3006 | Info     | Long rule chain depth (model-complexity primary)            |
//! | MC3007 | Warning  | Ratio-named measure using `Sum` aggregation                 |
//! | MC3008 | —        | **RETIRED** (promoted to MC2011 in Phase 3B)                |
//! | MC3009 | Info     | Unused input measure                                        |
//! | MC3010 | Info     | Unused derived measure                                      |
//! | MC3011 | Warning  | Hierarchy default has multiple roots                        |
//!
//! ### MC3006 threshold (handoff §C)
//!
//! ADR-0005 Decision 5's text says "≥ 5 deep". The handoff §C explicitly
//! recommends triggering at **strictly > 5** (depth ≥ 6) so Acme's
//! existing depth-5 chain (Spend → Clicks → Leads → Customers → Revenue
//! → Gross_Profit) stays clean. Phase 3B locks this interpretation in:
//! the threshold is `> 5`. Documented in the completion report.
//!
//! ### MC3008 retirement
//!
//! No active rule emits the code `"MC3008"`. The slot is permanently
//! reserved-as-retired (amendment #11). `tests/mc3008_retired.rs`
//! asserts this contract every CI run. New lint rules use MC3012+.

use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostic::{sort_diagnostics, Diagnostic, ModelPath, Severity};
use crate::schema::{ParsedDimension, ParsedHierarchy, ParsedRuleBody, ValidatedModel};

/// Run every Phase 3B lint rule and return the diagnostics in deterministic
/// emission order.
///
/// The function takes `&ValidatedModel` (not `&Path`, not raw YAML) — by
/// the time `lint()` runs, the model has parsed cleanly + passed every
/// validator. Lint rules concern themselves only with quality, never
/// correctness.
///
/// Output is **pre-sorted** per ADR-0005 amendment #14
/// `(severity desc, code asc, yaml_pointer asc, message asc)`. Callers
/// can render directly.
pub fn lint(model: &ValidatedModel) -> Vec<Diagnostic> {
    lint_with_file(model, std::path::PathBuf::new())
}

/// Like [`lint`] but tags every diagnostic with the source file path. The
/// CLI calls this with the YAML path the user passed; tests pass empty
/// paths or fixture paths as appropriate.
pub fn lint_with_file(
    model: &ValidatedModel,
    file: impl Into<std::path::PathBuf>,
) -> Vec<Diagnostic> {
    let file = file.into();
    let mut out = Vec::new();
    out.extend(mc3001_missing_dim_descriptions(model, &file));
    out.extend(mc3002_missing_measure_descriptions(model, &file));
    out.extend(mc3003_missing_rule_descriptions(model, &file));
    out.extend(mc3004_no_golden_tests(model, &file));
    out.extend(mc3005_orphan_elements(model, &file));
    out.extend(mc3006_long_rule_chain(model, &file));
    out.extend(mc3007_ratio_with_sum(model, &file));
    // MC3008 deliberately absent — see module docs.
    out.extend(mc3009_unused_input_measure(model, &file));
    out.extend(mc3010_unused_derived_measure(model, &file));
    out.extend(mc3011_hierarchy_root_ambiguity(model, &file));
    out.extend(mc3016_time_chronological_order(model, &file));
    out.extend(mc3017_stale_fitted_model(model, &file));
    out.extend(mc3018_stale_calibration_map(model, &file));
    sort_diagnostics(&mut out);
    out
}

// ---------------------------------------------------------------------------
// MC3001 — missing dimension description
// ---------------------------------------------------------------------------

fn mc3001_missing_dim_descriptions(
    model: &ValidatedModel,
    file: &std::path::Path,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (i, d) in model.parsed.dimensions.iter().enumerate() {
        if !has_text(&d.description) {
            out.push(Diagnostic {
                code: "MC3001",
                severity: Severity::Warning,
                path: ModelPath::new(
                    file,
                    format!("/dimensions/{i}"),
                    format!("dimensions.{}", d.name),
                ),
                message: format!("Dimension '{}' has no description", d.name),
                suggestion: Some(
                    "Add a one-line description explaining what the dim represents".into(),
                ),
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// MC3002 — missing measure description
// ---------------------------------------------------------------------------

fn mc3002_missing_measure_descriptions(
    model: &ValidatedModel,
    file: &std::path::Path,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (i, m) in model.parsed.measures.iter().enumerate() {
        if !has_text(&m.description) {
            out.push(Diagnostic {
                code: "MC3002",
                severity: Severity::Warning,
                path: ModelPath::new(
                    file,
                    format!("/measures/{i}"),
                    format!("measures.{}", m.name),
                ),
                message: format!("Measure '{}' has no description", m.name),
                suggestion: Some(
                    "Add a one-line description explaining what the measure represents and its unit"
                        .into(),
                ),
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// MC3003 — missing rule description
// ---------------------------------------------------------------------------

fn mc3003_missing_rule_descriptions(
    model: &ValidatedModel,
    file: &std::path::Path,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (i, r) in model.rules.iter().enumerate() {
        if !has_text(&r.description) {
            out.push(Diagnostic {
                code: "MC3003",
                severity: Severity::Warning,
                path: ModelPath::new(file, format!("/rules/{i}"), format!("rules.{}", r.name)),
                message: format!("Rule '{}' has no description", r.name),
                suggestion: Some(
                    "Add a one-line description explaining the business meaning of the rule".into(),
                ),
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// MC3004 — model has no golden tests
// ---------------------------------------------------------------------------

fn mc3004_no_golden_tests(model: &ValidatedModel, file: &std::path::Path) -> Vec<Diagnostic> {
    if !model.parsed.golden_tests.is_empty() {
        return Vec::new();
    }
    vec![Diagnostic {
        code: "MC3004",
        severity: Severity::Warning,
        path: ModelPath::new(file, "/golden_tests", "golden_tests"),
        message: "Model declares no golden_tests".into(),
        suggestion: Some(
            "Add at least one golden test pinning a known-good value (start with the model's anchor coords)"
                .into(),
        ),
    }]
}

// ---------------------------------------------------------------------------
// MC3005 — orphan element (not in default hierarchy)
//
// Fires only for dims that have at least one explicitly declared
// hierarchy with non-empty edges. For dims with no edges (Scenario,
// Version, Measure on Acme), every element is trivially "not in the
// hierarchy" but flagging them all would be noise — those dims don't
// participate in rollup. Skip.
// ---------------------------------------------------------------------------

fn mc3005_orphan_elements(model: &ValidatedModel, file: &std::path::Path) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (dim_idx, dim) in model.parsed.dimensions.iter().enumerate() {
        let Some(default_hier) = default_hierarchy_for(dim, &model.parsed.hierarchies) else {
            continue;
        };
        if default_hier.edges.is_empty() {
            continue;
        }
        let mut touched: BTreeSet<&str> = BTreeSet::new();
        for edge in &default_hier.edges {
            touched.insert(edge.parent.as_str());
            touched.insert(edge.child.as_str());
        }
        for (elem_idx, elem) in dim.elements.iter().enumerate() {
            if !touched.contains(elem.name.as_str()) {
                out.push(Diagnostic {
                    code: "MC3005",
                    severity: Severity::Warning,
                    path: ModelPath::new(
                        file,
                        format!("/dimensions/{dim_idx}/elements/{elem_idx}"),
                        format!("dimensions.{}.elements.{}", dim.name, elem.name),
                    ),
                    message: format!(
                        "Element '{}' in dim '{}' is not a member of default hierarchy '{}'",
                        elem.name, dim.name, default_hier.name
                    ),
                    suggestion: Some(
                        "Either add the element to the default hierarchy via an edge, or remove it if unused"
                            .into(),
                    ),
                });
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// MC3006 — long rule chain depth (threshold > 5; complexity primary)
// ---------------------------------------------------------------------------

fn mc3006_long_rule_chain(model: &ValidatedModel, file: &std::path::Path) -> Vec<Diagnostic> {
    let depths = compute_chain_depths(model);
    let mut out = Vec::new();
    // Phase 3B threshold (per handoff §C): trigger STRICTLY > 5 (depth ≥ 6)
    // so Acme's depth-5 chain (Gross_Profit → Revenue → Customers → Leads
    // → Clicks → Spend) stays clean. ADR-0005 Decision 5 says "≥ 5";
    // handoff §C documents the strict-> 5 interpretation.
    const THRESHOLD: usize = 5;
    for (rule_idx, rule) in model.rules.iter().enumerate() {
        let depth = depths
            .get(rule.target_measure.as_str())
            .copied()
            .unwrap_or(0);
        if depth > THRESHOLD {
            out.push(Diagnostic {
                code: "MC3006",
                severity: Severity::Info,
                path: ModelPath::new(
                    file,
                    format!("/rules/{rule_idx}"),
                    format!("rules.{}", rule.name),
                ),
                message: format!(
                    "Rule '{}' targets measure '{}' at chain depth {} (threshold > {})",
                    rule.name, rule.target_measure, depth, THRESHOLD
                ),
                suggestion: Some(
                    "Long rule chains are harder to reason about; consider whether intermediate \
                     measures could be inlined or whether the chain reflects unnecessary indirection. \
                     (Performance is a secondary concern: cold derived reads scale ~linearly with \
                     chain depth — see PERF.md §6.)"
                        .into(),
                ),
            });
        }
    }
    out
}

/// Compute chain depth for every measure. Inputs are depth 0; a rule body
/// referencing only inputs is depth 1; transitively `1 + max(depth of refs)`.
fn compute_chain_depths(model: &ValidatedModel) -> BTreeMap<String, usize> {
    let mut depths: BTreeMap<String, usize> = BTreeMap::new();
    for m in &model.parsed.measures {
        if m.role == "Input" {
            depths.insert(m.name.clone(), 0);
        }
    }
    // Iterate to fixed point — Phase 3A enforces an acyclic rule graph,
    // so this terminates in at most rules.len() passes.
    let n = model.rules.len();
    for _ in 0..=n {
        let mut changed = false;
        for r in &model.rules {
            let mut refs = BTreeSet::new();
            collect_body_refs(&r.body, &mut refs);
            let mut max_dep = 0usize;
            let mut all_resolved = true;
            for ref_name in &refs {
                match depths.get(ref_name) {
                    Some(&d) => max_dep = max_dep.max(d),
                    None => {
                        all_resolved = false;
                        break;
                    }
                }
            }
            if all_resolved {
                let new_depth = max_dep + 1;
                let entry = depths.entry(r.target_measure.clone()).or_insert(usize::MAX);
                if *entry == usize::MAX || *entry < new_depth {
                    *entry = new_depth;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    depths
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
        ParsedRuleBody::ActualRef(b) => {
            out.insert(b.measure.clone());
            if let Some(fb) = &b.fallback {
                collect_body_refs(fb, out);
            }
        }
        ParsedRuleBody::ScenarioRef(b) => {
            out.insert(b.measure.clone());
        }
        // Phase 3J item 7: extrapolate_last_value(measure).
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
        // Phase 3L: nbinom_sf / nbinom_cdf — collect refs from k, mu, alpha.
        ParsedRuleBody::NbinomSf(b) | ParsedRuleBody::NbinomCdf(b) => {
            collect_body_refs(&b.k, out);
            collect_body_refs(&b.mu, out);
            collect_body_refs(&b.alpha, out);
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
        ParsedRuleBody::IsElement(_) => {}
        ParsedRuleBody::AvgOver(b) | ParsedRuleBody::MinOver(b) | ParsedRuleBody::MaxOver(b) => {
            out.insert(b.measure.clone());
        }
        ParsedRuleBody::WAvgOver(b) => {
            out.insert(b.value_measure.clone());
            out.insert(b.weight_measure.clone());
        }
        // Phase 3J: string-domain primitives — no measure refs.
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
// MC3007 — ratio-named measure using Sum aggregation
//
// Per handoff §B: name match is case-insensitive on `*_rate`, `*_ratio`,
// `*_pct`, or one of the named ratio measures (cpc/cvr/aov/cpa/roas).
// Heuristic — false positives possible; the suggestion text says
// "verify the aggregation rule matches the measure's intent".
// ---------------------------------------------------------------------------

fn mc3007_ratio_with_sum(model: &ValidatedModel, file: &std::path::Path) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (i, m) in model.parsed.measures.iter().enumerate() {
        if m.aggregation == "Sum" && is_ratio_name(&m.name) {
            // Check if there's a rule targeting this measure whose body is
            // a Div with a Ref denominator — if so, suggest that ref as
            // weight_measure.
            let suggestion = derive_weight_suggestion(model, &m.name);
            let message = match find_rule_body_formula(model, &m.name) {
                Some(formula) => format!(
                    "Measure '{}' has rule body '{}' but uses aggregation Sum",
                    m.name, formula
                ),
                None => format!(
                    "Measure '{}' is named like a ratio but uses Sum aggregation",
                    m.name
                ),
            };
            out.push(Diagnostic {
                code: "MC3007",
                severity: Severity::Warning,
                path: ModelPath::new(
                    file,
                    format!("/measures/{i}/aggregation"),
                    format!("measures.{}.aggregation", m.name),
                ),
                message,
                suggestion: Some(suggestion),
            });
        }
    }
    out
}

/// If a rule targets `measure_name` and its body is a Div where the
/// denominator is a Ref, return the denominator measure name as a
/// specific weight_measure suggestion.
fn derive_weight_suggestion(model: &ValidatedModel, measure_name: &str) -> String {
    if let Some(denominator) = find_div_denominator_ref(model, measure_name) {
        format!(
            "Use aggregation: WeightedAverage, weight_measure: \"{denominator}\" \
             (the denominator of the division)"
        )
    } else {
        "Ratios should typically use WeightedAverage (e.g., CPC weighted by Spend); \
         Sum produces meaningless values when consolidated. Verify the aggregation rule \
         matches the measure's intent — this lint is heuristic and may produce false \
         positives."
            .into()
    }
}

/// If a rule targets `measure_name` and its body root is a `Div` with a
/// `Ref` as the second argument (denominator), return that ref's measure
/// name.
fn find_div_denominator_ref(model: &ValidatedModel, measure_name: &str) -> Option<String> {
    for r in &model.rules {
        if r.target_measure == measure_name {
            if let ParsedRuleBody::Div(ref div_body) = r.body {
                if div_body.div.len() == 2 {
                    if let ParsedRuleBody::Ref(ref ref_body) = div_body.div[1] {
                        return Some(ref_body.measure.clone());
                    }
                }
            }
        }
    }
    None
}

/// If a rule targets `measure_name`, render a human-readable formula
/// string from its body (best-effort, used for the enhanced message).
fn find_rule_body_formula(model: &ValidatedModel, measure_name: &str) -> Option<String> {
    for r in &model.rules {
        if r.target_measure == measure_name {
            if let ParsedRuleBody::Div(ref div_body) = r.body {
                if div_body.div.len() == 2 {
                    let lhs = body_to_formula(&div_body.div[0]);
                    let rhs = body_to_formula(&div_body.div[1]);
                    return Some(format!("{lhs} / {rhs}"));
                }
            }
        }
    }
    None
}

/// Best-effort formula rendering from a ParsedRuleBody node.
fn body_to_formula(body: &ParsedRuleBody) -> String {
    crate::formula::serialize(body)
}

/// Per handoff §B: case-insensitive match on `*_rate`, `*_ratio`, `*_pct`,
/// or exact-equal to one of the named ratio measures.
fn is_ratio_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with("_rate")
        || lower.ends_with("_ratio")
        || lower.ends_with("_pct")
        || matches!(lower.as_str(), "cpc" | "cvr" | "aov" | "cpa" | "roas")
}

// ---------------------------------------------------------------------------
// MC3009 — unused input measure
// MC3010 — unused derived measure
//
// "Unused" = not referenced by any rule body (Ref nodes), not declared
// as the target of any rule (irrelevant for inputs since input-with-rule
// is a validator error MC2007), not the weight_measure of any other
// measure, and not present in any golden_test coord.
// ---------------------------------------------------------------------------

fn mc3009_unused_input_measure(model: &ValidatedModel, file: &std::path::Path) -> Vec<Diagnostic> {
    unused_measures(model, file, "Input", "MC3009", "input")
}

fn mc3010_unused_derived_measure(
    model: &ValidatedModel,
    file: &std::path::Path,
) -> Vec<Diagnostic> {
    unused_measures(model, file, "Derived", "MC3010", "derived")
}

fn unused_measures(
    model: &ValidatedModel,
    file: &std::path::Path,
    role_filter: &str,
    code: &'static str,
    role_label: &str,
) -> Vec<Diagnostic> {
    let referenced = collect_referenced_measures(model);
    let mut out = Vec::new();
    for (i, m) in model.parsed.measures.iter().enumerate() {
        if m.role != role_filter {
            continue;
        }
        if referenced.contains(m.name.as_str()) {
            continue;
        }
        let measure_dim_name = model
            .parsed
            .dimensions
            .get(model.measure_dim_index)
            .map(|d| d.name.as_str())
            .unwrap_or("Measure");
        let in_golden = model.parsed.golden_tests.iter().any(|g| {
            g.coord
                .get(measure_dim_name)
                .map(|name| name == &m.name)
                .unwrap_or(false)
        });
        if in_golden {
            continue;
        }
        out.push(Diagnostic {
            code,
            severity: Severity::Info,
            path: ModelPath::new(
                file,
                format!("/measures/{i}"),
                format!("measures.{}", m.name),
            ),
            message: format!("Unused {} measure '{}'", role_label, m.name),
            suggestion: Some(
                "If intentional, add a description noting it's a placeholder for future use; \
                 otherwise consider removing"
                    .into(),
            ),
        });
    }
    out
}

/// Returns the set of measure names actually *consumed* by other model
/// elements:
///
/// - Any rule body (`Ref` nodes — recursive)
/// - Any other measure's `weight_measure`
///
/// **Note on `target_measure`:** a rule's target is what the rule
/// *produces*, not what it consumes. Including the target here would
/// short-circuit MC3010 (every derived measure is trivially the target of
/// its own rule), which contradicts the rule's intent ("derived measure
/// not referenced by any other rule and not present in any golden test").
/// So target_measure is deliberately excluded.
fn collect_referenced_measures(model: &ValidatedModel) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for r in &model.rules {
        let mut refs = BTreeSet::new();
        collect_body_refs(&r.body, &mut refs);
        out.extend(refs);
    }
    for m in &model.parsed.measures {
        if let Some(w) = &m.weight_measure {
            out.insert(w.clone());
        }
    }
    out
}

// ---------------------------------------------------------------------------
// MC3011 — hierarchy root ambiguity
//
// A "root" in a default hierarchy is a node that appears as a `parent`
// of some edge but never as a `child`. A well-formed hierarchy has
// exactly one such node (USA, All_Channels, FY_2026). Multiple roots
// usually indicate missing edges or an unintended structural shape.
// ---------------------------------------------------------------------------

fn mc3011_hierarchy_root_ambiguity(
    model: &ValidatedModel,
    file: &std::path::Path,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (dim_idx, dim) in model.parsed.dimensions.iter().enumerate() {
        let Some(default_hier) = default_hierarchy_for(dim, &model.parsed.hierarchies) else {
            continue;
        };
        if default_hier.edges.is_empty() {
            continue;
        }
        let mut parents: BTreeSet<&str> = BTreeSet::new();
        let mut children: BTreeSet<&str> = BTreeSet::new();
        for edge in &default_hier.edges {
            parents.insert(edge.parent.as_str());
            children.insert(edge.child.as_str());
        }
        let roots: Vec<&str> = parents.difference(&children).copied().collect();
        if roots.len() > 1 {
            // Find the YAML pointer of the hierarchy in the parsed model
            // for accurate model_path reporting.
            let hier_yaml_idx = model
                .parsed
                .hierarchies
                .iter()
                .position(|h| h.name == default_hier.name && h.dimension == dim.name);
            let yaml_pointer = match hier_yaml_idx {
                Some(idx) => format!("/hierarchies/{idx}"),
                None => format!("/dimensions/{dim_idx}"),
            };
            let mut roots_sorted = roots.clone();
            roots_sorted.sort_unstable();
            out.push(Diagnostic {
                code: "MC3011",
                severity: Severity::Warning,
                path: ModelPath::new(
                    file,
                    yaml_pointer,
                    format!("hierarchies.{}", default_hier.name),
                ),
                message: format!(
                    "Default hierarchy '{}' on dim '{}' has {} roots ({})",
                    default_hier.name,
                    dim.name,
                    roots.len(),
                    roots_sorted.join(", ")
                ),
                suggestion: Some(
                    "A hierarchy should typically have exactly one root (e.g., USA, All_Channels, \
                     FY). Multiple roots usually indicate missing edges or an unintended structural \
                     shape"
                        .into(),
                ),
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// MC3016 — time elements not in chronological order
//
// Fires when adjacent Time elements (that have period_start metadata) are
// not in ascending chronological order. Advisory only — some models may
// intentionally list elements in non-chronological order.
// ---------------------------------------------------------------------------

fn mc3016_time_chronological_order(
    model: &ValidatedModel,
    file: &std::path::Path,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (dim_idx, dim) in model.parsed.dimensions.iter().enumerate() {
        if dim.kind != "Time" {
            continue;
        }
        // Collect elements that have period_start, preserving declaration order
        let with_start: Vec<(usize, &str, &str)> = dim
            .elements
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                e.period_start
                    .as_ref()
                    .map(|ps| (i, e.name.as_str(), ps.as_str()))
            })
            .collect();

        for pair in with_start.windows(2) {
            let (_, name_a, start_a) = pair[0];
            let (elem_idx_b, name_b, start_b) = pair[1];
            // Lexicographic comparison works for ISO 8601 date strings
            if start_a > start_b {
                out.push(Diagnostic {
                    code: "MC3016",
                    severity: Severity::Warning,
                    path: ModelPath::new(
                        file,
                        format!("/dimensions/{dim_idx}/elements/{elem_idx_b}"),
                        format!("dimensions.{}.elements.{}", dim.name, name_b),
                    ),
                    message: format!(
                        "Time element '{}' (period_start: {}) appears after '{}' (period_start: {}) \
                         but is chronologically earlier (MC3016)",
                        name_b, start_b, name_a, start_a
                    ),
                    suggestion: Some(
                        "Reorder Time elements in chronological order for clarity".into(),
                    ),
                });
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// MC3017 — stale fitted_model metadata.fitted_at (> 6 months old)
// MC3018 — stale calibration_map metadata.fitted_at (> 6 months old)
// ---------------------------------------------------------------------------

fn mc3017_stale_fitted_model(model: &ValidatedModel, file: &std::path::Path) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (i, fm) in model.parsed.fitted_models.iter().enumerate() {
        if let Some(ref meta) = fm.metadata {
            if let Some(ref fitted_at) = meta.fitted_at {
                if is_stale_date(fitted_at) {
                    out.push(Diagnostic {
                        code: "MC3017",
                        severity: Severity::Warning,
                        path: ModelPath::new(
                            file,
                            format!("/fitted_models/{i}"),
                            format!("fitted_models.{}", fm.name),
                        ),
                        message: format!(
                            "Fitted model '{}' has fitted_at '{}' which is > 6 months old",
                            fm.name, fitted_at
                        ),
                        suggestion: Some(
                            "Consider retraining the model with recent data to avoid concept drift"
                                .into(),
                        ),
                    });
                }
            }
        }
    }
    out
}

fn mc3018_stale_calibration_map(model: &ValidatedModel, file: &std::path::Path) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (i, cm) in model.parsed.calibration_maps.iter().enumerate() {
        if let Some(ref meta) = cm.metadata {
            if let Some(ref fitted_at) = meta.fitted_at {
                if is_stale_date(fitted_at) {
                    out.push(Diagnostic {
                        code: "MC3018",
                        severity: Severity::Warning,
                        path: ModelPath::new(
                            file,
                            format!("/calibration_maps/{i}"),
                            format!("calibration_maps.{}", cm.name),
                        ),
                        message: format!(
                            "Calibration map '{}' has fitted_at '{}' which is > 6 months old",
                            cm.name, fitted_at
                        ),
                        suggestion: Some(
                            "Consider recalibrating with recent data to maintain accuracy".into(),
                        ),
                    });
                }
            }
        }
    }
    out
}

/// Check if a date string (ISO 8601 prefix, e.g. "2025-10-01T...") is
/// older than ~6 months. Simple heuristic: compare the first 10 chars
/// against a "6 months ago" threshold computed from today (2026-05-05).
fn is_stale_date(date_str: &str) -> bool {
    if date_str.len() < 10 {
        return false;
    }
    // ~6 months ago from today
    let threshold = "2025-11-05";
    &date_str[..10] < threshold
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Treat `Some("")` and `Some(whitespace-only)` the same as `None` — a
/// blank description is no description.
fn has_text(s: &Option<String>) -> bool {
    s.as_ref().map(|v| !v.trim().is_empty()).unwrap_or(false)
}

/// Pick the default hierarchy for a dimension. Mirrors the compile stage's
/// rule: explicit `default: true` flag wins, otherwise first-declared.
fn default_hierarchy_for<'a>(
    dim: &ParsedDimension,
    hierarchies: &'a [ParsedHierarchy],
) -> Option<&'a ParsedHierarchy> {
    let candidates: Vec<&ParsedHierarchy> = hierarchies
        .iter()
        .filter(|h| h.dimension == dim.name)
        .collect();
    if candidates.is_empty() {
        return None;
    }
    candidates
        .iter()
        .copied()
        .find(|h| h.default == Some(true))
        .or_else(|| candidates.first().copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratio_name_matches_expected_set() {
        assert!(is_ratio_name("CPC"));
        assert!(is_ratio_name("cvr"));
        assert!(is_ratio_name("Close_Rate"));
        assert!(is_ratio_name("foo_ratio"));
        assert!(is_ratio_name("BAR_PCT"));
        assert!(is_ratio_name("aov"));
        assert!(is_ratio_name("ROAS"));
        assert!(!is_ratio_name("Spend"));
        assert!(!is_ratio_name("Customers"));
        assert!(!is_ratio_name("ratebook"));
    }

    #[test]
    fn has_text_treats_blank_as_missing() {
        assert!(!has_text(&None));
        assert!(!has_text(&Some(String::new())));
        assert!(!has_text(&Some("   ".into())));
        assert!(has_text(&Some("real text".into())));
    }
}
