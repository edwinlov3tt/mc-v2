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
    /// `fallback`.
    IfNull(Box<Expr>, Box<Expr>),

    // -- Phase 3E: Comparisons --
    Gt(Box<Expr>, Box<Expr>),
    Lt(Box<Expr>, Box<Expr>),
    Gte(Box<Expr>, Box<Expr>),
    Lte(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Neq(Box<Expr>, Box<Expr>),

    // -- Phase 3E: Logical --
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),

    // -- Phase 3E: Functions --
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Min(Vec<Box<Expr>>),
    Max(Vec<Box<Expr>>),
    Abs(Box<Expr>),
    SafeDiv(Box<Expr>, Box<Expr>, Box<Expr>),
    Clamp(Box<Expr>, Box<Expr>, Box<Expr>),
    Coalesce(Vec<Box<Expr>>),
    ActualRef(ElementId),

    // -- Phase 3F: Time-series --
    Prev(ElementId),
    Lag(ElementId, Box<Expr>),
    Cumulative(ElementId),
    RollingAvg(ElementId, Box<Expr>),
    PeriodIndex,

    // -- Phase 3F.1: Anchor functions --
    AnchorIndex,
    IsPast,
    IsCurrent,
    IsFuture,
    PeriodsSinceAnchor,
    PeriodsToEnd,

    // -- Phase 3G: Reference-data --
    Benchmark(String, Box<Expr>),
    /// `lookup(name, key1, key2, ...)` — Phase 3I item 3 extended this to
    /// hold a Vec of key expressions. Single-key callers pass a 1-element
    /// vec; multi-key callers pass N elements joined with `|` at eval.
    Lookup(String, Vec<Box<Expr>>),
    Bucket(Box<Expr>, String),
    SumOver(crate::id::DimensionId, ElementId),
    /// Resolves to the name of the current coordinate's element in the
    /// given dimension. Used as a key expression in `lookup()`/`benchmark()`.
    DimElement(crate::id::DimensionId),

    // -- Phase 3H: Fitted-model evaluation --
    /// `predict("model_name", feature1, feature2, ...)`
    Predict(String, Vec<Box<Expr>>),
    /// `calibrate(value, "map_name")`
    Calibrate(Box<Expr>, String),
    /// `exp(x)` — Euler's number raised to the power of x
    Exp(Box<Expr>),
    /// `norm_cdf(x, mu, sigma)` — normal distribution CDF
    NormCdf(Box<Expr>, Box<Expr>, Box<Expr>),

    // -- Phase 3I: Math primitives --
    /// `pow(base, exponent)`
    Pow(Box<Expr>, Box<Expr>),
    /// `sqrt(x)`
    Sqrt(Box<Expr>),
    /// `ln(x)`
    Ln(Box<Expr>),
    /// `log10(x)`
    Log10(Box<Expr>),
    /// `round(x)` — banker's rounding (half-to-even).
    Round(Box<Expr>),
    /// `floor(x)`
    Floor(Box<Expr>),
    /// `ceil(x)`
    Ceil(Box<Expr>),
    /// `mod(a, b)` — Euclidean remainder. Null when b ≈ 0.
    Mod(Box<Expr>, Box<Expr>),
    /// `norm_inv(p, mu, sigma)` — inverse normal CDF (Beasley-Springer-Moro).
    NormInv(Box<Expr>, Box<Expr>, Box<Expr>),

    // -- Phase 3I: is_element narrow numeric form --
    /// `is_element(DimensionId, ElementId)` — returns 1.0 if the current
    /// coordinate's element in `DimensionId` equals `ElementId`, else 0.0.
    IsElement(crate::id::DimensionId, ElementId),

    // -- Phase 3I: cross-coord scans (avg/min/max/wavg over a dimension) --
    /// `avg_over(measure, dim)` — mean across leaf elements of `dim`,
    /// skipping Nulls. Empty/all-null → Null.
    AvgOver(crate::id::DimensionId, ElementId),
    /// `min_over(measure, dim)` — minimum across leaf elements of `dim`,
    /// skipping Nulls. All-null → Null.
    MinOver(crate::id::DimensionId, ElementId),
    /// `max_over(measure, dim)` — maximum across leaf elements of `dim`,
    /// skipping Nulls. All-null → Null.
    MaxOver(crate::id::DimensionId, ElementId),
    /// `wavg_over(measure, dim, weight_measure)` — weighted average across
    /// leaf elements of `dim`. Returns `sum(value*weight)/sum(weight)`,
    /// or Null if all weights are zero / all values are null.
    WAvgOver(crate::id::DimensionId, ElementId, ElementId),
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
            Expr::Const(_)
            | Expr::PeriodIndex
            | Expr::AnchorIndex
            | Expr::IsPast
            | Expr::IsCurrent
            | Expr::IsFuture
            | Expr::PeriodsSinceAnchor
            | Expr::PeriodsToEnd => {}
            Expr::SelfRef(m) | Expr::ActualRef(m) | Expr::Prev(m) | Expr::Cumulative(m) => {
                out.insert(*m);
            }
            Expr::Add(a, b)
            | Expr::Sub(a, b)
            | Expr::Mul(a, b)
            | Expr::Div(a, b)
            | Expr::IfNull(a, b)
            | Expr::Gt(a, b)
            | Expr::Lt(a, b)
            | Expr::Gte(a, b)
            | Expr::Lte(a, b)
            | Expr::Eq(a, b)
            | Expr::Neq(a, b)
            | Expr::And(a, b)
            | Expr::Or(a, b) => {
                walk(a, out);
                walk(b, out);
            }
            Expr::Not(a) | Expr::Abs(a) => walk(a, out),
            Expr::If(a, b, c) | Expr::SafeDiv(a, b, c) | Expr::Clamp(a, b, c) => {
                walk(a, out);
                walk(b, out);
                walk(c, out);
            }
            Expr::Min(args) | Expr::Max(args) | Expr::Coalesce(args) => {
                for a in args {
                    walk(a, out);
                }
            }
            Expr::Lag(m, periods) => {
                out.insert(*m);
                walk(periods, out);
            }
            Expr::RollingAvg(m, window) => {
                out.insert(*m);
                walk(window, out);
            }
            Expr::Benchmark(_, key) => walk(key, out),
            Expr::Lookup(_, keys) => {
                for k in keys {
                    walk(k, out);
                }
            }
            Expr::Bucket(v, _) => walk(v, out),
            Expr::SumOver(_, m) => {
                out.insert(*m);
            }
            Expr::DimElement(_) => {} // no measure dependency
            // Phase 3H
            Expr::Predict(_, features) => {
                for f in features {
                    walk(f, out);
                }
            }
            Expr::Calibrate(v, _) => walk(v, out),
            Expr::Exp(a) => walk(a, out),
            Expr::NormCdf(x, mu, sigma) => {
                walk(x, out);
                walk(mu, out);
                walk(sigma, out);
            }
            // Phase 3I: math primitives
            Expr::Pow(a, b) | Expr::Mod(a, b) => {
                walk(a, out);
                walk(b, out);
            }
            Expr::Sqrt(a)
            | Expr::Ln(a)
            | Expr::Log10(a)
            | Expr::Round(a)
            | Expr::Floor(a)
            | Expr::Ceil(a) => walk(a, out),
            Expr::NormInv(p, mu, sigma) => {
                walk(p, out);
                walk(mu, out);
                walk(sigma, out);
            }
            // Phase 3I: is_element / avg_over family
            Expr::IsElement(_, _) => {}
            Expr::AvgOver(_, m) | Expr::MinOver(_, m) | Expr::MaxOver(_, m) => {
                out.insert(*m);
            }
            Expr::WAvgOver(_, value, weight) => {
                out.insert(*value);
                out.insert(*weight);
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
        Expr::Const(_)
        | Expr::SelfRef(_)
        | Expr::ActualRef(_)
        | Expr::Prev(_)
        | Expr::Cumulative(_)
        | Expr::PeriodIndex
        | Expr::AnchorIndex
        | Expr::IsPast
        | Expr::IsCurrent
        | Expr::IsFuture
        | Expr::PeriodsSinceAnchor
        | Expr::PeriodsToEnd
        | Expr::SumOver(_, _)
        | Expr::DimElement(_)
        | Expr::IsElement(_, _)
        | Expr::AvgOver(_, _)
        | Expr::MinOver(_, _)
        | Expr::MaxOver(_, _)
        | Expr::WAvgOver(_, _, _) => 1,
        Expr::Add(a, b)
        | Expr::Sub(a, b)
        | Expr::Mul(a, b)
        | Expr::Div(a, b)
        | Expr::IfNull(a, b)
        | Expr::Gt(a, b)
        | Expr::Lt(a, b)
        | Expr::Gte(a, b)
        | Expr::Lte(a, b)
        | Expr::Eq(a, b)
        | Expr::Neq(a, b)
        | Expr::And(a, b)
        | Expr::Or(a, b) => 1 + expr_depth(a).max(expr_depth(b)),
        Expr::Not(a) | Expr::Abs(a) | Expr::Bucket(a, _) => 1 + expr_depth(a),
        Expr::If(a, b, c) | Expr::SafeDiv(a, b, c) | Expr::Clamp(a, b, c) => {
            1 + expr_depth(a).max(expr_depth(b)).max(expr_depth(c))
        }
        Expr::Min(args) | Expr::Max(args) | Expr::Coalesce(args) => {
            1 + args.iter().map(|a| expr_depth(a)).max().unwrap_or(0)
        }
        Expr::Lag(_, periods) | Expr::RollingAvg(_, periods) => 1 + expr_depth(periods),
        Expr::Benchmark(_, key) => 1 + expr_depth(key),
        Expr::Lookup(_, keys) => 1 + keys.iter().map(|k| expr_depth(k)).max().unwrap_or(0),
        // Phase 3H
        Expr::Predict(_, features) => 1 + features.iter().map(|f| expr_depth(f)).max().unwrap_or(0),
        Expr::Calibrate(v, _) | Expr::Exp(v) => 1 + expr_depth(v),
        Expr::NormCdf(x, mu, sigma) => 1 + expr_depth(x).max(expr_depth(mu)).max(expr_depth(sigma)),
        // Phase 3I
        Expr::Pow(a, b) | Expr::Mod(a, b) => 1 + expr_depth(a).max(expr_depth(b)),
        Expr::Sqrt(a)
        | Expr::Ln(a)
        | Expr::Log10(a)
        | Expr::Round(a)
        | Expr::Floor(a)
        | Expr::Ceil(a) => 1 + expr_depth(a),
        Expr::NormInv(p, mu, sigma) => 1 + expr_depth(p).max(expr_depth(mu)).max(expr_depth(sigma)),
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

/// Evaluate an `Expr` body. The caller supplies:
/// - `lookup_self`: resolves a `SelfRef(measure)` to its current `ScalarValue`
/// - `lookup_cross`: resolves cross-coordinate reads (actual_ref, prev, lag,
///   cumulative, rolling_avg, sum_over, benchmark, lookup, bucket, period_index)
///
/// Phase 3E extends the signature with `lookup_cross` for cross-coordinate reads.
/// Callers that don't use cross-coord functions pass a no-op closure.
///
/// See also [`eval_expr_unified`] for call sites that need a single closure
/// (e.g., when both self-ref and cross-coord reads must access the same
/// mutable state).
pub fn eval_expr<F, G>(
    expr: &Expr,
    lookup_self: &mut F,
    lookup_cross: &mut G,
) -> Result<ScalarValue, EngineError>
where
    F: FnMut(ElementId) -> Result<ScalarValue, EngineError>,
    G: FnMut(&CrossCoordRead) -> Result<ScalarValue, EngineError>,
{
    match expr {
        Expr::Const(v) => Ok(v.clone()),
        Expr::SelfRef(measure) => lookup_self(*measure),
        Expr::Add(a, b) => {
            let lhs = eval_expr(a, lookup_self, lookup_cross)?;
            let rhs = eval_expr(b, lookup_self, lookup_cross)?;
            Ok(null_add(lhs, rhs))
        }
        Expr::Sub(a, b) => {
            let lhs = eval_expr(a, lookup_self, lookup_cross)?;
            let rhs = eval_expr(b, lookup_self, lookup_cross)?;
            Ok(null_sub(lhs, rhs))
        }
        Expr::Mul(a, b) => {
            let lhs = eval_expr(a, lookup_self, lookup_cross)?;
            let rhs = eval_expr(b, lookup_self, lookup_cross)?;
            Ok(null_mul(lhs, rhs))
        }
        Expr::Div(a, b) => {
            let lhs = eval_expr(a, lookup_self, lookup_cross)?;
            let rhs = eval_expr(b, lookup_self, lookup_cross)?;
            Ok(null_div(lhs, rhs))
        }
        Expr::IfNull(primary, fallback) => {
            let p = eval_expr(primary, lookup_self, lookup_cross)?;
            if p.is_null() {
                eval_expr(fallback, lookup_self, lookup_cross)
            } else {
                Ok(p)
            }
        }
        // -- Phase 3E: Comparisons (Null propagation per ADR-0011 Decision 3) --
        Expr::Gt(a, b) => eval_comparison(a, b, lookup_self, lookup_cross, |l, r| l > r),
        Expr::Lt(a, b) => eval_comparison(a, b, lookup_self, lookup_cross, |l, r| l < r),
        Expr::Gte(a, b) => eval_comparison(a, b, lookup_self, lookup_cross, |l, r| l >= r),
        Expr::Lte(a, b) => eval_comparison(a, b, lookup_self, lookup_cross, |l, r| l <= r),
        Expr::Eq(a, b) => {
            eval_comparison(a, b, lookup_self, lookup_cross, |l, r| (l - r).abs() < 1e-9)
        }
        Expr::Neq(a, b) => eval_comparison(a, b, lookup_self, lookup_cross, |l, r| {
            (l - r).abs() >= 1e-9
        }),
        // -- Phase 3E: Logical --
        Expr::And(a, b) => {
            let lhs = eval_expr(a, lookup_self, lookup_cross)?;
            let rhs = eval_expr(b, lookup_self, lookup_cross)?;
            match (to_f64_opt(&lhs), to_f64_opt(&rhs)) {
                (None, _) | (_, None) => Ok(ScalarValue::Null),
                (Some(l), Some(r)) => Ok(bool_to_scalar(l != 0.0 && r != 0.0)),
            }
        }
        Expr::Or(a, b) => {
            let lhs = eval_expr(a, lookup_self, lookup_cross)?;
            let rhs = eval_expr(b, lookup_self, lookup_cross)?;
            match (to_f64_opt(&lhs), to_f64_opt(&rhs)) {
                (None, _) | (_, None) => Ok(ScalarValue::Null),
                (Some(l), Some(r)) => Ok(bool_to_scalar(l != 0.0 || r != 0.0)),
            }
        }
        Expr::Not(a) => {
            let v = eval_expr(a, lookup_self, lookup_cross)?;
            match to_f64_opt(&v) {
                None => Ok(ScalarValue::Null),
                // Phase 6A.1 MIN-6: use the project's 1e-9 epsilon
                // convention (see CLAUDE.md §3.1). Float arithmetic like
                // `Spend - Spend` can produce values like `-2.7e-17` that
                // are conceptually zero but `== 0.0` fails. Treat
                // anything whose abs() is below epsilon as falsy.
                Some(x) => Ok(bool_to_scalar(x.abs() < 1e-9)),
            }
        }
        // -- Phase 3E: Functions --
        Expr::If(cond, then_b, else_b) => {
            let c = eval_expr(cond, lookup_self, lookup_cross)?;
            match to_f64_opt(&c) {
                None => eval_expr(else_b, lookup_self, lookup_cross), // Null → else
                // Phase 6A.1 MIN-6: same epsilon convention as Expr::Not
                // above. Falsy iff the value is within 1e-9 of zero.
                Some(x) if x.abs() < 1e-9 => eval_expr(else_b, lookup_self, lookup_cross),
                Some(_) => eval_expr(then_b, lookup_self, lookup_cross), // truthy
            }
        }
        Expr::Min(args) => {
            let mut result: Option<f64> = None;
            for arg in args {
                match eval_expr(arg, lookup_self, lookup_cross)? {
                    ScalarValue::Null => return Ok(ScalarValue::Null),
                    ScalarValue::F64(v) => {
                        result = Some(match result {
                            None => v,
                            Some(curr) => curr.min(v),
                        });
                    }
                    _ => return Ok(ScalarValue::Null),
                }
            }
            Ok(result.map_or(ScalarValue::Null, ScalarValue::F64))
        }
        Expr::Max(args) => {
            let mut result: Option<f64> = None;
            for arg in args {
                match eval_expr(arg, lookup_self, lookup_cross)? {
                    ScalarValue::Null => return Ok(ScalarValue::Null),
                    ScalarValue::F64(v) => {
                        result = Some(match result {
                            None => v,
                            Some(curr) => curr.max(v),
                        });
                    }
                    _ => return Ok(ScalarValue::Null),
                }
            }
            Ok(result.map_or(ScalarValue::Null, ScalarValue::F64))
        }
        Expr::Abs(a) => match eval_expr(a, lookup_self, lookup_cross)? {
            ScalarValue::Null => Ok(ScalarValue::Null),
            ScalarValue::F64(v) => Ok(finite_or_null(v.abs())),
            _ => Ok(ScalarValue::Null),
        },
        Expr::SafeDiv(n, d, def) => {
            let nv = eval_expr(n, lookup_self, lookup_cross)?;
            let dv = eval_expr(d, lookup_self, lookup_cross)?;
            match (nv, dv) {
                (ScalarValue::Null, _) | (_, ScalarValue::Null) => {
                    eval_expr(def, lookup_self, lookup_cross)
                }
                (ScalarValue::F64(_), ScalarValue::F64(y)) if y.abs() < 1e-300 => {
                    eval_expr(def, lookup_self, lookup_cross)
                }
                (ScalarValue::F64(x), ScalarValue::F64(y)) => Ok(finite_or_null(x / y)),
                _ => eval_expr(def, lookup_self, lookup_cross),
            }
        }
        Expr::Clamp(v, lo, hi) => {
            let vv = eval_expr(v, lookup_self, lookup_cross)?;
            let lov = eval_expr(lo, lookup_self, lookup_cross)?;
            let hiv = eval_expr(hi, lookup_self, lookup_cross)?;
            match (to_f64_opt(&vv), to_f64_opt(&lov), to_f64_opt(&hiv)) {
                (Some(x), Some(l), Some(h)) => Ok(ScalarValue::F64(x.max(l).min(h))),
                _ => Ok(ScalarValue::Null),
            }
        }
        Expr::Coalesce(args) => {
            for arg in args {
                let v = eval_expr(arg, lookup_self, lookup_cross)?;
                if !v.is_null() {
                    return Ok(v);
                }
            }
            Ok(ScalarValue::Null)
        }
        // -- Cross-coordinate reads: delegate to lookup_cross --
        Expr::ActualRef(measure) => {
            lookup_cross(&CrossCoordRead::ScenarioShift { measure: *measure })
        }
        Expr::Prev(measure) => lookup_cross(&CrossCoordRead::TimeOffset {
            offset: -1,
            measure: *measure,
        }),
        Expr::Lag(measure, periods_expr) => {
            let periods_val = eval_expr(periods_expr, lookup_self, lookup_cross)?;
            match to_f64_opt(&periods_val) {
                Some(n) => {
                    let offset = -(n as i32);
                    lookup_cross(&CrossCoordRead::TimeOffset {
                        offset,
                        measure: *measure,
                    })
                }
                None => Ok(ScalarValue::Null),
            }
        }
        Expr::Cumulative(measure) => {
            lookup_cross(&CrossCoordRead::Cumulative { measure: *measure })
        }
        Expr::RollingAvg(measure, window_expr) => {
            let window_val = eval_expr(window_expr, lookup_self, lookup_cross)?;
            match to_f64_opt(&window_val) {
                Some(w) if w >= 1.0 => lookup_cross(&CrossCoordRead::RollingAvg {
                    measure: *measure,
                    window: w as u32,
                }),
                _ => Ok(ScalarValue::Null),
            }
        }
        Expr::PeriodIndex => lookup_cross(&CrossCoordRead::PeriodIndex),
        Expr::AnchorIndex => lookup_cross(&CrossCoordRead::AnchorIndex),
        Expr::IsPast => lookup_cross(&CrossCoordRead::IsPast),
        Expr::IsCurrent => lookup_cross(&CrossCoordRead::IsCurrent),
        Expr::IsFuture => lookup_cross(&CrossCoordRead::IsFuture),
        Expr::PeriodsSinceAnchor => lookup_cross(&CrossCoordRead::PeriodsSinceAnchor),
        Expr::PeriodsToEnd => lookup_cross(&CrossCoordRead::PeriodsToEnd),
        Expr::Benchmark(name, key_expr) => {
            let key = eval_expr(key_expr, lookup_self, lookup_cross)?;
            lookup_cross(&CrossCoordRead::BenchmarkLookup {
                name: name.clone(),
                key,
            })
        }
        Expr::Lookup(table, key_exprs) => {
            let mut keys = Vec::with_capacity(key_exprs.len());
            for ke in key_exprs {
                keys.push(eval_expr(ke, lookup_self, lookup_cross)?);
            }
            lookup_cross(&CrossCoordRead::TableLookup {
                table: table.clone(),
                keys,
            })
        }
        Expr::Bucket(value_expr, threshold_name) => {
            let v = eval_expr(value_expr, lookup_self, lookup_cross)?;
            lookup_cross(&CrossCoordRead::BucketLookup {
                threshold: threshold_name.clone(),
                value: v,
            })
        }
        Expr::SumOver(dim, measure) => lookup_cross(&CrossCoordRead::DimensionScan {
            dimension: *dim,
            measure: *measure,
        }),
        Expr::DimElement(dim) => {
            lookup_cross(&CrossCoordRead::CurrentElementName { dimension: *dim })
        }
        // -- Phase 3H: Fitted-model evaluation --
        Expr::Predict(model_id, feature_exprs) => {
            let mut features = Vec::with_capacity(feature_exprs.len());
            for fe in feature_exprs {
                let v = eval_expr(fe, lookup_self, lookup_cross)?;
                if v.is_null() {
                    return Ok(ScalarValue::Null); // Null-poisoning
                }
                features.push(v);
            }
            lookup_cross(&CrossCoordRead::PredictModel {
                model_id: model_id.clone(),
                features,
            })
        }
        Expr::Calibrate(value_expr, map_id) => {
            let v = eval_expr(value_expr, lookup_self, lookup_cross)?;
            if v.is_null() {
                return Ok(ScalarValue::Null); // Null-poisoning
            }
            lookup_cross(&CrossCoordRead::CalibrateMap {
                map_id: map_id.clone(),
                value: v,
            })
        }
        Expr::Exp(inner) => {
            let v = eval_expr(inner, lookup_self, lookup_cross)?;
            match to_f64_opt(&v) {
                None => Ok(ScalarValue::Null),
                Some(x) => Ok(finite_or_null(x.exp())),
            }
        }
        Expr::NormCdf(x_expr, mu_expr, sigma_expr) => {
            let xv = eval_expr(x_expr, lookup_self, lookup_cross)?;
            let muv = eval_expr(mu_expr, lookup_self, lookup_cross)?;
            let sv = eval_expr(sigma_expr, lookup_self, lookup_cross)?;
            match (to_f64_opt(&xv), to_f64_opt(&muv), to_f64_opt(&sv)) {
                (Some(x), Some(mu), Some(sigma)) => {
                    if sigma <= 0.0 {
                        Ok(ScalarValue::Null)
                    } else {
                        Ok(ScalarValue::F64(norm_cdf_compute(x, mu, sigma)))
                    }
                }
                _ => Ok(ScalarValue::Null),
            }
        }
        // -- Phase 3I: math primitives --
        Expr::Pow(base_expr, exp_expr) => {
            let bv = eval_expr(base_expr, lookup_self, lookup_cross)?;
            let ev = eval_expr(exp_expr, lookup_self, lookup_cross)?;
            Ok(eval_pow(bv, ev))
        }
        Expr::Sqrt(a) => {
            let v = eval_expr(a, lookup_self, lookup_cross)?;
            Ok(eval_sqrt(v))
        }
        Expr::Ln(a) => {
            let v = eval_expr(a, lookup_self, lookup_cross)?;
            Ok(eval_ln(v))
        }
        Expr::Log10(a) => {
            let v = eval_expr(a, lookup_self, lookup_cross)?;
            Ok(eval_log10(v))
        }
        Expr::Round(a) => {
            let v = eval_expr(a, lookup_self, lookup_cross)?;
            Ok(eval_unary_finite(v, |x| x.round()))
        }
        Expr::Floor(a) => {
            let v = eval_expr(a, lookup_self, lookup_cross)?;
            Ok(eval_unary_finite(v, |x| x.floor()))
        }
        Expr::Ceil(a) => {
            let v = eval_expr(a, lookup_self, lookup_cross)?;
            Ok(eval_unary_finite(v, |x| x.ceil()))
        }
        Expr::Mod(a, b) => {
            let av = eval_expr(a, lookup_self, lookup_cross)?;
            let bv = eval_expr(b, lookup_self, lookup_cross)?;
            Ok(eval_mod(av, bv))
        }
        Expr::NormInv(p_expr, mu_expr, sigma_expr) => {
            let pv = eval_expr(p_expr, lookup_self, lookup_cross)?;
            let muv = eval_expr(mu_expr, lookup_self, lookup_cross)?;
            let sv = eval_expr(sigma_expr, lookup_self, lookup_cross)?;
            Ok(eval_norm_inv(pv, muv, sv))
        }
        // -- Phase 3I: is_element --
        Expr::IsElement(dim, elem) => lookup_cross(&CrossCoordRead::IsElement {
            dimension: *dim,
            element: *elem,
        }),
        // -- Phase 3I: cross-coord scans --
        Expr::AvgOver(dim, measure) => lookup_cross(&CrossCoordRead::DimensionAvg {
            dimension: *dim,
            measure: *measure,
        }),
        Expr::MinOver(dim, measure) => lookup_cross(&CrossCoordRead::DimensionMin {
            dimension: *dim,
            measure: *measure,
        }),
        Expr::MaxOver(dim, measure) => lookup_cross(&CrossCoordRead::DimensionMax {
            dimension: *dim,
            measure: *measure,
        }),
        Expr::WAvgOver(dim, value_measure, weight_measure) => {
            lookup_cross(&CrossCoordRead::DimensionWAvg {
                dimension: *dim,
                value_measure: *value_measure,
                weight_measure: *weight_measure,
            })
        }
    }
}

/// Phase 3I math primitive: `pow(base, exp)` with Null edge cases per
/// handoff item 2 (negative base + non-integer exp → Null).
fn eval_pow(base: ScalarValue, exp: ScalarValue) -> ScalarValue {
    match (to_f64_opt(&base), to_f64_opt(&exp)) {
        (Some(b), Some(e)) => {
            if b < 0.0 && e.fract() != 0.0 {
                return ScalarValue::Null;
            }
            finite_or_null(b.powf(e))
        }
        _ => ScalarValue::Null,
    }
}

/// Phase 3I math primitive: `sqrt(x)` with Null on negative input.
fn eval_sqrt(v: ScalarValue) -> ScalarValue {
    match to_f64_opt(&v) {
        Some(x) if x >= 0.0 => finite_or_null(x.sqrt()),
        _ => ScalarValue::Null,
    }
}

/// Phase 3I math primitive: `ln(x)` with Null when x <= 0.
fn eval_ln(v: ScalarValue) -> ScalarValue {
    match to_f64_opt(&v) {
        Some(x) if x > 0.0 => finite_or_null(x.ln()),
        _ => ScalarValue::Null,
    }
}

/// Phase 3I math primitive: `log10(x)` with Null when x <= 0.
fn eval_log10(v: ScalarValue) -> ScalarValue {
    match to_f64_opt(&v) {
        Some(x) if x > 0.0 => finite_or_null(x.log10()),
        _ => ScalarValue::Null,
    }
}

/// Phase 3I math primitive shared body: round/floor/ceil with Null
/// propagation and finite-or-null guard.
fn eval_unary_finite(v: ScalarValue, f: impl Fn(f64) -> f64) -> ScalarValue {
    match to_f64_opt(&v) {
        Some(x) => finite_or_null(f(x)),
        None => ScalarValue::Null,
    }
}

/// Phase 3I math primitive: `mod(a, b)` via Euclidean remainder.
/// Null when b is near zero.
fn eval_mod(a: ScalarValue, b: ScalarValue) -> ScalarValue {
    match (to_f64_opt(&a), to_f64_opt(&b)) {
        (Some(x), Some(y)) if y.abs() >= 1e-300 => finite_or_null(x.rem_euclid(y)),
        _ => ScalarValue::Null,
    }
}

/// Phase 3I math primitive: `norm_inv(p, mu, sigma)` — inverse normal CDF.
///
/// Implementation: Beasley–Springer–Moro algorithm (Moro 1995). Pure
/// hand-roll with no external deps. Accuracy ~1e-9 in the central
/// region, falls back to Moro's tail series for |p − 0.5| > 0.42.
/// Reference: Moro, B. "The Full Monte." Risk, Feb 1995, pp. 57–58.
///
/// Returns Null at boundaries (p ≤ 0, p ≥ 1) and when sigma ≤ 0.
fn eval_norm_inv(p: ScalarValue, mu: ScalarValue, sigma: ScalarValue) -> ScalarValue {
    match (to_f64_opt(&p), to_f64_opt(&mu), to_f64_opt(&sigma)) {
        (Some(pv), Some(muv), Some(sv)) if pv > 0.0 && pv < 1.0 && sv > 0.0 => {
            finite_or_null(muv + sv * norm_inv_unit(pv))
        }
        _ => ScalarValue::Null,
    }
}

/// Beasley–Springer–Moro inverse standard-normal CDF.
fn norm_inv_unit(p: f64) -> f64 {
    // Beasley-Springer (central region) coefficients
    const A: [f64; 4] = [
        2.50662823884,
        -18.61500062529,
        41.39119773534,
        -25.44106049637,
    ];
    const B: [f64; 4] = [
        -8.47351093090,
        23.08336743743,
        -21.06224101826,
        3.13082909833,
    ];
    // Moro tail-series coefficients
    const C: [f64; 9] = [
        0.3374754822726147,
        0.9761690190917186,
        0.1607979714918209,
        0.0276438810333863,
        0.0038405729373609,
        0.0003951896511919,
        0.0000321767881768,
        0.0000002888167364,
        0.0000003960315187,
    ];
    let y = p - 0.5;
    if y.abs() < 0.42 {
        let r = y * y;
        let num = ((A[3] * r + A[2]) * r + A[1]) * r + A[0];
        let den = (((B[3] * r + B[2]) * r + B[1]) * r + B[0]) * r + 1.0;
        y * num / den
    } else {
        let r0 = if y > 0.0 { 1.0 - p } else { p };
        let r = (-r0.ln()).ln();
        let mut x = C[0]
            + r * (C[1]
                + r * (C[2]
                    + r * (C[3] + r * (C[4] + r * (C[5] + r * (C[6] + r * (C[7] + r * C[8])))))));
        if y < 0.0 {
            x = -x;
        }
        x
    }
}

/// Abramowitz & Stegun 26.2.17 polynomial approximation for the
/// standard normal CDF. Accuracy ~7.5e-8, zero deps.
fn norm_cdf_compute(x: f64, mu: f64, sigma: f64) -> f64 {
    let z = (x - mu) / sigma;
    let t = 1.0 / (1.0 + 0.2316419 * z.abs());
    let d = 0.3989422804014327 * (-z * z / 2.0).exp();
    let p =
        d * t * (0.3193815 + t * (-0.3565638 + t * (1.781478 + t * (-1.8212560 + t * 1.330274))));
    if z > 0.0 {
        1.0 - p
    } else {
        p
    }
}

/// Cross-coordinate read specification, passed to the `lookup_cross` closure.
/// The caller (typically `Cube::read`) resolves these against the full
/// coordinate context.
#[derive(Clone, Debug)]
pub enum CrossCoordRead {
    /// Shift Scenario dim to actuals_element, read measure there.
    ScenarioShift { measure: ElementId },
    /// Shift Time dim by offset positions, read measure there.
    TimeOffset { offset: i32, measure: ElementId },
    /// Running sum of measure up to current time position.
    Cumulative { measure: ElementId },
    /// Moving average of measure over a window of periods.
    RollingAvg { measure: ElementId, window: u32 },
    /// Current element's 0-based position in Time dim.
    PeriodIndex,
    /// Lookup a benchmark value by name and key.
    BenchmarkLookup { name: String, key: ScalarValue },
    /// Lookup a table value by name and key(s). Phase 3I item 3 added the
    /// multi-key variant: a 1-element keys vec is the original single-key
    /// `lookup(name, key)`; N-element vecs are the new multi-key
    /// `lookup(name, k1, k2, ...)` which the eval site joins with `|`
    /// before dispatching against the table.
    TableLookup {
        table: String,
        keys: Vec<ScalarValue>,
    },
    /// Lookup bucket band index by threshold name and value.
    BucketLookup {
        threshold: String,
        value: ScalarValue,
    },
    /// Sum across all leaf elements of a dimension for a measure.
    DimensionScan {
        dimension: crate::id::DimensionId,
        measure: ElementId,
    },
    // -- Phase 3H: Fitted-model evaluation --
    /// Evaluate a fitted model by name with the given feature values.
    PredictModel {
        model_id: String,
        features: Vec<ScalarValue>,
    },
    /// Apply a calibration map to a raw value.
    CalibrateMap { map_id: String, value: ScalarValue },
    // -- Phase 3F.1: Anchor functions --
    /// Period index of the time_anchor element.
    AnchorIndex,
    /// 1.0 if current period_index < anchor_index, else 0.0.
    IsPast,
    /// 1.0 if current period_index == anchor_index, else 0.0.
    IsCurrent,
    /// 1.0 if current period_index > anchor_index, else 0.0.
    IsFuture,
    /// period_index - anchor_index (negative = past).
    PeriodsSinceAnchor,
    /// max_period_index - period_index.
    PeriodsToEnd,
    /// Resolve the current coordinate's element name in the given dimension.
    CurrentElementName { dimension: crate::id::DimensionId },
    // -- Phase 3I: narrow element-match indicator --
    /// Returns 1.0 if the current coord's element in `dimension` is
    /// `element`, else 0.0. Element resolution is parse-time so the
    /// kernel doesn't see strings.
    IsElement {
        dimension: crate::id::DimensionId,
        element: ElementId,
    },
    // -- Phase 3I: cross-coord scans --
    /// Mean of `measure` across all leaf elements of `dimension`,
    /// skipping Nulls.
    DimensionAvg {
        dimension: crate::id::DimensionId,
        measure: ElementId,
    },
    /// Min of `measure` across leaf elements of `dimension`, skipping Nulls.
    DimensionMin {
        dimension: crate::id::DimensionId,
        measure: ElementId,
    },
    /// Max of `measure` across leaf elements of `dimension`, skipping Nulls.
    DimensionMax {
        dimension: crate::id::DimensionId,
        measure: ElementId,
    },
    /// Weighted average of `value_measure` across leaf elements of
    /// `dimension`, weights from `weight_measure`. Null when all weights
    /// are zero.
    DimensionWAvg {
        dimension: crate::id::DimensionId,
        value_measure: ElementId,
        weight_measure: ElementId,
    },
}

fn eval_comparison<F, G>(
    a: &Expr,
    b: &Expr,
    lookup_self: &mut F,
    lookup_cross: &mut G,
    cmp: impl Fn(f64, f64) -> bool,
) -> Result<ScalarValue, EngineError>
where
    F: FnMut(ElementId) -> Result<ScalarValue, EngineError>,
    G: FnMut(&CrossCoordRead) -> Result<ScalarValue, EngineError>,
{
    let lhs = eval_expr(a, lookup_self, lookup_cross)?;
    let rhs = eval_expr(b, lookup_self, lookup_cross)?;
    match (to_f64_opt(&lhs), to_f64_opt(&rhs)) {
        (Some(l), Some(r)) => Ok(bool_to_scalar(cmp(l, r))),
        _ => Ok(ScalarValue::Null), // Null in comparison returns Null
    }
}

fn to_f64_opt(v: &ScalarValue) -> Option<f64> {
    match v {
        ScalarValue::F64(x) => Some(*x),
        ScalarValue::Null => None,
        _ => None,
    }
}

fn bool_to_scalar(b: bool) -> ScalarValue {
    ScalarValue::F64(if b { 1.0 } else { 0.0 })
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

// ===========================================================================
// Unified eval — single-closure variant for call sites where both self-ref
// and cross-coord reads must go through the same mutable state.
// ===========================================================================

/// A lookup request dispatched during expression evaluation.
#[derive(Debug)]
pub enum EvalLookup<'a> {
    /// Resolve a self-ref measure (same coordinate, different measure slot).
    SelfRef(ElementId),
    /// Resolve a cross-coordinate read.
    Cross(&'a CrossCoordRead),
}

/// Like [`eval_expr`] but uses a single closure for both self-ref and
/// cross-coord reads. This avoids the borrow-checker conflict when both
/// kinds of reads need `&mut` access to the same state (e.g., the Cube's
/// `read_inner` method).
pub fn eval_expr_unified<H>(expr: &Expr, handler: &mut H) -> Result<ScalarValue, EngineError>
where
    H: FnMut(EvalLookup<'_>) -> Result<ScalarValue, EngineError>,
{
    // Delegate to eval_expr by splitting the single handler into two closures.
    // This works because both closures forward to the same handler (no split borrow).
    //
    // We use a Cell<Option<&mut H>> dance to share the handler between two closures.
    // Actually simpler: just re-implement the match arms inline.
    eval_expr_unified_inner(expr, handler)
}

fn eval_expr_unified_inner<H>(expr: &Expr, handler: &mut H) -> Result<ScalarValue, EngineError>
where
    H: FnMut(EvalLookup<'_>) -> Result<ScalarValue, EngineError>,
{
    match expr {
        Expr::Const(v) => Ok(v.clone()),
        Expr::SelfRef(measure) => handler(EvalLookup::SelfRef(*measure)),
        Expr::Add(a, b) => {
            let lhs = eval_expr_unified_inner(a, handler)?;
            let rhs = eval_expr_unified_inner(b, handler)?;
            Ok(null_add(lhs, rhs))
        }
        Expr::Sub(a, b) => {
            let lhs = eval_expr_unified_inner(a, handler)?;
            let rhs = eval_expr_unified_inner(b, handler)?;
            Ok(null_sub(lhs, rhs))
        }
        Expr::Mul(a, b) => {
            let lhs = eval_expr_unified_inner(a, handler)?;
            let rhs = eval_expr_unified_inner(b, handler)?;
            Ok(null_mul(lhs, rhs))
        }
        Expr::Div(a, b) => {
            let lhs = eval_expr_unified_inner(a, handler)?;
            let rhs = eval_expr_unified_inner(b, handler)?;
            Ok(null_div(lhs, rhs))
        }
        Expr::IfNull(primary, fallback) => {
            let p = eval_expr_unified_inner(primary, handler)?;
            if !p.is_null() {
                Ok(p)
            } else {
                eval_expr_unified_inner(fallback, handler)
            }
        }
        Expr::Gt(a, b) => eval_comparison_unified(a, b, handler, |l, r| l > r),
        Expr::Lt(a, b) => eval_comparison_unified(a, b, handler, |l, r| l < r),
        Expr::Gte(a, b) => eval_comparison_unified(a, b, handler, |l, r| l >= r),
        Expr::Lte(a, b) => eval_comparison_unified(a, b, handler, |l, r| l <= r),
        Expr::Eq(a, b) => eval_comparison_unified(a, b, handler, |l, r| (l - r).abs() < 1e-9),
        Expr::Neq(a, b) => eval_comparison_unified(a, b, handler, |l, r| (l - r).abs() >= 1e-9),
        Expr::And(a, b) => {
            let lhs = eval_expr_unified_inner(a, handler)?;
            let rhs = eval_expr_unified_inner(b, handler)?;
            match (to_f64_opt(&lhs), to_f64_opt(&rhs)) {
                (Some(l), Some(r)) => Ok(bool_to_scalar(l != 0.0 && r != 0.0)),
                _ => Ok(ScalarValue::Null),
            }
        }
        Expr::Or(a, b) => {
            let lhs = eval_expr_unified_inner(a, handler)?;
            let rhs = eval_expr_unified_inner(b, handler)?;
            match (to_f64_opt(&lhs), to_f64_opt(&rhs)) {
                (Some(l), Some(r)) => Ok(bool_to_scalar(l != 0.0 || r != 0.0)),
                _ => Ok(ScalarValue::Null),
            }
        }
        Expr::Not(a) => {
            let v = eval_expr_unified_inner(a, handler)?;
            match to_f64_opt(&v) {
                // Phase 6A.1 MIN-6: mirror the epsilon convention from eval_expr.
                // Float arithmetic like `A - A` can yield near-zero values (~1e-17)
                // that are conceptually zero but fail the exact `== 0.0` check.
                Some(x) => Ok(bool_to_scalar(x.abs() < 1e-9)),
                None => Ok(ScalarValue::Null),
            }
        }
        Expr::If(cond, then_b, else_b) => {
            let c = eval_expr_unified_inner(cond, handler)?;
            match to_f64_opt(&c) {
                None => eval_expr_unified_inner(else_b, handler),
                // Phase 6A.1 MIN-6: same epsilon convention as Expr::Not above.
                Some(x) if x.abs() < 1e-9 => eval_expr_unified_inner(else_b, handler),
                Some(_) => eval_expr_unified_inner(then_b, handler),
            }
        }
        Expr::Min(args) => {
            let mut min_val: Option<f64> = None;
            for arg in args {
                if let ScalarValue::F64(v) = eval_expr_unified_inner(arg, handler)? {
                    min_val = Some(match min_val {
                        Some(cur) => cur.min(v),
                        None => v,
                    });
                }
            }
            Ok(min_val.map_or(ScalarValue::Null, ScalarValue::F64))
        }
        Expr::Max(args) => {
            let mut max_val: Option<f64> = None;
            for arg in args {
                if let ScalarValue::F64(v) = eval_expr_unified_inner(arg, handler)? {
                    max_val = Some(match max_val {
                        Some(cur) => cur.max(v),
                        None => v,
                    });
                }
            }
            Ok(max_val.map_or(ScalarValue::Null, ScalarValue::F64))
        }
        Expr::Abs(a) => match eval_expr_unified_inner(a, handler)? {
            ScalarValue::F64(v) => Ok(ScalarValue::F64(v.abs())),
            other => Ok(other),
        },
        Expr::SafeDiv(n, d, def) => {
            let nv = eval_expr_unified_inner(n, handler)?;
            let dv = eval_expr_unified_inner(d, handler)?;
            match (to_f64_opt(&nv), to_f64_opt(&dv)) {
                (Some(num), Some(den)) if den.abs() >= 1e-300 => Ok(finite_or_null(num / den)),
                (Some(_), Some(_)) => eval_expr_unified_inner(def, handler),
                _ => eval_expr_unified_inner(def, handler),
            }
        }
        Expr::Clamp(v, lo, hi) => {
            let vv = eval_expr_unified_inner(v, handler)?;
            let lov = eval_expr_unified_inner(lo, handler)?;
            let hiv = eval_expr_unified_inner(hi, handler)?;
            match (to_f64_opt(&vv), to_f64_opt(&lov), to_f64_opt(&hiv)) {
                (Some(x), Some(l), Some(h)) => Ok(ScalarValue::F64(x.max(l).min(h))),
                _ => Ok(ScalarValue::Null),
            }
        }
        Expr::Coalesce(args) => {
            for arg in args {
                let v = eval_expr_unified_inner(arg, handler)?;
                if !v.is_null() {
                    return Ok(v);
                }
            }
            Ok(ScalarValue::Null)
        }
        // -- Cross-coordinate reads: delegate to handler --
        Expr::ActualRef(measure) => handler(EvalLookup::Cross(&CrossCoordRead::ScenarioShift {
            measure: *measure,
        })),
        Expr::Prev(measure) => handler(EvalLookup::Cross(&CrossCoordRead::TimeOffset {
            offset: -1,
            measure: *measure,
        })),
        Expr::Lag(measure, periods_expr) => {
            let periods_val = eval_expr_unified_inner(periods_expr, handler)?;
            match to_f64_opt(&periods_val) {
                Some(n) => {
                    let offset = -(n as i32);
                    handler(EvalLookup::Cross(&CrossCoordRead::TimeOffset {
                        offset,
                        measure: *measure,
                    }))
                }
                None => Ok(ScalarValue::Null),
            }
        }
        Expr::Cumulative(measure) => handler(EvalLookup::Cross(&CrossCoordRead::Cumulative {
            measure: *measure,
        })),
        Expr::RollingAvg(measure, window_expr) => {
            let window_val = eval_expr_unified_inner(window_expr, handler)?;
            match to_f64_opt(&window_val) {
                Some(w) if w >= 1.0 => handler(EvalLookup::Cross(&CrossCoordRead::RollingAvg {
                    measure: *measure,
                    window: w as u32,
                })),
                _ => Ok(ScalarValue::Null),
            }
        }
        Expr::PeriodIndex => handler(EvalLookup::Cross(&CrossCoordRead::PeriodIndex)),
        Expr::AnchorIndex => handler(EvalLookup::Cross(&CrossCoordRead::AnchorIndex)),
        Expr::IsPast => handler(EvalLookup::Cross(&CrossCoordRead::IsPast)),
        Expr::IsCurrent => handler(EvalLookup::Cross(&CrossCoordRead::IsCurrent)),
        Expr::IsFuture => handler(EvalLookup::Cross(&CrossCoordRead::IsFuture)),
        Expr::PeriodsSinceAnchor => handler(EvalLookup::Cross(&CrossCoordRead::PeriodsSinceAnchor)),
        Expr::PeriodsToEnd => handler(EvalLookup::Cross(&CrossCoordRead::PeriodsToEnd)),
        Expr::Benchmark(name, key_expr) => {
            let key = eval_expr_unified_inner(key_expr, handler)?;
            handler(EvalLookup::Cross(&CrossCoordRead::BenchmarkLookup {
                name: name.clone(),
                key,
            }))
        }
        Expr::Lookup(table, key_exprs) => {
            let mut keys = Vec::with_capacity(key_exprs.len());
            for ke in key_exprs {
                keys.push(eval_expr_unified_inner(ke, handler)?);
            }
            handler(EvalLookup::Cross(&CrossCoordRead::TableLookup {
                table: table.clone(),
                keys,
            }))
        }
        Expr::Bucket(value_expr, threshold_name) => {
            let v = eval_expr_unified_inner(value_expr, handler)?;
            handler(EvalLookup::Cross(&CrossCoordRead::BucketLookup {
                threshold: threshold_name.clone(),
                value: v,
            }))
        }
        Expr::SumOver(dim, measure) => handler(EvalLookup::Cross(&CrossCoordRead::DimensionScan {
            dimension: *dim,
            measure: *measure,
        })),
        Expr::DimElement(dim) => handler(EvalLookup::Cross(&CrossCoordRead::CurrentElementName {
            dimension: *dim,
        })),
        Expr::Predict(model_id, feature_exprs) => {
            let mut features = Vec::with_capacity(feature_exprs.len());
            for fe in feature_exprs {
                let v = eval_expr_unified_inner(fe, handler)?;
                if v.is_null() {
                    return Ok(ScalarValue::Null);
                }
                features.push(v);
            }
            handler(EvalLookup::Cross(&CrossCoordRead::PredictModel {
                model_id: model_id.clone(),
                features,
            }))
        }
        Expr::Calibrate(value_expr, map_id) => {
            let v = eval_expr_unified_inner(value_expr, handler)?;
            if v.is_null() {
                return Ok(ScalarValue::Null);
            }
            handler(EvalLookup::Cross(&CrossCoordRead::CalibrateMap {
                map_id: map_id.clone(),
                value: v,
            }))
        }
        Expr::Exp(inner) => {
            let v = eval_expr_unified_inner(inner, handler)?;
            match to_f64_opt(&v) {
                None => Ok(ScalarValue::Null),
                Some(x) => Ok(finite_or_null(x.exp())),
            }
        }
        Expr::NormCdf(x_expr, mu_expr, sigma_expr) => {
            let xv = eval_expr_unified_inner(x_expr, handler)?;
            let muv = eval_expr_unified_inner(mu_expr, handler)?;
            let sv = eval_expr_unified_inner(sigma_expr, handler)?;
            match (to_f64_opt(&xv), to_f64_opt(&muv), to_f64_opt(&sv)) {
                (Some(x), Some(mu), Some(sigma)) if sigma > 0.0 => {
                    Ok(ScalarValue::F64(norm_cdf_compute(x, mu, sigma)))
                }
                _ => Ok(ScalarValue::Null),
            }
        }
        // -- Phase 3I: math primitives --
        Expr::Pow(base_expr, exp_expr) => {
            let bv = eval_expr_unified_inner(base_expr, handler)?;
            let ev = eval_expr_unified_inner(exp_expr, handler)?;
            Ok(eval_pow(bv, ev))
        }
        Expr::Sqrt(a) => {
            let v = eval_expr_unified_inner(a, handler)?;
            Ok(eval_sqrt(v))
        }
        Expr::Ln(a) => {
            let v = eval_expr_unified_inner(a, handler)?;
            Ok(eval_ln(v))
        }
        Expr::Log10(a) => {
            let v = eval_expr_unified_inner(a, handler)?;
            Ok(eval_log10(v))
        }
        Expr::Round(a) => {
            let v = eval_expr_unified_inner(a, handler)?;
            Ok(eval_unary_finite(v, |x| x.round()))
        }
        Expr::Floor(a) => {
            let v = eval_expr_unified_inner(a, handler)?;
            Ok(eval_unary_finite(v, |x| x.floor()))
        }
        Expr::Ceil(a) => {
            let v = eval_expr_unified_inner(a, handler)?;
            Ok(eval_unary_finite(v, |x| x.ceil()))
        }
        Expr::Mod(a, b) => {
            let av = eval_expr_unified_inner(a, handler)?;
            let bv = eval_expr_unified_inner(b, handler)?;
            Ok(eval_mod(av, bv))
        }
        Expr::NormInv(p_expr, mu_expr, sigma_expr) => {
            let pv = eval_expr_unified_inner(p_expr, handler)?;
            let muv = eval_expr_unified_inner(mu_expr, handler)?;
            let sv = eval_expr_unified_inner(sigma_expr, handler)?;
            Ok(eval_norm_inv(pv, muv, sv))
        }
        // -- Phase 3I: is_element + cross-coord scans --
        Expr::IsElement(dim, elem) => handler(EvalLookup::Cross(&CrossCoordRead::IsElement {
            dimension: *dim,
            element: *elem,
        })),
        Expr::AvgOver(dim, measure) => handler(EvalLookup::Cross(&CrossCoordRead::DimensionAvg {
            dimension: *dim,
            measure: *measure,
        })),
        Expr::MinOver(dim, measure) => handler(EvalLookup::Cross(&CrossCoordRead::DimensionMin {
            dimension: *dim,
            measure: *measure,
        })),
        Expr::MaxOver(dim, measure) => handler(EvalLookup::Cross(&CrossCoordRead::DimensionMax {
            dimension: *dim,
            measure: *measure,
        })),
        Expr::WAvgOver(dim, value_measure, weight_measure) => {
            handler(EvalLookup::Cross(&CrossCoordRead::DimensionWAvg {
                dimension: *dim,
                value_measure: *value_measure,
                weight_measure: *weight_measure,
            }))
        }
    }
}

fn eval_comparison_unified<H>(
    a: &Expr,
    b: &Expr,
    handler: &mut H,
    cmp: impl Fn(f64, f64) -> bool,
) -> Result<ScalarValue, EngineError>
where
    H: FnMut(EvalLookup<'_>) -> Result<ScalarValue, EngineError>,
{
    let lhs = eval_expr_unified_inner(a, handler)?;
    let rhs = eval_expr_unified_inner(b, handler)?;
    match (to_f64_opt(&lhs), to_f64_opt(&rhs)) {
        (Some(l), Some(r)) => Ok(bool_to_scalar(cmp(l, r))),
        _ => Ok(ScalarValue::Null),
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

    fn no_cross(_: &CrossCoordRead) -> Result<ScalarValue, EngineError> {
        Ok(ScalarValue::Null)
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
        let v = eval_expr(&body, &mut lookup, &mut no_cross).expect("eval ok");
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
        let v = eval_expr(&body, &mut lookup, &mut no_cross).expect("eval ok");
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
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null), &mut no_cross).unwrap();
        assert!(v.is_null());

        // Add: Null + 5 = 5
        let body = Expr::Add(
            Box::new(Expr::Const(ScalarValue::Null)),
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(5.0));

        // Sub: Null - 5 = -5
        let body = Expr::Sub(
            Box::new(Expr::Const(ScalarValue::Null)),
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(-5.0));

        // Sub: 5 - Null = 5
        let body = Expr::Sub(
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
            Box::new(Expr::Const(ScalarValue::Null)),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(5.0));

        // Mul: Null * 5 = Null
        let body = Expr::Mul(
            Box::new(Expr::Const(ScalarValue::Null)),
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null), &mut no_cross).unwrap();
        assert!(v.is_null());

        // Mul: 5 * Null = Null
        let body = Expr::Mul(
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
            Box::new(Expr::Const(ScalarValue::Null)),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null), &mut no_cross).unwrap();
        assert!(v.is_null());

        // Div: Null / 5 = Null
        let body = Expr::Div(
            Box::new(Expr::Const(ScalarValue::Null)),
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null), &mut no_cross).unwrap();
        assert!(v.is_null());

        // Div: 5 / Null = Null
        let body = Expr::Div(
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
            Box::new(Expr::Const(ScalarValue::Null)),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null), &mut no_cross).unwrap();
        assert!(v.is_null());

        // Div: 5 / 0 = Null  (NOT Inf)
        let body = Expr::Div(
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
            Box::new(Expr::Const(ScalarValue::F64(0.0))),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null), &mut no_cross).unwrap();
        assert!(v.is_null(), "5 / 0 must be Null, not Inf or NaN");

        // Div: 5 / 1e-301 = Null  (treated as zero per |y| < 1e-300)
        let body = Expr::Div(
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
            Box::new(Expr::Const(ScalarValue::F64(1e-301))),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null), &mut no_cross).unwrap();
        assert!(v.is_null(), "5 / sub-epsilon must be Null");

        // IfNull(Null, 99) = 99
        let body = Expr::IfNull(
            Box::new(Expr::Const(ScalarValue::Null)),
            Box::new(Expr::Const(ScalarValue::F64(99.0))),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(99.0));

        // IfNull(7, 99) = 7
        let body = Expr::IfNull(
            Box::new(Expr::Const(ScalarValue::F64(7.0))),
            Box::new(Expr::Const(ScalarValue::F64(99.0))),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(7.0));

        // 0.0 distinct from Null: 0.0 + 5 = 5 (not Null), but Null + 5 = 5 (also 5 — special case).
        // The distinction lives more clearly in Mul:
        //  - 0.0 * 5  = 0.0 (numeric)
        //  - Null * 5 = Null
        let body = Expr::Mul(
            Box::new(Expr::Const(ScalarValue::F64(0.0))),
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
        );
        let v = eval_expr(&body, &mut make_lookup(ScalarValue::Null), &mut no_cross).unwrap();
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
        let v = eval_expr(&body, &mut lookup, &mut no_cross).unwrap();
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

    // -----------------------------------------------------------------------
    // Phase 3E: comparison + logical + function eval tests
    // -----------------------------------------------------------------------

    #[test]
    fn eval_comparison_gt() {
        let body = Expr::Gt(
            Box::new(Expr::Const(ScalarValue::F64(10.0))),
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
        );
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(1.0)); // true
    }

    #[test]
    fn eval_comparison_null_returns_null() {
        // Null > 5 = Null (per ADR-0011 Decision 3)
        let body = Expr::Gt(
            Box::new(Expr::Const(ScalarValue::Null)),
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
        );
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert!(v.is_null());
    }

    #[test]
    fn eval_if_null_condition_returns_else() {
        // if(Null, 10, 20) = 20 (Null condition → else branch)
        let body = Expr::If(
            Box::new(Expr::Const(ScalarValue::Null)),
            Box::new(Expr::Const(ScalarValue::F64(10.0))),
            Box::new(Expr::Const(ScalarValue::F64(20.0))),
        );
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(20.0));
    }

    #[test]
    fn eval_if_truthy_returns_then() {
        // if(1, 10, 20) = 10
        let body = Expr::If(
            Box::new(Expr::Const(ScalarValue::F64(1.0))),
            Box::new(Expr::Const(ScalarValue::F64(10.0))),
            Box::new(Expr::Const(ScalarValue::F64(20.0))),
        );
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(10.0));
    }

    #[test]
    fn eval_if_zero_returns_else() {
        // if(0, 10, 20) = 20 (zero is falsy)
        let body = Expr::If(
            Box::new(Expr::Const(ScalarValue::F64(0.0))),
            Box::new(Expr::Const(ScalarValue::F64(10.0))),
            Box::new(Expr::Const(ScalarValue::F64(20.0))),
        );
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(20.0));
    }

    #[test]
    fn eval_min_basic() {
        let body = Expr::Min(vec![
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
            Box::new(Expr::Const(ScalarValue::F64(3.0))),
            Box::new(Expr::Const(ScalarValue::F64(8.0))),
        ]);
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(3.0));
    }

    #[test]
    fn eval_min_null_propagates() {
        let body = Expr::Min(vec![
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
            Box::new(Expr::Const(ScalarValue::Null)),
        ]);
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert!(v.is_null());
    }

    #[test]
    fn eval_max_basic() {
        let body = Expr::Max(vec![
            Box::new(Expr::Const(ScalarValue::F64(5.0))),
            Box::new(Expr::Const(ScalarValue::F64(9.0))),
        ]);
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(9.0));
    }

    #[test]
    fn eval_abs() {
        let body = Expr::Abs(Box::new(Expr::Const(ScalarValue::F64(-7.0))));
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(7.0));
    }

    #[test]
    fn eval_safe_div_normal() {
        let body = Expr::SafeDiv(
            Box::new(Expr::Const(ScalarValue::F64(10.0))),
            Box::new(Expr::Const(ScalarValue::F64(2.0))),
            Box::new(Expr::Const(ScalarValue::F64(0.0))),
        );
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(5.0));
    }

    #[test]
    fn eval_safe_div_zero_returns_default() {
        let body = Expr::SafeDiv(
            Box::new(Expr::Const(ScalarValue::F64(10.0))),
            Box::new(Expr::Const(ScalarValue::F64(0.0))),
            Box::new(Expr::Const(ScalarValue::F64(-1.0))),
        );
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(-1.0));
    }

    #[test]
    fn eval_clamp() {
        // clamp(15, 0, 10) = 10
        let body = Expr::Clamp(
            Box::new(Expr::Const(ScalarValue::F64(15.0))),
            Box::new(Expr::Const(ScalarValue::F64(0.0))),
            Box::new(Expr::Const(ScalarValue::F64(10.0))),
        );
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(10.0));
    }

    #[test]
    fn eval_coalesce_first_non_null() {
        let body = Expr::Coalesce(vec![
            Box::new(Expr::Const(ScalarValue::Null)),
            Box::new(Expr::Const(ScalarValue::Null)),
            Box::new(Expr::Const(ScalarValue::F64(42.0))),
            Box::new(Expr::Const(ScalarValue::F64(99.0))),
        ]);
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(42.0));
    }

    #[test]
    fn eval_logical_and_or_not() {
        // 1 and 1 = 1 (true)
        let body = Expr::And(
            Box::new(Expr::Const(ScalarValue::F64(1.0))),
            Box::new(Expr::Const(ScalarValue::F64(1.0))),
        );
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(1.0));

        // 1 and 0 = 0 (false)
        let body = Expr::And(
            Box::new(Expr::Const(ScalarValue::F64(1.0))),
            Box::new(Expr::Const(ScalarValue::F64(0.0))),
        );
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(0.0));

        // 0 or 1 = 1
        let body = Expr::Or(
            Box::new(Expr::Const(ScalarValue::F64(0.0))),
            Box::new(Expr::Const(ScalarValue::F64(1.0))),
        );
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(1.0));

        // not 0 = 1
        let body = Expr::Not(Box::new(Expr::Const(ScalarValue::F64(0.0))));
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert_eq!(v.as_f64(), Some(1.0));

        // not Null = Null
        let body = Expr::Not(Box::new(Expr::Const(ScalarValue::Null)));
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert!(v.is_null());

        // Null and 1 = Null
        let body = Expr::And(
            Box::new(Expr::Const(ScalarValue::Null)),
            Box::new(Expr::Const(ScalarValue::F64(1.0))),
        );
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert!(v.is_null());
    }

    // -----------------------------------------------------------------------
    // Phase 3F: time-series eval with mock lookup_cross
    // -----------------------------------------------------------------------

    #[test]
    fn eval_prev_delegates_to_cross() {
        let measure = ElementId(1);
        let body = Expr::Prev(measure);
        let mut cross = |read: &CrossCoordRead| -> Result<ScalarValue, EngineError> {
            match read {
                CrossCoordRead::TimeOffset { offset, measure: m } => {
                    assert_eq!(*offset, -1);
                    assert_eq!(*m, measure);
                    Ok(ScalarValue::F64(100.0))
                }
                _ => Ok(ScalarValue::Null),
            }
        };
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut cross).unwrap();
        assert_eq!(v.as_f64(), Some(100.0));
    }

    #[test]
    fn eval_lag_with_negative_leads() {
        let measure = ElementId(1);
        // lag(measure, -2) → TimeOffset { offset: 2 } (lead 2 periods)
        let body = Expr::Lag(measure, Box::new(Expr::Const(ScalarValue::F64(-2.0))));
        let mut cross = |read: &CrossCoordRead| -> Result<ScalarValue, EngineError> {
            match read {
                CrossCoordRead::TimeOffset { offset, .. } => {
                    assert_eq!(*offset, 2); // negative lag = positive offset (lead)
                    Ok(ScalarValue::F64(200.0))
                }
                _ => Ok(ScalarValue::Null),
            }
        };
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut cross).unwrap();
        assert_eq!(v.as_f64(), Some(200.0));
    }

    #[test]
    fn eval_period_index_delegates() {
        let body = Expr::PeriodIndex;
        let mut cross = |read: &CrossCoordRead| -> Result<ScalarValue, EngineError> {
            match read {
                CrossCoordRead::PeriodIndex => Ok(ScalarValue::F64(3.0)),
                _ => Ok(ScalarValue::Null),
            }
        };
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut cross).unwrap();
        assert_eq!(v.as_f64(), Some(3.0));
    }

    #[test]
    fn eval_cumulative_delegates() {
        let measure = ElementId(5);
        let body = Expr::Cumulative(measure);
        let mut cross = |read: &CrossCoordRead| -> Result<ScalarValue, EngineError> {
            match read {
                CrossCoordRead::Cumulative { measure: m } => {
                    assert_eq!(*m, measure);
                    Ok(ScalarValue::F64(500.0))
                }
                _ => Ok(ScalarValue::Null),
            }
        };
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut cross).unwrap();
        assert_eq!(v.as_f64(), Some(500.0));
    }

    #[test]
    fn eval_rolling_avg_delegates() {
        let measure = ElementId(5);
        let body = Expr::RollingAvg(measure, Box::new(Expr::Const(ScalarValue::F64(3.0))));
        let mut cross = |read: &CrossCoordRead| -> Result<ScalarValue, EngineError> {
            match read {
                CrossCoordRead::RollingAvg { measure: m, window } => {
                    assert_eq!(*m, measure);
                    assert_eq!(*window, 3);
                    Ok(ScalarValue::F64(7.5))
                }
                _ => Ok(ScalarValue::Null),
            }
        };
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut cross).unwrap();
        assert_eq!(v.as_f64(), Some(7.5));
    }

    #[test]
    fn eval_rolling_avg_non_positive_window_returns_null() {
        let measure = ElementId(5);
        // window = 0 → Null (non-positive)
        let body = Expr::RollingAvg(measure, Box::new(Expr::Const(ScalarValue::F64(0.0))));
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert!(v.is_null());
    }

    // -----------------------------------------------------------------------
    // Phase 3G: reference-data eval with mock lookup_cross
    // -----------------------------------------------------------------------

    #[test]
    fn eval_benchmark_delegates() {
        let body = Expr::Benchmark(
            "industry_cpc".into(),
            Box::new(Expr::Const(ScalarValue::F64(1.0))), // key placeholder
        );
        let mut cross = |read: &CrossCoordRead| -> Result<ScalarValue, EngineError> {
            match read {
                CrossCoordRead::BenchmarkLookup { name, .. } => {
                    assert_eq!(name, "industry_cpc");
                    Ok(ScalarValue::F64(5.50))
                }
                _ => Ok(ScalarValue::Null),
            }
        };
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut cross).unwrap();
        assert_eq!(v.as_f64(), Some(5.50));
    }

    #[test]
    fn eval_lookup_delegates() {
        let body = Expr::Lookup(
            "tax_rate".into(),
            vec![Box::new(Expr::Const(ScalarValue::F64(1.0)))],
        );
        let mut cross = |read: &CrossCoordRead| -> Result<ScalarValue, EngineError> {
            match read {
                CrossCoordRead::TableLookup { table, .. } => {
                    assert_eq!(table, "tax_rate");
                    Ok(ScalarValue::F64(0.055))
                }
                _ => Ok(ScalarValue::Null),
            }
        };
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut cross).unwrap();
        assert_eq!(v.as_f64(), Some(0.055));
    }

    #[test]
    fn eval_bucket_null_input_returns_null() {
        let body = Expr::Bucket(
            Box::new(Expr::Const(ScalarValue::Null)),
            "cpc_health".into(),
        );
        let mut cross = |read: &CrossCoordRead| -> Result<ScalarValue, EngineError> {
            match read {
                CrossCoordRead::BucketLookup { value, .. } => {
                    // value is Null → bucket returns Null
                    assert!(value.is_null());
                    Ok(ScalarValue::Null)
                }
                _ => Ok(ScalarValue::Null),
            }
        };
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut cross).unwrap();
        assert!(v.is_null());
    }

    #[test]
    fn eval_bucket_delegates_value() {
        let body = Expr::Bucket(
            Box::new(Expr::Const(ScalarValue::F64(4.5))),
            "cpc_health".into(),
        );
        let mut cross = |read: &CrossCoordRead| -> Result<ScalarValue, EngineError> {
            match read {
                CrossCoordRead::BucketLookup { threshold, value } => {
                    assert_eq!(threshold, "cpc_health");
                    assert_eq!(value.as_f64(), Some(4.5));
                    Ok(ScalarValue::F64(1.0)) // band index 1 ("Warning")
                }
                _ => Ok(ScalarValue::Null),
            }
        };
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut cross).unwrap();
        assert_eq!(v.as_f64(), Some(1.0));
    }

    #[test]
    fn eval_sum_over_delegates() {
        let dim = crate::id::DimensionId(99);
        let measure = ElementId(10);
        let body = Expr::SumOver(dim, measure);
        let mut cross = |read: &CrossCoordRead| -> Result<ScalarValue, EngineError> {
            match read {
                CrossCoordRead::DimensionScan {
                    dimension: d,
                    measure: m,
                } => {
                    assert_eq!(*d, dim);
                    assert_eq!(*m, measure);
                    Ok(ScalarValue::F64(1000.0)) // total across dimension
                }
                _ => Ok(ScalarValue::Null),
            }
        };
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut cross).unwrap();
        assert_eq!(v.as_f64(), Some(1000.0));
    }

    // -----------------------------------------------------------------------
    // Phase 3F.1: Anchor function eval tests
    // -----------------------------------------------------------------------

    #[test]
    fn eval_anchor_index_delegates() {
        let body = Expr::AnchorIndex;
        let mut cross = |read: &CrossCoordRead| -> Result<ScalarValue, EngineError> {
            match read {
                CrossCoordRead::AnchorIndex => Ok(ScalarValue::F64(5.0)),
                _ => Ok(ScalarValue::Null),
            }
        };
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut cross).unwrap();
        assert_eq!(v.as_f64(), Some(5.0));
    }

    #[test]
    fn eval_is_past_delegates() {
        let body = Expr::IsPast;
        let mut cross = |read: &CrossCoordRead| -> Result<ScalarValue, EngineError> {
            match read {
                CrossCoordRead::IsPast => Ok(ScalarValue::F64(1.0)),
                _ => Ok(ScalarValue::Null),
            }
        };
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut cross).unwrap();
        assert_eq!(v.as_f64(), Some(1.0));
    }

    #[test]
    fn eval_is_current_delegates() {
        let body = Expr::IsCurrent;
        let mut cross = |read: &CrossCoordRead| -> Result<ScalarValue, EngineError> {
            match read {
                CrossCoordRead::IsCurrent => Ok(ScalarValue::F64(0.0)),
                _ => Ok(ScalarValue::Null),
            }
        };
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut cross).unwrap();
        assert_eq!(v.as_f64(), Some(0.0));
    }

    #[test]
    fn eval_periods_since_anchor_delegates() {
        let body = Expr::PeriodsSinceAnchor;
        let mut cross = |read: &CrossCoordRead| -> Result<ScalarValue, EngineError> {
            match read {
                CrossCoordRead::PeriodsSinceAnchor => Ok(ScalarValue::F64(-3.0)),
                _ => Ok(ScalarValue::Null),
            }
        };
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut cross).unwrap();
        assert_eq!(v.as_f64(), Some(-3.0));
    }

    #[test]
    fn eval_periods_to_end_delegates() {
        let body = Expr::PeriodsToEnd;
        let mut cross = |read: &CrossCoordRead| -> Result<ScalarValue, EngineError> {
            match read {
                CrossCoordRead::PeriodsToEnd => Ok(ScalarValue::F64(7.0)),
                _ => Ok(ScalarValue::Null),
            }
        };
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut cross).unwrap();
        assert_eq!(v.as_f64(), Some(7.0));
    }

    // -----------------------------------------------------------------------
    // Phase 3H: Fitted-model evaluation tests
    // -----------------------------------------------------------------------

    #[test]
    fn eval_exp_zero() {
        let body = Expr::Exp(Box::new(Expr::Const(ScalarValue::F64(0.0))));
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert!((v.as_f64().unwrap() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn eval_exp_one() {
        let body = Expr::Exp(Box::new(Expr::Const(ScalarValue::F64(1.0))));
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert!((v.as_f64().unwrap() - std::f64::consts::E).abs() < 1e-9);
    }

    #[test]
    fn eval_exp_negative() {
        let body = Expr::Exp(Box::new(Expr::Const(ScalarValue::F64(-1.0))));
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert!((v.as_f64().unwrap() - 0.367879441171442).abs() < 1e-9);
    }

    #[test]
    fn eval_exp_null_returns_null() {
        let body = Expr::Exp(Box::new(Expr::Const(ScalarValue::Null)));
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert!(v.is_null());
    }

    #[test]
    fn eval_norm_cdf_standard_at_zero() {
        // norm_cdf(0, 0, 1) ≈ 0.5
        let body = Expr::NormCdf(
            Box::new(Expr::Const(ScalarValue::F64(0.0))),
            Box::new(Expr::Const(ScalarValue::F64(0.0))),
            Box::new(Expr::Const(ScalarValue::F64(1.0))),
        );
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert!((v.as_f64().unwrap() - 0.5).abs() < 1e-4);
    }

    #[test]
    fn eval_norm_cdf_1_96() {
        // norm_cdf(1.96, 0, 1) ≈ 0.975
        let body = Expr::NormCdf(
            Box::new(Expr::Const(ScalarValue::F64(1.96))),
            Box::new(Expr::Const(ScalarValue::F64(0.0))),
            Box::new(Expr::Const(ScalarValue::F64(1.0))),
        );
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert!((v.as_f64().unwrap() - 0.975).abs() < 1e-3);
    }

    #[test]
    fn eval_norm_cdf_negative_sigma_returns_null() {
        let body = Expr::NormCdf(
            Box::new(Expr::Const(ScalarValue::F64(0.0))),
            Box::new(Expr::Const(ScalarValue::F64(0.0))),
            Box::new(Expr::Const(ScalarValue::F64(-1.0))),
        );
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert!(v.is_null());
    }

    #[test]
    fn eval_norm_cdf_zero_sigma_returns_null() {
        let body = Expr::NormCdf(
            Box::new(Expr::Const(ScalarValue::F64(0.0))),
            Box::new(Expr::Const(ScalarValue::F64(0.0))),
            Box::new(Expr::Const(ScalarValue::F64(0.0))),
        );
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert!(v.is_null());
    }

    #[test]
    fn eval_norm_cdf_null_returns_null() {
        let body = Expr::NormCdf(
            Box::new(Expr::Const(ScalarValue::Null)),
            Box::new(Expr::Const(ScalarValue::F64(0.0))),
            Box::new(Expr::Const(ScalarValue::F64(1.0))),
        );
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert!(v.is_null());
    }

    #[test]
    fn eval_predict_delegates() {
        let body = Expr::Predict(
            "my_model".into(),
            vec![
                Box::new(Expr::Const(ScalarValue::F64(99.2))),
                Box::new(Expr::Const(ScalarValue::F64(113.4))),
            ],
        );
        let mut cross = |read: &CrossCoordRead| -> Result<ScalarValue, EngineError> {
            match read {
                CrossCoordRead::PredictModel { model_id, features } => {
                    assert_eq!(model_id, "my_model");
                    assert_eq!(features.len(), 2);
                    assert_eq!(features[0].as_f64(), Some(99.2));
                    assert_eq!(features[1].as_f64(), Some(113.4));
                    Ok(ScalarValue::F64(211.34))
                }
                _ => Ok(ScalarValue::Null),
            }
        };
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut cross).unwrap();
        assert_eq!(v.as_f64(), Some(211.34));
    }

    #[test]
    fn eval_predict_null_feature_returns_null() {
        let body = Expr::Predict(
            "m".into(),
            vec![
                Box::new(Expr::Const(ScalarValue::F64(1.0))),
                Box::new(Expr::Const(ScalarValue::Null)),
                Box::new(Expr::Const(ScalarValue::F64(3.0))),
            ],
        );
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert!(v.is_null());
    }

    #[test]
    fn eval_calibrate_delegates() {
        let body = Expr::Calibrate(
            Box::new(Expr::Const(ScalarValue::F64(0.55))),
            "my_map".into(),
        );
        let mut cross = |read: &CrossCoordRead| -> Result<ScalarValue, EngineError> {
            match read {
                CrossCoordRead::CalibrateMap { map_id, value } => {
                    assert_eq!(map_id, "my_map");
                    assert_eq!(value.as_f64(), Some(0.55));
                    Ok(ScalarValue::F64(0.46))
                }
                _ => Ok(ScalarValue::Null),
            }
        };
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut cross).unwrap();
        assert_eq!(v.as_f64(), Some(0.46));
    }

    #[test]
    fn eval_calibrate_null_returns_null() {
        let body = Expr::Calibrate(Box::new(Expr::Const(ScalarValue::Null)), "m".into());
        let v = eval_expr(&body, &mut |_| Ok(ScalarValue::Null), &mut no_cross).unwrap();
        assert!(v.is_null());
    }

    // Phase 6A.1 MIN-6 regression: eval_expr_unified_inner (the production
    // eval path called from cube.rs) must use 1e-9 epsilon for not() and if().
    // A near-zero value like 5e-10 is conceptually false/zero; under the old
    // `x == 0.0` check it would test as truthy.
    #[test]
    fn eval_unified_not_near_zero_is_true() {
        // 5e-10 < 1e-9 → not(5e-10) should be true (1.0)
        let body = Expr::Not(Box::new(Expr::Const(ScalarValue::F64(5e-10))));
        let v = eval_expr_unified(&body, &mut |_| Ok(ScalarValue::Null)).unwrap();
        assert!(
            (v.as_f64().unwrap() - 1.0).abs() < 1e-12,
            "not(5e-10) expected 1.0, got {:?}",
            v
        );
    }

    #[test]
    fn eval_unified_if_near_zero_takes_else_branch() {
        // if(5e-10, 99.0, 1.0) → condition is near-zero (falsy) → else = 1.0
        let body = Expr::If(
            Box::new(Expr::Const(ScalarValue::F64(5e-10))),
            Box::new(Expr::Const(ScalarValue::F64(99.0))),
            Box::new(Expr::Const(ScalarValue::F64(1.0))),
        );
        let v = eval_expr_unified(&body, &mut |_| Ok(ScalarValue::Null)).unwrap();
        assert!(
            (v.as_f64().unwrap() - 1.0).abs() < 1e-12,
            "if(5e-10, 99, 1) expected 1.0 (else branch), got {:?}",
            v
        );
    }
}
