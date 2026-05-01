//! Engine-wide error type.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.20, with `LockedVersion` added
//! per the v1.1 cleanup pass.
//!
//! Every variant listed in the brief is included up front so downstream
//! modules can be added without touching this file. Variants that are only
//! produced by modules outside this deliverable (rule, dependency, lock,
//! permission, slice, snapshot, cube) are present but unproduced today —
//! that is intentional and is not dead code: tests or future modules will
//! produce them and the enum stays stable across deliverables.

use crate::coordinate::CellCoordinate;
use crate::element::{MeasureRole, VersionState};
use crate::id::{CubeId, DimensionId, ElementId, PrincipalId, Revision, RuleId};
use crate::value::{CellDataType, ScalarValue};

#[derive(thiserror::Error, Debug)]
pub enum EngineError {
    // ---- Dimension / element / hierarchy ----
    #[error("dimension '{name}' not found")]
    DimensionNotFound { name: String },

    #[error("element id {0:?} not found in dimension {1:?}")]
    ElementNotFound(ElementId, DimensionId),

    #[error("dimension already frozen")]
    DimensionFrozen,

    #[error("hierarchy cycle: {path:?}")]
    HierarchyCycle { path: Vec<ElementId> },

    #[error("invalid hierarchy weight: {0} (NaN, Inf, or otherwise non-finite)")]
    InvalidWeight(f64),

    #[error(
        "element {element:?} has multiple parents in single-parent hierarchy: \
         existing parent {existing:?}, attempted parent {attempted:?}"
    )]
    MultipleParents {
        element: ElementId,
        existing: ElementId,
        attempted: ElementId,
    },

    #[error("duplicate hierarchy edge: parent {parent:?} → child {child:?}")]
    DuplicateHierarchyEdge { parent: ElementId, child: ElementId },

    #[error("duplicate element name '{name}' in dimension {dim:?}")]
    DuplicateElementName { name: String, dim: DimensionId },

    #[error("duplicate element id {id:?} in dimension {dim:?}")]
    DuplicateElementId { id: ElementId, dim: DimensionId },

    #[error("element id {id:?} on a hierarchy edge is not a member of dimension {dim:?}")]
    HierarchyEdgeReferencesUnknownElement { id: ElementId, dim: DimensionId },

    #[error("dimension {dim:?} has no default hierarchy")]
    NoDefaultHierarchy { dim: DimensionId },

    #[error("default hierarchy '{name}' not found among the dimension's hierarchies")]
    DefaultHierarchyNotFound { name: String },

    #[error("dimension '{name}' has no elements")]
    DimensionEmpty { name: String },

    // ---- Coordinate ----
    #[error("coordinate slot for dimension {0:?} is unset")]
    CoordinateMissingDimension(DimensionId),

    #[error("coordinate dimension {dim:?} is not part of this cube")]
    CoordinateDimensionNotInCube { dim: DimensionId },

    #[error("element {element:?} is not a member of dimension {dim:?}; cannot bind to coordinate")]
    CoordinateElementNotInDimension {
        element: ElementId,
        dim: DimensionId,
    },

    #[error(
        "coordinate references cube {expected:?} but coordinate-builder was constructed for {actual:?}"
    )]
    CoordinateCubeMismatch { expected: CubeId, actual: CubeId },

    // ---- Rule / dependency (variants stable; produced when those modules land) ----
    #[error("dependency cycle detected: {path:?}")]
    DependencyCycle { path: Vec<CellCoordinate> },

    #[error("undeclared dependency: rule {rule:?} read {coord:?} but did not declare it")]
    UndeclaredDependency { rule: RuleId, coord: CellCoordinate },

    #[error("rule's target measure must be Derived; got {role:?}")]
    RuleTargetNotDerived { role: MeasureRole },

    #[error("rule body is not well-typed: {detail}")]
    RuleBodyTypeMismatch { detail: String },

    #[error("two rules target the same measure: {0:?}")]
    DuplicateRuleTarget(ElementId),

    // ---- Slice ----
    #[error("slice exceeds size limit: {actual} > {max}")]
    SliceTooLarge { actual: usize, max: usize },

    // ---- Permission / locks / version ----
    #[error("insufficient permission for principal {principal:?} on coord {coord:?}")]
    InsufficientPermission {
        principal: PrincipalId,
        coord: CellCoordinate,
    },

    #[error("locked: cell {coord:?} held by principal {owner:?}")]
    LockedCell {
        coord: CellCoordinate,
        owner: PrincipalId,
    },

    /// A write was attempted to a coordinate whose Version-dimension element
    /// is in `Approved` or `Archived` state. Per spec §9 I-Ver-3.
    #[error("locked version: write to {state:?} version {version:?} rejected")]
    LockedVersion {
        version: ElementId,
        state: VersionState,
    },

    // ---- Writeback ----
    #[error("write rejected (derived cell): {coord:?}")]
    DerivedCellNotWritable { coord: CellCoordinate },

    #[error("write rejected (consolidated cell): {coord:?}")]
    ConsolidatedCellNotWritable { coord: CellCoordinate },

    #[error("stale revision: expected {expected:?}, current {current:?}")]
    StaleRevision {
        expected: Revision,
        current: Revision,
    },

    #[error("type mismatch: expected {expected:?}, got value {got:?}")]
    TypeMismatch {
        expected: CellDataType,
        got: ScalarValue,
    },

    #[error("invalid value: {0}")]
    InvalidValue(&'static str),

    // ---- Snapshot ----
    #[error("snapshot cube mismatch: snapshot belongs to a different cube")]
    SnapshotCubeMismatch,

    // ---- Internal ----
    #[error("internal invariant violated: {0}")]
    Internal(&'static str),
}
