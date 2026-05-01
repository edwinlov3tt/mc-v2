//! Dirty tracking: cells whose cached value is no longer valid.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.13.
//!
//! Per spec §16 the dirty state is the engine's mechanism for lazy
//! evaluation: writes are O(1) (mark dirty, don't recompute); reads are
//! O(closure-size) on first read after dirtying, then O(1) cached.
//! `DirtyTracker` is not the cache itself — it's the flag set telling
//! `cube.rs` which cells need recomputation.

use ahash::AHashSet;

use crate::coordinate::CellCoordinate;
use crate::dependency::DependencyGraph;

#[derive(Debug, Default)]
pub struct DirtyTracker {
    set: AHashSet<CellCoordinate>,
}

impl DirtyTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark a single cell dirty. Idempotent — marking an already-dirty
    /// cell is a no-op (per spec §16 I-Dirty-6).
    pub fn mark(&mut self, coord: CellCoordinate) {
        self.set.insert(coord);
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
            self.set.insert(c);
        }
    }

    /// True iff `coord` is currently dirty.
    pub fn is_dirty(&self, coord: &CellCoordinate) -> bool {
        self.set.contains(coord)
    }

    /// Remove `coord` from the dirty set after a successful recompute
    /// (per spec §16 I-Dirty-4).
    pub fn clear(&mut self, coord: &CellCoordinate) {
        self.set.remove(coord);
    }

    /// Drop every dirty marker. Used by snapshot rollback (per
    /// `cube.rs::rollback_to`) since rolled-back state is by definition
    /// fresh.
    pub fn clear_all(&mut self) {
        self.set.clear();
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// Iterator over the dirty set. **Order is nondeterministic** — per
    /// CLAUDE.md §2.11 / brief §15 step 6, callers that need a stable
    /// sequence collect-and-sort.
    pub fn iter(&self) -> impl Iterator<Item = &CellCoordinate> {
        self.set.iter()
    }

    /// Snapshot the dirty set into a deterministic, sorted vec. Used by
    /// tests that assert membership and bounds without depending on
    /// AHashSet iteration order.
    pub fn snapshot_sorted(&self) -> Vec<CellCoordinate> {
        let mut v: Vec<CellCoordinate> = self.set.iter().cloned().collect();
        v.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependency::{DependencyEdge, DependencySource};
    use crate::id::{CubeId, ElementId, RuleId};

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
}
