//! Stage 3: `ValidatedModel` → `mc_core::Cube`.
//!
//! Walks the validated model in declaration order, allocates `mc_core`
//! IDs via a fresh `IdGenerator`, and assembles the cube via the kernel's
//! public builder API. Should not normally fail — by construction every
//! check the kernel performs has already passed in stage 2.
//!
//! Output is a [`CompiledCube`] bundling the cube, the root principal,
//! and a [`ModelRefs`] name → ID resolver. The CLI uses `ModelRefs` to
//! resolve the same coordinates `build_acme_cube()` exposes via
//! `AcmeRefs`.

use std::collections::BTreeMap;

use mc_core::{
    AggregationRule, CellCoordinate, CellDataType, CoordPattern, Cube, CubeBuilder, CubeId,
    DependencyDecl, Dimension, DimensionId, DimensionKind, Element, ElementId, EngineError, Expr,
    Hierarchy, IdGenerator, MeasureRole, PrincipalId, Rule, RuleId, ScalarValue, ScenarioMeta,
    Scope, VersionState,
};

use crate::schema::{ParsedMeasure, ParsedRuleBody, ParsedScalar, ValidatedModel};

/// Bundle returned by `compile`. The cube is fully built; `refs` lets
/// callers resolve dim / element / measure / rule names back to IDs without
/// rescanning the cube.
#[derive(Debug)]
pub struct CompiledCube {
    pub cube: Cube,
    pub root_principal: PrincipalId,
    pub refs: ModelRefs,
}

/// Name → kernel-ID lookup tables. Mirrors the role of `mc_fixtures::AcmeRefs`
/// for YAML-loaded cubes; built unconditionally so a CLI / test layer can
/// resolve coordinates without re-querying the cube's dimensions.
#[derive(Clone, Debug)]
pub struct ModelRefs {
    pub cube_id: CubeId,
    /// Dim-name → DimensionId.
    pub dimensions: BTreeMap<String, DimensionId>,
    /// (dim-name, element-name) → ElementId.
    pub elements: BTreeMap<(String, String), ElementId>,
    /// Rule-name → RuleId.
    pub rules: BTreeMap<String, RuleId>,
    /// Ordered list of dim names in the cube's dimension order.
    pub dimension_order: Vec<String>,
}

impl ModelRefs {
    /// Resolve `(dim_name, element_name)` to an `ElementId`. Returns
    /// `None` if either name is unknown.
    pub fn element(&self, dim: &str, element: &str) -> Option<ElementId> {
        self.elements
            .get(&(dim.to_string(), element.to_string()))
            .copied()
    }

    /// Build a `CellCoordinate` from a name-keyed map. The order of
    /// elements in the returned coord matches `self.dimension_order`.
    /// Returns `None` if any dim or element name is unknown, or if the
    /// map is missing a slot.
    pub fn coord_from_names(&self, names: &BTreeMap<String, String>) -> Option<CellCoordinate> {
        let mut slots: Vec<ElementId> = Vec::with_capacity(self.dimension_order.len());
        for dim in &self.dimension_order {
            let elem_name = names.get(dim)?;
            slots.push(self.element(dim, elem_name)?);
        }
        Some(CellCoordinate::from_parts(self.cube_id, slots))
    }
}

/// Compile a `ValidatedModel` into a `Cube`.
///
/// Per ADR-0004 Decision 9 + the handoff: this stage cannot fail except
/// for `EngineError::Internal`-class problems (the validator pre-clears
/// every kernel surface that returns a structured error). When it does
/// fail, we propagate the kernel error as-is.
pub fn compile(validated: ValidatedModel) -> Result<CompiledCube, EngineError> {
    let g = IdGenerator::new();
    let cube_id = g.cube();
    let root_principal = g.principal();

    let mut refs = ModelRefs {
        cube_id,
        dimensions: BTreeMap::new(),
        elements: BTreeMap::new(),
        rules: BTreeMap::new(),
        dimension_order: validated
            .parsed
            .dimensions
            .iter()
            .map(|d| d.name.clone())
            .collect(),
    };

    // Pre-allocate every element ID for every dim. We do this up front so
    // hierarchies (which reference elements by name) and measure
    // weighted-aggregations (which reference *other* measures by name)
    // can resolve their cross-references during the build walk.
    let mut element_ids_by_dim: Vec<Vec<ElementId>> =
        Vec::with_capacity(validated.parsed.dimensions.len());
    let mut dim_ids: Vec<DimensionId> = Vec::with_capacity(validated.parsed.dimensions.len());
    for dim in &validated.parsed.dimensions {
        let dim_id = g.dimension();
        dim_ids.push(dim_id);
        refs.dimensions.insert(dim.name.clone(), dim_id);

        let element_count = if dim.kind == "Measure" {
            // Measure dim's elements come from the top-level `measures:`
            // block, not from `dim.elements`. The validator already
            // surfaced any inline-element-list-on-Measure-dim mistakes.
            validated.parsed.measures.len()
        } else {
            dim.elements.len()
        };
        let mut ids = Vec::with_capacity(element_count);
        for _ in 0..element_count {
            ids.push(g.element());
        }

        if dim.kind == "Measure" {
            for (i, m) in validated.parsed.measures.iter().enumerate() {
                refs.elements
                    .insert((dim.name.clone(), m.name.clone()), ids[i]);
            }
        } else {
            for (i, e) in dim.elements.iter().enumerate() {
                refs.elements
                    .insert((dim.name.clone(), e.name.clone()), ids[i]);
            }
        }
        element_ids_by_dim.push(ids);
    }

    // ---- Build the dimensions ----
    let mut built_dims: Vec<Dimension> = Vec::with_capacity(validated.parsed.dimensions.len());
    for (i, dim) in validated.parsed.dimensions.iter().enumerate() {
        let dim_id = dim_ids[i];
        let element_ids = &element_ids_by_dim[i];

        let kind = parse_dim_kind(&dim.kind)?;
        let mut builder = Dimension::builder(dim_id, dim.name.clone(), kind);

        if dim.kind == "Measure" {
            // Measure-dim elements come from `validated.parsed.measures`.
            for (m_idx, measure) in validated.parsed.measures.iter().enumerate() {
                let agg = compile_aggregation(measure, &refs, &dim.name)?;
                let dtype = compile_data_type(measure)?;
                let role = match measure.role.as_str() {
                    "Input" => MeasureRole::Input,
                    "Derived" => MeasureRole::Derived,
                    _ => {
                        return Err(EngineError::Internal(
                            "compile: validator missed an unknown measure role",
                        ))
                    }
                };
                let elem = Element::measure(
                    element_ids[m_idx],
                    measure.name.clone(),
                    dim_id,
                    dtype,
                    role,
                    agg,
                );
                builder = builder.add_element(elem)?;
            }
        } else {
            for (e_idx, e) in dim.elements.iter().enumerate() {
                let elem = build_typed_element(
                    element_ids[e_idx],
                    &e.name,
                    dim_id,
                    &dim.kind,
                    e.version_state.as_deref(),
                    e.scenario_meta.as_deref(),
                )?;
                builder = builder.add_element(elem)?;
            }
        }

        // Hierarchies for this dim (Measure / Scenario / Version dims have
        // none in the Acme schema; the loop is just empty for them).
        let hierarchies_for_dim: Vec<&crate::schema::ParsedHierarchy> = validated
            .parsed
            .hierarchies
            .iter()
            .filter(|h| h.dimension == dim.name)
            .collect();

        if !hierarchies_for_dim.is_empty() {
            for h in &hierarchies_for_dim {
                let hier_id = g.hierarchy();
                let mut hb = Hierarchy::builder(hier_id, h.name.clone(), dim_id);
                for edge in &h.edges {
                    let parent =
                        refs.element(&dim.name, &edge.parent)
                            .ok_or(EngineError::Internal(
                                "compile: validator missed an unknown hierarchy-edge parent",
                            ))?;
                    let child =
                        refs.element(&dim.name, &edge.child)
                            .ok_or(EngineError::Internal(
                                "compile: validator missed an unknown hierarchy-edge child",
                            ))?;
                    hb = hb.add_edge(parent, child, edge.weight);
                }
                let hier = hb.build()?;
                builder = builder.add_hierarchy(hier)?;
            }
            // Pick the default: explicit `default: true` flag, or first.
            let default_h = hierarchies_for_dim
                .iter()
                .copied()
                .find(|h| h.default == Some(true))
                .or_else(|| hierarchies_for_dim.first().copied());
            if let Some(h) = default_h {
                builder = builder.default_hierarchy(h.name.clone());
            }
        }

        built_dims.push(builder.build()?);
    }

    // ---- Build the cube via CubeBuilder ----
    let cube_name = validated.parsed.metadata.name.clone();
    let mut cb: CubeBuilder = Cube::builder(cube_id, cube_name);
    let measure_dim_name = validated
        .parsed
        .dimensions
        .get(validated.measure_dim_index)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "Measure".to_string());
    for d in built_dims {
        cb = cb.add_dimension(d);
    }
    cb = cb
        .measure_dimension(measure_dim_name)
        .root_principal(root_principal);

    // ---- Build the rules ----
    // Phase 3D: walk validated.rules (flat ParsedRuleBody) instead of
    // validated.parsed.rules (which carries the ParsedRuleBodyForm
    // wrapper). The rules vec is built in validate() and matches
    // parsed.rules order 1:1.
    for rule in &validated.rules {
        let rule_id = g.rule();
        refs.rules.insert(rule.name.clone(), rule_id);
        let target = lookup_measure_id(&refs, &validated, &rule.target_measure)?;
        let body = compile_expr(&rule.body, &refs, &validated)?;
        let scope = match rule.scope.as_str() {
            "AllLeaves" => Scope::AllLeaves,
            _ => {
                return Err(EngineError::Internal(
                    "compile: validator missed an unknown rule scope",
                ))
            }
        };
        let declared_dependencies: Vec<DependencyDecl> = rule
            .declared_dependencies
            .iter()
            .map(|m_name| {
                let measure = lookup_measure_id(&refs, &validated, m_name)?;
                Ok::<_, EngineError>(DependencyDecl {
                    measure,
                    coord_pattern: CoordPattern::SameAsTarget,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let r = Rule {
            id: rule_id,
            cube: cube_id,
            target_measure: target,
            scope,
            body,
            declared_dependencies,
        };
        cb = cb.add_rule(r)?;
    }

    let mut cube = cb.build()?;

    // Populate reference data (benchmarks, lookup_tables, thresholds) so
    // the eval layer can resolve cross-coordinate reads.
    for bm in &validated.parsed.benchmarks {
        let table: ahash::AHashMap<String, f64> =
            bm.values.iter().map(|(k, v)| (k.clone(), *v)).collect();
        cube.reference_data
            .benchmarks
            .insert(bm.name.clone(), table);
    }
    for lt in &validated.parsed.lookup_tables {
        let table: ahash::AHashMap<String, f64> =
            lt.values.iter().map(|(k, v)| (k.clone(), *v)).collect();
        cube.reference_data
            .lookup_tables
            .insert(lt.name.clone(), table);
    }
    for st in &validated.parsed.status_thresholds {
        let bands: Vec<mc_core::ThresholdBand> = st
            .bands
            .iter()
            .map(|b| mc_core::ThresholdBand {
                label: b.label.clone(),
                max: b.max,
            })
            .collect();
        cube.reference_data
            .thresholds
            .insert(st.name.clone(), bands);
    }

    // Populate fitted_models
    for fm in &validated.parsed.fitted_models {
        let standardization = fm.standardization.as_ref().map(|sc| {
            sc.params.iter().map(|p| (p.mean, p.std)).collect()
        });
        let data = mc_core::FittedModelData {
            method: fm.method.clone(),
            intercept: fm.intercept,
            coefficients: fm.coefficients.iter().map(|c| c.weight).collect(),
            residual_std: fm.residual_std,
            standardization,
        };
        cube.reference_data.fitted_models.insert(fm.name.clone(), data);
    }

    // Populate calibration_maps
    for cm in &validated.parsed.calibration_maps {
        let data = mc_core::CalibrationMapData {
            method: cm.method.clone(),
            points: cm
                .points
                .as_ref()
                .map(|pts| pts.iter().map(|p| (p.raw, p.calibrated)).collect())
                .unwrap_or_default(),
            platt_a: cm.platt_params.as_ref().map(|p| p.a),
            platt_b: cm.platt_params.as_ref().map(|p| p.b),
        };
        cube.reference_data
            .calibration_maps
            .insert(cm.name.clone(), data);
    }

    // Populate time_anchor_index from the Time-kind dimension's time_anchor field.
    // The Time dim is identified by kind: "Time" (or kind: "Standard" with name "Time").
    for dim in &validated.parsed.dimensions {
        let is_time = dim.kind == "Time";
        if is_time {
            if let Some(ref anchor_name) = dim.time_anchor {
                // Find the element index that matches the anchor name
                for (idx, elem) in dim.elements.iter().enumerate() {
                    if elem.name == *anchor_name {
                        cube.reference_data.time_anchor_index = Some(idx);
                        break;
                    }
                }
            }
            break; // Only one Time-kind dim per ADR-0014
        }
    }

    Ok(CompiledCube {
        cube,
        root_principal,
        refs,
    })
}

fn parse_dim_kind(s: &str) -> Result<DimensionKind, EngineError> {
    match s {
        "Standard" | "Time" => Ok(DimensionKind::Standard),
        "Measure" => Ok(DimensionKind::Measure),
        "Scenario" => Ok(DimensionKind::Scenario),
        "Version" => Ok(DimensionKind::Version),
        _ => Err(EngineError::Internal(
            "compile: validator missed an unknown dimension kind",
        )),
    }
}

fn build_typed_element(
    id: ElementId,
    name: &str,
    dim_id: DimensionId,
    kind: &str,
    version_state: Option<&str>,
    scenario_meta: Option<&str>,
) -> Result<Element, EngineError> {
    match kind {
        "Standard" | "Time" => Ok(Element::leaf(id, name, dim_id)),
        "Version" => {
            let state = match version_state.unwrap_or("Draft") {
                "Draft" => VersionState::Draft,
                "Submitted" => VersionState::Submitted,
                "Approved" => VersionState::Approved,
                "Archived" => VersionState::Archived,
                _ => {
                    return Err(EngineError::Internal(
                        "compile: validator missed an unknown version_state",
                    ))
                }
            };
            Ok(Element::version(id, name, dim_id, state))
        }
        "Scenario" => {
            let meta = match scenario_meta.unwrap_or("NonDefault") {
                "Default" => ScenarioMeta::Default,
                "NonDefault" => ScenarioMeta::NonDefault,
                _ => {
                    return Err(EngineError::Internal(
                        "compile: validator missed an unknown scenario_meta",
                    ))
                }
            };
            Ok(Element::scenario(id, name, dim_id, meta))
        }
        "Measure" => Err(EngineError::Internal(
            "compile: build_typed_element called with Measure kind",
        )),
        _ => Err(EngineError::Internal(
            "compile: validator missed an unknown dimension kind",
        )),
    }
}

fn compile_aggregation(
    measure: &ParsedMeasure,
    refs: &ModelRefs,
    measure_dim_name: &str,
) -> Result<AggregationRule, EngineError> {
    match measure.aggregation.as_str() {
        "Sum" => Ok(AggregationRule::Sum),
        "Min" => Ok(AggregationRule::Min),
        "Max" => Ok(AggregationRule::Max),
        "WeightedAverage" => {
            let weight_name = measure
                .weight_measure
                .as_ref()
                .ok_or(EngineError::Internal(
                    "compile: validator missed WeightedAverage with no weight_measure",
                ))?;
            let weight_id =
                refs.element(measure_dim_name, weight_name)
                    .ok_or(EngineError::Internal(
                        "compile: validator missed an unknown weight_measure reference",
                    ))?;
            Ok(AggregationRule::WeightedAverage {
                weight_measure: weight_id,
            })
        }
        _ => Err(EngineError::Internal(
            "compile: validator missed an unsupported aggregation method",
        )),
    }
}

fn compile_data_type(measure: &ParsedMeasure) -> Result<CellDataType, EngineError> {
    match measure.data_type.as_str() {
        "F64" => Ok(CellDataType::F64),
        "I64" => Ok(CellDataType::I64),
        "Bool" => Ok(CellDataType::Bool),
        "Category" => {
            let domain = measure
                .category_domain
                .as_ref()
                .ok_or(EngineError::Internal(
                    "compile: validator missed Category measure with no domain",
                ))?;
            Ok(CellDataType::Category(domain.clone()))
        }
        _ => Err(EngineError::Internal(
            "compile: validator missed an unknown data_type",
        )),
    }
}

fn lookup_measure_id(
    refs: &ModelRefs,
    validated: &ValidatedModel,
    name: &str,
) -> Result<ElementId, EngineError> {
    let measure_dim = validated
        .parsed
        .dimensions
        .get(validated.measure_dim_index)
        .map(|d| d.name.as_str())
        .unwrap_or("Measure");
    refs.element(measure_dim, name).ok_or(EngineError::Internal(
        "compile: validator missed an unresolved measure name",
    ))
}

fn compile_expr(
    body: &ParsedRuleBody,
    refs: &ModelRefs,
    validated: &ValidatedModel,
) -> Result<Expr, EngineError> {
    match body {
        ParsedRuleBody::Const(c) => Ok(Expr::Const(match &c.value {
            ParsedScalar::Float(v) => ScalarValue::F64(*v),
            ParsedScalar::Int(v) => ScalarValue::I64(*v),
            ParsedScalar::Bool(v) => ScalarValue::Bool(*v),
            ParsedScalar::Null => ScalarValue::Null,
        })),
        ParsedRuleBody::Ref(r) => {
            // If the name matches a dimension, compile to DimElement (resolves
            // to the current element's name at eval time — used in lookup/benchmark keys).
            if let Some(&dim_id) = refs.dimensions.get(&r.measure) {
                Ok(Expr::DimElement(dim_id))
            } else {
                Ok(Expr::SelfRef(lookup_measure_id(
                    refs, validated, &r.measure,
                )?))
            }
        }
        ParsedRuleBody::Add(b) => binop(&b.add, refs, validated, Expr::Add),
        ParsedRuleBody::Sub(b) => binop(&b.sub, refs, validated, Expr::Sub),
        ParsedRuleBody::Mul(b) => binop(&b.mul, refs, validated, Expr::Mul),
        ParsedRuleBody::Div(b) => binop(&b.div, refs, validated, Expr::Div),
        ParsedRuleBody::IfNull(b) => binop(&b.if_null, refs, validated, Expr::IfNull),
        // Phase 3E: comparisons
        ParsedRuleBody::Gt(b) => compile_binop_pair(&b.left, &b.right, refs, validated, Expr::Gt),
        ParsedRuleBody::Lt(b) => compile_binop_pair(&b.left, &b.right, refs, validated, Expr::Lt),
        ParsedRuleBody::Gte(b) => compile_binop_pair(&b.left, &b.right, refs, validated, Expr::Gte),
        ParsedRuleBody::Lte(b) => compile_binop_pair(&b.left, &b.right, refs, validated, Expr::Lte),
        ParsedRuleBody::Eq(b) => compile_binop_pair(&b.left, &b.right, refs, validated, Expr::Eq),
        ParsedRuleBody::Neq(b) => compile_binop_pair(&b.left, &b.right, refs, validated, Expr::Neq),
        // Phase 3E: logical
        ParsedRuleBody::And(b) => compile_binop_pair(&b.left, &b.right, refs, validated, Expr::And),
        ParsedRuleBody::Or(b) => compile_binop_pair(&b.left, &b.right, refs, validated, Expr::Or),
        ParsedRuleBody::Not(b) => {
            let inner = compile_expr(&b.operand, refs, validated)?;
            Ok(Expr::Not(Box::new(inner)))
        }
        // Phase 3E: functions
        ParsedRuleBody::If(b) => {
            let cond = compile_expr(&b.condition, refs, validated)?;
            let then_b = compile_expr(&b.then_branch, refs, validated)?;
            let else_b = compile_expr(&b.else_branch, refs, validated)?;
            Ok(Expr::If(Box::new(cond), Box::new(then_b), Box::new(else_b)))
        }
        ParsedRuleBody::Min(b) => {
            let args = b
                .args
                .iter()
                .map(|a| compile_expr(a, refs, validated).map(Box::new))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expr::Min(args))
        }
        ParsedRuleBody::Max(b) => {
            let args = b
                .args
                .iter()
                .map(|a| compile_expr(a, refs, validated).map(Box::new))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expr::Max(args))
        }
        ParsedRuleBody::Abs(b) => {
            let inner = compile_expr(&b.operand, refs, validated)?;
            Ok(Expr::Abs(Box::new(inner)))
        }
        ParsedRuleBody::SafeDiv(b) => {
            let n = compile_expr(&b.numerator, refs, validated)?;
            let d = compile_expr(&b.denominator, refs, validated)?;
            let def = compile_expr(&b.default, refs, validated)?;
            Ok(Expr::SafeDiv(Box::new(n), Box::new(d), Box::new(def)))
        }
        ParsedRuleBody::Clamp(b) => {
            let v = compile_expr(&b.value, refs, validated)?;
            let lo = compile_expr(&b.lo, refs, validated)?;
            let hi = compile_expr(&b.hi, refs, validated)?;
            Ok(Expr::Clamp(Box::new(v), Box::new(lo), Box::new(hi)))
        }
        ParsedRuleBody::Coalesce(b) => {
            let args = b
                .args
                .iter()
                .map(|a| compile_expr(a, refs, validated).map(Box::new))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expr::Coalesce(args))
        }
        ParsedRuleBody::ActualRef(b) => {
            let measure = lookup_measure_id(refs, validated, &b.measure)?;
            Ok(Expr::ActualRef(measure))
        }
        // Phase 3F: time-series
        ParsedRuleBody::Prev(b) => {
            let measure = lookup_measure_id(refs, validated, &b.measure)?;
            Ok(Expr::Prev(measure))
        }
        ParsedRuleBody::Lag(b) => {
            let measure = lookup_measure_id(refs, validated, &b.measure)?;
            let periods = compile_expr(&b.periods, refs, validated)?;
            Ok(Expr::Lag(measure, Box::new(periods)))
        }
        ParsedRuleBody::Cumulative(b) => {
            let measure = lookup_measure_id(refs, validated, &b.measure)?;
            Ok(Expr::Cumulative(measure))
        }
        ParsedRuleBody::RollingAvg(b) => {
            let measure = lookup_measure_id(refs, validated, &b.measure)?;
            let window = compile_expr(&b.window, refs, validated)?;
            Ok(Expr::RollingAvg(measure, Box::new(window)))
        }
        ParsedRuleBody::PeriodIndex(_) => Ok(Expr::PeriodIndex),
        ParsedRuleBody::AnchorIndex(_) => Ok(Expr::AnchorIndex),
        ParsedRuleBody::IsPast(_) => Ok(Expr::IsPast),
        ParsedRuleBody::IsCurrent(_) => Ok(Expr::IsCurrent),
        ParsedRuleBody::IsFuture(_) => Ok(Expr::IsFuture),
        ParsedRuleBody::PeriodsSinceAnchor(_) => Ok(Expr::PeriodsSinceAnchor),
        ParsedRuleBody::PeriodsToEnd(_) => Ok(Expr::PeriodsToEnd),
        // Phase 3G: reference-data
        ParsedRuleBody::Benchmark(b) => {
            let key = compile_expr(&b.key_expr, refs, validated)?;
            Ok(Expr::Benchmark(b.name.clone(), Box::new(key)))
        }
        ParsedRuleBody::Lookup(b) => {
            let key = compile_expr(&b.key_expr, refs, validated)?;
            Ok(Expr::Lookup(b.table.clone(), Box::new(key)))
        }
        ParsedRuleBody::Bucket(b) => {
            let v = compile_expr(&b.value, refs, validated)?;
            Ok(Expr::Bucket(Box::new(v), b.threshold_name.clone()))
        }
        ParsedRuleBody::SumOver(b) => {
            // Resolve dimension name to DimensionId
            let dim_id =
                refs.dimensions
                    .get(&b.dimension)
                    .copied()
                    .ok_or(EngineError::Internal(
                        "compile: sum_over references unknown dimension",
                    ))?;
            let measure = lookup_measure_id(refs, validated, &b.measure)?;
            Ok(Expr::SumOver(dim_id, measure))
        }
        // Phase 3H: fitted-model evaluation
        ParsedRuleBody::Predict(b) => {
            let features = b
                .features
                .iter()
                .map(|f| compile_expr(f, refs, validated).map(Box::new))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expr::Predict(b.model_id.clone(), features))
        }
        ParsedRuleBody::Calibrate(b) => {
            let v = compile_expr(&b.value, refs, validated)?;
            Ok(Expr::Calibrate(Box::new(v), b.map_id.clone()))
        }
        ParsedRuleBody::Exp(b) => {
            let inner = compile_expr(&b.operand, refs, validated)?;
            Ok(Expr::Exp(Box::new(inner)))
        }
        ParsedRuleBody::NormCdf(b) => {
            let x = compile_expr(&b.x, refs, validated)?;
            let mu = compile_expr(&b.mu, refs, validated)?;
            let sigma = compile_expr(&b.sigma, refs, validated)?;
            Ok(Expr::NormCdf(Box::new(x), Box::new(mu), Box::new(sigma)))
        }
    }
}

fn compile_binop_pair(
    left: &ParsedRuleBody,
    right: &ParsedRuleBody,
    refs: &ModelRefs,
    validated: &ValidatedModel,
    ctor: fn(Box<Expr>, Box<Expr>) -> Expr,
) -> Result<Expr, EngineError> {
    let lhs = compile_expr(left, refs, validated)?;
    let rhs = compile_expr(right, refs, validated)?;
    Ok(ctor(Box::new(lhs), Box::new(rhs)))
}

fn binop(
    args: &[ParsedRuleBody],
    refs: &ModelRefs,
    validated: &ValidatedModel,
    ctor: fn(Box<Expr>, Box<Expr>) -> Expr,
) -> Result<Expr, EngineError> {
    if args.len() != 2 {
        return Err(EngineError::Internal(
            "compile: validator missed binary-op arity (must be exactly 2 args)",
        ));
    }
    let lhs = compile_expr(&args[0], refs, validated)?;
    let rhs = compile_expr(&args[1], refs, validated)?;
    Ok(ctor(Box::new(lhs), Box::new(rhs)))
}
