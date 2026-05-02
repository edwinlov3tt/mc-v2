//! Dirty tracking: cells whose cached value is no longer valid.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.13.
//!
//! Per spec §16 the dirty state is the engine's mechanism for lazy
//! evaluation: writes are O(1) (mark dirty, don't recompute); reads are
//! O(closure-size) on first read after dirtying, then O(1) cached.
//! `DirtyTracker` is not the cache itself — it's the flag set telling
//! `cube.rs` which cells need recomputation.
//!
//! # Phase 2D — bitset-backed fast path
//!
//! Per Phase 2D handoff (`docs/handoffs/phase-2d-handoff.md`) and
//! PERF.md §6.14 / §9.3, the per-mark hash-and-insert cost on the
//! original `AHashSet<CellCoordinate>` representation grows
//! super-linearly as the dirty set saturates: bulk ingest at 50× cells
//! takes 230 s (23× over the ADR-0003 patience-limit gate) because
//! AHashSet rehash + cache locality + collision probability all bite
//! together as the table grows from 0 to ~305 K entries.
//!
//! Phase 2D replaces the internal representation with a Cartesian-product
//! flat bitset keyed by linearized coordinate index (per-dim element-index
//! maps + per-dim strides; see [`crate::cube_shape::CubeShape`]). Per-mark
//! and per-check cost collapses to O(1) bit math, independent of set
//! size.
//!
//! The public API surface is preserved byte-for-byte; the only additive
//! surface is the `pub(crate)` `with_shape` constructor, called from
//! `CubeBuilder::build`. `new()` continues to construct an
//! AHashSet-backed tracker (no shape needed) so unit tests and any
//! caller without a cube context still compile.

use std::sync::Arc;

use ahash::AHashSet;

use crate::coordinate::CellCoordinate;
use crate::cube_shape::CubeShape;
use crate::dependency::DependencyGraph;

/// One entry in the `tracked` materialization Vec. Carrying the bit
/// `idx` alongside the coord lets `iter()` filter by the current bit
/// state without re-running `CubeShape::linearize` per yielded coord
/// (the original Phase 2D draft did re-linearize and the per-iter cost
/// dominated `WritebackResult.invalidated` construction at scale —
/// fixed before the acceptance bench was first read).
#[derive(Debug)]
struct TrackedEntry {
    idx: usize,
    coord: CellCoordinate,
}

/// Internal storage for `DirtyTracker`. The `Hash` arm is the legacy
/// AHashSet representation, retained as the fallback for `new()` and the
/// (extremely unlikely, see `CubeShape::new`'s cardinality guard) cube
/// whose Cartesian product overflows the bitset budget. The `Bitset` arm
/// is the Phase 2D fast path.
#[derive(Debug)]
enum DirtyImpl {
    Hash(AHashSet<CellCoordinate>),
    Bitset {
        shape: Arc<CubeShape>,
        /// Current dirty bits — one per Cartesian-product coord.
        bits: Vec<u64>,
        /// Sticky "ever marked since last `clear_all`" bits. Used to
        /// dedupe the `tracked` Vec across mark → clear → re-mark
        /// cycles. A coord is pushed onto `tracked` exactly once per
        /// `clear_all` lifetime, which keeps `iter()` from yielding the
        /// same coord twice (matching the AHashSet semantics that
        /// downstream callers — `cube.rs::write` building
        /// `WritebackResult.invalidated`, the `§10.1` dirty-set bound
        /// test — depend on).
        ever_marked: Vec<u64>,
        /// Materialized coords in first-mark order, each tagged with
        /// its precomputed bit index. `iter()` walks this and filters
        /// by the current `bits` state via a direct bit-test on `idx`,
        /// no per-yield linearize.
        tracked: Vec<TrackedEntry>,
        /// Live count — `bits` popcount, maintained incrementally so
        /// `len()` is O(1).
        len: usize,
    },
}

impl Default for DirtyImpl {
    fn default() -> Self {
        Self::Hash(AHashSet::new())
    }
}

#[derive(Debug, Default)]
pub struct DirtyTracker {
    inner: DirtyImpl,
}

impl DirtyTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a bitset-backed tracker. Called from
    /// `CubeBuilder::build` once the cube's `CubeShape` has been
    /// computed; per Phase 2D handoff this is the production path. The
    /// `new()` constructor stays available for callers without a cube
    /// context.
    pub(crate) fn with_shape(shape: Arc<CubeShape>) -> Self {
        let words = shape.cardinality().div_ceil(64);
        Self {
            inner: DirtyImpl::Bitset {
                shape,
                bits: vec![0u64; words],
                ever_marked: vec![0u64; words],
                tracked: Vec::new(),
                len: 0,
            },
        }
    }

    /// Mark a single cell dirty. Idempotent — marking an already-dirty
    /// cell is a no-op (per spec §16 I-Dirty-6).
    pub fn mark(&mut self, coord: CellCoordinate) {
        match &mut self.inner {
            DirtyImpl::Hash(set) => {
                set.insert(coord);
            }
            DirtyImpl::Bitset {
                shape,
                bits,
                ever_marked,
                tracked,
                len,
            } => {
                let Some(idx) = shape.linearize(&coord) else {
                    // Cube-internal callers always pass coords that
                    // linearize cleanly; a None here would mean the
                    // tracker was handed a coord from a different cube
                    // (or with mismatched arity), which is an upstream
                    // bug. Silently skip: the §10.1 invariant tests
                    // would surface any divergence.
                    debug_assert!(
                        false,
                        "DirtyTracker::mark: coord {coord:?} did not linearize against shape"
                    );
                    return;
                };
                let word = idx / 64;
                let bit = 1u64 << (idx % 64);
                if bits[word] & bit == 0 {
                    bits[word] |= bit;
                    *len += 1;
                }
                if ever_marked[word] & bit == 0 {
                    ever_marked[word] |= bit;
                    tracked.push(TrackedEntry { idx, coord });
                }
            }
        }
    }

    /// Mark every cell with a transitive dependency on `root` dirty.
    /// Walks the reverse-edge closure in `graph`. **Does NOT include
    /// `root` itself** — `root` is the freshly-written cell; the
    /// values that need recompute are the ones that *read from* it.
    ///
    /// Per spec §16 I-Dirty-1: marking a cell dirty also marks every
    /// cell with a transitive dependency dirty. The "the root cell
    /// itself" interpretation only applies to invalidation flows
    /// (which Phase 1 doesn't use); for `Cube::write` the root has
    /// just received a new value and is by definition clean.
    pub fn mark_closure(&mut self, root: &CellCoordinate, graph: &DependencyGraph) {
        for c in graph.closure_of_dependents(root) {
            self.mark(c);
        }
    }

    /// True iff `coord` is currently dirty.
    pub fn is_dirty(&self, coord: &CellCoordinate) -> bool {
        match &self.inner {
            DirtyImpl::Hash(set) => set.contains(coord),
            DirtyImpl::Bitset { shape, bits, .. } => {
                let Some(idx) = shape.linearize(coord) else {
                    return false;
                };
                let word = idx / 64;
                let bit = 1u64 << (idx % 64);
                bits[word] & bit != 0
            }
        }
    }

    /// Remove `coord` from the dirty set after a successful recompute
    /// (per spec §16 I-Dirty-4).
    pub fn clear(&mut self, coord: &CellCoordinate) {
        match &mut self.inner {
            DirtyImpl::Hash(set) => {
                set.remove(coord);
            }
            DirtyImpl::Bitset {
                shape, bits, len, ..
            } => {
                let Some(idx) = shape.linearize(coord) else {
                    return;
                };
                let word = idx / 64;
                let bit = 1u64 << (idx % 64);
                if bits[word] & bit != 0 {
                    bits[word] &= !bit;
                    *len -= 1;
                }
                // ever_marked / tracked are intentionally not touched —
                // a re-mark of the same coord must not push a duplicate
                // into `tracked` (otherwise iter() would yield it twice
                // and break the §10.1 dirty-set bound test).
            }
        }
    }

    /// Drop every dirty marker. Used by snapshot rollback (per
    /// `cube.rs::rollback_to`) since rolled-back state is by definition
    /// fresh.
    pub fn clear_all(&mut self) {
        match &mut self.inner {
            DirtyImpl::Hash(set) => set.clear(),
            DirtyImpl::Bitset {
                bits,
                ever_marked,
                tracked,
                len,
                ..
            } => {
                for w in bits.iter_mut() {
                    *w = 0;
                }
                for w in ever_marked.iter_mut() {
                    *w = 0;
                }
                tracked.clear();
                *len = 0;
            }
        }
    }

    pub fn len(&self) -> usize {
        match &self.inner {
            DirtyImpl::Hash(set) => set.len(),
            DirtyImpl::Bitset { len, .. } => *len,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Iterator over the dirty set. Order is implementation-defined but
    /// **deterministic across runs**: the bitset path yields coords in
    /// first-mark order (Vec insertion order); the AHashSet fallback
    /// path retains the original nondeterministic-across-runs ordering
    /// noted in CLAUDE.md §2.11. Either way, callers that need a stable
    /// sequence collect-and-sort (per CLAUDE.md §2.11 / brief §15
    /// step 6).
    ///
    /// The returned iterator reports an exact `size_hint` so
    /// `.collect::<Vec<_>>()` (the dominant call site in
    /// `cube.rs::write` building `WritebackResult.invalidated`) can
    /// pre-allocate. Without this hint, `cube.rs`'s
    /// `iter().cloned().collect()` would re-grow its Vec from
    /// capacity 0 on every write — at scale, that doubling-realloc
    /// cost dominates per-write time and was the cause of a +88%
    /// regression at 10× ingest in an early Phase 2D draft.
    pub fn iter(&self) -> impl Iterator<Item = &CellCoordinate> + '_ {
        DirtyIter::new(&self.inner, self.len())
    }

    /// Snapshot the dirty set into a deterministic, sorted vec. Used by
    /// tests that assert membership and bounds without depending on
    /// AHashSet iteration order.
    pub fn snapshot_sorted(&self) -> Vec<CellCoordinate> {
        let mut v: Vec<CellCoordinate> = self.iter().cloned().collect();
        v.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
        v
    }
}

/// Sum-type iterator over the two storage representations. Kept private:
/// the public `iter()` returns `impl Iterator<Item = &CellCoordinate>`,
/// so the concrete type stays internal.
enum DirtyIter<'a> {
    Hash(std::collections::hash_set::Iter<'a, CellCoordinate>),
    Bitset {
        bits: &'a [u64],
        tracked: &'a [TrackedEntry],
        pos: usize,
        /// Live dirty count remaining to be yielded. Initialized from
        /// `DirtyTracker::len` and decremented on each yield, so
        /// `size_hint` is exact and `collect()` pre-allocates.
        remaining: usize,
    },
}

impl<'a> DirtyIter<'a> {
    fn new(inner: &'a DirtyImpl, len: usize) -> Self {
        match inner {
            DirtyImpl::Hash(set) => Self::Hash(set.iter()),
            DirtyImpl::Bitset { bits, tracked, .. } => Self::Bitset {
                bits,
                tracked,
                pos: 0,
                remaining: len,
            },
        }
    }
}

impl<'a> Iterator for DirtyIter<'a> {
    type Item = &'a CellCoordinate;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Hash(it) => it.next(),
            Self::Bitset {
                bits,
                tracked,
                pos,
                remaining,
            } => {
                while *pos < tracked.len() {
                    let entry = &tracked[*pos];
                    *pos += 1;
                    let word = entry.idx / 64;
                    let bit = 1u64 << (entry.idx % 64);
                    if bits[word] & bit != 0 {
                        *remaining = remaining.saturating_sub(1);
                        return Some(&entry.coord);
                    }
                }
                None
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Hash(it) => it.size_hint(),
            Self::Bitset { remaining, .. } => (*remaining, Some(*remaining)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependency::{DependencyEdge, DependencySource};
    use crate::dimension::{Dimension, DimensionKind};
    use crate::element::Element;
    use crate::id::{CubeId, DimensionId, ElementId, RuleId};

    fn coord(elements: &[u64]) -> CellCoordinate {
        CellCoordinate::from_parts(CubeId(1), elements.iter().map(|&e| ElementId(e)))
    }

    #[test]
    fn mark_and_is_dirty_basic() {
        let mut t = DirtyTracker::new();
        let a = coord(&[1]);
        assert!(!t.is_dirty(&a));
        t.mark(a.clone());
        assert!(t.is_dirty(&a));
    }

    #[test]
    fn mark_is_idempotent() {
        let mut t = DirtyTracker::new();
        let a = coord(&[1]);
        t.mark(a.clone());
        t.mark(a.clone());
        t.mark(a.clone());
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn clear_removes_from_dirty_set() {
        let mut t = DirtyTracker::new();
        let a = coord(&[1]);
        t.mark(a.clone());
        assert!(t.is_dirty(&a));
        t.clear(&a);
        assert!(!t.is_dirty(&a));
        // Clearing a non-dirty cell is a no-op (no panic, no error).
        t.clear(&a);
    }

    #[test]
    fn clear_all_drops_everything() {
        let mut t = DirtyTracker::new();
        for i in 0..10 {
            t.mark(coord(&[i]));
        }
        assert_eq!(t.len(), 10);
        t.clear_all();
        assert!(t.is_empty());
    }

    #[test]
    fn mark_closure_walks_dependents_excluding_root() {
        // Spend → Clicks → Leads (rule edges, "reads")
        // i.e. forward: Clicks reads Spend, Leads reads Clicks
        // reverse: Spend's dependents = {Clicks}; Clicks's = {Leads}
        let mut g = DependencyGraph::new();
        let spend = coord(&[1]);
        let clicks = coord(&[2]);
        let leads = coord(&[3]);
        g.add_edge(
            clicks.clone(),
            DependencyEdge {
                to: spend.clone(),
                via: DependencySource::Rule(RuleId(1)),
            },
        );
        g.add_edge(
            leads.clone(),
            DependencyEdge {
                to: clicks.clone(),
                via: DependencySource::Rule(RuleId(2)),
            },
        );

        let mut t = DirtyTracker::new();
        t.mark_closure(&spend, &g);
        assert!(
            !t.is_dirty(&spend),
            "root is freshly-written and must NOT be dirty"
        );
        assert!(t.is_dirty(&clicks), "direct dependent must be dirty");
        assert!(t.is_dirty(&leads), "transitive dependent must be dirty");
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn mark_closure_does_not_dirty_unrelated_cells() {
        let mut g = DependencyGraph::new();
        let spend = coord(&[1]);
        let clicks = coord(&[2]);
        let unrelated = coord(&[100]);
        let unrelated_dep = coord(&[101]);
        g.add_edge(
            clicks.clone(),
            DependencyEdge {
                to: spend.clone(),
                via: DependencySource::Rule(RuleId(1)),
            },
        );
        g.add_edge(
            unrelated_dep.clone(),
            DependencyEdge {
                to: unrelated.clone(),
                via: DependencySource::Rule(RuleId(99)),
            },
        );

        let mut t = DirtyTracker::new();
        t.mark_closure(&spend, &g);
        assert!(!t.is_dirty(&spend), "root is fresh, not dirty");
        assert!(t.is_dirty(&clicks));
        assert!(!t.is_dirty(&unrelated));
        assert!(!t.is_dirty(&unrelated_dep));
    }

    #[test]
    fn snapshot_sorted_is_deterministic() {
        let mut t = DirtyTracker::new();
        for i in [3, 1, 2, 5, 4] {
            t.mark(coord(&[i]));
        }
        let s1 = t.snapshot_sorted();
        let s2 = t.snapshot_sorted();
        assert_eq!(s1, s2, "two snapshots of same dirty set must agree");
        assert_eq!(s1.len(), 5);
    }

    // --- Phase 2D: bitset-backed observational equivalence ---

    /// Build a small two-dim cube shape (3 × 4) so we can stand up a
    /// bitset-backed tracker without dragging in the full Acme fixture.
    /// The cube id is fixed; both representations see identical coords.
    fn small_shape_dims() -> Vec<Dimension> {
        let dim_a = DimensionId(101);
        let dim_b = DimensionId(102);
        let a_built = Dimension::builder(dim_a, "A", DimensionKind::Standard)
            .add_element(Element::leaf(ElementId(1), "a0", dim_a))
            .expect("a0")
            .add_element(Element::leaf(ElementId(2), "a1", dim_a))
            .expect("a1")
            .add_element(Element::leaf(ElementId(3), "a2", dim_a))
            .expect("a2")
            .build()
            .expect("dim a");
        let b_built = Dimension::builder(dim_b, "B", DimensionKind::Standard)
            .add_element(Element::leaf(ElementId(10), "b0", dim_b))
            .expect("b0")
            .add_element(Element::leaf(ElementId(11), "b1", dim_b))
            .expect("b1")
            .add_element(Element::leaf(ElementId(12), "b2", dim_b))
            .expect("b2")
            .add_element(Element::leaf(ElementId(13), "b3", dim_b))
            .expect("b3")
            .build()
            .expect("dim b");
        vec![a_built, b_built]
    }

    fn coord2(a: u64, b: u64) -> CellCoordinate {
        CellCoordinate::from_parts(CubeId(1), [ElementId(a), ElementId(b)])
    }

    fn collect_dirty_sorted(t: &DirtyTracker) -> Vec<CellCoordinate> {
        let mut v: Vec<CellCoordinate> = t.iter().cloned().collect();
        v.sort_by(|x, y| format!("{x:?}").cmp(&format!("{y:?}")));
        v
    }

    /// §10.1 dirty-set membership invariant — proven exactly. Drives a
    /// long mixed sequence of mark / clear / clear_all operations
    /// against both the AHashSet-backed (`new()`) and bitset-backed
    /// (`with_shape(...)`) trackers and asserts they agree on
    /// `is_dirty` for every coord, on `len()`, and on the sorted iter
    /// content after each operation. If this passes, every
    /// higher-level test that depends on the dirty-set
    /// representation (§10.1 in particular) inherits the
    /// equivalence.
    ///
    /// Per Phase 2D handoff §"Phase 2D scope" item 4.
    #[test]
    fn bitset_tracker_observationally_equivalent_to_ahashset() {
        let dims = small_shape_dims();
        let shape = CubeShape::new(&dims).expect("shape builds");

        let mut hash_t = DirtyTracker::new();
        let mut bitset_t = DirtyTracker::with_shape(Arc::clone(&shape));

        // Every coord in the 3×4 Cartesian product, plus a handful of
        // out-of-shape coords (different cube id) that the bitset path
        // is allowed to silently skip — those are intentionally NOT
        // exercised against the hash path so the comparison stays
        // apples-to-apples.
        let mut all_coords: Vec<CellCoordinate> = Vec::new();
        for a in &dims[0].elements {
            for b in &dims[1].elements {
                all_coords.push(CellCoordinate::from_parts(CubeId(1), [a.id, b.id]));
            }
        }

        // Op script: mix marks, idempotent re-marks, clears, re-marks
        // after clears (the duplicate-push hazard the `ever_marked`
        // bitset guards against), and one full `clear_all` cycle.
        // Every step asserts agreement; failure points the finger at
        // the exact diverging op.
        enum Op {
            Mark(usize),
            Clear(usize),
            ClearAll,
        }
        use Op::*;
        let ops: Vec<Op> = vec![
            Mark(0),
            Mark(0), // idempotent
            Mark(5),
            Mark(11),
            Clear(0),
            Mark(0), // re-mark after clear: must NOT push a duplicate
            Mark(5), // already dirty
            Clear(5),
            Clear(11),
            Mark(7),
            Mark(3),
            Mark(8),
            Clear(3),
            Mark(3), // re-mark again
            Mark(2),
            Mark(9),
            Clear(8),
            Clear(2),
            ClearAll,
            Mark(4),
            Mark(6),
            Mark(10),
            Clear(4),
            Mark(1),
        ];
        for (step, op) in ops.iter().enumerate() {
            match op {
                Mark(i) => {
                    let c = all_coords[*i].clone();
                    hash_t.mark(c.clone());
                    bitset_t.mark(c);
                }
                Clear(i) => {
                    let c = &all_coords[*i];
                    hash_t.clear(c);
                    bitset_t.clear(c);
                }
                ClearAll => {
                    hash_t.clear_all();
                    bitset_t.clear_all();
                }
            }

            assert_eq!(
                hash_t.len(),
                bitset_t.len(),
                "step {step}: len() diverged ({} vs {})",
                hash_t.len(),
                bitset_t.len()
            );
            assert_eq!(
                hash_t.is_empty(),
                bitset_t.is_empty(),
                "step {step}: is_empty() diverged"
            );
            for c in &all_coords {
                assert_eq!(
                    hash_t.is_dirty(c),
                    bitset_t.is_dirty(c),
                    "step {step}: is_dirty({c:?}) diverged"
                );
            }
            let hash_iter = collect_dirty_sorted(&hash_t);
            let bitset_iter = collect_dirty_sorted(&bitset_t);
            assert_eq!(
                hash_iter, bitset_iter,
                "step {step}: iter()-then-sort diverged"
            );
            assert_eq!(
                hash_t.snapshot_sorted(),
                bitset_t.snapshot_sorted(),
                "step {step}: snapshot_sorted diverged"
            );
        }
    }

    /// `mark_closure` walks `graph.closure_of_dependents(root)` and
    /// feeds each into `mark`. Confirm the bitset path receives every
    /// closure entry the hash path does.
    #[test]
    fn bitset_tracker_mark_closure_matches_hash() {
        let dims = small_shape_dims();
        let shape = CubeShape::new(&dims).expect("shape builds");

        let mut g = DependencyGraph::new();
        let root = coord2(1, 10);
        let dep_a = coord2(2, 10);
        let dep_b = coord2(2, 11);
        let dep_chain = coord2(3, 10);
        // dep_a reads root, dep_b reads root, dep_chain reads dep_a.
        g.add_edge(
            dep_a.clone(),
            DependencyEdge {
                to: root.clone(),
                via: DependencySource::Rule(RuleId(1)),
            },
        );
        g.add_edge(
            dep_b.clone(),
            DependencyEdge {
                to: root.clone(),
                via: DependencySource::Rule(RuleId(2)),
            },
        );
        g.add_edge(
            dep_chain.clone(),
            DependencyEdge {
                to: dep_a.clone(),
                via: DependencySource::Rule(RuleId(3)),
            },
        );

        let mut hash_t = DirtyTracker::new();
        let mut bitset_t = DirtyTracker::with_shape(Arc::clone(&shape));
        hash_t.mark_closure(&root, &g);
        bitset_t.mark_closure(&root, &g);

        assert_eq!(hash_t.len(), bitset_t.len());
        assert!(!hash_t.is_dirty(&root));
        assert!(!bitset_t.is_dirty(&root));
        assert!(hash_t.is_dirty(&dep_a) && bitset_t.is_dirty(&dep_a));
        assert!(hash_t.is_dirty(&dep_b) && bitset_t.is_dirty(&dep_b));
        assert!(hash_t.is_dirty(&dep_chain) && bitset_t.is_dirty(&dep_chain));
    }
}
