//! Structural-equivalence test (per Phase 3A handoff item 7 + ADR-0004
//! success criterion #2).
//!
//! Loads `examples/acme.yaml` via `mc_model::load`, builds the canonical
//! cube via `mc_fixtures::build_acme_cube`, and asserts both share the
//! same *structure*: dim count, dim names, per-dim element counts,
//! hierarchy edge counts, measure metadata, rule count + body shape.
//!
//! This test does NOT compare every coordinate — that's the demo
//! byte-for-byte diff's job. Its purpose is to surface YAML-vs-Rust
//! drift as a focused failure (e.g., "rule_revenue body shape
//! Mul(SelfRef, SelfRef) != Mul(SelfRef, Sub(...))" rather than as an
//! opaque "demo output differs").

use std::collections::BTreeMap;

use mc_core::{AggregationRule, Expr, MeasureRole};
use mc_model::load;

#[test]
fn yaml_cube_matches_build_acme_cube_structurally() {
    // ---- Build both ----
    let compiled = load("examples/acme.yaml").unwrap_or_else(|errs| {
        for e in &errs {
            eprintln!("model error: {e}");
        }
        panic!("acme.yaml failed to load");
    });
    let yaml_cube = &compiled.cube;
    let (rust_cube, _refs) = mc_fixtures::build_acme_cube().expect("build_acme_cube");

    // ---- Dim count + dim names ----
    assert_eq!(
        yaml_cube.dimensions().len(),
        rust_cube.dimensions().len(),
        "dim count mismatch"
    );
    let yaml_names: Vec<&str> = yaml_cube
        .dimensions()
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    let rust_names: Vec<&str> = rust_cube
        .dimensions()
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    assert_eq!(yaml_names, rust_names, "dim name order mismatch");

    // ---- Per-dim element count + element name set ----
    for (yd, rd) in yaml_cube.dimensions().iter().zip(rust_cube.dimensions()) {
        assert_eq!(
            yd.elements.len(),
            rd.elements.len(),
            "dim {:?}: element count differs (yaml {} vs rust {})",
            yd.name,
            yd.elements.len(),
            rd.elements.len()
        );
        let yaml_elem_names: Vec<&str> = yd.elements.iter().map(|e| e.name.as_str()).collect();
        let rust_elem_names: Vec<&str> = rd.elements.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            yaml_elem_names, rust_elem_names,
            "dim {:?}: element name order differs",
            yd.name
        );
        assert_eq!(yd.kind, rd.kind, "dim {:?}: kind differs", yd.name);
    }

    // ---- Hierarchy edge counts ----
    for (yd, rd) in yaml_cube.dimensions().iter().zip(rust_cube.dimensions()) {
        let y_edges = yd.default_hierarchy().edges.len();
        let r_edges = rd.default_hierarchy().edges.len();
        assert_eq!(
            y_edges, r_edges,
            "dim {:?}: default-hierarchy edge count differs",
            yd.name
        );
    }

    // ---- Measure metadata: role, dtype, agg-by-name ----
    let yaml_measures = &yaml_cube.measure_dimension().elements;
    let rust_measures = &rust_cube.measure_dimension().elements;
    assert_eq!(yaml_measures.len(), rust_measures.len());
    let yaml_meta: BTreeMap<&str, (MeasureRole, &str)> = yaml_measures
        .iter()
        .filter_map(|e| {
            e.measure_meta()
                .map(|m| (e.name.as_str(), (m.role, agg_label(&m.aggregation))))
        })
        .collect();
    let rust_meta: BTreeMap<&str, (MeasureRole, &str)> = rust_measures
        .iter()
        .filter_map(|e| {
            e.measure_meta()
                .map(|m| (e.name.as_str(), (m.role, agg_label(&m.aggregation))))
        })
        .collect();
    assert_eq!(
        yaml_meta, rust_meta,
        "measure role/aggregation metadata differs between yaml and rust"
    );

    // ---- WeightedAverage weight_measure target name ----
    // Resolve weight_measure ElementId on both sides back to a measure
    // name; mismatched names = drift.
    let yaml_weights = collect_weights(yaml_cube);
    let rust_weights = collect_weights(&rust_cube);
    assert_eq!(yaml_weights, rust_weights, "weight-measure targets differ");

    // ---- Rule count + body shape ----
    assert_eq!(
        yaml_cube.rules().len(),
        rust_cube.rules().len(),
        "rule count differs"
    );
    // Compare rule bodies by structural shape (operators + measure-name
    // leaves), not by raw ElementId — the IDs are independently
    // allocated by each cube's IdGenerator so they won't be ==.
    let yaml_shapes = collect_rule_shapes(yaml_cube);
    let rust_shapes = collect_rule_shapes(&rust_cube);
    assert_eq!(
        yaml_shapes, rust_shapes,
        "rule body shapes differ between yaml and rust"
    );
}

fn agg_label(a: &AggregationRule) -> &'static str {
    match a {
        AggregationRule::Sum => "Sum",
        AggregationRule::WeightedAverage { .. } => "WeightedAverage",
        AggregationRule::Min => "Min",
        AggregationRule::Max => "Max",
    }
}

fn collect_weights(cube: &mc_core::Cube) -> BTreeMap<String, String> {
    let measures = &cube.measure_dimension().elements;
    let name_by_id: BTreeMap<mc_core::ElementId, &str> =
        measures.iter().map(|e| (e.id, e.name.as_str())).collect();
    let mut out = BTreeMap::new();
    for e in measures {
        if let Some(meta) = e.measure_meta() {
            if let AggregationRule::WeightedAverage { weight_measure } = &meta.aggregation {
                let weight_name = name_by_id
                    .get(weight_measure)
                    .copied()
                    .unwrap_or("<unknown>");
                out.insert(e.name.clone(), weight_name.to_string());
            }
        }
    }
    out
}

/// `(rule_target_name, rule_body_shape_string)` — captures every operator
/// node + every SelfRef-by-name + every Const value, as a flat string.
fn collect_rule_shapes(cube: &mc_core::Cube) -> BTreeMap<String, String> {
    let measure_dim = cube.measure_dimension();
    let name_by_id: BTreeMap<mc_core::ElementId, &str> = measure_dim
        .elements
        .iter()
        .map(|e| (e.id, e.name.as_str()))
        .collect();
    let mut out = BTreeMap::new();
    for r in cube.rules().iter() {
        let target = name_by_id
            .get(&r.target_measure)
            .copied()
            .unwrap_or("<unknown>");
        out.insert(target.to_string(), expr_shape(&r.body, &name_by_id));
    }
    out
}

fn expr_shape(e: &Expr, names: &BTreeMap<mc_core::ElementId, &str>) -> String {
    match e {
        Expr::Const(v) => format!("Const({v:?})"),
        Expr::SelfRef(id) => {
            let name = names.get(id).copied().unwrap_or("<unknown>");
            format!("SelfRef({name})")
        }
        Expr::Add(a, b) => format!("Add({}, {})", expr_shape(a, names), expr_shape(b, names)),
        Expr::Sub(a, b) => format!("Sub({}, {})", expr_shape(a, names), expr_shape(b, names)),
        Expr::Mul(a, b) => format!("Mul({}, {})", expr_shape(a, names), expr_shape(b, names)),
        Expr::Div(a, b) => format!("Div({}, {})", expr_shape(a, names), expr_shape(b, names)),
        Expr::IfNull(a, b) => format!("IfNull({}, {})", expr_shape(a, names), expr_shape(b, names)),
        // Phase 3E+: render using Debug for new variants
        other => format!("{other:?}"),
    }
}
