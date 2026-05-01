//! Consolidation: read a coordinate that includes one or more
//! consolidated (non-leaf) elements by walking the relevant hierarchies,
//! gathering leaf-level cell values, and combining them per the
//! measure's `AggregationRule`.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.17 and engine-semantics.md
//! §11.
//!
//! # Behavior contract
//!
//! - Sum: weighted sum (cumulative-weight product × leaf value); Null
//!   leaves contribute nothing; if every leaf is Null, result is Null.
//! - WeightedAverage(weight=W): numerator = Σ(value × weight), denominator
//!   = Σ(weight). Null value contributes nothing; Null weight contributes
//!   nothing to either numerator or denominator. Zero-total-weight → Null.
//! - Min / Max: ignore Nulls; if every leaf is Null, result is Null.
//!
//! Per spec §11 I-Cons-3 / §7 (null-poison policy).
//!
//! # Architecture
//!
//! Consolidation is a behavior, not a struct. The [`Consolidator`] type
//! is a marker on which the entry point lives so call sites read as
//! `Consolidator::read(...)`. The function takes a `read_leaf` closure
//! (the cube's recursive leaf-read) and a `dims`/`hierarchies` slice so
//! it has no direct dependency on `cube.rs`. `cube.rs` wires the closure
//! to its own `read_inner` path.

use smallvec::SmallVec;

use crate::cell::Provenance;
use crate::coordinate::CellCoordinate;
use crate::dimension::Dimension;
use crate::element::{AggregationRule, MeasureMeta};
use crate::error::EngineError;
use crate::hierarchy::Hierarchy;
use crate::id::{ElementId, HierarchyId, Revision};
use crate::value::ScalarValue;

/// Shape of a single leaf's contribution to a consolidation. Returned
/// by the caller-supplied `read_leaf` closure.
#[derive(Clone, Debug)]
pub struct LeafReadout {
    pub value: ScalarValue,
    /// The cumulative weight from the consolidated coord down to this
    /// leaf, multiplied across every consolidated hierarchy. For the
    /// Acme demo (all weights 1.0) this is always 1.0; for a weighted
    /// hierarchy (e.g., 30/70 split) it carries the actual product.
    pub weight: f64,
}

/// Marker for the consolidation entry point. Per spec §11.2:
/// "Consolidation is a behavior, not a struct."
#[derive(Debug)]
pub struct Consolidator;

impl Consolidator {
    /// Read a (potentially-)consolidated coordinate. If every dim slot
    /// is a leaf, returns `Ok(None)` so the caller can treat it as a
    /// leaf read; if at least one slot is consolidated, walks the
    /// hierarchies and returns `Ok(Some((value, provenance)))`.
    ///
    /// `dims` and `coord` must agree in length (one element per dim, in
    /// dim order). `hierarchies_by_dim` provides the default hierarchy
    /// for each dim — passed as a parallel slice to keep this function
    /// independent of `Cube`. `measure_meta` is the measure being
    /// consolidated; its `AggregationRule` selects the combining
    /// operation.
    ///
    /// `read_at` is invoked for **every** cell read this consolidation
    /// performs — both leaf-value reads and (for `WeightedAverage`)
    /// weight-measure reads at the sibling coord. The cube wires this
    /// to its own `read_inner` so dependency tracking applies to weight
    /// reads too.
    pub fn read<R>(
        coord: &CellCoordinate,
        dims: &[Dimension],
        hierarchies_by_dim: &[&Hierarchy],
        measure_position: usize,
        measure_meta: &MeasureMeta,
        read_at: &mut R,
        revision: Revision,
    ) -> Result<Option<(ScalarValue, Provenance)>, EngineError>
    where
        R: FnMut(&CellCoordinate) -> Result<ScalarValue, EngineError>,
    {
        debug_assert_eq!(dims.len(), coord.elements().len());
        debug_assert_eq!(dims.len(), hierarchies_by_dim.len());

        // Identify which dim slots are consolidated (i.e., the bound
        // element is a non-leaf in that dim's default hierarchy).
        // Phase 1: a slot's element is "leaf" iff hierarchy.is_leaf()
        // is true OR the hierarchy is the synthesized flat hierarchy
        // (no edges) — in the latter case every element is structurally
        // a leaf.
        let mut consolidated_dims: Vec<usize> = Vec::new();
        let mut visited_hierarchies: SmallVec<[HierarchyId; 4]> = SmallVec::new();
        for (i, h) in hierarchies_by_dim.iter().enumerate() {
            let element = coord.element_at(i);
            // A flat (synthesized) hierarchy has empty edges; every
            // element is structurally a leaf even though `is_leaf()`
            // returns false (its `leaves` set is empty by design — see
            // dimension.rs's synthesize step). Treat that as leaf.
            let is_flat_synthetic = h.edges.is_empty();
            if !is_flat_synthetic && h.is_consolidated(element) {
                consolidated_dims.push(i);
                visited_hierarchies.push(h.id);
            }
        }

        // Pure leaf — let the caller handle.
        if consolidated_dims.is_empty() {
            return Ok(None);
        }

        // Per spec §11 I-Cons-7: writes to consolidated coords are
        // rejected upstream. Reads of derived measures at consolidated
        // coords are always defined — no rule-based eval at consolidated
        // levels in Phase 1; we always fall through to leaf aggregation.
        // (This holds because rules have Scope::AllLeaves, so they
        // apply only at leaves; consolidation is the ONLY way to
        // produce a value at a consolidated coord.)
        let _ = measure_position; // measure_position is implicit in `coord`

        // Build the per-dim leaf-with-weight lists, then walk the
        // Cartesian product.
        let mut per_dim_options: Vec<Vec<(ElementId, f64)>> = Vec::with_capacity(dims.len());
        for (i, h) in hierarchies_by_dim.iter().enumerate() {
            let element = coord.element_at(i);
            if consolidated_dims.contains(&i) {
                // Consolidated slot: expand to its leaf descendants.
                per_dim_options.push(h.descendants(element));
            } else {
                // Leaf slot: keep as-is with weight 1.0.
                per_dim_options.push(vec![(element, 1.0)]);
            }
        }

        // Walk the Cartesian product. For Acme's heaviest single
        // consolidated read (FY × All_Channels × USA × Spend) this is
        // 12 × 5 × 7 = 420 leaves. The 1A perf ceilings (deferred per
        // §0.A) bound this at < 20 ms per the 420-leaf bench.
        let combinator = pick_combinator(measure_meta);
        let mut child_count: u32 = 0;
        let mut state = combinator.new_state();
        let mut leaf_coord_buf: SmallVec<[ElementId; 8]> =
            coord.elements().iter().copied().collect();

        cartesian_walk(
            &per_dim_options,
            &mut |selection: &[(ElementId, f64)]| -> Result<(), EngineError> {
                // Build the leaf coord by patching the selected element
                // into each dim slot. Cumulative weight = product of
                // per-dim weights.
                let mut weight_product = 1.0_f64;
                for (i, &(element, w)) in selection.iter().enumerate() {
                    leaf_coord_buf[i] = element;
                    weight_product *= w;
                }
                let leaf_coord =
                    CellCoordinate::from_parts(coord.cube, leaf_coord_buf.iter().copied());
                let value = read_at(&leaf_coord)?;
                child_count += 1;

                match &measure_meta.aggregation {
                    AggregationRule::WeightedAverage { weight_measure } => {
                        // Read the sibling weight measure at the same
                        // leaf coord (replace the measure slot).
                        let mut wcoord_buf: SmallVec<[ElementId; 8]> =
                            leaf_coord.elements().iter().copied().collect();
                        // Find the measure slot — the only dim with kind
                        // == Measure.
                        let measure_dim_pos = dims
                            .iter()
                            .position(Dimension::is_measure_dimension)
                            .ok_or(EngineError::Internal(
                                "consolidation: no measure dimension in dims slice",
                            ))?;
                        wcoord_buf[measure_dim_pos] = *weight_measure;
                        let wcoord =
                            CellCoordinate::from_parts(coord.cube, wcoord_buf.iter().copied());
                        let weight_value = read_at(&wcoord)?;
                        combinator.observe_weighted(
                            &mut state,
                            value,
                            weight_value,
                            weight_product,
                        );
                    }
                    _ => {
                        combinator.observe(&mut state, value, weight_product);
                    }
                }
                Ok(())
            },
        )?;

        let value = combinator.finish(state);
        let prov = Provenance::Consolidation {
            hierarchies: visited_hierarchies,
            child_count,
        };
        // Tie the consolidation provenance to the cube's current revision
        // — caller (`cube.rs`) re-reads if that revision moves.
        let _ = revision; // captured by the surrounding CellValue, not by Provenance::Consolidation
        Ok(Some((value, prov)))
    }
}

/// Walk every combination of one element-from-each-dim. Calls `f` once
/// per combination with a borrow into a thread-local buffer.
///
/// Sized for Phase 1 cube counts; iterative recursion via index stack
/// avoids stack-depth blowup for ≤ 8 dims.
fn cartesian_walk<F>(options: &[Vec<(ElementId, f64)>], f: &mut F) -> Result<(), EngineError>
where
    F: FnMut(&[(ElementId, f64)]) -> Result<(), EngineError>,
{
    if options.iter().any(Vec::is_empty) {
        // Some dim has no leaves — empty Cartesian product.
        return Ok(());
    }
    let n = options.len();
    let mut indices = vec![0usize; n];
    let mut current: Vec<(ElementId, f64)> = options.iter().map(|opts| opts[0]).collect();
    loop {
        f(&current)?;
        // Increment the rightmost index; carry on overflow.
        let mut i = n;
        let mut carried = true;
        while i > 0 {
            i -= 1;
            indices[i] += 1;
            if indices[i] < options[i].len() {
                current[i] = options[i][indices[i]];
                carried = false;
                break;
            }
            // Overflow — reset and carry left.
            indices[i] = 0;
            current[i] = options[i][0];
        }
        if carried {
            // Walked past the end of the leftmost dim — done.
            return Ok(());
        }
    }
}

// ---------------------------------------------------------------------------
// Combinator: per-aggregation-rule reducer. Each variant maintains a
// running state and folds in observations.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Combinator {
    Sum,
    WeightedAverage,
    Min,
    Max,
}

#[derive(Default)]
struct CombinatorState {
    /// Sum: running total. WeightedAverage: numerator. Min/Max: best.
    accum: f64,
    /// WeightedAverage only: denominator.
    denom: f64,
    /// True iff at least one non-Null observation has been folded in.
    /// Without this flag we can't distinguish "no contributions" from
    /// "every contribution was zero" — material for Sum/Min/Max where
    /// the all-Null case must return Null.
    has_observation: bool,
}

fn pick_combinator(meta: &MeasureMeta) -> Combinator {
    match &meta.aggregation {
        AggregationRule::Sum => Combinator::Sum,
        AggregationRule::WeightedAverage { .. } => Combinator::WeightedAverage,
        AggregationRule::Min => Combinator::Min,
        AggregationRule::Max => Combinator::Max,
    }
}

impl Combinator {
    fn new_state(self) -> CombinatorState {
        CombinatorState::default()
    }

    /// Fold in a (value, weight) observation for Sum / Min / Max rules.
    /// Per spec §7: Null contributes nothing, except for Sum where Null
    /// is identity (i.e., adds 0).
    fn observe(self, state: &mut CombinatorState, value: ScalarValue, weight: f64) {
        match (self, &value) {
            (Combinator::Sum, ScalarValue::F64(v)) => {
                let contribution = v * weight;
                if contribution.is_finite() {
                    state.accum += contribution;
                    state.has_observation = true;
                }
            }
            (Combinator::Sum, ScalarValue::Null) => {
                // Null is identity for Sum; we don't bump
                // has_observation. If every leaf is Null, finish()
                // returns Null per spec §7.
            }
            (Combinator::Min, ScalarValue::F64(v)) => {
                if !state.has_observation || *v < state.accum {
                    state.accum = *v;
                    state.has_observation = true;
                }
            }
            (Combinator::Max, ScalarValue::F64(v)) => {
                if !state.has_observation || *v > state.accum {
                    state.accum = *v;
                    state.has_observation = true;
                }
            }
            (Combinator::Min | Combinator::Max, ScalarValue::Null) => {
                // Excluded.
            }
            // Non-F64 values flowing into a numeric combinator are a
            // type-system bug upstream (CubeBuilder::add_rule's well-
            // typedness check). Dropping them silently here matches the
            // null-poison philosophy: don't pollute the result with a
            // panic; let the calling code surface the type mismatch
            // through its own paths.
            _ => {}
        }
    }

    /// Fold in a (value, weight_value, weight_product) observation for
    /// the WeightedAverage rule. Both Null value and Null weight cause
    /// the leaf to be excluded.
    fn observe_weighted(
        self,
        state: &mut CombinatorState,
        value: ScalarValue,
        weight_value: ScalarValue,
        weight_product: f64,
    ) {
        debug_assert!(matches!(self, Combinator::WeightedAverage));
        let v = match value {
            ScalarValue::F64(x) if x.is_finite() => x,
            _ => return, // Null or non-F64 → excluded
        };
        let w = match weight_value {
            ScalarValue::F64(x) if x.is_finite() => x,
            _ => return, // Null or non-F64 weight → excluded entirely
        };
        let effective_weight = w * weight_product;
        if !effective_weight.is_finite() {
            return;
        }
        state.accum += v * effective_weight;
        state.denom += effective_weight;
        state.has_observation = true;
    }

    fn finish(self, state: CombinatorState) -> ScalarValue {
        match self {
            Combinator::Sum => {
                if state.has_observation {
                    if state.accum.is_finite() {
                        ScalarValue::F64(state.accum)
                    } else {
                        ScalarValue::Null
                    }
                } else {
                    ScalarValue::Null
                }
            }
            Combinator::WeightedAverage => {
                if !state.has_observation || state.denom.abs() < 1e-300 {
                    ScalarValue::Null
                } else {
                    let v = state.accum / state.denom;
                    if v.is_finite() {
                        ScalarValue::F64(v)
                    } else {
                        ScalarValue::Null
                    }
                }
            }
            Combinator::Min | Combinator::Max => {
                if state.has_observation && state.accum.is_finite() {
                    ScalarValue::F64(state.accum)
                } else {
                    ScalarValue::Null
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dimension::{DimensionBuilder, DimensionKind};
    use crate::element::{Element, MeasureRole};
    use crate::hierarchy::Hierarchy;
    use crate::id::{CubeId, ElementId, IdGenerator};
    use crate::value::CellDataType;

    /// Test-only constructor on `DimensionBuilder`. The crate-public
    /// builder API takes one element at a time; tests want a bulk form
    /// without re-validating each addition.
    trait DimensionBuilderTestHelpers {
        fn default_for_test(
            id: crate::id::DimensionId,
            name: &str,
            kind: crate::dimension::DimensionKind,
            elements: Vec<crate::element::Element>,
        ) -> crate::dimension::DimensionBuilder;
    }

    impl DimensionBuilderTestHelpers for crate::dimension::DimensionBuilder {
        fn default_for_test(
            id: crate::id::DimensionId,
            name: &str,
            kind: crate::dimension::DimensionKind,
            elements: Vec<crate::element::Element>,
        ) -> crate::dimension::DimensionBuilder {
            let mut b = crate::dimension::Dimension::builder(id, name, kind);
            for e in elements {
                b = b.add_element(e).expect("test fixture element add");
            }
            b
        }
    }

    /// Build a tiny 1-dim cube fixture for consolidation tests:
    /// - Time dim with leaves Jan/Feb/Mar and consolidated Q1.
    /// - Measure dim with one F64 measure (the test sets the rule).
    fn fixture_with_aggregation(
        agg: AggregationRule,
    ) -> (
        Vec<Dimension>,
        Vec<Hierarchy>,
        ElementId,      // q1
        Vec<ElementId>, // [jan, feb, mar]
        ElementId,      // measure
        MeasureMeta,
    ) {
        let id_gen = IdGenerator::new();
        let time_dim_id = id_gen.dimension();
        let measure_dim_id = id_gen.dimension();
        let q1 = id_gen.element();
        let jan = id_gen.element();
        let feb = id_gen.element();
        let mar = id_gen.element();
        let measure = id_gen.element();
        let weight_measure = id_gen.element();

        let time_hier = Hierarchy::builder(id_gen.hierarchy(), "calendar", time_dim_id)
            .add_edge(q1, jan, 1.0)
            .add_edge(q1, feb, 1.0)
            .add_edge(q1, mar, 1.0)
            .build()
            .expect("hierarchy ok");

        let time_dim = DimensionBuilder::default_for_test(
            time_dim_id,
            "Time",
            DimensionKind::Standard,
            vec![
                Element::leaf(q1, "Q1", time_dim_id),
                Element::leaf(jan, "Jan", time_dim_id),
                Element::leaf(feb, "Feb", time_dim_id),
                Element::leaf(mar, "Mar", time_dim_id),
            ],
        )
        .add_hierarchy(time_hier)
        .expect("hier add ok")
        .default_hierarchy("calendar")
        .build()
        .expect("dim build");

        // Build the measure with the requested aggregation. For
        // WeightedAverage the test fixture wires a sibling weight measure.
        let agg_concrete = match agg {
            AggregationRule::WeightedAverage { .. } => {
                AggregationRule::WeightedAverage { weight_measure }
            }
            other => other,
        };

        let meta = MeasureMeta {
            dtype: CellDataType::F64,
            role: MeasureRole::Input,
            aggregation: agg_concrete.clone(),
        };

        let measure_dim = DimensionBuilder::default_for_test(
            measure_dim_id,
            "Measure",
            DimensionKind::Measure,
            vec![
                Element::measure(
                    measure,
                    "Spend",
                    measure_dim_id,
                    CellDataType::F64,
                    MeasureRole::Input,
                    agg_concrete,
                ),
                Element::measure(
                    weight_measure,
                    "WEIGHT",
                    measure_dim_id,
                    CellDataType::F64,
                    MeasureRole::Input,
                    AggregationRule::Sum,
                ),
            ],
        )
        .build()
        .expect("measure dim build");

        let dims = vec![time_dim, measure_dim];
        let h = dims[0].default_hierarchy().clone();
        // No hierarchy on the measure dim — synthesized flat from
        // DimensionBuilder. We need to grab a reference to it.
        let measure_h = dims[1].default_hierarchy().clone();
        let hierarchies = vec![h, measure_h];

        (dims, hierarchies, q1, vec![jan, feb, mar], measure, meta)
    }

    fn coord_for(cube: CubeId, time: ElementId, measure: ElementId) -> CellCoordinate {
        CellCoordinate::from_parts(cube, [time, measure])
    }

    #[test]
    fn sum_three_leaves_present() {
        let (dims, hierarchies, q1, leaves, measure, meta) =
            fixture_with_aggregation(AggregationRule::Sum);
        let h_refs: Vec<&Hierarchy> = hierarchies.iter().collect();
        let cube = CubeId(1);
        let coord = coord_for(cube, q1, measure);

        let values: std::collections::HashMap<ElementId, ScalarValue> = [
            (leaves[0], ScalarValue::F64(10.0)),
            (leaves[1], ScalarValue::F64(20.0)),
            (leaves[2], ScalarValue::F64(30.0)),
        ]
        .into_iter()
        .collect();

        let mut read_leaf = |c: &CellCoordinate| -> Result<ScalarValue, EngineError> {
            let time_elem = c.element_at(0);
            Ok(values.get(&time_elem).cloned().unwrap_or(ScalarValue::Null))
        };
        let result = Consolidator::read(
            &coord,
            &dims,
            &h_refs,
            1,
            &meta,
            &mut read_leaf,
            Revision::ZERO,
        )
        .expect("consolidation ok");
        let (value, prov) = result.expect("consolidated coord");
        assert_eq!(value.as_f64(), Some(60.0));
        match prov {
            Provenance::Consolidation { child_count, .. } => {
                assert_eq!(child_count, 3);
            }
            _ => panic!("expected Consolidation provenance"),
        }
    }

    #[test]
    fn sum_with_one_null_leaf() {
        let (dims, hierarchies, q1, leaves, measure, meta) =
            fixture_with_aggregation(AggregationRule::Sum);
        let h_refs: Vec<&Hierarchy> = hierarchies.iter().collect();
        let cube = CubeId(1);
        let coord = coord_for(cube, q1, measure);

        let values: std::collections::HashMap<ElementId, ScalarValue> = [
            (leaves[0], ScalarValue::F64(10.0)),
            (leaves[1], ScalarValue::Null),
            (leaves[2], ScalarValue::F64(30.0)),
        ]
        .into_iter()
        .collect();
        let mut read_leaf = |c: &CellCoordinate| -> Result<ScalarValue, EngineError> {
            Ok(values
                .get(&c.element_at(0))
                .cloned()
                .unwrap_or(ScalarValue::Null))
        };
        let (value, _) = Consolidator::read(
            &coord,
            &dims,
            &h_refs,
            1,
            &meta,
            &mut read_leaf,
            Revision::ZERO,
        )
        .unwrap()
        .unwrap();
        assert_eq!(value.as_f64(), Some(40.0));
    }

    #[test]
    fn sum_all_null_leaves_returns_null() {
        let (dims, hierarchies, q1, _leaves, measure, meta) =
            fixture_with_aggregation(AggregationRule::Sum);
        let h_refs: Vec<&Hierarchy> = hierarchies.iter().collect();
        let coord = coord_for(CubeId(1), q1, measure);
        let mut read_leaf = |_: &CellCoordinate| Ok(ScalarValue::Null);
        let (value, _) = Consolidator::read(
            &coord,
            &dims,
            &h_refs,
            1,
            &meta,
            &mut read_leaf,
            Revision::ZERO,
        )
        .unwrap()
        .unwrap();
        assert!(value.is_null());
    }

    #[test]
    fn weighted_average_basic() {
        // 3 months: spend = [10, 20, 30], cpc = [1, 2, 3]
        // Σ(cpc * spend) / Σ(spend) = (10 + 40 + 90) / 60 = 140 / 60 ≈ 2.333
        let (dims, hierarchies, q1, leaves, measure, meta) =
            fixture_with_aggregation(AggregationRule::WeightedAverage {
                weight_measure: ElementId(0), // overridden in fixture builder
            });
        let h_refs: Vec<&Hierarchy> = hierarchies.iter().collect();
        let coord = coord_for(CubeId(1), q1, measure);

        // Pull the actual weight_measure id out of the meta we got back.
        let weight_measure = match &meta.aggregation {
            AggregationRule::WeightedAverage { weight_measure } => *weight_measure,
            _ => panic!(),
        };

        // Per-leaf-time CPC and Spend values keyed by time ElementId.
        let cpc: std::collections::HashMap<ElementId, ScalarValue> = [
            (leaves[0], ScalarValue::F64(1.0)),
            (leaves[1], ScalarValue::F64(2.0)),
            (leaves[2], ScalarValue::F64(3.0)),
        ]
        .into_iter()
        .collect();
        let spend: std::collections::HashMap<ElementId, ScalarValue> = [
            (leaves[0], ScalarValue::F64(10.0)),
            (leaves[1], ScalarValue::F64(20.0)),
            (leaves[2], ScalarValue::F64(30.0)),
        ]
        .into_iter()
        .collect();

        let measure_id = measure;

        let mut read_leaf = |c: &CellCoordinate| -> Result<ScalarValue, EngineError> {
            // The measure slot tells us which map to consult; this fixture
            // uses position 1 for the measure.
            let m = c.element_at(1);
            let t = c.element_at(0);
            if m == measure_id {
                Ok(cpc.get(&t).cloned().unwrap_or(ScalarValue::Null))
            } else if m == weight_measure {
                Ok(spend.get(&t).cloned().unwrap_or(ScalarValue::Null))
            } else {
                Ok(ScalarValue::Null)
            }
        };
        // Single read_at closure handles both leaf cpc reads AND weight
        // (Spend) reads — they're both lookups by (time, measure) pair.
        let _ = weight_measure;

        let (value, _) = Consolidator::read(
            &coord,
            &dims,
            &h_refs,
            1,
            &meta,
            &mut read_leaf,
            Revision::ZERO,
        )
        .unwrap()
        .unwrap();
        let v = value.as_f64().expect("F64");
        assert!(
            (v - 140.0 / 60.0).abs() < 1e-9,
            "weighted avg expected ~2.333, got {v}"
        );
    }

    #[test]
    fn weighted_average_zero_total_weight_returns_null() {
        let (dims, hierarchies, q1, leaves, measure, meta) =
            fixture_with_aggregation(AggregationRule::WeightedAverage {
                weight_measure: ElementId(0),
            });
        let h_refs: Vec<&Hierarchy> = hierarchies.iter().collect();
        let coord = coord_for(CubeId(1), q1, measure);
        // We don't need the weight_measure id in this test (every weight
        // value is zero, so the result is Null regardless of which slot
        // holds it). It still needs to exist on the meta because the
        // combinator's `observe_weighted` path looks it up.

        let cpc: std::collections::HashMap<ElementId, ScalarValue> = [
            (leaves[0], ScalarValue::F64(1.0)),
            (leaves[1], ScalarValue::F64(2.0)),
            (leaves[2], ScalarValue::F64(3.0)),
        ]
        .into_iter()
        .collect();
        let spend: std::collections::HashMap<ElementId, ScalarValue> = [
            (leaves[0], ScalarValue::F64(0.0)),
            (leaves[1], ScalarValue::F64(0.0)),
            (leaves[2], ScalarValue::F64(0.0)),
        ]
        .into_iter()
        .collect();
        let measure_id = measure;
        let mut read_leaf = |c: &CellCoordinate| {
            let m = c.element_at(1);
            let t = c.element_at(0);
            Ok(if m == measure_id {
                cpc.get(&t).cloned().unwrap_or(ScalarValue::Null)
            } else {
                spend.get(&t).cloned().unwrap_or(ScalarValue::Null)
            })
        };
        let (value, _) = Consolidator::read(
            &coord,
            &dims,
            &h_refs,
            1,
            &meta,
            &mut read_leaf,
            Revision::ZERO,
        )
        .unwrap()
        .unwrap();
        assert!(value.is_null(), "all-zero weights → Null");
    }

    #[test]
    fn min_with_nulls_excluded() {
        let (dims, hierarchies, q1, leaves, measure, meta) =
            fixture_with_aggregation(AggregationRule::Min);
        let h_refs: Vec<&Hierarchy> = hierarchies.iter().collect();
        let coord = coord_for(CubeId(1), q1, measure);
        let values: std::collections::HashMap<ElementId, ScalarValue> = [
            (leaves[0], ScalarValue::Null),
            (leaves[1], ScalarValue::F64(5.0)),
            (leaves[2], ScalarValue::F64(10.0)),
        ]
        .into_iter()
        .collect();
        let mut read_leaf = |c: &CellCoordinate| {
            Ok(values
                .get(&c.element_at(0))
                .cloned()
                .unwrap_or(ScalarValue::Null))
        };
        let (value, _) = Consolidator::read(
            &coord,
            &dims,
            &h_refs,
            1,
            &meta,
            &mut read_leaf,
            Revision::ZERO,
        )
        .unwrap()
        .unwrap();
        assert_eq!(value.as_f64(), Some(5.0));
    }

    #[test]
    fn max_with_nulls_excluded() {
        let (dims, hierarchies, q1, leaves, measure, meta) =
            fixture_with_aggregation(AggregationRule::Max);
        let h_refs: Vec<&Hierarchy> = hierarchies.iter().collect();
        let coord = coord_for(CubeId(1), q1, measure);
        let values: std::collections::HashMap<ElementId, ScalarValue> = [
            (leaves[0], ScalarValue::Null),
            (leaves[1], ScalarValue::F64(5.0)),
            (leaves[2], ScalarValue::F64(10.0)),
        ]
        .into_iter()
        .collect();
        let mut read_leaf = |c: &CellCoordinate| {
            Ok(values
                .get(&c.element_at(0))
                .cloned()
                .unwrap_or(ScalarValue::Null))
        };
        let (value, _) = Consolidator::read(
            &coord,
            &dims,
            &h_refs,
            1,
            &meta,
            &mut read_leaf,
            Revision::ZERO,
        )
        .unwrap()
        .unwrap();
        assert_eq!(value.as_f64(), Some(10.0));
    }

    #[test]
    fn pure_leaf_coord_returns_none() {
        let (dims, hierarchies, _q1, leaves, measure, meta) =
            fixture_with_aggregation(AggregationRule::Sum);
        let h_refs: Vec<&Hierarchy> = hierarchies.iter().collect();
        let coord = coord_for(CubeId(1), leaves[0], measure);
        let mut read_leaf = |_: &CellCoordinate| Ok(ScalarValue::F64(1.0));
        let result = Consolidator::read(
            &coord,
            &dims,
            &h_refs,
            1,
            &meta,
            &mut read_leaf,
            Revision::ZERO,
        )
        .unwrap();
        assert!(
            result.is_none(),
            "pure-leaf coord must return None so caller does the leaf path"
        );
    }
}
