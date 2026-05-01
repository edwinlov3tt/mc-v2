//! Cell value, provenance, uncertainty.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.8 (with cleanup-pass v1.1
//! changes: `Provenance::Consolidation` carries a `SmallVec<[HierarchyId; 4]>`).

use crate::id::{HierarchyId, PrincipalId, RuleId};
use crate::revision::Revision;
use crate::trace::Trace;
use crate::value::{CellDataType, ScalarValue};

/// What a read returns for a single cell. Carries value, dtype, provenance,
/// and optional uncertainty + trace.
///
/// `uncertainty` is **optional**, not universal. Deterministic finance cells
/// stay clean; only model-backed cells (Phase 4+) populate it. Per
/// engine-semantics §7.
#[derive(Clone, Debug)]
pub struct CellValue {
    pub value: ScalarValue,
    pub dtype: CellDataType,
    pub provenance: Provenance,
    pub uncertainty: Option<Uncertainty>,
    pub trace: Option<Trace>,
    pub revision: Revision,
}

#[derive(Clone, Debug)]
pub enum Provenance {
    Input {
        /// Unix-seconds timestamp (per spec §3.8 "u64 Unix-seconds").
        written_at: u64,
        written_by: PrincipalId,
    },
    Rule {
        rule_id: RuleId,
        computed_at: Revision,
    },
    /// A single consolidated cell may aggregate across MULTIPLE hierarchies
    /// simultaneously (e.g., Q1 × Paid_Media × Florida walks the Time,
    /// Channel, and Market hierarchies at once).
    Consolidation {
        hierarchies: smallvec::SmallVec<[HierarchyId; 4]>,
        child_count: u32,
    },
    Default {
        reason: &'static str,
    },
}

#[derive(Clone, Debug)]
pub enum Uncertainty {
    StdDev(f64),
    Interval {
        low: f64,
        high: f64,
        confidence: f64,
    },
}

/// What `HashMapStore` actually persists per coordinate. Lighter than
/// `CellValue` because the dtype and trace are reconstructed at read time.
#[derive(Clone, Debug)]
pub struct StoredCell {
    pub value: ScalarValue,
    pub provenance: Provenance,
    pub uncertainty: Option<Uncertainty>,
    pub revision: Revision,
}
