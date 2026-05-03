//! `mc-core` — the Mosaic engine kernel (the LNM kernel).
//!
//! Project renamed from "MarketingCubes V2" → "Mosaic" on 2026-05-03; the
//! `mc-` crate prefix stays as a backronym for "Mosaic Core". See the
//! repo `CLAUDE.md` for the binding naming-convention rule. Historical
//! docs (specs, ADRs, past completion reports) retain the original
//! "MarketingCubes" naming for audit-trail integrity.
//!
//! This crate is the Phase 1 deliverable. The semantics it implements are
//! defined in `docs/specs/engine-semantics.md`; the build contract is in
//! `docs/specs/phase-1-rust-kernel-build-brief.md`. Where the two differ,
//! the brief wins (per the user's Rule 1 in the kickoff prompt).
//!
//! ## Phase 1 module status
//!
//! Implemented in this deliverable:
//!   - `id`         — newtype IDs + `IdGenerator`
//!   - `revision`   — re-export of `Revision`
//!   - `value`      — `ScalarValue`, `CellDataType`, NaN/Inf rejection
//!   - `error`      — `EngineError` (full enum, all variants)
//!   - `element`    — `Element`, `MeasureMeta`, `MeasureRole`, `AggregationRule`,
//!                    `VersionState`, `ScenarioMeta`
//!   - `dimension`  — `Dimension`, `DimensionKind`, `DimensionBuilder`
//!   - `hierarchy`  — `Hierarchy`, `HierarchyBuilder` (cycle detection,
//!                    duplicate-edge / multi-parent / NaN-weight rejection)
//!   - `coordinate` — `CellCoordinate`, `CellCoordinateBuilder`
//!   - `cell`       — `CellValue`, `Provenance`, `Uncertainty`, `StoredCell`
//!   - `trace`      — types only (no walk algorithm yet)
//!   - `store`      — `HashMapStore` (concrete; no trait)
//!
//! Deferred to later deliverables (out of scope here):
//!   - `rule`, `dependency`, `dirty`, `consolidation`, `cube`, `slice`,
//!     `permission`, `lock`, `snapshot`

#![deny(rust_2018_idioms)]
#![warn(missing_debug_implementations)]
// Phase 1 forbids `unwrap` in library code per the build brief §12 #10.
// Tests, benches, and fixtures may use `expect("static reason")`.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod cell;
pub mod consolidation;
pub mod coordinate;
pub mod cube;
mod cube_shape;
pub mod dependency;
pub mod dimension;
pub mod dirty;
pub mod element;
pub mod error;
pub mod hierarchy;
pub mod id;
pub mod lock;
pub mod permission;
pub mod revision;
pub mod rule;
pub mod slice;
pub mod snapshot;
pub mod store;
pub mod trace;
pub mod value;

// Selective top-level re-exports for the most-used types. Callers can also
// use the module-qualified path; both forms work.
pub use cell::{CellValue, Provenance, StoredCell, Uncertainty};
pub use consolidation::{Consolidator, LeafReadout};
pub use coordinate::{CellCoordinate, CellCoordinateBuilder};
pub use cube::{Cube, CubeBuilder, WriteIntent, WritebackRequest, WritebackResult};
pub use dependency::{DependencyEdge, DependencyGraph, DependencySource};
pub use dimension::{Dimension, DimensionBuilder, DimensionKind};
pub use dirty::DirtyTracker;
pub use element::{AggregationRule, Element, MeasureMeta, MeasureRole, ScenarioMeta, VersionState};
pub use error::EngineError;
pub use hierarchy::{Hierarchy, HierarchyBuilder, HierarchyEdge};
pub use id::{
    CubeId, DimensionId, ElementId, HierarchyId, IdGenerator, LockId, PrincipalId, Revision, RuleId,
};
pub use lock::{ConflictKind, Lock, LockKind, LockTable, ReleaseError};
pub use permission::{
    capability, CapabilitySet, Grant, PermissionTable, ScopeBinding, ScopePattern,
};
pub use rule::{eval_expr, expr_depth, CoordPattern, DependencyDecl, Expr, Rule, RuleSet, Scope};
pub use slice::{SliceBinding, SliceQuery, SliceResult, PHASE_1_SLICE_LIMIT};
pub use snapshot::Snapshot;
pub use store::HashMapStore;
pub use trace::{ExprOp, ExprSummary, Trace, TraceNode, TraceOp};
pub use value::{validate_finite_f64, CellDataType, ScalarValue};
