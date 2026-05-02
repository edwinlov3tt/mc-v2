//! Cartesian-product coordinate shape — precomputed once at
//! `CubeBuilder::build` and shared (behind `Arc`) with the cube's
//! `DirtyTracker` so per-mark cost collapses from an `AHashSet` hash +
//! probe + occasional rehash to a single bit-test in a flat
//! `Vec<u64>`.
//!
//! Per Phase 2D handoff (`docs/handoffs/phase-2d-handoff.md` §B). The
//! load-bearing finding is PERF.md §6.14: `load_canonical_inputs` per-write
//! cost grows from 4.33× at 10× to 19.7× at 50× because every dirty mark
//! pays a hash-and-insert into a set that's actively rehashing as it
//! saturates. Linearizing each `CellCoordinate` to a `usize` index over a
//! Cartesian product of per-dim element-index maps + per-dim strides
//! turns mark/check into O(1) bit math, independent of set size.
//!
//! `CubeShape` is *internal* to `mc-core` (the module is private at the
//! crate root). It is not part of `mc-core`'s public surface and never
//! appears in the lib.rs re-exports — `with_shape` accepts it via
//! `pub(crate) fn` so `cube.rs` can construct the bitset-backed tracker
//! while no external caller can synthesize one.

use std::sync::Arc;

use crate::coordinate::CellCoordinate;
use crate::dimension::Dimension;

/// Maximum Cartesian-product cardinality the bitset path will commit
/// memory for. ~1 G coords ≈ 128 MB of dirty bits — the upper bound past
/// which a flat representation stops being the right shape (see Phase 2D
/// handoff §B "Cardinality-explosion guard"). Phase 2D's calibration
/// scales (≤ 100×) sit at ≤ 9.5 M coords, four orders of magnitude under
/// this guard. If a future cube exceeds this, `CubeShape::new` returns
/// `None` and the cube falls back to the legacy AHashSet-backed tracker.
const CARDINALITY_GUARD: usize = 1usize << 30;

/// Soft upper bound on the per-dim element-id range that `CubeShape`
/// will allocate a flat lookup vec for. Acme at 100× has max
/// element-id ≤ ~1 K; this guard catches a pathological future cube
/// (e.g., sparse non-monotonic ids) before it allocates a multi-MB
/// per-dim vec. If a dim trips this, `CubeShape::new` falls back to
/// `None` and the tracker uses the AHashSet path.
const MAX_PER_DIM_ID_RANGE: usize = 1usize << 24;

/// Sentinel for "no element with this id in this dim." Reserved from
/// the `u32` element-position space; a real per-dim element count would
/// have to exceed `u32::MAX - 1` (impossible under the cardinality
/// guard) to collide with this.
const ABSENT_ELEMENT: u32 = u32::MAX;

#[derive(Debug)]
pub(crate) struct CubeShape {
    /// Per-dim element-id → local-index lookup. Indexed by
    /// `ElementId.0 as usize`; entries with no element are
    /// `ABSENT_ELEMENT`. Sized per dim to `(max_id_in_dim + 1)`.
    /// Direct array indexing is ~5 ns vs ~30 ns for the AHashMap
    /// path the original Phase 2D draft used — the difference
    /// dominates per-mark cost at the scales Phase 2D's bitset is
    /// trying to flatten.
    element_index_in_dim: Vec<Vec<u32>>,
    /// Per-dim stride for the row-major linearization. `stride[i] =
    /// product of |dim_{i+1}| × |dim_{i+2}| × ... × |dim_{D-1}|`.
    /// `stride[D-1] = 1`.
    stride: Vec<usize>,
    /// Total Cartesian-product cardinality. The bitset needs
    /// `(cardinality + 63) / 64` words.
    cardinality: usize,
}

impl CubeShape {
    /// Compute the shape from the cube's dimensions. Returns `None` when
    /// the Cartesian-product cardinality overflows `usize` or exceeds
    /// `CARDINALITY_GUARD`. Caller (typically `CubeBuilder::build`)
    /// falls back to the no-shape tracker in that case.
    pub(crate) fn new(dimensions: &[Dimension]) -> Option<Arc<Self>> {
        // Per-dim sizes (every element in `dim.elements`, leaves +
        // consolidations, since dirty marks can target any element via
        // `mark_closure` / `compute_dirty_ancestors`). Per Phase 2D
        // handoff §B "Indexing domain".
        let dim_sizes: Vec<usize> = dimensions.iter().map(|d| d.elements.len()).collect();
        let mut cardinality: usize = 1;
        for &s in &dim_sizes {
            cardinality = cardinality.checked_mul(s)?;
        }
        if cardinality > CARDINALITY_GUARD {
            return None;
        }

        // Row-major strides: stride[D-1] = 1; stride[i] = stride[i+1] *
        // dim_sizes[i+1]. checked_mul guards against the (unreachable
        // given the cardinality guard) intermediate overflow.
        let dim_count = dim_sizes.len();
        let mut stride = vec![1usize; dim_count.max(1)];
        if dim_count >= 2 {
            for i in (0..dim_count - 1).rev() {
                stride[i] = stride[i + 1].checked_mul(dim_sizes[i + 1])?;
            }
        }
        stride.truncate(dim_count);

        let mut element_index_in_dim: Vec<Vec<u32>> = Vec::with_capacity(dim_count);
        for dim in dimensions {
            let max_id = dim.elements.iter().map(|e| e.id.0).max().unwrap_or(0);
            let max_id_usize = usize::try_from(max_id).ok()?;
            let size = max_id_usize.checked_add(1)?;
            if size > MAX_PER_DIM_ID_RANGE {
                return None;
            }
            let mut bucket = vec![ABSENT_ELEMENT; size];
            for (pos, element) in dim.elements.iter().enumerate() {
                let id = usize::try_from(element.id.0).ok()?;
                // pos < dim.elements.len() ≤ cardinality ≤ 1<<30, so
                // pos always fits in u32.
                bucket[id] = pos as u32;
            }
            element_index_in_dim.push(bucket);
        }

        Some(Arc::new(Self {
            element_index_in_dim,
            stride,
            cardinality,
        }))
    }

    pub(crate) fn cardinality(&self) -> usize {
        self.cardinality
    }

    /// Linearize a coord to a flat bit index. Returns `None` if the
    /// coord's arity does not match the shape's dim count or if any
    /// element id is not a member of its dim. Cube-internal callers
    /// always pass validated coords; the `Option` is the safety net for
    /// mis-shaped coords (which would otherwise panic).
    #[inline]
    pub(crate) fn linearize(&self, coord: &CellCoordinate) -> Option<usize> {
        let elements = coord.elements();
        let buckets = &self.element_index_in_dim;
        let strides = &self.stride;
        if elements.len() != buckets.len() {
            return None;
        }
        let mut idx: usize = 0;
        for dim in 0..elements.len() {
            let bucket = &buckets[dim];
            let id = elements[dim].0 as usize;
            if id >= bucket.len() {
                return None;
            }
            let local = bucket[id];
            if local == ABSENT_ELEMENT {
                return None;
            }
            // local × stride is bounded by the precomputed cardinality
            // (which fits in usize), so we can use unchecked arithmetic
            // here. The cardinality guard at construction is the only
            // overflow check needed.
            idx += (local as usize) * strides[dim];
        }
        Some(idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dimension::DimensionKind;
    use crate::element::Element;
    use crate::id::{CubeId, DimensionId, ElementId};

    fn three_by_two() -> Vec<Dimension> {
        let dim_a = DimensionId(1);
        let dim_b = DimensionId(2);
        let a0 = ElementId(10);
        let a1 = ElementId(11);
        let a2 = ElementId(12);
        let b0 = ElementId(20);
        let b1 = ElementId(21);
        let dim_a_built = Dimension::builder(dim_a, "A", DimensionKind::Standard)
            .add_element(Element::leaf(a0, "a0", dim_a))
            .expect("a0")
            .add_element(Element::leaf(a1, "a1", dim_a))
            .expect("a1")
            .add_element(Element::leaf(a2, "a2", dim_a))
            .expect("a2")
            .build()
            .expect("dim a");
        let dim_b_built = Dimension::builder(dim_b, "B", DimensionKind::Standard)
            .add_element(Element::leaf(b0, "b0", dim_b))
            .expect("b0")
            .add_element(Element::leaf(b1, "b1", dim_b))
            .expect("b1")
            .build()
            .expect("dim b");
        vec![dim_a_built, dim_b_built]
    }

    #[test]
    fn cardinality_is_product_of_dim_sizes() {
        let dims = three_by_two();
        let shape = CubeShape::new(&dims).expect("shape");
        assert_eq!(shape.cardinality(), 6);
    }

    #[test]
    fn linearize_round_trip_for_every_cartesian_coord() {
        let dims = three_by_two();
        let shape = CubeShape::new(&dims).expect("shape");
        let cube = CubeId(99);
        let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for a in &dims[0].elements {
            for b in &dims[1].elements {
                let coord = CellCoordinate::from_parts(cube, [a.id, b.id]);
                let idx = shape.linearize(&coord).expect("linearizable");
                assert!(idx < shape.cardinality());
                assert!(seen.insert(idx), "duplicate index {idx}");
            }
        }
        assert_eq!(seen.len(), shape.cardinality());
    }

    #[test]
    fn linearize_returns_none_for_arity_mismatch() {
        let dims = three_by_two();
        let shape = CubeShape::new(&dims).expect("shape");
        let cube = CubeId(99);
        let coord = CellCoordinate::from_parts(cube, [ElementId(10)]);
        assert!(shape.linearize(&coord).is_none());
    }

    #[test]
    fn linearize_returns_none_for_unknown_element() {
        let dims = three_by_two();
        let shape = CubeShape::new(&dims).expect("shape");
        let cube = CubeId(99);
        let coord = CellCoordinate::from_parts(cube, [ElementId(999), ElementId(20)]);
        assert!(shape.linearize(&coord).is_none());
    }
}
