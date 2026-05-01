//! Multi-cell reads — `SliceQuery` enumerates a region of the cube;
//! `Cube::slice` (in cube.rs) executes the enumeration and returns a
//! `SliceResult` with one `CellValue` per coordinate.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.19 and engine-semantics.md
//! §12.

use ahash::AHashMap;

use crate::cell::CellValue;
use crate::coordinate::CellCoordinate;
use crate::id::{CubeId, DimensionId, ElementId, Revision};

#[derive(Clone, Debug)]
pub struct SliceQuery {
    pub cube: CubeId,
    /// One binding per dimension — same shape as `ScopePattern`. Phase
    /// 1's brief calls every dim out by name (no "wildcard absent
    /// dim"); enforce that at execution time and return
    /// `EngineError::Internal` if a dim is missing.
    pub bindings: AHashMap<DimensionId, SliceBinding>,
    /// Whether to compute traces per cell. Expensive; default false.
    pub request_trace: bool,
}

#[derive(Clone, Debug)]
pub enum SliceBinding {
    One(ElementId),
    Many(Vec<ElementId>),
    /// Every leaf descendant of `root` in the dimension's default
    /// hierarchy, plus `root` itself if it is a leaf. (For a flat
    /// hierarchy `Subtree(x)` is equivalent to `One(x)`.)
    Subtree(ElementId),
    /// Every leaf in the dim's default hierarchy. For dims with no
    /// hierarchy (synthesized flat) this is every element.
    All,
    /// Every consolidated (non-leaf) element in the dim's default
    /// hierarchy. Empty for flat hierarchies.
    AllConsolidated,
}

#[derive(Clone, Debug)]
pub struct SliceResult {
    pub coords: Vec<CellCoordinate>,
    pub values: Vec<CellValue>,
    /// Cube revision at slice start. Per spec §12 I-Slice-6: every cell
    /// in the slice was read against this revision.
    pub revision: Revision,
}

/// Phase 1 hard limit per spec §12 I-Slice-2 / brief §3.19. Larger
/// slices return `EngineError::SliceTooLarge`.
pub const PHASE_1_SLICE_LIMIT: usize = 1_048_576;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_binding_clones_cleanly() {
        let b = SliceBinding::Many(vec![ElementId(1), ElementId(2)]);
        let _b2 = b.clone();
    }

    #[test]
    fn slice_query_has_request_trace_default_off_intent() {
        // Construction is purely declarative; no smarts here. This test
        // pins that the field is `bool` (not `Option<bool>` or some
        // tri-state) so callers always make an intentional choice.
        let q = SliceQuery {
            cube: CubeId(1),
            bindings: AHashMap::new(),
            request_trace: false,
        };
        assert!(!q.request_trace);
    }
}
