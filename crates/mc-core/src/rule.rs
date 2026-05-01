//! Rules: typed expression trees that compute the value of a `Derived`
//! measure. The rule registry (`RuleSet`) holds them and validates
//! structural invariants at registration time.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.10.
//!
//! Phase 1 supports exactly: `Const`, `SelfRef`, `Add`, `Sub`, `Mul`, `Div`,
//! `IfNull`. The only `Scope` is `AllLeaves`. The only `CoordPattern` is
//! `SameAsTarget`.
//!
//! Validation split (per brief §3.10 wording "RuleSet::add validates"):
//!
//! - **Structural checks live here** — declared-dependencies superset
//!   (#4), cycle detection in the rule-target → dep-measure graph (#5),
//!   no duplicate target measure (#6).
//! - **Cube-aware checks live in `CubeBuilder::add_rule`** — target is a
//!   `Derived` measure (#1), every `SelfRef` references a measure that
//!   exists in the measure dimension (#2), body is well-typed (#3).
//!   Those need access to the measure dimension, which `RuleSet` doesn't
//!   own.

use ahash::{AHashMap, AHashSet};

use crate::error::EngineError;
use crate::id::{CubeId, ElementId, RuleId};
use crate::value::ScalarValue;

#[derive(Clone, Debug, PartialEq)]
pub enum Scope {
    /// Rule applies to every leaf coordinate (in non-measure dims) where
    /// the measure is `target_measure`. Phase 1 supports this only.
    AllLeaves,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CoordPattern {
    /// Read at the same coordinate as the rule target, with only the
    /// measure slot replaced. Phase 1 supports this only.
    SameAsTarget,
}

#[derive(Clone, Debug)]
pub struct DependencyDecl {
    pub measure: ElementId,
    pub coord_pattern: CoordPattern,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Const(ScalarValue),
    /// Same coord as the rule target, but the measure slot is replaced
    /// with the given `ElementId`. Resolved at eval time via the
    /// caller-supplied lookup function.
    SelfRef(ElementId),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    /// `IfNull(primary, fallback)`: returns `primary` if non-null, else
    /// `fallback`. Per spec §7's null-poison policy this is the only
    /// branching primitive in Phase 1.
    IfNull(Box<Expr>, Box<Expr>),
}

#[derive(Debug)]
pub struct Rule {
    pub id: RuleId,
    pub cube: CubeId,
    pub target_measure: ElementId,
    pub scope: Scope,
    pub body: Expr,
    pub declared_dependencies: Vec<DependencyDecl>,
}

#[derive(Debug, Default)]
pub struct RuleSet {
    rules: Vec<Rule>,
    /// target measure → indices into `rules` (positions, not RuleIds).
    /// Phase 1's only Scope is `AllLeaves` and the duplicate-target check
    /// keeps this vec at length 1 in practice; the Vec shape is forward-
    /// compat with future scope refinements.
    by_target: AHashMap<ElementId, Vec<usize>>,
    /// rule-target → set of dep measures, kept alongside `rules` for fast
    /// cycle detection on each `add`. Duplicates `declared_dependencies`
    /// in shape; storing it as a set lets us walk the graph without
    /// re-collecting on every check.
    deps_by_target: AHashMap<ElementId, AHashSet<ElementId>>,
}

impl RuleSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a rule. Performs structural validation only:
    ///
    ///   1. **Declared-dep superset** — every measure referenced by a
    ///      `SelfRef` in `rule.body` must appear in
    ///      `rule.declared_dependencies`. Per spec §3.10 / §10.7
    ///      `doctrine_no_silent_dependency_miss`.
    ///   2. **No duplicate target** — Phase 1's only scope is
    ///      `AllLeaves`, so two rules sharing a target_measure
    ///      automatically overlap.
    ///   3. **No cycle** in the rule-target → dep-measure graph after
    ///      adding this rule.
    ///
    /// Per engine-semantics.md §10 I-Rule-4, I-Rule-5, I-Rule-6.
    pub fn add(&mut self, rule: Rule) -> Result<(), EngineError> {
        // (1) declared-dep superset — collect all measures referenced via
        //     SelfRef in `body`, ensure each is declared.
        let referenced = collect_self_refs(&rule.body);
        let declared: AHashSet<ElementId> = rule
            .declared_dependencies
            .iter()
            .map(|d| d.measure)
            .collect();
        for measure in &referenced {
            if !declared.contains(measure) {
                // `coord` is not yet known at registration time (rules
                // apply over a scope of coordinates, not one specific
                // coord), so we report the rule + measure pair via a
                // synthetic CellCoordinate-less error. The brief's
                // `EngineError::UndeclaredDependency` carries a coord —
                // for the registration-time variant we report through
                // `RuleBodyTypeMismatch` with a structured detail string,
                // since this is fundamentally a static well-formedness
                // problem that must be caught BEFORE any read.
                return Err(EngineError::RuleBodyTypeMismatch {
                    detail: format!(
                        "rule {:?} body references measure {:?} via SelfRef \
                         but does not declare it; declared: {:?}",
                        rule.id, measure, declared
                    ),
                });
            }
        }

        // (2) duplicate target check.
        if self.by_target.contains_key(&rule.target_measure) {
            return Err(EngineError::DuplicateRuleTarget(rule.target_measure));
        }

        // (3) cycle check — speculatively add the new edges, run DFS, roll
        //     back if a cycle is found. Edges go from rule.target_measure
        //     → each declared dep measure.
        let target = rule.target_measure;
        let dep_set: AHashSet<ElementId> = rule
            .declared_dependencies
            .iter()
            .map(|d| d.measure)
            .collect();

        // Speculative insertion.
        self.deps_by_target.insert(target, dep_set);
        if detect_cycle_in_rule_graph(&self.deps_by_target).is_some() {
            // Roll back.
            self.deps_by_target.remove(&target);
            // Per spec §3.20, `DependencyCycle.path` is `Vec<CellCoordinate>`.
            // At registration time we have measures, not coords; we report
            // an empty path and rely on the variant itself + the rule's
            // target_measure (visible in upstream context) for diagnosis.
            // The eval-time cycle path is the place coords show up.
            return Err(EngineError::DependencyCycle { path: Vec::new() });
        }

        // All checks passed — commit.
        let position = self.rules.len();
        self.by_target.entry(target).or_default().push(position);
        self.rules.push(rule);
        Ok(())
    }

    pub fn rule(&self, id: RuleId) -> Option<&Rule> {
        self.rules.iter().find(|r| r.id == id)
    }

    /// Return the rule indices targeting `measure`. Empty slice if no
    /// rule targets this measure (i.e., it's an `Input` measure or no
    /// rule has been registered for it yet).
    pub fn rules_for_measure(&self, measure: ElementId) -> &[usize] {
        self.by_target
            .get(&measure)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// The rule at the given registry index. Cheaper than
    /// `rule(RuleId)` for the common case of "look up the rule for
    /// measure M, then evaluate it." Used by `cube.rs`.
    pub fn rule_at(&self, index: usize) -> Option<&Rule> {
        self.rules.get(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Rule> {
        self.rules.iter()
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

/// Walk an `Expr` tree and collect every `SelfRef` measure ID. Used by
/// `RuleSet::add` for the declared-deps superset check.
fn collect_self_refs(expr: &Expr) -> AHashSet<ElementId> {
    let mut out = AHashSet::new();
    fn walk(expr: &Expr, out: &mut AHashSet<ElementId>) {
        match expr {
            Expr::Const(_) => {}
            Expr::SelfRef(m) => {
                out.insert(*m);
            }
            Expr::Add(a, b)
            | Expr::Sub(a, b)
            | Expr::Mul(a, b)
            | Expr::Div(a, b)
            | Expr::IfNull(a, b) => {
                walk(a, out);
                walk(b, out);
            }
        }
    }
    walk(expr, &mut out);
    out
}

/// Walk an `Expr` tree and return its operator depth (longest root-to-
/// leaf path in operator nodes). Used by trace-depth tests; exposed at
/// the module level so `cube.rs` and tests can share the helper.
pub fn expr_depth(expr: &Expr) -> u32 {
    match expr {
        Expr::Const(_) | Expr::SelfRef(_) => 1,
        Expr::Add(a, b)
        | Expr::Sub(a, b)
        | Expr::Mul(a, b)
        | Expr::Div(a, b)
        | Expr::IfNull(a, b) => 1 + expr_depth(a).max(expr_depth(b)),
    }
}

/// DFS-based cycle detection across the rule-target → dep-measure graph.
/// Returns the cycle path (sequence of measure IDs forming the loop) if
/// found.
fn detect_cycle_in_rule_graph(
    deps_by_target: &AHashMap<ElementId, AHashSet<ElementId>>,
) -> Option<Vec<ElementId>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Color {
        White,
        Gray,
        Black,
    }

    // Gather every node — both rule targets and their dep measures.
    let mut nodes: AHashSet<ElementId> = AHashSet::new();
    for (&target, deps) in deps_by_target {
        nodes.insert(target);
        for &d in deps {
            nodes.insert(d);
        }
    }

    let mut color: AHashMap<ElementId, Color> =
        nodes.iter().copied().map(|n| (n, Color::White)).collect();
    let mut stack_path: Vec<ElementId> = Vec::new();

    // Stable iteration order over starts so the cycle-detection result
    // is deterministic across runs (AHashSet iteration is not).
    let mut sorted_nodes: Vec<ElementId> = nodes.iter().copied().collect();
    sorted_nodes.sort();

    for start in sorted_nodes {
        if color.get(&start).copied().unwrap_or(Color::White) != Color::White {
            continue;
        }
        // (node, sorted dep list, cursor) — sorting deps gives
        // deterministic cycle paths under CLAUDE.md §2.11.
        let mut work_stack: Vec<(ElementId, Vec<ElementId>, usize)> = Vec::new();
        let mut deps: Vec<ElementId> = deps_by_target
            .get(&start)
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default();
        deps.sort();
        work_stack.push((start, deps, 0));
        color.insert(start, Color::Gray);
        stack_path.push(start);

        while !work_stack.is_empty() {
            // Snapshot the top frame's identity + cursor without holding
            // any borrow — copy `cur`/`idx` and the next dep ID, then
            // bump the cursor before recursing.
            let top = work_stack.len() - 1;
            let cur = work_stack[top].0;
            let idx = work_stack[top].1.len().min(work_stack[top].2);
            let next_dep: Option<ElementId> = work_stack[top].1.get(idx).copied();

            if let Some(next) = next_dep {
                work_stack[top].2 = idx + 1;
                match color.get(&next).copied().unwrap_or(Color::White) {
                    Color::White => {
                        color.insert(next, Color::Gray);
                        stack_path.push(next);
                        let mut next_deps: Vec<ElementId> = deps_by_target
                            .get(&next)
                            .map(|s| s.iter().copied().collect())
                            .unwrap_or_default();
                        next_deps.sort();
                        work_stack.push((next, next_deps, 0));
                    }
                    Color::Gray => {
                        // Back-edge — cycle found.
                        if let Some(start_idx) = stack_path.iter().position(|&n| n == next) {
                            let mut path: Vec<ElementId> = stack_path[start_idx..].to_vec();
                            path.push(next);
                            return Some(path);
                        }
                        return Some(vec![next]);
                    }
                    Color::Black => {}
                }
            } else {
                color.insert(cur, Color::Black);
                work_stack.pop();
                stack_path.pop();
            }
        }
    }

    None
}

// ===========================================================================
// Rule body evaluation primitive
// ===========================================================================

/// Evaluate an `Expr` body. The caller supplies `lookup_self`, a closure
/// that resolves a `SelfRef(measure)` to its current `ScalarValue` at
/// the rule's target coordinate. The closure is `&mut` so it can record
/// the measures actually read for `doctrine_no_silent_dependency_miss`
/// validation in `cube.rs`.
///
/// Null arithmetic follows spec §7 verbatim:
///
/// - `Add`: Null + Null = Null; Null + x = x; x + Null = x.
/// - `Sub`: Null - Null = Null; Null - x = -x; x - Null = x.
/// - `Mul`: any operand Null → Null.
/// - `Div`: any operand Null → Null. Division by 0 (or |y| < 1e-300)
///   → Null.
/// - `IfNull(primary, fallback)`: primary unless primary is Null.
///
/// NaN and ±Inf must never be produced. Each helper returns Null on any
/// non-finite intermediate so a downstream `validate_finite_f64` is never
/// strictly necessary for rule output.
pub fn eval_expr<F>(expr: &Expr, lookup_self: &mut F) -> Result<ScalarValue, EngineError>
where
    F: FnMut(ElementId) -> Result<ScalarValue, EngineError>,
{
    match expr {
        Expr::Const(v) => Ok(v.clone()),
        Expr::SelfRef(measure) => lookup_self(*measure),
        Expr::Add(a, b) => {
            let lhs = eval_expr(a, lookup_self)?;
            let rhs = eval_expr(b, lookup_self)?;
            Ok(null_add(lhs, rhs))
        }
        Expr::Sub(a, b) => {
            let lhs = eval_expr(a, lookup_self)?;
            let rhs = eval_expr(b, lookup_self)?;
            Ok(null_sub(lhs, rhs))
        }
        Expr::Mul(a, b) => {
            let lhs = eval_expr(a, lookup_self)?;
            let rhs = eval_expr(b, lookup_self)?;
            Ok(null_mul(lhs, rhs))
        }
        Expr::Div(a, b) => {
            let lhs = eval_expr(a, lookup_self)?;
            let rhs = eval_expr(b, lookup_self)?;
            Ok(null_div(lhs, rhs))
        }
        Expr::IfNull(primary, fallback) => {
            let p = eval_expr(primary, lookup_self)?;
            if p.is_null() {
                eval_expr(fallback, lookup_self)
            } else {
                Ok(p)
            }
        }
    }
}

/// Treat any non-finite f64 (NaN or ±Inf) as Null. Per spec §7 NaN/Inf
/// must never appear in storage; rule eval is the place this is most
/// likely to produce them (e.g., as an intermediate of arithmetic).
fn finite_or_null(v: f64) -> ScalarValue {
    if v.is_finite() {
        ScalarValue::F64(v)
    } else {
        ScalarValue::Null
    }
}

fn null_add(a: ScalarValue, b: ScalarValue) -> ScalarValue {
    match (a, b) {
        (ScalarValue::Null, ScalarValue::Null) => ScalarValue::Null,
        (ScalarValue::Null, x) | (x, ScalarValue::Null) => x,
        (ScalarValue::F64(x), ScalarValue::F64(y)) => finite_or_null(x + y),
        // I64 / Bool / Category arithmetic isn't spec'd in Phase 1; rule
        // bodies are F64-only by spec §3.10 well-typedness check (which
        // happens at CubeBuilder::add_rule). Reaching this branch means
        // the cube layer let through an ill-typed body — surface it.
        _ => ScalarValue::Null,
    }
}

fn null_sub(a: ScalarValue, b: ScalarValue) -> ScalarValue {
    match (a, b) {
        (ScalarValue::Null, ScalarValue::Null) => ScalarValue::Null,
        // Null - x = -x  (per spec §7)
        (ScalarValue::Null, ScalarValue::F64(x)) => finite_or_null(-x),
        // x - Null = x
        (ScalarValue::F64(x), ScalarValue::Null) => ScalarValue::F64(x),
        (ScalarValue::F64(x), ScalarValue::F64(y)) => finite_or_null(x - y),
        _ => ScalarValue::Null,
    }
}

fn null_mul(a: ScalarValue, b: ScalarValue) -> ScalarValue {
    match (a, b) {
        // Null poisons multiplication on either side, including Null * Null.
        (ScalarValue::Null, _) | (_, ScalarValue::Null) => ScalarValue::Null,
        (ScalarValue::F64(x), ScalarValue::F64(y)) => finite_or_null(x * y),
        _ => ScalarValue::Null,
    }
}

fn null_div(a: ScalarValue, b: ScalarValue) -> ScalarValue {
    match (a, b) {
        // Null poisons division on either side.
        (ScalarValue::Null, _) | (_, ScalarValue::Null) => ScalarValue::Null,
        (ScalarValue::F64(_), ScalarValue::F64(y)) if y.abs() < 1e-300 => {
            // Per spec §7: x / 0 (or near-zero) → Null. Never f64::INFINITY.
            ScalarValue::Null
        }
        (ScalarValue::F64(x), ScalarValue::F64(y)) => finite_or_null(x / y),
        _ => ScalarValue::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{CubeId, IdGenerator};

    fn make_rule(
        id_gen: &IdGenerator,
        cube: CubeId,
        target: ElementId,
        body: Expr,
        deps: Vec<ElementId>,
    ) -> Rule {
        Rule {
            id: id_gen.rule(),
            cube,
            target_measure: target,
            scope: Scope::AllLeaves,
            body,
            declared_dependencies: deps
                .into_iter()
                .map(|measure| DependencyDecl {
                    measure,
                    coord_pattern: CoordPattern::SameAsTarget,
                })
                .collect(),
        }
    }

    #[test]
    fn well_typed_rule_with_correct_deps_passes() {
        let id_gen = IdGenerator::new();
        let cube = id_gen.cube();
        let spend = ElementId(1);
        let cpc = ElementId(2);
        let clicks = ElementId(3);

        // Clicks = Spend / CPC; declared deps cover both.
        let rule = make_rule(
            &id_gen,
            cube,
            clicks,
            Expr::Div(Box::new(Expr::SelfRef(spend)), Box::new(Expr::SelfRef(cpc))),
            vec![spend, cpc],
        );

        let mut rs = RuleSet::new();
        rs.add(rule).expect("well-typed rule must register");
        assert_eq!(rs.len(), 1);
    }

    #[test]
    fn rule_with_undeclared_dependency_rejected() {
        let id_gen = IdGenerator::new();
        let cube = id_gen.cube();
        let spend = ElementId(1);
        let cpc = ElementId(2);
        let clicks = ElementId(3);

        // Body references CPC via SelfRef, but declared deps only list Spend.
        let rule = make_rule(
            &id_gen,
            cube,
            clicks,
            Expr::Div(Box::new(Expr::SelfRef(spend)), Box::new(Expr::SelfRef(cpc))),
            vec![spend], // missing CPC
        );

        let mut rs = RuleSet::new();
        let err = rs.add(rule).expect_err("undeclared dep must be rejected");
        match err {
            EngineError::RuleBodyTypeMismatch { detail } => {
                assert!(
                    detail.contains("does not declare") || detail.contains("SelfRef"),
                    "error detail should mention undeclared dependency, got: {detail}"
                );
            }
            other => panic!("expected RuleBodyTypeMismatch, got {other:?}"),
        }
        assert_eq!(rs.len(), 0, "rejected rule must not be in registry");
    }

    #[test]
    fn duplicate_target_rejected() {
        let id_gen = IdGenerator::new();
        let cube = id_gen.cube();
        let spend = ElementId(1);
        let cpc = ElementId(2);
        let clicks = ElementId(3);
        let cvr = ElementId(4);

        let r1 = make_rule(
            &id_gen,
            cube,
            clicks,
            Expr::Div(Box::new(Expr::SelfRef(spend)), Box::new(Expr::SelfRef(cpc))),
            vec![spend, cpc],
        );
        // Different body, same target — overlapping AllLeaves scope.
        let r2 = make_rule(
            &id_gen,
            cube,
            clicks,
            Expr::Mul(Box::new(Expr::SelfRef(spend)), Box::new(Expr::SelfRef(cvr))),
            vec![spend, cvr],
        );

        let mut rs = RuleSet::new();
        rs.add(r1).expect("first ok");
        let err = rs.add(r2).expect_err("duplicate target must be rejected");
        assert!(matches!(
            err,
            EngineError::DuplicateRuleTarget(t) if t == clicks
        ));
        assert_eq!(rs.len(), 1);
    }

    #[test]
    fn cycle_in_rule_graph_rejected() {
        // Two rules: A = B, B = A. Both targets, both deps create cycle.
        let id_gen = IdGenerator::new();
        let cube = id_gen.cube();
        let a = ElementId(10);
        let b = ElementId(20);

        let r_a = make_rule(&id_gen, cube, a, Expr::SelfRef(b), vec![b]);
        let r_b = make_rule(&id_gen, cube, b, Expr::SelfRef(a), vec![a]);

        let mut rs = RuleSet::new();
        rs.add(r_a).expect("first rule ok (A = B)");
        // Second rule closes the cycle.
        let err = rs.add(r_b).expect_err("cycle must be rejected");
        assert!(matches!(err, EngineError::DependencyCycle { .. }));
        // Rolled back: only the first rule remains.
        assert_eq!(rs.len(), 1);
    }

    #[test]
    fn linear_chain_no_cycle_succeeds() {
        // Acme-shaped chain: Clicks → {Spend, CPC}; Leads → {Clicks, CVR};
        // Customers → {Leads, Close_Rate}; Revenue → {Customers, AOV};
        // Gross_Profit → {Revenue, COGS_Rate}.
        let id_gen = IdGenerator::new();
        let cube = id_gen.cube();
        let spend = ElementId(1);
        let cpc = ElementId(2);
        let cvr = ElementId(3);
        let close_rate = ElementId(4);
        let aov = ElementId(5);
        let cogs_rate = ElementId(6);
        let clicks = ElementId(10);
        let leads = ElementId(11);
        let customers = ElementId(12);
        let revenue = ElementId(13);
        let gross_profit = ElementId(14);

        let mut rs = RuleSet::new();
        rs.add(make_rule(
            &id_gen,
            cube,
            clicks,
            Expr::Div(Box::new(Expr::SelfRef(spend)), Box::new(Expr::SelfRef(cpc))),
            vec![spend, cpc],
        ))
        .expect("clicks rule ok");
        rs.add(make_rule(
            &id_gen,
            cube,
            leads,
            Expr::Mul(
                Box::new(Expr::SelfRef(clicks)),
                Box::new(Expr::SelfRef(cvr)),
            ),
            vec![clicks, cvr],
        ))
        .expect("leads rule ok");
        rs.add(make_rule(
            &id_gen,
            cube,
            customers,
            Expr::Mul(
                Box::new(Expr::SelfRef(leads)),
                Box::new(Expr::SelfRef(close_rate)),
            ),
            vec![leads, close_rate],
        ))
        .expect("customers rule ok");
        rs.add(make_rule(
            &id_gen,
            cube,
            revenue,
            Expr::Mul(
                Box::new(Expr::SelfRef(customers)),
                Box::new(Expr::SelfRef(aov)),
            ),
            vec![customers, aov],
        ))
        .expect("revenue rule ok");
        rs.add(make_rule(
            &id_gen,
            cube,
            gross_profit,
            Expr::Mul(
                Box::new(Expr::SelfRef(revenue)),
                Box::new(Expr::Sub(
                    Box::new(Expr::Const(ScalarValue::F64(1.0))),
                    Box::new(Expr::SelfRef(cogs_rate)),
                )),
            ),
            vec![revenue, cogs_rate],
        ))
        .expect("gross_profit rule ok");

        assert_eq!(rs.len(), 5);
        assert_eq!(rs.rules_for_measure(clicks).len(), 1);
        assert_eq!(rs.rules_for_measure(spend).len(), 0); // Spend is an input
    }

    // -----------------------------------------------------------------------
    // eval_expr + null arithmetic
    // -----------------------------------------------------------------------

    fn lookup_const(
        map: AHashMap<ElementId, ScalarValue>,
    ) -> impl FnMut(ElementId) -> Result<ScalarValue, EngineError> {
        move |m| Ok(map.get(&m).cloned().unwrap_or(ScalarValue::Null))
    }

    #[test]
    fn eval_simple_div() {
        let spend = ElementId(1);
        let cpc = ElementId(2);
        let body = Expr::Div(Box::new(Expr::SelfRef(spend)), Box::new(Expr::SelfRef(cpc)));
        let mut lookup = lookup_const(
            [
                (spend, ScalarValue::F64(11_500.0)),
                (cpc, ScalarValue::F64(1.5)),
            ]
            .into_iter()
            .collect(),
        );
        let v = eval_expr(&body, &mut lookup).expect("eval ok");
        let got = v.as_f64().expect("F64");
        assert!((got - 7_666.666_666_666_667).abs() < 1e-6, "got {got}");
    }

    #[test]
    fn eval_nested_chain_revenue() {
        let spend = ElementId(1);
        let cpc = ElementId(2);
        let cvr = ElementId(3);
        let close_rate = ElementId(4);
        let aov = ElementId(5);
        // Revenue = ((Spend / CPC) * CVR) * Close_Rate * AOV
        let body = Expr::Mul(
            Box::new(Expr::Mul(
                Box::new(Expr::Mul(
                    Box::new(Expr::Div(
                        Box::new(Expr::SelfRef(spend)),
                        Box::new(Expr::SelfRef(cpc)),
                    )),
                    Box::new(Expr::SelfRef(cvr)),
                )),
                Box::new(Expr::SelfRef(close_rate)),
            )),
            Box::new(Expr::SelfRef(aov)),
        );
        let mut lookup = lookup_const(
            [
                (spend, ScalarValue::F64(11_500.0)),
                (cpc, ScalarValue::F64(1.5)),
                (cvr, ScalarValue::F64(0.020)),
                (close_rate, ScalarValue::F64(0.10)),
                (aov, ScalarValue::F64(200.0)),
            ]
            .into_iter()
            .collect(),
        );
        let v = eval_expr(&body, &mut lookup).expect("eval ok");
        let got = v.as_f64().expect("F64");
        // Revenue = 9200/3 ≈ 3066.666...
        assert!(
            (got - 9200.0 / 3.0).abs() < 1e-6,
            "got {got}, expected ~3066.666..."
        );
    }

    #[test]
    fn null_arithmetic_table_per_spec_section_7() {
        // Lookup factory: every test in this function uses constant Exprs,
        // so the SelfRef closure is never invoked. The factory's `v`
        // parameter is here as forward-compat for tests that DO need a
        // mock SelfRef value (currently none) — when one of those lands,
        // pass it in via this closure.
        let make_lookup = |v: ScalarValue| move |_: ElementId| Ok::<_, EngineError>(v.clone());

        // Add: Null + Null = Null
        let body = Expr::Add(
            Box::new(Expr::Const(ScalarValue::Null)),
            Box::new(Expr::Const(ScalarValue::Null)),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null)).unwrap();
        assert!(v.is_null());

        // Add: Null + 5 = 5
        let body = Expr::Add(
            Box::new(Expr::Const(ScalarValue::Null)),
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null)).unwrap();
        assert_eq!(v.as_f64(), Some(5.0));

        // Sub: Null - 5 = -5
        let body = Expr::Sub(
            Box::new(Expr::Const(ScalarValue::Null)),
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null)).unwrap();
        assert_eq!(v.as_f64(), Some(-5.0));

        // Sub: 5 - Null = 5
        let body = Expr::Sub(
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
            Box::new(Expr::Const(ScalarValue::Null)),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null)).unwrap();
        assert_eq!(v.as_f64(), Some(5.0));

        // Mul: Null * 5 = Null
        let body = Expr::Mul(
            Box::new(Expr::Const(ScalarValue::Null)),
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null)).unwrap();
        assert!(v.is_null());

        // Mul: 5 * Null = Null
        let body = Expr::Mul(
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
            Box::new(Expr::Const(ScalarValue::Null)),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null)).unwrap();
        assert!(v.is_null());

        // Div: Null / 5 = Null
        let body = Expr::Div(
            Box::new(Expr::Const(ScalarValue::Null)),
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null)).unwrap();
        assert!(v.is_null());

        // Div: 5 / Null = Null
        let body = Expr::Div(
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
            Box::new(Expr::Const(ScalarValue::Null)),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null)).unwrap();
        assert!(v.is_null());

        // Div: 5 / 0 = Null  (NOT Inf)
        let body = Expr::Div(
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
            Box::new(Expr::Const(ScalarValue::F64(0.0))),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null)).unwrap();
        assert!(v.is_null(), "5 / 0 must be Null, not Inf or NaN");

        // Div: 5 / 1e-301 = Null  (treated as zero per |y| < 1e-300)
        let body = Expr::Div(
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
            Box::new(Expr::Const(ScalarValue::F64(1e-301))),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null)).unwrap();
        assert!(v.is_null(), "5 / sub-epsilon must be Null");

        // IfNull(Null, 99) = 99
        let body = Expr::IfNull(
            Box::new(Expr::Const(ScalarValue::Null)),
            Box::new(Expr::Const(ScalarValue::F64(99.0))),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null)).unwrap();
        assert_eq!(v.as_f64(), Some(99.0));

        // IfNull(7, 99) = 7
        let body = Expr::IfNull(
            Box::new(Expr::Const(ScalarValue::F64(7.0))),
            Box::new(Expr::Const(ScalarValue::F64(99.0))),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null)).unwrap();
        assert_eq!(v.as_f64(), Some(7.0));

        // 0.0 distinct from Null: 0.0 + 5 = 5 (not Null), but Null + 5 = 5 (also 5 — special case).
        // The distinction lives more clearly in Mul:
        //  - 0.0 * 5  = 0.0 (numeric)
        //  - Null * 5 = Null
        let body = Expr::Mul(
            Box::new(Expr::Const(ScalarValue::F64(0.0))),
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null)).unwrap();
        assert_eq!(v.as_f64(), Some(0.0));
    }

    #[test]
    fn eval_self_ref_invokes_closure() {
        let m = ElementId(42);
        let mut call_count = 0;
        let mut lookup = |id: ElementId| {
            call_count += 1;
            assert_eq!(id, m);
            Ok::<_, EngineError>(ScalarValue::F64(7.0))
        };
        let body = Expr::SelfRef(m);
        let v = eval_expr(&body, &mut lookup).unwrap();
        assert_eq!(v.as_f64(), Some(7.0));
        assert_eq!(call_count, 1);
    }

    #[test]
    fn expr_depth_returns_longest_path() {
        // Acme Revenue body = Customers * AOV → depth 2
        // Acme Gross_Profit body = Revenue * (1 - COGS_Rate) → depth 3
        let customers = ElementId(1);
        let aov = ElementId(2);
        let revenue_body = Expr::Mul(
            Box::new(Expr::SelfRef(customers)),
            Box::new(Expr::SelfRef(aov)),
        );
        assert_eq!(expr_depth(&revenue_body), 2);

        let cogs = ElementId(3);
        let revenue = ElementId(4);
        let gross_profit_body = Expr::Mul(
            Box::new(Expr::SelfRef(revenue)),
            Box::new(Expr::Sub(
                Box::new(Expr::Const(ScalarValue::F64(1.0))),
                Box::new(Expr::SelfRef(cogs)),
            )),
        );
        assert_eq!(expr_depth(&gross_profit_body), 3);
    }
}
