//! Elements and their kind-specific metadata.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.4 (with the cleanup-pass
//! additions for `VersionState` and `ScenarioMeta` as per-kind metadata
//! fields).

use crate::id::{DimensionId, ElementId};
use crate::value::CellDataType;

/// A single named member of a dimension. Either a leaf (no children in any
/// hierarchy) or consolidated (has children in at least one hierarchy).
///
/// The kind-specific metadata (`measure_meta`, `version_state`,
/// `scenario_meta`) is populated only when the parent dimension's `kind`
/// matches: at most one of these is `Some` for any given element.
#[derive(Clone, Debug)]
pub struct Element {
    pub id: ElementId,
    pub name: String,
    pub dimension: DimensionId,
    /// Populated only when the parent dimension is `DimensionKind::Measure`.
    pub measure_meta: Option<MeasureMeta>,
    /// Populated only when the parent dimension is `DimensionKind::Version`.
    pub version_state: Option<VersionState>,
    /// Populated only when the parent dimension is `DimensionKind::Scenario`.
    pub scenario_meta: Option<ScenarioMeta>,
}

#[derive(Clone, Debug)]
pub struct MeasureMeta {
    pub dtype: CellDataType,
    pub role: MeasureRole,
    pub aggregation: AggregationRule,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum MeasureRole {
    Input,
    Derived,
    /// Phase 3J item 4: indicator measure. A `MeasureRole::Indicator`
    /// declares `dimension:` and `element:` fields (no `body:`, no
    /// `inputs:`); it is the declarative form of the
    /// `is_element(Dim, "Element")` formula function and compiles to
    /// the same `Expr::IsElement(DimensionId, ElementId)` AST per
    /// ADR-0016 Decision 7 + Amendment §6.
    ///
    /// At eval time, an Indicator measure behaves as a Derived measure
    /// with an implicit synthesized rule body. The cube treats it as
    /// non-writable (writeback rejects with `DerivedCellNotWritable`,
    /// same as a regular Derived measure).
    Indicator,
}

#[derive(Clone, Debug)]
pub enum AggregationRule {
    Sum,
    /// Weighted average: numerator = Σ(value × weight), denominator = Σ(weight).
    /// `weight_measure` references another measure in the same Measure
    /// dimension.
    WeightedAverage {
        weight_measure: ElementId,
    },
    Min,
    Max,
}

/// Carried only by elements of a `DimensionKind::Version` dimension.
/// Drives writeback gating: `Approved` and `Archived` versions are read-only.
/// Per spec §9.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VersionState {
    Draft,
    Submitted,
    Approved,
    Archived,
}

/// Carried only by elements of a `DimensionKind::Scenario` dimension. Phase 1
/// has no scenario inheritance; the field is reserved for future use and
/// distinguishes the default scenario (per spec §8 I-Scen-2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScenarioMeta {
    Default,
    NonDefault,
}

impl Element {
    /// Construct a leaf element of a non-measure, non-version, non-scenario
    /// dimension (e.g., Time, Channel, Market).
    pub fn leaf(id: ElementId, name: impl Into<String>, dim: DimensionId) -> Self {
        Element {
            id,
            name: name.into(),
            dimension: dim,
            measure_meta: None,
            version_state: None,
            scenario_meta: None,
        }
    }

    /// Construct a measure element (parent dim must be `DimensionKind::Measure`).
    pub fn measure(
        id: ElementId,
        name: impl Into<String>,
        dim: DimensionId,
        dtype: CellDataType,
        role: MeasureRole,
        agg: AggregationRule,
    ) -> Self {
        Element {
            id,
            name: name.into(),
            dimension: dim,
            measure_meta: Some(MeasureMeta {
                dtype,
                role,
                aggregation: agg,
            }),
            version_state: None,
            scenario_meta: None,
        }
    }

    /// Construct a version element (parent dim must be `DimensionKind::Version`).
    pub fn version(
        id: ElementId,
        name: impl Into<String>,
        dim: DimensionId,
        state: VersionState,
    ) -> Self {
        Element {
            id,
            name: name.into(),
            dimension: dim,
            measure_meta: None,
            version_state: Some(state),
            scenario_meta: None,
        }
    }

    /// Construct a scenario element (parent dim must be
    /// `DimensionKind::Scenario`).
    pub fn scenario(
        id: ElementId,
        name: impl Into<String>,
        dim: DimensionId,
        meta: ScenarioMeta,
    ) -> Self {
        Element {
            id,
            name: name.into(),
            dimension: dim,
            measure_meta: None,
            version_state: None,
            scenario_meta: Some(meta),
        }
    }

    /// Returns Some(state) only for elements in a Version dimension.
    pub fn version_state(&self) -> Option<VersionState> {
        self.version_state
    }

    /// Returns Some(meta) only for elements in a Measure dimension.
    pub fn measure_meta(&self) -> Option<&MeasureMeta> {
        self.measure_meta.as_ref()
    }

    /// Returns Some(meta) only for elements in a Scenario dimension.
    pub fn scenario_meta(&self) -> Option<ScenarioMeta> {
        self.scenario_meta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaf_constructor_leaves_kind_meta_none() {
        let dim = DimensionId(7);
        let id = ElementId(42);
        let e = Element::leaf(id, "Tampa", dim);
        assert_eq!(e.id, id);
        assert_eq!(e.dimension, dim);
        assert_eq!(e.name, "Tampa");
        assert!(e.measure_meta.is_none());
        assert!(e.version_state.is_none());
        assert!(e.scenario_meta.is_none());
    }

    #[test]
    fn measure_constructor_populates_only_measure_meta() {
        let dim = DimensionId(1);
        let id = ElementId(100);
        let e = Element::measure(
            id,
            "Spend",
            dim,
            CellDataType::F64,
            MeasureRole::Input,
            AggregationRule::Sum,
        );
        assert!(e.measure_meta.is_some());
        assert!(e.version_state.is_none());
        assert!(e.scenario_meta.is_none());
        let m = e
            .measure_meta
            .as_ref()
            .expect("measure constructor populates measure_meta");
        assert_eq!(m.role, MeasureRole::Input);
        assert!(matches!(m.dtype, CellDataType::F64));
    }

    #[test]
    fn version_constructor_populates_only_version_state() {
        let dim = DimensionId(2);
        let id = ElementId(200);
        let e = Element::version(id, "Approved", dim, VersionState::Approved);
        assert!(e.measure_meta.is_none());
        assert_eq!(e.version_state, Some(VersionState::Approved));
        assert!(e.scenario_meta.is_none());
    }

    #[test]
    fn scenario_constructor_populates_only_scenario_meta() {
        let dim = DimensionId(3);
        let id = ElementId(300);
        let e = Element::scenario(id, "Baseline", dim, ScenarioMeta::Default);
        assert!(e.measure_meta.is_none());
        assert!(e.version_state.is_none());
        assert_eq!(e.scenario_meta, Some(ScenarioMeta::Default));
    }
}
