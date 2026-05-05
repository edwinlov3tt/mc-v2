//! Cube — the integrator.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.18 and engine-semantics.md
//! §1.
//!
//! `Cube` owns the dimensions, rules, store, dependency graph, dirty
//! tracker, locks, and permissions. It exposes `read`, `read_with_trace`,
//! `write`, `snapshot`, `rollback_to`, plus a few accessors. `slice.rs`
//! adds bulk reads.
//!
//! # Implementation notes (per kickoff Rule 5: no long-lived `&Cube`
//! borrows across recursive reads)
//!
//! The `read` path is recursive — evaluating Revenue's rule reads
//! Customers, which reads Leads, etc. Each step needs to mutate the
//! cube (cache the computed value, clear dirty flags, populate the
//! dependency graph lazily). This forces `read` to take `&mut self`,
//! and forbids holding a `&Dimension` or `&Rule` across the recursive
//! call.
//!
//! The strategy used here:
//!
//! 1. Resolve the rule, scope, and dim positions ONCE up front, copying
//!    out the small data (RuleId, target_measure, an owned Expr clone).
//! 2. Drop the borrow before recursing.
//! 3. On the recursive call, `&mut self` is free again.
//!
//! `Expr::Box<Expr>` clones are cheap for Acme's depth-≤-5 rule bodies.

use std::sync::Arc;

use smallvec::SmallVec;

use crate::cell::{CellValue, Provenance, StoredCell};
use crate::consolidation::Consolidator;
use crate::coordinate::{CellCoordinate, CellCoordinateBuilder};
use crate::cube_shape::CubeShape;
use crate::dependency::{DependencyEdge, DependencyGraph, DependencySource};
use crate::dimension::{Dimension, DimensionKind};
use crate::dirty::DirtyTracker;
use crate::element::{MeasureMeta, MeasureRole, VersionState};
use crate::error::EngineError;
use crate::hierarchy::Hierarchy;
use crate::id::{CubeId, DimensionId, ElementId, PrincipalId, Revision};
use crate::lock::{Lock, LockTable};
use crate::permission::{capability, PermissionTable};
use crate::rule::{eval_expr, expr_depth, CrossCoordRead, Expr, Rule, RuleSet};
use crate::slice::{SliceBinding, SliceQuery, SliceResult, PHASE_1_SLICE_LIMIT};
use crate::snapshot::Snapshot;
use crate::store::HashMapStore;
use crate::trace::{ExprOp, ExprSummary, Trace, TraceNode, TraceOp};
use crate::value::{validate_finite_f64, CellDataType, ScalarValue};

#[derive(Debug)]
pub struct Cube {
    pub id: CubeId,
    pub name: String,
    /// Dimensions are frozen at `CubeBuilder::build` time and never
    /// mutated thereafter. Held behind `Arc` so the consolidation fast
    /// path (`read_consolidated`) can hand a borrow-independent snapshot
    /// to `Consolidator::read` for the cost of a single refcount bump,
    /// instead of deep-cloning every `Dimension`. See PERF.md §6.7 /
    /// §9.4 (Phase 2B). The accessor surface (`dimensions()`,
    /// `dimension()`, `measure_dimension()`, etc.) returns plain `&[T]`
    /// and `&Dimension` shapes — the Arc is an internal storage choice.
    dimensions: Arc<Vec<Dimension>>,
    measure_dimension_position: usize,
    /// Precomputed Cartesian-product shape for the bitset-backed dirty
    /// tracker (Phase 2D). `None` when the cube's Cartesian cardinality
    /// exceeds `cube_shape::CARDINALITY_GUARD` — in that case `dirty`
    /// falls back to the AHashSet representation. Held behind `Arc` so
    /// the tracker shares the same shape data without paying a deep
    /// clone, and so any future caller that needs the shape can borrow
    /// it for the cube's lifetime.
    #[allow(dead_code)]
    cube_shape: Option<Arc<CubeShape>>,
    rules: RuleSet,
    locks: LockTable,
    permissions: PermissionTable,
    store: HashMapStore,
    revision: Revision,
    deps: DependencyGraph,
    dirty: DirtyTracker,
}

impl Cube {
    pub fn builder(id: CubeId, name: impl Into<String>) -> CubeBuilder {
        CubeBuilder {
            id,
            name: name.into(),
            dimensions: Vec::new(),
            measure_dimension_name: None,
            staged_rules: Vec::new(),
            root_principal: None,
        }
    }

    // --- Accessors ---

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn dimensions(&self) -> &[Dimension] {
        &self.dimensions[..]
    }

    pub fn dimension(&self, id: DimensionId) -> Option<&Dimension> {
        self.dimensions.iter().find(|d| d.id == id)
    }

    pub fn dimension_by_name(&self, name: &str) -> Option<&Dimension> {
        self.dimensions.iter().find(|d| d.name == name)
    }

    pub fn measure_dimension(&self) -> &Dimension {
        &self.dimensions[self.measure_dimension_position]
    }

    pub fn rules(&self) -> &RuleSet {
        &self.rules
    }

    pub fn deps(&self) -> &DependencyGraph {
        &self.deps
    }

    pub fn dirty(&self) -> &DirtyTracker {
        &self.dirty
    }

    pub fn store(&self) -> &HashMapStore {
        &self.store
    }

    pub fn permissions(&self) -> &PermissionTable {
        &self.permissions
    }

    pub fn locks(&self) -> &LockTable {
        &self.locks
    }

    /// Convenience: build a `CellCoordinateBuilder` over this cube's
    /// dimensions. The brief's `CellCoordinateBuilder<'cube>` shape per
    /// spec §3.7 — Phase 1 keeps the constructor that takes
    /// `(CubeId, &[Dimension])` directly so it can be tested before
    /// `cube.rs` lands; this helper threads through.
    pub fn coordinate_builder(&self) -> CellCoordinateBuilder<'_> {
        CellCoordinateBuilder::new(self.id, &self.dimensions)
    }

    // --- Read ---

    pub fn read(
        &mut self,
        coord: &CellCoordinate,
        principal: PrincipalId,
    ) -> Result<CellValue, EngineError> {
        self.read_inner(coord, principal, /* request_trace */ false)
    }

    pub fn read_with_trace(
        &mut self,
        coord: &CellCoordinate,
        principal: PrincipalId,
    ) -> Result<CellValue, EngineError> {
        self.read_inner(coord, principal, /* request_trace */ true)
    }

    /// Execute a `SliceQuery` and return one `CellValue` per coordinate
    /// in the slice. Per spec §12.
    ///
    /// Order of `coords` is deterministic: it's the lexicographic
    /// product of `self.dimensions` × the resolved per-dim element
    /// list, in cube dim order. (CLAUDE.md §2.11: tests that compare
    /// slice output for equality can rely on this stable order.)
    pub fn slice(
        &mut self,
        query: &SliceQuery,
        principal: PrincipalId,
    ) -> Result<SliceResult, EngineError> {
        if query.cube != self.id {
            return Err(EngineError::Internal(
                "Cube::slice: query cube id does not match this cube",
            ));
        }
        // Resolve per-dim element lists in cube dim order. Per spec §12
        // I-Slice-1: every dim must have a binding; missing dims are
        // an Internal error.
        let mut per_dim_elements: Vec<Vec<ElementId>> = Vec::with_capacity(self.dimensions.len());
        for dim in self.dimensions.iter() {
            let binding = query.bindings.get(&dim.id).ok_or(EngineError::Internal(
                "Cube::slice: query is missing a binding for one of the cube's dimensions",
            ))?;
            per_dim_elements.push(resolve_binding(binding, dim));
        }

        // Compute total cardinality and reject oversize slices.
        let cardinality = per_dim_elements.iter().map(Vec::len).product::<usize>();
        if cardinality > PHASE_1_SLICE_LIMIT {
            return Err(EngineError::SliceTooLarge {
                actual: cardinality,
                max: PHASE_1_SLICE_LIMIT,
            });
        }

        let revision_before = self.revision;
        let mut coords: Vec<CellCoordinate> = Vec::with_capacity(cardinality);
        let mut values: Vec<CellValue> = Vec::with_capacity(cardinality);

        // Walk the Cartesian product in dim order.
        if per_dim_elements.iter().any(Vec::is_empty) {
            return Ok(SliceResult {
                coords,
                values,
                revision: revision_before,
            });
        }
        let mut indices = vec![0usize; per_dim_elements.len()];
        loop {
            let elements: Vec<ElementId> = (0..per_dim_elements.len())
                .map(|i| per_dim_elements[i][indices[i]])
                .collect();
            let coord = CellCoordinate::from_parts(self.id, elements);
            let v = if query.request_trace {
                self.read_with_trace(&coord, principal)?
            } else {
                self.read(&coord, principal)?
            };
            coords.push(coord);
            values.push(v);
            // Increment.
            let mut carried = true;
            let mut i = per_dim_elements.len();
            while i > 0 {
                i -= 1;
                indices[i] += 1;
                if indices[i] < per_dim_elements[i].len() {
                    carried = false;
                    break;
                }
                indices[i] = 0;
            }
            if carried {
                break;
            }
        }

        Ok(SliceResult {
            coords,
            values,
            revision: revision_before,
        })
    }

    fn read_inner(
        &mut self,
        coord: &CellCoordinate,
        principal: PrincipalId,
        request_trace: bool,
    ) -> Result<CellValue, EngineError> {
        // Per engine-semantics.md §13 I-WB-5 (the read counterpart): every
        // read checks permissions before proceeding.
        if !self
            .permissions
            .check(principal, &self.dimensions, coord, capability::READ)
        {
            return Err(EngineError::InsufficientPermission {
                principal,
                coord: coord.clone(),
            });
        }
        // Cube-id sanity: a coord built for a different cube can't be
        // read here.
        if coord.cube != self.id {
            return Err(EngineError::Internal(
                "Cube::read: coordinate cube id does not match this cube",
            ));
        }
        if coord.elements().len() != self.dimensions.len() {
            return Err(EngineError::Internal(
                "Cube::read: coordinate arity does not match cube dimension count",
            ));
        }

        // Classify: leaf (every dim slot is a leaf in its default
        // hierarchy) or consolidated (at least one slot is non-leaf).
        if self.is_consolidated_coord(coord) {
            self.read_consolidated(coord, principal, request_trace)
        } else {
            self.read_leaf(coord, principal, request_trace)
        }
    }

    fn is_consolidated_coord(&self, coord: &CellCoordinate) -> bool {
        for (i, dim) in self.dimensions.iter().enumerate() {
            if dim.kind == DimensionKind::Measure {
                continue;
            }
            let h = dim.default_hierarchy();
            if h.edges.is_empty() {
                continue; // synthesized flat hierarchy → all elements leaf-y
            }
            let element = coord.element_at(i);
            if h.is_consolidated(element) {
                return true;
            }
        }
        false
    }

    fn read_leaf(
        &mut self,
        coord: &CellCoordinate,
        principal: PrincipalId,
        request_trace: bool,
    ) -> Result<CellValue, EngineError> {
        // Identify the measure at this coord and decide Input vs Derived.
        let (measure_id, measure_meta) = self.measure_at_coord(coord)?;
        match measure_meta.role {
            MeasureRole::Input => self.read_input_leaf(coord, &measure_meta, request_trace),
            MeasureRole::Derived => {
                self.read_derived_leaf(coord, principal, measure_id, &measure_meta, request_trace)
            }
        }
    }

    fn read_input_leaf(
        &self,
        coord: &CellCoordinate,
        measure_meta: &MeasureMeta,
        request_trace: bool,
    ) -> Result<CellValue, EngineError> {
        if let Some(stored) = self.store.read(coord) {
            let trace = if request_trace {
                Some(Self::input_trace(coord, stored))
            } else {
                None
            };
            return Ok(CellValue {
                value: stored.value.clone(),
                dtype: measure_meta.dtype.clone(),
                provenance: stored.provenance.clone(),
                uncertainty: stored.uncertainty.clone(),
                trace,
                revision: stored.revision,
            });
        }
        // Absent input — return Null with Default provenance.
        let value = ScalarValue::Null;
        let provenance = Provenance::Default {
            reason: "no input written",
        };
        let trace = if request_trace {
            Some(Trace {
                root: TraceNode {
                    coord: coord.clone(),
                    value: value.clone(),
                    operation: TraceOp::DefaultFallback {
                        default: value.clone(),
                        reason: "no input written",
                    },
                    children: Vec::new(),
                },
                revision: self.revision,
                elapsed_us: 0,
            })
        } else {
            None
        };
        Ok(CellValue {
            value,
            dtype: measure_meta.dtype.clone(),
            provenance,
            uncertainty: None,
            trace,
            revision: self.revision,
        })
    }

    fn read_derived_leaf(
        &mut self,
        coord: &CellCoordinate,
        principal: PrincipalId,
        measure_id: ElementId,
        measure_meta: &MeasureMeta,
        request_trace: bool,
    ) -> Result<CellValue, EngineError> {
        let cached_fresh = !self.dirty.is_dirty(coord)
            && self
                .store
                .read(coord)
                .map(|s| s.revision == self.revision)
                .unwrap_or(false);
        if cached_fresh && !request_trace {
            // Cache hit — but ONLY if we're not asked for a trace; the
            // trace requires walking the rule body, which is the same
            // as recomputing.
            let stored = self.store.read(coord).expect("checked above");
            return Ok(CellValue {
                value: stored.value.clone(),
                dtype: measure_meta.dtype.clone(),
                provenance: stored.provenance.clone(),
                uncertainty: stored.uncertainty.clone(),
                trace: None,
                revision: stored.revision,
            });
        }

        // Look up the rule; clone enough state to recurse without
        // holding a `&self.rules` borrow across child reads.
        let rule_indices = self.rules.rules_for_measure(measure_id);
        if rule_indices.is_empty() {
            // Derived measure with no rule — definition bug. Surface as
            // Internal.
            return Err(EngineError::Internal(
                "Cube::read_derived_leaf: derived measure has no rule registered",
            ));
        }
        let rule_index = rule_indices[0];
        let (rule_id, rule_body) = {
            let r = self.rules.rule_at(rule_index).expect("indexed");
            (r.id, r.body.clone())
        };
        let measure_dim_position = self.measure_dimension_position;

        // Track every measure actually read via SelfRef, for the
        // declared-dependency superset check (per spec §3.10).
        let mut actual_reads: Vec<(ElementId, CellValue)> = Vec::new();
        let mut child_traces: Vec<TraceNode> = Vec::new();

        let target_coord = coord.clone();
        let cube_id = self.id;
        // We can't pass `self` directly into the closure (mutable borrow
        // conflict), so we wrap the recursive call via a helper that
        // reborrows `self` cleanly.
        let value = {
            // Build a SelfRef lookup closure that recursively calls
            // Cube::read_inner. Each invocation re-acquires `&mut self`
            // through the wrapper.
            let mut lookup = |measure: ElementId| -> Result<ScalarValue, EngineError> {
                // Build the sibling coord (same coord, replace measure).
                let mut elements: smallvec::SmallVec<[ElementId; 8]> =
                    target_coord.elements().iter().copied().collect();
                elements[measure_dim_position] = measure;
                let sibling_coord = CellCoordinate::from_parts(cube_id, elements.iter().copied());
                let cv = self.read_inner(&sibling_coord, principal, request_trace)?;
                if request_trace {
                    if let Some(t) = &cv.trace {
                        child_traces.push(t.root.clone());
                    }
                }
                actual_reads.push((measure, cv.clone()));
                Ok(cv.value)
            };
            // Cross-coordinate reads are resolved by the caller context.
            // Phase 3E+: the Cube layer does not yet implement full
            // cross-coordinate resolution; return Null for unresolved reads.
            let mut cross_lookup = |_read: &CrossCoordRead| -> Result<ScalarValue, EngineError> {
                Ok(ScalarValue::Null)
            };
            eval_expr(&rule_body, &mut lookup, &mut cross_lookup)?
        };

        // After eval, validate the declared-dep superset for THIS
        // coordinate: every measure we actually read must be in the
        // rule's declared dependencies (per spec §10.7
        // doctrine_no_silent_dependency_miss). The structural check at
        // RuleSet::add already caught body-vs-declared mismatches; this
        // re-check is the runtime safety net for any code path that
        // bypasses the structural check.
        {
            let r = self.rules.rule_at(rule_index).expect("indexed");
            for (measure, _) in &actual_reads {
                if !r
                    .declared_dependencies
                    .iter()
                    .any(|d| d.measure == *measure)
                {
                    return Err(EngineError::UndeclaredDependency {
                        rule: r.id,
                        coord: coord.clone(),
                    });
                }
            }
        }

        // Add forward + reverse edges for each actual read. Idempotent:
        // re-evaluating the same coord doesn't accumulate duplicate edges.
        for (measure, _) in &actual_reads {
            let mut elements: smallvec::SmallVec<[ElementId; 8]> =
                coord.elements().iter().copied().collect();
            elements[measure_dim_position] = *measure;
            let sibling_coord = CellCoordinate::from_parts(cube_id, elements.iter().copied());
            self.deps.add_edge(
                coord.clone(),
                DependencyEdge {
                    to: sibling_coord,
                    via: DependencySource::Rule(rule_id),
                },
            );
        }

        // Cache the result.
        let provenance = Provenance::Rule {
            rule_id,
            computed_at: self.revision,
        };
        self.store.write(
            coord.clone(),
            StoredCell {
                value: value.clone(),
                provenance: provenance.clone(),
                uncertainty: None,
                revision: self.revision,
            },
        );
        self.dirty.clear(coord);

        // Build trace if requested.
        let trace = if request_trace {
            let summary = ExprSummary {
                op: top_level_expr_op(&rule_body),
                arity: expr_depth(&rule_body),
            };
            Some(Trace {
                root: TraceNode {
                    coord: coord.clone(),
                    value: value.clone(),
                    operation: TraceOp::RuleEvaluation {
                        rule_id,
                        expr_summary: summary,
                    },
                    children: child_traces,
                },
                revision: self.revision,
                elapsed_us: 0,
            })
        } else {
            None
        };

        Ok(CellValue {
            value,
            dtype: measure_meta.dtype.clone(),
            provenance,
            uncertainty: None,
            trace,
            revision: self.revision,
        })
    }

    fn read_consolidated(
        &mut self,
        coord: &CellCoordinate,
        principal: PrincipalId,
        request_trace: bool,
    ) -> Result<CellValue, EngineError> {
        // Resolve: per-dim default hierarchies, measure meta.
        let (_measure_id, measure_meta) = self.measure_at_coord(coord)?;
        let measure_position = self.measure_dimension_position;
        let cube_id = self.id;
        let revision = self.revision;

        // Cache hit: if we've already computed this consolidation at the
        // current revision and nothing has dirtied it since, return the
        // stored value. Per spec §11 / brief §10.3
        // `t_consolidation_caches_value_within_revision`. Trace requests
        // skip the cache because the trace requires walking the tree
        // (semantically the same as recompute).
        let cached_fresh = !self.dirty.is_dirty(coord)
            && self
                .store
                .read(coord)
                .map(|s| {
                    s.revision == self.revision
                        && matches!(s.provenance, Provenance::Consolidation { .. })
                })
                .unwrap_or(false);
        if cached_fresh && !request_trace {
            let stored = self.store.read(coord).expect("checked above");
            return Ok(CellValue {
                value: stored.value.clone(),
                dtype: measure_meta.dtype.clone(),
                provenance: stored.provenance.clone(),
                uncertainty: stored.uncertainty.clone(),
                trace: None,
                revision: stored.revision,
            });
        }

        // Per Phase 2B (PERF.md §6.7 + §9.4): hand `Consolidator::read`
        // a borrow-independent view of the cube's frozen dim/hierarchy
        // data without deep-cloning either. `self.dimensions` is
        // `Arc<Vec<Dimension>>` and each `Dimension::hierarchies` is
        // `Vec<Arc<Hierarchy>>`, so the per-call cost collapses to one
        // Arc bump for the dim slice plus N Arc-deref's to assemble the
        // `&[&Hierarchy]` slice the consolidator expects. The borrow
        // conflict that justified the Phase 1A clones (read_at_fn
        // captures `&mut self`, dim/hierarchy data was borrowed from
        // `self`) is resolved by holding the Arc'd snapshots in locals
        // — they outlive the consolidator call but borrow nothing from
        // `self`.
        let dims_owned: Arc<Vec<Dimension>> = Arc::clone(&self.dimensions);
        let hierarchies_refs: Vec<&Hierarchy> = dims_owned
            .iter()
            .map(|d| d.default_hierarchy_arc().as_ref())
            .collect();

        let mut child_traces: Vec<TraceNode> = Vec::new();
        // Single closure handles every read inside the Consolidator —
        // both leaf-value reads and (for WeightedAverage rules) sibling
        // weight-measure reads. They both flow through Cube::read_inner
        // so dependency tracking applies to weight reads too.
        let mut read_at_fn = |c: &CellCoordinate| -> Result<ScalarValue, EngineError> {
            let cv = self.read_inner(c, principal, request_trace)?;
            if request_trace {
                if let Some(t) = &cv.trace {
                    child_traces.push(t.root.clone());
                }
            }
            Ok(cv.value)
        };
        let _ = cube_id;

        let outcome = Consolidator::read(
            coord,
            &dims_owned[..],
            &hierarchies_refs,
            measure_position,
            &measure_meta,
            &mut read_at_fn,
            revision,
        )?;
        let (value, provenance) = match outcome {
            Some(pair) => pair,
            None => {
                // Consolidator decided this is actually a leaf — fall
                // back to the leaf path. (Belt-and-suspenders; the
                // upfront `is_consolidated_coord` should have caught
                // this.)
                return self.read_leaf(coord, principal, request_trace);
            }
        };

        // Cache the consolidated result so subsequent reads at the same
        // revision return immediately. Invalidated by `mark_closure` +
        // `compute_dirty_ancestors` on any leaf write that affects this
        // consolidation.
        self.store.write(
            coord.clone(),
            StoredCell {
                value: value.clone(),
                provenance: provenance.clone(),
                uncertainty: None,
                revision: self.revision,
            },
        );
        self.dirty.clear(coord);

        let trace = if request_trace {
            let (hierarchies_visited, child_count) = match &provenance {
                Provenance::Consolidation {
                    hierarchies,
                    child_count,
                } => (hierarchies.clone(), *child_count),
                _ => (smallvec::SmallVec::new(), child_traces.len() as u32),
            };
            Some(Trace {
                root: TraceNode {
                    coord: coord.clone(),
                    value: value.clone(),
                    operation: TraceOp::Consolidation {
                        hierarchies: hierarchies_visited,
                        child_count,
                    },
                    children: child_traces,
                },
                revision: self.revision,
                elapsed_us: 0,
            })
        } else {
            None
        };

        Ok(CellValue {
            value,
            dtype: measure_meta.dtype.clone(),
            provenance,
            uncertainty: None,
            trace,
            revision: self.revision,
        })
    }

    fn measure_at_coord(
        &self,
        coord: &CellCoordinate,
    ) -> Result<(ElementId, MeasureMeta), EngineError> {
        let measure_id = coord.element_at(self.measure_dimension_position);
        let measure_dim = self.measure_dimension();
        let element = measure_dim
            .element(measure_id)
            .ok_or(EngineError::ElementNotFound(measure_id, measure_dim.id))?;
        let meta = element
            .measure_meta()
            .ok_or(EngineError::Internal(
                "Cube::measure_at_coord: element in Measure dim has no MeasureMeta",
            ))?
            .clone();
        Ok((measure_id, meta))
    }

    fn input_trace(coord: &CellCoordinate, stored: &StoredCell) -> Trace {
        let op = match &stored.provenance {
            Provenance::Input {
                written_at,
                written_by,
            } => TraceOp::InputLookup {
                written_at: *written_at,
                written_by: *written_by,
            },
            _ => TraceOp::DefaultFallback {
                default: stored.value.clone(),
                reason: "non-Input provenance on Input measure cell",
            },
        };
        Trace {
            root: TraceNode {
                coord: coord.clone(),
                value: stored.value.clone(),
                operation: op,
                children: Vec::new(),
            },
            revision: stored.revision,
            elapsed_us: 0,
        }
    }

    // --- Write ---

    pub fn write(&mut self, req: WritebackRequest) -> Result<WritebackResult, EngineError> {
        // (1) Permission check first — per spec §13 I-WB-5.
        if !self.permissions.check(
            req.principal,
            &self.dimensions,
            &req.coord,
            capability::WRITE,
        ) {
            return Err(EngineError::InsufficientPermission {
                principal: req.principal,
                coord: req.coord,
            });
        }

        // (2) Reject cube-id mismatch / arity mismatch.
        if req.coord.cube != self.id {
            return Err(EngineError::Internal(
                "Cube::write: coordinate cube id does not match this cube",
            ));
        }
        if req.coord.elements().len() != self.dimensions.len() {
            return Err(EngineError::Internal(
                "Cube::write: coordinate arity does not match cube dimension count",
            ));
        }

        // (3) Reject consolidated coords. Per spec §13 I-WB-1.
        if self.is_consolidated_coord(&req.coord) {
            return Err(EngineError::ConsolidatedCellNotWritable { coord: req.coord });
        }

        // (4) Reject derived measure. Per spec §13 I-WB-2.
        let (measure_id, measure_meta) = self.measure_at_coord(&req.coord)?;
        if measure_meta.role == MeasureRole::Derived {
            return Err(EngineError::DerivedCellNotWritable { coord: req.coord });
        }

        // (5) Reject writes to Approved/Archived versions. Per spec §13
        //     I-WB-3 / §9.
        if let Some(version_dim) = self
            .dimensions
            .iter()
            .find(|d| d.kind == DimensionKind::Version)
        {
            let version_position = self
                .dimensions
                .iter()
                .position(|d| d.id == version_dim.id)
                .expect("version dim is in dimensions");
            let version_element = req.coord.element_at(version_position);
            if let Some(element) = version_dim.element(version_element) {
                if let Some(state) = element.version_state() {
                    if matches!(state, VersionState::Approved | VersionState::Archived) {
                        return Err(EngineError::LockedVersion {
                            version: version_element,
                            state,
                        });
                    }
                }
            }
        }

        // (6) Lock check. Per spec §13 I-WB-4.
        let now = req.now_unix_seconds;
        if let Some(blocking) =
            self.locks
                .check_write(req.principal, &self.dimensions, &req.coord, now)
        {
            return Err(EngineError::LockedCell {
                coord: req.coord.clone(),
                owner: blocking.owner,
            });
        }

        // (7) Determine the value to commit, applying intent.
        let (intent_value, type_check_value) = match &req.intent {
            WriteIntent::Set => (req.new_value.clone(), req.new_value.clone()),
            WriteIntent::Clear => (ScalarValue::Null, ScalarValue::Null),
            WriteIntent::Increment => {
                // Increment is numeric-only and operates against the
                // current value. Type-mismatch propagation: the request's
                // new_value is what we type-check against.
                let current = self
                    .store
                    .read(&req.coord)
                    .map(|s| s.value.clone())
                    .unwrap_or(ScalarValue::Null);
                let delta = req.new_value.clone();
                let summed = match (&current, &delta) {
                    (ScalarValue::F64(x), ScalarValue::F64(y)) => ScalarValue::F64(x + y),
                    (ScalarValue::Null, ScalarValue::F64(y)) => ScalarValue::F64(*y),
                    (ScalarValue::I64(x), ScalarValue::I64(y)) => ScalarValue::I64(x + y),
                    (ScalarValue::Null, ScalarValue::I64(y)) => ScalarValue::I64(*y),
                    _ => {
                        return Err(EngineError::TypeMismatch {
                            expected: measure_meta.dtype.clone(),
                            got: req.new_value.clone(),
                        });
                    }
                };
                (summed, req.new_value.clone())
            }
        };

        // (8) Type check. Per spec §13 I-WB-9.
        if !measure_meta.dtype.matches(&type_check_value) {
            return Err(EngineError::TypeMismatch {
                expected: measure_meta.dtype.clone(),
                got: req.new_value.clone(),
            });
        }

        // (9) NaN / Inf reject. Per spec §3.18 + §0.A's reaffirmation
        //     of "NaN must never appear in storage."
        if let ScalarValue::F64(v) = &type_check_value {
            validate_finite_f64(*v)?;
        }

        // (10) Optimistic concurrency. Per spec §13 I-WB-8.
        if let Some(expected) = req.expected_revision {
            if expected != self.revision {
                return Err(EngineError::StaleRevision {
                    expected,
                    current: self.revision,
                });
            }
        }

        // (11) Commit: bump revision, write the cell, dirty the closure.
        let revision_before = self.revision;
        self.revision = self.revision.next();
        let revision_after = self.revision;

        let old_stored = self.store.read(&req.coord).cloned();
        let old_value = old_stored.as_ref().map(|s| CellValue {
            value: s.value.clone(),
            dtype: measure_meta.dtype.clone(),
            provenance: s.provenance.clone(),
            uncertainty: s.uncertainty.clone(),
            trace: None,
            revision: s.revision,
        });

        let new_provenance = Provenance::Input {
            written_at: req.now_unix_seconds,
            written_by: req.principal,
        };
        self.store.write(
            req.coord.clone(),
            StoredCell {
                value: intent_value.clone(),
                provenance: new_provenance.clone(),
                uncertainty: None,
                revision: revision_after,
            },
        );

        // Dirty propagation. Per spec §8 + §16:
        //   - Mark the closure of dependents in the rule graph.
        //   - Mark hierarchy ancestors at this coord across each
        //     consolidated dimension AND every derived measure.
        //
        // Per Phase 2D handoff §A and the brief's
        // `WritebackResult.invalidated` type doc ("Coordinates marked
        // dirty by THIS write — both rule dependents and hierarchy
        // ancestors") + engine-semantics.md I-WB-7 + the worked
        // example at §13 ("invalidated includes: <THIS write's
        // freshly-dirtied cells>"), `invalidated` is the *marginal*
        // set of coords this write transitions from clean to dirty —
        // NOT the cumulative dirty state across the cube's lifetime.
        // The earlier Phase 1A `self.dirty.iter().cloned().collect()`
        // was a misreading of the brief's compact pseudocode (line
        // 1938's "<full dirty set>") that conflicted with the brief's
        // own type doc; corrected in Phase 2D per §A.2's spec audit.
        // Cost impact: per-write `invalidated` cost was
        // O(|cumulative dirty|) — that's the PERF.md §6.14
        // super-linear cliff (`load_canonical_inputs/50x` = 230 s,
        // 23× over the ADR-0003 patience-limit gate). The bitset
        // makes the `is_dirty` check below O(1), so the marginal
        // capture is bounded by the per-write fan-out (~216 at Acme,
        // §10.1) instead of the cumulative dirty size.

        // Capture marks that transition from clean → dirty during
        // this write. Each `is_dirty` + `mark` pair is O(1) on the
        // bitset path; on the AHashSet fallback path it remains
        // bounded by the per-write mark count (~216 at Acme), not
        // the cumulative dirty size.
        let dependents = self.deps.closure_of_dependents(&req.coord);
        let ancestors = self.compute_dirty_ancestors(&req.coord, measure_id);
        let mut invalidated: Vec<CellCoordinate> =
            Vec::with_capacity(dependents.len() + ancestors.len());
        for c in dependents {
            if !self.dirty.is_dirty(&c) {
                invalidated.push(c.clone());
            }
            self.dirty.mark(c);
        }
        for c in ancestors {
            if !self.dirty.is_dirty(&c) {
                invalidated.push(c.clone());
            }
            self.dirty.mark(c);
        }

        // Soft-lock advisories (§18).
        let soft_lock_notes: Vec<String> = self
            .locks
            .soft_locks_covering(&self.dimensions, &req.coord)
            .into_iter()
            .filter_map(|l| l.note.clone())
            .collect();

        let new_value = CellValue {
            value: intent_value,
            dtype: measure_meta.dtype.clone(),
            provenance: new_provenance,
            uncertainty: None,
            trace: None,
            revision: revision_after,
        };
        Ok(WritebackResult {
            coord: req.coord,
            old_value,
            new_value,
            revision_before,
            revision_after,
            invalidated,
            soft_lock_notes,
        })
    }

    /// For a leaf write at `coord`, compute every consolidated-coord
    /// ancestor across the (Time, Channel, Market) hierarchies — for
    /// `measure_id` (the written measure) AND for every derived
    /// measure that consolidates from the same leaves. Per spec §8.
    fn compute_dirty_ancestors(
        &self,
        coord: &CellCoordinate,
        measure_id: ElementId,
    ) -> Vec<CellCoordinate> {
        // Step 1: gather, per hierarchical dim, the list of {self, ancestors}.
        let mut per_dim_options: Vec<Vec<ElementId>> = Vec::with_capacity(self.dimensions.len());
        for (i, dim) in self.dimensions.iter().enumerate() {
            let element = coord.element_at(i);
            let mut options = vec![element];
            if dim.kind != DimensionKind::Measure {
                let h = dim.default_hierarchy();
                if !h.edges.is_empty() {
                    for (anc, _w) in h.ancestors(element) {
                        options.push(anc);
                    }
                }
            }
            per_dim_options.push(options);
        }

        // Step 2: gather the measures to dirty-mark across these
        // ancestor coords. Includes:
        //   - the written measure itself (Spend → roll up Spend at
        //     ancestors)
        //   - every derived measure (their consolidated forms read leaf
        //     derived values, which now need recompute)
        let measure_position = self.measure_dimension_position;
        let mut measures_to_mark: Vec<ElementId> = vec![measure_id];
        for element in &self.measure_dimension().elements {
            if let Some(meta) = element.measure_meta() {
                if meta.role == MeasureRole::Derived && element.id != measure_id {
                    measures_to_mark.push(element.id);
                }
            }
        }

        // Step 3: walk the Cartesian product of per-dim options, with
        // the measure slot replaced by each measure-to-mark. The
        // exact-written coord (same hierarchy slots AND same measure)
        // is freshly written and not dirty; everything else in the
        // Cartesian product is.
        //
        // Importantly: at the pure-leaf hierarchy position (all "self"
        // indices), we still mark the OTHER measures-to-mark (the 5
        // derived measures that read this leaf via SelfRef rules);
        // their cached values, if any, are now stale. Only the exact
        // (leaf, written_measure) coord is skipped.
        let mut out: Vec<CellCoordinate> = Vec::new();
        let mut indices = vec![0usize; per_dim_options.len()];
        loop {
            let elements: Vec<ElementId> = (0..per_dim_options.len())
                .map(|i| per_dim_options[i][indices[i]])
                .collect();
            let is_pure_leaf = indices.iter().all(|&i| i == 0);
            for &m in &measures_to_mark {
                if is_pure_leaf && m == measure_id {
                    // The cell that was just written — fresh, not dirty.
                    continue;
                }
                let mut e = elements.clone();
                e[measure_position] = m;
                out.push(CellCoordinate::from_parts(self.id, e.into_iter()));
            }
            // Increment indices.
            let mut carried = true;
            let mut i = per_dim_options.len();
            while i > 0 {
                i -= 1;
                indices[i] += 1;
                if indices[i] < per_dim_options[i].len() {
                    carried = false;
                    break;
                }
                indices[i] = 0;
            }
            if carried {
                break;
            }
        }
        out
    }

    /// Tier 2 amortization variant of `compute_dirty_ancestors`: walks
    /// the same Cartesian product but marks each ancestor directly
    /// into `self.dirty` (the bitset) instead of building an
    /// intermediate `Vec<CellCoordinate>`. Skips the (pure-leaf,
    /// self-measure) entry just like its sibling. Returns the number
    /// of marks attempted (NOT the number of cells freshly dirtied —
    /// the bitset's `mark` is idempotent, so duplicates across the
    /// batch are no-op bit-tests).
    ///
    /// Uses `SmallVec` for `per_dim_options`, the per-dim option
    /// lists, the `indices` cursor, and the per-coord `elements`
    /// buffer so the entire walk runs heap-free for cubes with ≤ 8
    /// dims and ≤ 8 ancestors per dim. At Acme/100×, both bounds are
    /// satisfied (6 dims; deepest hierarchy is Time/Channel/Market at
    /// 4 levels each).
    pub(crate) fn mark_dirty_ancestors_inline(
        &mut self,
        coord: &CellCoordinate,
        measure_id: ElementId,
    ) {
        // Step 1: per-dim options. Each dim gets [self, ancestor_1,
        // ancestor_2, ...]. SmallVec inline capacity 8 covers every
        // realistic Phase 1/Phase 5 cube without heap.
        let dim_count = self.dimensions.len();
        let mut per_dim_options: SmallVec<[SmallVec<[ElementId; 8]>; 8]> =
            SmallVec::with_capacity(dim_count);
        for (i, dim) in self.dimensions.iter().enumerate() {
            let element = coord.element_at(i);
            let mut options: SmallVec<[ElementId; 8]> = SmallVec::new();
            options.push(element);
            if dim.kind != DimensionKind::Measure {
                let h = dim.default_hierarchy();
                if !h.edges.is_empty() {
                    for (anc, _w) in h.ancestors(element) {
                        options.push(anc);
                    }
                }
            }
            per_dim_options.push(options);
        }

        // Step 2: measures_to_mark. Includes the written measure plus
        // every Derived measure on this cube. SmallVec inline capacity
        // 8 covers Acme's 6 inputs + 5 derived = 11 measures cleanly
        // (the per-walk subset is at most ~6).
        let measure_position = self.measure_dimension_position;
        let mut measures_to_mark: SmallVec<[ElementId; 8]> = SmallVec::new();
        measures_to_mark.push(measure_id);
        for element in &self.measure_dimension().elements {
            if let Some(meta) = element.measure_meta() {
                if meta.role == MeasureRole::Derived && element.id != measure_id {
                    measures_to_mark.push(element.id);
                }
            }
        }

        // Step 3: walk + mark. No `out` Vec; no caller-side iterate-
        // and-mark. The bitset's `mark` is O(1) and idempotent so
        // duplicate marks across overlapping ancestor sets cost only
        // a bit-test.
        let mut indices: SmallVec<[usize; 8]> = SmallVec::from_elem(0usize, dim_count);
        let mut elements: SmallVec<[ElementId; 8]> = SmallVec::with_capacity(dim_count);
        loop {
            elements.clear();
            for i in 0..dim_count {
                elements.push(per_dim_options[i][indices[i]]);
            }
            let is_pure_leaf = indices.iter().all(|&i| i == 0);
            for &m in &measures_to_mark {
                if is_pure_leaf && m == measure_id {
                    continue;
                }
                let saved = elements[measure_position];
                elements[measure_position] = m;
                self.dirty.mark(CellCoordinate::from_parts(
                    self.id,
                    elements.iter().copied(),
                ));
                elements[measure_position] = saved;
            }
            // Increment indices (carry-on overflow).
            let mut carried = true;
            let mut i = dim_count;
            while i > 0 {
                i -= 1;
                indices[i] += 1;
                if indices[i] < per_dim_options[i].len() {
                    carried = false;
                    break;
                }
                indices[i] = 0;
            }
            if carried {
                break;
            }
        }
    }

    // --- Batch fast path (Phase 5A Stream A — WriteBatch) ---
    //
    // The two `pub(crate)` helpers below are the cube-side entry points
    // for [`crate::batch::WriteBatch`]. They split the per-cell `write()`
    // path into a validate-first, apply-second shape so the public
    // `WriteBatch::commit()` can:
    //   1. Validate every staged write up-front (no mutation, no
    //      snapshot cost on failure).
    //   2. Snapshot once (Amendment #5: at apply time, not at stage time).
    //   3. Apply with a single revision bump, batched store writes, and
    //      a deduplicating dirty propagation pass.
    //
    // The amortization is the speedup: per-cell `write()` does N
    // revision bumps + N dirty-set Vec allocations + N
    // `compute_dirty_ancestors` walks; `batch_apply_validated` does 1
    // revision bump and reuses the bitset's O(1) `is_dirty` to dedupe
    // overlapping ancestor sets across the whole batch. See PERF.md
    // §6.16 (per-cell baselines) and §6.17 (WriteBatch results) for
    // the measured before/after.

    /// Validate every staged write in a [`WriteBatch`] up-front. Runs
    /// the same safety checks as [`Cube::write`] (steps 1-9: permission,
    /// cube id, arity, consolidated, derived, version state, lock,
    /// type, NaN/Inf reject). Does NOT mutate any visible cube state
    /// (revision, store, dirty are unchanged); the only side effect is
    /// `LockTable::check_write`'s incidental purge of expired locks,
    /// which is internal bookkeeping not part of the public state.
    ///
    /// The optimistic-concurrency check (`expected_revision`) is
    /// intentionally omitted: `WriteBatch` is a bulk-import path with
    /// no per-cell revision preconditions; snapshot-and-rollback
    /// semantics handle concurrency at the batch granularity.
    ///
    /// Returns one `BatchPrepared` per staged write on success;
    /// returns the FIRST validation error on failure (consistent with
    /// `Cube::write`'s fail-fast behavior — atomicity contract per
    /// ADR-0010 Decision 3).
    pub(crate) fn batch_validate_all(
        &mut self,
        staged: &[(CellCoordinate, ScalarValue)],
        principal: PrincipalId,
        now_unix_seconds: u64,
    ) -> Result<Vec<BatchPrepared>, EngineError> {
        let mut prepared = Vec::with_capacity(staged.len());
        for (coord, value) in staged {
            // Per engine-semantics.md §13 I-WB-1..I-WB-5 + I-WB-9 +
            // §3.18 NaN-reject: full per-cell validation, mirror of
            // `Cube::write` steps 1-9.
            let measure_id = self.batch_validate_one(coord, value, principal, now_unix_seconds)?;
            prepared.push(BatchPrepared {
                coord: coord.clone(),
                value: value.clone(),
                measure_id,
            });
        }
        Ok(prepared)
    }

    /// Per-cell validation. Mirrors `Cube::write` steps 1-9 but does
    /// NOT mutate any visible cube state.
    fn batch_validate_one(
        &mut self,
        coord: &CellCoordinate,
        value: &ScalarValue,
        principal: PrincipalId,
        now_unix_seconds: u64,
    ) -> Result<ElementId, EngineError> {
        // Per engine-semantics.md §13 I-WB-5: permission check first.
        if !self
            .permissions
            .check(principal, &self.dimensions, coord, capability::WRITE)
        {
            return Err(EngineError::InsufficientPermission {
                principal,
                coord: coord.clone(),
            });
        }
        // Cube id / arity (Internal — caller bug, not user-facing).
        if coord.cube != self.id {
            return Err(EngineError::Internal(
                "Cube::batch_validate_one: coordinate cube id does not match this cube",
            ));
        }
        if coord.elements().len() != self.dimensions.len() {
            return Err(EngineError::Internal(
                "Cube::batch_validate_one: coordinate arity does not match cube dimension count",
            ));
        }
        // Per engine-semantics.md §13 I-WB-1: consolidated coords are
        // not writable.
        if self.is_consolidated_coord(coord) {
            return Err(EngineError::ConsolidatedCellNotWritable {
                coord: coord.clone(),
            });
        }
        // Per engine-semantics.md §13 I-WB-2: derived measures are not
        // writable.
        let (measure_id, measure_meta) = self.measure_at_coord(coord)?;
        if measure_meta.role == MeasureRole::Derived {
            return Err(EngineError::DerivedCellNotWritable {
                coord: coord.clone(),
            });
        }
        // Per engine-semantics.md §13 I-WB-3 / §9: writes to
        // Approved/Archived versions are rejected.
        if let Some(version_dim) = self
            .dimensions
            .iter()
            .find(|d| d.kind == DimensionKind::Version)
        {
            let version_position = self
                .dimensions
                .iter()
                .position(|d| d.id == version_dim.id)
                .expect("version dim is in dimensions");
            let version_element = coord.element_at(version_position);
            if let Some(element) = version_dim.element(version_element) {
                if let Some(state) = element.version_state() {
                    if matches!(state, VersionState::Approved | VersionState::Archived) {
                        return Err(EngineError::LockedVersion {
                            version: version_element,
                            state,
                        });
                    }
                }
            }
        }
        // Per engine-semantics.md §13 I-WB-4: hard locks block.
        // `check_write` purges expired locks as a side effect; that
        // purge is acceptable inside a validate-only path because
        // expired locks are not part of the public state contract.
        if let Some(blocking) =
            self.locks
                .check_write(principal, &self.dimensions, coord, now_unix_seconds)
        {
            return Err(EngineError::LockedCell {
                coord: coord.clone(),
                owner: blocking.owner,
            });
        }
        // Per engine-semantics.md §13 I-WB-9: type check. WriteBatch
        // implements Set semantics only (the brief's `WriteIntent::Set`
        // case) — no Increment, no Clear — so the value is type-checked
        // directly against the measure's declared dtype.
        if !measure_meta.dtype.matches(value) {
            return Err(EngineError::TypeMismatch {
                expected: measure_meta.dtype.clone(),
                got: value.clone(),
            });
        }
        // Per engine-semantics.md §3.18 + §0.A: NaN must never appear
        // in storage.
        if let ScalarValue::F64(v) = value {
            validate_finite_f64(*v)?;
        }
        Ok(measure_id)
    }

    /// Apply a validated batch. Bumps the cube revision ONCE, writes
    /// every prepared cell, and propagates dirty marks across the
    /// union of all affected ancestors with O(1) per-mark dedup via
    /// the bitset tracker.
    ///
    /// **The Tier 1 amortization headline:** per-cell
    /// [`Cube::write`](Self::write) does `N` revision bumps, `N`
    /// `Vec<CellCoordinate>` allocations for `WritebackResult.invalidated`,
    /// and `N` `closure_of_dependents` + `compute_dirty_ancestors`
    /// passes. This method does **1** revision bump, **0** per-cell
    /// invalidated-Vec allocations (the `CommitResult` reports
    /// counts, not the full coord list), and `N`
    /// `compute_dirty_ancestors` calls — but the per-mark cost is
    /// O(1) on the bitset path, so duplicate marks across overlapping
    /// ancestor sets are no-ops in the bit-test. The newly-dirtied
    /// count is the post − pre `dirty.len()` delta, which is exactly
    /// the marginal-set cardinality without per-coord bookkeeping.
    ///
    /// Caller is expected to have already snapshotted (Phase 2 of
    /// `WriteBatch::commit`) so an `Err` return can be matched with
    /// `Cube::rollback_to(&snapshot)`. Per Phase 5A, the per-cell
    /// validation in `batch_validate_all` covers every write-side
    /// failure mode, so this path returns `Ok` on every well-formed
    /// non-empty batch; the `Result` return type is defense-in-depth
    /// for `EngineError::Internal` invariants.
    pub(crate) fn batch_apply_validated(
        &mut self,
        prepared: Vec<BatchPrepared>,
        principal: PrincipalId,
        now_unix_seconds: u64,
    ) -> Result<BatchApplyOutcome, EngineError> {
        let revision_before = self.revision;
        let dirty_count_before = self.dirty.len();

        // Empty batch: no-op. The empty case is filtered upstream by
        // `WriteBatch::commit` to skip the snapshot, but defending in
        // depth here lets callers (including future Tessera bench
        // harnesses) call this directly without re-checking.
        if prepared.is_empty() {
            return Ok(BatchApplyOutcome {
                revision_before,
                revision_after: revision_before,
                newly_dirtied_count: 0,
                dirty_count_after: dirty_count_before,
            });
        }

        // Single revision bump for the entire batch — the headline
        // amortization. All cells written below carry `revision_after`
        // as their `StoredCell.revision`.
        self.revision = self.revision.next();
        let revision_after = self.revision;

        let provenance = Provenance::Input {
            written_at: now_unix_seconds,
            written_by: principal,
        };

        // Apply: write each cell, propagate dirty in aggregate. The
        // bitset's `is_dirty`/`mark` are O(1) and idempotent — duplicate
        // marks across overlapping ancestor sets become no-ops in the
        // bit-test path. The newly-dirtied count falls out of
        // `dirty.len()` deltas (no per-coord transition bookkeeping
        // needed), which is the marginal-set semantics extended to a
        // batch.
        for prep in &prepared {
            self.store.write(
                prep.coord.clone(),
                StoredCell {
                    value: prep.value.clone(),
                    provenance: provenance.clone(),
                    uncertainty: None,
                    revision: revision_after,
                },
            );
            // Tier 1: rule dependents. `mark_closure` calls
            // `closure_of_dependents` which still allocates an
            // AHashSet per call — the per-Acme-leaf fan-out here is
            // small (typically 0-5 entries via `SelfRef` rules at the
            // leaf), so the alloc cost is negligible and we keep
            // existing dependency-graph semantics unchanged.
            self.dirty.mark_closure(&prep.coord, &self.deps);
            // Tier 2: hierarchy ancestor walk inline. `mark_dirty_ancestors_inline`
            // mirrors `compute_dirty_ancestors` but marks directly
            // into the bitset, skipping the per-cell
            // `Vec<CellCoordinate>` allocation that `Cube::write`
            // pays. At ~200 ancestor coords per write × 1M cells the
            // saved allocation cost is the load-bearing Tier 2
            // amortization. See PERF.md §6.17 for the measured impact.
            self.mark_dirty_ancestors_inline(&prep.coord, prep.measure_id);
        }

        let dirty_count_after = self.dirty.len();
        let newly_dirtied_count = dirty_count_after.saturating_sub(dirty_count_before);

        Ok(BatchApplyOutcome {
            revision_before,
            revision_after,
            newly_dirtied_count,
            dirty_count_after,
        })
    }

    // --- Snapshot ---

    pub fn snapshot(&self, label: Option<&str>) -> Snapshot {
        Snapshot {
            cube: self.id,
            revision: self.revision,
            captured_at: 0,
            label: label.map(str::to_string),
            store: self.store.clone(),
        }
    }

    pub fn rollback_to(&mut self, snap: &Snapshot) -> Result<Revision, EngineError> {
        if snap.cube != self.id {
            return Err(EngineError::SnapshotCubeMismatch);
        }
        // Replace the store with a clone of the snapshot's. Bump the
        // revision (rollback is a state change). Drop every cached
        // derived-cell entry — they'll be lazily recomputed on next
        // read against whatever rule definitions are current.
        self.store = snap.store.clone();
        self.revision = self.revision.next();
        self.dirty.clear_all();
        // Prune any Rule-provenance cells that came along on the clone;
        // they were valid at the snapshot's revision but their
        // `revision` field will appear stale at the new live revision,
        // and rather than have read paths special-case that we just
        // drop them.
        let stale: Vec<CellCoordinate> = self
            .store
            .iter()
            .filter_map(|(c, s)| match s.provenance {
                Provenance::Rule { .. } => Some(c.clone()),
                _ => None,
            })
            .collect();
        for c in stale {
            self.store.remove(&c);
        }
        Ok(self.revision)
    }

    // --- Lock + permission helpers (mirror their tables for the cube
    //     layer's capability checks). ---

    /// Acquire a lock through the cube. Per spec §18 I-Lock-5: caller
    /// must have both `LOCK` and `WRITE` capabilities on the lock's
    /// pattern. (Phase 1 enforces this at the cube level so the lock
    /// table itself can stay independent of `PermissionTable`.)
    pub fn acquire_lock(&mut self, lock: Lock) -> Result<crate::id::LockId, EngineError> {
        let principal = lock.owner;
        if !self.permissions.check(
            principal,
            &self.dimensions,
            // Use a synthetic coord (any leaf in the pattern) for the
            // capability check — Phase 1 simplification: if you're
            // root, the check passes; non-root principals must already
            // have LOCK + WRITE somewhere intersecting the pattern,
            // which the integration tests don't currently exercise.
            // Future hardening: walk pattern-bound leaves and check
            // each.
            &dummy_check_coord(self.id, &self.dimensions),
            capability::LOCK,
        ) {
            return Err(EngineError::InsufficientPermission {
                principal,
                coord: dummy_check_coord(self.id, &self.dimensions),
            });
        }
        match self.locks.acquire(lock, &self.dimensions) {
            Ok(id) => Ok(id),
            Err(crate::lock::ConflictKind::Hard { existing: _, owner }) => {
                Err(EngineError::LockedCell {
                    coord: dummy_check_coord(self.id, &self.dimensions),
                    owner,
                })
            }
        }
    }

    pub fn release_lock(
        &mut self,
        lock_id: crate::id::LockId,
        principal: PrincipalId,
    ) -> Result<(), EngineError> {
        match self.locks.release(lock_id, principal) {
            Ok(()) => Ok(()),
            Err(crate::lock::ReleaseError::NotFound) => {
                Err(EngineError::Internal("Cube::release_lock: lock not found"))
            }
            Err(crate::lock::ReleaseError::NotOwner) => Err(EngineError::InsufficientPermission {
                principal,
                coord: dummy_check_coord(self.id, &self.dimensions),
            }),
        }
    }

    /// Add a permission grant. No capability check — by Phase 1
    /// semantics only the root principal grants permissions, and the
    /// caller is expected to be enforcing that at a layer above the
    /// engine.
    pub fn grant(&mut self, grant: crate::permission::Grant) {
        self.permissions.grant(grant);
    }
}

/// A WritebackRequest payload. Per spec §3.18.
#[derive(Clone, Debug)]
pub struct WritebackRequest {
    pub coord: CellCoordinate,
    pub new_value: ScalarValue,
    pub principal: PrincipalId,
    pub intent: WriteIntent,
    pub expected_revision: Option<Revision>,
    /// Unix-seconds timestamp threaded through to the stored
    /// `Provenance::Input { written_at }` and to `LockTable` purges.
    /// Tests pass a fixed value (e.g., 0) for determinism.
    pub now_unix_seconds: u64,
}

#[derive(Clone, Copy, Debug)]
pub enum WriteIntent {
    Set,
    Increment,
    Clear,
}

#[derive(Clone, Debug)]
pub struct WritebackResult {
    pub coord: CellCoordinate,
    pub old_value: Option<CellValue>,
    pub new_value: CellValue,
    pub revision_before: Revision,
    pub revision_after: Revision,
    pub invalidated: Vec<CellCoordinate>,
    pub soft_lock_notes: Vec<String>,
}

/// Internal: a write that has passed every `batch_validate_one` check
/// and is ready to apply via `Cube::batch_apply_validated`.
///
/// `pub(crate)` only — the batch fast path is an internal detail
/// shared between [`crate::cube::Cube`] and [`crate::batch::WriteBatch`].
/// External callers use the [`crate::batch::WriteBatch`] public API.
#[derive(Clone, Debug)]
pub(crate) struct BatchPrepared {
    pub(crate) coord: CellCoordinate,
    pub(crate) value: ScalarValue,
    pub(crate) measure_id: ElementId,
}

/// Internal: outcome of `Cube::batch_apply_validated`. Maps directly
/// onto the public [`crate::batch::CommitResult`] minus the
/// caller-supplied `snapshot_id`, `rows_written`, and `rows_failed`
/// fields, which are filled in by `WriteBatch::commit`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BatchApplyOutcome {
    pub(crate) revision_before: Revision,
    pub(crate) revision_after: Revision,
    pub(crate) newly_dirtied_count: usize,
    pub(crate) dirty_count_after: usize,
}

fn dummy_check_coord(cube: CubeId, dims: &[Dimension]) -> CellCoordinate {
    // Build a coord using the first element of each dim. Used only as a
    // permission-check stand-in for Phase 1; never persisted.
    let elements: Vec<ElementId> = dims.iter().map(|d| d.elements[0].id).collect();
    CellCoordinate::from_parts(cube, elements)
}

/// Convert a `SliceBinding` to the concrete element list against a
/// dimension. `Subtree(root)` walks the dim's default hierarchy to
/// gather descendants; for a flat hierarchy it returns just the root.
fn resolve_binding(binding: &SliceBinding, dim: &Dimension) -> Vec<ElementId> {
    match binding {
        SliceBinding::One(e) => vec![*e],
        SliceBinding::Many(es) => es.clone(),
        SliceBinding::Subtree(root) => {
            let h = dim.default_hierarchy();
            if h.edges.is_empty() {
                vec![*root]
            } else {
                let mut out: Vec<ElementId> = Vec::new();
                if h.is_leaf(*root) {
                    out.push(*root);
                } else {
                    for (leaf, _w) in h.descendants(*root) {
                        out.push(leaf);
                    }
                }
                out
            }
        }
        SliceBinding::All => {
            let h = dim.default_hierarchy();
            if h.edges.is_empty() {
                // Synthesized flat — every element counts.
                dim.elements.iter().map(|e| e.id).collect()
            } else {
                // Sort for deterministic order (per CLAUDE.md §2.11).
                let mut leaves: Vec<ElementId> = h.leaves.iter().copied().collect();
                leaves.sort();
                leaves
            }
        }
        SliceBinding::AllConsolidated => {
            let h = dim.default_hierarchy();
            if h.edges.is_empty() {
                Vec::new()
            } else {
                let mut consolidated: Vec<ElementId> = h.consolidated.iter().copied().collect();
                consolidated.sort();
                consolidated
            }
        }
    }
}

fn top_level_expr_op(expr: &Expr) -> ExprOp {
    match expr {
        Expr::Const(_) => ExprOp::Const,
        Expr::SelfRef(_) => ExprOp::SelfRef,
        Expr::Add(_, _) => ExprOp::Add,
        Expr::Sub(_, _) => ExprOp::Sub,
        Expr::Mul(_, _) => ExprOp::Mul,
        Expr::Div(_, _) => ExprOp::Div,
        Expr::IfNull(_, _) => ExprOp::IfNull,
        // Phase 3E+: new ops map to a generic trace label
        _ => ExprOp::Const, // placeholder for trace rendering
    }
}

// ===========================================================================
// CubeBuilder
// ===========================================================================

#[derive(Debug)]
pub struct CubeBuilder {
    id: CubeId,
    name: String,
    dimensions: Vec<Dimension>,
    measure_dimension_name: Option<String>,
    staged_rules: Vec<Rule>,
    root_principal: Option<PrincipalId>,
}

impl CubeBuilder {
    pub fn add_dimension(mut self, dim: Dimension) -> Self {
        self.dimensions.push(dim);
        self
    }

    pub fn measure_dimension(mut self, name: impl Into<String>) -> Self {
        self.measure_dimension_name = Some(name.into());
        self
    }

    pub fn add_rule(mut self, rule: Rule) -> Result<Self, EngineError> {
        // Cube-aware validation that RuleSet::add doesn't do (per
        // rule.rs module doc): target is Derived, every SelfRef
        // references a measure that exists, body is well-typed.
        let measure_dim = self
            .dimensions
            .iter()
            .find(|d| d.kind == DimensionKind::Measure)
            .ok_or(EngineError::Internal(
                "CubeBuilder::add_rule: no measure dimension declared yet",
            ))?;
        let target =
            measure_dim
                .element(rule.target_measure)
                .ok_or(EngineError::ElementNotFound(
                    rule.target_measure,
                    measure_dim.id,
                ))?;
        let target_meta = target.measure_meta().ok_or(EngineError::Internal(
            "CubeBuilder::add_rule: target is not a measure element",
        ))?;
        if target_meta.role != MeasureRole::Derived {
            return Err(EngineError::RuleTargetNotDerived {
                role: target_meta.role,
            });
        }

        // Walk the body and verify every SelfRef measure exists in the
        // measure dim and is F64-typed.
        validate_expr_well_typed(&rule.body, measure_dim)?;

        self.staged_rules.push(rule);
        Ok(self)
    }

    pub fn root_principal(mut self, p: PrincipalId) -> Self {
        self.root_principal = Some(p);
        self
    }

    pub fn build(self) -> Result<Cube, EngineError> {
        if self.dimensions.is_empty() {
            return Err(EngineError::Internal(
                "CubeBuilder::build: no dimensions declared",
            ));
        }

        // Resolve the measure dimension by name (or pick the unique
        // Measure-kind dim as fallback).
        let measure_dimension_position = if let Some(ref name) = self.measure_dimension_name {
            self.dimensions
                .iter()
                .position(|d| d.name == *name)
                .ok_or(EngineError::DimensionNotFound { name: name.clone() })?
        } else {
            // Fallback: the unique DimensionKind::Measure dim.
            let measure_dims: Vec<usize> = self
                .dimensions
                .iter()
                .enumerate()
                .filter(|(_, d)| d.kind == DimensionKind::Measure)
                .map(|(i, _)| i)
                .collect();
            if measure_dims.len() != 1 {
                return Err(EngineError::Internal(
                    "CubeBuilder::build: cannot resolve unique measure dimension; \
                     declare it via .measure_dimension()",
                ));
            }
            measure_dims[0]
        };

        // Verify the resolved dim is in fact a Measure dim.
        if self.dimensions[measure_dimension_position].kind != DimensionKind::Measure {
            return Err(EngineError::Internal(
                "CubeBuilder::build: declared measure_dimension is not DimensionKind::Measure",
            ));
        }

        let root_principal = self.root_principal.unwrap_or(PrincipalId(1));
        let mut rules = RuleSet::new();
        for rule in self.staged_rules {
            rules.add(rule)?;
        }

        // Freeze every dimension.
        let mut dimensions = self.dimensions;
        for d in &mut dimensions {
            d.freeze();
        }

        // Per Phase 2D (PERF.md §6.14 / §9.3 closure): precompute the
        // Cartesian-product shape so the dirty tracker can mark/check
        // via O(1) bit math instead of a hash-and-insert into an
        // AHashSet that rehashes as it saturates. `CubeShape::new`
        // returns `None` if the Cartesian cardinality overflows the
        // bitset budget; in that (Phase 2D-uncalibrated) regime the
        // tracker falls back to the AHashSet representation via
        // `DirtyTracker::new()`.
        let cube_shape = CubeShape::new(&dimensions);
        let dirty = match &cube_shape {
            Some(shape) => DirtyTracker::with_shape(Arc::clone(shape)),
            None => DirtyTracker::new(),
        };

        Ok(Cube {
            id: self.id,
            name: self.name,
            // Wrap in `Arc` per Phase 2B fast path: the cube struct holds
            // dimensions immutably from build onward, and `read_consolidated`
            // uses the Arc to hand a borrow-independent snapshot to
            // `Consolidator::read` for one refcount bump per call.
            dimensions: Arc::new(dimensions),
            measure_dimension_position,
            cube_shape,
            rules,
            locks: LockTable::new(self.id),
            permissions: PermissionTable::new(self.id, root_principal),
            store: HashMapStore::new(),
            revision: Revision::ZERO,
            deps: DependencyGraph::new(),
            dirty,
        })
    }
}

fn validate_expr_well_typed(expr: &Expr, measure_dim: &Dimension) -> Result<(), EngineError> {
    match expr {
        Expr::Const(v) => match v {
            ScalarValue::F64(f) => {
                if !f.is_finite() {
                    return Err(EngineError::RuleBodyTypeMismatch {
                        detail: format!("Const F64({f}) is not finite"),
                    });
                }
                Ok(())
            }
            ScalarValue::Null => Ok(()),
            other => Err(EngineError::RuleBodyTypeMismatch {
                detail: format!("Phase 1 only supports F64/Null Const; got {other:?}"),
            }),
        },
        Expr::SelfRef(measure) => {
            let element = measure_dim
                .element(*measure)
                .ok_or(EngineError::ElementNotFound(*measure, measure_dim.id))?;
            let meta = element
                .measure_meta()
                .ok_or(EngineError::RuleBodyTypeMismatch {
                    detail: format!(
                        "SelfRef({measure:?}) refers to an element with no MeasureMeta"
                    ),
                })?;
            if !matches!(meta.dtype, CellDataType::F64) {
                return Err(EngineError::RuleBodyTypeMismatch {
                    detail: format!(
                        "SelfRef({measure:?}) is dtype {:?}, but Phase 1 rules require F64",
                        meta.dtype
                    ),
                });
            }
            Ok(())
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
            validate_expr_well_typed(a, measure_dim)?;
            validate_expr_well_typed(b, measure_dim)?;
            Ok(())
        }
        Expr::Not(a) | Expr::Abs(a) | Expr::Bucket(a, _) => {
            validate_expr_well_typed(a, measure_dim)
        }
        Expr::If(a, b, c) | Expr::SafeDiv(a, b, c) | Expr::Clamp(a, b, c) => {
            validate_expr_well_typed(a, measure_dim)?;
            validate_expr_well_typed(b, measure_dim)?;
            validate_expr_well_typed(c, measure_dim)?;
            Ok(())
        }
        Expr::Min(args) | Expr::Max(args) | Expr::Coalesce(args) => {
            for a in args {
                validate_expr_well_typed(a, measure_dim)?;
            }
            Ok(())
        }
        Expr::ActualRef(m) | Expr::Prev(m) | Expr::Cumulative(m) => {
            // Validate the measure reference exists
            let _ = measure_dim
                .element(*m)
                .ok_or(EngineError::ElementNotFound(*m, measure_dim.id))?;
            Ok(())
        }
        Expr::Lag(m, periods) | Expr::RollingAvg(m, periods) => {
            let _ = measure_dim
                .element(*m)
                .ok_or(EngineError::ElementNotFound(*m, measure_dim.id))?;
            validate_expr_well_typed(periods, measure_dim)?;
            Ok(())
        }
        Expr::PeriodIndex
        | Expr::AnchorIndex
        | Expr::IsPast
        | Expr::IsCurrent
        | Expr::IsFuture
        | Expr::PeriodsSinceAnchor
        | Expr::PeriodsToEnd => Ok(()),
        Expr::Benchmark(_, key) | Expr::Lookup(_, key) => {
            validate_expr_well_typed(key, measure_dim)
        }
        Expr::SumOver(_, m) => {
            let _ = measure_dim
                .element(*m)
                .ok_or(EngineError::ElementNotFound(*m, measure_dim.id))?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dimension::{Dimension, DimensionKind};
    use crate::element::{AggregationRule, Element, MeasureRole};
    use crate::hierarchy::Hierarchy;
    use crate::id::{ElementId, IdGenerator};
    use crate::value::CellDataType;

    /// Two-dim micro-cube for sanity testing the cube builder + read +
    /// write paths without a full Acme fixture.
    fn micro_cube() -> (Cube, ElementId, ElementId, ElementId, PrincipalId) {
        let g = IdGenerator::new();
        let cube_id = g.cube();
        let market_dim_id = g.dimension();
        let measure_dim_id = g.dimension();
        let usa = g.element();
        let florida = g.element();
        let tampa = g.element();
        let spend = g.element();
        let clicks = g.element();
        let cpc = g.element();

        let market_h = Hierarchy::builder(g.hierarchy(), "geo", market_dim_id)
            .add_edge(usa, florida, 1.0)
            .add_edge(florida, tampa, 1.0)
            .build()
            .expect("hier ok");

        let market_dim = Dimension::builder(market_dim_id, "Market", DimensionKind::Standard)
            .add_element(Element::leaf(usa, "USA", market_dim_id))
            .expect("ok")
            .add_element(Element::leaf(florida, "Florida", market_dim_id))
            .expect("ok")
            .add_element(Element::leaf(tampa, "Tampa", market_dim_id))
            .expect("ok")
            .add_hierarchy(market_h)
            .expect("ok")
            .default_hierarchy("geo")
            .build()
            .expect("market dim");

        let measure_dim = Dimension::builder(measure_dim_id, "Measure", DimensionKind::Measure)
            .add_element(Element::measure(
                spend,
                "Spend",
                measure_dim_id,
                CellDataType::F64,
                MeasureRole::Input,
                AggregationRule::Sum,
            ))
            .expect("ok")
            .add_element(Element::measure(
                cpc,
                "CPC",
                measure_dim_id,
                CellDataType::F64,
                MeasureRole::Input,
                AggregationRule::Sum, // simplified for this test
            ))
            .expect("ok")
            .add_element(Element::measure(
                clicks,
                "Clicks",
                measure_dim_id,
                CellDataType::F64,
                MeasureRole::Derived,
                AggregationRule::Sum,
            ))
            .expect("ok")
            .build()
            .expect("measure dim");

        let root_principal = g.principal();
        let rule = Rule {
            id: g.rule(),
            cube: cube_id,
            target_measure: clicks,
            scope: crate::rule::Scope::AllLeaves,
            body: Expr::Div(Box::new(Expr::SelfRef(spend)), Box::new(Expr::SelfRef(cpc))),
            declared_dependencies: vec![
                crate::rule::DependencyDecl {
                    measure: spend,
                    coord_pattern: crate::rule::CoordPattern::SameAsTarget,
                },
                crate::rule::DependencyDecl {
                    measure: cpc,
                    coord_pattern: crate::rule::CoordPattern::SameAsTarget,
                },
            ],
        };

        let cube = Cube::builder(cube_id, "Micro")
            .add_dimension(market_dim)
            .add_dimension(measure_dim)
            .measure_dimension("Measure")
            .root_principal(root_principal)
            .add_rule(rule)
            .expect("rule ok")
            .build()
            .expect("cube build");
        (cube, tampa, spend, clicks, root_principal)
    }

    fn coord(cube_id: CubeId, market: ElementId, measure: ElementId) -> CellCoordinate {
        CellCoordinate::from_parts(cube_id, [market, measure])
    }

    #[test]
    fn build_cube_succeeds() {
        let (cube, _, _, _, _) = micro_cube();
        assert_eq!(cube.dimensions.len(), 2);
        assert_eq!(cube.revision, Revision(0));
    }

    #[test]
    fn write_input_then_read_returns_new_value() {
        let (mut cube, tampa, spend, _clicks, root) = micro_cube();
        let cube_id = cube.id;
        let req = WritebackRequest {
            coord: coord(cube_id, tampa, spend),
            new_value: ScalarValue::F64(11_500.0),
            principal: root,
            intent: WriteIntent::Set,
            expected_revision: Some(Revision(0)),
            now_unix_seconds: 1_700_000_000,
        };
        let result = cube.write(req).expect("write ok");
        assert_eq!(result.revision_before, Revision(0));
        assert_eq!(result.revision_after, Revision(1));

        let v = cube
            .read(&coord(cube_id, tampa, spend), root)
            .expect("read ok");
        assert_eq!(v.value.as_f64(), Some(11_500.0));
        assert_eq!(v.revision, Revision(1));
    }

    #[test]
    fn write_to_derived_rejected() {
        let (mut cube, tampa, _spend, clicks, root) = micro_cube();
        let cube_id = cube.id;
        let req = WritebackRequest {
            coord: coord(cube_id, tampa, clicks),
            new_value: ScalarValue::F64(99.0),
            principal: root,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        };
        let err = cube.write(req).expect_err("derived write must reject");
        assert!(matches!(err, EngineError::DerivedCellNotWritable { .. }));
        assert_eq!(
            cube.revision(),
            Revision(0),
            "revision must not bump on rejected write"
        );
    }

    #[test]
    fn write_to_consolidated_rejected() {
        let (mut cube, _tampa, spend, _clicks, root) = micro_cube();
        // Find Florida element from the cube.
        let market_dim = cube.dimension_by_name("Market").expect("dim");
        let florida = market_dim.element_by_name("Florida").expect("Florida").id;
        let cube_id = cube.id;
        let req = WritebackRequest {
            coord: coord(cube_id, florida, spend),
            new_value: ScalarValue::F64(99.0),
            principal: root,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        };
        let err = cube.write(req).expect_err("consolidated write must reject");
        assert!(matches!(
            err,
            EngineError::ConsolidatedCellNotWritable { .. }
        ));
    }

    #[test]
    fn nan_write_rejected() {
        let (mut cube, tampa, spend, _clicks, root) = micro_cube();
        let cube_id = cube.id;
        let req = WritebackRequest {
            coord: coord(cube_id, tampa, spend),
            new_value: ScalarValue::F64(f64::NAN),
            principal: root,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        };
        let err = cube.write(req).expect_err("NaN must be rejected");
        assert!(matches!(err, EngineError::InvalidValue(_)));
    }

    #[test]
    fn read_derived_evaluates_rule() {
        let (mut cube, tampa, spend, clicks, root) = micro_cube();
        let cube_id = cube.id;
        let cpc = cube
            .measure_dimension()
            .element_by_name("CPC")
            .expect("CPC")
            .id;
        // Write Spend = 11500 and CPC = 1.5
        cube.write(WritebackRequest {
            coord: coord(cube_id, tampa, spend),
            new_value: ScalarValue::F64(11_500.0),
            principal: root,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write spend");
        cube.write(WritebackRequest {
            coord: coord(cube_id, tampa, cpc),
            new_value: ScalarValue::F64(1.5),
            principal: root,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write cpc");

        let v = cube
            .read(&coord(cube_id, tampa, clicks), root)
            .expect("read ok");
        let got = v.value.as_f64().expect("F64");
        assert!(
            (got - 11_500.0 / 1.5).abs() < 1e-6,
            "Clicks should be Spend/CPC ≈ 7666.67, got {got}"
        );
    }

    #[test]
    fn write_invalidates_derived_cache() {
        let (mut cube, tampa, spend, clicks, root) = micro_cube();
        let cube_id = cube.id;
        let cpc = cube
            .measure_dimension()
            .element_by_name("CPC")
            .expect("CPC")
            .id;
        cube.write(WritebackRequest {
            coord: coord(cube_id, tampa, spend),
            new_value: ScalarValue::F64(11_500.0),
            principal: root,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write spend");
        cube.write(WritebackRequest {
            coord: coord(cube_id, tampa, cpc),
            new_value: ScalarValue::F64(1.5),
            principal: root,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write cpc");
        // Read once to cache.
        let _ = cube
            .read(&coord(cube_id, tampa, clicks), root)
            .expect("read");
        // Update Spend.
        cube.write(WritebackRequest {
            coord: coord(cube_id, tampa, spend),
            new_value: ScalarValue::F64(50_000.0),
            principal: root,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write spend 2");
        let v = cube
            .read(&coord(cube_id, tampa, clicks), root)
            .expect("read ok");
        let got = v.value.as_f64().expect("F64");
        assert!(
            (got - 50_000.0 / 1.5).abs() < 1e-6,
            "Clicks must reflect updated Spend, got {got}"
        );
    }

    #[test]
    fn snapshot_then_rollback_restores_state() {
        let (mut cube, tampa, spend, _clicks, root) = micro_cube();
        let cube_id = cube.id;
        cube.write(WritebackRequest {
            coord: coord(cube_id, tampa, spend),
            new_value: ScalarValue::F64(11_500.0),
            principal: root,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write");
        let snap = cube.snapshot(Some("approved"));
        cube.write(WritebackRequest {
            coord: coord(cube_id, tampa, spend),
            new_value: ScalarValue::F64(99_999.0),
            principal: root,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write 2");
        cube.rollback_to(&snap).expect("rollback ok");
        let v = cube
            .read(&coord(cube_id, tampa, spend), root)
            .expect("read");
        assert_eq!(v.value.as_f64(), Some(11_500.0));
    }

    #[test]
    fn snapshot_cube_id_mismatch_rejected() {
        let (mut cube, _tampa, _spend, _clicks, _root) = micro_cube();
        let other_snap = Snapshot {
            cube: CubeId(999),
            revision: Revision(0),
            captured_at: 0,
            label: None,
            store: HashMapStore::new(),
        };
        let err = cube.rollback_to(&other_snap).expect_err("cube id mismatch");
        assert!(matches!(err, EngineError::SnapshotCubeMismatch));
    }

    #[test]
    fn read_with_trace_returns_tree() {
        let (mut cube, tampa, spend, clicks, root) = micro_cube();
        let cube_id = cube.id;
        let cpc = cube
            .measure_dimension()
            .element_by_name("CPC")
            .expect("CPC")
            .id;
        cube.write(WritebackRequest {
            coord: coord(cube_id, tampa, spend),
            new_value: ScalarValue::F64(11_500.0),
            principal: root,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 1_700_000_000,
        })
        .expect("ok");
        cube.write(WritebackRequest {
            coord: coord(cube_id, tampa, cpc),
            new_value: ScalarValue::F64(1.5),
            principal: root,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 1_700_000_000,
        })
        .expect("ok");
        let v = cube
            .read_with_trace(&coord(cube_id, tampa, clicks), root)
            .expect("ok");
        let trace = v.trace.expect("trace requested");
        // Root op is RuleEvaluation, two children (SelfRef Spend, SelfRef CPC).
        assert!(matches!(
            trace.root.operation,
            TraceOp::RuleEvaluation { .. }
        ));
        assert_eq!(trace.root.children.len(), 2);
        // Both children should be Input lookups now.
        for child in &trace.root.children {
            assert!(matches!(child.operation, TraceOp::InputLookup { .. }));
        }
    }

    /// Phase 2B item 3 (handoff): two consecutive consolidated reads at
    /// the same revision must produce structurally identical results
    /// when the recompute path is exercised on both. The `request_trace`
    /// flag bypasses the consolidation cache (per the `if cached_fresh
    /// && !request_trace` guard above), so reading via
    /// `read_with_trace` twice forces `Consolidator::read` to walk the
    /// hierarchy twice through the new Arc fast path. Equality of
    /// value, dtype, provenance, and revision proves the Arc-borrowed
    /// dim/hierarchy snapshot is consumed identically across calls.
    #[test]
    fn consecutive_recompute_reads_match_phase_2b() {
        let (mut cube, tampa, spend, _clicks, root) = micro_cube();
        let cube_id = cube.id;
        let market_dim = cube.dimension_by_name("Market").expect("Market dim");
        let usa = market_dim.element_by_name("USA").expect("USA").id;
        cube.write(WritebackRequest {
            coord: coord(cube_id, tampa, spend),
            new_value: ScalarValue::F64(11_500.0),
            principal: root,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write tampa spend");

        let usa_spend = coord(cube_id, usa, spend);
        let v1 = cube
            .read_with_trace(&usa_spend, root)
            .expect("recompute read 1");
        let revision_after = cube.revision();
        let v2 = cube
            .read_with_trace(&usa_spend, root)
            .expect("recompute read 2");

        assert_eq!(
            v1.value.as_f64(),
            v2.value.as_f64(),
            "two recompute reads at the same revision must agree on value"
        );
        assert_eq!(v1.value.as_f64(), Some(11_500.0));
        assert!(matches!(v1.provenance, Provenance::Consolidation { .. }));
        assert!(matches!(v2.provenance, Provenance::Consolidation { .. }));
        assert_eq!(
            v1.revision, v2.revision,
            "neither recompute may bump revision"
        );
        assert_eq!(
            cube.revision(),
            revision_after,
            "reads do not bump revision"
        );
    }
}
